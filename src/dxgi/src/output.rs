//! The output, `IMTLDXGIOutput`: a monitor, its display modes and this object's gamma ramp.
//!
//! `dxgi_output.cpp`. The adapter makes a new output at each `EnumOutputs`, so two outputs of one
//! monitor keep two ramps. d3d11's swap chain reads the ramp through slot 29 at each fullscreen
//! present, on the present thread and with no lock: the curves are atomics, stored before
//! `version` moves on.

use core::{
    ffi::c_void,
    mem::offset_of,
    ptr,
    sync::atomic::{AtomicU32, AtomicU64, Ordering},
};

use dxmt_util::{com::vtbl, float_cast::u32_to_f32};
use winemetal_wire::WMTDisplayDescription;

use crate::{
    LOG,
    abi::{
        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020,
        DXGI_ENUM_MODES_SCALING, DXGI_ERROR_INVALID_CALL, DXGI_ERROR_MORE_DATA,
        DXGI_ERROR_NOT_FOUND, DXGI_ERROR_UNSUPPORTED, DXGI_FORMAT_UNKNOWN,
        DXGI_MODE_ROTATION_UNSPECIFIED, E_FAIL, E_INVALIDARG, E_NOTIMPL, GAMMA_CP_COUNT,
        GammaControl, GammaControlCapabilities, GammaRamp, Guid, Hmonitor, Hresult,
        IID_IDXGI_OBJECT, IID_IDXGI_OUTPUT, IID_IDXGI_OUTPUT1, IID_IDXGI_OUTPUT2,
        IID_IDXGI_OUTPUT3, IID_IDXGI_OUTPUT4, IID_IDXGI_OUTPUT5, IID_IDXGI_OUTPUT6,
        IID_IMTL_DXGI_OUTPUT, IID_IUNKNOWN, IUnknownVtbl, ModeDesc, ModeDesc1, ObjectVtbl,
        OutputDesc, OutputDesc1, OutputVtbl, Rational, Rect, Rgb, S_OK,
    },
    com::{self, Head},
    ffi, modes,
    private_data::PrivateData,
    wsi::{self, WsiMode},
};

/// The IIDs the output answers with itself.
const ANSWERED: [Guid; 10] = [
    IID_IUNKNOWN,
    IID_IDXGI_OBJECT,
    IID_IDXGI_OUTPUT,
    IID_IDXGI_OUTPUT1,
    IID_IDXGI_OUTPUT2,
    IID_IDXGI_OUTPUT3,
    IID_IDXGI_OUTPUT4,
    IID_IDXGI_OUTPUT5,
    IID_IDXGI_OUTPUT6,
    IID_IMTL_DXGI_OUTPUT,
];

/// The bytes of a `DXGI_OUTPUT_DESC1` that `GetDesc1` writes: all but its tail padding.
const DESC1_WRITTEN: usize = offset_of!(OutputDesc1, max_full_frame_luminance) + size_of::<f32>();

/// The bytes of a `DXGI_GAMMA_CONTROL_CAPABILITIES` the output writes: all but the 1025th position.
const CAPABILITIES_WRITTEN: usize = offset_of!(GammaControlCapabilities, control_point_positions)
    + GAMMA_CP_COUNT * size_of::<f32>();

/// The bytes of a `DXGI_GAMMA_CONTROL` the output writes: all but the curve's 1025th point.
const CONTROL_WRITTEN: usize =
    offset_of!(GammaControl, gamma_curve) + GAMMA_CP_COUNT * size_of::<Rgb>();

/// The output's slots.
static OUTPUT_VTBL: OutputVtbl = OutputVtbl {
    object: ObjectVtbl {
        unknown: IUnknownVtbl {
            query_interface: output_query_interface,
            add_ref: output_add_ref,
            release: output_release,
        },
        set_private_data: output_set_private_data,
        set_private_data_interface: output_set_private_data_interface,
        get_private_data: output_get_private_data,
        get_parent: output_get_parent,
    },
    get_desc: output_get_desc,
    get_display_mode_list: output_get_display_mode_list,
    find_closest_matching_mode: output_find_closest_matching_mode,
    wait_for_v_blank: output_wait_for_v_blank,
    take_ownership: output_take_ownership,
    release_ownership: output_release_ownership,
    get_gamma_control_capabilities: output_get_gamma_control_capabilities,
    set_gamma_control: output_set_gamma_control,
    get_gamma_control: output_get_gamma_control,
    set_display_surface: output_set_display_surface,
    get_display_surface_data: output_get_display_surface_data,
    get_frame_statistics: output_get_frame_statistics,
    get_display_mode_list1: output_get_display_mode_list1,
    find_closest_matching_mode1: output_find_closest_matching_mode1,
    get_display_surface_data1: output_get_display_surface_data1,
    duplicate_output: output_duplicate_output,
    supports_overlays: output_supports_overlays,
    check_overlay_support: output_check_overlay_support,
    check_overlay_color_space_support: output_check_overlay_color_space_support,
    duplicate_output1: output_duplicate_output1,
    get_desc1: output_get_desc1,
    check_hardware_composition_support: output_check_hardware_composition_support,
    get_gamma_ramp: output_get_gamma_ramp,
};

/// A new output with one reference on `monitor`; it takes a public reference to `adapter`.
///
/// `force_sdr` is the adapter's `dxgi.forceSDR`. The display is described once, now: winemetal's
/// primary display for the default monitor, its secondary one for any other.
///
/// # Safety
///
/// `adapter` is a live adapter of this crate.
#[must_use]
pub unsafe fn create(adapter: *mut c_void, monitor: Hmonitor, force_sdr: bool) -> *mut c_void {
    let display = if wsi::default_monitor().unwrap_or(ptr::null_mut()) == monitor {
        // SAFETY: winemetal's display ids take no argument.
        unsafe { ffi::WMTGetPrimaryDisplayId() }
    } else {
        // SAFETY: as above.
        unsafe { ffi::WMTGetSecondaryDisplayId() }
    };
    let mut description = WMTDisplayDescription {
        red_primaries: [0.0; 2],
        blue_primaries: [0.0; 2],
        green_primaries: [0.0; 2],
        white_points: [0.0; 2],
        maximum_edr_color_component_value: 0.0,
        maximum_potential_edr_color_component_value: 0.0,
        maximum_reference_edr_color_component_value: 0.0,
    };
    // SAFETY: `description` is a local the call fills.
    unsafe { ffi::WMTGetDisplayDescription(display, &raw mut description) };
    // SAFETY: the caller's: `adapter` is live.
    unsafe { new(adapter, monitor, force_sdr, description) }
}

/// An output: the C++'s `MTLDXGIOutputImpl`, its address the `this` of every slot.
#[repr(C)]
struct Output {
    head: Head<OutputVtbl>,
    /// A public reference to the adapter that made the output.
    adapter: *mut c_void,
    monitor: Hmonitor,
    /// `dxgi.forceSDR`: `GetDesc1` answers SDR whatever the display.
    force_sdr: bool,
    /// The display, as winemetal described it when the output was made.
    display: WMTDisplayDescription,
    private_data: PrivateData,
    ramp: GammaRamp,
}

impl Output {
    /// The output at `this`.
    ///
    /// # Safety
    ///
    /// `this` is a live output: the `this` of one of its slots.
    const unsafe fn from_this<'a>(this: *mut c_void) -> &'a Self {
        // SAFETY: the caller's.
        unsafe { &*this.cast::<Self>() }
    }

    /// The modes the output lists for `format` and `flags`.
    ///
    /// For a format no back buffer has, none, and the window system is not asked.
    fn modes(&self, format: u32, flags: u32) -> Vec<ModeDesc1> {
        if !modes::is_back_buffer_format(format) {
            return Vec::new();
        }
        let raw: Vec<WsiMode> = (0..=u32::MAX)
            .map_while(|number| wsi::display_mode(self.monitor, number))
            .collect();
        modes::mode_list(&raw, format, flags)
    }

    /// `GetDisplayModeList` and `GetDisplayModeList1`: the modes listed, to `desc`.
    ///
    /// A null `count` is `DXGI_ERROR_INVALID_CALL`. With a null `desc` the count is the list's
    /// length; else `modes::copy_out`'s rule, at most `*count` modes written, each as `convert`
    /// makes it.
    ///
    /// # Safety
    ///
    /// `count` is null or addresses a readable and writable `u32`; `desc` is null or addresses
    /// `*count` writable `T`s. Neither needs to be aligned.
    unsafe fn list<T>(
        &self,
        format: u32,
        flags: u32,
        count: *mut u32,
        desc: *mut T,
        convert: impl Fn(ModeDesc1) -> T,
    ) -> Hresult {
        if count.is_null() {
            return DXGI_ERROR_INVALID_CALL;
        }
        let list = self.modes(format, flags);
        if desc.is_null() {
            let listed = u32::try_from(list.len()).unwrap_or(u32::MAX);
            // SAFETY: the caller's: `count` is non-null here; the write assumes no alignment.
            unsafe { count.write_unaligned(listed) };
            return S_OK;
        }
        // SAFETY: as above, a read.
        let room = unsafe { count.read_unaligned() };
        let answer = modes::copy_out(list, room, |i, mode| {
            // SAFETY: the caller's: `desc` holds `room` modes and `i` is below it; no alignment.
            unsafe { desc.wrapping_add(i).write_unaligned(convert(mode)) };
        });
        let Some(listed) = answer else {
            return DXGI_ERROR_MORE_DATA;
        };
        // SAFETY: as above, a write.
        unsafe { count.write_unaligned(listed) };
        S_OK
    }

    /// `FindClosestMatchingMode1` once its pointers are checked: the closest listed mode.
    ///
    /// `device` is whether the caller passed one. A format of `UNKNOWN` without a device, or a
    /// width without a height or the other way round, is `DXGI_ERROR_INVALID_CALL`.
    fn closest(&self, target: &ModeDesc1, device: bool) -> Result<ModeDesc1, Hresult> {
        if target.format == DXGI_FORMAT_UNKNOWN && !device {
            return Err(DXGI_ERROR_INVALID_CALL);
        }
        if (target.width == 0) != (target.height == 0) {
            return Err(DXGI_ERROR_INVALID_CALL);
        }
        let current = wsi::current_display_mode(self.monitor).unwrap_or_default();
        let current = modes::from_wsi(&current);
        let format = if target.format == DXGI_FORMAT_UNKNOWN {
            current.format
        } else {
            target.format
        };
        let list = self.modes(format, DXGI_ENUM_MODES_SCALING);
        if list.is_empty() {
            LOG.err("No modes found");
            return Err(DXGI_ERROR_NOT_FOUND);
        }
        let found = modes::find_closest(list, target, &current).ok_or(DXGI_ERROR_NOT_FOUND)?;
        LOG.debug(&format!(
            "DXGI: For mode {}x{}@{} found closest mode {}x{}@{}",
            target.width,
            target.height,
            whole_rate(target.refresh_rate),
            found.width,
            found.height,
            whole_rate(found.refresh_rate)
        ));
        Ok(found)
    }

    /// `GetDesc` and `GetDesc1`: the first `len` bytes of its `DXGI_OUTPUT_DESC1`, to `out`.
    ///
    /// A `DXGI_OUTPUT_DESC` is that struct's first part. When the name fails the rectangle has
    /// been written, as the C++ writes it in place first.
    ///
    /// # Safety
    ///
    /// `out` addresses `len` writable bytes, `len` at most a `DXGI_OUTPUT_DESC1`; no alignment.
    unsafe fn describe(&self, out: *mut OutputDesc1, len: usize) -> Hresult {
        let Some(rect) = wsi::desktop_coordinates(self.monitor) else {
            LOG.err("Failed to query monitor coords");
            return E_FAIL;
        };
        let Some(name) = wsi::display_name(self.monitor) else {
            let at = out
                .wrapping_byte_add(offset_of!(OutputDesc1, desktop_coordinates))
                .cast::<Rect>();
            // SAFETY: the caller's: the rectangle lies in both structs; no alignment.
            unsafe { at.write_unaligned(rect) };
            LOG.err("Failed to query monitor name");
            return E_FAIL;
        };
        let edr = self.display.maximum_potential_edr_color_component_value;
        let (bits_per_color, color_space) = if !self.force_sdr && edr > 1.0 {
            (10, DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020)
        } else {
            (8, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709)
        };
        let max_luminance = edr * 100.0;
        let desc = OutputDesc1 {
            device_name: name,
            desktop_coordinates: rect,
            attached_to_desktop: 1,
            rotation: DXGI_MODE_ROTATION_UNSPECIFIED,
            monitor: self.monitor,
            bits_per_color,
            color_space,
            red_primary: self.display.red_primaries,
            green_primary: self.display.green_primaries,
            blue_primary: self.display.blue_primaries,
            white_point: self.display.white_points,
            min_luminance: 0.0,
            max_luminance,
            max_full_frame_luminance: max_luminance,
        };
        // SAFETY: the caller's.
        unsafe { write_prefix(out, &desc, len) };
        S_OK
    }

    /// `SetGammaControl`: the first 1024 points of `control`'s curve become the ramp's.
    ///
    /// The ramp's version moves on after them.
    ///
    /// # Safety
    ///
    /// `control` addresses a readable `DXGI_GAMMA_CONTROL`; no alignment.
    unsafe fn set_gamma(&self, control: *const GammaControl) {
        let curve = control
            .wrapping_byte_add(offset_of!(GammaControl, gamma_curve))
            .cast::<Rgb>();
        let ramp = &self.ramp;
        let points = ramp.red.iter().zip(&ramp.green).zip(&ramp.blue);
        for (i, ((red, green), blue)) in points.enumerate() {
            // SAFETY: the caller's: point `i`, below 1024, of the curve's 1025; no alignment.
            let point = unsafe { curve.wrapping_add(i).read_unaligned() };
            red.store(point.red.to_bits(), Ordering::Relaxed);
            green.store(point.green.to_bits(), Ordering::Relaxed);
            blue.store(point.blue.to_bits(), Ordering::Relaxed);
        }
        ramp.version.fetch_add(1, Ordering::Release);
    }

    /// `GetGammaControl`'s answer: scale 1, offset 0 and the ramp's curves.
    ///
    /// The curve's 1025th point, which the ramp lacks, is 0 and never written.
    fn gamma(&self) -> GammaControl {
        let point = |curve: &[AtomicU32; GAMMA_CP_COUNT], i: usize| {
            curve
                .get(i)
                .map_or(0.0, |value| f32::from_bits(value.load(Ordering::Relaxed)))
        };
        let ramp = &self.ramp;
        GammaControl {
            scale: Rgb {
                red: 1.0,
                green: 1.0,
                blue: 1.0,
            },
            offset: Rgb {
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            },
            gamma_curve: core::array::from_fn(|i| Rgb {
                red: point(&ramp.red, i),
                green: point(&ramp.green, i),
                blue: point(&ramp.blue, i),
            }),
        }
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        // SAFETY: the reference `new` took to the adapter, given back once.
        unsafe { dxmt_util::com::release(self.adapter) };
    }
}

/// A new output, as `create` makes it, with its display described already.
///
/// The ramp starts as the identity, at version 0.
///
/// # Safety
///
/// `adapter` is a live COM object: an adapter of this crate, or a test's stand-in.
unsafe fn new(
    adapter: *mut c_void,
    monitor: Hmonitor,
    force_sdr: bool,
    display: WMTDisplayDescription,
) -> *mut c_void {
    // SAFETY: the caller's: a live COM object starts with `IUnknown`'s slots.
    let unknown = unsafe { vtbl::<IUnknownVtbl>(adapter) };
    // SAFETY: as above; the output keeps the reference.
    unsafe { (unknown.add_ref)(adapter) };
    let curve = || core::array::from_fn(|i| AtomicU32::new(identity(i).to_bits()));
    Box::into_raw(Box::new(Output {
        head: Head::new(&OUTPUT_VTBL),
        adapter,
        monitor,
        force_sdr,
        display,
        private_data: PrivateData::default(),
        ramp: GammaRamp {
            red: curve(),
            green: curve(),
            blue: curve(),
            version: AtomicU64::new(0),
        },
    }))
    .cast()
}

/// Point `i` of the identity curve: `i / 1023`, divided in `f32` as the C++ divides.
fn identity(i: usize) -> f32 {
    let float = |n: usize| u32::try_from(n).map_or(f32::MAX, u32_to_f32);
    float(i) / float(GAMMA_CP_COUNT - 1)
}

/// A refresh rate in whole hertz, 0 for a zero denominator, as the closest mode's log line has it.
const fn whole_rate(rate: Rational) -> u32 {
    match rate.numerator.checked_div(rate.denominator) {
        Some(whole) => whole,
        None => 0,
    }
}

/// A listed mode as `GetDisplayModeList` and `FindClosestMatchingMode` give it: no stereo flag.
const fn mode_desc(mode: &ModeDesc1) -> ModeDesc {
    ModeDesc {
        width: mode.width,
        height: mode.height,
        refresh_rate: mode.refresh_rate,
        format: mode.format,
        scanline_ordering: mode.scanline_ordering,
        scaling: mode.scaling,
    }
}

/// Writes the first `len` bytes of `value` to `out`, which promises no alignment.
///
/// The C++ fills an application's struct field by field; this leaves the bytes after `len` (tail
/// padding, a point past DXMT's 1024) as the application had them.
///
/// # Safety
///
/// `out` addresses `len` writable bytes.
unsafe fn write_prefix<T>(out: *mut T, value: &T, len: usize) {
    let len = len.min(size_of::<T>());
    // SAFETY: the caller's; `value` holds `len` bytes, and a byte copy assumes no alignment.
    unsafe { ptr::copy_nonoverlapping(ptr::from_ref(value).cast::<u8>(), out.cast::<u8>(), len) };
}

/// `DuplicateOutput` and `DuplicateOutput1`: unsupported.
///
/// `out` is nulled first; without a device the answer is `E_INVALIDARG`.
///
/// # Safety
///
/// `out` is null or addresses a writable pointer; no alignment.
unsafe fn duplicate(device: *mut c_void, out: *mut *mut c_void) -> Hresult {
    if !out.is_null() {
        // SAFETY: the caller's: `out` is non-null here.
        unsafe { com::write_out(out, ptr::null_mut()) };
    }
    if device.is_null() {
        return E_INVALIDARG;
    }
    LOG.err("Not implemented");
    DXGI_ERROR_UNSUPPORTED
}

/// The application's flags `flags` set to 0 if it is not null: `S_OK`.
///
/// # Safety
///
/// `flags` is null or addresses a writable `u32`; no alignment.
const unsafe fn no_flags(flags: *mut u32) -> Hresult {
    if !flags.is_null() {
        // SAFETY: the caller's: `flags` is non-null here; the write assumes no alignment.
        unsafe { flags.write_unaligned(0) };
    }
    S_OK
}

/// A slot DXMT does not implement: its log line and `E_NOTIMPL`.
fn not_implemented() -> Hresult {
    LOG.err("Not implemented");
    E_NOTIMPL
}

extern "system" fn output_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let log_as = (&IID_IDXGI_OUTPUT, "DXGIOutput");
    // SAFETY: the slot's `this` is a live output, whose `AddRef` is `output_add_ref`; `riid` and
    // `out` are the application's, null or valid, with no alignment promised.
    unsafe { com::query_interface(this, riid, out, &ANSWERED, &[], log_as, output_add_ref) }
}

extern "system" fn output_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live output.
    unsafe { com::add_ref::<Output>(this) }
}

extern "system" fn output_release(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live output, from `Box::into_raw`.
    unsafe { com::release::<Output>(this) }
}

extern "system" fn output_set_private_data(
    this: *mut c_void,
    guid: *const Guid,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: the application's GUID pointer: null or a GUID, with no alignment promised.
    let Some(guid) = (unsafe { Guid::read(guid) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's `data`: null or `size` readable bytes.
    unsafe { output.private_data.set_data(guid, size, data) }
}

extern "system" fn output_set_private_data_interface(
    this: *mut c_void,
    guid: *const Guid,
    unknown: *mut c_void,
) -> Hresult {
    // SAFETY: the application's GUID pointer: null or a GUID, with no alignment promised.
    let Some(guid) = (unsafe { Guid::read(guid) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's `unknown`: null or a live COM object.
    unsafe { output.private_data.set_interface(guid, unknown) }
}

extern "system" fn output_get_private_data(
    this: *mut c_void,
    guid: *const Guid,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: the application's GUID pointer: null or a GUID, with no alignment promised.
    let Some(guid) = (unsafe { Guid::read(guid) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's `size` and `data`: each null or valid for `*size` bytes.
    unsafe { output.private_data.get_data(guid, size, data) }
}

extern "system" fn output_get_parent(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the output holds a reference to its adapter, a live COM object.
    let adapter = unsafe { vtbl::<IUnknownVtbl>(output.adapter) };
    // SAFETY: the adapter is live; `riid` and `out` go to its `QueryInterface` as they came.
    unsafe { (adapter.query_interface)(output.adapter, riid, out) }
}

extern "system" fn output_get_desc(this: *mut c_void, desc: *mut OutputDesc) -> Hresult {
    if desc.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's non-null `desc`, a `DXGI_OUTPUT_DESC`, which a
    // `DXGI_OUTPUT_DESC1` starts with; no alignment promised.
    unsafe { output.describe(desc.cast(), size_of::<OutputDesc>()) }
}

extern "system" fn output_get_display_mode_list(
    this: *mut c_void,
    format: u32,
    flags: u32,
    count: *mut u32,
    desc: *mut ModeDesc,
) -> Hresult {
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's `count`, null or a `UINT`, and `desc`, null or `*count` modes;
    // no alignment promised.
    unsafe { output.list(format, flags, count, desc, |mode| mode_desc(&mode)) }
}

extern "system" fn output_find_closest_matching_mode(
    this: *mut c_void,
    target: *const ModeDesc,
    closest: *mut ModeDesc,
    device: *mut c_void,
) -> Hresult {
    if target.is_null() || closest.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the application's non-null `target`; the read assumes no alignment.
    let target = unsafe { target.read_unaligned() };
    let target = ModeDesc1 {
        width: target.width,
        height: target.height,
        refresh_rate: target.refresh_rate,
        format: target.format,
        scanline_ordering: target.scanline_ordering,
        scaling: target.scaling,
        stereo: 0,
    };
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    match output.closest(&target, !device.is_null()) {
        Ok(mode) => {
            // SAFETY: the application's non-null `closest`; the write assumes no alignment.
            unsafe { closest.write_unaligned(mode_desc(&mode)) };
            S_OK
        }
        Err(hr) => hr,
    }
}

const extern "system" fn output_wait_for_v_blank(_this: *mut c_void) -> Hresult {
    S_OK
}

extern "system" fn output_take_ownership(
    _this: *mut c_void,
    _device: *mut c_void,
    _exclusive: i32,
) -> Hresult {
    LOG.warn("TakeOwnership: Stub");
    S_OK
}

extern "system" fn output_release_ownership(_this: *mut c_void) {
    LOG.warn("ReleaseOwnership: Stub");
}

extern "system" fn output_get_gamma_control_capabilities(
    _this: *mut c_void,
    capabilities: *mut GammaControlCapabilities,
) -> Hresult {
    if capabilities.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    let answer = GammaControlCapabilities {
        scale_and_offset_supported: 0,
        max_converted_value: 1.0,
        min_converted_value: 0.0,
        num_gamma_control_points: u32::try_from(GAMMA_CP_COUNT).unwrap_or(u32::MAX),
        control_point_positions: core::array::from_fn(identity),
    };
    // SAFETY: the application's non-null `capabilities`, a whole struct; no alignment promised.
    unsafe { write_prefix(capabilities, &answer, CAPABILITIES_WRITTEN) };
    S_OK
}

extern "system" fn output_set_gamma_control(
    this: *mut c_void,
    control: *const GammaControl,
) -> Hresult {
    if control.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's non-null `control`, a whole struct; no alignment promised.
    unsafe { output.set_gamma(control) };
    S_OK
}

extern "system" fn output_get_gamma_control(
    this: *mut c_void,
    control: *mut GammaControl,
) -> Hresult {
    if control.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    let answer = output.gamma();
    // SAFETY: the application's non-null `control`, a whole struct; no alignment promised.
    unsafe { write_prefix(control, &answer, CONTROL_WRITTEN) };
    S_OK
}

extern "system" fn output_set_display_surface(
    _this: *mut c_void,
    _surface: *mut c_void,
) -> Hresult {
    not_implemented()
}

extern "system" fn output_get_display_surface_data(
    _this: *mut c_void,
    _surface: *mut c_void,
) -> Hresult {
    not_implemented()
}

extern "system" fn output_get_frame_statistics(_this: *mut c_void, _stats: *mut c_void) -> Hresult {
    not_implemented()
}

extern "system" fn output_get_display_mode_list1(
    this: *mut c_void,
    format: u32,
    flags: u32,
    count: *mut u32,
    desc: *mut ModeDesc1,
) -> Hresult {
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's `count`, null or a `UINT`, and `desc`, null or `*count` modes;
    // no alignment promised.
    unsafe { output.list(format, flags, count, desc, core::convert::identity) }
}

extern "system" fn output_find_closest_matching_mode1(
    this: *mut c_void,
    target: *const ModeDesc1,
    closest: *mut ModeDesc1,
    device: *mut c_void,
) -> Hresult {
    if target.is_null() || closest.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the application's non-null `target`; the read assumes no alignment.
    let target = unsafe { target.read_unaligned() };
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    match output.closest(&target, !device.is_null()) {
        Ok(mode) => {
            // SAFETY: the application's non-null `closest`; the write assumes no alignment.
            unsafe { closest.write_unaligned(mode) };
            S_OK
        }
        Err(hr) => hr,
    }
}

extern "system" fn output_get_display_surface_data1(
    _this: *mut c_void,
    _destination: *mut c_void,
) -> Hresult {
    not_implemented()
}

extern "system" fn output_duplicate_output(
    _this: *mut c_void,
    device: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's `out`: null or a writable pointer, with no alignment promised.
    unsafe { duplicate(device, out) }
}

const extern "system" fn output_supports_overlays(_this: *mut c_void) -> i32 {
    0
}

const extern "system" fn output_check_overlay_support(
    _this: *mut c_void,
    _format: u32,
    _device: *mut c_void,
    flags: *mut u32,
) -> Hresult {
    // SAFETY: the application's `flags`: null or a `UINT`, with no alignment promised.
    unsafe { no_flags(flags) }
}

const extern "system" fn output_check_overlay_color_space_support(
    _this: *mut c_void,
    _format: u32,
    _color_space: u32,
    _device: *mut c_void,
    flags: *mut u32,
) -> Hresult {
    // SAFETY: the application's `flags`: null or a `UINT`, with no alignment promised.
    unsafe { no_flags(flags) }
}

extern "system" fn output_duplicate_output1(
    _this: *mut c_void,
    device: *mut c_void,
    _flags: u32,
    _format_count: u32,
    _formats: *const u32,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the application's `out`: null or a writable pointer, with no alignment promised.
    unsafe { duplicate(device, out) }
}

extern "system" fn output_get_desc1(this: *mut c_void, desc: *mut OutputDesc1) -> Hresult {
    if desc.is_null() {
        return DXGI_ERROR_INVALID_CALL;
    }
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    // SAFETY: the application's non-null `desc`, a whole `DXGI_OUTPUT_DESC1`; no alignment
    // promised.
    unsafe { output.describe(desc, DESC1_WRITTEN) }
}

const extern "system" fn output_check_hardware_composition_support(
    _this: *mut c_void,
    flags: *mut u32,
) -> Hresult {
    // SAFETY: the application's `flags`: null or a `UINT`, with no alignment promised.
    unsafe { no_flags(flags) }
}

const extern "system" fn output_get_gamma_ramp(this: *mut c_void) -> *const GammaRamp {
    // SAFETY: the slot's `this` is a live output.
    let output = unsafe { Output::from_this(this) };
    &raw const output.ramp
}

#[cfg(test)]
mod tests;
