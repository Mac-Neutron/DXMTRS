//! DXSO shader sampler and LOD metadata helpers.

use std::collections::BTreeMap;

use super::{
    VariantKey,
    ir::{Declaration, DxsoProgram, TextureType},
    opcode::Opcode,
};

/// Declared VS sampler slots and their texture dimensionality.
///
/// Vertex texture fetch: only `vs_3_0` can declare samplers (four slots,
/// `D3DVERTEXTEXTURESAMPLER0..3` on the API side, `s0..s3` in bytecode),
/// and only `texldl` may read them. The runtime binds the matching
/// vertex-stage texture and sampler for each entry.
#[must_use]
pub fn declared_vs_samplers(vs: &DxsoProgram) -> BTreeMap<u16, TextureType> {
    let mut samplers: BTreeMap<u16, TextureType> = BTreeMap::new();
    for decl in &vs.declarations {
        if let Declaration::Sampler { texture_type, reg } = decl {
            samplers.insert(reg.index, *texture_type);
        }
    }
    samplers
}

/// Declared PS sampler slots and their texture dimensionality.
///
/// SM2/SM3 read the explicit `dcl_<dim> sN`; SM1 has no sampler dcl, so a 2D
/// sampler is synthesized for every stage a sampling op targets
/// (dimensionality is not encoded in SM1 bytecode, and 2D matches every
/// conformance case).
#[must_use]
pub fn declared_ps_samplers(ps: &DxsoProgram) -> BTreeMap<u16, TextureType> {
    let mut samplers: BTreeMap<u16, TextureType> = BTreeMap::new();
    for decl in &ps.declarations {
        if let Declaration::Sampler { texture_type, reg } = decl {
            samplers.insert(reg.index, *texture_type);
        }
    }
    if ps.major == 1 {
        for inst in &ps.instructions {
            if sm1_op_samples(inst.opcode)
                && let Some(d) = inst.dst.as_ref()
            {
                samplers
                    .entry(d.reg.index)
                    .or_insert(TextureType::Texture2D);
            }
        }
    }
    samplers
}

/// Sampler slots a `texldl` names, bit `i` for `s<i>`.
///
/// Metal ignores a sampler's LOD clamps for a sample at an explicit level, so
/// these are the slots whose stage state the shader applies itself: through
/// the pixel `lod_bias` rows `.z` and `.w`, or for a `vs_3_0` the vertex
/// `vs_lod` rows (`VsSamplerKinds::lod_table`). Subroutine bodies count too.
#[must_use]
pub fn explicit_lod_samplers(program: &DxsoProgram) -> u16 {
    program
        .instructions
        .iter()
        .chain(program.subroutines.values().flatten())
        .filter(|inst| inst.opcode == Opcode::TexLdL)
        .filter_map(|inst| inst.srcs.get(1))
        .filter(|sampler| sampler.reg.index < 16)
        .fold(0, |mask, sampler| mask | (1u16 << sampler.reg.index))
}

/// The texture kind sampler slot `idx` reads, taken from the live bindings.
///
/// D3D9 samples the texture the application bound rather than the kind the
/// shader's `dcl_<dim>` names: a `dcl_2d` slot carrying a volume texture reads
/// the volume, and a `dcl_volume` slot carrying a 2D texture reads the 2D
/// texture. The declaration therefore decides only which slots exist. A slot
/// with nothing bound leaves every mask bit clear and reads the 1x1 black
/// fallback texture, which is 2D for the same reason.
#[must_use]
pub const fn bound_sampler_type(variant: VariantKey, idx: u16) -> TextureType {
    if idx >= 16 {
        return TextureType::Texture2D;
    }
    let bit = 1u16 << idx;
    if (variant.volume_sampler_mask & bit) != 0 {
        TextureType::Texture3D
    } else if (variant.cube_sampler_mask & bit) != 0 {
        TextureType::TextureCube
    } else {
        TextureType::Texture2D
    }
}

/// True for SM1 texture ops that issue a `sample` call.
///
/// So the destination register's stage needs an implicit sampler binding. SM1
/// bytecode has no `dcl_<dim> sN`; the sampler bound to a stage is implicit
/// (stage N -> sampler N). Excludes the pad / dot-only / depth-only ops.
const fn sm1_op_samples(op: Opcode) -> bool {
    matches!(
        op,
        Opcode::TexLd
            | Opcode::TexBem
            | Opcode::TexBemL
            | Opcode::TexM3x2Tex
            | Opcode::TexM3x3Tex
            | Opcode::TexM3x3Spec
            | Opcode::TexM3x3VSpec
            | Opcode::TexReg2Ar
            | Opcode::TexReg2Gb
            | Opcode::TexReg2Rgb
            | Opcode::TexDp3Tex
    )
}

#[cfg(test)]
mod tests;
