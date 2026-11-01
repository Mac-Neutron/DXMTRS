//! `IDXGISwapChain4` and `MetalFX` spatial swapchain (`d3d11_swapchain.cpp`).

use core::ffi::c_void;

use crate::{
    abi::records::{DxgiSwapChainDesc1, DxgiSwapChainFullscreenDesc},
    com::{E_NOTIMPL, Hresult},
};

/// `IMTLSwapChainFactory::CreateSwapChain` (`d3d11_swapchain.cpp:1149-1180`).
#[must_use]
pub const fn create_swap_chain(
    device: *mut c_void,
    factory: *mut c_void,
    hwnd: *mut c_void,
    desc: *const DxgiSwapChainDesc1,
    fs_desc: *const DxgiSwapChainFullscreenDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, factory, hwnd, desc, fs_desc, out);
    E_NOTIMPL
}
