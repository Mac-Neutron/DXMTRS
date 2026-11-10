//! Retained shader inputs for MSL regeneration and AIR translation.
//!
//! The explicit encoding carries DXSO tokens and every specialization input,
//! never Rust struct memory. Shader identities are shared with the live draw path.
//! A fixed-function source keeps its `FfVsKey`/`FfPsKey`, encoded field by field and
//! validated on decode, beside the tokens `ff_build` made from it.

use std::sync::Arc;

use airconv_ir::dxso::{
    link::Semantic,
    spec::{StageSpec, VsSpec},
};

use super::RecipeReader;
use crate::{
    draw_data::{ps_source_disk_key_ff, vs_source_disk_key_ff},
    dxso::{
        self, DxsoProgram, FfPsKey, FfStage, FfStageFlags, FfVsFlags, FfVsKey, VariantFlags,
        VariantKey, VsSamplerKinds,
    },
    ff_build::{self, FfProgram},
    ids::ProgramId,
    shader_key::{CachedKind, ps_source_disk_key_programmable, vs_source_disk_key_programmable},
    vs_draw::MAX_CLIP_PLANES,
};

/// Most world matrices `D3DRS_VERTEXBLEND` blends: `D3DVBF_3WEIGHTS`, three weights, one implied.
const MAX_BLEND_MATRICES: u8 = 4;

/// DXSO and the stage-specific inputs needed to reproduce a compiled variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderSource {
    tokens: Arc<[u32]>,
    specialization: Specialization,
}

impl ShaderSource {
    #[must_use]
    pub fn vertex(
        program: &DxsoProgram,
        provided_input_mask: u16,
        clip_plane_count: u8,
        sampler_kinds: VsSamplerKinds,
    ) -> Self {
        Self {
            tokens: program.bytecode().clone(),
            specialization: Specialization::Vertex {
                provided_input_mask,
                clip_plane_count,
                sampler_kinds,
            },
        }
    }

    #[must_use]
    pub fn pixel(program: &DxsoProgram, variant: VariantKey) -> Self {
        Self {
            tokens: program.bytecode().clone(),
            specialization: Specialization::Pixel(variant),
        }
    }

    /// A fixed-function vertex stage: `key` and the program `ff_build::vertex` built from it.
    #[must_use]
    pub fn ff_vertex(key: &FfVsKey, program: &FfProgram) -> Self {
        Self {
            tokens: program.tokens.clone(),
            specialization: Specialization::FfVertex {
                key: key.clone(),
                provided_input_mask: program.provided_input_mask,
            },
        }
    }

    /// A fixed-function pixel stage: `key`, its variant and the program `ff_build::pixel` built.
    #[must_use]
    pub fn ff_pixel(key: &FfPsKey, variant: VariantKey, program: &FfProgram) -> Self {
        Self {
            tokens: program.tokens.clone(),
            specialization: Specialization::FfPixel {
                key: key.clone(),
                variant,
            },
        }
    }

    /// The retained DXSO token stream, header through end token.
    #[must_use]
    pub fn tokens(&self) -> &[u32] {
        &self.tokens
    }

    /// The specialization the AIR translator reads, as `emit` gives it to the MSL emitters.
    #[must_use]
    pub const fn stage_spec(&self) -> StageSpec {
        match &self.specialization {
            Specialization::Vertex {
                provided_input_mask,
                clip_plane_count,
                sampler_kinds,
            } => StageSpec::Vs(VsSpec {
                provided_input_mask: *provided_input_mask,
                clip_plane_count: *clip_plane_count,
                sampler_kinds: *sampler_kinds,
            }),
            Specialization::FfVertex {
                key,
                provided_input_mask,
            } => StageSpec::Vs(VsSpec {
                provided_input_mask: *provided_input_mask,
                clip_plane_count: key.clip_plane_count,
                sampler_kinds: VsSamplerKinds {
                    volume_mask: 0,
                    cube_mask: 0,
                    lod_table: false,
                },
            }),
            Specialization::Pixel(variant) | Specialization::FfPixel { variant, .. } => {
                StageSpec::Ps(*variant)
            }
        }
    }

    /// The record key: the MSL twin's, so an AIR record and its MSL twin share it.
    #[must_use]
    pub fn disk_key(&self) -> u64 {
        match &self.specialization {
            Specialization::Vertex {
                provided_input_mask,
                clip_plane_count,
                sampler_kinds,
            } => vs_source_disk_key_programmable(
                ProgramId::from_tokens(&self.tokens),
                *provided_input_mask,
                *clip_plane_count,
                *sampler_kinds,
            ),
            Specialization::Pixel(variant) => {
                ps_source_disk_key_programmable(ProgramId::from_tokens(&self.tokens), *variant)
            }
            Specialization::FfVertex { key, .. } => vs_source_disk_key_ff(key),
            Specialization::FfPixel { key, variant } => ps_source_disk_key_ff(key, *variant),
        }
    }

    /// This fixed-function source's key built again, when the build differs from the stored one.
    ///
    /// `None` for a programmable source and for a current one. The builders are outside the
    /// translator key, so a builder change shows here: tokens or input mask differ.
    #[must_use]
    pub fn rebuilt_ff(&self) -> Option<Self> {
        let fresh = match &self.specialization {
            Specialization::FfVertex { key, .. } => Self::ff_vertex(key, &ff_build::vertex(key)),
            Specialization::FfPixel { key, variant } => {
                Self::ff_pixel(key, *variant, &ff_build::pixel(key))
            }
            Specialization::Vertex { .. } | Specialization::Pixel(_) => return None,
        };
        (fresh != *self).then_some(fresh)
    }

    /// Reparse retained DXSO and emit the same variant using the current translator.
    ///
    /// # Errors
    ///
    /// Returns the parser or emitter diagnostic when this translator cannot rebuild the source.
    pub fn emit(&self, entry: &str) -> Result<String, String> {
        let parse = || dxso::parse(&self.tokens).map_err(|error| format!("DXSO parse: {error:?}"));
        let result = match &self.specialization {
            Specialization::Vertex {
                provided_input_mask,
                clip_plane_count,
                sampler_kinds,
            } => dxso::emit_vs_programmable_named(
                &parse()?,
                entry,
                *provided_input_mask,
                *clip_plane_count,
                *sampler_kinds,
            ),
            Specialization::Pixel(variant) => {
                dxso::emit_ps_programmable_named(&parse()?, *variant, entry)
            }
            // `ff.rs` emits from the key alone, as the live fixed-function path does.
            Specialization::FfVertex { key, .. } => return Ok(dxso::emit_vs_ff_named(key, entry)),
            Specialization::FfPixel { key, variant } => {
                return Ok(dxso::emit_ps_ff_named(key, *variant, entry));
            }
        };
        result.map_err(|error| format!("MSL emission: {error:?}"))
    }

    pub(super) fn encode(&self, out: &mut Vec<u8>) {
        match &self.specialization {
            Specialization::Vertex {
                provided_input_mask,
                clip_plane_count,
                sampler_kinds,
            } => {
                out.extend_from_slice(&provided_input_mask.to_le_bytes());
                out.extend_from_slice(&[
                    *clip_plane_count,
                    sampler_kinds.volume_mask,
                    sampler_kinds.cube_mask,
                    u8::from(sampler_kinds.lod_table),
                ]);
            }
            Specialization::Pixel(variant) => encode_variant(*variant, out),
            Specialization::FfVertex {
                key,
                provided_input_mask,
            } => {
                encode_ff_vs(key, out);
                out.extend_from_slice(&provided_input_mask.to_le_bytes());
            }
            Specialization::FfPixel { key, variant } => {
                encode_ff_ps(key, out);
                encode_variant(*variant, out);
            }
        }
        out.extend_from_slice(
            &u32::try_from(self.tokens.len())
                .expect("DXSO token count fits u32")
                .to_le_bytes(),
        );
        for token in self.tokens.iter() {
            out.extend_from_slice(&token.to_le_bytes());
        }
    }

    /// Read a source of MSL kind `kind` (0 to 7); `None` unless it is of that stage and model.
    pub(super) fn decode(kind: CachedKind, reader: &mut RecipeReader<'_>) -> Option<Self> {
        let specialization = if !kind.is_programmable() {
            if kind.is_vertex() {
                Specialization::FfVertex {
                    key: decode_ff_vs(reader)?,
                    provided_input_mask: reader.u16()?,
                }
            } else {
                Specialization::FfPixel {
                    key: decode_ff_ps(reader)?,
                    variant: decode_variant(reader)?,
                }
            }
        } else if kind.is_vertex() {
            let provided_input_mask = reader.u16()?;
            let clip_plane_count = reader.u8()?;
            let sampler_kinds = VsSamplerKinds {
                volume_mask: reader.u8()?,
                cube_mask: reader.u8()?,
                lod_table: match reader.u8()? {
                    0 => false,
                    1 => true,
                    _ => return None,
                },
            };
            if usize::from(clip_plane_count) > MAX_CLIP_PLANES
                || (sampler_kinds.volume_mask | sampler_kinds.cube_mask) & !0x0F != 0
                || sampler_kinds.volume_mask & sampler_kinds.cube_mask != 0
            {
                return None;
            }
            Specialization::Vertex {
                provided_input_mask,
                clip_plane_count,
                sampler_kinds,
            }
        } else {
            Specialization::Pixel(decode_variant(reader)?)
        };
        let count = usize::try_from(reader.u32()?).ok()?;
        let bytes = reader.take(count.checked_mul(4)?)?;
        let tokens: Arc<[u32]> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|word| u32::from_le_bytes(*word))
            .collect();
        let header = *tokens.first()?;
        let pixel = match header >> 16 {
            0xFFFE => false,
            0xFFFF => true,
            _ => return None,
        };
        let major = u8::try_from((header >> 8) & 0xFF).ok()?;
        // `ff_build` writes `vs_3_0`/`ps_3_0` only.
        let stage = if kind.is_programmable() {
            CachedKind::from_programmable(major, pixel)?
        } else if header & 0xFFFF == 0x0300 {
            if pixel {
                CachedKind::FfPs
            } else {
                CachedKind::FfVs
            }
        } else {
            return None;
        };
        // Match the opcode-only END check used by shader creation and DXSO parsing.
        if stage != kind || tokens.last()? & 0xFFFF != 0xFFFF {
            return None;
        }
        Some(Self {
            tokens,
            specialization,
        })
    }
}

// Clone is required by CacheEntry's round-trip and concurrent-writer test fixtures.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Specialization {
    Vertex {
        provided_input_mask: u16,
        clip_plane_count: u8,
        sampler_kinds: VsSamplerKinds,
    },
    Pixel(VariantKey),
    /// The tokens are `ff_build::vertex(key)`'s; the mask is the inputs they declare.
    FfVertex {
        key: FfVsKey,
        provided_input_mask: u16,
    },
    /// The tokens are `ff_build::pixel(key)`'s.
    FfPixel {
        key: FfPsKey,
        variant: VariantKey,
    },
}

/// Every `FfVsKey` byte in field order, `reserved` included.
fn encode_ff_vs(key: &FfVsKey, out: &mut Vec<u8>) {
    let FfVsKey {
        flags,
        input_tex_coord_count,
        tex_coord_count,
        light_active_mask,
        light_directional_mask,
        light_spot_mask,
        diffuse_source,
        ambient_source,
        specular_source,
        emissive_source,
        fog_mode,
        tci,
        passthrough,
        tex_coord_dims,
        tt_flags,
        vertex_blend_count,
        declared_weights_count,
        clip_plane_count,
        reserved,
    } = key;
    out.extend_from_slice(&flags.bits().to_le_bytes());
    out.extend_from_slice(&[
        *input_tex_coord_count,
        *tex_coord_count,
        *light_active_mask,
        *light_directional_mask,
        *light_spot_mask,
        *diffuse_source,
        *ambient_source,
        *specular_source,
        *emissive_source,
        *fog_mode,
    ]);
    for array in [tci, passthrough, tex_coord_dims, tt_flags] {
        out.extend_from_slice(array);
    }
    out.extend_from_slice(&[
        *vertex_blend_count,
        *declared_weights_count,
        *clip_plane_count,
        *reserved,
    ]);
}

/// An `FfVsKey` the builder can take.
///
/// Refused only where `ff.rs`/`ff_build` would index past an array or read a value no live key
/// carries (C3c-4).
///
/// Values they merely warn about and fall back from (a material source above 2, a TCI mode
/// above 4, a fog mode they do not name) load: live keys carry them.
fn decode_ff_vs(reader: &mut RecipeReader<'_>) -> Option<FfVsKey> {
    let key = FfVsKey {
        flags: FfVsFlags::from_bits(reader.u16()?)?,
        input_tex_coord_count: reader.u8()?,
        tex_coord_count: reader.u8()?,
        light_active_mask: reader.u8()?,
        light_directional_mask: reader.u8()?,
        light_spot_mask: reader.u8()?,
        diffuse_source: reader.u8()?,
        ambient_source: reader.u8()?,
        specular_source: reader.u8()?,
        emissive_source: reader.u8()?,
        fog_mode: reader.u8()?,
        tci: reader.array()?,
        passthrough: reader.array()?,
        tex_coord_dims: reader.array()?,
        tt_flags: reader.array()?,
        vertex_blend_count: reader.u8()?,
        declared_weights_count: reader.u8()?,
        clip_plane_count: reader.u8()?,
        reserved: reader.u8()?,
    };
    let sets = key.tex_coord_dims.len();
    let valid = usize::from(key.input_tex_coord_count) <= sets
        && usize::from(key.tex_coord_count) <= key.tci.len()
        && key.tci.iter().all(|entry| usize::from(entry & 0x0F) < sets)
        && key.tex_coord_dims.iter().all(|dim| *dim <= 4)
        && key
            .passthrough
            .iter()
            .take_while(|code| **code != 0)
            .all(|code| Semantic::from_code(*code).code() == *code)
        && key.vertex_blend_count <= MAX_BLEND_MATRICES
        && usize::from(key.clip_plane_count) <= MAX_CLIP_PLANES
        && key.reserved == 0;
    valid.then_some(key)
}

/// Every `FfPsKey` byte in field order.
fn encode_ff_ps(key: &FfPsKey, out: &mut Vec<u8>) {
    let FfPsKey {
        stages,
        specular_add,
        tt_projected_mask,
    } = key;
    for stage in stages {
        let FfStage {
            color_op,
            color_arg1,
            color_arg2,
            alpha_op,
            alpha_arg1,
            alpha_arg2,
            flags,
        } = stage;
        out.extend_from_slice(&[
            *color_op,
            *color_arg1,
            *color_arg2,
            *alpha_op,
            *alpha_arg1,
            *alpha_arg2,
            flags.bits(),
        ]);
    }
    out.extend_from_slice(&[u8::from(*specular_add), *tt_projected_mask]);
}

/// An `FfPsKey`; unknown ops and arguments load (`ff.rs` warns), unknown flag bits do not.
fn decode_ff_ps(reader: &mut RecipeReader<'_>) -> Option<FfPsKey> {
    let mut stages = [FfStage::default(); 8];
    for stage in &mut stages {
        *stage = FfStage {
            color_op: reader.u8()?,
            color_arg1: reader.u8()?,
            color_arg2: reader.u8()?,
            alpha_op: reader.u8()?,
            alpha_arg1: reader.u8()?,
            alpha_arg2: reader.u8()?,
            flags: FfStageFlags::from_bits(reader.u8()?)?,
        };
    }
    Some(FfPsKey {
        stages,
        specular_add: match reader.u8()? {
            0 => false,
            1 => true,
            _ => return None,
        },
        tt_projected_mask: reader.u8()?,
    })
}

fn encode_variant(variant: VariantKey, out: &mut Vec<u8>) {
    let VariantKey {
        linked_input_mask,
        alpha_func,
        fog_mode,
        fog_table_mode,
        depth_sampler_mask,
        depth_fetch_mask,
        fetch4_mask,
        fetch4_alpha_mask,
        raw_depth_red_mask,
        volume_sampler_mask,
        cube_sampler_mask,
        tt_projected_mask,
        color_out_mask,
        sample_mask,
        flags,
    } = variant;
    out.extend_from_slice(&[alpha_func, fog_mode, fog_table_mode]);
    for mask in [
        depth_sampler_mask,
        depth_fetch_mask,
        fetch4_mask,
        fetch4_alpha_mask,
        raw_depth_red_mask,
        volume_sampler_mask,
        cube_sampler_mask,
    ] {
        out.extend_from_slice(&mask.to_le_bytes());
    }
    out.extend_from_slice(&[
        tt_projected_mask,
        color_out_mask,
        sample_mask,
        flags.bits(),
        linked_input_mask,
    ]);
}

fn decode_variant(reader: &mut RecipeReader<'_>) -> Option<VariantKey> {
    Some(VariantKey {
        alpha_func: reader.u8()?,
        fog_mode: reader.u8()?,
        fog_table_mode: reader.u8()?,
        depth_sampler_mask: reader.u16()?,
        depth_fetch_mask: reader.u16()?,
        fetch4_mask: reader.u16()?,
        fetch4_alpha_mask: reader.u16()?,
        raw_depth_red_mask: reader.u16()?,
        volume_sampler_mask: reader.u16()?,
        cube_sampler_mask: reader.u16()?,
        tt_projected_mask: reader.u8()?,
        color_out_mask: reader.u8()?,
        sample_mask: reader.u8()?,
        flags: VariantFlags::from_bits(reader.u8()?)?,
        linked_input_mask: reader.u8()?,
    })
}
