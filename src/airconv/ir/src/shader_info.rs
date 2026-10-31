//! What a DXBC shader declares: its registers, resources and their uses.
//!
//! [`ShaderInfo`] is the C++'s `ShaderInfo` (`A/dxbc_converter.hpp`) without its two binding
//! tables, the `dcl_*` maps as `BTreeMap`s; [`Declarations`] holds the scalars the control-flow
//! reader sets on `SM50ShaderInternal`. The DXIL front end fills the same `ShaderInfo`, so its
//! fields for DXIL are here too.

use std::collections::BTreeMap;

/// A resource range's binding: its first register, its size (`u32::MAX` unbounded) and space.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ResourceRange {
    pub range_id: u32,
    pub lower_bound: u32,
    pub size: u32,
    pub space: u32,
}

/// A resource's element type (`shader::common::ScalerDataType`).
#[derive(Debug, Default, PartialEq, Eq)]
pub enum ScalerDataType {
    #[default]
    Float,
    Uint,
    Int,
    Double,
}

/// A resource's kind (`shader::common::ResourceType`).
#[derive(Debug, Default, PartialEq, Eq)]
pub enum ResourceType {
    #[default]
    TextureBuffer,
    Texture1D,
    Texture1DArray,
    Texture2D,
    Texture2DArray,
    Texture2DMultisampled,
    Texture2DMultisampledArray,
    Texture3D,
    TextureCube,
    TextureCubeArray,
    NonApplicable,
}

/// A shader resource view.
///
/// `structure_stride` is `u32::MAX` for a typed view and 0 for a raw one. The argument indices
/// are set by the binding table after the walk (the C++ left them unset until then).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ShaderResourceViewInfo {
    pub range: ResourceRange,
    pub scaler_type: ScalerDataType,
    pub resource_type: ResourceType,
    pub read: bool,
    pub sampled: bool,
    /// Read with a comparison, so bound as a depth texture.
    pub compared: bool,
    pub structure_stride: u32,
    pub arg_index: u32,
    pub arg_metadata_index: u32,
}

/// How an unordered access view is used and declared, one bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UavFlags(u8);

/// An unordered access view.
///
/// `structure_stride` and the argument indices as [`ShaderResourceViewInfo`]'s.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UnorderedAccessViewInfo {
    pub range: ResourceRange,
    pub scaler_type: ScalerDataType,
    pub resource_type: ResourceType,
    pub flags: UavFlags,
    pub structure_stride: u32,
    pub arg_index: u32,
    pub arg_metadata_index: u32,
    pub arg_counter_index: u32,
}

/// A constant buffer, `size_in_vec4` in 16-byte vectors.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConstantBufferInfo {
    pub range: ResourceRange,
    pub size_in_vec4: u32,
    pub arg_index: u32,
}

/// A sampler.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SamplerInfo {
    pub range: ResourceRange,
    pub arg_index: u32,
    pub arg_cube_index: u32,
    pub arg_metadata_index: u32,
}

/// A thread group shared memory declaration: `size` bytes raw, or `size` elements of `stride`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ThreadgroupBufferInfo {
    pub size_in_uint: u32,
    pub size: u32,
    pub stride: u32,
    pub structured: bool,
}

/// A hull shader fork or join phase's own temporaries.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PhaseInfo {
    pub temp_register_count: u32,
    /// Each indexable temp's register count and component mask (x in bit 0).
    pub indexable_temp_register_counts: BTreeMap<u32, (u32, u32)>,
}

/// What a shader uses or declares, one bit each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShaderFlags(u8);

/// The shader's resources, registers and features, as the walk and the readers record them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ShaderInfo {
    pub imm_constant_buffer_data: Vec<[u32; 4]>,
    pub srv_map: BTreeMap<u32, ShaderResourceViewInfo>,
    pub uav_map: BTreeMap<u32, UnorderedAccessViewInfo>,
    pub cbuffer_map: BTreeMap<u32, ConstantBufferInfo>,
    pub sampler_map: BTreeMap<u32, SamplerInfo>,
    pub tgsm_map: BTreeMap<u32, ThreadgroupBufferInfo>,
    pub temp_register_count: u32,
    /// Each indexable temp's register count and component mask (x in bit 0).
    pub indexable_temp_register_counts: BTreeMap<u32, (u32, u32)>,
    pub flags: ShaderFlags,
    pub phases: Vec<PhaseInfo>,
    /// The inputs an `eval_*` reads, which the pixel shader pulls instead of receiving.
    pub pull_mode_reg_mask: u32,
}

/// What the declarations set on the shader beside [`ShaderInfo`] (`SM50ShaderInternal`'s fields).
///
/// The enumerations stay the format's numbers (`D3D11_SB_TESSELLATOR_*`, `D3D10_SB_PRIMITIVE*`).
#[derive(Debug, PartialEq)]
pub struct Declarations {
    pub threadgroup_size: [u32; 3],
    /// `FunctionSignatureBuilder::UseMaxWorkgroupSize`'s value, 0 unset.
    pub max_workgroup_size: u32,
    /// `FunctionSignatureBuilder::UseEarlyFragmentTests`.
    pub early_fragment_tests: bool,
    pub input_control_point_count: u32,
    pub output_control_point_count: u32,
    pub tessellation_partition: u32,
    pub max_tessellation_factor: f32,
    pub tessellator_output_primitive: u32,
    pub tessellation_domain: u32,
    pub hull_maximum_threads_per_patch: u32,
    pub gs_input_primitive: u32,
    pub gs_output_topology: u32,
    pub gs_max_vertex_output: u32,
    pub gs_instance_count: u32,
}

impl UavFlags {
    pub const READ: Self = Self(1);
    pub const WRITTEN: Self = Self(1 << 1);
    pub const GLOBAL_COHERENT: Self = Self(1 << 2);
    pub const RASTERIZER_ORDER: Self = Self(1 << 3);
    pub const WITH_COUNTER: Self = Self(1 << 4);
    /// DXIL: a float typed view that atomics reach, declared uint.
    pub const ATOMIC_VIEW: Self = Self(1 << 5);
    /// DXIL: a typed buffer that 64-bit atomics reach, read at its base address.
    pub const ADDRESS64: Self = Self(1 << 6);

    /// Whether every bit of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl ShaderFlags {
    pub const SKIP_OPTIMIZATION: Self = Self(1);
    pub const REFACTORING_ALLOWED: Self = Self(1 << 1);
    pub const USE_CMP_EXCH: Self = Self(1 << 2);
    pub const NO_CONTROL_POINT_PHASE_PASSTHROUGH: Self = Self(1 << 3);
    pub const OUTPUT_CONTROL_POINT_READ: Self = Self(1 << 4);
    pub const USE_MSAD: Self = Self(1 << 5);
    pub const USE_SAMPLEPOS: Self = Self(1 << 6);
    /// DXIL 64-bit atomics: the root arguments carry the device's lock table.
    pub const USE_ATOMIC64_LOCKS: Self = Self(1 << 7);

    /// Whether every bit of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }
}

impl Default for Declarations {
    /// `SM50ShaderInternal`'s initial values.
    fn default() -> Self {
        Self {
            threadgroup_size: [0; 3],
            max_workgroup_size: 0,
            early_fragment_tests: false,
            input_control_point_count: u32::MAX,
            output_control_point_count: u32::MAX,
            tessellation_partition: 0,
            max_tessellation_factor: 64.0,
            tessellator_output_primitive: 0,
            tessellation_domain: 0,
            hull_maximum_threads_per_patch: 0,
            gs_input_primitive: 0,
            gs_output_topology: 0,
            gs_max_vertex_output: 0,
            gs_instance_count: 1,
        }
    }
}
