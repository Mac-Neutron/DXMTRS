//! The decoded DXBC program as plain data: instructions, operands and basic blocks.
//!
//! Ported from `A/dxbc_instructions.hpp`: the 50 instruction structs, the operand variants (in
//! the C++'s order) and the basic block with its terminator. Blocks refer to each other by their
//! index in the control-flow graph's list. Fields named `_` in the C++ are `common` here.

use std::collections::BTreeMap;

/// No hull shader phase: what a register outside a fork or join phase carries (the C++'s `~0u`).
pub const NO_PHASE: u32 = u32::MAX;

/// The identity swizzle `xyzw`.
pub const SWIZZLE_IDENTITY: Swizzle = Swizzle([0, 1, 2, 3]);

/// The source component each of x, y, z and w reads (0 to 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Swizzle(pub [u8; 4]);

/// A register index: an immediate, or a register's component plus an offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperandIndex {
    Immediate(u32),
    IndexableTemp {
        regfile: u32,
        regid: u32,
        phase: u32,
        component: u8,
        offset: u32,
    },
    Temp {
        regid: u32,
        phase: u32,
        component: u8,
        offset: u32,
    },
    /// The control point a hull shader runs for, plus an offset.
    ControlPointId {
        offset: u32,
    },
}

/// How an operand's value is read or written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OperandDataType {
    Float,
    Integer,
    Half16X16,
}

/// A system-generated input (`shader::common::InputAttribute`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputAttribute {
    VertexId,
    InstanceId,
    ThreadId,
    ThreadIdInGroup,
    ThreadGroupId,
    ThreadIdInGroupFlatten,
    PrimitiveId,
    CoverageMask,
    OutputControlPointId,
    ForkInstanceId,
    JoinInstanceId,
    Domain,
    GsInstanceId,
}

/// What every source operand carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrcOperandCommon {
    pub swizzle: Swizzle,
    pub abs: bool,
    pub neg: bool,
    pub read_type: OperandDataType,
}

/// A source operand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SrcOperand {
    /// Four 32-bit values (a one-component immediate is repeated), as bits.
    Immediate32 {
        common: SrcOperandCommon,
        value: [u32; 4],
    },
    Temp {
        common: SrcOperandCommon,
        regid: u32,
        phase: u32,
    },
    IndexableTemp {
        common: SrcOperandCommon,
        regfile: u32,
        regindex: OperandIndex,
        phase: u32,
    },
    Input {
        common: SrcOperandCommon,
        regid: u32,
    },
    ConstantBuffer {
        common: SrcOperandCommon,
        rangeid: u32,
        rangeindex: OperandIndex,
        regindex: OperandIndex,
    },
    ImmediateConstantBuffer {
        common: SrcOperandCommon,
        regindex: OperandIndex,
    },
    Attribute {
        common: SrcOperandCommon,
        attribute: InputAttribute,
    },
    IndexableInput {
        common: SrcOperandCommon,
        regindex: OperandIndex,
    },
    InputIcp {
        common: SrcOperandCommon,
        cpid: OperandIndex,
        regid: u32,
    },
    InputOcp {
        common: SrcOperandCommon,
        cpid: OperandIndex,
        regid: u32,
    },
    InputPc {
        common: SrcOperandCommon,
        regindex: OperandIndex,
    },
}

/// What every destination operand carries: `mask` is the written components, x in bit 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DstOperandCommon {
    pub mask: u32,
    pub write_type: OperandDataType,
}

/// A destination operand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DstOperand {
    Null {
        common: DstOperandCommon,
    },
    /// An atomic's discarded result.
    SideEffect {
        common: DstOperandCommon,
    },
    Temp {
        common: DstOperandCommon,
        regid: u32,
        phase: u32,
    },
    IndexableTemp {
        common: DstOperandCommon,
        regfile: u32,
        regindex: OperandIndex,
        phase: u32,
    },
    Output {
        common: DstOperandCommon,
        regid: u32,
        phase: u32,
    },
    OutputDepth {
        common: DstOperandCommon,
    },
    OutputCoverageMask {
        common: DstOperandCommon,
    },
    OutputStencilRef {
        common: DstOperandCommon,
    },
    IndexableOutput {
        common: DstOperandCommon,
        regindex: OperandIndex,
        phase: u32,
    },
}

/// A shader resource view read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrcOperandResource {
    pub range_id: u32,
    pub index: OperandIndex,
    pub read_swizzle: Swizzle,
}

/// A sampler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrcOperandSampler {
    pub range_id: u32,
    pub index: OperandIndex,
    pub gather_channel: u8,
}

/// An unordered access view read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrcOperandUav {
    pub range_id: u32,
    pub index: OperandIndex,
    pub read_swizzle: Swizzle,
}

/// A thread group shared memory read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SrcOperandTgsm {
    pub id: u32,
    pub read_swizzle: Swizzle,
}

/// An unordered access view written or updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomicDstOperandUav {
    pub range_id: u32,
    pub index: OperandIndex,
    pub mask: u32,
}

/// Thread group shared memory written or updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AtomicOperandTgsm {
    pub id: u32,
    pub mask: u32,
}

/// A view an info query reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceOrUav {
    Resource(SrcOperandResource),
    Uav(SrcOperandUav),
}

/// What a raw or structured load reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypelessSrc {
    Resource(SrcOperandResource),
    Uav(SrcOperandUav),
    Tgsm(SrcOperandTgsm),
}

/// What a raw or structured store or an atomic writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtomicDst {
    Uav(AtomicDstOperandUav),
    Tgsm(AtomicOperandTgsm),
}

/// What the arithmetic instructions carry.
///
/// `precise_mask` is the components that must not be refactored: the token's, or all four when
/// the shader does not allow refactoring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstructionCommon {
    pub saturate: bool,
    pub precise_mask: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstMov {
    pub common: InstructionCommon,
    pub dst: DstOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstMovConditional {
    pub common: InstructionCommon,
    pub dst: DstOperand,
    pub src_cond: SrcOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSwapConditional {
    pub dst0: DstOperand,
    pub dst1: DstOperand,
    pub src_cond: SrcOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSample {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
    pub offsets: [i32; 3],
    pub min_lod_clamp: Option<SrcOperand>,
    pub feedback: Option<DstOperand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSampleCompare {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
    pub src_reference: SrcOperand,
    pub offsets: [i32; 3],
    pub min_lod_clamp: Option<SrcOperand>,
    pub feedback: Option<DstOperand>,
    pub level_zero: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSampleBias {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
    pub src_bias: SrcOperand,
    pub offsets: [i32; 3],
    pub min_lod_clamp: Option<SrcOperand>,
    pub feedback: Option<DstOperand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSampleDerivative {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
    pub src_x_derivative: SrcOperand,
    pub src_y_derivative: SrcOperand,
    pub offsets: [i32; 3],
    pub min_lod_clamp: Option<SrcOperand>,
    pub feedback: Option<DstOperand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSampleLod {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
    pub src_lod: SrcOperand,
    pub offsets: [i32; 3],
    pub feedback: Option<DstOperand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstGather {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
    pub offset: SrcOperand,
    pub feedback: Option<DstOperand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstGatherCompare {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
    pub src_reference: SrcOperand,
    pub offset: SrcOperand,
    pub feedback: Option<DstOperand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstBufferInfo {
    pub dst: DstOperand,
    pub src: ResourceOrUav,
}

/// `resinfo`'s return type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResInfoModifier {
    None,
    Uint,
    Rcp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstResourceInfo {
    pub dst: DstOperand,
    pub src_mip_level: SrcOperand,
    pub src_resource: ResourceOrUav,
    pub modifier: ResInfoModifier,
}

/// `sampleinfo`: of a resource, or of the rasterizer when `src` is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSampleInfo {
    pub dst: DstOperand,
    pub src: Option<SrcOperandResource>,
    pub uint_result: bool,
    pub read_swizzle: Swizzle,
}

/// `samplepos`: of a resource, or of the rasterizer when `src` is `None`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSamplePos {
    pub dst: DstOperand,
    pub src: Option<SrcOperandResource>,
    pub src_sample_index: SrcOperand,
    pub read_swizzle: Swizzle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstLoad {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sample_index: Option<SrcOperand>,
    pub offsets: [i32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstLoadRaw {
    pub dst: DstOperand,
    pub src_byte_offset: SrcOperand,
    pub src: TypelessSrc,
    /// The offset is an immediate multiple of 16, so four components load at once.
    pub opt_flag_offset_is_vec4_aligned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstLoadStructured {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_byte_offset: SrcOperand,
    pub src: TypelessSrc,
    pub opt_flag_offset_is_vec4_aligned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstLoadUavTyped {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_uav: SrcOperandUav,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstStoreUavTyped {
    pub dst: AtomicDstOperandUav,
    pub src_address: SrcOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstStoreRaw {
    pub dst: AtomicDst,
    pub dst_byte_offset: SrcOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstStoreStructured {
    pub dst: AtomicDst,
    pub dst_address: SrcOperand,
    pub dst_byte_offset: SrcOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstDotProduct {
    pub common: InstructionCommon,
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
    pub dimension: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSinCos {
    pub common: InstructionCommon,
    pub dst_sin: DstOperand,
    pub dst_cos: DstOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FloatComparison {
    Equal,
    NotEqual,
    GreaterEqual,
    LessThan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstFloatCompare {
    pub common: InstructionCommon,
    pub cmp: FloatComparison,
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegerComparison {
    Equal,
    NotEqual,
    SignedLessThan,
    SignedGreaterEqual,
    UnsignedLessThan,
    UnsignedGreaterEqual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstIntegerCompare {
    pub cmp: IntegerComparison,
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FloatBinaryOp {
    Add,
    Mul,
    Div,
    Min,
    Max,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstFloatBinaryOp {
    pub common: InstructionCommon,
    pub op: FloatBinaryOp,
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FloatUnaryOp {
    Log2,
    Exp2,
    Rcp,
    Rsq,
    Sqrt,
    Fraction,
    RoundNearestEven,
    RoundNegativeInf,
    RoundPositiveInf,
    RoundZero,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstFloatUnaryOp {
    pub common: InstructionCommon,
    pub op: FloatUnaryOp,
    pub dst: DstOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegerUnaryOp {
    Neg,
    Not,
    ReverseBits,
    CountBits,
    FirstHiBitSigned,
    FirstHiBit,
    FirstLowBit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstIntegerUnaryOp {
    pub op: IntegerUnaryOp,
    pub dst: DstOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstFloatMad {
    pub common: InstructionCommon,
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
    pub src2: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstIntegerMad {
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
    pub src2: SrcOperand,
    pub is_signed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstMaskedSumOfAbsDiff {
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
    pub src2: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegerBinaryOp {
    UMin,
    UMax,
    IMin,
    IMax,
    IShl,
    IShr,
    UShr,
    Xor,
    Or,
    And,
    Add,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstIntegerBinaryOp {
    pub op: IntegerBinaryOp,
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IntegerBinaryOpWithTwoDst {
    IMul,
    UAddCarry,
    UDiv,
    UMul,
    USubBorrow,
}

/// An operation with two results: the high or quotient first, the low, remainder or carry second.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstIntegerBinaryOpWithTwoDst {
    pub common: InstructionCommon,
    pub op: IntegerBinaryOpWithTwoDst,
    pub dst_hi: DstOperand,
    pub dst_low: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstExtractBits {
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
    pub src2: SrcOperand,
    pub is_signed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstBitFieldInsert {
    pub dst: DstOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
    pub src2: SrcOperand,
    pub src3: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversionOp {
    HalfToFloat,
    FloatToHalf,
    FloatToSigned,
    SignedToFloat,
    FloatToUnsigned,
    UnsignedToFloat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstConvert {
    pub op: ConversionOp,
    pub dst: DstOperand,
    pub src: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstPartialDerivative {
    pub common: InstructionCommon,
    pub dst: DstOperand,
    pub src: SrcOperand,
    /// `ddy`, else `ddx`.
    pub ddy: bool,
    /// Coarse, else fine.
    pub coarse: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstCalcLod {
    pub dst: DstOperand,
    pub src_address: SrcOperand,
    pub src_resource: SrcOperandResource,
    pub src_sampler: SrcOperandSampler,
}

/// The memory a `sync` orders for unordered access views.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UavBoundary {
    Global,
    Group,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstSync {
    pub uav_boundary: UavBoundary,
    pub tgsm_memory_barrier: bool,
    pub tgsm_execution_barrier: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AtomicBinaryOp {
    And,
    Or,
    Xor,
    Add,
    IMax,
    IMin,
    UMax,
    UMin,
    Xchg,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstAtomicBinOp {
    pub op: AtomicBinaryOp,
    pub dst: AtomicDst,
    pub dst_address: SrcOperand,
    pub src: SrcOperand,
    pub dst_original: DstOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstAtomicImmIncrement {
    pub dst: DstOperand,
    pub uav: AtomicDstOperandUav,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstAtomicImmDecrement {
    pub dst: DstOperand,
    pub uav: AtomicDstOperandUav,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstAtomicImmCmpExchange {
    pub dst: DstOperand,
    pub dst_resource: AtomicDst,
    pub dst_address: SrcOperand,
    pub src0: SrcOperand,
    pub src1: SrcOperand,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstInterpolateCentroid {
    pub dst: DstOperand,
    pub regid: u32,
    pub read_swizzle: Swizzle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstInterpolateSample {
    pub dst: DstOperand,
    pub sample_index: SrcOperand,
    pub regid: u32,
    pub read_swizzle: Swizzle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstInterpolateOffset {
    pub dst: DstOperand,
    pub offset: SrcOperand,
    pub regid: u32,
    pub read_swizzle: Swizzle,
}

/// One instruction (`InstructionList`'s 50 kinds, in its order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    Mov(InstMov),
    MovConditional(InstMovConditional),
    SwapConditional(InstSwapConditional),
    DotProduct(InstDotProduct),
    SinCos(InstSinCos),
    Convert(InstConvert),
    IntegerCompare(InstIntegerCompare),
    FloatCompare(InstFloatCompare),
    FloatBinaryOp(InstFloatBinaryOp),
    IntegerBinaryOp(InstIntegerBinaryOp),
    IntegerBinaryOpWithTwoDst(InstIntegerBinaryOpWithTwoDst),
    FloatUnaryOp(InstFloatUnaryOp),
    IntegerUnaryOp(InstIntegerUnaryOp),
    FloatMad(InstFloatMad),
    IntegerMad(InstIntegerMad),
    MaskedSumOfAbsDiff(InstMaskedSumOfAbsDiff),
    ExtractBits(InstExtractBits),
    BitFieldInsert(InstBitFieldInsert),
    Sample(InstSample),
    SampleCompare(InstSampleCompare),
    Gather(InstGather),
    GatherCompare(InstGatherCompare),
    SampleBias(InstSampleBias),
    SampleDerivative(InstSampleDerivative),
    SampleLod(InstSampleLod),
    SamplePos(InstSamplePos),
    SampleInfo(InstSampleInfo),
    BufferInfo(InstBufferInfo),
    ResourceInfo(InstResourceInfo),
    Load(InstLoad),
    LoadUavTyped(InstLoadUavTyped),
    StoreUavTyped(InstStoreUavTyped),
    LoadRaw(InstLoadRaw),
    LoadStructured(InstLoadStructured),
    StoreRaw(InstStoreRaw),
    StoreStructured(InstStoreStructured),
    Nop,
    Emit,
    Cut,
    PixelDiscard,
    PartialDerivative(InstPartialDerivative),
    CalcLod(InstCalcLod),
    InterpolateCentroid(InstInterpolateCentroid),
    InterpolateSample(InstInterpolateSample),
    InterpolateOffset(InstInterpolateOffset),
    Sync(InstSync),
    AtomicBinOp(InstAtomicBinOp),
    AtomicImmCmpExchange(InstAtomicImmCmpExchange),
    AtomicImmIncrement(InstAtomicImmIncrement),
    AtomicImmDecrement(InstAtomicImmDecrement),
}

/// A basic block's index in the control-flow graph's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockId(usize);

/// A branch's condition: the operand's first selected component is tested for nonzero or zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    pub operand: SrcOperand,
    pub test_nonzero: bool,
}

/// How a basic block ends (`BasicBlockTarget`, in its order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    ConditionalBranch {
        cond: Condition,
        true_branch: BlockId,
        false_branch: BlockId,
    },
    UnconditionalBranch(BlockId),
    Switch {
        value: SrcOperand,
        cases: BTreeMap<u32, BlockId>,
        case_default: BlockId,
    },
    Return,
    /// A hull shader phase run `instance_count` times: `active` per instance, then `sync`.
    InstanceBarrier {
        instance_count: u32,
        active: BlockId,
        sync: BlockId,
    },
    /// A hull shader's control points copied out, then `epilogue`.
    HullShaderWriteOutput {
        instance_count: u32,
        epilogue: BlockId,
    },
    /// A call, before inlining replaces it (a function's own blocks keep theirs).
    Call {
        func_id: u32,
        return_point: BlockId,
    },
    Undefined,
}

/// A run of instructions and how it ends.
#[derive(Debug, PartialEq, Eq)]
pub struct BasicBlock {
    pub instructions: Vec<Instruction>,
    pub target: Target,
    pub debug_name: &'static str,
}

impl BlockId {
    /// The block's position in the graph's list.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }

    /// Adds an empty block named `debug_name` to the list, and gives its index.
    pub fn push(blocks: &mut Vec<BasicBlock>, debug_name: &'static str) -> Self {
        blocks.push(BasicBlock::new(debug_name));
        Self(blocks.len() - 1)
    }
}

impl BasicBlock {
    /// An empty block with an undefined end.
    #[must_use]
    pub const fn new(debug_name: &'static str) -> Self {
        Self {
            instructions: Vec::new(),
            target: Target::Undefined,
            debug_name,
        }
    }
}
