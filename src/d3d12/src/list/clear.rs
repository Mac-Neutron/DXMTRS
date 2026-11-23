//! RTV/DSV/UAV clears, `DiscardResource`, and `BeginRenderPass`/`EndRenderPass`
//!
//! (`d3d12_command_list.cpp`).

use core::{
    ffi::c_void,
    mem::{align_of, size_of},
    ptr,
};
use std::sync::Arc;

use dxmt_core::{
    format,
    resource::{Texture, TextureViewDescriptor},
    settings::{Flags, settings},
};
use winemetal_wire::{
    WMT_LOAD_ACTION_DONT_CARE, WMT_STORE_ACTION_DONT_CARE, WMT_STORE_ACTION_MULTISAMPLE_RESOLVE,
    WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE, WMTClearColor, WMTScissorRect, original_format,
};

use crate::{
    abi::{
        list_ops::{
            D3D12_CLEAR_FLAG_DEPTH, D3D12_CLEAR_FLAG_STENCIL,
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_CLEAR,
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD,
            D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_NO_ACCESS,
            D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD,
            D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_RESOLVE,
            D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_DEPTH,
            D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_STENCIL, D3D12_RENDER_PASS_FLAG_RESUMING_PASS,
            D3D12_RENDER_PASS_FLAG_SUSPENDING_PASS, D3D12_RESOLVE_MODE_AVERAGE,
            D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT, DiscardRegion, Rect,
            RenderPassDepthStencilDesc, RenderPassEndingAccess, RenderPassRenderTargetDesc,
        },
        resource::D3D12_RESOURCE_DIMENSION_TEXTURE3D,
        vtbl::{D3d12CpuDescriptorHandle, D3d12GpuDescriptorHandle},
    },
    allocator::Allocator,
    descriptor_heap::{self, ShaderVisibleDescriptor},
    encoder::{ClearEncoderData, EncoderData, EncoderType, RenderEncoderData, TextureViewRef},
    list::{DiscardedSubresource, List, PendingResolve, copy, state},
    resource::Resource,
};

fn purge_discarded(list: &mut List, tex_ptr: *const c_void) {
    if tex_ptr.is_null() || list.discarded_resources.is_empty() {
        return;
    }
    list.discarded_resources
        .retain(|entry| entry.tex_ptr != tex_ptr);
    if !list.allocator.is_null() {
        // SAFETY: `list.allocator` is valid while `list` is recording.
        let alloc = unsafe { &mut *list.allocator };
        alloc.discard_tracking = !list.discarded_resources.is_empty();
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
    purge_discarded(list, Arc::as_ptr(&texture).cast::<c_void>());

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };

    if !rects.is_null() {
        let Some(mut clear_rtv) = alloc.clear_rtv.take() else {
            return;
        };
        clear_rtv.begin(alloc, texture, attachment_desc.view, 0, checked_flags);
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
                alloc,
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
        clear_rtv.end(alloc);
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
    purge_discarded(list, Arc::as_ptr(&texture).cast::<c_void>());
    // SAFETY: `color_rgba` points to 4 `f32` elements.
    let color = unsafe { ptr::read_unaligned(color_rgba.cast::<[f32; 4]>()) };

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };

    if !rects.is_null() {
        let Some(mut clear_rtv) = alloc.clear_rtv.take() else {
            return;
        };
        clear_rtv.begin(
            alloc,
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
                alloc,
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
        clear_rtv.end(alloc);
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
    if let ShaderVisibleDescriptor::UavTexture {
        texture: Some(texture),
        ..
    } = &descriptor
    {
        purge_discarded(list, Arc::as_ptr(texture).cast::<c_void>());
    }
    // SAFETY: `values` points to 4 `u32` elements.
    let color = unsafe { ptr::read_unaligned(values.cast::<[u32; 4]>()) };

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };
    let Some(mut clear_uav) = alloc.clear_uav.take() else {
        return;
    };

    let full_rect = match descriptor {
        ShaderVisibleDescriptor::UavBuffer {
            buffer: Some(buffer),
            slice,
        } => {
            clear_uav.begin_buffer_u32(alloc, color, &buffer);
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
            clear_uav.begin_texture_u32(alloc, color, &texture, view);
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
            clear_uav.begin_texel_buffer_u32(alloc, color, &buffer, view);
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
            clear_uav.end(alloc);
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
            alloc,
            rect.left.cast_unsigned(),
            rect.top.cast_unsigned(),
            width.cast_unsigned(),
            height.cast_unsigned(),
        );
    }

    clear_uav.end(alloc);
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
    if let ShaderVisibleDescriptor::UavTexture {
        texture: Some(texture),
        ..
    } = &descriptor
    {
        purge_discarded(list, Arc::as_ptr(texture).cast::<c_void>());
    }
    // SAFETY: `values` points to 4 `f32` elements.
    let color = unsafe { ptr::read_unaligned(values.cast::<[f32; 4]>()) };

    // SAFETY: `list.allocator` was checked non-null above.
    let alloc = unsafe { &mut *list.allocator };
    let Some(mut clear_uav) = alloc.clear_uav.take() else {
        return;
    };

    let full_rect = match descriptor {
        ShaderVisibleDescriptor::UavBuffer {
            buffer: Some(buffer),
            slice,
        } => {
            clear_uav.begin_buffer_f32(alloc, color, &buffer);
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
            clear_uav.begin_texture_f32(alloc, color, &texture, view);
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
            clear_uav.begin_texel_buffer_f32(alloc, color, &buffer, view);
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
            clear_uav.end(alloc);
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
            alloc,
            rect.left.cast_unsigned(),
            rect.top.cast_unsigned(),
            width.cast_unsigned(),
            height.cast_unsigned(),
        );
    }

    clear_uav.end(alloc);
    alloc.clear_uav = Some(clear_uav);
}

/// `ID3D12GraphicsCommandList::DiscardResource` (`d3d12_command_list.cpp:1842`).
///
/// # Safety
///
/// `this`, `resource`, and `region` are caller-supplied pointers.
/// `ID3D12GraphicsCommandList::DiscardResource` (`d3d12_command_list.cpp`).
///
/// # Safety
///
/// `this`, `resource`, and `region` are caller-supplied pointers.
pub unsafe fn discard_resource(this: *mut c_void, resource: *mut c_void, region: *const c_void) {
    crate::scope!("list.DiscardResource");
    if this.is_null() || resource.is_null() || !settings().flags.contains(Flags::DISCARD_LOAD_STORE)
    {
        return;
    }
    // SAFETY: `this` is a valid `List` pointer.
    let list = unsafe { &mut *this.cast::<List>() };
    // SAFETY: `resource` is a valid `Resource` pointer.
    let res = unsafe { &*resource.cast::<Resource>() };
    let Some(tex) = res.texture.as_ref() else {
        return;
    };
    let tex_ptr = Arc::as_ptr(tex).cast::<c_void>();
    if region.is_null() {
        list.discarded_resources.push(DiscardedSubresource {
            tex_ptr,
            mip: 0,
            slice: 0,
            plane: 0,
            all: true,
        });
    } else {
        // SAFETY: `region` points to a readable `DiscardRegion`.
        let reg = unsafe { ptr::read_unaligned(region.cast::<DiscardRegion>()) };
        if reg.num_rects != 0 || reg.num_subresources == 0 {
            return;
        }
        let mip_levels = u32::from(res.desc.mip_levels).max(1);
        let array_size = if res.desc.dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
            1
        } else {
            u32::from(res.desc.depth_or_array_size).max(1)
        };
        let plane_stride = mip_levels.saturating_mul(array_size);
        let max_expand = 64_u32;
        if reg.num_subresources > max_expand {
            list.discarded_resources.push(DiscardedSubresource {
                tex_ptr,
                mip: 0,
                slice: 0,
                plane: 0,
                all: true,
            });
        } else {
            for i in 0..reg.num_subresources {
                let sub = reg.first_subresource.saturating_add(i);
                let plane = sub / plane_stride;
                let rem = sub % plane_stride;
                let slice = rem / mip_levels;
                let mip = rem % mip_levels;
                list.discarded_resources.push(DiscardedSubresource {
                    tex_ptr,
                    mip,
                    slice,
                    plane,
                    all: false,
                });
            }
        }
    }
    if !list.allocator.is_null() {
        // SAFETY: `list.allocator` is valid while `list` is recording.
        let alloc = unsafe { &mut *list.allocator };
        alloc.discard_tracking = true;
    }
}

unsafe fn keep_ending_resolve(list: &mut List, end: &RenderPassEndingAccess, is_color: bool) {
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
            let eligible = is_color
                && r.subresource_count == 1
                && r.resolve_mode == D3D12_RESOLVE_MODE_AVERAGE
                && pending.same_size;
            let preserve = r.preserve_resolve_source != 0;
            list.pending_resolves.push(pending);
            list.render_pass_state
                .pending_resolve_inline
                .push((eligible, preserve));
        }
    }
}

fn try_inline_resolve(
    render: &mut RenderEncoderData,
    pending: &PendingResolve,
    preserve: bool,
) -> Option<(*const c_void, Arc<Texture>)> {
    let (src_view, dst_view) = (pending.src_view.as_ref()?, pending.dst_view.as_ref()?);
    if src_view.descriptor.info().sample_count() <= 1
        || dst_view.descriptor.info().sample_count() > 1
    {
        return None;
    }
    let (hr, fmt_desc) = format::query_dxgi(src_view.key.format());
    if hr < 0 {
        return None;
    }
    let target_fmt = original_format(fmt_desc.pixel_format);
    for color in &mut render.colors {
        let Some(att) = color.attachment.as_ref() else {
            continue;
        };
        if color.resolve_attachment.is_some() {
            continue;
        }
        if !Arc::ptr_eq(&att.descriptor, &src_view.descriptor)
            || att.key.array_size() != 1
            || original_format(att.key.format()) != target_fmt
        {
            continue;
        }
        let att_mip = att.key.first_mip().saturating_add(u32::from(color.level));
        let att_slice = att.key.first_slice().saturating_add(u32::from(color.slice));
        if att_mip != src_view.key.first_mip() || att_slice != src_view.key.first_slice() {
            continue;
        }
        if (att.descriptor.info().width >> att_mip).max(1) != pending.src_w
            || (att.descriptor.info().height >> att_mip).max(1) != pending.src_h
        {
            continue;
        }

        let dst_level = dst_view.key.first_mip();
        let dst_slice = dst_view.key.first_slice();
        let (resolve_key, resolve_level, resolve_slice) =
            if original_format(dst_view.descriptor.info().pixel_format) == target_fmt {
                (
                    dst_view.descriptor.full_view(),
                    u16::try_from(dst_level).ok()?,
                    u16::try_from(dst_slice).ok()?,
                )
            } else {
                let key = TextureViewDescriptor {
                    format: fmt_desc.pixel_format,
                    texture_type: dst_view.descriptor.info().r#type(),
                    first_mip: dst_level,
                    mip_count: 1,
                    first_slice: dst_slice,
                    array_size: 1,
                }
                .key();
                (key, 0_u16, 0_u16)
            };
        let resolve_ref = TextureViewRef::from_texture(&dst_view.descriptor, resolve_key)?;
        if resolve_ref.texture() == 0 {
            return None;
        }
        color.resolve_attachment = Some(resolve_ref);
        color.resolve_level = resolve_level;
        color.resolve_slice = resolve_slice;
        color.resolve_depth_plane = 0;
        color.store_action = if preserve {
            WMT_STORE_ACTION_STORE_AND_MULTISAMPLE_RESOLVE
        } else {
            WMT_STORE_ACTION_MULTISAMPLE_RESOLVE
        };
        return Some((dst_view.descriptor_ptr(), Arc::clone(&dst_view.descriptor)));
    }
    None
}

fn apply_ro_dsv_render_pass_flags(list: &mut List, flags: u32) {
    let mut extra_ro = 0_u8;
    if (flags & D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_DEPTH) != 0 {
        extra_ro |= 1;
    }
    if (flags & D3D12_RENDER_PASS_FLAG_BIND_READ_ONLY_STENCIL) != 0 {
        extra_ro |= 2;
    }
    if extra_ro == 0 {
        return;
    }
    list.render_pass.dsv_readonly_flags |= extra_ro;
    if (extra_ro & 1) != 0 && list.render_pass.depth.attachment.is_some() {
        list.render_pass.depth.store_action = WMT_STORE_ACTION_DONT_CARE;
    }
    if (extra_ro & 2) != 0 && list.render_pass.stencil.attachment.is_some() {
        list.render_pass.stencil.store_action = WMT_STORE_ACTION_DONT_CARE;
    }
    let ro = list.render_pass.dsv_readonly_flags;
    if (list.render_pass.dsv_planar_flags & !ro) == 0
        && let Some(dsv_att) = list.render_pass.depth.attachment.as_ref().or(list
            .render_pass
            .stencil
            .attachment
            .as_ref())
    {
        let dsv_ptr_key = dsv_att.descriptor_ptr();
        let rtv_also_uses = list.render_pass.colors.iter().any(|c| {
            c.attachment
                .as_ref()
                .is_some_and(|a| a.descriptor_ptr() == dsv_ptr_key)
        });
        if !rtv_also_uses {
            let count = usize::from(list.render_pass.base.write_count);
            if let Some(pos) = list.render_pass.base.writes[..count]
                .iter()
                .position(|&p| p == dsv_ptr_key)
            {
                let tail_idx = count - 1;
                list.render_pass.base.writes[pos] = list.render_pass.base.writes[tail_idx];
                list.render_pass.base.writes[tail_idx] = ptr::null();
                list.render_pass.base.write_count -= 1;
            }
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
    flags: u32,
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
    list.render_pass_state.reset();
    // Ensure `om_set_render_targets` closes any prior pass before opening this render pass.
    list.inside_render_pass = false;

    let discard_enabled = settings().flags.contains(Flags::DISCARD_LOAD_STORE);
    let resuming = (flags & D3D12_RENDER_PASS_FLAG_RESUMING_PASS) != 0;
    let suspending = (flags & D3D12_RENDER_PASS_FLAG_SUSPENDING_PASS) != 0;

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
        } else if rt.beginning_access.r#type == D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD
            && discard_enabled
            && !resuming
        {
            list.render_pass_state.color_load_override[idx] = WMT_LOAD_ACTION_DONT_CARE;
        }
        if !suspending
            && rt.ending_access.r#type == D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD
            && discard_enabled
        {
            list.render_pass_state.color_ending_discard |= 1_u8 << i;
        }
        // SAFETY: `rt.ending_access` is part of the caller-supplied render target descriptor.
        unsafe { keep_ending_resolve(list, &rt.ending_access, true) };
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
        } else if ds.depth_beginning_access.r#type
            == D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD
            && discard_enabled
            && !resuming
        {
            list.render_pass_state.depth_load_override = WMT_LOAD_ACTION_DONT_CARE;
        }
        if ds.stencil_beginning_access.r#type == D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_CLEAR {
            clear |= D3D12_CLEAR_FLAG_STENCIL;
        } else if ds.stencil_beginning_access.r#type
            == D3D12_RENDER_PASS_BEGINNING_ACCESS_TYPE_DISCARD
            && discard_enabled
            && !resuming
        {
            list.render_pass_state.stencil_load_override = WMT_LOAD_ACTION_DONT_CARE;
        }
        if !suspending && discard_enabled {
            if ds.depth_ending_access.r#type == D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD {
                list.render_pass_state.dsv_ending_discard |= 1;
            }
            if ds.stencil_ending_access.r#type == D3D12_RENDER_PASS_ENDING_ACCESS_TYPE_DISCARD {
                list.render_pass_state.dsv_ending_discard |= 2;
            }
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
        // SAFETY: `ds.depth_ending_access` is part of `ds`.
        unsafe { keep_ending_resolve(list, &ds.depth_ending_access, false) };
        // SAFETY: `ds.stencil_ending_access` is part of `ds`.
        unsafe { keep_ending_resolve(list, &ds.stencil_ending_access, false) };
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
    if !depth_stencil.is_null() && bind_dsv {
        apply_ro_dsv_render_pass_flags(list, flags);
    }
    list.render_pass_state.active = true;
    list.render_pass_state.flags = flags;
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
    let rp_state = core::mem::take(&mut list.render_pass_state);
    let resolves = core::mem::take(&mut list.pending_resolves);

    let suspending = (rp_state.flags & D3D12_RENDER_PASS_FLAG_SUSPENDING_PASS) != 0;
    let discard_enabled = settings().flags.contains(Flags::DISCARD_LOAD_STORE);
    let mut handled_mask = 0_u64;
    let mut inline_writes: Vec<(*const c_void, Arc<Texture>)> = Vec::new();

    if list.inside_render_pass && !suspending && !list.allocator.is_null() {
        // SAFETY: `list.allocator` is valid while recording.
        let alloc: &mut Allocator = unsafe { &mut *list.allocator };
        if !alloc.encoder_current.is_null()
            // SAFETY: `alloc.encoder_current` is a live `EncoderData` node.
            && unsafe { (*alloc.encoder_current).kind } == EncoderType::Render
        {
            // SAFETY: `kind == EncoderType::Render` guarantees `RenderEncoderData`.
            let render = unsafe { &mut *alloc.encoder_current.cast::<RenderEncoderData>() };

            for (idx, pending) in resolves.iter().enumerate() {
                let Some(&(eligible, preserve)) = rp_state.pending_resolve_inline.get(idx) else {
                    continue;
                };
                if !eligible {
                    continue;
                }
                if let Some(write) = try_inline_resolve(render, pending, preserve) {
                    inline_writes.push(write);
                    if idx < 64 {
                        handled_mask |= 1_u64 << idx;
                    }
                }
            }

            if discard_enabled {
                for (slot, color) in render.colors.iter_mut().enumerate() {
                    if (rp_state.color_ending_discard & (1_u8 << slot)) != 0
                        && color.attachment.is_some()
                        && color.resolve_attachment.is_none()
                    {
                        color.store_action = WMT_STORE_ACTION_DONT_CARE;
                    }
                }
                if (rp_state.dsv_ending_discard & 1) != 0 && render.depth.attachment.is_some() {
                    render.depth.store_action = WMT_STORE_ACTION_DONT_CARE;
                }
                if (rp_state.dsv_ending_discard & 2) != 0 && render.stencil.attachment.is_some() {
                    render.stencil.store_action = WMT_STORE_ACTION_DONT_CARE;
                }
            }
        }
        for (tex_ptr, tex) in inline_writes {
            alloc.writes(tex_ptr);
            alloc.keep_texture(tex);
        }
    }

    list.inside_render_pass = false;
    for (idx, pending) in resolves.iter().enumerate() {
        if idx < 64 && (handled_mask & (1_u64 << idx)) != 0 {
            continue;
        }
        copy::execute_pending_resolve(list, pending);
    }
}

#[cfg(test)]
mod tests;
