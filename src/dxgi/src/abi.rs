//! DXGI's ABI as dxgi.dll and dxgi.dylib speak it: types, values, IIDs and vtables.
//!
//! The DXGI and D3D10 home of CONVENTIONS' "ABI constants have one home", until the bindgen of the
//! spec's §7: the structs of `dxgi1_6.h` and DXMT's `dxgi_interfaces.h`, the Win32 and D3DKMT
//! mirrors the PE build calls with, and winemetal's window-system hooks the native build reads.
//! Fields are the C's in snake case; a struct's layout is the C's on every target, asserted at the
//! end against clang's record layouts (llvm-mingw's sysroot for the four PE targets,
//! `include/native` and `winemetal.h` natively). Natively `WCHAR` is `wchar_t`, four bytes.

use core::{ffi::c_void, mem::offset_of};

use dxmt_util::GammaRamp;
pub use dxmt_util::com::{
    E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, Guid, Hresult, IUnknownVtbl,
    S_FALSE, S_OK,
};

/// `WCHAR`: UTF-16 on PE.
#[cfg(windows)]
pub type Wchar = u16;
/// `WCHAR`: natively `wchar_t`, UTF-32.
#[cfg(not(windows))]
pub type Wchar = u32;
/// `HWND`.
pub type Hwnd = *mut c_void;
/// `HMONITOR`.
pub type Hmonitor = *mut c_void;
/// `HKEY`.
#[cfg(windows)]
pub type Hkey = *mut c_void;
/// `MONITORENUMPROC`: `EnumDisplayMonitors`' callback.
#[cfg(windows)]
pub type MonitorEnumProc = extern "system" fn(Hmonitor, *mut c_void, *mut Rect, isize) -> i32;

pub const DXGI_ERROR_INVALID_CALL: Hresult = 0x887a_0001_u32.cast_signed();
pub const DXGI_ERROR_NOT_FOUND: Hresult = 0x887a_0002_u32.cast_signed();
pub const DXGI_ERROR_MORE_DATA: Hresult = 0x887a_0003_u32.cast_signed();
pub const DXGI_ERROR_UNSUPPORTED: Hresult = 0x887a_0004_u32.cast_signed();
pub const DXGI_ERROR_ALREADY_EXISTS: Hresult = 0x887a_0036_u32.cast_signed();

pub const DXGI_FORMAT_UNKNOWN: u32 = 0;
pub const DXGI_FORMAT_R16G16B16A16_FLOAT: u32 = 10;
pub const DXGI_FORMAT_R10G10B10A2_UNORM: u32 = 24;
pub const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
pub const DXGI_FORMAT_R8G8B8A8_UNORM_SRGB: u32 = 29;
pub const DXGI_FORMAT_B8G8R8A8_UNORM: u32 = 87;
pub const DXGI_FORMAT_B8G8R8X8_UNORM: u32 = 88;
pub const DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM: u32 = 89;
pub const DXGI_FORMAT_B8G8R8A8_UNORM_SRGB: u32 = 91;
pub const DXGI_FORMAT_B8G8R8X8_UNORM_SRGB: u32 = 93;
pub const DXGI_ENUM_MODES_INTERLACED: u32 = 1;
pub const DXGI_ENUM_MODES_SCALING: u32 = 2;
pub const DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED: u32 = 0;
pub const DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE: u32 = 1;
pub const DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST: u32 = 2;
pub const DXGI_MODE_SCALING_UNSPECIFIED: u32 = 0;
pub const DXGI_MODE_ROTATION_UNSPECIFIED: u32 = 0;
pub const DXGI_SCALING_STRETCH: u32 = 0;
pub const DXGI_ALPHA_MODE_IGNORE: u32 = 3;
pub const DXGI_FEATURE_PRESENT_ALLOW_TEARING: u32 = 0;
pub const DXGI_MEMORY_SEGMENT_GROUP_LOCAL: u32 = 0;
pub const DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL: u32 = 1;
pub const DXGI_ADAPTER_FLAG3_NONE: u32 = 0;
pub const DXGI_GRAPHICS_PREEMPTION_DMA_BUFFER_BOUNDARY: u32 = 0;
pub const DXGI_COMPUTE_PREEMPTION_DMA_BUFFER_BOUNDARY: u32 = 0;
pub const DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709: u32 = 0;
pub const DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020: u32 = 12;

#[cfg(windows)]
pub const MONITOR_DEFAULTTOPRIMARY: u32 = 1;
#[cfg(windows)]
pub const DM_INTERLACED: u32 = 2;
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

pub const IID_IUNKNOWN: Guid = Guid::new(0, 0, 0, [0xc0, 0, 0, 0, 0, 0, 0, 0x46]);
pub const IID_IDXGI_OBJECT: Guid = Guid::new(
    0xaec2_2fb8,
    0x76f3,
    0x4639,
    [0x9b, 0xe0, 0x28, 0xeb, 0x43, 0xa6, 0x7a, 0x2e],
);
pub const IID_IDXGI_FACTORY: Guid = Guid::new(
    0x7b71_66ec,
    0x21c7,
    0x44ae,
    [0xb2, 0x1a, 0xc9, 0xae, 0x32, 0x1a, 0xe3, 0x69],
);
pub const IID_IDXGI_FACTORY1: Guid = Guid::new(
    0x770a_ae78,
    0xf26f,
    0x4dba,
    [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
);
pub const IID_IDXGI_FACTORY2: Guid = Guid::new(
    0x50c8_3a1c,
    0xe072,
    0x4c48,
    [0x87, 0xb0, 0x36, 0x30, 0xfa, 0x36, 0xa6, 0xd0],
);
pub const IID_IDXGI_FACTORY3: Guid = Guid::new(
    0x2548_3823,
    0xcd46,
    0x4c7d,
    [0x86, 0xca, 0x47, 0xaa, 0x95, 0xb8, 0x37, 0xbd],
);
pub const IID_IDXGI_FACTORY4: Guid = Guid::new(
    0x1bc6_ea02,
    0xef36,
    0x464f,
    [0xbf, 0x0c, 0x21, 0xca, 0x39, 0xe5, 0x16, 0x8a],
);
pub const IID_IDXGI_FACTORY5: Guid = Guid::new(
    0x7632_e1f5,
    0xee65,
    0x4dca,
    [0x87, 0xfd, 0x84, 0xcd, 0x75, 0xf8, 0x83, 0x8d],
);
pub const IID_IDXGI_FACTORY6: Guid = Guid::new(
    0xc1b6_694f,
    0xff09,
    0x44a9,
    [0xb0, 0x3c, 0x77, 0x90, 0x0a, 0x0a, 0x1d, 0x17],
);
pub const IID_IDXGI_FACTORY7: Guid = Guid::new(
    0xa496_6eed,
    0x76db,
    0x44da,
    [0x84, 0xc1, 0xee, 0x9a, 0x7a, 0xfb, 0x20, 0xa8],
);
pub const IID_IDXGI_ADAPTER: Guid = Guid::new(
    0x2411_e7e1,
    0x12ac,
    0x4ccf,
    [0xbd, 0x14, 0x97, 0x98, 0xe8, 0x53, 0x4d, 0xc0],
);
pub const IID_IDXGI_ADAPTER1: Guid = Guid::new(
    0x2903_8f61,
    0x3839,
    0x4626,
    [0x91, 0xfd, 0x08, 0x68, 0x79, 0x01, 0x1a, 0x05],
);
pub const IID_IDXGI_ADAPTER2: Guid = Guid::new(
    0x0aa1_ae0a,
    0xfa0e,
    0x4b84,
    [0x86, 0x44, 0xe0, 0x5f, 0xf8, 0xe5, 0xac, 0xb5],
);
pub const IID_IDXGI_ADAPTER3: Guid = Guid::new(
    0x6459_67a4,
    0x1392,
    0x4310,
    [0xa7, 0x98, 0x80, 0x53, 0xce, 0x3e, 0x93, 0xfd],
);
pub const IID_IDXGI_ADAPTER4: Guid = Guid::new(
    0x3c8d_99d1,
    0x4fbf,
    0x4181,
    [0xa8, 0x2c, 0xaf, 0x66, 0xbf, 0x7b, 0xd2, 0x4e],
);
pub const IID_IDXGI_OUTPUT: Guid = Guid::new(
    0xae02_eedb,
    0xc735,
    0x4690,
    [0x8d, 0x52, 0x5a, 0x8d, 0xc2, 0x02, 0x13, 0xaa],
);
pub const IID_IDXGI_OUTPUT1: Guid = Guid::new(
    0x00cd_dea8,
    0x939b,
    0x4b83,
    [0xa3, 0x40, 0xa6, 0x85, 0x22, 0x66, 0x66, 0xcc],
);
pub const IID_IDXGI_OUTPUT2: Guid = Guid::new(
    0x595e_39d1,
    0x2724,
    0x4663,
    [0x99, 0xb1, 0xda, 0x96, 0x9d, 0xe2, 0x83, 0x64],
);
pub const IID_IDXGI_OUTPUT3: Guid = Guid::new(
    0x8a6b_b301,
    0x7e7e,
    0x41f4,
    [0xa8, 0xe0, 0x5b, 0x32, 0xf7, 0xf9, 0x9b, 0x18],
);
pub const IID_IDXGI_OUTPUT4: Guid = Guid::new(
    0xdc7d_ca35,
    0x2196,
    0x414d,
    [0x9f, 0x53, 0x61, 0x78, 0x84, 0x03, 0x2a, 0x60],
);
pub const IID_IDXGI_OUTPUT5: Guid = Guid::new(
    0x80a0_7424,
    0xab52,
    0x42eb,
    [0x83, 0x3c, 0x0c, 0x42, 0xfd, 0x28, 0x2d, 0x98],
);
pub const IID_IDXGI_OUTPUT6: Guid = Guid::new(
    0x0683_46e8,
    0xaaec,
    0x4b84,
    [0xad, 0xd7, 0x13, 0x7f, 0x51, 0x3f, 0x77, 0xa1],
);
pub const IID_IDXGI_DEVICE: Guid = Guid::new(
    0x54ec_77fa,
    0x1377,
    0x44e6,
    [0x8c, 0x32, 0x88, 0xfd, 0x5f, 0x44, 0xc8, 0x4c],
);
pub const IID_ID3D10_DEVICE: Guid = Guid::new(
    0x9b7e_4c0f,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);
pub const IID_ID3D10_DEVICE1: Guid = Guid::new(
    0x9b7e_4c8f,
    0x342c,
    0x4106,
    [0xa1, 0x9f, 0x4f, 0x27, 0x04, 0xf6, 0x89, 0xf0],
);
/// DXMT's `IMTLDXGIAdapter`: `IDXGIAdapter4`, then `GetMTLDevice` and `GetLocalD3DKMT`.
pub const IID_IMTL_DXGI_ADAPTER: Guid = Guid::new(
    0xacdf_3ef1,
    0xb33a,
    0x4cb6,
    [0x97, 0xbd, 0x1c, 0x19, 0x74, 0x82, 0x7e, 0x6d],
);
/// DXMT's `IMTLDXGIOutput`: `IDXGIOutput6`, then `GetGammaRamp`.
pub const IID_IMTL_DXGI_OUTPUT: Guid = Guid::new(
    0x4c11_28cd,
    0xb437,
    0x4d44,
    [0x99, 0x58, 0x5d, 0x76, 0x5b, 0xb8, 0x5a, 0x76],
);
/// DXMT's `IMTLDXGIDevice` (d3d11's DXGI device).
pub const IID_IMTL_DXGI_DEVICE: Guid = Guid::new(
    0x6bfa_1657,
    0x9cb1,
    0x471a,
    [0xa4, 0xfb, 0x7c, 0xac, 0xf8, 0xa8, 0x12, 0x07],
);
/// DXMT's `IMTLSwapChainFactory`, which a D3D11 or D3D12 device answers.
pub const IID_IMTL_SWAP_CHAIN_FACTORY: Guid = Guid::new(
    0x6c03_0003,
    0x460a,
    0x4b19,
    [0x90, 0x9b, 0x38, 0xf9, 0x7d, 0x20, 0x3e, 0x45],
);
/// Wine's private adapter interface, which the adapter refuses without a log line.
pub const IID_WINE_DXGI_ADAPTER: Guid = Guid::new(
    0x1739_9d75,
    0x964e,
    0x4c03,
    [0x99, 0xf8, 0x9d, 0x4f, 0xd1, 0x96, 0xdd, 0x62],
);
/// DXMT's NVIDIA extension, which `DXGIGetDebugInterface1` answers when it is on.
pub const DXMT_NVEXT_GUID: Guid = Guid::new(
    0xba0a_f616,
    0x4a43,
    0x4259,
    [0x81, 0x5c, 0xdb, 0x3b, 0x89, 0x82, 0x99, 0x05],
);

/// `0x3f`, `?`: what a byte sequence that is not UTF-8 decodes to.
const QUESTION: u32 = 0x3f;

/// `IUnknown` and `IDXGIObject`'s slots, which the factory, adapter and output start with.
#[repr(C)]
pub struct ObjectVtbl {
    pub unknown: IUnknownVtbl,
    pub set_private_data:
        unsafe extern "system" fn(*mut c_void, *const Guid, u32, *const c_void) -> Hresult,
    pub set_private_data_interface:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut c_void) -> Hresult,
    pub get_private_data:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut u32, *mut c_void) -> Hresult,
    pub get_parent:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult,
}

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

/// `IMTLDXGIAdapter`'s 21 slots: `IDXGIAdapter4`'s 19, then DXMT's two.
#[repr(C)]
pub struct AdapterVtbl {
    pub object: ObjectVtbl,
    pub enum_outputs: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> Hresult,
    pub get_desc: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc) -> Hresult,
    pub check_interface_support:
        unsafe extern "system" fn(*mut c_void, *const Guid, *mut i64) -> Hresult,
    pub get_desc1: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc1) -> Hresult,
    pub get_desc2: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc2) -> Hresult,
    pub register_hardware_content_protection_teardown_status_event:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> Hresult,
    pub unregister_hardware_content_protection_teardown_status:
        unsafe extern "system" fn(*mut c_void, u32),
    pub query_video_memory_info:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut QueryVideoMemoryInfo) -> Hresult,
    pub set_video_memory_reservation:
        unsafe extern "system" fn(*mut c_void, u32, u32, u64) -> Hresult,
    pub register_video_memory_budget_change_notification_event:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut u32) -> Hresult,
    pub unregister_video_memory_budget_change_notification:
        unsafe extern "system" fn(*mut c_void, u32),
    pub get_desc3: unsafe extern "system" fn(*mut c_void, *mut AdapterDesc3) -> Hresult,
    /// The Metal device, a winemetal handle the adapter keeps retained.
    pub get_mtl_device: unsafe extern "system" fn(*mut c_void) -> u64,
    pub get_local_d3dkmt: unsafe extern "system" fn(*mut c_void) -> u32,
}

/// `IMTLDXGIOutput`'s 30 slots: `IDXGIOutput6`'s 29, then `GetGammaRamp`.
#[repr(C)]
pub struct OutputVtbl {
    pub object: ObjectVtbl,
    pub get_desc: unsafe extern "system" fn(*mut c_void, *mut OutputDesc) -> Hresult,
    pub get_display_mode_list:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut u32, *mut ModeDesc) -> Hresult,
    pub find_closest_matching_mode: unsafe extern "system" fn(
        *mut c_void,
        *const ModeDesc,
        *mut ModeDesc,
        *mut c_void,
    ) -> Hresult,
    pub wait_for_v_blank: unsafe extern "system" fn(*mut c_void) -> Hresult,
    pub take_ownership: unsafe extern "system" fn(*mut c_void, *mut c_void, i32) -> Hresult,
    pub release_ownership: unsafe extern "system" fn(*mut c_void),
    pub get_gamma_control_capabilities:
        unsafe extern "system" fn(*mut c_void, *mut GammaControlCapabilities) -> Hresult,
    pub set_gamma_control: unsafe extern "system" fn(*mut c_void, *const GammaControl) -> Hresult,
    pub get_gamma_control: unsafe extern "system" fn(*mut c_void, *mut GammaControl) -> Hresult,
    pub set_display_surface: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    pub get_display_surface_data: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    /// `GetFrameStatistics`: its `DXGI_FRAME_STATISTICS` is never written (a stub).
    pub get_frame_statistics: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    pub get_display_mode_list1:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut u32, *mut ModeDesc1) -> Hresult,
    pub find_closest_matching_mode1: unsafe extern "system" fn(
        *mut c_void,
        *const ModeDesc1,
        *mut ModeDesc1,
        *mut c_void,
    ) -> Hresult,
    pub get_display_surface_data1: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
    pub duplicate_output:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> Hresult,
    pub supports_overlays: unsafe extern "system" fn(*mut c_void) -> i32,
    pub check_overlay_support:
        unsafe extern "system" fn(*mut c_void, u32, *mut c_void, *mut u32) -> Hresult,
    pub check_overlay_color_space_support:
        unsafe extern "system" fn(*mut c_void, u32, u32, *mut c_void, *mut u32) -> Hresult,
    pub duplicate_output1: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        u32,
        u32,
        *const u32,
        *mut *mut c_void,
    ) -> Hresult,
    pub get_desc1: unsafe extern "system" fn(*mut c_void, *mut OutputDesc1) -> Hresult,
    pub check_hardware_composition_support:
        unsafe extern "system" fn(*mut c_void, *mut u32) -> Hresult,
    /// The output's gamma ramp, which d3d11's swap chain reads at each fullscreen present.
    pub get_gamma_ramp: unsafe extern "system" fn(*mut c_void) -> *const GammaRamp,
}

/// `IMTLSwapChainFactory`, at a base of a D3D11 or D3D12 device that is not its first.
///
/// `CreateSwapChain` is C++ that may throw, so its slot unwinds.
#[repr(C)]
pub struct SwapChainFactoryVtbl {
    pub unknown: IUnknownVtbl,
    pub create_swap_chain: unsafe extern "system-unwind" fn(
        *mut c_void,
        *mut c_void,
        Hwnd,
        *const SwapChainDesc1,
        *const SwapChainFullscreenDesc,
        *mut *mut c_void,
    ) -> Hresult,
}

/// `LUID`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Luid {
    pub low_part: u32,
    pub high_part: i32,
}

/// `RECT`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// `DXGI_RATIONAL`.
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

/// `DXGI_SAMPLE_DESC`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct SampleDesc {
    pub count: u32,
    pub quality: u32,
}

/// `DXGI_RGB`.
#[repr(C)]
pub struct Rgb {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

/// `DXGI_ADAPTER_DESC`.
#[repr(C)]
pub struct AdapterDesc {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
}

/// `DXGI_ADAPTER_DESC1`.
#[repr(C)]
pub struct AdapterDesc1 {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
    pub flags: u32,
}

/// `DXGI_ADAPTER_DESC2`.
#[repr(C)]
pub struct AdapterDesc2 {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
    pub flags: u32,
    pub graphics_preemption_granularity: u32,
    pub compute_preemption_granularity: u32,
}

/// `DXGI_ADAPTER_DESC3`.
#[repr(C)]
pub struct AdapterDesc3 {
    pub description: [Wchar; 128],
    pub vendor_id: u32,
    pub device_id: u32,
    pub sub_sys_id: u32,
    pub revision: u32,
    pub dedicated_video_memory: usize,
    pub dedicated_system_memory: usize,
    pub shared_system_memory: usize,
    pub adapter_luid: Luid,
    pub flags: u32,
    pub graphics_preemption_granularity: u32,
    pub compute_preemption_granularity: u32,
}

/// `DXGI_OUTPUT_DESC`.
#[repr(C)]
pub struct OutputDesc {
    pub device_name: [Wchar; 32],
    pub desktop_coordinates: Rect,
    pub attached_to_desktop: i32,
    pub rotation: u32,
    pub monitor: Hmonitor,
}

/// `DXGI_OUTPUT_DESC1`.
#[repr(C)]
pub struct OutputDesc1 {
    pub device_name: [Wchar; 32],
    pub desktop_coordinates: Rect,
    pub attached_to_desktop: i32,
    pub rotation: u32,
    pub monitor: Hmonitor,
    pub bits_per_color: u32,
    pub color_space: u32,
    pub red_primary: [f32; 2],
    pub green_primary: [f32; 2],
    pub blue_primary: [f32; 2],
    pub white_point: [f32; 2],
    pub min_luminance: f32,
    pub max_luminance: f32,
    pub max_full_frame_luminance: f32,
}

/// `DXGI_MODE_DESC`.
#[repr(C)]
pub struct ModeDesc {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: Rational,
    pub format: u32,
    pub scanline_ordering: u32,
    pub scaling: u32,
}

/// `DXGI_MODE_DESC1`.
#[derive(Default)]
#[repr(C)]
pub struct ModeDesc1 {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: Rational,
    pub format: u32,
    pub scanline_ordering: u32,
    pub scaling: u32,
    pub stereo: i32,
}

/// `DXGI_SWAP_CHAIN_DESC`.
#[repr(C)]
pub struct SwapChainDesc {
    pub buffer_desc: ModeDesc,
    pub sample_desc: SampleDesc,
    pub buffer_usage: u32,
    pub buffer_count: u32,
    pub output_window: Hwnd,
    pub windowed: i32,
    pub swap_effect: u32,
    pub flags: u32,
}

/// `DXGI_SWAP_CHAIN_DESC1`.
#[repr(C)]
pub struct SwapChainDesc1 {
    pub width: u32,
    pub height: u32,
    pub format: u32,
    pub stereo: i32,
    pub sample_desc: SampleDesc,
    pub buffer_usage: u32,
    pub buffer_count: u32,
    pub scaling: u32,
    pub swap_effect: u32,
    pub alpha_mode: u32,
    pub flags: u32,
}

/// `DXGI_SWAP_CHAIN_FULLSCREEN_DESC`.
#[repr(C)]
pub struct SwapChainFullscreenDesc {
    pub refresh_rate: Rational,
    pub scanline_ordering: u32,
    pub scaling: u32,
    pub windowed: i32,
}

/// `DXGI_GAMMA_CONTROL`.
#[repr(C)]
pub struct GammaControl {
    pub scale: Rgb,
    pub offset: Rgb,
    pub gamma_curve: [Rgb; 1025],
}

/// `DXGI_GAMMA_CONTROL_CAPABILITIES`.
#[repr(C)]
pub struct GammaControlCapabilities {
    pub scale_and_offset_supported: i32,
    pub max_converted_value: f32,
    pub min_converted_value: f32,
    pub num_gamma_control_points: u32,
    pub control_point_positions: [f32; 1025],
}

/// `DXGI_QUERY_VIDEO_MEMORY_INFO`.
#[repr(C)]
pub struct QueryVideoMemoryInfo {
    pub budget: u64,
    pub current_usage: u64,
    pub available_for_reservation: u64,
    pub current_reservation: u64,
}

/// `POINT`, which `MonitorFromPoint` takes by value.
#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// `MONITORINFOEXW`: `MONITORINFO` and the display's device name.
#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
pub struct MonitorInfoExW {
    pub size: u32,
    pub monitor: Rect,
    pub work: Rect,
    pub flags: u32,
    pub device: [Wchar; 32],
}

/// `DEVMODEW`, with its two unions' display arms.
///
/// The first union's printer arm (`dmOrientation` to `dmPrintQuality`) lies under `position`,
/// `display_orientation` and `display_fixed_output`; the second union's `dmNup` under
/// `display_flags`.
#[cfg(windows)]
#[derive(Default)]
#[repr(C)]
pub struct DevModeW {
    pub device_name: [Wchar; 32],
    pub spec_version: u16,
    pub driver_version: u16,
    pub size: u16,
    pub driver_extra: u16,
    pub fields: u32,
    pub position: Point,
    pub display_orientation: u32,
    pub display_fixed_output: u32,
    pub color: i16,
    pub duplex: i16,
    pub y_resolution: i16,
    pub tt_option: i16,
    pub collate: i16,
    pub form_name: [Wchar; 32],
    pub log_pixels: u16,
    pub bits_per_pel: u32,
    pub pels_width: u32,
    pub pels_height: u32,
    pub display_flags: u32,
    pub display_frequency: u32,
    pub icm_method: u32,
    pub icm_intent: u32,
    pub media_type: u32,
    pub dither_type: u32,
    pub reserved1: u32,
    pub reserved2: u32,
    pub panning_width: u32,
    pub panning_height: u32,
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

/// winemetal's `WMTWsiWindow`: a window's state as a VM host's guest last told it.
#[cfg(not(windows))]
#[derive(Default)]
#[repr(C)]
pub struct WsiWindow {
    pub monitor: u64,
    pub width: u32,
    pub height: u32,
    pub flags: u32,
}

/// winemetal's `WMTWsiDisplay`: a display, its rectangle and its UTF-16 name.
#[cfg(not(windows))]
#[derive(Default)]
#[repr(C)]
pub struct WsiDisplay {
    pub monitor: u64,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub name: [u16; 32],
}

/// winemetal's `WMTWsiMode`: a display mode.
#[cfg(not(windows))]
#[derive(Default)]
#[repr(C)]
pub struct WsiHookMode {
    pub width: u32,
    pub height: u32,
    pub refresh_num: u32,
    pub refresh_den: u32,
}

/// winemetal's `WMTWsiHooks`: the window system a VM host answers for its guest.
///
/// Each hook answers 1 with its out-structure filled, or 0. `action` takes a `WMTWsiAction`,
/// which dxgi never makes. A null hook is `None`.
#[cfg(not(windows))]
#[repr(C)]
pub struct WsiHooks {
    pub window: Option<unsafe extern "C" fn(u64, *mut WsiWindow) -> i32>,
    pub display: Option<unsafe extern "C" fn(u32, *mut WsiDisplay) -> i32>,
    pub mode: Option<unsafe extern "C" fn(u64, u32, *mut WsiHookMode) -> i32>,
    pub action: Option<unsafe extern "C" fn(*const c_void)>,
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
    assert!(size_of::<DevModeW>() == 220 && align_of::<DevModeW>() == 4);
    assert!(offset_of!(DevModeW, device_name) == 0);
    assert!(offset_of!(DevModeW, spec_version) == 64);
    assert!(offset_of!(DevModeW, driver_version) == 66);
    assert!(offset_of!(DevModeW, size) == 68);
    assert!(offset_of!(DevModeW, driver_extra) == 70);
    assert!(offset_of!(DevModeW, fields) == 72);
    assert!(offset_of!(DevModeW, position) == 76);
    assert!(offset_of!(DevModeW, display_orientation) == 84);
    assert!(offset_of!(DevModeW, display_fixed_output) == 88);
    assert!(offset_of!(DevModeW, color) == 92);
    assert!(offset_of!(DevModeW, duplex) == 94);
    assert!(offset_of!(DevModeW, y_resolution) == 96);
    assert!(offset_of!(DevModeW, tt_option) == 98);
    assert!(offset_of!(DevModeW, collate) == 100);
    assert!(offset_of!(DevModeW, form_name) == 102);
    assert!(offset_of!(DevModeW, log_pixels) == 166);
    assert!(offset_of!(DevModeW, bits_per_pel) == 168);
    assert!(offset_of!(DevModeW, pels_width) == 172);
    assert!(offset_of!(DevModeW, pels_height) == 176);
    assert!(offset_of!(DevModeW, display_flags) == 180);
    assert!(offset_of!(DevModeW, display_frequency) == 184);
    assert!(offset_of!(DevModeW, icm_method) == 188);
    assert!(offset_of!(DevModeW, icm_intent) == 192);
    assert!(offset_of!(DevModeW, media_type) == 196);
    assert!(offset_of!(DevModeW, dither_type) == 200);
    assert!(offset_of!(DevModeW, reserved1) == 204);
    assert!(offset_of!(DevModeW, reserved2) == 208);
    assert!(offset_of!(DevModeW, panning_width) == 212);
    assert!(offset_of!(DevModeW, panning_height) == 216);
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
    assert!(size_of::<MonitorInfoExW>() == 104 && align_of::<MonitorInfoExW>() == 4);
    assert!(offset_of!(MonitorInfoExW, size) == 0);
    assert!(offset_of!(MonitorInfoExW, monitor) == 4);
    assert!(offset_of!(MonitorInfoExW, work) == 20);
    assert!(offset_of!(MonitorInfoExW, flags) == 36);
    assert!(offset_of!(MonitorInfoExW, device) == 40);
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
    assert!(size_of::<Point>() == 8 && align_of::<Point>() == 4);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
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
    assert!(size_of::<DevModeW>() == 220 && align_of::<DevModeW>() == 4);
    assert!(offset_of!(DevModeW, device_name) == 0);
    assert!(offset_of!(DevModeW, spec_version) == 64);
    assert!(offset_of!(DevModeW, driver_version) == 66);
    assert!(offset_of!(DevModeW, size) == 68);
    assert!(offset_of!(DevModeW, driver_extra) == 70);
    assert!(offset_of!(DevModeW, fields) == 72);
    assert!(offset_of!(DevModeW, position) == 76);
    assert!(offset_of!(DevModeW, display_orientation) == 84);
    assert!(offset_of!(DevModeW, display_fixed_output) == 88);
    assert!(offset_of!(DevModeW, color) == 92);
    assert!(offset_of!(DevModeW, duplex) == 94);
    assert!(offset_of!(DevModeW, y_resolution) == 96);
    assert!(offset_of!(DevModeW, tt_option) == 98);
    assert!(offset_of!(DevModeW, collate) == 100);
    assert!(offset_of!(DevModeW, form_name) == 102);
    assert!(offset_of!(DevModeW, log_pixels) == 166);
    assert!(offset_of!(DevModeW, bits_per_pel) == 168);
    assert!(offset_of!(DevModeW, pels_width) == 172);
    assert!(offset_of!(DevModeW, pels_height) == 176);
    assert!(offset_of!(DevModeW, display_flags) == 180);
    assert!(offset_of!(DevModeW, display_frequency) == 184);
    assert!(offset_of!(DevModeW, icm_method) == 188);
    assert!(offset_of!(DevModeW, icm_intent) == 192);
    assert!(offset_of!(DevModeW, media_type) == 196);
    assert!(offset_of!(DevModeW, dither_type) == 200);
    assert!(offset_of!(DevModeW, reserved1) == 204);
    assert!(offset_of!(DevModeW, reserved2) == 208);
    assert!(offset_of!(DevModeW, panning_width) == 212);
    assert!(offset_of!(DevModeW, panning_height) == 216);
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
    assert!(size_of::<MonitorInfoExW>() == 104 && align_of::<MonitorInfoExW>() == 4);
    assert!(offset_of!(MonitorInfoExW, size) == 0);
    assert!(offset_of!(MonitorInfoExW, monitor) == 4);
    assert!(offset_of!(MonitorInfoExW, work) == 20);
    assert!(offset_of!(MonitorInfoExW, flags) == 36);
    assert!(offset_of!(MonitorInfoExW, device) == 40);
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
    assert!(size_of::<Point>() == 8 && align_of::<Point>() == 4);
    assert!(offset_of!(Point, x) == 0);
    assert!(offset_of!(Point, y) == 4);
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
    assert!(size_of::<WsiDisplay>() == 88 && align_of::<WsiDisplay>() == 8);
    assert!(offset_of!(WsiDisplay, monitor) == 0);
    assert!(offset_of!(WsiDisplay, left) == 8);
    assert!(offset_of!(WsiDisplay, top) == 12);
    assert!(offset_of!(WsiDisplay, right) == 16);
    assert!(offset_of!(WsiDisplay, bottom) == 20);
    assert!(offset_of!(WsiDisplay, name) == 24);
    assert!(size_of::<WsiHookMode>() == 16 && align_of::<WsiHookMode>() == 4);
    assert!(offset_of!(WsiHookMode, width) == 0);
    assert!(offset_of!(WsiHookMode, height) == 4);
    assert!(offset_of!(WsiHookMode, refresh_num) == 8);
    assert!(offset_of!(WsiHookMode, refresh_den) == 12);
    assert!(size_of::<WsiHooks>() == 32 && align_of::<WsiHooks>() == 8);
    assert!(offset_of!(WsiHooks, window) == 0);
    assert!(offset_of!(WsiHooks, display) == 8);
    assert!(offset_of!(WsiHooks, mode) == 16);
    assert!(offset_of!(WsiHooks, action) == 24);
    assert!(size_of::<WsiWindow>() == 24 && align_of::<WsiWindow>() == 8);
    assert!(offset_of!(WsiWindow, monitor) == 0);
    assert!(offset_of!(WsiWindow, width) == 8);
    assert!(offset_of!(WsiWindow, height) == 12);
    assert!(offset_of!(WsiWindow, flags) == 16);
};

#[cfg(test)]
mod tests;
