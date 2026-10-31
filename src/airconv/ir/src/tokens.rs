// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License (MIT).

//! The tokenized program format: its constants, and the parser that reads its instructions.
//!
//! Ported from `libs/DXBCParser/d3d12tokenizedprogramformat.hpp` and `CShaderCodeParser` of
//! `libs/DXBCParser/ShaderBinary.cpp` (Microsoft, MIT licence). The parser keeps the C++'s
//! memory: an instruction starts cleared, an operand's write mask and swizzle share one word, and
//! the walk moves on by each instruction's length token whatever its operands took. What the C++
//! trusted is checked: every token read stays inside the program's part, the declared length
//! fits the part, and an unknown opcode, an instruction of length 0, an invalid component
//! selection or relative indices nested too deep are errors.

use crate::container::{Error, read_u32};

/// `D3D10_NAME`: the system value a signature blob stores for a parameter.
pub mod name {
    pub const UNDEFINED: u32 = 0;
    pub const FINAL_QUAD_EDGE_TESSFACTOR: u32 = 11;
    pub const FINAL_QUAD_INSIDE_TESSFACTOR: u32 = 12;
    pub const FINAL_TRI_EDGE_TESSFACTOR: u32 = 13;
    pub const FINAL_TRI_INSIDE_TESSFACTOR: u32 = 14;
    pub const FINAL_LINE_DETAIL_TESSFACTOR: u32 = 15;
    pub const FINAL_LINE_DENSITY_TESSFACTOR: u32 = 16;
    pub const TARGET: u32 = 64;
    pub const DEPTH: u32 = 65;
    pub const COVERAGE: u32 = 66;
}

/// `D3D10_SB_NAME`: the system value the shader's code names.
///
/// Values 0 to 10 are the same as [`name`]'s; the tessellation factors, which the blob stores
/// once per kind, are told apart by edge here.
pub mod sb_name {
    pub const UNDEFINED: u32 = 0;
    pub const FINAL_QUAD_U_EQ_0_EDGE_TESSFACTOR: u32 = 11;
    pub const FINAL_QUAD_V_EQ_0_EDGE_TESSFACTOR: u32 = 12;
    pub const FINAL_QUAD_U_EQ_1_EDGE_TESSFACTOR: u32 = 13;
    pub const FINAL_QUAD_V_EQ_1_EDGE_TESSFACTOR: u32 = 14;
    pub const FINAL_QUAD_U_INSIDE_TESSFACTOR: u32 = 15;
    pub const FINAL_QUAD_V_INSIDE_TESSFACTOR: u32 = 16;
    pub const FINAL_TRI_U_EQ_0_EDGE_TESSFACTOR: u32 = 17;
    pub const FINAL_TRI_V_EQ_0_EDGE_TESSFACTOR: u32 = 18;
    pub const FINAL_TRI_W_EQ_0_EDGE_TESSFACTOR: u32 = 19;
    pub const FINAL_TRI_INSIDE_TESSFACTOR: u32 = 20;
    pub const FINAL_LINE_DETAIL_TESSFACTOR: u32 = 21;
    pub const FINAL_LINE_DENSITY_TESSFACTOR: u32 = 22;
}

/// `D3D10_SB_TOKENIZED_PROGRAM_TYPE`: the stage, the high half of a program's first token.
pub mod program_type {
    pub const PIXEL: u32 = 0;
    pub const VERTEX: u32 = 1;
    pub const GEOMETRY: u32 = 2;
    pub const HULL: u32 = 3;
    pub const DOMAIN: u32 = 4;
    pub const COMPUTE: u32 = 5;
}

/// `D3D10_SB_OPCODE_TYPE`: an instruction's operation, the low 11 bits of its first token.
pub mod opcode {
    pub const ADD: u32 = 0;
    pub const AND: u32 = 1;
    pub const BREAK: u32 = 2;
    pub const BREAKC: u32 = 3;
    pub const CALL: u32 = 4;
    pub const CALLC: u32 = 5;
    pub const CASE: u32 = 6;
    pub const CONTINUE: u32 = 7;
    pub const CONTINUEC: u32 = 8;
    pub const CUT: u32 = 9;
    pub const DEFAULT: u32 = 10;
    pub const DERIV_RTX: u32 = 11;
    pub const DERIV_RTY: u32 = 12;
    pub const DISCARD: u32 = 13;
    pub const DIV: u32 = 14;
    pub const DP2: u32 = 15;
    pub const DP3: u32 = 16;
    pub const DP4: u32 = 17;
    pub const ELSE: u32 = 18;
    pub const EMIT: u32 = 19;
    pub const EMITTHENCUT: u32 = 20;
    pub const ENDIF: u32 = 21;
    pub const ENDLOOP: u32 = 22;
    pub const ENDSWITCH: u32 = 23;
    pub const EQ: u32 = 24;
    pub const EXP: u32 = 25;
    pub const FRC: u32 = 26;
    pub const FTOI: u32 = 27;
    pub const FTOU: u32 = 28;
    pub const GE: u32 = 29;
    pub const IADD: u32 = 30;
    pub const IF: u32 = 31;
    pub const IEQ: u32 = 32;
    pub const IGE: u32 = 33;
    pub const ILT: u32 = 34;
    pub const IMAD: u32 = 35;
    pub const IMAX: u32 = 36;
    pub const IMIN: u32 = 37;
    pub const IMUL: u32 = 38;
    pub const INE: u32 = 39;
    pub const INEG: u32 = 40;
    pub const ISHL: u32 = 41;
    pub const ISHR: u32 = 42;
    pub const ITOF: u32 = 43;
    pub const LABEL: u32 = 44;
    pub const LD: u32 = 45;
    pub const LD_MS: u32 = 46;
    pub const LOG: u32 = 47;
    pub const LOOP: u32 = 48;
    pub const LT: u32 = 49;
    pub const MAD: u32 = 50;
    pub const MIN: u32 = 51;
    pub const MAX: u32 = 52;
    pub const CUSTOMDATA: u32 = 53;
    pub const MOV: u32 = 54;
    pub const MOVC: u32 = 55;
    pub const MUL: u32 = 56;
    pub const NE: u32 = 57;
    pub const NOP: u32 = 58;
    pub const NOT: u32 = 59;
    pub const OR: u32 = 60;
    pub const RESINFO: u32 = 61;
    pub const RET: u32 = 62;
    pub const RETC: u32 = 63;
    pub const ROUND_NE: u32 = 64;
    pub const ROUND_NI: u32 = 65;
    pub const ROUND_PI: u32 = 66;
    pub const ROUND_Z: u32 = 67;
    pub const RSQ: u32 = 68;
    pub const SAMPLE: u32 = 69;
    pub const SAMPLE_C: u32 = 70;
    pub const SAMPLE_C_LZ: u32 = 71;
    pub const SAMPLE_L: u32 = 72;
    pub const SAMPLE_D: u32 = 73;
    pub const SAMPLE_B: u32 = 74;
    pub const SQRT: u32 = 75;
    pub const SWITCH: u32 = 76;
    pub const SINCOS: u32 = 77;
    pub const UDIV: u32 = 78;
    pub const ULT: u32 = 79;
    pub const UGE: u32 = 80;
    pub const UMUL: u32 = 81;
    pub const UMAD: u32 = 82;
    pub const UMAX: u32 = 83;
    pub const UMIN: u32 = 84;
    pub const USHR: u32 = 85;
    pub const UTOF: u32 = 86;
    pub const XOR: u32 = 87;
    pub const DCL_RESOURCE: u32 = 88;
    pub const DCL_CONSTANT_BUFFER: u32 = 89;
    pub const DCL_SAMPLER: u32 = 90;
    pub const DCL_INDEX_RANGE: u32 = 91;
    pub const DCL_GS_OUTPUT_PRIMITIVE_TOPOLOGY: u32 = 92;
    pub const DCL_GS_INPUT_PRIMITIVE: u32 = 93;
    pub const DCL_MAX_OUTPUT_VERTEX_COUNT: u32 = 94;
    pub const DCL_INPUT: u32 = 95;
    pub const DCL_INPUT_SGV: u32 = 96;
    pub const DCL_INPUT_SIV: u32 = 97;
    pub const DCL_INPUT_PS: u32 = 98;
    pub const DCL_INPUT_PS_SGV: u32 = 99;
    pub const DCL_INPUT_PS_SIV: u32 = 100;
    pub const DCL_OUTPUT: u32 = 101;
    pub const DCL_OUTPUT_SGV: u32 = 102;
    pub const DCL_OUTPUT_SIV: u32 = 103;
    pub const DCL_TEMPS: u32 = 104;
    pub const DCL_INDEXABLE_TEMP: u32 = 105;
    pub const DCL_GLOBAL_FLAGS: u32 = 106;
    pub const RESERVED0: u32 = 107;
    pub const LOD: u32 = 108;
    pub const GATHER4: u32 = 109;
    pub const SAMPLE_POS: u32 = 110;
    pub const SAMPLE_INFO: u32 = 111;
    pub const RESERVED1: u32 = 112;
    pub const HS_DECLS: u32 = 113;
    pub const HS_CONTROL_POINT_PHASE: u32 = 114;
    pub const HS_FORK_PHASE: u32 = 115;
    pub const HS_JOIN_PHASE: u32 = 116;
    pub const EMIT_STREAM: u32 = 117;
    pub const CUT_STREAM: u32 = 118;
    pub const EMITTHENCUT_STREAM: u32 = 119;
    pub const INTERFACE_CALL: u32 = 120;
    pub const BUFINFO: u32 = 121;
    pub const DERIV_RTX_COARSE: u32 = 122;
    pub const DERIV_RTX_FINE: u32 = 123;
    pub const DERIV_RTY_COARSE: u32 = 124;
    pub const DERIV_RTY_FINE: u32 = 125;
    pub const GATHER4_C: u32 = 126;
    pub const GATHER4_PO: u32 = 127;
    pub const GATHER4_PO_C: u32 = 128;
    pub const RCP: u32 = 129;
    pub const F32TOF16: u32 = 130;
    pub const F16TOF32: u32 = 131;
    pub const UADDC: u32 = 132;
    pub const USUBB: u32 = 133;
    pub const COUNTBITS: u32 = 134;
    pub const FIRSTBIT_HI: u32 = 135;
    pub const FIRSTBIT_LO: u32 = 136;
    pub const FIRSTBIT_SHI: u32 = 137;
    pub const UBFE: u32 = 138;
    pub const IBFE: u32 = 139;
    pub const BFI: u32 = 140;
    pub const BFREV: u32 = 141;
    pub const SWAPC: u32 = 142;
    pub const DCL_STREAM: u32 = 143;
    pub const DCL_FUNCTION_BODY: u32 = 144;
    pub const DCL_FUNCTION_TABLE: u32 = 145;
    pub const DCL_INTERFACE: u32 = 146;
    pub const DCL_INPUT_CONTROL_POINT_COUNT: u32 = 147;
    pub const DCL_OUTPUT_CONTROL_POINT_COUNT: u32 = 148;
    pub const DCL_TESS_DOMAIN: u32 = 149;
    pub const DCL_TESS_PARTITIONING: u32 = 150;
    pub const DCL_TESS_OUTPUT_PRIMITIVE: u32 = 151;
    pub const DCL_HS_MAX_TESSFACTOR: u32 = 152;
    pub const DCL_HS_FORK_PHASE_INSTANCE_COUNT: u32 = 153;
    pub const DCL_HS_JOIN_PHASE_INSTANCE_COUNT: u32 = 154;
    pub const DCL_THREAD_GROUP: u32 = 155;
    pub const DCL_UNORDERED_ACCESS_VIEW_TYPED: u32 = 156;
    pub const DCL_UNORDERED_ACCESS_VIEW_RAW: u32 = 157;
    pub const DCL_UNORDERED_ACCESS_VIEW_STRUCTURED: u32 = 158;
    pub const DCL_THREAD_GROUP_SHARED_MEMORY_RAW: u32 = 159;
    pub const DCL_THREAD_GROUP_SHARED_MEMORY_STRUCTURED: u32 = 160;
    pub const DCL_RESOURCE_RAW: u32 = 161;
    pub const DCL_RESOURCE_STRUCTURED: u32 = 162;
    pub const LD_UAV_TYPED: u32 = 163;
    pub const STORE_UAV_TYPED: u32 = 164;
    pub const LD_RAW: u32 = 165;
    pub const STORE_RAW: u32 = 166;
    pub const LD_STRUCTURED: u32 = 167;
    pub const STORE_STRUCTURED: u32 = 168;
    pub const ATOMIC_AND: u32 = 169;
    pub const ATOMIC_OR: u32 = 170;
    pub const ATOMIC_XOR: u32 = 171;
    pub const ATOMIC_CMP_STORE: u32 = 172;
    pub const ATOMIC_IADD: u32 = 173;
    pub const ATOMIC_IMAX: u32 = 174;
    pub const ATOMIC_IMIN: u32 = 175;
    pub const ATOMIC_UMAX: u32 = 176;
    pub const ATOMIC_UMIN: u32 = 177;
    pub const IMM_ATOMIC_ALLOC: u32 = 178;
    pub const IMM_ATOMIC_CONSUME: u32 = 179;
    pub const IMM_ATOMIC_IADD: u32 = 180;
    pub const IMM_ATOMIC_AND: u32 = 181;
    pub const IMM_ATOMIC_OR: u32 = 182;
    pub const IMM_ATOMIC_XOR: u32 = 183;
    pub const IMM_ATOMIC_EXCH: u32 = 184;
    pub const IMM_ATOMIC_CMP_EXCH: u32 = 185;
    pub const IMM_ATOMIC_IMAX: u32 = 186;
    pub const IMM_ATOMIC_IMIN: u32 = 187;
    pub const IMM_ATOMIC_UMAX: u32 = 188;
    pub const IMM_ATOMIC_UMIN: u32 = 189;
    pub const SYNC: u32 = 190;
    pub const DADD: u32 = 191;
    pub const DMAX: u32 = 192;
    pub const DMIN: u32 = 193;
    pub const DMUL: u32 = 194;
    pub const DEQ: u32 = 195;
    pub const DGE: u32 = 196;
    pub const DLT: u32 = 197;
    pub const DNE: u32 = 198;
    pub const DMOV: u32 = 199;
    pub const DMOVC: u32 = 200;
    pub const DTOF: u32 = 201;
    pub const FTOD: u32 = 202;
    pub const EVAL_SNAPPED: u32 = 203;
    pub const EVAL_SAMPLE_INDEX: u32 = 204;
    pub const EVAL_CENTROID: u32 = 205;
    pub const DCL_GS_INSTANCE_COUNT: u32 = 206;
    pub const ABORT: u32 = 207;
    pub const DEBUG_BREAK: u32 = 208;
    pub const D3D11_RESERVED0: u32 = 209;
    pub const DDIV: u32 = 210;
    pub const DFMA: u32 = 211;
    pub const DRCP: u32 = 212;
    pub const MSAD: u32 = 213;
    pub const DTOI: u32 = 214;
    pub const DTOU: u32 = 215;
    pub const ITOD: u32 = 216;
    pub const UTOD: u32 = 217;
    pub const D3D11_1_RESERVED0: u32 = 218;
    pub const GATHER4_FEEDBACK: u32 = 219;
    pub const GATHER4_C_FEEDBACK: u32 = 220;
    pub const GATHER4_PO_FEEDBACK: u32 = 221;
    pub const GATHER4_PO_C_FEEDBACK: u32 = 222;
    pub const LD_FEEDBACK: u32 = 223;
    pub const LD_MS_FEEDBACK: u32 = 224;
    pub const LD_UAV_TYPED_FEEDBACK: u32 = 225;
    pub const LD_RAW_FEEDBACK: u32 = 226;
    pub const LD_STRUCTURED_FEEDBACK: u32 = 227;
    pub const SAMPLE_L_FEEDBACK: u32 = 228;
    pub const SAMPLE_C_LZ_FEEDBACK: u32 = 229;
    pub const SAMPLE_CLAMP_FEEDBACK: u32 = 230;
    pub const SAMPLE_B_CLAMP_FEEDBACK: u32 = 231;
    pub const SAMPLE_D_CLAMP_FEEDBACK: u32 = 232;
    pub const SAMPLE_C_CLAMP_FEEDBACK: u32 = 233;
    pub const CHECK_ACCESS_FULLY_MAPPED: u32 = 234;
    pub const D3DWDDM1_3_RESERVED0: u32 = 235;
    pub const NUM_OPCODES: u32 = 236;
}

/// `D3D10_SB_OPERAND_TYPE`: the register file or value an operand names.
pub mod operand_type {
    pub const TEMP: u32 = 0;
    pub const INPUT: u32 = 1;
    pub const OUTPUT: u32 = 2;
    pub const INDEXABLE_TEMP: u32 = 3;
    pub const IMMEDIATE32: u32 = 4;
    pub const IMMEDIATE64: u32 = 5;
    pub const SAMPLER: u32 = 6;
    pub const RESOURCE: u32 = 7;
    pub const CONSTANT_BUFFER: u32 = 8;
    pub const IMMEDIATE_CONSTANT_BUFFER: u32 = 9;
    pub const LABEL: u32 = 10;
    pub const INPUT_PRIMITIVEID: u32 = 11;
    pub const OUTPUT_DEPTH: u32 = 12;
    pub const NULL: u32 = 13;
    pub const RASTERIZER: u32 = 14;
    pub const OUTPUT_COVERAGE_MASK: u32 = 15;
    pub const STREAM: u32 = 16;
    pub const FUNCTION_BODY: u32 = 17;
    pub const FUNCTION_TABLE: u32 = 18;
    pub const INTERFACE: u32 = 19;
    pub const FUNCTION_INPUT: u32 = 20;
    pub const FUNCTION_OUTPUT: u32 = 21;
    pub const OUTPUT_CONTROL_POINT_ID: u32 = 22;
    pub const INPUT_FORK_INSTANCE_ID: u32 = 23;
    pub const INPUT_JOIN_INSTANCE_ID: u32 = 24;
    pub const INPUT_CONTROL_POINT: u32 = 25;
    pub const OUTPUT_CONTROL_POINT: u32 = 26;
    pub const INPUT_PATCH_CONSTANT: u32 = 27;
    pub const INPUT_DOMAIN_POINT: u32 = 28;
    pub const THIS_POINTER: u32 = 29;
    pub const UNORDERED_ACCESS_VIEW: u32 = 30;
    pub const THREAD_GROUP_SHARED_MEMORY: u32 = 31;
    pub const INPUT_THREAD_ID: u32 = 32;
    pub const INPUT_THREAD_GROUP_ID: u32 = 33;
    pub const INPUT_THREAD_ID_IN_GROUP: u32 = 34;
    pub const INPUT_COVERAGE_MASK: u32 = 35;
    pub const INPUT_THREAD_ID_IN_GROUP_FLATTENED: u32 = 36;
    pub const INPUT_GS_INSTANCE_ID: u32 = 37;
    pub const OUTPUT_DEPTH_GREATER_EQUAL: u32 = 38;
    pub const OUTPUT_DEPTH_LESS_EQUAL: u32 = 39;
    pub const CYCLE_COUNTER: u32 = 40;
    pub const OUTPUT_STENCIL_REF: u32 = 41;
    pub const INNER_COVERAGE: u32 = 42;
}

/// The fields of an operand token.
///
/// Its component count, selection mode, index dimension and representation, and the modifier of
/// its extended token.
pub mod operand {
    pub const ONE_COMPONENT: u32 = 1;
    pub const FOUR_COMPONENT: u32 = 2;
    pub const MASK_MODE: u32 = 0;
    pub const SWIZZLE_MODE: u32 = 1;
    pub const SELECT_1_MODE: u32 = 2;
    pub const INDEX_0D: u32 = 0;
    pub const INDEX_1D: u32 = 1;
    pub const INDEX_2D: u32 = 2;
    pub const INDEX_3D: u32 = 3;
    pub const INDEX_IMMEDIATE32: u32 = 0;
    pub const INDEX_IMMEDIATE64: u32 = 1;
    pub const INDEX_RELATIVE: u32 = 2;
    pub const INDEX_IMMEDIATE32_PLUS_RELATIVE: u32 = 3;
    pub const MODIFIER_NEG: u32 = 1;
    pub const MODIFIER_ABS: u32 = 2;
}

/// The opcode-specific values of an instruction or declaration that the readers test.
pub mod instruction {
    /// `D3D10_SB_INSTRUCTION_TEST_NONZERO`.
    pub const TEST_NONZERO: u32 = 1;
    pub const RESINFO_RETURN_RCPFLOAT: u32 = 1;
    pub const RESINFO_RETURN_UINT: u32 = 2;
    /// `D3D10_SB_INSTRUCTION_RETURN_UINT`, `sampleinfo`'s return type.
    pub const RETURN_UINT: u32 = 1;
    pub const SYNC_THREADS_IN_GROUP: u32 = 0x800;
    pub const SYNC_THREAD_GROUP_SHARED_MEMORY: u32 = 0x1000;
    pub const SYNC_UNORDERED_ACCESS_VIEW_MEMORY_GROUP: u32 = 0x2000;
    pub const SYNC_UNORDERED_ACCESS_VIEW_MEMORY_GLOBAL: u32 = 0x4000;
    pub const GLOBAL_FLAG_REFACTORING_ALLOWED: u32 = 1 << 11;
    pub const GLOBAL_FLAG_FORCE_EARLY_DEPTH_STENCIL: u32 = 1 << 13;
    pub const GLOBAL_FLAG_SKIP_OPTIMIZATION: u32 = 1 << 15;
    pub const GLOBALLY_COHERENT_ACCESS: u32 = 0x1_0000;
    pub const RASTERIZER_ORDERED_ACCESS: u32 = 0x2_0000;
    pub const CUSTOMDATA_DCL_IMMEDIATE_CONSTANT_BUFFER: u32 = 3;
}

/// `D3D10_SB_RESOURCE_DIMENSION`.
pub mod resource_dimension {
    pub const UNKNOWN: u32 = 0;
    pub const BUFFER: u32 = 1;
    pub const TEXTURE1D: u32 = 2;
    pub const TEXTURE2D: u32 = 3;
    pub const TEXTURE2DMS: u32 = 4;
    pub const TEXTURE3D: u32 = 5;
    pub const TEXTURECUBE: u32 = 6;
    pub const TEXTURE1DARRAY: u32 = 7;
    pub const TEXTURE2DARRAY: u32 = 8;
    pub const TEXTURE2DMSARRAY: u32 = 9;
    pub const TEXTURECUBEARRAY: u32 = 10;
    pub const RAW_BUFFER: u32 = 11;
    pub const STRUCTURED_BUFFER: u32 = 12;
}

/// `D3D10_SB_RESOURCE_RETURN_TYPE`.
pub mod return_type {
    pub const UNORM: u32 = 1;
    pub const SNORM: u32 = 2;
    pub const SINT: u32 = 3;
    pub const UINT: u32 = 4;
    pub const FLOAT: u32 = 5;
    pub const MIXED: u32 = 6;
    pub const DOUBLE: u32 = 7;
    pub const CONTINUED: u32 = 8;
    pub const UNUSED: u32 = 9;
}

/// The most operands an instruction holds (`D3D10_SB_MAX_INSTRUCTION_OPERANDS`).
pub const MAX_OPERANDS: usize = 8;

/// The program's header, or the length it declares, runs past its part.
pub const PROGRAM_PAST_PART: Error =
    Error::fail("Invalid DXBC bytecode: the shader program runs past its part");
/// An instruction's tokens run past the program's part.
pub const INSTRUCTION_PAST_PART: Error =
    Error::fail("Invalid DXBC bytecode: an instruction runs past its part");
/// An opcode past the format's last one (the C++ threw, which aborts the process).
pub const UNKNOWN_OPCODE: Error = Error::fail("Invalid DXBC bytecode: an unknown opcode");
/// An instruction whose length token is 0 (the C++ parsed it again forever).
pub const EMPTY_INSTRUCTION: Error =
    Error::fail("Invalid DXBC bytecode: an instruction of length 0");
/// A four-component operand whose selection mode is 3 (the C++ threw, which aborts).
pub const BAD_SELECTION: Error =
    Error::fail("Invalid DXBC bytecode: an operand's component selection is invalid");
/// Relative indices nested deeper than any compiler writes (the C++ recursed without bound).
pub const INDEX_NESTING: Error =
    Error::fail("Invalid DXBC bytecode: operand indices nest too deep");

/// One index of an operand, as `COperandIndex` holds it after parsing.
///
/// Only what the readers use: the immediate (the low word of a 64-bit one) and, for a relative
/// index, the register file, the selected component and the first two indices of the operand
/// that holds the value.
#[derive(Debug, Default)]
pub struct Index {
    pub reg: u32,
    pub rel_type: u32,
    pub rel_component: u32,
    pub rel_index: u32,
    pub rel_index1: u32,
}

/// One operand, as `COperandBase` holds it after `ParseOperand`.
///
/// What the token did not set stays 0, as the C++'s cleared instruction left it.
#[derive(Debug, Default)]
pub struct Operand {
    pub ty: u32,
    pub num_components: u32,
    pub selection: u32,
    /// `m_WriteMask` and `m_Swizzle`, one word as the C++'s union keeps them.
    pub mask_or_swizzle: u32,
    pub component: u32,
    pub modifier: u32,
    pub min_precision: u32,
    pub value: [u32; 4],
    pub index_dimension: u32,
    pub index_type: [u32; 3],
    pub index: [Index; 3],
}

/// What an instruction or declaration carries beyond its operands, by opcode.
///
/// The C++ kept these in one union that a cleared instruction left 0; each reader reads the
/// member its opcode set.
#[derive(Debug, Default)]
pub enum Payload {
    #[default]
    None,
    /// The single word the opcode carries.
    ///
    /// A test, a return type, sync or global flags, a count, a topology, a primitive, a
    /// tessellation mode, the maximum factor's bits, a temp count, a raw TGSM's size, a function
    /// body's or an interface call's index.
    Word(u32),
    /// A `dcl_input*` or `dcl_output*` system value, and a pixel shader input's interpolation.
    SystemValue { interpolation: u32, name: u32 },
    /// `dcl_resource`.
    Resource {
        dimension: u32,
        return_type: [u32; 4],
        sample_count: u32,
        space: u32,
    },
    /// `dcl_uav_typed`.
    TypedUav {
        dimension: u32,
        flags: u32,
        return_type: [u32; 4],
        space: u32,
    },
    /// `dcl_sampler`.
    Sampler { mode: u32, space: u32 },
    /// `dcl_constantbuffer`: `size` in vectors.
    ConstantBuffer {
        access_pattern: u32,
        size: u32,
        space: u32,
    },
    /// `dcl_resource_raw` (no flags) and `dcl_uav_raw`.
    Raw { flags: u32, space: u32 },
    /// `dcl_resource_structured` (no flags) and `dcl_uav_structured`.
    Structured { flags: u32, stride: u32, space: u32 },
    /// `dcl_tgsm_structured`.
    Tgsm { stride: u32, count: u32 },
    /// `dcl_indexableTemp`: `mask` is the components' mask bits in place (`0x10` is x).
    IndexableTemp {
        number: u32,
        registers: u32,
        mask: u32,
    },
    /// `dcl_thread_group`.
    ThreadGroup([u32; 3]),
    /// Custom data: its class and its words after the length token.
    CustomData { class: u32, data: Vec<u32> },
}

/// One instruction, as `CInstruction` holds it after `ParseInstruction`.
#[derive(Debug, Default)]
pub struct Instruction {
    pub opcode: u32,
    /// How many operands the format gives the opcode (0 for custom data).
    pub num_operands: u32,
    pub operands: [Operand; MAX_OPERANDS],
    pub precise_mask: u32,
    pub saturate: bool,
    pub texel_offset: [i8; 3],
    pub payload: Payload,
}

/// A program part (`SHDR` or `SHEX`) whose header and declared length fit it.
#[derive(Debug)]
pub struct Program<'a> {
    part: &'a [u8],
    version: u32,
    end: usize,
}

/// A program's instructions in order, as `CShaderCodeParser::ParseInstruction` reads them.
///
/// It stops after the first error.
#[derive(Debug)]
pub struct Instructions<'a> {
    part: &'a [u8],
    end: usize,
    sm51_plus: bool,
    at: usize,
    failed: bool,
}

impl Operand {
    /// `m_WriteMask`: the mask bits in place (`0x10` is x), not shifted down.
    #[must_use]
    pub const fn write_mask(&self) -> u32 {
        self.mask_or_swizzle
    }

    /// `m_Swizzle`: the same word's four bytes.
    #[must_use]
    pub const fn swizzle(&self) -> [u8; 4] {
        self.mask_or_swizzle.to_le_bytes()
    }
}

impl Instruction {
    /// The system value a `dcl_input*` or `dcl_output*` declares, 0 for anything else.
    ///
    /// Every declaration struct the C++'s union held for these opcodes keeps `Name` in the same
    /// place for its kind, so its readers took it from any of them.
    #[must_use]
    pub const fn name(&self) -> u32 {
        match self.payload {
            Payload::SystemValue { name, .. } => name,
            _ => 0,
        }
    }

    /// A pixel shader input's interpolation mode, 0 for anything else.
    #[must_use]
    pub const fn interpolation(&self) -> u32 {
        match self.payload {
            Payload::SystemValue { interpolation, .. } => interpolation,
            _ => 0,
        }
    }

    /// The opcode's single word ([`Payload::Word`]), 0 for anything else.
    #[must_use]
    pub const fn word(&self) -> u32 {
        match self.payload {
            Payload::Word(word) => word,
            _ => 0,
        }
    }
}

impl<'a> Program<'a> {
    /// Reads the program header of a part and checks that the length it declares fits.
    ///
    /// The C++ trusted the length token; a shorter declared length is kept (the C++ read only
    /// that far).
    ///
    /// # Errors
    ///
    /// [`PROGRAM_PAST_PART`] when the header or the declared length runs past the part.
    pub fn new(part: &'a [u8]) -> Result<Self, Error> {
        let version = read_u32(part, 0).ok_or(PROGRAM_PAST_PART)?;
        let length = read_u32(part, 4).ok_or(PROGRAM_PAST_PART)?;
        let end = to_usize(length);
        if end.checked_mul(4).is_none_or(|bytes| bytes > part.len()) {
            return Err(PROGRAM_PAST_PART);
        }
        Ok(Self { part, version, end })
    }

    /// The stage ([`program_type`]).
    #[must_use]
    pub const fn program_type(&self) -> u32 {
        self.version >> 16
    }

    #[must_use]
    pub const fn major_version(&self) -> u32 {
        (self.version & 0xf0) >> 4
    }

    #[must_use]
    pub const fn minor_version(&self) -> u32 {
        self.version & 0xf
    }

    /// The instructions, from the first token after the header to the declared length.
    #[must_use]
    pub const fn instructions(&self) -> Instructions<'a> {
        let major = self.major_version();
        Instructions {
            part: self.part,
            end: self.end,
            sm51_plus: major > 5 || (major == 5 && self.minor_version() > 0),
            at: 2,
            failed: false,
        }
    }
}

impl Iterator for Instructions<'_> {
    type Item = Result<Instruction, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed || self.at >= self.end {
            return None;
        }
        let instruction = self.instruction();
        self.failed = instruction.is_err();
        Some(instruction)
    }
}

impl Instructions<'_> {
    fn instruction(&mut self) -> Result<Instruction, Error> {
        let start = self.at;
        let token = self.next_token()?;
        let opcode = token & 0x7ff;
        let num_operands = *NUM_OPERANDS.get(to_usize(opcode)).ok_or(UNKNOWN_OPCODE)?;
        let mut inst = Instruction {
            opcode,
            num_operands: u32::from(num_operands),
            precise_mask: (token & 0x0078_0000) >> 19,
            saturate: token & 0x2000 != 0,
            ..Instruction::default()
        };
        let mut length = (token & 0x7f00_0000) >> 24;
        let extended = token & EXTENDED != 0;
        if extended && (opcode == opcode::DCL_INTERFACE || opcode == opcode::DCL_FUNCTION_TABLE) {
            // These may be longer than the length field holds.
            length = self.next_token()?;
        } else if extended {
            for _ in 0..MAX_EXTENDED_OPCODES {
                let ext = self.next_token()?;
                if ext & 0x3f == EXTENDED_OPCODE_SAMPLE_CONTROLS {
                    for (offset, shift) in inst.texel_offset.iter_mut().zip([9, 13, 17]) {
                        // A 4-bit two's complement value.
                        let nibble = u8::try_from((ext >> shift) & 0xf).unwrap_or(0);
                        *offset = (nibble << 4).cast_signed() >> 4;
                    }
                }
                if ext & EXTENDED == 0 {
                    break;
                }
            }
        }
        self.payload(token, &mut inst, &mut length)?;
        if length == 0 {
            return Err(EMPTY_INSTRUCTION);
        }
        // The next instruction starts where the length says, whatever the operands took.
        self.at = start
            .checked_add(to_usize(length))
            .ok_or(INSTRUCTION_PAST_PART)?;
        Ok(inst)
    }

    /// The operands and the payload, by opcode (`ParseInstruction`'s switch).
    fn payload(
        &mut self,
        token: u32,
        inst: &mut Instruction,
        length: &mut u32,
    ) -> Result<(), Error> {
        let [op0, op1, op2, ..] = &mut inst.operands;
        inst.payload = match inst.opcode {
            opcode::CUSTOMDATA => {
                inst.precise_mask = 0;
                inst.saturate = false;
                inst.num_operands = 0;
                let class = (token & 0xffff_f800) >> 11;
                let words = self.token(self.at)?;
                let data = if words < 2 {
                    *length = 2;
                    Vec::new()
                } else {
                    *length = words;
                    let bytes = self.span(self.at + 1, to_usize(words - 2))?;
                    bytes
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|&word| u32::from_le_bytes(word))
                        .collect()
                };
                Payload::CustomData { class, data }
            }
            opcode::DCL_FUNCTION_BODY
            | opcode::DCL_TEMPS
            | opcode::DCL_MAX_OUTPUT_VERTEX_COUNT
            | opcode::DCL_GS_INSTANCE_COUNT
            | opcode::DCL_HS_MAX_TESSFACTOR
            | opcode::DCL_HS_FORK_PHASE_INSTANCE_COUNT
            | opcode::DCL_HS_JOIN_PHASE_INSTANCE_COUNT => Payload::Word(self.next_token()?),
            opcode::DCL_FUNCTION_TABLE => {
                self.next_token()?;
                let table = self.token(self.at)?;
                self.at += 1;
                // The C++ copied the table; nothing reads it, but it has to be there.
                self.span(self.at, to_usize(table))?;
                Payload::None
            }
            opcode::DCL_INTERFACE => {
                self.next_token()?;
                self.next_token()?;
                let table = self.token(self.at)? & 0xffff;
                self.at += 1;
                self.span(self.at, to_usize(table))?;
                Payload::None
            }
            opcode::INTERFACE_CALL => {
                let function = self.next_token()?;
                *op0 = self.operand(0)?;
                Payload::Word(function)
            }
            opcode::DCL_RESOURCE => {
                *op0 = self.operand(0)?;
                let return_type = return_types(self.token(self.at)?);
                self.at += 1;
                Payload::Resource {
                    dimension: (token & 0xf800) >> 11,
                    return_type,
                    sample_count: (token & 0x007f_0000) >> 16,
                    space: self.space()?,
                }
            }
            opcode::DCL_SAMPLER => {
                *op0 = self.operand(0)?;
                Payload::Sampler {
                    mode: (token & 0x7800) >> 11,
                    space: self.space()?,
                }
            }
            opcode::DCL_STREAM | opcode::DCL_INPUT | opcode::DCL_OUTPUT => {
                *op0 = self.operand(0)?;
                Payload::None
            }
            opcode::DCL_INDEXABLE_TEMP => {
                let number = self.next_token()?;
                let registers = self.next_token()?;
                let mask = match self.token(self.at)?.clamp(1, 4) {
                    1 => 0x10,
                    2 => 0x30,
                    3 => 0x70,
                    _ => 0xf0,
                };
                self.at += 1;
                Payload::IndexableTemp {
                    number,
                    registers,
                    mask,
                }
            }
            opcode::DCL_INPUT_SIV
            | opcode::DCL_INPUT_SGV
            | opcode::DCL_OUTPUT_SIV
            | opcode::DCL_OUTPUT_SGV => {
                *op0 = self.operand(0)?;
                Payload::SystemValue {
                    interpolation: 0,
                    name: self.next_token()? & 0xffff,
                }
            }
            opcode::DCL_INPUT_PS => {
                *op0 = self.operand(0)?;
                Payload::SystemValue {
                    interpolation: (token & 0x7800) >> 11,
                    name: 0,
                }
            }
            opcode::DCL_INPUT_PS_SIV | opcode::DCL_INPUT_PS_SGV => {
                *op0 = self.operand(0)?;
                Payload::SystemValue {
                    interpolation: (token & 0x7800) >> 11,
                    name: self.next_token()? & 0xffff,
                }
            }
            opcode::DCL_INDEX_RANGE | opcode::DCL_THREAD_GROUP_SHARED_MEMORY_RAW => {
                *op0 = self.operand(0)?;
                Payload::Word(self.next_token()?)
            }
            opcode::DCL_CONSTANT_BUFFER => {
                *op0 = self.operand(0)?;
                let (size, space) = if self.sm51_plus {
                    (self.next_token()?, self.next_token()?)
                } else {
                    (op0.index[1].reg, 0)
                };
                Payload::ConstantBuffer {
                    access_pattern: (token & 0x800) >> 11,
                    size,
                    space,
                }
            }
            opcode::DCL_GS_OUTPUT_PRIMITIVE_TOPOLOGY
            | opcode::DCL_GS_INPUT_PRIMITIVE
            | opcode::DCL_INPUT_CONTROL_POINT_COUNT
            | opcode::DCL_OUTPUT_CONTROL_POINT_COUNT => Payload::Word((token & 0x1_f800) >> 11),
            opcode::DCL_GLOBAL_FLAGS => Payload::Word(token & 0x00ff_f800),
            opcode::DCL_TESS_DOMAIN => Payload::Word((token & 0x1800) >> 11),
            opcode::DCL_TESS_PARTITIONING | opcode::DCL_TESS_OUTPUT_PRIMITIVE => {
                Payload::Word((token & 0x3800) >> 11)
            }
            opcode::DCL_THREAD_GROUP => {
                Payload::ThreadGroup([self.next_token()?, self.next_token()?, self.next_token()?])
            }
            opcode::DCL_UNORDERED_ACCESS_VIEW_TYPED => {
                *op0 = self.operand(0)?;
                let return_type = return_types(self.token(self.at)?);
                self.at += 1;
                Payload::TypedUav {
                    dimension: (token & 0xf800) >> 11,
                    flags: token & RESOURCE_FLAGS,
                    return_type,
                    space: self.space()?,
                }
            }
            opcode::DCL_UNORDERED_ACCESS_VIEW_RAW | opcode::DCL_RESOURCE_RAW => {
                *op0 = self.operand(0)?;
                Payload::Raw {
                    flags: if inst.opcode == opcode::DCL_RESOURCE_RAW {
                        0
                    } else {
                        token & RESOURCE_FLAGS
                    },
                    space: self.space()?,
                }
            }
            opcode::DCL_UNORDERED_ACCESS_VIEW_STRUCTURED | opcode::DCL_RESOURCE_STRUCTURED => {
                *op0 = self.operand(0)?;
                Payload::Structured {
                    flags: if inst.opcode == opcode::DCL_RESOURCE_STRUCTURED {
                        0
                    } else {
                        token & RESOURCE_FLAGS
                    },
                    stride: self.next_token()?,
                    space: self.space()?,
                }
            }
            opcode::DCL_THREAD_GROUP_SHARED_MEMORY_STRUCTURED => {
                *op0 = self.operand(0)?;
                Payload::Tgsm {
                    stride: self.next_token()?,
                    count: self.next_token()?,
                }
            }
            opcode::SYNC => Payload::Word(token & 0x7800),
            opcode::RESINFO => {
                *op0 = self.operand(0)?;
                *op1 = self.operand(0)?;
                *op2 = self.operand(0)?;
                Payload::Word((token & 0x1800) >> 11)
            }
            opcode::SAMPLE_INFO => {
                *op0 = self.operand(0)?;
                *op1 = self.operand(0)?;
                Payload::Word((token & 0x1800) >> 11)
            }
            opcode::IF | opcode::BREAKC | opcode::CONTINUEC | opcode::RETC | opcode::DISCARD => {
                *op0 = self.operand(0)?;
                Payload::Word((token & 0x4_0000) >> 18)
            }
            opcode::CALLC => {
                *op0 = self.operand(0)?;
                *op1 = self.operand(0)?;
                Payload::Word((token & 0x4_0000) >> 18)
            }
            _ => {
                let count = to_usize(inst.num_operands);
                for operand in inst.operands.iter_mut().take(count) {
                    *operand = self.operand(0)?;
                }
                Payload::None
            }
        };
        Ok(())
    }

    /// `ParseOperand`: one operand and its indices, `depth` relative indices down.
    fn operand(&mut self, depth: u32) -> Result<Operand, Error> {
        let token = self.next_token()?;
        let mut o = Operand {
            ty: (token & 0x000f_f000) >> 12,
            num_components: token & 3,
            ..Operand::default()
        };
        let immediate = o.ty == operand_type::IMMEDIATE32 || o.ty == operand_type::IMMEDIATE64;
        if !immediate {
            if o.num_components == operand::FOUR_COMPONENT {
                o.selection = (token & 0xc) >> 2;
                match o.selection {
                    operand::MASK_MODE => o.mask_or_swizzle = token & 0xf0,
                    operand::SWIZZLE_MODE => {
                        o.mask_or_swizzle = u32::from_le_bytes(
                            [4, 6, 8, 10]
                                .map(|shift| u8::try_from((token >> shift) & 3).unwrap_or(0)),
                        );
                    }
                    operand::SELECT_1_MODE => {
                        o.component = (token & 0x30) >> 4;
                        o.mask_or_swizzle = o.component * 0x0101_0101;
                    }
                    _ => return Err(BAD_SELECTION),
                }
            }
            o.index_dimension = (token & 0x0030_0000) >> 20;
            let dims = to_usize(o.index_dimension);
            for (ty, shift) in o.index_type.iter_mut().zip([22, 25, 28]).take(dims) {
                *ty = (token >> shift) & 3;
            }
        }
        if token & EXTENDED != 0 {
            let ext = self.next_token()?;
            if ext & 0x3f == EXTENDED_OPERAND_MODIFIER {
                o.modifier = (ext & 0x3fc0) >> 6;
                o.min_precision = (ext & 0x1_c000) >> 14;
            }
        }
        if immediate {
            let count = match o.num_components {
                operand::ONE_COMPONENT => 1,
                operand::FOUR_COMPONENT => 4,
                _ => 0,
            };
            for value in o.value.iter_mut().take(count) {
                *value = self.next_token()?;
            }
        }
        let dims = to_usize(o.index_dimension);
        for (index, &rep) in o.index.iter_mut().zip(&o.index_type).take(dims) {
            *index = self.index(rep, depth)?;
        }
        Ok(o)
    }

    /// `ParseIndex`: one index of an operand.
    fn index(&mut self, rep: u32, depth: u32) -> Result<Index, Error> {
        match rep {
            operand::INDEX_IMMEDIATE32 => Ok(Index {
                reg: self.next_token()?,
                rel_type: operand_type::IMMEDIATE32,
                ..Index::default()
            }),
            operand::INDEX_IMMEDIATE64 => {
                let reg = self.next_token()?;
                self.next_token()?;
                Ok(Index {
                    reg,
                    rel_type: operand_type::IMMEDIATE64,
                    ..Index::default()
                })
            }
            operand::INDEX_RELATIVE => self.relative(0, depth),
            _ => {
                let reg = self.next_token()?;
                self.relative(reg, depth)
            }
        }
    }

    /// A relative index: the operand that holds it, `reg` added.
    fn relative(&mut self, reg: u32, depth: u32) -> Result<Index, Error> {
        // ponytail: compilers nest one level (x1[r0.x]); 8 keeps the recursion's stack small,
        // where the C++ recursed as deep as the bytes went.
        if depth >= MAX_INDEX_NESTING {
            return Err(INDEX_NESTING);
        }
        let held = self.operand(depth + 1)?;
        Ok(Index {
            reg,
            rel_type: held.ty,
            rel_component: held.component,
            rel_index: held.index[0].reg,
            rel_index1: held.index[1].reg,
        })
    }

    /// The register space a 5.1 declaration carries in its next token, 0 before 5.1.
    fn space(&mut self) -> Result<u32, Error> {
        if self.sm51_plus {
            self.next_token()
        } else {
            Ok(0)
        }
    }

    fn next_token(&mut self) -> Result<u32, Error> {
        let token = self.token(self.at)?;
        self.at += 1;
        Ok(token)
    }

    fn token(&self, at: usize) -> Result<u32, Error> {
        at.checked_mul(4)
            .and_then(|byte| read_u32(self.part, byte))
            .ok_or(INSTRUCTION_PAST_PART)
    }

    /// The bytes of `count` tokens from `at`, if the part holds them.
    fn span(&self, at: usize, count: usize) -> Result<&[u8], Error> {
        let start = at.checked_mul(4).ok_or(INSTRUCTION_PAST_PART)?;
        count
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(start))
            .and_then(|end| self.part.get(start..end))
            .ok_or(INSTRUCTION_PAST_PART)
    }
}

/// A program part for `stage`, version 5.0 (5.1 with `sm51`), holding `words` after its header.
#[cfg(test)]
#[must_use]
pub fn test_part(stage: u32, sm51: bool, words: &[u32]) -> Vec<u8> {
    let length = u32::try_from(words.len() + 2).unwrap_or(u32::MAX);
    [(stage << 16) | 0x50 | u32::from(sm51), length]
        .iter()
        .chain(words)
        .flat_map(|word| word.to_le_bytes())
        .collect()
}

/// The extended bit of an opcode or operand token.
const EXTENDED: u32 = 0x8000_0000;
const MAX_EXTENDED_OPCODES: usize = 3;
const EXTENDED_OPCODE_SAMPLE_CONTROLS: u32 = 1;
const EXTENDED_OPERAND_MODIFIER: u32 = 1;
/// `D3D11_SB_RESOURCE_FLAGS_MASK`: globally coherent, rasterizer ordered, order-preserving counter.
const RESOURCE_FLAGS: u32 = 0x0083_0000;
const MAX_INDEX_NESTING: u32 = 8;

/// `g_InstructionInfo[op].m_NumOperands`: what `InitInstructionInfo` sets, 0 for the rest.
const NUM_OPERANDS: [u8; opcode::NUM_OPCODES as usize] = [
    3, 3, 0, 1, 1, 2, 1, 0, 1, 0, 0, 2, 2, 1, 3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 3, 2, 2, 2, 2, 3, 3, 1,
    3, 3, 3, 4, 3, 3, 4, 3, 2, 3, 3, 2, 1, 3, 4, 2, 0, 3, 4, 3, 3, 0, 2, 4, 3, 3, 0, 2, 3, 3, 0, 1,
    2, 2, 2, 2, 2, 4, 5, 5, 5, 6, 5, 2, 1, 3, 4, 3, 3, 4, 4, 3, 3, 3, 2, 3, 1, 1, 1, 1, 0, 0, 0, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 4, 4, 3, 2, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 2, 5, 5,
    6, 2, 2, 2, 4, 4, 2, 2, 2, 2, 4, 4, 5, 2, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1,
    1, 1, 1, 3, 3, 3, 3, 4, 4, 3, 3, 3, 4, 3, 3, 3, 3, 3, 2, 2, 4, 4, 4, 4, 4, 5, 4, 4, 4, 4, 0, 3,
    3, 3, 3, 3, 3, 3, 3, 2, 4, 2, 2, 3, 3, 2, 0, 0, 0, 0, 3, 4, 2, 4, 2, 2, 2, 2, 0, 5, 6, 6, 7, 4,
    5, 4, 4, 5, 6, 6, 6, 7, 8, 7, 2, 0,
];

/// A resource declaration's four return types, four bits each.
const fn return_types(token: u32) -> [u32; 4] {
    [
        token & 0xf,
        (token >> 4) & 0xf,
        (token >> 8) & 0xf,
        (token >> 12) & 0xf,
    ]
}

/// A token's value as an index or count; a `u32` always fits.
fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
