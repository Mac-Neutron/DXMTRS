use core::{cell::Cell, ffi::c_void, mem::size_of, ptr};

use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_sys::{
    Obj,
    wire::{
        MemPtr, WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
        WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
        WMT_TEXTURE_FLAG_LOSSLESS, WMT_TEXTURE_TYPE2D_MULTISAMPLE,
        WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW, WMT_TEXTURE_USAGE_RENDER_TARGET,
        WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE, WMTBufferInfo,
        WMTSizeAndAlign, WMTTextureInfo,
    },
};

use super::*;
use crate::{
    abi::{
        barrier::{
            D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_GENERIC_READ,
        },
        pipeline::SampleDesc,
        resource::{
            D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
            D3D12_HEAP_FLAG_NONE, D3D12_MEMORY_POOL_UNKNOWN, D3D12_RESOURCE_DIMENSION_BUFFER,
            D3D12_RESOURCE_DIMENSION_TEXTURE1D, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            D3D12_RESOURCE_DIMENSION_TEXTURE3D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_RESOURCE_FLAG_NONE, D3D12_TEXTURE_LAYOUT_ROW_MAJOR, D3D12_TEXTURE_LAYOUT_UNKNOWN,
            D3d12Box, HeapProperties, ResourceDesc, ResourceDesc1,
        },
        views::{
            ConstantBufferViewDesc, D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            D3D12_DSV_DIMENSION_TEXTURE1D, D3D12_DSV_DIMENSION_TEXTURE1DARRAY,
            D3D12_DSV_DIMENSION_TEXTURE2D, D3D12_DSV_DIMENSION_TEXTURE2DARRAY,
            D3D12_DSV_DIMENSION_TEXTURE2DMS, D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY,
            D3D12_DSV_FLAG_NONE, D3D12_HEAP_FLAG_DENY_BUFFERS, D3D12_RTV_DIMENSION_TEXTURE1D,
            D3D12_RTV_DIMENSION_TEXTURE1DARRAY, D3D12_RTV_DIMENSION_TEXTURE2D,
            D3D12_RTV_DIMENSION_TEXTURE2DARRAY, D3D12_RTV_DIMENSION_TEXTURE2DMS,
            D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY, D3D12_RTV_DIMENSION_TEXTURE3D,
            D3D12_SRV_DIMENSION_TEXTURE1D, D3D12_SRV_DIMENSION_TEXTURE1DARRAY,
            D3D12_SRV_DIMENSION_TEXTURE2D, D3D12_SRV_DIMENSION_TEXTURE2DARRAY,
            D3D12_SRV_DIMENSION_TEXTURE2DMS, D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY,
            D3D12_SRV_DIMENSION_TEXTURE3D, D3D12_UAV_DIMENSION_TEXTURE1D,
            D3D12_UAV_DIMENSION_TEXTURE1DARRAY, D3D12_UAV_DIMENSION_TEXTURE2D,
            D3D12_UAV_DIMENSION_TEXTURE2DARRAY, D3D12_UAV_DIMENSION_TEXTURE3D, D3d12HeapDesc,
            D3d12Range, DepthStencilViewDesc, RenderTargetViewDesc, ShaderResourceViewDesc,
            UnorderedAccessViewDesc,
        },
        vtbl::ID3D12HeapVtbl,
    },
    com::{self, E_FAIL, E_INVALIDARG, E_NOTIMPL, IID_ID3D12_RESOURCE, S_FALSE, S_OK},
    device::Device,
    heap::HeapObject,
    slot,
};

const DXGI_FORMAT_UNKNOWN: u32 = 0;
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_R32_TYPELESS: u32 = 39;
const DXGI_FORMAT_R16_TYPELESS: u32 = 53;

thread_local! {
    static FAKE_BACKING_PTR: Cell<usize> = const { Cell::new(0) };
    static LAST_HEAP_OFFSET: Cell<u64> = const { Cell::new(0) };
    static FAKE_TEX_SIZE: Cell<u32> = const { Cell::new(4096) };
}

#[unsafe(export_name = "MTLHeap_newBuffer")]
extern "C-unwind" fn fake_heap_new_buffer(
    _heap: u64,
    info: *mut WMTBufferInfo,
    offset: u64,
) -> u64 {
    LAST_HEAP_OFFSET.with(|c| c.set(offset));
    let base = FAKE_BACKING_PTR.with(Cell::get);
    if !info.is_null() {
        // SAFETY: `info` is a valid caller-supplied `WMTBufferInfo`.
        let out = unsafe { &mut *info };
        out.gpu_address = 0x4000_0000 + offset;
        #[cfg(target_pointer_width = "64")]
        {
            out.memory = MemPtr {
                ptr: if base != 0 {
                    (base as u64).wrapping_add(offset)
                } else {
                    0
                },
            };
        }
        #[cfg(target_pointer_width = "32")]
        {
            out.memory = MemPtr {
                ptr: if base != 0 {
                    u32::try_from(base).unwrap_or(0).wrapping_add(offset as u32)
                } else {
                    0
                },
                high: 0,
            };
        }
    }
    0xBEEF
}

#[unsafe(export_name = "MTLDevice_heapBufferSizeAndAlign")]
extern "C-unwind" fn fake_heap_buffer_size_and_align(
    _device: u64,
    length: u64,
    _options: u64,
) -> WMTSizeAndAlign {
    WMTSizeAndAlign {
        size: u32::try_from(length).unwrap_or(u32::MAX),
        align: 256,
    }
}

#[unsafe(export_name = "MTLDevice_heapTextureSizeAndAlign")]
extern "C-unwind" fn fake_heap_texture_size_and_align(
    _device: u64,
    _info: *mut WMTTextureInfo,
) -> WMTSizeAndAlign {
    WMTSizeAndAlign {
        size: FAKE_TEX_SIZE.with(Cell::get),
        align: 4096,
    }
}

#[repr(C)]
struct TestHeap {
    inner: HeapObject,
    desc: D3d12HeapDesc,
}

static TEST_HEAP_VTBL: ID3D12HeapVtbl = ID3D12HeapVtbl {
    query_interface: slot!(
        (_this: *mut c_void, _riid: *const c_void, _out: *mut *mut c_void) -> Hresult => S_OK
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a valid `TestHeap`.
        unsafe { com::add_ref_com(this.cast::<HeapObject>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a valid `TestHeap`.
        unsafe { com::release_com(this.cast::<HeapObject>()) }
    }),
    get_private_data: slot!(
        (_this: *mut c_void, _guid: *const c_void, _size: *mut u32, _data: *mut c_void) -> Hresult => S_OK
    ),
    set_private_data: slot!(
        (_this: *mut c_void, _guid: *const c_void, _size: u32, _data: *const c_void) -> Hresult => S_OK
    ),
    set_private_data_interface: slot!(
        (_this: *mut c_void, _guid: *const c_void, _data: *const c_void) -> Hresult => S_OK
    ),
    set_name: slot!((_this: *mut c_void, _name: *const c_void) -> Hresult => S_OK),
    get_device: slot!(
        (_this: *mut c_void, _riid: *const c_void, _out: *mut *mut c_void) -> Hresult => S_OK
    ),
    get_desc: slot!(
        (this: *mut c_void, ret: *mut c_void) -> *mut c_void => {
            // SAFETY: `this` is a valid `TestHeap`.
            let th = unsafe { &*this.cast::<TestHeap>() };
            // SAFETY: `ret` points to a caller-supplied `D3d12HeapDesc`.
            unsafe {
                ret.cast::<D3d12HeapDesc>().write(D3d12HeapDesc {
                    size_in_bytes: th.desc.size_in_bytes,
                    properties: clone_heap_props(&th.desc.properties),
                    alignment: th.desc.alignment,
                    flags: th.desc.flags,
                });
            }
            ret
        }
    ),
};

fn test_device() -> Box<Device> {
    let mut dev = Device::new_dummy();
    // SAFETY: fake non-zero `MTLDevice` handle (`NSObject_release` is a no-op in tests).
    dev.mtl = unsafe { Obj::adopt(0x1001) };
    dev
}

fn buffer_resource_desc(width: u64) -> ResourceDesc {
    ResourceDesc {
        dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
        width,
        height: 1,
        depth_or_array_size: 1,
        mip_levels: 1,
        format: DXGI_FORMAT_UNKNOWN,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        flags: D3D12_RESOURCE_FLAG_NONE,
    }
}

#[test]
fn placed_buffer_on_heap_offset() {
    let mut dev = test_device();
    let dev_ptr = &raw mut *dev;
    // SAFETY: fake non-zero `MTLHeap` handle.
    let fake_heap1 = unsafe { Obj::adopt(0x2001) }.expect("non-zero handle");
    // SAFETY: fake non-zero `MTLHeap` handle.
    let fake_heap2 = unsafe { Obj::adopt(0x2002) }.expect("non-zero handle");

    // 1. Placed buffer on an `UPLOAD` heap at offset 512 forwards `heap_offset` (`512`) to
    // `MTLHeap_newBuffer`, sets `gpu_address == 0x4000_0000 + 512`, and registers VA on the device.
    let mut backing = vec![0u8; 4096];
    FAKE_BACKING_PTR.with(|c| c.set(backing.as_mut_ptr() as usize));

    let mut cpu_heap = TestHeap {
        inner: HeapObject {
            vtbl: &TEST_HEAP_VTBL,
            com: ComObject::with_initial_ref(),
            private_data: PrivateData::default(),
            device: ptr::null_mut(),
            heap: Some(fake_heap1),
            cpu_buffer: None,
            cpu_memory: ptr::null_mut(),
        },
        desc: D3d12HeapDesc {
            size_in_bytes: 65536,
            properties: HeapProperties {
                r#type: D3D12_HEAP_TYPE_UPLOAD,
                cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
                creation_node_mask: 1,
                visible_node_mask: 1,
            },
            alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
            flags: D3D12_HEAP_FLAG_NONE,
        },
    };

    let desc = buffer_resource_desc(1024);
    let riid = IID_ID3D12_RESOURCE;
    let mut out_res: *mut c_void = ptr::null_mut();
    // SAFETY: pointers are valid local test structs.
    let hr = unsafe {
        create_placed_resource(
            dev_ptr.cast(),
            (&raw mut cpu_heap).cast(),
            512,
            (&raw const desc).cast(),
            D3D12_RESOURCE_STATE_GENERIC_READ,
            ptr::null(),
            (&raw const riid).cast(),
            &raw mut out_res,
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!out_res.is_null());
    assert_eq!(LAST_HEAP_OFFSET.with(Cell::get), 512);

    // SAFETY: `out_res` was returned by `create_placed_resource`.
    let res = unsafe { cast_resource(out_res) }.expect("cast_resource succeeds");
    // SAFETY: `out_res` is a valid `ID3D12Resource`.
    let vtbl_va = unsafe { (res.vtbl.get_gpu_virtual_address)(out_res) };
    assert_eq!(vtbl_va, 0x4000_0000 + 512);
    assert_eq!(res.gpu_va(), 0x4000_0000 + 512);
    assert_eq!(res.placed_heap, (&raw mut cpu_heap).cast::<HeapObject>());
    assert_eq!(cpu_heap.inner.com.private_count(), 2);

    // Release the placed resource and verify it releases its private hold on the heap.
    // SAFETY: `out_res` holds one COM reference from `create_placed_resource`.
    let remaining = unsafe { (res.vtbl.release)(out_res) };
    assert_eq!(remaining, 0);
    assert_eq!(cpu_heap.inner.com.private_count(), 1);

    // 2. Placed buffer on a `D3D12_HEAP_TYPE_DEFAULT` heap forwards `heap_offset` (`768`) to
    // `MTLHeap_newBuffer`, and rejects `DENY_BUFFERS` or out-of-bounds offsets.
    FAKE_BACKING_PTR.with(|c| c.set(0));
    let mut mtl_heap = TestHeap {
        inner: HeapObject {
            vtbl: &TEST_HEAP_VTBL,
            com: ComObject::with_initial_ref(),
            private_data: PrivateData::default(),
            device: ptr::null_mut(),
            heap: Some(fake_heap2),
            cpu_buffer: None,
            cpu_memory: ptr::null_mut(),
        },
        desc: D3d12HeapDesc {
            size_in_bytes: 65536,
            properties: HeapProperties {
                r#type: D3D12_HEAP_TYPE_DEFAULT,
                cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
                creation_node_mask: 1,
                visible_node_mask: 1,
            },
            alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
            flags: D3D12_HEAP_FLAG_NONE,
        },
    };
    let mut out_mtl_res: *mut c_void = ptr::null_mut();
    // SAFETY: pointers are valid local test structs.
    let hr2 = unsafe {
        create_placed_resource(
            dev_ptr.cast(),
            (&raw mut mtl_heap).cast(),
            768,
            (&raw const desc).cast(),
            D3D12_RESOURCE_STATE_COMMON,
            ptr::null(),
            (&raw const riid).cast(),
            &raw mut out_mtl_res,
        )
    };
    assert_eq!(hr2, S_OK);
    assert_eq!(LAST_HEAP_OFFSET.with(Cell::get), 768);
    // SAFETY: `out_mtl_res` was returned by `create_placed_resource`.
    let mtl_res = unsafe { cast_resource(out_mtl_res) }.expect("cast_resource succeeds");
    // SAFETY: `out_mtl_res` is a valid `ID3D12Resource`.
    let mtl_va = unsafe { (mtl_res.vtbl.get_gpu_virtual_address)(out_mtl_res) };
    assert_eq!(mtl_va, 0x4000_0000 + 768);
    // SAFETY: release the single COM reference.
    unsafe { (mtl_res.vtbl.release)(out_mtl_res) };

    // Out-of-bounds offset (65536 + 1024 > 65536) returns `E_INVALIDARG`.
    // SAFETY: pointers are valid local test structs.
    let hr_oob = unsafe {
        create_placed_resource(
            dev_ptr.cast(),
            (&raw mut mtl_heap).cast(),
            65536,
            (&raw const desc).cast(),
            D3D12_RESOURCE_STATE_COMMON,
            ptr::null(),
            (&raw const riid).cast(),
            &raw mut out_mtl_res,
        )
    };
    assert_eq!(hr_oob, E_INVALIDARG);

    // `D3D12_HEAP_FLAG_DENY_BUFFERS` returns `E_INVALIDARG`.
    mtl_heap.desc.flags = D3D12_HEAP_FLAG_DENY_BUFFERS;
    // SAFETY: pointers are valid local test structs.
    let hr_deny = unsafe {
        create_placed_resource(
            dev_ptr.cast(),
            (&raw mut mtl_heap).cast(),
            0,
            (&raw const desc).cast(),
            D3D12_RESOURCE_STATE_COMMON,
            ptr::null(),
            (&raw const riid).cast(),
            &raw mut out_mtl_res,
        )
    };
    assert_eq!(hr_deny, E_INVALIDARG);
}

#[test]
fn view_descs_as_cpp() {
    // 0. ABI sizes of view descriptors match `d3d12.h`.
    assert_eq!(size_of::<ConstantBufferViewDesc>(), 16);
    assert_eq!(size_of::<ShaderResourceViewDesc>(), 40);
    assert_eq!(size_of::<UnorderedAccessViewDesc>(), 40);
    assert_eq!(size_of::<RenderTargetViewDesc>(), 24);
    assert_eq!(size_of::<DepthStencilViewDesc>(), 24);

    // 1. Buffer resource cannot default any of SRV/UAV/RTV/DSV (`E_FAIL`).
    let buf = buffer_resource_desc(256);
    assert_eq!(extract_entire_srv(&buf).err(), Some(E_FAIL));
    assert_eq!(extract_entire_uav(&buf).err(), Some(E_FAIL));
    assert_eq!(extract_entire_rtv(&buf).err(), Some(E_FAIL));
    assert_eq!(extract_entire_dsv(&buf).err(), Some(E_FAIL));

    // 2. 1D texture (array = 1 vs array = 4).
    let mut tex1d = ResourceDesc {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE1D,
        alignment: 0,
        width: 128,
        height: 1,
        depth_or_array_size: 1,
        mip_levels: 5,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags: D3D12_RESOURCE_FLAG_NONE,
    };
    let srv1d = extract_entire_srv(&tex1d).expect("1D SRV");
    assert_eq!(srv1d.format, DXGI_FORMAT_R8G8B8A8_UNORM);
    assert_eq!(srv1d.view_dimension, D3D12_SRV_DIMENSION_TEXTURE1D);
    assert_eq!(
        srv1d.shader_4_component_mapping,
        D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING
    );
    // SAFETY: `view_dimension` is `TEXTURE1D`.
    assert_eq!(unsafe { srv1d.view.texture_1d.mip_levels }, 5);

    let uav1d = extract_entire_uav(&tex1d).expect("1D UAV");
    assert_eq!(uav1d.view_dimension, D3D12_UAV_DIMENSION_TEXTURE1D);
    // SAFETY: `view_dimension` is `TEXTURE1D`.
    assert_eq!(unsafe { uav1d.view.texture_1d.mip_slice }, 0);

    let rtv1d = extract_entire_rtv(&tex1d).expect("1D RTV");
    assert_eq!(rtv1d.view_dimension, D3D12_RTV_DIMENSION_TEXTURE1D);

    let dsv1d = extract_entire_dsv(&tex1d).expect("1D DSV");
    assert_eq!(dsv1d.view_dimension, D3D12_DSV_DIMENSION_TEXTURE1D);
    assert_eq!(dsv1d.flags, D3D12_DSV_FLAG_NONE);

    tex1d.depth_or_array_size = 4;
    let srv1da = extract_entire_srv(&tex1d).expect("1DArray SRV");
    assert_eq!(srv1da.view_dimension, D3D12_SRV_DIMENSION_TEXTURE1DARRAY);
    // SAFETY: `view_dimension` is `TEXTURE1DARRAY`.
    assert_eq!(unsafe { srv1da.view.texture_1d_array.array_size }, 4);
    let uav1da = extract_entire_uav(&tex1d).expect("1DArray UAV");
    assert_eq!(uav1da.view_dimension, D3D12_UAV_DIMENSION_TEXTURE1DARRAY);
    let rtv1da = extract_entire_rtv(&tex1d).expect("1DArray RTV");
    assert_eq!(rtv1da.view_dimension, D3D12_RTV_DIMENSION_TEXTURE1DARRAY);
    let dsv1da = extract_entire_dsv(&tex1d).expect("1DArray DSV");
    assert_eq!(dsv1da.view_dimension, D3D12_DSV_DIMENSION_TEXTURE1DARRAY);
    // SAFETY: `view_dimension` is `TEXTURE1DARRAY`.
    assert_eq!(unsafe { dsv1da.view.texture_1d_array.array_size }, 4);

    // 3. 2D texture (single, array, MSAA single, MSAA array).
    let mut tex2d = ResourceDesc {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        alignment: 0,
        width: 64,
        height: 32,
        depth_or_array_size: 1,
        mip_levels: 3,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET | D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
    };
    assert_eq!(
        extract_entire_srv(&tex2d).expect("2D SRV").view_dimension,
        D3D12_SRV_DIMENSION_TEXTURE2D
    );
    assert_eq!(
        extract_entire_uav(&tex2d).expect("2D UAV").view_dimension,
        D3D12_UAV_DIMENSION_TEXTURE2D
    );
    assert_eq!(
        extract_entire_rtv(&tex2d).expect("2D RTV").view_dimension,
        D3D12_RTV_DIMENSION_TEXTURE2D
    );
    assert_eq!(
        extract_entire_dsv(&tex2d).expect("2D DSV").view_dimension,
        D3D12_DSV_DIMENSION_TEXTURE2D
    );

    tex2d.depth_or_array_size = 6;
    let srv2da = extract_entire_srv(&tex2d).expect("2DArray SRV");
    assert_eq!(srv2da.view_dimension, D3D12_SRV_DIMENSION_TEXTURE2DARRAY);
    // SAFETY: `view_dimension` is `TEXTURE2DARRAY`.
    assert_eq!(unsafe { srv2da.view.texture_2d_array.array_size }, 6);
    assert_eq!(
        extract_entire_uav(&tex2d)
            .expect("2DArray UAV")
            .view_dimension,
        D3D12_UAV_DIMENSION_TEXTURE2DARRAY
    );
    assert_eq!(
        extract_entire_rtv(&tex2d)
            .expect("2DArray RTV")
            .view_dimension,
        D3D12_RTV_DIMENSION_TEXTURE2DARRAY
    );
    assert_eq!(
        extract_entire_dsv(&tex2d)
            .expect("2DArray DSV")
            .view_dimension,
        D3D12_DSV_DIMENSION_TEXTURE2DARRAY
    );

    tex2d.sample_desc.count = 4;
    tex2d.mip_levels = 1;
    let srv2dmsa = extract_entire_srv(&tex2d).expect("2DMSArray SRV");
    assert_eq!(
        srv2dmsa.view_dimension,
        D3D12_SRV_DIMENSION_TEXTURE2DMSARRAY
    );
    // SAFETY: `view_dimension` is `TEXTURE2DMSARRAY`.
    assert_eq!(unsafe { srv2dmsa.view.texture_2dms_array.array_size }, 6);
    assert_eq!(extract_entire_uav(&tex2d).err(), Some(E_FAIL));
    assert_eq!(
        extract_entire_rtv(&tex2d)
            .expect("2DMSArray RTV")
            .view_dimension,
        D3D12_RTV_DIMENSION_TEXTURE2DMSARRAY
    );
    assert_eq!(
        extract_entire_dsv(&tex2d)
            .expect("2DMSArray DSV")
            .view_dimension,
        D3D12_DSV_DIMENSION_TEXTURE2DMSARRAY
    );

    tex2d.depth_or_array_size = 1;
    assert_eq!(
        extract_entire_srv(&tex2d).expect("2DMS SRV").view_dimension,
        D3D12_SRV_DIMENSION_TEXTURE2DMS
    );
    assert_eq!(extract_entire_uav(&tex2d).err(), Some(E_FAIL));
    assert_eq!(
        extract_entire_rtv(&tex2d).expect("2DMS RTV").view_dimension,
        D3D12_RTV_DIMENSION_TEXTURE2DMS
    );
    assert_eq!(
        extract_entire_dsv(&tex2d).expect("2DMS DSV").view_dimension,
        D3D12_DSV_DIMENSION_TEXTURE2DMS
    );

    // 4. 3D texture: SRV/UAV/RTV succeed (`w_size == depth_or_array_size`), DSV returns `E_FAIL`.
    let tex3d = ResourceDesc {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE3D,
        alignment: 0,
        width: 16,
        height: 16,
        depth_or_array_size: 8,
        mip_levels: 2,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET | D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
    };
    assert_eq!(
        extract_entire_srv(&tex3d).expect("3D SRV").view_dimension,
        D3D12_SRV_DIMENSION_TEXTURE3D
    );
    let uav3d = extract_entire_uav(&tex3d).expect("3D UAV");
    assert_eq!(uav3d.view_dimension, D3D12_UAV_DIMENSION_TEXTURE3D);
    // SAFETY: `view_dimension` is `TEXTURE3D`.
    assert_eq!(unsafe { uav3d.view.texture_3d.w_size }, 8);
    let rtv3d = extract_entire_rtv(&tex3d).expect("3D RTV");
    assert_eq!(rtv3d.view_dimension, D3D12_RTV_DIMENSION_TEXTURE3D);
    // SAFETY: `view_dimension` is `TEXTURE3D`.
    assert_eq!(unsafe { rtv3d.view.texture_3d.w_size }, 8);
    assert_eq!(extract_entire_dsv(&tex3d).err(), Some(E_FAIL));

    // 5. `populate_wmt_texture_info` matches `d3d12_texture.cpp:28-181`.
    let dev = test_device();
    let mut info = WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: 0,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let msaa_rt = ResourceDesc {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        alignment: 0,
        width: 1920,
        height: 1080,
        depth_or_array_size: 1,
        mip_levels: 1,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_desc: SampleDesc {
            count: 4,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags: D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET
            | D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL
            | D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
    };
    assert_eq!(populate_wmt_texture_info(&dev, &msaa_rt, &mut info), S_OK);
    assert_eq!(info.r#type(), WMT_TEXTURE_TYPE2D_MULTISAMPLE);
    assert_eq!(info.sample_count(), 4);
    assert_eq!(info.mipmap_level_count(), 1);
    assert_eq!(info.options, WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED);
    assert_eq!(
        info.usage(),
        WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW
            | WMT_TEXTURE_USAGE_SHADER_READ
            | WMT_TEXTURE_USAGE_SHADER_WRITE
            | WMT_TEXTURE_USAGE_RENDER_TARGET
    );
    // 4-byte-per-texel `RGBA8Unorm` with `ALLOW_UNORDERED_ACCESS` disables lossless compression (`r32_view && uav`).
    assert_eq!(info.flags, 0);
    let mut rt_no_uav = clone_desc(&msaa_rt);
    rt_no_uav.flags = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
    assert_eq!(populate_wmt_texture_info(&dev, &rt_no_uav, &mut info), S_OK);
    assert_eq!(info.flags, WMT_TEXTURE_FLAG_LOSSLESS);

    // Depth-stencil format promotion (`R32_TYPELESS` -> `Depth32Float`, `R16_TYPELESS` -> `Depth16Unorm`).
    let mut ds32 = clone_desc(&msaa_rt);
    ds32.format = DXGI_FORMAT_R32_TYPELESS;
    ds32.flags = D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;
    ds32.sample_desc.count = 1;
    assert_eq!(populate_wmt_texture_info(&dev, &ds32, &mut info), S_OK);
    assert_eq!(info.pixel_format, WMT_PIXEL_FORMAT_DEPTH32_FLOAT);

    let mut ds16 = clone_desc(&ds32);
    ds16.format = DXGI_FORMAT_R16_TYPELESS;
    assert_eq!(populate_wmt_texture_info(&dev, &ds16, &mut info), S_OK);
    assert_eq!(info.pixel_format, WMT_PIXEL_FORMAT_DEPTH16_UNORM);

    // 4 KiB small alignment succeeds when single-slice mip 0 fits within 64 KiB, and fails when oversized.
    let mut small_tex = clone_desc(&tex2d);
    small_tex.sample_desc.count = 1;
    small_tex.flags = D3D12_RESOURCE_FLAG_NONE;
    small_tex.alignment = D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT;
    FAKE_TEX_SIZE.with(|c| c.set(16384));
    assert_eq!(populate_wmt_texture_info(&dev, &small_tex, &mut info), S_OK);
    assert_eq!(info.flags, 0);
    let mut alloc_info = crate::abi::resource::ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 0,
    };
    // SAFETY: `dev` and `small_tex` are valid stack/box references.
    unsafe {
        crate::device::features::get_resource_allocation_info(
            ptr::from_ref::<Device>(&*dev).cast_mut().cast::<c_void>(),
            (&raw mut alloc_info).cast::<c_void>(),
            0,
            1,
            (&raw const small_tex).cast::<c_void>(),
        );
    }
    assert_eq!(alloc_info.size_in_bytes, 16384);
    assert_eq!(
        alloc_info.alignment,
        D3D12_SMALL_RESOURCE_PLACEMENT_ALIGNMENT
    );
    FAKE_TEX_SIZE.with(|c| c.set(131_072));
    assert_eq!(
        populate_wmt_texture_info(&dev, &small_tex, &mut info),
        E_INVALIDARG
    );
    // SAFETY: `dev` and `small_tex` are valid stack/box references.
    unsafe {
        crate::device::features::get_resource_allocation_info(
            ptr::from_ref::<Device>(&*dev).cast_mut().cast::<c_void>(),
            (&raw mut alloc_info).cast::<c_void>(),
            0,
            1,
            (&raw const small_tex).cast::<c_void>(),
        );
    }
    assert_eq!(alloc_info.size_in_bytes, u64::MAX);

    // `encode_component_mapping` preserves identity swizzle for `D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING`.
    assert_eq!(
        texture::encode_component_mapping(
            D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            WMT_PIXEL_FORMAT_RGBA8_UNORM
        ),
        WMT_PIXEL_FORMAT_RGBA8_UNORM
    );
}

#[test]
fn map_returns_buffer_pointer() {
    let mut dev = test_device();
    let dev_ptr = &raw mut *dev;
    // SAFETY: fake non-zero `MTLHeap` handle.
    let fake_heap = unsafe { Obj::adopt(0x3003) }.expect("non-zero handle");

    let mut backing = vec![0xA5u8; 2048];
    let base_ptr = backing.as_mut_ptr();
    FAKE_BACKING_PTR.with(|c| c.set(base_ptr as usize));

    let mut cpu_heap = TestHeap {
        inner: HeapObject {
            vtbl: &TEST_HEAP_VTBL,
            com: ComObject::with_initial_ref(),
            private_data: PrivateData::default(),
            device: ptr::null_mut(),
            heap: Some(fake_heap),
            cpu_buffer: None,
            cpu_memory: ptr::null_mut(),
        },
        desc: D3d12HeapDesc {
            size_in_bytes: 65536,
            properties: HeapProperties {
                r#type: D3D12_HEAP_TYPE_UPLOAD,
                cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
                memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
                creation_node_mask: 1,
                visible_node_mask: 1,
            },
            alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
            flags: D3D12_HEAP_FLAG_NONE,
        },
    };

    let desc = buffer_resource_desc(512);
    let riid = IID_ID3D12_RESOURCE;
    let mut out_res: *mut c_void = ptr::null_mut();
    // SAFETY: pointers are valid local test structs.
    let hr = unsafe {
        create_placed_resource(
            dev_ptr.cast(),
            (&raw mut cpu_heap).cast(),
            256,
            (&raw const desc).cast(),
            D3D12_RESOURCE_STATE_GENERIC_READ,
            ptr::null(),
            (&raw const riid).cast(),
            &raw mut out_res,
        )
    };
    assert_eq!(hr, S_OK);
    // SAFETY: `out_res` is a valid `Resource` pointer.
    let res = unsafe { cast_resource(out_res) }.expect("cast_resource succeeds");
    let map_fn = res.vtbl.map;
    let unmap_fn = res.vtbl.unmap;
    let write_sub_fn = res.vtbl.write_to_subresource;
    let read_sub_fn = res.vtbl.read_from_subresource;

    // 1. Subresource != 0 returns `E_INVALIDARG`.
    let mut mapped: *mut c_void = ptr::null_mut();
    // SAFETY: `out_res` and `mapped` are valid.
    let hr_sub1 = unsafe { map_fn(out_res, 1, ptr::null(), &raw mut mapped) };
    assert_eq!(hr_sub1, E_INVALIDARG);

    // 2. Null `ppData` returns `S_OK` (`d3d12_buffer.cpp:116-118`).
    let range = D3d12Range { begin: 0, end: 64 };
    // SAFETY: `out_res` and `range` are valid.
    let hr_null_data = unsafe { map_fn(out_res, 0, (&raw const range).cast(), ptr::null_mut()) };
    assert_eq!(hr_null_data, S_OK);

    // 3. Valid `ppData` writes `base_ptr + 256` and returns `S_OK`.
    // SAFETY: `out_res` and `mapped` are valid.
    let hr_map = unsafe { map_fn(out_res, 0, ptr::null(), &raw mut mapped) };
    assert_eq!(hr_map, S_OK);
    // SAFETY: `base_ptr` has 2048 bytes, so `+ 256` is in bounds.
    let expected_ptr = unsafe { base_ptr.add(256).cast::<c_void>() };
    assert_eq!(mapped, expected_ptr);

    // 4. `Unmap` is a no-op and `WriteToSubresource` / `ReadFromSubresource` on buffers return `E_INVALIDARG`.
    // SAFETY: `out_res` is valid.
    unsafe { unmap_fn(out_res, 0, ptr::null()) };
    // SAFETY: `out_res` is valid.
    let hr_write_sub = unsafe { write_sub_fn(out_res, 0, ptr::null(), ptr::null(), 0, 0) };
    assert_eq!(hr_write_sub, E_INVALIDARG);
    // SAFETY: `out_res` is valid.
    let hr_read_sub = unsafe { read_sub_fn(out_res, ptr::null_mut(), 0, 0, 0, ptr::null()) };
    assert_eq!(hr_read_sub, E_INVALIDARG);

    // 5. `GetHeapProperties` returns `S_OK` when both outputs are null, and copies properties otherwise.
    let get_hp_fn = res.vtbl.get_heap_properties;
    // SAFETY: `out_res` is valid.
    let hr_hp_null = unsafe { get_hp_fn(out_res, ptr::null_mut(), ptr::null_mut()) };
    assert_eq!(hr_hp_null, S_OK);
    let mut heap_props_out = HeapProperties {
        r#type: 0,
        cpu_page_property: 0,
        memory_pool_preference: 0,
        creation_node_mask: 0,
        visible_node_mask: 0,
    };
    let mut heap_flags_out = 99u32;
    // SAFETY: `out_res`, `heap_props_out`, and `heap_flags_out` are valid.
    let hr_hp = unsafe {
        get_hp_fn(
            out_res,
            (&raw mut heap_props_out).cast(),
            &raw mut heap_flags_out,
        )
    };
    assert_eq!(hr_hp, S_OK);
    assert_eq!(heap_props_out.r#type, D3D12_HEAP_TYPE_UPLOAD);
    assert_eq!(heap_flags_out, D3D12_HEAP_FLAG_NONE);

    // 6. `GetDesc1` copies the 512-byte buffer description and zeros the mip region.
    let mut d1 = ResourceDesc1 {
        dimension: 0,
        alignment: 0,
        width: 0,
        height: 0,
        depth_or_array_size: 0,
        mip_levels: 0,
        format: 0,
        sample_desc: SampleDesc {
            count: 0,
            quality: 0,
        },
        layout: 0,
        flags: 0,
        sampler_feedback_mip_region: crate::abi::resource::MipRegion {
            width: 9,
            height: 9,
            depth: 9,
        },
    };
    // SAFETY: `out_res` and `d1` are valid.
    let _ = unsafe { (res.vtbl.get_desc1)(out_res, (&raw mut d1).cast()) };
    assert_eq!(d1.width, 512);
    assert_eq!(d1.sampler_feedback_mip_region.width, 0);

    // 7. `GetProtectedResourceSession` zeros `session` and returns `E_NOTIMPL`.
    let mut session: *mut c_void = ptr::dangling_mut();
    // SAFETY: `out_res` and `session` are valid.
    let hr_session = unsafe {
        (res.vtbl.get_protected_resource_session)(out_res, ptr::null(), &raw mut session)
    };
    assert_eq!(hr_session, E_NOTIMPL);
    assert!(session.is_null());

    // SAFETY: release the single COM reference.
    unsafe { (res.vtbl.release)(out_res) };
}

#[test]
fn texture_create_and_subresource_validation() {
    let mut dev = Device::new_dummy();
    let dev_ptr = &raw mut *dev;
    let riid = IID_ID3D12_RESOURCE;

    let default_hp = HeapProperties {
        r#type: D3D12_HEAP_TYPE_DEFAULT,
        cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
        creation_node_mask: 1,
        visible_node_mask: 1,
    };
    let tex_desc = ResourceDesc {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        alignment: 0,
        width: 32,
        height: 16,
        depth_or_array_size: 2,
        mip_levels: 0,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags: D3D12_RESOURCE_FLAG_NONE,
    };

    // Null `ppvResource` returns `S_FALSE` after validating description.
    // SAFETY: pointers are valid local test structs.
    let hr_null = unsafe {
        texture::create(
            dev_ptr,
            (&default_hp, D3D12_HEAP_FLAG_NONE),
            &tex_desc,
            ptr::null(),
            None,
            &[],
            ((&raw const riid).cast(), ptr::null_mut()),
        )
    };
    assert_eq!(hr_null, S_FALSE);

    let mut out_tex: *mut c_void = ptr::null_mut();
    // SAFETY: pointers are valid local test structs.
    let hr = unsafe {
        texture::create(
            dev_ptr,
            (&default_hp, D3D12_HEAP_FLAG_NONE),
            &tex_desc,
            ptr::null(),
            None,
            &[],
            ((&raw const riid).cast(), &raw mut out_tex),
        )
    };
    assert_eq!(hr, S_OK);
    // SAFETY: `out_tex` was returned by `create_committed_resource`.
    let tex_res = unsafe { cast_resource(out_tex) }.expect("cast_resource succeeds");
    // `mip_levels == 0` is normalized to 6 (`32 -> 16 -> 8 -> 4 -> 2 -> 1`).
    assert_eq!(tex_res.desc.mip_levels, 6);
    assert_eq!(
        tex_res.desc.alignment,
        D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
    );
    // SAFETY: `out_tex` is a valid `ID3D12Resource`.
    let tex_va = unsafe { (tex_res.vtbl.get_gpu_virtual_address)(out_tex) };
    assert_eq!(tex_va, 0);
    // `Map` and `WriteToSubresource` on a `DEFAULT` (non-CPU-visible) heap return `E_INVALIDARG`.
    // SAFETY: `out_tex` is valid.
    let hr_tex_map = unsafe { (tex_res.vtbl.map)(out_tex, 0, ptr::null(), ptr::null_mut()) };
    assert_eq!(hr_tex_map, E_INVALIDARG);
    // SAFETY: `out_tex` is valid.
    let hr_tex_write =
        unsafe { (tex_res.vtbl.write_to_subresource)(out_tex, 0, ptr::null(), ptr::null(), 0, 0) };
    assert_eq!(hr_tex_write, E_INVALIDARG);
    // SAFETY: release the single COM reference.
    unsafe { (tex_res.vtbl.release)(out_tex) };

    // Create a texture on a CPU-visible `CUSTOM` heap (`texture::create` directly) and validate
    // `Map`, `WriteToSubresource`, and `ReadFromSubresource`.
    let custom_hp = HeapProperties {
        r#type: 4,                 // D3D12_HEAP_TYPE_CUSTOM
        cpu_page_property: 2,      // WRITE_COMBINE
        memory_pool_preference: 1, // L0
        creation_node_mask: 1,
        visible_node_mask: 1,
    };
    let mut cpu_tex: *mut c_void = ptr::null_mut();
    // SAFETY: pointers are valid local test structs.
    let hr_cpu = unsafe {
        texture::create(
            dev_ptr,
            (&custom_hp, D3D12_HEAP_FLAG_NONE),
            &tex_desc,
            ptr::null(),
            None,
            &[],
            ((&raw const riid).cast(), &raw mut cpu_tex),
        )
    };
    assert_eq!(hr_cpu, S_OK);
    // SAFETY: `cpu_tex` is a valid `Resource` pointer.
    let ct = unsafe { cast_resource(cpu_tex) }.expect("cast_resource succeeds");
    // `texture.Map` succeeds when `data` is null and `subresource` is in bounds (`0..12`), and
    // returns `E_INVALIDARG` when `data` is non-null or `subresource >= 12`.
    // SAFETY: `cpu_tex` is valid.
    let hr_ct_map0 = unsafe { (ct.vtbl.map)(cpu_tex, 0, ptr::null(), ptr::null_mut()) };
    assert_eq!(hr_ct_map0, S_OK);
    // SAFETY: `cpu_tex` is valid.
    let hr_ct_map12 = unsafe { (ct.vtbl.map)(cpu_tex, 12, ptr::null(), ptr::null_mut()) };
    assert_eq!(hr_ct_map12, E_INVALIDARG);
    let mut non_null_data: *mut c_void = ptr::null_mut();
    // SAFETY: `cpu_tex` and `non_null_data` are valid.
    let hr_ct_map_nonnull =
        unsafe { (ct.vtbl.map)(cpu_tex, 0, ptr::null(), &raw mut non_null_data) };
    assert_eq!(hr_ct_map_nonnull, E_INVALIDARG);

    // `WriteToSubresource` / `ReadFromSubresource` validate bounds and return `S_OK` for an empty box.
    let empty_box = D3d12Box {
        left: 4,
        top: 0,
        front: 0,
        right: 4,
        bottom: 4,
        back: 1,
    };
    // SAFETY: `cpu_tex` and `empty_box` are valid.
    let hr_empty_box = unsafe {
        (ct.vtbl.write_to_subresource)(
            cpu_tex,
            0,
            (&raw const empty_box).cast(),
            ptr::null(),
            128,
            0,
        )
    };
    assert_eq!(hr_empty_box, S_OK);
    let oob_box = D3d12Box {
        left: 0,
        top: 0,
        front: 0,
        right: 64,
        bottom: 16,
        back: 1,
    };
    // SAFETY: `cpu_tex` and `oob_box` are valid.
    let hr_oob_box = unsafe {
        (ct.vtbl.read_from_subresource)(
            cpu_tex,
            ptr::null_mut(),
            128,
            0,
            0,
            (&raw const oob_box).cast(),
        )
    };
    assert_eq!(hr_oob_box, E_INVALIDARG);
    // SAFETY: release the single COM reference.
    unsafe { (ct.vtbl.release)(cpu_tex) };
}
