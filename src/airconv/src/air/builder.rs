//! AIR's intrinsics: one declaring method per operation, with its mangled name and attributes.
//!
//! Ported from `nt/air_builder.*` (`AIRBuilder`). A method declares its `air.*` function through
//! [`Module::get_or_insert_function`] with the C++'s attribute list (given only to a new
//! declaration) and calls it; an overload suffix is read from the operands' `Ty`s, a pointer's
//! from its pointee. Instructions are made in the C++'s order: clang evaluates a call's arguments
//! left to right, so `CreateSelect(CreateIsNull(h), 0, CreateCall(…))` makes the compare, then
//! the call. Where the C++ answered null (after an "invalid operation" line to a null stream) or
//! called with a null operand, the method answers an error naming the operation (P15).
//!
//! The C++'s overloads are one method each here: an immediate offset is
//! [`AirBuilder::get_offset`]'s value, the level arguments of a sample are a [`Lod`], and the
//! operands that travel together are [`SampleAt`], [`TexelAt`] and [`Gradient`].

use core::ffi::CStr;

use super::ty::AirTypes;
use crate::{
    abi::{SM50_SHADER_FLAG_DEFUSE_FMA, SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO},
    api::Error,
    llvm::{Builder, Func, Module, Ty, TyCx, TyKind, Val, sys},
};

/// An attribute list: `(index, attribute name)`, the index 1 for the first parameter.
type Attrs = [(sys::AttributeIndex, &'static [u8])];

/// The function's attribute index (`~0U`).
const FN: sys::AttributeIndex = sys::ATTRIBUTE_FUNCTION_INDEX;

/// Atomic texture and pointer operations.
const ATOMIC: &Attrs = &[(1, b"nocapture"), (FN, b"nounwind"), (FN, b"willreturn")];
/// Samples and level computations, which take derivatives.
const SAMPLE: &Attrs = &[
    (1, b"nocapture"),
    (1, b"readonly"),
    (2, b"nocapture"),
    (2, b"readonly"),
    (FN, b"argmemonly"),
    (FN, b"convergent"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
    (FN, b"readonly"),
];
/// Gradient samples and gathers: `SAMPLE` without `convergent`.
const SAMPLE_EXPLICIT: &Attrs = &[
    (1, b"nocapture"),
    (1, b"readonly"),
    (2, b"nocapture"),
    (2, b"readonly"),
    (FN, b"argmemonly"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
    (FN, b"readonly"),
];
/// Reads, texture queries and the center, centroid and offset interpolations.
const READ: &Attrs = &[
    (1, b"nocapture"),
    (1, b"readonly"),
    (FN, b"argmemonly"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
    (FN, b"readonly"),
];
/// Writes and the mesh setters.
const WRITE: &Attrs = &[
    (1, b"nocapture"),
    (FN, b"argmemonly"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
];
/// The texture fence.
const TEXTURE_FENCE: &Attrs = &[
    (1, b"nocapture"),
    (FN, b"mustprogress"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
];
/// Math, conversions and the sample count.
const PURE: &Attrs = &[(FN, b"nounwind"), (FN, b"willreturn"), (FN, b"readnone")];
/// Derivatives and discard.
const PLAIN: &Attrs = &[(FN, b"nounwind"), (FN, b"willreturn")];
/// Barriers (the shim's barrier pass declares `air.simdgroup.barrier` with the same list).
const BARRIER: &Attrs = &[(FN, b"convergent"), (FN, b"nounwind"), (FN, b"willreturn")];
/// The device-coherent load.
const COHERENT_LOAD: &Attrs = &[
    (1, b"nocapture"),
    (1, b"readonly"),
    (FN, b"argmemonly"),
    (FN, b"readonly"),
    (FN, b"nofree"),
    (FN, b"nosync"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
];
/// The device-coherent store.
const COHERENT_STORE: &Attrs = &[
    (2, b"nocapture"),
    (2, b"writeonly"),
    (FN, b"argmemonly"),
    (FN, b"nofree"),
    (FN, b"nosync"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
];
/// The sample interpolation.
const INTERPOLATE_SAMPLE: &Attrs = &[
    (1, b"nocapture"),
    (1, b"readonly"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
    (FN, b"argmemonly"),
];
/// The object shader's mesh grid size.
const MESH_PROPERTIES: &Attrs = &[
    (1, b"nocapture"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
    (FN, b"argmemonly"),
    (FN, b"mustprogress"),
];

/// `TextureOperationInfo`'s flags, packed.
const ARRAY: u8 = 1;
const CUBE: u8 = 1 << 1;
const DEPTH: u8 = 1 << 2;
const MS: u8 = 1 << 3;
const MIPMAPPED: u8 = 1 << 4;

/// A texture's kind (`Texture::ResourceKind`), in the C++'s order.
#[derive(Clone, Copy)]
pub enum TextureKind {
    /// `texture_buffer`.
    TextureBuffer,
    /// `texture1d`.
    Texture1d,
    /// `texture1d_array`.
    Texture1dArray,
    /// `texture2d`.
    Texture2d,
    /// `texture2d_array`.
    Texture2dArray,
    /// `texture3d`.
    Texture3d,
    /// `texturecube`.
    TextureCube,
    /// `texturecube_array`.
    TextureCubeArray,
    /// `texture2d_ms`.
    Texture2dMs,
    /// `texture2d_ms_array`.
    Texture2dMsArray,
    /// `depth2d`.
    Depth2d,
    /// `depth2d_array`.
    Depth2dArray,
    /// `depthcube`.
    DepthCube,
    /// `depthcube_array`.
    DepthCubeArray,
    /// `depth_2d_ms`.
    Depth2dMs,
    /// `depth_2d_ms_array`.
    Depth2dMsArray,
}

/// What a texture's values are (`Texture::SampleType`).
pub enum SampleType {
    /// `sample_float`.
    Float,
    /// `sample_int`.
    Int,
    /// `sample_uint`.
    Uint,
}

/// How a texture is accessed (`Texture::MemoryAccess`), the value its calls pass.
#[derive(Clone, Copy)]
#[repr(u32)]
pub enum MemoryAccess {
    /// `access_sample`.
    Sample = 0,
    /// `access_read`.
    Read = 1,
    /// `access_write`.
    Write = 2,
    /// `acesss_readwrite`.
    ReadWrite = 3,
}

/// A texture operand's description (`Texture`).
pub struct Texture {
    /// The kind.
    pub kind: TextureKind,
    /// What its texels are.
    pub sample_type: SampleType,
    /// How it is accessed.
    pub memory_access: MemoryAccess,
}

/// A texture query (`Texture::Query`).
pub enum Query {
    /// `get_width`.
    Width,
    /// `get_height`.
    Height,
    /// `get_depth`.
    Depth,
    /// `get_array_size`.
    ArrayLength,
    /// `get_num_mip_levels`.
    NumMipLevels,
    /// `get_num_samples`.
    NumSamples,
}

/// How an overload suffix spells an integer's or a float's sign (`Signedness`).
pub enum Signedness {
    /// No sign part.
    DontCare,
    /// `s.` for an integer, `f.` for a float.
    Signed,
    /// `u.` for an integer, `f.` for a float.
    Unsigned,
}

/// The memory a barrier or fence orders (`MemFlags`), bits that combine.
#[derive(Clone, Copy)]
pub struct MemFlags(u32);

impl MemFlags {
    /// `None`.
    pub const NONE: Self = Self(0);
    /// `Device`.
    pub const DEVICE: Self = Self(1);
    /// `Threadgroup`.
    pub const THREADGROUP: Self = Self(2);
    /// `Texture`.
    pub const TEXTURE: Self = Self(4);

    /// The flags of both.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The flags as the call passes them.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// The threads a fence orders against (`ThreadScope`).
#[repr(u32)]
pub enum ThreadScope {
    /// `Threadgroup`.
    Threadgroup = 1,
    /// `Device`.
    Device = 2,
}

/// A floating-point unary operation (`AIRBuilder::FPUnOp`).
pub enum FpUnOp {
    /// `saturate`.
    Saturate,
    /// `log2`.
    Log2,
    /// `exp2`.
    Exp2,
    /// `sqrt`.
    Sqrt,
    /// `rsqrt`.
    Rsqrt,
    /// `fract`.
    Fract,
    /// `rint`.
    Rint,
    /// `floor`.
    Floor,
    /// `ceil`.
    Ceil,
    /// `trunc`.
    Trunc,
    /// `cos`.
    Cos,
    /// `sin`.
    Sin,
    /// `fabs`.
    Fabs,
    /// `tan`.
    Tan,
    /// `acos`.
    Acos,
    /// `asin`.
    Asin,
    /// `atan`.
    Atan,
    /// `cosh`.
    Cosh,
    /// `sinh`.
    Sinh,
    /// `tanh`.
    Tanh,
}

/// A floating-point binary operation (`AIRBuilder::FPBinOp`).
pub enum FpBinOp {
    /// `fmax`.
    Fmax,
    /// `fmin`.
    Fmin,
}

/// An integer unary operation (`AIRBuilder::IntUnOp`).
pub enum IntUnOp {
    /// `reverse_bits`.
    ReverseBits,
    /// `popcount`.
    Popcount,
}

/// An integer binary operation (`AIRBuilder::IntBinOp`).
pub enum IntBinOp {
    /// `max`.
    Max,
    /// `min`.
    Min,
    /// `mul_hi`.
    MulHi,
}

/// A sample's level arguments: the C++'s `sample_level`, `sample_bias` and `sample_min_lod_clamp`.
pub enum Lod<'cx> {
    /// Neither (`CreateSample` with no tag).
    #[cfg(test)]
    Implicit,
    /// `sample_level`.
    Level(Val<'cx>),
    /// `sample_bias` and `sample_min_lod_clamp`.
    BiasMin(Val<'cx>, Val<'cx>),
}

/// Where a sample or gather reads: the coordinates, the array slice and the texel offset.
///
/// The slice is used by array kinds, the offset by the kinds that take one ([`get_offset`]'s);
/// a kind that uses one it lacks is an error, as the C++ passed a null operand.
///
/// [`get_offset`]: AirBuilder::get_offset
pub struct SampleAt<'cx> {
    /// The coordinates.
    pub coord: Val<'cx>,
    /// The array slice.
    pub array_index: Option<Val<'cx>>,
    /// The texel offset.
    pub offset: Option<Val<'cx>>,
}

/// Where a read or write lands: the position, the slice, the sample or cube face, the level.
pub struct TexelAt<'cx> {
    /// The position.
    pub pos: Val<'cx>,
    /// The array slice (array kinds).
    pub array_index: Option<Val<'cx>>,
    /// The cube face (cube kinds) or the sample (multisampled reads).
    pub sample_or_face: Option<Val<'cx>>,
    /// The level (mipmapped kinds).
    pub level: Option<Val<'cx>>,
}

/// A gradient sample's derivatives and level clamp.
pub struct Gradient<'cx> {
    /// The derivative along x.
    pub dx: Val<'cx>,
    /// The derivative along y.
    pub dy: Val<'cx>,
    /// The minimum level (`0.0` where the C++ defaulted it).
    pub min_lod: Val<'cx>,
}

/// `AIRBuilder`: declares and calls AIR's intrinsics at the builder's position in `module`.
///
/// The C++'s options are the header's shader flags (`SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO`, which
/// zeroes NaN sample coordinates and derivatives, and `SM50_SHADER_FLAG_DEFUSE_FMA`).
pub struct AirBuilder<'a, 'cx> {
    b: &'a Builder<'cx>,
    m: &'a Module<'cx>,
    ty: &'a AirTypes,
    shader_flags: u32,
}

impl<'a, 'cx> AirBuilder<'a, 'cx> {
    /// An AIR builder over `b`, declaring into `m`, with `shader_flags`' options.
    #[must_use]
    pub const fn new(
        b: &'a Builder<'cx>,
        m: &'a Module<'cx>,
        ty: &'a AirTypes,
        shader_flags: u32,
    ) -> Self {
        Self {
            b,
            m,
            ty,
            shader_flags,
        }
    }

    /// The instruction builder.
    #[must_use]
    pub const fn builder(&self) -> &'a Builder<'cx> {
        self.b
    }

    /// The module the intrinsics are declared in (`getModule`).
    #[must_use]
    pub const fn module(&self) -> &'a Module<'cx> {
        self.m
    }

    /// The translator's named types.
    #[must_use]
    pub const fn types(&self) -> &'a AirTypes {
        self.ty
    }

    // Texture operations.

    /// The offset vector of an immediate offset, for the kinds that take one (`GetOffset`).
    #[must_use]
    pub fn get_offset(&self, texture: &Texture, offset: [i32; 3]) -> Option<Val<'cx>> {
        let [x, y, z] = offset.map(i32::cast_unsigned);
        match texture.kind {
            TextureKind::Texture1d | TextureKind::Texture1dArray => Some(self.get_int(x)),
            TextureKind::Texture2d
            | TextureKind::Texture2dArray
            | TextureKind::Depth2d
            | TextureKind::Depth2dArray => Some(self.get_int2(x, y)),
            TextureKind::Texture3d => Some(self.get_int3(x, y, z)),
            _ => None,
        }
    }

    /// The offset vector of three offset values (`GetOffset`, SM 6.7's programmable offsets).
    #[must_use]
    pub fn get_offset_values(&self, texture: &Texture, offset: [Val<'cx>; 3]) -> Option<Val<'cx>> {
        let n: u32 = match texture.kind {
            TextureKind::Texture1d | TextureKind::Texture1dArray => return Some(offset[0]),
            TextureKind::Texture2d
            | TextureKind::Texture2dArray
            | TextureKind::Depth2d
            | TextureKind::Depth2dArray => 2,
            TextureKind::Texture3d => 3,
            _ => return None,
        };
        let undef = self.b.cx().undef(self.int_ty(n));
        Some(
            offset
                .iter()
                .zip(0..u64::from(n))
                .fold(undef, |v, (&element, i)| {
                    self.b.insert_element_u64(v, element, i)
                }),
        )
    }

    /// `air.sample_*`: the texel and the residency byte (`CreateSample`).
    ///
    /// # Errors
    ///
    /// A slice or offset the kind uses and `at` lacks.
    pub fn create_sample(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        sampler: Val<'cx>,
        at: &SampleAt<'cx>,
        lod: &Lod<'cx>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        let mut name = [b"air.sample_".as_slice(), info.suffix].concat();
        name.extend(self.type_overload_suffix(self.texel_type(texture), texel_sign(texture)));
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.sampler_handle_type(), sampler);
        if info.has(DEPTH) {
            call.push(self.int_ty(1), self.get_int(1));
        }
        call.push(
            self.texture_sample_coord_type(texture),
            self.nan_to_zero_if_asked(at.coord),
        );
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(at.array_index, &name)?);
        }
        if let Some(n) = offset_width(texture.kind, false) {
            call.push(self.bool_ty(), self.get_bool(n > 1));
            call.push(self.int_ty(n), operand(at.offset, &name)?);
        }
        let (control, a, b) = self.lod_args(lod);
        call.push(self.bool_ty(), self.get_bool(control));
        call.push(self.float_ty(1), a);
        call.push(self.float_ty(1), b);
        call.push(self.int_ty(1), self.get_int(0));
        let result = call.finish(&name, self.texture_sample_result_type(texture), SAMPLE)?;
        Ok(self.pair(result))
    }

    /// `air.sample_compare_*`: a depth comparison (`CreateSampleCmp`).
    ///
    /// # Errors
    ///
    /// A texture that is not a depth texture (the C++'s null), or a slice or offset the kind uses
    /// and `at` lacks.
    pub fn create_sample_cmp(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        sampler: Val<'cx>,
        at: &SampleAt<'cx>,
        reference: Val<'cx>,
        lod: &Lod<'cx>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        if !info.has(DEPTH) {
            return Err(Error::new(
                b"invalid operation: compare on non depth texture.",
            ));
        }
        let mut name = [b"air.sample_compare_".as_slice(), info.suffix].concat();
        name.extend(self.type_overload_suffix(self.texel_type(texture), texel_sign(texture)));
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.sampler_handle_type(), sampler);
        call.push(self.int_ty(1), self.get_int(1));
        call.push(
            self.texture_sample_coord_type(texture),
            self.nan_to_zero_if_asked(at.coord),
        );
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(at.array_index, &name)?);
        }
        call.push(self.float_ty(1), reference);
        if matches!(
            texture.kind,
            TextureKind::Depth2d | TextureKind::Depth2dArray
        ) {
            call.push(self.bool_ty(), self.get_bool(true));
            call.push(self.int_ty(2), operand(at.offset, &name)?);
        }
        let (control, a, b) = self.lod_args(lod);
        call.push(self.bool_ty(), self.get_bool(control));
        call.push(self.float_ty(1), a);
        call.push(self.float_ty(1), b);
        call.push(self.int_ty(1), self.get_int(0));
        let result = call.finish(&name, self.texture_sample_result_type(texture), SAMPLE)?;
        Ok(self.pair(result))
    }

    /// `air.sample_*_grad`: a sample with explicit derivatives (`CreateSampleGrad`).
    ///
    /// # Errors
    ///
    /// A slice or offset the kind uses and `at` lacks.
    pub fn create_sample_grad(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        sampler: Val<'cx>,
        at: &SampleAt<'cx>,
        grad: &Gradient<'cx>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        let mut name = [b"air.sample_".as_slice(), info.suffix, b"_grad"].concat();
        name.extend(self.type_overload_suffix(self.texel_type(texture), texel_sign(texture)));
        let coord_ty = self.texture_sample_coord_type(texture);
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.sampler_handle_type(), sampler);
        if info.has(DEPTH) {
            call.push(self.int_ty(1), self.get_int(1));
        }
        call.push(coord_ty, self.nan_to_zero_if_asked(at.coord));
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(at.array_index, &name)?);
        }
        call.push(coord_ty, self.nan_to_zero_if_asked(grad.dx));
        call.push(coord_ty, self.nan_to_zero_if_asked(grad.dy));
        call.push(self.float_ty(1), grad.min_lod);
        if let Some(n) = offset_width(texture.kind, false) {
            call.push(self.bool_ty(), self.get_bool(n > 1));
            call.push(self.int_ty(n), operand(at.offset, &name)?);
        }
        call.push(self.int_ty(1), self.get_int(0));
        let result = call.finish(
            &name,
            self.texture_sample_result_type(texture),
            SAMPLE_EXPLICIT,
        )?;
        Ok(self.pair(result))
    }

    /// `air.sample_compare_*_grad`: a depth comparison with derivatives (`CreateSampleCmpGrad`).
    ///
    /// Cubes take no offset.
    ///
    /// # Errors
    ///
    /// A texture that is not a depth texture (the C++'s null), or a slice or offset the kind uses
    /// and `at` lacks.
    pub fn create_sample_cmp_grad(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        sampler: Val<'cx>,
        at: &SampleAt<'cx>,
        reference: Val<'cx>,
        grad: &Gradient<'cx>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        if !info.has(DEPTH) {
            return Err(Error::new(
                b"invalid operation: compare on non depth texture.",
            ));
        }
        let mut name = [b"air.sample_compare_".as_slice(), info.suffix, b"_grad"].concat();
        name.extend(self.type_overload_suffix(self.texel_type(texture), texel_sign(texture)));
        let coord_ty = self.texture_sample_coord_type(texture);
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.sampler_handle_type(), sampler);
        call.push(self.int_ty(1), self.get_int(1));
        call.push(coord_ty, self.nan_to_zero_if_asked(at.coord));
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(at.array_index, &name)?);
        }
        call.push(self.float_ty(1), reference);
        call.push(coord_ty, self.nan_to_zero_if_asked(grad.dx));
        call.push(coord_ty, self.nan_to_zero_if_asked(grad.dy));
        call.push(self.float_ty(1), grad.min_lod);
        if !info.has(CUBE) {
            call.push(self.bool_ty(), self.get_bool(true));
            call.push(self.int_ty(2), operand(at.offset, &name)?);
        }
        call.push(self.int_ty(1), self.get_int(0));
        let result = call.finish(
            &name,
            self.texture_sample_result_type(texture),
            SAMPLE_EXPLICIT,
        )?;
        Ok(self.pair(result))
    }

    /// `air.atomic_fetch_*_explicit_*` or `air.atomic_exchange_explicit_*` on a texel.
    ///
    /// The texture form of `CreateAtomicRMW`; `op` is the C API's (`ATOMIC_RMW_BIN_OP_ADD`, …).
    ///
    /// # Errors
    ///
    /// A texture of floats or a depth texture (the C++'s null), or an array kind without a slice.
    pub fn create_texture_atomic_rmw(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        op: sys::AtomicRMWBinOp,
        pos: Val<'cx>,
        value: Val<'cx>,
        array_index: Option<Val<'cx>>,
    ) -> Result<Val<'cx>, Error> {
        let info = atomic_texture(texture)?;
        let texel = self.texel_type(texture);
        let mut name = b"air.atomic_".to_vec();
        let sign = if op == sys::ATOMIC_RMW_BIN_OP_XCHG {
            name.extend(b"exchange");
            Signedness::Unsigned
        } else {
            name.extend(b"fetch_");
            let (op_name, sign) = atomic_op(op);
            name.extend(op_name);
            sign
        };
        name.extend([b"_explicit_".as_slice(), info.suffix].concat());
        name.extend(self.type_overload_suffix(texel, sign));
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.texture_rw_position_type(texture), pos);
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(array_index, &name)?);
        }
        call.push(texel, value);
        call.push(self.int_ty(1), self.get_int(0));
        call.push(self.int_ty(1), self.get_int(texture.memory_access as u32));
        call.finish(&name, texel, ATOMIC)
    }

    /// `air.atomic_compare_exchange_weak_explicit_*`: the old texel and whether it was swapped.
    ///
    /// `CreateAtomicCmpXchg`: `cmp` goes through a stack slot, which the call updates.
    ///
    /// # Errors
    ///
    /// A texture of floats or a depth texture (the C++'s null), or an array kind without a slice.
    pub fn create_atomic_cmpxchg(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        pos: Val<'cx>,
        cmp: Val<'cx>,
        new: Val<'cx>,
        array_index: Option<Val<'cx>>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = atomic_texture(texture)?;
        let texel = self.texel_type(texture);
        let mut name = [
            b"air.atomic_compare_exchange_weak_explicit_".as_slice(),
            info.suffix,
        ]
        .concat();
        name.extend(self.type_overload_suffix(texel, Signedness::Unsigned));
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.texture_rw_position_type(texture), pos);
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(array_index, &name)?);
        }
        let slot = self.b.alloca(texel, c"");
        let ptr = self.b.const_gep1_64(texel, slot, 0);
        self.b.store(cmp, ptr);
        call.push(self.tcx().ptr(texel, 0), slot);
        call.push(texel, new);
        call.push(self.int_ty(1), self.get_int(0));
        call.push(self.int_ty(1), self.get_int(0));
        call.push(self.int_ty(1), self.get_int(texture.memory_access as u32));
        let swapped = call.finish(&name, self.bool_ty(), ATOMIC)?;
        Ok((self.b.load(texel, ptr), swapped))
    }

    /// `air.read_*`: the texel and the residency byte (`CreateRead`).
    ///
    /// # Errors
    ///
    /// A slice, sample, face or level the kind uses and `at` lacks.
    pub fn create_read(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        at: &TexelAt<'cx>,
        device_coherent: bool,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        let mut name = [b"air.read_".as_slice(), info.suffix].concat();
        if device_coherent {
            name.extend(b".device_coherent");
        }
        name.extend(self.type_overload_suffix(self.texel_type(texture), texel_sign(texture)));
        let int = self.int_ty(1);
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        if info.has(DEPTH) {
            call.push(int, self.get_int(1));
        }
        call.push(self.texture_rw_position_type(texture), at.pos);
        if info.has(CUBE) {
            call.push(int, operand(at.sample_or_face, &name)?);
        }
        if info.has(ARRAY) {
            call.push(int, operand(at.array_index, &name)?);
        }
        if info.has(MS) {
            call.push(int, operand(at.sample_or_face, &name)?);
        }
        if info.has(MIPMAPPED) {
            call.push(int, operand(at.level, &name)?);
        }
        call.push(int, self.get_int(texture.memory_access as u32));
        let result = call.finish(&name, self.texture_sample_result_type(texture), READ)?;
        Ok(self.pair(result))
    }

    /// `air.write_*` (`CreateWrite`).
    ///
    /// # Errors
    ///
    /// A slice, face or level the kind uses and `at` lacks.
    pub fn create_write(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        at: &TexelAt<'cx>,
        value: Val<'cx>,
        device_coherent: bool,
    ) -> Result<Val<'cx>, Error> {
        let info = texture.kind.info();
        let mut name = [b"air.write_".as_slice(), info.suffix].concat();
        if device_coherent {
            name.extend(b".device_coherent");
        }
        name.extend(self.type_overload_suffix(self.texel_type(texture), texel_sign(texture)));
        let int = self.int_ty(1);
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.texture_rw_position_type(texture), at.pos);
        if info.has(CUBE) {
            call.push(int, operand(at.sample_or_face, &name)?);
        }
        if info.has(ARRAY) {
            call.push(int, operand(at.array_index, &name)?);
        }
        call.push(self.texel_type(texture), value);
        if info.has(MIPMAPPED) {
            call.push(int, operand(at.level, &name)?);
        }
        call.push(int, self.get_int(texture.memory_access as u32));
        call.finish(&name, self.void_ty(), WRITE)
    }

    /// `air.gather_*`: four texels' `component` and the residency byte (`CreateGather`).
    ///
    /// A depth texture's gather takes no component.
    ///
    /// # Errors
    ///
    /// A slice, offset or component the kind uses and the operands lack.
    pub fn create_gather(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        sampler: Val<'cx>,
        at: &SampleAt<'cx>,
        component: Option<Val<'cx>>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        let mut name = [b"air.gather_".as_slice(), info.suffix].concat();
        name.extend(
            self.type_overload_suffix(self.texel_gather_type(texture), texel_sign(texture)),
        );
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.sampler_handle_type(), sampler);
        if info.has(DEPTH) {
            call.push(self.int_ty(1), self.get_int(1));
        }
        call.push(
            self.texture_sample_coord_type(texture),
            self.nan_to_zero_if_asked(at.coord),
        );
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(at.array_index, &name)?);
        }
        if offset_width(texture.kind, true).is_some() {
            call.push(self.bool_ty(), self.get_bool(true));
            call.push(self.int_ty(2), operand(at.offset, &name)?);
        }
        if !info.has(DEPTH) {
            call.push(self.int_ty(1), operand(component, &name)?);
        }
        call.push(self.int_ty(1), self.get_int(0));
        let result = call.finish(
            &name,
            self.texture_gather_result_type(texture),
            SAMPLE_EXPLICIT,
        )?;
        Ok(self.pair(result))
    }

    /// `air.gather_compare_*`: four depth comparisons (`CreateGatherCompare`).
    ///
    /// # Errors
    ///
    /// A texture that is not a depth texture (the C++'s null), or a slice or offset the kind uses
    /// and `at` lacks.
    pub fn create_gather_compare(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        sampler: Val<'cx>,
        at: &SampleAt<'cx>,
        reference: Val<'cx>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        if !info.has(DEPTH) {
            return Err(Error::new(
                b"invalid operation: compare on non depth texture.",
            ));
        }
        let name = [b"air.gather_compare_".as_slice(), info.suffix, b".f32"].concat();
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.push(self.sampler_handle_type(), sampler);
        call.push(self.int_ty(1), self.get_int(1));
        call.push(
            self.texture_sample_coord_type(texture),
            self.nan_to_zero_if_asked(at.coord),
        );
        if info.has(ARRAY) {
            call.push(self.int_ty(1), operand(at.array_index, &name)?);
        }
        call.push(self.float_ty(1), reference);
        if matches!(
            texture.kind,
            TextureKind::Depth2d | TextureKind::Depth2dArray
        ) {
            call.push(self.bool_ty(), self.get_bool(true));
            call.push(self.int_ty(2), operand(at.offset, &name)?);
        }
        call.push(self.int_ty(1), self.get_int(0));
        let result = call.finish(
            &name,
            self.texture_gather_result_type(texture),
            SAMPLE_EXPLICIT,
        )?;
        Ok(self.pair(result))
    }

    /// `air.get_*`: a texture's size, slices, levels or samples, 0 for a null handle.
    ///
    /// `CreateTextureQuery`; a mipmapped kind's width, height and depth take `level`.
    ///
    /// # Errors
    ///
    /// A level the query uses and the operands lack.
    pub fn create_texture_query(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        query: Query,
        level: Option<Val<'cx>>,
    ) -> Result<Val<'cx>, Error> {
        let info = texture.kind.info();
        let sized = matches!(query, Query::Width | Query::Height | Query::Depth);
        let name = [b"air.".as_slice(), query.name(), b"_", info.suffix].concat();
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        if sized && info.has(MIPMAPPED) {
            call.push(self.int_ty(1), operand(level, &name)?);
        }
        let is_null = self.b.is_null(handle);
        let value = call.finish(&name, self.int_ty(1), READ)?;
        Ok(self.b.select(is_null, self.get_int(0), value))
    }

    /// `air.calculate_clamped_lod_*` and `air.calculate_unclamped_lod_*`, in that order.
    ///
    /// `CreateCalculateLOD`: both levels at `coord`.
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which these names pass.
    pub fn create_calculate_lod(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
        sampler: Val<'cx>,
        coord: Val<'cx>,
    ) -> Result<(Val<'cx>, Val<'cx>), Error> {
        let info = texture.kind.info();
        let params = [
            self.texture_handle_type(texture.kind),
            self.sampler_handle_type(),
            self.texture_sample_coord_type(texture),
            self.int_ty(1),
        ];
        let args = [
            handle,
            sampler,
            self.nan_to_zero_if_asked(coord),
            self.get_int(0),
        ];
        let fn_ty = self.tcx().function(self.float_ty(1), &params, false);
        let clamped = self.declare(
            &[b"air.calculate_clamped_lod_".as_slice(), info.suffix].concat(),
            fn_ty,
            SAMPLE,
        )?;
        let unclamped = self.declare(
            &[b"air.calculate_unclamped_lod_".as_slice(), info.suffix].concat(),
            fn_ty,
            SAMPLE,
        )?;
        Ok((self.b.call(clamped, &args), self.b.call(unclamped, &args)))
    }

    /// `air.fence_*` (`CreateTextureFence`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which these names pass.
    pub fn create_texture_fence(
        &self,
        texture: &Texture,
        handle: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let name = [b"air.fence_".as_slice(), texture.kind.info().suffix].concat();
        let mut call = Call::new(self);
        call.push(self.texture_handle_type(texture.kind), handle);
        call.finish(&name, self.void_ty(), TEXTURE_FENCE)
    }

    // Fragment shaders.

    /// `air.get_num_samples.i32` (`CreateGetNumSamples`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the name passes.
    pub fn create_get_num_samples(&self) -> Result<Val<'cx>, Error> {
        let mut call = Call::new(self);
        call.push(self.int_ty(1), self.get_int(0));
        call.finish(b"air.get_num_samples.i32", self.int_ty(1), PURE)
    }

    /// `air.dfdx` or `air.dfdy` of a floating-point value (`CreateDerivative`).
    ///
    /// # Errors
    ///
    /// A value that is not floating-point (the C++'s null).
    pub fn create_derivative(&self, v: Val<'cx>, y_axis: bool) -> Result<Val<'cx>, Error> {
        if !self.is_fp(v.ty()) {
            return Err(Error::new(b"invalid operation: derivative on non-fp type."));
        }
        let mut name = if y_axis { b"air.dfdy" } else { b"air.dfdx" }.to_vec();
        name.extend(self.type_overload_suffix(v.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(v.ty(), v);
        call.finish(&name, v.ty(), PLAIN)
    }

    /// `air.discard_fragment` (`CreateDiscard`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the name passes.
    pub fn create_discard(&self) -> Result<Val<'cx>, Error> {
        Call::new(self).finish(b"air.discard_fragment", self.void_ty(), PLAIN)
    }

    // Compute shaders.

    /// `air.wg.barrier` or, for a SIMD group, `air.simdgroup.barrier` (`CreateBarrier`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_barrier(&self, flags: MemFlags, simdgroup: bool) -> Result<Val<'cx>, Error> {
        let name: &[u8] = if simdgroup {
            b"air.simdgroup.barrier"
        } else {
            b"air.wg.barrier"
        };
        let mut call = Call::new(self);
        call.push(self.int_ty(1), self.get_int(flags.bits()));
        call.push(self.int_ty(1), self.get_int(1));
        call.finish(name, self.void_ty(), BARRIER)
    }

    // Math.

    /// `air.fma`, or a multiply and an add when FMA is defused (`CreateFMA`).
    ///
    /// # Errors
    ///
    /// Operands that are not floating-point or not of one type (the C++'s null).
    pub fn create_fma(&self, x: Val<'cx>, y: Val<'cx>, z: Val<'cx>) -> Result<Val<'cx>, Error> {
        if !self.is_fp(x.ty()) {
            return Err(Error::new(b"invalid operation: fma on non-fp type."));
        }
        if x.ty() != y.ty() || x.ty() != z.ty() {
            return Err(Error::new(
                b"invalid operation: fma has unmatched operand types.",
            ));
        }
        if self.shader_flags & SM50_SHADER_FLAG_DEFUSE_FMA != 0 {
            return Ok(self.b.fadd(self.b.fmul(x, y), z));
        }
        let mut name = b"air.fma".to_vec();
        name.extend(self.type_overload_suffix(x.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(x.ty(), x);
        call.push(y.ty(), y);
        call.push(z.ty(), z);
        call.finish(&name, x.ty(), PURE)
    }

    /// `air.dot`, or a multiply of scalars (`CreateDotProduct`).
    ///
    /// # Errors
    ///
    /// Operands that are not floating-point or not of one type (the C++'s null).
    pub fn create_dot_product(&self, lhs: Val<'cx>, rhs: Val<'cx>) -> Result<Val<'cx>, Error> {
        if !self.is_fp(lhs.ty()) {
            return Err(Error::new(b"invalid operation: dotproduct on non-fp type."));
        }
        if lhs.ty() != rhs.ty() {
            return Err(Error::new(
                b"invalid operation: dotproduct has unmatched operand types.",
            ));
        }
        let TyKind::Vector(scalar, _) = self.tcx().kind(lhs.ty()) else {
            return Ok(self.b.fmul(lhs, rhs));
        };
        let mut name = b"air.dot".to_vec();
        name.extend(self.type_overload_suffix(lhs.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(lhs.ty(), lhs);
        call.push(rhs.ty(), rhs);
        call.finish(&name, scalar, PURE)
    }

    /// `air.ctz` or `air.clz`, zero inputs defined (`CreateCountZero`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_count_zero(&self, v: Val<'cx>, trailing: bool) -> Result<Val<'cx>, Error> {
        let mut name = if trailing { b"air.ctz" } else { b"air.clz" }.to_vec();
        name.extend(self.type_overload_suffix(v.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(v.ty(), v);
        call.push(self.bool_ty(), self.get_bool(false));
        call.finish(&name, v.ty(), PURE)
    }

    // Conversions.

    /// `air.convert` to `float` lanes, from a value of `src_sign` (`CreateConvertToFloat`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_convert_to_float(
        &self,
        v: Val<'cx>,
        src_sign: Signedness,
    ) -> Result<Val<'cx>, Error> {
        let to = self.float_ty(self.lanes(v.ty()));
        self.convert(v, to, Signedness::Signed, src_sign)
    }

    /// `air.convert` to `half` lanes, from a value of `src_sign` (`CreateConvertToHalf`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_convert_to_half(
        &self,
        v: Val<'cx>,
        src_sign: Signedness,
    ) -> Result<Val<'cx>, Error> {
        let to = self.half_ty(self.lanes(v.ty()));
        self.convert(v, to, Signedness::Signed, src_sign)
    }

    /// `air.convert` to signed `i32` lanes (`CreateConvertToSigned`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_convert_to_signed(&self, v: Val<'cx>) -> Result<Val<'cx>, Error> {
        let to = self.int_ty(self.lanes(v.ty()));
        self.convert(v, to, Signedness::Signed, Signedness::Signed)
    }

    /// `air.convert` to unsigned `i32` lanes (`CreateConvertToUnsigned`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_convert_to_unsigned(&self, v: Val<'cx>) -> Result<Val<'cx>, Error> {
        let to = self.int_ty(self.lanes(v.ty()));
        self.convert(v, to, Signedness::Unsigned, Signedness::Unsigned)
    }

    /// `air.load.device_coherent` of a `ty` (`CreateDeviceCoherentLoad`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_device_coherent_load(&self, ty: Ty, ptr: Val<'cx>) -> Result<Val<'cx>, Error> {
        let mut name = b"air.load.device_coherent".to_vec();
        name.extend(self.type_overload_suffix(ty, Signedness::DontCare));
        name.extend(self.type_overload_suffix(ptr.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(ptr.ty(), ptr);
        call.finish(&name, ty, COHERENT_LOAD)
    }

    /// `air.store.device_coherent` (`CreateDeviceCoherentStore`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_device_coherent_store(
        &self,
        v: Val<'cx>,
        ptr: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let mut name = b"air.store.device_coherent".to_vec();
        name.extend(self.type_overload_suffix(v.ty(), Signedness::DontCare));
        name.extend(self.type_overload_suffix(ptr.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(v.ty(), v);
        call.push(ptr.ty(), ptr);
        call.finish(&name, self.void_ty(), COHERENT_STORE)
    }

    // Pull-model interpolation.

    /// `air.interpolate_center_*` (`CreateInterpolateAtCenter`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_interpolate_at_center(
        &self,
        interpolant: Val<'cx>,
        perspective: bool,
    ) -> Result<Val<'cx>, Error> {
        let name: &[u8] = if perspective {
            b"air.interpolate_center_perspective.v4f32"
        } else {
            b"air.interpolate_center_no_perspective.v4f32"
        };
        let mut call = Call::new(self);
        call.push(interpolant.ty(), interpolant);
        call.finish(name, self.float_ty(4), READ)
    }

    /// `air.interpolate_centroid_*` (`CreateInterpolateAtCentroid`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_interpolate_at_centroid(
        &self,
        interpolant: Val<'cx>,
        perspective: bool,
    ) -> Result<Val<'cx>, Error> {
        let name: &[u8] = if perspective {
            b"air.interpolate_centroid_perspective.v4f32"
        } else {
            b"air.interpolate_centroid_no_perspective.v4f32"
        };
        let mut call = Call::new(self);
        call.push(interpolant.ty(), interpolant);
        call.finish(name, self.float_ty(4), READ)
    }

    /// `air.interpolate_sample_*` (`CreateInterpolateAtSample`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_interpolate_at_sample(
        &self,
        interpolant: Val<'cx>,
        sample: Val<'cx>,
        perspective: bool,
    ) -> Result<Val<'cx>, Error> {
        let name: &[u8] = if perspective {
            b"air.interpolate_sample_perspective.v4f32"
        } else {
            b"air.interpolate_sample_no_perspective.v4f32"
        };
        let mut call = Call::new(self);
        call.push(interpolant.ty(), interpolant);
        call.push(self.int_ty(1), sample);
        call.finish(name, self.float_ty(4), INTERPOLATE_SAMPLE)
    }

    /// `air.interpolate_offset_*` (`CreateInterpolateAtOffset`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_interpolate_at_offset(
        &self,
        interpolant: Val<'cx>,
        offset: Val<'cx>,
        perspective: bool,
    ) -> Result<Val<'cx>, Error> {
        let name: &[u8] = if perspective {
            b"air.interpolate_offset_perspective.v4f32"
        } else {
            b"air.interpolate_offset_no_perspective.v4f32"
        };
        let mut call = Call::new(self);
        call.push(interpolant.ty(), interpolant);
        call.push(self.float_ty(2), offset);
        call.finish(name, self.float_ty(4), READ)
    }

    // Object and mesh shaders: the function's mesh or grid argument is the call's first.

    /// `air.set_threadgroups_per_grid_mesh_properties` (`CreateSetMeshProperties`).
    ///
    /// # Errors
    ///
    /// A function without the grid properties argument (where the C++ called with null).
    pub fn create_set_mesh_properties(&self, grid_size: Val<'cx>) -> Result<Val<'cx>, Error> {
        let name = b"air.set_threadgroups_per_grid_mesh_properties";
        let props_ty = self.mesh_grid_props_type();
        let mut call = Call::new(self);
        call.push(props_ty, operand(self.function_arg(props_ty), name)?);
        call.push(self.int_ty(3), grid_size);
        call.finish(name, self.void_ty(), MESH_PROPERTIES)
    }

    /// `air.set_render_target_array_index_mesh.i32` (`CreateSetMeshRenderTargetArrayIndex`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_render_target_array_index(
        &self,
        vertex: Val<'cx>,
        array_index: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let int = self.int_ty(1);
        self.mesh_call(
            b"air.set_render_target_array_index_mesh.i32",
            &[(int, vertex), (int, array_index)],
        )
    }

    /// `air.set_viewport_array_index_mesh.i32` (`CreateSetMeshViewportArrayIndex`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_viewport_array_index(
        &self,
        vertex: Val<'cx>,
        array_index: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let int = self.int_ty(1);
        self.mesh_call(
            b"air.set_viewport_array_index_mesh.i32",
            &[(int, vertex), (int, array_index)],
        )
    }

    /// `air.set_position_mesh`, a non-finite position replaced (`CreateSetMeshPosition`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_position(
        &self,
        vertex: Val<'cx>,
        position: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let name = b"air.set_position_mesh";
        let mesh_ty = self.mesh_handle_type();
        let params = [mesh_ty, self.int_ty(1), self.float_ty(4)];
        let callee = self.declare(
            name,
            self.tcx().function(self.void_ty(), &params, false),
            WRITE,
        )?;
        let mesh = operand(self.function_arg(mesh_ty), name)?;
        let position = self.sanitize_position(position);
        Ok(self.b.call(callee, &[mesh, vertex, position]))
    }

    /// `air.set_clip_distance_mesh` (`CreateSetMeshClipDistance`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_clip_distance(
        &self,
        vertex: Val<'cx>,
        index: Val<'cx>,
        value: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let int = self.int_ty(1);
        self.mesh_call(
            b"air.set_clip_distance_mesh",
            &[(int, index), (int, vertex), (self.float_ty(1), value)],
        )
    }

    /// `air.set_vertex_data_mesh` of a varying (`CreateSetMeshVertexData`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_vertex_data(
        &self,
        vertex: Val<'cx>,
        data_index: Val<'cx>,
        value: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let mut name = b"air.set_vertex_data_mesh".to_vec();
        name.extend(self.type_overload_suffix(value.ty(), Signedness::DontCare));
        let int = self.int_ty(1);
        self.mesh_call(
            &name,
            &[(int, data_index), (int, vertex), (value.ty(), value)],
        )
    }

    /// `air.set_index_mesh`, the vertex as a byte (`CreateSetMeshIndex`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_index(
        &self,
        index: Val<'cx>,
        vertex: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let name = b"air.set_index_mesh";
        let mesh_ty = self.mesh_handle_type();
        let params = [mesh_ty, self.int_ty(1), self.byte_ty()];
        let callee = self.declare(
            name,
            self.tcx().function(self.void_ty(), &params, false),
            WRITE,
        )?;
        let mesh = operand(self.function_arg(mesh_ty), name)?;
        let vertex = self.b.zext_or_trunc(vertex, self.byte_ty());
        Ok(self.b.call(callee, &[mesh, index, vertex]))
    }

    /// `air.set_primitive_count_mesh` (`CreateSetMeshPrimitiveCount`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_primitive_count(&self, count: Val<'cx>) -> Result<Val<'cx>, Error> {
        self.mesh_call(b"air.set_primitive_count_mesh", &[(self.int_ty(1), count)])
    }

    /// `air.set_point_size_mesh` (`CreateSetMeshPointSize`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_point_size(
        &self,
        vertex: Val<'cx>,
        size: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        self.mesh_call(
            b"air.set_point_size_mesh",
            &[(self.int_ty(1), vertex), (self.float_ty(1), size)],
        )
    }

    // Floating-point and integer operations.

    /// `air.[fast_]<op>` of a floating-point value (`CreateFPUnOp`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_fp_un_op(&self, op: FpUnOp, v: Val<'cx>, fast: bool) -> Result<Val<'cx>, Error> {
        let mut name = if fast {
            b"air.fast_".as_slice()
        } else {
            b"air."
        }
        .to_vec();
        name.extend(op.name());
        name.extend(self.type_overload_suffix(v.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(v.ty(), v);
        call.finish(&name, v.ty(), PURE)
    }

    /// Whether each lane of a `float` value is a NaN, read from its bits (`CreateIsNaN`).
    #[must_use]
    pub fn create_is_nan(&self, v: Val<'cx>) -> Val<'cx> {
        let (low31, inf) = (self.get_int(0x7fff_ffff), self.get_int(0x7f80_0000));
        let (low31, inf, cast_ty) = match self.tcx().kind(v.ty()) {
            TyKind::Vector(_, n) => (
                self.b.vector_splat(n, low31, c""),
                self.b.vector_splat(n, inf, c""),
                self.int_ty(n),
            ),
            _ => (low31, inf, self.int_ty(1)),
        };
        let bits = self.b.and(self.b.bitcast(v, cast_ty), low31);
        self.b.icmp(sys::INT_UGT, bits, inf)
    }

    /// `air.[fast_]fmax` or `air.[fast_]fmin` (`CreateFPBinOp`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_fp_bin_op(
        &self,
        op: FpBinOp,
        lhs: Val<'cx>,
        rhs: Val<'cx>,
        fast: bool,
    ) -> Result<Val<'cx>, Error> {
        let mut name = if fast {
            b"air.fast_".as_slice()
        } else {
            b"air."
        }
        .to_vec();
        name.extend(op.name());
        name.extend(self.type_overload_suffix(lhs.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(lhs.ty(), lhs);
        call.push(lhs.ty(), rhs);
        call.finish(&name, lhs.ty(), PURE)
    }

    /// `air.reverse_bits` or `air.popcount` (`CreateIntUnOp`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_int_un_op(&self, op: IntUnOp, v: Val<'cx>) -> Result<Val<'cx>, Error> {
        let mut name = [b"air.".as_slice(), op.name()].concat();
        name.extend(self.type_overload_suffix(v.ty(), Signedness::DontCare));
        let mut call = Call::new(self);
        call.push(v.ty(), v);
        call.finish(&name, v.ty(), PURE)
    }

    /// `air.max`, `air.min` or `air.mul_hi`, signed or not (`CreateIntBinOp`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the names pass.
    pub fn create_int_bin_op(
        &self,
        op: IntBinOp,
        lhs: Val<'cx>,
        rhs: Val<'cx>,
        signed: bool,
    ) -> Result<Val<'cx>, Error> {
        let mut name = [b"air.".as_slice(), op.name()].concat();
        let sign = if signed {
            Signedness::Signed
        } else {
            Signedness::Unsigned
        };
        name.extend(self.type_overload_suffix(lhs.ty(), sign));
        let mut call = Call::new(self);
        call.push(lhs.ty(), lhs);
        call.push(lhs.ty(), rhs);
        call.finish(&name, lhs.ty(), PURE)
    }

    /// `air.atomic.global.*` or `air.atomic.local.*` on a device or threadgroup pointer.
    ///
    /// The pointer form of `CreateAtomicRMW`; `op` is the C API's (`ATOMIC_RMW_BIN_OP_ADD`, …).
    ///
    /// # Errors
    ///
    /// A `ptr` that is not a pointer to `v`'s type in address space 1 or 3 (the C++'s null).
    pub fn create_atomic_rmw(
        &self,
        op: sys::AtomicRMWBinOp,
        ptr: Val<'cx>,
        v: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let tcx = self.tcx();
        let TyKind::Ptr { pointee, addrspace } = tcx.kind(ptr.ty()) else {
            return Err(Error::new(b"invalid operation: atomicrmw: not a pointer."));
        };
        if pointee != v.ty() {
            return Err(Error::new(
                b"invalid operation: atomicrmw: mismatched atomic operands.",
            ));
        }
        let (space, mem_flags): (&[u8], u32) = match addrspace {
            1 => (b"global.", 3),
            3 => (b"local.", 1),
            _ => {
                return Err(Error::new(
                    b"invalid operation: atomicrmw: not a valid address space.",
                ));
            }
        };
        let mut name = [b"air.atomic.".as_slice(), space].concat();
        let sign = if op == sys::ATOMIC_RMW_BIN_OP_XCHG {
            name.extend(b"xchg");
            Signedness::DontCare
        } else {
            let (op_name, sign) = atomic_op(op);
            name.extend(op_name);
            sign
        };
        name.extend(self.type_overload_suffix(pointee, sign));
        let mut call = Call::new(self);
        call.push(ptr.ty(), ptr);
        call.push(pointee, v);
        call.push(self.int_ty(1), self.get_int(0));
        call.push(self.int_ty(1), self.get_int(mem_flags));
        call.push(self.bool_ty(), self.get_bool(true));
        call.finish(&name, pointee, ATOMIC)
    }

    /// The position, or `(0, 0, 1, 0)` when a lane is not finite (`SanitizePosition`).
    #[must_use]
    pub fn sanitize_position(&self, pos: Val<'cx>) -> Val<'cx> {
        let cx = self.b.cx();
        let exponent = self.get_int(0x7f80_0000);
        let exponents = cx.const_vector(&[exponent; 4]);
        let bits = self.b.and(self.b.bitcast(pos, self.int_ty(4)), exponents);
        let mask = self.b.icmp(sys::INT_NE, bits, exponents);
        let xy = self.b.and(
            self.b.extract_element_u64(mask, 0),
            self.b.extract_element_u64(mask, 1),
        );
        let zw = self.b.and(
            self.b.extract_element_u64(mask, 2),
            self.b.extract_element_u64(mask, 3),
        );
        let valid = self.b.and(xy, zw);
        let clipped = cx.const_vector(&[
            self.get_float(0.0),
            self.get_float(0.0),
            self.get_float(1.0),
            self.get_float(0.0),
        ]);
        self.b.select(valid, pos, clipped)
    }

    // Constants.

    /// The `float` constant `v` (`getFloat`).
    #[must_use]
    pub fn get_float(&self, v: f32) -> Val<'cx> {
        self.b.cx().const_real(self.float_ty(1), f64::from(v))
    }

    /// The `<2 x float>` constant (`getFloat2`).
    #[must_use]
    pub fn get_float2(&self, x: f32, y: f32) -> Val<'cx> {
        self.b
            .cx()
            .const_vector(&[self.get_float(x), self.get_float(y)])
    }

    /// The `half` constant of the bits `bits` (`getHalf`).
    #[must_use]
    pub fn get_half(&self, bits: u16) -> Val<'cx> {
        let int = self
            .b
            .cx()
            .const_int(self.tcx().int(16), u64::from(bits), false);
        self.b.bitcast(int, self.half_ty(1))
    }

    /// The `i32` constant `v` (`getInt`).
    #[must_use]
    pub fn get_int(&self, v: u32) -> Val<'cx> {
        self.b.cx().const_int(self.int_ty(1), u64::from(v), false)
    }

    /// The `<2 x i32>` constant (`getInt2`).
    #[must_use]
    pub fn get_int2(&self, x: u32, y: u32) -> Val<'cx> {
        self.b
            .cx()
            .const_vector(&[self.get_int(x), self.get_int(y)])
    }

    /// The `<3 x i32>` constant (`getInt3`).
    #[must_use]
    pub fn get_int3(&self, x: u32, y: u32, z: u32) -> Val<'cx> {
        self.b
            .cx()
            .const_vector(&[self.get_int(x), self.get_int(y), self.get_int(z)])
    }

    /// The `i1` constant (`getBool`).
    #[must_use]
    pub fn get_bool(&self, v: bool) -> Val<'cx> {
        self.b.cx().const_int(self.bool_ty(), u64::from(v), false)
    }

    // Types.

    /// `i32`, or a vector of `n` of them (`getIntTy`).
    #[must_use]
    pub fn int_ty(&self, n: u32) -> Ty {
        self.lanes_of(self.ty.int, n)
    }

    /// `float`, or a vector of `n` of them (`getFloatTy`).
    #[must_use]
    pub fn float_ty(&self, n: u32) -> Ty {
        self.lanes_of(self.ty.float, n)
    }

    /// `half`, or a vector of `n` of them (`getHalfTy`).
    #[must_use]
    pub fn half_ty(&self, n: u32) -> Ty {
        self.lanes_of(self.ty.half, n)
    }

    /// `i1` (`getBoolTy`).
    #[must_use]
    pub const fn bool_ty(&self) -> Ty {
        self.ty.bool
    }

    /// `void` (`getVoidTy`).
    #[must_use]
    pub fn void_ty(&self) -> Ty {
        self.tcx().void()
    }

    /// `i8` (`getByteTy`).
    #[must_use]
    pub const fn byte_ty(&self) -> Ty {
        self.ty.byte
    }

    /// The device pointer to a texture kind's struct (`getTextureHandleType`).
    #[must_use]
    pub fn texture_handle_type(&self, kind: TextureKind) -> Ty {
        let tcx = self.tcx();
        tcx.ptr(tcx.named_struct(kind.info().struct_name), 1)
    }

    /// A texel: one `float` for depth, else four lanes of the sample type (`getTexelType`).
    #[must_use]
    pub fn texel_type(&self, texture: &Texture) -> Ty {
        if texture.kind.info().has(DEPTH) {
            return self.float_ty(1);
        }
        match texture.sample_type {
            SampleType::Int | SampleType::Uint => self.int_ty(4),
            SampleType::Float => self.float_ty(4),
        }
    }

    /// A gather's texel: four `float`s for depth, else the texel (`getTexelGatherType`).
    #[must_use]
    pub fn texel_gather_type(&self, texture: &Texture) -> Ty {
        if texture.kind.info().has(DEPTH) {
            return self.float_ty(4);
        }
        self.texel_type(texture)
    }

    /// The `float` lanes of a sample's coordinates (`getTextureSampleCoordType`).
    #[must_use]
    pub fn texture_sample_coord_type(&self, texture: &Texture) -> Ty {
        self.float_ty(texture.kind.info().dimension)
    }

    /// The `i32` lanes of a read's or write's position, two for a cube's face.
    ///
    /// `getTextureRWPositionType`.
    #[must_use]
    pub fn texture_rw_position_type(&self, texture: &Texture) -> Ty {
        let info = texture.kind.info();
        self.int_ty(if info.has(CUBE) { 2 } else { info.dimension })
    }

    /// `{ texel, i8 }` (`getTextureSampleResultType`).
    #[must_use]
    pub fn texture_sample_result_type(&self, texture: &Texture) -> Ty {
        self.tcx()
            .literal_struct(&[self.texel_type(texture), self.byte_ty()], false)
    }

    /// `{ gather texel, i8 }` (`getTextureGatherResultType`).
    #[must_use]
    pub fn texture_gather_result_type(&self, texture: &Texture) -> Ty {
        self.tcx()
            .literal_struct(&[self.texel_gather_type(texture), self.byte_ty()], false)
    }

    /// `%struct._sampler_t addrspace(2)*` (`getSamplerHandleType`).
    #[must_use]
    pub fn sampler_handle_type(&self) -> Ty {
        let tcx = self.tcx();
        tcx.ptr(tcx.named_struct(c"struct._sampler_t"), 2)
    }

    /// `%struct._mesh_t addrspace(7)*` (`getMeshHandleType`).
    #[must_use]
    pub fn mesh_handle_type(&self) -> Ty {
        let tcx = self.tcx();
        tcx.ptr(tcx.named_struct(c"struct._mesh_t"), 7)
    }

    /// `%struct._mesh_grid_properties_t addrspace(3)*` (`getMeshGridPropsType`).
    #[must_use]
    pub fn mesh_grid_props_type(&self) -> Ty {
        let tcx = self.tcx();
        tcx.ptr(tcx.named_struct(c"struct._mesh_grid_properties_t"), 3)
    }

    /// The current function's mesh argument (`getMeshHandle`).
    #[must_use]
    pub fn mesh_handle(&self) -> Option<Val<'cx>> {
        self.function_arg(self.mesh_handle_type())
    }

    /// `v`, with its NaN lanes replaced by zero (`getNaNToZero`).
    #[must_use]
    pub fn nan_to_zero(&self, v: Val<'cx>) -> Val<'cx> {
        let is_nan = self.create_is_nan(v);
        self.b.select(is_nan, self.b.cx().null(v.ty()), v)
    }

    /// The overload suffix of `ty` (`getTypeOverloadSuffix`): `.f.v4f32`, `.u.p1i32`, ….
    ///
    /// A pointer's is its pointee's with `p<address space>`; a type that is neither an integer,
    /// `half` nor `float` is `.unknown_type_overload`.
    #[must_use]
    pub fn type_overload_suffix(&self, ty: Ty, sign: Signedness) -> Vec<u8> {
        let tcx = self.tcx();
        let (ty, addrspace) = match tcx.kind(ty) {
            TyKind::Ptr { pointee, addrspace } => (pointee, Some(addrspace)),
            _ => (ty, None),
        };
        let (scalar, lanes) = match tcx.kind(ty) {
            TyKind::Vector(element, n) => (element, Some(n)),
            _ => (ty, None),
        };
        let (sign_part, element): (&str, String) = match (tcx.kind(scalar), sign) {
            (TyKind::Float, Signedness::DontCare) => ("", "f32".into()),
            (TyKind::Float, _) => ("f.", "f32".into()),
            (TyKind::Half, Signedness::DontCare) => ("", "f16".into()),
            (TyKind::Half, _) => ("f.", "f16".into()),
            (TyKind::Int(bits), Signedness::DontCare) => ("", format!("i{bits}")),
            (TyKind::Int(bits), Signedness::Signed) => ("s.", format!("i{bits}")),
            (TyKind::Int(bits), Signedness::Unsigned) => ("u.", format!("i{bits}")),
            _ => return b".unknown_type_overload".to_vec(),
        };
        let space = addrspace.map(|a| format!("p{a}")).unwrap_or_default();
        let lanes = lanes.map(|n| format!("v{n}")).unwrap_or_default();
        format!(".{sign_part}{space}{lanes}{element}").into_bytes()
    }

    // Private: what the methods share.

    const fn tcx(&self) -> &'cx TyCx {
        self.b.cx().tcx()
    }

    /// `scalar`, or a vector of `n` of it.
    fn lanes_of(&self, scalar: Ty, n: u32) -> Ty {
        if n == 1 {
            scalar
        } else {
            self.tcx().vector(scalar, n)
        }
    }

    /// The lanes of `ty`: a vector's count, else 1.
    fn lanes(&self, ty: Ty) -> u32 {
        match self.tcx().kind(ty) {
            TyKind::Vector(_, n) => n,
            _ => 1,
        }
    }

    /// Whether `ty`'s scalar is floating-point (`getScalarType()->isFloatingPointTy()`).
    fn is_fp(&self, ty: Ty) -> bool {
        let tcx = self.tcx();
        let scalar = match tcx.kind(ty) {
            TyKind::Vector(element, _) => element,
            _ => ty,
        };
        matches!(
            tcx.kind(scalar),
            TyKind::Half | TyKind::Float | TyKind::Double
        )
    }

    /// `v`, through [`Self::nan_to_zero`] when the shader flags ask for it.
    fn nan_to_zero_if_asked(&self, v: Val<'cx>) -> Val<'cx> {
        if self.shader_flags & SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO != 0 {
            self.nan_to_zero(v)
        } else {
            v
        }
    }

    /// The control bit and the two level arguments of `lod`, as the C++'s overloads pass them.
    fn lod_args(&self, lod: &Lod<'cx>) -> (bool, Val<'cx>, Val<'cx>) {
        let zero = || self.get_float(0.0);
        match *lod {
            #[cfg(test)]
            Lod::Implicit => (false, zero(), zero()),
            Lod::Level(level) => (true, level, zero()),
            Lod::BiasMin(bias, min_lod) => (false, bias, min_lod),
        }
    }

    /// The two fields of a sample's, read's or gather's `{ texel, i8 }`.
    fn pair(&self, result: Val<'cx>) -> (Val<'cx>, Val<'cx>) {
        (
            self.b.extract_value(result, 0),
            self.b.extract_value(result, 1),
        )
    }

    /// The function `name` of type `fn_ty`, declared with `attrs` if it is new.
    fn declare(&self, name: &[u8], fn_ty: Ty, attrs: &Attrs) -> Result<Func<'cx>, Error> {
        self.m.get_or_insert_function(name, fn_ty, attrs)
    }

    /// `name(converted)`: the `air.convert` of `v` to `to`.
    fn convert(
        &self,
        v: Val<'cx>,
        to: Ty,
        to_sign: Signedness,
        from_sign: Signedness,
    ) -> Result<Val<'cx>, Error> {
        let mut name = b"air.convert".to_vec();
        name.extend(self.type_overload_suffix(to, to_sign));
        name.extend(self.type_overload_suffix(v.ty(), from_sign));
        let mut call = Call::new(self);
        call.push(v.ty(), v);
        call.finish(&name, to, PURE)
    }

    /// A mesh setter: the mesh argument, then `operands`.
    fn mesh_call(&self, name: &[u8], operands: &[(Ty, Val<'cx>)]) -> Result<Val<'cx>, Error> {
        let mesh_ty = self.mesh_handle_type();
        let mut call = Call::new(self);
        call.push(mesh_ty, operand(self.function_arg(mesh_ty), name)?);
        for &(ty, v) in operands {
            call.push(ty, v);
        }
        call.finish(name, self.void_ty(), WRITE)
    }

    /// The first argument of type `ty` of the function the builder is in.
    fn function_arg(&self, ty: Ty) -> Option<Val<'cx>> {
        let block = self.b.insert_block()?;
        // SAFETY: a block of this context.
        let function = unsafe { sys::get_basic_block_parent(block.raw()) };
        if function.is_null() {
            return None;
        }
        let raw_ty = self.tcx().lower(ty);
        // SAFETY: a function (a block's parent).
        let count = unsafe { sys::count_params(function) };
        (0..count).find_map(|i| {
            // SAFETY: a function with more than `i` parameters.
            let param = unsafe { sys::get_param(function, i) };
            // SAFETY: a live parameter.
            let param_ty = unsafe { sys::type_of(param) };
            // SAFETY: a parameter of this context whose LLVM type is `ty`'s.
            (param_ty == raw_ty).then(|| unsafe { self.b.cx().val(param, ty) })
        })
    }
}

/// A call being assembled: parameter types and arguments, pushed in the C++'s order.
struct Call<'s, 'a, 'cx> {
    air: &'s AirBuilder<'a, 'cx>,
    params: Vec<Ty>,
    args: Vec<Val<'cx>>,
}

impl<'s, 'a, 'cx> Call<'s, 'a, 'cx> {
    const fn new(air: &'s AirBuilder<'a, 'cx>) -> Self {
        Self {
            air,
            params: Vec::new(),
            args: Vec::new(),
        }
    }

    fn push(&mut self, ty: Ty, v: Val<'cx>) {
        self.params.push(ty);
        self.args.push(v);
    }

    /// Declares `name` returning `ret` with `attrs` (`getOrInsertFunction`) and calls it.
    fn finish(self, name: &[u8], ret: Ty, attrs: &Attrs) -> Result<Val<'cx>, Error> {
        let fn_ty = self.air.tcx().function(ret, &self.params, false);
        let callee = self.air.declare(name, fn_ty, attrs)?;
        Ok(self.air.b.call(callee, &self.args))
    }
}

/// A texture kind's row of `TextureInfo`.
struct TextureInfo {
    /// The name part of its intrinsics (`air_symbol_suffix`).
    suffix: &'static [u8],
    /// Its handle's struct, `struct._<suffix>_t`.
    struct_name: &'static CStr,
    /// The coordinates' lanes.
    dimension: u32,
    /// `ARRAY`, `CUBE`, `DEPTH`, `MS`, `MIPMAPPED`.
    flags: u8,
}

impl TextureInfo {
    const fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }
}

impl TextureKind {
    /// The kind's row of the C++'s `TextureInfo` table.
    const fn info(self) -> TextureInfo {
        let (suffix, struct_name, dimension, flags): (&[u8], &CStr, u32, u8) = match self {
            Self::TextureBuffer => (b"texture_buffer_1d", c"struct._texture_buffer_1d_t", 1, 0),
            // Mipmapped, though only level 0 is valid.
            Self::Texture1d => (b"texture_1d", c"struct._texture_1d_t", 1, MIPMAPPED),
            Self::Texture1dArray => (
                b"texture_1d_array",
                c"struct._texture_1d_array_t",
                1,
                ARRAY | MIPMAPPED,
            ),
            Self::Texture2d => (b"texture_2d", c"struct._texture_2d_t", 2, MIPMAPPED),
            Self::Texture2dArray => (
                b"texture_2d_array",
                c"struct._texture_2d_array_t",
                2,
                ARRAY | MIPMAPPED,
            ),
            Self::Texture3d => (b"texture_3d", c"struct._texture_3d_t", 3, MIPMAPPED),
            Self::TextureCube => (
                b"texture_cube",
                c"struct._texture_cube_t",
                3,
                CUBE | MIPMAPPED,
            ),
            Self::TextureCubeArray => (
                b"texture_cube_array",
                c"struct._texture_cube_array_t",
                3,
                ARRAY | CUBE | MIPMAPPED,
            ),
            Self::Texture2dMs => (b"texture_2d_ms", c"struct._texture_2d_ms_t", 2, MS),
            Self::Texture2dMsArray => (
                b"texture_2d_ms_array",
                c"struct._texture_2d_ms_array_t",
                2,
                ARRAY | MS,
            ),
            Self::Depth2d => (b"depth_2d", c"struct._depth_2d_t", 2, DEPTH | MIPMAPPED),
            Self::Depth2dArray => (
                b"depth_2d_array",
                c"struct._depth_2d_array_t",
                2,
                ARRAY | DEPTH | MIPMAPPED,
            ),
            Self::DepthCube => (
                b"depth_cube",
                c"struct._depth_cube_t",
                3,
                CUBE | DEPTH | MIPMAPPED,
            ),
            Self::DepthCubeArray => (
                b"depth_cube_array",
                c"struct._depth_cube_array_t",
                3,
                ARRAY | CUBE | DEPTH | MIPMAPPED,
            ),
            Self::Depth2dMs => (b"depth_2d_ms", c"struct._depth_2d_ms_t", 2, DEPTH | MS),
            Self::Depth2dMsArray => (
                b"depth_2d_ms_array",
                c"struct._depth_2d_ms_array_t",
                2,
                ARRAY | DEPTH | MS,
            ),
        };
        TextureInfo {
            suffix,
            struct_name,
            dimension,
            flags,
        }
    }
}

impl Query {
    /// The query's name part (the C++'s `QUERIES`).
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Width => b"get_width",
            Self::Height => b"get_height",
            Self::Depth => b"get_depth",
            Self::ArrayLength => b"get_array_size",
            Self::NumMipLevels => b"get_num_mip_levels",
            Self::NumSamples => b"get_num_samples",
        }
    }
}

impl FpUnOp {
    /// The operation's name part (the C++'s `FnNames`).
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Saturate => b"saturate",
            Self::Log2 => b"log2",
            Self::Exp2 => b"exp2",
            Self::Sqrt => b"sqrt",
            Self::Rsqrt => b"rsqrt",
            Self::Fract => b"fract",
            Self::Rint => b"rint",
            Self::Floor => b"floor",
            Self::Ceil => b"ceil",
            Self::Trunc => b"trunc",
            Self::Cos => b"cos",
            Self::Sin => b"sin",
            Self::Fabs => b"fabs",
            Self::Tan => b"tan",
            Self::Acos => b"acos",
            Self::Asin => b"asin",
            Self::Atan => b"atan",
            Self::Cosh => b"cosh",
            Self::Sinh => b"sinh",
            Self::Tanh => b"tanh",
        }
    }
}

impl FpBinOp {
    /// The operation's name part.
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Fmax => b"fmax",
            Self::Fmin => b"fmin",
        }
    }
}

impl IntUnOp {
    /// The operation's name part.
    const fn name(self) -> &'static [u8] {
        match self {
            Self::ReverseBits => b"reverse_bits",
            Self::Popcount => b"popcount",
        }
    }
}

impl IntBinOp {
    /// The operation's name part.
    const fn name(self) -> &'static [u8] {
        match self {
            Self::Max => b"max",
            Self::Min => b"min",
            Self::MulHi => b"mul_hi",
        }
    }
}

/// The sign of a texture's texels in an overload suffix (`getTexelSign`).
const fn texel_sign(texture: &Texture) -> Signedness {
    match texture.sample_type {
        SampleType::Int => Signedness::Signed,
        SampleType::Uint => Signedness::Unsigned,
        SampleType::Float => Signedness::DontCare,
    }
}

/// The offset lanes a sample (or, with `gather`, a gather) of `kind` takes, if any.
///
/// 1D kinds take one `i32` behind a false flag, 2D and depth 2D kinds two, 3D three (a gather:
/// only the 2D ones); cubes and multisampled kinds none.
const fn offset_width(kind: TextureKind, gather: bool) -> Option<u32> {
    match kind {
        TextureKind::Texture1d | TextureKind::Texture1dArray if !gather => Some(1),
        TextureKind::Texture2d
        | TextureKind::Texture2dArray
        | TextureKind::Depth2d
        | TextureKind::Depth2dArray => Some(2),
        TextureKind::Texture3d if !gather => Some(3),
        _ => None,
    }
}

/// The name part and sign of an atomic fetch operation; `<invalid atomic op>` for another.
const fn atomic_op(op: sys::AtomicRMWBinOp) -> (&'static [u8], Signedness) {
    match op {
        sys::ATOMIC_RMW_BIN_OP_ADD => (b"add", Signedness::Unsigned),
        sys::ATOMIC_RMW_BIN_OP_SUB => (b"sub", Signedness::Unsigned),
        sys::ATOMIC_RMW_BIN_OP_AND => (b"and", Signedness::Unsigned),
        sys::ATOMIC_RMW_BIN_OP_OR => (b"or", Signedness::Unsigned),
        sys::ATOMIC_RMW_BIN_OP_XOR => (b"xor", Signedness::Unsigned),
        sys::ATOMIC_RMW_BIN_OP_MAX => (b"max", Signedness::Signed),
        sys::ATOMIC_RMW_BIN_OP_MIN => (b"min", Signedness::Signed),
        sys::ATOMIC_RMW_BIN_OP_U_MAX => (b"max", Signedness::Unsigned),
        sys::ATOMIC_RMW_BIN_OP_U_MIN => (b"min", Signedness::Unsigned),
        _ => (b"<invalid atomic op>", Signedness::Unsigned),
    }
}

/// The kind's info, if atomics are allowed on `texture` (integers, not depth).
fn atomic_texture(texture: &Texture) -> Result<TextureInfo, Error> {
    if !matches!(texture.sample_type, SampleType::Int | SampleType::Uint) {
        return Err(Error::new(
            b"invalid operation: atomic operation on non integer texture.",
        ));
    }
    let info = texture.kind.info();
    if info.has(DEPTH) {
        return Err(Error::new(
            b"invalid operation: atomic operation on depth texture.",
        ));
    }
    Ok(info)
}

/// An operand the C++ passed whether or not it was null: an error naming the call when absent.
fn operand<'cx>(v: Option<Val<'cx>>, name: &[u8]) -> Result<Val<'cx>, Error> {
    v.ok_or_else(|| {
        Error::new(
            &[
                b"invalid operation: ".as_slice(),
                name,
                b" lacks an operand.",
            ]
            .concat(),
        )
    })
}

#[cfg(test)]
mod tests;
