//! `d3d11.dll` / `d3d11.dylib` exported entry points (`d3d11.def`, `d3d11.cpp`).

use core::ffi::c_void;

use crate::{
    abi::records::DxgiSwapChainDesc,
    com::{E_NOTIMPL, Guid, Hresult},
};

/// `D3D11CoreCreateDevice`.
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "D3D11CoreCreateDevice"))]
pub const extern "system" fn d3d11_core_create_device(
    factory: *mut c_void,
    adapter: *mut c_void,
    flags: u32,
    feature_levels: *const u32,
    levels: u32,
    out_device: *mut *mut c_void,
) -> Hresult {
    let _ = (factory, adapter, flags, feature_levels, levels, out_device);
    E_NOTIMPL
}

/// `D3D11CreateDevice`.
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "D3D11CreateDevice"))]
pub const extern "system" fn d3d11_create_device(
    adapter: *mut c_void,
    driver_type: u32,
    software: *mut c_void,
    flags: u32,
    feature_levels: *const u32,
    levels: u32,
    sdk_version: u32,
    out_device: *mut *mut c_void,
    out_feature_level: *mut u32,
    out_immediate_context: *mut *mut c_void,
) -> Hresult {
    let _ = (
        adapter,
        driver_type,
        software,
        flags,
        feature_levels,
        levels,
        sdk_version,
        out_device,
        out_feature_level,
        out_immediate_context,
    );
    E_NOTIMPL
}

/// `D3D11CreateDeviceAndSwapChain`.
#[must_use]
#[cfg_attr(
    feature = "front-end",
    unsafe(export_name = "D3D11CreateDeviceAndSwapChain")
)]
pub const extern "system" fn d3d11_create_device_and_swap_chain(
    adapter: *mut c_void,
    driver_type: u32,
    software: *mut c_void,
    flags: u32,
    feature_levels: *const u32,
    levels: u32,
    sdk_version: u32,
    swap_chain_desc: *const DxgiSwapChainDesc,
    out_swap_chain: *mut *mut c_void,
    out_device: *mut *mut c_void,
    out_feature_level: *mut u32,
    out_immediate_context: *mut *mut c_void,
) -> Hresult {
    let _ = (
        adapter,
        driver_type,
        software,
        flags,
        feature_levels,
        levels,
        sdk_version,
        swap_chain_desc,
        out_swap_chain,
        out_device,
        out_feature_level,
        out_immediate_context,
    );
    E_NOTIMPL
}

/// `D3D11On12CreateDevice`: zeroes out-parameters and returns `E_NOTIMPL` (`d3d11.cpp:259-277`).
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "D3D11On12CreateDevice"))]
pub const extern "system" fn d3d11_on12_create_device(
    device: *mut c_void,
    flags: u32,
    feature_levels: *const u32,
    levels: u32,
    command_queues: *const *mut c_void,
    num_queues: u32,
    node_mask: u32,
    out_device: *mut *mut c_void,
    out_immediate_context: *mut *mut c_void,
    out_chosen_feature_level: *mut u32,
) -> Hresult {
    let _ = (
        device,
        flags,
        feature_levels,
        levels,
        command_queues,
        num_queues,
        node_mask,
        out_device,
        out_immediate_context,
        out_chosen_feature_level,
    );
    E_NOTIMPL
}

/// `D3D10CoreCreateDevice` internal helper (`MTLD3D10CreateDeviceAndSwapChain` path).
#[must_use]
pub const fn create_d3d10_device(
    factory: *mut c_void,
    adapter: *mut c_void,
    flags: u32,
    feature_level: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (factory, adapter, flags, feature_level, riid, out);
    E_NOTIMPL
}

/// Native export `MTLD3D11WaitIdle` (`d3d11.cpp:244-251`).
#[cfg(target_os = "macos")]
#[must_use]
#[cfg_attr(feature = "front-end", unsafe(export_name = "MTLD3D11WaitIdle"))]
pub extern "C-unwind" fn mtld3d11_wait_idle() -> i32 {
    i32::from(crate::scheduler::wait_idle(10_000))
}
