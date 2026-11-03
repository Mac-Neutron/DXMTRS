//! Static vtables for [`Context<Immediate>`] and [`Context<Deferred>`].
//!
//! Every slot forwards to a `Context<M>` method implemented in its owning lane's submodule.

use core::ffi::c_void;

use super::{Context, Deferred, Immediate, Mode};
use crate::{
    abi::{
        ext::{
            D3D11Piece, D3D11UpscaleDesc, IMTLD3D11ContextExt1Vtbl, IMTLD3D11ContextExt2Vtbl,
            IMTLD3D11ContextExtVtbl, IMTLD3D11DeviceContextVtbl,
        },
        vtbl::{ID3D11DeviceContext4Vtbl, ID3DUserDefinedAnnotationVtbl, IUnknownVtbl},
    },
    com::container_of,
};

macro_rules! slot_unlocked {
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? ) -> $ret:ty) => {{
        unsafe extern "system-unwind" fn thunk<M: Mode>(
            this: *mut c_void,
            $($arg: $ty),*
        ) -> $ret {
            // SAFETY: `this` is a valid `Context<M>` COM interface pointer.
            let ctx = unsafe { Context::<M>::from_iface(this) };
            ctx.$name($($arg),*)
        }
        thunk::<M>
    }};
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? )) => {{
        unsafe extern "system-unwind" fn thunk<M: Mode>(
            this: *mut c_void,
            $($arg: $ty),*
        ) {
            // SAFETY: `this` is a valid `Context<M>` COM interface pointer.
            let ctx = unsafe { Context::<M>::from_iface(this) };
            ctx.$name($($arg),*);
        }
        thunk::<M>
    }};
}

macro_rules! slot {
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? ) -> $ret:ty) => {{
        unsafe extern "system-unwind" fn thunk<M: Mode>(
            this: *mut c_void,
            $($arg: $ty),*
        ) -> $ret {
            let _guard = M::lock_iface(this);
            // SAFETY: `this` is a valid `Context<M>` COM interface pointer.
            let ctx = unsafe { Context::<M>::from_iface(this) };
            ctx.$name($($arg),*)
        }
        thunk::<M>
    }};
    ($name:ident ( $($arg:ident : $ty:ty),* $(,)? )) => {{
        unsafe extern "system-unwind" fn thunk<M: Mode>(
            this: *mut c_void,
            $($arg: $ty),*
        ) {
            let _guard = M::lock_iface(this);
            // SAFETY: `this` is a valid `Context<M>` COM interface pointer.
            let ctx = unsafe { Context::<M>::from_iface(this) };
            ctx.$name($($arg),*);
        }
        thunk::<M>
    }};
}

/// Builds an [`IMTLD3D11DeviceContextVtbl`] for `Context<M>`.
#[must_use]
pub const fn context_vtbl<M: Mode>() -> IMTLD3D11DeviceContextVtbl {
    IMTLD3D11DeviceContextVtbl {
        base: ID3D11DeviceContext4Vtbl {
            query_interface: slot_unlocked!(
                query_interface(riid: *const c_void, out: *mut *mut c_void) -> i32
            ),
            add_ref: slot_unlocked!(add_ref() -> u32),
            release: slot_unlocked!(release() -> u32),
            get_device: slot_unlocked!(get_device(out: *mut *mut c_void)),
            get_private_data: slot_unlocked!(
                get_private_data(
                    guid: *const c_void,
                    data_size: *mut u32,
                    data: *mut c_void,
                ) -> i32
            ),
            set_private_data: slot_unlocked!(
                set_private_data(guid: *const c_void, data_size: u32, data: *const c_void) -> i32
            ),
            set_private_data_interface: slot_unlocked!(
                set_private_data_interface(guid: *const c_void, object: *const c_void) -> i32
            ),
            vs_set_constant_buffers: slot!(
                vs_set_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                )
            ),
            ps_set_shader_resources: slot!(
                ps_set_shader_resources(start_slot: u32, num_views: u32, views: *const *mut c_void)
            ),
            ps_set_shader: slot!(
                ps_set_shader(
                    shader: *mut c_void,
                    class_instances: *const *mut c_void,
                    num_class_instances: u32,
                )
            ),
            ps_set_samplers: slot!(
                ps_set_samplers(start_slot: u32, num_samplers: u32, samplers: *const *mut c_void)
            ),
            vs_set_shader: slot!(
                vs_set_shader(
                    shader: *mut c_void,
                    class_instances: *const *mut c_void,
                    num_class_instances: u32,
                )
            ),
            draw_indexed: slot!(draw_indexed(index_count: u32, start_index: u32, base_vertex: i32)),
            draw: slot!(draw(vertex_count: u32, start_vertex: u32)),
            map: slot!(
                map(
                    resource: *mut c_void,
                    subresource: u32,
                    map_type: u32,
                    map_flags: u32,
                    mapped_resource: *mut c_void,
                ) -> i32
            ),
            unmap: slot!(unmap(resource: *mut c_void, subresource: u32)),
            ps_set_constant_buffers: slot!(
                ps_set_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                )
            ),
            ia_set_input_layout: slot!(ia_set_input_layout(input_layout: *mut c_void)),
            ia_set_vertex_buffers: slot!(
                ia_set_vertex_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                    strides: *const u32,
                    offsets: *const u32,
                )
            ),
            ia_set_index_buffer: slot!(
                ia_set_index_buffer(index_buffer: *mut c_void, format: u32, offset: u32)
            ),
            draw_indexed_instanced: slot!(
                draw_indexed_instanced(
                    index_count_per_instance: u32,
                    instance_count: u32,
                    start_index: u32,
                    base_vertex: i32,
                    start_instance: u32,
                )
            ),
            draw_instanced: slot!(
                draw_instanced(
                    vertex_count_per_instance: u32,
                    instance_count: u32,
                    start_vertex: u32,
                    start_instance: u32,
                )
            ),
            gs_set_constant_buffers: slot!(
                gs_set_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                )
            ),
            gs_set_shader: slot!(
                gs_set_shader(
                    shader: *mut c_void,
                    class_instances: *const *mut c_void,
                    num_class_instances: u32,
                )
            ),
            ia_set_primitive_topology: slot!(ia_set_primitive_topology(topology: u32)),
            vs_set_shader_resources: slot!(
                vs_set_shader_resources(start_slot: u32, num_views: u32, views: *const *mut c_void)
            ),
            vs_set_samplers: slot!(
                vs_set_samplers(start_slot: u32, num_samplers: u32, samplers: *const *mut c_void)
            ),
            begin: slot!(begin(r#async: *mut c_void)),
            end: slot!(end(r#async: *mut c_void)),
            get_data: slot!(
                get_data(
                    r#async: *mut c_void,
                    data: *mut c_void,
                    data_size: u32,
                    get_data_flags: u32,
                ) -> i32
            ),
            set_predication: slot!(set_predication(predicate: *mut c_void, predicate_value: i32)),
            gs_set_shader_resources: slot!(
                gs_set_shader_resources(start_slot: u32, num_views: u32, views: *const *mut c_void)
            ),
            gs_set_samplers: slot!(
                gs_set_samplers(start_slot: u32, num_samplers: u32, samplers: *const *mut c_void)
            ),
            om_set_render_targets: slot!(
                om_set_render_targets(
                    num_views: u32,
                    render_target_views: *const *mut c_void,
                    depth_stencil_view: *mut c_void,
                )
            ),
            om_set_render_targets_and_unordered_access_views: slot!(
                om_set_render_targets_and_unordered_access_views(
                    num_rtvs: u32,
                    render_target_views: *const *mut c_void,
                    depth_stencil_view: *mut c_void,
                    uav_start_slot: u32,
                    num_uavs: u32,
                    unordered_access_views: *const *mut c_void,
                    uav_initial_counts: *const u32,
                )
            ),
            om_set_blend_state: slot!(
                om_set_blend_state(
                    blend_state: *mut c_void,
                    blend_factor: *const f32,
                    sample_mask: u32,
                )
            ),
            om_set_depth_stencil_state: slot!(
                om_set_depth_stencil_state(depth_stencil_state: *mut c_void, stencil_ref: u32)
            ),
            so_set_targets: slot!(
                so_set_targets(num_buffers: u32, targets: *const *mut c_void, offsets: *const u32)
            ),
            draw_auto: slot!(draw_auto()),
            draw_indexed_instanced_indirect: slot!(
                draw_indexed_instanced_indirect(
                    buffer_for_args: *mut c_void,
                    aligned_byte_offset_for_args: u32,
                )
            ),
            draw_instanced_indirect: slot!(
                draw_instanced_indirect(
                    buffer_for_args: *mut c_void,
                    aligned_byte_offset_for_args: u32,
                )
            ),
            dispatch: slot!(
                dispatch(
                    thread_group_count_x: u32,
                    thread_group_count_y: u32,
                    thread_group_count_z: u32,
                )
            ),
            dispatch_indirect: slot!(
                dispatch_indirect(buffer_for_args: *mut c_void, aligned_byte_offset_for_args: u32)
            ),
            rs_set_state: slot!(rs_set_state(rasterizer_state: *mut c_void)),
            rs_set_viewports: slot!(rs_set_viewports(num_viewports: u32, viewports: *const c_void)),
            rs_set_scissor_rects: slot!(rs_set_scissor_rects(num_rects: u32, rects: *const c_void)),
            copy_subresource_region: slot!(
                copy_subresource_region(
                    dst_resource: *mut c_void,
                    dst_subresource: u32,
                    dst_x: u32,
                    dst_y: u32,
                    dst_z: u32,
                    src_resource: *mut c_void,
                    src_subresource: u32,
                    src_box: *const c_void,
                )
            ),
            copy_resource: slot!(
                copy_resource(dst_resource: *mut c_void, src_resource: *mut c_void)
            ),
            update_subresource: slot!(
                update_subresource(
                    dst_resource: *mut c_void,
                    dst_subresource: u32,
                    dst_box: *const c_void,
                    src_data: *const c_void,
                    src_row_pitch: u32,
                    src_depth_pitch: u32,
                )
            ),
            copy_structure_count: slot!(
                copy_structure_count(
                    dst_buffer: *mut c_void,
                    dst_aligned_byte_offset: u32,
                    src_view: *mut c_void,
                )
            ),
            clear_render_target_view: slot!(
                clear_render_target_view(render_target_view: *mut c_void, color_rgba: *const f32)
            ),
            clear_unordered_access_view_uint: slot!(
                clear_unordered_access_view_uint(
                    unordered_access_view: *mut c_void,
                    values: *const u32,
                )
            ),
            clear_unordered_access_view_float: slot!(
                clear_unordered_access_view_float(
                    unordered_access_view: *mut c_void,
                    values: *const f32,
                )
            ),
            clear_depth_stencil_view: slot!(
                clear_depth_stencil_view(
                    depth_stencil_view: *mut c_void,
                    clear_flags: u32,
                    depth: f32,
                    stencil: u8,
                )
            ),
            generate_mips: slot!(generate_mips(shader_resource_view: *mut c_void)),
            set_resource_min_lod: slot!(set_resource_min_lod(resource: *mut c_void, min_lod: f32)),
            get_resource_min_lod: slot!(get_resource_min_lod(resource: *mut c_void) -> f32),
            resolve_subresource: slot!(
                resolve_subresource(
                    dst_resource: *mut c_void,
                    dst_subresource: u32,
                    src_resource: *mut c_void,
                    src_subresource: u32,
                    format: u32,
                )
            ),
            execute_command_list: slot!(
                execute_command_list(command_list: *mut c_void, restore_context_state: i32)
            ),
            hs_set_shader_resources: slot!(
                hs_set_shader_resources(start_slot: u32, num_views: u32, views: *const *mut c_void)
            ),
            hs_set_shader: slot!(
                hs_set_shader(
                    shader: *mut c_void,
                    class_instances: *const *mut c_void,
                    num_class_instances: u32,
                )
            ),
            hs_set_samplers: slot!(
                hs_set_samplers(start_slot: u32, num_samplers: u32, samplers: *const *mut c_void)
            ),
            hs_set_constant_buffers: slot!(
                hs_set_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                )
            ),
            ds_set_shader_resources: slot!(
                ds_set_shader_resources(start_slot: u32, num_views: u32, views: *const *mut c_void)
            ),
            ds_set_shader: slot!(
                ds_set_shader(
                    shader: *mut c_void,
                    class_instances: *const *mut c_void,
                    num_class_instances: u32,
                )
            ),
            ds_set_samplers: slot!(
                ds_set_samplers(start_slot: u32, num_samplers: u32, samplers: *const *mut c_void)
            ),
            ds_set_constant_buffers: slot!(
                ds_set_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                )
            ),
            cs_set_shader_resources: slot!(
                cs_set_shader_resources(start_slot: u32, num_views: u32, views: *const *mut c_void)
            ),
            cs_set_unordered_access_views: slot!(
                cs_set_unordered_access_views(
                    start_slot: u32,
                    num_uavs: u32,
                    unordered_access_views: *const *mut c_void,
                    uav_initial_counts: *const u32,
                )
            ),
            cs_set_shader: slot!(
                cs_set_shader(
                    shader: *mut c_void,
                    class_instances: *const *mut c_void,
                    num_class_instances: u32,
                )
            ),
            cs_set_samplers: slot!(
                cs_set_samplers(start_slot: u32, num_samplers: u32, samplers: *const *mut c_void)
            ),
            cs_set_constant_buffers: slot!(
                cs_set_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                )
            ),
            vs_get_constant_buffers: slot!(
                vs_get_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                )
            ),
            ps_get_shader_resources: slot!(
                ps_get_shader_resources(start_slot: u32, num_views: u32, views: *mut *mut c_void)
            ),
            ps_get_shader: slot!(
                ps_get_shader(
                    shader: *mut *mut c_void,
                    class_instances: *mut *mut c_void,
                    num_class_instances: *mut u32,
                )
            ),
            ps_get_samplers: slot!(
                ps_get_samplers(start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void)
            ),
            vs_get_shader: slot!(
                vs_get_shader(
                    shader: *mut *mut c_void,
                    class_instances: *mut *mut c_void,
                    num_class_instances: *mut u32,
                )
            ),
            ps_get_constant_buffers: slot!(
                ps_get_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                )
            ),
            ia_get_input_layout: slot!(ia_get_input_layout(input_layout: *mut *mut c_void)),
            ia_get_vertex_buffers: slot!(
                ia_get_vertex_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                    strides: *mut u32,
                    offsets: *mut u32,
                )
            ),
            ia_get_index_buffer: slot!(
                ia_get_index_buffer(
                    index_buffer: *mut *mut c_void,
                    format: *mut u32,
                    offset: *mut u32,
                )
            ),
            gs_get_constant_buffers: slot!(
                gs_get_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                )
            ),
            gs_get_shader: slot!(
                gs_get_shader(
                    shader: *mut *mut c_void,
                    class_instances: *mut *mut c_void,
                    num_class_instances: *mut u32,
                )
            ),
            ia_get_primitive_topology: slot!(ia_get_primitive_topology(topology: *mut u32)),
            vs_get_shader_resources: slot!(
                vs_get_shader_resources(start_slot: u32, num_views: u32, views: *mut *mut c_void)
            ),
            vs_get_samplers: slot!(
                vs_get_samplers(start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void)
            ),
            get_predication: slot!(
                get_predication(predicate: *mut *mut c_void, predicate_value: *mut i32)
            ),
            gs_get_shader_resources: slot!(
                gs_get_shader_resources(start_slot: u32, num_views: u32, views: *mut *mut c_void)
            ),
            gs_get_samplers: slot!(
                gs_get_samplers(start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void)
            ),
            om_get_render_targets: slot!(
                om_get_render_targets(
                    num_views: u32,
                    render_target_views: *mut *mut c_void,
                    depth_stencil_view: *mut *mut c_void,
                )
            ),
            om_get_render_targets_and_unordered_access_views: slot!(
                om_get_render_targets_and_unordered_access_views(
                    num_rtvs: u32,
                    render_target_views: *mut *mut c_void,
                    depth_stencil_view: *mut *mut c_void,
                    uav_start_slot: u32,
                    num_uavs: u32,
                    unordered_access_views: *mut *mut c_void,
                )
            ),
            om_get_blend_state: slot!(
                om_get_blend_state(
                    blend_state: *mut *mut c_void,
                    blend_factor: *mut f32,
                    sample_mask: *mut u32,
                )
            ),
            om_get_depth_stencil_state: slot!(
                om_get_depth_stencil_state(
                    depth_stencil_state: *mut *mut c_void,
                    stencil_ref: *mut u32,
                )
            ),
            so_get_targets: slot!(so_get_targets(num_buffers: u32, targets: *mut *mut c_void)),
            rs_get_state: slot!(rs_get_state(rasterizer_state: *mut *mut c_void)),
            rs_get_viewports: slot!(
                rs_get_viewports(num_viewports: *mut u32, viewports: *mut c_void)
            ),
            rs_get_scissor_rects: slot!(
                rs_get_scissor_rects(num_rects: *mut u32, rects: *mut c_void)
            ),
            hs_get_shader_resources: slot!(
                hs_get_shader_resources(start_slot: u32, num_views: u32, views: *mut *mut c_void)
            ),
            hs_get_shader: slot!(
                hs_get_shader(
                    shader: *mut *mut c_void,
                    class_instances: *mut *mut c_void,
                    num_class_instances: *mut u32,
                )
            ),
            hs_get_samplers: slot!(
                hs_get_samplers(start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void)
            ),
            hs_get_constant_buffers: slot!(
                hs_get_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                )
            ),
            ds_get_shader_resources: slot!(
                ds_get_shader_resources(start_slot: u32, num_views: u32, views: *mut *mut c_void)
            ),
            ds_get_shader: slot!(
                ds_get_shader(
                    shader: *mut *mut c_void,
                    class_instances: *mut *mut c_void,
                    num_class_instances: *mut u32,
                )
            ),
            ds_get_samplers: slot!(
                ds_get_samplers(start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void)
            ),
            ds_get_constant_buffers: slot!(
                ds_get_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                )
            ),
            cs_get_shader_resources: slot!(
                cs_get_shader_resources(start_slot: u32, num_views: u32, views: *mut *mut c_void)
            ),
            cs_get_unordered_access_views: slot!(
                cs_get_unordered_access_views(
                    start_slot: u32,
                    num_uavs: u32,
                    unordered_access_views: *mut *mut c_void,
                )
            ),
            cs_get_shader: slot!(
                cs_get_shader(
                    shader: *mut *mut c_void,
                    class_instances: *mut *mut c_void,
                    num_class_instances: *mut u32,
                )
            ),
            cs_get_samplers: slot!(
                cs_get_samplers(start_slot: u32, num_samplers: u32, samplers: *mut *mut c_void)
            ),
            cs_get_constant_buffers: slot!(
                cs_get_constant_buffers(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                )
            ),
            clear_state: slot!(clear_state()),
            flush: slot!(flush()),
            get_type: slot!(get_type() -> u32),
            get_context_flags: slot!(get_context_flags() -> u32),
            finish_command_list: slot!(
                finish_command_list(
                    restore_deferred_context_state: i32,
                    command_list: *mut *mut c_void,
                ) -> i32
            ),
            copy_subresource_region1: slot!(
                copy_subresource_region1(
                    dst_resource: *mut c_void,
                    dst_subresource: u32,
                    dst_x: u32,
                    dst_y: u32,
                    dst_z: u32,
                    src_resource: *mut c_void,
                    src_subresource: u32,
                    src_box: *const c_void,
                    copy_flags: u32,
                )
            ),
            update_subresource1: slot!(
                update_subresource1(
                    dst_resource: *mut c_void,
                    dst_subresource: u32,
                    dst_box: *const c_void,
                    src_data: *const c_void,
                    src_row_pitch: u32,
                    src_depth_pitch: u32,
                    copy_flags: u32,
                )
            ),
            discard_resource: slot!(discard_resource(resource: *mut c_void)),
            discard_view: slot!(discard_view(resource_view: *mut c_void)),
            vs_set_constant_buffers1: slot!(
                vs_set_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                    first_constant: *const u32,
                    num_constants: *const u32,
                )
            ),
            hs_set_constant_buffers1: slot!(
                hs_set_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                    first_constant: *const u32,
                    num_constants: *const u32,
                )
            ),
            ds_set_constant_buffers1: slot!(
                ds_set_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                    first_constant: *const u32,
                    num_constants: *const u32,
                )
            ),
            gs_set_constant_buffers1: slot!(
                gs_set_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                    first_constant: *const u32,
                    num_constants: *const u32,
                )
            ),
            ps_set_constant_buffers1: slot!(
                ps_set_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                    first_constant: *const u32,
                    num_constants: *const u32,
                )
            ),
            cs_set_constant_buffers1: slot!(
                cs_set_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *const *mut c_void,
                    first_constant: *const u32,
                    num_constants: *const u32,
                )
            ),
            vs_get_constant_buffers1: slot!(
                vs_get_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                    first_constant: *mut u32,
                    num_constants: *mut u32,
                )
            ),
            hs_get_constant_buffers1: slot!(
                hs_get_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                    first_constant: *mut u32,
                    num_constants: *mut u32,
                )
            ),
            ds_get_constant_buffers1: slot!(
                ds_get_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                    first_constant: *mut u32,
                    num_constants: *mut u32,
                )
            ),
            gs_get_constant_buffers1: slot!(
                gs_get_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                    first_constant: *mut u32,
                    num_constants: *mut u32,
                )
            ),
            ps_get_constant_buffers1: slot!(
                ps_get_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                    first_constant: *mut u32,
                    num_constants: *mut u32,
                )
            ),
            cs_get_constant_buffers1: slot!(
                cs_get_constant_buffers1(
                    start_slot: u32,
                    num_buffers: u32,
                    buffers: *mut *mut c_void,
                    first_constant: *mut u32,
                    num_constants: *mut u32,
                )
            ),
            swap_device_context_state: slot!(
                swap_device_context_state(state: *mut c_void, previous_state: *mut *mut c_void)
            ),
            clear_view: slot!(
                clear_view(
                    view: *mut c_void,
                    color: *const f32,
                    rect: *const c_void,
                    num_rects: u32,
                )
            ),
            discard_view1: slot!(
                discard_view1(resource_view: *mut c_void, rects: *const c_void, num_rects: u32)
            ),
            update_tile_mappings: slot!(
                update_tile_mappings(
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
                ) -> i32
            ),
            copy_tile_mappings: slot!(
                copy_tile_mappings(
                    dst_tiled_resource: *mut c_void,
                    dst_region_start_coordinate: *const c_void,
                    src_tiled_resource: *mut c_void,
                    src_region_start_coordinate: *const c_void,
                    region_size: *const c_void,
                    flags: u32,
                ) -> i32
            ),
            copy_tiles: slot!(
                copy_tiles(
                    tiled_resource: *mut c_void,
                    tile_region_start_coordinate: *const c_void,
                    tile_region_size: *const c_void,
                    buffer: *mut c_void,
                    buffer_start_offset_in_bytes: u64,
                    flags: u32,
                )
            ),
            update_tiles: slot!(
                update_tiles(
                    dst_tiled_resource: *mut c_void,
                    dst_tile_region_start_coordinate: *const c_void,
                    dst_tile_region_size: *const c_void,
                    src_tile_data: *const c_void,
                    flags: u32,
                )
            ),
            resize_tile_pool: slot!(
                resize_tile_pool(tile_pool: *mut c_void, new_size_in_bytes: u64) -> i32
            ),
            tiled_resource_barrier: slot!(
                tiled_resource_barrier(
                    tiled_resource_or_view_access_before_barrier: *mut c_void,
                    tiled_resource_or_view_access_after_barrier: *mut c_void,
                )
            ),
            is_annotation_enabled: slot!(is_annotation_enabled() -> i32),
            set_marker_int: slot!(set_marker_int(label: *const c_void, data: i32)),
            begin_event_int: slot!(begin_event_int(label: *const c_void, data: i32)),
            end_event: slot!(end_event()),
            flush1: slot!(flush1(context_type: u32, event: *mut c_void)),
            set_hardware_protection_state: slot!(
                set_hardware_protection_state(hw_protection_enable: i32)
            ),
            get_hardware_protection_state: slot!(
                get_hardware_protection_state(hw_protection_enable: *mut i32)
            ),
            signal: slot!(signal(fence: *mut c_void, value: u64) -> i32),
            wait: slot!(wait(fence: *mut c_void, value: u64) -> i32),
        },
        wait_until_gpu_idle: slot!(wait_until_gpu_idle()),
        prepare_flush: slot!(prepare_flush()),
        commit: slot!(commit()),
        so_get_targets_with_offsets: slot!(
            so_get_targets_with_offsets(
                num_buffers: u32,
                targets: *mut *mut c_void,
                offsets: *mut u32,
            )
        ),
        acquire_sync: slot!(acquire_sync(sync_resource: *mut c_void, key: u64)),
        release_sync: slot!(release_sync(sync_resource: *mut c_void, key: u64)),
    }
}

/// Static vtable for [`Context<Immediate>`] (`IMTLD3D11DeviceContext`).
pub static IMMEDIATE_VTBL: IMTLD3D11DeviceContextVtbl = context_vtbl::<Immediate>();

/// Static vtable for [`Context<Deferred>`] (`ID3D11DeviceContext4`).
pub static DEFERRED_VTBL: ID3D11DeviceContext4Vtbl = context_vtbl::<Deferred>().base;

/// Builds an [`IMTLD3D11ContextExt2Vtbl`] for `Context<M>::ext_vtbl`.
#[must_use]
pub const fn ext_vtbl<M: Mode>() -> IMTLD3D11ContextExt2Vtbl {
    unsafe extern "system-unwind" fn query_interface<M: Mode>(
        this: *mut c_void,
        riid: *const c_void,
        out: *mut *mut c_void,
    ) -> i32 {
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &mut *container_of!(this, Context<M>, ext_vtbl) };
        ctx.query_interface(riid, out)
    }

    unsafe extern "system-unwind" fn add_ref<M: Mode>(this: *mut c_void) -> u32 {
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &*container_of!(this, Context<M>, ext_vtbl) };
        ctx.add_ref()
    }

    unsafe extern "system-unwind" fn release<M: Mode>(this: *mut c_void) -> u32 {
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &*container_of!(this, Context<M>, ext_vtbl) };
        ctx.release()
    }

    unsafe extern "system-unwind" fn temporal_upscale<M: Mode>(
        this: *mut c_void,
        desc: *const D3D11UpscaleDesc,
    ) {
        let raw = container_of!(this, Context<M>, ext_vtbl);
        let _guard = M::lock_iface(raw.cast());
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &mut *raw };
        ctx.temporal_upscale(desc);
    }

    unsafe extern "system-unwind" fn begin_uav_overlap<M: Mode>(this: *mut c_void) {
        let raw = container_of!(this, Context<M>, ext_vtbl);
        let _guard = M::lock_iface(raw.cast());
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &mut *raw };
        ctx.begin_uav_overlap();
    }

    unsafe extern "system-unwind" fn end_uav_overlap<M: Mode>(this: *mut c_void) {
        let raw = container_of!(this, Context<M>, ext_vtbl);
        let _guard = M::lock_iface(raw.cast());
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &mut *raw };
        ctx.end_uav_overlap();
    }

    unsafe extern "system-unwind" fn check_feature_support<M: Mode>(
        this: *mut c_void,
        feature: u32,
        feature_data: *mut c_void,
        feature_data_size: u32,
    ) -> i32 {
        let raw = container_of!(this, Context<M>, ext_vtbl);
        let _guard = M::lock_iface(raw.cast());
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &mut *raw };
        ctx.check_ext_feature_support(feature, feature_data, feature_data_size)
    }

    unsafe extern "system-unwind" fn get_pieces<M: Mode>(
        this: *mut c_void,
        resource: *mut c_void,
        subresource: u32,
        count: u32,
        pieces: *mut D3D11Piece,
    ) -> i32 {
        let raw = container_of!(this, Context<M>, ext_vtbl);
        let _guard = M::lock_iface(raw.cast());
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &mut *raw };
        ctx.get_pieces(resource, subresource, count, pieces)
    }

    unsafe extern "system-unwind" fn use_piece<M: Mode>(
        this: *mut c_void,
        resource: *mut c_void,
        subresource: u32,
        piece_id: u64,
    ) {
        let raw = container_of!(this, Context<M>, ext_vtbl);
        let _guard = M::lock_iface(raw.cast());
        // SAFETY: `this` points to `Context::<M>::ext_vtbl`.
        let ctx = unsafe { &mut *raw };
        ctx.use_piece(resource, subresource, piece_id);
    }

    IMTLD3D11ContextExt2Vtbl {
        base: IMTLD3D11ContextExt1Vtbl {
            base: IMTLD3D11ContextExtVtbl {
                unknown: IUnknownVtbl {
                    query_interface: query_interface::<M>,
                    add_ref: add_ref::<M>,
                    release: release::<M>,
                },
                temporal_upscale: temporal_upscale::<M>,
                begin_uav_overlap: begin_uav_overlap::<M>,
                end_uav_overlap: end_uav_overlap::<M>,
            },
            check_feature_support: check_feature_support::<M>,
        },
        get_pieces: get_pieces::<M>,
        use_piece: use_piece::<M>,
    }
}

/// Builds an [`ID3DUserDefinedAnnotationVtbl`] for `Context<M>::annotation_vtbl`.
#[must_use]
pub const fn annotation_vtbl<M: Mode>() -> ID3DUserDefinedAnnotationVtbl {
    unsafe extern "system-unwind" fn query_interface<M: Mode>(
        this: *mut c_void,
        riid: *const c_void,
        out: *mut *mut c_void,
    ) -> i32 {
        // SAFETY: `this` points to `Context::<M>::annotation_vtbl`.
        let ctx = unsafe { &mut *container_of!(this, Context<M>, annotation_vtbl) };
        ctx.query_interface(riid, out)
    }

    unsafe extern "system-unwind" fn add_ref<M: Mode>(this: *mut c_void) -> u32 {
        // SAFETY: `this` points to `Context::<M>::annotation_vtbl`.
        let ctx = unsafe { &*container_of!(this, Context<M>, annotation_vtbl) };
        ctx.add_ref()
    }

    unsafe extern "system-unwind" fn release<M: Mode>(this: *mut c_void) -> u32 {
        // SAFETY: `this` points to `Context::<M>::annotation_vtbl`.
        let ctx = unsafe { &*container_of!(this, Context<M>, annotation_vtbl) };
        ctx.release()
    }

    const unsafe extern "system-unwind" fn begin_event(
        _this: *mut c_void,
        _name: *const c_void,
    ) -> i32 {
        -1
    }

    const unsafe extern "system-unwind" fn end_event(_this: *mut c_void) -> i32 {
        -1
    }

    const unsafe extern "system-unwind" fn set_marker(_this: *mut c_void, _name: *const c_void) {}

    const unsafe extern "system-unwind" fn get_status(_this: *mut c_void) -> i32 {
        0
    }

    ID3DUserDefinedAnnotationVtbl {
        query_interface: query_interface::<M>,
        add_ref: add_ref::<M>,
        release: release::<M>,
        begin_event,
        end_event,
        set_marker,
        get_status,
    }
}

/// Static `IMTLD3D11ContextExt2` vtable for [`Context<Immediate>`].
pub static IMMEDIATE_EXT_VTBL: IMTLD3D11ContextExt2Vtbl = ext_vtbl::<Immediate>();

/// Static `IMTLD3D11ContextExt2` vtable for [`Context<Deferred>`].
pub static DEFERRED_EXT_VTBL: IMTLD3D11ContextExt2Vtbl = ext_vtbl::<Deferred>();

/// Static `ID3DUserDefinedAnnotation` vtable for [`Context<Immediate>`].
pub static IMMEDIATE_ANNOTATION_VTBL: ID3DUserDefinedAnnotationVtbl =
    annotation_vtbl::<Immediate>();

/// Static `ID3DUserDefinedAnnotation` vtable for [`Context<Deferred>`].
pub static DEFERRED_ANNOTATION_VTBL: ID3DUserDefinedAnnotationVtbl = annotation_vtbl::<Deferred>();
