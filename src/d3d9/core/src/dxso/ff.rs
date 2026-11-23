//! Fixed-function shader keys, constants, and stage metadata helpers.
//!
//! `FfVsKey` / `FfStage` / `FfPsKey` fields are part of the crate's public
//! cache-key contract with `ff_state` and `rt::device::shader_cache`: callers
//! construct them field-by-field from D3D9 state and use them as hash-map
//! keys, so every field is deliberately `pub`.

use mtld3d_shared::mtl::VERTEX_ATTRIBUTE_SLOTS;
use mtld3d_types::{
    D3DDECLUSAGE_BLENDINDICES, D3DDECLUSAGE_BLENDWEIGHT, D3DDECLUSAGE_COLOR, D3DDECLUSAGE_NORMAL,
    D3DDECLUSAGE_POSITION, D3DDECLUSAGE_POSITIONT, D3DDECLUSAGE_PSIZE, D3DDECLUSAGE_TEXCOORD,
    D3DTA_CONSTANT, D3DTA_CURRENT, D3DTA_SELECTMASK, D3DTA_TEXTURE, D3DTA_TFACTOR,
    D3DTOP_BLENDFACTORALPHA, D3DTOP_BUMPENVMAP, D3DTOP_BUMPENVMAPLUMINANCE, D3DTOP_DISABLE,
    D3DTOP_DOTPRODUCT3, D3DTOP_SELECTARG1, D3DTOP_SELECTARG2,
};

use super::link::MAX_LINKED_INPUTS;

/// First FF VS constant row of the world-matrix palette.
///
/// Every section below it has a fixed row range (`FfVsDirty` in `ff_state`
/// names each one); the palette runs from here to the end of the constant
/// block, four rows per matrix, and the fixed-function VS reads it as
/// `vs_c + FF_VS_PALETTE_BASE_ROW + idx * 4`.
pub const FF_VS_PALETTE_BASE_ROW: u16 = 95;

/// Highest `D3DTS_WORLDMATRIX(i)` index a draw can read through vertex blending.
///
/// The encoder binds 256 FF VS constant rows (`CONSTANT_ROWS` in the `d3d9`
/// crate, which asserts that its mirror still holds this index), so rows
/// 95..=255 carry 40 whole matrices and 39 is the last index whose four rows
/// are inside the block. `caps::fill` reports it as
/// `D3DCAPS9::MaxVertexBlendMatrixIndex`, so a title that sizes its bone
/// palette from the cap never asks for a matrix the layout has no rows for.
pub const MAX_VERTEX_BLEND_MATRIX_INDEX: u32 = 39;

/// Vertex attribute of the first declaration element a pre-transformed draw passes through.
///
/// Entry `k` of `FfVsKey::passthrough` reads the attribute `k` slots above
/// this one, past every fixed-function attribute `ff_attr_index_for_semantic`
/// hands out.
pub const FF_PASSTHROUGH_ATTR_BASE: u16 = 15;

const _: () = assert!(
    FF_PASSTHROUGH_ATTR_BASE as usize + MAX_LINKED_INPUTS <= VERTEX_ATTRIBUTE_SLOTS as usize
);

bitflags::bitflags! {
    /// Boolean predicates for `FfVsKey`.
    ///
    /// Packs boolean fields into a `u16` so the cache key stays compact and
    /// `Hash` walks one word.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct FfVsFlags: u16 {
        /// Vertex declaration has a NORMAL element.
        const HAS_NORMAL = 1 << 0;
        /// Vertex declaration has a COLOR0 element (diffuse).
        const HAS_COLOR0 = 1 << 1;
        /// Vertex declaration has a COLOR1 element (specular).
        const HAS_COLOR1 = 1 << 2;
        /// `D3DRS_LIGHTING` enabled.
        ///
        /// XYZRHW bypasses per-vertex lighting regardless of the RS, so this
        /// is always false when `HAS_RHW`.
        const LIGHTING_ENABLED = 1 << 3;
        /// `D3DFVF_XYZRHW`: position is pre-transformed (screen-space with 1/w).
        const HAS_RHW = 1 << 4;
        /// `D3DRS_COLORVERTEX`, which gates the material-source override.
        ///
        /// When clear, the resolver ignores `*_source` and always reads
        /// from the material constant. Canonicalized at key build: only set
        /// on a lit draw, the one branch that reads it.
        const COLOR_VERTEX = 1 << 5;
        /// `D3DRS_SPECULARENABLE`.
        ///
        /// Gates per-light Blinn-Phong specular emission into `color1`; when
        /// clear, a lit `color1` is the vertex specular colour, or zero
        /// without one. Canonicalized at key build: only set on a lit draw,
        /// since the unlit `color1` is the vertex specular colour either way.
        const SPECULAR_ENABLE = 1 << 6;
        /// `D3DRS_INDEXEDVERTEXBLENDENABLE`: the world-matrix index source.
        ///
        /// When set, per-vertex BLENDINDICES select world matrices from
        /// `world_palette[idx[i]]`; when clear, sequential matrices
        /// `world_palette[0..count]` are used. Only set when the declaration
        /// carries BLENDINDICES (`DECLARED_INDICES`): without them the
        /// render state blends the sequential matrices.
        const VERTEX_BLEND_INDEXED = 1 << 7;
        /// Vertex declaration has a BLENDINDICES element.
        ///
        /// Mirrored into the key so adding the indices attribute triggers a
        /// fresh shader variant.
        const DECLARED_INDICES = 1 << 8;
        /// `D3DRS_LOCALVIEWER`: selects the specular view-vector model.
        ///
        /// Per-vertex `normalize(-posEye)` when set (the D3D9 default), the
        /// constant infinite-viewer `(0, 0, -1)` when clear. Canonicalized at
        /// key build: only set when lighting + specular are both on, so draws
        /// that never read V don't fork variants.
        const LOCAL_VIEWER = 1 << 9;
        /// `D3DRS_NORMALIZENORMALS` is enabled.
        ///
        /// The FF VS then renormalizes the eye-space normal that lighting and
        /// texgen share; when clear (the D3D9 default) the transformed normal
        /// keeps its magnitude, so a non-unit normal scales the lighting and
        /// the generated coordinates. Canonicalized at key build: only set
        /// when something reads the normal.
        const NORMALIZE_NORMALS = 1 << 12;
        /// Vertex declaration has a PSIZE element (`D3DFVF_PSIZE`).
        ///
        /// The per-vertex size replaces `D3DRS_POINTSIZE` as the point size
        /// before the scale and clamp.
        const HAS_PSIZE = 1 << 13;
        /// `D3DRS_POINTSCALEENABLE`: the point size is attenuated by eye distance.
        ///
        /// Only ever set on an untransformed layout; pre-transformed vertices
        /// have no eye-space distance to scale by.
        const POINT_SCALE = 1 << 14;
        /// Computed vertex fog uses distance from the eye instead of Z.
        ///
        /// Canonicalized off for disabled, table and supplied-factor fog.
        const RANGE_FOG = 1 << 15;
    }
}

/// Summary of the current D3D9 Fixed-Function vertex state.
#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct FfVsKey {
    /// Boolean predicates. See `FfVsFlags` for individual bit semantics.
    pub flags: FfVsFlags,
    /// Number of TEXCOORD input attributes the VS declares.
    ///
    /// Equal to the vertex-stream texcoord count derived from the FVF /
    /// `VertexDeclaration` (`FfVsLayout::tex_coord_count`). Must agree with
    /// the vertex descriptor built by `convert::resolve_attrs_for_ff`.
    pub input_tex_coord_count: u8,
    /// Number of per-stage texcoord varyings the VS emits for the PS to sample.
    ///
    /// Covers every stage whose TCI generates a coordinate or routes a set
    /// the stream carries, whatever its `COLOROP`, since a programmable PS
    /// samples the stages it names. Inflated above `input_tex_coord_count`
    /// by such a stage, and by an active textured stage with no matching
    /// vertex-stream texcoords, whose passthru read falls back to zero.
    pub tex_coord_count: u8,
    /// Bit `i` set iff slot `i`'s `D3DLIGHT9` contributes to FF VS shading.
    ///
    /// True iff its `Type` is POINT, SPOT or DIRECTIONAL AND it is enabled via
    /// `LightEnable(i, TRUE)`.
    pub light_active_mask: u8,
    /// Bit `i` set iff active light `i` is DIRECTIONAL.
    ///
    /// Only meaningful when the corresponding `light_active_mask` bit is set.
    pub light_directional_mask: u8,
    /// Bit `i` set iff active light `i` is SPOT.
    ///
    /// Only meaningful when the corresponding `light_active_mask` bit is set;
    /// a slot with neither type bit takes the POINT branch.
    pub light_spot_mask: u8,
    /// `D3DRS_DIFFUSEMATERIALSOURCE` (0 = `MCS_MATERIAL`, 1 = `MCS_COLOR1`, 2 = `MCS_COLOR2`).
    ///
    /// Canonicalized at key build to `MCS_MATERIAL` unless `COLOR_VERTEX` is
    /// set, the only case that reads it.
    pub diffuse_source: u8,
    /// `D3DRS_AMBIENTMATERIALSOURCE`.
    ///
    /// Canonicalized as `diffuse_source` is.
    pub ambient_source: u8,
    /// `D3DRS_SPECULARMATERIALSOURCE`.
    ///
    /// Canonicalized as `diffuse_source` is.
    pub specular_source: u8,
    /// `D3DRS_EMISSIVEMATERIALSOURCE`.
    ///
    /// Canonicalized as `diffuse_source` is.
    pub emissive_source: u8,
    /// Vertex fog mode, resolved from the fog render states and `HAS_RHW`.
    ///
    /// 0 = off, 1 = EXP, 2 = EXP2, 3 = LINEAR, 4 = factor taken straight from
    /// the specular (COLOR1) alpha, which is what D3D9 does when fog is enabled
    /// and both `D3DRS_FOGVERTEXMODE` and `D3DRS_FOGTABLEMODE` are
    /// `D3DFOG_NONE`. A non-`NONE` table mode fogs per-pixel and holds this at
    /// 0. XYZRHW vertices bypass the vertex-fog computation, so a `HAS_RHW`
    /// draw carries either 0 (fog off, or table fog) or 4.
    pub fog_mode: u8,
    /// Per-stage `D3DTSS_TEXCOORDINDEX`, narrowed to a byte by [`tci_entry`].
    ///
    /// The high nibble is the TCI mode (0 = PASSTHRU, 1 = CAMERASPACENORMAL,
    /// 2 = CAMERASPACEPOSITION, 3 = CAMERASPACEREFLECTIONVECTOR,
    /// 4 = SPHEREMAP; higher values are undefined and fall back to passthru
    /// with a one-shot warn), the low nibble the input coordinate set
    /// (0..7). Read them with [`FfVsKey::tci_mode`] and [`FfVsKey::tci_set`].
    /// Canonicalized at key build: zero for every stage at or past
    /// `tex_coord_count`, which the shader writes as a zero coordinate whatever
    /// its TCI.
    pub tci: [u8; 8],
    /// The declaration elements a pre-transformed draw passes to the pixel stage by semantic.
    ///
    /// The list `link::decl_passthrough_code` describes: a semantic code per
    /// entry, zero past the end, entry `k` read from attribute
    /// [`FF_PASSTHROUGH_ATTR_BASE`] `+ k`. Empty on an untransformed layout.
    pub passthrough: [u8; MAX_LINKED_INPUTS],
    /// Declared component count (1..=4) of each input TEXCOORD set.
    ///
    /// Indexed by coord-set (`usage_index`); `0` if the set is absent.
    /// Mirrors `FfVsLayout::tex_coord_dims`.
    pub tex_coord_dims: [u8; 8],
    /// Per-stage texture-transform flags packed: low 3 bits = count, bit 4 = `D3DTTFF_PROJECTED`.
    ///
    /// Canonicalized at key build: zero for every stage at or past
    /// `tex_coord_count`, as `tci` is.
    pub tt_flags: [u8; 8],
    /// Number of world matrices blended per vertex. `0` disables blending.
    ///
    /// Derived from `D3DRS_VERTEXBLEND`.
    pub vertex_blend_count: u8,
    /// Number of weight lanes the vertex decl declares.
    pub declared_weights_count: u8,
    /// User clip planes the draw applies (`vs_draw::clip_plane_count`), 0..=6.
    ///
    /// Always 0 on an RHW layout, which D3D9 never clips against user planes.
    pub clip_plane_count: u8,
    /// Initialized padding in the canonical capture record.
    pub reserved: u8,
}

// Preserve the existing cache identity: canonical padding is not shader state.
impl core::hash::Hash for FfVsKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(&self.flags, state);
        core::hash::Hash::hash(&self.input_tex_coord_count, state);
        core::hash::Hash::hash(&self.tex_coord_count, state);
        core::hash::Hash::hash(&self.light_active_mask, state);
        core::hash::Hash::hash(&self.light_directional_mask, state);
        core::hash::Hash::hash(&self.light_spot_mask, state);
        core::hash::Hash::hash(&self.diffuse_source, state);
        core::hash::Hash::hash(&self.ambient_source, state);
        core::hash::Hash::hash(&self.specular_source, state);
        core::hash::Hash::hash(&self.emissive_source, state);
        core::hash::Hash::hash(&self.fog_mode, state);
        core::hash::Hash::hash(&self.tci, state);
        core::hash::Hash::hash(&self.passthrough, state);
        core::hash::Hash::hash(&self.tex_coord_dims, state);
        core::hash::Hash::hash(&self.tt_flags, state);
        core::hash::Hash::hash(&self.vertex_blend_count, state);
        core::hash::Hash::hash(&self.declared_weights_count, state);
        core::hash::Hash::hash(&self.clip_plane_count, state);
    }
}

impl FfVsKey {
    #[inline]
    #[must_use]
    pub const fn has_normal(&self) -> bool {
        self.flags.contains(FfVsFlags::HAS_NORMAL)
    }
    #[inline]
    #[must_use]
    pub const fn has_color0(&self) -> bool {
        self.flags.contains(FfVsFlags::HAS_COLOR0)
    }
    #[inline]
    #[must_use]
    pub const fn has_color1(&self) -> bool {
        self.flags.contains(FfVsFlags::HAS_COLOR1)
    }
    #[inline]
    #[must_use]
    pub const fn lighting_enabled(&self) -> bool {
        self.flags.contains(FfVsFlags::LIGHTING_ENABLED)
    }
    #[inline]
    #[must_use]
    pub const fn has_rhw(&self) -> bool {
        self.flags.contains(FfVsFlags::HAS_RHW)
    }
    #[inline]
    #[must_use]
    pub const fn has_psize(&self) -> bool {
        self.flags.contains(FfVsFlags::HAS_PSIZE)
    }
    #[inline]
    #[must_use]
    pub const fn point_scale(&self) -> bool {
        self.flags.contains(FfVsFlags::POINT_SCALE)
    }
    #[inline]
    #[must_use]
    pub const fn color_vertex(&self) -> bool {
        self.flags.contains(FfVsFlags::COLOR_VERTEX)
    }
    #[inline]
    #[must_use]
    pub const fn specular_enable(&self) -> bool {
        self.flags.contains(FfVsFlags::SPECULAR_ENABLE)
    }
    #[inline]
    #[must_use]
    pub const fn local_viewer(&self) -> bool {
        self.flags.contains(FfVsFlags::LOCAL_VIEWER)
    }
    #[inline]
    #[must_use]
    pub const fn normalize_normals(&self) -> bool {
        self.flags.contains(FfVsFlags::NORMALIZE_NORMALS)
    }
    #[inline]
    #[must_use]
    pub const fn vertex_blend_indexed(&self) -> bool {
        self.flags.contains(FfVsFlags::VERTEX_BLEND_INDEXED)
    }
    #[inline]
    #[must_use]
    pub const fn declared_indices(&self) -> bool {
        self.flags.contains(FfVsFlags::DECLARED_INDICES)
    }
    /// The TCI mode of `stage`, the high nibble of its `tci` entry.
    #[inline]
    #[must_use]
    pub const fn tci_mode(&self, stage: usize) -> u8 {
        self.tci[stage] >> 4
    }
    /// The input coordinate set of `stage` (0..7), the low nibble of its `tci` entry.
    #[inline]
    #[must_use]
    pub const fn tci_set(&self, stage: usize) -> u8 {
        self.tci[stage] & 0x0F
    }
}

/// One `FfVsKey::tci` entry from a TCI mode and an input coordinate set.
///
/// A mode above 15 keeps the nibble's highest value and a set above 7 reads
/// set 7, the same undefined-mode passthru and the same coordinate set the
/// fixed-function vertex stage takes for the raw values.
#[must_use]
pub const fn tci_entry(mode: u8, set: u8) -> u8 {
    let mode = if mode > 15 { 15 } else { mode };
    let set = if set > 7 { 7 } else { set };
    (mode << 4) | set
}

/// Extract the coordinate component count from packed texture-transform flags.
#[must_use]
pub const fn tt_count(flags: u8) -> u8 {
    flags & 0x07
}

/// Return whether packed texture-transform flags include `D3DTTFF_PROJECTED`.
#[must_use]
pub const fn tt_projected(flags: u8) -> bool {
    (flags & 0x10) != 0
}

/// Whether `op` is `D3DTOP_BUMPENVMAP` or `D3DTOP_BUMPENVMAPLUMINANCE`.
///
/// A colour operation of either kind makes its stage a bump-environment stage:
/// the stage writes no register and its texture displaces, and for the
/// luminance kind also scales, the next stage's sample.
#[must_use]
pub const fn is_bump_env_op(op: u32) -> bool {
    matches!(op, D3DTOP_BUMPENVMAP | D3DTOP_BUMPENVMAPLUMINANCE)
}

bitflags::bitflags! {
    /// Texture presence and result destination share one byte in the stage key.
    // Copy keeps the existing seven-byte FfStage value semantics.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct FfStageFlags: u8 {
        const HAS_TEXTURE = 1 << 0;
        const RESULT_TEMP = 1 << 1;
    }
}

/// Register receiving one whole texture-stage result.
// Copy lets the stage setter accept this scalar selector by value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FfStageResult {
    Current,
    Temp,
}

/// Per-stage texture combiner state derived from `texture_stage_states[stage]`.
///
/// `D3DTSS_TEXCOORDINDEX` controls both the VS (TCI mode + input coord-set
/// selection) and the PS (which varying to sample from). Both concerns are
/// handled on the VS side (`FfVsKey::tci`, one entry per stage) so the VS
/// writes the coordinate for each stage into `texcoord[stage]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct FfStage {
    pub color_op: u8,
    pub color_arg1: u8,
    pub color_arg2: u8,
    pub alpha_op: u8,
    pub alpha_arg1: u8,
    pub alpha_arg2: u8,
    pub flags: FfStageFlags,
}

impl FfStage {
    #[must_use]
    pub const fn has_texture(&self) -> bool {
        self.flags.contains(FfStageFlags::HAS_TEXTURE)
    }

    #[must_use]
    pub const fn result(&self) -> FfStageResult {
        if self.flags.contains(FfStageFlags::RESULT_TEMP) {
            FfStageResult::Temp
        } else {
            FfStageResult::Current
        }
    }

    /// Whether the effective color or alpha operation consumes this argument source.
    ///
    /// Explicit unbound-texture fallback reads CURRENT, effective DOTPRODUCT3
    /// supplies alpha itself, and a disabled alpha operation keeps the result
    /// register's alpha without reading an argument. Unsupported operations
    /// conservatively retain their inputs so their arg1 fallback cannot read an
    /// unbound constant buffer.
    #[must_use]
    pub fn reads_argument(&self, selector: u32) -> bool {
        let color = if self.color_uses_unbound_fallback() {
            selector == D3DTA_CURRENT
        } else {
            op_reads_argument(self.color_op, self.color_arg1, self.color_arg2, selector)
        };
        color
            || (!self.color_writes_alpha()
                && if self.alpha_uses_unbound_fallback() {
                    selector == D3DTA_CURRENT
                } else {
                    op_reads_argument(self.alpha_op, self.alpha_arg1, self.alpha_arg2, selector)
                })
    }

    pub fn set_result(&mut self, result: FfStageResult) {
        self.flags
            .set(FfStageFlags::RESULT_TEMP, result == FfStageResult::Temp);
    }

    fn color_uses_unbound_fallback(self) -> bool {
        !self.has_texture() && op_reads_texture(self.color_op, self.color_arg1, self.color_arg2)
    }

    fn alpha_uses_unbound_fallback(self) -> bool {
        !self.has_texture() && op_reads_texture(self.alpha_op, self.alpha_arg1, self.alpha_arg2)
    }

    fn color_writes_alpha(self) -> bool {
        u32::from(self.color_op) == D3DTOP_DOTPRODUCT3 && !self.color_uses_unbound_fallback()
    }

    fn references_texture_factor(self) -> bool {
        let selects_factor = |a: u8| u32::from(a) & D3DTA_SELECTMASK == D3DTA_TFACTOR;
        u32::from(self.color_op) == D3DTOP_BLENDFACTORALPHA
            || u32::from(self.alpha_op) == D3DTOP_BLENDFACTORALPHA
            || selects_factor(self.color_arg1)
            || selects_factor(self.color_arg2)
            || (u32::from(self.alpha_op) != D3DTOP_DISABLE
                && (selects_factor(self.alpha_arg1) || selects_factor(self.alpha_arg2)))
    }
}

/// Summary of the current D3D9 Fixed-Function pixel state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct FfPsKey {
    pub stages: [FfStage; 8],
    /// `D3DRS_SPECULARENABLE`.
    ///
    /// When set, the interpolated specular color (`color1`) is added to the
    /// cascade result after the last texture stage and before fog. FF-only: a
    /// bound programmable PS never applies it.
    pub specular_add: bool,
    /// Bit `i` set iff stage `i` has `D3DTTFF_PROJECTED`.
    ///
    /// The FF sampler divides the texture coordinate by its `.w` component
    /// before sampling (the projective divide). `.w == 0` samples at the
    /// origin (D3D9 returns the `(0,0)` texel there).
    pub tt_projected_mask: u8,
}

impl FfPsKey {
    /// Stages the fragment function declares a texture and sampler for.
    ///
    /// Bit `i` is set for every stage ahead of the first `D3DTOP_DISABLE` that
    /// carries a bound texture. A stage outside the mask is one the shader
    /// never samples, so binding the game's texture there is skipped.
    #[must_use]
    pub fn sampled_stage_mask(&self) -> u16 {
        let mut mask = 0u16;
        for (i, stage) in self.stages.iter().enumerate() {
            if u32::from(stage.color_op) == D3DTOP_DISABLE {
                break;
            }
            if stage.has_texture() {
                mask |= 1u16 << i;
            }
        }
        mask
    }

    /// Whether any active stage reads the texture-factor row.
    ///
    /// `D3DTA_TFACTOR` on an argument and the `D3DTOP_BLENDFACTORALPHA` op are
    /// the readers of row zero, which holds `D3DRS_TEXTUREFACTOR`. Stage
    /// constants occupy later rows independently. Over-approximates on the
    /// argument an op discards (`D3DTOP_SELECTARG1` ignoring arg2), which only
    /// costs a bind that would otherwise be skipped.
    #[must_use]
    pub fn reads_texture_factor(&self) -> bool {
        self.stages
            .iter()
            .take_while(|s| u32::from(s.color_op) != D3DTOP_DISABLE)
            .any(|s| s.references_texture_factor())
    }

    /// Number of fragment constant rows read by the active combiner cascade.
    ///
    /// Row zero retains texture factor; row `stage + 1` holds that stage's
    /// constant. Zero skips the binding, one keeps the ordinary 16-byte path.
    #[must_use]
    pub fn constant_rows(&self) -> u8 {
        let mut rows = 0;
        for (extent, stage) in (2u8..).zip(&self.stages) {
            if u32::from(stage.color_op) == D3DTOP_DISABLE {
                break;
            }
            if stage.reads_argument(D3DTA_CONSTANT) {
                rows = extent;
            } else if rows == 0 && stage.references_texture_factor() {
                rows = 1;
            }
        }
        rows
    }
}

/// Fixed-function attribute convention for vertex declarations.
///
/// Returns the `[[attribute(N)]]` index that a vertex element with the given
/// `(usage, usage_index)` lands on in the FF VS. `None` means the FF VS does
/// not consume this semantic, and callers skip the element unless a
/// pre-transformed layout passes it through (`FF_PASSTHROUGH_ATTR_BASE`).
///
/// `usage` / `usage_index` use the `D3DDECLUSAGE_*` byte values (0=POSITION,
/// 3=NORMAL, 5=TEXCOORD, 9=POSITIONT, 10=COLOR, ...).
#[must_use]
pub const fn ff_attr_index_for_semantic(usage: u8, usage_index: u8) -> Option<u16> {
    match (usage, usage_index) {
        (D3DDECLUSAGE_POSITION | D3DDECLUSAGE_POSITIONT, 0) => Some(0),
        (D3DDECLUSAGE_NORMAL, 0) => Some(1),
        (D3DDECLUSAGE_COLOR, 0) => Some(2),
        (D3DDECLUSAGE_COLOR, 1) => Some(3),
        (D3DDECLUSAGE_TEXCOORD, i) if i < 8 => Some(4 + i as u16),
        (D3DDECLUSAGE_BLENDWEIGHT, 0) => Some(12),
        (D3DDECLUSAGE_BLENDINDICES, 0) => Some(13),
        (D3DDECLUSAGE_PSIZE, 0) => Some(14),
        _ => None,
    }
}

/// Does `op` read a `D3DTA_TEXTURE` argument?
///
/// An op that selects only the other slot (SELECTARG1 ignores arg2, SELECTARG2
/// ignores arg1) does not consume the unread one. Used to detect a texture
/// stage referencing an unbound texture so it falls back to CURRENT.
fn op_reads_texture(op: u8, arg1: u8, arg2: u8) -> bool {
    op_reads_argument(op, arg1, arg2, D3DTA_TEXTURE)
}

/// Does `op` consume an argument naming `selector`?
///
/// `D3DTOP_DISABLE` consumes none: a disabled colour operation ends the
/// cascade, and a disabled alpha operation keeps the register's alpha.
fn op_reads_argument(op: u8, arg1: u8, arg2: u8, selector: u32) -> bool {
    let op = u32::from(op);
    let selected = |a: u8| u32::from(a) & D3DTA_SELECTMASK == selector;
    op != D3DTOP_DISABLE
        && ((selected(arg1) && op != D3DTOP_SELECTARG2)
            || (selected(arg2) && op != D3DTOP_SELECTARG1))
}

#[cfg(test)]
mod tests;
