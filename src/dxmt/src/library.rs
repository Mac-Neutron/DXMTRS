//! The core's Metal library of internal shaders, and what clears a resource with them.
//!
//! The library is built from `dxmt_command.metal`; [`ClearRtv`] and [`ClearUav`] clear through a
//! front end's [`SimpleCommandContext`], which records their passes into its own encoder.
//!
//! Behaviour is the C++'s (`dxmt_command.cpp`, `dxmt_command_clear.hpp`). Written ahead for the
//! switches of `d3d11.dll` and `d3d12.dll`, which are its first callers.

use std::{ffi::CStr, sync::Arc};

use winemetal_sys::{
    DispatchData_alloc_init, MTLDevice_newDepthStencilState, MTLDevice_newLibrary,
    MTLDevice_newRenderPipelineState, MTLLibrary_newFunction, NSObject_description,
    NSObject_release, NSString_getCString, NSString_lengthOfBytesUsingEncoding, Obj, Pool,
    wire::{
        MemPtr, WMT_BLEND_FACTOR_ONE, WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD,
        WMT_COLOR_WRITE_MASK_ALL, WMT_LOGIC_OPERATION_CLEAR, WMT_PIXEL_FORMAT_INVALID,
        WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED, WMT_TESSELLATION_FACTOR_STEP_FUNCTION_CONSTANT,
        WMT_TESSELLATION_PARTITION_MODE_POW2, WMT_WINDING_CLOCKWISE, WMTColorAttachmentBlendInfo,
        WMTDepthStencilInfo, WMTRenderPipelineInfo, WMTScissorRect, WMTSize,
        WMTUTF8_STRING_ENCODING, WMTViewport,
    },
};

use crate::resource::{Buffer, BufferViewKey, Texture, TextureViewKey};

mod clear_rtv;
mod clear_uav;
#[cfg(test)]
mod fake;

pub use clear_rtv::ClearRtv;
pub use clear_uav::ClearUav;

/// The argument-table slot the internal shaders read their constants from.
pub const CUSTOM_BUFFER_ARGUMENT_INDEX_0: u32 = 30;

/// The metallib `build.rs` took from `$DXMT_COMMAND_METALLIB`, or nothing when that was unset.
const METALLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/dxmt_command.metallib"));

#[cfg(target_pointer_width = "64")]
const NULL_PTR: MemPtr = MemPtr { ptr: 0 };
#[cfg(target_pointer_width = "32")]
const NULL_PTR: MemPtr = MemPtr { ptr: 0, high: 0 };

bitflags::bitflags! {
    /// How a pass uses a resource: the C++'s `ResourceAccess` bits.
    #[derive(Clone, Copy)]
    pub struct ResourceAccess: u32 {
        const READ = 1;
        const WRITE = 1 << 1;
        const UAV = 1 << 2;
    }
}

/// The internal command library: the compiled shaders of `dxmt_command.metal`.
pub struct Library {
    library: Obj,
}

impl Library {
    /// Loads the embedded metallib on `device`.
    ///
    /// # Errors
    ///
    /// Metal's description of the error, if it makes an error object or no library; the C++ logs
    /// it and aborts.
    pub fn new(device: &Obj) -> Result<Self, String> {
        let length = u64::try_from(METALLIB.len()).unwrap_or(0);
        // SAFETY: the bytes are `'static`; Metal reads `length` of them.
        let data = unsafe {
            DispatchData_alloc_init(METALLIB.as_ptr().expose_provenance() as u64, length)
        };
        let mut error = 0;
        // SAFETY: `device` holds a live `MTLDevice` and `data` is the data object just made; the
        // call writes `error`.
        let library = unsafe { MTLDevice_newLibrary(device.handle(), data, &raw mut error) };
        // SAFETY: `data` carries the one reference `alloc_init` gave it, given up here.
        unsafe { NSObject_release(data) };
        // SAFETY: a new library's handle carries its reference.
        let library = unsafe { Obj::adopt(library) };
        // SAFETY: a new error's handle carries its reference.
        let error = unsafe { Obj::adopt(error) };
        match (library, error) {
            (_, Some(error)) => Err(description(&error)),
            (Some(library), None) => Ok(Self { library }),
            (None, None) => Err("no library and no error".to_owned()),
        }
    }

    /// Raw `MTLLibrary` handle.
    #[must_use]
    pub const fn handle(&self) -> u64 {
        self.library.handle()
    }

    /// The function `name`, or `None` when the library has none.
    #[must_use]
    pub fn function(&self, name: &CStr) -> Option<Obj> {
        // SAFETY: the library is live and `name` is NUL-terminated for the call.
        let function = unsafe { MTLLibrary_newFunction(self.library.handle(), name.as_ptr()) };
        // SAFETY: a new function's handle carries its reference.
        unsafe { Obj::adopt(function) }
    }
}

/// What a front end's encoder offers the clear helpers.
///
/// The 20 calls of the C++'s `SimpleCommandContext`.
///
/// A pass runs from its start to [`Self::end_pass`]. The bytes a call takes are copied into the
/// command, which runs later.
pub trait SimpleCommandContext {
    /// A compute pipeline of the library's function `name`, or `None` when it cannot be made.
    fn compute_pipeline(&mut self, name: &CStr) -> Option<Obj>;
    fn start_compute_pass(&mut self);
    fn end_pass(&mut self);
    fn set_compute_pso(&mut self, pso: &Obj, threadgroup_size: WMTSize);
    fn dispatch(&mut self, size: WMTSize);
    fn set_compute_texture(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        access: ResourceAccess,
    );
    fn set_compute_texel_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        view: BufferViewKey,
        access: ResourceAccess,
    );
    /// Binds `buffer`, of which the pass uses `length` bytes from `offset`.
    fn set_compute_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        offset: u32,
        length: u32,
        access: ResourceAccess,
    );
    fn set_compute_bytes(&mut self, index: u32, bytes: &[u8]);
    /// The internal command library.
    fn default_library(&self) -> &Library;
    fn start_render_pass(&mut self);
    /// Attaches a colour target.
    ///
    /// All attachments of a pass (the depth-stencil one too) have one array length and one sample
    /// count, and none is skipped: index 2 needs 0 and 1.
    fn set_color_attachment(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        depth_plane: u32,
    );
    /// Attaches a depth-stencil target; `dsv_flag` has bit 0 for depth and bit 1 for stencil.
    fn set_depth_stencil_attachment(
        &mut self,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        dsv_flag: u32,
    );
    fn set_render_pso(&mut self, pso: &Obj);
    fn set_viewport(&mut self, viewport: WMTViewport);
    fn set_depth_stencil_state(&mut self, state: &Obj);
    fn set_scissor_rect(&mut self, rect: WMTScissorRect);
    fn set_stencil_reference(&mut self, stencil_ref: u8);
    fn set_fragment_bytes(&mut self, index: u32, bytes: &[u8]);
    /// A draw of `primitive` (a `WMTPrimitiveType`).
    fn draw(
        &mut self,
        primitive: u8,
        vertex_start: u32,
        vertex_count: u32,
        base_instance: i32,
        instance_count: u32,
    );
}

/// A pipeline description set the way `InitializeRenderPipelineInfo` sets one.
///
/// No formats, no blending, rasterization on, one sample.
#[must_use]
pub const fn render_pipeline_info() -> WMTRenderPipelineInfo {
    let color = WMTColorAttachmentBlendInfo {
        pixel_format: WMT_PIXEL_FORMAT_INVALID,
        rgb_blend_operation: WMT_BLEND_OPERATION_ADD,
        alpha_blend_operation: WMT_BLEND_OPERATION_ADD,
        src_rgb_blend_factor: WMT_BLEND_FACTOR_ONE,
        dst_rgb_blend_factor: WMT_BLEND_FACTOR_ZERO,
        src_alpha_blend_factor: WMT_BLEND_FACTOR_ONE,
        dst_alpha_blend_factor: WMT_BLEND_FACTOR_ZERO,
        write_mask: WMT_COLOR_WRITE_MASK_ALL,
        blending_enabled: 0,
    };
    WMTRenderPipelineInfo {
        colors: [color; 8],
        alpha_to_coverage_enabled: 0,
        logic_operation_enabled: 0,
        logic_operation: WMT_LOGIC_OPERATION_CLEAR,
        rasterization_enabled: 1,
        raster_sample_count: 1,
        depth_pixel_format: WMT_PIXEL_FORMAT_INVALID,
        stencil_pixel_format: WMT_PIXEL_FORMAT_INVALID,
        vertex_function: 0,
        fragment_function: 0,
        immutable_vertex_buffers: 0,
        immutable_fragment_buffers: 0,
        input_primitive_topology: WMT_PRIMITIVE_TOPOLOGY_CLASS_UNSPECIFIED,
        tessellation_partition_mode: WMT_TESSELLATION_PARTITION_MODE_POW2,
        max_tessellation_factor: 16,
        tessellation_output_winding_order: WMT_WINDING_CLOCKWISE,
        tessellation_factor_step: WMT_TESSELLATION_FACTOR_STEP_FUNCTION_CONSTANT,
        binary_archive_for_serialization: 0,
        binary_archives_for_lookup: NULL_PTR,
        num_binary_archives_for_lookup: 0,
        fail_on_binary_archive_miss: 0,
        support_indirect_command_buffers: 0,
        padding: [0; 5],
    }
}

/// The render pipeline `info` describes on `device`, or `None` when Metal makes none.
#[must_use]
pub fn render_pipeline(device: &Obj, info: &WMTRenderPipelineInfo) -> Option<Obj> {
    let mut error = 0;
    // SAFETY: `device` holds a live `MTLDevice`; `info` and `error` are valid for the call.
    let pso = unsafe { MTLDevice_newRenderPipelineState(device.handle(), info, &raw mut error) };
    // SAFETY: a new error's handle carries its reference, which is dropped here unread.
    drop(unsafe { Obj::adopt(error) });
    // SAFETY: a new pipeline's handle carries its reference.
    unsafe { Obj::adopt(pso) }
}

/// The depth-stencil state `info` describes on `device`, or `None` when Metal makes none.
#[must_use]
pub fn depth_stencil_state(device: &Obj, info: &WMTDepthStencilInfo) -> Option<Obj> {
    // SAFETY: `device` holds a live `MTLDevice`; `info` is valid for the call.
    let state = unsafe { MTLDevice_newDepthStencilState(device.handle(), info) };
    // SAFETY: a new state's handle carries its reference.
    unsafe { Obj::adopt(state) }
}

/// An Objective-C object's description, such as an error's.
fn description(object: &Obj) -> String {
    let _pool = Pool::new();
    // SAFETY: `object` is live; the description is an autoreleased string, valid under the pool.
    let text = unsafe { NSObject_description(object.handle()) };
    // SAFETY: `text` is that live string.
    let length = unsafe { NSString_lengthOfBytesUsingEncoding(text, WMTUTF8_STRING_ENCODING) };
    let length = usize::try_from(length).unwrap_or(0);
    let mut buffer = vec![0u8; length + 1];
    // SAFETY: `buffer` has room for `length` bytes and the NUL asked for.
    unsafe {
        NSString_getCString(
            text,
            buffer.as_mut_ptr().cast(),
            u64::try_from(buffer.len()).unwrap_or(0),
            WMTUTF8_STRING_ENCODING,
        );
    }
    buffer.truncate(length);
    String::from_utf8_lossy(&buffer).into_owned()
}

#[cfg(test)]
mod tests;
