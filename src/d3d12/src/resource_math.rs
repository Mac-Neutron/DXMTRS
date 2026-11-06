//! Resource math: footprints, allocation sizes, extents and the creation checks.
//!
//! Ported from `d3d12_resource_helper.cpp` (`GetResourceExtent`, `DecomposeSubresource`,
//! `IsD3D12BoxInBounds`, `IsCpuVisibleHeap`, `IsPrivateHeap` and the three `Validate…`
//! functions) and from `d3d12_device.cpp` (`GetCopyableFootprints`, the arithmetic of
//! `GetResourceAllocationInfo1` and `PlainDesc`). They answer for every input as the C++ does,
//! including the 32-bit wrap of its `UINT` arithmetic and the shifts the hardware masks.
//!
//! What the C++ asks Metal is a parameter: [`allocation_info`] takes the heap sizes. What it reads
//! from the environment is a parameter too: [`is_private_heap`] takes the answer of
//! `DXMT_D3D12_PRIVATE`. The descriptions are values the front end has read out of the
//! application's memory, one unaligned read each.

use dxmt_core::format::{
    MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_DEPTH_PLANER, MTL_DXGI_FORMAT_STENCIL_PLANER,
    query_dxgi_ref,
};
use dxmt_util::com::Hresult;

use crate::abi::{
    E_INVALIDARG,
    barrier::{
        D3D12_HEAP_TYPE_CUSTOM, D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_READBACK,
        D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST,
        D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_RENDER_TARGET,
        D3D12_RESOURCE_STATE_RESOLVE_DEST, D3D12_RESOURCE_STATE_STREAM_OUT,
        D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_VIDEO_DECODE_WRITE,
        D3D12_RESOURCE_STATE_VIDEO_ENCODE_WRITE, D3D12_RESOURCE_STATE_VIDEO_PROCESS_WRITE,
    },
    pipeline::{S_OK, SampleDesc},
    resource::{
        D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE, D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT,
        D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH,
        D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER, D3D12_MEMORY_POOL_L1, D3D12_MEMORY_POOL_UNKNOWN,
        D3D12_RESOURCE_DIMENSION_BUFFER, D3D12_RESOURCE_DIMENSION_TEXTURE1D,
        D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_DIMENSION_TEXTURE3D,
        D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET,
        D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
        D3D12_TEXTURE_DATA_PITCH_ALIGNMENT, D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT,
        D3D12_TEXTURE_LAYOUT_ROW_MAJOR, D3d12Box, DXGI_FORMAT_R8_TYPELESS,
        DXGI_FORMAT_R32_TYPELESS, DXGI_FORMAT_UNKNOWN, HeapProperties, PlacedSubresourceFootprint,
        ResourceAllocationInfo, ResourceAllocationInfo1, ResourceDesc, ResourceDesc1,
        SubresourceFootprint,
    },
};

/// The resource states a resource is written in, each alone (`kExclusiveWrite`).
const EXCLUSIVE_WRITE: u32 = D3D12_RESOURCE_STATE_RENDER_TARGET
    | D3D12_RESOURCE_STATE_UNORDERED_ACCESS
    | D3D12_RESOURCE_STATE_DEPTH_WRITE
    | D3D12_RESOURCE_STATE_STREAM_OUT
    | D3D12_RESOURCE_STATE_COPY_DEST
    | D3D12_RESOURCE_STATE_RESOLVE_DEST
    | D3D12_RESOURCE_STATE_VIDEO_DECODE_WRITE
    | D3D12_RESOURCE_STATE_VIDEO_PROCESS_WRITE
    | D3D12_RESOURCE_STATE_VIDEO_ENCODE_WRITE;

/// The four numbers `DecomposeSubresource` answers.
pub struct DecomposedSubresource {
    /// The mip count of the resource: its `MipLevels`, or the full chain for 0.
    pub mip_levels: u32,
    /// The subresource's mip level.
    pub mip_slice: u32,
    /// The subresource's array slice.
    pub array_slice: u32,
    /// The subresource's plane.
    pub plane_slice: u32,
}

/// One subresource of `GetCopyableFootprints`: what goes in the three output arrays.
pub struct Footprint {
    /// `pLayouts[i]`.
    pub layout: PlacedSubresourceFootprint,
    /// `pNumRows[i]`.
    pub num_rows: u32,
    /// `pRowSizeInBytes[i]`.
    pub row_size: u64,
}

/// What Metal answers for a heap allocation: `WMTSizeAndAlign`.
pub struct HeapSizeAndAlign {
    /// The size in bytes.
    pub size: u32,
    /// The alignment in bytes.
    pub align: u32,
}

/// What `GetResourceAllocationInfo1` answers.
pub struct AllocationInfos {
    /// The returned allocation: all the resources placed one after the other.
    pub total: ResourceAllocationInfo,
    /// `pAllocationInfos[i]`, one for each resource up to the first invalid one.
    pub each: Vec<ResourceAllocationInfo1>,
}

/// The low 32 bits of a 64-bit value, as a C++ conversion to `uint32_t` keeps them.
const fn low32(value: u64) -> u32 {
    (value & 0xffff_ffff) as u32
}

/// `align` of `util_math.hpp` on `UINT`s, with the wrap of C++'s unsigned arithmetic.
const fn align32(what: u32, to: u32) -> u32 {
    what.wrapping_add(to).wrapping_sub(1) & !to.wrapping_sub(1)
}

/// `align` of `util_math.hpp` on `UINT64`s, with the wrap of C++'s unsigned arithmetic.
const fn align64(what: u64, to: u64) -> u64 {
    what.wrapping_add(to).wrapping_sub(1) & !to.wrapping_sub(1)
}

/// A resource's extent at a mip level, 1 for each dimension it lacks (`GetResourceExtent`).
///
/// The shifts are the hardware's, which uses only the low bits of a mip level past the type's
/// width; a 64-bit width is cut to 32 bits after the shift.
#[must_use]
pub fn resource_extent(desc: &ResourceDesc, mip_slice: u32) -> D3d12Box {
    let mut extent = D3d12Box {
        left: 0,
        top: 0,
        front: 0,
        right: 1,
        bottom: 1,
        back: 1,
    };
    let depth = u32::from(desc.depth_or_array_size).wrapping_shr(mip_slice);
    let height = desc.height.wrapping_shr(mip_slice);
    let width = low32(desc.width.wrapping_shr(mip_slice));
    match desc.dimension {
        D3D12_RESOURCE_DIMENSION_TEXTURE3D => {
            extent.back = depth.max(1);
            extent.bottom = height.max(1);
            extent.right = width.max(1);
        }
        D3D12_RESOURCE_DIMENSION_TEXTURE2D => {
            extent.bottom = height.max(1);
            extent.right = width.max(1);
        }
        D3D12_RESOURCE_DIMENSION_BUFFER | D3D12_RESOURCE_DIMENSION_TEXTURE1D => {
            extent.right = width.max(1);
        }
        _ => {}
    }
    extent
}

/// A subresource index as its mip, array and plane slices (`DecomposeSubresource`).
///
/// A buffer (or any other dimension) has the one subresource and all slices 0. A texture whose
/// mip count or array size is 0 divides by zero in the C++; its slices are 0 here.
#[must_use]
pub fn decompose_subresource(desc: &ResourceDesc, subresource: u32) -> DecomposedSubresource {
    let mut extent_or = low32(desc.width);
    let array_size = match desc.dimension {
        D3D12_RESOURCE_DIMENSION_TEXTURE1D => u32::from(desc.depth_or_array_size),
        D3D12_RESOURCE_DIMENSION_TEXTURE2D => {
            extent_or |= desc.height;
            u32::from(desc.depth_or_array_size)
        }
        D3D12_RESOURCE_DIMENSION_TEXTURE3D => {
            extent_or |= desc.height | u32::from(desc.depth_or_array_size);
            1
        }
        _ => {
            return DecomposedSubresource {
                mip_levels: 1,
                mip_slice: 0,
                array_slice: 0,
                plane_slice: 0,
            };
        }
    };
    let mip_levels = if desc.mip_levels == 0 {
        32 - extent_or.leading_zeros()
    } else {
        u32::from(desc.mip_levels)
    };
    DecomposedSubresource {
        mip_levels,
        mip_slice: subresource.checked_rem(mip_levels).unwrap_or(0),
        array_slice: subresource
            .checked_div(mip_levels)
            .and_then(|slice| slice.checked_rem(array_size))
            .unwrap_or(0),
        plane_slice: subresource
            .checked_div(mip_levels.wrapping_mul(array_size))
            .unwrap_or(0),
    }
}

/// Whether `inner` lies inside `bounds` (`IsD3D12BoxInBounds`).
#[must_use]
pub const fn is_box_in_bounds(inner: &D3d12Box, bounds: &D3d12Box) -> bool {
    inner.left >= bounds.left
        && inner.top >= bounds.top
        && inner.front >= bounds.front
        && inner.right <= bounds.right
        && inner.bottom <= bounds.bottom
        && inner.back <= bounds.back
}

/// Whether the CPU can see a heap with these properties (`IsCpuVisibleHeap`).
#[must_use]
pub const fn is_cpu_visible_heap(heap: &HeapProperties) -> bool {
    match heap.r#type {
        D3D12_HEAP_TYPE_UPLOAD | D3D12_HEAP_TYPE_READBACK => true,
        D3D12_HEAP_TYPE_CUSTOM => heap.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE,
        _ => false,
    }
}

/// Whether a heap with these properties is Metal `Private` storage (`IsPrivateHeap`).
///
/// A heap the CPU cannot see is, unless `DXMT_D3D12_PRIVATE=0` turned that off: `private_on` is
/// whether it is not 0.
#[must_use]
pub const fn is_private_heap(heap: &HeapProperties, private_on: bool) -> bool {
    private_on && !is_cpu_visible_heap(heap)
}

/// Checks a resource's initial state against its heap (`ValidateResourceStates`).
#[must_use]
pub const fn validate_resource_states(state: u32, heap: &HeapProperties) -> Hresult {
    if state & EXCLUSIVE_WRITE != 0 && (state & !EXCLUSIVE_WRITE != 0 || state.count_ones() != 1) {
        return E_INVALIDARG;
    }
    if heap.r#type == D3D12_HEAP_TYPE_READBACK
        && state != D3D12_RESOURCE_STATE_COPY_DEST
        && state != D3D12_RESOURCE_STATE_COMMON
    {
        return E_INVALIDARG;
    }
    S_OK
}

/// Checks a resource description against the heap it goes in (`ValidateResourceDescs`).
#[must_use]
pub const fn validate_resource_descs(desc: &ResourceDesc, heap: &HeapProperties) -> Hresult {
    match heap.r#type {
        D3D12_HEAP_TYPE_UPLOAD | D3D12_HEAP_TYPE_READBACK => {
            if desc.flags & D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET != 0
                || desc.flags & D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS != 0
                || desc.dimension != D3D12_RESOURCE_DIMENSION_BUFFER
            {
                return E_INVALIDARG;
            }
        }
        D3D12_HEAP_TYPE_DEFAULT | D3D12_HEAP_TYPE_CUSTOM => {}
        _ => return E_INVALIDARG,
    }
    match desc.dimension {
        D3D12_RESOURCE_DIMENSION_BUFFER => {
            if desc.flags & D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS != 0 {
                return E_INVALIDARG;
            }
        }
        D3D12_RESOURCE_DIMENSION_TEXTURE1D
        | D3D12_RESOURCE_DIMENSION_TEXTURE2D
        | D3D12_RESOURCE_DIMENSION_TEXTURE3D
            if desc.layout == D3D12_TEXTURE_LAYOUT_ROW_MAJOR
                && (desc.dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
                    || desc.flags & D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER == 0
                    || desc.mip_levels != 1
                    || desc.depth_or_array_size != 1
                    || is_cpu_visible_heap(heap)) =>
        {
            return E_INVALIDARG;
        }
        _ => {}
    }
    S_OK
}

/// Checks a heap's properties and flags (`ValidateHeapProperties`).
///
/// `adapter_is_numa` is whether the device advertises a NUMA adapter.
#[must_use]
pub const fn validate_heap_properties(
    heap: &HeapProperties,
    flags: u32,
    adapter_is_numa: bool,
) -> Hresult {
    match heap.r#type {
        D3D12_HEAP_TYPE_DEFAULT | D3D12_HEAP_TYPE_READBACK => {
            if flags & D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH != 0 {
                return E_INVALIDARG;
            }
            if heap.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_UNKNOWN
                || heap.memory_pool_preference != D3D12_MEMORY_POOL_UNKNOWN
            {
                return E_INVALIDARG;
            }
        }
        D3D12_HEAP_TYPE_UPLOAD => {
            if heap.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_UNKNOWN
                || heap.memory_pool_preference != D3D12_MEMORY_POOL_UNKNOWN
            {
                return E_INVALIDARG;
            }
        }
        D3D12_HEAP_TYPE_CUSTOM => {
            if heap.cpu_page_property == D3D12_CPU_PAGE_PROPERTY_UNKNOWN
                || heap.memory_pool_preference == D3D12_MEMORY_POOL_UNKNOWN
            {
                return E_INVALIDARG;
            }
            if heap.memory_pool_preference == D3D12_MEMORY_POOL_L1
                && adapter_is_numa
                && heap.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE
            {
                return E_INVALIDARG;
            }
        }
        _ => return E_INVALIDARG,
    }
    if heap.memory_pool_preference == D3D12_MEMORY_POOL_L1
        && (!adapter_is_numa || heap.cpu_page_property != D3D12_CPU_PAGE_PROPERTY_NOT_AVAILABLE)
    {
        return E_INVALIDARG;
    }
    if flags & D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER != 0 && is_cpu_visible_heap(heap) {
        return E_INVALIDARG;
    }
    S_OK
}

/// The plain description of a `D3D12_RESOURCE_DESC1` (`PlainDesc`).
///
/// The sampler feedback's mip region only matters to sampler feedback, which is not supported.
#[must_use]
pub const fn plain_desc(desc: &ResourceDesc1) -> ResourceDesc {
    ResourceDesc {
        dimension: desc.dimension,
        alignment: desc.alignment,
        width: desc.width,
        height: desc.height,
        depth_or_array_size: desc.depth_or_array_size,
        mip_levels: desc.mip_levels,
        format: desc.format,
        sample_desc: SampleDesc {
            count: desc.sample_desc.count,
            quality: desc.sample_desc.quality,
        },
        layout: desc.layout,
        flags: desc.flags,
    }
}

/// How a resource's subresources are laid out in a copy buffer, from its format.
struct Planes {
    /// How many planes the format has.
    count: u32,
    /// The format of each plane's footprint.
    formats: [u32; 2],
    /// Each plane's bytes per texel (or per block).
    bytes_per_texel: [u32; 2],
    /// The block width: 4 for a block-compressed format, else 1.
    block_width: u32,
    /// The subresources of one plane.
    per_plane_subresources: u32,
}

/// The planes of a resource, or `None` when `GetCopyableFootprints` refuses its description.
fn planes(desc: &ResourceDesc) -> Option<Planes> {
    let mut per_plane_subresources = decompose_subresource(desc, 0).mip_levels;
    let mut planes = Planes {
        count: 1,
        formats: [DXGI_FORMAT_UNKNOWN, 0],
        bytes_per_texel: [1, 0],
        block_width: 1,
        per_plane_subresources: 0,
    };
    if desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
        if desc.format != DXGI_FORMAT_UNKNOWN {
            return None;
        }
    } else {
        let (hr, format) = query_dxgi_ref(desc.format);
        if hr < 0 {
            return None;
        }
        if desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE1D && desc.height != 1 {
            return None;
        }
        if format.flag & MTL_DXGI_FORMAT_BC != 0 {
            planes.block_width = 4;
        }
        if format.planar_count > 1 {
            debug_assert!(
                format.flag & (MTL_DXGI_FORMAT_DEPTH_PLANER | MTL_DXGI_FORMAT_STENCIL_PLANER) != 0,
                "a planar format is a depth or stencil one"
            );
            planes.count = u32::from(format.planar_count);
            planes.formats = [DXGI_FORMAT_R32_TYPELESS, DXGI_FORMAT_R8_TYPELESS];
            planes.bytes_per_texel = [4, 1];
        } else {
            planes.formats[0] = desc.format;
            planes.bytes_per_texel[0] = format.bytes_per_texel;
        }
    }
    if desc.dimension != D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        per_plane_subresources =
            per_plane_subresources.wrapping_mul(u32::from(desc.depth_or_array_size));
    }
    planes.per_plane_subresources = per_plane_subresources;
    Some(planes)
}

/// The value `GetCopyableFootprints` gives a subresource it cannot lay out.
const fn refused() -> Footprint {
    Footprint {
        layout: PlacedSubresourceFootprint {
            offset: u64::MAX,
            footprint: SubresourceFootprint {
                format: u32::MAX,
                width: u32::MAX,
                height: u32::MAX,
                depth: u32::MAX,
                row_pitch: u32::MAX,
            },
        },
        num_rows: u32::MAX,
        row_size: u64::MAX,
    }
}

/// Lays out subresources in a copy buffer (`GetCopyableFootprints`).
///
/// It places `subresource_count` of them from `first_subresource`, the first at `base_offset`.
/// `each` receives every subresource's footprint with its index, in order; the front end copies
/// it into the arrays the application gave, those it gave. Returns the total size in bytes.
/// When the description cannot be laid out (none, a typed buffer, a format Metal has no
/// counterpart for, a 1D texture taller than 1, a range past the subresources), every one of
/// the `subresource_count` footprints is all ones, as is the total.
///
/// A format that is not planar, has a row and has no bytes per texel is `IMPLEMENT_ME` in the
/// C++, which aborts or throws; none of `dxmt_core::format`'s rows is one.
pub fn copyable_footprints(
    desc: Option<&ResourceDesc>,
    first_subresource: u32,
    subresource_count: u32,
    base_offset: u64,
    mut each: impl FnMut(u32, &Footprint),
) -> u64 {
    let plan = desc.and_then(|desc| planes(desc).map(|planes| (desc, planes)));
    let in_range = plan.as_ref().is_some_and(|(_, planes)| {
        let total = planes.per_plane_subresources.wrapping_mul(planes.count);
        first_subresource < total && subresource_count <= total - first_subresource
    });
    let Some((desc, planes)) = plan.filter(|_| in_range) else {
        let refused = refused();
        for index in 0..subresource_count {
            each(index, &refused);
        }
        return u64::MAX;
    };
    let mut offset = 0_u64;
    let mut total_bytes = 0_u64;
    for index in 0..subresource_count {
        let slices = decompose_subresource(desc, first_subresource.wrapping_add(index));
        let plane = usize::try_from(slices.plane_slice).unwrap_or(0).min(1);
        let extent = resource_extent(desc, slices.mip_slice);
        let width = align32(extent.right, planes.block_width);
        let height = align32(extent.bottom, planes.block_width);
        let row_count = height / planes.block_width;
        let depth = extent.back;
        let row_size = (width / planes.block_width).wrapping_mul(planes.bytes_per_texel[plane]);
        let plane_pitch = D3D12_TEXTURE_DATA_PITCH_ALIGNMENT.wrapping_mul(planes.count);
        let row_pitch = align32(row_size, plane_pitch);
        each(
            index,
            &Footprint {
                layout: PlacedSubresourceFootprint {
                    offset: base_offset.wrapping_add(offset),
                    footprint: SubresourceFootprint {
                        format: planes.formats[plane],
                        width,
                        height,
                        depth,
                        row_pitch,
                    },
                },
                num_rows: row_count,
                row_size: u64::from(row_size),
            },
        );
        let mut size = row_pitch
            .wrapping_mul(row_count.wrapping_sub(1))
            .wrapping_add(row_size);
        size = align32(size, plane_pitch)
            .wrapping_mul(depth.wrapping_sub(1))
            .wrapping_add(size);
        total_bytes = offset.wrapping_add(u64::from(size));
        offset = align64(total_bytes, D3D12_TEXTURE_DATA_PLACEMENT_ALIGNMENT);
    }
    total_bytes
}

/// Places resources one after the other in one allocation (`GetResourceAllocationInfo1`).
///
/// `heap_size_and_align` is what Metal answers for one resource: its heap buffer's size for a
/// buffer, its heap texture's size and alignment for a texture, `None` for a description the
/// C++'s `PopulateWMTTextureInfo` refuses. A buffer's `Alignment` can only be 0 or 64 KiB, a
/// texture's is at least what Metal says and what the description or its sample count asks.
/// The first resource that is invalid ends it: the total is all ones in size, aligned as the
/// resources so far ask for, and `each` stops before it.
pub fn allocation_info(
    descs: &[ResourceDesc],
    mut heap_size_and_align: impl FnMut(&ResourceDesc) -> Option<HeapSizeAndAlign>,
) -> AllocationInfos {
    let mut total = ResourceAllocationInfo {
        size_in_bytes: 0,
        alignment: 1,
    };
    let mut each = Vec::with_capacity(descs.len());
    let mut has_msaa_resource = false;
    for desc in descs {
        has_msaa_resource |= desc.sample_desc.count > 1;
        let is_buffer = desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER;
        let bad_buffer_alignment =
            desc.alignment != 0 && desc.alignment != D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT;
        let sized = if is_buffer && bad_buffer_alignment {
            None
        } else {
            heap_size_and_align(desc)
        };
        let Some(heap) = sized else {
            return AllocationInfos {
                total: ResourceAllocationInfo {
                    size_in_bytes: u64::MAX,
                    alignment: if has_msaa_resource {
                        D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT
                    } else {
                        D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
                    },
                },
                each,
            };
        };
        let alignment = if is_buffer {
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
        } else {
            let requested = if desc.alignment != 0 {
                desc.alignment
            } else if desc.sample_desc.count > 1 {
                D3D12_DEFAULT_MSAA_RESOURCE_PLACEMENT_ALIGNMENT
            } else {
                D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
            };
            u64::from(heap.align).max(requested)
        };
        let size_in_bytes = align64(u64::from(heap.size), alignment);
        let offset = align64(total.size_in_bytes, alignment);
        each.push(ResourceAllocationInfo1 {
            offset,
            alignment,
            size_in_bytes,
        });
        total.size_in_bytes = offset.wrapping_add(size_in_bytes);
        total.alignment = total.alignment.max(alignment);
    }
    total.size_in_bytes = align64(total.size_in_bytes, total.alignment);
    AllocationInfos { total, each }
}

#[cfg(test)]
mod tests;
