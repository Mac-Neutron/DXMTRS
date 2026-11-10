//! The pixel builder against `ff.rs`'s MSL for the same key.
//!
//! Every key of `ff_build::tests`'s lists, and a sweep of every op and a set of arguments, is
//! built, parsed and lowered; the rows, textures, samples and `bump_env` rows the program names
//! are the ones the MSL text names.

use std::collections::BTreeSet;

use airconv_ir::{
    dxso::{
        Declaration, DxsoProgram, RegKind,
        ir::{InstrFlags, SrcModifier, SrcOperand},
        lower::{self, range},
        opcode::Opcode,
        parse,
        spec::{VariantFlags, VariantKey},
    },
    dxso_write::write,
};
use mtld3d_types::{
    D3DCMP_EQUAL, D3DFOG_EXP, D3DTA_ALPHAREPLICATE, D3DTA_COMPLEMENT, D3DTA_CONSTANT,
    D3DTA_CURRENT, D3DTA_DIFFUSE, D3DTA_SPECULAR, D3DTA_TEMP, D3DTA_TEXTURE, D3DTA_TFACTOR,
    D3DTOP_ADD, D3DTOP_BUMPENVMAP, D3DTOP_DISABLE, D3DTOP_LERP, D3DTOP_MODULATE, D3DTOP_SELECTARG1,
};

use super::{WARN_ARGUMENT, WARN_MODIFIER, WARN_OP, pixel};
use crate::{
    dxso::{FfPsKey, FfStageFlags, FfStageResult, emit_ps_ff_named},
    ff_build::{
        FfProgram,
        asm::TEMP_LIMIT,
        tests::{all_ps_keys, ps_key},
    },
};

/// A selector past `D3DTA_CONSTANT`, which `ff.rs` has no case for.
const UNKNOWN_SELECTOR: u32 = 7;
/// An argument modifier bit other than alpha replicate and complement.
const UNKNOWN_MODIFIER: u32 = 0x40;

fn built(key: &FfPsKey) -> (FfProgram, DxsoProgram) {
    let program = pixel(key);
    let parsed = parse(&program.tokens)
        .unwrap_or_else(|e| panic!("{key:?} builds tokens that parse: {e:?}"));
    (program, parsed)
}

/// Variants beside a key's own, each exercising what the lowering applies from the variant.
fn variants() -> Vec<VariantKey> {
    let plain = VariantKey::default();
    vec![
        VariantKey {
            flags: VariantFlags::LOD_BIAS,
            ..plain
        },
        VariantKey {
            depth_sampler_mask: 0xFF,
            flags: VariantFlags::LOD_BIAS,
            ..plain
        },
        VariantKey {
            depth_sampler_mask: 0xFF,
            depth_fetch_mask: 0xFF,
            raw_depth_red_mask: 0x0F,
            ..plain
        },
        VariantKey {
            cube_sampler_mask: 0xFF,
            ..plain
        },
        VariantKey {
            volume_sampler_mask: 0xFF,
            flags: VariantFlags::LOD_BIAS,
            ..plain
        },
        VariantKey {
            fetch4_mask: 0xFF,
            fetch4_alpha_mask: 0x0F,
            ..plain
        },
        VariantKey {
            alpha_func: u8::try_from(D3DCMP_EQUAL).unwrap_or(0),
            fog_table_mode: u8::try_from(D3DFOG_EXP).unwrap_or(0),
            sample_mask: 0b101,
            flags: VariantFlags::FLAT_SHADE
                | VariantFlags::POINT_SPRITE
                | VariantFlags::SRGB_WRITE
                | VariantFlags::SAMPLE_MASK,
            ..plain
        },
    ]
}

/// Every op value 0 to 27 and 255, as stage 0's colour op and as its alpha op.
///
/// Over arguments with modifiers on either side, an unknown selector and modifier, with and
/// without a texture, writing CURRENT and TEMP (which stage 1 reads).
fn sweep_keys() -> Vec<(FfPsKey, VariantKey)> {
    let args = [
        (D3DTA_TEXTURE, D3DTA_DIFFUSE),
        (
            D3DTA_TEXTURE | D3DTA_COMPLEMENT,
            D3DTA_CURRENT | D3DTA_ALPHAREPLICATE | D3DTA_COMPLEMENT,
        ),
        (
            D3DTA_TEMP | D3DTA_COMPLEMENT,
            D3DTA_TEXTURE | D3DTA_COMPLEMENT,
        ),
        (D3DTA_CONSTANT, UNKNOWN_SELECTOR | D3DTA_COMPLEMENT),
        (D3DTA_TFACTOR | UNKNOWN_MODIFIER, D3DTA_SPECULAR),
    ];
    let reader = [
        D3DTOP_ADD,
        D3DTA_TEMP,
        D3DTA_CURRENT,
        D3DTOP_ADD,
        D3DTA_TEMP,
        D3DTA_CURRENT,
    ];
    let mut keys = Vec::new();
    for op in (0..=27).chain([255]) {
        for (a1, a2) in args {
            let stages = [
                [op, a1, a2, D3DTOP_SELECTARG1, a1, a2],
                [D3DTOP_MODULATE, a1, a2, op, a1, a2],
            ];
            for stage in stages {
                for textured in [false, true] {
                    for temp in [false, true] {
                        let mut k = ps_key(&[stage, reader], temp);
                        k.stages[0].flags.set(FfStageFlags::HAS_TEXTURE, textured);
                        if temp {
                            k.stages[0].set_result(FfStageResult::Temp);
                        }
                        keys.push((k, VariantKey::default()));
                    }
                }
            }
        }
        // TEMP's liveness hangs on whether the swept op's text names it: stage 1 writes TEMP.
        let writer = [
            D3DTOP_MODULATE,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
            D3DTOP_MODULATE,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
        ];
        for (a1, a2) in [(D3DTA_TEMP, D3DTA_DIFFUSE), (D3DTA_DIFFUSE, D3DTA_TEMP)] {
            for stage in [
                [op, a1, a2, D3DTOP_SELECTARG1, D3DTA_DIFFUSE, D3DTA_DIFFUSE],
                [D3DTOP_MODULATE, D3DTA_DIFFUSE, D3DTA_DIFFUSE, op, a1, a2],
            ] {
                let mut k = ps_key(&[stage, writer], false);
                k.stages[1].set_result(FfStageResult::Temp);
                keys.push((k, VariantKey::default()));
            }
        }
    }
    keys
}

fn keys() -> Vec<(FfPsKey, VariantKey)> {
    let mut keys = all_ps_keys();
    keys.extend(sweep_keys());
    keys
}

fn all_srcs(p: &DxsoProgram) -> impl Iterator<Item = &SrcOperand> {
    p.instructions.iter().flat_map(|i| &i.srcs)
}

/// The numbers `text` names as `<prefix><n><suffix>`.
fn named(text: &str, prefix: &str, suffix: &str) -> BTreeSet<u16> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        rest = &rest[at + prefix.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if rest[digits.len()..].starts_with(suffix)
            && let Ok(n) = digits.parse()
        {
            found.insert(n);
        }
    }
    found
}

fn mask_bits(mask: u16) -> BTreeSet<u16> {
    (0..16).filter(|i| mask & (1 << i) != 0).collect()
}

fn declared_samplers(p: &DxsoProgram) -> BTreeSet<u16> {
    p.declarations
        .iter()
        .filter_map(|d| match d {
            Declaration::Sampler { reg, .. } => Some(reg.index),
            Declaration::Semantic { .. } => None,
        })
        .collect()
}

#[test]
fn every_synthetic_ps_key_lowers() {
    let extra = variants();
    for (key, variant) in &keys() {
        let (_, p) = built(key);
        let own = (key, variant);
        for variant in core::iter::once(variant).chain(&extra) {
            if let Err(e) = lower::pixel(&p, variant) {
                panic!("{own:?} under {variant:?} does not lower: {e:?}");
            }
        }
    }
}

#[test]
fn ps_reads_the_rows_the_msl_reads() {
    for (key, variant) in &keys() {
        let (_, p) = built(key);
        let text = emit_ps_ff_named(key, *variant, "t");
        let direct: BTreeSet<u16> = all_srcs(&p)
            .filter(|s| s.reg.kind == RegKind::Const && s.reg.index < 16)
            .map(|s| s.reg.index)
            .collect();
        assert_eq!(direct, named(&text, "ps_c[", "]"), "{key:?}");
        assert!(
            all_srcs(&p).all(|s| s.rel_addr.is_none()),
            "{key:?}: no relative read"
        );
    }
}

#[test]
fn ps_declares_the_msl_textures() {
    for (key, variant) in &keys() {
        let (_, p) = built(key);
        let text = emit_ps_ff_named(key, *variant, "t");
        let declared = declared_samplers(&p);
        assert_eq!(declared, mask_bits(key.sampled_stage_mask()), "{key:?}");
        assert_eq!(declared, named(&text, "[[texture(", ")]]"), "{key:?}");
        let bound: BTreeSet<u16> = lower::ps_bindings(&p, variant)
            .samplers
            .into_keys()
            .collect();
        assert_eq!(bound, declared, "{key:?}");
    }
}

#[test]
fn ps_samples_what_the_msl_samples() {
    for (key, variant) in &keys() {
        let (_, p) = built(key);
        let text = emit_ps_ff_named(key, *variant, "t");
        let sampled: Vec<u16> = p
            .instructions
            .iter()
            .filter(|i| i.opcode == Opcode::TexLd)
            .map(|i| i.srcs[1].reg.index)
            .collect();
        let unique: BTreeSet<u16> = sampled.iter().copied().collect();
        assert_eq!(unique.len(), sampled.len(), "{key:?}: each stage once");
        assert_eq!(unique, named(&text, "float4 t", " = "), "{key:?}");
    }
}

#[test]
fn ps_binds_bump_env_when_the_msl_does() {
    for (key, variant) in &keys() {
        let (_, p) = built(key);
        let text = emit_ps_ff_named(key, *variant, "t");
        let msl = text.contains("bump_env [[buffer(12)]]");
        let bound = lower::ps_bindings(&p, variant)
            .ranges
            .contains(&range::BUMP_ENV);
        assert_eq!(bound, msl, "{key:?}");
    }
}

#[test]
fn ps_reads_the_bump_rows_the_msl_reads() {
    for (key, variant) in &keys() {
        let (_, p) = built(key);
        let text = emit_ps_ff_named(key, *variant, "t");
        let matrices: BTreeSet<u16> = p
            .instructions
            .iter()
            .filter(|i| i.opcode == Opcode::Bem)
            .filter_map(|i| i.dst.map(|d| 2 * d.reg.index))
            .collect();
        let luminance: BTreeSet<u16> = all_srcs(&p)
            .filter(|s| s.reg.kind == RegKind::Output)
            .map(|s| s.reg.index)
            .collect();
        let msl = named(&text, "bump_env[", "]");
        let even: BTreeSet<u16> = msl.iter().copied().filter(|r| r % 2 == 0).collect();
        let odd: BTreeSet<u16> = msl.iter().copied().filter(|r| r % 2 == 1).collect();
        assert_eq!(matrices, even, "{key:?}: bem matrices");
        assert_eq!(luminance, odd, "{key:?}: luminance rows");
    }
}

#[test]
fn ps_guards_the_projections_the_msl_guards() {
    for (key, variant) in &keys() {
        let (_, p) = built(key);
        let text = emit_ps_ff_named(key, *variant, "t");
        let guarded: BTreeSet<u16> = p
            .instructions
            .iter()
            .filter(|i| i.opcode == Opcode::Cmp)
            .map(|i| i.srcs[2].reg.index - 2)
            .collect();
        let msl: BTreeSet<u16> = named(&text, "in.texcoord", ".w != 0.0")
            .union(&named(&text, "(in.texcoord", ".w != 0.0"))
            .copied()
            .collect();
        assert_eq!(guarded, msl, "{key:?}");
        let projected = p
            .instructions
            .iter()
            .filter(|i| i.opcode == Opcode::TexLd && i.flags.contains(InstrFlags::TEX_PROJECTED));
        let divided = p
            .instructions
            .iter()
            .filter(|i| i.srcs.iter().any(|s| s.modifier == SrcModifier::Dw));
        assert_eq!(
            projected.count() + divided.count(),
            guarded.len(),
            "{key:?}"
        );
    }
}

#[test]
fn ps_tokens_round_trip() {
    for (key, _) in &keys() {
        let (program, p) = built(key);
        assert_eq!(&write(&p)[..], &program.tokens[..], "{key:?}");
    }
}

#[test]
fn ps_temporaries_fit() {
    for (key, _) in &keys() {
        let (_, p) = built(key);
        let mut temps = p
            .instructions
            .iter()
            .flat_map(|i| {
                i.dst
                    .iter()
                    .map(|d| d.reg)
                    .chain(i.srcs.iter().map(|s| s.reg))
            })
            .filter(|r| r.kind == RegKind::Temp);
        assert!(temps.clone().count() > 0, "{key:?}: uses temporaries");
        assert!(temps.all(|r| r.index < TEMP_LIMIT), "{key:?}");
    }
}

#[test]
fn ps_build_is_deterministic() {
    for (key, _) in &keys() {
        let (a, b) = (pixel(key), pixel(key));
        assert_eq!(a.tokens, b.tokens, "{key:?}");
        assert_eq!(a.warnings, b.warnings, "{key:?}");
        assert_eq!(
            a.provided_input_mask, 0,
            "{key:?}: a pixel program declares no vertex input"
        );
    }
}

#[test]
fn ps_warns_where_ff_rs_warns() {
    let stage = |op: u32, a1: u32, a2: u32| [op, a1, a2, D3DTOP_SELECTARG1, D3DTA_DIFFUSE, 0];
    let modulate = stage(D3DTOP_MODULATE, D3DTA_TEXTURE, D3DTA_DIFFUSE);
    // Stage 1 writes TEMP and nothing reads it: dead, but `ff.rs` still resolves it.
    let mut dead = ps_key(&[modulate, stage(D3DTOP_LERP, D3DTA_DIFFUSE, 0)], false);
    dead.stages[1].set_result(FfStageResult::Temp);
    // A bump stage and the unbound fallback resolve nothing.
    let bump = ps_key(
        &[
            stage(D3DTOP_BUMPENVMAP, UNKNOWN_SELECTOR, D3DTA_CURRENT),
            modulate,
        ],
        false,
    );
    let mut fallback = ps_key(
        &[stage(D3DTOP_MODULATE, D3DTA_TEXTURE, UNKNOWN_SELECTOR)],
        false,
    );
    fallback.stages[0].flags = FfStageFlags::empty();
    // A kept alpha resolves no alpha argument.
    let kept = ps_key(
        &[[
            D3DTOP_MODULATE,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
            D3DTOP_DISABLE,
            UNKNOWN_SELECTOR,
            UNKNOWN_MODIFIER,
        ]],
        false,
    );
    // A stage behind DISABLE is not in the cascade.
    let behind = ps_key(
        &[
            modulate,
            stage(D3DTOP_DISABLE, D3DTA_DIFFUSE, D3DTA_CURRENT),
            stage(D3DTOP_LERP, UNKNOWN_SELECTOR, 0),
        ],
        false,
    );
    let cases: [(FfPsKey, &[&str]); 11] = [
        (ps_key(&[modulate], false), &[]),
        (
            ps_key(&[stage(D3DTOP_ADD, UNKNOWN_SELECTOR, D3DTA_DIFFUSE)], false),
            &[WARN_ARGUMENT],
        ),
        // In the slot SELECTARG1 discards.
        (
            ps_key(
                &[stage(D3DTOP_SELECTARG1, D3DTA_DIFFUSE, UNKNOWN_SELECTOR)],
                false,
            ),
            &[WARN_ARGUMENT],
        ),
        (
            ps_key(
                &[stage(
                    D3DTOP_ADD,
                    D3DTA_DIFFUSE | UNKNOWN_MODIFIER,
                    D3DTA_CURRENT,
                )],
                false,
            ),
            &[WARN_MODIFIER],
        ),
        (
            ps_key(
                &[
                    stage(D3DTOP_ADD, UNKNOWN_SELECTOR | 0x80, D3DTA_CURRENT),
                    stage(D3DTOP_ADD, UNKNOWN_SELECTOR, D3DTA_CURRENT),
                ],
                false,
            ),
            &[WARN_ARGUMENT, WARN_MODIFIER],
        ),
        (
            ps_key(&[stage(D3DTOP_LERP, D3DTA_DIFFUSE, D3DTA_CURRENT)], false),
            &[WARN_OP],
        ),
        (dead, &[WARN_OP]),
        (bump, &[]),
        (fallback, &[]),
        (kept, &[]),
        (behind, &[]),
    ];
    for (key, want) in &cases {
        assert_eq!(pixel(key).warnings, *want, "{key:?}");
    }
    // An alpha op `ff.rs` has no arm for, and an unknown alpha argument.
    let alpha = ps_key(
        &[[
            D3DTOP_MODULATE,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
            0,
            D3DTA_DIFFUSE,
            UNKNOWN_SELECTOR,
        ]],
        false,
    );
    assert_eq!(pixel(&alpha).warnings, [WARN_ARGUMENT, WARN_OP]);
    for (key, _) in &keys() {
        let warnings = pixel(key).warnings;
        let unique: BTreeSet<&str> = warnings.iter().copied().collect();
        assert_eq!(unique.len(), warnings.len(), "{key:?}: once each");
    }
}

#[test]
fn ps_key_lists_name_the_new_shapes() {
    let keys = all_ps_keys();
    let bump_stage = |k: &FfPsKey| {
        u32::from(k.stages[0].color_op) == D3DTOP_BUMPENVMAP
            && !k.stages[0].has_texture()
            && (u32::from(k.stages[0].color_arg1) & 0xF) != D3DTA_TEXTURE
    };
    assert!(
        keys.iter().any(|(k, _)| bump_stage(k)),
        "an untextured bump stage"
    );
    assert!(
        keys.iter()
            .any(|(k, _)| u32::from(k.stages[0].alpha_arg2) == (D3DTA_CURRENT | D3DTA_COMPLEMENT)),
        "a complemented second argument"
    );
    assert!(
        keys.iter()
            .any(|(k, _)| u32::from(k.stages[0].alpha_op) == D3DTOP_BUMPENVMAP),
        "a bump op as the alpha op"
    );
    assert!(
        keys.iter()
            .any(|(k, _)| u32::from(k.stages[0].color_arg2) & D3DTA_ALPHAREPLICATE != 0),
        "a replicated second argument"
    );
}
