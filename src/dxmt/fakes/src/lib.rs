//! The winemetal fakes a front end's host tests run `dxmt-core` on.
//!
//! Each fake is the winemetal function of its `export_name`, so a test binary that links this
//! crate runs a core [`Device`](dxmt_core::device::Device) on a made-up handle
//! (`Device::headless`) through its queue's real encode and finish threads. Only a front end's
//! own tests link it, in a cargo command of their own: `dxmt-core`'s, d3d11's and d3d12's test
//! binaries define their own fakes, and two definitions of one symbol do not link.
//!
//! What Metal is asked to do is captured per device ([`take_calls`]): the command buffers'
//! encoders (a render pass's first colour target as a `color0` line), the records each
//! `encodeCommands` is handed (as [`format_pass_records`] prints them), the ends, the commits,
//! pipelines and presents. Objects a device's queue makes carry
//! its handle, so tests running at once each read their own device's calls. What the core logs
//! goes to [`take_log`]. The capture manager and the scalers have no fakes: a call to one aborts
//! the test (the test binary links with `-undefined dynamic_lookup`).

use core::{alloc::Layout, ffi::c_char};
use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard, PoisonError},
};

use dxmt_core::test_support::{Records, format_pass_records};
use winemetal_sys::wire::{
    WMT_BLIT_COMMAND_COPY_FROM_BUFFER_TO_TEXTURE, WMT_BLIT_COMMAND_COPY_FROM_TEXTURE_TO_BUFFER,
    WMT_BLIT_COMMAND_GENERATE_MIPMAPS, WMT_COMMAND_BUFFER_STATUS_COMPLETED, WMTBufferInfo,
    WMTDepthStencilInfo, WMTEDRValue, WMTFunctionConstant, WMTHDRMetadata, WMTLayerProps,
    WMTRenderPassInfo, WMTRenderPipelineInfo, WMTSampleBufferAttachmentInfo, WMTSamplerInfo,
    WMTTextureInfo, WMTTextureSwizzleChannels, WMTVertexDescriptorInfo, WmtcmdBase,
    WmtcmdBlitCopyFromBufferToTexture, WmtcmdBlitCopyFromTextureToBuffer,
    WmtcmdBlitGenerateMipmaps,
};

/// The first handle the fakes make: above every made-up device handle (at most 0x1000).
const FIRST_HANDLE: u64 = 0x10_0000;

/// What the fakes made and saw, for every thread: the encode thread calls them, not the test's.
struct State {
    next: u64,
    /// The device each object a device's queue made belongs to.
    owner: BTreeMap<u64, u64>,
    /// Each call a test reads, after its device.
    calls: Vec<(u64, String)>,
    log: Vec<(u32, String)>,
}

/// The fakes' one [`State`].
///
/// A static because the caller has no object to reach: a fake is a C function that gets only
/// handles, called on the queue's threads, so it keys what it records by the device the handle
/// belongs to.
static STATE: Mutex<State> = Mutex::new(State {
    next: FIRST_HANDLE,
    owner: BTreeMap::new(),
    calls: Vec::new(),
    log: Vec::new(),
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
const extern "C-unwind" fn fake_pixel_format(_texture: u64) -> u32 {
    0
}

#[unsafe(export_name = "MTLTexture_width")]
const extern "C-unwind" fn fake_width(_texture: u64) -> u64 {
    0
}

#[unsafe(export_name = "MTLTexture_height")]
const extern "C-unwind" fn fake_height(_texture: u64) -> u64 {
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

#[unsafe(export_name = "MTLDevice_newRenderPipelineState")]
extern "C-unwind" fn fake_new_render_pipeline(
    device: u64,
    _info: *const WMTRenderPipelineInfo,
    _error: *mut u64,
) -> u64 {
    let mut state = state();
    state.say(device, "MTLDevice_newRenderPipelineState".to_owned());
    state.made_of(device)
}

#[unsafe(export_name = "MTLDevice_newRenderPipelineStateWithVertexDescriptor")]
extern "C-unwind" fn fake_new_render_pipeline_with_vertex_descriptor(
    device: u64,
    info: *const WMTRenderPipelineInfo,
    _vertex: *const WMTVertexDescriptorInfo,
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
    // After the line the older tests read: the planes the pipeline declares, when it does.
    if depth != 0 || stencil != 0 {
        state.say(device, format!("  pso depth={depth} stencil={stencil}"));
    }
    if device == REFUSES_PIPELINES {
        return 0;
    }
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
extern "C-unwind" fn fake_dispatch_data(_bytes: u64, _length: u64) -> u64 {
    state().made_of(0)
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
const extern "C-unwind" fn fake_status(_cmdbuf: u64) -> u64 {
    WMT_COMMAND_BUFFER_STATUS_COMPLETED
}

#[unsafe(export_name = "MTLCommandBuffer_waitUntilCompleted")]
const extern "C-unwind" fn fake_wait_until_completed(_cmdbuf: u64) {}

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
    encode(
        "MTLRenderCommandEncoder_encodeCommands",
        &Records::Render,
        encoder,
        head,
    );
}

/// A blit encoder's records, with the copies into textures and the mipmap generations in full.
///
/// [`format_pass_records`] prints a blit record as its type alone; the texture uploads a front
/// end's tests assert on are decoded here.
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
        let mips = format!("blit {WMT_BLIT_COMMAND_GENERATE_MIPMAPS}");
        if line == typed || line == mips {
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
const extern "C-unwind" fn fake_layer_set_props(_layer: u64, _props: *const WMTLayerProps) {}

#[unsafe(export_name = "MetalLayer_setColorSpace")]
const extern "C-unwind" fn fake_layer_set_color_space(_layer: u64, _colorspace: u64) -> bool {
    true
}

#[unsafe(export_name = "MetalLayer_getEDRValue")]
const extern "C-unwind" fn fake_layer_edr_value(_layer: u64, value: *mut WMTEDRValue) {
    // SAFETY: the caller's value, writable; all-zero is a valid value of the plain fields.
    unsafe { value.write_bytes(0, 1) };
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
    state().made_of(layer)
}

#[unsafe(export_name = "MetalDrawable_texture")]
extern "C-unwind" fn fake_drawable_texture(drawable: u64) -> u64 {
    state().made_of(drawable)
}

#[unsafe(export_name = "MTLCommandBuffer_presentDrawable")]
extern "C-unwind" fn fake_present_drawable(cmdbuf: u64, _drawable: u64) {
    state().say(cmdbuf, "MTLCommandBuffer_presentDrawable".to_owned());
}

#[unsafe(export_name = "MTLCommandBuffer_presentDrawableAfterMinimumDuration")]
extern "C-unwind" fn fake_present_drawable_after(cmdbuf: u64, _drawable: u64, _after: f64) {
    state().say(
        cmdbuf,
        "MTLCommandBuffer_presentDrawableAfterMinimumDuration".to_owned(),
    );
}
