//! The winemetal fakes a front end's host tests run `dxmt-core` on.
//!
//! Each fake is the winemetal function of its `export_name`, so a test binary that links this
//! crate runs a core [`Device`](dxmt_core::device::Device) on a made-up handle
//! (`Device::headless`) through its queue's real encode and finish threads. Only a front end's
//! own tests link it, in a cargo command of their own: `dxmt-core`'s, d3d11's and d3d12's test
//! binaries define their own fakes, and two definitions of one symbol do not link.
//!
//! What Metal is asked to do is captured per device ([`take_calls`]): the command buffers'
//! encoders (a render pass's colour targets as `color<i>` lines, the views they resolve into
//! and a raster sample count above one), the records each `encodeCommands` is handed (as
//! [`format_pass_records`] prints them), the ends, the commits, pipelines (a raster sample count
//! other than one or alpha-to-coverage, a vertex descriptor's used attributes and layouts; the
//! tile pipelines too) and presents. Objects a device's queue makes carry its handle, so tests
//! running at once each read their own device's calls.
//! What the core logs goes to [`take_log`]. The capture manager and the scalers have no fakes: a
//! call to one aborts the test (the test binary links with `-undefined dynamic_lookup`).

use core::{
    alloc::Layout,
    ffi::{CStr, c_char, c_void},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Mutex, MutexGuard, PoisonError},
};

use dxmt_core::test_support::{Records, format_pass_records};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER, WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE,
    WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER, WMT_BLIT_COMMAND_GENERATE_MIPMAPS,
    WMT_COMMAND_BUFFER_STATUS_COMPLETED, WMT_COMMAND_BUFFER_STATUS_ERROR,
    WMT_COMMAND_BUFFER_STATUS_SCHEDULED, WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
    WMT_VIEW_ATTACH_FLAG_HDR_ENABLE, WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_ON,
    WMT_VIEW_POLL_FLAG_HDR_ACTIVE, WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE, WMTBufferInfo,
    WMTCursorOverlayParams, WMTDepthStencilInfo, WMTEDRValue, WMTFXSpatialScalerInfo,
    WMTFunctionConstant, WMTHDRMetadata, WMTLayerProps, WMTRenderPassInfo, WMTRenderPipelineInfo,
    WMTSampleBufferAttachmentInfo, WMTSamplerInfo, WMTTextureInfo, WMTTextureSwizzleChannels,
    WMTTileRenderPipelineInfo, WMTVertexDescriptorInfo, WMTViewAttachDesc, WMTViewPollOut,
    WmtcmdBase, WmtcmdBlitCopyFromBufferToBuffer, WmtcmdBlitCopyFromBufferToTexture,
    WmtcmdBlitCopyFromTextureToBuffer, WmtcmdBlitGenerateMipmaps, WmtcmdRenderSetbytes,
};

/// The first handle the fakes make: above every made-up device handle (at most 0x1000).
const FIRST_HANDLE: u64 = 0x10_0000;

type FakeCacheTable = BTreeMap<Vec<u8>, Vec<u8>>;

/// What the fakes made and saw, for every thread: the encode thread calls them, not the test's.
struct State {
    next: u64,
    /// The device each object a device's queue made belongs to.
    owner: BTreeMap<u64, u64>,
    /// Armed command-buffer failures per device, consumed when a buffer finishes.
    fail_next: BTreeMap<u64, u32>,
    /// Per-command-buffer simulated Metal completion status.
    cmdbuf_status: BTreeMap<u64, u64>,
    /// Each call a test reads, after its device.
    calls: Vec<(u64, String)>,
    log: Vec<(u32, String)>,
    /// Byte payloads stored for `DispatchData_alloc_init` / `DispatchData_copyBytes`.
    dispatch_data: BTreeMap<u64, Vec<u8>>,
    /// In-memory SQLite cache simulation keyed by path: `(version, map<key, bytes>)`.
    cache_dbs: BTreeMap<String, (u64, FakeCacheTable)>,
    /// Open `CacheReader` handles mapped to their database path.
    cache_readers: BTreeMap<u64, String>,
    /// Open `CacheWriter` handles mapped to their database path.
    cache_writers: BTreeMap<u64, String>,
    /// Opt-in per-device `WMTEDRValue` returned by `MetalLayer_getEDRValue`.
    edr: BTreeMap<u64, WMTEDRValue>,
    /// Opt-in per-device drawable pixel format returned by `MTLTexture_pixelFormat`.
    drawable_format: BTreeMap<u64, u32>,
    /// Opt-in per-device drawable size returned by `MTLTexture_width` / `height`.
    drawable_size: BTreeMap<u64, (u64, u64)>,
    /// Handles allocated by `MetalDrawable_texture`.
    drawable_textures: BTreeSet<u64>,
    /// `(pixel_format, width, height)` recorded at `MTLDevice_newTexture`.
    texture_props: BTreeMap<u64, (u32, u64, u64)>,
    /// Per-device `MetalFX` spatial scaler support returned by `MTLDevice_supportsFXSpatialScaler`.
    spatial_scaler_supported: BTreeMap<u64, bool>,
    /// Per-device flag forcing `MTLDevice_newSpatialScaler` to return `0`.
    spatial_scaler_fail: BTreeMap<u64, bool>,
    /// Opt-in per-device `WMTViewPollOut` returned by `WMTAttachMetalView` / `WMTViewSyncAndPoll`.
    view_poll: BTreeMap<u64, WMTViewPollOut>,
    /// Per-device flag forcing `MetalLayer_nextDrawable` to return `0` (simulating timeout).
    next_drawable_zero: BTreeMap<u64, bool>,
}

/// The fakes' one [`State`].
///
/// A static because the caller has no object to reach: a fake is a C function that gets only
/// handles, called on the queue's threads, so it keys what it records by the device the handle
/// belongs to.
static STATE: Mutex<State> = Mutex::new(State {
    next: FIRST_HANDLE,
    owner: BTreeMap::new(),
    fail_next: BTreeMap::new(),
    cmdbuf_status: BTreeMap::new(),
    calls: Vec::new(),
    log: Vec::new(),
    dispatch_data: BTreeMap::new(),
    cache_dbs: BTreeMap::new(),
    cache_readers: BTreeMap::new(),
    cache_writers: BTreeMap::new(),
    edr: BTreeMap::new(),
    drawable_format: BTreeMap::new(),
    drawable_size: BTreeMap::new(),
    drawable_textures: BTreeSet::new(),
    texture_props: BTreeMap::new(),
    spatial_scaler_supported: BTreeMap::new(),
    spatial_scaler_fail: BTreeMap::new(),
    view_poll: BTreeMap::new(),
    next_drawable_zero: BTreeMap::new(),
});

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

impl State {
    /// A new handle; one made of `parent` (a device, or what a device made) is that device's.
    fn made_of(&mut self, parent: u64) -> u64 {
        let handle = self.next;
        self.next += 0x10;
        if let Some(device) = self.device_of(parent) {
            self.owner.insert(handle, device);
        }
        handle
    }

    fn device_of(&self, object: u64) -> Option<u64> {
        if object != 0 && object < FIRST_HANDLE {
            return Some(object);
        }
        self.owner.get(&object).copied()
    }

    /// Captures `line` for the device `object` belongs to.
    fn say(&mut self, object: u64, line: String) {
        if let Some(device) = self.device_of(object) {
            self.calls.push((device, line));
        }
    }
}

/// The calls captured for `device` (its made-up handle) since the last take, in order.
///
/// A call is its winemetal name; each record an `encodeCommands` call is handed follows it as a
/// line of its own, indented by two spaces.
#[must_use]
pub fn take_calls(device: u64) -> Vec<String> {
    let mut state = state();
    let (mine, others) = core::mem::take(&mut state.calls)
        .into_iter()
        .partition(|(d, _)| *d == device);
    state.calls = others;
    drop(state);
    mine.into_iter().map(|(_, line)| line).collect::<Vec<_>>()
}

/// What the core logged since the last take, by every test: the level and the text of each line.
#[must_use]
pub fn take_log() -> Vec<(u32, String)> {
    core::mem::take(&mut state().log)
}

/// Arms `device`'s next finished command buffer to complete with `WMT_COMMAND_BUFFER_STATUS_ERROR`.
pub fn fail_next_command_buffer(device: u64) {
    let mut state = state();
    *state.fail_next.entry(device).or_insert(0) += 1;
}

/// Clears the in-memory fake shader cache databases and open reader/writer handles.
pub fn reset_shader_cache() {
    let mut state = state();
    state.cache_dbs.clear();
    state.cache_readers.clear();
    state.cache_writers.clear();
}

/// Sets or clears the opt-in `WMTEDRValue` returned by `MetalLayer_getEDRValue` for `device`.
pub fn set_edr_value(device: u64, edr: Option<WMTEDRValue>) {
    let mut state = state();
    match edr {
        Some(value) => {
            state.edr.insert(device, value);
        }
        None => {
            state.edr.remove(&device);
        }
    }
}

/// Sets or clears the opt-in drawable pixel format for `MTLTexture_pixelFormat` on `device`.
pub fn set_drawable_format(device: u64, format: Option<u32>) {
    let mut state = state();
    match format {
        Some(fmt) => {
            state.drawable_format.insert(device, fmt);
        }
        None => {
            state.drawable_format.remove(&device);
        }
    }
}

/// Sets or clears the opt-in drawable extent for `MTLTexture_width` / `height` on `device`.
pub fn set_drawable_size(device: u64, size: Option<(u64, u64)>) {
    let mut state = state();
    match size {
        Some(sz) => {
            state.drawable_size.insert(device, sz);
        }
        None => {
            state.drawable_size.remove(&device);
        }
    }
}

/// Controls whether `MTLDevice_supportsFXSpatialScaler` reports support on `device`.
pub fn set_spatial_scaler_supported(device: u64, supported: bool) {
    state().spatial_scaler_supported.insert(device, supported);
}

/// Controls whether `MTLDevice_newSpatialScaler` returns null (`0`) on `device`.
pub fn set_spatial_scaler_fail(device: u64, fail: bool) {
    state().spatial_scaler_fail.insert(device, fail);
}

/// Sets or clears the opt-in `WMTViewPollOut` returned for `device`.
pub fn set_view_poll(device: u64, poll: Option<WMTViewPollOut>) {
    let mut state = state();
    match poll {
        Some(value) => {
            state.view_poll.insert(device, value);
        }
        None => {
            state.view_poll.remove(&device);
        }
    }
}

/// Controls whether `MetalLayer_nextDrawable` returns `0` on `device`.
pub fn set_next_drawable_zero(device: u64, zero: bool) {
    let mut state = state();
    if zero {
        state.next_drawable_zero.insert(device, true);
    } else {
        state.next_drawable_zero.remove(&device);
    }
}

/// The devices `WMTCopyAllDevices` lists, in order: made-up device handles.
pub const FAKE_DEVICES: [u64; 2] = [0xe1, 0xe2];

/// A made-up device whose `MTLDevice_newRenderPipelineStateWithVertexDescriptor` makes nothing.
///
/// The call is still captured; a test runs a pipeline that does not build on it.
pub const REFUSES_PIPELINES: u64 = 0xbad;

/// The registry ID `MTLDevice_registryID` answers for `device`.
#[must_use]
pub const fn fake_registry_id(device: u64) -> u64 {
    0x1_0000_0000 | device
}

/// Captures an `encodeCommands` call and the records from `head` on, as `kind`'s.
fn encode(name: &str, kind: &Records, encoder: u64, head: *const WmtcmdBase) {
    // SAFETY: the core hands winemetal a list of live records of `kind`'s encoder.
    let records = unsafe { format_pass_records(kind, head) };
    let mut state = state();
    state.say(encoder, name.to_owned());
    for record in records {
        state.say(encoder, format!("  {record}"));
    }
    drop(state);
}

#[unsafe(export_name = "dxmt_log")]
extern "C" fn fake_dxmt_log(level: u32, message: *const u8, length: usize) {
    // SAFETY: the core passes `length` bytes of UTF-8 at `message`.
    let bytes = unsafe { core::slice::from_raw_parts(message, length) };
    let text = String::from_utf8_lossy(bytes).into_owned();
    state().log.push((level, text));
}

#[unsafe(export_name = "NSObject_retain")]
const extern "C-unwind" fn fake_retain(_object: u64) {}

#[unsafe(export_name = "NSObject_release")]
const extern "C-unwind" fn fake_release(_object: u64) {}

#[unsafe(export_name = "NSObject_description")]
const extern "C-unwind" fn fake_description(_object: u64) -> u64 {
    0
}

#[unsafe(export_name = "NSString_string")]
extern "C-unwind" fn fake_nsstring_string(_data: *const c_char, _encoding: u64) -> u64 {
    state().made_of(0)
}

#[unsafe(export_name = "NSString_lengthOfBytesUsingEncoding")]
const extern "C-unwind" fn fake_nsstring_length(_string: u64, _encoding: u64) -> u64 {
    0
}

#[unsafe(export_name = "NSString_getCString")]
const extern "C-unwind" fn fake_nsstring_get_cstring(
    _string: u64,
    _buffer: *mut c_char,
    _max: u64,
    _encoding: u64,
) -> u32 {
    0
}

#[unsafe(export_name = "NSAutoreleasePool_alloc_init")]
extern "C-unwind" fn fake_autorelease_pool() -> u64 {
    state().made_of(0)
}

#[unsafe(export_name = "WMTGetOSVersion")]
const extern "C-unwind" fn fake_os_version(major: *mut u64, minor: *mut u64, patch: *mut u64) {
    // SAFETY: the core passes three writable locals.
    unsafe { major.write(15) };
    // SAFETY: as above.
    unsafe { minor.write(0) };
    // SAFETY: as above.
    unsafe { patch.write(0) };
}

#[unsafe(export_name = "MTLDevice_supportsFamily")]
const extern "C-unwind" fn fake_supports_family(_device: u64, _family: u32) -> bool {
    true
}

#[unsafe(export_name = "MTLDevice_supportsBCTextureCompression")]
const extern "C-unwind" fn fake_supports_bc(_device: u64) -> bool {
    true
}

#[unsafe(export_name = "MTLDevice_supportsFXSpatialScaler")]
extern "C-unwind" fn fake_supports_fx_spatial_scaler(device: u64) -> bool {
    state()
        .spatial_scaler_supported
        .get(&device)
        .copied()
        .unwrap_or(true)
}

#[unsafe(export_name = "MTLDevice_newSpatialScaler")]
extern "C-unwind" fn fake_new_spatial_scaler(
    device: u64,
    info: *const WMTFXSpatialScalerInfo,
) -> u64 {
    let mut state = state();
    let supported = state
        .spatial_scaler_supported
        .get(&device)
        .copied()
        .unwrap_or(true);
    let fail = state
        .spatial_scaler_fail
        .get(&device)
        .copied()
        .unwrap_or(false);
    if !supported || fail {
        return 0;
    }
    // SAFETY: the caller passes a live `WMTFXSpatialScalerInfo` for the call.
    let i = unsafe { &*info };
    let handle = state.made_of(device);
    let line = format!(
        "MTLDevice_newSpatialScaler {}x{}->{}x{} fmt={}/{}",
        i.input_width,
        i.input_height,
        i.output_width,
        i.output_height,
        i.color_format,
        i.output_format,
    );
    state.say(device, line);
    handle
}

#[unsafe(export_name = "MTLDevice_setShouldMaximizeConcurrentCompilation")]
const extern "C-unwind" fn fake_maximize_concurrent(_device: u64, _value: bool) {}

#[unsafe(export_name = "WMTCopyAllDevices")]
extern "C-unwind" fn fake_copy_all_devices() -> u64 {
    state().made_of(0)
}

#[unsafe(export_name = "NSArray_count")]
const extern "C-unwind" fn fake_array_count(_array: u64) -> u64 {
    FAKE_DEVICES.len() as u64
}

#[unsafe(export_name = "NSArray_object")]
extern "C-unwind" fn fake_array_object(_array: u64, index: u64) -> u64 {
    usize::try_from(index)
        .ok()
        .and_then(|i| FAKE_DEVICES.get(i))
        .copied()
        .unwrap_or(0)
}

#[unsafe(export_name = "MTLDevice_registryID")]
const extern "C-unwind" fn fake_registry_id_of(device: u64) -> u64 {
    fake_registry_id(device)
}

#[unsafe(export_name = "MTLDevice_newCommandQueue")]
extern "C-unwind" fn fake_new_command_queue(device: u64, _max_command_buffers: u64) -> u64 {
    state().made_of(device)
}

#[unsafe(export_name = "MTLDevice_newSharedEvent")]
extern "C-unwind" fn fake_new_shared_event(device: u64) -> u64 {
    state().made_of(device)
}

#[unsafe(export_name = "MTLSharedEvent_signaledValue")]
const extern "C-unwind" fn fake_signaled_value(_event: u64) -> u64 {
    0
}

#[unsafe(export_name = "MTLSharedEvent_waitUntilSignaledValue")]
const extern "C-unwind" fn fake_wait_until_signaled_value(
    _event: u64,
    _value: u64,
    _timeout: u64,
) -> bool {
    true
}

#[unsafe(export_name = "MTLSharedEvent_signalValue")]
extern "C-unwind" fn fake_shared_event_signal_value(event: u64, value: u64) {
    state().say(event, format!("MTLSharedEvent_signalValue value={value}"));
}

#[unsafe(export_name = "MTLDevice_newEvent")]
extern "C-unwind" fn fake_new_event(device: u64) -> u64 {
    state().made_of(device)
}

#[unsafe(export_name = "MTLDevice_newFence")]
extern "C-unwind" fn fake_new_fence(device: u64) -> u64 {
    state().made_of(device)
}

#[unsafe(export_name = "MTLDevice_newSamplerState")]
extern "C-unwind" fn fake_new_sampler_state(device: u64, info: *mut WMTSamplerInfo) -> u64 {
    let mut state = state();
    let handle = state.made_of(device);
    // SAFETY: the core's sampler description, readable and writable for the call.
    let i = unsafe { &mut *info };
    i.gpu_resource_id = handle + 1;
    let line = format!(
        "MTLDevice_newSamplerState min={} mag={} mip={} address={},{},{} border={} compare={} \
         lod={}..{} aniso={} normalized={} argument_buffers={}",
        i.min_filter,
        i.mag_filter,
        i.mip_filter,
        i.s_address_mode,
        i.t_address_mode,
        i.r_address_mode,
        i.border_color,
        i.compare_function,
        i.lod_min_clamp,
        i.lod_max_clamp,
        i.max_anisotroy,
        i.normalized_coords,
        i.support_argument_buffers
    );
    state.say(device, line);
    handle
}

/// A buffer whose contents are real memory, which the rings and the dummy buffer write through.
#[unsafe(export_name = "MTLDevice_newBuffer")]
extern "C-unwind" fn fake_new_buffer(device: u64, info: *mut WMTBufferInfo) -> u64 {
    // SAFETY: the core's buffer description, readable and writable for the call.
    let info = unsafe { &mut *info };
    if info.memory.ptr == 0 {
        let length = usize::try_from(info.length.max(1)).expect("a length");
        let layout = Layout::from_size_align(length, 4096).expect("a buffer's layout");
        // ponytail: leaked; a test process makes few buffers.
        // SAFETY: a layout of non-zero size.
        let memory = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!memory.is_null(), "memory for a fake buffer");
        info.memory.ptr = memory.expose_provenance() as u64;
    }
    let handle = state().made_of(device);
    info.gpu_address = handle << 16;
    handle
}

#[unsafe(export_name = "MTLDevice_newTexture")]
extern "C-unwind" fn fake_new_texture(device: u64, info: *mut WMTTextureInfo) -> u64 {
    let mut state = state();
    let handle = state.made_of(device);
    // SAFETY: the core's texture description, readable and writable for the call.
    let i = unsafe { &mut *info };
    i.gpu_resource_id = handle + 1;
    state.texture_props.insert(
        handle,
        (i.pixel_format, u64::from(i.width), u64::from(i.height)),
    );
    let line = format!(
        "MTLDevice_newTexture #{handle} format={} size={}x{}x{} array={} bits={:#x}",
        i.pixel_format, i.width, i.height, i.depth, i.array_length, i.bits
    );
    state.say(device, line);
    handle
}

#[unsafe(export_name = "MTLTexture_newTextureView")]
extern "C-unwind" fn fake_new_texture_view(
    texture: u64,
    format: u32,
    texture_type: u32,
    level_start: u16,
    _level_count: u16,
    slice_start: u16,
    _slice_count: u16,
    _swizzle: WMTTextureSwizzleChannels,
    gpu_resource_id: *mut u64,
) -> u64 {
    let mut state = state();
    let handle = state.made_of(texture);
    state.say(
        texture,
        format!(
            "MTLTexture_newTextureView #{handle} of #{texture} format={format} type={texture_type} \
             mip={level_start} slice={slice_start}"
        ),
    );
    drop(state);
    // SAFETY: the core's out pointer, writable.
    unsafe { gpu_resource_id.write(handle + 1) };
    handle
}

#[unsafe(export_name = "MTLTexture_pixelFormat")]
extern "C-unwind" fn fake_pixel_format(texture: u64) -> u32 {
    let state = state();
    if let Some(device) = state.device_of(texture)
        && let Some(&fmt) = state.drawable_format.get(&device)
    {
        if state.drawable_textures.contains(&texture) {
            return fmt;
        }
        return state.texture_props.get(&texture).map_or(0, |p| p.0);
    }
    0
}

#[unsafe(export_name = "MTLTexture_width")]
extern "C-unwind" fn fake_width(texture: u64) -> u64 {
    let state = state();
    if let Some(device) = state.device_of(texture)
        && let Some(&sz) = state.drawable_size.get(&device)
    {
        if state.drawable_textures.contains(&texture) {
            return sz.0;
        }
        return state.texture_props.get(&texture).map_or(0, |p| p.1);
    }
    0
}

#[unsafe(export_name = "MTLTexture_height")]
extern "C-unwind" fn fake_height(texture: u64) -> u64 {
    let state = state();
    if let Some(device) = state.device_of(texture)
        && let Some(&sz) = state.drawable_size.get(&device)
    {
        if state.drawable_textures.contains(&texture) {
            return sz.1;
        }
        return state.texture_props.get(&texture).map_or(0, |p| p.2);
    }
    0
}

#[unsafe(export_name = "MTLDevice_newDepthStencilState")]
extern "C-unwind" fn fake_new_depth_stencil_state(
    device: u64,
    info: *const WMTDepthStencilInfo,
) -> u64 {
    // SAFETY: the caller's state description, alive for the call.
    let info = unsafe { &*info };
    let face = |s: &winemetal_sys::wire::WMTStencilInfo| {
        format!(
            "{},{},{},{},{},{},{}",
            s.enabled,
            s.stencil_compare_function,
            s.stencil_fail_op,
            s.depth_fail_op,
            s.depth_stencil_pass_op,
            s.read_mask,
            s.write_mask
        )
    };
    let made = format!(
        "cmp={} write={} front={} back={}",
        info.depth_compare_function,
        info.depth_write_enabled,
        face(&info.front_stencil),
        face(&info.back_stencil)
    );
    let mut state = state();
    let handle = state.made_of(device);
    state.say(
        device,
        format!("MTLDevice_newDepthStencilState #{handle} {made}"),
    );
    drop(state);
    handle
}

/// A pipeline's raster sample count and alpha-to-coverage, when either is not the default.
fn say_coverage(state: &mut State, device: u64, info: &WMTRenderPipelineInfo) {
    if info.raster_sample_count != 1 || info.alpha_to_coverage_enabled != 0 {
        state.say(
            device,
            format!(
                "  pso samples={} a2c={}",
                info.raster_sample_count, info.alpha_to_coverage_enabled
            ),
        );
    }
}

/// A vertex descriptor's used attributes (a format) and layouts (a stride), each in index order.
fn say_vertex(state: &mut State, device: u64, vertex: &WMTVertexDescriptorInfo) {
    for (i, a) in vertex.attributes.iter().enumerate() {
        if a.format != 0 {
            state.say(
                device,
                format!(
                    "  attr {i} format={} offset={} buffer={}",
                    a.format, a.offset, a.buffer_index
                ),
            );
        }
    }
    for (i, l) in vertex.layouts.iter().enumerate() {
        if l.stride != 0 {
            state.say(
                device,
                format!(
                    "  layout {i} stride={} step={} rate={}",
                    l.stride, l.step_function, l.step_rate
                ),
            );
        }
    }
}

#[unsafe(export_name = "MTLDevice_newRenderPipelineState")]
extern "C-unwind" fn fake_new_render_pipeline(
    device: u64,
    info: *const WMTRenderPipelineInfo,
    _error: *mut u64,
) -> u64 {
    // SAFETY: the core's pipeline description, alive for the call.
    let info = unsafe { &*info };
    let mut state = state();
    state.say(device, "MTLDevice_newRenderPipelineState".to_owned());
    say_coverage(&mut state, device, info);
    state.made_of(device)
}

#[unsafe(export_name = "MTLDevice_newRenderPipelineStateWithVertexDescriptor")]
extern "C-unwind" fn fake_new_render_pipeline_with_vertex_descriptor(
    device: u64,
    info: *const WMTRenderPipelineInfo,
    vertex: *const WMTVertexDescriptorInfo,
    _error: *mut u64,
) -> u64 {
    // SAFETY: the core's pipeline description, alive for the call.
    let info = unsafe { &*info };
    let (color0, depth, stencil) = (
        info.colors[0].pixel_format,
        info.depth_pixel_format,
        info.stencil_pixel_format,
    );
    let mut state = state();
    state.say(
        device,
        "MTLDevice_newRenderPipelineStateWithVertexDescriptor".to_owned(),
    );
    state.say(device, format!("  pso color0={color0}"));
    // After the line the older tests read: each other colour the pipeline declares, then the
    // planes, when it does.
    for (i, c) in info.colors.iter().enumerate().skip(1) {
        if c.pixel_format != 0 {
            state.say(
                device,
                format!(
                    "  pso color{i}={} mask={} blend={}",
                    c.pixel_format, c.write_mask, c.blending_enabled
                ),
            );
        }
    }
    if depth != 0 || stencil != 0 {
        state.say(device, format!("  pso depth={depth} stencil={stencil}"));
    }
    say_coverage(&mut state, device, info);
    // SAFETY: the core's vertex descriptor, null or alive for the call.
    if let Some(vertex) = unsafe { vertex.as_ref() } {
        say_vertex(&mut state, device, vertex);
    }
    if device == REFUSES_PIPELINES {
        return 0;
    }
    state.made_of(device)
}

/// A tile pipeline (a render pass's tile barrier): the formats, samples and threadgroup flag.
#[unsafe(export_name = "MTLDevice_newTileRenderPipelineState")]
extern "C-unwind" fn fake_new_tile_pipeline(
    device: u64,
    info: *const WMTTileRenderPipelineInfo,
    _error: *mut u64,
) -> u64 {
    // SAFETY: the core's tile pipeline description, alive for the call.
    let info = unsafe { &*info };
    let mut state = state();
    state.say(device, "MTLDevice_newTileRenderPipelineState".to_owned());
    for (i, format) in info.color_formats.iter().enumerate() {
        if *format != 0 {
            state.say(device, format!("  tile color{i}={format}"));
        }
    }
    state.say(
        device,
        format!("  tile samples={}", info.raster_sample_count),
    );
    state.say(
        device,
        format!("  tile tgsize={}", info.tgsize_matches_tile_size),
    );
    state.made_of(device)
}

#[unsafe(export_name = "MTLDevice_newComputePipelineState")]
extern "C-unwind" fn fake_new_compute_pipeline(
    device: u64,
    _info: *const winemetal_sys::wire::WMTComputePipelineInfo,
    _error: *mut u64,
) -> u64 {
    state().made_of(device)
}

#[unsafe(export_name = "DispatchData_alloc_init")]
extern "C-unwind" fn fake_dispatch_data(bytes: u64, length: u64) -> u64 {
    let len = usize::try_from(length).unwrap_or(0);
    let payload = if bytes != 0 && len > 0 {
        let ptr = core::ptr::with_exposed_provenance::<u8>(
            usize::try_from(bytes).expect("a pointer that fits usize"),
        );
        // SAFETY: caller passes a readable buffer of `len` bytes for the call.
        unsafe { core::slice::from_raw_parts(ptr, len) }.to_vec()
    } else {
        Vec::new()
    };
    let mut state = state();
    let handle = state.made_of(0);
    state.dispatch_data.insert(handle, payload);
    drop(state);
    handle
}

#[unsafe(export_name = "DispatchData_copyBytes")]
extern "C-unwind" fn fake_dispatch_data_copy_bytes(
    data: u64,
    buffer: *mut c_void,
    capacity: u64,
) -> u64 {
    let state = state();
    let Some(bytes) = state.dispatch_data.get(&data) else {
        return 0;
    };
    let total = bytes.len() as u64;
    if !buffer.is_null() {
        let cap = usize::try_from(capacity).unwrap_or(usize::MAX);
        let count = bytes.len().min(cap);
        if count > 0 {
            // SAFETY: caller passes a writable `buffer` of at least `capacity` bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast::<u8>(), count);
            }
        }
    }
    drop(state);
    total
}

fn cache_path(path: *const c_char) -> Option<String> {
    if path.is_null() {
        return None;
    }
    // SAFETY: caller passes a NUL-terminated path alive for the call.
    Some(
        unsafe { CStr::from_ptr(path) }
            .to_string_lossy()
            .into_owned(),
    )
}

#[unsafe(export_name = "CacheWriter_alloc_init")]
extern "C-unwind" fn fake_cache_writer_alloc_init(path: *const c_char, version: u64) -> u64 {
    let Some(path) = cache_path(path) else {
        return 0;
    };
    let mut state = state();
    match state.cache_dbs.get(&path) {
        Some((existing_ver, _)) if *existing_ver == version => {}
        _ => {
            state
                .cache_dbs
                .insert(path.clone(), (version, BTreeMap::new()));
        }
    }
    let handle = state.made_of(0);
    state.cache_writers.insert(handle, path);
    drop(state);
    handle
}

#[unsafe(export_name = "CacheReader_alloc_init")]
extern "C-unwind" fn fake_cache_reader_alloc_init(path: *const c_char, version: u64) -> u64 {
    let Some(path) = cache_path(path) else {
        return 0;
    };
    let mut state = state();
    if state
        .cache_dbs
        .get(&path)
        .is_none_or(|(existing_ver, _)| *existing_ver != version)
    {
        return 0;
    }
    let handle = state.made_of(0);
    state.cache_readers.insert(handle, path);
    drop(state);
    handle
}

#[unsafe(export_name = "CacheReader_get")]
extern "C-unwind" fn fake_cache_reader_get(reader: u64, key: *const c_void, length: u64) -> u64 {
    let Ok(len) = usize::try_from(length) else {
        return 0;
    };
    if len > 0 && key.is_null() {
        return 0;
    }
    let key_bytes = if len == 0 {
        &[]
    } else {
        // SAFETY: caller passes a readable `key` slice of `len` bytes.
        unsafe { core::slice::from_raw_parts(key.cast::<u8>(), len) }
    };
    let mut state = state();
    let payload = state
        .cache_readers
        .get(&reader)
        .and_then(|path| state.cache_dbs.get(path))
        .and_then(|(_, db)| db.get(key_bytes))
        .cloned();
    let Some(payload) = payload else {
        return 0;
    };
    let handle = state.made_of(0);
    state.dispatch_data.insert(handle, payload);
    drop(state);
    handle
}

#[unsafe(export_name = "CacheWriter_set")]
extern "C-unwind" fn fake_cache_writer_set(
    writer: u64,
    key: *const c_void,
    key_length: u64,
    value: u64,
) {
    let Ok(len) = usize::try_from(key_length) else {
        return;
    };
    if len > 0 && key.is_null() {
        return;
    }
    let key_bytes = if len == 0 {
        &[]
    } else {
        // SAFETY: caller passes a readable `key` slice of `len` bytes.
        unsafe { core::slice::from_raw_parts(key.cast::<u8>(), len) }
    };
    let mut state = state();
    let Some(path) = state.cache_writers.get(&writer).cloned() else {
        return;
    };
    let Some(payload) = state.dispatch_data.get(&value).cloned() else {
        return;
    };
    if let Some((_, db)) = state.cache_dbs.get_mut(&path) {
        db.insert(key_bytes.to_vec(), payload);
    }
    drop(state);
}

#[unsafe(export_name = "MTLDevice_newLibrary")]
extern "C-unwind" fn fake_new_library(device: u64, _data: u64, _error: *mut u64) -> u64 {
    state().made_of(device)
}

#[unsafe(export_name = "MTLLibrary_newFunction")]
extern "C-unwind" fn fake_new_function(library: u64, _name: *const c_char) -> u64 {
    state().made_of(library)
}

#[unsafe(export_name = "MTLLibrary_newFunctionWithConstants")]
extern "C-unwind" fn fake_new_function_with_constants(
    library: u64,
    _name: *const c_char,
    _constants: *const WMTFunctionConstant,
    _count: u32,
    _error: *mut u64,
) -> u64 {
    state().made_of(library)
}

/// The metallib a fake compile gives: bytes nothing reads.
static METALLIB: [u8; 4] = *b"MTLB";

#[unsafe(export_name = "SM50Initialize")]
extern "C-unwind" fn fake_sm50_initialize(
    _bytecode: *const core::ffi::c_void,
    _size: usize,
    shader: *mut u64,
    _reflection: *mut winemetal_sys::sm50::MtlShaderReflection,
    _error: *mut u64,
) -> i32 {
    let handle = state().made_of(0);
    // SAFETY: the core's out pointer, writable.
    unsafe { shader.write(handle) };
    0
}

#[unsafe(export_name = "SM50Destroy")]
const extern "C-unwind" fn fake_sm50_destroy(_shader: u64) {}

#[unsafe(export_name = "SM50Compile")]
extern "C-unwind" fn fake_sm50_compile(
    _shader: u64,
    _args: *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData,
    _name: *const c_char,
    bitcode: *mut u64,
    _error: *mut u64,
) -> i32 {
    let handle = state().made_of(0);
    // SAFETY: the core's out pointer, writable.
    unsafe { bitcode.write(handle) };
    0
}

#[unsafe(export_name = "SM50GetCompiledBitcode")]
extern "C-unwind" fn fake_sm50_compiled_bitcode(
    _bitcode: u64,
    data: *mut winemetal_sys::sm50::Sm50CompiledBitcode,
) {
    let compiled = winemetal_sys::sm50::Sm50CompiledBitcode {
        data: METALLIB.as_ptr().expose_provenance() as u64,
        size: METALLIB.len() as u64,
    };
    // SAFETY: the core's out pointer, writable.
    unsafe { data.write(compiled) };
}

#[unsafe(export_name = "SM50DestroyBitcode")]
const extern "C-unwind" fn fake_sm50_destroy_bitcode(_bitcode: u64) {}

#[unsafe(export_name = "SM50GetErrorMessage")]
const extern "C-unwind" fn fake_sm50_error_message(
    _error: u64,
    _buffer: *mut c_char,
    _size: usize,
) -> usize {
    0
}

#[unsafe(export_name = "SM50FreeError")]
const extern "C-unwind" fn fake_sm50_free_error(_error: u64) {}

#[unsafe(export_name = "SharedEventListener_create")]
extern "C-unwind" fn fake_listener_create() -> u64 {
    state().made_of(0)
}

#[unsafe(export_name = "SharedEventListener_start")]
const extern "C-unwind" fn fake_listener_start(_listener: u64) {}

#[unsafe(export_name = "SharedEventListener_destroy")]
const extern "C-unwind" fn fake_listener_destroy(_listener: u64) {}

#[unsafe(export_name = "MTLCommandQueue_commandBuffer")]
extern "C-unwind" fn fake_command_buffer(queue: u64) -> u64 {
    state().made_of(queue)
}

#[unsafe(export_name = "MTLCommandBuffer_encodeSignalEvent")]
const extern "C-unwind" fn fake_encode_signal_event(_cmdbuf: u64, _event: u64, _value: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_encodeWaitForEvent")]
const extern "C-unwind" fn fake_encode_wait_for_event(_cmdbuf: u64, _event: u64, _value: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_commit")]
extern "C-unwind" fn fake_commit(cmdbuf: u64) {
    state().say(cmdbuf, "MTLCommandBuffer_commit".to_owned());
}

#[unsafe(export_name = "MTLCommandBuffer_status")]
extern "C-unwind" fn fake_status(cmdbuf: u64) -> u64 {
    let mut state = state();
    let status = if let Some(&status) = state.cmdbuf_status.get(&cmdbuf) {
        status
    } else if let Some(device) = state.device_of(cmdbuf)
        && let Some(count) = state.fail_next.get_mut(&device)
        && *count > 0
    {
        *count -= 1;
        state
            .cmdbuf_status
            .insert(cmdbuf, WMT_COMMAND_BUFFER_STATUS_SCHEDULED);
        WMT_COMMAND_BUFFER_STATUS_SCHEDULED
    } else {
        WMT_COMMAND_BUFFER_STATUS_COMPLETED
    };
    drop(state);
    status
}

#[unsafe(export_name = "MTLCommandBuffer_waitUntilCompleted")]
extern "C-unwind" fn fake_wait_until_completed(cmdbuf: u64) {
    let mut state = state();
    if let Some(status) = state.cmdbuf_status.get_mut(&cmdbuf)
        && *status == WMT_COMMAND_BUFFER_STATUS_SCHEDULED
    {
        *status = WMT_COMMAND_BUFFER_STATUS_ERROR;
    }
}

#[unsafe(export_name = "MTLCommandBuffer_error")]
const extern "C-unwind" fn fake_cmdbuf_error(_cmdbuf: u64) -> u64 {
    0
}

#[unsafe(export_name = "MTLCommandBuffer_property")]
const extern "C-unwind" fn fake_property(_cmdbuf: u64, _property: u32) -> u64 {
    0
}

#[unsafe(export_name = "MTLCommandBuffer_renderCommandEncoder")]
extern "C-unwind" fn fake_render_encoder(cmdbuf: u64, info: *mut WMTRenderPassInfo) -> u64 {
    // SAFETY: the core hands winemetal its pass description, alive for the call.
    let pass = unsafe { &*info };
    let color = &pass.colors[0];
    let mut state = state();
    state.say(cmdbuf, "MTLCommandBuffer_renderCommandEncoder".to_owned());
    if color.texture != 0 {
        let c = &color.clear_color;
        state.say(
            cmdbuf,
            format!(
                "  color0 load={} store={} clear={},{},{},{}",
                color.load_action, color.store_action, c.r, c.g, c.b, c.a
            ),
        );
        // After the line the older tests index: the attached texture or view, and the pass's size.
        state.say(
            cmdbuf,
            format!(
                "  target0 #{} {}x{}",
                color.texture, pass.render_target_width, pass.render_target_height
            ),
        );
        // After a colour's lines: the view it resolves into, when it has one.
        if color.resolve_texture != 0 {
            state.say(cmdbuf, format!("  resolve0 #{}", color.resolve_texture));
        }
    }
    // After colour 0's lines: each other attached colour, its view without the pass's size.
    for (i, color) in pass.colors.iter().enumerate().skip(1) {
        if color.texture != 0 {
            let c = &color.clear_color;
            state.say(
                cmdbuf,
                format!(
                    "  color{i} load={} store={} clear={},{},{},{}",
                    color.load_action, color.store_action, c.r, c.g, c.b, c.a
                ),
            );
            state.say(cmdbuf, format!("  target{i} #{}", color.texture));
            if color.resolve_texture != 0 {
                state.say(cmdbuf, format!("  resolve{i} #{}", color.resolve_texture));
            }
        }
    }
    // After the colour lines: each attached depth-stencil plane.
    if pass.depth.texture != 0 {
        let d = &pass.depth;
        state.say(
            cmdbuf,
            format!(
                "  depth #{} load={} store={} clear={}",
                d.texture, d.load_action, d.store_action, d.clear_depth
            ),
        );
    }
    if pass.stencil.texture != 0 {
        let s = &pass.stencil;
        state.say(
            cmdbuf,
            format!(
                "  stencil #{} load={} store={} clear={}",
                s.texture, s.load_action, s.store_action, s.clear_stencil
            ),
        );
    }
    // After the planes: the raster sample count the pass names, when it is above one.
    if pass.default_raster_sample_count > 1 {
        state.say(
            cmdbuf,
            format!("  samples={}", pass.default_raster_sample_count),
        );
    }
    state.made_of(cmdbuf)
}

#[unsafe(export_name = "MTLCommandBuffer_blitCommandEncoder")]
extern "C-unwind" fn fake_blit_encoder(cmdbuf: u64) -> u64 {
    let mut state = state();
    state.say(cmdbuf, "MTLCommandBuffer_blitCommandEncoder".to_owned());
    state.made_of(cmdbuf)
}

#[unsafe(export_name = "MTLCommandBuffer_blitCommandEncoderWithSampleBuffers")]
extern "C-unwind" fn fake_blit_encoder_with_sample_buffers(
    cmdbuf: u64,
    _attachments: *mut WMTSampleBufferAttachmentInfo,
    _count: u64,
) -> u64 {
    fake_blit_encoder(cmdbuf)
}

#[unsafe(export_name = "MTLCommandBuffer_computeCommandEncoder")]
extern "C-unwind" fn fake_compute_encoder(cmdbuf: u64, _concurrent: bool) -> u64 {
    let mut state = state();
    state.say(cmdbuf, "MTLCommandBuffer_computeCommandEncoder".to_owned());
    state.made_of(cmdbuf)
}

#[unsafe(export_name = "MTLComputeCommandEncoder_encodeCommands")]
extern "C-unwind" fn fake_compute_encode_commands(encoder: u64, head: *const WmtcmdBase) {
    encode(
        "MTLComputeCommandEncoder_encodeCommands",
        &Records::Compute,
        encoder,
        head,
    );
}

#[unsafe(export_name = "MTLRenderCommandEncoder_encodeCommands")]
extern "C-unwind" fn fake_render_encode_commands(encoder: u64, head: *const WmtcmdBase) {
    // SAFETY: the core hands winemetal a list of live render records.
    let lines = unsafe { format_pass_records(&Records::Render, head) };
    let mut details = Vec::new();
    let mut at = head.expose_provenance() as u64;
    while at != 0 {
        let base_ptr = core::ptr::with_exposed_provenance::<WmtcmdBase>(
            usize::try_from(at).expect("a pointer"),
        );
        // SAFETY: a record of the list.
        let base = unsafe { &*base_ptr };
        if base.r#type == WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES {
            // SAFETY: a record of this type.
            let c = unsafe { &*base_ptr.cast::<WmtcmdRenderSetbytes>() };
            let len = usize::try_from(c.length).unwrap_or(0);
            let payload = if c.bytes.ptr != 0 && len > 0 {
                let ptr = core::ptr::with_exposed_provenance::<u8>(
                    usize::try_from(c.bytes.ptr).expect("a pointer"),
                );
                // SAFETY: caller passes `len` readable bytes for the call.
                unsafe { core::slice::from_raw_parts(ptr, len) }
            } else {
                &[]
            };
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for &b in payload {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
            details.push(format!(
                "  fragment_bytes index={} length={} fnv={hash:016x}",
                c.index, c.length
            ));
        }
        at = base.next.ptr;
    }
    let mut details = details.into_iter();
    let tag = format!("render {WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES} ");
    let mut state = state();
    state.say(encoder, "MTLRenderCommandEncoder_encodeCommands".to_owned());
    for line in lines {
        let extra = line
            .starts_with(&tag)
            .then(|| details.next().expect("a decoded fragment_bytes record"));
        state.say(encoder, format!("  {line}"));
        if let Some(detail) = extra {
            state.say(encoder, format!("  {detail}"));
        }
    }
    drop(state);
}

/// A blit encoder's records, with the copies into textures and buffers and the mipmaps in full.
///
/// [`format_pass_records`] prints a blit record as its type alone; the texture and buffer uploads
/// a front end's tests assert on are decoded here.
#[unsafe(export_name = "MTLBlitCommandEncoder_encodeCommands")]
extern "C-unwind" fn fake_blit_encode_commands(encoder: u64, head: *const WmtcmdBase) {
    // SAFETY: the core hands winemetal a list of live blit records.
    let lines = unsafe { format_pass_records(&Records::Blit, head) };
    let mut details = Vec::new();
    let mut reads = Vec::new();
    let mut at = head.expose_provenance() as u64;
    while at != 0 {
        let base_ptr = core::ptr::with_exposed_provenance::<WmtcmdBase>(
            usize::try_from(at).expect("a pointer"),
        );
        // SAFETY: a record of the list.
        let base = unsafe { &*base_ptr };
        match base.r#type {
            WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE => {
                // SAFETY: a record of this type.
                let c = unsafe { &*base_ptr.cast::<WmtcmdBlitCopyFromBufferToTexture>() };
                details.push(format!(
                    "blit copy_to_texture #{} level={} slice={} origin={},{},{} size={}x{}x{} \
                     offset={} bytes_per_row={} bytes_per_image={}",
                    c.dst,
                    c.level,
                    c.slice,
                    c.origin.x,
                    c.origin.y,
                    c.origin.z,
                    c.size.width,
                    c.size.height,
                    c.size.depth,
                    c.src_offset,
                    c.bytes_per_row,
                    c.bytes_per_image
                ));
            }
            WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER => {
                // SAFETY: a record of this type.
                let c = unsafe { &*base_ptr.cast::<WmtcmdBlitCopyFromBufferToBuffer>() };
                details.push(format!(
                    "blit copy_to_buffer_from_buffer #{} src_offset={} #{} dst_offset={} \
                     length={}",
                    c.src, c.src_offset, c.dst, c.dst_offset, c.copy_length
                ));
            }
            WMT_BLIT_COMMAND_GENERATE_MIPMAPS => {
                // SAFETY: a record of this type.
                let c = unsafe { &*base_ptr.cast::<WmtcmdBlitGenerateMipmaps>() };
                details.push(format!("blit generate_mipmaps #{}", c.texture));
            }
            WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER => {
                // SAFETY: a record of this type.
                let c = unsafe { &*base_ptr.cast::<WmtcmdBlitCopyFromTextureToBuffer>() };
                reads.push(format!(
                    "  copy_to_buffer #{} level={} slice={} size={}x{} bytes_per_row={}",
                    c.src, c.level, c.slice, c.size.width, c.size.height, c.bytes_per_row
                ));
            }
            _ => {}
        }
        at = base.next.ptr;
    }
    let mut details = details.into_iter();
    let mut reads = reads.into_iter();
    let read = format!("blit {WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER}");
    let mut out = Vec::with_capacity(lines.len());
    for mut line in lines {
        let typed = format!("blit {WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE}");
        let copy = format!("blit {WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_BUFFER}");
        let mips = format!("blit {WMT_BLIT_COMMAND_GENERATE_MIPMAPS}");
        if line == typed || line == copy || line == mips {
            line = details.next().expect("a decoded blit record");
        }
        // A texture-to-buffer copy keeps its line, which older tests match; its detail follows.
        let detail = (line == read).then(|| reads.next().expect("a decoded read"));
        out.push(line);
        out.extend(detail);
    }
    let mut state = state();
    state.say(encoder, "MTLBlitCommandEncoder_encodeCommands".to_owned());
    for line in out {
        state.say(encoder, format!("  {line}"));
    }
    drop(state);
}

#[unsafe(export_name = "MTLCommandEncoder_endEncoding")]
extern "C-unwind" fn fake_end_encoding(encoder: u64) {
    state().say(encoder, "MTLCommandEncoder_endEncoding".to_owned());
}

#[unsafe(export_name = "MTLCommandEncoder_setLabel")]
const extern "C-unwind" fn fake_set_label(_encoder: u64, _label: u64) {}

/// Slot 72: a view of the window and its layer, both the device's.
#[unsafe(export_name = "CreateMetalViewFromHWND")]
extern "C-unwind" fn fake_create_metal_view(_hwnd: isize, device: u64, layer: *mut u64) -> u64 {
    let mut state = state();
    let (view, made) = (state.made_of(device), state.made_of(device));
    drop(state);
    // SAFETY: the caller's out pointer, writable.
    unsafe { layer.write(made) };
    view
}

/// No VM host's window-system hooks: the presenter takes drawables itself.
#[unsafe(export_name = "WMTGetWsiHooks")]
const extern "C-unwind" fn fake_wsi_hooks() -> *const core::ffi::c_void {
    core::ptr::null()
}

#[unsafe(export_name = "ReleaseMetalView")]
const extern "C-unwind" fn fake_release_metal_view(_view: u64) {}

#[unsafe(export_name = "MetalLayer_getProps")]
const extern "C-unwind" fn fake_layer_get_props(_layer: u64, props: *mut WMTLayerProps) {
    // SAFETY: the caller's props, writable; all-zero is a valid value of the plain fields.
    unsafe { props.write_bytes(0, 1) };
}

#[unsafe(export_name = "MetalLayer_setProps")]
extern "C-unwind" fn fake_layer_set_props(layer: u64, props: *const WMTLayerProps) {
    // SAFETY: caller passes a readable `WMTLayerProps` alive for the call.
    let p = unsafe { &*props };
    state().say(
        layer,
        format!(
            "MetalLayer_setProps format={} size={}x{} display_sync={}",
            p.pixel_format, p.drawable_width, p.drawable_height, p.display_sync_enabled
        ),
    );
}

#[unsafe(export_name = "MetalLayer_setColorSpace")]
extern "C-unwind" fn fake_layer_set_color_space(layer: u64, colorspace: u64) -> bool {
    state().say(layer, format!("MetalLayer_setColorSpace {colorspace}"));
    true
}

#[unsafe(export_name = "MetalLayer_getEDRValue")]
extern "C-unwind" fn fake_layer_edr_value(layer: u64, value: *mut WMTEDRValue) {
    let state = state();
    let edr = state
        .device_of(layer)
        .and_then(|d| state.edr.get(&d).copied());
    drop(state);
    if let Some(edr) = edr {
        // SAFETY: the caller's value, writable.
        unsafe { value.write(edr) };
    } else {
        // SAFETY: the caller's value, writable; all-zero is a valid value of the plain fields.
        unsafe { value.write_bytes(0, 1) };
    }
}

#[unsafe(export_name = "WMTQueryDisplaySettingForLayer")]
const extern "C-unwind" fn fake_query_display_setting(
    _layer: u64,
    version: *mut u64,
    colorspace: *mut u64,
    metadata: *mut WMTHDRMetadata,
    edr: *mut WMTEDRValue,
) {
    // SAFETY: the caller's out pointers, each writable; all-zero is a valid value of each.
    unsafe { version.write(0) };
    // SAFETY: as above.
    unsafe { colorspace.write(0) };
    // SAFETY: as above.
    unsafe { metadata.write_bytes(0, 1) };
    // SAFETY: as above.
    unsafe { edr.write_bytes(0, 1) };
}

#[unsafe(export_name = "MetalLayer_nextDrawable")]
extern "C-unwind" fn fake_next_drawable(layer: u64) -> u64 {
    let mut state = state();
    if let Some(dev) = state.device_of(layer)
        && state.next_drawable_zero.get(&dev).copied().unwrap_or(false)
    {
        return 0;
    }
    state.made_of(layer)
}

#[unsafe(export_name = "MetalDrawable_texture")]
extern "C-unwind" fn fake_drawable_texture(drawable: u64) -> u64 {
    let mut state = state();
    let handle = state.made_of(drawable);
    state.drawable_textures.insert(handle);
    handle
}

#[unsafe(export_name = "MTLCommandBuffer_encodeSpatialScale")]
extern "C-unwind" fn fake_encode_spatial_scale(
    cmdbuf: u64,
    scaler: u64,
    color: u64,
    output: u64,
    fence: u64,
) {
    state().say(
        cmdbuf,
        format!(
            "MTLCommandBuffer_encodeSpatialScale scaler=#{scaler} color=#{color} output=#{output} fence=#{fence}"
        ),
    );
}

#[unsafe(export_name = "MTLCommandBuffer_presentDrawable")]
extern "C-unwind" fn fake_present_drawable(cmdbuf: u64, _drawable: u64) {
    state().say(cmdbuf, "MTLCommandBuffer_presentDrawable".to_owned());
}

#[unsafe(export_name = "MTLCommandBuffer_presentDrawableAfterMinimumDuration")]
extern "C-unwind" fn fake_present_drawable_after(cmdbuf: u64, _drawable: u64, after: f64) {
    state().say(
        cmdbuf,
        format!("MTLCommandBuffer_presentDrawableAfterMinimumDuration after={after}"),
    );
}

#[unsafe(export_name = "WMTDeclareLatencyCriticalActivity")]
const extern "C-unwind" fn fake_declare_latency_critical_activity() -> u64 {
    1
}

#[unsafe(export_name = "WMTAttachMetalView")]
extern "C-unwind" fn fake_attach_metal_view(
    _hwnd: isize,
    device: u64,
    desc: *const WMTViewAttachDesc,
    out_poll: *mut WMTViewPollOut,
    ret_layer: *mut u64,
) -> u64 {
    // SAFETY: caller supplies a readable `WMTViewAttachDesc` or null.
    let attach_desc = unsafe { desc.as_ref() }.copied();
    let mut state = state();
    let (view, layer) = (state.made_of(device), state.made_of(device));
    let mut poll = state.view_poll.get(&device).copied().unwrap_or_else(|| {
        let flags = attach_desc.map_or(0, |d| {
            let mut f = 0u32;
            if d.flags & WMT_VIEW_ATTACH_FLAG_HDR_ENABLE != 0 {
                f |= WMT_VIEW_POLL_FLAG_HDR_ACTIVE;
            }
            if d.flags & WMT_VIEW_ATTACH_FLAG_SOFTWARE_CURSOR_ON != 0 {
                f |= WMT_VIEW_POLL_FLAG_SOFTWARE_CURSOR_ACTIVE;
            }
            f
        });
        WMTViewPollOut {
            drawable_width: attach_desc.map_or(0, |d| d.width),
            drawable_height: attach_desc.map_or(0, |d| d.height),
            backing_scale: 1,
            edr_current: 1.0,
            edr_potential: 1.0,
            panel_max_hz: 60,
            flags,
            reserved: 0,
            screen_sizes: 0,
        }
    });
    if let Some((w, h)) = state.drawable_size.get(&device).copied() {
        poll.drawable_width = u32::try_from(w).unwrap_or(u32::MAX);
        poll.drawable_height = u32::try_from(h).unwrap_or(u32::MAX);
    }
    state.say(
        device,
        format!(
            "WMTAttachMetalView view=#{view} layer=#{layer} flags={:#x}",
            poll.flags
        ),
    );
    drop(state);
    // SAFETY: caller provides writable `ret_layer`.
    unsafe { ret_layer.write(layer) };
    if !out_poll.is_null() {
        // SAFETY: caller provides writable `out_poll`.
        unsafe { out_poll.write(poll) };
    }
    view
}

#[unsafe(export_name = "WMTRetireMetalView")]
extern "C-unwind" fn fake_retire_metal_view(view: u64, flags: u32) {
    state().say(
        view,
        format!("WMTRetireMetalView view=#{view} flags={flags:#x}"),
    );
}

#[unsafe(export_name = "WMTViewSyncAndPoll")]
extern "C-unwind" fn fake_view_sync_and_poll(
    view: u64,
    layer: u64,
    flags: u32,
    out_poll: *mut WMTViewPollOut,
) {
    let mut state = state();
    let dev = state
        .device_of(view)
        .or_else(|| state.device_of(layer))
        .unwrap_or(0);
    let mut poll = state
        .view_poll
        .get(&dev)
        .copied()
        .unwrap_or(WMTViewPollOut {
            drawable_width: 0,
            drawable_height: 0,
            backing_scale: 1,
            edr_current: 1.0,
            edr_potential: 1.0,
            panel_max_hz: 60,
            flags: 0,
            reserved: 0,
            screen_sizes: 0,
        });
    if let Some((w, h)) = state.drawable_size.get(&dev).copied() {
        poll.drawable_width = u32::try_from(w).unwrap_or(u32::MAX);
        poll.drawable_height = u32::try_from(h).unwrap_or(u32::MAX);
    }
    if let Some(edr) = state.edr.get(&dev).copied() {
        poll.edr_current = edr.maximum_edr_color_component_value;
        poll.edr_potential = edr.maximum_potential_edr_color_component_value;
    }
    state.say(
        dev,
        format!(
            "WMTViewSyncAndPoll view=#{view} req_flags={flags:#x} dw={} dh={} hz={} flags={:#x}",
            poll.drawable_width, poll.drawable_height, poll.panel_max_hz, poll.flags
        ),
    );
    drop(state);
    if !out_poll.is_null() {
        // SAFETY: caller provides writable `out_poll`.
        unsafe { out_poll.write(poll) };
    }
}

#[unsafe(export_name = "WMTPresentClientSurface")]
extern "C-unwind" fn fake_present_client_surface(view: u64, reasons: u32) {
    state().say(
        view,
        format!("WMTPresentClientSurface view=#{view} reasons={reasons:#x}"),
    );
}

#[unsafe(export_name = "MTLDrawable_setPresentedEvent")]
extern "C-unwind" fn fake_drawable_set_presented_event(
    drawable: u64,
    view: u64,
    shared_event: u64,
    signal_value: u64,
    flags: u32,
) {
    let target = if drawable != 0 {
        drawable
    } else if view != 0 {
        view
    } else {
        shared_event
    };
    state().say(
        target,
        format!(
            "MTLDrawable_setPresentedEvent drawable=#{drawable} view=#{view} event=#{shared_event} value={signal_value} flags={flags:#x}"
        ),
    );
}

#[unsafe(export_name = "WMTSetCursorOverlay")]
extern "C-unwind" fn fake_set_cursor_overlay(view: u64, params: *const WMTCursorOverlayParams) {
    // SAFETY: caller supplies a readable `WMTCursorOverlayParams` or null.
    let flags = unsafe { params.as_ref() }.map_or(0, |p| p.flags);
    state().say(
        view,
        format!("WMTSetCursorOverlay view=#{view} flags={flags:#x}"),
    );
}
