//! The binding recorders: each resource's argument slot and reflection entry.
//!
//! [`ArgumentTable`] is `ArgumentBufferBuilder`'s recording half (`A/air_signature.cpp:90-162`):
//! an argument buffer's fields in order, each with the slot D3D binds it to.
//! [`FunctionSignatureDesc`] is `FunctionSignatureBuilder`'s (`:443-520`): a function's inputs
//! and outputs, where an element defined again answers the index of the first, as the C++'s
//! did. The LLVM layer lowers both (`Build`, `CreateFunction`); here they are plain data, with the
//! Metal types they name. [`argument_index`] is `airconv_public.h`'s `GetArgumentIndex`, and
//! [`ShaderArgument`] one entry of the tables `SM50GetArgumentsInfo` copies out.
//!
//! A type or metadata node the LLVM layer made (an argument buffer's structure, a draw-arguments
//! structure) is carried as a `u32` handle that layer gives meaning to.

use crate::shader_info::{ResourceType, ScalerDataType};

/// A scalar type of the Metal shading language (`MSLScalerType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MslScalar {
    Float,
    Int,
    Uint,
    Bool,
    Half,
    Ushort,
    Ulong,
}

/// `dimension` scalars (`MSLVector`); a dimension of 1 is the scalar itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MslVector {
    pub dimension: u32,
    pub scalar: MslScalar,
}

/// A scalar or a vector (`MSLScalerOrVectorType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MslValue {
    Scalar(MslScalar),
    Vector(MslVector),
}

/// How a texture or buffer is accessed (`air::MemoryAccess`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryAccess {
    Sample,
    Read,
    Write,
    ReadWrite,
}

/// A buffer's address space (`air::AddressSpace`), its number in the IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddressSpace {
    Unknown = 0,
    Device = 1,
    Constant = 2,
    Threadgroup = 3,
    ObjectData = 6,
}

/// A texture's kind (`llvm::air::Texture::ResourceKind`), in the C++'s order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureKind {
    TextureBuffer,
    Texture1d,
    Texture1dArray,
    Texture2d,
    Texture2dArray,
    Texture3d,
    TextureCube,
    TextureCubeArray,
    Texture2dMs,
    Texture2dMsArray,
    Depth2d,
    Depth2dArray,
    DepthCube,
    DepthCubeArray,
    Depth2dMs,
    Depth2dMsArray,
}

/// A texture argument's type (`MSLTexture`).
///
/// `kind` is what Metal binds (a 1D texture is bound as 2D), `kind_logical` what the shader
/// declared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MslTexture {
    pub component: MslScalar,
    pub access: MemoryAccess,
    pub kind: TextureKind,
    pub kind_logical: TextureKind,
}

/// A type an argument may have (`MSLRepresentableType`).
///
/// `Struct` is `MSLWhateverStruct`: a structure the LLVM layer made, by its handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MslType {
    Value(MslValue),
    Sampler,
    Texture(MslTexture),
    Struct { name: String, ty: u32 },
}

/// A constant argument's type (`MSLRepresentableTypeWithArray`).
#[derive(Debug, PartialEq, Eq)]
pub enum MslArgType {
    Type(MslType),
    Array {
        size: u32,
        element: MslType,
    },
    Pointer {
        pointee: MslType,
        address_space: AddressSpace,
    },
}

/// A buffer argument (`ArgumentBindingBuffer`); `buffer_size` `None` is unbounded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferBinding {
    pub buffer_size: Option<u32>,
    pub location_index: u32,
    pub array_size: u32,
    pub memory_access: MemoryAccess,
    pub address_space: AddressSpace,
    pub ty: MslType,
    pub arg_name: String,
    pub raster_order_group: Option<u32>,
}

/// A sampler argument (`ArgumentBindingSampler`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamplerBinding {
    pub location_index: u32,
    pub array_size: u32,
    pub arg_name: String,
}

/// A texture argument (`ArgumentBindingTexture`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextureBinding {
    pub location_index: u32,
    pub array_size: u32,
    pub memory_access: MemoryAccess,
    pub ty: MslTexture,
    pub arg_name: String,
    pub raster_order_group: Option<u32>,
}

/// An argument buffer passed to a function (`ArgumentBindingIndirectBuffer`).
///
/// `struct_type` and `struct_type_info` are the LLVM layer's handles of the buffer's structure
/// and its metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndirectBufferBinding {
    pub location_index: u32,
    pub array_size: u32,
    pub memory_access: MemoryAccess,
    pub address_space: AddressSpace,
    pub struct_type: u32,
    pub struct_type_info: u32,
    pub arg_name: String,
}

/// A constant in an argument buffer (`ArgumentBindingIndirectConstant`).
#[derive(Debug, PartialEq, Eq)]
pub struct IndirectConstantBinding {
    pub location_index: u32,
    pub array_size: u32,
    pub ty: MslArgType,
    pub arg_name: String,
}

/// One field of an argument buffer (`ArgumentBufferArguments`).
#[derive(Debug, PartialEq, Eq)]
pub enum TableField {
    Buffer(BufferBinding),
    IndirectBuffer(IndirectBufferBinding),
    Texture(TextureBinding),
    IndirectConstant(IndirectConstantBinding),
    Sampler(SamplerBinding),
}

/// An argument buffer's fields in order (`ArgumentBufferBuilder`'s `fieldsType`).
///
/// Each `define_*` answers the field's index. A `location_index` of `u32::MAX` is the field's
/// index, as the C++'s default argument made it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ArgumentTable {
    fields: Vec<TableField>,
}

/// How a pixel shader input is interpolated (`air::Interpolation`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Interpolation {
    CenterPerspective,
    CenterNoPerspective,
    CentroidPerspective,
    CentroidNoPerspective,
    SamplePerspective,
    SampleNoPerspective,
    Flat,
}

/// A depth output's comparison (`air::DepthArgument`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepthArgument {
    Any = 0,
    Greater = 1,
    Less = 2,
}

/// A mesh function's output topology (`air::MeshOutputTopology`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshOutputTopology {
    Point,
    Line,
    Triangle,
}

/// A function input (`FunctionInput`): an argument binding or a stage's input.
///
/// `VertexStageIn::component_type` is the signature's component type as the blob gives it
/// (`InputAttributeComponentType`: 1 uint, 2 int, 3 float).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionInput {
    Buffer(BufferBinding),
    IndirectBuffer(IndirectBufferBinding),
    Texture(TextureBinding),
    Sampler(SamplerBinding),
    VertexId,
    InstanceId,
    BaseVertex,
    BaseInstance,
    VertexStageIn {
        attribute: u32,
        component_type: u32,
        name: String,
    },
    PrimitiveId,
    ViewportArrayIndex,
    RenderTargetArrayIndex,
    FrontFacing,
    Position {
        interpolation: Interpolation,
    },
    SampleIndex,
    FragmentStageIn {
        user: String,
        ty: MslValue,
        interpolation: Interpolation,
        pull_mode: bool,
    },
    InputCoverage,
    Payload {
        size: u32,
    },
    MeshGridProperties,
    Mesh {
        vertex_count: u32,
        primitive_count: u32,
        topology: MeshOutputTopology,
    },
    ThreadIndexInThreadgroup,
    ThreadPositionInThreadgroup,
    ThreadPositionInGrid,
    ThreadgroupPositionInGrid,
    ThreadgroupsPerGrid,
    ThreadIndexInSimdgroup,
    ThreadsPerSimdgroup,
}

/// A function output (`FunctionOutput`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionOutput {
    Vertex {
        user: String,
        ty: MslValue,
    },
    Position {
        ty: MslValue,
    },
    ClipDistance {
        count: usize,
    },
    RenderTargetArrayIndex,
    ViewportArrayIndex,
    RenderTarget {
        dual_source_blending: bool,
        index: u32,
        ty: MslValue,
    },
    Depth {
        argument: DepthArgument,
    },
    CoverageMask,
    StencilRef,
}

/// A mesh's per-vertex or per-primitive user data (`OutputMeshData`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeshData {
    pub user: String,
    pub ty: MslValue,
    pub index: u32,
}

/// A mesh function's per-vertex output (`MeshVertexOutput`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshVertexOutput {
    Data(MeshData),
    Position { ty: MslValue },
    ClipDistance { count: usize },
    PointSize,
}

/// A mesh function's per-primitive output (`MeshPrimitiveOutput`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeshPrimitiveOutput {
    Data(MeshData),
    RenderTargetArrayIndex,
    ViewportArrayIndex,
}

/// A function's inputs and outputs, as `FunctionSignatureBuilder` records them.
///
/// The compiles copy the shader's and go on defining into the copy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionSignatureDesc {
    inputs: Vec<FunctionInput>,
    outputs: Vec<FunctionOutput>,
    mesh_vertex_outputs: Vec<MeshVertexOutput>,
    mesh_primitive_outputs: Vec<MeshPrimitiveOutput>,
    early_fragment_tests: bool,
    max_work_group_size: u32,
    max_mesh_work_group_size: u32,
}

/// A resource class of the reflection's tables (`SM50BindingType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingType {
    ConstantBuffer = 0,
    Sampler = 1,
    Srv = 2,
    Uav = 3,
}

/// One entry of the reflection's tables (`MTL_SM50_SHADER_ARGUMENT`).
///
/// `flags` are [`flag`]'s bits; `structure_ptr_offset` is the field's index in its argument
/// buffer.
#[derive(Debug, PartialEq, Eq)]
pub struct ShaderArgument {
    pub ty: BindingType,
    pub slot: u32,
    pub flags: u32,
    pub structure_ptr_offset: u32,
}

/// `MTL_SM50_SHADER_ARGUMENT_FLAG`: what a reflected argument is and how it is used.
pub mod flag {
    pub const BUFFER: u32 = 1 << 0;
    pub const TEXTURE: u32 = 1 << 1;
    pub const ELEMENT_WIDTH: u32 = 1 << 2;
    pub const UAV_COUNTER: u32 = 1 << 3;
    pub const TEXTURE_MINLOD_CLAMP: u32 = 1 << 4;
    pub const TBUFFER_OFFSET: u32 = 1 << 5;
    pub const TEXTURE_ARRAY: u32 = 1 << 6;
    pub const READ_ACCESS: u32 = 1 << 10;
    pub const WRITE_ACCESS: u32 = 1 << 11;
}

impl MslValue {
    pub const UINT: Self = Self::Scalar(MslScalar::Uint);
    pub const ULONG: Self = Self::Scalar(MslScalar::Ulong);
    pub const INT4: Self = Self::vector(4, MslScalar::Int);
    pub const UINT4: Self = Self::vector(4, MslScalar::Uint);
    pub const FLOAT4: Self = Self::vector(4, MslScalar::Float);

    const fn vector(dimension: u32, scalar: MslScalar) -> Self {
        Self::Vector(MslVector { dimension, scalar })
    }
}

impl ArgumentTable {
    /// `DefineBuffer`.
    pub fn define_buffer(
        &mut self,
        name: String,
        address_space: AddressSpace,
        memory_access: MemoryAccess,
        ty: MslType,
        location_index: u32,
        raster_order_group: Option<u32>,
    ) -> u32 {
        let index = self.next_index();
        self.fields.push(TableField::Buffer(BufferBinding {
            buffer_size: None,
            location_index: location_or(location_index, index),
            array_size: 1,
            memory_access,
            address_space,
            ty,
            arg_name: name,
            raster_order_group,
        }));
        index
    }

    /// `DefineSampler`.
    pub fn define_sampler(&mut self, name: String, location_index: u32) -> u32 {
        let index = self.next_index();
        self.fields.push(TableField::Sampler(SamplerBinding {
            location_index: location_or(location_index, index),
            array_size: 1,
            arg_name: name,
        }));
        index
    }

    /// `DefineTexture`: a 1D texture is bound as 2D (`lowering_texture_1d_to_2d`).
    pub fn define_texture(
        &mut self,
        name: String,
        kind: TextureKind,
        memory_access: MemoryAccess,
        component: MslScalar,
        location_index: u32,
        raster_order_group: Option<u32>,
    ) -> u32 {
        let index = self.next_index();
        let bound = match kind {
            TextureKind::Texture1d => TextureKind::Texture2d,
            TextureKind::Texture1dArray => TextureKind::Texture2dArray,
            other => other,
        };
        self.fields.push(TableField::Texture(TextureBinding {
            location_index: location_or(location_index, index),
            array_size: 1,
            memory_access,
            ty: MslTexture {
                component,
                access: memory_access,
                kind: bound,
                kind_logical: kind,
            },
            arg_name: name,
            raster_order_group,
        }));
        index
    }

    /// `DefineInteger64`: a `ulong` constant.
    pub fn define_integer64(&mut self, name: String, location_index: u32) -> u32 {
        let index = self.next_index();
        self.fields
            .push(TableField::IndirectConstant(IndirectConstantBinding {
                location_index: location_or(location_index, index),
                array_size: 1,
                ty: MslArgType::Type(MslType::Value(MslValue::ULONG)),
                arg_name: name,
            }));
        index
    }

    /// The fields, in order.
    #[must_use]
    pub fn fields(&self) -> &[TableField] {
        &self.fields
    }

    /// How many fields (`Size`), each one qword of the argument table.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.fields.len()
    }

    /// Whether the table has no field (`Empty`).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    fn next_index(&self) -> u32 {
        count_u32(self.fields.len())
    }
}

impl FunctionSignatureDesc {
    /// `DefineInput`: the index of an input of the same kind already defined, else a new one.
    ///
    /// The same kind is the same variant and, for a stage-in attribute its attribute, for a
    /// fragment input its name, for an argument binding its location; any other two of one
    /// variant are the same input.
    pub fn define_input(&mut self, input: FunctionInput) -> u32 {
        define(&mut self.inputs, input, same_input)
    }

    /// `DefineOutput`: the index of the same output already defined, else a new one.
    ///
    /// A vertex output is the same by its name, a render target by its index; any other two of
    /// one variant are the same output.
    pub fn define_output(&mut self, output: FunctionOutput) -> u32 {
        define(&mut self.outputs, output, |a, b| match (a, b) {
            (FunctionOutput::Vertex { user: a, .. }, FunctionOutput::Vertex { user: b, .. }) => {
                a == b
            }
            (
                FunctionOutput::RenderTarget { index: a, .. },
                FunctionOutput::RenderTarget { index: b, .. },
            ) => a == b,
            (a, b) => same_variant(a, b),
        })
    }

    /// `DefineMeshVertexOutput`: user data is the same by name and index.
    pub fn define_mesh_vertex_output(&mut self, output: MeshVertexOutput) -> u32 {
        define(&mut self.mesh_vertex_outputs, output, |a, b| match (a, b) {
            (MeshVertexOutput::Data(a), MeshVertexOutput::Data(b)) => same_mesh_data(a, b),
            (a, b) => same_variant(a, b),
        })
    }

    /// `DefineMeshPrimitiveOutput`: user data is the same by name and index.
    pub fn define_mesh_primitive_output(&mut self, output: MeshPrimitiveOutput) -> u32 {
        define(&mut self.mesh_primitive_outputs, output, |a, b| {
            match (a, b) {
                (MeshPrimitiveOutput::Data(a), MeshPrimitiveOutput::Data(b)) => {
                    same_mesh_data(a, b)
                }
                (a, b) => same_variant(a, b),
            }
        })
    }

    /// `UseEarlyFragmentTests`.
    pub const fn use_early_fragment_tests(&mut self) {
        self.early_fragment_tests = true;
    }

    /// `UseMaxWorkgroupSize`.
    pub const fn use_max_work_group_size(&mut self, size: u32) {
        self.max_work_group_size = size;
    }

    /// `UseMaxMeshWorkgroupSize`.
    pub const fn use_max_mesh_work_group_size(&mut self, size: u32) {
        self.max_mesh_work_group_size = size;
    }

    #[must_use]
    pub fn inputs(&self) -> &[FunctionInput] {
        &self.inputs
    }

    #[must_use]
    pub fn outputs(&self) -> &[FunctionOutput] {
        &self.outputs
    }

    #[must_use]
    pub fn mesh_vertex_outputs(&self) -> &[MeshVertexOutput] {
        &self.mesh_vertex_outputs
    }

    #[must_use]
    pub fn mesh_primitive_outputs(&self) -> &[MeshPrimitiveOutput] {
        &self.mesh_primitive_outputs
    }

    #[must_use]
    pub const fn early_fragment_tests(&self) -> bool {
        self.early_fragment_tests
    }

    /// The maximum work group size, 0 when unset.
    #[must_use]
    pub const fn max_work_group_size(&self) -> u32 {
        self.max_work_group_size
    }

    /// The maximum mesh work group size, 0 when unset.
    #[must_use]
    pub const fn max_mesh_work_group_size(&self) -> u32 {
        self.max_mesh_work_group_size
    }
}

/// `GetArgumentIndex`: where a D3D slot's first field sits in the argument table.
///
/// The C++'s unsigned arithmetic wraps, so a slot no shader declares wraps too.
#[must_use]
pub const fn argument_index(ty: BindingType, slot: u32) -> u32 {
    match ty {
        BindingType::ConstantBuffer => slot,
        BindingType::Sampler => slot.wrapping_add(32),
        BindingType::Srv => slot.wrapping_mul(3).wrapping_add(128),
        BindingType::Uav => slot.wrapping_mul(3).wrapping_add(512),
    }
}

/// `to_air_resource_type`: a view's texture kind.
///
/// `None` for a buffer (`NonApplicable`, where the C++ asserted); `depth` for a view read with a
/// comparison.
#[must_use]
pub const fn texture_kind(ty: &ResourceType, depth: bool) -> Option<TextureKind> {
    Some(match (ty, depth) {
        (ResourceType::TextureBuffer, _) => TextureKind::TextureBuffer,
        (ResourceType::Texture1D, _) => TextureKind::Texture1d,
        (ResourceType::Texture1DArray, _) => TextureKind::Texture1dArray,
        (ResourceType::Texture2D, false) => TextureKind::Texture2d,
        (ResourceType::Texture2D, true) => TextureKind::Depth2d,
        (ResourceType::Texture2DArray, false) => TextureKind::Texture2dArray,
        (ResourceType::Texture2DArray, true) => TextureKind::Depth2dArray,
        (ResourceType::Texture2DMultisampled, false) => TextureKind::Texture2dMs,
        (ResourceType::Texture2DMultisampled, true) => TextureKind::Depth2dMs,
        (ResourceType::Texture2DMultisampledArray, false) => TextureKind::Texture2dMsArray,
        (ResourceType::Texture2DMultisampledArray, true) => TextureKind::Depth2dMsArray,
        (ResourceType::Texture3D, _) => TextureKind::Texture3d,
        (ResourceType::TextureCube, false) => TextureKind::TextureCube,
        (ResourceType::TextureCube, true) => TextureKind::DepthCube,
        (ResourceType::TextureCubeArray, false) => TextureKind::TextureCubeArray,
        (ResourceType::TextureCubeArray, true) => TextureKind::DepthCubeArray,
        (ResourceType::NonApplicable, _) => return None,
    })
}

/// `to_air_scaler_type`: a view's element type.
///
/// `None` for doubles, where the C++ asserted and fell off its end.
#[must_use]
pub const fn msl_scalar(ty: &ScalerDataType) -> Option<MslScalar> {
    match ty {
        ScalerDataType::Float => Some(MslScalar::Float),
        ScalerDataType::Uint => Some(MslScalar::Uint),
        ScalerDataType::Int => Some(MslScalar::Int),
        ScalerDataType::Double => None,
    }
}

/// A count or an index of a table as the C++'s `uint32_t` holds it.
///
/// A shader's declarations come from a part shorter than 4 GiB, so no table of them reaches
/// `u32::MAX` entries; the conversion saturates rather than wraps.
#[must_use]
pub fn count_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The C++'s default location: the field's own index.
const fn location_or(location_index: u32, index: u32) -> u32 {
    if location_index == u32::MAX {
        index
    } else {
        location_index
    }
}

/// The index of the first element `same` as `element`, else `element` pushed and its index.
fn define<T>(list: &mut Vec<T>, element: T, same: impl Fn(&T, &T) -> bool) -> u32 {
    if let Some(index) = list.iter().position(|e| same(e, &element)) {
        return count_u32(index);
    }
    let index = count_u32(list.len());
    list.push(element);
    index
}

/// Whether two values are of one variant (`std::variant::index` equal).
fn same_variant<T>(a: &T, b: &T) -> bool {
    core::mem::discriminant(a) == core::mem::discriminant(b)
}

fn same_input(a: &FunctionInput, b: &FunctionInput) -> bool {
    match (a, b) {
        (
            FunctionInput::VertexStageIn { attribute: a, .. },
            FunctionInput::VertexStageIn { attribute: b, .. },
        ) => a == b,
        (
            FunctionInput::FragmentStageIn { user: a, .. },
            FunctionInput::FragmentStageIn { user: b, .. },
        ) => a == b,
        (FunctionInput::Buffer(a), FunctionInput::Buffer(b)) => {
            a.location_index == b.location_index
        }
        (FunctionInput::Sampler(a), FunctionInput::Sampler(b)) => {
            a.location_index == b.location_index
        }
        (FunctionInput::Texture(a), FunctionInput::Texture(b)) => {
            a.location_index == b.location_index
        }
        (FunctionInput::IndirectBuffer(a), FunctionInput::IndirectBuffer(b)) => {
            a.location_index == b.location_index
        }
        (a, b) => same_variant(a, b),
    }
}

fn same_mesh_data(a: &MeshData, b: &MeshData) -> bool {
    a.user == b.user && a.index == b.index
}

#[cfg(test)]
mod tests;
