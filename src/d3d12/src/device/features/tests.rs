use core::ptr;

use dxmt_util::dxgi_consts::{
    DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R16_UINT,
    DXGI_FORMAT_R32_UINT, DXGI_FORMAT_UNKNOWN,
};
use winemetal_sys::Obj;
use winemetal_wire::{WMT_TEXTURE_FLAG_LOSSLESS, WMTTextureInfo};

use super::*;
use crate::abi::{
    pipeline::SampleDesc,
    resource::{
        D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_RESOURCE_DIMENSION_BUFFER,
        D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
        D3D12_RESOURCE_FLAG_NONE, D3D12_TEXTURE_LAYOUT_ROW_MAJOR, D3D12_TEXTURE_LAYOUT_UNKNOWN,
        MipRegion, ResourceAllocationInfo, ResourceAllocationInfo1, ResourceDesc1,
    },
};

fn tex_desc1(format: u32, flags: u32) -> ResourceDesc1 {
    ResourceDesc1 {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        alignment: 0,
        width: 128,
        height: 64,
        depth_or_array_size: 1,
        mip_levels: 1,
        format,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags,
        sampler_feedback_mip_region: MipRegion {
            width: 0,
            height: 0,
            depth: 0,
        },
    }
}

fn buf_desc1(width: u64) -> ResourceDesc1 {
    ResourceDesc1 {
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
        sampler_feedback_mip_region: MipRegion {
            width: 0,
            height: 0,
            depth: 0,
        },
    }
}

#[test]
fn get_resource_allocation_info3_validates_and_sizes_castable_formats() {
    let mut dev = Device::new_dummy();
    // SAFETY: fake non-zero `MTLDevice` handle (`NSObject_release` is a no-op in tests).
    dev.mtl = unsafe { Obj::adopt(0x1001) };
    let dev_ptr: *mut Device = &raw mut *dev;

    let descs = [
        tex_desc1(
            DXGI_FORMAT_R8G8B8A8_UNORM,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
        ),
        buf_desc1(1024),
    ];
    let cast0 = [DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R32_UINT];
    let counts = [2u32, 0u32];
    let lists = [cast0.as_ptr(), ptr::null()];
    let mut total = ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 0,
    };
    let mut infos1 = [
        ResourceAllocationInfo1 {
            offset: 0,
            alignment: 0,
            size_in_bytes: 0,
        },
        ResourceAllocationInfo1 {
            offset: 0,
            alignment: 0,
            size_in_bytes: 0,
        },
    ];

    // SAFETY: `dev_ptr`, `total`, `descs`, `counts`, `lists`, and `infos1` are valid.
    let ret = unsafe {
        get_resource_allocation_info3(
            dev_ptr.cast(),
            (&raw mut total).cast(),
            0,
            2,
            descs.as_ptr().cast(),
            counts.as_ptr(),
            lists.as_ptr(),
            infos1.as_mut_ptr().cast(),
        )
    };
    assert_eq!(ret, (&raw mut total).cast());
    assert_ne!(total.size_in_bytes, u64::MAX);
    assert_eq!(total.alignment, D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT);
    assert_eq!(infos1[0].offset, 0);
    assert_eq!(infos1[1].offset, D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT);

    // Null `pNumCastableFormats` succeeds for textures and buffers.
    let mut total_null_counts = ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 0,
    };
    // SAFETY: `dev_ptr`, `total_null_counts`, and `descs` are valid.
    unsafe {
        get_resource_allocation_info3(
            dev_ptr.cast(),
            (&raw mut total_null_counts).cast(),
            0,
            2,
            descs.as_ptr().cast(),
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
        );
    }
    assert_eq!(total_null_counts.size_in_bytes, total.size_in_bytes);

    // Non-zero count with null `ppCastableFormats` fails sizing (`u64::MAX`).
    let mut total_null_lists = ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 0,
    };
    // SAFETY: `dev_ptr`, `total_null_lists`, `descs`, and `counts` are valid.
    unsafe {
        get_resource_allocation_info3(
            dev_ptr.cast(),
            (&raw mut total_null_lists).cast(),
            0,
            2,
            descs.as_ptr().cast(),
            counts.as_ptr(),
            ptr::null(),
            ptr::null_mut(),
        );
    }
    assert_eq!(total_null_lists.size_in_bytes, u64::MAX);

    // Mismatched texel byte size in castable formats fails sizing (`u64::MAX`).
    let bad_cast = [DXGI_FORMAT_R16_UINT];
    let bad_counts = [1u32];
    let bad_lists = [bad_cast.as_ptr()];
    let mut total_bad = ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 0,
    };
    // SAFETY: `dev_ptr`, `total_bad`, `descs`, `bad_counts`, and `bad_lists` are valid.
    unsafe {
        get_resource_allocation_info3(
            dev_ptr.cast(),
            (&raw mut total_bad).cast(),
            0,
            1,
            descs.as_ptr().cast(),
            bad_counts.as_ptr(),
            bad_lists.as_ptr(),
            ptr::null_mut(),
        );
    }
    assert_eq!(total_bad.size_in_bytes, u64::MAX);

    // Buffer with non-zero castable formats fails sizing (`u64::MAX`).
    let buf_only = [buf_desc1(1024)];
    let buf_cast = [DXGI_FORMAT_R32_UINT];
    let buf_counts = [1u32];
    let buf_lists = [buf_cast.as_ptr()];
    let mut total_buf_bad = ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 0,
    };
    // SAFETY: `dev_ptr`, `total_buf_bad`, `buf_only`, `buf_counts`, and `buf_lists` are valid.
    unsafe {
        get_resource_allocation_info3(
            dev_ptr.cast(),
            (&raw mut total_buf_bad).cast(),
            0,
            1,
            buf_only.as_ptr().cast(),
            buf_counts.as_ptr(),
            buf_lists.as_ptr(),
            ptr::null_mut(),
        );
    }
    assert_eq!(total_buf_bad.size_in_bytes, u64::MAX);

    // Verify lossless suppression matches `CreatePlacedResource2` for the same descriptor.
    let plain = plain_desc(&descs[0]);
    let mut info_cross = WMTTextureInfo {
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
    // SAFETY: `dev_ptr` is a valid `Device` pointer.
    let hr = unsafe {
        crate::resource::populate_texture_info_opt(dev_ptr, &plain, &cast0, &mut info_cross)
    };
    assert_eq!(hr, S_OK);
    assert_eq!(info_cross.flags & WMT_TEXTURE_FLAG_LOSSLESS, 0);
}
