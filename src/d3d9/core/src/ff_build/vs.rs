//! The fixed-function vertex stage as `vs_3_0`.
//!
//! `dxso::emit_vs`'s MSL in its order, one feature at a time, unrolled for the key: the
//! pre-transformed path, or the transform (`WV` then `PROJ`, two steps), vertex blending, the
//! point size, the clip basis, the eye normal, lighting, the texture coordinates and fog. What
//! the lowering's epilogue already does is left to it: the half-pixel shift, `fog_z`, the depth
//! bias, the point size clamp and the clip plane dots (against the `o31` basis written here).
//!
//! Conventions the MSL dictates, as SM3 does not: the lit colours are `_sat` (SM3 clamps none),
//! `oFog` is always written in all four lanes (no write would make it `color1.w`), a length is
//! `rcp(rsq(d))` (zero at zero), conditions are `cmp` selects (never a 0/1 multiply, which turns
//! an infinity into a NaN), and no `lit` (its power clamp is not the MSL's `pow`).

use airconv_ir::dxso::{
    DeclUsage, ShaderType,
    ir::{DstOperand, Register, SrcOperand},
    link::{Semantic, passthrough_entries},
    lower::VS_CLIP_BASIS_OUTPUT,
    opcode::Opcode::{
        Add, Cmp, Crs, Dp3, Dp4, Exp, Frc, Mad, Max, Min, Mov, MovA, Mul, Nrm, Pow, Rcp, Rsq, Sge,
    },
};

use super::{
    FfProgram,
    asm::{A0, Asm, POS_FIXUP, VS_DEF_BASE, abs, c, dst as d, neg, rel, sat, src as s, vs_draw},
};
use crate::dxso::{
    FF_PASSTHROUGH_ATTR_BASE, FF_VS_PALETTE_BASE_ROW, FfVsFlags, FfVsKey,
    MAX_VERTEX_BLEND_MATRIX_INDEX,
};

/// `ff.rs` warns: a passthru coordinate set the vertex stream does not carry reads zero.
pub const WARN_ABSENT_SET: &str = "dxso FF: passthru coord-set not in vertex stream → float4(0)";
/// `ff.rs` warns: a texgen mode on a pre-transformed stage falls back to passthru.
pub const WARN_RHW_TEXGEN: &str =
    "dxso FF: TCI mode 1-4 on an XYZRHW stage — eye-space undefined, falling back to passthru";
/// `ff.rs` warns: a TCI mode past SPHEREMAP is undefined and passes through.
pub const WARN_UNDEFINED_TCI: &str = "dxso FF: TCI mode above SPHEREMAP is undefined → passthru";
/// `ff.rs` warns: a material source past `D3DMCS_COLOR2` reads the material.
pub const WARN_MATERIAL_SOURCE: &str = "dxso FF: unknown material source → MCS_MATERIAL";

/// The first `n` lanes, and the lanes after them, for `n` in 1..=4.
const LANES: [&str; 4] = ["x", "xy", "xyz", "xyzw"];
const REST: [&str; 4] = ["yzw", "zw", "w", ""];
const COMPONENTS: [&str; 4] = ["x", "y", "z", "w"];

/// `-1 / ln(2)`: `exp(-x)` is `exp2(-1.442695 * x)`.
const NEG_LOG2_E: f32 = -core::f32::consts::LOG2_E;

/// Builds `key`'s vertex stage.
#[must_use]
pub fn vertex(key: &FfVsKey) -> FfProgram {
    let mut vs = Vs {
        a: Asm::new(ShaderType::Vertex, VS_DEF_BASE),
        key,
        warnings: Vec::new(),
    };
    let inputs = vs.inputs();
    if key.has_rhw() {
        vs.pre_transformed(&inputs);
    } else {
        vs.transformed(&inputs);
    }
    FfProgram {
        tokens: vs.a.finish().into(),
        provided_input_mask: vs.a.input_mask(),
        warnings: vs.warnings,
    }
}

/// The vertex inputs, declared as `emit_vertex_in` declares them.
struct Inputs {
    position: Register,
    normal: Option<Register>,
    color: [Option<Register>; 2],
    /// Coordinate set `i` at `v<4 + i>`, for `i` below `input_tex_coord_count`.
    texcoord: Vec<Register>,
    weights: Option<Register>,
    indices: Option<Register>,
    psize: Option<Register>,
    passthrough: Vec<(Semantic, Register)>,
}

struct Vs<'k> {
    a: Asm,
    key: &'k FfVsKey,
    warnings: Vec<&'static str>,
}

impl Vs<'_> {
    fn warn(&mut self, warning: &'static str) {
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }

    fn lit(&mut self, value: f32) -> SrcOperand {
        self.a.lit(value)
    }

    fn mov(&mut self, dst: DstOperand, src: SrcOperand) {
        self.a.op(Mov, dst, [src]);
    }

    fn inputs(&mut self) -> Inputs {
        let k = self.key;
        let rhw = k.has_rhw();
        let blending = k.vertex_blend_count > 0 && !rhw;
        let usage = if rhw {
            DeclUsage::PositionT
        } else {
            DeclUsage::Position
        };
        let position = self.a.input(usage, 0, 0);
        let normal = (k.has_normal() && !rhw).then(|| self.a.input(DeclUsage::Normal, 0, 1));
        let color = [
            k.has_color0().then(|| self.a.input(DeclUsage::Color, 0, 2)),
            k.has_color1().then(|| self.a.input(DeclUsage::Color, 1, 3)),
        ];
        let texcoord = (0..k.input_tex_coord_count.min(8))
            .map(|i| {
                self.a
                    .input(DeclUsage::Texcoord, u32::from(i), 4 + u16::from(i))
            })
            .collect();
        let weights = (blending && k.declared_weights_count > 0)
            .then(|| self.a.input(DeclUsage::BlendWeight, 0, 12));
        let indices = (blending && k.declared_indices())
            .then(|| self.a.input(DeclUsage::BlendIndices, 0, 13));
        let psize = k.has_psize().then(|| self.a.input(DeclUsage::PSize, 0, 14));
        let passthrough = passthrough_entries(&k.passthrough)
            .map(|(slot, semantic)| {
                let index = FF_PASSTHROUGH_ATTR_BASE + u16::try_from(slot).unwrap_or(0);
                let r = self
                    .a
                    .input(semantic.usage, u32::from(semantic.index), index);
                (semantic, r)
            })
            .collect();
        Inputs {
            position,
            normal,
            color,
            texcoord,
            weights,
            indices,
            psize,
            passthrough,
        }
    }

    /// Coordinate set `set` into `into`, the lanes past its dimension zeroed (`masked_input_rhs`).
    ///
    /// Returns the dimension, 0 for a set the stream does not carry, which reads zero.
    fn masked(&mut self, inputs: &Inputs, set: u8, into: Register) -> u8 {
        let n = if set < self.key.input_tex_coord_count {
            self.key
                .tex_coord_dims
                .get(usize::from(set))
                .copied()
                .unwrap_or(0)
        } else {
            0
        };
        let v = inputs.texcoord.get(usize::from(set)).copied();
        if let (1..=4, Some(v)) = (n, v) {
            let lanes = usize::from(n - 1);
            self.mov(d(into, LANES[lanes]), s(v, "xyzw"));
            if n < 4 {
                let zero = self.lit(0.0);
                self.mov(d(into, REST[lanes]), zero);
            }
        } else {
            self.warn(WARN_ABSENT_SET);
            let zero = self.lit(0.0);
            self.mov(d(into, "xyzw"), zero);
        }
        n
    }

    /// `ff.rs:1104-1226`: screen-space position, colours, coordinates, fog and passthrough.
    fn pre_transformed(&mut self, inputs: &Inputs) {
        let key = self.key;
        let v0 = inputs.position;
        let pos = self.a.output(DeclUsage::Position, 0, "xyzw");
        let inv_w = self.a.temp();
        let tiny = self.lit(1e-30);
        self.a.op(Max, d(inv_w, "x"), [s(v0, "w"), tiny]);
        self.a.op(Rcp, d(inv_w, "x"), [s(inv_w, "x")]);
        // `vs_c[0]` = (vp_w, vp_h, vp_x, vp_y), the logical viewport.
        let recip = self.a.temp();
        self.a.op(Rcp, d(recip, "x"), [s(c(0), "x")]);
        self.a.op(Rcp, d(recip, "y"), [s(c(0), "y")]);
        let ndc = self.a.temp();
        self.a
            .op(Add, d(ndc, "xy"), [s(v0, "xy"), neg(s(c(0), "zw"))]);
        self.a.op(Mul, d(ndc, "xy"), [s(ndc, "xy"), s(recip, "xy")]);
        let (two, minus_one) = (self.lit(2.0), self.lit(-1.0));
        let (minus_two, one) = (self.lit(-2.0), self.lit(1.0));
        self.a.op(Mad, d(ndc, "x"), [s(ndc, "x"), two, minus_one]);
        self.a.op(Add, d(ndc, "x"), [s(ndc, "x"), s(recip, "x")]);
        self.a.op(Mad, d(ndc, "y"), [s(ndc, "y"), minus_two, one]);
        self.a
            .op(Add, d(ndc, "y"), [s(ndc, "y"), neg(s(recip, "y"))]);
        // The half pixel is the logical viewport's: the epilogue adds `pos_fixup.xy * w`
        // (the target's) back, and reading `c2044` turns its depth clamp on.
        self.a.op(Mul, d(ndc, "xy"), [s(ndc, "xy"), s(inv_w, "x")]);
        self.a.op(
            Mad,
            d(pos, "xy"),
            [neg(s(POS_FIXUP, "xy")), s(inv_w, "x"), s(ndc, "xy")],
        );
        self.a.op(Mul, d(pos, "z"), [s(v0, "z"), s(inv_w, "x")]);
        self.mov(d(pos, "w"), s(inv_w, "x"));
        self.a.release(0);

        self.point_size(inputs, None);
        if let Some(v2) = inputs.color[0] {
            let out = self.a.output(DeclUsage::Color, 0, "xyzw");
            self.mov(d(out, "xyzw"), s(v2, "xyzw"));
        }
        if let Some(v3) = inputs.color[1] {
            let out = self.a.output(DeclUsage::Color, 1, "xyzw");
            self.mov(d(out, "xyzw"), s(v3, "xyzw"));
        }

        for stage in 0..key.tex_coord_count.min(8) {
            if matches!(key.tci_mode(usize::from(stage)), 1..=4) {
                self.warn(WARN_RHW_TEXGEN);
            }
            let out = self.a.output(DeclUsage::Texcoord, u32::from(stage), "xyzw");
            let dim = self.masked(inputs, key.tci_set(usize::from(stage)), out);
            if (1..4).contains(&dim) {
                let one = self.lit(1.0);
                self.mov(d(out, "w"), one);
            }
        }

        let fog = self.a.output(DeclUsage::Fog, 0, "xyzw");
        match inputs.color[1] {
            Some(v3) if key.fog_mode == 4 => self.fog_factor(fog, s(v3, "w")),
            _ => {
                let one = self.lit(1.0);
                self.mov(d(fog, "xyzw"), one);
            }
        }
        // Every other declaration element a `ps_3_0` reads by semantic; FOG0 replaces the fog
        // varying unless the specular alpha is the factor (fog mode 4).
        for (semantic, v) in &inputs.passthrough {
            let out = if (semantic.usage, semantic.index) == (DeclUsage::Fog, 0) {
                if key.fog_mode == 4 {
                    continue;
                }
                fog
            } else {
                self.a
                    .output(semantic.usage, u32::from(semantic.index), "xyzw")
            };
            self.mov(d(out, "xyzw"), s(*v, "xyzw"));
        }
    }

    /// `ff.rs:1228-1736`: transform, blending, lighting, texture coordinates and fog.
    fn transformed(&mut self, inputs: &Inputs) {
        let k = self.key;
        let needs_normal = reads_eye_normal(k);
        let blended = k.vertex_blend_count > 0;

        let pos_view = self.a.temp();
        let n = needs_normal.then(|| self.a.temp());
        let mark = self.a.mark();
        let pos = self.a.temp();
        self.mov(d(pos, "xyz"), s(inputs.position, "xyzw"));
        let one = self.lit(1.0);
        self.mov(d(pos, "w"), one);
        if blended {
            self.blend(inputs, pos, pos_view, n);
        } else {
            for (lane, row) in COMPONENTS.into_iter().zip(0..) {
                self.a
                    .op(Dp4, d(pos_view, lane), [s(pos, "xyzw"), s(c(row), "xyzw")]);
            }
        }
        self.a.release(mark);
        let position = self.a.output(DeclUsage::Position, 0, "xyzw");
        for (lane, row) in COMPONENTS.into_iter().zip(4..) {
            self.a.op(
                Dp4,
                d(position, lane),
                [s(pos_view, "xyzw"), s(c(row), "xyzw")],
            );
        }
        self.point_size(inputs, Some(pos_view));
        // World-space clip planes: the epilogue dots `o31` with each plane.
        if k.clip_plane_count > 0 {
            let basis = self
                .a
                .output_at(VS_CLIP_BASIS_OUTPUT, DeclUsage::Position, 0, "xyzw");
            for (lane, row) in COMPONENTS.into_iter().zip(2..) {
                self.a.op(
                    Dp4,
                    d(basis, lane),
                    [s(pos_view, "xyzw"), s(vs_draw(row), "xyzw")],
                );
            }
        }
        if let (Some(n), Some(normal)) = (n, inputs.normal) {
            if !blended {
                self.adjugate_normal(normal, n);
            }
            if k.normalize_normals() {
                self.a.op(Nrm, d(n, "xyz"), [s(n, "xyzw")]);
            }
        }

        if k.lighting_enabled() {
            self.lighting(inputs, pos_view, n);
        } else {
            if let Some(v2) = inputs.color[0] {
                let o = self.a.output(DeclUsage::Color, 0, "xyzw");
                self.mov(d(o, "xyzw"), s(v2, "xyzw"));
            }
            if let Some(v3) = inputs.color[1] {
                let o = self.a.output(DeclUsage::Color, 1, "xyzw");
                self.mov(d(o, "xyzw"), s(v3, "xyzw"));
            }
        }

        for stage in 0..k.tex_coord_count.min(8) {
            let mark = self.a.mark();
            self.texcoord(inputs, stage, pos_view, n);
            self.a.release(mark);
        }
        self.fog(inputs, pos_view);
    }

    /// `emit_vertex_blend`: `pos_view` and, when `n` is read, the blended normal into `n`.
    fn blend(&mut self, inputs: &Inputs, pos: Register, pos_view: Register, n: Option<Register>) {
        let k = self.key;
        let count = usize::from(k.vertex_blend_count.min(4));
        let indexed = k.vertex_blend_indexed();
        if indexed {
            // `uint(clamp(rint(idx), 0, 39)) * 4`: clamped, rounded half up (`floor(t + 0.5)`;
            // only a float index at .5 differs from `rint`'s half to even), times four, into
            // `a0`; bone `i` reads `c[a0.<i> + 95 + j]`.
            let lanes = LANES[count - 1];
            let indices = inputs
                .indices
                .map_or_else(|| self.lit(0.0), |r| s(r, "xyzw"));
            let t = self.a.temp();
            let f = self.a.temp();
            let (zero, last) = (
                self.lit(0.0),
                self.lit(f32::from(
                    u8::try_from(MAX_VERTEX_BLEND_MATRIX_INDEX).unwrap_or(39),
                )),
            );
            let (half, four) = (self.lit(0.5), self.lit(4.0));
            self.a.op(Max, d(t, lanes), [indices, zero]);
            self.a.op(Min, d(t, lanes), [s(t, "xyzw"), last]);
            self.a.op(Add, d(t, lanes), [s(t, "xyzw"), half]);
            self.a.op(Frc, d(f, lanes), [s(t, "xyzw")]);
            self.a
                .op(Add, d(t, lanes), [s(t, "xyzw"), neg(s(f, "xyzw"))]);
            self.a.op(Mul, d(t, lanes), [s(t, "xyzw"), four]);
            self.a.op(MovA, d(A0, lanes), [s(t, "xyzw")]);
        }
        let row = |bone: usize, j: u16| {
            let lane = ['x', 'y', 'z', 'w'][bone];
            if indexed {
                rel(s(c(FF_VS_PALETTE_BASE_ROW + j), "xyzw"), lane)
            } else {
                let base = FF_VS_PALETTE_BASE_ROW + 4 * u16::try_from(bone).unwrap_or(0);
                s(c(base + j), "xyzw")
            }
        };
        let normal = inputs.normal.filter(|_| n.is_some());
        let bone = |vs: &mut Self, i: usize, into: Register, normal_into: Option<Register>| {
            for (lane, j) in COMPONENTS.into_iter().zip(0..) {
                vs.a.op(Dp4, d(into, lane), [s(pos, "xyzw"), row(i, j)]);
            }
            if let (Some(nd), Some(v1)) = (normal_into, normal) {
                for (lane, j) in COMPONENTS.into_iter().take(3).zip(0..) {
                    vs.a.op(Dp3, d(nd, lane), [s(v1, "xyzw"), row(i, j)]);
                }
            }
        };
        let n = n.filter(|_| normal.is_some());
        if count == 1 && indexed {
            bone(self, 0, pos_view, n);
            return;
        }
        let explicit = count - 1;
        let t = self.a.temp();
        let tn = n.map(|_| self.a.temp());
        let sum = self.a.temp();
        for (i, lane) in COMPONENTS.into_iter().enumerate().take(count) {
            let w = if i < explicit {
                inputs.weights.map_or_else(|| self.lit(0.0), |r| s(r, lane))
            } else if explicit == 0 {
                self.lit(1.0)
            } else {
                // `1 - weight_sum`, the sum in lane order.
                let one = self.lit(1.0);
                self.a.op(Add, d(sum, "y"), [one, neg(s(sum, "x"))]);
                s(sum, "y")
            };
            if i < explicit {
                if i == 0 {
                    self.mov(d(sum, "x"), w);
                } else {
                    self.a.op(Add, d(sum, "x"), [s(sum, "x"), w]);
                }
            }
            bone(self, i, t, tn);
            if i == 0 {
                self.a.op(Mul, d(pos_view, "xyzw"), [s(t, "xyzw"), w]);
            } else {
                self.a.op(
                    Mad,
                    d(pos_view, "xyzw"),
                    [s(t, "xyzw"), w, s(pos_view, "xyzw")],
                );
            }
            if let (Some(n), Some(tn)) = (n, tn) {
                if i == 0 {
                    self.a.op(Mul, d(n, "xyz"), [s(tn, "xyzw"), w]);
                } else {
                    self.a
                        .op(Mad, d(n, "xyz"), [s(tn, "xyzw"), w, s(n, "xyzw")]);
                }
            }
        }
    }

    /// `out.point_size` before the epilogue's clamp; scaled by eye distance under `POINT_SCALE`.
    ///
    /// `pos_view` is `None` on the pre-transformed path, which never scales. Without a write
    /// the size is `D3DRS_POINTSIZE` (the lowering's default), as the MSL's unscaled one.
    fn point_size(&mut self, inputs: &Inputs, pos_view: Option<Register>) {
        let size = inputs.psize.map(|r| s(r, "x"));
        let Some(pos_view) = pos_view.filter(|_| self.key.point_scale()) else {
            if let Some(size) = size {
                let o = self.a.output(DeclUsage::PSize, 0, "x");
                self.mov(d(o, "x"), size);
            }
            return;
        };
        let size = size.unwrap_or_else(|| s(vs_draw(0), "x"));
        let o = self.a.output(DeclUsage::PSize, 0, "x");
        let mark = self.a.mark();
        let t = self.a.temp();
        // `de = length(pos_view.xyz)`.
        self.a
            .op(Dp3, d(t, "x"), [s(pos_view, "xyzw"), s(pos_view, "xyzw")]);
        self.a.op(Rsq, d(t, "x"), [s(t, "x")]);
        self.a.op(Rcp, d(t, "x"), [s(t, "x")]);
        // `A + B * de + C * de * de`, floored at zero, then `1 / sqrt`.
        let abc = vs_draw(1);
        self.a
            .op(Mad, d(t, "y"), [s(abc, "y"), s(t, "x"), s(abc, "x")]);
        self.a.op(Mul, d(t, "z"), [s(abc, "z"), s(t, "x")]);
        self.a.op(Mad, d(t, "y"), [s(t, "z"), s(t, "x"), s(t, "y")]);
        let zero = self.lit(0.0);
        self.a.op(Max, d(t, "y"), [s(t, "y"), zero]);
        self.a.op(Rsq, d(t, "y"), [s(t, "y")]);
        // `Vh` in logical pixels: `(-1 / pos_fixup.y) / pos_fixup.w`.
        self.a.op(Rcp, d(t, "z"), [s(POS_FIXUP, "y")]);
        self.a.op(Rcp, d(t, "w"), [s(POS_FIXUP, "w")]);
        self.a.op(Mul, d(t, "z"), [neg(s(t, "z")), s(t, "w")]);
        self.a.op(Mul, d(t, "z"), [s(t, "z"), s(t, "y")]);
        self.a.op(Mul, d(o, "x"), [size, s(t, "z")]);
        self.a.release(mark);
    }

    /// The D3D9 normal matrix, the 3x3 block of `inverse(WV)`, from the adjugate of `WV`.
    ///
    /// Row `i` of `adj(WV)` is the generalised cross product of the other three columns of `WV`
    /// (`vs_c[0..3]`); a determinant within 1e-12 of zero uses `WV`'s own 3x3 block instead.
    fn adjugate_normal(&mut self, normal: Register, n: Register) {
        let mark = self.a.mark();
        let adj = [self.a.temp(), self.a.temp(), self.a.temp()];
        let scratch = self.a.temp();
        self.cross4(adj[0], [1, 2, 3], true, 4, scratch);
        self.cross4(adj[1], [2, 3, 0], false, 3, scratch);
        self.cross4(adj[2], [3, 0, 1], true, 3, scratch);
        let det = self.a.temp();
        self.a
            .op(Dp4, d(det, "x"), [s(adj[0], "xyzw"), s(c(0), "xyzw")]);
        let v1 = s(normal, "xyzw");
        let solved = self.a.temp();
        for (lane, row) in COMPONENTS.into_iter().zip(adj) {
            self.a.op(Dp3, d(solved, lane), [s(row, "xyzw"), v1]);
        }
        self.a.op(Rcp, d(det, "y"), [s(det, "x")]);
        self.a
            .op(Mul, d(solved, "xyz"), [s(solved, "xyzw"), s(det, "y")]);
        let fallback = self.a.temp();
        self.a
            .op(Mul, d(fallback, "xyz"), [s(c(0), "xyzw"), s(normal, "x")]);
        self.a.op(
            Mad,
            d(fallback, "xyz"),
            [s(c(1), "xyzw"), s(normal, "y"), s(fallback, "xyzw")],
        );
        self.a.op(
            Mad,
            d(fallback, "xyz"),
            [s(c(2), "xyzw"), s(normal, "z"), s(fallback, "xyzw")],
        );
        // `abs(det) > 1e-12 ? solved : fallback`: `1e-12 - |det|` is negative exactly then.
        let threshold = self.lit(1e-12);
        self.a
            .op(Add, d(det, "z"), [neg(abs(s(det, "x"))), threshold]);
        self.a.op(
            Cmp,
            d(n, "xyz"),
            [s(det, "z"), s(fallback, "xyzw"), s(solved, "xyzw")],
        );
        self.a.release(mark);
    }

    /// `±mtld3d_cross4(c[u], c[v], c[w])`'s first `lanes` lanes into `into`.
    ///
    /// Lane `i` is `±det3` of the three columns without component `i`, signed `-, +, -, +`.
    fn cross4(
        &mut self,
        into: Register,
        [u, v, w]: [u16; 3],
        negated: bool,
        lanes: usize,
        scratch: Register,
    ) {
        const WITHOUT: [&str; 4] = ["yzw", "xzw", "xyw", "xyz"];
        for (i, (lane, others)) in COMPONENTS.into_iter().zip(WITHOUT).enumerate().take(lanes) {
            self.a
                .op(Crs, d(scratch, "xyz"), [s(c(v), others), s(c(w), others)]);
            let a = s(c(u), others);
            let minus = (i % 2 == 0) != negated;
            let a = if minus { neg(a) } else { a };
            self.a.op(Dp3, d(into, lane), [a, s(scratch, "xyzw")]);
        }
    }

    /// `resolve_mat`: the vertex colour a `D3DMCS_*` source names under `COLORVERTEX`, or the row.
    fn material(&mut self, inputs: &Inputs, source: u8, row: u16) -> Register {
        if self.key.color_vertex() {
            match (source, inputs.color) {
                (1, [Some(v2), _]) => return v2,
                (2, [_, Some(v3)]) => return v3,
                (3.., _) => self.warn(WARN_MATERIAL_SOURCE),
                _ => {}
            }
        }
        c(row)
    }

    /// `ff.rs:1354-1522`: Blinn-Phong over the active lights, into `oD0` and `oD1`.
    fn lighting(&mut self, inputs: &Inputs, pos_view: Register, n: Option<Register>) {
        let k = self.key;
        let n = n.filter(|_| k.has_normal());
        let specular = k.specular_enable();
        let lit_specular = n.is_some() && specular && k.light_active_mask != 0;
        let mat_diffuse = self.material(inputs, k.diffuse_source, 10);
        let mat_ambient = self.material(inputs, k.ambient_source, 11);
        let mat_specular = self.material(inputs, k.specular_source, 12);
        let mat_emissive = self.material(inputs, k.emissive_source, 13);
        let mark = self.a.mark();
        let diffuse = self.a.temp();
        self.a.op(
            Mad,
            d(diffuse, "xyzw"),
            [
                s(c(9), "xyzw"),
                s(mat_ambient, "xyzw"),
                s(mat_emissive, "xyzw"),
            ],
        );
        let spec = specular.then(|| self.a.temp());
        if let Some(spec) = spec {
            let zero = self.lit(0.0);
            self.mov(d(spec, "xyzw"), zero);
        }
        let view = if !lit_specular {
            None
        } else if k.local_viewer() {
            let v = self.a.temp();
            self.a.op(Nrm, d(v, "xyz"), [neg(s(pos_view, "xyzw"))]);
            Some(s(v, "xyzw"))
        } else {
            Some(self.a.vec4([0.0, 0.0, -1.0, 0.0]))
        };
        let mut active = k.light_active_mask;
        while active != 0 {
            let i = active.trailing_zeros();
            active &= active - 1;
            let light = Light {
                base: 15 + 6 * u16::try_from(i).unwrap_or(0),
                directional: k.light_directional_mask & (1 << i) != 0,
                spot: k.light_spot_mask & (1 << i) != 0,
            };
            let sums = Sums {
                diffuse,
                spec,
                view,
                mat_diffuse,
                mat_ambient,
            };
            let mark = self.a.mark();
            self.light(&light, pos_view, n, &sums);
            self.a.release(mark);
        }
        let o0 = self.a.output(DeclUsage::Color, 0, "xyzw");
        self.mov(sat(d(o0, "xyz")), s(diffuse, "xyzw"));
        self.mov(sat(d(o0, "w")), s(mat_diffuse, "w"));
        if let Some(spec) = spec {
            let o1 = self.a.output(DeclUsage::Color, 1, "xyzw");
            self.a.op(
                Mul,
                sat(d(o1, "xyzw")),
                [s(spec, "xyzw"), s(mat_specular, "xyzw")],
            );
        } else if let Some(v3) = inputs.color[1] {
            let o1 = self.a.output(DeclUsage::Color, 1, "xyzw");
            self.mov(d(o1, "xyzw"), s(v3, "xyzw"));
        }
        self.a.release(mark);
    }

    /// One light's terms into the diffuse and specular sums.
    fn light(&mut self, light: &Light, pos_view: Register, n: Option<Register>, sums: &Sums) {
        let Sums {
            diffuse,
            spec,
            view,
            mat_diffuse,
            mat_ambient,
        } = *sums;
        let base = light.base;
        let needs_light_vector = n.is_some() || (light.spot && !light.directional);
        let (to_light, atten) = if light.directional {
            (neg(s(c(base + 1), "xyzw")), None)
        } else {
            let tmp = self.a.temp();
            self.a.op(
                Add,
                d(tmp, "xyz"),
                [s(c(base), "xyzw"), neg(s(pos_view, "xyzw"))],
            );
            // `dist = length(toL)` in `tmp.w`.
            self.a
                .op(Dp3, d(tmp, "w"), [s(tmp, "xyzw"), s(tmp, "xyzw")]);
            self.a.op(Rsq, d(tmp, "w"), [s(tmp, "w")]);
            self.a.op(Rcp, d(tmp, "w"), [s(tmp, "w")]);
            let dir = self.a.temp();
            if needs_light_vector {
                let tiny = self.lit(1e-30);
                self.a.op(Max, d(dir, "w"), [s(tmp, "w"), tiny]);
                self.a.op(Rcp, d(dir, "w"), [s(dir, "w")]);
                self.a.op(Mul, d(dir, "xyz"), [s(tmp, "xyzw"), s(dir, "w")]);
            }
            // `1 / max(k.x + k.y * d + k.z * d * d, 1e-30) * step(d, k.w)` (`k`: the row).
            let atten_k = c(base + 4);
            let att = self.a.temp();
            self.a.op(
                Mad,
                d(att, "x"),
                [s(atten_k, "y"), s(tmp, "w"), s(atten_k, "x")],
            );
            self.a.op(Mul, d(att, "y"), [s(atten_k, "z"), s(tmp, "w")]);
            self.a
                .op(Mad, d(att, "x"), [s(att, "y"), s(tmp, "w"), s(att, "x")]);
            let tiny = self.lit(1e-30);
            self.a.op(Max, d(att, "x"), [s(att, "x"), tiny]);
            self.a.op(Rcp, d(att, "x"), [s(att, "x")]);
            self.a.op(Sge, d(att, "y"), [s(atten_k, "w"), s(tmp, "w")]);
            self.a.op(Mul, d(att, "x"), [s(att, "x"), s(att, "y")]);
            if light.spot {
                // `pow(saturate(rho * scale + offset), falloff)`, `rho = dot(-L, direction)`.
                self.a.op(
                    Dp3,
                    d(att, "y"),
                    [neg(s(dir, "xyzw")), s(c(base + 1), "xyzw")],
                );
                self.a.op(
                    Mad,
                    sat(d(att, "y")),
                    [s(att, "y"), s(c(base + 5), "w"), s(c(base + 3), "w")],
                );
                self.a
                    .op(Pow, d(att, "y"), [s(att, "y"), s(c(base + 1), "w")]);
                self.a.op(Mul, d(att, "x"), [s(att, "x"), s(att, "y")]);
            }
            (s(dir, "xyzw"), Some(s(att, "x")))
        };
        let tmp = self.a.temp();
        let ndotl = self.a.temp();
        if let Some(n) = n {
            self.a.op(Dp3, sat(d(ndotl, "x")), [s(n, "xyzw"), to_light]);
            self.a.op(
                Mul,
                d(tmp, "xyzw"),
                [s(c(base + 2), "xyzw"), s(mat_diffuse, "xyzw")],
            );
            let weight = match atten {
                Some(atten) => {
                    self.a.op(Mul, d(ndotl, "y"), [atten, s(ndotl, "x")]);
                    s(ndotl, "y")
                }
                None => s(ndotl, "x"),
            };
            self.a.op(
                Mad,
                d(diffuse, "xyzw"),
                [weight, s(tmp, "xyzw"), s(diffuse, "xyzw")],
            );
        }
        // The ambient term needs no normal.
        self.a.op(
            Mul,
            d(tmp, "xyzw"),
            [s(c(base + 3), "xyzw"), s(mat_ambient, "xyzw")],
        );
        match atten {
            Some(atten) => {
                self.a.op(
                    Mad,
                    d(diffuse, "xyzw"),
                    [atten, s(tmp, "xyzw"), s(diffuse, "xyzw")],
                );
            }
            None => self.a.op(
                Add,
                d(diffuse, "xyzw"),
                [s(diffuse, "xyzw"), s(tmp, "xyzw")],
            ),
        }
        let (Some(n), Some(spec), Some(view)) = (n, spec, view) else {
            return;
        };
        // `H = normalize(L + V)`, `ndoth = max(0, n.H)`, the power only where both dots are
        // positive (both are at least zero here, so where their minimum is not zero).
        let half_vec = self.a.temp();
        self.a.op(Add, d(half_vec, "xyz"), [to_light, view]);
        self.a.op(Nrm, d(half_vec, "xyz"), [s(half_vec, "xyzw")]);
        self.a
            .op(Dp3, d(half_vec, "w"), [s(n, "xyzw"), s(half_vec, "xyzw")]);
        let zero = self.lit(0.0);
        self.a.op(Max, d(half_vec, "w"), [s(half_vec, "w"), zero]);
        self.a
            .op(Pow, d(ndotl, "z"), [s(half_vec, "w"), s(c(14), "x")]);
        self.a
            .op(Min, d(ndotl, "w"), [s(ndotl, "x"), s(half_vec, "w")]);
        self.a.op(
            Cmp,
            d(ndotl, "z"),
            [neg(s(ndotl, "w")), zero, s(ndotl, "z")],
        );
        let factor = match atten {
            Some(atten) => {
                self.a.op(Mul, d(ndotl, "z"), [atten, s(ndotl, "z")]);
                s(ndotl, "z")
            }
            None => s(ndotl, "z"),
        };
        self.mov(d(tmp, "xyz"), s(c(base + 5), "xyzw"));
        self.mov(d(tmp, "w"), s(c(base + 2), "w"));
        self.a.op(
            Mad,
            d(spec, "xyzw"),
            [factor, s(tmp, "xyzw"), s(spec, "xyzw")],
        );
    }

    /// `ff.rs:1548-1667`: stage `stage`'s coordinate by its TCI mode, through its transform.
    fn texcoord(&mut self, inputs: &Inputs, stage: u8, pos_view: Register, n: Option<Register>) {
        let key = self.key;
        let index = usize::from(stage);
        let tt = key.tt_flags[index];
        let count = tt & 7;
        let projected = tt & 0x10 != 0;
        let raw = self.a.temp();
        let zero = self.lit(0.0);
        let dim = match key.tci_mode(index) {
            0 => self.masked(inputs, key.tci_set(index), raw),
            1 => {
                match n.filter(|_| key.has_normal()) {
                    Some(n) => {
                        self.mov(d(raw, "xyz"), s(n, "xyzw"));
                        self.mov(d(raw, "w"), zero);
                    }
                    None => self.mov(d(raw, "xyzw"), zero),
                }
                3
            }
            2 => {
                self.mov(d(raw, "xyz"), s(pos_view, "xyzw"));
                self.mov(d(raw, "w"), zero);
                3
            }
            mode @ 3..=4 => {
                // `R = reflect(normalize(posEye), n)`, `E` without a normal.
                let refl = self.a.temp();
                self.a.op(Nrm, d(refl, "xyz"), [s(pos_view, "xyzw")]);
                if let Some(n) = n.filter(|_| key.has_normal()) {
                    self.a
                        .op(Dp3, d(refl, "w"), [s(n, "xyzw"), s(refl, "xyzw")]);
                    self.a.op(Add, d(refl, "w"), [s(refl, "w"), s(refl, "w")]);
                    self.a.op(
                        Mad,
                        d(refl, "xyz"),
                        [neg(s(refl, "w")), s(n, "xyzw"), s(refl, "xyzw")],
                    );
                }
                if mode == 3 {
                    self.mov(d(raw, "xyz"), s(refl, "xyzw"));
                    self.mov(d(raw, "w"), zero);
                } else {
                    // `(R.xy / m + 0.5, 0, 0)`, `m = 2 * length(R + (0, 0, 1))`.
                    let sphere = self.a.temp();
                    let up = self.a.vec4([0.0, 0.0, 1.0, 0.0]);
                    self.a.op(Add, d(sphere, "xyz"), [s(refl, "xyzw"), up]);
                    self.a
                        .op(Dp3, d(sphere, "w"), [s(sphere, "xyzw"), s(sphere, "xyzw")]);
                    self.a.op(Rsq, d(sphere, "w"), [s(sphere, "w")]);
                    self.a.op(Rcp, d(sphere, "w"), [s(sphere, "w")]);
                    let (two, half) = (self.lit(2.0), self.lit(0.5));
                    self.a.op(Mul, d(sphere, "w"), [s(sphere, "w"), two]);
                    self.a.op(Rcp, d(sphere, "w"), [s(sphere, "w")]);
                    self.a
                        .op(Mad, d(raw, "xy"), [s(refl, "xyzw"), s(sphere, "w"), half]);
                    self.mov(d(raw, "zw"), zero);
                }
                3
            }
            _ => {
                self.warn(WARN_UNDEFINED_TCI);
                self.masked(inputs, key.tci_set(index), raw)
            }
        };
        let out = self.a.output(DeclUsage::Texcoord, u32::from(stage), "xyzw");
        if (2..=4).contains(&count) {
            // The first unbacked component is 1, then the stage's transposed matrix.
            if (1..4).contains(&dim) {
                let one = self.lit(1.0);
                self.mov(d(raw, COMPONENTS[usize::from(dim)]), one);
            }
            let base = 63 + 4 * u16::from(stage);
            if count == 4 {
                for (lane, j) in COMPONENTS.into_iter().zip(0..) {
                    self.a
                        .op(Dp4, d(out, lane), [s(raw, "xyzw"), s(c(base + j), "xyzw")]);
                }
                return;
            }
            let product = self.a.temp();
            for (lane, j) in COMPONENTS.into_iter().zip(0..count.into()) {
                self.a.op(
                    Dp4,
                    d(product, lane),
                    [s(raw, "xyzw"), s(c(base + j), "xyzw")],
                );
            }
            match (count, projected) {
                (2, false) => {
                    self.mov(d(out, "xy"), s(product, "xyzw"));
                    self.mov(d(out, "zw"), zero);
                }
                (2, true) => {
                    self.mov(d(out, "xyw"), s(product, "xyyy"));
                    self.mov(d(out, "z"), zero);
                }
                (_, false) => {
                    self.mov(d(out, "xyz"), s(product, "xyzw"));
                    self.mov(d(out, "w"), zero);
                }
                (_, true) => self.mov(d(out, "xyzw"), s(product, "xyzz")),
            }
        } else if projected && dim >= 1 {
            // The last provided component is the divisor.
            self.mov(d(out, "xyz"), s(raw, "xyzw"));
            let last = COMPONENTS[usize::from(dim.min(4) - 1)];
            self.mov(d(out, "w"), s(raw, last));
        } else {
            self.mov(d(out, "xyzw"), s(raw, "xyzw"));
        }
    }

    /// `ff.rs:1677-1729`: the vertex fog factor in `oFog.x`, its other lanes zero; 1 unfogged.
    fn fog(&mut self, inputs: &Inputs, pos_view: Register) {
        let k = self.key;
        let fog = self.a.output(DeclUsage::Fog, 0, "xyzw");
        let mark = self.a.mark();
        let t = self.a.temp();
        let z = if k.flags.contains(FfVsFlags::RANGE_FOG) && matches!(k.fog_mode, 1..=3) {
            self.a
                .op(Dp3, d(t, "w"), [s(pos_view, "xyzw"), s(pos_view, "xyzw")]);
            self.a.op(Rsq, d(t, "w"), [s(t, "w")]);
            self.a.op(Rcp, d(t, "w"), [s(t, "w")]);
            s(t, "w")
        } else {
            abs(s(pos_view, "z"))
        };
        let params = c(8);
        match (k.fog_mode, inputs.color[1]) {
            (1, _) => {
                let scale = self.lit(NEG_LOG2_E);
                self.a.op(Mul, d(t, "x"), [scale, s(params, "z")]);
                self.a.op(Mul, d(t, "x"), [s(t, "x"), z]);
                self.a.op(Exp, sat(d(fog, "x")), [s(t, "x")]);
                self.zero_fog_lanes(fog);
            }
            (2, _) => {
                let scale = self.lit(NEG_LOG2_E);
                self.a.op(Mul, d(t, "x"), [s(params, "z"), z]);
                self.a.op(Mul, d(t, "y"), [scale, s(t, "x")]);
                self.a.op(Mul, d(t, "y"), [s(t, "y"), s(t, "x")]);
                self.a.op(Exp, sat(d(fog, "x")), [s(t, "y")]);
                self.zero_fog_lanes(fog);
            }
            (3, _) => {
                // `range == 0 ? 0 : saturate((end - z) / range)`, the range signed.
                self.a
                    .op(Add, d(t, "x"), [s(params, "y"), neg(s(params, "x"))]);
                self.a.op(Add, d(t, "y"), [s(params, "y"), neg(z)]);
                self.a.op(Rcp, d(t, "z"), [s(t, "x")]);
                self.a.op(Mul, sat(d(t, "y")), [s(t, "y"), s(t, "z")]);
                let zero = self.lit(0.0);
                self.a
                    .op(Cmp, d(fog, "x"), [neg(abs(s(t, "x"))), zero, s(t, "y")]);
                self.zero_fog_lanes(fog);
            }
            (4, Some(v3)) => self.fog_factor(fog, s(v3, "w")),
            _ => {
                let one = self.lit(1.0);
                self.mov(d(fog, "xyzw"), one);
            }
        }
        self.a.release(mark);
    }

    /// `oFog = (factor, 0, 0, 0)`.
    fn fog_factor(&mut self, fog: Register, factor: SrcOperand) {
        self.mov(d(fog, "x"), factor);
        self.zero_fog_lanes(fog);
    }

    fn zero_fog_lanes(&mut self, fog: Register) {
        let zero = self.lit(0.0);
        self.mov(d(fog, "yzw"), zero);
    }
}

/// The lighting sums and what every light reads beside its rows.
struct Sums {
    diffuse: Register,
    spec: Option<Register>,
    /// The view vector, when a light adds specular.
    view: Option<SrcOperand>,
    mat_diffuse: Register,
    mat_ambient: Register,
}

/// One active light: its first constant row and its type.
struct Light {
    base: u16,
    directional: bool,
    spot: bool,
}

/// `ff.rs`'s `reads_eye_normal`: lighting with an active light, or a normal-reading texgen.
fn reads_eye_normal(k: &FfVsKey) -> bool {
    let active = usize::from(k.tex_coord_count).min(8);
    let texgen = (0..active).any(|stage| matches!(k.tci_mode(stage), 1 | 3 | 4));
    let lit = k.lighting_enabled() && k.light_active_mask != 0;
    k.has_normal() && !k.has_rhw() && (texgen || lit)
}
