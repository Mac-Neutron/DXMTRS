//! DXGI's ABI as dxgi.dll and dxgi.dylib speak it: types, values, IIDs and vtables.
//!
//! The DXGI and D3D10 home of CONVENTIONS' "ABI constants have one home", until the bindgen of the
//! spec's §7: the structs of `dxgi1_6.h` and DXMT's `dxgi_interfaces.h`, the Win32 and D3DKMT
//! mirrors the PE build calls with, and winemetal's window-system hooks the native build reads.
//! Fields are the C's in snake case; a struct's layout is the C's on every target, asserted at the
//! end against clang's record layouts (llvm-mingw's sysroot for the four PE targets,
//! `include/native` and `winemetal.h` natively). Natively `WCHAR` is `wchar_t`, four bytes.

use core::{ffi::c_void, mem::offset_of};

pub use dxmt_util::{
    com::{
        DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL,
        E_POINTER, Guid, Hresult, IID_IUNKNOWN, IUnknownVtbl, S_FALSE, S_OK,
        hresult::{DXGI_ERROR_ALREADY_EXISTS, DXGI_ERROR_INVALID_CALL, DXGI_ERROR_UNSUPPORTED},
        iid::{
            IID_ID3D10_DEVICE, IID_ID3D10_DEVICE1, IID_IDXGI_ADAPTER, IID_IDXGI_ADAPTER1,
            IID_IDXGI_ADAPTER2, IID_IDXGI_ADAPTER3, IID_IDXGI_ADAPTER4, IID_IDXGI_DEVICE,
            IID_IDXGI_FACTORY, IID_IDXGI_FACTORY1, IID_IDXGI_FACTORY2, IID_IDXGI_FACTORY3,
            IID_IDXGI_FACTORY4, IID_IDXGI_FACTORY5, IID_IDXGI_FACTORY6, IID_IDXGI_FACTORY7,
            IID_IDXGI_OBJECT, IID_IDXGI_OUTPUT, IID_IDXGI_OUTPUT1, IID_IDXGI_OUTPUT2,
            IID_IDXGI_OUTPUT3, IID_IDXGI_OUTPUT4, IID_IDXGI_OUTPUT5, IID_IDXGI_OUTPUT6,
            IID_WINE_DXGI_ADAPTER,
        },
    },
    dxgi_consts::{
        DXGI_ADAPTER_FLAG3_NONE, DXGI_ALPHA_MODE_IGNORE, DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
        DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020, DXGI_COMPUTE_PREEMPTION_DMA_BUFFER_BOUNDARY,
        DXGI_ENUM_MODES_INTERLACED, DXGI_ENUM_MODES_SCALING, DXGI_FEATURE_PRESENT_ALLOW_TEARING,
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_B8G8R8X8_UNORM,
        DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM,
        DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_UNKNOWN,
        DXGI_GRAPHICS_PREEMPTION_DMA_BUFFER_BOUNDARY, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
        DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, DXGI_MODE_ROTATION_UNSPECIFIED,
        DXGI_MODE_SCALING_UNSPECIFIED, DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE,
        DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST,
        DXGI_SCALING_STRETCH,
    },
    ext::{
        AdapterDesc, AdapterDesc1, AdapterDesc2, AdapterDesc3, AdapterVtbl, DXMT_NVEXT_GUID,
        GammaControl, GammaControlCapabilities, Hmonitor, Hwnd, IID_IMTL_DXGI_ADAPTER,
        IID_IMTL_DXGI_DEVICE, IID_IMTL_DXGI_OUTPUT, IID_IMTL_SWAP_CHAIN_FACTORY,
        IMTLDXGIAdapterVtbl, IMTLDXGIOutputVtbl, IMTLSwapChainFactoryVtbl, Luid, ModeDesc,
        ModeDesc1, ObjectVtbl, OutputDesc, OutputDesc1, OutputVtbl, QueryVideoMemoryInfo, Rational,
        Rect, Rgb, SampleDesc, SwapChainDesc, SwapChainDesc1, SwapChainFactoryVtbl,
        SwapChainFullscreenDesc, Wchar,
    },
};

/// `HKEY`.
#[cfg(windows)]
pub type Hkey = *mut c_void;

/// `HKEY_LOCAL_MACHINE`: `(HKEY)(ULONG_PTR)(LONG)0x80000002`, sign-extended on 64-bit.
#[cfg(windows)]
pub const HKEY_LOCAL_MACHINE: Hkey =
    core::ptr::without_provenance_mut((0x8000_0002_u32.cast_signed() as isize).cast_unsigned());
#[cfg(windows)]
pub const KEY_ALL_ACCESS: u32 = 0xf_003f;
#[cfg(windows)]
pub const REG_SZ: u32 = 1;
#[cfg(windows)]
pub const REG_DWORD: u32 = 4;
#[cfg(windows)]
pub const DLL_PROCESS_ATTACH: u32 = 1;

/// `0x3f`, `?`: what a byte sequence that is not UTF-8 decodes to.
const QUESTION: u32 = 0x3f;

/// `IDXGIFactory6`'s 30 slots.
#[repr(C)]
pub struct FactoryVtbl {
    pub object: ObjectVtbl,
    pub enum_adapters: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> Hresult,
    pub make_window_association: unsafe extern "system" fn(*mut c_void, Hwnd, u32) -> Hresult,
    pub get_window_association: unsafe extern "system" fn(*mut c_void, *mut Hwnd) -> Hresult,
    pub create_swap_chain: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut SwapChainDesc,
        *mut *mut c_void,
    ) -> Hresult,
    pub create_software_adapter:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> Hresult,
    pub enum_adapters1: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> Hresult,
    pub is_current: unsafe extern "system" fn(*mut c_void) -> i32,
    pub is_windowed_stereo_enabled: unsafe extern "system" fn(*mut c_void) -> i32,
    pub create_swap_chain_for_hwnd: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        Hwnd,
        *const SwapChainDesc1,
        *const SwapChainFullscreenDesc,
        *mut c_void,
        *mut *mut c_void,
    ) -> Hresult,
    pub create_swap_chain_for_core_window: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *mut c_void,
        *const SwapChainDesc1,
        *mut c_void,
        *mut *mut c_void,
    ) -> Hresult,
    pub get_shared_resource_adapter_luid:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut Luid) -> Hresult,
    pub register_stereo_status_window:
        unsafe extern "system" fn(*mut c_void, Hwnd, u32, *mut u32) -> Hresult,
    pub register_stereo_status_event:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> Hresult,
    pub unregister_stereo_status: unsafe extern "system" fn(*mut c_void, u32),
    pub register_occlusion_status_window:
        unsafe extern "system" fn(*mut c_void, Hwnd, u32, *mut u32) -> Hresult,
    pub register_occlusion_status_event:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> Hresult,
    pub unregister_occlusion_status: unsafe extern "system" fn(*mut c_void, u32),
    pub create_swap_chain_for_composition: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *const SwapChainDesc1,
        *mut c_void,
        *mut *mut c_void,
    ) -> Hresult,
    pub get_creation_flags: unsafe extern "system" fn(*mut c_void) -> u32,
    pub enum_adapter_by_luid:
        unsafe extern "system" fn(*mut c_void, Luid, *const Guid, *mut *mut c_void) -> Hresult,
    pub enum_warp_adapter:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
    pub check_feature_support:
        unsafe extern "system" fn(*mut c_void, u32, *mut c_void, u32) -> Hresult,
    pub enum_adapter_by_gpu_preference:
        unsafe extern "system" fn(*mut c_void, u32, u32, *const Guid, *mut *mut c_void) -> Hresult,
}

/// `D3DKMT_OPENADAPTERFROMLUID`.
#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
pub struct D3dkmtOpenAdapterFromLuid {
    pub adapter_luid: Luid,
    pub adapter: u32,
}

/// `D3DKMT_CLOSEADAPTER`.
#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
pub struct D3dkmtCloseAdapter {
    pub adapter: u32,
}

/// Writes the UTF-8 in `src` to `dst` as `str::transcodeString` does: the units written.
///
/// A stray continuation byte (with the ones after it), an invalid lead byte, and a character cut
/// off by the end of `src` (which ends it) each decode to `?`; continuation bytes are not checked.
/// A character that does not fit what is left of `dst`, or that UTF-16 cannot encode, is dropped,
/// and a later one may still take its place. A NUL is written and ends the text. Nothing is
/// written past `dst`.
pub fn transcode_into(src: &[u8], dst: &mut [Wchar]) -> usize {
    transcode(src, dst)
}

/// A wide string's code unit: UTF-16's or UTF-32's.
trait Unit: Sized {
    /// Writes `ch` at the start of `dst`: the units it took, 0 when it does not fit.
    fn encode(dst: &mut [Self], ch: u32) -> usize;
}

impl Unit for u16 {
    fn encode(dst: &mut [Self], ch: u32) -> usize {
        match ch {
            0xd800..=0xdfff | 0x11_0000.. => 0,
            0x1_0000.. => {
                let pair = ch - 0x1_0000;
                let (Ok(high), Ok(low)) = (
                    Self::try_from(0xd800 + (pair >> 10)),
                    Self::try_from(0xdc00 + (pair & 0x3ff)),
                ) else {
                    return 0;
                };
                let [first, second, ..] = dst else {
                    return 0;
                };
                (*first, *second) = (high, low);
                2
            }
            _ => match (dst.first_mut(), Self::try_from(ch)) {
                (Some(first), Ok(unit)) => {
                    *first = unit;
                    1
                }
                _ => 0,
            },
        }
    }
}

impl Unit for u32 {
    fn encode(dst: &mut [Self], ch: u32) -> usize {
        dst.first_mut().map_or(0, |first| {
            *first = ch;
            1
        })
    }
}

/// `transcode_into` for either width.
fn transcode<U: Unit>(src: &[u8], dst: &mut [U]) -> usize {
    let mut written = 0;
    let mut rest = src;
    while let Some((ch, used)) = decode(rest) {
        rest = rest.get(used..).unwrap_or_default();
        written += U::encode(dst.get_mut(written..).unwrap_or_default(), ch);
        if ch == 0 {
            break;
        }
    }
    written
}

/// The first character of `src` and the bytes it took, as `decodeTypedChar` reads UTF-8.
fn decode(src: &[u8]) -> Option<(u32, usize)> {
    let (&first, tail) = src.split_first()?;
    let lead = u32::from(first);
    if first < 0x80 {
        return Some((lead, 1));
    }
    if first < 0xc0 {
        return Some((
            QUESTION,
            1 + tail.iter().take_while(|&&byte| byte & 0xc0 == 0x80).count(),
        ));
    }
    // The lead byte's leading ones are the character's length.
    let length = usize::try_from((!lead << 24).leading_zeros()).unwrap_or(usize::MAX);
    let Some(bytes) = src.get(..length) else {
        return Some((QUESTION, src.len()));
    };
    let next = |i: usize| bytes.get(i).map_or(0, |&byte| u32::from(byte) & 0x3f);
    let ch = match first {
        0xc0..=0xdf => (lead & 0x1f) << 6 | next(1),
        0xe0..=0xef => (lead & 0x0f) << 12 | next(1) << 6 | next(2),
        0xf0..=0xf7 => (lead & 0x07) << 18 | next(1) << 12 | next(2) << 6 | next(3),
        _ => QUESTION,
    };
    Some((ch, length))
}

// The layouts of clang's `-fdump-record-layouts` over the headers named above: every mirrored
// struct's size, alignment and field offsets, and every slot of the C vtables, per target. The
// three 64-bit PE targets' dumps are the same.
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: () = {
    assert!(offset_of!(AdapterVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(AdapterVtbl, object.unknown.add_ref) == 4);
    assert!(offset_of!(AdapterVtbl, object.unknown.release) == 8);
    assert!(offset_of!(AdapterVtbl, object.set_private_data) == 12);
    assert!(offset_of!(AdapterVtbl, object.set_private_data_interface) == 16);
    assert!(offset_of!(AdapterVtbl, object.get_private_data) == 20);
    assert!(offset_of!(AdapterVtbl, object.get_parent) == 24);
    assert!(offset_of!(AdapterVtbl, enum_outputs) == 28);
    assert!(offset_of!(AdapterVtbl, get_desc) == 32);
    assert!(offset_of!(AdapterVtbl, check_interface_support) == 36);
    assert!(offset_of!(AdapterVtbl, get_desc1) == 40);
    assert!(offset_of!(AdapterVtbl, get_desc2) == 44);
    assert!(
        offset_of!(
            AdapterVtbl,
            register_hardware_content_protection_teardown_status_event
        ) == 48
    );
    assert!(
        offset_of!(
            AdapterVtbl,
            unregister_hardware_content_protection_teardown_status
        ) == 52
    );
    assert!(offset_of!(AdapterVtbl, query_video_memory_info) == 56);
    assert!(offset_of!(AdapterVtbl, set_video_memory_reservation) == 60);
    assert!(
        offset_of!(
            AdapterVtbl,
            register_video_memory_budget_change_notification_event
        ) == 64
    );
    assert!(
        offset_of!(
            AdapterVtbl,
            unregister_video_memory_budget_change_notification
        ) == 68
    );
    assert!(offset_of!(AdapterVtbl, get_desc3) == 72);
    assert!(offset_of!(FactoryVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(FactoryVtbl, object.unknown.add_ref) == 4);
    assert!(offset_of!(FactoryVtbl, object.unknown.release) == 8);
    assert!(offset_of!(FactoryVtbl, object.set_private_data) == 12);
    assert!(offset_of!(FactoryVtbl, object.set_private_data_interface) == 16);
    assert!(offset_of!(FactoryVtbl, object.get_private_data) == 20);
    assert!(offset_of!(FactoryVtbl, object.get_parent) == 24);
    assert!(offset_of!(FactoryVtbl, enum_adapters) == 28);
    assert!(offset_of!(FactoryVtbl, make_window_association) == 32);
    assert!(offset_of!(FactoryVtbl, get_window_association) == 36);
    assert!(offset_of!(FactoryVtbl, create_swap_chain) == 40);
    assert!(offset_of!(FactoryVtbl, create_software_adapter) == 44);
    assert!(offset_of!(FactoryVtbl, enum_adapters1) == 48);
    assert!(offset_of!(FactoryVtbl, is_current) == 52);
    assert!(offset_of!(FactoryVtbl, is_windowed_stereo_enabled) == 56);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_hwnd) == 60);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_core_window) == 64);
    assert!(offset_of!(FactoryVtbl, get_shared_resource_adapter_luid) == 68);
    assert!(offset_of!(FactoryVtbl, register_stereo_status_window) == 72);
    assert!(offset_of!(FactoryVtbl, register_stereo_status_event) == 76);
    assert!(offset_of!(FactoryVtbl, unregister_stereo_status) == 80);
    assert!(offset_of!(FactoryVtbl, register_occlusion_status_window) == 84);
    assert!(offset_of!(FactoryVtbl, register_occlusion_status_event) == 88);
    assert!(offset_of!(FactoryVtbl, unregister_occlusion_status) == 92);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_composition) == 96);
    assert!(offset_of!(FactoryVtbl, get_creation_flags) == 100);
    assert!(offset_of!(FactoryVtbl, enum_adapter_by_luid) == 104);
    assert!(offset_of!(FactoryVtbl, enum_warp_adapter) == 108);
    assert!(offset_of!(FactoryVtbl, check_feature_support) == 112);
    assert!(offset_of!(FactoryVtbl, enum_adapter_by_gpu_preference) == 116);
    assert!(offset_of!(OutputVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(OutputVtbl, object.unknown.add_ref) == 4);
    assert!(offset_of!(OutputVtbl, object.unknown.release) == 8);
    assert!(offset_of!(OutputVtbl, object.set_private_data) == 12);
    assert!(offset_of!(OutputVtbl, object.set_private_data_interface) == 16);
    assert!(offset_of!(OutputVtbl, object.get_private_data) == 20);
    assert!(offset_of!(OutputVtbl, object.get_parent) == 24);
    assert!(offset_of!(OutputVtbl, get_desc) == 28);
    assert!(offset_of!(OutputVtbl, get_display_mode_list) == 32);
    assert!(offset_of!(OutputVtbl, find_closest_matching_mode) == 36);
    assert!(offset_of!(OutputVtbl, wait_for_v_blank) == 40);
    assert!(offset_of!(OutputVtbl, take_ownership) == 44);
    assert!(offset_of!(OutputVtbl, release_ownership) == 48);
    assert!(offset_of!(OutputVtbl, get_gamma_control_capabilities) == 52);
    assert!(offset_of!(OutputVtbl, set_gamma_control) == 56);
    assert!(offset_of!(OutputVtbl, get_gamma_control) == 60);
    assert!(offset_of!(OutputVtbl, set_display_surface) == 64);
    assert!(offset_of!(OutputVtbl, get_display_surface_data) == 68);
    assert!(offset_of!(OutputVtbl, get_frame_statistics) == 72);
    assert!(offset_of!(OutputVtbl, get_display_mode_list1) == 76);
    assert!(offset_of!(OutputVtbl, find_closest_matching_mode1) == 80);
    assert!(offset_of!(OutputVtbl, get_display_surface_data1) == 84);
    assert!(offset_of!(OutputVtbl, duplicate_output) == 88);
    assert!(offset_of!(OutputVtbl, supports_overlays) == 92);
    assert!(offset_of!(OutputVtbl, check_overlay_support) == 96);
    assert!(offset_of!(OutputVtbl, check_overlay_color_space_support) == 100);
    assert!(offset_of!(OutputVtbl, duplicate_output1) == 104);
    assert!(offset_of!(OutputVtbl, get_desc1) == 108);
    assert!(offset_of!(OutputVtbl, check_hardware_composition_support) == 112);
    assert!(size_of::<AdapterDesc>() == 292 && align_of::<AdapterDesc>() == 4);
    assert!(offset_of!(AdapterDesc, description) == 0);
    assert!(offset_of!(AdapterDesc, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc, device_id) == 260);
    assert!(offset_of!(AdapterDesc, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc, revision) == 268);
    assert!(offset_of!(AdapterDesc, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc, dedicated_system_memory) == 276);
    assert!(offset_of!(AdapterDesc, shared_system_memory) == 280);
    assert!(offset_of!(AdapterDesc, adapter_luid) == 284);
    assert!(size_of::<AdapterDesc1>() == 296 && align_of::<AdapterDesc1>() == 4);
    assert!(offset_of!(AdapterDesc1, description) == 0);
    assert!(offset_of!(AdapterDesc1, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc1, device_id) == 260);
    assert!(offset_of!(AdapterDesc1, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc1, revision) == 268);
    assert!(offset_of!(AdapterDesc1, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc1, dedicated_system_memory) == 276);
    assert!(offset_of!(AdapterDesc1, shared_system_memory) == 280);
    assert!(offset_of!(AdapterDesc1, adapter_luid) == 284);
    assert!(offset_of!(AdapterDesc1, flags) == 292);
    assert!(size_of::<AdapterDesc2>() == 304 && align_of::<AdapterDesc2>() == 4);
    assert!(offset_of!(AdapterDesc2, description) == 0);
    assert!(offset_of!(AdapterDesc2, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc2, device_id) == 260);
    assert!(offset_of!(AdapterDesc2, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc2, revision) == 268);
    assert!(offset_of!(AdapterDesc2, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc2, dedicated_system_memory) == 276);
    assert!(offset_of!(AdapterDesc2, shared_system_memory) == 280);
    assert!(offset_of!(AdapterDesc2, adapter_luid) == 284);
    assert!(offset_of!(AdapterDesc2, flags) == 292);
    assert!(offset_of!(AdapterDesc2, graphics_preemption_granularity) == 296);
    assert!(offset_of!(AdapterDesc2, compute_preemption_granularity) == 300);
    assert!(size_of::<AdapterDesc3>() == 304 && align_of::<AdapterDesc3>() == 4);
    assert!(offset_of!(AdapterDesc3, description) == 0);
    assert!(offset_of!(AdapterDesc3, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc3, device_id) == 260);
    assert!(offset_of!(AdapterDesc3, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc3, revision) == 268);
    assert!(offset_of!(AdapterDesc3, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc3, dedicated_system_memory) == 276);
    assert!(offset_of!(AdapterDesc3, shared_system_memory) == 280);
    assert!(offset_of!(AdapterDesc3, adapter_luid) == 284);
    assert!(offset_of!(AdapterDesc3, flags) == 292);
    assert!(offset_of!(AdapterDesc3, graphics_preemption_granularity) == 296);
    assert!(offset_of!(AdapterDesc3, compute_preemption_granularity) == 300);
    assert!(size_of::<D3dkmtCloseAdapter>() == 4 && align_of::<D3dkmtCloseAdapter>() == 4);
    assert!(offset_of!(D3dkmtCloseAdapter, adapter) == 0);
    assert!(
        size_of::<D3dkmtOpenAdapterFromLuid>() == 12
            && align_of::<D3dkmtOpenAdapterFromLuid>() == 4
    );
    assert!(offset_of!(D3dkmtOpenAdapterFromLuid, adapter_luid) == 0);
    assert!(offset_of!(D3dkmtOpenAdapterFromLuid, adapter) == 8);
    assert!(size_of::<GammaControl>() == 12324 && align_of::<GammaControl>() == 4);
    assert!(offset_of!(GammaControl, scale) == 0);
    assert!(offset_of!(GammaControl, offset) == 12);
    assert!(offset_of!(GammaControl, gamma_curve) == 24);
    assert!(
        size_of::<GammaControlCapabilities>() == 4116
            && align_of::<GammaControlCapabilities>() == 4
    );
    assert!(offset_of!(GammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(GammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(GammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(GammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(GammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<Luid>() == 8 && align_of::<Luid>() == 4);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 92 && align_of::<OutputDesc>() == 4);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc, rotation) == 84);
    assert!(offset_of!(OutputDesc, monitor) == 88);
    assert!(size_of::<OutputDesc1>() == 144 && align_of::<OutputDesc1>() == 4);
    assert!(offset_of!(OutputDesc1, device_name) == 0);
    assert!(offset_of!(OutputDesc1, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc1, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc1, rotation) == 84);
    assert!(offset_of!(OutputDesc1, monitor) == 88);
    assert!(offset_of!(OutputDesc1, bits_per_color) == 92);
    assert!(offset_of!(OutputDesc1, color_space) == 96);
    assert!(offset_of!(OutputDesc1, red_primary) == 100);
    assert!(offset_of!(OutputDesc1, green_primary) == 108);
    assert!(offset_of!(OutputDesc1, blue_primary) == 116);
    assert!(offset_of!(OutputDesc1, white_point) == 124);
    assert!(offset_of!(OutputDesc1, min_luminance) == 132);
    assert!(offset_of!(OutputDesc1, max_luminance) == 136);
    assert!(offset_of!(OutputDesc1, max_full_frame_luminance) == 140);
    assert!(size_of::<QueryVideoMemoryInfo>() == 32 && align_of::<QueryVideoMemoryInfo>() == 8);
    assert!(offset_of!(QueryVideoMemoryInfo, budget) == 0);
    assert!(offset_of!(QueryVideoMemoryInfo, current_usage) == 8);
    assert!(offset_of!(QueryVideoMemoryInfo, available_for_reservation) == 16);
    assert!(offset_of!(QueryVideoMemoryInfo, current_reservation) == 24);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgb>() == 12 && align_of::<Rgb>() == 4);
    assert!(offset_of!(Rgb, red) == 0);
    assert!(offset_of!(Rgb, green) == 4);
    assert!(offset_of!(Rgb, blue) == 8);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 60 && align_of::<SwapChainDesc>() == 4);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 44);
    assert!(offset_of!(SwapChainDesc, windowed) == 48);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 52);
    assert!(offset_of!(SwapChainDesc, flags) == 56);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};
#[cfg(all(target_os = "windows", target_pointer_width = "64"))]
const _: () = {
    assert!(offset_of!(AdapterVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(AdapterVtbl, object.unknown.add_ref) == 8);
    assert!(offset_of!(AdapterVtbl, object.unknown.release) == 16);
    assert!(offset_of!(AdapterVtbl, object.set_private_data) == 24);
    assert!(offset_of!(AdapterVtbl, object.set_private_data_interface) == 32);
    assert!(offset_of!(AdapterVtbl, object.get_private_data) == 40);
    assert!(offset_of!(AdapterVtbl, object.get_parent) == 48);
    assert!(offset_of!(AdapterVtbl, enum_outputs) == 56);
    assert!(offset_of!(AdapterVtbl, get_desc) == 64);
    assert!(offset_of!(AdapterVtbl, check_interface_support) == 72);
    assert!(offset_of!(AdapterVtbl, get_desc1) == 80);
    assert!(offset_of!(AdapterVtbl, get_desc2) == 88);
    assert!(
        offset_of!(
            AdapterVtbl,
            register_hardware_content_protection_teardown_status_event
        ) == 96
    );
    assert!(
        offset_of!(
            AdapterVtbl,
            unregister_hardware_content_protection_teardown_status
        ) == 104
    );
    assert!(offset_of!(AdapterVtbl, query_video_memory_info) == 112);
    assert!(offset_of!(AdapterVtbl, set_video_memory_reservation) == 120);
    assert!(
        offset_of!(
            AdapterVtbl,
            register_video_memory_budget_change_notification_event
        ) == 128
    );
    assert!(
        offset_of!(
            AdapterVtbl,
            unregister_video_memory_budget_change_notification
        ) == 136
    );
    assert!(offset_of!(AdapterVtbl, get_desc3) == 144);
    assert!(offset_of!(FactoryVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(FactoryVtbl, object.unknown.add_ref) == 8);
    assert!(offset_of!(FactoryVtbl, object.unknown.release) == 16);
    assert!(offset_of!(FactoryVtbl, object.set_private_data) == 24);
    assert!(offset_of!(FactoryVtbl, object.set_private_data_interface) == 32);
    assert!(offset_of!(FactoryVtbl, object.get_private_data) == 40);
    assert!(offset_of!(FactoryVtbl, object.get_parent) == 48);
    assert!(offset_of!(FactoryVtbl, enum_adapters) == 56);
    assert!(offset_of!(FactoryVtbl, make_window_association) == 64);
    assert!(offset_of!(FactoryVtbl, get_window_association) == 72);
    assert!(offset_of!(FactoryVtbl, create_swap_chain) == 80);
    assert!(offset_of!(FactoryVtbl, create_software_adapter) == 88);
    assert!(offset_of!(FactoryVtbl, enum_adapters1) == 96);
    assert!(offset_of!(FactoryVtbl, is_current) == 104);
    assert!(offset_of!(FactoryVtbl, is_windowed_stereo_enabled) == 112);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_hwnd) == 120);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_core_window) == 128);
    assert!(offset_of!(FactoryVtbl, get_shared_resource_adapter_luid) == 136);
    assert!(offset_of!(FactoryVtbl, register_stereo_status_window) == 144);
    assert!(offset_of!(FactoryVtbl, register_stereo_status_event) == 152);
    assert!(offset_of!(FactoryVtbl, unregister_stereo_status) == 160);
    assert!(offset_of!(FactoryVtbl, register_occlusion_status_window) == 168);
    assert!(offset_of!(FactoryVtbl, register_occlusion_status_event) == 176);
    assert!(offset_of!(FactoryVtbl, unregister_occlusion_status) == 184);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_composition) == 192);
    assert!(offset_of!(FactoryVtbl, get_creation_flags) == 200);
    assert!(offset_of!(FactoryVtbl, enum_adapter_by_luid) == 208);
    assert!(offset_of!(FactoryVtbl, enum_warp_adapter) == 216);
    assert!(offset_of!(FactoryVtbl, check_feature_support) == 224);
    assert!(offset_of!(FactoryVtbl, enum_adapter_by_gpu_preference) == 232);
    assert!(offset_of!(OutputVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(OutputVtbl, object.unknown.add_ref) == 8);
    assert!(offset_of!(OutputVtbl, object.unknown.release) == 16);
    assert!(offset_of!(OutputVtbl, object.set_private_data) == 24);
    assert!(offset_of!(OutputVtbl, object.set_private_data_interface) == 32);
    assert!(offset_of!(OutputVtbl, object.get_private_data) == 40);
    assert!(offset_of!(OutputVtbl, object.get_parent) == 48);
    assert!(offset_of!(OutputVtbl, get_desc) == 56);
    assert!(offset_of!(OutputVtbl, get_display_mode_list) == 64);
    assert!(offset_of!(OutputVtbl, find_closest_matching_mode) == 72);
    assert!(offset_of!(OutputVtbl, wait_for_v_blank) == 80);
    assert!(offset_of!(OutputVtbl, take_ownership) == 88);
    assert!(offset_of!(OutputVtbl, release_ownership) == 96);
    assert!(offset_of!(OutputVtbl, get_gamma_control_capabilities) == 104);
    assert!(offset_of!(OutputVtbl, set_gamma_control) == 112);
    assert!(offset_of!(OutputVtbl, get_gamma_control) == 120);
    assert!(offset_of!(OutputVtbl, set_display_surface) == 128);
    assert!(offset_of!(OutputVtbl, get_display_surface_data) == 136);
    assert!(offset_of!(OutputVtbl, get_frame_statistics) == 144);
    assert!(offset_of!(OutputVtbl, get_display_mode_list1) == 152);
    assert!(offset_of!(OutputVtbl, find_closest_matching_mode1) == 160);
    assert!(offset_of!(OutputVtbl, get_display_surface_data1) == 168);
    assert!(offset_of!(OutputVtbl, duplicate_output) == 176);
    assert!(offset_of!(OutputVtbl, supports_overlays) == 184);
    assert!(offset_of!(OutputVtbl, check_overlay_support) == 192);
    assert!(offset_of!(OutputVtbl, check_overlay_color_space_support) == 200);
    assert!(offset_of!(OutputVtbl, duplicate_output1) == 208);
    assert!(offset_of!(OutputVtbl, get_desc1) == 216);
    assert!(offset_of!(OutputVtbl, check_hardware_composition_support) == 224);
    assert!(size_of::<AdapterDesc>() == 304 && align_of::<AdapterDesc>() == 8);
    assert!(offset_of!(AdapterDesc, description) == 0);
    assert!(offset_of!(AdapterDesc, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc, device_id) == 260);
    assert!(offset_of!(AdapterDesc, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc, revision) == 268);
    assert!(offset_of!(AdapterDesc, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc, dedicated_system_memory) == 280);
    assert!(offset_of!(AdapterDesc, shared_system_memory) == 288);
    assert!(offset_of!(AdapterDesc, adapter_luid) == 296);
    assert!(size_of::<AdapterDesc1>() == 312 && align_of::<AdapterDesc1>() == 8);
    assert!(offset_of!(AdapterDesc1, description) == 0);
    assert!(offset_of!(AdapterDesc1, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc1, device_id) == 260);
    assert!(offset_of!(AdapterDesc1, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc1, revision) == 268);
    assert!(offset_of!(AdapterDesc1, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc1, dedicated_system_memory) == 280);
    assert!(offset_of!(AdapterDesc1, shared_system_memory) == 288);
    assert!(offset_of!(AdapterDesc1, adapter_luid) == 296);
    assert!(offset_of!(AdapterDesc1, flags) == 304);
    assert!(size_of::<AdapterDesc2>() == 320 && align_of::<AdapterDesc2>() == 8);
    assert!(offset_of!(AdapterDesc2, description) == 0);
    assert!(offset_of!(AdapterDesc2, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc2, device_id) == 260);
    assert!(offset_of!(AdapterDesc2, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc2, revision) == 268);
    assert!(offset_of!(AdapterDesc2, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc2, dedicated_system_memory) == 280);
    assert!(offset_of!(AdapterDesc2, shared_system_memory) == 288);
    assert!(offset_of!(AdapterDesc2, adapter_luid) == 296);
    assert!(offset_of!(AdapterDesc2, flags) == 304);
    assert!(offset_of!(AdapterDesc2, graphics_preemption_granularity) == 308);
    assert!(offset_of!(AdapterDesc2, compute_preemption_granularity) == 312);
    assert!(size_of::<AdapterDesc3>() == 320 && align_of::<AdapterDesc3>() == 8);
    assert!(offset_of!(AdapterDesc3, description) == 0);
    assert!(offset_of!(AdapterDesc3, vendor_id) == 256);
    assert!(offset_of!(AdapterDesc3, device_id) == 260);
    assert!(offset_of!(AdapterDesc3, sub_sys_id) == 264);
    assert!(offset_of!(AdapterDesc3, revision) == 268);
    assert!(offset_of!(AdapterDesc3, dedicated_video_memory) == 272);
    assert!(offset_of!(AdapterDesc3, dedicated_system_memory) == 280);
    assert!(offset_of!(AdapterDesc3, shared_system_memory) == 288);
    assert!(offset_of!(AdapterDesc3, adapter_luid) == 296);
    assert!(offset_of!(AdapterDesc3, flags) == 304);
    assert!(offset_of!(AdapterDesc3, graphics_preemption_granularity) == 308);
    assert!(offset_of!(AdapterDesc3, compute_preemption_granularity) == 312);
    assert!(size_of::<D3dkmtCloseAdapter>() == 4 && align_of::<D3dkmtCloseAdapter>() == 4);
    assert!(offset_of!(D3dkmtCloseAdapter, adapter) == 0);
    assert!(
        size_of::<D3dkmtOpenAdapterFromLuid>() == 12
            && align_of::<D3dkmtOpenAdapterFromLuid>() == 4
    );
    assert!(offset_of!(D3dkmtOpenAdapterFromLuid, adapter_luid) == 0);
    assert!(offset_of!(D3dkmtOpenAdapterFromLuid, adapter) == 8);
    assert!(size_of::<GammaControl>() == 12324 && align_of::<GammaControl>() == 4);
    assert!(offset_of!(GammaControl, scale) == 0);
    assert!(offset_of!(GammaControl, offset) == 12);
    assert!(offset_of!(GammaControl, gamma_curve) == 24);
    assert!(
        size_of::<GammaControlCapabilities>() == 4116
            && align_of::<GammaControlCapabilities>() == 4
    );
    assert!(offset_of!(GammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(GammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(GammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(GammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(GammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<Luid>() == 8 && align_of::<Luid>() == 4);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 96 && align_of::<OutputDesc>() == 8);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc, rotation) == 84);
    assert!(offset_of!(OutputDesc, monitor) == 88);
    assert!(size_of::<OutputDesc1>() == 152 && align_of::<OutputDesc1>() == 8);
    assert!(offset_of!(OutputDesc1, device_name) == 0);
    assert!(offset_of!(OutputDesc1, desktop_coordinates) == 64);
    assert!(offset_of!(OutputDesc1, attached_to_desktop) == 80);
    assert!(offset_of!(OutputDesc1, rotation) == 84);
    assert!(offset_of!(OutputDesc1, monitor) == 88);
    assert!(offset_of!(OutputDesc1, bits_per_color) == 96);
    assert!(offset_of!(OutputDesc1, color_space) == 100);
    assert!(offset_of!(OutputDesc1, red_primary) == 104);
    assert!(offset_of!(OutputDesc1, green_primary) == 112);
    assert!(offset_of!(OutputDesc1, blue_primary) == 120);
    assert!(offset_of!(OutputDesc1, white_point) == 128);
    assert!(offset_of!(OutputDesc1, min_luminance) == 136);
    assert!(offset_of!(OutputDesc1, max_luminance) == 140);
    assert!(offset_of!(OutputDesc1, max_full_frame_luminance) == 144);
    assert!(size_of::<QueryVideoMemoryInfo>() == 32 && align_of::<QueryVideoMemoryInfo>() == 8);
    assert!(offset_of!(QueryVideoMemoryInfo, budget) == 0);
    assert!(offset_of!(QueryVideoMemoryInfo, current_usage) == 8);
    assert!(offset_of!(QueryVideoMemoryInfo, available_for_reservation) == 16);
    assert!(offset_of!(QueryVideoMemoryInfo, current_reservation) == 24);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgb>() == 12 && align_of::<Rgb>() == 4);
    assert!(offset_of!(Rgb, red) == 0);
    assert!(offset_of!(Rgb, green) == 4);
    assert!(offset_of!(Rgb, blue) == 8);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 72 && align_of::<SwapChainDesc>() == 8);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 48);
    assert!(offset_of!(SwapChainDesc, windowed) == 56);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(SwapChainDesc, flags) == 64);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: () = {
    assert!(offset_of!(AdapterVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(AdapterVtbl, object.unknown.add_ref) == 8);
    assert!(offset_of!(AdapterVtbl, object.unknown.release) == 16);
    assert!(offset_of!(AdapterVtbl, object.set_private_data) == 24);
    assert!(offset_of!(AdapterVtbl, object.set_private_data_interface) == 32);
    assert!(offset_of!(AdapterVtbl, object.get_private_data) == 40);
    assert!(offset_of!(AdapterVtbl, object.get_parent) == 48);
    assert!(offset_of!(AdapterVtbl, enum_outputs) == 56);
    assert!(offset_of!(AdapterVtbl, get_desc) == 64);
    assert!(offset_of!(AdapterVtbl, check_interface_support) == 72);
    assert!(offset_of!(AdapterVtbl, get_desc1) == 80);
    assert!(offset_of!(AdapterVtbl, get_desc2) == 88);
    assert!(
        offset_of!(
            AdapterVtbl,
            register_hardware_content_protection_teardown_status_event
        ) == 96
    );
    assert!(
        offset_of!(
            AdapterVtbl,
            unregister_hardware_content_protection_teardown_status
        ) == 104
    );
    assert!(offset_of!(AdapterVtbl, query_video_memory_info) == 112);
    assert!(offset_of!(AdapterVtbl, set_video_memory_reservation) == 120);
    assert!(
        offset_of!(
            AdapterVtbl,
            register_video_memory_budget_change_notification_event
        ) == 128
    );
    assert!(
        offset_of!(
            AdapterVtbl,
            unregister_video_memory_budget_change_notification
        ) == 136
    );
    assert!(offset_of!(AdapterVtbl, get_desc3) == 144);
    assert!(offset_of!(FactoryVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(FactoryVtbl, object.unknown.add_ref) == 8);
    assert!(offset_of!(FactoryVtbl, object.unknown.release) == 16);
    assert!(offset_of!(FactoryVtbl, object.set_private_data) == 24);
    assert!(offset_of!(FactoryVtbl, object.set_private_data_interface) == 32);
    assert!(offset_of!(FactoryVtbl, object.get_private_data) == 40);
    assert!(offset_of!(FactoryVtbl, object.get_parent) == 48);
    assert!(offset_of!(FactoryVtbl, enum_adapters) == 56);
    assert!(offset_of!(FactoryVtbl, make_window_association) == 64);
    assert!(offset_of!(FactoryVtbl, get_window_association) == 72);
    assert!(offset_of!(FactoryVtbl, create_swap_chain) == 80);
    assert!(offset_of!(FactoryVtbl, create_software_adapter) == 88);
    assert!(offset_of!(FactoryVtbl, enum_adapters1) == 96);
    assert!(offset_of!(FactoryVtbl, is_current) == 104);
    assert!(offset_of!(FactoryVtbl, is_windowed_stereo_enabled) == 112);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_hwnd) == 120);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_core_window) == 128);
    assert!(offset_of!(FactoryVtbl, get_shared_resource_adapter_luid) == 136);
    assert!(offset_of!(FactoryVtbl, register_stereo_status_window) == 144);
    assert!(offset_of!(FactoryVtbl, register_stereo_status_event) == 152);
    assert!(offset_of!(FactoryVtbl, unregister_stereo_status) == 160);
    assert!(offset_of!(FactoryVtbl, register_occlusion_status_window) == 168);
    assert!(offset_of!(FactoryVtbl, register_occlusion_status_event) == 176);
    assert!(offset_of!(FactoryVtbl, unregister_occlusion_status) == 184);
    assert!(offset_of!(FactoryVtbl, create_swap_chain_for_composition) == 192);
    assert!(offset_of!(FactoryVtbl, get_creation_flags) == 200);
    assert!(offset_of!(FactoryVtbl, enum_adapter_by_luid) == 208);
    assert!(offset_of!(FactoryVtbl, enum_warp_adapter) == 216);
    assert!(offset_of!(FactoryVtbl, check_feature_support) == 224);
    assert!(offset_of!(FactoryVtbl, enum_adapter_by_gpu_preference) == 232);
    assert!(offset_of!(OutputVtbl, object.unknown.query_interface) == 0);
    assert!(offset_of!(OutputVtbl, object.unknown.add_ref) == 8);
    assert!(offset_of!(OutputVtbl, object.unknown.release) == 16);
    assert!(offset_of!(OutputVtbl, object.set_private_data) == 24);
    assert!(offset_of!(OutputVtbl, object.set_private_data_interface) == 32);
    assert!(offset_of!(OutputVtbl, object.get_private_data) == 40);
    assert!(offset_of!(OutputVtbl, object.get_parent) == 48);
    assert!(offset_of!(OutputVtbl, get_desc) == 56);
    assert!(offset_of!(OutputVtbl, get_display_mode_list) == 64);
    assert!(offset_of!(OutputVtbl, find_closest_matching_mode) == 72);
    assert!(offset_of!(OutputVtbl, wait_for_v_blank) == 80);
    assert!(offset_of!(OutputVtbl, take_ownership) == 88);
    assert!(offset_of!(OutputVtbl, release_ownership) == 96);
    assert!(offset_of!(OutputVtbl, get_gamma_control_capabilities) == 104);
    assert!(offset_of!(OutputVtbl, set_gamma_control) == 112);
    assert!(offset_of!(OutputVtbl, get_gamma_control) == 120);
    assert!(offset_of!(OutputVtbl, set_display_surface) == 128);
    assert!(offset_of!(OutputVtbl, get_display_surface_data) == 136);
    assert!(offset_of!(OutputVtbl, get_frame_statistics) == 144);
    assert!(offset_of!(OutputVtbl, get_display_mode_list1) == 152);
    assert!(offset_of!(OutputVtbl, find_closest_matching_mode1) == 160);
    assert!(offset_of!(OutputVtbl, get_display_surface_data1) == 168);
    assert!(offset_of!(OutputVtbl, duplicate_output) == 176);
    assert!(offset_of!(OutputVtbl, supports_overlays) == 184);
    assert!(offset_of!(OutputVtbl, check_overlay_support) == 192);
    assert!(offset_of!(OutputVtbl, check_overlay_color_space_support) == 200);
    assert!(offset_of!(OutputVtbl, duplicate_output1) == 208);
    assert!(offset_of!(OutputVtbl, get_desc1) == 216);
    assert!(offset_of!(OutputVtbl, check_hardware_composition_support) == 224);
    assert!(size_of::<AdapterDesc>() == 560 && align_of::<AdapterDesc>() == 8);
    assert!(offset_of!(AdapterDesc, description) == 0);
    assert!(offset_of!(AdapterDesc, vendor_id) == 512);
    assert!(offset_of!(AdapterDesc, device_id) == 516);
    assert!(offset_of!(AdapterDesc, sub_sys_id) == 520);
    assert!(offset_of!(AdapterDesc, revision) == 524);
    assert!(offset_of!(AdapterDesc, dedicated_video_memory) == 528);
    assert!(offset_of!(AdapterDesc, dedicated_system_memory) == 536);
    assert!(offset_of!(AdapterDesc, shared_system_memory) == 544);
    assert!(offset_of!(AdapterDesc, adapter_luid) == 552);
    assert!(size_of::<AdapterDesc1>() == 568 && align_of::<AdapterDesc1>() == 8);
    assert!(offset_of!(AdapterDesc1, description) == 0);
    assert!(offset_of!(AdapterDesc1, vendor_id) == 512);
    assert!(offset_of!(AdapterDesc1, device_id) == 516);
    assert!(offset_of!(AdapterDesc1, sub_sys_id) == 520);
    assert!(offset_of!(AdapterDesc1, revision) == 524);
    assert!(offset_of!(AdapterDesc1, dedicated_video_memory) == 528);
    assert!(offset_of!(AdapterDesc1, dedicated_system_memory) == 536);
    assert!(offset_of!(AdapterDesc1, shared_system_memory) == 544);
    assert!(offset_of!(AdapterDesc1, adapter_luid) == 552);
    assert!(offset_of!(AdapterDesc1, flags) == 560);
    assert!(size_of::<AdapterDesc2>() == 576 && align_of::<AdapterDesc2>() == 8);
    assert!(offset_of!(AdapterDesc2, description) == 0);
    assert!(offset_of!(AdapterDesc2, vendor_id) == 512);
    assert!(offset_of!(AdapterDesc2, device_id) == 516);
    assert!(offset_of!(AdapterDesc2, sub_sys_id) == 520);
    assert!(offset_of!(AdapterDesc2, revision) == 524);
    assert!(offset_of!(AdapterDesc2, dedicated_video_memory) == 528);
    assert!(offset_of!(AdapterDesc2, dedicated_system_memory) == 536);
    assert!(offset_of!(AdapterDesc2, shared_system_memory) == 544);
    assert!(offset_of!(AdapterDesc2, adapter_luid) == 552);
    assert!(offset_of!(AdapterDesc2, flags) == 560);
    assert!(offset_of!(AdapterDesc2, graphics_preemption_granularity) == 564);
    assert!(offset_of!(AdapterDesc2, compute_preemption_granularity) == 568);
    assert!(size_of::<AdapterDesc3>() == 576 && align_of::<AdapterDesc3>() == 8);
    assert!(offset_of!(AdapterDesc3, description) == 0);
    assert!(offset_of!(AdapterDesc3, vendor_id) == 512);
    assert!(offset_of!(AdapterDesc3, device_id) == 516);
    assert!(offset_of!(AdapterDesc3, sub_sys_id) == 520);
    assert!(offset_of!(AdapterDesc3, revision) == 524);
    assert!(offset_of!(AdapterDesc3, dedicated_video_memory) == 528);
    assert!(offset_of!(AdapterDesc3, dedicated_system_memory) == 536);
    assert!(offset_of!(AdapterDesc3, shared_system_memory) == 544);
    assert!(offset_of!(AdapterDesc3, adapter_luid) == 552);
    assert!(offset_of!(AdapterDesc3, flags) == 560);
    assert!(offset_of!(AdapterDesc3, graphics_preemption_granularity) == 564);
    assert!(offset_of!(AdapterDesc3, compute_preemption_granularity) == 568);
    assert!(size_of::<GammaControl>() == 12324 && align_of::<GammaControl>() == 4);
    assert!(offset_of!(GammaControl, scale) == 0);
    assert!(offset_of!(GammaControl, offset) == 12);
    assert!(offset_of!(GammaControl, gamma_curve) == 24);
    assert!(
        size_of::<GammaControlCapabilities>() == 4116
            && align_of::<GammaControlCapabilities>() == 4
    );
    assert!(offset_of!(GammaControlCapabilities, scale_and_offset_supported) == 0);
    assert!(offset_of!(GammaControlCapabilities, max_converted_value) == 4);
    assert!(offset_of!(GammaControlCapabilities, min_converted_value) == 8);
    assert!(offset_of!(GammaControlCapabilities, num_gamma_control_points) == 12);
    assert!(offset_of!(GammaControlCapabilities, control_point_positions) == 16);
    assert!(size_of::<Luid>() == 8 && align_of::<Luid>() == 4);
    assert!(offset_of!(Luid, low_part) == 0);
    assert!(offset_of!(Luid, high_part) == 4);
    assert!(size_of::<ModeDesc>() == 28 && align_of::<ModeDesc>() == 4);
    assert!(offset_of!(ModeDesc, width) == 0);
    assert!(offset_of!(ModeDesc, height) == 4);
    assert!(offset_of!(ModeDesc, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc, format) == 16);
    assert!(offset_of!(ModeDesc, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc, scaling) == 24);
    assert!(size_of::<ModeDesc1>() == 32 && align_of::<ModeDesc1>() == 4);
    assert!(offset_of!(ModeDesc1, width) == 0);
    assert!(offset_of!(ModeDesc1, height) == 4);
    assert!(offset_of!(ModeDesc1, refresh_rate) == 8);
    assert!(offset_of!(ModeDesc1, format) == 16);
    assert!(offset_of!(ModeDesc1, scanline_ordering) == 20);
    assert!(offset_of!(ModeDesc1, scaling) == 24);
    assert!(offset_of!(ModeDesc1, stereo) == 28);
    assert!(size_of::<OutputDesc>() == 160 && align_of::<OutputDesc>() == 8);
    assert!(offset_of!(OutputDesc, device_name) == 0);
    assert!(offset_of!(OutputDesc, desktop_coordinates) == 128);
    assert!(offset_of!(OutputDesc, attached_to_desktop) == 144);
    assert!(offset_of!(OutputDesc, rotation) == 148);
    assert!(offset_of!(OutputDesc, monitor) == 152);
    assert!(size_of::<OutputDesc1>() == 216 && align_of::<OutputDesc1>() == 8);
    assert!(offset_of!(OutputDesc1, device_name) == 0);
    assert!(offset_of!(OutputDesc1, desktop_coordinates) == 128);
    assert!(offset_of!(OutputDesc1, attached_to_desktop) == 144);
    assert!(offset_of!(OutputDesc1, rotation) == 148);
    assert!(offset_of!(OutputDesc1, monitor) == 152);
    assert!(offset_of!(OutputDesc1, bits_per_color) == 160);
    assert!(offset_of!(OutputDesc1, color_space) == 164);
    assert!(offset_of!(OutputDesc1, red_primary) == 168);
    assert!(offset_of!(OutputDesc1, green_primary) == 176);
    assert!(offset_of!(OutputDesc1, blue_primary) == 184);
    assert!(offset_of!(OutputDesc1, white_point) == 192);
    assert!(offset_of!(OutputDesc1, min_luminance) == 200);
    assert!(offset_of!(OutputDesc1, max_luminance) == 204);
    assert!(offset_of!(OutputDesc1, max_full_frame_luminance) == 208);
    assert!(size_of::<QueryVideoMemoryInfo>() == 32 && align_of::<QueryVideoMemoryInfo>() == 8);
    assert!(offset_of!(QueryVideoMemoryInfo, budget) == 0);
    assert!(offset_of!(QueryVideoMemoryInfo, current_usage) == 8);
    assert!(offset_of!(QueryVideoMemoryInfo, available_for_reservation) == 16);
    assert!(offset_of!(QueryVideoMemoryInfo, current_reservation) == 24);
    assert!(size_of::<Rational>() == 8 && align_of::<Rational>() == 4);
    assert!(offset_of!(Rational, numerator) == 0);
    assert!(offset_of!(Rational, denominator) == 4);
    assert!(size_of::<Rect>() == 16 && align_of::<Rect>() == 4);
    assert!(offset_of!(Rect, left) == 0);
    assert!(offset_of!(Rect, top) == 4);
    assert!(offset_of!(Rect, right) == 8);
    assert!(offset_of!(Rect, bottom) == 12);
    assert!(size_of::<Rgb>() == 12 && align_of::<Rgb>() == 4);
    assert!(offset_of!(Rgb, red) == 0);
    assert!(offset_of!(Rgb, green) == 4);
    assert!(offset_of!(Rgb, blue) == 8);
    assert!(size_of::<SampleDesc>() == 8 && align_of::<SampleDesc>() == 4);
    assert!(offset_of!(SampleDesc, count) == 0);
    assert!(offset_of!(SampleDesc, quality) == 4);
    assert!(size_of::<SwapChainDesc>() == 72 && align_of::<SwapChainDesc>() == 8);
    assert!(offset_of!(SwapChainDesc, buffer_desc) == 0);
    assert!(offset_of!(SwapChainDesc, sample_desc) == 28);
    assert!(offset_of!(SwapChainDesc, buffer_usage) == 36);
    assert!(offset_of!(SwapChainDesc, buffer_count) == 40);
    assert!(offset_of!(SwapChainDesc, output_window) == 48);
    assert!(offset_of!(SwapChainDesc, windowed) == 56);
    assert!(offset_of!(SwapChainDesc, swap_effect) == 60);
    assert!(offset_of!(SwapChainDesc, flags) == 64);
    assert!(size_of::<SwapChainDesc1>() == 48 && align_of::<SwapChainDesc1>() == 4);
    assert!(offset_of!(SwapChainDesc1, width) == 0);
    assert!(offset_of!(SwapChainDesc1, height) == 4);
    assert!(offset_of!(SwapChainDesc1, format) == 8);
    assert!(offset_of!(SwapChainDesc1, stereo) == 12);
    assert!(offset_of!(SwapChainDesc1, sample_desc) == 16);
    assert!(offset_of!(SwapChainDesc1, buffer_usage) == 24);
    assert!(offset_of!(SwapChainDesc1, buffer_count) == 28);
    assert!(offset_of!(SwapChainDesc1, scaling) == 32);
    assert!(offset_of!(SwapChainDesc1, swap_effect) == 36);
    assert!(offset_of!(SwapChainDesc1, alpha_mode) == 40);
    assert!(offset_of!(SwapChainDesc1, flags) == 44);
    assert!(
        size_of::<SwapChainFullscreenDesc>() == 20 && align_of::<SwapChainFullscreenDesc>() == 4
    );
    assert!(offset_of!(SwapChainFullscreenDesc, refresh_rate) == 0);
    assert!(offset_of!(SwapChainFullscreenDesc, scanline_ordering) == 8);
    assert!(offset_of!(SwapChainFullscreenDesc, scaling) == 12);
    assert!(offset_of!(SwapChainFullscreenDesc, windowed) == 16);
};

#[cfg(test)]
mod tests;
