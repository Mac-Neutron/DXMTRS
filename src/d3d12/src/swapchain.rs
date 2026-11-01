//! `IDXGISwapChain4` (`d3d12_swapchain.cpp`): D3D12 flip-model swap chain over
//!
//! [`dxmt_core::presenter::Presenter`].

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{hud::Hud, presenter::Presenter};
use dxmt_util::{com::ComObject, dxgi_vtbl::IDXGISwapChain4Vtbl, private_data::PrivateData};

use crate::{
    com::{ComRefCounted, Hresult},
    device::Device,
    queue::Queue,
};

/// `SwapChain` (`d3d12_swapchain.cpp:102`).
#[repr(C)]
pub struct SwapChain {
    pub vtbl: &'static IDXGISwapChain4Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub queue: *const Queue,
    pub presenter: Option<Arc<Presenter>>,
    pub hud: Option<Hud>,
}

impl ComRefCounted for SwapChain {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

/// Creates a D3D12 [`SwapChain`] (`CreateSwapChain`, `d3d12_swapchain.cpp:976`).
///
/// # Safety
///
/// `factory`, `hwnd`, `desc`, `fullscreen_desc`, and `out` are caller-supplied pointers.
pub unsafe fn create(
    _factory: *mut c_void,
    _device: &Device,
    _queue: &Queue,
    _hwnd: *mut c_void,
    _desc: *const c_void,
    _fullscreen_desc: *const c_void,
    _out: *mut *mut c_void,
) -> Hresult {
    crate::not_implemented!("swapchain::create")
}

/// `IDXGISwapChain1::GetCoreWindow` (`d3d12_swapchain.cpp:768`).
///
/// # Safety
///
/// `this`, `refiid`, and `unk` are caller-supplied pointers.
pub unsafe fn get_core_window(
    _this: *mut c_void,
    _refiid: *const c_void,
    _unk: *mut *mut c_void,
) -> Hresult {
    crate::scope!("swapchain.GetCoreWindow");
    crate::not_implemented!("IDXGISwapChain::GetCoreWindow")
}
