//! `ID3D12Resource2` (`d3d12_buffer.cpp`, `d3d12_texture.cpp`, `d3d12_resource_helper.cpp`,
//!
//! `d3d12_device.hpp:112-150`).

pub mod buffer;
pub mod texture;

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::resource::{Buffer, Texture};
use dxmt_util::{com::ComObject, private_data::PrivateData};

use crate::{
    abi::{
        resource::{HeapProperties, ResourceDesc},
        vtbl::{D3d12CpuDescriptorHandle, ID3D12Resource2Vtbl},
    },
    com::{ComRefCounted, Hresult},
    device::Device,
    heap::HeapObject,
};

/// `MTLD3D12Resource` (`d3d12_device.hpp:112-150`).
#[repr(C)]
pub struct Resource {
    pub vtbl: &'static ID3D12Resource2Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub texture: Option<Arc<Texture>>,
    pub buffer: Option<Arc<Buffer>>,
    pub placed_heap: *mut HeapObject,
    pub desc: ResourceDesc,
    pub heap_props: HeapProperties,
}

impl ComRefCounted for Resource {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Resource {
    /// Underlying core [`Buffer`], if this resource is a buffer.
    #[must_use]
    pub const fn buffer(&self) -> Option<&Arc<Buffer>> {
        self.buffer.as_ref()
    }

    /// Underlying core [`Texture`], if this resource is a texture.
    #[must_use]
    pub const fn texture(&self) -> Option<&Arc<Texture>> {
        self.texture.as_ref()
    }

    /// GPU virtual address (`0` for textures).
    #[must_use]
    pub fn gpu_va(&self) -> u64 {
        self.buffer
            .as_ref()
            .and_then(|b| b.current())
            .map_or(0, |a| a.gpu_address())
    }

    /// D3D12 resource description (`GetDesc`).
    #[must_use]
    pub const fn desc(&self) -> &ResourceDesc {
        &self.desc
    }

    /// D3D12 heap properties (`GetHeapProperties`).
    #[must_use]
    pub const fn heap_props(&self) -> &HeapProperties {
        &self.heap_props
    }
}

/// Casts a non-null `ID3D12Resource` pointer if its vtable matches [`Resource`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_resource<'a>(raw: *mut c_void) -> Option<&'a Resource> {
    let _ = raw;
    crate::not_implemented!("resource::cast_resource")
}

/// `ID3D12Device::CreateConstantBufferView` (`d3d12_device.cpp:837`).
///
/// # Safety
///
/// `this` and `desc` are caller-supplied pointers.
pub unsafe fn create_constant_buffer_view(
    _this: *mut c_void,
    _desc: *const c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateConstantBufferView");
    let (): () = crate::not_implemented!("ID3D12Device::CreateConstantBufferView");
}

/// `ID3D12Device::CreateShaderResourceView` (`d3d12_device.cpp:842`).
///
/// # Safety
///
/// `this`, `resource`, and `desc` are caller-supplied pointers.
pub unsafe fn create_shader_resource_view(
    _this: *mut c_void,
    _resource: *mut c_void,
    _desc: *const c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateShaderResourceView");
    let (): () = crate::not_implemented!("ID3D12Device::CreateShaderResourceView");
}

/// `ID3D12Device::CreateUnorderedAccessView` (`d3d12_device.cpp:854`).
///
/// # Safety
///
/// `this`, `resource`, `counter`, and `desc` are caller-supplied pointers.
pub unsafe fn create_unordered_access_view(
    _this: *mut c_void,
    _resource: *mut c_void,
    _counter: *mut c_void,
    _desc: *const c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateUnorderedAccessView");
    let (): () = crate::not_implemented!("ID3D12Device::CreateUnorderedAccessView");
}

/// `ID3D12Device::CreateRenderTargetView` (`d3d12_device.cpp:860`).
///
/// # Safety
///
/// `this`, `resource`, and `desc` are caller-supplied pointers.
pub unsafe fn create_render_target_view(
    _this: *mut c_void,
    _resource: *mut c_void,
    _desc: *const c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateRenderTargetView");
    let (): () = crate::not_implemented!("ID3D12Device::CreateRenderTargetView");
}

/// `ID3D12Device::CreateDepthStencilView` (`d3d12_device.cpp:868`).
///
/// # Safety
///
/// `this`, `resource`, and `desc` are caller-supplied pointers.
pub unsafe fn create_depth_stencil_view(
    _this: *mut c_void,
    _resource: *mut c_void,
    _desc: *const c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateDepthStencilView");
    let (): () = crate::not_implemented!("ID3D12Device::CreateDepthStencilView");
}

/// `ID3D12Device8::CreateSamplerFeedbackUnorderedAccessView` (`d3d12_device.cpp:1790`).
///
/// # Safety
///
/// `this` and the resource arguments are caller-supplied pointers.
pub unsafe fn create_sampler_feedback_unordered_access_view(
    _this: *mut c_void,
    _targeted_resource: *mut c_void,
    _feedback_resource: *mut c_void,
    _dest: D3d12CpuDescriptorHandle,
) {
    crate::scope!("device.CreateSamplerFeedbackUnorderedAccessView");
    let (): () = crate::not_implemented!("ID3D12Device::CreateSamplerFeedbackUnorderedAccessView");
}

/// `ID3D12Device::CreateCommittedResource` (`d3d12_device.cpp:1174`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_committed_resource(
    _this: *mut c_void,
    _heap_props: *const c_void,
    _heap_flags: u32,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource");
    crate::not_implemented!("ID3D12Device::CreateCommittedResource")
}

/// `ID3D12Device::CreatePlacedResource` (`d3d12_device.cpp:1200`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_placed_resource(
    _this: *mut c_void,
    _heap: *mut c_void,
    _heap_offset: u64,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePlacedResource");
    crate::not_implemented!("ID3D12Device::CreatePlacedResource")
}

/// `ID3D12Device::CreateReservedResource` (`d3d12_device.cpp:1218`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe fn create_reserved_resource(
    _this: *mut c_void,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateReservedResource");
    crate::not_implemented!("ID3D12Device::CreateReservedResource")
}

/// `ID3D12Device4::CreateCommittedResource1` (`d3d12_device.cpp:1600`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_committed_resource1(
    _this: *mut c_void,
    _heap_props: *const c_void,
    _heap_flags: u32,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _protected_session: *mut c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource1");
    crate::not_implemented!("ID3D12Device::CreateCommittedResource1")
}

/// `ID3D12Device4::CreateReservedResource1` (`d3d12_device.cpp:1620`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe fn create_reserved_resource1(
    _this: *mut c_void,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _protected_session: *mut c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateReservedResource1");
    crate::not_implemented!("ID3D12Device::CreateReservedResource1")
}

/// `ID3D12Device8::CreateCommittedResource2` (`d3d12_device.cpp:1767`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_committed_resource2(
    _this: *mut c_void,
    _heap_props: *const c_void,
    _heap_flags: u32,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _protected_session: *mut c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource2");
    crate::not_implemented!("ID3D12Device::CreateCommittedResource2")
}

/// `ID3D12Device8::CreatePlacedResource1` (`d3d12_device.cpp:1779`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_placed_resource1(
    _this: *mut c_void,
    _heap: *mut c_void,
    _heap_offset: u64,
    _desc: *const c_void,
    _initial_state: u32,
    _optimized_clear: *const c_void,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePlacedResource1");
    crate::not_implemented!("ID3D12Device::CreatePlacedResource1")
}

/// `ID3D12Device10::CreateCommittedResource3` (`d3d12_device.cpp:1816`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_committed_resource3(
    _this: *mut c_void,
    _heap_props: *const c_void,
    _heap_flags: u32,
    _desc: *const c_void,
    _initial_layout: u32,
    _optimized_clear: *const c_void,
    _protected_session: *mut c_void,
    _num_castable: u32,
    _castable_formats: *mut u32,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommittedResource3");
    crate::not_implemented!("ID3D12Device::CreateCommittedResource3")
}

/// `ID3D12Device10::CreatePlacedResource2` (`d3d12_device.cpp:1833`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_placed_resource2(
    _this: *mut c_void,
    _heap: *mut c_void,
    _heap_offset: u64,
    _desc: *const c_void,
    _initial_layout: u32,
    _optimized_clear: *const c_void,
    _num_castable: u32,
    _castable_formats: *mut u32,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePlacedResource2");
    crate::not_implemented!("ID3D12Device::CreatePlacedResource2")
}

/// `ID3D12Device10::CreateReservedResource2` (`d3d12_device.cpp:1849`).
///
/// # Safety
///
/// `this` and the resource-creation pointers are caller-supplied.
pub unsafe extern "system-unwind" fn create_reserved_resource2(
    _this: *mut c_void,
    _desc: *const c_void,
    _initial_layout: u32,
    _optimized_clear: *const c_void,
    _protected_session: *mut c_void,
    _num_castable: u32,
    _castable_formats: *mut u32,
    _riid: *const c_void,
    _resource: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateReservedResource2");
    crate::not_implemented!("ID3D12Device::CreateReservedResource2")
}
