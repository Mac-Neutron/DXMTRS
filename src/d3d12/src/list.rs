//! `ID3D12GraphicsCommandList7` and `IMTLD3D12CommandListExt` (`d3d12_command_list.cpp`,
//!
//! `d3d12_device.hpp:68-95`): the command list struct, recording state, encoder open/close seams,
//! and vtable dispatch.

pub mod barrier;
pub mod clear;
pub mod copy;
pub mod draw;
pub mod indirect;
pub mod state;
pub mod submission;
pub mod upscale;

use core::{ffi::c_void, mem::offset_of, ptr};

use bitflags::bitflags;
use dxmt_util::{com::ComObject, private_data::PrivateData};
use winemetal_wire::{WMT_INDEX_TYPE_U_INT16, WMTScissorRect, WMTViewport};

use crate::{
    abi::{
        ext::{D3D12CommandListExtVtbl, D3D12UpscaleDesc},
        vtbl::{
            D3d12CpuDescriptorHandle, D3d12GpuDescriptorHandle, ID3D12GraphicsCommandList7Vtbl,
            IUnknownVtbl,
        },
    },
    allocator::Allocator,
    com::{self, ComRefCounted, Hresult},
    descriptor_heap,
    device::Device,
    encoder::{
        ClearEncoderData, EncoderData, EncoderSample, EncoderType,
        RenderEncoderColorAttachmentData, RenderEncoderData, RenderEncoderDepthAttachmentData,
        RenderEncoderStencilAttachmentData, Submission, TextureViewRef,
    },
    pipeline::{Compute, Graphics},
    root_signature::object::RootSignature,
    slot,
};

bitflags! {
    /// Dirty state flags tracked during command list recording (`DirtyFlags`,
    ///
    /// `d3d12_command_list.cpp:80-95`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct DirtyFlags: u32 {
        const PSO = 1 << 0;
        const VIEWPORT = 1 << 1;
        const SCISSOR = 1 << 2;
        const BLEND_FACTOR = 1 << 3;
        const STENCIL_REF = 1 << 4;
        const VERTEX_BUFFERS = 1 << 5;
        const INDEX_BUFFER = 1 << 6;
        const STATIC_SAMPLERS = 1 << 7;
        const COMPUTE_STATIC_SAMPLERS = 1 << 8;
        const ROOT_ARGS = 1 << 9;
        const COMPUTE_ROOT_ARGS = 1 << 10;
        const DESCRIPTOR_HEAP = 1 << 11;
        const VISIBILITY_BUFFER = 1 << 12;
        const DEPTH_BOUNDS = 1 << 13;
    }
}

/// Result of [`List::pre_draw`] / [`List::pre_dispatch`] (`DrawCallStatus`,
///
/// `d3d12_command_list.cpp:43`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawCallStatus {
    Invalid,
    Valid,
    Geometry,
}

/// Deferred MSAA resolve recorded inside a render pass (`PendingResolve`,
///
/// `d3d12_command_list.cpp:97-101`).
#[derive(Clone)]
pub struct PendingResolve {
    pub src_view: Option<TextureViewRef>,
    pub src_x: u32,
    pub src_y: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub dst_view: Option<TextureViewRef>,
    pub dst_x: u32,
    pub dst_y: u32,
    pub same_size: bool,
}

/// Retained COM object pointer and its private release thunk (`retained_objects_`).
pub type RetainedObject = (*mut c_void, unsafe fn(*mut c_void));

/// `MTLD3D12GraphicsCommandListImpl` (`d3d12_command_list.cpp:41-190`).
///
/// Primary vtable pointer (`ID3D12GraphicsCommandList7`) sits at offset 0; the companion
/// `IMTLD3D12CommandListExt` vtable pointer sits at [`List::EXT_OFFSET`].
#[repr(C)]
pub struct List {
    pub vtbl: &'static ID3D12GraphicsCommandList7Vtbl,
    pub ext_vtbl: &'static D3D12CommandListExtVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub allocator: *mut Allocator,
    pub submission: Submission,
    pub last_timestamp: Option<EncoderSample>,
    pub pso_graphics: *const Graphics,
    pub pso_compute: *const Compute,
    pub root_sig_graphics: *const RootSignature,
    pub root_sig_compute: *const RootSignature,
    pub render_pass: RenderEncoderData,
    pub deferred_clears: Vec<ClearEncoderData>,
    pub pending_resolves: Vec<PendingResolve>,
    pub active_visibility_offset: u64,
    pub render_pass_visibility_buffer: u64,
    pub root_args_graphics: [u64; 64],
    pub root_args_compute: [u64; 64],
    pub bound_cbv_heap: *const descriptor_heap::Heap,
    pub bound_sampler_heap: *const descriptor_heap::Heap,
    pub viewports: [WMTViewport; 16],
    pub viewport_count: u8,
    pub scissor_rects: [WMTScissorRect; 16],
    pub scissor_rect_count: u8,
    pub primitive_topology: u32,
    pub bound_pso: u64,
    pub bound_geometry: bool,
    pub vertex_buffers: [(u64, u32, u32); 32],
    pub index_buffer: u64,
    pub index_buffer_offset: u64,
    pub index_buffer_size: u64,
    pub index_buffer_format: u32,
    pub sm50_index_buffer_format: u32,
    pub blend_factor: [f32; 4],
    pub stencil_ref: u32,
    pub dirty_state: DirtyFlags,
    pub retained_objects: Vec<RetainedObject>,
    pub inside_render_pass: bool,
}

impl ComRefCounted for List {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl List {
    /// Byte offset of `ext_vtbl` inside `List` (`IMTLD3D12CommandListExt` companion pointer).
    pub const EXT_OFFSET: usize = offset_of!(Self, ext_vtbl);

    /// Creates an empty `RenderEncoderData` staging state (`render_pass_`).
    #[must_use]
    pub fn empty_render_pass() -> RenderEncoderData {
        let mut base = EncoderData::new(EncoderType::Render, 0);
        base.writes_unknown = false;
        RenderEncoderData {
            base,
            colors: core::array::from_fn(|_| RenderEncoderColorAttachmentData::default()),
            depth: RenderEncoderDepthAttachmentData::default(),
            stencil: RenderEncoderStencilAttachmentData::default(),
            default_raster_sample_count: 0,
            render_target_array_length: 0,
            render_target_height: 0,
            render_target_width: 0,
            // SAFETY: `WmtcmdRenderNop` is a `#[repr(C)]` plain-old-data wire struct valid when zeroed.
            cmd_head: unsafe { core::mem::zeroed() },
            cmd_tail: ptr::null_mut(),
            dsv_planar_flags: 0,
            dsv_readonly_flags: 0,
            render_target_count: 0,
            use_visibility_result: false,
            visibility_buffer: 0,
            use_tessellation: false,
            use_geometry: false,
            uav_writes: false,
            // SAFETY: `WmtcmdComputeNop` is a `#[repr(C)]` plain-old-data wire struct valid when zeroed.
            pre_head: unsafe { core::mem::zeroed() },
            pre_tail: ptr::null_mut(),
        }
    }

    /// Creates a new `List` bound to `device` in the closed state (`allocator = null`).
    #[must_use]
    pub fn new_closed(device: *mut Device) -> Box<Self> {
        Box::new(Self {
            vtbl: &LIST_VTBL,
            ext_vtbl: &LIST_EXT_VTBL,
            com: ComObject::new(),
            private_data: PrivateData::default(),
            device,
            allocator: ptr::null_mut(),
            submission: Submission::new(),
            last_timestamp: None,
            pso_graphics: ptr::null(),
            pso_compute: ptr::null(),
            root_sig_graphics: ptr::null(),
            root_sig_compute: ptr::null(),
            render_pass: Self::empty_render_pass(),
            deferred_clears: Vec::new(),
            pending_resolves: Vec::new(),
            active_visibility_offset: u64::MAX,
            render_pass_visibility_buffer: 0,
            root_args_graphics: [0; 64],
            root_args_compute: [0; 64],
            bound_cbv_heap: ptr::null(),
            bound_sampler_heap: ptr::null(),
            viewports: [WMTViewport {
                origin_x: 0.0,
                origin_y: 0.0,
                width: 0.0,
                height: 0.0,
                znear: 0.0,
                zfar: 1.0,
            }; 16],
            viewport_count: 0,
            scissor_rects: [WMTScissorRect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            }; 16],
            scissor_rect_count: 0,
            primitive_topology: 0,
            bound_pso: 0,
            bound_geometry: false,
            vertex_buffers: [(0, 0, 0); 32],
            index_buffer: 0,
            index_buffer_offset: 0,
            index_buffer_size: 0,
            index_buffer_format: u32::from(WMT_INDEX_TYPE_U_INT16),
            sm50_index_buffer_format: 0,
            blend_factor: [1.0; 4],
            stencil_ref: 0,
            dirty_state: DirtyFlags::empty(),
            retained_objects: Vec::new(),
            inside_render_pass: false,
        })
    }

    /// Closed command-list fields read by the command queue (`d3d12_device.hpp:68-95`).
    #[must_use]
    pub const fn submission(&self) -> &Submission {
        &self.submission
    }

    /// Mutable access to the submission fields during recording (`Close`/`Reset`).
    pub const fn submission_mut(&mut self) -> &mut Submission {
        &mut self.submission
    }

    /// Flushes dirty graphics state before a draw or graphics `ExecuteIndirect` (`PreDraw`,
    ///
    /// `d3d12_command_list.cpp:1428`).
    pub fn pre_draw(&mut self) -> DrawCallStatus {
        draw::pre_draw(self)
    }

    /// Flushes dirty compute state before a dispatch or compute `ExecuteIndirect` (`PreDispatch`,
    ///
    /// `d3d12_command_list.cpp:1661`).
    pub fn pre_dispatch(&mut self) -> DrawCallStatus {
        draw::pre_dispatch(self)
    }
}

/// Casts a non-null `ID3D12CommandList` pointer if its vtable matches [`List`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_list<'a>(raw: *mut c_void) -> Option<&'a List> {
    // SAFETY: If non-null, `raw` points to a COM object whose first word is its vtable pointer.
    if unsafe { com::has_vtbl(raw, &LIST_VTBL) } {
        // SAFETY: `raw`'s vtable is `LIST_VTBL`, so `raw` is a live `List`.
        Some(unsafe { &*raw.cast::<List>() })
    } else {
        None
    }
}

/// Primary `ID3D12GraphicsCommandList7` vtable for [`List`].
pub static LIST_VTBL: ID3D12GraphicsCommandList7Vtbl = ID3D12GraphicsCommandList7Vtbl {
    query_interface: slot!(
        (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::query_interface(this, riid, ppv_object) }
        }
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `List` COM pointer.
        unsafe { com::add_ref_com(this.cast::<List>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `List` COM pointer.
        unsafe { com::release_com(this.cast::<List>()) }
    }),
    get_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::get_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::set_private_data(this, guid, data_size, data) }
        }
    ),
    set_private_data_interface: slot!(
        (this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::set_private_data_interface(this, guid, data) }
        }
    ),
    set_name: slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::set_name(this, name) }
    }),
    get_device: slot!(
        (this: *mut c_void, riid: *const c_void, device: *mut *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::get_device(this, riid, device) }
        }
    ),
    get_type: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::get_type(this) }
    }),
    close: slot!((this: *mut c_void) -> Hresult => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::close(this) }
    }),
    reset: slot!(
        (this: *mut c_void, allocator: *mut c_void, initial_state: *mut c_void) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::reset(this, allocator, initial_state) }
        }
    ),
    clear_state: slot!((this: *mut c_void, pipeline_state: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::clear_state(this, pipeline_state) }
    }),
    draw_instanced: slot!(
        (
            this: *mut c_void,
            vertex_count_per_instance: u32,
            instance_count: u32,
            start_vertex_location: u32,
            start_instance_location: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                draw::draw_instanced(
                    this,
                    vertex_count_per_instance,
                    instance_count,
                    start_vertex_location,
                    start_instance_location,
                );
            }
        }
    ),
    draw_indexed_instanced: slot!(
        (
            this: *mut c_void,
            index_count_per_instance: u32,
            instance_count: u32,
            start_index_location: u32,
            base_vertex_location: i32,
            start_instance_location: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                draw::draw_indexed_instanced(
                    this,
                    index_count_per_instance,
                    instance_count,
                    start_index_location,
                    base_vertex_location,
                    start_instance_location,
                );
            }
        }
    ),
    dispatch: slot!(
        (this: *mut c_void, x: u32, y: u32, z: u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { draw::dispatch(this, x, y, z) }
        }
    ),
    copy_buffer_region: slot!(
        (
            this: *mut c_void,
            dst_buffer: *mut c_void,
            dst_offset: u64,
            src_buffer: *mut c_void,
            src_offset: u64,
            num_bytes: u64
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                copy::copy_buffer_region(
                    this,
                    dst_buffer,
                    dst_offset,
                    src_buffer,
                    src_offset,
                    num_bytes,
                );
            }
        }
    ),
    copy_texture_region: slot!(
        (
            this: *mut c_void,
            dst: *const c_void,
            dst_x: u32,
            dst_y: u32,
            dst_z: u32,
            src: *const c_void,
            src_box: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { copy::copy_texture_region(this, dst, dst_x, dst_y, dst_z, src, src_box) }
        }
    ),
    copy_resource: slot!(
        (this: *mut c_void, dst_resource: *mut c_void, src_resource: *mut c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { copy::copy_resource(this, dst_resource, src_resource) }
        }
    ),
    copy_tiles: slot!(
        (
            this: *mut c_void,
            tiled_resource: *mut c_void,
            tile_region_start_coord: *const c_void,
            tile_region_size: *const c_void,
            buffer: *mut c_void,
            buffer_start_offset_in_bytes: u64,
            flags: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                copy::copy_tiles(
                    this,
                    tiled_resource,
                    tile_region_start_coord,
                    tile_region_size,
                    buffer,
                    buffer_start_offset_in_bytes,
                    flags,
                );
            }
        }
    ),
    resolve_subresource: slot!(
        (
            this: *mut c_void,
            dst_resource: *mut c_void,
            dst_subresource: u32,
            src_resource: *mut c_void,
            src_subresource: u32,
            format: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                copy::resolve_subresource(
                    this,
                    dst_resource,
                    dst_subresource,
                    src_resource,
                    src_subresource,
                    format,
                );
            }
        }
    ),
    ia_set_primitive_topology: slot!((this: *mut c_void, topology: u32) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::ia_set_primitive_topology(this, topology) }
    }),
    rs_set_viewports: slot!(
        (this: *mut c_void, num_viewports: u32, viewports: *const c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::rs_set_viewports(this, num_viewports, viewports) }
        }
    ),
    rs_set_scissor_rects: slot!(
        (this: *mut c_void, num_rects: u32, rects: *const c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::rs_set_scissor_rects(this, num_rects, rects) }
        }
    ),
    om_set_blend_factor: slot!((this: *mut c_void, blend_factor: *const f32) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::om_set_blend_factor(this, blend_factor) }
    }),
    om_set_stencil_ref: slot!((this: *mut c_void, stencil_ref: u32) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::om_set_stencil_ref(this, stencil_ref) }
    }),
    set_pipeline_state: slot!((this: *mut c_void, pipeline_state: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::set_pipeline_state(this, pipeline_state) }
    }),
    resource_barrier: slot!(
        (this: *mut c_void, num_barriers: u32, barriers: *const c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { barrier::resource_barrier(this, num_barriers, barriers) }
        }
    ),
    execute_bundle: slot!((this: *mut c_void, command_list: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::execute_bundle(this, command_list) }
    }),
    set_descriptor_heaps: slot!(
        (this: *mut c_void, num_descriptor_heaps: u32, descriptor_heaps: *const *mut c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::set_descriptor_heaps(this, num_descriptor_heaps, descriptor_heaps) }
        }
    ),
    set_compute_root_signature: slot!((this: *mut c_void, root_signature: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::set_compute_root_signature(this, root_signature) }
    }),
    set_graphics_root_signature: slot!((this: *mut c_void, root_signature: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::set_graphics_root_signature(this, root_signature) }
    }),
    set_compute_root_descriptor_table: slot!(
        (this: *mut c_void, root_parameter_index: u32, base_descriptor: D3d12GpuDescriptorHandle) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_compute_root_descriptor_table(this, root_parameter_index, base_descriptor);
            }
        }
    ),
    set_graphics_root_descriptor_table: slot!(
        (this: *mut c_void, root_parameter_index: u32, base_descriptor: D3d12GpuDescriptorHandle) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_graphics_root_descriptor_table(
                    this,
                    root_parameter_index,
                    base_descriptor,
                );
            }
        }
    ),
    set_compute_root32_bit_constant: slot!(
        (
            this: *mut c_void,
            root_parameter_index: u32,
            src_data: u32,
            dest_offset_in32_bit_values: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_compute_root32_bit_constant(
                    this,
                    root_parameter_index,
                    src_data,
                    dest_offset_in32_bit_values,
                );
            }
        }
    ),
    set_graphics_root32_bit_constant: slot!(
        (
            this: *mut c_void,
            root_parameter_index: u32,
            src_data: u32,
            dest_offset_in32_bit_values: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_graphics_root32_bit_constant(
                    this,
                    root_parameter_index,
                    src_data,
                    dest_offset_in32_bit_values,
                );
            }
        }
    ),
    set_compute_root32_bit_constants: slot!(
        (
            this: *mut c_void,
            root_parameter_index: u32,
            num32_bit_values_to_set: u32,
            src_data: *const c_void,
            dest_offset_in32_bit_values: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_compute_root32_bit_constants(
                    this,
                    root_parameter_index,
                    num32_bit_values_to_set,
                    src_data,
                    dest_offset_in32_bit_values,
                );
            }
        }
    ),
    set_graphics_root32_bit_constants: slot!(
        (
            this: *mut c_void,
            root_parameter_index: u32,
            num32_bit_values_to_set: u32,
            src_data: *const c_void,
            dest_offset_in32_bit_values: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_graphics_root32_bit_constants(
                    this,
                    root_parameter_index,
                    num32_bit_values_to_set,
                    src_data,
                    dest_offset_in32_bit_values,
                );
            }
        }
    ),
    set_compute_root_constant_buffer_view: slot!(
        (this: *mut c_void, root_parameter_index: u32, buffer_location: u64) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_compute_root_constant_buffer_view(
                    this,
                    root_parameter_index,
                    buffer_location,
                );
            }
        }
    ),
    set_graphics_root_constant_buffer_view: slot!(
        (this: *mut c_void, root_parameter_index: u32, buffer_location: u64) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_graphics_root_constant_buffer_view(
                    this,
                    root_parameter_index,
                    buffer_location,
                );
            }
        }
    ),
    set_compute_root_shader_resource_view: slot!(
        (this: *mut c_void, root_parameter_index: u32, buffer_location: u64) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_compute_root_shader_resource_view(
                    this,
                    root_parameter_index,
                    buffer_location,
                );
            }
        }
    ),
    set_graphics_root_shader_resource_view: slot!(
        (this: *mut c_void, root_parameter_index: u32, buffer_location: u64) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_graphics_root_shader_resource_view(
                    this,
                    root_parameter_index,
                    buffer_location,
                );
            }
        }
    ),
    set_compute_root_unordered_access_view: slot!(
        (this: *mut c_void, root_parameter_index: u32, buffer_location: u64) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_compute_root_unordered_access_view(
                    this,
                    root_parameter_index,
                    buffer_location,
                );
            }
        }
    ),
    set_graphics_root_unordered_access_view: slot!(
        (this: *mut c_void, root_parameter_index: u32, buffer_location: u64) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_graphics_root_unordered_access_view(
                    this,
                    root_parameter_index,
                    buffer_location,
                );
            }
        }
    ),
    ia_set_index_buffer: slot!((this: *mut c_void, view: *const c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::ia_set_index_buffer(this, view) }
    }),
    ia_set_vertex_buffers: slot!(
        (this: *mut c_void, start_slot: u32, num_views: u32, views: *const c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::ia_set_vertex_buffers(this, start_slot, num_views, views) }
        }
    ),
    so_set_targets: slot!(
        (this: *mut c_void, start_slot: u32, num_views: u32, views: *const c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::so_set_targets(this, start_slot, num_views, views) }
        }
    ),
    om_set_render_targets: slot!(
        (
            this: *mut c_void,
            num_render_target_descriptors: u32,
            render_target_descriptors: *const c_void,
            rts_single_handle_to_descriptor_range: i32,
            depth_stencil_descriptor: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::om_set_render_targets(
                    this,
                    num_render_target_descriptors,
                    render_target_descriptors,
                    rts_single_handle_to_descriptor_range,
                    depth_stencil_descriptor,
                );
            }
        }
    ),
    clear_depth_stencil_view: slot!(
        (
            this: *mut c_void,
            depth_stencil_view: D3d12CpuDescriptorHandle,
            clear_flags: u32,
            depth: f32,
            stencil: u8,
            num_rects: u32,
            rects: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                clear::clear_depth_stencil_view(
                    this,
                    depth_stencil_view,
                    clear_flags,
                    depth,
                    stencil,
                    num_rects,
                    rects,
                );
            }
        }
    ),
    clear_render_target_view: slot!(
        (
            this: *mut c_void,
            render_target_view: D3d12CpuDescriptorHandle,
            color_rgba: *const f32,
            num_rects: u32,
            rects: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                clear::clear_render_target_view(
                    this,
                    render_target_view,
                    color_rgba,
                    num_rects,
                    rects,
                );
            }
        }
    ),
    clear_unordered_access_view_uint: slot!(
        (
            this: *mut c_void,
            view_gpu_handle_in_current_heap: D3d12GpuDescriptorHandle,
            view_cpu_handle: D3d12CpuDescriptorHandle,
            resource: *mut c_void,
            values: *const u32,
            num_rects: u32,
            rects: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                clear::clear_unordered_access_view_uint(
                    this,
                    view_gpu_handle_in_current_heap,
                    view_cpu_handle,
                    resource,
                    values,
                    num_rects,
                    rects,
                );
            }
        }
    ),
    clear_unordered_access_view_float: slot!(
        (
            this: *mut c_void,
            view_gpu_handle_in_current_heap: D3d12GpuDescriptorHandle,
            view_cpu_handle: D3d12CpuDescriptorHandle,
            resource: *mut c_void,
            values: *const f32,
            num_rects: u32,
            rects: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                clear::clear_unordered_access_view_float(
                    this,
                    view_gpu_handle_in_current_heap,
                    view_cpu_handle,
                    resource,
                    values,
                    num_rects,
                    rects,
                );
            }
        }
    ),
    discard_resource: slot!(
        (this: *mut c_void, resource: *mut c_void, region: *const c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { clear::discard_resource(this, resource, region) }
        }
    ),
    begin_query: slot!(
        (this: *mut c_void, query_heap: *mut c_void, query_type: u32, index: u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { copy::begin_query(this, query_heap, query_type, index) }
        }
    ),
    end_query: slot!(
        (this: *mut c_void, query_heap: *mut c_void, query_type: u32, index: u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { copy::end_query(this, query_heap, query_type, index) }
        }
    ),
    resolve_query_data: slot!(
        (
            this: *mut c_void,
            query_heap: *mut c_void,
            query_type: u32,
            start_index: u32,
            num_queries: u32,
            destination_buffer: *mut c_void,
            aligned_destination_buffer_offset: u64
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                copy::resolve_query_data(
                    this,
                    query_heap,
                    query_type,
                    start_index,
                    num_queries,
                    destination_buffer,
                    aligned_destination_buffer_offset,
                );
            }
        }
    ),
    set_predication: slot!(
        (this: *mut c_void, buffer: *mut c_void, aligned_buffer_offset: u64, operation: u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::set_predication(this, buffer, aligned_buffer_offset, operation) }
        }
    ),
    set_marker: slot!(
        (this: *mut c_void, metadata: u32, data: *const c_void, size: u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::set_marker(this, metadata, data, size) }
        }
    ),
    begin_event: slot!(
        (this: *mut c_void, metadata: u32, data: *const c_void, size: u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::begin_event(this, metadata, data, size) }
        }
    ),
    end_event: slot!((this: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::end_event(this) }
    }),
    execute_indirect: slot!(
        (
            this: *mut c_void,
            command_signature: *mut c_void,
            max_command_count: u32,
            argument_buffer: *mut c_void,
            argument_buffer_offset: u64,
            count_buffer: *mut c_void,
            count_buffer_offset: u64
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                indirect::execute_indirect(
                    this,
                    command_signature,
                    max_command_count,
                    argument_buffer,
                    argument_buffer_offset,
                    count_buffer,
                    count_buffer_offset,
                );
            }
        }
    ),
    atomic_copy_buffer_uint: slot!(
        (
            this: *mut c_void,
            dst_buffer: *mut c_void,
            dst_offset: u64,
            src_buffer: *mut c_void,
            src_offset: u64,
            dependencies: u32,
            dependent_resources: *const *mut c_void,
            dependent_subresource_ranges: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                copy::atomic_copy_buffer_uint(
                    this,
                    dst_buffer,
                    dst_offset,
                    src_buffer,
                    src_offset,
                    dependencies,
                    dependent_resources,
                    dependent_subresource_ranges,
                );
            }
        }
    ),
    atomic_copy_buffer_uint64: slot!(
        (
            this: *mut c_void,
            dst_buffer: *mut c_void,
            dst_offset: u64,
            src_buffer: *mut c_void,
            src_offset: u64,
            dependencies: u32,
            dependent_resources: *const *mut c_void,
            dependent_subresource_ranges: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                copy::atomic_copy_buffer_uint64(
                    this,
                    dst_buffer,
                    dst_offset,
                    src_buffer,
                    src_offset,
                    dependencies,
                    dependent_resources,
                    dependent_subresource_ranges,
                );
            }
        }
    ),
    om_set_depth_bounds: slot!((this: *mut c_void, min: f32, max: f32) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::om_set_depth_bounds(this, min, max) }
    }),
    set_sample_positions: slot!(
        (
            this: *mut c_void,
            num_samples_per_pixel: u32,
            num_pixels: u32,
            sample_positions: *mut c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                state::set_sample_positions(
                    this,
                    num_samples_per_pixel,
                    num_pixels,
                    sample_positions,
                );
            }
        }
    ),
    resolve_subresource_region: slot!(
        (
            this: *mut c_void,
            dst_resource: *mut c_void,
            dst_subresource: u32,
            dst_x: u32,
            dst_y: u32,
            src_resource: *mut c_void,
            src_subresource: u32,
            src_rect: *mut c_void,
            format: u32,
            resolve_mode: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                copy::resolve_subresource_region(
                    this,
                    dst_resource,
                    dst_subresource,
                    dst_x,
                    dst_y,
                    src_resource,
                    src_subresource,
                    src_rect,
                    format,
                    resolve_mode,
                );
            }
        }
    ),
    set_view_instance_mask: slot!((this: *mut c_void, mask: u32) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::set_view_instance_mask(this, mask) }
    }),
    write_buffer_immediate: slot!(
        (this: *mut c_void, count: u32, params: *const c_void, modes: *const u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { copy::write_buffer_immediate(this, count, params, modes) }
        }
    ),
    set_protected_resource_session: slot!((this: *mut c_void, session: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::set_protected_resource_session(this, session) }
    }),
    begin_render_pass: slot!(
        (
            this: *mut c_void,
            num_render_targets: u32,
            render_targets: *const c_void,
            depth_stencil: *const c_void,
            flags: u32
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                clear::begin_render_pass(
                    this,
                    num_render_targets,
                    render_targets,
                    depth_stencil,
                    flags,
                );
            }
        }
    ),
    end_render_pass: slot!((this: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { clear::end_render_pass(this) }
    }),
    initialize_meta_command: slot!(
        (
            this: *mut c_void,
            meta_command: *mut c_void,
            initialization_parameters_data: *const c_void,
            initialization_parameters_data_size_in_bytes: usize
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                draw::initialize_meta_command(
                    this,
                    meta_command,
                    initialization_parameters_data,
                    initialization_parameters_data_size_in_bytes,
                );
            }
        }
    ),
    execute_meta_command: slot!(
        (
            this: *mut c_void,
            meta_command: *mut c_void,
            execution_parameters_data: *const c_void,
            execution_parameters_data_size_in_bytes: usize
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                draw::execute_meta_command(
                    this,
                    meta_command,
                    execution_parameters_data,
                    execution_parameters_data_size_in_bytes,
                );
            }
        }
    ),
    build_raytracing_acceleration_structure: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            num_postbuild_info_descs: u32,
            postbuild_info_descs: *const c_void
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                draw::build_raytracing_acceleration_structure(
                    this,
                    desc,
                    num_postbuild_info_descs,
                    postbuild_info_descs,
                );
            }
        }
    ),
    emit_raytracing_acceleration_structure_postbuild_info: slot!(
        (
            this: *mut c_void,
            desc: *const c_void,
            num_source_acceleration_structures: u32,
            source_acceleration_structure_data: *const u64
        ) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                draw::emit_raytracing_acceleration_structure_postbuild_info(
                    this,
                    desc,
                    num_source_acceleration_structures,
                    source_acceleration_structure_data,
                );
            }
        }
    ),
    copy_raytracing_acceleration_structure: slot!(
        (this: *mut c_void, dest_acceleration_structure_data: u64, source_acceleration_structure_data: u64, mode: u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe {
                draw::copy_raytracing_acceleration_structure(
                    this,
                    dest_acceleration_structure_data,
                    source_acceleration_structure_data,
                    mode,
                );
            }
        }
    ),
    set_pipeline_state1: slot!((this: *mut c_void, state_object: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::set_pipeline_state1(this, state_object) }
    }),
    dispatch_rays: slot!((this: *mut c_void, desc: *const c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { draw::dispatch_rays(this, desc) }
    }),
    rs_set_shading_rate: slot!(
        (this: *mut c_void, base_shading_rate: u32, combiners: *const u32) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { state::rs_set_shading_rate(this, base_shading_rate, combiners) }
        }
    ),
    rs_set_shading_rate_image: slot!((this: *mut c_void, shading_rate_image: *mut c_void) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { state::rs_set_shading_rate_image(this, shading_rate_image) }
    }),
    dispatch_mesh: slot!((this: *mut c_void, x: u32, y: u32, z: u32) => {
        // SAFETY: caller forwards COM slot arguments.
        unsafe { draw::dispatch_mesh(this, x, y, z) }
    }),
    barrier: slot!(
        (this: *mut c_void, num_barrier_groups: u32, barrier_groups: *const c_void) => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { barrier::barrier(this, num_barrier_groups, barrier_groups) }
        }
    ),
};

/// Companion `IMTLD3D12CommandListExt` vtable for [`List`], seated at [`List::EXT_OFFSET`].
pub static LIST_EXT_VTBL: D3D12CommandListExtVtbl = D3D12CommandListExtVtbl {
    unknown: IUnknownVtbl {
        query_interface: slot!(
            (this: *mut c_void, riid: *const c_void, object: *mut *mut c_void) -> Hresult => {
                // SAFETY: caller forwards COM slot arguments.
                unsafe { state::ext_query_interface(this, riid, object) }
            }
        ),
        add_ref: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `List` companion pointer at `List::EXT_OFFSET`.
            unsafe { state::ext_add_ref(this) }
        }),
        release: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `List` companion pointer at `List::EXT_OFFSET`.
            unsafe { state::ext_release(this) }
        }),
    },
    temporal_upscale: slot!(
        (this: *mut c_void, scaler: *mut c_void, desc: *const D3D12UpscaleDesc) -> Hresult => {
            // SAFETY: caller forwards COM slot arguments.
            unsafe { upscale::temporal_upscale(this, scaler, desc) }
        }
    ),
};
