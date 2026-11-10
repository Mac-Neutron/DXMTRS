//! The fixed-function key lists both builders test, and the vertex builder's tests.
//!
//! [`synthetic_vs_keys`] and [`synthetic_ps_keys`] restate `emit_corpus`'s `synthetic_ff()`
//! (an example, which unit tests cannot call); [`extra_vs_keys`] and [`extra_ps_keys`] are the
//! further shapes the corpus adds for the AIR lane. `emit_corpus` restates these in turn.

use std::collections::BTreeSet;

use airconv_ir::{
    dxso::{
        DeclUsage, Declaration, DxsoProgram, RegKind,
        ir::{Instruction, SrcModifier, SrcOperand},
        link::SemanticSet,
        lower::{self, VS_CLIP_BASIS_OUTPUT, VS_WINDOW, VS_WINDOW_POS_FIXUP},
        opcode::Opcode,
        parse,
        spec::{VariantFlags, VariantKey, VsSpec},
    },
    dxso_write::write,
};
use mtld3d_types::{
    D3DCMP_GREATEREQUAL, D3DDECLUSAGE_BINORMAL, D3DDECLUSAGE_BLENDWEIGHT, D3DDECLUSAGE_COLOR,
    D3DDECLUSAGE_DEPTH, D3DDECLUSAGE_FOG, D3DDECLUSAGE_NORMAL, D3DDECLUSAGE_POSITION,
    D3DDECLUSAGE_TANGENT, D3DFOG_LINEAR, D3DMCS_COLOR1, D3DMCS_COLOR2, D3DMCS_MATERIAL,
    D3DTA_ALPHAREPLICATE, D3DTA_COMPLEMENT, D3DTA_CONSTANT, D3DTA_CURRENT, D3DTA_DIFFUSE,
    D3DTA_SPECULAR, D3DTA_TEMP, D3DTA_TEXTURE, D3DTA_TFACTOR, D3DTOP_ADD, D3DTOP_BLENDCURRENTALPHA,
    D3DTOP_BLENDTEXTUREALPHA, D3DTOP_BUMPENVMAP, D3DTOP_BUMPENVMAPLUMINANCE, D3DTOP_DISABLE,
    D3DTOP_LERP, D3DTOP_MODULATE, D3DTOP_MODULATE2X, D3DTOP_MODULATEALPHA_ADDCOLOR,
    D3DTOP_SELECTARG1, D3DTSS_ALPHAARG1, D3DTSS_ALPHAARG2, D3DTSS_ALPHAOP, D3DTSS_COLORARG1,
    D3DTSS_COLORARG2, D3DTSS_COLOROP, texture_stage_state_defaults,
};

use super::{FfProgram, vertex, vs};
use crate::dxso::{
    FfPsKey, FfStage, FfStageFlags, FfStageResult, FfVsFlags, FfVsKey, decl_passthrough_code,
    emit_ps_ff_named, emit_vs_ff_named, tci_entry,
};

/// TCI modes, the high nibble of an `FfVsKey::tci` entry.
const TCI_PASSTHRU: u8 = 0;
const TCI_NORMAL: u8 = 1;
const TCI_POSITION: u8 = 2;
const TCI_REFLECTION: u8 = 3;
const TCI_SPHEREMAP: u8 = 4;
/// A mode past SPHEREMAP, undefined.
const TCI_UNDEFINED: u8 = 5;
/// `FfVsKey::tt_flags`: the count in the low three bits, `D3DTTFF_PROJECTED` at bit 4.
const TT_PROJECTED: u8 = 0x10;

/// The palette's first row and the last bone index the builder clamps to.
const PALETTE: u16 = 95;
const LAST_BONE: f32 = 39.0;

fn narrow(value: u32) -> u8 {
    u8::try_from(value).expect("D3D9 fixed-function enum value fits u8")
}

/// A key with `flags`, `sets` two-component coordinate sets, `stages` stages each reading its own.
fn key(flags: FfVsFlags, sets: u8, stages: u8) -> FfVsKey {
    let mut k = FfVsKey {
        flags,
        input_tex_coord_count: sets,
        tex_coord_count: stages,
        light_active_mask: 0,
        light_directional_mask: 0,
        light_spot_mask: 0,
        diffuse_source: 0,
        ambient_source: 0,
        specular_source: 0,
        emissive_source: 0,
        fog_mode: 0,
        tci: core::array::from_fn(|s| tci_entry(TCI_PASSTHRU, u8::try_from(s).unwrap_or(0))),
        passthrough: [0; 8],
        tex_coord_dims: [0; 8],
        tt_flags: [0; 8],
        vertex_blend_count: 0,
        declared_weights_count: 0,
        clip_plane_count: 0,
        reserved: 0,
    };
    k.tex_coord_dims[..usize::from(sets)].fill(2);
    k
}

const fn lights(mut k: FfVsKey, active: u8, directional: u8, spot: u8) -> FfVsKey {
    k.light_active_mask = active;
    k.light_directional_mask = directional;
    k.light_spot_mask = spot;
    k
}

/// `synthetic_ff()`'s vertex keys, in its order.
pub fn synthetic_vs_keys() -> Vec<FfVsKey> {
    // (texture sets, normal, rhw), lighting (`None` off, `Some(n)` n lights), fog, specular,
    // texturing: 0 plain, 1 env map, 2 scrolled, 3 shared UV.
    let mut shapes = Vec::new();
    for tex in [1, 2] {
        for lighting in [None, Some(1), Some(2), Some(4)] {
            for fog in [false, true] {
                let mut variants = vec![(1, false), (0, false)];
                if lighting.is_some() {
                    variants.push((0, true));
                }
                for (texturing, specular) in variants {
                    shapes.push(((tex, true, false), lighting, fog, specular, texturing));
                }
            }
        }
        for fog in [false, true] {
            shapes.push(((tex, false, false), None, fog, false, 0));
        }
    }
    for fog in [false, true] {
        shapes.push(((1, true, false), Some(0), fog, false, 0));
        shapes.push(((1, false, false), None, fog, false, 2));
        shapes.push(((2, true, false), Some(1), fog, false, 3));
    }
    shapes.push(((1, true, false), Some(1), true, false, 2));
    shapes.push(((2, false, false), None, false, false, 3));
    for tex in [0, 1] {
        shapes.push(((tex, false, true), None, false, false, 0));
    }
    shapes
        .into_iter()
        .map(|((tex, normal, rhw), lighting, fog, specular, texturing)| {
            let lit = normal && lighting.is_some();
            let lights_on = if lit { lighting.unwrap_or(0) } else { 0 };
            let specular = lights_on > 0 && specular;
            let mut flags = FfVsFlags::COLOR_VERTEX;
            flags.set(FfVsFlags::HAS_NORMAL, normal);
            flags.set(FfVsFlags::HAS_COLOR0, !normal);
            flags.set(FfVsFlags::HAS_RHW, rhw);
            flags.set(FfVsFlags::LIGHTING_ENABLED, lit);
            flags.set(FfVsFlags::SPECULAR_ENABLE, specular);
            flags.set(FfVsFlags::LOCAL_VIEWER, specular);
            let active = if lights_on == 0 {
                0
            } else {
                0xFFu8 >> (8 - lights_on)
            };
            let point = if lights_on > 1 {
                1u8 << (lights_on - 1)
            } else {
                0
            };
            let sets = if texturing == 3 { 1 } else { tex };
            let mut k = lights(key(flags, sets, tex), active, active & !point, 0);
            k.diffuse_source = narrow(D3DMCS_COLOR1);
            k.ambient_source = narrow(D3DMCS_MATERIAL);
            k.specular_source = narrow(D3DMCS_COLOR2);
            k.emissive_source = narrow(D3DMCS_MATERIAL);
            k.fog_mode = if fog && !rhw {
                narrow(D3DFOG_LINEAR)
            } else {
                0
            };
            match texturing {
                1 => {
                    let stage = usize::from(tex);
                    k.tex_coord_count = tex + 1;
                    k.tci[stage] = tci_entry(TCI_REFLECTION, tex);
                    k.tt_flags[stage] = 3;
                }
                2 => k.tt_flags[0] = 2,
                3 => k.tci[1] = tci_entry(0, 0),
                _ => {}
            }
            k
        })
        .collect()
}

/// The further vertex shapes of the AIR corpus, and each builder warning's key.
pub fn extra_vs_keys() -> Vec<FfVsKey> {
    let lit = FfVsFlags::HAS_NORMAL | FfVsFlags::LIGHTING_ENABLED | FfVsFlags::COLOR_VERTEX;
    let specular = lit | FfVsFlags::SPECULAR_ENABLE | FfVsFlags::LOCAL_VIEWER;
    let unlit = FfVsFlags::HAS_COLOR0;
    let rhw = FfVsFlags::HAS_RHW | FfVsFlags::HAS_COLOR0;
    let mut keys = Vec::new();

    // User clip planes, lit and unlit.
    for planes in [1, 3, 6] {
        let mut k = lights(key(lit, 1, 1), 0b1, 0b1, 0);
        k.clip_plane_count = planes;
        keys.push(k);
        let mut k = key(unlit, 1, 1);
        k.clip_plane_count = planes;
        keys.push(k);
    }

    // Blending: sequential and indexed, one to four matrices, with the normal through the bones.
    for count in 1..=4u8 {
        for indexed in [false, true] {
            for normal in [false, true] {
                let mut flags = if normal { lit } else { unlit };
                if indexed {
                    flags |= FfVsFlags::VERTEX_BLEND_INDEXED | FfVsFlags::DECLARED_INDICES;
                }
                let mut k = key(flags, 1, 1);
                if normal {
                    k = lights(k, 0b11, 0b01, 0);
                }
                k.vertex_blend_count = count;
                k.declared_weights_count = count - 1;
                keys.push(k);
            }
        }
    }
    // Indices declared, the render state sequential; a texgen-only normal through the bones.
    let mut k = key(unlit | FfVsFlags::DECLARED_INDICES, 1, 1);
    k.vertex_blend_count = 3;
    k.declared_weights_count = 2;
    keys.push(k);
    let mut k = key(FfVsFlags::HAS_NORMAL | FfVsFlags::NORMALIZE_NORMALS, 1, 2);
    k.tci[1] = tci_entry(TCI_REFLECTION, 0);
    k.vertex_blend_count = 2;
    k.declared_weights_count = 1;
    keys.push(k);

    // Pre-transformed: passthrough extras (FOG0 under fog mode 4 and not), PSIZE, every dim.
    let pass = |usage: u8, index: u8| decl_passthrough_code(usage, index).unwrap_or(0);
    let extras = [
        pass(D3DDECLUSAGE_NORMAL, 0),
        pass(D3DDECLUSAGE_TANGENT, 0),
        pass(D3DDECLUSAGE_BINORMAL, 0),
        pass(D3DDECLUSAGE_COLOR, 2),
        pass(D3DDECLUSAGE_FOG, 0),
        pass(D3DDECLUSAGE_POSITION, 1),
        pass(D3DDECLUSAGE_BLENDWEIGHT, 0),
        pass(D3DDECLUSAGE_DEPTH, 0),
    ];
    for fog_mode in [0, 4] {
        let mut k = key(rhw | FfVsFlags::HAS_COLOR1 | FfVsFlags::HAS_PSIZE, 4, 8);
        k.tex_coord_dims[..4].copy_from_slice(&[1, 2, 3, 4]);
        k.passthrough = extras;
        k.fog_mode = fog_mode;
        keys.push(k);
    }
    let mut k = key(rhw, 1, 1);
    k.passthrough[..2].copy_from_slice(&extras[..2]);
    keys.push(k);
    // Fog mode 4 without a specular colour, pre-transformed and not.
    let mut k = key(rhw, 1, 1);
    k.fog_mode = 4;
    keys.push(k);
    let mut k = key(unlit, 1, 1);
    k.fog_mode = 4;
    keys.push(k);

    // Point size: per vertex or the render state, scaled or not; pre-transformed with PSIZE.
    for flags in [
        unlit | FfVsFlags::POINT_SCALE,
        unlit | FfVsFlags::POINT_SCALE | FfVsFlags::HAS_PSIZE,
        unlit | FfVsFlags::HAS_PSIZE,
        rhw | FfVsFlags::HAS_PSIZE,
    ] {
        keys.push(key(flags, 1, 1));
    }

    // Spot lights beside directional and point ones; a spot light with no normal.
    keys.push(lights(key(specular, 1, 1), 0b111, 0b001, 0b100));
    keys.push(lights(key(lit, 1, 1), 0b11, 0, 0b11));
    keys.push(lights(
        key(
            FfVsFlags::LIGHTING_ENABLED | unlit | FfVsFlags::HAS_COLOR1,
            1,
            1,
        ),
        0b11,
        0,
        0b10,
    ));
    // Specular with an infinite viewer, lit with a specular colour but no specular.
    keys.push(lights(
        key(lit | FfVsFlags::SPECULAR_ENABLE, 1, 1),
        0b1,
        0b1,
        0,
    ));
    keys.push(lights(key(lit | FfVsFlags::HAS_COLOR1, 1, 1), 0b1, 0b1, 0));

    // Every TCI mode, lit and unlit, with and without a normal, and through a texture matrix.
    for mode in [
        TCI_PASSTHRU,
        TCI_NORMAL,
        TCI_POSITION,
        TCI_REFLECTION,
        TCI_SPHEREMAP,
        TCI_UNDEFINED,
    ] {
        for flags in [lit, FfVsFlags::HAS_NORMAL, unlit] {
            for count in [0, 3] {
                let mut k = key(flags, 1, 2);
                if flags.contains(FfVsFlags::LIGHTING_ENABLED) {
                    k = lights(k, 0b1, 0b1, 0);
                }
                k.tci[1] = tci_entry(mode, 0);
                k.tt_flags[1] = count;
                keys.push(k);
            }
        }
    }

    // Texture transforms: counts 1-7, projected or not, over every input dimension.
    for count in 1..=7u8 {
        for projected in [0, TT_PROJECTED] {
            let mut k = key(unlit, 4, 4);
            k.tex_coord_dims[..4].copy_from_slice(&[1, 2, 3, 4]);
            k.tt_flags[..4].fill(count | projected);
            keys.push(k);
        }
    }
    let mut k = key(unlit, 4, 4);
    k.tex_coord_dims[..4].copy_from_slice(&[1, 2, 3, 4]);
    k.tt_flags[..4].fill(TT_PROJECTED);
    keys.push(k);

    // Vertex fog: every mode, by depth and by range.
    for fog_mode in 1..=4 {
        for range in [false, true] {
            let mut flags = lit | FfVsFlags::HAS_COLOR1;
            flags.set(FfVsFlags::RANGE_FOG, range && fog_mode < 4);
            let mut k = lights(key(flags, 1, 1), 0b1, 0b1, 0);
            k.fog_mode = fog_mode;
            keys.push(k);
        }
    }

    // Material sources from the vertex colours, with and without them.
    for (diffuse, colors) in [
        (D3DMCS_COLOR1, FfVsFlags::HAS_COLOR0),
        (D3DMCS_COLOR2, FfVsFlags::HAS_COLOR1),
        (D3DMCS_COLOR1, FfVsFlags::empty()),
        (D3DMCS_COLOR2, FfVsFlags::HAS_COLOR0 | FfVsFlags::HAS_COLOR1),
    ] {
        let mut k = lights(key(specular | colors, 1, 1), 0b11, 0b01, 0);
        k.diffuse_source = narrow(diffuse);
        k.ambient_source = narrow(D3DMCS_COLOR2);
        k.specular_source = narrow(D3DMCS_COLOR1);
        k.emissive_source = narrow(D3DMCS_COLOR2);
        keys.push(k);
    }
    // Lit without a normal (ambient and emissive alone), with a light.
    keys.push(lights(
        key(FfVsFlags::LIGHTING_ENABLED | unlit, 1, 1),
        0b1,
        0b1,
        0,
    ));
    // Normalized normals.
    keys.push(lights(
        key(lit | FfVsFlags::NORMALIZE_NORMALS, 1, 1),
        0b1,
        0,
        0,
    ));

    // The warnings: an absent set, a material source past COLOR2, texgen on a pre-transformed
    // stage, an undefined mode (the last two in the TCI loop above too).
    let mut k = key(unlit, 1, 2);
    k.tci[1] = tci_entry(TCI_PASSTHRU, 3);
    keys.push(k);
    let mut k = lights(key(lit, 1, 1), 0b1, 0b1, 0);
    k.ambient_source = 3;
    keys.push(k);
    let mut k = key(rhw, 1, 2);
    k.tci[1] = tci_entry(TCI_POSITION, 0);
    keys.push(k);
    let mut k = key(rhw, 1, 2);
    k.tci[1] = tci_entry(TCI_PASSTHRU, 5);
    keys.push(k);

    keys.push(worst_case_vs_key());
    keys
}

/// The key holding the most at once.
///
/// Four indexed bones, eight spot lights with specular, eight texgen stages, point scale, six
/// clip planes and range fog.
fn worst_case_vs_key() -> FfVsKey {
    let flags = FfVsFlags::HAS_NORMAL
        | FfVsFlags::LIGHTING_ENABLED
        | FfVsFlags::COLOR_VERTEX
        | FfVsFlags::HAS_COLOR0
        | FfVsFlags::HAS_COLOR1
        | FfVsFlags::SPECULAR_ENABLE
        | FfVsFlags::LOCAL_VIEWER
        | FfVsFlags::NORMALIZE_NORMALS
        | FfVsFlags::VERTEX_BLEND_INDEXED
        | FfVsFlags::DECLARED_INDICES
        | FfVsFlags::POINT_SCALE
        | FfVsFlags::HAS_PSIZE
        | FfVsFlags::RANGE_FOG;
    let mut k = lights(key(flags, 8, 8), 0xFF, 0, 0xFF);
    let modes = [
        TCI_NORMAL,
        TCI_POSITION,
        TCI_REFLECTION,
        TCI_SPHEREMAP,
        TCI_PASSTHRU,
        TCI_SPHEREMAP,
        TCI_REFLECTION,
        TCI_NORMAL,
    ];
    for (stage, mode) in modes.into_iter().enumerate() {
        k.tci[stage] = tci_entry(mode, u8::try_from(stage).unwrap_or(0));
        k.tt_flags[stage] = 4 | TT_PROJECTED;
    }
    k.tex_coord_dims = [1, 2, 3, 4, 1, 2, 3, 4];
    k.diffuse_source = narrow(D3DMCS_COLOR1);
    k.specular_source = narrow(D3DMCS_COLOR2);
    k.vertex_blend_count = 4;
    k.declared_weights_count = 3;
    k.clip_plane_count = 6;
    k.fog_mode = 2;
    k
}

/// Every vertex key the builders test.
pub fn all_vs_keys() -> Vec<FfVsKey> {
    let mut keys = synthetic_vs_keys();
    keys.extend(extra_vs_keys());
    keys
}

/// One stage's colour op, args, alpha op, args.
type StageOps = [u32; 6];

/// A pixel key from its cascade; stages past it keep their D3D9 defaults.
///
/// A stage gets a texture when one of its arguments names `D3DTA_TEXTURE`, or the op does.
fn ps_key(cascade: &[StageOps], specular_add: bool) -> FfPsKey {
    let stages = core::array::from_fn(|index| {
        let defaults = texture_stage_state_defaults(u8::try_from(index).unwrap_or(0));
        let state = |ty: u32| defaults[usize::try_from(ty).unwrap_or(0)];
        let ops = cascade.get(index).copied().unwrap_or_else(|| {
            [
                state(D3DTSS_COLOROP),
                state(D3DTSS_COLORARG1),
                state(D3DTSS_COLORARG2),
                state(D3DTSS_ALPHAOP),
                state(D3DTSS_ALPHAARG1),
                state(D3DTSS_ALPHAARG2),
            ]
        });
        let [
            color_op,
            color_arg1,
            color_arg2,
            alpha_op,
            alpha_arg1,
            alpha_arg2,
        ] = ops;
        let textured = index < cascade.len()
            && [color_arg1, color_arg2, alpha_arg1, alpha_arg2]
                .iter()
                .any(|a| (a & 0xF) == D3DTA_TEXTURE);
        FfStage {
            color_op: narrow(color_op),
            color_arg1: narrow(color_arg1),
            color_arg2: narrow(color_arg2),
            alpha_op: narrow(alpha_op),
            alpha_arg1: narrow(alpha_arg1),
            alpha_arg2: narrow(alpha_arg2),
            flags: if textured {
                FfStageFlags::HAS_TEXTURE
            } else {
                FfStageFlags::empty()
            },
        }
    });
    FfPsKey {
        stages,
        specular_add,
        tt_projected_mask: 0,
    }
}

/// `synthetic_ff()`'s pixel keys and variants, in its order.
pub fn synthetic_ps_keys() -> Vec<(FfPsKey, VariantKey)> {
    let modulate = [
        D3DTOP_MODULATE,
        D3DTA_TEXTURE,
        D3DTA_DIFFUSE,
        D3DTOP_MODULATE,
        D3DTA_TEXTURE,
        D3DTA_DIFFUSE,
    ];
    let select_texture = [
        D3DTOP_SELECTARG1,
        D3DTA_TEXTURE,
        D3DTA_CURRENT,
        D3DTOP_SELECTARG1,
        D3DTA_TEXTURE,
        D3DTA_CURRENT,
    ];
    let modulate2x = [
        D3DTOP_MODULATE2X,
        D3DTA_TEXTURE,
        D3DTA_DIFFUSE,
        D3DTOP_SELECTARG1,
        D3DTA_DIFFUSE,
        D3DTA_CURRENT,
    ];
    let over_current = |op| {
        [
            op,
            D3DTA_TEXTURE,
            D3DTA_CURRENT,
            D3DTOP_SELECTARG1,
            D3DTA_CURRENT,
            D3DTA_CURRENT,
        ]
    };
    let diffuse = [
        D3DTOP_SELECTARG1,
        D3DTA_DIFFUSE,
        D3DTA_CURRENT,
        D3DTOP_SELECTARG1,
        D3DTA_DIFFUSE,
        D3DTA_CURRENT,
    ];
    let cascades: Vec<Vec<StageOps>> = vec![
        vec![modulate],
        vec![modulate, over_current(D3DTOP_MODULATE)],
        vec![select_texture],
        vec![modulate2x],
        vec![diffuse],
        vec![select_texture, over_current(D3DTOP_BLENDTEXTUREALPHA)],
        vec![modulate, over_current(D3DTOP_ADD)],
        vec![modulate2x, over_current(D3DTOP_MODULATE2X)],
        vec![modulate, over_current(D3DTOP_MODULATEALPHA_ADDCOLOR)],
        vec![modulate, over_current(D3DTOP_BLENDCURRENTALPHA)],
        vec![
            modulate2x,
            over_current(D3DTOP_BLENDTEXTUREALPHA),
            over_current(D3DTOP_MODULATE),
        ],
    ];
    let mut keys = Vec::new();
    for (index, cascade) in cascades.iter().enumerate() {
        for alpha_test in [false, true] {
            for fog in [false, true] {
                for specular_add in [false, true] {
                    if specular_add && index > 1 {
                        continue;
                    }
                    let variant = VariantKey {
                        alpha_func: if alpha_test {
                            narrow(D3DCMP_GREATEREQUAL)
                        } else {
                            0
                        },
                        fog_mode: if fog { narrow(D3DFOG_LINEAR) } else { 0 },
                        ..VariantKey::default()
                    };
                    keys.push((ps_key(cascade, specular_add), variant));
                }
            }
        }
    }
    keys
}

/// The further pixel shapes of the AIR corpus.
///
/// Every colour and alpha op over texture, diffuse and the modifiers; TEMP results; bump
/// environment stages with and without a texture, projected and chained; the specular add;
/// stages sampling depth, cube and volume slots, under a LOD bias, raw depth and projection.
pub fn extra_ps_keys() -> Vec<(FfPsKey, VariantKey)> {
    let plain = VariantKey::default();
    let mut keys = Vec::new();
    let tex_diffuse = |op: u32| {
        [
            op,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
            op,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
        ]
    };
    // Every op (DISABLE aside) as stage 0's colour and alpha op, then over CURRENT as stage 1.
    for op in D3DTOP_SELECTARG1..=D3DTOP_LERP {
        keys.push((ps_key(&[tex_diffuse(op)], false), plain));
        let second = [
            op,
            D3DTA_TEXTURE | D3DTA_COMPLEMENT,
            D3DTA_CURRENT | D3DTA_ALPHAREPLICATE,
            op,
            D3DTA_SPECULAR,
            D3DTA_TFACTOR,
        ];
        keys.push((
            ps_key(&[tex_diffuse(D3DTOP_MODULATE), second], false),
            plain,
        ));
    }
    // Arguments: constant, temp, specular, factor; a stage writing TEMP read by the next.
    let mut k = ps_key(
        &[
            [
                D3DTOP_MODULATE,
                D3DTA_TEXTURE,
                D3DTA_CONSTANT,
                D3DTOP_SELECTARG1,
                D3DTA_TFACTOR,
                D3DTA_CURRENT,
            ],
            [
                D3DTOP_ADD,
                D3DTA_TEMP,
                D3DTA_SPECULAR,
                D3DTOP_MODULATE,
                D3DTA_TEMP | D3DTA_COMPLEMENT,
                D3DTA_CURRENT,
            ],
        ],
        true,
    );
    k.stages[0].set_result(FfStageResult::Temp);
    keys.push((k, plain));
    // Bump environment stages, with and without a texture, luminance, chained, projected.
    for op in [D3DTOP_BUMPENVMAP, D3DTOP_BUMPENVMAPLUMINANCE] {
        let bump = [
            op,
            D3DTA_TEXTURE,
            D3DTA_CURRENT,
            D3DTOP_SELECTARG1,
            D3DTA_TEXTURE,
            D3DTA_CURRENT,
        ];
        let next = tex_diffuse(D3DTOP_MODULATE);
        keys.push((ps_key(&[bump, next], false), plain));
        let mut k = ps_key(&[bump, next], false);
        k.stages[0].flags = FfStageFlags::empty();
        keys.push((k, plain));
        keys.push((ps_key(&[bump, bump, next], false), plain));
        let mut k = ps_key(&[bump, next], false);
        k.tt_projected_mask = 0b11;
        let projected = VariantKey {
            tt_projected_mask: 0b11,
            ..plain
        };
        keys.push((k, projected));
    }
    // Sampling a depth, a cube and a volume slot; a LOD bias; raw depth; projected depth.
    let three = ps_key(
        &[
            tex_diffuse(D3DTOP_MODULATE),
            tex_diffuse(D3DTOP_MODULATE),
            tex_diffuse(D3DTOP_MODULATE),
        ],
        false,
    );
    let mut projected = three.clone();
    projected.tt_projected_mask = 0b001;
    for (k, variant) in [
        (
            three.clone(),
            VariantKey {
                depth_sampler_mask: 0b001,
                cube_sampler_mask: 0b010,
                volume_sampler_mask: 0b100,
                ..plain
            },
        ),
        (
            three.clone(),
            VariantKey {
                flags: VariantFlags::LOD_BIAS,
                ..plain
            },
        ),
        (
            three,
            VariantKey {
                depth_sampler_mask: 0b001,
                depth_fetch_mask: 0b001,
                ..plain
            },
        ),
        (
            projected,
            VariantKey {
                depth_sampler_mask: 0b001,
                tt_projected_mask: 0b001,
                ..plain
            },
        ),
    ] {
        keys.push((k, variant));
    }
    // A cascade cut short by DISABLE.
    let mut k = ps_key(&[tex_diffuse(D3DTOP_MODULATE)], true);
    k.stages[1].color_op = narrow(D3DTOP_DISABLE);
    keys.push((k, plain));
    keys
}

/// Every pixel key and variant the builders test.
pub fn all_ps_keys() -> Vec<(FfPsKey, VariantKey)> {
    let mut keys = synthetic_ps_keys();
    keys.extend(extra_ps_keys());
    keys
}

// ── The vertex builder ──

fn built(key: &FfVsKey) -> (FfProgram, DxsoProgram) {
    let program = vertex(key);
    let parsed = parse(&program.tokens)
        .unwrap_or_else(|e| panic!("{key:?} builds tokens that parse: {e:?}"));
    (program, parsed)
}

fn spec(key: &FfVsKey, program: &FfProgram) -> VsSpec {
    VsSpec {
        provided_input_mask: program.provided_input_mask,
        clip_plane_count: key.clip_plane_count,
        ..VsSpec::default()
    }
}

fn all_srcs(p: &DxsoProgram) -> impl Iterator<Item = &SrcOperand> {
    p.instructions.iter().flat_map(|i| &i.srcs)
}

/// The `c#` rows below 256 the program reads directly.
fn direct_rows(p: &DxsoProgram) -> BTreeSet<u16> {
    all_srcs(p)
        .filter(|s| s.reg.kind == RegKind::Const && s.rel_addr.is_none() && s.reg.index < 256)
        .map(|s| s.reg.index)
        .collect()
}

/// The window registers the program reads.
fn window_rows(p: &DxsoProgram) -> BTreeSet<u16> {
    all_srcs(p)
        .filter(|s| s.reg.kind == RegKind::Const && s.rel_addr.is_none())
        .map(|s| s.reg.index)
        .filter(|i| (VS_WINDOW..=VS_WINDOW_POS_FIXUP).contains(i))
        .collect()
}

fn relative_rows(p: &DxsoProgram) -> BTreeSet<u16> {
    all_srcs(p)
        .filter(|s| s.reg.kind == RegKind::Const && s.rel_addr.is_some())
        .map(|s| s.reg.index)
        .collect()
}

/// The `vs_c` rows the MSL reads: every `vs_c[<n>]`, and a sequential bone's four palette rows.
fn msl_rows(text: &str) -> BTreeSet<u16> {
    let mut rows = BTreeSet::new();
    for (prefix, bone) in [("vs_c[", false), ("uint idx = ", true)] {
        let mut rest = text;
        while let Some(at) = rest.find(prefix) {
            rest = &rest[at + prefix.len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            let tail = &rest[digits.len()..];
            let Ok(n) = digits.parse::<u16>() else {
                continue;
            };
            if bone && tail.starts_with("u;") {
                rows.extend((0..4).map(|j| PALETTE + 4 * n + j));
            } else if !bone && tail.starts_with(']') {
                rows.insert(n);
            }
        }
    }
    rows
}

/// Texture matrix rows the MSL reads into lanes its output drops.
///
/// `COUNT2`'s z and w, `COUNT3`'s w.
fn dead_rows(key: &FfVsKey) -> BTreeSet<u16> {
    let mut rows = BTreeSet::new();
    if key.has_rhw() {
        return rows;
    }
    for stage in 0..usize::from(key.tex_coord_count).min(8) {
        let base = 63 + 4 * u16::try_from(stage).unwrap_or(0);
        match key.tt_flags[stage] & 7 {
            2 => rows.extend([base + 2, base + 3]),
            3 => {
                rows.insert(base + 3);
            }
            _ => {}
        }
    }
    rows
}

/// The window rows the builder reads beyond the epilogue's own.
fn expected_window(key: &FfVsKey) -> BTreeSet<u16> {
    let mut rows = BTreeSet::new();
    if key.has_rhw() {
        rows.insert(VS_WINDOW_POS_FIXUP);
        return rows;
    }
    if key.point_scale() {
        rows.extend([VS_WINDOW + 1, VS_WINDOW_POS_FIXUP]);
        if !key.has_psize() {
            rows.insert(VS_WINDOW);
        }
    }
    if key.clip_plane_count > 0 {
        rows.extend(VS_WINDOW + 2..VS_WINDOW + 6);
    }
    rows
}

/// The `[[attribute(N)]]` indices the MSL's `VertexIn` declares.
fn msl_attributes(text: &str) -> BTreeSet<u16> {
    let prefix = "[[attribute(";
    let mut rows = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        rest = &rest[at + prefix.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        rows.insert(digits.parse().expect("an attribute index"));
    }
    rows
}

fn declared_inputs(p: &DxsoProgram) -> BTreeSet<u16> {
    p.declarations
        .iter()
        .filter_map(|d| match d {
            Declaration::Semantic { reg, .. } if reg.kind == RegKind::Input => Some(reg.index),
            _ => None,
        })
        .collect()
}

#[test]
fn every_synthetic_vs_key_lowers() {
    let keys = all_vs_keys();
    assert_eq!(synthetic_vs_keys().len(), 58, "synthetic_ff's vertex keys");
    for key in &keys {
        let (program, parsed) = built(key);
        if let Err(e) = lower::vertex(&parsed, &spec(key, &program)) {
            panic!("{key:?} does not lower: {e:?}");
        }
    }
}

#[test]
fn vs_reads_the_rows_the_msl_reads() {
    for key in &all_vs_keys() {
        let (_, p) = built(key);
        let text = emit_vs_ff_named(key, "t");
        let msl = msl_rows(&text);
        let direct = direct_rows(&p);
        assert!(
            direct.is_subset(&msl),
            "{key:?}: reads {:?} the MSL does not",
            direct.difference(&msl).collect::<Vec<_>>()
        );
        let unread: BTreeSet<u16> = msl.difference(&direct).copied().collect();
        let indexed = key.vertex_blend_count > 0 && key.vertex_blend_indexed();
        let unread_ok = dead_rows(key);
        assert!(
            unread.is_subset(&unread_ok),
            "{key:?}: leaves {:?} unread",
            unread.difference(&unread_ok).collect::<Vec<_>>()
        );
        // The indexed palette only relatively, from its first row.
        let relative = relative_rows(&p);
        if indexed {
            assert!(direct.iter().all(|r| *r < PALETTE), "{key:?}");
            assert_eq!(relative, (PALETTE..PALETTE + 4).collect(), "{key:?}");
        } else {
            assert!(relative.is_empty(), "{key:?}");
        }
        // The window only where `ff.rs` reads `vs_draw`/`pos_fixup` itself.
        let window = window_rows(&p);
        assert_eq!(window, expected_window(key), "{key:?}");
        assert_eq!(
            text.contains("vs_draw.point_scale"),
            window.contains(&(VS_WINDOW + 1)),
            "{key:?}"
        );
        assert_eq!(
            text.contains("vs_draw.inv_view"),
            window.contains(&(VS_WINDOW + 2)),
            "{key:?}"
        );
    }
}

#[test]
fn vs_declares_the_msl_inputs() {
    for key in &all_vs_keys() {
        let (program, p) = built(key);
        let text = emit_vs_ff_named(key, "t");
        let declared = declared_inputs(&p);
        assert_eq!(declared, msl_attributes(&text), "{key:?}");
        let mask = declared
            .iter()
            .filter(|r| **r < 16)
            .fold(0u16, |m, r| m | 1 << r);
        assert_eq!(program.provided_input_mask, mask, "{key:?}");
    }
}

/// The vertex inputs the MSL's body reads: `in.v<N>`, the blend inputs, the size, `in.p<k>`.
fn msl_reads(text: &str) -> BTreeSet<u16> {
    let body = text.split_once(") {").map_or(text, |(_, body)| body);
    let mut reads = BTreeSet::new();
    let mut rest = body;
    while let Some(at) = rest.find("in.") {
        rest = &rest[at + 3..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let index = match name.as_str() {
            "blend_weight" => Some(12),
            "blend_indices" => Some(13),
            "psize" => Some(14),
            _ => name
                .strip_prefix('v')
                .and_then(|n| n.parse().ok())
                .or_else(|| {
                    name.strip_prefix('p')
                        .and_then(|k| k.parse::<u16>().ok())
                        .map(|k| 15 + k)
                }),
        };
        reads.extend(index);
    }
    reads
}

#[test]
fn vs_reads_the_inputs_the_msl_reads() {
    for key in &all_vs_keys() {
        let (_, p) = built(key);
        let read: BTreeSet<u16> = all_srcs(&p)
            .filter(|s| s.reg.kind == RegKind::Input)
            .map(|s| s.reg.index)
            .collect();
        assert_eq!(read, msl_reads(&emit_vs_ff_named(key, "t")), "{key:?}");
    }
}

#[test]
fn vs_writes_fog_in_full() {
    for key in &all_vs_keys() {
        let (_, p) = built(key);
        let fog: Vec<_> = p
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Semantic {
                    usage: DeclUsage::Fog,
                    usage_index: 0,
                    reg,
                    mask,
                } if reg.kind != RegKind::Input => Some((*reg, *mask)),
                _ => None,
            })
            .collect();
        let [(reg, mask)] = fog.as_slice() else {
            panic!("{key:?}: one fog output, not {fog:?}");
        };
        assert_eq!(mask.0, 0xF, "{key:?}");
        let written = p
            .instructions
            .iter()
            .filter_map(|i| i.dst)
            .filter(|d| d.reg == *reg)
            .fold(0, |m, d| m | d.write_mask.0);
        assert_eq!(written, 0xF, "{key:?}: writes lanes {written:#x} of oFog");
    }
}

#[test]
fn vs_outputs_are_the_passthrough_extras() {
    for key in &all_vs_keys() {
        let (program, p) = built(key);
        let lowered = lower::vertex(&p, &spec(key, &program)).expect("lowers");
        assert_eq!(
            lowered.outputs,
            SemanticSet::passthrough_outputs(&key.passthrough),
            "{key:?}"
        );
        let outputs: Vec<(u16, DeclUsage, u32)> = p
            .declarations
            .iter()
            .filter_map(|d| match d {
                Declaration::Semantic {
                    usage,
                    usage_index,
                    reg,
                    ..
                } if reg.kind != RegKind::Input => Some((reg.index, *usage, *usage_index)),
                _ => None,
            })
            .collect();
        let basis = outputs
            .iter()
            .filter(|o| o.0 == VS_CLIP_BASIS_OUTPUT)
            .count();
        let clips = key.clip_plane_count > 0 && !key.has_rhw();
        assert_eq!(
            basis,
            usize::from(clips),
            "{key:?}: o31 is the clip basis alone"
        );
        assert!(
            outputs
                .iter()
                .all(|o| (o.0 == VS_CLIP_BASIS_OUTPUT && o.1 == DeclUsage::Position) || o.0 <= 20),
            "{key:?}: {outputs:?}"
        );
    }
}

/// The blend index chain's value for one index lane, run on its own instructions.
fn run_index_chain(chain: &[&Instruction], p: &DxsoProgram, index: f32) -> f32 {
    let read = |s: &SrcOperand, regs: &std::collections::BTreeMap<(RegKind, u16), f32>| {
        let v = match s.reg.kind {
            RegKind::Const => {
                let def = p
                    .def_constants
                    .iter()
                    .find(|d| d.reg.index == s.reg.index)
                    .expect("a literal");
                def.value[usize::from(s.swizzle.0[0])]
            }
            RegKind::Input => index,
            _ => regs.get(&(s.reg.kind, s.reg.index)).copied().unwrap_or(0.0),
        };
        match s.modifier {
            SrcModifier::None => v,
            SrcModifier::Neg => -v,
            other => panic!("{other:?} in the index chain"),
        }
    };
    let mut regs = std::collections::BTreeMap::new();
    let mut last = 0.0;
    for inst in chain {
        let a = read(&inst.srcs[0], &regs);
        let b = inst.srcs.get(1).map_or(0.0, |s| read(s, &regs));
        last = match inst.opcode {
            Opcode::Max => a.max(b),
            Opcode::Min => a.min(b),
            Opcode::Add => a + b,
            Opcode::Mul => a * b,
            Opcode::Frc => a - a.floor(),
            Opcode::MovA => a,
            other => panic!("{other:?} in the index chain"),
        };
        if let Some(d) = inst.dst {
            regs.insert((d.reg.kind, d.reg.index), last);
        }
    }
    last
}

#[test]
fn vs_blend_index_is_clamped_and_rounded() {
    let indexed: Vec<FfVsKey> = all_vs_keys()
        .into_iter()
        .filter(|k| k.vertex_blend_count > 0 && k.vertex_blend_indexed())
        .collect();
    assert_eq!(indexed.len(), 9, "the indexed blend keys");
    for key in &indexed {
        let (_, p) = built(key);
        let movas: Vec<usize> = (0..p.instructions.len())
            .filter(|i| p.instructions[*i].opcode == Opcode::MovA)
            .collect();
        let [mova] = movas.as_slice() else {
            panic!("{key:?}: one mova, not {}", movas.len());
        };
        assert!(*mova >= 6, "{key:?}");
        let chain: Vec<&Instruction> = p.instructions[mova - 6..=*mova].iter().collect();
        let ops: Vec<Opcode> = chain.iter().map(|i| i.opcode).collect();
        assert_eq!(
            ops,
            [
                Opcode::Max,
                Opcode::Min,
                Opcode::Add,
                Opcode::Frc,
                Opcode::Add,
                Opcode::Mul,
                Opcode::MovA
            ],
            "{key:?}"
        );
        assert_eq!(chain[0].srcs[0].reg.kind, RegKind::Input, "{key:?}");
        assert_eq!(chain[0].srcs[0].reg.index, 13, "{key:?}");
        // Clamped to the palette, rounded half up (not to even), then times four.
        for (index, bone) in [
            (2.4, 2.0),
            (2.5, 3.0),
            (3.5, 4.0),
            (-3.0, 0.0),
            (50.0, LAST_BONE),
            (39.5, LAST_BONE),
        ] {
            assert_eq!(
                run_index_chain(&chain, &p, index).to_bits(),
                (bone * 4.0f32).to_bits(),
                "{key:?}: index {index}"
            );
        }
    }
}

#[test]
fn vs_temporaries_fit() {
    for key in &all_vs_keys() {
        let (_, p) = built(key);
        let top = p
            .instructions
            .iter()
            .flat_map(|i| {
                i.dst
                    .iter()
                    .map(|d| d.reg)
                    .chain(i.srcs.iter().map(|s| s.reg))
            })
            .filter(|r| r.kind == RegKind::Temp)
            .map(|r| r.index)
            .max();
        assert!(
            top.is_none_or(|t| t < super::asm::TEMP_LIMIT),
            "{key:?}: r{top:?}"
        );
    }
}

#[test]
fn vs_tokens_round_trip() {
    for key in &all_vs_keys() {
        let (program, p) = built(key);
        assert_eq!(&write(&p)[..], &program.tokens[..], "{key:?}");
    }
}

#[test]
fn ff_build_is_deterministic() {
    for key in &all_vs_keys() {
        let (a, b) = (vertex(key), vertex(key));
        assert_eq!(a.tokens, b.tokens, "{key:?}");
        assert_eq!(a.warnings, b.warnings, "{key:?}");
        assert_eq!(a.provided_input_mask, b.provided_input_mask, "{key:?}");
    }
}

#[test]
fn vs_warns_where_ff_rs_warns() {
    let unlit = FfVsFlags::HAS_COLOR0;
    let rhw = FfVsFlags::HAS_RHW | FfVsFlags::HAS_COLOR0;
    let lit = FfVsFlags::HAS_NORMAL | FfVsFlags::LIGHTING_ENABLED | FfVsFlags::COLOR_VERTEX;
    let mut absent = key(unlit, 1, 2);
    absent.tci[1] = tci_entry(TCI_PASSTHRU, 3);
    let mut absent_rhw = key(rhw, 1, 2);
    absent_rhw.tci[1] = tci_entry(TCI_PASSTHRU, 5);
    let mut texgen_rhw = key(rhw, 1, 2);
    texgen_rhw.tci[1] = tci_entry(TCI_SPHEREMAP, 0);
    let mut undefined = key(unlit, 1, 2);
    undefined.tci[1] = tci_entry(TCI_UNDEFINED, 0);
    let mut undefined_absent = key(unlit, 1, 2);
    undefined_absent.tci[1] = tci_entry(TCI_UNDEFINED, 4);
    // Undefined on a pre-transformed stage takes the passthru without a warning.
    let mut undefined_rhw = key(rhw, 1, 2);
    undefined_rhw.tci[1] = tci_entry(TCI_UNDEFINED, 0);
    let mut material = lights(key(lit, 1, 1), 0b1, 0b1, 0);
    material.emissive_source = 3;
    // Without COLOR_VERTEX the source is not read, so not warned about.
    let mut quiet_material = lights(key(lit - FfVsFlags::COLOR_VERTEX, 1, 1), 0b1, 0b1, 0);
    quiet_material.emissive_source = 3;
    let cases: [(&FfVsKey, &[&str]); 9] = [
        (&key(unlit, 1, 1), &[]),
        (&absent, &[vs::WARN_ABSENT_SET]),
        (&absent_rhw, &[vs::WARN_ABSENT_SET]),
        (&texgen_rhw, &[vs::WARN_RHW_TEXGEN]),
        (&undefined, &[vs::WARN_UNDEFINED_TCI]),
        (
            &undefined_absent,
            &[vs::WARN_UNDEFINED_TCI, vs::WARN_ABSENT_SET],
        ),
        (&undefined_rhw, &[]),
        (&material, &[vs::WARN_MATERIAL_SOURCE]),
        (&quiet_material, &[]),
    ];
    for (key, want) in cases {
        assert_eq!(vertex(key).warnings, want, "{key:?}");
    }
    for key in &all_vs_keys() {
        let warnings = vertex(key).warnings;
        let unique: BTreeSet<&str> = warnings.iter().copied().collect();
        assert_eq!(unique.len(), warnings.len(), "{key:?}: once each");
    }
}

#[test]
fn ps_key_lists_emit() {
    assert_eq!(synthetic_ps_keys().len(), 52, "synthetic_ff's pixel keys");
    for (key, variant) in &all_ps_keys() {
        assert!(
            emit_ps_ff_named(key, *variant, "t").contains("fragment"),
            "{key:?}"
        );
    }
}
