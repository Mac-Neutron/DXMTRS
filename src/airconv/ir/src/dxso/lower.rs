//! D3D9 shaders lowered into the DXBC instruction IR, which airconv's DXBC lowering takes to AIR.
//!
//! The oracle is D3D9's MSL emitter (`mtld3d-core`'s `dxso/emit.rs`): what it writes is what a
//! player sees, so each instruction lowers to the same operations in the same order, and a
//! construct it refuses, or that Metal refuses in the MSL it writes, is refused here as
//! [`LowerError::Unsupported`]. D3D9-only semantics expand into plain IR (`lit`, `nrm`, the
//! `m4x4` family, predicates, `loop aL`, relative constants, the source modifiers), and
//! `call`/`callnz` are inlined at their call sites.
//!
//! Every D3D9 register lives in the DXBC temporary file, outputs included: they are written
//! back to the output registers only at the end, after the epilogue `emit.rs` runs. [`ROWS`]
//! is the opcode table both stages are checked against, one row per opcode and control.

use std::collections::BTreeMap;

use super::{
    DxsoProgram, Semantic, SemanticSet, TextureType,
    ir::{CmpFunc, Declaration, InstrFlags, Instruction, RegKind, ShaderType, SrcModifier},
    opcode::Opcode,
    spec::{VariantKey, VsSpec},
};
use crate::{
    cfg::Cfg,
    handlers::SignatureState,
    ir::{BasicBlock, BlockId},
};

mod inline;
mod ops;
mod regs;
mod vs;

/// A lowered stage: its blocks, its signature and handlers, and the semantics it outputs.
pub struct Lowered {
    pub cfg: Cfg,
    pub io: SignatureState,
    /// The SM3 extra semantics a vertex shader outputs ([`SemanticSet::vs_outputs`]).
    pub outputs: SemanticSet,
}

/// What the lowering refuses.
#[derive(Debug, PartialEq, Eq)]
pub enum LowerError {
    /// An instruction, operand or register `emit.rs` (or Metal, in the MSL it writes) refuses.
    ///
    /// `opcode` is the `D3DSIO_*` value of the instruction it appears in.
    Unsupported { opcode: u16, what: &'static str },
}

/// The constant-buffer ranges the lowering reads; the bindings map each to its Metal slot.
pub mod range {
    /// `c#`, `float4` rows.
    pub const FLOAT: u32 = 0;
    /// `i#`, `int4` rows; declared only for a dynamic integer constant.
    pub const INT: u32 = 1;
    /// `b#`, one `uint` bit mask, read only as its first word.
    pub const BOOL: u32 = 2;
    /// `PosFixup`: row 0 is `(x, y, z, w)`, row 1's first word `depth_bias` (five floats).
    pub const POS_FIXUP: u32 = 3;
    /// `VsDraw`: row 0 `point`, 1 `point_scale`, 2-5 `inv_view`, 6-11 `clip`.
    pub const VS_DRAW: u32 = 4;
    /// The vertex LOD table, one `float2` per sampler slot, read one word at a time.
    pub const VS_LOD: u32 = 5;
}

/// `VsDraw`'s first user clip plane row.
pub const VS_DRAW_CLIP_ROW: u32 = 6;
/// The most user clip planes D3D9 has (`D3DMAXUSERCLIPPLANES`), the rows `VsDraw` holds.
pub const MAX_CLIP_PLANES: u8 = 6;

/// The varying name of the base member `fog_z`, which carries no semantic.
pub const FOG_Z_VARYING: &str = "d3d9_fog_z";

/// The varying a semantic travels in: `d3d9_<code>`, `code` being [`Semantic::code`].
///
/// Both stages of an AIR pair name their `user(...)` members this way, so Metal links a
/// vertex output to the pixel input of the same semantic, as it links the MSL members.
#[must_use]
pub fn varying_name(semantic: Semantic) -> String {
    format!("d3d9_{}", semantic.code())
}

/// What a vertex stage binds besides its constants, under the MSL emitter's conditions.
pub struct VsBindings {
    /// `i#` at its slot: the program reads an integer constant no `defi` defines.
    pub int_consts: bool,
    /// `b#` at its slot: the program reads a boolean constant no `defb` defines.
    pub bool_consts: bool,
    /// The declared sampler slots, each typed by the texture kind bound there.
    pub samplers: BTreeMap<u16, TextureType>,
    /// The vertex LOD table: the spec asks for it and the program declares a sampler.
    pub lod_table: bool,
}

/// The vertex stage's bindings (`emit.rs`'s `emit_vs_function` conditions).
#[must_use]
pub fn vs_bindings(program: &DxsoProgram, spec: &VsSpec) -> VsBindings {
    let mut samplers = BTreeMap::new();
    for decl in &program.declarations {
        if let Declaration::Sampler { reg, .. } = decl {
            samplers.insert(reg.index, spec.sampler_kinds.kind(reg.index));
        }
    }
    VsBindings {
        int_consts: program.uses_dynamic_int_constants(),
        bool_consts: program.uses_dynamic_bool_constants(),
        lod_table: spec.sampler_kinds.lod_table && !samplers.is_empty(),
        samplers,
    }
}

/// Lowers a vertex shader specialized by `spec`.
///
/// # Errors
///
/// [`LowerError::Unsupported`] for what `emit.rs` refuses or what Metal refuses in its MSL.
pub fn vertex(program: &DxsoProgram, spec: &VsSpec) -> Result<Lowered, LowerError> {
    vs::lower(program, *spec)
}

/// Lowers a pixel shader specialized by `key`.
///
/// # Errors
///
/// [`LowerError::Unsupported`]: pixel shaders are not lowered yet.
pub const fn pixel(program: &DxsoProgram, key: &VariantKey) -> Result<Lowered, LowerError> {
    let _ = (program, key);
    Err(LowerError::Unsupported {
        opcode: 0,
        what: PS_LATER,
    })
}

/// An instruction's control bits, as far as they select its behaviour.
#[derive(Debug, PartialEq, Eq)]
pub enum Control {
    None,
    /// `texldp` (`D3DSI_TEXLD_PROJECT`).
    Projected,
    /// `texldb` (`D3DSI_TEXLD_BIAS`).
    Biased,
    /// `ifc`/`breakc`/`setp`'s comparison; `None` for a value the parser does not know.
    Cmp(Option<CmpFunc>),
    /// A co-issued `cnd` (`D3DSI_COISSUE`).
    Coissue,
}

/// What a stage does with an opcode under a control.
#[derive(Debug, PartialEq, Eq)]
pub enum Support {
    Lowered,
    /// Refused, with the reason.
    Unsupported(&'static str),
    /// Never an instruction: the parser consumes it (declarations, comments, labels).
    Parser,
}

/// One row of the opcode table.
pub struct Row {
    pub opcode: Opcode,
    pub control: Control,
    pub vs: Support,
    pub ps: Support,
}

/// Pixel shaders, until their lowering lands.
const PS_LATER: &str = "pixel shaders are not lowered yet";
const PS_ONLY: &str = "a pixel shader op (it reads pixel texture coordinates or writes a pixel)";
const DERIVATIVE: &str = "a derivative in a vertex function (Metal refuses it)";
const DISCARD: &str = "a discard in a vertex function (Metal refuses it)";

/// A row lowered in a vertex shader.
const fn vs_row(opcode: Opcode, control: Control) -> Row {
    Row {
        opcode,
        control,
        vs: Support::Lowered,
        ps: Support::Unsupported(PS_LATER),
    }
}

/// A row a vertex shader refuses.
const fn vs_refused(opcode: Opcode, why: &'static str) -> Row {
    Row {
        opcode,
        control: Control::None,
        vs: Support::Unsupported(why),
        ps: Support::Unsupported(PS_LATER),
    }
}

/// A token the parser consumes.
const fn parsed(opcode: Opcode) -> Row {
    Row {
        opcode,
        control: Control::None,
        vs: Support::Parser,
        ps: Support::Parser,
    }
}

const fn plain(opcode: Opcode) -> Row {
    vs_row(opcode, Control::None)
}

const fn cmp(opcode: Opcode, func: Option<CmpFunc>) -> Row {
    vs_row(opcode, Control::Cmp(func))
}

/// The opcode table: one row per (opcode, control) pair, in `opcode.rs`'s order.
pub const ROWS: &[Row] = &[
    plain(Opcode::Nop),
    plain(Opcode::Mov),
    plain(Opcode::Add),
    plain(Opcode::Sub),
    plain(Opcode::Mad),
    plain(Opcode::Mul),
    plain(Opcode::Rcp),
    plain(Opcode::Rsq),
    plain(Opcode::Dp3),
    plain(Opcode::Dp4),
    plain(Opcode::Min),
    plain(Opcode::Max),
    plain(Opcode::Slt),
    plain(Opcode::Sge),
    plain(Opcode::Exp),
    plain(Opcode::Log),
    plain(Opcode::Lit),
    plain(Opcode::Dst),
    plain(Opcode::Lrp),
    plain(Opcode::Frc),
    plain(Opcode::M4x4),
    plain(Opcode::M4x3),
    plain(Opcode::M3x4),
    plain(Opcode::M3x3),
    plain(Opcode::M3x2),
    plain(Opcode::Call),
    plain(Opcode::CallNz),
    plain(Opcode::Loop),
    parsed(Opcode::Ret),
    plain(Opcode::EndLoop),
    parsed(Opcode::Label),
    parsed(Opcode::Dcl),
    plain(Opcode::Pow),
    plain(Opcode::Crs),
    plain(Opcode::Sgn),
    plain(Opcode::Abs),
    plain(Opcode::Nrm),
    plain(Opcode::SinCos),
    plain(Opcode::Rep),
    plain(Opcode::EndRep),
    plain(Opcode::If),
    cmp(Opcode::Ifc, None),
    cmp(Opcode::Ifc, Some(CmpFunc::Gt)),
    cmp(Opcode::Ifc, Some(CmpFunc::Eq)),
    cmp(Opcode::Ifc, Some(CmpFunc::Ge)),
    cmp(Opcode::Ifc, Some(CmpFunc::Lt)),
    cmp(Opcode::Ifc, Some(CmpFunc::Ne)),
    cmp(Opcode::Ifc, Some(CmpFunc::Le)),
    plain(Opcode::Else),
    plain(Opcode::EndIf),
    plain(Opcode::Break),
    cmp(Opcode::BreakC, None),
    cmp(Opcode::BreakC, Some(CmpFunc::Gt)),
    cmp(Opcode::BreakC, Some(CmpFunc::Eq)),
    cmp(Opcode::BreakC, Some(CmpFunc::Ge)),
    cmp(Opcode::BreakC, Some(CmpFunc::Lt)),
    cmp(Opcode::BreakC, Some(CmpFunc::Ne)),
    cmp(Opcode::BreakC, Some(CmpFunc::Le)),
    plain(Opcode::MovA),
    parsed(Opcode::DefB),
    parsed(Opcode::DefI),
    vs_refused(Opcode::TexCoord, PS_ONLY),
    vs_refused(Opcode::TexKill, DISCARD),
    vs_row(Opcode::TexLd, Control::None),
    vs_row(Opcode::TexLd, Control::Projected),
    vs_row(Opcode::TexLd, Control::Biased),
    vs_refused(Opcode::TexBem, PS_ONLY),
    vs_refused(Opcode::TexBemL, PS_ONLY),
    vs_refused(Opcode::TexReg2Ar, PS_ONLY),
    vs_refused(Opcode::TexReg2Gb, PS_ONLY),
    vs_refused(Opcode::TexM3x2Pad, PS_ONLY),
    vs_refused(Opcode::TexM3x2Tex, PS_ONLY),
    vs_refused(Opcode::TexM3x3Pad, PS_ONLY),
    vs_refused(Opcode::TexM3x3Tex, PS_ONLY),
    vs_refused(Opcode::TexM3x3Spec, PS_ONLY),
    vs_refused(Opcode::TexM3x3VSpec, PS_ONLY),
    vs_refused(Opcode::TexReg2Rgb, PS_ONLY),
    vs_refused(Opcode::TexDp3Tex, PS_ONLY),
    vs_refused(Opcode::TexM3x2Depth, PS_ONLY),
    vs_refused(Opcode::TexDp3, PS_ONLY),
    vs_refused(Opcode::TexM3x3, PS_ONLY),
    vs_refused(Opcode::TexDepth, PS_ONLY),
    vs_refused(Opcode::Bem, PS_ONLY),
    plain(Opcode::ExpP),
    plain(Opcode::LogP),
    vs_row(Opcode::Cnd, Control::None),
    vs_row(Opcode::Cnd, Control::Coissue),
    parsed(Opcode::Def),
    plain(Opcode::Cmp),
    plain(Opcode::Dp2Add),
    vs_refused(Opcode::Dsx, DERIVATIVE),
    vs_refused(Opcode::Dsy, DERIVATIVE),
    plain(Opcode::TexLdD),
    cmp(Opcode::SetP, None),
    cmp(Opcode::SetP, Some(CmpFunc::Gt)),
    cmp(Opcode::SetP, Some(CmpFunc::Eq)),
    cmp(Opcode::SetP, Some(CmpFunc::Ge)),
    cmp(Opcode::SetP, Some(CmpFunc::Lt)),
    cmp(Opcode::SetP, Some(CmpFunc::Ne)),
    cmp(Opcode::SetP, Some(CmpFunc::Le)),
    plain(Opcode::TexLdL),
    plain(Opcode::BreakP),
    parsed(Opcode::Phase),
    parsed(Opcode::End),
    parsed(Opcode::Comment),
];

/// The source modifiers, each with what the two stages do with it.
pub const MODIFIER_ROWS: &[(SrcModifier, Support, Support)] = &[
    (
        SrcModifier::None,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Neg,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Bias,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::BiasNeg,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Sign,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::SignNeg,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Comp,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::X2,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::X2Neg,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Dz,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Dw,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Abs,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::AbsNeg,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        SrcModifier::Not,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
];

/// A relative address through a register `emit.rs` has no case for.
const REL_KIND: &str = "a relative address through a register other than a0 or aL";

/// The register kinds a relative constant address may go through.
pub const REL_ADDR_ROWS: &[(RegKind, Support, Support)] = &[
    (
        RegKind::Addr,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Loop,
        Support::Lowered,
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Temp,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Input,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Const,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::RastOut,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::AttrOut,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::TexcoordOut,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::ConstInt,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::ColorOut,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::DepthOut,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Sampler,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Output,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::ConstBool,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::TempFloat16,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::MiscType,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Label,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
    (
        RegKind::Predicate,
        Support::Unsupported(REL_KIND),
        Support::Unsupported(PS_LATER),
    ),
];

/// The control of `inst`, as the table keys it.
#[must_use]
pub const fn control(inst: &Instruction) -> Control {
    match inst.opcode {
        Opcode::TexLd if inst.flags.contains(InstrFlags::TEX_PROJECTED) => Control::Projected,
        Opcode::TexLd if inst.flags.contains(InstrFlags::TEX_BIASED) => Control::Biased,
        Opcode::Ifc | Opcode::BreakC | Opcode::SetP => Control::Cmp(inst.cmp_func),
        Opcode::Cnd if inst.flags.contains(InstrFlags::COISSUE) => Control::Coissue,
        _ => Control::None,
    }
}

/// What `stage` does with `support`'s row for the pair.
const fn for_stage(row: &Row, stage: ShaderType) -> &Support {
    match stage {
        ShaderType::Vertex => &row.vs,
        ShaderType::Pixel => &row.ps,
    }
}

/// The table's answer for an opcode under a control; `None` for a pair it has no row for.
#[must_use]
pub fn support(stage: ShaderType, opcode: Opcode, control: &Control) -> Option<&'static Support> {
    ROWS.iter()
        .find(|row| row.opcode == opcode && row.control == *control)
        .map(|row| for_stage(row, stage))
}

/// The table's answer for a source modifier.
#[must_use]
pub fn modifier_support(stage: ShaderType, modifier: SrcModifier) -> Option<&'static Support> {
    MODIFIER_ROWS
        .iter()
        .find(|(m, ..)| *m == modifier)
        .map(|(_, vs, ps)| match stage {
            ShaderType::Vertex => vs,
            ShaderType::Pixel => ps,
        })
}

/// The table's answer for a relative address through `kind`.
#[must_use]
pub fn rel_addr_support(stage: ShaderType, kind: RegKind) -> Option<&'static Support> {
    REL_ADDR_ROWS
        .iter()
        .find(|(k, ..)| *k == kind)
        .map(|(_, vs, ps)| match stage {
            ShaderType::Vertex => vs,
            ShaderType::Pixel => ps,
        })
}

/// `D3DSIO_*`: an opcode's token value.
#[must_use]
pub const fn opcode_value(opcode: Opcode) -> u16 {
    match opcode {
        Opcode::Nop => 0,
        Opcode::Mov => 1,
        Opcode::Add => 2,
        Opcode::Sub => 3,
        Opcode::Mad => 4,
        Opcode::Mul => 5,
        Opcode::Rcp => 6,
        Opcode::Rsq => 7,
        Opcode::Dp3 => 8,
        Opcode::Dp4 => 9,
        Opcode::Min => 10,
        Opcode::Max => 11,
        Opcode::Slt => 12,
        Opcode::Sge => 13,
        Opcode::Exp => 14,
        Opcode::Log => 15,
        Opcode::Lit => 16,
        Opcode::Dst => 17,
        Opcode::Lrp => 18,
        Opcode::Frc => 19,
        Opcode::M4x4 => 20,
        Opcode::M4x3 => 21,
        Opcode::M3x4 => 22,
        Opcode::M3x3 => 23,
        Opcode::M3x2 => 24,
        Opcode::Call => 25,
        Opcode::CallNz => 26,
        Opcode::Loop => 27,
        Opcode::Ret => 28,
        Opcode::EndLoop => 29,
        Opcode::Label => 30,
        Opcode::Dcl => 31,
        Opcode::Pow => 32,
        Opcode::Crs => 33,
        Opcode::Sgn => 34,
        Opcode::Abs => 35,
        Opcode::Nrm => 36,
        Opcode::SinCos => 37,
        Opcode::Rep => 38,
        Opcode::EndRep => 39,
        Opcode::If => 40,
        Opcode::Ifc => 41,
        Opcode::Else => 42,
        Opcode::EndIf => 43,
        Opcode::Break => 44,
        Opcode::BreakC => 45,
        Opcode::MovA => 46,
        Opcode::DefB => 47,
        Opcode::DefI => 48,
        Opcode::TexCoord => 64,
        Opcode::TexKill => 65,
        Opcode::TexLd => 66,
        Opcode::TexBem => 67,
        Opcode::TexBemL => 68,
        Opcode::TexReg2Ar => 69,
        Opcode::TexReg2Gb => 70,
        Opcode::TexM3x2Pad => 71,
        Opcode::TexM3x2Tex => 72,
        Opcode::TexM3x3Pad => 73,
        Opcode::TexM3x3Tex => 74,
        Opcode::TexM3x3Spec => 76,
        Opcode::TexM3x3VSpec => 77,
        Opcode::ExpP => 78,
        Opcode::LogP => 79,
        Opcode::Cnd => 80,
        Opcode::Def => 81,
        Opcode::TexReg2Rgb => 82,
        Opcode::TexDp3Tex => 83,
        Opcode::TexM3x2Depth => 84,
        Opcode::TexDp3 => 85,
        Opcode::TexM3x3 => 86,
        Opcode::TexDepth => 87,
        Opcode::Cmp => 88,
        Opcode::Bem => 89,
        Opcode::Dp2Add => 90,
        Opcode::Dsx => 91,
        Opcode::Dsy => 92,
        Opcode::TexLdD => 93,
        Opcode::SetP => 94,
        Opcode::TexLdL => 95,
        Opcode::BreakP => 96,
        Opcode::Phase => 0xFFFD,
        Opcode::Comment => 0xFFFE,
        Opcode::End => 0xFFFF,
    }
}

/// The most IR instructions a program may lower to (calls inside calls multiply).
const MAX_INSTRUCTIONS: usize = 1 << 20;
/// The deepest `call` nesting; D3D9 allows four, a recursive call is skipped before this.
const MAX_CALL_DEPTH: usize = 32;

/// A flow-control construct the program has open: one brace of the MSL.
enum Frame {
    /// `if`: the block the `else` (or the end) starts, and the end once an `else` was seen.
    If {
        alternative: BlockId,
        end: Option<BlockId>,
    },
    /// `loop`/`rep`: the block testing the count, the block after the loop, its counters.
    Loop {
        header: BlockId,
        exit: BlockId,
        counter: Counter,
    },
}

/// A loop's counters, integer temporaries.
enum Counter {
    /// `loop aL, iN`: `aL` and its step.
    Al { al: u32, step: u32 },
    /// `rep iN`: the iterations run so far.
    Rep { count: u32 },
}

/// The lowering of one program: the blocks so far and the registers they use.
struct Lower<'p> {
    program: &'p DxsoProgram,
    stage: ShaderType,
    blocks: Vec<BasicBlock>,
    current: BlockId,
    /// The next temporary never handed out.
    next_temp: u32,
    /// Scratch temporaries free for the current instruction, and those it took.
    free: Vec<u32>,
    busy: Vec<u32>,
    frames: Vec<Frame>,
    /// The labels being expanded, innermost last.
    calls: Vec<u32>,
    emitted: usize,
    /// The `D3DSIO_*` value of the instruction being lowered, for errors.
    opcode: u16,
    defs: BTreeMap<u16, [f32; 4]>,
    defi: BTreeMap<u16, [i32; 4]>,
    defb: BTreeMap<u16, bool>,
    vs: vs::VsIo,
}

impl<'p> Lower<'p> {
    fn new(program: &'p DxsoProgram, stage: ShaderType, vs: vs::VsIo) -> Self {
        let mut blocks = Vec::new();
        let current = BlockId::push(&mut blocks, "entry");
        Self {
            program,
            stage,
            blocks,
            current,
            next_temp: regs::FIRST_FREE,
            free: Vec::new(),
            busy: Vec::new(),
            frames: Vec::new(),
            calls: Vec::new(),
            emitted: 0,
            opcode: 0,
            defs: program
                .def_constants
                .iter()
                .filter(|d| d.reg.kind == RegKind::Const)
                .map(|d| (d.reg.index, d.value))
                .collect(),
            defi: program
                .def_int_constants
                .iter()
                .map(|d| (d.reg.index, d.value))
                .collect(),
            defb: program
                .def_bool_constants
                .iter()
                .map(|d| (d.reg.index, d.value))
                .collect(),
            vs,
        }
    }

    const fn is_vertex(&self) -> bool {
        matches!(self.stage, ShaderType::Vertex)
    }

    /// The error for what the current instruction cannot lower.
    const fn refuse(&self, what: &'static str) -> LowerError {
        LowerError::Unsupported {
            opcode: self.opcode,
            what,
        }
    }

    /// A temporary that lives until the next instruction.
    fn scratch(&mut self) -> u32 {
        let t = self.free.pop().unwrap_or_else(|| {
            let t = self.next_temp;
            self.next_temp = t.wrapping_add(1);
            t
        });
        self.busy.push(t);
        t
    }

    /// A temporary that lives as long as the program.
    const fn perm(&mut self) -> u32 {
        let t = self.next_temp;
        self.next_temp = t.wrapping_add(1);
        t
    }

    /// Hands the scratch temporaries of the instruction before back.
    fn release(&mut self) {
        self.free.append(&mut self.busy);
    }

    /// Appends `inst` to the current block.
    fn emit(&mut self, inst: crate::ir::Instruction) -> Result<(), LowerError> {
        self.emitted += 1;
        if self.emitted > MAX_INSTRUCTIONS {
            return Err(self.refuse("a program that lowers past the instruction budget"));
        }
        let Some(block) = self.blocks.get_mut(self.current.index()) else {
            return Err(self.refuse("an instruction outside every block"));
        };
        block.instructions.push(inst);
        Ok(())
    }

    /// A new empty block.
    fn block(&mut self, name: &'static str) -> Result<BlockId, LowerError> {
        if self.blocks.len() >= crate::cfg::MAX_BLOCKS {
            return Err(self.refuse("a program that lowers past the block budget"));
        }
        Ok(BlockId::push(&mut self.blocks, name))
    }

    /// Ends the current block with `target`.
    fn end_block(&mut self, target: crate::ir::Target) -> Result<(), LowerError> {
        let Some(block) = self.blocks.get_mut(self.current.index()) else {
            return Err(self.refuse("a branch outside every block"));
        };
        block.target = target;
        Ok(())
    }

    /// The innermost `loop`'s `aL`, `None` outside every `loop` (`current_loop_al`).
    fn loop_counter(&self) -> Option<u32> {
        self.frames.iter().rev().find_map(|f| match f {
            Frame::Loop {
                counter: Counter::Al { al, .. },
                ..
            } => Some(*al),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests;
