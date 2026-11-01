//! Resource copy and tiled-resource stubs (`CopyResource`, `CopySubresourceRegion*`).

use core::ffi::c_void;

use super::{Context, Mode};
use crate::com::{E_NOTIMPL, Hresult};

impl<M: Mode> Context<M> {
    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext::CopySubresourceRegion"
    )]
    pub const fn copy_subresource_region(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_x: u32,
        dst_y: u32,
        dst_z: u32,
        src_resource: *mut c_void,
        src_subresource: u32,
        src_box: *const c_void,
    ) {
        self.copy_subresource_region1(
            dst_resource,
            dst_subresource,
            dst_x,
            dst_y,
            dst_z,
            src_resource,
            src_subresource,
            src_box,
            0,
        );
    }

    pub const fn copy_resource(&mut self, dst_resource: *mut c_void, src_resource: *mut c_void) {
        let _ = (dst_resource, src_resource);
    }

    pub const fn copy_structure_count(
        &mut self,
        dst_buffer: *mut c_void,
        dst_aligned_byte_offset: u32,
        src_view: *mut c_void,
    ) {
        let _ = (dst_buffer, dst_aligned_byte_offset, src_view);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext1::CopySubresourceRegion1"
    )]
    pub const fn copy_subresource_region1(
        &mut self,
        dst_resource: *mut c_void,
        dst_subresource: u32,
        dst_x: u32,
        dst_y: u32,
        dst_z: u32,
        src_resource: *mut c_void,
        src_subresource: u32,
        src_box: *const c_void,
        copy_flags: u32,
    ) {
        let _ = (
            dst_resource,
            dst_subresource,
            dst_x,
            dst_y,
            dst_z,
            src_resource,
            src_subresource,
            src_box,
            copy_flags,
        );
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "matches ID3D11DeviceContext2::UpdateTileMappings"
    )]
    pub const fn update_tile_mappings(
        &mut self,
        tiled_resource: *mut c_void,
        num_tiled_resource_regions: u32,
        tiled_resource_region_start_coordinates: *const c_void,
        tiled_resource_region_sizes: *const c_void,
        tile_pool: *mut c_void,
        num_ranges: u32,
        range_flags: *const u32,
        tile_pool_start_offsets: *const u32,
        range_tile_counts: *const u32,
        flags: u32,
    ) -> Hresult {
        let _ = (
            tiled_resource,
            num_tiled_resource_regions,
            tiled_resource_region_start_coordinates,
            tiled_resource_region_sizes,
            tile_pool,
            num_ranges,
            range_flags,
            tile_pool_start_offsets,
            range_tile_counts,
            flags,
        );
        E_NOTIMPL
    }

    pub const fn copy_tile_mappings(
        &mut self,
        dst_tiled_resource: *mut c_void,
        dst_region_start_coordinate: *const c_void,
        src_tiled_resource: *mut c_void,
        src_region_start_coordinate: *const c_void,
        region_size: *const c_void,
        flags: u32,
    ) -> Hresult {
        let _ = (
            dst_tiled_resource,
            dst_region_start_coordinate,
            src_tiled_resource,
            src_region_start_coordinate,
            region_size,
            flags,
        );
        E_NOTIMPL
    }

    pub const fn copy_tiles(
        &mut self,
        tiled_resource: *mut c_void,
        tile_region_start_coordinate: *const c_void,
        tile_region_size: *const c_void,
        buffer: *mut c_void,
        buffer_start_offset_in_bytes: u64,
        flags: u32,
    ) {
        let _ = (
            tiled_resource,
            tile_region_start_coordinate,
            tile_region_size,
            buffer,
            buffer_start_offset_in_bytes,
            flags,
        );
    }

    pub const fn update_tiles(
        &mut self,
        dst_tiled_resource: *mut c_void,
        dst_tile_region_start_coordinate: *const c_void,
        dst_tile_region_size: *const c_void,
        src_tile_data: *const c_void,
        flags: u32,
    ) {
        let _ = (
            dst_tiled_resource,
            dst_tile_region_start_coordinate,
            dst_tile_region_size,
            src_tile_data,
            flags,
        );
    }

    pub const fn resize_tile_pool(
        &mut self,
        tile_pool: *mut c_void,
        new_size_in_bytes: u64,
    ) -> Hresult {
        let _ = (tile_pool, new_size_in_bytes);
        E_NOTIMPL
    }

    pub const fn tiled_resource_barrier(
        &mut self,
        tiled_resource_or_view_access_before_barrier: *mut c_void,
        tiled_resource_or_view_access_after_barrier: *mut c_void,
    ) {
        let _ = (
            tiled_resource_or_view_access_before_barrier,
            tiled_resource_or_view_access_after_barrier,
        );
    }
}
