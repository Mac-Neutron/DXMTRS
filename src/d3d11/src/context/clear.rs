//! View clear and discard operations (`Clear*`, `Discard*`).

use core::{ffi::c_void, ptr, slice};

use dxmt_core::{
    deptrack::Access,
    encoder::{Encoder, NO_NEXT, stage},
    format,
    library::CUSTOM_BUFFER_ARGUMENT_INDEX_0,
};
use winemetal_sys::wire::{
    MemPtr, WMT_COMPUTE_COMMAND_DISPATCH_THREADS, WMT_COMPUTE_COMMAND_SET_BUFFER,
    WMT_COMPUTE_COMMAND_SET_BYTES, WMT_COMPUTE_COMMAND_SET_PSO, WMT_COMPUTE_COMMAND_SET_TEXTURE,
    WMT_LOAD_ACTION_LOAD, WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_R32_UINT,
    WMT_PIXEL_FORMAT_RG11B10_FLOAT, WMT_PRIMITIVE_TYPE_TRIANGLE, WMT_RENDER_COMMAND_DRAW,
    WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE, WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
    WMT_RENDER_COMMAND_SET_PSO, WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
    WMT_RENDER_COMMAND_SET_STENCIL_REF, WMT_RENDER_COMMAND_SET_VIEWPORT, WMT_STORE_ACTION_STORE,
    WMT_TEXTURE_TYPE1D, WMT_TEXTURE_TYPE1D_ARRAY, WMT_TEXTURE_TYPE2D, WMT_TEXTURE_TYPE2D_ARRAY,
    WMT_TEXTURE_TYPE3D, WMTClearColor, WMTScissorRect, WMTSize, WMTViewport, WmtcmdComputeDispatch,
    WmtcmdComputeSetbuffer, WmtcmdComputeSetbytes, WmtcmdComputeSetpso, WmtcmdComputeSettexture,
    WmtcmdRenderDraw, WmtcmdRenderSetbytes, WmtcmdRenderSetdepthstencilstate, WmtcmdRenderSetpso,
    WmtcmdRenderSetscissorrect, WmtcmdRenderSetstencilref, WmtcmdRenderSetviewport,
};

use super::{Context, Mode};
use crate::{
    abi::records::Rect,
    view::{Dsv, Rtv, Uav},
};

/// `DXGI_FORMAT_UNKNOWN`.
const DXGI_FORMAT_UNKNOWN: u32 = 0;
/// `D3D11_CLEAR_DEPTH`.
const D3D11_CLEAR_DEPTH: u32 = 1;

/// Threadgroup size for linear buffer / texel-buffer clears.
const LINEAR_GROUP: WMTSize = WMTSize {
    width: 32,
    height: 1,
    depth: 1,
};

/// Threadgroup size for 2D / 3D texture clears.
const TILE_GROUP: WMTSize = WMTSize {
    width: 8,
    height: 4,
    depth: 1,
};

#[cfg(target_pointer_width = "64")]
const fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr { ptr: addr as u64 }
}

#[cfg(target_pointer_width = "32")]
fn mem_ptr(addr: usize) -> MemPtr {
    MemPtr {
        ptr: u32::try_from(addr).unwrap_or(0),
        high: 0,
    }
}

/// Binds a copy of `words` at compute buffer slot `index`.
fn set_compute_words<const N: usize>(enc: &mut Encoder, words: &[u32; N], index: u8) {
    let byte_len = N * 4;
    let copy = enc.allocate_cpu(byte_len, 8);
    // SAFETY: `copy` has `byte_len` bytes in the encoder's arena, disjoint from `words`.
    unsafe {
        ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>(), copy.as_ptr(), byte_len);
    }
    enc.encode_compute_command(WmtcmdComputeSetbytes {
        r#type: WMT_COMPUTE_COMMAND_SET_BYTES,
        reserved: [0; 3],
        next: NO_NEXT,
        bytes: mem_ptr(copy.as_ptr().expose_provenance()),
        length: byte_len as u64,
        index,
    });
}

/// Binds a copy of `words` at fragment buffer slot `index`.
fn set_fragment_words<const N: usize>(enc: &mut Encoder, words: &[u32; N], index: u8) {
    let byte_len = N * 4;
    let copy = enc.allocate_cpu(byte_len, 16);
    // SAFETY: `copy` has `byte_len` bytes in the encoder's arena, disjoint from `words`.
    unsafe {
        ptr::copy_nonoverlapping(words.as_ptr().cast::<u8>(), copy.as_ptr(), byte_len);
    }
    enc.encode_render_command(WmtcmdRenderSetbytes {
        r#type: WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
        reserved: [0; 3],
        next: NO_NEXT,
        bytes: mem_ptr(copy.as_ptr().expose_provenance()),
        length: byte_len as u64,
        index,
    });
}

/// Encodes an emulated buffer clear dispatch (`EmulatedCommandContext::ClearBufferUint` / `Float`).
fn encode_clear_buffer_uint(
    enc: &mut Encoder,
    buffer: u64,
    byte_offset: u64,
    elements: u32,
    value: [u32; 4],
) {
    enc.encode_compute_command(WmtcmdComputeSetpso {
        r#type: WMT_COMPUTE_COMMAND_SET_PSO,
        reserved: [0; 3],
        next: NO_NEXT,
        pso: 0,
        threadgroup_size: LINEAR_GROUP,
    });
    enc.encode_compute_command(WmtcmdComputeSetbuffer {
        r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
        reserved: [0; 3],
        next: NO_NEXT,
        buffer,
        offset: byte_offset,
        index: 0,
    });
    set_compute_words(enc, &value, 1);
    set_compute_words(enc, &[elements], 2);
    enc.encode_compute_command(WmtcmdComputeDispatch {
        r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
        reserved: [0; 3],
        next: NO_NEXT,
        size: WMTSize {
            width: u64::from(elements),
            height: 1,
            depth: 1,
        },
    });
}

/// Encodes an emulated texel-buffer clear dispatch (`ClearTextureBufferUint` / `Float`).
fn encode_clear_texture_buffer(
    enc: &mut Encoder,
    texture: u64,
    offset: u32,
    size: u32,
    value: [u32; 4],
) {
    enc.encode_compute_command(WmtcmdComputeSetpso {
        r#type: WMT_COMPUTE_COMMAND_SET_PSO,
        reserved: [0; 3],
        next: NO_NEXT,
        pso: 0,
        threadgroup_size: LINEAR_GROUP,
    });
    enc.encode_compute_command(WmtcmdComputeSettexture {
        r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
        reserved: [0; 3],
        next: NO_NEXT,
        texture,
        index: 0,
    });
    let [a, b, c, d] = value;
    set_compute_words(enc, &[a, b, c, d, offset, size, 0, 0], 1);
    enc.encode_compute_command(WmtcmdComputeDispatch {
        r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
        reserved: [0; 3],
        next: NO_NEXT,
        size: WMTSize {
            width: u64::from(size),
            height: 1,
            depth: 1,
        },
    });
}

/// Encodes an emulated 2D / 2D-array / 3D texture clear dispatch (`ClearTexture*`).
fn encode_clear_texture(enc: &mut Encoder, texture: u64, size: [u32; 3], value: [u32; 4]) {
    enc.encode_compute_command(WmtcmdComputeSetpso {
        r#type: WMT_COMPUTE_COMMAND_SET_PSO,
        reserved: [0; 3],
        next: NO_NEXT,
        pso: 0,
        threadgroup_size: TILE_GROUP,
    });
    enc.encode_compute_command(WmtcmdComputeSettexture {
        r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
        reserved: [0; 3],
        next: NO_NEXT,
        texture,
        index: 0,
    });
    set_compute_words(enc, &value, 1);
    enc.encode_compute_command(WmtcmdComputeDispatch {
        r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
        reserved: [0; 3],
        next: NO_NEXT,
        size: WMTSize {
            width: u64::from(size[0]),
            height: u64::from(size[1]),
            depth: u64::from(size[2]),
        },
    });
}

impl<M: Mode> Context<M> {
    pub fn clear_render_target_view(
        &mut self,
        render_target_view: *mut c_void,
        color_rgba: *const f32,
    ) {
        if render_target_view.is_null() || color_rgba.is_null() {
            return;
        }
        let Some(rtv) = Rtv::from_iface(render_target_view) else {
            return;
        };
        let (Some(texture), Some(view_key)) = (rtv.texture.clone(), rtv.texture_view) else {
            return;
        };
        // SAFETY: `color_rgba` points to a 4-float RGBA array per the D3D11 API contract.
        let rgba = unsafe { *(color_rgba.cast::<[f32; 4]>()) };
        self.invalidate_current_pass(false);
        let array_length = rtv.array_length;
        let clear_color = WMTClearColor {
            r: f64::from(rgba[0]),
            g: f64::from(rgba[1]),
            b: f64::from(rgba[2]),
            a: f64::from(rgba[3]),
        };
        // C++ closure (`ClearRenderTargetView`, d3d11_context_impl.cpp:4391).
        M::emit_op(self, move |enc| {
            enc.clear_color(&texture, view_key, array_length, clear_color);
        });
    }

    pub fn clear_unordered_access_view_uint(
        &mut self,
        unordered_access_view: *mut c_void,
        values: *const u32,
    ) {
        if unordered_access_view.is_null() || values.is_null() {
            return;
        }
        let Some(uav) = Uav::from_iface(unordered_access_view) else {
            return;
        };
        // SAFETY: `values` points to a 4-uint array per the D3D11 API contract.
        let uint_vals = unsafe { *(values.cast::<[u32; 4]>()) };

        self.invalidate_current_pass(true);
        self.switch_to_compute_encoder();
        let is_live = self.is_live();

        if let Some(buffer) = uav.buffer.clone() {
            let byte_offset = uav.slice.byte_offset;
            let byte_length = uav.slice.byte_length;
            let first_element = uav.slice.first_element;
            let element_count = uav.slice.element_count;
            let uav_format = uav.format;
            let buffer_view = uav.buffer_view;
            // C++ closure (`ClearUnorderedAccessViewUint` buffer, d3d11_context_impl.cpp:718).
            M::emit_op(self, move |enc| {
                if uav_format == DXGI_FORMAT_UNKNOWN || buffer_view.is_none() {
                    if let Some((alloc, sub_off)) =
                        enc.access_buffer::<{ stage::COMPUTE }>(&buffer, Access::WRITE)
                    {
                        if is_live {
                            let buf_h = alloc.buffer();
                            let off = u64::from(byte_offset) + sub_off;
                            let elems = u64::from(byte_length >> 2);
                            enc.with_emulated(|emu, enc| {
                                emu.clear_buffer_uint(enc, buf_h, off, elems, uint_vals);
                            });
                        } else {
                            encode_clear_buffer_uint(
                                enc,
                                alloc.buffer(),
                                u64::from(byte_offset) + sub_off,
                                byte_length >> 2,
                                uint_vals,
                            );
                        }
                    }
                } else if let Some(view_key) = buffer_view
                    && let Some((view, texel_offset)) = enc
                        .access_buffer_view::<{ stage::COMPUTE }>(&buffer, view_key, Access::WRITE)
                {
                    let offset = first_element + u32::try_from(texel_offset).unwrap_or(0);
                    if is_live {
                        let tex_h = view.texture;
                        enc.with_emulated(|emu, enc| {
                            emu.clear_texture_buffer_uint(
                                enc,
                                tex_h,
                                offset,
                                element_count,
                                uint_vals,
                            );
                        });
                    } else {
                        encode_clear_texture_buffer(
                            enc,
                            view.texture,
                            offset,
                            element_count,
                            uint_vals,
                        );
                    }
                }
            });
        } else if let (Some(texture), Some(view_key)) = (uav.texture.clone(), uav.texture_view) {
            // C++ closure (`ClearUnorderedAccessViewUint` texture, d3d11_context_impl.cpp:735).
            M::emit_op(self, move |enc| {
                let view_format = view_key.format();
                let mut uint_format = format::unsigned_integer(view_format);
                let mut value = uint_vals;
                if view_format == WMT_PIXEL_FORMAT_RG11B10_FLOAT {
                    uint_format = WMT_PIXEL_FORMAT_R32_UINT;
                    value[0] = (value[0] & 0x7FF)
                        | ((value[1] & 0x7FF) << 11)
                        | ((value[2] & 0x3FF) << 22);
                }
                if uint_format == WMT_PIXEL_FORMAT_INVALID {
                    return;
                }
                let view_checked = view_key.with_format(uint_format);
                let Some(view) = enc.access_texture_view::<{ stage::COMPUTE }>(
                    &texture,
                    view_checked,
                    Access::WRITE,
                ) else {
                    return;
                };
                let [w, h, d] = texture.view_size(view_checked);
                let tex_h = view.texture;
                match view_checked.texture_type() {
                    WMT_TEXTURE_TYPE1D | WMT_TEXTURE_TYPE2D => {
                        if is_live {
                            enc.with_emulated(|emu, enc| {
                                emu.clear_texture_2d_uint(
                                    enc,
                                    tex_h,
                                    [u64::from(w), u64::from(h)],
                                    value,
                                );
                            });
                        } else {
                            encode_clear_texture(enc, tex_h, [w, h, 1], value);
                        }
                    }
                    WMT_TEXTURE_TYPE1D_ARRAY | WMT_TEXTURE_TYPE2D_ARRAY => {
                        let slices = view_checked.array_size();
                        if is_live {
                            enc.with_emulated(|emu, enc| {
                                emu.clear_texture_2d_array_uint(
                                    enc,
                                    tex_h,
                                    [u64::from(w), u64::from(h), u64::from(slices)],
                                    value,
                                );
                            });
                        } else {
                            encode_clear_texture(enc, tex_h, [w, h, slices], value);
                        }
                    }
                    WMT_TEXTURE_TYPE3D => {
                        if is_live {
                            enc.with_emulated(|emu, enc| {
                                emu.clear_texture_3d_uint(
                                    enc,
                                    tex_h,
                                    [u64::from(w), u64::from(h), u64::from(d)],
                                    value,
                                );
                            });
                        } else {
                            encode_clear_texture(enc, tex_h, [w, h, d], value);
                        }
                    }
                    _ => {}
                }
            });
        }
        self.invalidate_current_pass(false);
    }

    pub fn clear_unordered_access_view_float(
        &mut self,
        unordered_access_view: *mut c_void,
        values: *const f32,
    ) {
        if unordered_access_view.is_null() || values.is_null() {
            return;
        }
        let Some(uav) = Uav::from_iface(unordered_access_view) else {
            return;
        };
        // SAFETY: `values` points to a 4-float array per the D3D11 API contract.
        let raw_vals = unsafe { *(values.cast::<[f32; 4]>()) };
        let bits = raw_vals.map(f32::to_bits);

        self.invalidate_current_pass(true);
        self.switch_to_compute_encoder();
        let is_live = self.is_live();

        if let Some(buffer) = uav.buffer.clone() {
            let byte_offset = uav.slice.byte_offset;
            let byte_length = uav.slice.byte_length;
            let first_element = uav.slice.first_element;
            let element_count = uav.slice.element_count;
            let uav_format = uav.format;
            let buffer_view = uav.buffer_view;
            // C++ closure (`ClearUnorderedAccessViewFloat` buffer, d3d11_context_impl.cpp:788).
            M::emit_op(self, move |enc| {
                if uav_format == DXGI_FORMAT_UNKNOWN || buffer_view.is_none() {
                    if let Some((alloc, sub_off)) =
                        enc.access_buffer::<{ stage::COMPUTE }>(&buffer, Access::WRITE)
                    {
                        if is_live {
                            let buf_h = alloc.buffer();
                            let off = u64::from(byte_offset) + sub_off;
                            let elems = u64::from(byte_length >> 2);
                            enc.with_emulated(|emu, enc| {
                                emu.clear_buffer_float(enc, buf_h, off, elems, raw_vals);
                            });
                        } else {
                            encode_clear_buffer_uint(
                                enc,
                                alloc.buffer(),
                                u64::from(byte_offset) + sub_off,
                                byte_length >> 2,
                                bits,
                            );
                        }
                    }
                } else if let Some(view_key) = buffer_view
                    && let Some((view, texel_offset)) = enc
                        .access_buffer_view::<{ stage::COMPUTE }>(&buffer, view_key, Access::WRITE)
                {
                    let offset = first_element + u32::try_from(texel_offset).unwrap_or(0);
                    if is_live {
                        let tex_h = view.texture;
                        enc.with_emulated(|emu, enc| {
                            emu.clear_texture_buffer_float(
                                enc,
                                tex_h,
                                offset,
                                element_count,
                                raw_vals,
                            );
                        });
                    } else {
                        encode_clear_texture_buffer(enc, view.texture, offset, element_count, bits);
                    }
                }
            });
        } else if let (Some(texture), Some(view_key)) = (uav.texture.clone(), uav.texture_view) {
            // C++ closure (`ClearUnorderedAccessViewFloat` texture, d3d11_context_impl.cpp:804).
            M::emit_op(self, move |enc| {
                let Some(view) = enc.access_texture_view::<{ stage::COMPUTE }>(
                    &texture,
                    view_key,
                    Access::WRITE,
                ) else {
                    return;
                };
                let [w, h, d] = texture.view_size(view_key);
                let tex_h = view.texture;
                match view_key.texture_type() {
                    WMT_TEXTURE_TYPE1D | WMT_TEXTURE_TYPE2D => {
                        if is_live {
                            enc.with_emulated(|emu, enc| {
                                emu.clear_texture_2d_float(
                                    enc,
                                    tex_h,
                                    [u64::from(w), u64::from(h)],
                                    raw_vals,
                                );
                            });
                        } else {
                            encode_clear_texture(enc, tex_h, [w, h, 1], bits);
                        }
                    }
                    WMT_TEXTURE_TYPE1D_ARRAY | WMT_TEXTURE_TYPE2D_ARRAY => {
                        let slices = view_key.array_size();
                        if is_live {
                            enc.with_emulated(|emu, enc| {
                                emu.clear_texture_2d_array_float(
                                    enc,
                                    tex_h,
                                    [u64::from(w), u64::from(h), u64::from(slices)],
                                    raw_vals,
                                );
                            });
                        } else {
                            encode_clear_texture(enc, tex_h, [w, h, slices], bits);
                        }
                    }
                    WMT_TEXTURE_TYPE3D => {
                        if is_live {
                            enc.with_emulated(|emu, enc| {
                                emu.clear_texture_3d_float(
                                    enc,
                                    tex_h,
                                    [u64::from(w), u64::from(h), u64::from(d)],
                                    raw_vals,
                                );
                            });
                        } else {
                            encode_clear_texture(enc, tex_h, [w, h, d], bits);
                        }
                    }
                    _ => {}
                }
            });
        }
        self.invalidate_current_pass(false);
    }

    pub fn clear_depth_stencil_view(
        &mut self,
        depth_stencil_view: *mut c_void,
        clear_flags: u32,
        depth: f32,
        stencil: u8,
    ) {
        if depth_stencil_view.is_null() || clear_flags == 0 {
            return;
        }
        let Some(dsv) = Dsv::from_iface(depth_stencil_view) else {
            return;
        };
        let (Some(texture), Some(view_key)) = (dsv.texture.clone(), dsv.texture_view) else {
            return;
        };
        let renamable = if self.is_live() {
            crate::view::DsvObject::from_iface(depth_stencil_view).and_then(|o| o.renamable.clone())
        } else {
            None
        };
        let array_length = dsv.array_length;
        let masked_flags = clear_flags & 0b11;
        self.invalidate_current_pass(false);
        // C++ closure (`ClearDepthStencilView`, d3d11_context_impl.cpp:4405).
        M::emit_op(self, move |enc| {
            if let Some(dyn_tex) = renamable.as_ref()
                && masked_flags == format::depth_stencil_planar_flags(texture.info().pixel_format)
                && let Ok(mut pool) = dyn_tex.lock()
            {
                let _ = texture.rename(pool.next_allocation(enc.current_frame_id()));
            }
            enc.clear_depth_stencil(
                &texture,
                view_key,
                array_length,
                masked_flags,
                depth,
                stencil,
            );
        });
    }

    pub fn clear_view(
        &mut self,
        view: *mut c_void,
        color: *const f32,
        rect: *const c_void,
        num_rects: u32,
    ) {
        if (num_rects != 0 && rect.is_null()) || view.is_null() || color.is_null() {
            return;
        }
        // SAFETY: `color` points to a 4-float array per the D3D11 API contract.
        let rgba = unsafe { *(color.cast::<[f32; 4]>()) };
        let rects: &[Rect] = if !rect.is_null() && num_rects > 0 {
            // SAFETY: caller supplied `num_rects` elements at `rect`.
            unsafe { slice::from_raw_parts(rect.cast::<Rect>(), num_rects as usize) }
        } else {
            &[]
        };

        if let Some(rtv) = Rtv::from_iface(view) {
            self.clear_view_rtv(view, color, rtv, rgba, rects, num_rects);
            return;
        }
        if let Some(dsv) = Dsv::from_iface(view) {
            self.clear_view_dsv(view, dsv, rgba, rects, num_rects);
            return;
        }
        if let Some(uav) = Uav::from_iface(view) {
            self.clear_view_uav(uav, rgba, rects);
        }
    }

    fn clear_view_rtv(
        &mut self,
        view: *mut c_void,
        color: *const f32,
        rtv: &Rtv,
        rgba: [f32; 4],
        rects: &[Rect],
        num_rects: u32,
    ) {
        let (Some(texture), Some(view_key)) = (rtv.texture.clone(), rtv.texture_view) else {
            return;
        };
        if view_key.texture_type() == WMT_TEXTURE_TYPE3D {
            return;
        }
        if num_rects <= 1 {
            let full = rects.first().is_none_or(|r| {
                r.top == 0
                    && r.left == 0
                    && r.bottom >= 0
                    && r.right >= 0
                    && r.right.cast_unsigned() == rtv.width
                    && r.bottom.cast_unsigned() == rtv.height
            });
            if full {
                self.clear_render_target_view(view, color);
                return;
            }
        }
        self.invalidate_current_pass(true);
        let depth_plane =
            crate::view::RtvObject::from_iface(view).map_or(0, |o| o.pass_desc.depth_plane);
        let array_length = rtv.array_length;
        if self.is_live() {
            let tex_begin = texture;
            M::emit_st(self, move |enc| {
                let tex = tex_begin.clone();
                enc.with_clear_rtv(|crtv, enc| {
                    crtv.begin(enc, tex, view_key, depth_plane, 0);
                });
            });
            for r in rects {
                let rect_offset_x = r.left.max(0).cast_unsigned();
                let rect_offset_y = r.top.max(0).cast_unsigned();
                let rect_width = r.right - rect_offset_x.cast_signed();
                let rect_height = r.bottom - rect_offset_y.cast_signed();
                if rect_height <= 0 || rect_width <= 0 {
                    continue;
                }
                let w_u = rect_width.cast_unsigned();
                let h_u = rect_height.cast_unsigned();
                M::emit_op(self, move |enc| {
                    enc.with_clear_rtv(|crtv, enc| {
                        crtv.clear_color(
                            enc,
                            WMTScissorRect {
                                x: u64::from(rect_offset_x),
                                y: u64::from(rect_offset_y),
                                width: u64::from(w_u),
                                height: u64::from(h_u),
                            },
                            array_length,
                            rgba,
                        );
                    });
                });
            }
            M::emit_st(self, move |enc| {
                enc.with_clear_rtv(dxmt_core::library::ClearRtv::end);
            });
            return;
        }
        let tex_begin = texture;
        // C++ closure (`ClearView` RTV begin, d3d11_context_impl.cpp:866).
        M::emit_st(self, move |enc| {
            let [w, h, _] = tex_begin.view_size(view_key);
            enc.start_render_pass(0, 0, 1, None);
            let att = enc.attachment::<{ stage::PIXEL }>(&tex_begin, view_key, Access::WRITE);
            if let Some(targets) = enc.render_targets() {
                targets.colors[0].target.attachment = att;
                targets.colors[0].target.load_action = WMT_LOAD_ACTION_LOAD;
                targets.colors[0].target.store_action = WMT_STORE_ACTION_STORE;
                targets.render_target_width = w;
                targets.render_target_height = h;
                targets.render_target_array_length =
                    u16::try_from(view_key.array_size()).unwrap_or(0);
                targets.default_raster_sample_count =
                    u8::try_from(tex_begin.info().sample_count()).unwrap_or(1);
            }
            enc.encode_render_command(WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: NO_NEXT,
                pso: 0,
            });
            enc.encode_render_command(WmtcmdRenderSetviewport {
                r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
                reserved: [0; 3],
                next: NO_NEXT,
                viewport: WMTViewport {
                    origin_x: 0.0,
                    origin_y: 0.0,
                    width: f64::from(w),
                    height: f64::from(h),
                    znear: 0.0,
                    zfar: 1.0,
                },
            });
            enc.encode_render_command(WmtcmdRenderSetdepthstencilstate {
                r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
                reserved: [0; 3],
                next: NO_NEXT,
                depth_stencil_state: 0,
            });
        });
        let color_bits = rgba.map(f32::to_bits);
        for r in rects {
            let rect_offset_x = r.left.max(0).cast_unsigned();
            let rect_offset_y = r.top.max(0).cast_unsigned();
            let rect_width = r.right - rect_offset_x.cast_signed();
            let rect_height = r.bottom - rect_offset_y.cast_signed();
            if rect_height <= 0 || rect_width <= 0 {
                continue;
            }
            let w_u = rect_width.cast_unsigned();
            let h_u = rect_height.cast_unsigned();
            // C++ closure (`ClearView` RTV rect, d3d11_context_impl.cpp:877).
            M::emit_op(self, move |enc| {
                enc.encode_render_command(WmtcmdRenderSetscissorrect {
                    r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    scissor_rect: WMTScissorRect {
                        x: u64::from(rect_offset_x),
                        y: u64::from(rect_offset_y),
                        width: u64::from(w_u),
                        height: u64::from(h_u),
                    },
                });
                let idx = u8::try_from(CUSTOM_BUFFER_ARGUMENT_INDEX_0).unwrap_or(30);
                set_fragment_words(enc, &color_bits, idx);
                enc.encode_render_command(WmtcmdRenderDraw {
                    r#type: WMT_RENDER_COMMAND_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                    vertex_start: 0,
                    vertex_count: 3,
                    instance_count: array_length.max(1),
                    base_instance: 0,
                });
            });
        }
        // C++ closure (`ClearView` RTV end, d3d11_context_impl.cpp:881).
        M::emit_st(self, Encoder::end_pass);
    }

    fn clear_view_dsv(
        &mut self,
        view: *mut c_void,
        dsv: &Dsv,
        rgba: [f32; 4],
        rects: &[Rect],
        num_rects: u32,
    ) {
        let (Some(texture), Some(view_key)) = (dsv.texture.clone(), dsv.texture_view) else {
            return;
        };
        if view_key.texture_type() == WMT_TEXTURE_TYPE3D {
            return;
        }
        if num_rects <= 1 {
            let full = rects.first().is_none_or(|r| {
                r.top == 0
                    && r.left == 0
                    && r.bottom >= 0
                    && r.right >= 0
                    && r.right.cast_unsigned() == dsv.width
                    && r.bottom.cast_unsigned() == dsv.height
            });
            if full {
                self.clear_depth_stencil_view(view, D3D11_CLEAR_DEPTH, rgba[0], 0);
                return;
            }
        }
        self.invalidate_current_pass(true);
        let depth_val = rgba[0];
        if self.is_live() {
            let tex_begin = texture;
            M::emit_st(self, move |enc| {
                let tex = tex_begin.clone();
                enc.with_clear_rtv(|crtv, enc| {
                    crtv.begin(enc, tex, view_key, 0, D3D11_CLEAR_DEPTH);
                });
            });
            for r in rects {
                let rect_offset_x = r.left.max(0).cast_unsigned();
                let rect_offset_y = r.top.max(0).cast_unsigned();
                let rect_width = r.right - rect_offset_x.cast_signed();
                let rect_height = r.bottom - rect_offset_y.cast_signed();
                if rect_height <= 0 || rect_width <= 0 {
                    continue;
                }
                let w_u = rect_width.cast_unsigned();
                let h_u = rect_height.cast_unsigned();
                M::emit_op(self, move |enc| {
                    enc.with_clear_rtv(|crtv, enc| {
                        crtv.clear_depth_stencil(
                            enc,
                            WMTScissorRect {
                                x: u64::from(rect_offset_x),
                                y: u64::from(rect_offset_y),
                                width: u64::from(w_u),
                                height: u64::from(h_u),
                            },
                            depth_val,
                            0,
                        );
                    });
                });
            }
            M::emit_st(self, move |enc| {
                enc.with_clear_rtv(dxmt_core::library::ClearRtv::end);
            });
            return;
        }
        let tex_begin = texture;
        // C++ closure (`ClearView` DSV begin, d3d11_context_impl.cpp:907).
        M::emit_st(self, move |enc| {
            let [w, h, _] = tex_begin.view_size(view_key);
            enc.start_render_pass(1, 0, 0, None);
            let att = enc.attachment::<{ stage::PIXEL }>(&tex_begin, view_key, Access::WRITE);
            if let Some(targets) = enc.render_targets() {
                targets.depth.target.attachment = att;
                targets.depth.target.load_action = WMT_LOAD_ACTION_LOAD;
                targets.depth.target.store_action = WMT_STORE_ACTION_STORE;
                targets.render_target_width = w;
                targets.render_target_height = h;
                targets.render_target_array_length =
                    u16::try_from(view_key.array_size()).unwrap_or(0);
                targets.default_raster_sample_count =
                    u8::try_from(tex_begin.info().sample_count()).unwrap_or(1);
            }
            enc.encode_render_command(WmtcmdRenderSetpso {
                r#type: WMT_RENDER_COMMAND_SET_PSO,
                reserved: [0; 3],
                next: NO_NEXT,
                pso: 0,
            });
            enc.encode_render_command(WmtcmdRenderSetviewport {
                r#type: WMT_RENDER_COMMAND_SET_VIEWPORT,
                reserved: [0; 3],
                next: NO_NEXT,
                viewport: WMTViewport {
                    origin_x: 0.0,
                    origin_y: 0.0,
                    width: f64::from(w),
                    height: f64::from(h),
                    znear: 0.0,
                    zfar: 1.0,
                },
            });
            enc.encode_render_command(WmtcmdRenderSetdepthstencilstate {
                r#type: WMT_RENDER_COMMAND_SET_DEPTH_STENCIL_STATE,
                reserved: [0; 3],
                next: NO_NEXT,
                depth_stencil_state: 0,
            });
        });
        let array_slices = view_key.array_size().max(1);
        for r in rects {
            let rect_offset_x = r.left.max(0).cast_unsigned();
            let rect_offset_y = r.top.max(0).cast_unsigned();
            let rect_width = r.right - rect_offset_x.cast_signed();
            let rect_height = r.bottom - rect_offset_y.cast_signed();
            if rect_height <= 0 || rect_width <= 0 {
                continue;
            }
            let w_u = rect_width.cast_unsigned();
            let h_u = rect_height.cast_unsigned();
            // C++ closure (`ClearView` DSV rect, d3d11_context_impl.cpp:918).
            M::emit_op(self, move |enc| {
                enc.encode_render_command(WmtcmdRenderSetscissorrect {
                    r#type: WMT_RENDER_COMMAND_SET_SCISSOR_RECT,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    scissor_rect: WMTScissorRect {
                        x: u64::from(rect_offset_x),
                        y: u64::from(rect_offset_y),
                        width: u64::from(w_u),
                        height: u64::from(h_u),
                    },
                });
                let idx = u8::try_from(CUSTOM_BUFFER_ARGUMENT_INDEX_0).unwrap_or(30);
                let bits = [depth_val.to_bits(); 4];
                set_fragment_words(enc, &bits, idx);
                enc.encode_render_command(WmtcmdRenderSetstencilref {
                    r#type: WMT_RENDER_COMMAND_SET_STENCIL_REF,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    stencil_ref: 0,
                });
                enc.encode_render_command(WmtcmdRenderDraw {
                    r#type: WMT_RENDER_COMMAND_DRAW,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                    vertex_start: 0,
                    vertex_count: 3,
                    instance_count: array_slices,
                    base_instance: 0,
                });
            });
        }
        // C++ closure (`ClearView` DSV end, d3d11_context_impl.cpp:922).
        M::emit_st(self, Encoder::end_pass);
    }

    fn clear_view_uav(&mut self, uav: &Uav, rgba: [f32; 4], rects: &[Rect]) {
        self.invalidate_current_pass(true);
        if self.is_live() {
            let tex_opt = uav.texture.clone();
            let tex_vk = uav.texture_view;
            let buf_opt = uav.buffer.clone();
            let buf_vk = uav.buffer_view;
            if let (Some(tex), Some(vk)) = (tex_opt, tex_vk) {
                let is_int = format::is_integer(vk.format());
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "D3D11 ClearView converts non-negative f32 components to u32"
                )]
                let uint_color = rgba.map(|v| v.max(0.0) as u32);
                M::emit_st(self, move |enc| {
                    enc.with_clear_uav(|cuav, enc| {
                        if is_int {
                            cuav.begin_texture_u32(enc, uint_color, &tex, vk);
                        } else {
                            cuav.begin_texture_f32(enc, rgba, &tex, vk);
                        }
                    });
                });
            } else if let (Some(buf), Some(vk)) = (buf_opt.clone(), buf_vk) {
                let is_int = format::is_integer(vk.format());
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "D3D11 ClearView converts non-negative f32 components to u32"
                )]
                let uint_color = rgba.map(|v| v.max(0.0) as u32);
                M::emit_st(self, move |enc| {
                    enc.with_clear_uav(|cuav, enc| {
                        if is_int {
                            cuav.begin_texel_buffer_u32(enc, uint_color, &buf, vk);
                        } else {
                            cuav.begin_texel_buffer_f32(enc, rgba, &buf, vk);
                        }
                    });
                });
            } else if let Some(buf) = buf_opt {
                M::emit_st(self, move |enc| {
                    enc.with_clear_uav(|cuav, enc| {
                        cuav.begin_buffer_f32(enc, rgba, &buf);
                    });
                });
            } else {
                return;
            }
            for r in rects {
                let rect_offset_x = r.left.max(0).cast_unsigned();
                let rect_offset_y = r.top.max(0).cast_unsigned();
                let rect_width = r.right - rect_offset_x.cast_signed();
                let rect_height = r.bottom - rect_offset_y.cast_signed();
                if rect_height <= 0 || rect_width <= 0 {
                    continue;
                }
                let w_u = rect_width.cast_unsigned();
                let h_u = rect_height.cast_unsigned();
                M::emit_op(self, move |enc| {
                    enc.with_clear_uav(|cuav, enc| {
                        cuav.clear(enc, rect_offset_x, rect_offset_y, w_u, h_u);
                    });
                });
            }
            M::emit_st(self, move |enc| {
                enc.with_clear_uav(dxmt_core::library::ClearUav::end);
            });
            return;
        }
        let color_bits = rgba.map(f32::to_bits);
        if uav.buffer.is_none() {
            // C++ closure (`ClearView` UAV texture begin, d3d11_context_impl.cpp:935).
            M::emit_st(self, move |enc| {
                enc.start_compute_pass(None);
                enc.encode_compute_command(WmtcmdComputeSetpso {
                    r#type: WMT_COMPUTE_COMMAND_SET_PSO,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    pso: 0,
                    threadgroup_size: LINEAR_GROUP,
                });
            });
        } else if uav.buffer_view.is_some() {
            // C++ closure (`ClearView` UAV buffer view begin, d3d11_context_impl.cpp:940).
            M::emit_st(self, move |enc| {
                enc.start_compute_pass(None);
                enc.encode_compute_command(WmtcmdComputeSetpso {
                    r#type: WMT_COMPUTE_COMMAND_SET_PSO,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    pso: 0,
                    threadgroup_size: LINEAR_GROUP,
                });
            });
        } else {
            // C++ closure (`ClearView` UAV raw buffer begin, d3d11_context_impl.cpp:944).
            M::emit_st(self, move |enc| {
                enc.start_compute_pass(None);
                enc.encode_compute_command(WmtcmdComputeSetpso {
                    r#type: WMT_COMPUTE_COMMAND_SET_PSO,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    pso: 0,
                    threadgroup_size: LINEAR_GROUP,
                });
            });
        }
        for r in rects {
            let rect_offset_x = r.left.max(0).cast_unsigned();
            let rect_offset_y = r.top.max(0).cast_unsigned();
            let rect_width = r.right - rect_offset_x.cast_signed();
            let rect_height = r.bottom - rect_offset_y.cast_signed();
            if rect_height <= 0 || rect_width <= 0 {
                continue;
            }
            let w_u = rect_width.cast_unsigned();
            let h_u = rect_height.cast_unsigned();
            let tex_opt = uav.texture.clone();
            let tex_vk = uav.texture_view;
            let buf_opt = uav.buffer.clone();
            let buf_vk = uav.buffer_view;
            // C++ closure (`ClearView` UAV rect, d3d11_context_impl.cpp:956).
            M::emit_op(self, move |enc| {
                let mut dispatch_depth = 1_u32;
                if let (Some(tex), Some(vk)) = (&tex_opt, tex_vk) {
                    let view_tex = enc
                        .access_texture_view::<{ stage::COMPUTE }>(tex, vk, Access::WRITE)
                        .map_or(0, |v| v.texture);
                    enc.encode_compute_command(WmtcmdComputeSettexture {
                        r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        texture: view_tex,
                        index: 0,
                    });
                    dispatch_depth = match vk.texture_type() {
                        WMT_TEXTURE_TYPE1D_ARRAY | WMT_TEXTURE_TYPE2D_ARRAY => vk.array_size(),
                        WMT_TEXTURE_TYPE3D => tex.view_size(vk)[2],
                        _ => 1,
                    };
                } else if let (Some(buf), Some(vk)) = (&buf_opt, buf_vk) {
                    let view_tex = enc
                        .access_buffer_view::<{ stage::COMPUTE }>(buf, vk, Access::WRITE)
                        .map_or(0, |(v, _)| v.texture);
                    enc.encode_compute_command(WmtcmdComputeSettexture {
                        r#type: WMT_COMPUTE_COMMAND_SET_TEXTURE,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        texture: view_tex,
                        index: 0,
                    });
                } else if let Some(buf) = &buf_opt {
                    let handle = enc
                        .access_buffer::<{ stage::COMPUTE }>(buf, Access::WRITE)
                        .map_or(0, |(a, _)| a.buffer());
                    enc.encode_compute_command(WmtcmdComputeSetbuffer {
                        r#type: WMT_COMPUTE_COMMAND_SET_BUFFER,
                        reserved: [0; 3],
                        next: NO_NEXT,
                        buffer: handle,
                        offset: 0,
                        index: 0,
                    });
                }
                let [c0, c1, c2, c3] = color_bits;
                set_compute_words(
                    enc,
                    &[c0, c1, c2, c3, rect_offset_x, rect_offset_y, w_u, h_u],
                    1,
                );
                enc.encode_compute_command(WmtcmdComputeDispatch {
                    r#type: WMT_COMPUTE_COMMAND_DISPATCH_THREADS,
                    reserved: [0; 3],
                    next: NO_NEXT,
                    size: WMTSize {
                        width: u64::from(w_u),
                        height: u64::from(h_u),
                        depth: u64::from(dispatch_depth),
                    },
                });
            });
        }
        // C++ closure (`ClearView` UAV end, d3d11_context_impl.cpp:960).
        M::emit_st(self, Encoder::end_pass);
    }

    pub const fn discard_resource(&mut self, resource: *mut c_void) {
        let _ = resource;
    }

    pub const fn discard_view(&mut self, resource_view: *mut c_void) {
        self.discard_view1(resource_view, ptr::null(), 0);
    }

    pub const fn discard_view1(
        &mut self,
        resource_view: *mut c_void,
        rects: *const c_void,
        num_rects: u32,
    ) {
        let _ = (resource_view, rects, num_rects);
    }
}
