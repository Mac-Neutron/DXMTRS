//! `IMTLD3D11ContextExt` / `Ext1` / `Ext2` implementation (`d3d11_context_impl.cpp:5195-5429`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::{
    encoder::TemporalTextures,
    frame_stats::ScalerType,
    resource::{Texture, TextureAllocationFlags, TextureViewDescriptor},
    scaler::Scaler,
};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_RG16_FLOAT, WMT_PIXEL_FORMAT_RG16_SINT,
    WMT_PIXEL_FORMAT_RG16_SNORM, WMT_PIXEL_FORMAT_RG16_UINT, WMT_PIXEL_FORMAT_RG16_UNORM,
    WMT_PIXEL_FORMAT_RG32_FLOAT, WMT_PIXEL_FORMAT_RG32_SINT, WMT_PIXEL_FORMAT_RG32_UINT,
    WMT_PIXEL_FORMAT_RGBA16_FLOAT, WMT_PIXEL_FORMAT_RGBA16_SINT, WMT_PIXEL_FORMAT_RGBA16_SNORM,
    WMT_PIXEL_FORMAT_RGBA16_UINT, WMT_PIXEL_FORMAT_RGBA16_UNORM, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
    WMT_TEXTURE_TYPE2D, WMT_TEXTURE_USAGE_SHADER_READ, WMT_TEXTURE_USAGE_SHADER_WRITE,
    WMTFXTemporalScalerInfo, WMTFXTemporalScalerProps, WMTTextureInfo,
};

use super::{
    Context, Mode,
    immediate::{live_device, push_encoder_trace, resource_common},
    state::DirtyFlags,
};
use crate::{
    LOG,
    abi::ext::{D3D11Piece, D3D11UpscaleDesc, FEATURE_METALFX_TEMPORAL_SCALER},
    com::{E_INVALIDARG, E_NOTIMPL, Hresult, S_OK},
};

const D3D11_BIND_VERTEX_BUFFER: u32 = 0x1;
const D3D11_BIND_INDEX_BUFFER: u32 = 0x2;
const D3D11_BIND_CONSTANT_BUFFER: u32 = 0x4;
const D3D11_BIND_SHADER_RESOURCE: u32 = 0x8;

fn piece_id(allocation: u64, suballocation: u32) -> u64 {
    (allocation << 16) | u64::from(suballocation)
}

const fn read_upscale_desc(desc: *const D3D11UpscaleDesc) -> D3D11UpscaleDesc {
    // SAFETY: `desc` was verified non-null by `temporal_upscale`.
    unsafe { desc.read_unaligned() }
}

const fn write_piece(pieces: *mut D3D11Piece, index: usize, piece: D3D11Piece) {
    // SAFETY: `pieces` has at least `index + 1` elements per `get_pieces` contract.
    let slot = unsafe { pieces.add(index) };
    // SAFETY: `slot` points to a valid element within `pieces`.
    unsafe {
        slot.write_unaligned(piece);
    }
}

const fn get_correct_motion_vector_format(format: u32) -> u32 {
    match format {
        WMT_PIXEL_FORMAT_RG16_UINT
        | WMT_PIXEL_FORMAT_RG16_FLOAT
        | WMT_PIXEL_FORMAT_RG16_SINT
        | WMT_PIXEL_FORMAT_RG16_SNORM
        | WMT_PIXEL_FORMAT_RG16_UNORM => WMT_PIXEL_FORMAT_RG16_FLOAT,
        WMT_PIXEL_FORMAT_RG32_UINT | WMT_PIXEL_FORMAT_RG32_FLOAT | WMT_PIXEL_FORMAT_RG32_SINT => {
            WMT_PIXEL_FORMAT_RG32_FLOAT
        }
        WMT_PIXEL_FORMAT_RGBA16_SINT
        | WMT_PIXEL_FORMAT_RGBA16_SNORM
        | WMT_PIXEL_FORMAT_RGBA16_UINT
        | WMT_PIXEL_FORMAT_RGBA16_UNORM
        | WMT_PIXEL_FORMAT_RGBA16_FLOAT => WMT_PIXEL_FORMAT_RGBA16_FLOAT,
        _ => WMT_PIXEL_FORMAT_INVALID,
    }
}

impl<M: Mode> Context<M> {
    /// `IMTLD3D11ContextExt::TemporalUpscale`.
    pub fn temporal_upscale(&mut self, desc: *const D3D11UpscaleDesc) {
        if desc.is_null() {
            return;
        }
        let d = read_upscale_desc(desc);
        let live_inputs = live_device(self.device).and_then(|dev| {
            let input = Arc::clone(resource_common(d.color)?.texture.as_ref()?);
            let output = Arc::clone(resource_common(d.output)?.texture.as_ref()?);
            let depth = Arc::clone(resource_common(d.depth)?.texture.as_ref()?);
            let motion_vector = Arc::clone(resource_common(d.motion_vector)?.texture.as_ref()?);
            let exposure = if d.exposure_texture.is_null() {
                None
            } else {
                Some(Arc::clone(
                    resource_common(d.exposure_texture)?.texture.as_ref()?,
                ))
            };
            Some((dev, input, output, depth, motion_vector, exposure))
        });

        let Some((dev, input, output, depth, motion_vector, exposure)) = live_inputs else {
            let [scale_x, scale_y] = d.motion_vector_scale;
            let need_mv_scale = (scale_x - 1.0).abs() > f32::EPSILON
                || (scale_y - 1.0).abs() > f32::EPSILON
                || d.motion_vector_in_display_res != 0;
            if need_mv_scale {
                self.begin_compute_pass();
                M::emit_op(self, move |enc| {
                    push_encoder_trace(enc, "MvScale");
                });
            }
            self.finish_pass(false);
            M::emit_op(self, move |enc| {
                push_encoder_trace(enc, "UpscaleTemporal");
            });
            return;
        };

        let motion_vector_format =
            get_correct_motion_vector_format(motion_vector.info().pixel_format);
        if motion_vector_format == WMT_PIXEL_FORMAT_INVALID {
            LOG.err("TemporalUpscale: invalid motion vector format");
            return;
        }

        let input_width = input.info().width;
        let input_height = input.info().height;
        let output_width = output.info().width;
        let output_height = output.info().height;
        // SAFETY: `WMTFXTemporalScalerInfo` is a C ABI struct of plain integers and floats.
        let mut info: WMTFXTemporalScalerInfo = unsafe { core::mem::zeroed() };
        info.color_format = input.info().pixel_format;
        info.output_format = output.info().pixel_format;
        info.depth_format = depth.info().pixel_format;
        info.motion_format = motion_vector_format;
        info.input_width = input_width;
        info.input_height = input_height;
        info.output_width = output_width;
        info.output_height = output_height;
        info.input_content_min_scale = 1.0;
        info.input_content_max_scale = 3.0;
        info.input_content_properties_enabled = 1;
        info.auto_exposure = u8::from(d.auto_exposure != 0);
        info.requires_synchronous_initialization = 1;
        // SAFETY: `dev.core().device().handle()` is a live `MTLDevice`.
        let scaler = Arc::new(unsafe { Scaler::temporal(dev.core().device().handle(), &info) });
        let mv_downscaled = if d.motion_vector_in_display_res != 0 {
            let usage = WMT_TEXTURE_USAGE_SHADER_READ | WMT_TEXTURE_USAGE_SHADER_WRITE;
            let tex_info = WMTTextureInfo {
                pixel_format: motion_vector_format,
                width: input_width,
                height: input_height,
                depth: 1,
                array_length: 1,
                bits: WMT_TEXTURE_TYPE2D
                    | (1 << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
                    | (1 << WMTTextureInfo::SAMPLE_COUNT_SHIFT)
                    | (usage << WMTTextureInfo::USAGE_SHIFT),
                options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
                flags: 0,
                mach_port: 0,
                gpu_resource_id: 0,
            };
            let tex = Arc::new(Texture::new(tex_info, dev.core().device()));
            let alloc = tex.allocate(TextureAllocationFlags::GPU_PRIVATE);
            tex.rename(alloc);
            Some(tex)
        } else {
            None
        };

        // SAFETY: `WMTFXTemporalScalerProps` is a C ABI struct of plain integers and floats.
        let mut props: WMTFXTemporalScalerProps = unsafe { core::mem::zeroed() };
        props.input_content_width = d.input_content_width;
        props.input_content_height = d.input_content_height;
        props.reset = u8::from(d.in_reset != 0);
        props.depth_reversed = u8::from(d.depth_reversed != 0);
        props.motion_vector_scale_x = d.motion_vector_scale[0];
        props.motion_vector_scale_y = d.motion_vector_scale[1];
        props.jitter_offset_x = d.jitter_offset[0];
        props.jitter_offset_y = d.jitter_offset[1];
        props.pre_exposure = d.pre_exposure;

        self.finish_pass(false);
        M::emit_op(self, move |enc| {
            let mv_view = TextureViewDescriptor {
                format: motion_vector_format,
                texture_type: WMT_TEXTURE_TYPE2D,
                first_mip: 0,
                mip_count: 1,
                first_slice: 0,
                array_size: 1,
            }
            .key();
            let highres = mv_downscaled.is_some();
            if let Some(stats) = enc.current_frame_statistics() {
                stats.last_scaler_info.kind = ScalerType::Temporal;
                stats.last_scaler_info.auto_exposure = exposure.is_none();
                stats.last_scaler_info.input_width = props.input_content_width;
                stats.last_scaler_info.input_height = props.input_content_height;
                stats.last_scaler_info.output_width = output.info().width;
                stats.last_scaler_info.output_height = output.info().height;
                stats.last_scaler_info.motion_vector_highres = highres;
            }
            if let Some(mv_down) = &mv_downscaled {
                enc.mv_scale_dispatch_views(
                    &motion_vector,
                    mv_view,
                    mv_down,
                    mv_down.full_view(),
                    props.motion_vector_scale_x,
                    props.motion_vector_scale_y,
                );
                let mut new_props = props;
                new_props.motion_vector_scale_x = 1.0;
                new_props.motion_vector_scale_y = 1.0;
                enc.upscale_temporal(
                    &TemporalTextures {
                        input: &input,
                        output: &output,
                        depth: &depth,
                        motion_vector: mv_down,
                        motion_vector_view: mv_down.full_view(),
                        exposure: exposure.as_deref(),
                    },
                    Arc::clone(&scaler),
                    new_props,
                );
            } else {
                enc.upscale_temporal(
                    &TemporalTextures {
                        input: &input,
                        output: &output,
                        depth: &depth,
                        motion_vector: &motion_vector,
                        motion_vector_view: mv_view,
                        exposure: exposure.as_deref(),
                    },
                    Arc::clone(&scaler),
                    props,
                );
            }
        });
    }

    /// `IMTLD3D11ContextExt::BeginUAVOverlap`.
    pub fn begin_uav_overlap(&mut self) {
        M::emit_op(self, |enc| {
            enc.set_intrapass_barrier_control(1);
            push_encoder_trace(enc, "BarrierControl(0)");
        });
    }

    /// `IMTLD3D11ContextExt::EndUAVOverlap`.
    pub fn end_uav_overlap(&mut self) {
        M::emit_op(self, |enc| {
            enc.set_intrapass_barrier_control(0);
            push_encoder_trace(enc, "BarrierControl(1)");
        });
    }

    /// `IMTLD3D11ContextExt1::CheckFeatureSupport`.
    pub const fn check_ext_feature_support(
        &mut self,
        feature: u32,
        feature_data: *mut c_void,
        feature_data_size: u32,
    ) -> Hresult {
        if feature != FEATURE_METALFX_TEMPORAL_SCALER
            || feature_data.is_null()
            || (feature_data_size as usize) != size_of::<u32>()
        {
            return E_INVALIDARG;
        }
        // SAFETY: `feature_data` was checked for null and `feature_data_size == 4`.
        unsafe {
            feature_data.cast::<u32>().write_unaligned(1);
        }
        S_OK
    }

    /// `IMTLD3D11ContextExt2::GetPieces`.
    pub fn get_pieces(
        &mut self,
        resource: *mut c_void,
        subresource: u32,
        count: u32,
        pieces: *mut D3D11Piece,
    ) -> Hresult {
        let Some(imm) = self.as_immediate() else {
            return E_NOTIMPL;
        };
        if resource.is_null() || pieces.is_null() {
            return E_INVALIDARG;
        }
        let Some(common) = resource_common(resource) else {
            return E_INVALIDARG;
        };
        let (dyn_buf, row_pitch, depth_pitch) = if let Some(b) = common.dynamic_buffer.as_ref() {
            let len = u32::try_from(b.buffer().length()).unwrap_or(0);
            (b, len, len)
        } else if let Some((b, rp, dp)) =
            crate::resource::dynamic_texture_subresource(resource, subresource)
        {
            (b, rp, dp)
        } else {
            return E_NOTIMPL;
        };
        if count == 0 {
            let piece = D3D11Piece {
                id: piece_id(dyn_buf.immediate_piece(), dyn_buf.immediate_suballocation()),
                cpu: dyn_buf.immediate_mapped_memory().cast(),
                row_pitch,
                depth_pitch,
            };
            write_piece(pieces, 0, piece);
            return S_OK;
        }
        let coherent = imm.active_coherent_seq();
        let total = count as usize;
        let mut i = 0_usize;
        while i < total {
            let (id, alloc) = dyn_buf.lend(coherent);
            let mut sub = 0_u32;
            while i < total && sub < 0x1_0000 && alloc.has_suballocation(sub) {
                let piece = D3D11Piece {
                    id: piece_id(id, sub),
                    cpu: alloc.mapped_memory(sub).cast(),
                    row_pitch,
                    depth_pitch,
                };
                write_piece(pieces, i, piece);
                sub += 1;
                i += 1;
            }
        }
        S_OK
    }

    /// `IMTLD3D11ContextExt2::UsePiece`.
    pub fn use_piece(&mut self, resource: *mut c_void, subresource: u32, piece: u64) {
        if resource.is_null() {
            return;
        }
        let Some(common) = resource_common(resource) else {
            return;
        };
        let (dyn_buf, bind_flags, tex_pitches) = if let Some(b) = common.dynamic_buffer.as_ref() {
            (b, common.bind_flags, None)
        } else if let Some((b, rp, dp)) =
            crate::resource::dynamic_texture_subresource(resource, subresource)
        {
            (b, common.bind_flags, Some((rp, dp)))
        } else {
            return;
        };
        let Some(imm) = self.as_immediate_mut() else {
            return;
        };
        let id = piece >> 16;
        let mut sub = (piece & 0xffff) as u32;
        if id == dyn_buf.immediate_piece() {
            if !dyn_buf.immediate_name().has_suballocation(sub) {
                return;
            }
            dyn_buf.use_immediate_suballocation(sub);
            let alloc = dyn_buf.immediate_name();
            super::Immediate::emit_st(imm, move |_enc| {
                alloc.use_suballocation(sub);
            });
        } else {
            let Some(lent) = dyn_buf.take_lent(id) else {
                return;
            };
            if !lent.has_suballocation(sub) {
                sub = 0;
            }
            let seq = imm.active_seq();
            dyn_buf.update_immediate_name(seq, Arc::clone(&lent), sub, false);
            dyn_buf.set_immediate_piece(id);
            let alloc = dyn_buf.immediate_name();
            let buf = Arc::clone(dyn_buf.buffer());
            super::Immediate::emit_st(imm, move |_enc| {
                alloc.use_suballocation(sub);
                let _ = buf.rename(Arc::clone(&alloc));
            });
        }
        if let Some((row_pitch, depth_pitch)) = tex_pitches {
            for stage in &mut imm.state.shader_stages {
                stage.srvs.set_all_dirty();
            }
            imm.update_texture_from_buffer(
                resource,
                subresource,
                ptr::null(),
                Arc::clone(dyn_buf.buffer()),
                row_pitch,
                depth_pitch,
            );
            return;
        }
        if (bind_flags & D3D11_BIND_VERTEX_BUFFER) != 0 {
            imm.dirty.insert(DirtyFlags::VERTEX_BUFFERS);
            imm.state.input_assembler.vertex_buffers.set_all_dirty();
        }
        if (bind_flags & D3D11_BIND_INDEX_BUFFER) != 0 {
            imm.dirty.insert(DirtyFlags::INDEX_BUFFER);
        }
        if (bind_flags & D3D11_BIND_CONSTANT_BUFFER) != 0 {
            for stage in &mut imm.state.shader_stages {
                stage.constant_buffers.set_all_dirty();
            }
        }
        if (bind_flags & D3D11_BIND_SHADER_RESOURCE) != 0 {
            for stage in &mut imm.state.shader_stages {
                stage.srvs.set_all_dirty();
            }
        }
    }
}

#[cfg(test)]
mod tests;
