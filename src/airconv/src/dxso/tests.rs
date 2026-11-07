//! D3D9 vertex shaders: the lowering's IR for each feature, and a metallib from each.
//!
//! Programs are assembled from tokens here, as `emit.rs`'s tests write them.

use airconv_ir::{
    binding::{FunctionInput, FunctionOutput, FunctionSignatureDesc},
    dxso::{
        lower::{self, LowerError, Lowered, range},
        parse,
        spec::{StageSpec, VsSamplerKinds, VsSpec},
    },
    handlers::SignatureHandler,
    ir::{
        FloatBinaryOp, FloatComparison, FloatUnaryOp, Instruction, IntegerBinaryOp,
        IntegerComparison, OperandIndex, SrcOperand, Target,
    },
};

use super::{bindings, translate};

const VS_1_1: u32 = 0xfffe_0101;
const VS_2_0: u32 = 0xfffe_0200;
const VS_3_0: u32 = 0xfffe_0300;
const END: u32 = 0x0000_ffff;

const TEMP: u32 = 0;
const INPUT: u32 = 1;
const CONST: u32 = 2;
const ADDR: u32 = 3;
const RASTOUT: u32 = 4;
const ATTROUT: u32 = 5;
const OUTPUT: u32 = 6;
const CONSTINT: u32 = 7;
const SAMPLER: u32 = 10;
const CONSTBOOL: u32 = 14;
const LOOP: u32 = 15;
const LABEL: u32 = 18;
const PREDICATE: u32 = 19;

const XYZW: u8 = 0xe4;
const XXXX: u8 = 0x00;
const YYYY: u8 = 0x55;
const ALL: u8 = 0xf;

const NOT: u32 = 13 << 24;
const PREDICATED: u32 = 1 << 28;

mod op {
    pub const MOV: u16 = 1;
    pub const ADD: u16 = 2;
    pub const MUL: u16 = 5;
    pub const LIT: u16 = 16;
    pub const DST: u16 = 17;
    pub const M4X4: u16 = 20;
    pub const CALL: u16 = 25;
    pub const CALLNZ: u16 = 26;
    pub const LOOP: u16 = 27;
    pub const RET: u16 = 28;
    pub const ENDLOOP: u16 = 29;
    pub const LABEL: u16 = 30;
    pub const DCL: u16 = 31;
    pub const POW: u16 = 32;
    pub const CRS: u16 = 33;
    pub const NRM: u16 = 36;
    pub const SINCOS: u16 = 37;
    pub const REP: u16 = 38;
    pub const ENDREP: u16 = 39;
    pub const IF: u16 = 40;
    pub const ENDIF: u16 = 43;
    pub const MOVA: u16 = 46;
    pub const DEFB: u16 = 47;
    pub const DEFI: u16 = 48;
    pub const DEF: u16 = 81;
    pub const SETP: u16 = 94;
    pub const TEXLD: u16 = 66;
    pub const TEXLDD: u16 = 93;
    pub const TEXLDL: u16 = 95;
    pub const BREAKP: u16 = 96;
}

/// A register token: type bits 28-30 and 11-12, the index, bit 31 set.
const fn reg(ty: u32, index: u32) -> u32 {
    0x8000_0000 | ((ty & 7) << 28) | (((ty >> 3) & 3) << 11) | index
}

/// A destination token.
const fn d(ty: u32, index: u32, mask: u8) -> u32 {
    reg(ty, index) | ((mask as u32) << 16)
}

/// A source token.
const fn s(ty: u32, index: u32, swizzle: u8) -> u32 {
    reg(ty, index) | ((swizzle as u32) << 16)
}

/// An instruction token with its operand token count.
const fn ins(opcode: u16, len: u32) -> u32 {
    opcode as u32 | (len << 24)
}

/// `dcl_<usage><index>` of a register.
fn dcl(usage: u32, index: u32, register: u32) -> Vec<u32> {
    vec![
        ins(op::DCL, 2),
        0x8000_0000 | (index << 16) | usage,
        register,
    ]
}

/// `dcl_2d`/`dcl_cube`/`dcl_volume` of a sampler.
fn dcl_sampler(slot: u32, ty: u32) -> Vec<u32> {
    vec![
        ins(op::DCL, 2),
        0x8000_0000 | (ty << 27),
        d(SAMPLER, slot, ALL),
    ]
}

fn def(index: u32, v: [f32; 4]) -> Vec<u32> {
    let mut t = vec![ins(op::DEF, 5), d(CONST, index, ALL)];
    t.extend(v.map(f32::to_bits));
    t
}

fn defi(index: u32, v: [i32; 4]) -> Vec<u32> {
    let mut t = vec![ins(op::DEFI, 5), d(CONSTINT, index, ALL)];
    t.extend(v.map(i32::cast_unsigned));
    t
}

const POSITION: u32 = 0;
const TEXCOORD: u32 = 5;
const FOG: u32 = 11;

/// A program from its body: the header, the tokens, the end.
fn program(header: u32, body: &[Vec<u32>]) -> Vec<u32> {
    let mut t = vec![header];
    for part in body {
        t.extend_from_slice(part);
    }
    t.push(END);
    t
}

/// Every input provided, no clip plane, every sampler 2D.
fn spec() -> VsSpec {
    VsSpec {
        provided_input_mask: u16::MAX,
        ..VsSpec::default()
    }
}

fn lowered(tokens: &[u32], spec: VsSpec) -> Lowered {
    let program = parse(tokens).unwrap_or_else(|e| panic!("parse: {e:?}"));
    lower::vertex(&program, &spec).unwrap_or_else(|e| panic!("lower: {e:?}"))
}

fn refused(tokens: &[u32], spec: VsSpec) -> LowerError {
    let program = parse(tokens).unwrap_or_else(|e| panic!("parse: {e:?}"));
    match lower::vertex(&program, &spec) {
        Ok(_) => panic!("lowered"),
        Err(e) => e,
    }
}

/// `translate` answers a metallib for the program.
fn metallib(tokens: &[u32], spec: VsSpec) {
    match translate(tokens, &StageSpec::Vs(spec), c"main") {
        Ok(bytes) => assert_eq!(bytes.get(..4), Some(&b"MTLB"[..]), "a metallib"),
        Err(e) => panic!("translate: {}", String::from_utf8_lossy(e.text())),
    }
}

fn instructions(l: &Lowered) -> impl Iterator<Item = &Instruction> {
    l.cfg.blocks.iter().flat_map(|b| b.instructions.iter())
}

fn count(l: &Lowered, f: impl Fn(&Instruction) -> bool) -> usize {
    instructions(l).filter(|i| f(i)).count()
}

/// Whether `op` reads row `row` of constant-buffer range `range`.
fn reads_row(op: &SrcOperand, range: u32, row: u32) -> bool {
    matches!(op, SrcOperand::ConstantBuffer { rangeid, regindex: OperandIndex::Immediate(r), .. }
        if *rangeid == range && *r == row)
}

/// Whether `op` reads constant-buffer range `range` at all.
fn reads_range(op: &SrcOperand, range: u32) -> bool {
    matches!(op, SrcOperand::ConstantBuffer { rangeid, .. } if *rangeid == range)
}

/// Every source operand of an instruction this test file builds.
fn sources(i: &Instruction) -> Vec<&SrcOperand> {
    match i {
        Instruction::Mov(m) => vec![&m.src],
        Instruction::MovConditional(m) => vec![&m.src_cond, &m.src0, &m.src1],
        Instruction::DotProduct(m) => vec![&m.src0, &m.src1],
        Instruction::FloatBinaryOp(m) => vec![&m.src0, &m.src1],
        Instruction::FloatUnaryOp(m) => vec![&m.src],
        Instruction::FloatMad(m) => vec![&m.src0, &m.src1, &m.src2],
        Instruction::FloatCompare(m) => vec![&m.src0, &m.src1],
        Instruction::IntegerBinaryOp(m) => vec![&m.src0, &m.src1],
        Instruction::IntegerCompare(m) => vec![&m.src0, &m.src1],
        Instruction::Convert(m) => vec![&m.src],
        Instruction::SinCos(m) => vec![&m.src],
        Instruction::SampleLod(m) => vec![&m.src_address, &m.src_lod],
        _ => Vec::new(),
    }
}

fn any_source(l: &Lowered, f: impl Fn(&SrcOperand) -> bool) -> bool {
    instructions(l).flat_map(sources).any(f)
}

/// The function outputs and the handler popping each.
fn pops(l: &Lowered) -> Vec<(u32, &FunctionOutput)> {
    l.io.signature_handlers
        .iter()
        .filter_map(|h| match h {
            SignatureHandler::PopOutput { reg, index, .. } => {
                l.io.func_signature
                    .outputs()
                    .get(*index as usize)
                    .map(|o| (*reg, o))
            }
            _ => None,
        })
        .collect()
}

/// `vs_1_1`: `dcl_position v0; m4x4 oPos, v0, c0`.
fn m4x4() -> Vec<u32> {
    program(
        VS_1_1,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            vec![
                ins(op::M4X4, 3),
                d(RASTOUT, 0, ALL),
                s(INPUT, 0, XYZW),
                s(CONST, 0, XYZW),
            ],
        ],
    )
}

#[test]
fn vs_1_1_m4x4_position() {
    let l = lowered(&m4x4(), spec());
    // One dp4 per row, rows c0-c3.
    for row in 0..4 {
        assert_eq!(
            count(&l, |i| matches!(i, Instruction::DotProduct(d)
                if d.dimension == 4 && reads_row(&d.src1, range::FLOAT, row))),
            1,
            "row {row}"
        );
    }
    assert!(l.io.signature_handlers.iter().any(|h| matches!(
        h,
        SignatureHandler::VertexInput {
            reg: 0,
            component_type: 3,
            ..
        }
    )));
    let outputs = pops(&l);
    assert!(matches!(
        outputs.first(),
        Some((0, FunctionOutput::Position { .. }))
    ));
    metallib(&m4x4(), spec());
}

/// `vs_2_0`: `loop aL, i0` reading `c[aL + 1]`, `rep i1` (dynamic), `call l0`.
fn loop_rep_call() -> Vec<u32> {
    program(
        VS_2_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            defi(0, [3, 0, 1, 0]),
            vec![ins(op::MOV, 2), d(TEMP, 0, ALL), s(INPUT, 0, XYZW)],
            vec![ins(op::LOOP, 2), s(LOOP, 0, XYZW), s(CONSTINT, 0, XYZW)],
            vec![
                ins(op::ADD, 4),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(CONST, 1, XYZW) | (1 << 13),
                s(LOOP, 0, XXXX),
            ],
            vec![ins(op::ENDLOOP, 0)],
            vec![ins(op::REP, 1), s(CONSTINT, 1, XYZW)],
            vec![ins(op::CALL, 1), s(LABEL, 0, XYZW)],
            vec![ins(op::ENDREP, 0)],
            vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(TEMP, 0, XYZW)],
            vec![ins(op::RET, 0)],
            vec![ins(op::LABEL, 1), s(LABEL, 0, XYZW)],
            vec![
                ins(op::MUL, 3),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(CONST, 5, XYZW),
            ],
            vec![ins(op::RET, 0)],
        ],
    )
}

#[test]
fn vs_2_0_loop_rep_call() {
    let l = lowered(&loop_rep_call(), spec());
    let headers = l
        .cfg
        .blocks
        .iter()
        .filter(|b| b.debug_name == "loop_header")
        .count();
    assert_eq!(headers, 2);
    // `c[aL + 1]`: the buffer row indexed by a temporary plus one, no clamp without `def`s.
    assert!(any_source(&l, |o| matches!(
        o,
        SrcOperand::ConstantBuffer {
            rangeid: range::FLOAT,
            regindex: OperandIndex::Temp { offset: 1, .. },
            ..
        }
    )));
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::IntegerBinaryOp(b) if b.op == IntegerBinaryOp::IMin)
        ),
        0
    );
    // `rep i1` reads the integer file; `i0` is a `defi`.
    assert!(any_source(&l, |o| reads_row(o, range::INT, 1)));
    assert!(!any_source(&l, |o| reads_row(o, range::INT, 0)));
    // The label's body is inlined: its `c5` read is in the function.
    assert!(any_source(&l, |o| reads_row(o, range::FLOAT, 5)));
    metallib(&loop_rep_call(), spec());
}

/// `callnz l0, b0`, `callnz l1, p0.x` and `l2`, read inside a loop, using `c[aL]`.
fn calls() -> Vec<u32> {
    program(
        VS_3_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            dcl(POSITION, 0, d(OUTPUT, 0, ALL)),
            defi(0, [2, 0, 1, 0]),
            vec![ins(op::MOV, 2), d(TEMP, 0, ALL), s(INPUT, 0, XYZW)],
            vec![
                ins(op::SETP, 3) | (1 << 16),
                d(PREDICATE, 0, ALL),
                s(TEMP, 0, XYZW),
                s(CONST, 0, XYZW),
            ],
            vec![ins(op::CALLNZ, 2), s(LABEL, 0, XYZW), s(CONSTBOOL, 0, XYZW)],
            vec![ins(op::CALLNZ, 2), s(LABEL, 1, XYZW), s(PREDICATE, 0, XXXX)],
            vec![ins(op::LOOP, 2), s(LOOP, 0, XYZW), s(CONSTINT, 0, XYZW)],
            vec![ins(op::CALL, 1), s(LABEL, 2, XYZW)],
            vec![ins(op::ENDLOOP, 0)],
            vec![ins(op::MOV, 2), d(OUTPUT, 0, ALL), s(TEMP, 0, XYZW)],
            vec![ins(op::RET, 0)],
            vec![ins(op::LABEL, 1), s(LABEL, 0, XYZW)],
            vec![
                ins(op::ADD, 3),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(CONST, 1, XYZW),
            ],
            // A recursive call: skipped, as the MSL skips it.
            vec![ins(op::CALL, 1), s(LABEL, 0, XYZW)],
            vec![ins(op::RET, 0)],
            vec![ins(op::LABEL, 1), s(LABEL, 1, XYZW)],
            vec![
                ins(op::MUL, 3),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(CONST, 2, XYZW),
            ],
            vec![ins(op::RET, 0)],
            vec![ins(op::LABEL, 1), s(LABEL, 2, XYZW)],
            vec![
                ins(op::ADD, 4),
                d(TEMP, 0, ALL),
                s(TEMP, 0, XYZW),
                s(CONST, 8, XYZW) | (1 << 13),
                s(LOOP, 0, XXXX),
            ],
            vec![ins(op::RET, 0)],
        ],
    )
}

/// `count` labels each calling the next.
fn call_chain(depth: u32) -> Vec<u32> {
    let mut body = vec![
        dcl(POSITION, 0, d(INPUT, 0, ALL)),
        vec![ins(op::CALL, 1), s(LABEL, 0, XYZW)],
        vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(INPUT, 0, XYZW)],
        vec![ins(op::RET, 0)],
    ];
    for n in 0..depth {
        body.push(vec![ins(op::LABEL, 1), s(LABEL, n, XYZW)]);
        body.push(vec![ins(op::CALL, 1), s(LABEL, n + 1, XYZW)]);
        body.push(vec![ins(op::RET, 0)]);
    }
    program(VS_2_0, &body)
}

#[test]
fn call_callnz_label_inlined() {
    let l = lowered(&calls(), spec());
    // Each body once (the recursive call is skipped), each `callnz` an `if`.
    assert_eq!(
        count(&l, |i| sources(i).iter().any(|o| reads_row(
            o,
            range::FLOAT,
            1
        ))),
        1
    );
    assert_eq!(
        count(&l, |i| sources(i).iter().any(|o| reads_row(
            o,
            range::FLOAT,
            2
        ))),
        1
    );
    let ifs = l
        .cfg
        .blocks
        .iter()
        .filter(|b| b.debug_name == "if_true")
        .count();
    assert_eq!(ifs, 2);
    // `callnz b0` tests bit 0 of the boolean word.
    assert!(any_source(&l, |o| reads_row(o, range::BOOL, 0)));
    // `aL` inside the subroutine is the calling loop's counter: `c[aL + 8]`.
    assert!(any_source(&l, |o| matches!(
        o,
        SrcOperand::ConstantBuffer {
            regindex: OperandIndex::Temp { offset: 8, .. },
            ..
        }
    )));
    metallib(&calls(), spec());
    // The depth limit: 32 nested calls lower, 33 are refused.
    let _ = lowered(&call_chain(32), spec());
    assert!(matches!(
        refused(&call_chain(33), spec()),
        LowerError::Unsupported { opcode: 25, .. }
    ));
    // A call tree is bounded by the instructions it expands to, `call`s included: 31 levels of
    // two calls each, and two levels of 300, are refused at once; four levels of four lower.
    for tokens in [call_tree(31, 2), call_tree(2, 300)] {
        let start = std::time::Instant::now();
        assert!(matches!(
            refused(&tokens, spec()),
            LowerError::Unsupported { what, .. } if what.contains("65536")
        ));
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
    }
    let _ = lowered(&call_tree(4, 4), spec());
    metallib(&call_tree(4, 4), spec());
}

/// `depth` labels, each calling the next `fan` times; the last one writes the position.
fn call_tree(depth: u32, fan: usize) -> Vec<u32> {
    let mut body = vec![
        dcl(POSITION, 0, d(INPUT, 0, ALL)),
        vec![ins(op::CALL, 1), s(LABEL, 0, XYZW)],
        vec![ins(op::RET, 0)],
    ];
    for n in 0..depth {
        body.push(vec![ins(op::LABEL, 1), s(LABEL, n, XYZW)]);
        for _ in 0..fan {
            body.push(vec![ins(op::CALL, 1), s(LABEL, n + 1, XYZW)]);
        }
        body.push(vec![ins(op::RET, 0)]);
    }
    body.push(vec![ins(op::LABEL, 1), s(LABEL, depth, XYZW)]);
    body.push(vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(INPUT, 0, XYZW)]);
    body.push(vec![ins(op::RET, 0)]);
    program(VS_2_0, &body)
}

/// `setp_gt p0, r0, c0`; `(p0.x) mov r1, c1`; `(!p0.y) add r1, r1, c2`; `breakp p0.z`.
fn predicates() -> Vec<u32> {
    program(
        VS_3_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            dcl(POSITION, 0, d(OUTPUT, 0, ALL)),
            defi(0, [4, 0, 1, 0]),
            vec![ins(op::MOV, 2), d(TEMP, 0, ALL), s(INPUT, 0, XYZW)],
            vec![ins(op::MOV, 2), d(TEMP, 1, ALL), s(INPUT, 0, XYZW)],
            vec![
                ins(op::SETP, 3) | (1 << 16),
                d(PREDICATE, 0, 0x7),
                s(TEMP, 0, XYZW),
                s(CONST, 0, XYZW),
            ],
            vec![
                ins(op::MOV, 3) | PREDICATED,
                d(TEMP, 1, ALL),
                s(PREDICATE, 0, XXXX),
                s(CONST, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 4) | PREDICATED,
                d(TEMP, 1, ALL),
                s(PREDICATE, 0, YYYY) | NOT,
                s(TEMP, 1, XYZW),
                s(CONST, 2, XYZW),
            ],
            vec![ins(op::LOOP, 2), s(LOOP, 0, XYZW), s(CONSTINT, 0, XYZW)],
            vec![ins(op::BREAKP, 1), s(PREDICATE, 0, 0xaa)],
            vec![ins(op::ENDLOOP, 0)],
            vec![ins(op::MOV, 2), d(OUTPUT, 0, ALL), s(TEMP, 1, XYZW)],
        ],
    )
}

#[test]
fn vs_3_0_predicates_and_setp() {
    let l = lowered(&predicates(), spec());
    // `setp_gt` is `c0 < r0`, ordered, written to p0 under its mask.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatCompare(c)
        if c.cmp == FloatComparison::LessThan && reads_row(&c.src0, range::FLOAT, 0)))
    );
    // Both predicated writes select on p0 (temporary 33).
    let selects = count(&l, |i| {
        matches!(i, Instruction::MovConditional(m)
        if matches!(m.src_cond, SrcOperand::Temp { regid: 33, .. }))
    });
    assert_eq!(selects, 2);
    // `breakp` leaves the loop on a condition.
    assert!(
        l.cfg
            .blocks
            .iter()
            .any(|b| b.debug_name == "break_not_taken")
    );
    metallib(&predicates(), spec());
}

/// `mova a0.x, v0.x; mov oPos, c[a0.x + 2]`, with `def c3` when `with_def`.
fn relative(with_def: bool) -> Vec<u32> {
    let mut body = vec![dcl(POSITION, 0, d(INPUT, 0, ALL))];
    if with_def {
        body.push(def(3, [7.0, 8.0, 9.0, 10.0]));
    }
    body.push(vec![ins(op::MOVA, 2), d(ADDR, 0, 0x1), s(INPUT, 0, XXXX)]);
    body.push(vec![
        ins(op::MOV, 3),
        d(RASTOUT, 0, ALL),
        s(CONST, 2, XYZW) | (1 << 13),
        s(ADDR, 0, XXXX),
    ]);
    program(VS_2_0, &body)
}

#[test]
fn relative_constant_index() {
    // With a `def`: the unclamped index compared against c3, the buffer read clamped to 0-255.
    let l = lowered(&relative(true), spec());
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::IntegerCompare(c)
        if c.cmp == IntegerComparison::Equal
            && matches!(c.src1, SrcOperand::Immediate32 { value: [3, 3, 3, 3], .. })))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::IntegerBinaryOp(b)
        if b.op == IntegerBinaryOp::IMin
            && matches!(b.src1, SrcOperand::Immediate32 { value: [255, 255, 255, 255], .. })))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::IntegerBinaryOp(b)
        if b.op == IntegerBinaryOp::IMax))
    );
    let def_value = [7.0f32, 8.0, 9.0, 10.0].map(f32::to_bits);
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::MovConditional(m)
        if matches!(m.src0, SrcOperand::Immediate32 { value, .. } if value == def_value)))
    );
    metallib(&relative(true), spec());
    // Without one: the raw index, as the MSL.
    let l = lowered(&relative(false), spec());
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::IntegerBinaryOp(b) if b.op == IntegerBinaryOp::IMin)
        ),
        0
    );
    assert!(any_source(&l, |o| matches!(
        o,
        SrcOperand::ConstantBuffer {
            regindex: OperandIndex::Temp {
                regid: 32,
                offset: 2,
                ..
            },
            ..
        }
    )));
    metallib(&relative(false), spec());
    // A `def` names its local by index whatever its register kind (`emit.rs`): a `def r3` still
    // overlays `c3`; a register defined twice is refused (the MSL's two locals collide).
    let mut other_kind = relative(true);
    let def_reg = d(CONST, 3, ALL);
    let at = other_kind
        .iter()
        .position(|t| *t == def_reg)
        .unwrap_or_else(|| panic!("def"));
    if let Some(t) = other_kind.get_mut(at) {
        *t = d(TEMP, 3, ALL);
    }
    let l = lowered(&other_kind, spec());
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::MovConditional(m)
        if matches!(m.src0, SrcOperand::Immediate32 { value, .. } if value == def_value)))
    );
    let twice = program(
        VS_2_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            def(3, [1.0; 4]),
            def(3, [2.0; 4]),
            vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(INPUT, 0, XYZW)],
        ],
    );
    assert_eq!(
        refused(&twice, spec()),
        LowerError::Unsupported {
            opcode: 81,
            what: "a constant register defined twice"
        }
    );
}

/// `if b3` (dynamic) and `if b1` (a `defb`).
fn bool_ifs() -> Vec<u32> {
    program(
        VS_2_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            vec![ins(op::DEFB, 2), d(CONSTBOOL, 1, ALL), 1],
            vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(INPUT, 0, XYZW)],
            vec![ins(op::IF, 1), s(CONSTBOOL, 3, XYZW)],
            vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(CONST, 0, XYZW)],
            vec![ins(op::ENDIF, 0)],
            vec![ins(op::IF, 1), s(CONSTBOOL, 1, XYZW)],
            vec![ins(op::MOV, 2), d(ATTROUT, 0, ALL), s(CONST, 1, XYZW)],
            vec![ins(op::ENDIF, 0)],
        ],
    )
}

#[test]
fn if_bool_constant_bit() {
    let l = lowered(&bool_ifs(), spec());
    // Bit 3 of the word: shifted right by 3, masked by 1.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::IntegerBinaryOp(b)
        if b.op == IntegerBinaryOp::UShr && reads_row(&b.src0, range::BOOL, 0)
            && matches!(b.src1, SrcOperand::Immediate32 { value: [3, 3, 3, 3], .. })))
    );
    assert_eq!(
        count(&l, |i| sources(i)
            .iter()
            .any(|o| reads_range(o, range::BOOL))),
        1
    );
    metallib(&bool_ifs(), spec());
}

/// `lit`, `dst`, `nrm`, `crs`, `sincos` and `pow` into the texture coordinates.
fn expansions() -> Vec<u32> {
    let one = |opcode: u16, out: u32, n: u8| {
        let mut t = vec![ins(opcode, 1 + u32::from(n)), d(OUTPUT, out, ALL)];
        t.extend(core::iter::repeat_n(s(INPUT, 0, XYZW), usize::from(n)));
        t
    };
    let sincos = vec![ins(op::SINCOS, 2), d(OUTPUT, 6, 0x3), s(INPUT, 0, XXXX)];
    program(
        VS_3_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            dcl(POSITION, 0, d(OUTPUT, 0, ALL)),
            dcl(TEXCOORD, 0, d(OUTPUT, 1, ALL)),
            dcl(TEXCOORD, 1, d(OUTPUT, 2, ALL)),
            dcl(TEXCOORD, 2, d(OUTPUT, 3, ALL)),
            dcl(TEXCOORD, 3, d(OUTPUT, 4, ALL)),
            dcl(TEXCOORD, 4, d(OUTPUT, 5, ALL)),
            dcl(TEXCOORD, 5, d(OUTPUT, 6, ALL)),
            vec![ins(op::MOV, 2), d(OUTPUT, 0, ALL), s(INPUT, 0, XYZW)],
            one(op::LIT, 1, 1),
            one(op::DST, 2, 2),
            one(op::NRM, 3, 1),
            one(op::CRS, 4, 2),
            vec![
                ins(op::POW, 3),
                d(OUTPUT, 5, ALL),
                s(INPUT, 0, XXXX),
                s(INPUT, 0, YYYY),
            ],
            sincos,
        ],
    )
}

#[test]
fn lit_dst_nrm_crs_sincos_pow_expansions() {
    let l = lowered(&expansions(), spec());
    // `lit` and `pow`: `exp2(e * log2(b))` twice.
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::FloatUnaryOp(u) if u.op == FloatUnaryOp::Log2)
        ),
        2
    );
    assert_eq!(
        count(
            &l,
            |i| matches!(i, Instruction::FloatUnaryOp(u) if u.op == FloatUnaryOp::Exp2)
        ),
        2
    );
    // `lit`'s exponent clamped to ±127.9961.
    let clamp = 127.9961f32.to_bits();
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Min
            && matches!(b.src1, SrcOperand::Immediate32 { value, .. } if value == [clamp; 4])))
    );
    // `nrm`: a dp3, an rsqrt, the zero-length select.
    assert!(instructions(&l).any(|i| matches!(i, Instruction::DotProduct(d) if d.dimension == 3)));
    assert!(
        instructions(&l)
            .any(|i| matches!(i, Instruction::FloatUnaryOp(u) if u.op == FloatUnaryOp::Rsq))
    );
    // `crs`: a fused multiply-add of the rotated operands; `sincos` once.
    assert!(instructions(&l).any(|i| matches!(i, Instruction::FloatMad(_))));
    assert_eq!(count(&l, |i| matches!(i, Instruction::SinCos(_))), 1);
    metallib(&expansions(), spec());
}

/// `dcl_position v0; dcl_normal v1; mov oPos, v0; mov oT0, v1` (`vs_2_0`).
fn two_inputs() -> Vec<u32> {
    program(
        VS_2_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            dcl(3, 0, d(INPUT, 1, ALL)),
            vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(INPUT, 0, XYZW)],
            vec![ins(op::MOV, 2), d(OUTPUT, 0, ALL), s(INPUT, 1, XYZW)],
        ],
    )
}

#[test]
fn absent_input_reads_zero() {
    let only_v0 = VsSpec {
        provided_input_mask: 0b1,
        ..VsSpec::default()
    };
    let l = lowered(&two_inputs(), only_v0);
    let inputs: Vec<u32> =
        l.io.signature_handlers
            .iter()
            .filter_map(|h| match h {
                SignatureHandler::VertexInput { reg, .. } => Some(*reg),
                _ => None,
            })
            .collect();
    assert_eq!(inputs, [0]);
    assert!(!any_source(&l, |o| matches!(
        o,
        SrcOperand::Input { regid: 1, .. }
    )));
    assert!(any_source(&l, |o| matches!(
        o,
        SrcOperand::Input { regid: 0, .. }
    )));
    metallib(&two_inputs(), only_v0);
}

#[test]
fn clip_planes_from_vs_draw() {
    let two = VsSpec {
        clip_plane_count: 2,
        ..spec()
    };
    let l = lowered(&m4x4(), two);
    // One dp4 of the position with each `VsDraw` clip row (6, 7).
    for row in [6, 7] {
        assert!(
            instructions(&l).any(|i| matches!(i, Instruction::DotProduct(d)
            if d.dimension == 4 && reads_row(&d.src1, range::VS_DRAW, row)))
        );
    }
    assert_eq!(l.io.clip_distance_scalars.len(), 2);
    metallib(&m4x4(), two);
    // A count past D3D9's six planes is clamped, never indexed past `VsDraw`.
    let many = VsSpec {
        clip_plane_count: 9,
        ..spec()
    };
    let l = lowered(&m4x4(), many);
    assert_eq!(l.io.clip_distance_scalars.len(), 6);
    assert!(!any_source(&l, |o| reads_row(o, range::VS_DRAW, 12)));
    metallib(&m4x4(), many);
}

/// `mov oPos, v0; mov oPts.x, c0.x` (`vs_2_0`).
fn point_size() -> Vec<u32> {
    program(
        VS_2_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(INPUT, 0, XYZW)],
            vec![ins(op::MOV, 2), d(RASTOUT, 2, 0x1), s(CONST, 0, XXXX)],
        ],
    )
}

#[test]
fn point_size_clamped_by_vs_draw() {
    let l = lowered(&point_size(), spec());
    // `clamp(size, point.y, point.z) * pos_fixup.w`.
    let lane_of = |o: &SrcOperand, range: u32, c: u8| {
        reads_row(o, range, 0)
            && matches!(o, SrcOperand::ConstantBuffer { common, .. } if common.swizzle.0 == [c; 4])
    };
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Max && lane_of(&b.src1, range::VS_DRAW, 1)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Min && lane_of(&b.src1, range::VS_DRAW, 2)))
    );
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatBinaryOp(b)
        if b.op == FloatBinaryOp::Mul && lane_of(&b.src1, range::POS_FIXUP, 3)))
    );
    assert!(
        pops(&l)
            .iter()
            .any(|(_, o)| matches!(o, FunctionOutput::PointSize))
    );
    metallib(&point_size(), spec());
}

/// `mov oPos, v0`, then `mov oFog, c0.x` when `writes`.
fn fog(writes: bool) -> Vec<u32> {
    let mut body = vec![
        dcl(POSITION, 0, d(INPUT, 0, ALL)),
        vec![ins(op::MOV, 2), d(RASTOUT, 0, ALL), s(INPUT, 0, XYZW)],
        vec![ins(op::MOV, 2), d(ATTROUT, 1, ALL), s(CONST, 1, XYZW)],
    ];
    if writes {
        body.push(vec![ins(op::MOV, 2), d(RASTOUT, 1, 0x1), s(CONST, 0, XXXX)]);
    }
    program(VS_2_0, &body)
}

#[test]
fn fog_output_and_default() {
    // The fog member reads the specular alpha only when the shader writes no fog.
    let fallback = |l: &Lowered| {
        instructions(l).any(|i| matches!(i, Instruction::Mov(m)
            if matches!(m.src, SrcOperand::Temp { ref common, .. } if common.swizzle.0 == [3; 4])
                && matches!(m.dst, airconv_ir::ir::DstOperand::Temp { common: ref c, .. } if c.mask == 0xf)))
    };
    let silent = lowered(&fog(false), spec());
    assert!(fallback(&silent));
    let writes = lowered(&fog(true), spec());
    assert!(!fallback(&writes));
    // The fog varying is `FOG0`'s.
    assert!(
        pops(&writes)
            .iter()
            .any(|(_, o)| matches!(o, FunctionOutput::Vertex { user, .. }
        if user == "d3d9_176"))
    );
    metallib(&fog(false), spec());
    metallib(&fog(true), spec());
    // SM3: `FOG0` on lane y of a register `TEXCOORD0` shares, written through its staging mirror.
    let staged = program(
        VS_3_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            dcl(POSITION, 0, d(OUTPUT, 0, ALL)),
            dcl(TEXCOORD, 0, d(OUTPUT, 1, 0x1)),
            dcl(FOG, 0, d(OUTPUT, 1, 0x2)),
            vec![ins(op::MOV, 2), d(OUTPUT, 0, ALL), s(INPUT, 0, XYZW)],
            vec![ins(op::MOV, 2), d(OUTPUT, 1, 0x3), s(CONST, 0, XYZW)],
        ],
    );
    let l = lowered(&staged, spec());
    assert!(!fallback(&l));
    // The fog member takes the staged lane y in every lane.
    assert!(instructions(&l).any(|i| matches!(i, Instruction::Mov(m)
        if matches!(m.src, SrcOperand::Temp { ref common, .. } if common.swizzle.0 == [1; 4])
            && matches!(m.dst, airconv_ir::ir::DstOperand::Temp { common: ref c, .. } if c.mask == 0xf))));
    metallib(&staged, spec());
}

#[test]
fn pos_fixup_half_pixel_and_depth_bias() {
    let l = lowered(&point_size(), spec());
    // `pos.x += fixup.x * pos.w`, `pos.y += fixup.y * pos.w`, fused.
    for c in [0, 1] {
        assert!(instructions(&l).any(|i| matches!(i, Instruction::FloatMad(m)
            if reads_row(&m.src0, range::POS_FIXUP, 0)
                && matches!(&m.src0, SrcOperand::ConstantBuffer { common, .. } if common.swizzle.0 == [c; 4]))));
    }
    // The depth bias, row 1's first word, read alone.
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::FloatMad(m)
        if reads_row(&m.src0, range::POS_FIXUP, 1)))
    );
    // `fog_z` is `z / w` before the bias.
    assert!(
        instructions(&l)
            .any(|i| matches!(i, Instruction::FloatBinaryOp(b) if b.op == FloatBinaryOp::Div))
    );
    metallib(&point_size(), spec());
}

/// `texldl` from a 2D slot 0 and a cube slot 1 (`vs_3_0`).
fn texldl() -> Vec<u32> {
    program(
        VS_3_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            dcl(POSITION, 0, d(OUTPUT, 0, ALL)),
            dcl(TEXCOORD, 0, d(OUTPUT, 1, ALL)),
            dcl_sampler(0, 2),
            dcl_sampler(1, 2),
            vec![ins(op::MOV, 2), d(OUTPUT, 0, ALL), s(INPUT, 0, XYZW)],
            vec![
                ins(op::TEXLDL, 3),
                d(TEMP, 0, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 0, XYZW),
            ],
            vec![
                ins(op::TEXLDL, 3),
                d(TEMP, 1, ALL),
                s(INPUT, 0, XYZW),
                s(SAMPLER, 1, XYZW),
            ],
            vec![
                ins(op::ADD, 3),
                d(OUTPUT, 1, ALL),
                s(TEMP, 0, XYZW),
                s(TEMP, 1, XYZW),
            ],
        ],
    )
}

#[test]
fn texldl_vertex_sampler_kinds() {
    let kinds = VsSpec {
        sampler_kinds: VsSamplerKinds {
            volume_mask: 0,
            cube_mask: 0b10,
            lod_table: true,
        },
        ..spec()
    };
    let program = parse(&texldl()).unwrap_or_else(|e| panic!("{e:?}"));
    let b = lower::vs_bindings(&program, &kinds);
    assert_eq!(
        b.samplers.get(&1),
        Some(&airconv_ir::dxso::TextureType::TextureCube),
        "the bound kind wins over the dcl"
    );
    let l = lowered(&texldl(), kinds);
    assert_eq!(count(&l, |i| matches!(i, Instruction::SampleLod(_))), 2);
    // Under the LOD table: `max(w + lod[s].x, lod[s].y)`, slot 1 in row 0's z/w.
    assert!(any_source(&l, |o| reads_row(o, range::VS_LOD, 0)
        && matches!(o, SrcOperand::ConstantBuffer { common, .. } if common.swizzle.0 == [3; 4])));
    metallib(&texldl(), kinds);
    // Without it, the level is the coordinate's `.w` and no table is read.
    let l = lowered(&texldl(), spec());
    assert!(!any_source(&l, |o| reads_range(o, range::VS_LOD)));
    metallib(&texldl(), spec());
    // `texld`, `texldp`, `texldb` and `texldd` compile in a vertex function as the MSL's
    // `sample(samp, coord.xy)` (bias 0), `(coord / coord.w).xy`, `bias(coord.w)` and
    // `gradient2d(ddx.xy, ddy.xy)`, which Metal accepts there.
    for control in [0, 1, 2] {
        let t = textured(vec![
            ins(op::TEXLD, 3) | (control << 16),
            d(TEMP, 0, ALL),
            s(INPUT, 0, XYZW),
            s(SAMPLER, 0, XYZW),
        ]);
        let l = lowered(&t, spec());
        let sample = count(&l, |i| matches!(i, Instruction::Sample(_)));
        let biased = count(&l, |i| {
            matches!(i, Instruction::SampleBias(b)
            if matches!(&b.src_bias, SrcOperand::Input { common, .. } if common.swizzle.0 == [3; 4]))
        });
        let projects = count(&l, |i| {
            matches!(i, Instruction::FloatBinaryOp(b)
            if b.op == FloatBinaryOp::Div
                && matches!(&b.src1, SrcOperand::Input { common, .. } if common.swizzle.0 == [3; 4]))
        });
        assert_eq!(
            (sample, biased, projects),
            match control {
                0 => (1, 0, 0),
                1 => (1, 0, 1),
                _ => (0, 1, 0),
            }
        );
        metallib(&t, spec());
    }
    let t = textured(vec![
        ins(op::TEXLDD, 5),
        d(TEMP, 0, ALL),
        s(INPUT, 0, XYZW),
        s(SAMPLER, 0, XYZW),
        s(CONST, 1, XYZW),
        s(CONST, 2, XYZW),
    ]);
    let l = lowered(&t, spec());
    assert!(
        instructions(&l).any(|i| matches!(i, Instruction::SampleDerivative(g)
        if reads_row(&g.src_x_derivative, range::FLOAT, 1)
            && reads_row(&g.src_y_derivative, range::FLOAT, 2)))
    );
    metallib(&t, spec());
}

/// A `vs_3_0` sampling slot 0 with `sample` into `r0`, written to `o1`.
fn textured(sample: Vec<u32>) -> Vec<u32> {
    program(
        VS_3_0,
        &[
            dcl(POSITION, 0, d(INPUT, 0, ALL)),
            dcl(POSITION, 0, d(OUTPUT, 0, ALL)),
            dcl(TEXCOORD, 0, d(OUTPUT, 1, ALL)),
            dcl_sampler(0, 2),
            vec![ins(op::MOV, 2), d(OUTPUT, 0, ALL), s(INPUT, 0, XYZW)],
            sample,
            vec![ins(op::MOV, 2), d(OUTPUT, 1, ALL), s(TEMP, 0, XYZW)],
        ],
    )
}

/// The buffers `bindings::vertex` declares for a program and spec, by slot.
fn buffer_slots(tokens: &[u32], spec: VsSpec) -> Vec<u32> {
    let program = parse(tokens).unwrap_or_else(|e| panic!("{e:?}"));
    let mut desc = FunctionSignatureDesc::default();
    let _ = bindings::vertex(&lower::vs_bindings(&program, &spec), &mut desc);
    desc.inputs()
        .iter()
        .filter_map(|i| match i {
            FunctionInput::Buffer(b) => Some(b.location_index),
            _ => None,
        })
        .collect()
}

#[test]
fn vs_buffers_only_under_their_condition() {
    // c#, PosFixup and VsDraw always; nothing else for a plain shader.
    assert_eq!(buffer_slots(&m4x4(), spec()), [30, 28, 27]);
    // A dynamic `i#` (29) and `b#` (26).
    assert_eq!(buffer_slots(&loop_rep_call(), spec()), [30, 28, 27, 29]);
    assert_eq!(buffer_slots(&bool_ifs(), spec()), [30, 28, 27, 26]);
    // The LOD table (25) only with samplers and the spec's flag.
    let lod = VsSpec {
        sampler_kinds: VsSamplerKinds {
            lod_table: true,
            ..VsSamplerKinds::default()
        },
        ..spec()
    };
    assert_eq!(buffer_slots(&texldl(), lod), [30, 28, 27, 25]);
    assert_eq!(buffer_slots(&texldl(), spec()), [30, 28, 27]);
    assert_eq!(buffer_slots(&m4x4(), lod), [30, 28, 27]);
}

#[test]
fn nonfinite_position_is_not_sanitized() {
    let l = lowered(&m4x4(), spec());
    // The position pops raw (`PopOutput`), never through `PopPosition`'s sanitize.
    assert!(
        !l.io
            .signature_handlers
            .iter()
            .any(|h| matches!(h, SignatureHandler::PopPosition { .. }))
    );
    assert!(l.io.signature_handlers.iter().any(|h| matches!(
        h,
        SignatureHandler::PopOutput {
            reg: 0,
            index: 0,
            ..
        }
    )));
    // And the block ends in a return to the epilogue that pops it.
    assert!(l.cfg.blocks.iter().any(|b| b.target == Target::Return));
    metallib(&m4x4(), spec());
}

mod ps;
