//! `ID3D12CommandQueue` and `IMTLSwapChainFactory` (`d3d12_command_queue.cpp`): encoder ordering,
//!
//! merging, clear/timestamp folding, in-flight command-buffer pool, completion thread, and present.

pub mod dump;

use core::{ffi::c_void, mem::offset_of};
use std::sync::{Arc, Mutex};

use dxmt_core::presenter::{Presenter, framegen::FrameGen};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::Obj;

use crate::{
    abi::{ext::IMTLSwapChainFactoryVtbl, vtbl::ID3D12CommandQueueVtbl},
    com::{ComRefCounted, Hresult},
    device::Device,
    fence::FenceObject,
    list::List,
    resource::Resource,
};

/// `MTLD3D12CommandQueueImpl` (`d3d12_command_queue.cpp:1227`).
///
/// Primary vtable pointer (`ID3D12CommandQueue`) sits at offset 0; the companion
/// `IMTLSwapChainFactory` vtable pointer sits at [`Queue::FACTORY_OFFSET`].
#[repr(C)]
pub struct Queue {
    pub vtbl: &'static ID3D12CommandQueueVtbl,
    pub swapchain_factory_vtbl: &'static IMTLSwapChainFactoryVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub mtl_queue: Option<Obj>,
    pub frame_gen: Mutex<FrameGen>,
}

impl ComRefCounted for Queue {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Queue {
    /// Byte offset of `swapchain_factory_vtbl` inside `Queue` (`IMTLSwapChainFactory` companion
    ///
    /// pointer).
    pub const FACTORY_OFFSET: usize = offset_of!(Self, swapchain_factory_vtbl);

    /// Executes `lists` in a single Metal command buffer (`ExecuteCommandLists`,
    ///
    /// `d3d12_command_queue.cpp:1921`).
    pub fn execute(&self, _lists: &[&List]) {
        let (): () = crate::not_implemented!("queue::Queue::execute");
    }

    /// Encodes a GPU signal on `fence` to `value` (`Signal`, `d3d12_command_queue.cpp:2071`).
    pub fn signal(&self, _fence: &FenceObject, _value: u64) -> Hresult {
        crate::not_implemented!("queue::Queue::signal")
    }

    /// Encodes a GPU wait on `fence` for `value` (`Wait`, `d3d12_command_queue.cpp:2080`).
    pub fn wait(&self, _fence: &FenceObject, _value: u64) -> Hresult {
        crate::not_implemented!("queue::Queue::wait")
    }

    /// Presents `backbuffer` through `presenter` (`Present`, `d3d12_command_queue.cpp:2480`).
    pub fn present(
        &self,
        _presenter: &Arc<Presenter>,
        _backbuffer: &Resource,
        _latency_waitable: *mut c_void,
        _after: f64,
        _refresh: f64,
    ) -> Hresult {
        crate::not_implemented!("queue::Queue::present")
    }
}

/// Casts a non-null `ID3D12CommandQueue` pointer if its vtable matches [`Queue`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_queue<'a>(raw: *mut c_void) -> Option<&'a Queue> {
    let _ = raw;
    crate::not_implemented!("queue::cast_queue")
}

/// `ID3D12Device::CreateCommandQueue` (`d3d12_device.cpp:477`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `queue` are caller-supplied COM pointers.
pub unsafe fn create_command_queue(
    _this: *mut c_void,
    _desc: *const c_void,
    _riid: *const c_void,
    _queue: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandQueue");
    crate::not_implemented!("ID3D12Device::CreateCommandQueue")
}

/// `ID3D12Device9::CreateCommandQueue1` (`d3d12_device.cpp:1833`).
///
/// # Safety
///
/// `this`, `desc`, `creator_id`, `riid`, and `queue` are caller-supplied COM pointers.
pub unsafe fn create_command_queue1(
    _this: *mut c_void,
    _desc: *const c_void,
    _creator_id: *const c_void,
    _riid: *const c_void,
    _queue: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandQueue1");
    crate::not_implemented!("ID3D12Device::CreateCommandQueue1")
}
