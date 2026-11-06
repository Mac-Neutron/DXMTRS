//! Feature queries, allocation info, heap properties, and footprints (`d3d12_device.cpp`).

use core::ffi::c_void;
use std::sync::LazyLock;

use dxmt_core::settings::{Flags, settings};
use winemetal_sys::{MTLDevice_heapBufferSizeAndAlign, MTLDevice_heapTextureSizeAndAlign, Obj};
use winemetal_wire::WMTTextureInfo;

use crate::{
    abi::resource::{
        D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE, D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        D3D12_CPU_PAGE_PROPERTY_WRITE_BACK, D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE,
        D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_MEMORY_POOL_L0, D3D12_MEMORY_POOL_L1,
        D3D12_MEMORY_POOL_UNKNOWN, D3D12_RESOURCE_DIMENSION_BUFFER,
        D3D12_RESOURCE_DIMENSION_TEXTURE1D, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        D3D12_RESOURCE_DIMENSION_TEXTURE3D, HeapProperties, PlacedSubresourceFootprint,
        ResourceAllocationInfo, ResourceAllocationInfo1, ResourceDesc, ResourceDesc1,
    },
    com::{E_INVALIDARG, E_NOTIMPL, Hresult, S_OK},
    device::Device,
    resource_math::{self, HeapSizeAndAlign, plain_desc},
};

const D3D12_HEAP_TYPE_CUSTOM: u32 = 4;

impl crate::feature::Device for Device {
    fn sm6_caps(&self) -> bool {
        static SM6: LazyLock<bool> = LazyLock::new(|| {
            #[cfg(not(feature = "perf-build"))]
            if dxmt_core::dump::on() {
                return true;
            }
            settings().enabled(Flags::SM6)
        });
        *SM6
    }

    fn advertise_numa(&self) -> bool {
        self.advertise_numa
    }

    fn supports_texture_sample_count(&self, count: u32) -> bool {
        let device = self.mtl.as_ref().map_or(0, Obj::handle);
        // SAFETY: `device` is the null handle or the live `MTLDevice` `self` holds.
        unsafe { dxmt_core::format::supports_sample_count(device, count) }
    }
}

/// `ID3D12Device::CheckFeatureSupport` (`d3d12_device.cpp:544`).
///
/// # Safety
///
/// `this` and `data` are caller-supplied pointers.
pub unsafe fn check_feature_support(
    this: *mut c_void,
    feature: u32,
    data: *mut c_void,
    size: u32,
) -> Hresult {
    crate::scope!("device.CheckFeatureSupport");
    if this.is_null() || data.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a non-null `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    // SAFETY: `data` points to `size` writable bytes supplied by the COM caller.
    let buf = unsafe { core::slice::from_raw_parts_mut(data.cast::<u8>(), size as usize) };
    // SAFETY: `buf` is the caller's feature data buffer.
    let hr = unsafe { crate::feature::check_feature_support(dev, feature, buf) };
    if hr == E_NOTIMPL {
        crate::err!(
            "CheckFeatureSupport: unhandled feature {}",
            feature.cast_signed()
        );
    }
    hr
}

unsafe fn compute_allocation_info(
    device: &Device,
    descs: &[ResourceDesc],
    info1_out: *mut c_void,
) -> ResourceAllocationInfo {
    if descs.is_empty() {
        return ResourceAllocationInfo {
            size_in_bytes: u64::MAX,
            alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
        };
    }
    let infos = resource_math::allocation_info(descs, |d| match d.dimension {
        D3D12_RESOURCE_DIMENSION_BUFFER => {
            if d.alignment != 0 && d.alignment != D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT {
                crate::debug!(
                    "GetResourceAllocationInfo: invalid alignment {} for buffer resource.\n",
                    d.alignment
                );
                return None;
            }
            device.mtl.as_ref().map_or_else(
                || {
                    Some(HeapSizeAndAlign {
                        size: u32::try_from(d.width).unwrap_or(u32::MAX),
                        align: 256,
                    })
                },
                |mtl| {
                    let h = mtl.handle();
                    // SAFETY: `h` is a live `MTLDevice` handle.
                    let sa = unsafe { MTLDevice_heapBufferSizeAndAlign(h, d.width, 0) };
                    Some(HeapSizeAndAlign {
                        size: sa.size,
                        align: sa.align,
                    })
                },
            )
        }
        D3D12_RESOURCE_DIMENSION_TEXTURE1D
        | D3D12_RESOURCE_DIMENSION_TEXTURE2D
        | D3D12_RESOURCE_DIMENSION_TEXTURE3D => {
            let mut tex_info = WMTTextureInfo {
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
            if crate::resource::populate_wmt_texture_info(device, d, &mut tex_info) != S_OK {
                crate::debug!("GetResourceAllocationInfo: invalid texture descriptor\n");
                return None;
            }
            device.mtl.as_ref().map_or(
                Some(HeapSizeAndAlign {
                    size: 4096,
                    align: 4096,
                }),
                |mtl| {
                    let h = mtl.handle();
                    // SAFETY: `h` is a live `MTLDevice` handle and `tex_info` is initialized.
                    let sa = unsafe { MTLDevice_heapTextureSizeAndAlign(h, &raw mut tex_info) };
                    Some(HeapSizeAndAlign {
                        size: sa.size,
                        align: sa.align,
                    })
                },
            )
        }
        _ => None,
    });
    if !info1_out.is_null() {
        let out_ptr = info1_out.cast::<ResourceAllocationInfo1>();
        for (i, entry) in infos.each.iter().enumerate() {
            let item = ResourceAllocationInfo1 {
                offset: entry.offset,
                alignment: entry.alignment,
                size_in_bytes: entry.size_in_bytes,
            };
            // SAFETY: `i` is within the caller's `descs.len()` array.
            let dst = unsafe { out_ptr.add(i) };
            // SAFETY: `dst` points to a writable `ResourceAllocationInfo1`.
            unsafe { dst.write_unaligned(item) };
        }
    }
    infos.total
}

/// `ID3D12Device::GetResourceAllocationInfo` (`d3d12_device.cpp:956`).
///
/// # Safety
///
/// `this`, `ret`, and `descs` are caller-supplied pointers.
pub unsafe fn get_resource_allocation_info(
    this: *mut c_void,
    ret: *mut c_void,
    _visible_mask: u32,
    num_descs: u32,
    descs: *const c_void,
) -> *mut c_void {
    crate::scope!("device.GetResourceAllocationInfo");
    let result = if this.is_null() || num_descs == 0 || descs.is_null() {
        ResourceAllocationInfo {
            size_in_bytes: u64::MAX,
            alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
        }
    } else {
        // SAFETY: `this` is a non-null `Device` pointer.
        let dev = unsafe { &*this.cast::<Device>() };
        let count = num_descs as usize;
        let mut plain = Vec::with_capacity(count);
        let ptr = descs.cast::<ResourceDesc>();
        for i in 0..count {
            // SAFETY: `i` is within `count` elements.
            let elem = unsafe { ptr.add(i) };
            // SAFETY: `elem` points to a readable `ResourceDesc`.
            plain.push(unsafe { elem.read_unaligned() });
        }
        // SAFETY: `plain` is valid and `info1_out` is null.
        unsafe { compute_allocation_info(dev, &plain, core::ptr::null_mut()) }
    };
    if !ret.is_null() {
        // SAFETY: `ret` is a valid caller-provided sret pointer.
        unsafe { ret.cast::<ResourceAllocationInfo>().write_unaligned(result) };
    }
    ret
}

/// `ID3D12Device::GetCustomHeapProperties` (`d3d12_device.cpp:964`).
///
/// # Safety
///
/// `this` and `ret` are caller-supplied pointers.
pub unsafe fn get_custom_heap_properties(
    this: *mut c_void,
    ret: *mut c_void,
    _node_mask: u32,
    heap_type: u32,
) -> *mut c_void {
    crate::scope!("device.GetCustomHeapProperties");
    let advertise_numa = if this.is_null() {
        false
    } else {
        // SAFETY: `this` is a non-null `Device` pointer.
        unsafe { (*this.cast::<Device>()).advertise_numa }
    };
    let props = match heap_type {
        1 => HeapProperties {
            r#type: D3D12_HEAP_TYPE_CUSTOM,
            cpu_page_property: D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE,
            memory_pool_preference: if advertise_numa {
                D3D12_MEMORY_POOL_L1
            } else {
                D3D12_MEMORY_POOL_L0
            },
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
        2 | 5 => HeapProperties {
            r#type: D3D12_HEAP_TYPE_CUSTOM,
            cpu_page_property: D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE,
            memory_pool_preference: D3D12_MEMORY_POOL_L0,
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
        3 => HeapProperties {
            r#type: D3D12_HEAP_TYPE_CUSTOM,
            cpu_page_property: D3D12_CPU_PAGE_PROPERTY_WRITE_BACK,
            memory_pool_preference: D3D12_MEMORY_POOL_L0,
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
        _ => HeapProperties {
            r#type: heap_type,
            cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
            creation_node_mask: 1,
            visible_node_mask: 1,
        },
    };
    if !ret.is_null() {
        // SAFETY: `ret` is a valid caller-provided sret pointer.
        unsafe { ret.cast::<HeapProperties>().write_unaligned(props) };
    }
    ret
}

struct FootprintOutputs {
    layouts: *mut c_void,
    num_rows: *mut u32,
    row_sizes: *mut u64,
    total_bytes: *mut u64,
}

unsafe fn copyable_footprints_inner(
    desc: Option<&ResourceDesc>,
    first_sub: u32,
    num_sub: u32,
    base_offset: u64,
    out: &FootprintOutputs,
) {
    let total =
        resource_math::copyable_footprints(desc, first_sub, num_sub, base_offset, |index, fp| {
            let idx = index as usize;
            if !out.layouts.is_null() {
                let layout = PlacedSubresourceFootprint {
                    offset: fp.layout.offset,
                    footprint: crate::abi::resource::SubresourceFootprint {
                        format: fp.layout.footprint.format,
                        width: fp.layout.footprint.width,
                        height: fp.layout.footprint.height,
                        depth: fp.layout.footprint.depth,
                        row_pitch: fp.layout.footprint.row_pitch,
                    },
                };
                let base = out.layouts.cast::<PlacedSubresourceFootprint>();
                // SAFETY: `idx` is within `num_sub` elements.
                let dst = unsafe { base.add(idx) };
                // SAFETY: `dst` points to a writable `PlacedSubresourceFootprint`.
                unsafe { dst.write_unaligned(layout) };
            }
            if !out.num_rows.is_null() {
                // SAFETY: `idx` is within `num_sub` elements.
                let dst = unsafe { out.num_rows.add(idx) };
                // SAFETY: `dst` points to a writable `u32`.
                unsafe { dst.write_unaligned(fp.num_rows) };
            }
            if !out.row_sizes.is_null() {
                // SAFETY: `idx` is within `num_sub` elements.
                let dst = unsafe { out.row_sizes.add(idx) };
                // SAFETY: `dst` points to a writable `u64`.
                unsafe { dst.write_unaligned(fp.row_size) };
            }
        });
    if !out.total_bytes.is_null() {
        // SAFETY: `out.total_bytes` is a non-null writable `u64` pointer.
        unsafe { out.total_bytes.write_unaligned(total) };
    }
}

/// `ID3D12Device::GetCopyableFootprints` (`d3d12_device.cpp:1291`).
///
/// # Safety
///
/// `this`, `desc`, `layouts`, `num_rows`, `row_sizes`, and `total_bytes` are caller-supplied
/// pointers.
pub unsafe extern "system-unwind" fn get_copyable_footprints(
    this: *mut c_void,
    desc: *const c_void,
    first_sub: u32,
    num_sub: u32,
    base_offset: u64,
    layouts: *mut c_void,
    num_rows: *mut u32,
    row_sizes: *mut u64,
    total_bytes: *mut u64,
) {
    crate::scope!("device.GetCopyableFootprints");
    let _ = this;
    let d = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` is non-null and points to a readable `ResourceDesc`.
        Some(unsafe { desc.cast::<ResourceDesc>().read_unaligned() })
    };
    let out = FootprintOutputs {
        layouts,
        num_rows,
        row_sizes,
        total_bytes,
    };
    // SAFETY: pointer arguments are forwarded from the COM caller.
    unsafe {
        copyable_footprints_inner(d.as_ref(), first_sub, num_sub, base_offset, &out);
    };
}

/// `ID3D12Device::GetResourceTiling` (`d3d12_device.cpp:1463`).
///
/// # Safety
///
/// `this` and the out-parameter pointers are caller-supplied.
pub unsafe extern "system-unwind" fn get_resource_tiling(
    _this: *mut c_void,
    _tiled_resource: *mut c_void,
    _num_tiles: *mut u32,
    _packed_mip_desc: *mut c_void,
    _standard_tile_shape: *mut c_void,
    _num_sub_tilings: *mut u32,
    _first_sub_tiling: u32,
    _sub_tilings: *mut c_void,
) {
    crate::scope!("device.GetResourceTiling");
    let (): () = crate::not_implemented!("ID3D12Device::GetResourceTiling");
}

/// `ID3D12Device4::GetResourceAllocationInfo1` (`d3d12_device.cpp:1632`).
///
/// # Safety
///
/// `this`, `ret`, `descs`, and `info1` are caller-supplied pointers.
pub unsafe fn get_resource_allocation_info1(
    this: *mut c_void,
    ret: *mut c_void,
    _visible_mask: u32,
    num_descs: u32,
    descs: *const c_void,
    info1: *mut c_void,
) -> *mut c_void {
    crate::scope!("device.GetResourceAllocationInfo1");
    let result = if this.is_null() || num_descs == 0 || descs.is_null() {
        ResourceAllocationInfo {
            size_in_bytes: u64::MAX,
            alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
        }
    } else {
        // SAFETY: `this` is a non-null `Device` pointer.
        let dev = unsafe { &*this.cast::<Device>() };
        let count = num_descs as usize;
        let mut plain = Vec::with_capacity(count);
        let ptr = descs.cast::<ResourceDesc>();
        for i in 0..count {
            // SAFETY: `i` is within `count` elements.
            let elem = unsafe { ptr.add(i) };
            // SAFETY: `elem` points to a readable `ResourceDesc`.
            plain.push(unsafe { elem.read_unaligned() });
        }
        // SAFETY: `plain` is valid and `info1` is forwarded from the COM caller.
        unsafe { compute_allocation_info(dev, &plain, info1) }
    };
    if !ret.is_null() {
        // SAFETY: `ret` is a valid caller-provided sret pointer.
        unsafe { ret.cast::<ResourceAllocationInfo>().write_unaligned(result) };
    }
    ret
}

/// `ID3D12Device8::GetResourceAllocationInfo2` (`d3d12_device.cpp:1777`).
///
/// # Safety
///
/// `this`, `ret`, `descs`, and `info1` are caller-supplied pointers.
pub unsafe fn get_resource_allocation_info2(
    this: *mut c_void,
    ret: *mut c_void,
    _visible_mask: u32,
    num_descs: u32,
    descs: *const c_void,
    info1: *mut c_void,
) -> *mut c_void {
    crate::scope!("device.GetResourceAllocationInfo2");
    let result = if this.is_null() || num_descs == 0 || descs.is_null() {
        ResourceAllocationInfo {
            size_in_bytes: u64::MAX,
            alignment: D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT,
        }
    } else {
        // SAFETY: `this` is a non-null `Device` pointer.
        let dev = unsafe { &*this.cast::<Device>() };
        let count = num_descs as usize;
        let mut plain = Vec::with_capacity(count);
        let ptr = descs.cast::<ResourceDesc1>();
        for i in 0..count {
            // SAFETY: `i` is within `count` elements.
            let elem = unsafe { ptr.add(i) };
            // SAFETY: `elem` points to a readable `ResourceDesc1`.
            let d1 = unsafe { elem.read_unaligned() };
            plain.push(plain_desc(&d1));
        }
        // SAFETY: `plain` is valid and `info1` is forwarded from the COM caller.
        unsafe { compute_allocation_info(dev, &plain, info1) }
    };
    if !ret.is_null() {
        // SAFETY: `ret` is a valid caller-provided sret pointer.
        unsafe { ret.cast::<ResourceAllocationInfo>().write_unaligned(result) };
    }
    ret
}

/// `ID3D12Device8::GetCopyableFootprints1` (`d3d12_device.cpp:1798`).
///
/// # Safety
///
/// `this`, `desc`, `layouts`, `num_rows`, `row_sizes`, and `total_bytes` are caller-supplied
/// pointers.
pub unsafe extern "system-unwind" fn get_copyable_footprints1(
    this: *mut c_void,
    desc: *const c_void,
    first_sub: u32,
    num_sub: u32,
    base_offset: u64,
    layouts: *mut c_void,
    num_rows: *mut u32,
    row_sizes: *mut u64,
    total_bytes: *mut u64,
) {
    crate::scope!("device.GetCopyableFootprints1");
    let _ = this;
    let d = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` is non-null and points to a readable `ResourceDesc1`.
        let d1 = unsafe { desc.cast::<ResourceDesc1>().read_unaligned() };
        Some(plain_desc(&d1))
    };
    let out = FootprintOutputs {
        layouts,
        num_rows,
        row_sizes,
        total_bytes,
    };
    // SAFETY: pointer arguments are forwarded from the COM caller.
    unsafe {
        copyable_footprints_inner(d.as_ref(), first_sub, num_sub, base_offset, &out);
    };
}
