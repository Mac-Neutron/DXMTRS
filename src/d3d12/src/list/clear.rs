//! RTV/DSV/UAV clears, `DiscardResource`, and `BeginRenderPass`/`EndRenderPass`
//!
//! (`d3d12_command_list.cpp`).

use core::{
    ffi::{CStr, c_void},
    mem::{align_of, size_of},
    ptr,
};
use std::sync::Arc;

use dxmt_core::{
    format,
    library::{self, Library, ResourceAccess, SimpleCommandContext},
    resource::{Buffer, BufferViewKey, Texture, TextureViewKey},
};
use winemetal_sys::{MTLDevice_newComputePipelineState, Obj};
use winemetal_wire::{
    MemPtr, WMT_COMPUTE_COMMAND_DISPATCH_THREADS, WMT_COMPUTE_COMMAND_NOP,
    WMT_COMPUTE_COMMAND_SET_BUFFER, WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO,
    WMT_COMPUTE_COMMAND_SET_TEXTURE, WMT_LOAD_ACTION_LOAD, WMT_RENDER_COMMAND_DRAW,
    WMT_RENDER_COMMAND_NOP, WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
    WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES, WMT_RENDER_COMMAND_SET_PSO,
    WMT_RENDER_COMMAND_SET_SCISSOR_RECT, WMT_RENDER_COMMAND_SET_STENCIL_REF,
    WMT_RENDER_COMMAND_SET_VIEWPORT, WMT_STORE_ACTION_STORE, WMT_TEXTURE_TYPE3D, WMTClearColor,
    WMTComputePipelineInfo, WMTScissorRect, WMTSize, WMTViewport, WmtcmdBase,
    WmtcmdComputeDispatch, WmtcmdComputeNop, WmtcmdComputeSetbuffer, WmtcmdComputeSetbytes,
    WmtcmdComputeSetpso, WmtcmdComputeSettexture, WmtcmdRenderDraw, WmtcmdRenderNop,
    WmtcmdRenderSetbytes, WmtcmdRenderSetdepthstencilstate, WmtcmdRenderSetpso,
    WmtcmdRenderSetscissorrect, WmtcmdRenderSetstencilref, WmtcmdRenderSetviewport,
};

use crate::{
    abi::{
        list_ops::{
            D3D12_CLEAR_FLAG_DEPTH, D3D12_CLEAR_FLAG_STENCIL,
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_CLEAR,
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_NO_ACCESS,
            D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_RESOLVE, D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT,
            Rect, RenderPassDepthStencilDesc, RenderPassEndingAccess, RenderPassRenderTargetDesc,
        },
        vtbl::{D3d12CpuDescriptorHandle, D3d12GpuDescriptorHandle},
    },
    allocator::Allocator,
    descriptor_heap::{self, ShaderVisibleDescriptor},
    encoder::{
        ClearEncoderData, ComputeEncoderData, EncoderData, EncoderType, RenderEncoderData,
        RenderEncoderDepthAttachmentData, RenderEncoderStencilAttachmentData, TextureViewRef,
    },
    list::{List, copy, state},
};

struct ClearContext<'a>(&'a mut Allocator);

impl ClearContext<'_> {
    fn encode_compute_cmd<T>(&mut self, mut cmd: T) {
        // SAFETY: `self.0.encoder_current` is a `ComputeEncoderData` initialized by
        // `start_compute_pass`.
        let enc = unsafe { &mut *self.0.encoder_current.cast::<ComputeEncoderData>() };
        let storage = self
            .0
            .allocate_cpu_heap(size_of::<T>(), 16)
            .cast::<c_void>()
            .cast::<T>();
        // SAFETY: `cmd` starts with `WmtcmdBase` at offset 0.
        let base = unsafe { &mut *(&raw mut cmd).cast::<WmtcmdBase>() };
        base.next = MemPtr::NULL;
        // SAFETY: `storage` points to freshly allocated aligned CPU heap storage for `T`.
        unsafe { ptr::write(storage, cmd) };
        let storage_base = storage.cast::<WmtcmdBase>();
        // SAFETY: `enc.cmd_tail` points to a valid command node in the CPU heap.
        unsafe { (*enc.cmd_tail).next = MemPtr::from_ptr(storage_base) };
        enc.cmd_tail = storage_base;
        enc.base.barriers_last = self.0.barriers;
    }

    fn encode_render_cmd<T>(&mut self, mut cmd: T) {
        // SAFETY: `self.0.encoder_current` is a `RenderEncoderData` initialized by
        // `start_render_pass`.
        let enc = unsafe { &mut *self.0.encoder_current.cast::<RenderEncoderData>() };
        let storage = self
            .0
            .allocate_cpu_heap(size_of::<T>(), 16)
            .cast::<c_void>()
            .cast::<T>();
        // SAFETY: `cmd` starts with `WmtcmdBase` at offset 0.
        let base = unsafe { &mut *(&raw mut cmd).cast::<WmtcmdBase>() };
        base.next = MemPtr::NULL;
        // SAFETY: `storage` points to freshly allocated aligned CPU heap storage for `T`.
        unsafe { ptr::write(storage, cmd) };
        let storage_base = storage.cast::<WmtcmdBase>();
        // SAFETY: `enc.cmd_tail` points to a valid command node in the CPU heap.
        unsafe { (*enc.cmd_tail).next = MemPtr::from_ptr(storage_base) };
        enc.cmd_tail = storage_base;
        enc.base.barriers_last = self.0.barriers;
    }
}

impl SimpleCommandContext for ClearContext<'_> {
    fn compute_pipeline(&mut self, name: &CStr) -> Option<Obj> {
        if self.0.device.is_null() {
            return None;
        }
        // SAFETY: `self.0.device` is retained by `self.0`.
        let dev = unsafe { &*self.0.device };
        let mtl = dev.mtl.as_ref()?;
        let func = dev.library.as_ref()?.function(name)?;
        let info = WMTComputePipelineInfo {
            compute_function: func.handle(),
            binary_archives_for_lookup: MemPtr::NULL,
            binary_archive_for_serialization: 0,
            num_binary_archives_for_lookup: 0,
            fail_on_binary_archive_miss: 0,
            support_indirect_command_buffers: 0,
            tgsize_is_multiple_of_sgwidth: 0,
            immutable_buffers: 0,
        };
        let mut err = 0u64;
        // SAFETY: `mtl` and `func` are live Metal objects; `info` and `err` are valid stack
        // pointers.
        let pso = unsafe {
            MTLDevice_newComputePipelineState(mtl.handle(), &raw const info, &raw mut err)
        };
        // SAFETY: `MTLDevice_newComputePipelineState` returns a +1 retained error or 0.
        if let Some(err_obj) = unsafe { Obj::adopt(err) } {
            crate::err!(
                "Failed to create compute PSO: {}",
                library::description(&err_obj)
            );
        }
        // SAFETY: `MTLDevice_newComputePipelineState` returns a +1 retained PSO or 0.
        unsafe { Obj::adopt(pso) }
    }

    fn start_compute_pass(&mut self) {
        self.0.invalidate_current_pass();
        let barriers = self.0.barriers;
        let ptr = self
            .0
            .allocate_cpu_heap(
                size_of::<ComputeEncoderData>(),
                align_of::<ComputeEncoderData>(),
            )
            .cast::<c_void>()
            .cast::<ComputeEncoderData>();
        let mut base = EncoderData::new(EncoderType::Compute, 0);
        base.barriers = barriers;
        base.barriers_last = barriers;
        let init = ComputeEncoderData {
            base,
            cmd_head: WmtcmdComputeNop {
                r#type: WMT_COMPUTE_COMMAND_NOP,
                reserved: [0; 3],
                next: MemPtr::NULL,
            },
            cmd_tail: ptr::null_mut(),
        };
        // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for
        // `ComputeEncoderData`.
        unsafe { ptr::write(ptr, init) };
        // SAFETY: `ptr` was just initialized above.
        let enc = unsafe { &mut *ptr };
        enc.cmd_tail = (&raw mut enc.cmd_head).cast::<WmtcmdBase>();
        self.0.encoder_current = ptr.cast::<EncoderData>();
    }

    fn end_pass(&mut self) {
        self.0.invalidate_current_pass();
    }

    fn set_compute_pso(&mut self, pso: &Obj, threadgroup_size: WMTSize) {
        self.encode_compute_cmd(WmtcmdComputeSetpso {
            r#type: WMT_COMPUTE_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: MemPtr::NULL,
            pso: pso.handle(),
            threadgroup_size,
        });
    }

    fn dispatch(&mut self, size: WMTSize) {
        self.encode_compute_cmd(WmtcmdComputeDispatch {
            r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
            reserved: [0; 3],
            next: MemPtr::NULL,
            size,
        });
    }

    fn set_compute_texture(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        _access: ResourceAccess,
    ) {
        let handle = texture
            .current()
            .map_or(0, |alloc| alloc.view(view).texture);
        self.encode_compute_cmd(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: MemPtr::NULL,
            texture: handle,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_texel_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        view: BufferViewKey,
        _access: ResourceAccess,
    ) {
        let handle = buffer
            .current()
            .map_or(0, |alloc| buffer.view(&alloc, view).texture);
        self.encode_compute_cmd(WmtcmdComputeSettexture {
            r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
            reserved: [0; 3],
            next: MemPtr::NULL,
            texture: handle,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_buffer(
        &mut self,
        index: u32,
        buffer: &Arc<Buffer>,
        _offset: u32,
        _length: u32,
        _access: ResourceAccess,
    ) {
        let handle = buffer.current().map_or(0, |alloc| alloc.buffer());
        self.encode_compute_cmd(WmtcmdComputeSetbuffer {
            r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
            reserved: [0; 3],
            next: MemPtr::NULL,
            buffer: handle,
            offset: 0,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn set_compute_bytes(&mut self, index: u32, bytes: &[u8]) {
        let temp = self.0.allocate_cpu_heap(bytes.len(), 16);
        if !temp.is_null() && !bytes.is_empty() {
            // SAFETY: `temp` has `bytes.len()` writable bytes in the CPU heap.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), temp, bytes.len()) };
        }
        self.encode_compute_cmd(WmtcmdComputeSetbytes {
            r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
            reserved: [0; 3],
            next: MemPtr::NULL,
            bytes: MemPtr::from_ptr(temp),
            length: bytes.len() as u64,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn default_library(&self) -> &Library {
        // SAFETY: `default_library` is only called when `self.0.device` is non-null and has an
        // initialized `Library`.
        let dev = unsafe { &*self.0.device };
        dev.library
            .as_ref()
            .unwrap_or_else(|| unreachable!("Device has an internal Library"))
    }

    fn start_render_pass(&mut self) {
        self.0.invalidate_current_pass();
        let barriers = self.0.barriers;
        let ptr = self
            .0
            .allocate_cpu_heap(
                size_of::<RenderEncoderData>(),
                align_of::<RenderEncoderData>(),
            )
            .cast::<c_void>()
            .cast::<RenderEncoderData>();
        let mut base = EncoderData::new(EncoderType::Render, 0);
        base.barriers = barriers;
        base.barriers_last = barriers;
        let init = RenderEncoderData {
            base,
            colors: Default::default(),
            depth: RenderEncoderDepthAttachmentData::default(),
            stencil: RenderEncoderStencilAttachmentData::default(),
            default_raster_sample_count: 1,
            render_target_array_length: 0,
            render_target_height: 16384,
            render_target_width: 16384,
            cmd_head: WmtcmdRenderNop {
                r#type: WMT_RENDER_COMMAND_NOP,
                reserved: [0; 3],
                next: MemPtr::NULL,
            },
            cmd_tail: ptr::null_mut(),
            dsv_planar_flags: 0,
            dsv_readonly_flags: 0,
            render_target_count: 0,
            use_visibility_result: false,
            visibility_buffer: 0,
            use_tessellation: false,
            use_geometry: false,
            uav_writes: false,
            pre_head: WmtcmdComputeNop {
                r#type: WMT_COMPUTE_COMMAND_NOP,
                reserved: [0; 3],
                next: MemPtr::NULL,
            },
            pre_tail: ptr::null_mut(),
        };
        // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for
        // `RenderEncoderData`.
        unsafe { ptr::write(ptr, init) };
        // SAFETY: `ptr` was just initialized above.
        let enc = unsafe { &mut *ptr };
        enc.cmd_tail = (&raw mut enc.cmd_head).cast::<WmtcmdBase>();
        self.0.encoder_current = ptr.cast::<EncoderData>();
    }

    fn set_color_attachment(
        &mut self,
        index: u32,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        depth_plane: u32,
    ) {
        let idx = usize::try_from(index).unwrap_or(0);
        // SAFETY: `self.0.encoder_current` is a `RenderEncoderData` initialized by
        // `start_render_pass`.
        let enc = unsafe { &mut *self.0.encoder_current.cast::<RenderEncoderData>() };
        enc.colors[idx].attachment = TextureViewRef::from_texture(texture, view);
        enc.colors[idx].depth_plane = depth_plane;
        enc.colors[idx].load_action = WMT_LOAD_ACTION_LOAD;
        enc.colors[idx].store_action = WMT_STORE_ACTION_STORE;
        let next_count = u8::try_from(index + 1).unwrap_or(u8::MAX);
        enc.render_target_count = enc.render_target_count.max(next_count);
        let [width, height, depth] = texture.view_size(view);
        enc.render_target_width = enc.render_target_width.min(width);
        enc.render_target_height = enc.render_target_height.min(height);
        let array_len = if texture.info().r#type() == WMT_TEXTURE_TYPE3D {
            depth.saturating_sub(depth_plane)
        } else {
            view.array_size()
        };
        enc.render_target_array_length = u16::try_from(array_len).unwrap_or(u16::MAX);
        enc.default_raster_sample_count = u8::try_from(texture.info().sample_count()).unwrap_or(1);
    }

    fn set_depth_stencil_attachment(
        &mut self,
        texture: &Arc<Texture>,
        view: TextureViewKey,
        dsv_flag: u32,
    ) {
        // SAFETY: `self.0.encoder_current` is a `RenderEncoderData` initialized by
        // `start_render_pass`.
        let enc = unsafe { &mut *self.0.encoder_current.cast::<RenderEncoderData>() };
        let view_ref = TextureViewRef::from_texture(texture, view);
        if (dsv_flag & 1) != 0 {
            enc.depth.attachment.clone_from(&view_ref);
            enc.depth.load_action = WMT_LOAD_ACTION_LOAD;
            enc.depth.store_action = WMT_STORE_ACTION_STORE;
        }
        if (dsv_flag & 2) != 0 {
            enc.stencil.attachment = view_ref;
            enc.stencil.load_action = WMT_LOAD_ACTION_LOAD;
            enc.stencil.store_action = WMT_STORE_ACTION_STORE;
        }
        enc.dsv_planar_flags = u8::try_from(dsv_flag).unwrap_or(0);
        let [width, height, _] = texture.view_size(view);
        enc.render_target_width = enc.render_target_width.min(width);
        enc.render_target_height = enc.render_target_height.min(height);
        enc.render_target_array_length = u16::try_from(view.array_size()).unwrap_or(u16::MAX);
        enc.default_raster_sample_count = u8::try_from(texture.info().sample_count()).unwrap_or(1);
    }

    fn set_render_pso(&mut self, pso: &Obj) {
        self.encode_render_cmd(WmtcmdRenderSetpso {
            r#type: WMT_RENDER_COMMAND_SET_PSO,
            reserved: [0; 3],
            next: MemPtr::NULL,
            pso: pso.handle(),
        });
    }

    fn set_viewport(&mut self, viewport: WMTViewport) {
        self.encode_render_cmd(WmtcmdRenderSetviewport {
            r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
            reserved: [0; 3],
            next: MemPtr::NULL,
            viewport,
        });
    }

    fn set_depth_stencil_state(&mut self, state: &Obj) {
        self.encode_render_cmd(WmtcmdRenderSetdepthstencilstate {
            r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
            reserved: [0; 3],
            next: MemPtr::NULL,
            depth_stencil_state: state.handle(),
        });
    }

    fn set_scissor_rect(&mut self, rect: WMTScissorRect) {
        self.encode_render_cmd(WmtcmdRenderSetscissorrect {
            r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
            reserved: [0; 3],
            next: MemPtr::NULL,
            scissor_rect: rect,
        });
    }

    fn set_stencil_reference(&mut self, stencil_ref: u8) {
        self.encode_render_cmd(WmtcmdRenderSetstencilref {
            r#type: WMT_RENDER_COMMAND_SET_STENCIL_REF,
            reserved: [0; 3],
            next: MemPtr::NULL,
            stencil_ref,
        });
    }

    fn set_fragment_bytes(&mut self, index: u32, bytes: &[u8]) {
        let temp = self.0.allocate_cpu_heap(bytes.len(), 16);
        if !temp.is_null() && !bytes.is_empty() {
            // SAFETY: `temp` has `bytes.len()` writable bytes in the CPU heap.
            unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), temp, bytes.len()) };
        }
        self.encode_render_cmd(WmtcmdRenderSetbytes {
            r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
            reserved: [0; 3],
            next: MemPtr::NULL,
            bytes: MemPtr::from_ptr(temp),
            length: bytes.len() as u64,
            index: u8::try_from(index).unwrap_or(0),
        });
    }

    fn draw(
        &mut self,
        primitive: u8,
        vertex_start: u32,
        vertex_count: u32,
        base_instance: i32,
        instance_count: u32,
    ) {
        self.encode_render_cmd(WmtcmdRenderDraw {
            r#type: WMT_RENDER_COMMAND_DRAW,
            reserved: [0; 3],
            next: MemPtr::NULL,
            primitive_type: primitive,
            vertex_start: u64::from(vertex_start),
            vertex_count: u64::from(vertex_count),
            instance_count,
            base_instance: base_instance.cast_unsigned(),
        });
    }
}

/// `ID3D12GraphicsCommandList::ClearDepthStencilView` (`d3d12_command_list.cpp:1645`).
///
/// # Safety
///
/// `this` and `rects` are caller-supplied pointers.
pub unsafe fn clear_depth_stencil_view(
    this: *mut c_void,
    depth_stencil_view: D3d12CpuDescriptorHandle,
    clear_flags: u32,
    depth: f32,
    stencil: u8,
    num_rects: u32,
    rects: *const c_void,
) {
    crate::scope!("list.ClearDepthStencilView");
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `depth_stencil_view` is a caller-supplied DSV descriptor handle.
    let Some(attachment_desc) = (unsafe { descriptor_heap::render_target(depth_stencil_view) })
    else {
        return;
    };
    let Some(texture) = attachment_desc.texture else {
        return;
    };
    let checked_flags =
        clear_flags & format::depth_stencil_planar_flags(attachment_desc.view.format());
    if checked_flags == 0 {
        return;
    }

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };

    if !rects.is_null() {
        let Some(mut clear_rtv) = alloc.clear_rtv.take() else {
            return;
        };
        let mut ctx = ClearContext(alloc);
        clear_rtv.begin(&mut ctx, texture, attachment_desc.view, 0, checked_flags);
        let rect_ptr = rects.cast::<Rect>();
        for i in 0..num_rects {
            // SAFETY: `rect_ptr` has `num_rects` readable `Rect` elements.
            let rect_elem = unsafe { rect_ptr.add(i as usize) };
            // SAFETY: `rect_elem` is a valid element of the `rects` array.
            let rect = unsafe { &*rect_elem };
            let rect_offset_x = rect.left.max(0).cast_unsigned();
            let rect_offset_y = rect.top.max(0).cast_unsigned();
            let rect_width = rect.right.wrapping_sub(rect_offset_x.cast_signed());
            let rect_height = rect.bottom.wrapping_sub(rect_offset_y.cast_signed());
            if rect_height <= 0 || rect_width <= 0 {
                continue;
            }
            clear_rtv.clear_depth_stencil(
                &mut ctx,
                WMTScissorRect {
                    x: u64::from(rect_offset_x),
                    y: u64::from(rect_offset_y),
                    width: u64::from(rect_width.cast_unsigned()),
                    height: u64::from(rect_height.cast_unsigned()),
                },
                depth,
                stencil,
            );
        }
        clear_rtv.end(&mut ctx);
        alloc.clear_rtv = Some(clear_rtv);
        return;
    }

    alloc.invalidate_current_pass();
    let barriers = alloc.barriers;
    let ptr = alloc
        .allocate_cpu_heap(
            size_of::<ClearEncoderData>(),
            align_of::<ClearEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<ClearEncoderData>();
    let mut base = EncoderData::new(EncoderType::Clear, 0);
    base.barriers = barriers;
    base.barriers_last = barriers;
    base.writes_unknown = false;
    let init = ClearEncoderData {
        base,
        color: WMTClearColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        },
        depth_stencil: (depth, stencil),
        attachment: TextureViewRef::from_texture(&texture, attachment_desc.view),
        clear_dsv: checked_flags,
        array_length: attachment_desc.render_target_array_length,
        width: attachment_desc.width,
        height: attachment_desc.height,
        depth_plane: 0,
    };
    // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `ClearEncoderData`.
    unsafe { ptr::write(ptr, init) };
    alloc.encoder_current = ptr.cast::<EncoderData>();
    alloc.writes(Arc::as_ptr(&texture).cast::<c_void>());
    alloc.invalidate_current_pass();
}

/// `ID3D12GraphicsCommandList::ClearRenderTargetView` (`d3d12_command_list.cpp:1688`).
///
/// # Safety
///
/// `this`, `color_rgba`, and `rects` are caller-supplied pointers.
pub unsafe fn clear_render_target_view(
    this: *mut c_void,
    render_target_view: D3d12CpuDescriptorHandle,
    color_rgba: *const f32,
    num_rects: u32,
    rects: *const c_void,
) {
    crate::scope!("list.ClearRenderTargetView");
    if color_rgba.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `render_target_view` is a caller-supplied RTV descriptor handle.
    let Some(attachment_desc) = (unsafe { descriptor_heap::render_target(render_target_view) })
    else {
        return;
    };
    let Some(texture) = attachment_desc.texture else {
        return;
    };
    // SAFETY: `color_rgba` points to 4 `f32` elements.
    let color = unsafe { ptr::read_unaligned(color_rgba.cast::<[f32; 4]>()) };

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };

    if !rects.is_null() {
        let Some(mut clear_rtv) = alloc.clear_rtv.take() else {
            return;
        };
        let mut ctx = ClearContext(alloc);
        clear_rtv.begin(
            &mut ctx,
            texture,
            attachment_desc.view,
            attachment_desc.depth_plane,
            0,
        );
        let rect_ptr = rects.cast::<Rect>();
        for i in 0..num_rects {
            // SAFETY: `rect_ptr` has `num_rects` readable `Rect` elements.
            let rect_elem = unsafe { rect_ptr.add(i as usize) };
            // SAFETY: `rect_elem` is a valid element of the `rects` array.
            let rect = unsafe { &*rect_elem };
            let rect_offset_x = rect.left.max(0).cast_unsigned();
            let rect_offset_y = rect.top.max(0).cast_unsigned();
            let rect_width = rect.right.wrapping_sub(rect_offset_x.cast_signed());
            let rect_height = rect.bottom.wrapping_sub(rect_offset_y.cast_signed());
            if rect_height <= 0 || rect_width <= 0 {
                continue;
            }
            clear_rtv.clear_color(
                &mut ctx,
                WMTScissorRect {
                    x: u64::from(rect_offset_x),
                    y: u64::from(rect_offset_y),
                    width: u64::from(rect_width.cast_unsigned()),
                    height: u64::from(rect_height.cast_unsigned()),
                },
                attachment_desc.render_target_array_length,
                color,
            );
        }
        clear_rtv.end(&mut ctx);
        alloc.clear_rtv = Some(clear_rtv);
        return;
    }

    alloc.invalidate_current_pass();
    let barriers = alloc.barriers;
    let ptr = alloc
        .allocate_cpu_heap(
            size_of::<ClearEncoderData>(),
            align_of::<ClearEncoderData>(),
        )
        .cast::<c_void>()
        .cast::<ClearEncoderData>();
    let mut base = EncoderData::new(EncoderType::Clear, 0);
    base.barriers = barriers;
    base.barriers_last = barriers;
    base.writes_unknown = false;
    let mut clear_color = WMTClearColor {
        r: f64::from(color[0]),
        g: f64::from(color[1]),
        b: f64::from(color[2]),
        a: f64::from(color[3]),
    };
    format::sanitize_clear_color(attachment_desc.view.format(), &mut clear_color);
    let init = ClearEncoderData {
        base,
        color: clear_color,
        depth_stencil: (0.0, 0),
        attachment: TextureViewRef::from_texture(&texture, attachment_desc.view),
        clear_dsv: 0,
        array_length: attachment_desc.render_target_array_length,
        width: attachment_desc.width,
        height: attachment_desc.height,
        depth_plane: attachment_desc.depth_plane,
    };
    // SAFETY: `ptr` points to freshly allocated aligned CPU heap storage for `ClearEncoderData`.
    unsafe { ptr::write(ptr, init) };
    alloc.encoder_current = ptr.cast::<EncoderData>();
    alloc.writes(Arc::as_ptr(&texture).cast::<c_void>());
    alloc.invalidate_current_pass();
}

/// `ID3D12GraphicsCommandList::ClearUnorderedAccessViewUint` (`d3d12_command_list.cpp:1733`).
///
/// # Safety
///
/// `this`, `resource`, `values`, and `rects` are caller-supplied pointers.
pub unsafe fn clear_unordered_access_view_uint(
    this: *mut c_void,
    _view_gpu_handle_in_current_heap: D3d12GpuDescriptorHandle,
    view_cpu_handle: D3d12CpuDescriptorHandle,
    _resource: *mut c_void,
    values: *const u32,
    num_rects: u32,
    rects: *const c_void,
) {
    crate::scope!("list.ClearUnorderedAccessViewUint");
    if values.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `view_cpu_handle` is a caller-supplied CPU descriptor handle.
    let Some((heap, index)) = (unsafe { descriptor_heap::shader_visible(view_cpu_handle) }) else {
        return;
    };
    let descriptor = heap.descriptor(index);
    // SAFETY: `values` points to 4 `u32` elements.
    let color = unsafe { ptr::read_unaligned(values.cast::<[u32; 4]>()) };

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };
    let Some(mut clear_uav) = alloc.clear_uav.take() else {
        return;
    };
    let mut ctx = ClearContext(alloc);

    let full_rect = match descriptor {
        ShaderVisibleDescriptor::UavBuffer {
            buffer: Some(buffer),
            slice,
        } => {
            clear_uav.begin_buffer_u32(&mut ctx, color, &buffer);
            Rect {
                left: i32::try_from(slice.byte_offset >> 2).unwrap_or(i32::MAX),
                top: 0,
                right: i32::try_from(slice.byte_offset.wrapping_add(slice.byte_length) >> 2)
                    .unwrap_or(i32::MAX),
                bottom: 1,
            }
        }
        ShaderVisibleDescriptor::UavTexture {
            texture: Some(texture),
            view,
        } => {
            clear_uav.begin_texture_u32(&mut ctx, color, &texture, view);
            let [w, h, _] = texture.view_size(view);
            Rect {
                left: 0,
                top: 0,
                right: w.cast_signed(),
                bottom: h.cast_signed(),
            }
        }
        ShaderVisibleDescriptor::UavTexelBuffer {
            buffer: Some(buffer),
            view,
            slice,
        } => {
            clear_uav.begin_texel_buffer_u32(&mut ctx, color, &buffer, view);
            Rect {
                left: i32::try_from(slice.first_element).unwrap_or(i32::MAX),
                top: 0,
                right: i32::try_from(
                    slice
                        .first_element
                        .wrapping_add(u64::from(slice.element_count)),
                )
                .unwrap_or(i32::MAX),
                bottom: 1,
            }
        }
        _ => {
            clear_uav.end(&mut ctx);
            alloc.clear_uav = Some(clear_uav);
            return;
        }
    };

    let (rect_ptr, rect_count) = if num_rects > 0 && !rects.is_null() {
        (rects.cast::<Rect>(), num_rects)
    } else {
        (&raw const full_rect, 1)
    };

    for i in 0..rect_count {
        // SAFETY: `rect_ptr` has `rect_count` readable `Rect` elements.
        let rect_elem = unsafe { rect_ptr.add(i as usize) };
        // SAFETY: `rect_elem` is a valid element of `rect_ptr`.
        let rect = unsafe { &*rect_elem };
        let width = rect.right.wrapping_sub(rect.left);
        let height = rect.bottom.wrapping_sub(rect.top);
        if width <= 0 || height <= 0 {
            continue;
        }
        clear_uav.clear(
            &mut ctx,
            rect.left.cast_unsigned(),
            rect.top.cast_unsigned(),
            width.cast_unsigned(),
            height.cast_unsigned(),
        );
    }

    clear_uav.end(&mut ctx);
    alloc.clear_uav = Some(clear_uav);
}

/// `ID3D12GraphicsCommandList::ClearUnorderedAccessViewFloat` (`d3d12_command_list.cpp:1787`).
///
/// # Safety
///
/// `this`, `resource`, `values`, and `rects` are caller-supplied pointers.
pub unsafe fn clear_unordered_access_view_float(
    this: *mut c_void,
    _view_gpu_handle_in_current_heap: D3d12GpuDescriptorHandle,
    view_cpu_handle: D3d12CpuDescriptorHandle,
    _resource: *mut c_void,
    values: *const f32,
    num_rects: u32,
    rects: *const c_void,
) {
    crate::scope!("list.ClearUnorderedAccessViewFloat");
    if values.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    if list.allocator.is_null() {
        return;
    }
    // SAFETY: `view_cpu_handle` is a caller-supplied CPU descriptor handle.
    let Some((heap, index)) = (unsafe { descriptor_heap::shader_visible(view_cpu_handle) }) else {
        return;
    };
    let descriptor = heap.descriptor(index);
    // SAFETY: `values` points to 4 `f32` elements.
    let color = unsafe { ptr::read_unaligned(values.cast::<[f32; 4]>()) };

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };
    let Some(mut clear_uav) = alloc.clear_uav.take() else {
        return;
    };
    let mut ctx = ClearContext(alloc);

    let full_rect = match descriptor {
        ShaderVisibleDescriptor::UavBuffer {
            buffer: Some(buffer),
            slice,
        } => {
            clear_uav.begin_buffer_f32(&mut ctx, color, &buffer);
            Rect {
                left: i32::try_from(slice.byte_offset >> 2).unwrap_or(i32::MAX),
                top: 0,
                right: i32::try_from(slice.byte_offset.wrapping_add(slice.byte_length) >> 2)
                    .unwrap_or(i32::MAX),
                bottom: 1,
            }
        }
        ShaderVisibleDescriptor::UavTexture {
            texture: Some(texture),
            view,
        } => {
            clear_uav.begin_texture_f32(&mut ctx, color, &texture, view);
            let [w, h, _] = texture.view_size(view);
            Rect {
                left: 0,
                top: 0,
                right: w.cast_signed(),
                bottom: h.cast_signed(),
            }
        }
        ShaderVisibleDescriptor::UavTexelBuffer {
            buffer: Some(buffer),
            view,
            slice,
        } => {
            clear_uav.begin_texel_buffer_f32(&mut ctx, color, &buffer, view);
            Rect {
                left: i32::try_from(slice.first_element).unwrap_or(i32::MAX),
                top: 0,
                right: i32::try_from(
                    slice
                        .first_element
                        .wrapping_add(u64::from(slice.element_count)),
                )
                .unwrap_or(i32::MAX),
                bottom: 1,
            }
        }
        _ => {
            clear_uav.end(&mut ctx);
            alloc.clear_uav = Some(clear_uav);
            return;
        }
    };

    let (rect_ptr, rect_count) = if num_rects > 0 && !rects.is_null() {
        (rects.cast::<Rect>(), num_rects)
    } else {
        (&raw const full_rect, 1)
    };

    for i in 0..rect_count {
        // SAFETY: `rect_ptr` has `rect_count` readable `Rect` elements.
        let rect_elem = unsafe { rect_ptr.add(i as usize) };
        // SAFETY: `rect_elem` is a valid element of `rect_ptr`.
        let rect = unsafe { &*rect_elem };
        let width = rect.right.wrapping_sub(rect.left);
        let height = rect.bottom.wrapping_sub(rect.top);
        if width <= 0 || height <= 0 {
            continue;
        }
        clear_uav.clear(
            &mut ctx,
            rect.left.cast_unsigned(),
            rect.top.cast_unsigned(),
            width.cast_unsigned(),
            height.cast_unsigned(),
        );
    }

    clear_uav.end(&mut ctx);
    alloc.clear_uav = Some(clear_uav);
}

/// `ID3D12GraphicsCommandList::DiscardResource` (`d3d12_command_list.cpp:1842`).
///
/// # Safety
///
/// `this`, `resource`, and `region` are caller-supplied pointers.
pub unsafe fn discard_resource(this: *mut c_void, _resource: *mut c_void, _region: *const c_void) {
    crate::scope!("list.DiscardResource");
    let _ = this.expose_provenance();
}

unsafe fn keep_ending_resolve(list: &mut List, end: &RenderPassEndingAccess) {
    if end.r#type != D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_RESOLVE {
        return;
    }
    // SAFETY: `end.r#type == D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_RESOLVE` selects the
    // `resolve` union arm.
    let r = unsafe { &end.access.resolve };
    if r.subresource_parameters.is_null() || r.subresource_count == 0 {
        return;
    }
    for i in 0..r.subresource_count {
        // SAFETY: `r.subresource_parameters` has `r.subresource_count` readable elements.
        let sub_ptr = unsafe { r.subresource_parameters.add(i as usize) };
        // SAFETY: `sub_ptr` is a valid element of `r.subresource_parameters`.
        let s = unsafe { &*sub_ptr };
        let desc = copy::ResolveRegionDesc {
            dst_resource: r.dst_resource,
            dst_subresource: s.dst_subresource,
            dst_x: s.dst_x,
            dst_y: s.dst_y,
            src_resource: r.src_resource,
            src_subresource: s.src_subresource,
            src_rect: &raw const s.src_rect,
            format: r.format,
            resolve_mode: r.resolve_mode,
        };
        // SAFETY: `r.dst_resource`, `r.src_resource`, and `s.src_rect` come from the render pass
        // ending access descriptor.
        if let Some(pending) = unsafe { copy::prepare_pending_resolve(&desc) } {
            list.pending_resolves.push(pending);
        }
    }
}

/// `ID3D12GraphicsCommandList4::BeginRenderPass` (`d3d12_command_list.cpp:2234`).
///
/// # Safety
///
/// `this`, `render_targets`, and `depth_stencil` are caller-supplied pointers.
pub unsafe fn begin_render_pass(
    this: *mut c_void,
    num_render_targets: u32,
    render_targets: *const c_void,
    depth_stencil: *const c_void,
    _flags: u32,
) {
    crate::scope!("list.BeginRenderPass");
    if num_render_targets > D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT
        || (num_render_targets > 0 && render_targets.is_null())
    {
        return;
    }
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    list.pending_resolves.clear();
    list.inside_render_pass = true;

    let mut rtvs =
        [D3d12CpuDescriptorHandle { ptr: 0 }; D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT as usize];
    let rt_ptr = render_targets.cast::<RenderPassRenderTargetDesc>();
    for i in 0..num_render_targets {
        let idx = i as usize;
        // SAFETY: `rt_ptr` has `num_render_targets` readable `RenderPassRenderTargetDesc` elements.
        let rt_elem = unsafe { rt_ptr.add(idx) };
        // SAFETY: `rt_elem` is a valid element of `render_targets`.
        let rt = unsafe { &*rt_elem };
        rtvs[idx] = rt.cpu_descriptor;
        if rt.beginning_access.r#type == D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_CLEAR {
            // SAFETY: `r#type == CLEAR` selects `clear`.
            let clear_params = unsafe { &rt.beginning_access.access.clear };
            // SAFETY: `color` is 4 `f32` values for RTV clear.
            let color_ptr = unsafe { clear_params.clear_value.value.color.as_ptr() };
            // SAFETY: `this` and `color_ptr` are valid.
            unsafe {
                clear_render_target_view(this, rt.cpu_descriptor, color_ptr, 0, ptr::null());
            };
        }
        // SAFETY: `rt.ending_access` is part of the caller-supplied render target descriptor.
        unsafe { keep_ending_resolve(list, &rt.ending_access) };
    }

    let mut bind_dsv = false;
    let mut dsv_handle = D3d12CpuDescriptorHandle { ptr: 0 };
    if !depth_stencil.is_null() {
        // SAFETY: `depth_stencil` was checked non-null above.
        let ds = unsafe { &*depth_stencil.cast::<RenderPassDepthStencilDesc>() };
        dsv_handle = ds.cpu_descriptor;
        let mut clear = 0u32;
        if ds.depth_beginning_access.r#type == D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_CLEAR {
            clear |= D3D12_CLEAR_FLAG_DEPTH;
        }
        if ds.stencil_beginning_access.r#type == D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_CLEAR {
            clear |= D3D12_CLEAR_FLAG_STENCIL;
        }
        if clear != 0 {
            // SAFETY: `depth_beginning_access` union arm is `clear`.
            let depth_clear = unsafe { &ds.depth_beginning_access.access.clear };
            // SAFETY: `depth_clear.clear_value.value` holds `depth_stencil`.
            let depth = unsafe { depth_clear.clear_value.value.depth_stencil.depth };
            // SAFETY: `stencil_beginning_access` union arm is `clear`.
            let stencil_clear = unsafe { &ds.stencil_beginning_access.access.clear };
            // SAFETY: `stencil_clear.clear_value.value` holds `depth_stencil`.
            let stencil = unsafe { stencil_clear.clear_value.value.depth_stencil.stencil };
            // SAFETY: `this` is a valid `List` pointer.
            unsafe {
                clear_depth_stencil_view(
                    this,
                    ds.cpu_descriptor,
                    clear,
                    depth,
                    stencil,
                    0,
                    ptr::null(),
                );
            };
        }
        // SAFETY: `ds.depth_ending_access` and `ds.stencil_ending_access` are part of `ds`.
        unsafe { keep_ending_resolve(list, &ds.depth_ending_access) };
        // SAFETY: `ds.stencil_ending_access` is part of `ds`.
        unsafe { keep_ending_resolve(list, &ds.stencil_ending_access) };
        bind_dsv = ds.depth_beginning_access.r#type
            != D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_NO_ACCESS
            || ds.stencil_beginning_access.r#type
                != D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_NO_ACCESS;
    }

    let dsv_ptr = if bind_dsv {
        (&raw const dsv_handle).cast::<c_void>()
    } else {
        ptr::null()
    };
    // SAFETY: `this`, `rtvs`, and `dsv_ptr` are valid stack/caller pointers.
    unsafe {
        state::om_set_render_targets(
            this,
            num_render_targets,
            rtvs.as_ptr().cast::<c_void>(),
            0,
            dsv_ptr,
        );
    };
}

/// `ID3D12GraphicsCommandList4::EndRenderPass` (`d3d12_command_list.cpp:2280`).
///
/// # Safety
///
/// `this` is a valid `List` pointer.
pub unsafe fn end_render_pass(this: *mut c_void) {
    crate::scope!("list.EndRenderPass");
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    list.inside_render_pass = false;
    let resolves = core::mem::take(&mut list.pending_resolves);
    for pending in &resolves {
        copy::execute_pending_resolve(list, pending);
    }
}
