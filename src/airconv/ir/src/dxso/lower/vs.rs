//! The vertex stage: its inputs, its outputs' mirrors, its prologue and its epilogue.
//!
//! `emit.rs`'s `emit_vs_function`: the stage-in attributes a declaration provides (an absent
//! one reads zero), the `Varyings` members in its order with its defaults, every write to an
//! output landing in the member's mirror (SM3 by `dcl` semantic, a register several semantics
//! share through a staging mirror whose lanes move at the end), then its epilogue in order:
//! the staged lanes, the SM1/SM2 colour clamp, the fog fallback, the clip distances, the
//! half-pixel fixup, `fog_z`, the depth bias and the point size clamp. The mirrors are then
//! written to the output registers the signature pops, the position as `PopOutput` into
//! `FunctionOutput::Position` (raw, as the MSL writes it, never `PopPosition`).

use std::collections::{BTreeMap, BTreeSet};

use super::{
    FOG_Z_VARYING, Lower, LowerError, Lowered, MAX_CLIP_PLANES, VS_DRAW_CLIP_ROW, VsBindings,
    range,
    regs::{
        MASK_ALL, MASK_X, MASK_Y, MASK_Z, Target, cbuf, dot, fbin, fcmp, fdst, fsplat, ftemp, ibin,
        idst, itemp, lane, mad, mov, mov_sat, movc, odst,
    },
    varying_name, vs_bindings,
};
use crate::{
    binding::{FunctionOutput, MslScalar, MslValue},
    cfg::Cfg,
    dxso::{
        TextureType,
        ir::{DeclUsage, Declaration, DxsoProgram, RegKind, ShaderType, WriteMask},
        link::{Element, Semantic, SemanticSet, elements, is_output_kind},
        spec::VsSpec,
    },
    handlers::{ScalarInfo, SignatureHandler, SignatureState},
    ir::{FloatBinaryOp, FloatComparison, IntegerBinaryOp, OperandDataType, Target as Exit},
    shader_info::{Declarations, ShaderInfo},
};

/// The output registers the epilogue writes, in the order the signature pops them.
const OUT_POSITION: u32 = 0;
const OUT_POSITION1: u32 = 1;
const OUT_TEXCOORD: u32 = 2;
const OUT_COLOR: u32 = 18;
const OUT_FOG: u32 = 20;
const OUT_FOG_Z: u32 = 21;
const OUT_POINT_SIZE: u32 = 22;
/// Clip distances 0-3 and 4-5.
const OUT_CLIP: u32 = 23;
const OUT_EXTRA: u32 = 25;
/// Texture coordinate members (`emit_varyings`'s sixteen).
const TEXCOORDS: u8 = 16;

/// A float operand reading `VsDraw` row 0 (`point`: size, min, max).
const fn vs_draw(row: u32) -> crate::ir::SrcOperand {
    cbuf(range::VS_DRAW, row, OperandDataType::Float)
}

const fn pos_fixup(row: u32) -> crate::ir::SrcOperand {
    cbuf(range::POS_FIXUP, row, OperandDataType::Float)
}

/// The vertex stage's registers and mirrors.
pub struct VsIo {
    /// Input registers the declaration provides and the program declares.
    pub inputs: BTreeSet<u16>,
    pub provided_mask: u16,
    pub samplers: BTreeMap<u16, TextureType>,
    pub lod_table: bool,
    pub position: u32,
    pub position1: u32,
    pub texcoord: Vec<u32>,
    pub color: Vec<u32>,
    pub fog: u32,
    pub point_size: u32,
    /// The SM3 extra semantics, each with its mirror.
    pub extras: Vec<(Semantic, u32)>,
    /// SM3: each output register's target; `None` when SM1/SM2 (targets by register kind).
    pub targets: Option<BTreeMap<(RegKind, u16), u32>>,
    /// SM3 registers several semantics share: the staging mirror and its elements.
    pub staged: Vec<(u32, Vec<Element>)>,
}

impl VsIo {
    pub fn empty(bindings: VsBindings, provided_mask: u16) -> Self {
        Self {
            inputs: BTreeSet::new(),
            provided_mask,
            samplers: bindings.samplers,
            lod_table: bindings.lod_table,
            position: 0,
            position1: 0,
            texcoord: Vec::new(),
            color: Vec::new(),
            fog: 0,
            point_size: 0,
            extras: Vec::new(),
            targets: None,
            staged: Vec::new(),
        }
    }

    /// Whether input `vN` is provided by the declaration (`vs_input_provided`).
    const fn provided(&self, index: u16) -> bool {
        index >= 16 || self.provided_mask & (1 << index) != 0
    }

    /// The member mirror a semantic travels in (`VsOutputs`' `direct_target`).
    fn member(&self, semantic: Semantic) -> Option<u32> {
        match (semantic.usage, semantic.index) {
            (DeclUsage::Position, 0) => Some(self.position),
            (DeclUsage::Position, 1) => Some(self.position1),
            (DeclUsage::Texcoord, i) => self.texcoord.get(usize::from(i)).copied(),
            (DeclUsage::Color, i @ (0 | 1)) => self.color.get(usize::from(i)).copied(),
            (DeclUsage::Fog, 0) => Some(self.fog),
            (DeclUsage::PSize, 0) => Some(self.point_size),
            _ => self
                .extras
                .iter()
                .find(|(s, _)| *s == semantic)
                .map(|(_, t)| *t),
        }
    }
}

/// The lane `mask`'s first lane covers, 0 for x.
fn first_lane(mask: WriteMask) -> u8 {
    (0..4).find(|c| mask.covers(*c)).unwrap_or(0)
}

/// Lowers a vertex shader (`emit_vs_programmable_named`).
pub fn lower(program: &DxsoProgram, spec: VsSpec) -> Result<Lowered, LowerError> {
    if program.shader_type != ShaderType::Vertex {
        return Err(LowerError::Unsupported {
            opcode: 0,
            what: "a pixel shader lowered as a vertex shader",
        });
    }
    let bindings = vs_bindings(program, &spec);
    let mut l = Lower::new(
        program,
        ShaderType::Vertex,
        VsIo::empty(bindings, spec.provided_input_mask),
        super::ps::PsIo::none(),
    );
    l.check_definitions()?;
    l.plan_vertex();
    l.vertex_prologue()?;
    l.body(&program.instructions)?;
    l.release();
    l.opcode = 0;
    if !l.frames.is_empty() {
        return Err(l.refuse("a block left open at the end of the program"));
    }
    let clip_planes = spec.clip_plane_count.min(MAX_CLIP_PLANES);
    l.vertex_epilogue(clip_planes)?;
    l.end_block(Exit::Return)?;
    let io = l.vertex_signature(clip_planes);
    Ok(Lowered {
        cfg: Cfg {
            blocks: l.blocks,
            info: ShaderInfo {
                temp_register_count: l.next_temp,
                ..ShaderInfo::default()
            },
            decls: Declarations::default(),
        },
        io,
        outputs: SemanticSet::vs_outputs(program),
    })
}

impl Lower<'_> {
    /// The inputs, the member mirrors and, for SM3, each output register's target.
    fn plan_vertex(&mut self) {
        let program = self.program;
        for decl in &program.declarations {
            if let Declaration::Semantic { reg, .. } = decl
                && reg.kind == RegKind::Input
                && self.vs.provided(reg.index)
            {
                self.vs.inputs.insert(reg.index);
            }
        }
        self.vs.position = self.perm();
        self.vs.position1 = self.perm();
        self.vs.texcoord = (0..TEXCOORDS).map(|_| self.perm()).collect();
        self.vs.color = (0..2).map(|_| self.perm()).collect();
        self.vs.fog = self.perm();
        self.vs.point_size = self.perm();
        if program.major != 3 {
            return;
        }
        self.vs.extras = SemanticSet::vs_outputs(program)
            .iter()
            .map(|s| (s, self.perm()))
            .collect();
        let mut by_reg: BTreeMap<(RegKind, u16), Vec<Element>> = BTreeMap::new();
        for element in elements(program).filter(|e| is_output_kind(e.reg.0)) {
            by_reg.entry(element.reg).or_default().push(element);
        }
        let mut targets = BTreeMap::new();
        for (reg, group) in by_reg {
            let direct = match group.as_slice() {
                [only] => {
                    let scalar = matches!(
                        (only.semantic.usage, only.semantic.index),
                        (DeclUsage::PSize | DeclUsage::Fog, 0)
                    );
                    if scalar && first_lane(only.mask) != 0 {
                        None
                    } else {
                        self.vs.member(only.semantic)
                    }
                }
                _ => None,
            };
            let target = direct.unwrap_or_else(|| {
                let local = self.perm();
                self.vs.staged.push((local, group));
                local
            });
            targets.insert(reg, target);
        }
        self.vs.targets = Some(targets);
    }

    /// The registers zeroed and the members given their defaults (the MSL's locals).
    fn vertex_prologue(&mut self) -> Result<(), LowerError> {
        self.zero_registers()?;
        let zero = [self.vs.position, self.vs.position1];
        let colors = self.vs.color.clone();
        for t in zero.into_iter().chain(self.vs.texcoord.clone()) {
            self.emit(mov(fdst(t, MASK_ALL), fsplat(0.0)))?;
        }
        for (t, v) in colors.into_iter().zip([1.0, 0.0]) {
            self.emit(mov(fdst(t, MASK_ALL), fsplat(v)))?;
        }
        self.emit(mov(fdst(self.vs.fog, MASK_ALL), fsplat(1.0)))?;
        // `D3DRS_POINTSIZE`, the size of a point whose shader never writes one.
        self.emit(mov(fdst(self.vs.point_size, MASK_ALL), lane(vs_draw(0), 0)))?;
        let extras: Vec<u32> = self.vs.extras.iter().map(|(_, t)| *t).collect();
        for t in extras {
            self.emit(mov(fdst(t, MASK_ALL), fsplat(0.0)))?;
        }
        // A staging mirror's lanes start at their semantics' defaults (`vertex_default`).
        let staged: Vec<(u32, [Option<Semantic>; 4])> = self
            .vs
            .staged
            .iter()
            .map(|(t, group)| {
                let lane_of = |c: u8| group.iter().find(|e| e.mask.covers(c)).map(|e| e.semantic);
                (*t, [lane_of(0), lane_of(1), lane_of(2), lane_of(3)])
            })
            .collect();
        for (t, lanes) in staged {
            for (c, semantic) in (0u8..).zip(lanes) {
                let d = fdst(t, 1 << c);
                let value = match semantic.map(|s| (s.usage, s.index)) {
                    Some((DeclUsage::Color | DeclUsage::Fog, 0)) => fsplat(1.0),
                    Some((DeclUsage::PSize, 0)) => lane(vs_draw(0), 0),
                    _ => fsplat(0.0),
                };
                self.emit(mov(d, value))?;
            }
        }
        Ok(())
    }

    /// Where a vertex shader's write to `kind`/`index` lands (`register_write_target`).
    pub fn vs_target(&self, kind: RegKind, index: u16) -> Result<Target, LowerError> {
        if let Some(targets) = &self.vs.targets
            && is_output_kind(kind)
        {
            return Ok(targets
                .get(&(kind, index))
                .map_or(Target::Discard, |t| Target::Temp(*t)));
        }
        match kind {
            RegKind::RastOut => Ok(match index {
                0 => Target::Temp(self.vs.position),
                1 => Target::Temp(self.vs.fog),
                2 => Target::Temp(self.vs.point_size),
                _ => Target::Discard,
            }),
            RegKind::AttrOut => self
                .vs
                .color
                .get(usize::from(index))
                .map(|t| Target::Temp(*t))
                .ok_or_else(|| self.refuse("a colour output above oD1")),
            RegKind::TexcoordOut => self
                .vs
                .texcoord
                .get(usize::from(index))
                .map(|t| Target::Temp(*t))
                .ok_or_else(|| self.refuse("a texture coordinate output above oT15")),
            _ => Err(self.refuse("a register a vertex shader cannot write")),
        }
    }

    /// Input `vN` (`in.vN`, or zero for one the declaration omits).
    pub fn read_input(&self, index: u16) -> Result<crate::ir::SrcOperand, LowerError> {
        if !self.vs.provided(index) {
            return Ok(fsplat(0.0));
        }
        if !self.vs.inputs.contains(&index) {
            return Err(self.refuse("an input register with no dcl"));
        }
        Ok(crate::ir::SrcOperand::Input {
            common: crate::ir::SrcOperandCommon {
                swizzle: crate::ir::Swizzle([0, 1, 2, 3]),
                abs: false,
                neg: false,
                read_type: OperandDataType::Float,
            },
            regid: u32::from(index),
        })
    }

    /// Whether the program writes the fog output anywhere (`vs_writes_fog`).
    fn writes_fog(&self) -> bool {
        let program = self.program;
        // SM3: the last `dcl_fog0`'s register and its first lane.
        let fog = elements(program)
            .filter(|e| is_output_kind(e.reg.0))
            .filter(|e| (e.semantic.usage, e.semantic.index) == (DeclUsage::Fog, 0))
            .map(|e| (e.reg, 1u8 << first_lane(e.mask)))
            .last();
        program
            .all_instructions()
            .filter_map(|i| i.dst.as_ref())
            .any(|d| {
                if program.major == 3 {
                    fog.is_some_and(|(reg, lane)| {
                        reg == (d.reg.kind, d.reg.index) && d.write_mask.0 & lane != 0
                    })
                } else {
                    d.reg.kind == RegKind::RastOut && d.reg.index == 1
                }
            })
    }

    /// `emit_vs_function`'s epilogue, then each mirror to its output register.
    fn vertex_epilogue(&mut self, clip_planes: u8) -> Result<(), LowerError> {
        let pos = self.vs.position;
        // SM3: each staged semantic's lanes into its member.
        let staged = core::mem::take(&mut self.vs.staged);
        for (local, group) in &staged {
            for e in group {
                let c = first_lane(e.mask);
                match (e.semantic.usage, e.semantic.index) {
                    (DeclUsage::PSize, 0) => {
                        self.emit(mov(
                            fdst(self.vs.point_size, MASK_X),
                            lane(ftemp(*local), c),
                        ))?;
                    }
                    (DeclUsage::Fog, 0) => {
                        self.emit(mov(fdst(self.vs.fog, MASK_ALL), lane(ftemp(*local), c)))?;
                    }
                    _ => {
                        if let Some(member) = self.vs.member(e.semantic) {
                            self.emit(mov(fdst(member, u32::from(e.mask.0)), ftemp(*local)))?;
                        }
                    }
                }
            }
        }
        self.vs.staged = staged;
        // SM1/SM2 clamp the colours; SM3 does not.
        if self.program.major < 3 {
            let colors = self.vs.color.clone();
            for t in colors {
                self.emit(mov_sat(fdst(t, MASK_ALL), ftemp(t), true))?;
            }
        }
        // No static fog write: the fog factor is the specular alpha.
        if !self.writes_fog()
            && let Some(specular) = self.vs.color.get(1).copied()
        {
            self.emit(mov(fdst(self.vs.fog, MASK_ALL), lane(ftemp(specular), 3)))?;
        }
        // User clip planes against the application's own position, before the fixup.
        for i in 0..clip_planes {
            let reg = OUT_CLIP + u32::from(i / 4);
            self.emit(dot(
                4,
                odst(reg, 1 << (i % 4)),
                ftemp(pos),
                vs_draw(VS_DRAW_CLIP_ROW + u32::from(i)),
            ))?;
        }
        // The half-pixel shift, scaled by `.w`.
        for (mask, c) in [(MASK_X, 0), (MASK_Y, 1)] {
            self.emit(mad(
                fdst(pos, mask),
                lane(pos_fixup(0), c),
                lane(ftemp(pos), 3),
                lane(ftemp(pos), c),
            ))?;
        }
        // `fog_z = z / w`, unbiased.
        let fog_z = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Div,
            fdst(fog_z, MASK_X),
            lane(ftemp(pos), 2),
            lane(ftemp(pos), 3),
        ))?;
        self.depth_bias(pos)?;
        // `clamp(size, min, max) * render scale`.
        let size = self.scratch();
        let point = vs_draw(0);
        self.emit(fbin(
            FloatBinaryOp::Max,
            fdst(size, MASK_X),
            lane(ftemp(self.vs.point_size), 0),
            lane(point.clone(), 1),
        ))?;
        self.emit(fbin(
            FloatBinaryOp::Min,
            fdst(size, MASK_X),
            lane(ftemp(size), 0),
            lane(point, 2),
        ))?;
        self.emit(fbin(
            FloatBinaryOp::Mul,
            fdst(size, MASK_X),
            lane(ftemp(size), 0),
            lane(pos_fixup(0), 3),
        ))?;
        // The mirrors to the output registers.
        let mut moves = vec![(OUT_POSITION, pos), (OUT_POSITION1, self.vs.position1)];
        moves.extend((OUT_TEXCOORD..).zip(self.vs.texcoord.iter().copied()));
        moves.extend((OUT_COLOR..).zip(self.vs.color.iter().copied()));
        moves.push((OUT_FOG, self.vs.fog));
        moves.extend((OUT_EXTRA..).zip(self.vs.extras.iter().map(|(_, t)| *t)));
        for (out, t) in moves {
            self.emit(mov(odst(out, MASK_ALL), ftemp(t)))?;
        }
        self.emit(mov(odst(OUT_FOG_Z, MASK_X), lane(ftemp(fog_z), 0)))?;
        self.emit(mov(odst(OUT_POINT_SIZE, MASK_X), lane(ftemp(size), 0)))?;
        Ok(())
    }

    /// `D3DRS_DEPTHBIAS`: `z + bias * w`, clamped to `[0, w]` when `z` was inside it.
    fn depth_bias(&mut self, pos: u32) -> Result<(), LowerError> {
        let (z, w) = (lane(ftemp(pos), 2), lane(ftemp(pos), 3));
        let biased = self.scratch();
        self.emit(mad(
            fdst(biased, MASK_X),
            lane(pos_fixup(1), 0),
            w.clone(),
            z.clone(),
        ))?;
        let clamped = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Max,
            fdst(clamped, MASK_X),
            lane(ftemp(biased), 0),
            fsplat(0.0),
        ))?;
        self.emit(fbin(
            FloatBinaryOp::Min,
            fdst(clamped, MASK_X),
            lane(ftemp(clamped), 0),
            w.clone(),
        ))?;
        let inside = self.scratch();
        let upper = self.scratch();
        self.emit(fcmp(
            FloatComparison::GreaterEqual,
            idst(inside, MASK_X),
            z.clone(),
            fsplat(0.0),
        ))?;
        self.emit(fcmp(
            FloatComparison::GreaterEqual,
            idst(upper, MASK_X),
            w,
            z,
        ))?;
        self.emit(ibin(
            IntegerBinaryOp::And,
            idst(inside, MASK_X),
            lane(itemp(inside), 0),
            lane(itemp(upper), 0),
        ))?;
        self.emit(movc(
            fdst(pos, MASK_Z),
            lane(itemp(inside), 0),
            lane(ftemp(clamped), 0),
            lane(ftemp(biased), 0),
        ))
    }

    /// The function signature: stage-in attributes, the outputs in `Varyings`' order.
    fn vertex_signature(&self, clip_planes: u8) -> SignatureState {
        let mut s = SignatureState::default();
        for reg in &self.vs.inputs {
            s.signature_handlers.push(SignatureHandler::VertexInput {
                reg: u32::from(*reg),
                mask: MASK_ALL,
                component_type: 3,
                name: format!("v{reg}"),
            });
        }
        s.max_input_register = self.vs.inputs.last().map_or(0, |r| u32::from(*r) + 1);
        let pop = |s: &mut SignatureState, reg: u32, mask: u32, output: FunctionOutput| {
            let index = s.func_signature.define_output(output);
            s.signature_handlers
                .push(SignatureHandler::PopOutput { reg, mask, index });
        };
        let varying = |usage: DeclUsage, index: u32| FunctionOutput::Vertex {
            user: varying_name(Semantic::new(usage, index)),
            ty: MslValue::FLOAT4,
        };
        pop(
            &mut s,
            OUT_POSITION,
            MASK_ALL,
            FunctionOutput::Position {
                ty: MslValue::FLOAT4,
            },
        );
        pop(
            &mut s,
            OUT_POSITION1,
            MASK_ALL,
            varying(DeclUsage::Position, 1),
        );
        for (i, reg) in (0..u32::from(TEXCOORDS)).zip(OUT_TEXCOORD..) {
            pop(&mut s, reg, MASK_ALL, varying(DeclUsage::Texcoord, i));
        }
        for (i, reg) in (0..2).zip(OUT_COLOR..) {
            pop(&mut s, reg, MASK_ALL, varying(DeclUsage::Color, i));
        }
        pop(&mut s, OUT_FOG, MASK_ALL, varying(DeclUsage::Fog, 0));
        pop(
            &mut s,
            OUT_FOG_Z,
            MASK_X,
            FunctionOutput::Vertex {
                user: FOG_Z_VARYING.to_owned(),
                ty: MslValue::Scalar(MslScalar::Float),
            },
        );
        pop(&mut s, OUT_POINT_SIZE, MASK_X, FunctionOutput::PointSize);
        for ((semantic, _), reg) in self.vs.extras.iter().zip(OUT_EXTRA..) {
            let output = FunctionOutput::Vertex {
                user: varying_name(*semantic),
                ty: MslValue::FLOAT4,
            };
            pop(&mut s, reg, MASK_ALL, output);
        }
        for i in 0..clip_planes {
            s.clip_distance_scalars.push(ScalarInfo {
                component: i % 4,
                reg: u8::try_from(OUT_CLIP).unwrap_or(0) + i / 4,
            });
        }
        s.max_output_register = OUT_EXTRA + u32::try_from(self.vs.extras.len()).unwrap_or(0);
        s
    }
}
