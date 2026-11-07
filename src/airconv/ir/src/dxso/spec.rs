//! The specialization a D3D9 stage is compiled under, and its wire form.
//!
//! `VariantKey` (pixel) and `VsSpec` (vertex) are what the MSL emitters and the AIR translator
//! read besides the program. `StageSpec` carries either across the VM boundary as fixed-size
//! little-endian bytes (`to_wire` / `from_wire`), the guest facade building them without LLVM.

use super::ir::TextureType;

/// The AIR compile argument type of a D3D9 shader (`SM50_SHADER_D3D9` in `airconv_public.h`).
pub const SM50_SHADER_D3D9: u32 = 10;

/// Bytes of `VsSpec` on the wire: its `#[repr(C)]` size.
pub const VS_SPEC_WIRE: usize = 6;
/// Bytes of `VariantKey` on the wire: its `#[repr(C)]` size.
pub const VARIANT_KEY_WIRE: usize = 22;

bitflags::bitflags! {
    /// Per-variant boolean features folded into the PS shader cache key.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct VariantFlags: u8 {
        /// Table-fog source select, only meaningful when `fog_table_mode != 0`.
        ///
        /// Clear otherwise, to avoid variant churn: clear = pixel depth
        /// (`in.position.z` + `D3DRS_DEPTHBIAS`), set = eye W
        /// (`1 / in.position.w`). D3D9 picks by the projection matrix's 4th
        /// column — (0,0,0,1) means W==1 everywhere (orthographic) so fog reads
        /// Z; anything else reads W (the D3D9 pixel-fog source rule).
        const FOG_SOURCE_W = 1 << 0;
        /// `D3DRS_SHADEMODE == D3DSHADE_FLAT`.
        ///
        /// When set, the PS `Varyings` struct declares `color0`/`color1` with
        /// the `[[flat]]` interpolation qualifier so the diffuse/specular
        /// colours come from the provoking (first) vertex instead of being
        /// perspective-interpolated. Folded into the PS cache key so FLAT and
        /// GOURAUD draws get distinct libraries.
        const FLAT_SHADE = 1 << 1;
        /// `D3DRS_SRGBWRITEENABLE != 0` on a colour target with no sRGB Metal view.
        ///
        /// When set, the PS applies the linear→sRGB OETF to the final colour
        /// rgb (alpha untouched) just before output, after
        /// fog/specular/alpha-test. This is the fallback path: where the
        /// bound colour target has an sRGB twin the render pass attaches it
        /// instead and the hardware encodes after the blender, which is what
        /// D3D9 does; the encoder clears this flag for such a draw. It stays
        /// set for a target whose format has no sRGB twin (e.g. R5G6B5),
        /// where the encode can only happen in-shader and therefore lands
        /// before the blender. Folded into the PS cache key so sRGB-write and
        /// plain draws get distinct libraries.
        const SRGB_WRITE = 1 << 2;
        /// `D3DRS_POINTSPRITEENABLE != 0` on a point-list draw.
        ///
        /// When set, the PS takes `[[point_coord]]` and substitutes it for
        /// every texture-coordinate varying before the body runs, which is
        /// what D3D9 does for point sprites: each point is textured as a whole
        /// quad regardless of the vertex's own coordinates. The API thread
        /// sets it from the render state and `emit_draw` clears it for any
        /// primitive other than points, so triangle draws keep their
        /// libraries. Folded into the PS cache key.
        const POINT_SPRITE = 1 << 3;
        /// The render pass has no depth attachment.
        ///
        /// A PS writing `oDepth` computes the value but does not export it:
        /// D3D9 discards the write, while Metal rejects a pipeline whose
        /// fragment function declares a depth output against no depth
        /// attachment. Folded into the PS cache key.
        const NO_DEPTH_ATTACHMENT = 1 << 4;
        /// A sampler slot this draw binds carries a LOD bias or an explicit-LOD clamp.
        ///
        /// Metal samplers have no LOD bias, and ignore their LOD clamps for a
        /// sample at an explicit level, so both are applied at the sample
        /// site: the PS takes the per-slot LOD table on `PS_LOD_BIAS_SLOT`,
        /// every implicit-LOD sample passes `bias(...)`, and every sample at
        /// an explicit level clamps it by the slot's row
        /// (`sampler_state::explicit_lod_row`). A draw with neither compiles
        /// the unchanged shader and binds nothing, so the common case costs
        /// nothing. Folded into the PS cache key.
        const LOD_BIAS = 1 << 5;
        /// The bound colour target is rasterized below the resolution D3D9 reports.
        ///
        /// Set only for a `ps_3_0` shader that declares `vPos`: the function
        /// then takes the `PsDraw` uniform and reads the register as the
        /// rasterized position scaled into the reported space, so a shader
        /// comparing `vPos` against the back-buffer size it was told, or
        /// dividing by it for a screen-space UV, sees the same values it
        /// would at the identity. A shader without `vPos` never carries the
        /// bit, so its MSL and its key are unchanged.
        const VPOS_SCALE = 1 << 7;
        /// `D3DRS_MULTISAMPLEMASK` selects fewer than all samples.
        ///
        /// When set, the fragment function returns a struct carrying a
        /// `[[sample_mask]]` member and writes `VariantKey::sample_mask` to
        /// it, which is the only per-draw sample coverage control Metal
        /// offers: there is no equivalent of Vulkan's `pSampleMask` on the
        /// pipeline descriptor. Set only for a maskable multisampled render
        /// target whose mask is not already all-ones, so single-sampled draws
        /// keep their existing libraries. Folded into the PS cache key.
        const SAMPLE_MASK = 1 << 6;
    }
}

/// Shader specialization key carried through PS emission.
///
/// Covers the features that aren't part of the per-stage pixel combiner state.
///
/// `alpha_func` is the D3DCMP_* value for the alpha test; `fog_mode` mirrors
/// `FfVsKey::fog_mode` and gates the PS-side fog blend (0 = no blend
/// emitted).
// Copy is required by the encoder's pass-specific key adjustment and cache-key probes.
// The key contains only narrow masks and flags; no resource or heap storage is copied.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct VariantKey {
    pub alpha_func: u8,
    pub fog_mode: u8,
    /// Per-pixel ("table") fog — `D3DRS_FOGTABLEMODE`.
    ///
    /// Set when fog is enabled and the table mode isn't NONE: 1 = EXP,
    /// 2 = EXP2, 3 = LINEAR (`D3DFOG_*`). Non-zero makes the PS compute the fog
    /// factor per-pixel from the rasterizer position (the interpolated
    /// `in.fog.x` vertex factor is ignored); `fog_mode` is 0 in that case. Fog
    /// params arrive in `fog_data[1]` = (start, end, density, depth-bias) on
    /// buffer 13.
    pub fog_table_mode: u8,
    /// Bit `i` set ⇒ the bound vertex shader outputs the PS's `i`-th extra input semantic.
    ///
    /// The extra inputs are the `ps_3_0` input semantics outside the members
    /// every `Varyings` struct declares (NORMAL, TANGENT, COLOR2, …), in the
    /// order [`super::link::LinkInputs`] lists them. The PS declares a stage-in
    /// member for each set bit and reads zero for each clear one, since Metal
    /// rejects a fragment input the vertex function does not write. The
    /// encoder derives it from the draw's two shaders; the API sends zero.
    /// Part of the PS cache key.
    pub linked_input_mask: u8,
    /// Bit `i` set ⇒ sampler slot `i` is bound to a depth-format texture.
    ///
    /// The PS emitter outputs `depth2d<float>` for that slot and wraps
    /// `s{i}.sample(...)` in `float4(...)` (the depth sampler returns a
    /// single-channel scalar; downstream code reads `.x/.y/.z/.w` and would
    /// otherwise miscompile). Every depth-format texture is 2D, so this wins
    /// over the volume and cube masks. Folded into the PS shader cache key so
    /// different bind shapes get distinct compiled libraries.
    pub depth_sampler_mask: u16,
    /// Bit `i` set ⇒ sampler slot `i` is a "readable raw depth" FOURCC texture.
    ///
    /// INTZ/DF24/DF16 are a subset of `depth_sampler_mask`. The slot still
    /// binds as `depth2d<float>`, but reads the stored depth through `.sample()`
    /// and a non-comparison sampler. `raw_depth_red_mask` selects the result's
    /// channel fill. Part of the PS cache key.
    pub depth_fetch_mask: u16,
    /// Slots using a native four-texel gather instead of a filtered sample.
    pub fetch4_mask: u16,
    /// Gather slots reading alpha rather than red (the A8 format).
    pub fetch4_alpha_mask: u16,
    /// DF16/DF24 raw samples return red with zero green/blue and alpha one.
    pub raw_depth_red_mask: u16,
    /// Bit `i` set ⇒ sampler slot `i` is bound to a volume (3D) texture.
    ///
    /// A `CreateVolumeTexture` resource with `depth > 1`, which the unix
    /// side creates as `MTLTextureType3D`. Both pixel-shader emitters declare
    /// `texture3d<float>` and sample with the texcoord's `.xyz` for such
    /// slots; declaring `texture2d<float>` would fail Metal's binding
    /// type-check and sample black. Part of the PS cache key.
    pub volume_sampler_mask: u16,
    /// Bit `i` set when sampling slot `i` is bound to a cube texture.
    ///
    /// Both pixel-shader emitters declare `texturecube<float>` and sample with
    /// the texcoord's `.xyz` for such slots. Part of the PS cache key.
    pub cube_sampler_mask: u16,
    /// Bit `i` set ⇒ texture stage `i` has `D3DTTFF_PROJECTED`.
    ///
    /// The fixed-function and `ps_1_0`..`ps_1_3` pixel pipelines apply an
    /// IMPLICIT per-pixel projective divide (coord ÷ its `.w`) before sampling
    /// that stage; `ps_1_4` projects via DZ/DW modifiers and `ps_2_0`+ ignore
    /// TTFF, so the SM1 emitter consumes this only for `shader_minor < 4`.
    /// Folded in at draw time (the FF PS uses its own `FfPsKey` mask). Part of
    /// the PS cache key.
    pub tt_projected_mask: u8,
    /// Bit `i` set ⇒ colour attachment `i` is present in the render pass.
    ///
    /// The programmable PS exports `[[color(i)]]` only for attachments that
    /// are both present here and written by the shader (see
    /// `DxsoProgram::color_out_mask`); a write to any other `oCi` sinks into
    /// a plain local. Metal rejects a pipeline whose fragment function writes
    /// a colour slot the pass leaves unbound, so the mask must match the
    /// pass exactly. Bit 0 is implied: the emitter ORs it in, so the derived
    /// `Default` (0) still means "render target 0 only" and the FF PS, which
    /// writes one output, keys on the default. Part of the PS cache key.
    pub color_out_mask: u8,
    /// `D3DRS_MULTISAMPLEMASK` narrowed to the pass's sample count.
    ///
    /// Bit `i` selects sample `i`. Only read when
    /// [`VariantFlags::SAMPLE_MASK`] is set, and held at 0 otherwise so a
    /// render state a single-sampled draw ignores cannot fragment the cache.
    /// `u8` because Metal caps a texture at 8 samples.
    pub sample_mask: u8,
    /// Packed boolean features — pixel-fog source, flat shade, sRGB write.
    ///
    /// See [`VariantFlags`].
    pub flags: VariantFlags,
}

// Field by field, so the hash shape stays explicit and independent of the record layout.
impl core::hash::Hash for VariantKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::hash::Hash::hash(&self.alpha_func, state);
        core::hash::Hash::hash(&self.fog_mode, state);
        core::hash::Hash::hash(&self.fog_table_mode, state);
        core::hash::Hash::hash(&self.depth_sampler_mask, state);
        core::hash::Hash::hash(&self.depth_fetch_mask, state);
        core::hash::Hash::hash(&self.fetch4_mask, state);
        core::hash::Hash::hash(&self.fetch4_alpha_mask, state);
        core::hash::Hash::hash(&self.raw_depth_red_mask, state);
        core::hash::Hash::hash(&self.volume_sampler_mask, state);
        core::hash::Hash::hash(&self.cube_sampler_mask, state);
        core::hash::Hash::hash(&self.tt_projected_mask, state);
        core::hash::Hash::hash(&self.color_out_mask, state);
        core::hash::Hash::hash(&self.sample_mask, state);
        core::hash::Hash::hash(&self.flags, state);
        core::hash::Hash::hash(&self.linked_input_mask, state);
    }
}

/// Texture kind bound to each vertex-fetch sampler slot.
///
/// The four `D3DVERTEXTEXTURESAMPLER0..3` slots (API stages 257..260) bind
/// whatever `SetTexture` last put there, which need not be the kind the
/// `vs_3_0` declared with `dcl_2d` / `dcl_cube` / `dcl_volume`. The vertex
/// emitter types the `[[texture(n)]]` argument and picks the sample coordinate
/// swizzle from these masks instead of from the declaration, so the Metal
/// binding always matches the texture the draw actually binds. A slot with
/// neither bit set is 2D, which is also what an unbound slot gets: the black
/// fallback the draw path binds there is 2D as well.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct VsSamplerKinds {
    /// Bit `i` set ⇒ vertex slot `i` binds a volume (3D) texture.
    pub volume_mask: u8,
    /// Bit `i` set ⇒ vertex slot `i` binds a cube texture.
    pub cube_mask: u8,
    /// A slot the shader samples carries a texture LOD, a LOD bias or a finest level.
    ///
    /// Metal ignores a sampler's LOD clamps for a sample at an explicit level
    /// and has no sampler bias, so every `texldl` then reads its slot's row of
    /// the vertex LOD table on `VS_LOD_SLOT`
    /// (`sampler_state::VertexLodTable`) and samples
    /// `level(max(lod + offset, floor))`. The encoder derives it from the
    /// draw's vertex sampler states and the shader's `texldl` slots; the API
    /// sends false. A draw without it compiles the unchanged shader and binds
    /// nothing. Part of the VS library and disk keys.
    pub lod_table: bool,
}

impl VsSamplerKinds {
    /// Number of vertex texture fetch slots D3D9 defines.
    pub const SLOT_COUNT: u16 = 4;

    /// The texture kind bound at vertex slot `idx`.
    ///
    /// Record the kind bound at slot `idx`.
    ///
    /// `volume` and `cube` come straight off the bound texture, so at most one
    /// is true; both false is a 2D texture or an empty slot. Slots at or past
    /// [`Self::SLOT_COUNT`] are ignored (D3D9 defines exactly four).
    pub const fn set_slot(&mut self, idx: usize, volume: bool, cube: bool) {
        if idx >= Self::SLOT_COUNT as usize {
            return;
        }
        let bit = 1u8 << idx;
        self.volume_mask = if volume {
            self.volume_mask | bit
        } else {
            self.volume_mask & !bit
        };
        self.cube_mask = if cube {
            self.cube_mask | bit
        } else {
            self.cube_mask & !bit
        };
    }

    /// The texture kind bound at vertex slot `idx`.
    ///
    /// A texture is volume or cube or neither, so the two masks never share a
    /// bit; the order below only settles a mask a caller built wrong. Slots at
    /// or past [`Self::SLOT_COUNT`] read as 2D.
    #[must_use]
    pub const fn kind(self, idx: u16) -> TextureType {
        if idx >= Self::SLOT_COUNT {
            return TextureType::Texture2D;
        }
        let bit = 1u8 << idx;
        if (self.volume_mask & bit) != 0 {
            TextureType::Texture3D
        } else if (self.cube_mask & bit) != 0 {
            TextureType::TextureCube
        } else {
            TextureType::Texture2D
        }
    }
}

/// What a vertex stage is specialized on beyond its program.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct VsSpec {
    /// Bit `i` set ⇒ the vertex declaration provides input register `vi`; the rest read zero.
    pub provided_input_mask: u16,
    /// User clip planes the stage writes `[[clip_distance]]` for.
    pub clip_plane_count: u8,
    /// The texture kind bound at each vertex-fetch slot.
    pub sampler_kinds: VsSamplerKinds,
}

/// One stage's specialization: the argument of an AIR compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageSpec {
    Vs(VsSpec),
    Ps(VariantKey),
}

/// A wire specialization the translator has no case for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecError {
    /// A stage other than 0 (vertex) or 1 (pixel).
    Stage(u32),
    /// A `lod_table` byte other than 0 or 1.
    LodTable(u8),
    /// An `alpha_func` above `D3DCMP_ALWAYS`.
    AlphaFunc(u8),
    /// A `fog_mode` above 4 (`D3DFOG_*` and the specular-alpha factor).
    FogMode(u8),
    /// A `fog_table_mode` above `D3DFOG_LINEAR`.
    FogTableMode(u8),
}

const VS_PROVIDED: usize = core::mem::offset_of!(VsSpec, provided_input_mask);
const VS_CLIP: usize = core::mem::offset_of!(VsSpec, clip_plane_count);
const VS_KINDS: usize = core::mem::offset_of!(VsSpec, sampler_kinds);
const KINDS_VOLUME: usize = VS_KINDS + core::mem::offset_of!(VsSamplerKinds, volume_mask);
const KINDS_CUBE: usize = VS_KINDS + core::mem::offset_of!(VsSamplerKinds, cube_mask);
const KINDS_LOD: usize = VS_KINDS + core::mem::offset_of!(VsSamplerKinds, lod_table);

// The pixel key's fields, each at its `#[repr(C)]` offset.
const PS_ALPHA_FUNC: usize = core::mem::offset_of!(VariantKey, alpha_func);
const PS_FOG_MODE: usize = core::mem::offset_of!(VariantKey, fog_mode);
const PS_FOG_TABLE: usize = core::mem::offset_of!(VariantKey, fog_table_mode);
const PS_LINKED: usize = core::mem::offset_of!(VariantKey, linked_input_mask);
const PS_DEPTH: usize = core::mem::offset_of!(VariantKey, depth_sampler_mask);
const PS_DEPTH_FETCH: usize = core::mem::offset_of!(VariantKey, depth_fetch_mask);
const PS_FETCH4: usize = core::mem::offset_of!(VariantKey, fetch4_mask);
const PS_FETCH4_ALPHA: usize = core::mem::offset_of!(VariantKey, fetch4_alpha_mask);
const PS_RAW_DEPTH: usize = core::mem::offset_of!(VariantKey, raw_depth_red_mask);
const PS_VOLUME: usize = core::mem::offset_of!(VariantKey, volume_sampler_mask);
const PS_CUBE: usize = core::mem::offset_of!(VariantKey, cube_sampler_mask);
const PS_TT: usize = core::mem::offset_of!(VariantKey, tt_projected_mask);
const PS_COLOR_OUT: usize = core::mem::offset_of!(VariantKey, color_out_mask);
const PS_SAMPLE_MASK: usize = core::mem::offset_of!(VariantKey, sample_mask);
const PS_FLAGS: usize = core::mem::offset_of!(VariantKey, flags);

// The wire lengths are the structs' sizes, so a field added to either breaks the build here.
const _: () = assert!(size_of::<VsSpec>() == VS_SPEC_WIRE);
const _: () = assert!(size_of::<VariantKey>() == VARIANT_KEY_WIRE);

/// Write `bytes` into `out` at `at`; every offset below is a field of the same-sized struct.
fn put(out: &mut [u8], at: usize, bytes: &[u8]) {
    for (slot, byte) in out.iter_mut().skip(at).zip(bytes) {
        *slot = *byte;
    }
}

fn byte(wire: &[u8], at: usize) -> u8 {
    wire.get(at).copied().unwrap_or(0)
}

fn word(wire: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([byte(wire, at), byte(wire, at + 1)])
}

impl StageSpec {
    /// The wire form: `(stage, vertex bytes, pixel bytes)`, the half of the other stage zero.
    ///
    /// Each field sits at its `#[repr(C)]` offset in little-endian, padding zero.
    #[must_use]
    pub fn to_wire(&self) -> (u32, [u8; VS_SPEC_WIRE], [u8; VARIANT_KEY_WIRE]) {
        let mut vs = [0; VS_SPEC_WIRE];
        let mut ps = [0; VARIANT_KEY_WIRE];
        let stage = match self {
            Self::Vs(spec) => {
                put(
                    &mut vs,
                    VS_PROVIDED,
                    &spec.provided_input_mask.to_le_bytes(),
                );
                put(&mut vs, VS_CLIP, &[spec.clip_plane_count]);
                put(&mut vs, KINDS_VOLUME, &[spec.sampler_kinds.volume_mask]);
                put(&mut vs, KINDS_CUBE, &[spec.sampler_kinds.cube_mask]);
                put(
                    &mut vs,
                    KINDS_LOD,
                    &[u8::from(spec.sampler_kinds.lod_table)],
                );
                0
            }
            Self::Ps(key) => {
                put(&mut ps, PS_ALPHA_FUNC, &[key.alpha_func]);
                put(&mut ps, PS_FOG_MODE, &[key.fog_mode]);
                put(&mut ps, PS_FOG_TABLE, &[key.fog_table_mode]);
                put(&mut ps, PS_LINKED, &[key.linked_input_mask]);
                put(&mut ps, PS_DEPTH, &key.depth_sampler_mask.to_le_bytes());
                put(&mut ps, PS_DEPTH_FETCH, &key.depth_fetch_mask.to_le_bytes());
                put(&mut ps, PS_FETCH4, &key.fetch4_mask.to_le_bytes());
                put(
                    &mut ps,
                    PS_FETCH4_ALPHA,
                    &key.fetch4_alpha_mask.to_le_bytes(),
                );
                put(&mut ps, PS_RAW_DEPTH, &key.raw_depth_red_mask.to_le_bytes());
                put(&mut ps, PS_VOLUME, &key.volume_sampler_mask.to_le_bytes());
                put(&mut ps, PS_CUBE, &key.cube_sampler_mask.to_le_bytes());
                put(&mut ps, PS_TT, &[key.tt_projected_mask]);
                put(&mut ps, PS_COLOR_OUT, &[key.color_out_mask]);
                put(&mut ps, PS_SAMPLE_MASK, &[key.sample_mask]);
                put(&mut ps, PS_FLAGS, &[key.flags.bits()]);
                1
            }
        };
        (stage, vs, ps)
    }

    /// Read a wire form, refusing what the emitters have no case for.
    ///
    /// The bytes come from a guest: a stage above 1, a `lod_table` byte above 1 (not a valid
    /// `bool`), an `alpha_func`, fog mode or table-fog mode outside the values the emitters
    /// handle is an error, never a value the translator meets. Every `VariantFlags` bit has a
    /// meaning, so no flag byte is refused.
    ///
    /// # Errors
    ///
    /// [`SpecError`] naming the refused field and its value.
    pub fn from_wire(
        stage: u32,
        vs: &[u8; VS_SPEC_WIRE],
        ps: &[u8; VARIANT_KEY_WIRE],
    ) -> Result<Self, SpecError> {
        match stage {
            0 => {
                let lod_table = match byte(vs, KINDS_LOD) {
                    0 => false,
                    1 => true,
                    other => return Err(SpecError::LodTable(other)),
                };
                Ok(Self::Vs(VsSpec {
                    provided_input_mask: word(vs, VS_PROVIDED),
                    clip_plane_count: byte(vs, VS_CLIP),
                    sampler_kinds: VsSamplerKinds {
                        volume_mask: byte(vs, KINDS_VOLUME),
                        cube_mask: byte(vs, KINDS_CUBE),
                        lod_table,
                    },
                }))
            }
            1 => {
                let alpha_func = byte(ps, PS_ALPHA_FUNC);
                if alpha_func > 8 {
                    return Err(SpecError::AlphaFunc(alpha_func));
                }
                let fog_mode = byte(ps, PS_FOG_MODE);
                if fog_mode > 4 {
                    return Err(SpecError::FogMode(fog_mode));
                }
                let fog_table_mode = byte(ps, PS_FOG_TABLE);
                if fog_table_mode > 3 {
                    return Err(SpecError::FogTableMode(fog_table_mode));
                }
                Ok(Self::Ps(VariantKey {
                    alpha_func,
                    fog_mode,
                    fog_table_mode,
                    linked_input_mask: byte(ps, PS_LINKED),
                    depth_sampler_mask: word(ps, PS_DEPTH),
                    depth_fetch_mask: word(ps, PS_DEPTH_FETCH),
                    fetch4_mask: word(ps, PS_FETCH4),
                    fetch4_alpha_mask: word(ps, PS_FETCH4_ALPHA),
                    raw_depth_red_mask: word(ps, PS_RAW_DEPTH),
                    volume_sampler_mask: word(ps, PS_VOLUME),
                    cube_sampler_mask: word(ps, PS_CUBE),
                    tt_projected_mask: byte(ps, PS_TT),
                    color_out_mask: byte(ps, PS_COLOR_OUT),
                    sample_mask: byte(ps, PS_SAMPLE_MASK),
                    flags: VariantFlags::from_bits_retain(byte(ps, PS_FLAGS)),
                }))
            }
            other => Err(SpecError::Stage(other)),
        }
    }
}

#[cfg(test)]
mod tests;
