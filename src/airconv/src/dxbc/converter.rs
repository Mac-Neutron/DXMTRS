//! The DXBC converter: one method per instruction.
//!
//! Ported from `A/nt/dxbc_converter_base.*` and `A/nt/dxbc_binding_map.hpp`. [`Converter`] lowers
//! the decoded instructions of `airconv-ir` at the builder's position, reading registers and
//! resources through the compile's [`Resources`] and [`BindingMap`]. Values are made in the C++'s
//! order (clang evaluates a call's arguments and a braced list left to right). Where the C++
//! dereferenced a null (an undeclared register, a missing resource map entry, an empty
//! `std::function`) the method answers an `invalid operation` error instead (P15); where it
//! returned early, so does the port.

use airconv_ir::{
    binding::TextureKind as Kind,
    ir::{
        AtomicBinaryOp, AtomicDst, AtomicDstOperandUav, AtomicOperandTgsm, ConversionOp,
        DstOperand, DstOperandCommon, FloatBinaryOp, FloatComparison, FloatUnaryOp, InputAttribute,
        InstAtomicBinOp, InstAtomicImmCmpExchange, InstBitFieldInsert, InstBufferInfo, InstCalcLod,
        InstConvert, InstDotProduct, InstExtractBits, InstFloatBinaryOp, InstFloatCompare,
        InstFloatMad, InstFloatUnaryOp, InstGather, InstGatherCompare, InstIntegerBinaryOp,
        InstIntegerBinaryOpWithTwoDst, InstIntegerCompare, InstIntegerMad, InstIntegerUnaryOp,
        InstLoad, InstLoadRaw, InstLoadStructured, InstLoadUavTyped, InstMaskedSumOfAbsDiff,
        InstMov, InstMovConditional, InstPartialDerivative, InstResourceInfo, InstSample,
        InstSampleBias, InstSampleCompare, InstSampleDerivative, InstSampleInfo, InstSampleLod,
        InstSamplePos, InstSinCos, InstStoreRaw, InstStoreStructured, InstStoreUavTyped,
        InstSwapConditional, InstSync, Instruction, IntegerBinaryOp, IntegerBinaryOpWithTwoDst,
        IntegerComparison, IntegerUnaryOp, NO_PHASE, OperandDataType, OperandIndex,
        ResInfoModifier, ResourceOrUav, SWIZZLE_IDENTITY, SrcOperand, SrcOperandCommon,
        SrcOperandResource, SrcOperandSampler, SrcOperandTgsm, SrcOperandUav, Swizzle, TypelessSrc,
        UavBoundary,
    },
    tokens::program_type,
};

use super::resources::{IndexableRegisterFile, Resources};
use crate::{
    abi::SM50_SHADER_METAL_320,
    air::{
        builder::{
            AirBuilder, FpBinOp, FpUnOp, Gradient, IntBinOp, IntUnOp, Lod, MemFlags, MemoryAccess,
            Query, SampleAt, SampleType, Signedness, TexelAt, Texture, TextureKind as AirKind,
        },
        ty::AirTypes,
    },
    api::Error,
    llvm::{
        Builder, Func, Ty, TyKind, Val,
        builder::{FastMath, FastMathGuard},
        sys,
    },
};

/// Components, x in bit 0 (`mask_t`).
pub type Mask = u32;
pub const MASK_ALL: Mask = 0b1111;
pub const MASK_X: Mask = 0b1;
pub const MASK_Y: Mask = 0b10;
pub const MASK_Z: Mask = 0b100;
pub const MASK_W: Mask = 0b1000;
pub const MASK_XY: Mask = 0b11;
pub const MASK_XYZ: Mask = 0b111;

/// The attribute list of the declarations the converter makes itself.
type Attrs = [(sys::AttributeIndex, &'static [u8])];
const FN: sys::AttributeIndex = sys::ATTRIBUTE_FUNCTION_INDEX;
/// `air.atomic.fence`'s, as the AIR builder declares it.
const ATOMIC_FENCE: &Attrs = &[
    (FN, b"mustprogress"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
];

/// A constant buffer's address, and its descriptor's metadata where it has one.
pub struct ConstantBufferDescriptor<'cx> {
    pub pointer: Val<'cx>,
    pub metadata: Option<Val<'cx>>,
}

/// A sampler, its cube variant and its metadata (the bias in the low word).
pub struct SamplerDescriptor<'cx> {
    pub sampler: Val<'cx>,
    pub cube_sampler: Val<'cx>,
    pub metadata: Val<'cx>,
}

/// A texture's handle and metadata, its kind as bound (`kind`, a 1D one as 2D) and as declared.
pub struct TextureDescriptor<'cx> {
    pub handle: Val<'cx>,
    pub metadata: Val<'cx>,
    pub global_coherent: bool,
    pub kind: Kind,
    pub kind_logical: Kind,
    pub memory_access: MemoryAccess,
    pub sample_type: SampleType,
    /// DXIL's 64-bit atomics: a typed buffer's base address, from its descriptor's fourth qword.
    pub buffer_address: Option<Val<'cx>>,
}

/// A buffer's `i32` pointer, its metadata (none for a root descriptor's) and structure stride.
pub struct BufferDescriptor<'cx> {
    pub pointer: Val<'cx>,
    pub metadata: Option<Val<'cx>>,
    pub structure_stride: u32,
    pub global_coherent: bool,
}

/// A view's counter.
pub struct CounterDescriptor<'cx> {
    pub pointer: Val<'cx>,
}

/// Where a compile's resources are bound (`BindingMap`): D3D11's tables or a root signature.
///
/// Each method makes its loads at the builder's position in `function` and answers `None` for a
/// range the map does not bind.
pub trait BindingMap<'cx> {
    /// The constant buffer of `range`, element `index` of it.
    ///
    /// # Errors
    ///
    /// An internal inconsistency (a table argument that is not a structure pointer).
    fn constant_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<ConstantBufferDescriptor<'cx>>, Error>;

    /// The sampler of `range`.
    ///
    /// # Errors
    ///
    /// As [`BindingMap::constant_buffer`].
    fn sampler(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<SamplerDescriptor<'cx>>, Error>;

    /// The shader resource view of `range` when it is a texture.
    ///
    /// # Errors
    ///
    /// As [`BindingMap::constant_buffer`].
    fn srv_texture(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error>;

    /// The unordered access view of `range` when it is a texture.
    ///
    /// # Errors
    ///
    /// As [`BindingMap::constant_buffer`].
    fn uav_texture(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error>;

    /// The shader resource view of `range` when it is a buffer.
    ///
    /// # Errors
    ///
    /// As [`BindingMap::constant_buffer`].
    fn srv_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error>;

    /// The unordered access view of `range` when it is a buffer.
    ///
    /// # Errors
    ///
    /// As [`BindingMap::constant_buffer`].
    fn uav_buffer(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error>;

    /// The counter of the unordered access view of `range`.
    ///
    /// # Errors
    ///
    /// As [`BindingMap::constant_buffer`].
    fn uav_counter(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        range: u32,
        index: Val<'cx>,
    ) -> Result<Option<CounterDescriptor<'cx>>, Error>;

    /// DXIL's 64-bit atomics: the device's lock table (`GetAtomic64Locks`), `None` without one.
    ///
    /// # Errors
    ///
    /// As [`BindingMap::constant_buffer`].
    fn atomic64_locks(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
    ) -> Result<Option<Val<'cx>>, Error> {
        let _ = (air, function);
        Ok(None)
    }
}

/// What a geometry shader's `emit` and `cut` do (the C++'s `call_emit` and `call_cut`).
pub trait GeometryOutput {
    /// `emit`.
    ///
    /// # Errors
    ///
    /// What the output's lowering refuses.
    fn emit(&self) -> Result<(), Error>;

    /// `cut`.
    ///
    /// # Errors
    ///
    /// What the output's lowering refuses.
    fn cut(&self) -> Result<(), Error>;
}

/// The tessellator's partitioning (`TessellatorPartitioning`).
pub enum Partitioning {
    Integer,
    Pow2,
    FractionalOdd,
    FractionalEven,
}

/// The tessellator's output primitive (`TessellatorOutputPrimitive`).
pub enum OutputPrimitive {
    Point,
    Line,
    Triangle,
    TriangleCcw,
}

/// A texture operand: the AIR description, the declared kind, handle, metadata and swizzle.
pub struct TextureHandle<'cx> {
    pub texture: Texture,
    pub logical: Kind,
    pub handle: Val<'cx>,
    pub metadata: Val<'cx>,
    pub swizzle: Swizzle,
    pub global_coherent: bool,
}

/// A buffer operand (`BufferResourceHandle`, `AtomicBufferResourceHandle`).
///
/// A read uses its swizzle, a write its mask.
pub struct BufferHandle<'cx> {
    pub pointer: Val<'cx>,
    pub metadata: Option<Val<'cx>>,
    pub structure_stride: u32,
    pub swizzle: Swizzle,
    pub mask: Mask,
    pub global_coherent: bool,
}

/// A sampler operand: its handle, cube handle and bias.
pub struct SamplerHandle<'cx> {
    pub handle: Val<'cx>,
    pub handle_cube: Val<'cx>,
    pub bias: Val<'cx>,
}

/// The lowering of one function's instructions (`dxbc::Converter`).
pub struct Converter<'a, 'cx> {
    pub air: &'a AirBuilder<'a, 'cx>,
    pub ir: &'a Builder<'cx>,
    pub function: Func<'cx>,
    pub binding: &'a dyn BindingMap<'cx>,
    pub res: &'a Resources<'cx>,
    pub types: &'a AirTypes,
    pub pso_sample_mask: u32,
    /// `D3D10_SB_TOKENIZED_PROGRAM_TYPE`.
    pub shader_type: u32,
    /// `SM50_SHADER_METAL_*`.
    pub metal_version: u32,
    /// A geometry shader's `emit` and `cut`; `None` for every other stage.
    pub geometry: Option<&'a dyn GeometryOutput>,
}

impl<'a, 'cx> Converter<'a, 'cx> {
    // Helpers.

    fn int(&self, v: u32) -> Val<'cx> {
        self.air.get_int(v)
    }

    /// `ConstantInt::get(ty, v)`: a splat for a vector type.
    fn const_like(&self, ty: Ty, v: u64) -> Val<'cx> {
        let cx = self.ir.cx();
        match cx.tcx().kind(ty) {
            TyKind::Vector(element, n) => {
                let scalar = cx.const_int(element, v, false);
                cx.const_vector(&vec![scalar; n as usize])
            }
            _ => cx.const_int(ty, v, false),
        }
    }

    /// `ty` with `bits`-bit integer lanes (`getWithNewBitWidth`).
    fn with_bits(&self, ty: Ty, bits: u32) -> Ty {
        let tcx = self.ir.cx().tcx();
        match tcx.kind(ty) {
            TyKind::Vector(_, n) => tcx.vector(tcx.int(bits), n),
            _ => tcx.int(bits),
        }
    }

    fn scalar_kind(&self, ty: Ty) -> TyKind {
        let tcx = self.ir.cx().tcx();
        match tcx.kind(ty) {
            TyKind::Vector(element, _) => tcx.kind(element),
            kind => kind,
        }
    }

    fn lanes(&self, ty: Ty) -> Option<u32> {
        match self.ir.cx().tcx().kind(ty) {
            TyKind::Vector(_, n) => Some(n),
            _ => None,
        }
    }

    /// The pointee of a register file or buffer pointer (`GetArrayType`).
    fn pointee(&self, ptr: Val<'cx>) -> Result<Ty, Error> {
        self.ir
            .cx()
            .tcx()
            .pointee(ptr.ty())
            .ok_or_else(|| invalid("a register file is not a pointer"))
    }

    fn thread_id_in_patch(&self) -> Result<Val<'cx>, Error> {
        req(self.res.thread_id_in_patch, "no thread id in the patch")
    }

    /// The temporaries of `phase` (the shader's own outside a phase).
    fn temp_file(&self, phase: u32) -> Result<Val<'cx>, Error> {
        let file = if phase == NO_PHASE {
            self.res.temp
        } else {
            self.res
                .phases
                .get(phase as usize)
                .ok_or_else(|| invalid("a register of an undeclared phase"))?
                .temp
        };
        req(file.ptr_int4, "no temporary register file")
    }

    /// The indexable temp `regfile` of `phase`.
    fn indexable(&self, phase: u32, regfile: u32) -> Result<IndexableRegisterFile<'cx>, Error> {
        let map = if phase == NO_PHASE {
            &self.res.indexable_temp_map
        } else {
            &self
                .res
                .phases
                .get(phase as usize)
                .ok_or_else(|| invalid("a register of an undeclared phase"))?
                .indexable_temp_map
        };
        map.get(&regfile)
            .copied()
            .ok_or_else(|| invalid("an undeclared indexable temp"))
    }

    /// The output register file, or the patch constants' inside a phase.
    fn output_file(&self, phase: u32) -> Result<Val<'cx>, Error> {
        if phase == NO_PHASE {
            req(self.res.output.ptr_int4, "no output register file")
        } else {
            req(
                self.res.patch_constant_output.ptr_int4,
                "no patch constant register file",
            )
        }
    }

    /// `UseFastMath`: none for a precise instruction, else the stage's flags until dropped.
    #[must_use]
    pub fn use_fast_math(&self, precise_mask: u8) -> Option<FastMathGuard<'a, 'cx>> {
        if precise_mask != 0 {
            return None;
        }
        let mut flags = FastMath::NONE;
        if self.shader_type == program_type::COMPUTE || self.shader_type == program_type::PIXEL {
            flags = flags
                .with(FastMath::ALLOW_CONTRACT)
                .with(FastMath::ALLOW_REASSOC)
                .with(FastMath::ALLOW_RECIPROCAL);
        }
        flags = flags
            .with(FastMath::APPROX_FUNC)
            .with(FastMath::NO_SIGNED_ZEROS);
        Some(self.ir.fast_math_guard(flags))
    }

    /// `SupportsMemoryCoherency`.
    #[must_use]
    pub const fn supports_memory_coherency(&self) -> bool {
        self.metal_version >= SM50_SHADER_METAL_320
    }

    /// `SupportsNonExecutionBarrier`.
    #[must_use]
    pub const fn supports_non_execution_barrier(&self) -> bool {
        self.metal_version >= SM50_SHADER_METAL_320
    }

    /// Declares `name` of `ty` with `attrs` (given to a new declaration only) and calls it.
    fn call_declared(
        &self,
        name: &[u8],
        ret: Ty,
        args: &[Val<'cx>],
        attrs: &Attrs,
    ) -> Result<Val<'cx>, Error> {
        let cx = self.ir.cx();
        let params: Vec<Ty> = args.iter().map(|a| a.ty()).collect();
        let fn_ty = cx.tcx().function(ret, &params, false);
        let attrs: Vec<(sys::AttributeIndex, sys::AttributeRef)> = attrs
            .iter()
            .filter_map(|&(index, attr)| Some((index, cx.enum_attribute(attr, 0)?)))
            .collect();
        let callee = self
            .air
            .module()
            .get_or_insert_function(name, fn_ty, &attrs)?;
        Ok(self.ir.call(callee, args))
    }

    /// `air.atomic.fence` at a scope of several bits (the AIR builder's takes one).
    fn atomic_fence(&self, flags: MemFlags, scope: u32) -> Result<Val<'cx>, Error> {
        let void = self.air.void_ty();
        self.call_declared(
            b"air.atomic.fence",
            void,
            &[self.int(flags.bits()), self.int(5), self.int(scope)],
            ATOMIC_FENCE,
        )
    }

    /// A load, `volatile` when asked (`CreateLoad(ty, ptr, isVolatile)`).
    fn load(&self, ty: Ty, ptr: Val<'cx>, volatile: bool) -> Val<'cx> {
        let v = self.ir.load(ty, ptr);
        if volatile {
            // SAFETY: a load instruction just made.
            unsafe { sys::set_volatile(v.raw(), 1) };
        }
        v
    }

    /// A store, `volatile` when asked (`CreateStore(v, ptr, isVolatile)`).
    fn store(&self, v: Val<'cx>, ptr: Val<'cx>, volatile: bool) {
        let s = self.ir.store(v, ptr);
        if volatile {
            // SAFETY: a store instruction just made.
            unsafe { sys::set_volatile(s.raw(), 1) };
        }
    }

    // Operand indices and loads.

    /// An operand index's value (`LoadOperandIndex`).
    ///
    /// # Errors
    ///
    /// A register no declaration made.
    pub fn load_operand_index(&self, index: &OperandIndex) -> Result<Val<'cx>, Error> {
        match *index {
            OperandIndex::Immediate(v) => Ok(self.int(v)),
            OperandIndex::Temp {
                regid,
                phase,
                component,
                offset,
            } => {
                let rel = self.load_operand(
                    &SrcOperand::Temp {
                        common: index_common(component),
                        regid,
                        phase,
                    },
                    MASK_X,
                )?;
                Ok(self.ir.add(rel, self.int(offset)))
            }
            OperandIndex::IndexableTemp {
                regfile,
                regid,
                phase,
                component,
                offset,
            } => {
                let rel = self.load_operand(
                    &SrcOperand::IndexableTemp {
                        common: index_common(component),
                        regfile,
                        regindex: OperandIndex::Immediate(regid),
                        phase,
                    },
                    MASK_X,
                )?;
                Ok(self.ir.add(rel, self.int(offset)))
            }
            OperandIndex::ControlPointId { offset } => {
                Ok(self.ir.add(self.thread_id_in_patch()?, self.int(offset)))
            }
        }
    }

    /// An operand's `mask` components, swizzled and modified (`LoadOperand`).
    ///
    /// # Errors
    ///
    /// A register no declaration made; an immediate read of no component.
    pub fn load_operand(&self, src: &SrcOperand, mask: Mask) -> Result<Val<'cx>, Error> {
        match src {
            SrcOperand::Immediate32 { common, value } => self.load_immediate(common, value, mask),
            SrcOperand::Temp {
                common,
                regid,
                phase,
            } => self.load_temp(common, *regid, *phase, mask),
            SrcOperand::IndexableTemp {
                common,
                regfile,
                regindex,
                phase,
            } => self.load_indexable_temp(common, *regfile, regindex, *phase, mask),
            SrcOperand::Input { common, regid } => {
                let handle = req(self.res.input.ptr_int4, "no input register file")?;
                self.load_vec4_array(common, handle, &[self.int(0), self.int(*regid)], mask)
            }
            SrcOperand::ConstantBuffer {
                common,
                rangeid,
                rangeindex,
                regindex,
            } => self.load_constant_buffer(common, *rangeid, rangeindex, regindex, mask),
            SrcOperand::ImmediateConstantBuffer { common, regindex } => {
                let handle = req(self.res.icb, "no immediate constant buffer")?;
                let index = self.load_operand_index(regindex)?;
                self.load_vec4_array(common, handle, &[self.int(0), index], mask)
            }
            SrcOperand::Attribute { common, attribute } => {
                self.load_attribute(common, attribute, mask)
            }
            SrcOperand::IndexableInput { common, regindex } => {
                let handle = req(self.res.input.ptr_int4, "no input register file")?;
                let index = self.load_operand_index(regindex)?;
                self.load_vec4_array(common, handle, &[self.int(0), index], mask)
            }
            SrcOperand::InputIcp {
                common,
                cpid,
                regid,
            } => {
                let handle = req(self.res.input.ptr_int4, "no input register file")?;
                let cp = self.load_operand_index(cpid)?;
                self.load_vec4_array(common, handle, &[self.int(0), cp, self.int(*regid)], mask)
            }
            SrcOperand::InputPc { common, regindex } => {
                let handle = req(
                    self.res.patch_constant_output.ptr_int4,
                    "no patch constant register file",
                )?;
                let index = self.load_operand_index(regindex)?;
                self.load_vec4_array(common, handle, &[self.int(0), index], mask)
            }
            SrcOperand::InputOcp {
                common,
                cpid,
                regid,
            } => {
                let handle = req(self.res.output.ptr_int4, "no output register file")?;
                let cp = self.load_operand_index(cpid)?;
                self.load_vec4_array(common, handle, &[self.int(0), cp, self.int(*regid)], mask)
            }
        }
    }

    /// A register array read: the input, ICB and patch constant operands.
    ///
    /// One component (GEP `indices` plus the component) or the whole `<4 x i32>`, modified.
    fn load_vec4_array(
        &self,
        common: &SrcOperandCommon,
        handle: Val<'cx>,
        indices: &[Val<'cx>],
        mask: Mask,
    ) -> Result<Val<'cx>, Error> {
        let ty_handle = self.pointee(handle)?;
        if let Some(comp) = component_from_scalar_mask(mask, &common.swizzle) {
            let mut indices = indices.to_vec();
            indices.push(self.int(comp));
            let ptr = self.ir.gep(ty_handle, handle, &indices);
            let v = self.ir.load(self.types.int, ptr);
            return self.apply_src_modifier(common, v, mask);
        }
        let ptr = self.ir.gep(ty_handle, handle, indices);
        let v = self.ir.load(self.types.int4, ptr);
        self.apply_src_modifier(common, v, mask)
    }

    fn load_constant_buffer(
        &self,
        common: &SrcOperandCommon,
        range: u32,
        rangeindex: &OperandIndex,
        regindex: &OperandIndex,
        mask: Mask,
    ) -> Result<Val<'cx>, Error> {
        let index = self.load_operand_index(rangeindex)?;
        let Some(descriptor) =
            self.binding
                .constant_buffer(self.air, self.function, range, index)?
        else {
            let zero = self.ir.cx().null(self.types.int4);
            return self.apply_src_modifier(common, zero, mask);
        };
        let handle = descriptor.pointer;
        if let Some(comp) = component_from_scalar_mask(mask, &common.swizzle) {
            let reg = self.load_operand_index(regindex)?;
            let ptr = self.ir.gep(self.types.int4, handle, &[reg, self.int(comp)]);
            let v = self.ir.load(self.types.int, ptr);
            return self.apply_src_modifier(common, v, mask);
        }
        let reg = self.load_operand_index(regindex)?;
        let ptr = self.ir.gep(self.types.int4, handle, &[reg]);
        let v = self.ir.load(self.types.int4, ptr);
        self.apply_src_modifier(common, v, mask)
    }

    fn load_immediate(
        &self,
        common: &SrcOperandCommon,
        value: &[u32; 4],
        mask: Mask,
    ) -> Result<Val<'cx>, Error> {
        let constant = |v: u32| match common.read_type {
            OperandDataType::Float => self.air.get_float(f32::from_bits(v)),
            OperandDataType::Integer => self.air.get_int(v),
            OperandDataType::Half16X16 => self.air.get_half(low16(v)),
        };
        let at = |i: usize| value.get(i).copied().unwrap_or(0);
        match mask {
            0b1 => return Ok(constant(at(0))),
            0b10 => return Ok(constant(at(1))),
            0b100 => return Ok(constant(at(2))),
            0b1000 => return Ok(constant(at(3))),
            _ => {}
        }
        let constants: Vec<Val<'cx>> = components(mask)
            .map(|(bit, _)| constant(at(bit as usize)))
            .collect();
        if constants.is_empty() {
            return Err(invalid("an immediate read of no component"));
        }
        Ok(self.ir.cx().const_vector(&constants))
    }

    fn load_temp(
        &self,
        common: &SrcOperandCommon,
        regid: u32,
        phase: u32,
        mask: Mask,
    ) -> Result<Val<'cx>, Error> {
        let handle = self.temp_file(phase)?;
        let ty_handle = self.pointee(handle)?;
        let ty_int = self.types.int;
        let element = |comp: u32| {
            let at = self
                .ir
                .add(self.ir.shl(self.int(regid), self.int(2)), self.int(comp));
            self.ir.gep(ty_handle, handle, &[self.int(0), at])
        };
        if let Some(comp) = component_from_scalar_mask(mask, &common.swizzle) {
            let v = self.ir.load(ty_int, element(comp));
            return self.apply_src_modifier(common, v, mask);
        }
        let mut v = self.ir.cx().poison(self.types.int4);
        for (comp, _) in components(memory_access_mask(mask, &common.swizzle)) {
            let ptr = element(comp);
            v = self
                .ir
                .insert_element_u64(v, self.ir.load(ty_int, ptr), comp.into());
        }
        self.apply_src_modifier(common, v, mask)
    }

    fn load_attribute(
        &self,
        common: &SrcOperandCommon,
        attribute: &InputAttribute,
        mask: Mask,
    ) -> Result<Val<'cx>, Error> {
        let res = self.res;
        let ret = match attribute {
            // Handled by the prologue: never read here.
            InputAttribute::VertexId | InputAttribute::InstanceId => {
                self.ir.cx().poison(self.types.int4)
            }
            InputAttribute::ThreadId => req(res.thread_id_arg, "no thread id")?,
            InputAttribute::ThreadIdInGroup => {
                req(res.thread_id_in_group_arg, "no thread id in the group")?
            }
            InputAttribute::ThreadGroupId => req(res.thread_group_id_arg, "no thread group id")?,
            InputAttribute::ThreadIdInGroupFlatten => req(
                res.thread_id_in_group_flat_arg,
                "no flattened thread id in the group",
            )?,
            InputAttribute::OutputControlPointId
            | InputAttribute::ForkInstanceId
            | InputAttribute::JoinInstanceId => self.thread_id_in_patch()?,
            InputAttribute::CoverageMask => {
                let arg = req(res.coverage_mask_arg, "no coverage mask")?;
                if self.pso_sample_mask == u32::MAX {
                    arg
                } else {
                    self.ir
                        .and(arg, self.const_like(arg.ty(), self.pso_sample_mask.into()))
                }
            }
            InputAttribute::Domain => req(res.domain, "no domain location")?,
            InputAttribute::PrimitiveId => req(res.patch_id, "no primitive id")?,
            InputAttribute::GsInstanceId => req(res.gs_instance_id, "no instance id")?,
        };
        self.apply_src_modifier(common, ret, mask)
    }

    fn load_indexable_temp(
        &self,
        common: &SrcOperandCommon,
        regfile: u32,
        regindex: &OperandIndex,
        phase: u32,
        mask: Mask,
    ) -> Result<Val<'cx>, Error> {
        let file = self.indexable(phase, regfile)?;
        let handle = file.ptr_int_vec;
        let ty_handle = self.pointee(handle)?;
        let index = self.load_operand_index(regindex)?;
        let ty_int = self.types.int;
        let element = |comp: u32| {
            let at = self
                .ir
                .add(self.ir.mul(index, self.int(file.vec_size)), self.int(comp));
            self.ir.gep(ty_handle, handle, &[self.int(0), at])
        };
        if let Some(comp) = component_from_scalar_mask(mask, &common.swizzle) {
            let v = self.ir.load(ty_int, element(comp));
            return self.apply_src_modifier(common, v, mask);
        }
        let mut v = self.ir.cx().poison(self.air.int_ty(file.vec_size));
        for (comp, _) in components(memory_access_mask(mask, &common.swizzle)) {
            let ptr = element(comp);
            v = self
                .ir
                .insert_element_u64(v, self.ir.load(ty_int, ptr), comp.into());
        }
        self.apply_src_modifier(common, v, mask)
    }

    /// Swizzles, converts to the read type and applies `abs` and `neg` (`ApplySrcModifier`).
    fn apply_src_modifier(
        &self,
        common: &SrcOperandCommon,
        value: Val<'cx>,
        mask: Mask,
    ) -> Result<Val<'cx>, Error> {
        let mut value = self.mask_swizzle(value, mask, &common.swizzle);
        match common.read_type {
            OperandDataType::Float => {
                value = self.bitcast_to_float(value);
                if common.abs {
                    value = self.air.create_fp_un_op(FpUnOp::Fabs, value, true)?;
                }
                if common.neg {
                    value = self.ir.fneg(value);
                }
            }
            OperandDataType::Integer => {
                value = self.bitcast_to_int32(value);
                if common.neg {
                    value = self.ir.neg(value);
                }
            }
            OperandDataType::Half16X16 => {
                value = self.trunc_and_bitcast_to_half(value);
                if common.neg {
                    value = self.ir.fneg(value);
                }
            }
        }
        Ok(value)
    }

    // Resources.

    fn texture_handle(
        &self,
        descriptor: Option<TextureDescriptor<'cx>>,
        swizzle: Swizzle,
    ) -> Option<TextureHandle<'cx>> {
        let d = descriptor?;
        Some(TextureHandle {
            texture: Texture {
                kind: air_kind(d.kind),
                sample_type: d.sample_type,
                memory_access: d.memory_access,
            },
            logical: d.kind_logical,
            handle: d.handle,
            metadata: d.metadata,
            swizzle,
            global_coherent: d.global_coherent && self.supports_memory_coherency(),
        })
    }

    /// A shader resource view as a texture (`LoadTexture`).
    fn load_srv_texture(
        &self,
        src: &SrcOperandResource,
    ) -> Result<Option<TextureHandle<'cx>>, Error> {
        let index = self.load_operand_index(&src.index)?;
        let d = self
            .binding
            .srv_texture(self.air, self.function, src.range_id, index)?;
        Ok(self.texture_handle(d, src.read_swizzle.clone()))
    }

    /// An unordered access view as a texture, read with `swizzle` (`LoadTexture`).
    fn load_uav_texture(
        &self,
        range_id: u32,
        index: &OperandIndex,
        swizzle: Swizzle,
    ) -> Result<Option<TextureHandle<'cx>>, Error> {
        let index = self.load_operand_index(index)?;
        let d = self
            .binding
            .uav_texture(self.air, self.function, range_id, index)?;
        Ok(self.texture_handle(d, swizzle))
    }

    fn load_view_texture(&self, src: &ResourceOrUav) -> Result<Option<TextureHandle<'cx>>, Error> {
        match src {
            ResourceOrUav::Resource(r) => self.load_srv_texture(r),
            ResourceOrUav::Uav(u) => {
                self.load_uav_texture(u.range_id, &u.index, u.read_swizzle.clone())
            }
        }
    }

    fn buffer_handle(
        &self,
        descriptor: Option<BufferDescriptor<'cx>>,
        swizzle: Swizzle,
        mask: Mask,
    ) -> Option<BufferHandle<'cx>> {
        let d = descriptor?;
        Some(BufferHandle {
            pointer: d.pointer,
            metadata: d.metadata,
            structure_stride: d.structure_stride,
            swizzle,
            mask,
            global_coherent: d.global_coherent && self.supports_memory_coherency(),
        })
    }

    fn load_srv_buffer(
        &self,
        src: &SrcOperandResource,
    ) -> Result<Option<BufferHandle<'cx>>, Error> {
        let index = self.load_operand_index(&src.index)?;
        let d = self
            .binding
            .srv_buffer(self.air, self.function, src.range_id, index)?;
        Ok(self.buffer_handle(d, src.read_swizzle.clone(), MASK_ALL))
    }

    fn load_uav_buffer(&self, src: &SrcOperandUav) -> Result<Option<BufferHandle<'cx>>, Error> {
        let index = self.load_operand_index(&src.index)?;
        let d = self
            .binding
            .uav_buffer(self.air, self.function, src.range_id, index)?;
        Ok(self.buffer_handle(d, src.read_swizzle.clone(), MASK_ALL))
    }

    fn load_atomic_uav_buffer(
        &self,
        dst: &AtomicDstOperandUav,
    ) -> Result<Option<BufferHandle<'cx>>, Error> {
        let index = self.load_operand_index(&dst.index)?;
        let d = self
            .binding
            .uav_buffer(self.air, self.function, dst.range_id, index)?;
        Ok(self.buffer_handle(d, SWIZZLE_IDENTITY, dst.mask))
    }

    /// Threadgroup memory as an `i32` pointer.
    fn load_tgsm(&self, id: u32, swizzle: Swizzle, mask: Mask) -> Result<BufferHandle<'cx>, Error> {
        let &(stride, global) = self
            .res
            .tgsm_map
            .get(&id)
            .ok_or_else(|| invalid("undeclared threadgroup memory"))?;
        let tcx = self.ir.cx().tcx();
        let value_ty = self.pointee(global)?;
        let addrspace = tcx.addrspace(global.ty()).unwrap_or(0);
        let mut zero = [self.int(0).raw()];
        // SAFETY: a global of this context, its value type and one constant index.
        let gep = unsafe {
            sys::const_in_bounds_gep2(tcx.lower(value_ty), global.raw(), zero.as_mut_ptr(), 1)
        };
        // SAFETY: a constant of this context; one zero index keeps the global's type.
        let gep = unsafe { self.ir.cx().val(gep, global.ty()) };
        let pointer = self
            .ir
            .pointer_cast(gep, tcx.ptr(self.types.int, addrspace));
        Ok(BufferHandle {
            pointer,
            metadata: None,
            structure_stride: stride,
            swizzle,
            mask,
            global_coherent: false,
        })
    }

    fn load_typeless(&self, src: &TypelessSrc) -> Result<Option<BufferHandle<'cx>>, Error> {
        match src {
            TypelessSrc::Resource(r) => self.load_srv_buffer(r),
            TypelessSrc::Uav(u) => self.load_uav_buffer(u),
            TypelessSrc::Tgsm(SrcOperandTgsm { id, read_swizzle }) => self
                .load_tgsm(*id, read_swizzle.clone(), MASK_ALL)
                .map(Some),
        }
    }

    fn load_atomic_buffer(&self, dst: &AtomicDst) -> Result<Option<BufferHandle<'cx>>, Error> {
        match dst {
            AtomicDst::Uav(u) => self.load_atomic_uav_buffer(u),
            AtomicDst::Tgsm(AtomicOperandTgsm { id, mask }) => {
                self.load_tgsm(*id, SWIZZLE_IDENTITY, *mask).map(Some)
            }
        }
    }

    fn load_atomic_texture(&self, dst: &AtomicDst) -> Result<Option<TextureHandle<'cx>>, Error> {
        match dst {
            AtomicDst::Uav(u) => self.load_uav_texture(u.range_id, &u.index, SWIZZLE_IDENTITY),
            AtomicDst::Tgsm(_) => Ok(None),
        }
    }

    fn load_counter(&self, uav: &AtomicDstOperandUav) -> Result<Option<Val<'cx>>, Error> {
        let index = self.load_operand_index(&uav.index)?;
        Ok(self
            .binding
            .uav_counter(self.air, self.function, uav.range_id, index)?
            .map(|c| c.pointer))
    }

    /// A sampler and its bias, the metadata's low word as a `float` (`LoadSampler`).
    fn load_sampler(&self, src: &SrcOperandSampler) -> Result<Option<SamplerHandle<'cx>>, Error> {
        let index = self.load_operand_index(&src.index)?;
        let Some(d) = self
            .binding
            .sampler(self.air, self.function, src.range_id, index)?
        else {
            return Ok(None);
        };
        let bias = self
            .ir
            .bitcast(self.ir.trunc(d.metadata, self.types.int), self.types.float);
        Ok(Some(SamplerHandle {
            handle: d.sampler,
            handle_cube: d.cube_sampler,
            bias,
        }))
    }

    // Stores.

    /// Stores `value` to `dst` (`StoreOperand`).
    ///
    /// Saturated first when asked and written as float.
    ///
    /// # Errors
    ///
    /// A register no declaration made.
    pub fn store_operand(
        &self,
        dst: &DstOperand,
        value: Val<'cx>,
        saturate: bool,
    ) -> Result<(), Error> {
        let value = if dst_common(dst).write_type == OperandDataType::Float && saturate {
            self.air.create_fp_un_op(FpUnOp::Saturate, value, true)?
        } else {
            value
        };
        match dst {
            DstOperand::Null { .. } | DstOperand::SideEffect { .. } => Ok(()),
            DstOperand::Output {
                common,
                regid,
                phase,
            } => {
                if self.shader_type == program_type::HULL && *phase == NO_PHASE {
                    let cp = self.thread_id_in_patch()?;
                    return self.store_vec4_array(
                        common.mask,
                        self.output_file(NO_PHASE)?,
                        &[self.int(0), cp, self.int(*regid)],
                        value,
                    );
                }
                let handle = self.output_file(*phase)?;
                self.store_vec4_array(common.mask, handle, &[self.int(0), self.int(*regid)], value)
            }
            DstOperand::IndexableOutput {
                common,
                regindex,
                phase,
            } => {
                let value_int = self.zext_and_bitcast_to_int32(value);
                if self.shader_type == program_type::HULL && *phase == NO_PHASE {
                    let handle = self.output_file(NO_PHASE)?;
                    let index = self.ir.add(
                        self.ir.mul(
                            self.thread_id_in_patch()?,
                            self.int(self.res.output_element_count),
                        ),
                        self.load_operand_index(regindex)?,
                    );
                    return self.store_vec4_int(
                        common.mask,
                        handle,
                        &[self.int(0), index],
                        value_int,
                    );
                }
                let handle = self.output_file(*phase)?;
                let index = self.load_operand_index(regindex)?;
                self.store_vec4_int(common.mask, handle, &[self.int(0), index], value_int)
            }
            DstOperand::OutputCoverageMask { .. } => {
                let reg = req(self.res.coverage_mask_reg, "no coverage mask output")?;
                self.store_special(reg, self.types.int, self.zext_and_bitcast_to_int32(value));
                Ok(())
            }
            DstOperand::OutputStencilRef { .. } => {
                let reg = req(self.res.stencil_ref_reg, "no stencil reference output")?;
                self.store_special(reg, self.types.int, self.zext_and_bitcast_to_int32(value));
                Ok(())
            }
            DstOperand::OutputDepth { .. } => {
                let reg = req(self.res.depth_output_reg, "no depth output")?;
                self.store_special(reg, self.types.float, self.zext_and_bitcast_to_float(value));
                Ok(())
            }
            DstOperand::Temp {
                common,
                regid,
                phase,
            } => {
                let value_int = self.zext_and_bitcast_to_int32(value);
                let handle = self.temp_file(*phase)?;
                let ty_handle = self.pointee(handle)?;
                for (dst_comp, src_comp) in components(common.mask) {
                    let at = self.ir.add(
                        self.ir.shl(self.int(*regid), self.int(2)),
                        self.int(dst_comp),
                    );
                    let ptr = self.ir.inbounds_gep(ty_handle, handle, &[self.int(0), at]);
                    self.ir
                        .store(self.element_or_scalar(value_int, src_comp.into()), ptr);
                }
                Ok(())
            }
            DstOperand::IndexableTemp {
                common,
                regfile,
                regindex,
                phase,
            } => {
                let value_int = self.zext_and_bitcast_to_int32(value);
                let file = self.indexable(*phase, *regfile)?;
                let handle = file.ptr_int_vec;
                let ty_handle = self.pointee(handle)?;
                let index = self.load_operand_index(regindex)?;
                for (dst_comp, src_comp) in components(common.mask) {
                    let at = self.ir.add(
                        self.ir.mul(index, self.int(file.vec_size)),
                        self.int(dst_comp),
                    );
                    let ptr = self.ir.inbounds_gep(ty_handle, handle, &[self.int(0), at]);
                    self.ir
                        .store(self.element_or_scalar(value_int, src_comp.into()), ptr);
                }
                Ok(())
            }
        }
    }

    /// An output store through `indices` (`StoreOperand(DstOperandOutput)` and its hull form).
    fn store_vec4_array(
        &self,
        mask: Mask,
        handle: Val<'cx>,
        indices: &[Val<'cx>],
        value: Val<'cx>,
    ) -> Result<(), Error> {
        let value_int = self.zext_and_bitcast_to_int32(value);
        self.store_vec4_int(mask, handle, indices, value_int)
    }

    /// The four components at once for a full mask, else one store per component.
    fn store_vec4_int(
        &self,
        mask: Mask,
        handle: Val<'cx>,
        indices: &[Val<'cx>],
        value_int: Val<'cx>,
    ) -> Result<(), Error> {
        let ty_handle = self.pointee(handle)?;
        if mask & MASK_ALL == MASK_ALL {
            let ptr = self.ir.inbounds_gep(ty_handle, handle, indices);
            self.ir.store(self.splat_unless_vector(4, value_int)?, ptr);
            return Ok(());
        }
        for (dst_comp, src_comp) in components(mask) {
            let mut at = indices.to_vec();
            at.push(self.int(dst_comp));
            let ptr = self.ir.inbounds_gep(ty_handle, handle, &at);
            self.ir
                .store(self.element_or_scalar(value_int, src_comp.into()), ptr);
        }
        Ok(())
    }

    /// A special output's first component (`CreateConstGEP1_32(ty, reg, 0)`).
    fn store_special(&self, reg: Val<'cx>, ty: Ty, value: Val<'cx>) {
        let ptr = self.ir.const_gep1_32(ty, reg, 0);
        self.ir.store(self.element_or_scalar(value, 0), ptr);
    }

    // Conversions.

    /// `BitcastToFloat`.
    fn bitcast_to_float(&self, value: Val<'cx>) -> Val<'cx> {
        if self.scalar_kind(value.ty()) == TyKind::Float {
            return value;
        }
        let to = self
            .lanes(value.ty())
            .map_or(self.types.float, |n| self.air.float_ty(n));
        self.ir.bitcast(value, to)
    }

    /// `BitcastToInt32`.
    fn bitcast_to_int32(&self, value: Val<'cx>) -> Val<'cx> {
        if self.scalar_kind(value.ty()) == TyKind::Int(32) {
            return value;
        }
        let to = self
            .lanes(value.ty())
            .map_or(self.types.int, |n| self.air.int_ty(n));
        self.ir.bitcast(value, to)
    }

    /// `TruncAndBitcastToHalf`.
    fn trunc_and_bitcast_to_half(&self, value: Val<'cx>) -> Val<'cx> {
        let scalar = self.scalar_kind(value.ty());
        if scalar == TyKind::Half {
            return value;
        }
        let tcx = self.ir.cx().tcx();
        let int32 = scalar == TyKind::Int(32);
        if let Some(n) = self.lanes(value.ty()) {
            let value = if int32 {
                value
            } else {
                self.ir.bitcast(value, self.air.int_ty(n))
            };
            let short = self.ir.trunc(value, tcx.vector(tcx.int(16), n));
            return self.ir.bitcast(short, self.air.half_ty(n));
        }
        let value = if int32 {
            value
        } else {
            self.ir.bitcast(value, self.types.int)
        };
        self.ir
            .bitcast(self.ir.trunc(value, tcx.int(16)), self.types.half)
    }

    /// `ZExtAndBitcastToInt32`.
    fn zext_and_bitcast_to_int32(&self, value: Val<'cx>) -> Val<'cx> {
        if self.scalar_kind(value.ty()) == TyKind::Half {
            let tcx = self.ir.cx().tcx();
            if let Some(n) = self.lanes(value.ty()) {
                let short = self.ir.bitcast(value, tcx.vector(tcx.int(16), n));
                return self.ir.zext(short, self.air.int_ty(n));
            }
            return self
                .ir
                .zext(self.ir.bitcast(value, tcx.int(16)), self.types.int);
        }
        self.bitcast_to_int32(value)
    }

    /// `ZExtAndBitcastToFloat`.
    fn zext_and_bitcast_to_float(&self, value: Val<'cx>) -> Val<'cx> {
        let value = if self.scalar_kind(value.ty()) == TyKind::Half {
            self.zext_and_bitcast_to_int32(value)
        } else {
            value
        };
        self.bitcast_to_float(value)
    }

    /// The `mask` components of `value` in `swizzle`'s order; a scalar is splat (`MaskSwizzle`).
    fn mask_swizzle(&self, value: Val<'cx>, mask: Mask, swizzle: &Swizzle) -> Val<'cx> {
        if self.lanes(value.ty()).is_none() {
            return match mask {
                0b1100 | 0b0110 | 0b0011 | 0b1010 | 0b0101 | 0b1001 => {
                    self.ir.vector_splat(2, value, c"")
                }
                0b1101 | 0b1011 | 0b1110 | 0b0111 => self.ir.vector_splat(3, value, c""),
                0b1111 => self.ir.vector_splat(4, value, c""),
                _ => value,
            };
        }
        let s = |i| lane(swizzle, i);
        match mask {
            0b1 => self.ir.extract_element_u64(value, sw(swizzle, 0).into()),
            0b10 => self.ir.extract_element_u64(value, sw(swizzle, 1).into()),
            0b100 => self.ir.extract_element_u64(value, sw(swizzle, 2).into()),
            0b1000 => self.ir.extract_element_u64(value, sw(swizzle, 3).into()),
            0b1100 => self.ir.shuffle_vector1(value, &[s(2), s(3)]),
            0b0110 => self.ir.shuffle_vector1(value, &[s(1), s(2)]),
            0b0011 => self.ir.shuffle_vector1(value, &[s(0), s(1)]),
            0b1010 => self.ir.shuffle_vector1(value, &[s(1), s(3)]),
            0b0101 => self.ir.shuffle_vector1(value, &[s(0), s(2)]),
            0b1001 => self.ir.shuffle_vector1(value, &[s(0), s(3)]),
            0b1101 => self.ir.shuffle_vector1(value, &[s(0), s(2), s(3)]),
            0b1011 => self.ir.shuffle_vector1(value, &[s(0), s(1), s(3)]),
            0b1110 => self.ir.shuffle_vector1(value, &[s(1), s(2), s(3)]),
            0b0111 => self.ir.shuffle_vector1(value, &[s(0), s(1), s(2)]),
            0b1111 => self.ir.shuffle_vector1(value, &[s(0), s(1), s(2), s(3)]),
            _ => value,
        }
    }

    /// Element `element` of a vector, or the scalar itself (`ExtractElement`).
    fn element_or_scalar(&self, value: Val<'cx>, element: u64) -> Val<'cx> {
        if self.lanes(value.ty()).is_some() {
            self.ir.extract_element_u64(value, element)
        } else {
            value
        }
    }

    /// A vector of `count` (`VectorSplat`): one already that wide as it is.
    fn splat_unless_vector(&self, count: u32, value: Val<'cx>) -> Result<Val<'cx>, Error> {
        match self.lanes(value.ty()) {
            Some(n) if n == count => Ok(value),
            Some(_) => Err(invalid("a vector splat of a vector of another width")),
            None => Ok(self.ir.vector_splat(count, value, c"")),
        }
    }

    /// The `mask` components of a value read for `combined` (`ExtractFromCombinedMask`).
    fn extract_from_combined_mask(&self, value: Val<'cx>, combined: Mask, mask: Mask) -> Val<'cx> {
        if mask == combined {
            return value;
        }
        if matches!(mask, 0b1 | 0b10 | 0b100 | 0b1000) {
            return self.ir.extract_element_u64(value, ctz(mask).into());
        }
        let mut idx = Vec::new();
        let mut src = 0_i32;
        for bit in 0..4 {
            if combined & (1 << bit) != 0 {
                if mask & (1 << bit) != 0 {
                    idx.push(src);
                }
                src += 1;
            }
        }
        self.ir.shuffle_vector1(value, &idx)
    }

    /// Every bit set where `value & mask` is nonzero (`MaxIfInMask`).
    fn max_if_in_mask(&self, mask: u64, value: Val<'cx>) -> Val<'cx> {
        let max = self.ir.cx().all_ones(value.ty());
        let masked = self.ir.and(value, self.const_like(value.ty(), mask));
        self.ir.select(self.ir.is_null(masked), value, max)
    }

    fn and_imm(&self, v: Val<'cx>, imm: u64) -> Val<'cx> {
        self.ir.and(v, self.const_like(v.ty(), imm))
    }

    fn lshr_imm(&self, v: Val<'cx>, imm: u64) -> Val<'cx> {
        self.ir.lshr(v, self.const_like(v.ty(), imm))
    }

    fn decode_texture_buffer_offset(&self, metadata: Val<'cx>) -> Val<'cx> {
        self.ir.trunc(metadata, self.types.int)
    }

    fn decode_texture_min_lod_clamp(&self, metadata: Val<'cx>) -> Val<'cx> {
        self.ir
            .bitcast(self.ir.trunc(metadata, self.types.int), self.types.float)
    }

    fn decode_texture_array_length(&self, metadata: Val<'cx>) -> Val<'cx> {
        self.ir.trunc(self.lshr_imm(metadata, 32), self.types.int)
    }

    fn decode_raw_buffer_byte_length(&self, metadata: Val<'cx>) -> Val<'cx> {
        self.ir.trunc(metadata, self.types.int)
    }

    fn decode_texture_buffer_element(&self, metadata: Val<'cx>) -> Val<'cx> {
        self.ir.trunc(self.lshr_imm(metadata, 32), self.types.int)
    }

    /// An array slice rounded, made an integer and clamped to the texture's (`ClampArrayIndex`).
    fn clamp_array_index(&self, value: Val<'cx>, metadata: Val<'cx>) -> Result<Val<'cx>, Error> {
        let rounded = self.air.create_fp_un_op(FpUnOp::Rint, value, true)?;
        let integer = self.air.create_convert_to_signed(rounded)?;
        let positive = self
            .air
            .create_int_bin_op(IntBinOp::Max, integer, self.int(0), true)?;
        let max_index = self
            .ir
            .sub(self.decode_texture_array_length(metadata), self.int(1));
        self.air
            .create_int_bin_op(IntBinOp::Min, positive, max_index, true)
    }

    /// A buffer's `i32` element, null past its byte length (`CreateGEPInt32WithBoundCheck`).
    fn gep_int32_with_bound_check(&self, buffer: &BufferHandle<'cx>, index: Val<'cx>) -> Val<'cx> {
        let addr = self.ir.gep(self.types.int, buffer.pointer, &[index]);
        let Some(metadata) = buffer.metadata else {
            return addr;
        };
        let byte_length = self.decode_raw_buffer_byte_length(metadata);
        let in_bounds = self
            .ir
            .icmp(sys::INT_ULT, index, self.lshr_imm(byte_length, 2));
        self.ir
            .select(in_bounds, addr, self.ir.cx().null(addr.ty()))
    }

    /// An `i32` vector of two with `x` in its first lane (a 1D address as 2D).
    fn int2_x(&self, x: Val<'cx>) -> Val<'cx> {
        let zero = self.ir.cx().null(self.types.int2);
        self.ir.insert_element_u64(zero, x, 0)
    }

    /// A `float` vector of two with `x` in its first lane (a 1D coordinate as 2D).
    fn float2_x(&self, x: Val<'cx>) -> Val<'cx> {
        let zero = self.ir.cx().null(self.types.float2);
        self.ir.insert_element_u64(zero, x, 0)
    }

    // Instructions.

    /// Lowers one instruction.
    ///
    /// # Errors
    ///
    /// What the instruction's lowering refuses.
    pub fn convert(&self, inst: &Instruction) -> Result<(), Error> {
        match inst {
            Instruction::Nop => Ok(()),
            Instruction::PixelDiscard => self.air.create_discard().map(drop),
            Instruction::Sync(sync) => self.sync(sync),
            Instruction::Mov(i) => self.mov(i),
            Instruction::MovConditional(i) => self.movc(i),
            Instruction::SwapConditional(i) => self.swapc(i),
            Instruction::DotProduct(i) => self.dot(i),
            Instruction::FloatUnaryOp(i) => self.float_unary(i),
            Instruction::FloatBinaryOp(i) => self.float_binary(i),
            Instruction::IntegerUnaryOp(i) => self.integer_unary(i),
            Instruction::IntegerBinaryOp(i) => self.integer_binary(i),
            Instruction::PartialDerivative(i) => self.derivative(i),
            Instruction::Convert(i) => self.convert_op(i),
            Instruction::FloatCompare(i) => self.float_compare(i),
            Instruction::FloatMad(i) => self.float_mad(i),
            Instruction::SinCos(i) => self.sincos(i),
            Instruction::IntegerCompare(i) => self.integer_compare(i),
            Instruction::IntegerMad(i) => self.integer_mad(i),
            Instruction::IntegerBinaryOpWithTwoDst(i) => self.integer_binary_two(i),
            Instruction::ExtractBits(i) => self.extract_bits(i),
            Instruction::BitFieldInsert(i) => self.bfi(i),
            Instruction::Load(i) => self.ld(i),
            Instruction::LoadUavTyped(i) => self.ld_uav_typed(i),
            Instruction::StoreUavTyped(i) => self.store_uav_typed(i),
            Instruction::Sample(i) => self.sample(i),
            Instruction::SampleLod(i) => self.sample_l(i),
            Instruction::SampleBias(i) => self.sample_b(i),
            Instruction::SampleDerivative(i) => self.sample_d(i),
            Instruction::SampleCompare(i) => self.sample_c(i),
            Instruction::Gather(i) => self.gather(i),
            Instruction::GatherCompare(i) => self.gather_c(i),
            Instruction::CalcLod(i) => self.lod(i),
            Instruction::ResourceInfo(i) => self.resinfo(i),
            Instruction::SampleInfo(i) => self.sampleinfo(i),
            Instruction::SamplePos(i) => self.samplepos(i),
            Instruction::BufferInfo(i) => self.bufinfo(i),
            Instruction::LoadRaw(i) => self.ld_raw(i),
            Instruction::LoadStructured(i) => self.ld_structured(i),
            Instruction::StoreRaw(i) => self.store_raw(i),
            Instruction::StoreStructured(i) => self.store_structured(i),
            Instruction::AtomicBinOp(i) => self.atomic_bin_op(i),
            Instruction::AtomicImmCmpExchange(i) => self.atomic_cmp_exchange(i),
            Instruction::AtomicImmIncrement(i) => {
                self.atomic_counter(&i.dst, &i.uav, sys::ATOMIC_RMW_BIN_OP_ADD)
            }
            Instruction::AtomicImmDecrement(i) => {
                self.atomic_counter(&i.dst, &i.uav, sys::ATOMIC_RMW_BIN_OP_SUB)
            }
            Instruction::InterpolateCentroid(i) => {
                self.interpolate(&i.dst, i.regid, &i.read_swizzle, |itp| {
                    self.air
                        .create_interpolate_at_centroid(itp.handle, itp.perspective)
                })
            }
            Instruction::InterpolateSample(i) => {
                self.interpolate(&i.dst, i.regid, &i.read_swizzle, |itp| {
                    let sample = self.load_operand(&i.sample_index, MASK_X)?;
                    self.air
                        .create_interpolate_at_sample(itp.handle, sample, itp.perspective)
                })
            }
            Instruction::InterpolateOffset(i) => {
                self.interpolate(&i.dst, i.regid, &i.read_swizzle, |itp| {
                    let offset = self.load_operand(&i.offset, MASK_XY)?;
                    // truncated = (offset.xy + 8) & 0b1111
                    let truncated = self.ir.and(
                        self.ir.add(offset, self.air.get_int2(8, 8)),
                        self.air.get_int2(0b1111, 0b1111),
                    );
                    let offset_float = self.ir.fmul(
                        self.air
                            .create_convert_to_float(truncated, Signedness::Signed)?,
                        self.air.get_float2(1.0 / 16.0, 1.0 / 16.0),
                    );
                    self.air
                        .create_interpolate_at_offset(itp.handle, offset_float, itp.perspective)
                })
            }
            Instruction::MaskedSumOfAbsDiff(i) => self.msad(i),
            Instruction::Emit => self
                .geometry
                .ok_or_else(|| invalid("emit outside a geometry shader"))?
                .emit(),
            Instruction::Cut => self
                .geometry
                .ok_or_else(|| invalid("cut outside a geometry shader"))?
                .cut(),
        }
    }

    // MacNeutron: a UAV sync fences at its scope at any Metal version, then waits for the group
    // if asked (a barrier's memory flags order only within the group); a TGSM fence without the
    // group sync (GroupMemoryBarrier) is a fence alone.
    fn sync(&self, sync: &InstSync) -> Result<(), Error> {
        let mut mem_flag = if sync.tgsm_memory_barrier {
            MemFlags::THREADGROUP
        } else {
            MemFlags::NONE
        };
        let uav = sync.uav_boundary != UavBoundary::None;
        if uav {
            mem_flag = mem_flag.with(MemFlags::DEVICE).with(MemFlags::TEXTURE);
        }
        if uav || (!sync.tgsm_execution_barrier && mem_flag.bits() != 0) {
            // ThreadScope::Device | ThreadScope::Threadgroup, or Threadgroup.
            let scope = if sync.uav_boundary == UavBoundary::Global {
                3
            } else {
                1
            };
            self.atomic_fence(mem_flag, scope)?;
        }
        if sync.tgsm_execution_barrier {
            let flags = if uav && self.supports_non_execution_barrier() {
                MemFlags::NONE
            } else {
                mem_flag
            };
            self.air.create_barrier(flags, false)?;
        }
        Ok(())
    }

    fn mov(&self, mov: &InstMov) -> Result<(), Error> {
        let _fm = self.use_fast_math(mov.common.precise_mask);
        let mask = dst_mask(&mov.dst);
        let value = self.load_operand(&mov.src, mask)?;
        self.store_operand(&mov.dst, value, mov.common.saturate)
    }

    fn movc(&self, movc: &InstMovConditional) -> Result<(), Error> {
        let _fm = self.use_fast_math(movc.common.precise_mask);
        let mask = dst_mask(&movc.dst);
        let src0 = self.load_operand(&movc.src0, mask)?;
        let src1 = self.load_operand(&movc.src1, mask)?;
        let cond = self.load_operand(&movc.src_cond, mask)?;
        let zero = self.ir.cx().null(cond.ty());
        let result = self
            .ir
            .select(self.ir.icmp(sys::INT_NE, cond, zero), src0, src1);
        self.store_operand(&movc.dst, result, movc.common.saturate)
    }

    fn swapc(&self, swapc: &InstSwapConditional) -> Result<(), Error> {
        let mask0 = dst_mask(&swapc.dst0);
        let mask1 = dst_mask(&swapc.dst1);
        let combined = mask0 | mask1;
        let src0 = self.load_operand(&swapc.src0, combined)?;
        let src1 = self.load_operand(&swapc.src1, combined)?;
        let cond = self.load_operand(&swapc.src_cond, combined)?;
        let pick = |mask, then, otherwise| {
            let c = self
                .ir
                .is_not_null(self.extract_from_combined_mask(cond, combined, mask));
            let t = self.extract_from_combined_mask(then, combined, mask);
            let o = self.extract_from_combined_mask(otherwise, combined, mask);
            self.ir.select(c, t, o)
        };
        let result0 = pick(mask0, src1, src0);
        let result1 = pick(mask1, src0, src1);
        self.store_operand(&swapc.dst0, result0, false)?;
        self.store_operand(&swapc.dst1, result1, false)
    }

    fn dot(&self, dot: &InstDotProduct) -> Result<(), Error> {
        let _fm = self.use_fast_math(dot.common.precise_mask);
        let mask = match dot.dimension {
            2 => MASK_XY,
            3 => MASK_XYZ,
            4 => MASK_ALL,
            _ => return Ok(()),
        };
        let lhs = self.load_operand(&dot.src0, mask)?;
        let rhs = self.load_operand(&dot.src1, mask)?;
        let result = self.air.create_dot_product(lhs, rhs)?;
        self.store_operand(&dot.dst, result, dot.common.saturate)
    }

    fn float_unary(&self, unary: &InstFloatUnaryOp) -> Result<(), Error> {
        let _fm = self.use_fast_math(unary.common.precise_mask);
        let mask = dst_mask(&unary.dst);
        let value = self.load_operand(&unary.src, mask)?;
        let op = |op| self.air.create_fp_un_op(op, value, true);
        let value = match unary.op {
            FloatUnaryOp::Log2 => op(FpUnOp::Log2)?,
            FloatUnaryOp::Exp2 => op(FpUnOp::Exp2)?,
            FloatUnaryOp::Rcp => {
                let one = self.ir.cx().const_real(value.ty(), 1.0);
                self.ir.fdiv(one, value)
            }
            FloatUnaryOp::Rsq => op(FpUnOp::Rsqrt)?,
            FloatUnaryOp::Sqrt => op(FpUnOp::Sqrt)?,
            FloatUnaryOp::Fraction => op(FpUnOp::Fract)?,
            FloatUnaryOp::RoundNearestEven => op(FpUnOp::Rint)?,
            FloatUnaryOp::RoundNegativeInf => op(FpUnOp::Floor)?,
            FloatUnaryOp::RoundPositiveInf => op(FpUnOp::Ceil)?,
            FloatUnaryOp::RoundZero => op(FpUnOp::Trunc)?,
        };
        self.store_operand(&unary.dst, value, unary.common.saturate)
    }

    fn float_binary(&self, bin: &InstFloatBinaryOp) -> Result<(), Error> {
        let _fm = self.use_fast_math(bin.common.precise_mask);
        let mask = dst_mask(&bin.dst);
        let lhs = self.load_operand(&bin.src0, mask)?;
        let rhs = self.load_operand(&bin.src1, mask)?;
        let result = match bin.op {
            FloatBinaryOp::Add => self.ir.fadd(lhs, rhs),
            FloatBinaryOp::Mul => self.ir.fmul(lhs, rhs),
            FloatBinaryOp::Div => self.ir.fdiv(lhs, rhs),
            FloatBinaryOp::Min => self.air.create_fp_bin_op(FpBinOp::Fmin, lhs, rhs, true)?,
            FloatBinaryOp::Max => self.air.create_fp_bin_op(FpBinOp::Fmax, lhs, rhs, true)?,
        };
        self.store_operand(&bin.dst, result, bin.common.saturate)
    }

    fn integer_unary(&self, unary: &InstIntegerUnaryOp) -> Result<(), Error> {
        let mask = dst_mask(&unary.dst);
        let value = self.load_operand(&unary.src, mask)?;
        let first_bit_mask = u64::from(!0x1f_u32);
        let value = match unary.op {
            IntegerUnaryOp::Neg => self.ir.neg(value),
            IntegerUnaryOp::Not => self.ir.not(value),
            IntegerUnaryOp::ReverseBits => {
                self.air.create_int_un_op(IntUnOp::ReverseBits, value)?
            }
            IntegerUnaryOp::CountBits => self.air.create_int_un_op(IntUnOp::Popcount, value)?,
            IntegerUnaryOp::FirstHiBitSigned => {
                let not_neg = self.ir.is_not_neg(value);
                let positive = self.air.create_count_zero(value, false)?;
                let shifted = self
                    .ir
                    .shl(self.ir.not(value), self.const_like(value.ty(), 1));
                let negative = self.ir.add(
                    self.air.create_count_zero(shifted, false)?,
                    self.const_like(value.ty(), 1),
                );
                let v = self.ir.select(not_neg, positive, negative);
                self.max_if_in_mask(first_bit_mask, v)
            }
            IntegerUnaryOp::FirstHiBit => {
                let v = self.air.create_count_zero(value, false)?;
                self.max_if_in_mask(first_bit_mask, v)
            }
            IntegerUnaryOp::FirstLowBit => {
                let v = self.air.create_count_zero(value, true)?;
                self.max_if_in_mask(first_bit_mask, v)
            }
        };
        self.store_operand(&unary.dst, value, false)
    }

    fn integer_binary(&self, bin: &InstIntegerBinaryOp) -> Result<(), Error> {
        let mask = dst_mask(&bin.dst);
        let lhs = self.load_operand(&bin.src0, mask)?;
        let rhs = self.load_operand(&bin.src1, mask)?;
        let result = match bin.op {
            IntegerBinaryOp::UMin => self.air.create_int_bin_op(IntBinOp::Min, lhs, rhs, false)?,
            IntegerBinaryOp::UMax => self.air.create_int_bin_op(IntBinOp::Max, lhs, rhs, false)?,
            IntegerBinaryOp::IMin => self.air.create_int_bin_op(IntBinOp::Min, lhs, rhs, true)?,
            IntegerBinaryOp::IMax => self.air.create_int_bin_op(IntBinOp::Max, lhs, rhs, true)?,
            IntegerBinaryOp::IShl => self.ir.shl(lhs, self.and_imm(rhs, 0x1f)),
            IntegerBinaryOp::IShr => self.ir.ashr(lhs, self.and_imm(rhs, 0x1f)),
            IntegerBinaryOp::UShr => self.ir.lshr(lhs, self.and_imm(rhs, 0x1f)),
            IntegerBinaryOp::Xor => self.ir.xor(lhs, rhs),
            IntegerBinaryOp::Or => self.ir.or(lhs, rhs),
            IntegerBinaryOp::And => self.ir.and(lhs, rhs),
            IntegerBinaryOp::Add => self.ir.add(lhs, rhs),
        };
        self.store_operand(&bin.dst, result, false)
    }

    fn derivative(&self, deriv: &InstPartialDerivative) -> Result<(), Error> {
        let _fm = self.use_fast_math(deriv.common.precise_mask);
        let mask = dst_mask(&deriv.dst);
        let src = self.load_operand(&deriv.src, mask)?;
        let result = self.air.create_derivative(src, deriv.ddy)?;
        self.store_operand(&deriv.dst, result, deriv.common.saturate)
    }

    fn convert_op(&self, convert: &InstConvert) -> Result<(), Error> {
        let mask = dst_mask(&convert.dst);
        let value = self.load_operand(&convert.src, mask)?;
        let value = match convert.op {
            ConversionOp::HalfToFloat | ConversionOp::SignedToFloat => self
                .air
                .create_convert_to_float(value, Signedness::Signed)?,
            ConversionOp::FloatToHalf => {
                self.air.create_convert_to_half(value, Signedness::Signed)?
            }
            ConversionOp::FloatToSigned => self.air.create_convert_to_signed(value)?,
            ConversionOp::FloatToUnsigned => self.air.create_convert_to_unsigned(value)?,
            ConversionOp::UnsignedToFloat => self
                .air
                .create_convert_to_float(value, Signedness::Unsigned)?,
        };
        self.store_operand(&convert.dst, value, false)
    }

    fn float_compare(&self, cmp: &InstFloatCompare) -> Result<(), Error> {
        let _fm = self.use_fast_math(cmp.common.precise_mask);
        let mask = dst_mask(&cmp.dst);
        let lhs = self.load_operand(&cmp.src0, mask)?;
        let rhs = self.load_operand(&cmp.src1, mask)?;
        let pred = match cmp.cmp {
            FloatComparison::Equal => sys::REAL_OEQ,
            FloatComparison::NotEqual => sys::REAL_UNE,
            FloatComparison::GreaterEqual => sys::REAL_OGE,
            FloatComparison::LessThan => sys::REAL_OLT,
        };
        let result = self.ir.fcmp(pred, lhs, rhs);
        let wide = self.ir.sext(result, self.with_bits(result.ty(), 32));
        self.store_operand(&cmp.dst, wide, false)
    }

    fn float_mad(&self, mad: &InstFloatMad) -> Result<(), Error> {
        let _fm = self.use_fast_math(mad.common.precise_mask);
        let mask = dst_mask(&mad.dst);
        let a = self.load_operand(&mad.src0, mask)?;
        let b = self.load_operand(&mad.src1, mask)?;
        let c = self.load_operand(&mad.src2, mask)?;
        let result = self.air.create_fma(a, b, c)?;
        self.store_operand(&mad.dst, result, mad.common.saturate)
    }

    fn sincos(&self, sincos: &InstSinCos) -> Result<(), Error> {
        let _fm = self.use_fast_math(sincos.common.precise_mask);
        let mask_cos = dst_mask(&sincos.dst_cos);
        let mask_sin = dst_mask(&sincos.dst_sin);
        let combined = mask_cos | mask_sin;
        let src = self.load_operand(&sincos.src, combined)?;
        let src_cos = self.extract_from_combined_mask(src, combined, mask_cos);
        let src_sin = self.extract_from_combined_mask(src, combined, mask_sin);
        if !is_null(&sincos.dst_cos) {
            let v = self.air.create_fp_un_op(FpUnOp::Cos, src_cos, true)?;
            self.store_operand(&sincos.dst_cos, v, sincos.common.saturate)?;
        }
        if !is_null(&sincos.dst_sin) {
            let v = self.air.create_fp_un_op(FpUnOp::Sin, src_sin, true)?;
            self.store_operand(&sincos.dst_sin, v, sincos.common.saturate)?;
        }
        Ok(())
    }

    fn integer_compare(&self, cmp: &InstIntegerCompare) -> Result<(), Error> {
        let mask = dst_mask(&cmp.dst);
        let lhs = self.load_operand(&cmp.src0, mask)?;
        let rhs = self.load_operand(&cmp.src1, mask)?;
        let pred = match cmp.cmp {
            IntegerComparison::Equal => sys::INT_EQ,
            IntegerComparison::NotEqual => sys::INT_NE,
            IntegerComparison::SignedLessThan => sys::INT_SLT,
            IntegerComparison::SignedGreaterEqual => sys::INT_SGE,
            IntegerComparison::UnsignedLessThan => sys::INT_ULT,
            IntegerComparison::UnsignedGreaterEqual => sys::INT_UGE,
        };
        let result = self.ir.icmp(pred, lhs, rhs);
        let wide = self.ir.sext(result, self.with_bits(result.ty(), 32));
        self.store_operand(&cmp.dst, wide, false)
    }

    fn integer_mad(&self, mad: &InstIntegerMad) -> Result<(), Error> {
        let mask = dst_mask(&mad.dst);
        let a = self.load_operand(&mad.src0, mask)?;
        let b = self.load_operand(&mad.src1, mask)?;
        let c = self.load_operand(&mad.src2, mask)?;
        let result = self.ir.add(self.ir.mul(a, b), c);
        self.store_operand(&mad.dst, result, false)
    }

    fn integer_binary_two(&self, bin: &InstIntegerBinaryOpWithTwoDst) -> Result<(), Error> {
        let mask_hi = dst_mask(&bin.dst_hi);
        let mask_lo = dst_mask(&bin.dst_low);
        let combined = mask_hi | mask_lo;
        let src0 = self.load_operand(&bin.src0, combined)?;
        let src1 = self.load_operand(&bin.src1, combined)?;
        let src0_hi = self.extract_from_combined_mask(src0, combined, mask_hi);
        let src0_lo = self.extract_from_combined_mask(src0, combined, mask_lo);
        let src1_hi = self.extract_from_combined_mask(src1, combined, mask_hi);
        let src1_lo = self.extract_from_combined_mask(src1, combined, mask_lo);
        let hi = !is_null(&bin.dst_hi);
        let lo = !is_null(&bin.dst_low);
        match bin.op {
            IntegerBinaryOpWithTwoDst::IMul | IntegerBinaryOpWithTwoDst::UMul => {
                let signed = bin.op == IntegerBinaryOpWithTwoDst::IMul;
                if hi {
                    let v =
                        self.air
                            .create_int_bin_op(IntBinOp::MulHi, src0_hi, src1_hi, signed)?;
                    self.store_operand(&bin.dst_hi, v, false)?;
                }
                if lo {
                    self.store_operand(&bin.dst_low, self.ir.mul(src0_lo, src1_lo), false)?;
                }
            }
            IntegerBinaryOpWithTwoDst::UDiv => {
                if hi {
                    let v = self.ir.select(
                        self.ir.is_null(src1_hi),
                        self.ir.cx().all_ones(src1_hi.ty()),
                        self.ir.udiv(src0_hi, src1_hi),
                    );
                    self.store_operand(&bin.dst_hi, v, false)?;
                }
                if lo {
                    let v = self.ir.select(
                        self.ir.is_null(src1_lo),
                        self.ir.cx().all_ones(src1_lo.ty()),
                        self.ir.urem(src0_lo, src1_lo),
                    );
                    self.store_operand(&bin.dst_low, v, false)?;
                }
            }
            IntegerBinaryOpWithTwoDst::UAddCarry | IntegerBinaryOpWithTwoDst::USubBorrow => {
                let add = bin.op == IntegerBinaryOpWithTwoDst::UAddCarry;
                if hi {
                    let v = if add {
                        self.ir.add(src0_hi, src1_hi)
                    } else {
                        self.ir.sub(src0_hi, src1_hi)
                    };
                    self.store_operand(&bin.dst_hi, v, false)?;
                }
                if lo {
                    let name: &[u8] = if add {
                        b"llvm.uadd.with.overflow"
                    } else {
                        b"llvm.usub.with.overflow"
                    };
                    let tuple = self
                        .ir
                        .binary_intrinsic(name, src0_lo, src1_lo)
                        .ok_or_else(|| invalid("an overflow intrinsic was not declared"))?;
                    let flag = self.ir.extract_value(tuple, 1);
                    let wide = self.ir.zext(flag, self.with_bits(flag.ty(), 32));
                    self.store_operand(&bin.dst_low, wide, false)?;
                }
            }
        }
        Ok(())
    }

    fn extract_bits(&self, extract: &InstExtractBits) -> Result<(), Error> {
        let mask = dst_mask(&extract.dst);
        let src0 = self.load_operand(&extract.src0, mask)?;
        let src1 = self.load_operand(&extract.src1, mask)?;
        let src2 = self.load_operand(&extract.src2, mask)?;
        let width = self.and_imm(src0, 0x1f);
        let offset = self.and_imm(src1, 0x1f);
        let width_add_offset = self.ir.add(width, offset);
        let c32 = self.const_like(width.ty(), 32);
        let clamped_src2 = self.ir.shl(src2, self.ir.sub(c32, width_add_offset));
        let (need_clamp, no_clamp, clamped) = if extract.is_signed {
            (
                self.ir.icmp(sys::INT_SLT, width_add_offset, c32),
                self.ir.ashr(src2, offset),
                self.ir.ashr(clamped_src2, self.ir.sub(c32, width)),
            )
        } else {
            (
                self.ir.icmp(sys::INT_ULT, width_add_offset, c32),
                self.ir.lshr(src2, offset),
                self.ir.lshr(clamped_src2, self.ir.sub(c32, width)),
            )
        };
        let result = self.ir.select(
            self.ir.is_null(width),
            self.const_like(width.ty(), 0),
            self.ir.select(need_clamp, clamped, no_clamp),
        );
        self.store_operand(&extract.dst, result, false)
    }

    fn bfi(&self, bfi: &InstBitFieldInsert) -> Result<(), Error> {
        let mask = dst_mask(&bfi.dst);
        let src0 = self.load_operand(&bfi.src0, mask)?;
        let src1 = self.load_operand(&bfi.src1, mask)?;
        let src2 = self.load_operand(&bfi.src2, mask)?;
        let src3 = self.load_operand(&bfi.src3, mask)?;
        let width = self.and_imm(src0, 0x1f);
        let offset = self.and_imm(src1, 0x1f);
        let c1 = self.const_like(width.ty(), 1);
        let bitmask = self.ir.shl(self.ir.sub(self.ir.shl(c1, width), c1), offset);
        let result = self.ir.or(
            self.ir.and(self.ir.shl(src2, offset), bitmask),
            self.ir.and(src3, self.ir.not(bitmask)),
        );
        self.store_operand(&bfi.dst, result, false)
    }

    fn ld(&self, load: &InstLoad) -> Result<(), Error> {
        let Some(tex) = self.load_srv_texture(&load.src_resource)? else {
            return Ok(());
        };
        let [o0, o1, o2] = load.offsets.map(i32::cast_unsigned);
        let mut array_index = None;
        let mut sample_index = None;
        let mut oob = None;
        let sample = || -> Result<Val<'cx>, Error> {
            let src = load
                .src_sample_index
                .as_ref()
                .ok_or_else(|| invalid("a multisampled load without a sample index"))?;
            self.load_operand(src, MASK_X)
        };
        let (mut address, offset) = match tex.logical {
            Kind::TextureBuffer => {
                let address = self.load_operand(&load.src_address, MASK_X)?;
                let offset = self.int(o0);
                oob = Some(self.ir.icmp(
                    sys::INT_UGE,
                    self.ir.add(address, offset),
                    self.decode_texture_buffer_element(tex.metadata),
                ));
                let address = self
                    .ir
                    .add(address, self.decode_texture_buffer_offset(tex.metadata));
                (address, offset)
            }
            Kind::Texture1d => {
                let address = self.int2_x(self.load_operand(&load.src_address, MASK_X)?);
                (address, self.air.get_int2(o0, 0))
            }
            Kind::Texture1dArray => {
                let address = self.int2_x(self.load_operand(&load.src_address, MASK_X)?);
                array_index = Some(self.load_operand(&load.src_address, MASK_Y)?);
                (address, self.air.get_int2(o0, 0))
            }
            Kind::Texture2d | Kind::Depth2d => (
                self.load_operand(&load.src_address, MASK_XY)?,
                self.air.get_int2(o0, o1),
            ),
            Kind::Texture2dArray | Kind::Depth2dArray => {
                let address = self.load_operand(&load.src_address, MASK_XY)?;
                array_index = Some(self.load_operand(&load.src_address, MASK_Z)?);
                (address, self.air.get_int2(o0, o1))
            }
            Kind::Texture3d => (
                self.load_operand(&load.src_address, MASK_XYZ)?,
                self.air.get_int3(o0, o1, o2),
            ),
            Kind::Texture2dMs | Kind::Depth2dMs => {
                let address = self.load_operand(&load.src_address, MASK_XY)?;
                sample_index = Some(sample()?);
                (address, self.air.get_int2(o0, o1))
            }
            Kind::Texture2dMsArray | Kind::Depth2dMsArray => {
                let address = self.load_operand(&load.src_address, MASK_XY)?;
                array_index = Some(self.load_operand(&load.src_address, MASK_Z)?);
                sample_index = Some(sample()?);
                (address, self.air.get_int2(o0, o1))
            }
            _ => return Ok(()),
        };
        // SAFETY: a live constant.
        if unsafe { sys::is_null(offset.raw()) } == 0 {
            address = self.ir.add(address, offset);
        }
        if let Some(oob) = oob {
            address = self
                .ir
                .select(oob, self.ir.cx().all_ones(address.ty()), address);
        }
        let level = self.load_operand(&load.src_address, MASK_W)?;
        let at = TexelAt {
            pos: address,
            array_index,
            sample_or_face: sample_index,
            level: Some(level),
        };
        let (value, _) =
            self.air
                .create_read(&tex.texture, tex.handle, &at, tex.global_coherent)?;
        let swizzled = self.mask_swizzle(value, dst_mask(&load.dst), &tex.swizzle);
        self.store_operand(&load.dst, swizzled, false)
    }

    /// A typed view's address by its declared kind, and its slice.
    ///
    /// For the UAV loads, stores and atomics; multisampled kinds are not views of these.
    fn uav_address(
        &self,
        tex: &TextureHandle<'cx>,
        src: &SrcOperand,
    ) -> Result<Option<(Val<'cx>, Option<Val<'cx>>)>, Error> {
        Ok(Some(match tex.logical {
            Kind::TextureBuffer => (self.load_operand(src, MASK_X)?, None),
            Kind::Texture1d => (self.int2_x(self.load_operand(src, MASK_X)?), None),
            Kind::Texture1dArray => {
                let address = self.int2_x(self.load_operand(src, MASK_X)?);
                (address, Some(self.load_operand(src, MASK_Y)?))
            }
            Kind::Texture2d | Kind::Depth2d => (self.load_operand(src, MASK_XY)?, None),
            Kind::Texture2dArray | Kind::Depth2dArray => {
                let address = self.load_operand(src, MASK_XY)?;
                (address, Some(self.load_operand(src, MASK_Z)?))
            }
            Kind::Texture3d => (self.load_operand(src, MASK_XYZ)?, None),
            _ => return Ok(None),
        }))
    }

    fn ld_uav_typed(&self, load: &InstLoadUavTyped) -> Result<(), Error> {
        let uav = &load.src_uav;
        let Some(tex) =
            self.load_uav_texture(uav.range_id, &uav.index, uav.read_swizzle.clone())?
        else {
            return Ok(());
        };
        let Some((mut address, array_index)) = self.uav_address(&tex, &load.src_address)? else {
            return Ok(());
        };
        let mut oob = None;
        if tex.logical == Kind::TextureBuffer {
            oob = Some(self.ir.icmp(
                sys::INT_UGE,
                address,
                self.decode_texture_buffer_element(tex.metadata),
            ));
            address = self
                .ir
                .add(address, self.decode_texture_buffer_offset(tex.metadata));
        }
        if matches!(tex.texture.memory_access, MemoryAccess::ReadWrite) {
            self.air.create_texture_fence(&tex.texture, tex.handle)?;
        }
        if let Some(oob) = oob {
            address = self
                .ir
                .select(oob, self.ir.cx().all_ones(address.ty()), address);
        }
        let at = TexelAt {
            pos: address,
            array_index,
            sample_or_face: None,
            level: Some(self.int(0)),
        };
        let (value, _) =
            self.air
                .create_read(&tex.texture, tex.handle, &at, tex.global_coherent)?;
        let swizzled = self.mask_swizzle(value, dst_mask(&load.dst), &tex.swizzle);
        self.store_operand(&load.dst, swizzled, false)
    }

    fn store_uav_typed(&self, store: &InstStoreUavTyped) -> Result<(), Error> {
        let Some(tex) =
            self.load_uav_texture(store.dst.range_id, &store.dst.index, SWIZZLE_IDENTITY)?
        else {
            return Ok(());
        };
        // The store takes no depth kind (the C++'s switch had none).
        if matches!(tex.logical, Kind::Depth2d | Kind::Depth2dArray) {
            return Ok(());
        }
        let Some((mut address, array_index)) = self.uav_address(&tex, &store.src_address)? else {
            return Ok(());
        };
        if tex.logical == Kind::TextureBuffer {
            address = self
                .ir
                .add(address, self.decode_texture_buffer_offset(tex.metadata));
        }
        let value = self.load_operand(&store.src, MASK_ALL)?;
        let at = TexelAt {
            pos: address,
            array_index,
            sample_or_face: None,
            level: Some(self.int(0)),
        };
        self.air
            .create_write(&tex.texture, tex.handle, &at, value, tex.global_coherent)?;
        Ok(())
    }

    /// A texture and its sampler, the instruction's resource and sampler operands.
    fn texture_and_sampler(
        &self,
        resource: &SrcOperandResource,
        sampler: &SrcOperandSampler,
    ) -> Result<Option<(TextureHandle<'cx>, SamplerHandle<'cx>)>, Error> {
        let Some(tex) = self.load_srv_texture(resource)? else {
            return Ok(None);
        };
        let Some(sampler) = self.load_sampler(sampler)? else {
            return Ok(None);
        };
        Ok(Some((tex, sampler)))
    }

    /// The minimum level: the texture's, raised by the instruction's clamp where it has one.
    fn min_lod_clamp(
        &self,
        tex: &TextureHandle<'cx>,
        clamp: Option<&SrcOperand>,
    ) -> Result<Val<'cx>, Error> {
        let min = self.decode_texture_min_lod_clamp(tex.metadata);
        let Some(clamp) = clamp else {
            return Ok(min);
        };
        let shader_clamp = self.load_operand(clamp, MASK_X)?;
        self.air
            .create_fp_bin_op(FpBinOp::Fmax, min, shader_clamp, true)
    }

    /// A sample's coordinates by the texture's declared kind.
    ///
    /// With the derivatives (`sample_d`'s, read after the coordinates) and the slice, clamped;
    /// `None` for a kind `allowed` refuses.
    fn sample_coord(
        &self,
        tex: &TextureHandle<'cx>,
        sampler: &SamplerHandle<'cx>,
        address: &SrcOperand,
        derivatives: Option<(&SrcOperand, &SrcOperand)>,
        allowed: fn(Kind) -> bool,
    ) -> Result<Option<SampleCoord<'cx>>, Error> {
        if !allowed(tex.logical) {
            return Ok(None);
        }
        let (mask, one_d, slice, cube) = match tex.logical {
            Kind::Texture1d => (MASK_X, true, None, false),
            Kind::Texture1dArray => (MASK_X, true, Some(MASK_Y), false),
            Kind::Texture2d | Kind::Depth2d => (MASK_XY, false, None, false),
            Kind::Texture2dArray | Kind::Depth2dArray => (MASK_XY, false, Some(MASK_Z), false),
            Kind::Texture3d => (MASK_XYZ, false, None, false),
            Kind::TextureCube | Kind::DepthCube => (MASK_XYZ, false, None, true),
            Kind::TextureCubeArray | Kind::DepthCubeArray => (MASK_XYZ, false, Some(MASK_W), true),
            _ => return Ok(None),
        };
        let read = |src: &SrcOperand| -> Result<Val<'cx>, Error> {
            let v = self.load_operand(src, mask)?;
            Ok(if one_d { self.float2_x(v) } else { v })
        };
        let coord = read(address)?;
        let derivatives = match derivatives {
            Some((dx, dy)) => {
                let dx = read(dx)?;
                Some((dx, read(dy)?))
            }
            None => None,
        };
        let array_index = match slice {
            Some(slice) => {
                let index = self.load_operand(address, slice)?;
                Some(self.clamp_array_index(index, tex.metadata)?)
            }
            None => None,
        };
        Ok(Some(SampleCoord {
            coord,
            array_index,
            sampler: if cube {
                sampler.handle_cube
            } else {
                sampler.handle
            },
            derivatives,
        }))
    }

    fn store_texel(
        &self,
        dst: &DstOperand,
        value: Val<'cx>,
        swizzle: &Swizzle,
    ) -> Result<(), Error> {
        let swizzled = self.mask_swizzle(value, dst_mask(dst), swizzle);
        self.store_operand(dst, swizzled, false)
    }

    fn sample(&self, sample: &InstSample) -> Result<(), Error> {
        let Some((tex, sampler)) =
            self.texture_and_sampler(&sample.src_resource, &sample.src_sampler)?
        else {
            return Ok(());
        };
        let min_lod = self.min_lod_clamp(&tex, sample.min_lod_clamp.as_ref())?;
        let Some(c) = self.sample_coord(&tex, &sampler, &sample.src_address, None, sampled)? else {
            return Ok(());
        };
        let at = SampleAt {
            coord: c.coord,
            array_index: c.array_index,
            offset: self.air.get_offset(&tex.texture, sample.offsets),
        };
        let (value, _) = self.air.create_sample(
            &tex.texture,
            tex.handle,
            c.sampler,
            &at,
            &Lod::BiasMinLod(sampler.bias, min_lod),
        )?;
        self.store_texel(&sample.dst, value, &tex.swizzle)
    }

    fn sample_l(&self, sample: &InstSampleLod) -> Result<(), Error> {
        let Some((tex, sampler)) =
            self.texture_and_sampler(&sample.src_resource, &sample.src_sampler)?
        else {
            return Ok(());
        };
        let Some(c) = self.sample_coord(&tex, &sampler, &sample.src_address, None, sampled)? else {
            return Ok(());
        };
        let lod = self
            .ir
            .fadd(self.load_operand(&sample.src_lod, MASK_X)?, sampler.bias);
        let at = SampleAt {
            coord: c.coord,
            array_index: c.array_index,
            offset: self.air.get_offset(&tex.texture, sample.offsets),
        };
        let (value, _) =
            self.air
                .create_sample(&tex.texture, tex.handle, c.sampler, &at, &Lod::Level(lod))?;
        self.store_texel(&sample.dst, value, &tex.swizzle)
    }

    fn sample_b(&self, sample: &InstSampleBias) -> Result<(), Error> {
        let Some((tex, sampler)) =
            self.texture_and_sampler(&sample.src_resource, &sample.src_sampler)?
        else {
            return Ok(());
        };
        let min_lod = self.min_lod_clamp(&tex, sample.min_lod_clamp.as_ref())?;
        let Some(c) = self.sample_coord(&tex, &sampler, &sample.src_address, None, sampled)? else {
            return Ok(());
        };
        let bias = self
            .ir
            .fadd(sampler.bias, self.load_operand(&sample.src_bias, MASK_X)?);
        let at = SampleAt {
            coord: c.coord,
            array_index: c.array_index,
            offset: self.air.get_offset(&tex.texture, sample.offsets),
        };
        let (value, _) = self.air.create_sample(
            &tex.texture,
            tex.handle,
            c.sampler,
            &at,
            &Lod::BiasMinLod(bias, min_lod),
        )?;
        self.store_texel(&sample.dst, value, &tex.swizzle)
    }

    fn sample_d(&self, sample: &InstSampleDerivative) -> Result<(), Error> {
        let Some((tex, sampler)) =
            self.texture_and_sampler(&sample.src_resource, &sample.src_sampler)?
        else {
            return Ok(());
        };
        let min_lod = self.min_lod_clamp(&tex, sample.min_lod_clamp.as_ref())?;
        let Some(c) = self.sample_coord(
            &tex,
            &sampler,
            &sample.src_address,
            Some((&sample.src_x_derivative, &sample.src_y_derivative)),
            sampled,
        )?
        else {
            return Ok(());
        };
        let (ddx, ddy) = c
            .derivatives
            .ok_or_else(|| invalid("a gradient sample without derivatives"))?;
        // MacNeutron: D3D adds the sampler's MipLODBias to sample_d's LOD; Metal's gradient
        // sample takes no bias. Gradients scaled by 2^bias move the LOD by bias, also past the
        // anisotropy cap.
        let n = self
            .lanes(ddx.ty())
            .ok_or_else(|| invalid("a scalar gradient"))?;
        let exp2 = self
            .air
            .create_fp_un_op(FpUnOp::Exp2, sampler.bias, false)?;
        let scale = self.ir.vector_splat(n, exp2, c"");
        let ddx = self.ir.fmul(ddx, scale);
        let ddy = self.ir.fmul(ddy, scale);
        let at = SampleAt {
            coord: c.coord,
            array_index: c.array_index,
            offset: self.air.get_offset(&tex.texture, sample.offsets),
        };
        let grad = Gradient {
            dx: ddx,
            dy: ddy,
            min_lod,
        };
        let (value, _) =
            self.air
                .create_sample_grad(&tex.texture, tex.handle, c.sampler, &at, &grad)?;
        self.store_texel(&sample.dst, value, &tex.swizzle)
    }

    fn sample_c(&self, sample: &InstSampleCompare) -> Result<(), Error> {
        let Some((tex, sampler)) =
            self.texture_and_sampler(&sample.src_resource, &sample.src_sampler)?
        else {
            return Ok(());
        };
        let min_lod = self.min_lod_clamp(&tex, sample.min_lod_clamp.as_ref())?;
        let reference = self.load_operand(&sample.src_reference, MASK_X)?;
        let Some(c) = self.sample_coord(&tex, &sampler, &sample.src_address, None, compared)?
        else {
            return Ok(());
        };
        let at = SampleAt {
            coord: c.coord,
            array_index: c.array_index,
            offset: self.air.get_offset(&tex.texture, sample.offsets),
        };
        let lod = if sample.level_zero {
            Lod::Level(self.air.get_float(0.0))
        } else {
            Lod::BiasMinLod(sampler.bias, min_lod)
        };
        let (value, _) = self.air.create_sample_cmp(
            &tex.texture,
            tex.handle,
            c.sampler,
            &at,
            reference,
            &lod,
        )?;
        self.store_texel(&sample.dst, value, &tex.swizzle)
    }

    /// A gather's offset, read after the coordinates and slice: two lanes, three for cubes.
    fn gather_offset(
        &self,
        tex: &TextureHandle<'cx>,
        offset: &SrcOperand,
    ) -> Result<Val<'cx>, Error> {
        let mask = match tex.logical {
            Kind::TextureCube | Kind::DepthCube | Kind::TextureCubeArray | Kind::DepthCubeArray => {
                MASK_XYZ
            }
            _ => MASK_XY,
        };
        self.load_operand(offset, mask)
    }

    fn gather(&self, sample: &InstGather) -> Result<(), Error> {
        let Some((tex, sampler)) =
            self.texture_and_sampler(&sample.src_resource, &sample.src_sampler)?
        else {
            return Ok(());
        };
        let component = self.int(sample.src_sampler.gather_channel.into());
        let Some(c) = self.sample_coord(&tex, &sampler, &sample.src_address, None, gathered)?
        else {
            return Ok(());
        };
        let offset = self.gather_offset(&tex, &sample.offset)?;
        let at = SampleAt {
            coord: c.coord,
            array_index: c.array_index,
            offset: Some(offset),
        };
        let (value, _) =
            self.air
                .create_gather(&tex.texture, tex.handle, c.sampler, &at, Some(component))?;
        self.store_texel(&sample.dst, value, &tex.swizzle)
    }

    fn gather_c(&self, sample: &InstGatherCompare) -> Result<(), Error> {
        let Some((tex, sampler)) =
            self.texture_and_sampler(&sample.src_resource, &sample.src_sampler)?
        else {
            return Ok(());
        };
        let reference = self.load_operand(&sample.src_reference, MASK_X)?;
        let Some(c) =
            self.sample_coord(&tex, &sampler, &sample.src_address, None, gathered_depth)?
        else {
            return Ok(());
        };
        let offset = self.gather_offset(&tex, &sample.offset)?;
        let at = SampleAt {
            coord: c.coord,
            array_index: c.array_index,
            offset: Some(offset),
        };
        let (value, _) =
            self.air
                .create_gather_compare(&tex.texture, tex.handle, c.sampler, &at, reference)?;
        self.store_texel(&sample.dst, value, &tex.swizzle)
    }

    fn lod(&self, lod: &InstCalcLod) -> Result<(), Error> {
        let Some((tex, sampler)) = self.texture_and_sampler(&lod.src_resource, &lod.src_sampler)?
        else {
            return Ok(());
        };
        let min_lod = self.decode_texture_min_lod_clamp(tex.metadata);
        let (coord, handle) = match tex.logical {
            Kind::Texture1d | Kind::Texture1dArray => (
                self.float2_x(self.load_operand(&lod.src_address, MASK_X)?),
                sampler.handle,
            ),
            Kind::Depth2d | Kind::Texture2d | Kind::Depth2dArray | Kind::Texture2dArray => (
                self.load_operand(&lod.src_address, MASK_XY)?,
                sampler.handle,
            ),
            Kind::Texture3d => (
                self.load_operand(&lod.src_address, MASK_XYZ)?,
                sampler.handle,
            ),
            Kind::TextureCube | Kind::DepthCube | Kind::TextureCubeArray | Kind::DepthCubeArray => {
                (
                    self.load_operand(&lod.src_address, MASK_XYZ)?,
                    sampler.handle_cube,
                )
            }
            _ => return Ok(()),
        };
        let (clamped, unclamped) =
            self.air
                .create_calculate_lod(&tex.texture, tex.handle, handle, coord)?;
        let clamped = self
            .air
            .create_fp_bin_op(FpBinOp::Fmax, min_lod, clamped, true)?;
        let zero = self.ir.cx().null(self.types.float4);
        let v = self.ir.insert_element_u64(zero, clamped, 0);
        let v = self.ir.insert_element_u64(v, unclamped, 1);
        self.store_texel(&lod.dst, v, &tex.swizzle)
    }

    fn resinfo(&self, resinfo: &InstResourceInfo) -> Result<(), Error> {
        let Some(tex) = self.load_view_texture(&resinfo.src_resource)? else {
            return Ok(());
        };
        let level = self.load_operand(&resinfo.src_mip_level, MASK_X)?;
        let query = |q, level| {
            self.air
                .create_texture_query(&tex.texture, tex.handle, q, Some(level))
        };
        let zero = self.int(0);
        let (mut x, mut y, mut z) = match tex.logical {
            Kind::Texture1d => (query(Query::Width, level)?, zero, zero),
            Kind::Texture1dArray => {
                let x = query(Query::Width, level)?;
                (x, query(Query::ArrayLength, zero)?, zero)
            }
            Kind::Depth2d
            | Kind::Texture2d
            | Kind::TextureCube
            | Kind::DepthCube
            | Kind::Depth2dMs
            | Kind::Texture2dMs => {
                let x = query(Query::Width, level)?;
                (x, query(Query::Height, level)?, zero)
            }
            Kind::Depth2dArray
            | Kind::Texture2dArray
            | Kind::TextureCubeArray
            | Kind::DepthCubeArray
            | Kind::Depth2dMsArray
            | Kind::Texture2dMsArray => {
                let x = query(Query::Width, level)?;
                let y = query(Query::Height, level)?;
                (x, y, query(Query::ArrayLength, zero)?)
            }
            Kind::Texture3d => {
                let x = query(Query::Width, level)?;
                let y = query(Query::Height, level)?;
                (x, y, query(Query::Depth, level)?)
            }
            Kind::TextureBuffer => return Ok(()),
        };
        let mut mip_count = if matches!(
            tex.logical,
            Kind::Texture2dMs | Kind::Texture2dMsArray | Kind::Depth2dMs | Kind::Depth2dMsArray
        ) {
            self.int(1)
        } else {
            query(Query::NumMipLevels, zero)?
        };
        let to_float = |v| self.air.create_convert_to_float(v, Signedness::Signed);
        match resinfo.modifier {
            ResInfoModifier::None => {
                x = to_float(x)?;
                y = to_float(y)?;
                z = to_float(z)?;
                mip_count = to_float(mip_count)?;
            }
            ResInfoModifier::Rcp => {
                x = to_float(x)?;
                y = to_float(y)?;
                z = to_float(z)?;
                let one = self.air.get_float(1.0);
                x = self.ir.fdiv(one, x);
                y = self.ir.fdiv(one, y);
                z = self.ir.fdiv(one, z);
                mip_count = to_float(mip_count)?;
            }
            ResInfoModifier::Uint => {}
        }
        let ty = if resinfo.modifier == ResInfoModifier::Uint {
            self.types.int4
        } else {
            self.types.float4
        };
        let mut value = self.ir.cx().null(ty);
        for (i, v) in [x, y, z, mip_count].into_iter().enumerate() {
            value = self.ir.insert_element_u64(value, v, i as u64);
        }
        self.store_texel(&resinfo.dst, value, &tex.swizzle)
    }

    /// The sample count of a resource, or of the rasterizer without one: 0 for an unbound one.
    fn sample_count(&self, src: Option<&SrcOperandResource>) -> Result<Val<'cx>, Error> {
        let tex = match src {
            Some(src) => self.load_srv_texture(src)?,
            None => None,
        };
        match (tex, src) {
            (Some(tex), _) => self.air.create_texture_query(
                &tex.texture,
                tex.handle,
                Query::NumSamples,
                Some(self.int(0)),
            ),
            (None, Some(_)) => Ok(self.int(0)),
            (None, None) => self.air.create_get_num_samples(),
        }
    }

    fn sampleinfo(&self, sample: &InstSampleInfo) -> Result<(), Error> {
        let mut count = self.sample_count(sample.src.as_ref())?;
        if !sample.uint_result {
            count = self
                .air
                .create_convert_to_float(count, Signedness::Signed)?;
        }
        let ty = if sample.uint_result {
            self.types.int4
        } else {
            self.types.float4
        };
        let value = self.ir.insert_element_u64(self.ir.cx().null(ty), count, 0);
        self.store_texel(&sample.dst, value, &sample.read_swizzle)
    }

    fn samplepos(&self, sample: &InstSamplePos) -> Result<(), Error> {
        let count = self.sample_count(sample.src.as_ref())?;
        let index = self.load_operand(&sample.src_sample_index, MASK_X)?;
        let zero = self.ir.cx().null(self.types.float2);
        let pos = self.sample_pos(count, index)?;
        let value = self.ir.shuffle_vector(pos, zero, &[0, 1, 2, 3]);
        self.store_texel(&sample.dst, value, &sample.read_swizzle)
    }

    fn bufinfo(&self, bufinfo: &InstBufferInfo) -> Result<(), Error> {
        let buf = match &bufinfo.src {
            ResourceOrUav::Resource(r) => self.load_srv_buffer(r)?,
            ResourceOrUav::Uav(u) => self.load_uav_buffer(u)?,
        };
        let mut value = self.int(0);
        if let Some((buf, metadata)) = buf.and_then(|b| b.metadata.map(|m| (b, m))) {
            value = self.decode_raw_buffer_byte_length(metadata);
            if buf.structure_stride != 0 {
                value = self.ir.udiv(value, self.int(buf.structure_stride));
            }
        } else if let Some(tex) = self.load_view_texture(&bufinfo.src)? {
            value = self.decode_texture_buffer_element(tex.metadata);
        }
        self.store_operand(&bufinfo.dst, value, false)
    }

    /// Whether a buffer is threadgroup memory, whose accesses are `volatile`.
    fn is_threadgroup(&self, buf: &BufferHandle<'cx>) -> bool {
        self.ir.cx().tcx().addrspace(buf.pointer.ty()) == Some(3)
    }

    /// A raw or structured load from element `index` on (`ld_raw`, `ld_structured`).
    fn load_typeless_at(
        &self,
        dst: &DstOperand,
        buf: &BufferHandle<'cx>,
        index: Val<'cx>,
    ) -> Result<(), Error> {
        let mask = dst_mask(dst);
        let volatile = self.is_threadgroup(buf);
        let element = |comp: u32| -> Result<Val<'cx>, Error> {
            let ptr = self.gep_int32_with_bound_check(buf, self.ir.add(index, self.int(comp)));
            Ok(if buf.global_coherent {
                self.air.create_device_coherent_load(self.types.int, ptr)?
            } else {
                self.load(self.types.int, ptr, volatile)
            })
        };
        if let Some(comp) = component_from_scalar_mask(mask, &buf.swizzle) {
            let v = element(comp)?;
            return self.store_operand(dst, self.mask_swizzle(v, mask, &SWIZZLE_IDENTITY), false);
        }
        let mut v = self.ir.cx().poison(self.types.int4);
        for (comp, _) in components(memory_access_mask(mask, &buf.swizzle)) {
            let element = element(comp)?;
            v = self.ir.insert_element_u64(v, element, comp.into());
        }
        self.store_operand(dst, self.mask_swizzle(v, mask, &buf.swizzle), false)
    }

    fn ld_raw(&self, load: &InstLoadRaw) -> Result<(), Error> {
        let Some(buf) = self.load_typeless(&load.src)? else {
            return Ok(());
        };
        let index = self.lshr_imm(self.load_operand(&load.src_byte_offset, MASK_X)?, 2);
        self.load_typeless_at(&load.dst, &buf, index)
    }

    fn ld_structured(&self, load: &InstLoadStructured) -> Result<(), Error> {
        let Some(buf) = self.load_typeless(&load.src)? else {
            return Ok(());
        };
        let index_struct = self.ir.mul(
            self.int(buf.structure_stride >> 2),
            self.load_operand(&load.src_address, MASK_X)?,
        );
        let index = self.ir.add(
            index_struct,
            self.lshr_imm(self.load_operand(&load.src_byte_offset, MASK_X)?, 2),
        );
        self.load_typeless_at(&load.dst, &buf, index)
    }

    /// A raw or structured store of `src` from element `index` on.
    ///
    /// `store_raw` and `store_structured`.
    fn store_typeless_at(
        &self,
        buf: &BufferHandle<'cx>,
        index: Val<'cx>,
        src: &SrcOperand,
    ) -> Result<(), Error> {
        let volatile = self.is_threadgroup(buf);
        let value = self.load_operand(src, buf.mask)?;
        for (comp, _) in components(buf.mask) {
            let ptr = self.gep_int32_with_bound_check(buf, self.ir.add(index, self.int(comp)));
            let element = self.element_or_scalar(value, comp.into());
            if buf.global_coherent {
                self.air.create_device_coherent_store(element, ptr)?;
            } else {
                self.store(element, ptr, volatile);
            }
        }
        Ok(())
    }

    fn store_raw(&self, store: &InstStoreRaw) -> Result<(), Error> {
        let Some(buf) = self.load_atomic_buffer(&store.dst)? else {
            return Ok(());
        };
        if !matches!(buf.mask, 0b1 | 0b11 | 0b111 | 0b1111) {
            return Ok(());
        }
        let index = self.lshr_imm(self.load_operand(&store.dst_byte_offset, MASK_X)?, 2);
        self.store_typeless_at(&buf, index, &store.src)
    }

    fn store_structured(&self, store: &InstStoreStructured) -> Result<(), Error> {
        let Some(buf) = self.load_atomic_buffer(&store.dst)? else {
            return Ok(());
        };
        if !matches!(buf.mask, 0b1 | 0b11 | 0b111 | 0b1111) {
            return Ok(());
        }
        let index_struct = self.ir.mul(
            self.int(buf.structure_stride >> 2),
            self.load_operand(&store.dst_address, MASK_X)?,
        );
        let index = self.ir.add(
            index_struct,
            self.lshr_imm(self.load_operand(&store.dst_byte_offset, MASK_X)?, 2),
        );
        self.store_typeless_at(&buf, index, &store.src)
    }

    /// An atomic's `i32` element (`LoadAtomicOpAddress`).
    fn load_atomic_op_address(
        &self,
        buf: &BufferHandle<'cx>,
        address: &SrcOperand,
    ) -> Result<Val<'cx>, Error> {
        if buf.structure_stride > 0 {
            let address_2d = self.load_operand(address, MASK_XY)?;
            let offset = self.ir.add(
                self.ir.mul(
                    self.int(buf.structure_stride),
                    self.element_or_scalar(address_2d, 0),
                ),
                self.element_or_scalar(address_2d, 1),
            );
            return Ok(self.lshr_imm(offset, 2));
        }
        let address_1d = self.load_operand(address, MASK_X)?;
        Ok(self.lshr_imm(address_1d, 2))
    }

    fn atomic_bin_op(&self, atomic: &InstAtomicBinOp) -> Result<(), Error> {
        let op = match atomic.op {
            AtomicBinaryOp::And => sys::ATOMIC_RMW_BIN_OP_AND,
            AtomicBinaryOp::Or => sys::ATOMIC_RMW_BIN_OP_OR,
            AtomicBinaryOp::Xor => sys::ATOMIC_RMW_BIN_OP_XOR,
            AtomicBinaryOp::Add => sys::ATOMIC_RMW_BIN_OP_ADD,
            AtomicBinaryOp::IMax => sys::ATOMIC_RMW_BIN_OP_MAX,
            AtomicBinaryOp::IMin => sys::ATOMIC_RMW_BIN_OP_MIN,
            AtomicBinaryOp::UMax => sys::ATOMIC_RMW_BIN_OP_U_MAX,
            AtomicBinaryOp::UMin => sys::ATOMIC_RMW_BIN_OP_U_MIN,
            AtomicBinaryOp::Xchg => sys::ATOMIC_RMW_BIN_OP_XCHG,
        };
        if let Some(buf) = self.load_atomic_buffer(&atomic.dst)? {
            let offset = self.load_atomic_op_address(&buf, &atomic.dst_address)?;
            let ptr = self.ir.gep(self.types.int, buf.pointer, &[offset]);
            let src = self.load_operand(&atomic.src, MASK_X)?;
            let value = self.air.create_atomic_rmw(op, ptr, src)?;
            return self.store_operand(&atomic.dst_original, value, false);
        }
        let Some(tex) = self.load_atomic_texture(&atomic.dst)? else {
            return Ok(());
        };
        if matches!(tex.logical, Kind::Depth2d | Kind::Depth2dArray) {
            return Ok(());
        }
        let Some((mut address, array_index)) = self.uav_address(&tex, &atomic.dst_address)? else {
            return Ok(());
        };
        let mut oob = None;
        if tex.logical == Kind::TextureBuffer {
            oob = Some(self.ir.icmp(
                sys::INT_UGE,
                address,
                self.decode_texture_buffer_element(tex.metadata),
            ));
            address = self
                .ir
                .add(address, self.decode_texture_buffer_offset(tex.metadata));
        }
        if let Some(oob) = oob {
            address = self
                .ir
                .select(oob, self.ir.cx().all_ones(address.ty()), address);
        }
        let src = self.load_operand(&atomic.src, MASK_ALL)?;
        let value = self.air.create_texture_atomic_rmw(
            &tex.texture,
            tex.handle,
            op,
            address,
            src,
            array_index,
        )?;
        self.store_operand(&atomic.dst_original, value, false)
    }

    fn atomic_cmp_exchange(&self, atomic: &InstAtomicImmCmpExchange) -> Result<(), Error> {
        if let Some(buf) = self.load_atomic_buffer(&atomic.dst_resource)? {
            let offset = self.load_atomic_op_address(&buf, &atomic.dst_address)?;
            let ptr = self.ir.gep(self.types.int, buf.pointer, &[offset]);
            let cmp = self.load_operand(&atomic.src0, MASK_X)?;
            let new = self.load_operand(&atomic.src1, MASK_X)?;
            let value = self.ir.atomic_cmpxchg(
                ptr,
                cmp,
                new,
                sys::ATOMIC_ORDERING_MONOTONIC,
                sys::ATOMIC_ORDERING_MONOTONIC,
                false,
            );
            return self.store_operand(&atomic.dst, self.ir.extract_value(value, 0), false);
        }
        let Some(tex) = self.load_atomic_texture(&atomic.dst_resource)? else {
            return Ok(());
        };
        if matches!(tex.logical, Kind::Depth2d | Kind::Depth2dArray) {
            return Ok(());
        }
        let Some((mut address, array_index)) = self.uav_address(&tex, &atomic.dst_address)? else {
            return Ok(());
        };
        if tex.logical == Kind::TextureBuffer {
            address = self
                .ir
                .add(address, self.decode_texture_buffer_offset(tex.metadata));
        }
        let cmp = self.load_operand(&atomic.src0, MASK_ALL)?;
        let new = self.load_operand(&atomic.src1, MASK_ALL)?;
        let (value, _) = self.air.create_atomic_cmpxchg(
            &tex.texture,
            tex.handle,
            address,
            cmp,
            new,
            array_index,
        )?;
        self.store_operand(&atomic.dst, value, false)
    }

    /// `imm_atomic_alloc` and `imm_atomic_consume`; the consume answers the new value.
    fn atomic_counter(
        &self,
        dst: &DstOperand,
        uav: &AtomicDstOperandUav,
        op: sys::AtomicRMWBinOp,
    ) -> Result<(), Error> {
        let Some(counter) = self.load_counter(uav)? else {
            return Ok(());
        };
        let value = self.air.create_atomic_rmw(op, counter, self.int(1))?;
        let value = if op == sys::ATOMIC_RMW_BIN_OP_SUB {
            self.ir.sub(value, self.int(1))
        } else {
            value
        };
        self.store_operand(dst, value, false)
    }

    /// An `eval_*`: nothing for an input no interpolant is recorded for (`LoadInterpolant`).
    fn interpolate(
        &self,
        dst: &DstOperand,
        regid: u32,
        swizzle: &Swizzle,
        at: impl FnOnce(super::resources::Interpolant<'cx>) -> Result<Val<'cx>, Error>,
    ) -> Result<(), Error> {
        let Some(&itp) = self.res.interpolant_map.get(&regid) else {
            return Ok(());
        };
        let value = at(itp)?;
        self.store_texel(dst, value, swizzle)
    }

    fn msad(&self, msad: &InstMaskedSumOfAbsDiff) -> Result<(), Error> {
        let mask = dst_mask(&msad.dst);
        let reference = self.load_operand(&msad.src0, mask)?;
        let src = self.load_operand(&msad.src1, mask)?;
        let accum = self.load_operand(&msad.src2, mask)?;
        let mut name = b"dxmt.msad".to_vec();
        name.extend(
            self.air
                .type_overload_suffix(reference.ty(), Signedness::DontCare),
        );
        let result = self.call_declared(
            &name,
            accum.ty(),
            &[reference, src, accum],
            &[(FN, b"readnone"), (FN, b"nounwind"), (FN, b"willreturn")],
        )?;
        self.store_operand(&msad.dst, result, false)
    }

    /// `dxmt.sample_pos.i32`: a sample's position for a sample count (`GetSamplePos`).
    ///
    /// # Errors
    ///
    /// None in practice: only the name check.
    pub fn sample_pos(&self, count: Val<'cx>, index: Val<'cx>) -> Result<Val<'cx>, Error> {
        self.call_declared(
            b"dxmt.sample_pos.i32",
            self.types.float2,
            &[count, index],
            &[(FN, b"nounwind"), (FN, b"willreturn"), (FN, b"readnone")],
        )
    }

    // Tessellation's helpers (`dxmt.*` of the tessellation helper module).

    /// `dxmt.generate_workload.<triangle|quad>.<partitioning>` (`HullGenerateWorkloadFor*`).
    ///
    /// `operands`: the patch index, the count and data pointers, then the factors in and out.
    ///
    /// # Errors
    ///
    /// None in practice: only the name check.
    pub fn hull_generate_workload(
        &self,
        quad: bool,
        partitioning: &Partitioning,
        operands: &[Val<'cx>],
    ) -> Result<(), Error> {
        let mut name = if quad {
            b"dxmt.generate_workload.quad".to_vec()
        } else {
            b"dxmt.generate_workload.triangle".to_vec()
        };
        name.extend(partitioning_suffix(partitioning));
        let void = self.air.void_ty();
        self.call_declared(
            &name,
            void,
            operands,
            &[(3, b"writeonly"), (FN, b"nounwind"), (FN, b"willreturn")],
        )?;
        Ok(())
    }

    /// `dxmt.get_domain_patch_index` (`DomainGetPatchIndex`).
    ///
    /// # Errors
    ///
    /// None in practice: only the name check.
    pub fn domain_get_patch_index(
        &self,
        workload: Val<'cx>,
        data: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        self.call_declared(
            b"dxmt.get_domain_patch_index",
            self.types.int,
            &[workload, data],
            &[
                (2, b"readonly"),
                (2, b"nocapture"),
                (FN, b"nounwind"),
                (FN, b"willreturn"),
                (FN, b"readonly"),
            ],
        )
    }

    /// `dxmt.get_domain_location.<partitioning>` (`DomainGetLocation`).
    ///
    /// The location, and whether the thread makes a vertex and a primitive.
    ///
    /// # Errors
    ///
    /// None in practice: only the name check.
    pub fn domain_get_location(
        &self,
        workload: Val<'cx>,
        thread: Val<'cx>,
        data: Val<'cx>,
        partitioning: &Partitioning,
    ) -> Result<(Val<'cx>, Val<'cx>, Val<'cx>), Error> {
        let mut name = b"dxmt.get_domain_location".to_vec();
        name.extend(partitioning_suffix(partitioning));
        let tcx = self.ir.cx().tcx();
        // A new unnamed structure each call, as `StructType::create(…, "")`.
        let ret = tcx.create_struct(
            c"",
            Some((
                &[self.types.float2, self.types.byte, self.types.byte],
                false,
            )),
        );
        let result = self.call_declared(
            &name,
            ret,
            &[workload, thread, data],
            &[
                (3, b"readonly"),
                (3, b"nocapture"),
                (FN, b"nounwind"),
                (FN, b"willreturn"),
                (FN, b"readonly"),
            ],
        )?;
        let location = self.ir.extract_value(result, 0);
        let vertex = self.ir.is_not_null(self.ir.extract_value(result, 1));
        let primitive = self.ir.is_not_null(self.ir.extract_value(result, 2));
        Ok((location, vertex, primitive))
    }

    /// `dxmt.domain_generate_primitives.<primitive>` (`DomainGeneratePrimitives`).
    ///
    /// Into the function's mesh.
    ///
    /// # Errors
    ///
    /// A function with no mesh argument.
    pub fn domain_generate_primitives(
        &self,
        workload: Val<'cx>,
        data: Val<'cx>,
        primitive: &OutputPrimitive,
    ) -> Result<(), Error> {
        let mut name = b"dxmt.domain_generate_primitives".to_vec();
        name.extend(match primitive {
            OutputPrimitive::Point => b".point".as_slice(),
            OutputPrimitive::Line => b".line",
            OutputPrimitive::Triangle => b".triangle",
            OutputPrimitive::TriangleCcw => b".triangle_ccw",
        });
        let mesh = req(self.air.mesh_handle(), "no mesh")?;
        let void = self.air.void_ty();
        self.call_declared(
            &name,
            void,
            &[workload, data, mesh],
            &[
                (2, b"readonly"),
                (2, b"nocapture"),
                (3, b"nocapture"),
                (FN, b"nounwind"),
                (FN, b"willreturn"),
            ],
        )?;
        Ok(())
    }
}

/// The AIR builder's kind of a binding's kind.
#[must_use]
pub const fn air_kind(kind: Kind) -> AirKind {
    match kind {
        Kind::TextureBuffer => AirKind::TextureBuffer,
        Kind::Texture1d => AirKind::Texture1d,
        Kind::Texture1dArray => AirKind::Texture1dArray,
        Kind::Texture2d => AirKind::Texture2d,
        Kind::Texture2dArray => AirKind::Texture2dArray,
        Kind::Texture3d => AirKind::Texture3d,
        Kind::TextureCube => AirKind::TextureCube,
        Kind::TextureCubeArray => AirKind::TextureCubeArray,
        Kind::Texture2dMs => AirKind::Texture2dMs,
        Kind::Texture2dMsArray => AirKind::Texture2dMsArray,
        Kind::Depth2d => AirKind::Depth2d,
        Kind::Depth2dArray => AirKind::Depth2dArray,
        Kind::DepthCube => AirKind::DepthCube,
        Kind::DepthCubeArray => AirKind::DepthCubeArray,
        Kind::Depth2dMs => AirKind::Depth2dMs,
        Kind::Depth2dMsArray => AirKind::Depth2dMsArray,
    }
}

/// A 1D kind bound as 2D (`lowering_texture_1d_to_2d`).
#[must_use]
pub const fn lowered(kind: Kind) -> Kind {
    match kind {
        Kind::Texture1d => Kind::Texture2d,
        Kind::Texture1dArray => Kind::Texture2dArray,
        other => other,
    }
}

/// An internal inconsistency or a register no declaration made.
#[must_use]
pub fn invalid(what: &str) -> Error {
    Error::new(format!("invalid operation: {what}.").as_bytes())
}

/// The value, or [`invalid`]'s error naming it.
///
/// # Errors
///
/// `v` is `None`.
pub fn req<'cx>(v: Option<Val<'cx>>, what: &str) -> Result<Val<'cx>, Error> {
    v.ok_or_else(|| invalid(what))
}

/// What an operand index reads: an integer register's one component.
const fn index_common(component: u8) -> SrcOperandCommon {
    SrcOperandCommon {
        swizzle: Swizzle([component; 4]),
        abs: false,
        neg: false,
        read_type: OperandDataType::Integer,
    }
}

const fn partitioning_suffix(partitioning: &Partitioning) -> &'static [u8] {
    match partitioning {
        Partitioning::Integer => b".integer",
        Partitioning::Pow2 => b".pow2",
        Partitioning::FractionalOdd => b".odd",
        Partitioning::FractionalEven => b".even",
    }
}

/// The kinds `sample`, `sample_l`, `sample_b` and `sample_d` read.
const fn sampled(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Texture1d
            | Kind::Texture1dArray
            | Kind::Texture2d
            | Kind::Depth2d
            | Kind::Texture2dArray
            | Kind::Depth2dArray
            | Kind::Texture3d
            | Kind::TextureCube
            | Kind::DepthCube
            | Kind::TextureCubeArray
            | Kind::DepthCubeArray
    )
}

/// The kinds `sample_c` reads (3D among them, as the C++'s switch).
const fn compared(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Depth2d
            | Kind::Depth2dArray
            | Kind::Texture3d
            | Kind::DepthCube
            | Kind::DepthCubeArray
    )
}

/// The kinds `gather4` reads.
const fn gathered(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Texture2d
            | Kind::Depth2d
            | Kind::Texture2dArray
            | Kind::Depth2dArray
            | Kind::TextureCube
            | Kind::DepthCube
            | Kind::TextureCubeArray
            | Kind::DepthCubeArray
    )
}

/// The kinds `gather4_c` reads.
const fn gathered_depth(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Depth2d | Kind::Depth2dArray | Kind::DepthCube | Kind::DepthCubeArray
    )
}

/// Where a sample reads: the coordinates, the clamped array slice, the sampler to use.
struct SampleCoord<'cx> {
    coord: Val<'cx>,
    array_index: Option<Val<'cx>>,
    sampler: Val<'cx>,
    derivatives: Option<(Val<'cx>, Val<'cx>)>,
}

/// The lowest set bit's index (`__builtin_ctz` of a nonzero mask).
const fn ctz(mask: Mask) -> u32 {
    mask.trailing_zeros()
}

/// Each set component of `mask` and its place among them (`EnumerateComponents`).
fn components(mask: Mask) -> impl Iterator<Item = (u32, u32)> {
    (0..4_u32)
        .filter(move |bit| mask & (1 << bit) != 0)
        .zip(0_u32..)
}

/// The components `swizzle` reads for `mask` (`MemoryAccessMask`).
fn memory_access_mask(mask: Mask, swizzle: &Swizzle) -> Mask {
    components(mask).fold(0, |ret, (bit, _)| ret | 1 << sw(swizzle, bit))
}

/// The source component `swizzle` gives component `i`.
fn sw(swizzle: &Swizzle, i: u32) -> u32 {
    swizzle.0.get(i as usize).copied().map_or(0, u32::from)
}

/// The component a one-component read takes (`ComponentFromScalarMask`).
fn component_from_scalar_mask(mask: Mask, swizzle: &Swizzle) -> Option<u32> {
    match mask {
        0b1 => Some(sw(swizzle, 0)),
        0b10 => Some(sw(swizzle, 1)),
        0b100 => Some(sw(swizzle, 2)),
        0b1000 => Some(sw(swizzle, 3)),
        _ => match memory_access_mask(mask, swizzle) {
            access @ (0b1 | 0b10 | 0b100 | 0b1000) => Some(ctz(access)),
            _ => None,
        },
    }
}

/// The written components (`GetMask`): all four for a null or side-effect destination.
const fn dst_mask(dst: &DstOperand) -> Mask {
    match dst {
        DstOperand::Null { .. } | DstOperand::SideEffect { .. } => MASK_ALL,
        DstOperand::Temp { common, .. }
        | DstOperand::IndexableTemp { common, .. }
        | DstOperand::Output { common, .. }
        | DstOperand::OutputDepth { common }
        | DstOperand::OutputCoverageMask { common }
        | DstOperand::OutputStencilRef { common }
        | DstOperand::IndexableOutput { common, .. } => common.mask,
    }
}

const fn dst_common(dst: &DstOperand) -> &DstOperandCommon {
    match dst {
        DstOperand::Null { common }
        | DstOperand::SideEffect { common }
        | DstOperand::Temp { common, .. }
        | DstOperand::IndexableTemp { common, .. }
        | DstOperand::Output { common, .. }
        | DstOperand::OutputDepth { common }
        | DstOperand::OutputCoverageMask { common }
        | DstOperand::OutputStencilRef { common }
        | DstOperand::IndexableOutput { common, .. } => common,
    }
}

const fn is_null(dst: &DstOperand) -> bool {
    matches!(dst, DstOperand::Null { .. })
}

/// The low 16 bits (the C++'s implicit `uint16_t` conversion).
const fn low16(v: u32) -> u16 {
    let b = v.to_le_bytes();
    u16::from_le_bytes([b[0], b[1]])
}

/// A swizzle lane as a shuffle index.
fn lane(swizzle: &Swizzle, i: u32) -> i32 {
    i32::from(swizzle.0.get(i as usize).copied().unwrap_or(0))
}
