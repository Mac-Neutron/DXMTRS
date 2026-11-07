//! The pixel stage, shader models 2 and 3: its inputs, its outputs' mirrors and its epilogue.
//!
//! `emit.rs`'s `emit_ps_function`: the `Varyings` members arrive as fragment inputs named by
//! semantic (`d3d9_<code>`, colours `[[flat]]` under `FLAT_SHADE`, `fog_z` centre without
//! perspective), the position raw (`fix_w: false`: the MSL reads `in.position` as Metal gives
//! it), the point coordinate standing in for texture coordinates 0-7 under `POINT_SPRITE`, the
//! front-facing flag for a declared `vFace`. `ps_2_0` reads `vN` as colour N and `tN` as texture
//! coordinate N; `ps_3_0` each input by its `dcl` semantic, a register several semantics share
//! assembled lane by lane, an extra semantic only when the bound vertex shader outputs it
//! (`linked_input_mask`). Writes land in mirrors of `oC#` and `oDepth`; the epilogue then runs
//! the alpha test, the fog blend and the sRGB encode on them, in that order, and the mirrors go
//! to the output registers the signature pops: each exported colour, the depth unless the pass
//! has no depth attachment, and the key's sample mask under `SAMPLE_MASK`.

use std::collections::BTreeMap;

use super::{
    FOG_Z_VARYING, Lower, LowerError, Lowered, VsBindings, ps_bindings, range,
    regs::{
        A0, MASK_ALL, MASK_W, MASK_X, MASK_Y, MASK_Z, P0, cbuf, fbin, fcmp, fdst, fimm, fsplat,
        ftemp, fun, ibin, idst, iimm, isplat, itemp, lane, mad, mov, mov_sat, movc, negate, odst,
        swizzle,
    },
    slot_bit, varying_name,
    vs::VsIo,
};
use crate::{
    binding::{DepthArgument, FunctionInput, FunctionOutput, Interpolation, MslScalar, MslValue},
    cfg::Cfg,
    dxso::{
        TextureType,
        ir::{DeclUsage, Declaration, DxsoProgram, InstrFlags, Instruction, RegKind, ShaderType},
        link::{Element, LinkInputs, Semantic, SemanticSet, elements},
        opcode::Opcode,
        spec::{VariantFlags, VariantKey},
    },
    handlers::{SignatureHandler, SignatureState},
    ir::{
        Condition, DstOperand as Dst, DstOperandCommon, FloatBinaryOp, FloatComparison,
        FloatUnaryOp, InstGather, InstPartialDerivative, InstSample, InstSampleBias,
        InstSampleCompare, InstSampleCompareGrad, InstSampleCompareLevel, InstSampleDerivative,
        InstSampleLod, InstTextureSize, Instruction as Ir, InstructionCommon, IntegerBinaryOp,
        NO_PHASE, OperandDataType, OperandIndex, SrcOperand as Src, SrcOperandCommon,
        SrcOperandResource, SrcOperandSampler, Swizzle, Target as Exit,
    },
    shader_info::{Declarations, ShaderInfo},
};

/// The input registers the signature fills, in the `Varyings` members' order.
const IN_POSITION: u32 = 0;
const IN_POSITION1: u32 = 1;
const IN_TEXCOORD: u32 = 2;
const IN_COLOR: u32 = 18;
const IN_FOG: u32 = 20;
const IN_FOG_Z: u32 = 21;
const IN_FACE: u32 = 22;
const IN_POINT_COORD: u32 = 23;
const IN_EXTRA: u32 = 24;
/// The output registers: colours 0-3 at their index, then the depth and the sample mask.
const OUT_DEPTH: u32 = 4;
const OUT_SAMPLE_MASK: u32 = 5;
/// Texture coordinate members, and those a point sprite replaces.
const TEXCOORDS: u32 = 16;
const SPRITE_TEXCOORDS: u32 = 8;
/// The render targets a D3D9 pass exports.
const RENDER_TARGETS: u32 = 4;
/// `D3DCMP_NEVER`: the alpha test discards every fragment.
const ALPHA_NEVER: u8 = 1;

/// What an input register reads.
enum Read {
    /// A fragment input, by its input register.
    Varying(u32),
    /// A semantic no vertex output supplies.
    Zero,
    /// A register several semantics share, assembled in the prologue.
    Local(u32),
}

/// The pixel stage's registers and mirrors.
pub struct PsIo {
    pub key: VariantKey,
    /// The declared sampler slots, typed by the bound texture (a depth slot by its key bit).
    pub samplers: BTreeMap<u16, TextureType>,
    /// `ps_3_0`: each input register's read.
    reads: BTreeMap<u16, Read>,
    /// The extra semantics declared, those the bound vertex shader outputs.
    extras: Vec<Semantic>,
    /// Under `POINT_SPRITE`: texture coordinates 0-7, the point coordinate.
    sprite: Vec<u32>,
    /// `oC0`.. as the program writes them (`emit.rs`'s locals).
    colors: Vec<u32>,
    /// `_depth_storage`, when the program writes `oDepth`.
    depth: Option<u32>,
    /// `vPos` declared, and read scaled.
    vpos: bool,
    vpos_scaled: bool,
    /// `vFace` declared.
    vface: bool,
}

impl PsIo {
    /// The pixel state of a vertex shader's lowering: none.
    pub fn none() -> Self {
        Self {
            key: VariantKey::default(),
            samplers: BTreeMap::new(),
            reads: BTreeMap::new(),
            extras: Vec::new(),
            sprite: Vec::new(),
            colors: Vec::new(),
            depth: None,
            vpos: false,
            vpos_scaled: false,
            vface: false,
        }
    }
}

/// The refusal of the stage as a whole, before any instruction.
const fn refused(what: &'static str) -> LowerError {
    LowerError::Unsupported { opcode: 0, what }
}

/// Lowers a pixel shader (`emit_ps_programmable_named`).
pub fn lower(program: &DxsoProgram, key: &VariantKey) -> Result<Lowered, LowerError> {
    if program.shader_type != ShaderType::Pixel {
        return Err(refused("a vertex shader lowered as a pixel shader"));
    }
    if program.major < 2 {
        return Err(refused("ps_1_x (lowered with ps_1_x, not yet)"));
    }
    let bindings = ps_bindings(program, key);
    // `emit.rs` shifts a 16-bit mask by the slot; D3D9 has sixteen.
    if bindings.samplers.keys().any(|slot| *slot >= 16) {
        return Err(LowerError::Unsupported {
            opcode: super::opcode_value(Opcode::Dcl),
            what: "a sampler register above s15",
        });
    }
    let mut ps = PsIo::none();
    ps.key = *key;
    ps.samplers = bindings
        .samplers
        .iter()
        .map(|(slot, (ty, _))| (*slot, *ty))
        .collect();
    let vs = VsIo::empty(
        VsBindings {
            int_consts: false,
            bool_consts: false,
            samplers: BTreeMap::new(),
            lod_table: false,
        },
        0,
    );
    let mut l = Lower::new(program, ShaderType::Pixel, vs, ps);
    l.check_definitions()?;
    l.plan_pixel();
    l.pixel_prologue()?;
    l.body(&program.instructions)?;
    l.release();
    l.opcode = 0;
    if !l.frames.is_empty() {
        return Err(l.refuse("a block left open at the end of the program"));
    }
    l.pixel_epilogue()?;
    l.end_block(Exit::Return)?;
    let io = l.pixel_signature();
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
        outputs: SemanticSet::default(),
    })
}

/// Input register `reg` read as floats.
const fn input(reg: u32) -> Src {
    Src::Input {
        common: SrcOperandCommon {
            swizzle: Swizzle([0, 1, 2, 3]),
            abs: false,
            neg: false,
            read_type: OperandDataType::Float,
        },
        regid: reg,
    }
}

/// Output register `reg`'s `mask` lanes, written as integers.
const fn odst_int(reg: u32, mask: u32) -> Dst {
    Dst::Output {
        common: DstOperandCommon {
            mask,
            write_type: OperandDataType::Integer,
        },
        regid: reg,
        phase: NO_PHASE,
    }
}

const fn ic() -> InstructionCommon {
    InstructionCommon {
        saturate: false,
        precise_mask: 0,
    }
}

/// The `float4` row `row` of pixel range `range`.
const fn row(range: u32, row: u32) -> Src {
    cbuf(range, row, OperandDataType::Float)
}

impl Lower<'_> {
    /// The inputs each register reads, the mirrors, the point sprite's coordinates.
    fn plan_pixel(&mut self) {
        let program = self.program;
        let key = self.ps.key;
        self.ps.vpos = program.reads_vpos();
        self.ps.vpos_scaled = self.ps.vpos && key.flags.contains(VariantFlags::VPOS_SCALE);
        self.ps.vface = program.declarations.iter().any(|d| {
            matches!(d, Declaration::Semantic { reg, .. }
                if reg.kind == RegKind::MiscType && reg.index == 1)
        });
        // `oC0` always, and up to the highest one the program writes.
        let written = program.color_out_mask().0;
        let locals = 8 - written.leading_zeros().min(7);
        self.ps.colors = (0..locals).map(|_| self.perm()).collect();
        let writes_depth = program.all_instructions().any(|i| {
            matches!(i.opcode, Opcode::TexDepth | Opcode::TexM3x2Depth)
                || i.dst.is_some_and(|d| d.reg.kind == RegKind::DepthOut)
        });
        if writes_depth {
            self.ps.depth = Some(self.perm());
        }
        if key.flags.contains(VariantFlags::POINT_SPRITE) {
            self.ps.sprite = (0..SPRITE_TEXCOORDS).map(|_| self.perm()).collect();
        }
        if program.major != 3 {
            return;
        }
        let link = LinkInputs::ps_inputs(program).0;
        self.ps.extras = link
            .iter()
            .enumerate()
            .filter(|(i, _)| key.linked_input_mask & (1 << i) != 0)
            .map(|(_, s)| s)
            .collect();
        let mut by_reg: BTreeMap<u16, Vec<Element>> = BTreeMap::new();
        for element in elements(program).filter(|e| e.reg.0 == RegKind::Input) {
            by_reg.entry(element.reg.1).or_default().push(element);
        }
        for (index, group) in by_reg {
            let read = match group.as_slice() {
                [only] => self
                    .semantic_input(only.semantic)
                    .map_or(Read::Zero, Read::Varying),
                _ => Read::Local(self.perm()),
            };
            self.ps.reads.insert(index, read);
        }
    }

    /// The input register a semantic arrives in, `None` for one that reads zero (`PsInputs`).
    fn semantic_input(&self, semantic: Semantic) -> Option<u32> {
        match (semantic.usage, semantic.index) {
            // `dcl_position0 vN` reads the rasterizer position.
            (DeclUsage::Position, 0) => Some(IN_POSITION),
            (DeclUsage::PSize, 0) => None,
            (DeclUsage::Position, 1) => Some(IN_POSITION1),
            (DeclUsage::Texcoord, i) => Some(IN_TEXCOORD + u32::from(i)),
            (DeclUsage::Color, i @ (0 | 1)) => Some(IN_COLOR + u32::from(i)),
            (DeclUsage::Fog, 0) => Some(IN_FOG),
            _ => self
                .ps
                .extras
                .iter()
                .position(|s| *s == semantic)
                .and_then(|i| u32::try_from(i).ok())
                .map(|i| IN_EXTRA + i),
        }
    }

    /// A fragment input as the body reads it: texture coordinates 0-7 replaced under a sprite.
    fn varying(&self, reg: u32) -> Src {
        let sprite = reg
            .checked_sub(IN_TEXCOORD)
            .and_then(|i| usize::try_from(i).ok())
            .and_then(|i| self.ps.sprite.get(i));
        sprite.map_or_else(|| input(reg), |t| ftemp(*t))
    }

    /// The registers zeroed, the sprite's coordinates and the shared inputs assembled.
    fn pixel_prologue(&mut self) -> Result<(), LowerError> {
        // `in.texcoordN = float4(point_coord.x, point_coord.y, 0.0, 0.0)`.
        let sprite = self.ps.sprite.clone();
        for t in sprite {
            self.emit(mov(
                fdst(t, MASK_X | MASK_Y),
                swizzle(input(IN_POINT_COORD), [0, 1, 0, 1]),
            ))?;
            self.emit(mov(fdst(t, MASK_Z | MASK_W), fsplat(0.0)))?;
        }
        // A register several semantics share, lane by lane from the semantic covering it.
        let program = self.program;
        let locals: Vec<(u16, u32)> = self
            .ps
            .reads
            .iter()
            .filter_map(|(i, r)| match r {
                Read::Local(t) => Some((*i, *t)),
                _ => None,
            })
            .collect();
        for (index, t) in locals {
            let group: Vec<Element> = elements(program)
                .filter(|e| e.reg == (RegKind::Input, index))
                .collect();
            for c in 0..4u8 {
                let element = group
                    .iter()
                    .find(|e| e.mask.covers(c))
                    .or_else(|| group.first());
                let value = element
                    .and_then(|e| self.semantic_input(e.semantic))
                    .map_or_else(|| fsplat(0.0), |reg| lane(self.varying(reg), c));
                self.emit(mov(fdst(t, 1 << c), value))?;
            }
        }
        self.zero_registers()?;
        let mirrors: Vec<u32> = self
            .ps
            .colors
            .iter()
            .copied()
            .chain(self.ps.depth)
            .collect();
        for t in mirrors {
            self.emit(mov(fdst(t, MASK_ALL), fsplat(0.0)))?;
        }
        Ok(())
    }

    /// Every `r#` the program names, `a0` and `p0` zeroed (the MSL's `= {}`, `int4(0)`, `false`).
    pub fn zero_registers(&mut self) -> Result<(), LowerError> {
        let used: std::collections::BTreeSet<u16> = self
            .program
            .all_instructions()
            .flat_map(|i| {
                i.dst
                    .iter()
                    .map(|d| d.reg)
                    .chain(i.srcs.iter().map(|s| s.reg))
            })
            .filter(|r| r.kind == RegKind::Temp && r.index < super::regs::TEMPS)
            .map(|r| r.index)
            .collect();
        for r in used {
            self.emit(mov(fdst(u32::from(r), MASK_ALL), fsplat(0.0)))?;
        }
        self.emit(mov(idst(A0, MASK_ALL), isplat(0)))?;
        self.emit(mov(idst(P0, MASK_ALL), isplat(0)))
    }

    /// A pixel register read (`register_read_expr`'s pixel arms).
    ///
    /// # Errors
    ///
    /// A register the MSL names but does not declare.
    pub fn ps_read(&mut self, kind: RegKind, index: u16) -> Result<Src, LowerError> {
        Ok(match kind {
            RegKind::Input => match self.ps.reads.get(&index) {
                Some(Read::Varying(reg)) => self.varying(*reg),
                Some(Read::Zero) => fsplat(0.0),
                Some(Read::Local(t)) => ftemp(*t),
                // `ps_2_0`'s `vN`, and a `ps_3_0` register without a `dcl`: `in.colorN`.
                None if index < 2 => self.varying(IN_COLOR + u32::from(index)),
                None => return Err(self.refuse("an input register past v1 with no dcl")),
            },
            // `tN`: `in.texcoordN`.
            RegKind::Addr if u32::from(index) < TEXCOORDS => {
                self.varying(IN_TEXCOORD + u32::from(index))
            }
            RegKind::Addr => return Err(self.refuse("a texture register above t15")),
            RegKind::ColorOut => match self.ps.colors.get(usize::from(index)) {
                Some(t) => ftemp(*t),
                None => return Err(self.refuse("a colour output the program never writes")),
            },
            RegKind::MiscType => self.misc(index)?,
            _ => return Err(self.refuse("a register the stage cannot read")),
        })
    }

    /// `vPos`, `vFace`, or zero for either without its `dcl`.
    fn misc(&mut self, index: u16) -> Result<Src, LowerError> {
        let position = input(IN_POSITION);
        Ok(match index {
            // `float4(floor(in.position.xy * ps_draw.vpos_scale.xy), in.position.zw)`.
            0 if self.ps.vpos_scaled => {
                let s = self.scratch();
                self.emit(fbin(
                    FloatBinaryOp::Mul,
                    fdst(s, MASK_X | MASK_Y),
                    position.clone(),
                    row(range::PS_DRAW, 0),
                ))?;
                self.emit(fun(
                    FloatUnaryOp::RoundNegativeInf,
                    fdst(s, MASK_X | MASK_Y),
                    ftemp(s),
                ))?;
                self.emit(mov(fdst(s, MASK_Z | MASK_W), position))?;
                ftemp(s)
            }
            // `in.position - 0.5`: the pixel's corner, every lane shifted as the MSL shifts it.
            0 if self.ps.vpos => {
                let s = self.scratch();
                self.emit(fbin(
                    FloatBinaryOp::Add,
                    fdst(s, MASK_ALL),
                    position,
                    fsplat(-0.5),
                ))?;
                ftemp(s)
            }
            // `v_face_in ? 1.0 : -1.0`.
            1 if self.ps.vface => {
                let s = self.scratch();
                let face = Src::Input {
                    common: SrcOperandCommon {
                        swizzle: Swizzle([0; 4]),
                        abs: false,
                        neg: false,
                        read_type: OperandDataType::Integer,
                    },
                    regid: IN_FACE,
                };
                self.emit(movc(fdst(s, MASK_ALL), face, fsplat(1.0), fsplat(-1.0)))?;
                ftemp(s)
            }
            _ => fsplat(0.0),
        })
    }

    /// Where a pixel shader's write lands: an `oC#` or `oDepth` mirror.
    ///
    /// # Errors
    ///
    /// A register the MSL has no local for.
    pub fn ps_target(&self, kind: RegKind, index: u16) -> Result<super::regs::Target, LowerError> {
        let target = match kind {
            RegKind::ColorOut => self.ps.colors.get(usize::from(index)).copied(),
            RegKind::DepthOut => self.ps.depth,
            _ => None,
        };
        target
            .map(super::regs::Target::Temp)
            .ok_or_else(|| self.refuse("a register a pixel shader cannot write"))
    }

    /// Ends the current block in a discard when `cond` holds, and goes on after it.
    fn discard_if(&mut self, cond: Condition) -> Result<(), LowerError> {
        let kill = self.block("discard_fulfilled")?;
        let rest = self.block("discard_otherwise")?;
        self.end_block(Exit::ConditionalBranch {
            cond,
            true_branch: kill,
            false_branch: rest,
        })?;
        self.current = kill;
        self.emit(Ir::PixelDiscard)?;
        self.end_block(Exit::UnconditionalBranch(rest))?;
        self.current = rest;
        Ok(())
    }

    /// `texkill`: `if (any(r.<mask> < 0.0)) discard_fragment();`, a predicate ignored.
    ///
    /// # Errors
    ///
    /// A `texkill` without its operand, or one reading a register the stage cannot read.
    pub fn texkill(&mut self, inst: &Instruction) -> Result<(), LowerError> {
        let Some(dst) = inst.dst else {
            return Err(self.refuse("texkill without its operand"));
        };
        let value = self.src(&crate::dxso::ir::SrcOperand {
            reg: dst.reg,
            swizzle: crate::dxso::ir::Swizzle::IDENTITY,
            modifier: crate::dxso::ir::SrcModifier::None,
            rel_addr: None,
        })?;
        let lanes: Vec<u8> = (0..4).filter(|c| dst.write_mask.covers(*c)).collect();
        let Some((first, rest)) = lanes.split_first() else {
            return Ok(());
        };
        let below = self.mask(FloatComparison::LessThan, value, fsplat(0.0))?;
        let any = self.scratch();
        self.emit(mov(idst(any, MASK_X), lane(below.clone(), *first)))?;
        for c in rest {
            self.emit(ibin(
                IntegerBinaryOp::Or,
                idst(any, MASK_X),
                lane(itemp(any), 0),
                lane(below.clone(), *c),
            ))?;
        }
        self.discard_if(Condition {
            operand: lane(itemp(any), 0),
            test_nonzero: true,
        })
    }

    /// `dsx`/`dsy`: `dfdx(s)`, `dfdy(s)`.
    ///
    /// # Errors
    ///
    /// An instruction too short for its source.
    pub fn derivative(&mut self, src: Src, ddy: bool) -> Result<Src, LowerError> {
        let s = self.scratch();
        self.emit(Ir::PartialDerivative(InstPartialDerivative {
            common: ic(),
            dst: fdst(s, MASK_ALL),
            src,
            ddy,
            coarse: false,
        }))?;
        Ok(ftemp(s))
    }

    /// The pixel LOD table's lane `c` for sampler slot `slot`.
    fn lod_row(slot: u16, c: u8) -> Src {
        lane(row(range::LOD_BIAS, u32::from(slot)), c)
    }

    /// A pixel `texldl`'s level: `coord.w`, or `max(coord.w + lod.z, lod.w)` under the table.
    pub fn pixel_level(&mut self, slot: u16, coord: Src) -> Result<Src, LowerError> {
        if !self.ps.key.flags.contains(VariantFlags::LOD_BIAS) {
            return Ok(lane(coord, 3));
        }
        let l = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Add,
            fdst(l, MASK_X),
            lane(coord, 3),
            Self::lod_row(slot, 2),
        ))?;
        self.emit(fbin(
            FloatBinaryOp::Max,
            fdst(l, MASK_X),
            lane(ftemp(l), 0),
            Self::lod_row(slot, 3),
        ))?;
        Ok(lane(ftemp(l), 0))
    }

    /// The level a depth sample pins when it names none: `max(lod.w, 0)` under the table, else 0.
    fn pinned_level(&mut self, slot: u16) -> Result<Option<Src>, LowerError> {
        if !self.ps.key.flags.contains(VariantFlags::LOD_BIAS) {
            return Ok(None);
        }
        let l = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Max,
            fdst(l, MASK_X),
            Self::lod_row(slot, 3),
            fsplat(0.0),
        ))?;
        Ok(Some(lane(ftemp(l), 0)))
    }

    /// `texldd`'s gradients, each scaled by `lod.y` under the table.
    fn gradients(&mut self, slot: u16, dx: Src, dy: Src) -> Result<(Src, Src), LowerError> {
        if !self.ps.key.flags.contains(VariantFlags::LOD_BIAS) {
            return Ok((dx, dy));
        }
        let scale = Self::lod_row(slot, 1);
        let x = self.binary(FloatBinaryOp::Mul, dx, scale.clone())?;
        let y = self.binary(FloatBinaryOp::Mul, dy, scale)?;
        Ok((x, y))
    }

    /// A pixel sample: `sample_or_compare`, its result swizzled by the sampler operand.
    ///
    /// Fetch4 gathers (ignoring every level control), a readable raw depth slot samples the
    /// stored depth at a pinned level, a depth slot compares against the saturated `.z`, and a
    /// colour slot samples with the instruction's and the table's bias.
    ///
    /// # Errors
    ///
    /// A sample the MSL's overloads do not compile for the slot's bound kind.
    pub fn ps_texture(
        &mut self,
        inst: &Instruction,
        srcs: &[Option<Src>],
        slot: u16,
        result_swizzle: [u8; 4],
    ) -> Result<Src, LowerError> {
        let key = self.ps.key;
        let kind = self
            .ps
            .samplers
            .get(&slot)
            .copied()
            .unwrap_or(TextureType::Texture2D);
        let depth = slot_bit(key.depth_sampler_mask, slot);
        let raw = slot_bit(key.depth_fetch_mask, slot);
        let plain_2d = depth || kind == TextureType::Texture2D;
        let coord = self.arg(srcs, 0)?;
        // `texldp`: the coordinate over its `.w`; `texldb`: the unprojected `.w` as a bias.
        let address =
            if inst.opcode == Opcode::TexLd && inst.flags.contains(InstrFlags::TEX_PROJECTED) {
                self.binary(FloatBinaryOp::Div, coord.clone(), lane(coord.clone(), 3))?
            } else {
                coord.clone()
            };
        let instruction_bias = (inst.opcode == Opcode::TexLd
            && inst.flags.contains(InstrFlags::TEX_BIASED))
        .then(|| lane(coord.clone(), 3));
        let src_resource = SrcOperandResource {
            range_id: u32::from(slot),
            index: OperandIndex::Immediate(0),
            read_swizzle: Swizzle([0, 1, 2, 3]),
        };
        let src_sampler = |gather_channel| SrcOperandSampler {
            range_id: u32::from(slot),
            index: OperandIndex::Immediate(0),
            gather_channel,
        };
        let t = self.scratch();
        let dst = fdst(t, MASK_ALL);
        let gradients = if inst.opcode == Opcode::TexLdD {
            Some((self.arg(srcs, 2)?, self.arg(srcs, 3)?))
        } else {
            None
        };
        let value = if slot_bit(key.fetch4_mask, slot) {
            // `s.gather(samp, uv + 0.5 / float2(width, height)[, int2(0), component::w]).zxyw`,
            // every level control ignored.
            if !plain_2d {
                return Err(self.refuse("fetch4 on a cube or volume texture (no float2 gather)"));
            }
            let size = self.scratch();
            self.emit(Ir::TextureSize(InstTextureSize {
                dst: fdst(size, MASK_X | MASK_Y),
                src_resource: src_resource.clone(),
            }))?;
            let half = self.scratch();
            self.emit(fbin(
                FloatBinaryOp::Div,
                fdst(half, MASK_X | MASK_Y),
                fsplat(0.5),
                ftemp(size),
            ))?;
            let uv = self.scratch();
            self.emit(fbin(
                FloatBinaryOp::Add,
                fdst(uv, MASK_X | MASK_Y),
                address,
                ftemp(half),
            ))?;
            let alpha = !depth && slot_bit(key.fetch4_alpha_mask, slot);
            self.emit(Ir::Gather(InstGather {
                dst,
                src_address: ftemp(uv),
                src_resource,
                src_sampler: src_sampler(if alpha { 3 } else { 0 }),
                offset: iimm([0; 4]),
                feedback: None,
            }))?;
            swizzle(ftemp(t), [2, 0, 1, 3])
        } else if raw {
            // `float4(s.sample(samp, coord.xy, <level>)[, 0, 0, 1])`.
            let fill = slot_bit(key.raw_depth_red_mask, slot);
            if !plain_2d || (!depth && fill) {
                return Err(self.refuse("a raw depth read the MSL does not compile for the slot"));
            }
            if let Some((dx, dy)) = gradients {
                if kind != TextureType::Texture2D {
                    return Err(self.refuse("a gradient overload the depth texture lacks"));
                }
                let (dx, dy) = self.gradients(slot, dx, dy)?;
                self.emit(Ir::SampleDerivative(InstSampleDerivative {
                    dst,
                    src_address: address,
                    src_resource,
                    src_sampler: src_sampler(0),
                    src_x_derivative: dx,
                    src_y_derivative: dy,
                    offsets: [0; 3],
                    min_lod_clamp: None,
                    feedback: None,
                }))?;
            } else {
                let src_lod = if inst.opcode == Opcode::TexLdL {
                    self.pixel_level(slot, coord)?
                } else {
                    self.pinned_level(slot)?.unwrap_or_else(|| fsplat(0.0))
                };
                self.emit(Ir::SampleLod(InstSampleLod {
                    dst,
                    src_address: address,
                    src_resource,
                    src_sampler: src_sampler(0),
                    src_lod,
                    offsets: [0; 3],
                    feedback: None,
                }))?;
            }
            if !depth {
                ftemp(t)
            } else if fill {
                let s = self.scratch();
                self.emit(mov(fdst(s, MASK_X), lane(ftemp(t), 0)))?;
                self.emit(mov(
                    fdst(s, MASK_Y | MASK_Z | MASK_W),
                    fimm([0.0, 0.0, 0.0, 1.0]),
                ))?;
                ftemp(s)
            } else {
                lane(ftemp(t), 0)
            }
        } else if depth {
            // `float4(s.sample_compare(samp, coord.xy, saturate(coord.z), <level>))`.
            let r = self.scratch();
            self.emit(mov_sat(fdst(r, MASK_ALL), address.clone(), true))?;
            let src_reference = lane(ftemp(r), 2);
            if let Some((dx, dy)) = gradients {
                if kind != TextureType::Texture2D {
                    return Err(self.refuse("a gradient overload the depth texture lacks"));
                }
                let (dx, dy) = self.gradients(slot, dx, dy)?;
                self.emit(Ir::SampleCompareGrad(InstSampleCompareGrad {
                    dst,
                    src_address: address,
                    src_resource,
                    src_sampler: src_sampler(0),
                    src_reference,
                    src_x_derivative: dx,
                    src_y_derivative: dy,
                    offsets: [0; 3],
                }))?;
            } else {
                let level = if inst.opcode == Opcode::TexLdL {
                    Some(self.pixel_level(slot, coord)?)
                } else {
                    self.pinned_level(slot)?
                };
                let inst = match level {
                    Some(src_lod) => Ir::SampleCompareLevel(InstSampleCompareLevel {
                        dst,
                        src_address: address,
                        src_resource,
                        src_sampler: src_sampler(0),
                        src_reference,
                        src_lod,
                        offsets: [0; 3],
                    }),
                    None => Ir::SampleCompare(InstSampleCompare {
                        dst,
                        src_address: address,
                        src_resource,
                        src_sampler: src_sampler(0),
                        src_reference,
                        offsets: [0; 3],
                        min_lod_clamp: None,
                        feedback: None,
                        level_zero: true,
                    }),
                };
                self.emit(inst)?;
            }
            lane(ftemp(t), 0)
        } else {
            let lod_bias = key.flags.contains(VariantFlags::LOD_BIAS);
            let inst = match (inst.opcode, gradients) {
                (Opcode::TexLdL, _) => Ir::SampleLod(InstSampleLod {
                    dst,
                    src_lod: self.pixel_level(slot, coord)?,
                    src_address: address,
                    src_resource,
                    src_sampler: src_sampler(0),
                    offsets: [0; 3],
                    feedback: None,
                }),
                (_, Some((dx, dy))) => {
                    let (dx, dy) = self.gradients(slot, dx, dy)?;
                    Ir::SampleDerivative(InstSampleDerivative {
                        dst,
                        src_address: address,
                        src_resource,
                        src_sampler: src_sampler(0),
                        src_x_derivative: dx,
                        src_y_derivative: dy,
                        offsets: [0; 3],
                        min_lod_clamp: None,
                        feedback: None,
                    })
                }
                _ => {
                    // `bias(w + lod.x)`, `bias(w)`, `bias(lod.x)` or none.
                    let src_bias = match (instruction_bias, lod_bias) {
                        (Some(w), true) => {
                            let b = self.scratch();
                            self.emit(fbin(
                                FloatBinaryOp::Add,
                                fdst(b, MASK_X),
                                w,
                                Self::lod_row(slot, 0),
                            ))?;
                            Some(lane(ftemp(b), 0))
                        }
                        (Some(w), false) => Some(w),
                        (None, true) => Some(Self::lod_row(slot, 0)),
                        (None, false) => None,
                    };
                    match src_bias {
                        Some(src_bias) => Ir::SampleBias(InstSampleBias {
                            dst,
                            src_address: address,
                            src_resource,
                            src_sampler: src_sampler(0),
                            src_bias,
                            offsets: [0; 3],
                            min_lod_clamp: None,
                            feedback: None,
                        }),
                        None => Ir::Sample(InstSample {
                            dst,
                            src_address: address,
                            src_resource,
                            src_sampler: src_sampler(0),
                            offsets: [0; 3],
                            min_lod_clamp: None,
                            feedback: None,
                        }),
                    }
                }
            };
            self.emit(inst)?;
            ftemp(t)
        };
        Ok(swizzle(value, result_swizzle))
    }

    /// `emit_ps_function`'s tail: the alpha test, the fog blend, the sRGB encode, the outputs.
    fn pixel_epilogue(&mut self) -> Result<(), LowerError> {
        let Some(c0) = self.ps.colors.first().copied() else {
            return Err(self.refuse("a pixel shader without oC0"));
        };
        self.alpha_test(c0)?;
        self.fog(c0)?;
        let exported = self.exported();
        if self.ps.key.flags.contains(VariantFlags::SRGB_WRITE) {
            for i in exported.clone() {
                if let Some(t) = self.ps.colors.get(i).copied() {
                    self.srgb(t)?;
                }
            }
        }
        for i in exported {
            if let (Some(t), Ok(reg)) = (self.ps.colors.get(i).copied(), u32::try_from(i)) {
                self.emit(mov(odst(reg, MASK_ALL), ftemp(t)))?;
            }
        }
        if let Some(t) = self.ps.depth
            && self.exports_depth()
        {
            self.emit(mov(odst(OUT_DEPTH, MASK_X), lane(ftemp(t), 0)))?;
        }
        if self.ps.key.flags.contains(VariantFlags::SAMPLE_MASK) {
            let mask = u32::from(self.ps.key.sample_mask);
            self.emit(mov(odst_int(OUT_SAMPLE_MASK, MASK_X), isplat(mask)))?;
        }
        Ok(())
    }

    /// The colour targets exported: written and attached, target 0 always.
    fn exported(&self) -> Vec<usize> {
        let written = self.program.color_out_mask().0;
        let exported = (written & self.ps.key.color_out_mask) | 1;
        (0..RENDER_TARGETS)
            .filter(|i| exported & (1 << i) != 0)
            .filter_map(|i| usize::try_from(i).ok())
            .collect()
    }

    /// Whether `oDepth` reaches the depth output (`NO_DEPTH_ATTACHMENT` keeps it a local).
    const fn exports_depth(&self) -> bool {
        self.ps.depth.is_some()
            && !self
                .ps
                .key
                .flags
                .contains(VariantFlags::NO_DEPTH_ATTACHMENT)
    }

    /// `if (!(oC0.a <cmp> alpha_ref)) discard_fragment();` (`emit_alpha_test`).
    fn alpha_test(&mut self, c0: u32) -> Result<(), LowerError> {
        let func = self.ps.key.alpha_func;
        if func == ALPHA_NEVER {
            return self.emit(Ir::PixelDiscard);
        }
        let (a, r) = (lane(ftemp(c0), 3), lane(row(range::ALPHA_REF, 0), 0));
        let (cmp, x, y) = match func {
            2 => (FloatComparison::LessThan, a, r),
            3 => (FloatComparison::Equal, a, r),
            4 => (FloatComparison::GreaterEqual, r, a),
            5 => (FloatComparison::LessThan, r, a),
            6 => (FloatComparison::NotEqual, a, r),
            7 => (FloatComparison::GreaterEqual, a, r),
            // None, `D3DCMP_ALWAYS` and a value `emit.rs` has no case for pass.
            _ => return Ok(()),
        };
        let pass = self.scratch();
        self.emit(fcmp(cmp, idst(pass, MASK_X), x, y))?;
        self.discard_if(Condition {
            operand: lane(itemp(pass), 0),
            test_nonzero: false,
        })
    }

    /// The fog blend into `oC0.rgb` (`write_fog_blend`).
    fn fog(&mut self, c0: u32) -> Result<(), LowerError> {
        let key = self.ps.key;
        if key.fog_mode == 0 && key.fog_table_mode == 0 {
            return Ok(());
        }
        let params = row(range::FOG, 1);
        let f = self.scratch();
        if key.fog_table_mode == 0 {
            // `saturate(in.fog.x)`.
            self.emit(mov_sat(fdst(f, MASK_X), lane(input(IN_FOG), 0), true))?;
        } else {
            // The source: `1 / in.position.w`, or `in.fog_z + fog_data[1].w`.
            let c = self.scratch();
            if key.flags.contains(VariantFlags::FOG_SOURCE_W) {
                self.emit(fun(
                    FloatUnaryOp::Rcp,
                    fdst(c, MASK_X),
                    lane(input(IN_POSITION), 3),
                ))?;
            } else {
                self.emit(fbin(
                    FloatBinaryOp::Add,
                    fdst(c, MASK_X),
                    lane(input(IN_FOG_Z), 0),
                    lane(params.clone(), 3),
                ))?;
            }
            let c = lane(ftemp(c), 0);
            let density = lane(params.clone(), 2);
            match key.fog_table_mode {
                // EXP `saturate(exp(-density * c))`, EXP2 `saturate(exp(-(density c)^2))`, as
                // `exp2(x * log2(e))`.
                1 | 2 => {
                    let x = if key.fog_table_mode == 1 {
                        self.binary(FloatBinaryOp::Mul, negate(density), c)?
                    } else {
                        let dz = self.binary(FloatBinaryOp::Mul, density, c)?;
                        self.binary(FloatBinaryOp::Mul, negate(dz.clone()), dz)?
                    };
                    let e =
                        self.binary(FloatBinaryOp::Mul, x, fsplat(core::f32::consts::LOG2_E))?;
                    let e = self.unary(FloatUnaryOp::Exp2, e)?;
                    self.emit(mov_sat(fdst(f, MASK_X), lane(e, 0), true))?;
                }
                // LINEAR: `range == 0 ? 0 : saturate((end - c) / range)`.
                _ => {
                    let (start, end) = (lane(params.clone(), 0), lane(params, 1));
                    let range = self.binary(FloatBinaryOp::Add, end.clone(), negate(start))?;
                    let left = self.binary(FloatBinaryOp::Add, end, negate(c))?;
                    let q = self.binary(FloatBinaryOp::Div, left, range.clone())?;
                    let q = {
                        let s = self.scratch();
                        self.emit(mov_sat(fdst(s, MASK_X), lane(q, 0), true))?;
                        lane(ftemp(s), 0)
                    };
                    let empty = self.mask(FloatComparison::Equal, range, fsplat(0.0))?;
                    self.emit(movc(fdst(f, MASK_X), lane(empty, 0), fsplat(0.0), q))?;
                }
            }
        }
        // `mix(fog_data[0].rgb, oC0.rgb, f)`.
        let colour = row(range::FOG, 0);
        let rgb = MASK_X | MASK_Y | MASK_Z;
        let d = self.scratch();
        self.emit(fbin(
            FloatBinaryOp::Add,
            fdst(d, rgb),
            ftemp(c0),
            negate(colour.clone()),
        ))?;
        self.emit(mad(fdst(c0, rgb), ftemp(d), lane(ftemp(f), 0), colour))
    }

    /// `oCi.rgb = mtld3d_linear_to_srgb(oCi.rgb)`.
    fn srgb(&mut self, t: u32) -> Result<(), LowerError> {
        let c = ftemp(t);
        let lo = self.binary(FloatBinaryOp::Mul, c.clone(), fsplat(12.92))?;
        let floor = self.binary(FloatBinaryOp::Max, c.clone(), fsplat(0.0))?;
        let p = self.pow(floor, fsplat(1.0 / 2.4))?;
        let hi = self.scratch();
        self.emit(mad(fdst(hi, MASK_ALL), p, fsplat(1.055), fsplat(-0.055)))?;
        let low = self.mask(FloatComparison::GreaterEqual, fsplat(0.003_130_8), c)?;
        self.emit(movc(fdst(t, MASK_X | MASK_Y | MASK_Z), low, lo, ftemp(hi)))
    }

    /// The function signature: the `Varyings` members, then the outputs in `PsOut`'s order.
    fn pixel_signature(&self) -> SignatureState {
        let key = self.ps.key;
        let mut s = SignatureState::default();
        let init = |s: &mut SignatureState, reg: u32, mask: u32, input: FunctionInput| {
            let index = s.func_signature.define_input(input);
            s.signature_handlers.push(SignatureHandler::InitInput {
                index,
                reg,
                mask,
                fix_w: false,
            });
        };
        let varying = |semantic: Semantic, interpolation| FunctionInput::FragmentStageIn {
            user: varying_name(semantic),
            ty: MslValue::FLOAT4,
            interpolation,
            pull_mode: false,
        };
        let smooth = || Interpolation::CenterPerspective;
        init(
            &mut s,
            IN_POSITION,
            MASK_ALL,
            FunctionInput::Position {
                interpolation: Interpolation::CenterNoPerspective,
            },
        );
        init(
            &mut s,
            IN_POSITION1,
            MASK_ALL,
            varying(Semantic::new(DeclUsage::Position, 1), smooth()),
        );
        for i in 0..TEXCOORDS {
            let semantic = Semantic::new(DeclUsage::Texcoord, i);
            init(
                &mut s,
                IN_TEXCOORD + i,
                MASK_ALL,
                varying(semantic, smooth()),
            );
        }
        let flat = key.flags.contains(VariantFlags::FLAT_SHADE);
        for i in 0..2 {
            let interpolation = if flat { Interpolation::Flat } else { smooth() };
            let semantic = Semantic::new(DeclUsage::Color, i);
            init(
                &mut s,
                IN_COLOR + i,
                MASK_ALL,
                varying(semantic, interpolation),
            );
        }
        init(
            &mut s,
            IN_FOG,
            MASK_ALL,
            varying(Semantic::new(DeclUsage::Fog, 0), smooth()),
        );
        init(
            &mut s,
            IN_FOG_Z,
            MASK_X,
            FunctionInput::FragmentStageIn {
                user: FOG_Z_VARYING.to_owned(),
                ty: MslValue::Scalar(MslScalar::Float),
                interpolation: Interpolation::CenterNoPerspective,
                pull_mode: false,
            },
        );
        for (semantic, reg) in self.ps.extras.iter().zip(IN_EXTRA..) {
            init(&mut s, reg, MASK_ALL, varying(*semantic, smooth()));
        }
        if !self.ps.sprite.is_empty() {
            init(
                &mut s,
                IN_POINT_COORD,
                MASK_X | MASK_Y,
                FunctionInput::PointCoord,
            );
        }
        if self.ps.vface {
            init(&mut s, IN_FACE, MASK_X, FunctionInput::FrontFacing);
        }
        s.max_input_register = IN_EXTRA + u32::try_from(self.ps.extras.len()).unwrap_or(0);
        let pop = |s: &mut SignatureState, reg: u32, mask: u32, output: FunctionOutput| {
            let index = s.func_signature.define_output(output);
            s.signature_handlers
                .push(SignatureHandler::PopOutput { reg, mask, index });
        };
        for i in self.exported() {
            let Ok(index) = u32::try_from(i) else {
                continue;
            };
            let output = FunctionOutput::RenderTarget {
                dual_source_blending: false,
                index,
                ty: MslValue::FLOAT4,
            };
            pop(&mut s, index, MASK_ALL, output);
        }
        if self.exports_depth() {
            let output = FunctionOutput::Depth {
                argument: DepthArgument::Any,
            };
            pop(&mut s, OUT_DEPTH, MASK_X, output);
        }
        if key.flags.contains(VariantFlags::SAMPLE_MASK) {
            pop(
                &mut s,
                OUT_SAMPLE_MASK,
                MASK_X,
                FunctionOutput::CoverageMask,
            );
        }
        s.max_output_register = OUT_SAMPLE_MASK + 1;
        s
    }
}
