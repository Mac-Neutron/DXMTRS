//! The fixed-function pixel stage as `ps_3_0`.
//!
//! `dxso::emit_ps`'s MSL in its order, unrolled for the key: `current` starts at the diffuse
//! colour, each active stage samples its texture where its emitted expressions name it and
//! writes CURRENT or TEMP, then the specular add and `oC0`. What the variant drives (the texture
//! kinds, depth compares, fetch4, the LOD table, alpha test, fog, sRGB, the sample mask, point
//! sprites, flat shading) the lowering applies, so the tokens depend on the key alone.
//!
//! The control sets are `ff.rs`'s rules restated, as its predicates are private to the
//! fingerprinted emitter: the active cascade, bump-environment stages, the unbound-texture
//! fallbacks, which arguments an op's emitted text names (which decides the samples and whether
//! TEMP is live), and when `bump_env` is read.
//!
//! Registers: `v0`/`v1` the colours, `v<2 + n>` texture coordinate set `n`; `r<n>` the
//! displaced coordinate of the stage after bump stage `n` (`bem`'s matrix is its destination's
//! `bump_env` row `2n`), `r8` CURRENT, `r9` TEMP (zero until written, as the lowering zeroes
//! every named register), `r<10 + n>` stage `n`'s texel, scratch from `r18`. A stage computes
//! colour and alpha into one scratch register before writing either, so both read the old
//! values. Blends are `mul` + `mad` (`lrp` reassociates); `_sat` goes on the last instruction of
//! an op only.

use airconv_ir::dxso::{
    DeclUsage, RegKind, ShaderType, TextureType,
    ir::{DstOperand, InstrFlags, Register, SrcModifier, SrcOperand, Swizzle},
    opcode::Opcode::{Add, Bem, Cmp, Dp3, Mad, Mov, Mul},
};
use mtld3d_types::{
    D3DTA_ALPHAREPLICATE, D3DTA_COMPLEMENT, D3DTA_CONSTANT, D3DTA_CURRENT, D3DTA_DIFFUSE,
    D3DTA_SELECTMASK, D3DTA_SPECULAR, D3DTA_TEMP, D3DTA_TEXTURE, D3DTA_TFACTOR, D3DTOP_ADD,
    D3DTOP_ADDSIGNED, D3DTOP_ADDSIGNED2X, D3DTOP_ADDSMOOTH, D3DTOP_BLENDCURRENTALPHA,
    D3DTOP_BLENDDIFFUSEALPHA, D3DTOP_BLENDFACTORALPHA, D3DTOP_BLENDTEXTUREALPHA,
    D3DTOP_BLENDTEXTUREALPHAPM, D3DTOP_BUMPENVMAPLUMINANCE, D3DTOP_DISABLE, D3DTOP_DOTPRODUCT3,
    D3DTOP_MODULATE, D3DTOP_MODULATE2X, D3DTOP_MODULATE4X, D3DTOP_MODULATEALPHA_ADDCOLOR,
    D3DTOP_MODULATECOLOR_ADDALPHA, D3DTOP_MODULATEINVALPHA_ADDCOLOR,
    D3DTOP_MODULATEINVCOLOR_ADDALPHA, D3DTOP_SELECTARG1, D3DTOP_SELECTARG2, D3DTOP_SUBTRACT,
};

use super::{
    FfProgram,
    asm::{
        Asm, PS_DEF_BASE, abs, bias, bump_env, c, color_out, comp, dst as d, dw, neg, reg, sat,
        shift, src as s,
    },
};
use crate::dxso::{FfPsKey, FfStage, FfStageResult, is_bump_env_op};

/// `ff.rs` warns: an argument selector it has no case for reads `float4(1.0)`.
pub const WARN_ARGUMENT: &str = "ff-fallback texture-arg unhandled → float4(1.0)";
/// `ff.rs` warns: an argument modifier bit other than alpha replicate and complement is ignored.
pub const WARN_MODIFIER: &str = "ff-fallback texture-arg modifier unhandled";
/// `ff.rs` warns: an operation it has no arm for selects its first argument.
pub const WARN_OP: &str = "ff-fallback texture-op unhandled → SELECTARG1";

/// `r8`: CURRENT.
const CURRENT: Register = temp(8);
/// `r9`: TEMP.
const TEMP: Register = temp(9);
/// `r10` to `r17`: the stages' texels.
const TEXEL_BASE: u16 = 10;
/// The registers reserved before the first scratch, `r0` to `r17`.
const FIXED_TEMPS: u16 = 18;
/// The diffuse and specular colours.
const DIFFUSE: Register = reg(RegKind::Input, 0);
const SPECULAR: Register = reg(RegKind::Input, 1);
/// The texture coordinate sets, `v2` to `v9`.
const TEXCOORD_BASE: u16 = 2;

/// Builds `key`'s pixel stage.
#[must_use]
pub fn pixel(key: &FfPsKey) -> FfProgram {
    let active = &key.stages[..active_count(key)];
    let mut ps = Ps {
        a: Asm::new(ShaderType::Pixel, PS_DEF_BASE),
        key,
        warnings: Vec::new(),
    };
    for _ in 0..FIXED_TEMPS {
        ps.a.temp();
    }
    ps.a.input(DeclUsage::Color, 0, DIFFUSE.index);
    ps.a.input(DeclUsage::Color, 1, SPECULAR.index);
    for set in 0..8 {
        ps.a.input(DeclUsage::Texcoord, u32::from(set), TEXCOORD_BASE + set);
    }
    let declared = key.sampled_stage_mask();
    for slot in (0..8).filter(|slot| declared & (1 << slot) != 0) {
        ps.a.sampler(TextureType::Texture2D, slot);
    }
    for stage in active.iter().filter(|st| !is_bump(**st)) {
        ps.warn_stage(*stage);
    }
    let reads_temp = active
        .iter()
        .any(|st| !is_bump(*st) && stage_names(*st, D3DTA_TEMP));
    let sampled = sampled_stages(active, reads_temp);

    ps.mov(d(CURRENT, "xyzw"), s(DIFFUSE, "xyzw"));
    for (i, stage) in active.iter().enumerate() {
        let sampled_here = sampled & (1 << i) != 0;
        if is_bump(*stage) {
            if sampled_here {
                ps.sample(i);
            }
            continue;
        }
        if stage.result() == FfStageResult::Temp && !reads_temp {
            continue;
        }
        if sampled_here {
            ps.sample(i);
        }
        ps.stage(*stage, i);
    }
    if key.specular_add {
        ps.a.op(
            Add,
            sat(d(CURRENT, "xyz")),
            [s(CURRENT, "xyzw"), s(SPECULAR, "xyzw")],
        );
    }
    ps.mov(d(color_out(0), "xyzw"), s(CURRENT, "xyzw"));
    FfProgram {
        tokens: ps.a.finish().into(),
        provided_input_mask: 0,
        warnings: ps.warnings,
    }
}

/// Where one stage's result alpha comes from (`ff.rs`'s `StageAlpha`, its fallback apart).
#[derive(PartialEq, Eq)]
enum AlphaSource {
    /// The colour op writes all four channels (DOTPRODUCT3).
    FromColor,
    /// The destination keeps its alpha (DISABLE, or a bump op in the alpha role).
    Kept,
    /// The unbound-texture fallback: CURRENT's alpha.
    Current,
    /// The alpha op's result.
    Op,
}

struct Ps<'k> {
    a: Asm,
    key: &'k FfPsKey,
    warnings: Vec<&'static str>,
}

impl Ps<'_> {
    fn warn(&mut self, warning: &'static str) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    /// `ff.rs`'s warnings for one stage, in its order: both arguments resolve, then the op.
    fn warn_stage(&mut self, stage: FfStage) {
        if !colour_fallback(stage) {
            self.warn_op(stage.color_op, stage.color_arg1, stage.color_arg2);
        }
        if alpha_source(stage) == AlphaSource::Op {
            self.warn_op(stage.alpha_op, stage.alpha_arg1, stage.alpha_arg2);
        }
    }

    fn warn_op(&mut self, op: u8, arg1: u8, arg2: u8) {
        for arg in [arg1, arg2] {
            if u32::from(arg) & D3DTA_SELECTMASK > D3DTA_CONSTANT {
                self.warn(WARN_ARGUMENT);
            }
            if u32::from(arg) & 0xF0 & !(D3DTA_ALPHAREPLICATE | D3DTA_COMPLEMENT) != 0 {
                self.warn(WARN_MODIFIER);
            }
        }
        if !handled_op(op) {
            self.warn(WARN_OP);
        }
    }

    fn mov(&mut self, dst: DstOperand, src: SrcOperand) {
        self.a.op(Mov, dst, [src]);
    }

    /// `src` with no modifier: itself, or a scratch copy.
    fn plain(&mut self, src: SrcOperand) -> SrcOperand {
        if src.modifier == SrcModifier::None {
            return src;
        }
        let t = self.a.temp();
        self.mov(d(t, "xyzw"), src);
        s(t, "xyzw")
    }

    /// `coord`, or `(0, 0, 0, 1)` where its `w` is zero: the origin a projected sample reads.
    fn guarded(&mut self, coord: Register) -> Register {
        let origin = self.a.vec4([0.0, 0.0, 0.0, 1.0]);
        let t = self.a.temp();
        self.a.op(
            Cmp,
            d(t, "xyzw"),
            [neg(abs(s(coord, "w"))), origin, s(coord, "xyzw")],
        );
        t
    }

    /// Stage `i`'s texel into `r<10 + i>` (`emit_stage_sample`).
    ///
    /// A bump stage before it with a texture displaces the coordinate (after the projective
    /// divide, so the sample is not projected); a luminance one then scales all four channels.
    fn sample(&mut self, i: usize) {
        let mark = self.a.mark();
        let slot = u16::try_from(i).unwrap_or(0);
        let texel = temp(TEXEL_BASE + slot);
        let coord = reg(RegKind::Input, TEXCOORD_BASE + slot);
        let sampler = reg(RegKind::Sampler, slot);
        let projected = self.key.tt_projected_mask & (1 << i) != 0;
        let bump = i
            .checked_sub(1)
            .map(|n| (n, self.key.stages[n]))
            .filter(|(_, st)| is_bump(*st));
        match bump {
            Some((n, st)) if st.has_texture() => {
                let displaced = temp(u16::try_from(n).unwrap_or(0));
                if projected {
                    let p = self.guarded(coord);
                    self.mov(d(displaced, "xyz"), dw(s(p, "xyzw")));
                    let one = self.a.lit(1.0);
                    self.mov(d(displaced, "w"), one);
                } else {
                    self.mov(d(displaced, "xyzw"), s(coord, "xyzw"));
                }
                self.a.op(
                    Bem,
                    d(displaced, "xy"),
                    [s(displaced, "xyzw"), s(texel_of(n), "xyzw")],
                );
                self.a.texld(
                    d(texel, "xyzw"),
                    s(displaced, "xyzw"),
                    sampler,
                    InstrFlags::empty(),
                );
            }
            _ if projected => {
                let p = self.guarded(coord);
                self.a.texld(
                    d(texel, "xyzw"),
                    s(p, "xyzw"),
                    sampler,
                    InstrFlags::TEX_PROJECTED,
                );
            }
            _ => self.a.texld(
                d(texel, "xyzw"),
                s(coord, "xyzw"),
                sampler,
                InstrFlags::empty(),
            ),
        }
        if let Some((n, st)) = bump.filter(|(_, st)| is_luminance(*st)) {
            let terms = bump_env(u16::try_from(2 * n + 1).unwrap_or(0));
            let l = self.a.temp();
            if st.has_texture() {
                self.a.op(
                    Mad,
                    sat(d(l, "x")),
                    [s(texel_of(n), "z"), s(terms, "x"), s(terms, "y")],
                );
            } else {
                self.mov(sat(d(l, "x")), s(terms, "y"));
            }
            self.a
                .op(Mul, d(texel, "xyzw"), [s(texel, "xyzw"), s(l, "x")]);
        }
        self.a.release(mark);
    }

    /// Stage `i`'s colour and alpha, written to its result register.
    fn stage(&mut self, stage: FfStage, i: usize) {
        let mark = self.a.mark();
        let r = self.a.temp();
        let alpha = alpha_source(stage);
        let colour_mask = if alpha == AlphaSource::FromColor {
            "xyzw"
        } else {
            "xyz"
        };
        if colour_fallback(stage) {
            self.mov(d(r, colour_mask), s(CURRENT, "xyzw"));
        } else {
            self.op(
                [stage.color_op, stage.color_arg1, stage.color_arg2],
                d(r, colour_mask),
                stage,
                i,
            );
        }
        match alpha {
            AlphaSource::FromColor | AlphaSource::Kept => {}
            AlphaSource::Current => self.mov(d(r, "w"), s(CURRENT, "w")),
            AlphaSource::Op => self.op(
                [stage.alpha_op, stage.alpha_arg1, stage.alpha_arg2],
                d(r, "w"),
                stage,
                i,
            ),
        }
        let result = match stage.result() {
            FfStageResult::Current => CURRENT,
            FfStageResult::Temp => TEMP,
        };
        let write = if alpha == AlphaSource::Kept {
            "xyz"
        } else {
            "xyzw"
        };
        self.mov(d(result, write), s(r, "xyzw"));
        self.a.release(mark);
    }

    /// Argument `arg` of stage `i` (`resolve_arg`): alpha replicated, then complemented.
    fn arg(&mut self, arg: u8, i: usize, textured: bool) -> SrcOperand {
        let base = match u32::from(arg) & D3DTA_SELECTMASK {
            D3DTA_DIFFUSE => s(DIFFUSE, "xyzw"),
            D3DTA_CURRENT => s(CURRENT, "xyzw"),
            D3DTA_TEMP => s(TEMP, "xyzw"),
            D3DTA_SPECULAR => s(SPECULAR, "xyzw"),
            D3DTA_TEXTURE if textured => s(texel_of(i), "xyzw"),
            D3DTA_TFACTOR => s(c(0), "xyzw"),
            D3DTA_CONSTANT => s(c(u16::try_from(i + 1).unwrap_or(0)), "xyzw"),
            _ => self.a.lit(1.0),
        };
        let replicated = if u32::from(arg) & D3DTA_ALPHAREPLICATE != 0 {
            alpha_of(base)
        } else {
            base
        };
        if u32::from(arg) & D3DTA_COMPLEMENT != 0 {
            comp(replicated)
        } else {
            replicated
        }
    }

    /// `apply_op`'s arm for `[op, arg1, arg2]` into `dst`, resolving only the arguments it uses.
    fn op(&mut self, [op, arg1, arg2]: [u8; 3], dst: DstOperand, stage: FfStage, i: usize) {
        let textured = stage.has_texture();
        let ([uses_a, uses_b], _) = arm(op, textured);
        // An argument the arm discards is never emitted: no literal for it.
        let unused = s(DIFFUSE, "xyzw");
        let a = if uses_a {
            self.arg(arg1, i, textured)
        } else {
            unused
        };
        let b = if uses_b {
            self.arg(arg2, i, textured)
        } else {
            unused
        };
        let partial = s(dst.reg, "xyzw");
        let texel_alpha = s(texel_of(i), "w");
        match u32::from(op) {
            D3DTOP_BLENDTEXTUREALPHA if textured => self.blend(dst, a, b, texel_alpha),
            D3DTOP_BLENDTEXTUREALPHAPM if textured => {
                self.a.op(Mad, sat(dst), [b, comp(texel_alpha), a]);
            }
            // An implicit-only missing texture has zero alpha: the blend selects arg2.
            D3DTOP_SELECTARG2 | D3DTOP_BLENDTEXTUREALPHA => self.mov(dst, b),
            D3DTOP_MODULATE => self.a.op(Mul, dst, [a, b]),
            D3DTOP_MODULATE2X => self.a.op(Mul, shift(sat(dst), 1), [a, b]),
            D3DTOP_MODULATE4X => self.a.op(Mul, shift(sat(dst), 2), [a, b]),
            D3DTOP_MODULATEALPHA_ADDCOLOR => self.a.op(Mad, sat(dst), [alpha_of(a), b, a]),
            D3DTOP_MODULATECOLOR_ADDALPHA => self.a.op(Mad, sat(dst), [a, b, alpha_of(a)]),
            D3DTOP_MODULATEINVALPHA_ADDCOLOR => {
                let a = self.plain(a);
                self.a.op(Mad, sat(dst), [comp(alpha_of(a)), b, a]);
            }
            D3DTOP_MODULATEINVCOLOR_ADDALPHA => {
                let a = self.plain(a);
                self.a.op(Mad, sat(dst), [comp(a), b, alpha_of(a)]);
            }
            D3DTOP_ADD | D3DTOP_BLENDTEXTUREALPHAPM => self.a.op(Add, sat(dst), [a, b]),
            D3DTOP_ADDSIGNED | D3DTOP_ADDSIGNED2X => {
                let half = self.a.lit(-0.5);
                let scale = i8::from(u32::from(op) == D3DTOP_ADDSIGNED2X);
                self.a.op(Add, dst, [a, b]);
                self.a.op(Add, shift(sat(dst), scale), [partial, half]);
            }
            D3DTOP_SUBTRACT => {
                let b = self.plain(b);
                self.a.op(Add, sat(dst), [a, neg(b)]);
            }
            D3DTOP_ADDSMOOTH => {
                let a = self.plain(a);
                self.a.op(Mad, sat(dst), [b, comp(a), a]);
            }
            D3DTOP_BLENDDIFFUSEALPHA => self.blend(dst, a, b, s(DIFFUSE, "w")),
            D3DTOP_BLENDFACTORALPHA => self.blend(dst, a, b, s(c(0), "w")),
            D3DTOP_BLENDCURRENTALPHA => self.blend(dst, a, b, s(CURRENT, "w")),
            D3DTOP_DOTPRODUCT3 => {
                let (a, b) = (self.plain(a), self.plain(b));
                self.a.op(Dp3, shift(sat(dst), 2), [bias(a), bias(b)]);
            }
            // SELECTARG1, and every op without an arm (warned): the first argument.
            _ => self.mov(dst, a),
        }
    }

    /// `a * f + b * (1 - f)`, as `ff.rs` associates it.
    fn blend(&mut self, dst: DstOperand, a: SrcOperand, b: SrcOperand, factor: SrcOperand) {
        self.a.op(Mul, dst, [b, comp(factor)]);
        self.a.op(Mad, dst, [a, factor, s(dst.reg, "xyzw")]);
    }
}

const fn temp(index: u16) -> Register {
    reg(RegKind::Temp, index)
}

fn texel_of(stage: usize) -> Register {
    temp(TEXEL_BASE + u16::try_from(stage).unwrap_or(0))
}

/// `s` read through its own lane `w`, replicated: `(s).a` of an expression.
const fn alpha_of(src: SrcOperand) -> SrcOperand {
    SrcOperand {
        swizzle: Swizzle([src.swizzle.0[3]; 4]),
        ..src
    }
}

/// The stages before the first DISABLE colour op.
fn active_count(key: &FfPsKey) -> usize {
    key.stages
        .iter()
        .take_while(|st| u32::from(st.color_op) != D3DTOP_DISABLE)
        .count()
}

/// `ff.rs`'s `op_reads_argument`: SELECTARG1 ignores arg2, SELECTARG2 arg1, DISABLE both.
fn op_reads(op: u8, arg1: u8, arg2: u8, selector: u32) -> bool {
    let op = u32::from(op);
    let selected = |a: u8| u32::from(a) & D3DTA_SELECTMASK == selector;
    op != D3DTOP_DISABLE
        && ((selected(arg1) && op != D3DTOP_SELECTARG2)
            || (selected(arg2) && op != D3DTOP_SELECTARG1))
}

fn colour_fallback(st: FfStage) -> bool {
    !st.has_texture() && op_reads(st.color_op, st.color_arg1, st.color_arg2, D3DTA_TEXTURE)
}

fn alpha_fallback(st: FfStage) -> bool {
    !st.has_texture() && op_reads(st.alpha_op, st.alpha_arg1, st.alpha_arg2, D3DTA_TEXTURE)
}

fn is_bump(st: FfStage) -> bool {
    is_bump_env_op(u32::from(st.color_op)) && !colour_fallback(st)
}

fn is_luminance(st: FfStage) -> bool {
    u32::from(st.color_op) == D3DTOP_BUMPENVMAPLUMINANCE && is_bump(st)
}

/// `stage_expressions`'s alpha, in its order of tests.
fn alpha_source(st: FfStage) -> AlphaSource {
    if u32::from(st.color_op) == D3DTOP_DOTPRODUCT3 && !colour_fallback(st) {
        AlphaSource::FromColor
    } else if u32::from(st.alpha_op) == D3DTOP_DISABLE {
        AlphaSource::Kept
    } else if alpha_fallback(st) {
        AlphaSource::Current
    } else if is_bump_env_op(u32::from(st.alpha_op)) {
        AlphaSource::Kept
    } else {
        AlphaSource::Op
    }
}

/// The ops `apply_op` has an arm for (a bump op never reaches it).
fn handled_op(op: u8) -> bool {
    matches!(
        u32::from(op),
        D3DTOP_SELECTARG1..=D3DTOP_BLENDCURRENTALPHA
            | D3DTOP_MODULATEALPHA_ADDCOLOR..=D3DTOP_MODULATEINVCOLOR_ADDALPHA
            | D3DTOP_DOTPRODUCT3
    )
}

/// Which of `a`, `b` `apply_op`'s arm for `op` emits, and whether it reads the texel's alpha.
fn arm(op: u8, textured: bool) -> ([bool; 2], bool) {
    match u32::from(op) {
        D3DTOP_BLENDTEXTUREALPHA | D3DTOP_BLENDTEXTUREALPHAPM if textured => ([true, true], true),
        D3DTOP_SELECTARG2 | D3DTOP_BLENDTEXTUREALPHA => ([false, true], false),
        _ if handled_op(op) && u32::from(op) != D3DTOP_SELECTARG1 => ([true, true], false),
        _ => ([true, false], false),
    }
}

/// Whether `[op, arg1, arg2]`'s emitted expression names `selector`'s local (TEXTURE or TEMP).
fn op_names(op: u8, arg1: u8, arg2: u8, textured: bool, selector: u32) -> bool {
    let ([uses_a, uses_b], texel_alpha) = arm(op, textured);
    let names = |arg: u8| {
        u32::from(arg) & D3DTA_SELECTMASK == selector && (selector != D3DTA_TEXTURE || textured)
    };
    (uses_a && names(arg1)) || (uses_b && names(arg2)) || (selector == D3DTA_TEXTURE && texel_alpha)
}

/// Whether a stage with expressions names `selector`'s local in its colour or alpha.
fn stage_names(st: FfStage, selector: u32) -> bool {
    let textured = st.has_texture();
    (!colour_fallback(st)
        && op_names(
            st.color_op,
            st.color_arg1,
            st.color_arg2,
            textured,
            selector,
        ))
        || (alpha_source(st) == AlphaSource::Op
            && op_names(
                st.alpha_op,
                st.alpha_arg1,
                st.alpha_arg2,
                textured,
                selector,
            ))
}

/// `cascade_sampled_stages`: the walk from the last active stage back.
///
/// A bump stage samples when it has a texture and the next stage samples; a TEMP stage nothing
/// reads samples nothing; any other stage samples when its expressions name its texel.
fn sampled_stages(active: &[FfStage], reads_temp: bool) -> u16 {
    let mut mask = 0u16;
    for (i, st) in active.iter().enumerate().rev() {
        let samples = if is_bump(*st) {
            st.has_texture() && mask & (1 << (i + 1)) != 0
        } else if st.result() == FfStageResult::Temp && !reads_temp {
            false
        } else {
            stage_names(*st, D3DTA_TEXTURE)
        };
        if samples {
            mask |= 1 << i;
        }
    }
    mask
}

#[cfg(test)]
mod tests;
