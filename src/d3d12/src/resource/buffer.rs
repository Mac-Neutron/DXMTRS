//! `ID3D12Resource` buffer implementation (`d3d12_buffer.cpp`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::resource::{Buffer, BufferAllocationFlags, BufferViewDescriptor, External};
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::{
    MTLDevice_heapBufferSizeAndAlign,
    wire::{WMT_PIXEL_FORMAT_R32_FLOAT, WMT_PIXEL_FORMAT_R32_UINT},
};

use crate::{
    abi::{
        resource::{
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_RESOURCE_DIMENSION_BUFFER,
            D3D12_TEXTURE_LAYOUT_ROW_MAJOR, HeapProperties, ResourceDesc,
        },
        views::{
            D3D12_BUFFER_SRV_FLAG_RAW, D3D12_BUFFER_UAV_FLAG_RAW, D3D12_HEAP_FLAG_DENY_BUFFERS,
            D3D12_SRV_DIMENSION_BUFFER, D3D12_UAV_DIMENSION_BUFFER, ShaderResourceViewDesc,
            UnorderedAccessViewDesc,
        },
        vtbl::{D3d12CpuDescriptorHandle, ID3D12Resource2Vtbl},
    },
    com::{self, E_FAIL, E_INVALIDARG, E_NOTIMPL, Hresult, S_FALSE, S_OK},
    descriptor_heap::BufferSlice,
    device::Device,
    heap::HeapObject,
    resource::Resource,
    resource_math::{is_cpu_visible_heap, is_private_heap},
    slot,
};

/// `ID3D12Resource2` vtable for buffer resources (`d3d12_buffer.cpp`).
pub static BUFFER_VTBL: ID3D12Resource2Vtbl = ID3D12Resource2Vtbl {
    query_interface: slot!(
        "buffer.QueryInterface",
        (this: *mut c_void, riid: *const c_void, out: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this`, `riid`, and `out` are forwarded from COM slot.
            unsafe { super::query_interface(this, riid, out, "D3D12Buffer") }
        }
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a valid `Resource` COM pointer.
        unsafe { com::add_ref_com(this.cast::<Resource>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a valid `Resource` COM pointer.
        unsafe { com::release_com(this.cast::<Resource>()) }
    }),
    get_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::get_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::set_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data_interface: slot!(
        (this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::set_private_data_interface(this, guid, data) }
        }
    ),
    set_name: slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: forwarded from COM slot.
        unsafe { super::set_name(this, name) }
    }),
    get_device: slot!(
        (this: *mut c_void, riid: *const c_void, out: *mut *mut c_void) -> Hresult => {
            // SAFETY: forwarded from COM slot.
            unsafe { super::get_device(this, riid, out) }
        }
    ),
    map: slot!(
        "buffer.Map",
        (this: *mut c_void, subresource: u32, _read_range: *const c_void, data: *mut *mut c_void) -> Hresult => {
            if subresource != 0 {
                return E_INVALIDARG;
            }
            // SAFETY: `this` is a valid `Resource` COM pointer.
            let res = unsafe { &*this.cast::<Resource>() };
            if !is_cpu_visible_heap(&res.heap_props) {
                return E_INVALIDARG;
            }
            if !data.is_null() {
                let ptr = res
                    .buffer
                    .as_ref()
                    .and_then(|b| b.current())
                    .map_or(ptr::null_mut(), |a| a.mapped_memory(0).cast::<c_void>());
                // SAFETY: `data` was checked non-null above.
                unsafe { *data = ptr };
            }
            S_OK
        }
    ),
    unmap: slot!(
        "buffer.Unmap",
        (_this: *mut c_void, _subresource: u32, _written_range: *const c_void) => {}
    ),
    get_desc: slot!(
        "buffer.GetDesc",
        (this: *mut c_void, out: *mut c_void) -> *mut c_void => {
            // SAFETY: `this` is a valid `Resource` COM pointer.
            let res = unsafe { &*this.cast::<Resource>() };
            // SAFETY: `out` points to a caller-provided `D3D12_RESOURCE_DESC`.
            unsafe { *out.cast::<ResourceDesc>() = super::clone_desc(&res.desc) };
            out
        }
    ),
    get_gpu_virtual_address: slot!(
        "buffer.GetGPUVirtualAddress",
        (this: *mut c_void) -> u64 => {
            // SAFETY: `this` is a valid `Resource` COM pointer.
            let res = unsafe { &*this.cast::<Resource>() };
            res.buffer
                .as_ref()
                .and_then(|b| b.current())
                .map_or(0, |a| a.gpu_address())
        }
    ),
    write_to_subresource: slot!(
        "buffer.WriteToSubresource",
        (
            _this: *mut c_void,
            _dst_subresource: u32,
            _dst_box: *const c_void,
            _src_data: *const c_void,
            _src_row_pitch: u32,
            _src_depth_pitch: u32
        ) -> Hresult => E_INVALIDARG
    ),
    read_from_subresource: slot!(
        "buffer.ReadFromSubresource",
        (
            _this: *mut c_void,
            _dst_data: *mut c_void,
            _dst_row_pitch: u32,
            _dst_depth_pitch: u32,
            _src_subresource: u32,
            _src_box: *const c_void
        ) -> Hresult => E_INVALIDARG
    ),
    get_heap_properties: slot!(
        "buffer.GetHeapProperties",
        (this: *mut c_void, heap_props: *mut c_void, heap_flags: *mut u32) -> Hresult => {
            // SAFETY: `this`, `heap_props`, and `heap_flags` are forwarded from COM slot.
            unsafe { super::get_heap_properties(this, heap_props, heap_flags) }
        }
    ),
    get_protected_resource_session: slot!(
        (_this: *mut c_void, _riid: *const c_void, session: *mut *mut c_void) -> Hresult => {
            if !session.is_null() {
                // SAFETY: `session` was checked non-null above.
                unsafe { com::init_out_ptr(session) };
            }
            E_NOTIMPL
        }
    ),
    get_desc1: slot!(
        (this: *mut c_void, out: *mut c_void) -> *mut c_void => {
            // SAFETY: `this` and `out` are forwarded from COM slot.
            unsafe { super::get_desc1(this, out) }
        }
    ),
};

/// Creates a committed or placed buffer `Resource`.
///
/// Mirrors `CreateCommittedBuffer` / `CreatePlacedBuffer` (`d3d12_buffer.cpp:287-320`).
///
/// # Safety
///
/// `device`, `riid`, and `out` are caller-supplied COM pointers. If `placed` is `Some`, its
/// `HeapObject` pointer must be valid.
pub unsafe fn create(
    device: *mut Device,
    heap_props: &HeapProperties,
    heap_flags: u32,
    desc: &ResourceDesc,
    placed: Option<(*mut HeapObject, u64)>,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if placed.is_some() && (heap_flags & D3D12_HEAP_FLAG_DENY_BUFFERS) != 0 {
        return E_INVALIDARG;
    }
    if desc.dimension != D3D12_RESOURCE_DIMENSION_BUFFER
        || desc.width == 0
        || desc.height != 1
        || desc.depth_or_array_size != 1
        || desc.mip_levels != 1
        || desc.format != 0
        || desc.sample_desc.count != 1
        || desc.sample_desc.quality != 0
        || desc.layout != D3D12_TEXTURE_LAYOUT_ROW_MAJOR
    {
        return E_INVALIDARG;
    }

    let mut norm_desc = super::clone_desc(desc);
    let default_align = D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT;
    if norm_desc.alignment != 0 {
        if norm_desc.alignment != default_align {
            return E_INVALIDARG;
        }
    } else {
        norm_desc.alignment = default_align;
    }

    // SAFETY: if non-null, `device` is a valid `Device` pointer.
    let mtl = (!device.is_null())
        .then(|| unsafe { (*device).mtl.as_ref() })
        .flatten();

    if let Some((heap_ptr, offset)) = placed
        && !heap_ptr.is_null()
    {
        // SAFETY: `heap_ptr` is a valid `HeapObject` pointer from the caller.
        let hdesc = unsafe { super::heap_desc(heap_ptr) };
        let req_size = if let Some(mtl_dev) = mtl {
            // SAFETY: `mtl_dev` holds a live `MTLDevice`.
            u64::from(unsafe {
                MTLDevice_heapBufferSizeAndAlign(mtl_dev.handle(), norm_desc.width, 0).size
            })
        } else {
            norm_desc.width
        };
        if req_size.saturating_add(offset) > hdesc.size_in_bytes {
            return E_INVALIDARG;
        }
    }

    if out.is_null() {
        return S_FALSE;
    }

    let mut placed_heap = ptr::null_mut();
    let buffer = if let Some(mtl_dev) = mtl {
        let buf = Arc::new(Buffer::new(norm_desc.width, mtl_dev));
        let mut flags = BufferAllocationFlags::empty();
        if placed.is_some() && is_private_heap(heap_props, super::private_heap_enabled()) {
            flags |= BufferAllocationFlags::CPU_INVISIBLE;
        }
        if let Some((heap_ptr, offset)) = placed
            && !heap_ptr.is_null()
        {
            // SAFETY: `heap_ptr` is a valid `HeapObject` pointer.
            let cpu_mem = unsafe { (*heap_ptr).cpu_memory };
            if cpu_mem.is_null() {
                // SAFETY: `heap_ptr` is a valid `HeapObject` pointer.
                let mtl_heap = unsafe { (*heap_ptr).heap.as_ref() };
                if let Some(h) = mtl_heap {
                    let _ = buf.rename(buf.allocate_on_heap(flags, h, offset));
                } else {
                    let _ = buf.rename(buf.allocate(flags));
                }
            } else {
                // SAFETY: `offset` was validated against `hdesc.size_in_bytes` above.
                let ptr = unsafe {
                    cpu_mem
                        .cast::<u8>()
                        .add(usize::try_from(offset).unwrap_or(0))
                };
                let ext = External {
                    ptr,
                    len: usize::try_from(norm_desc.width).unwrap_or(0),
                };
                // SAFETY: `ext` points into the heap's live CPU memory allocation.
                let alloc = unsafe { buf.allocate_over(flags, &ext) };
                let _ = buf.rename(alloc);
            }
            // SAFETY: `heap_ptr` is a valid `ComRefCounted` `HeapObject`.
            unsafe { com::add_ref_private_com(heap_ptr) };
            placed_heap = heap_ptr;
        } else {
            let _ = buf.rename(buf.allocate(flags));
        }
        // SAFETY: `device` is non-null when `mtl` is `Some`.
        let dev_ref = unsafe { &*device };
        crate::device::residency::register_with_va(dev_ref, &buf);
        Some(buf)
    } else {
        if let Some((heap_ptr, _)) = placed
            && !heap_ptr.is_null()
        {
            // SAFETY: `heap_ptr` is a valid `ComRefCounted` `HeapObject`.
            unsafe { com::add_ref_private_com(heap_ptr) };
            placed_heap = heap_ptr;
        }
        None
    };

    if !device.is_null() {
        // SAFETY: `device` is a valid `ComRefCounted` `Device` pointer.
        unsafe { com::add_ref_com(device) };
    }

    let resource = Box::new(Resource {
        vtbl: &BUFFER_VTBL,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device,
        texture: None,
        buffer,
        placed_heap,
        desc: norm_desc,
        heap_props: super::clone_heap_props(heap_props),
    });
    // SAFETY: `riid` and `out` are caller-supplied COM pointers.
    unsafe { super::return_resource(resource, riid, out) }
}

/// `MTLD3D12Buffer::CreateShaderResourceView` (`d3d12_buffer.cpp:166-206`).
///
/// # Safety
///
/// `desc` is null or points to a readable `D3D12_SHADER_RESOURCE_VIEW_DESC`.
pub unsafe fn create_srv(
    resource: &Resource,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("buffer.CreateShaderResourceView");
    let view_desc = if desc.is_null() {
        match super::extract_entire_srv(&resource.desc) {
            Ok(v) => v,
            Err(hr) => return hr,
        }
    } else {
        // SAFETY: `desc` is non-null and points to a `ShaderResourceViewDesc`.
        unsafe { ptr::read(desc.cast::<ShaderResourceViewDesc>()) }
    };

    if view_desc.view_dimension != D3D12_SRV_DIMENSION_BUFFER {
        return E_INVALIDARG;
    }

    // SAFETY: `view_dimension == D3D12_SRV_DIMENSION_BUFFER` selects the `buffer` union arm.
    let buf_srv = unsafe { &*view_desc.view.buffer };

    if view_desc.format == 0 || (buf_srv.flags & D3D12_BUFFER_SRV_FLAG_RAW) != 0 {
        let stride = if (buf_srv.flags & D3D12_BUFFER_SRV_FLAG_RAW) != 0 {
            4
        } else {
            buf_srv.structure_byte_stride
        };
        let slice = BufferSlice {
            byte_offset: buf_srv.first_element * u64::from(stride),
            byte_length: u64::from(buf_srv.num_elements) * u64::from(stride),
            first_element: buf_srv.first_element,
            element_count: buf_srv.num_elements,
            byte_stride: stride,
        };
        // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
        return unsafe {
            crate::descriptor_heap::write_srv_buffer(descriptor, resource.buffer.clone(), slice)
        };
    }

    let (hr, fmt) = dxmt_core::format::query_dxgi(view_desc.format);
    if hr != S_OK {
        crate::err!(
            "D3D12Buffer::CreateShaderResourceView: not an ordinary or packed format: {}",
            view_desc.format
        );
        return E_FAIL;
    }
    let view = BufferViewDescriptor {
        format: fmt.pixel_format,
        texel_size: fmt.bytes_per_texel,
    }
    .key();
    let slice = BufferSlice {
        byte_offset: u64::from(fmt.bytes_per_texel) * buf_srv.first_element,
        byte_length: u64::from(fmt.bytes_per_texel) * u64::from(buf_srv.num_elements),
        first_element: buf_srv.first_element,
        element_count: buf_srv.num_elements,
        byte_stride: fmt.bytes_per_texel,
    };
    // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
    unsafe {
        crate::descriptor_heap::write_srv_texel_buffer(
            descriptor,
            resource.buffer.clone(),
            view,
            slice,
        )
    }
}

/// `MTLD3D12Buffer::CreateUnorderedAccessView` (`d3d12_buffer.cpp:209-262`).
///
/// # Safety
///
/// `counter` is null or a valid `ID3D12Resource` pointer; `desc` is null or points to a readable
/// `D3D12_UNORDERED_ACCESS_VIEW_DESC`.
pub unsafe fn create_uav(
    resource: &Resource,
    counter: *mut c_void,
    desc: *const c_void,
    descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("buffer.CreateUnorderedAccessView");
    let view_desc = if desc.is_null() {
        match super::extract_entire_uav(&resource.desc) {
            Ok(v) => v,
            Err(hr) => return hr,
        }
    } else {
        // SAFETY: `desc` is non-null and points to an `UnorderedAccessViewDesc`.
        unsafe { ptr::read(desc.cast::<UnorderedAccessViewDesc>()) }
    };

    if view_desc.view_dimension != D3D12_UAV_DIMENSION_BUFFER {
        return E_INVALIDARG;
    }

    // SAFETY: `view_dimension == D3D12_UAV_DIMENSION_BUFFER` selects the `buffer` union arm.
    let buf_uav = unsafe { &*view_desc.view.buffer };

    if view_desc.format == 0 || (buf_uav.flags & D3D12_BUFFER_UAV_FLAG_RAW) != 0 {
        let stride = if (buf_uav.flags & D3D12_BUFFER_UAV_FLAG_RAW) != 0 {
            4
        } else {
            buf_uav.structure_byte_stride
        };
        let slice = BufferSlice {
            byte_offset: buf_uav.first_element * u64::from(stride),
            byte_length: u64::from(buf_uav.num_elements) * u64::from(stride),
            first_element: buf_uav.first_element,
            element_count: buf_uav.num_elements,
            byte_stride: stride,
        };
        // SAFETY: `counter` is null or a COM resource pointer.
        let counter_buf = unsafe { super::cast_resource(counter) }.and_then(|r| r.buffer.clone());
        let counter_offset = if counter_buf.is_some() {
            u32::try_from(buf_uav.counter_offset_in_bytes).unwrap_or(0)
        } else {
            0
        };
        // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
        return unsafe {
            crate::descriptor_heap::write_uav_buffer(
                descriptor,
                resource.buffer.clone(),
                slice,
                counter_buf,
                counter_offset,
            )
        };
    }

    let (hr, fmt) = dxmt_core::format::query_dxgi(view_desc.format);
    if hr != S_OK {
        crate::err!(
            "D3D12Buffer::CreateUnorderedAccessView: not an ordinary or packed format: {}",
            view_desc.format
        );
        return E_FAIL;
    }
    let view = BufferViewDescriptor {
        format: fmt.pixel_format,
        texel_size: fmt.bytes_per_texel,
    }
    .key();
    let slice = BufferSlice {
        byte_offset: u64::from(fmt.bytes_per_texel) * buf_uav.first_element,
        byte_length: u64::from(fmt.bytes_per_texel) * u64::from(buf_uav.num_elements),
        first_element: buf_uav.first_element,
        element_count: buf_uav.num_elements,
        byte_stride: fmt.bytes_per_texel,
    };
    let atomic_view = (fmt.pixel_format == WMT_PIXEL_FORMAT_R32_FLOAT).then(|| {
        BufferViewDescriptor {
            format: WMT_PIXEL_FORMAT_R32_UINT,
            texel_size: 4,
        }
        .key()
    });
    // SAFETY: `descriptor` is forwarded to `descriptor_heap`.
    unsafe {
        crate::descriptor_heap::write_uav_texel_buffer(
            descriptor,
            resource.buffer.clone(),
            view,
            slice,
            atomic_view,
        )
    }
}

/// `MTLD3D12Buffer::CreateRenderTargetView` (`d3d12_buffer.cpp:265-269`).
pub fn create_rtv(
    _resource: &Resource,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("buffer.CreateRenderTargetView");
    let (): () = crate::not_implemented!("Buffer::CreateRenderTargetView");
    S_OK
}

/// `MTLD3D12Buffer::CreateDepthStencilView` (`d3d12_buffer.cpp:272-276`).
pub fn create_dsv(
    _resource: &Resource,
    _desc: *const c_void,
    _descriptor: D3d12CpuDescriptorHandle,
) -> Hresult {
    crate::scope!("buffer.CreateDepthStencilView");
    let (): () = crate::not_implemented!("Buffer::CreateDepthStencilView");
    S_OK
}

/// `MTLD3D12Buffer::GetResourceTiling` (`d3d12_buffer.cpp:278-284`).
pub fn get_resource_tiling(_resource: &Resource) {
    crate::scope!("buffer.GetResourceTiling");
    let (): () = crate::not_implemented!("Buffer::GetResourceTiling");
}
