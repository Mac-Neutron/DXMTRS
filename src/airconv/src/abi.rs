//! `airconv_public.h`'s and `dxil/dxil_public.h`'s structures and constants, mirrored.
//!
//! `#[repr(C)]` mirrors with the C's storage types, since the caller may be another process (a
//! guest, through winemetal): an enum is its integer and never a Rust enum, a `bool` a `u8`, a
//! `sm50_ptr64_t` a pointer. An anonymous union is the field `u`, of type `<Structure>Union`.
//! `SM50_IA_INPUT_ELEMENT`'s bitfields are one `u32`, `bits`, read LSB first through
//! `<FIELD>_SHIFT`, `<FIELD>_WIDTH` and a getter per field. Names take Rust casing:
//! `SM50_SHADER_COMMON_DATA` is [`Sm50ShaderCommonData`], `ConstanttBufferTableBindIndex` is
//! `constantt_buffer_table_bind_index`; constants keep the C's names. `layouts.rs`, written by
//! `tools/winemetal-layouts.sh --airconv`, asserts every size, offset, bitfield and constant
//! against clang's reading of the headers.

use core::{
    ffi::{c_char, c_void},
    ptr,
};

mod layouts;

// SM50_BINDING_INDEX
pub const SM50_BINDING_INDEX_CONSTANT_BUFFER: u32 = 0;
pub const SM50_BINDING_INDEX_ARGUMENT_TABLE: u32 = 1;
pub const SM50_BINDING_INDEX_VERTEX_BUFFER: u32 = 2;
pub const SM50_BINDING_INDEX_INDEX_BUFFER: u32 = 3;
pub const SM50_BINDING_INDEX_DRAW_ARGUMENTS: u32 = 4;
pub const SM50_BINDING_INDEX_INDIRECT_ARGUMENTS: u32 = 4;
pub const SM50_BINDING_INDEX_STREAM_OUTPUT0: u32 = 5;
pub const SM50_BINDING_INDEX_CONSTANT_BUFFER2: u32 = 6;
pub const SM50_BINDING_INDEX_ARGUMENT_TABLE2: u32 = 7;
pub const SM50_BINDING_INDEX_ROOT_ARGUMENTS: u32 = 0;
pub const SM50_BINDING_INDEX_STATIC_SAMPLERS: u32 = 1;

// MTL_SM50_SHADER_ARGUMENT_FLAG
pub const MTL_SM50_SHADER_ARGUMENT_BUFFER: u32 = 1 << 0;
pub const MTL_SM50_SHADER_ARGUMENT_TEXTURE: u32 = 1 << 1;
pub const MTL_SM50_SHADER_ARGUMENT_ELEMENT_WIDTH: u32 = 1 << 2;
pub const MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER: u32 = 1 << 3;
pub const MTL_SM50_SHADER_ARGUMENT_TEXTURE_MINLOD_CLAMP: u32 = 1 << 4;
pub const MTL_SM50_SHADER_ARGUMENT_TBUFFER_OFFSET: u32 = 1 << 5;
pub const MTL_SM50_SHADER_ARGUMENT_TEXTURE_ARRAY: u32 = 1 << 6;
pub const MTL_SM50_SHADER_ARGUMENT_READ_ACCESS: u32 = 1 << 10;
pub const MTL_SM50_SHADER_ARGUMENT_WRITE_ACCESS: u32 = 1 << 11;

// MTL_TESSELLATOR_OUTPUT_PRIMITIVE
pub const MTL_TESSELLATOR_OUTPUT_POINT: u32 = 1;
pub const MTL_TESSELLATOR_OUTPUT_LINE: u32 = 2;
pub const MTL_TESSELLATOR_OUTPUT_TRIANGLE_CW: u32 = 3;
pub const MTL_TESSELLATOR_TRIANGLE_CCW: u32 = 4;

// SM50_SHADER_COMPILATION_ARGUMENT_TYPE
pub const SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT: u32 = 1;
pub const SM50_SHADER_COMMON: u32 = 2;
pub const SM50_SHADER_PSO_PIXEL_SHADER: u32 = 3;
pub const SM50_SHADER_IA_INPUT_LAYOUT: u32 = 4;
pub const SM50_SHADER_GS_PASS_THROUGH: u32 = 5;
pub const SM50_SHADER_PSO_GEOMETRY_SHADER: u32 = 6;
pub const SM50_SHADER_PSO_TESSELLATOR: u32 = 7;
pub const SM50_SHADER_ROOT_SIGNATURE: u32 = 8;
pub const SM50_SHADER_ROOT_SIGNATURE2: u32 = 9;
pub const SM50_SHADER_ARGUMENT_TYPE_MAX: u32 = 0xffff_ffff;

// SM50_SHADER_METAL_VERSION
pub const SM50_SHADER_METAL_310: u32 = 310;
pub const SM50_SHADER_METAL_320: u32 = 320;
pub const SM50_SHADER_METAL_MAX: u32 = 0xffff_ffff;

// SM50_SHADER_FLAG
pub const SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO: u32 = 1 << 0;
pub const SM50_SHADER_FLAG_DEFUSE_FMA: u32 = 1 << 1;

// SM50_INDEX_BUFFER_FORMAT
pub const SM50_INDEX_BUFFER_FORMAT_NONE: u32 = 0;
pub const SM50_INDEX_BUFFER_FORMAT_UINT16: u32 = 1;
pub const SM50_INDEX_BUFFER_FORMAT_UINT32: u32 = 2;

// DXIL_SHADER_KIND
pub const DXIL_SHADER_KIND_PIXEL: u32 = 0;
pub const DXIL_SHADER_KIND_VERTEX: u32 = 1;
pub const DXIL_SHADER_KIND_GEOMETRY: u32 = 2;
pub const DXIL_SHADER_KIND_HULL: u32 = 3;
pub const DXIL_SHADER_KIND_DOMAIN: u32 = 4;
pub const DXIL_SHADER_KIND_COMPUTE: u32 = 5;

// DXIL_SEMANTIC
pub const DXIL_SEMANTIC_ARBITRARY: u32 = 0;
pub const DXIL_SEMANTIC_POSITION: u32 = 3;
pub const DXIL_SEMANTIC_DEPTH: u32 = 17;
pub const DXIL_SEMANTIC_DEPTH_LESS_EQUAL: u32 = 18;
pub const DXIL_SEMANTIC_DEPTH_GREATER_EQUAL: u32 = 19;
pub const DXIL_SEMANTIC_STENCIL_REF: u32 = 20;

// DXIL_RESOURCE_CLASS
pub const DXIL_RESOURCE_CLASS_SRV: u32 = 0;
pub const DXIL_RESOURCE_CLASS_UAV: u32 = 1;
pub const DXIL_RESOURCE_CLASS_CBUFFER: u32 = 2;
pub const DXIL_RESOURCE_CLASS_SAMPLER: u32 = 3;

#[repr(C)]
pub struct MtlSm50ShaderArgument {
    /// An `SM50BindingType`: constant buffer 0, sampler 1, SRV 2, UAV 3.
    pub r#type: u32,
    pub sm50_binding_slot: u32,
    /// `MTL_SM50_SHADER_ARGUMENT_*` bits.
    pub flags: u32,
    pub structure_ptr_offset: u32,
}

/// A union member, so `Copy`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MtlTessellatorReflection {
    pub partition: u32,
    pub max_factor: f32,
    pub output_primitive: u32,
}

/// A union member, so `Copy`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MtlGeometryShaderPassThrough {
    pub render_target_array_index_reg: u8,
    pub render_target_array_index_component: u8,
    pub viewport_array_index_reg: u8,
    pub viewport_array_index_component: u8,
}

/// [`MtlGeometryShaderReflection`]'s union, itself a union member, so `Copy`.
#[derive(Clone, Copy)]
#[repr(C)]
pub union MtlGeometryShaderReflectionUnion {
    pub data: MtlGeometryShaderPassThrough,
    pub gs_pass_through: u32,
}

/// A union member, so `Copy`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MtlGeometryShaderReflection {
    pub u: MtlGeometryShaderReflectionUnion,
    pub primitive: u32,
}

/// A union member, so `Copy`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MtlPostTessellatorReflection {
    pub max_potential_tess_factor: u32,
}

/// A union member, so `Copy`.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct MtlPixelShaderReflection {
    pub valid_render_targets: u32,
    pub has_coverage_output: u32,
}

/// The stage's part of [`MtlShaderReflection`].
#[repr(C)]
pub union MtlShaderReflectionUnion {
    pub threadgroup_size: [u32; 3],
    pub tessellator: MtlTessellatorReflection,
    pub geometry_shader: MtlGeometryShaderReflection,
    pub post_tessellator: MtlPostTessellatorReflection,
    pub ps_valid_render_targets: u32,
    pub pixel_shader: MtlPixelShaderReflection,
}

/// A shader's reflection, which `SM50Initialize` writes into the caller's memory.
///
/// D3D11 hands it uninitialised: it is written field by field through raw pointers, never read.
#[repr(C)]
pub struct MtlShaderReflection {
    pub constantt_buffer_table_bind_index: u32,
    pub argument_buffer_bind_index: u32,
    pub num_constant_buffers: u32,
    pub num_arguments: u32,
    pub u: MtlShaderReflectionUnion,
    pub constant_buffer_slot_mask: u16,
    pub sampler_slot_mask: u16,
    pub uav_slot_mask: u64,
    pub srv_slot_mask_hi: u64,
    pub srv_slot_mask_lo: u64,
    pub num_output_element: u32,
    pub threads_per_patch: u32,
    pub argument_table_qwords: u32,
    pub writes_uav: u32,
}

#[repr(C)]
pub struct Sm50CompiledBitcode {
    pub data: *mut c_void,
    pub size: u64,
}

/// The header every node of an argument chain starts with.
#[repr(C)]
pub struct Sm50ShaderCompilationArgumentData {
    pub next: *mut c_void,
    /// An `SM50_SHADER_*` argument type.
    pub r#type: u32,
}

#[repr(C)]
pub struct Sm50StreamOutputElement {
    pub reg_id: u32,
    pub component: u32,
    pub output_slot: u32,
    pub offset: u32,
}

#[repr(C)]
pub struct Sm50ShaderEmulateVertexStreamOutputData {
    pub next: *mut c_void,
    pub r#type: u32,
    pub num_output_slots: u32,
    pub num_elements: u32,
    pub strides: [u32; 4],
    pub elements: *mut Sm50StreamOutputElement,
}

#[repr(C)]
pub struct Sm50ShaderCommonData {
    pub next: *mut c_void,
    pub r#type: u32,
    /// An `SM50_SHADER_METAL_*` version.
    pub metal_version: u32,
    /// `SM50_SHADER_FLAG_*` bits.
    pub flags: u32,
}

#[repr(C)]
pub struct Sm50ShaderPsoPixelShaderData {
    pub next: *mut c_void,
    pub r#type: u32,
    pub sample_mask: u32,
    pub dual_source_blending: u8,
    pub disable_depth_output: u8,
    pub unorm_output_reg_mask: u32,
    /// `MTLPixelFormat`s.
    pub pixel_formats: [u32; 8],
}

#[repr(C)]
pub struct Sm50IaInputElement {
    pub reg: u32,
    pub slot: u32,
    pub aligned_byte_offset: u32,
    /// An `MTLAttributeFormat`.
    pub format: u32,
    /// `step_function : 1` and `step_rate : 31`, LSB first.
    pub bits: u32,
}

#[repr(C)]
pub struct Sm50ShaderIaInputLayoutData {
    pub next: *mut c_void,
    pub r#type: u32,
    /// An `SM50_INDEX_BUFFER_FORMAT_*` value.
    pub index_buffer_format: u32,
    pub slot_mask: u32,
    pub num_elements: u32,
    pub elements: *mut Sm50IaInputElement,
}

/// The pass-through data of [`Sm50ShaderGsPassThroughData`], as fields or encoded.
#[repr(C)]
pub union Sm50ShaderGsPassThroughDataUnion {
    pub data: MtlGeometryShaderPassThrough,
    pub data_encoded: u32,
}

#[repr(C)]
pub struct Sm50ShaderGsPassThroughData {
    pub next: *mut c_void,
    pub r#type: u32,
    pub u: Sm50ShaderGsPassThroughDataUnion,
    pub rasterization_disabled: u8,
}

#[repr(C)]
pub struct Sm50ShaderPsoGeometryShaderData {
    pub next: *mut c_void,
    pub r#type: u32,
    pub strip_topology: u8,
}

#[repr(C)]
pub struct Sm50ShaderPsoTessellatorData {
    pub next: *mut c_void,
    pub r#type: u32,
    pub max_potential_tess_factor: u32,
}

/// A root signature's blob; the chain's `SM50_SHADER_ROOT_SIGNATURE` and `…2` nodes.
#[repr(C)]
pub struct Sm50ShaderRootSignatureData {
    pub next: *mut c_void,
    pub r#type: u32,
    pub bytecode: *const c_void,
    pub bytecode_length: usize,
}

#[repr(C)]
pub struct DxilEntryElement {
    /// A `DXIL_SEMANTIC_*` value, or another of DXC's `SemanticKind` numbers.
    pub kind: u32,
    pub rows: u32,
    /// -1: a system value that is not packed.
    pub start_row: i32,
    pub name: *const c_char,
    pub num_semantic_indices: u32,
    pub semantic_indices: *const u32,
}

#[repr(C)]
pub struct DxilEntryResource {
    /// A `DXIL_RESOURCE_CLASS_*` value.
    pub cls: u32,
    pub space: u32,
    pub lower_bound: u32,
    pub size: u32,
    pub heap: u32,
}

/// What `DXILGetEntry` answers; the pointers belong to the shader handle.
#[repr(C)]
pub struct DxilEntryInfo {
    pub is_dxil: u32,
    /// A `DXIL_SHADER_KIND_*` value.
    pub stage: u32,
    pub num_inputs: u32,
    pub num_outputs: u32,
    pub num_resources: u32,
    pub inputs: *const DxilEntryElement,
    pub outputs: *const DxilEntryElement,
    pub resources: *const DxilEntryResource,
}

/// An argument chain's nodes, the first of each type, as the C++'s `args_get_data` finds them.
///
/// A node of a type the header does not define is skipped.
pub struct ArgChain<'a> {
    pub stream_output: Option<&'a Sm50ShaderEmulateVertexStreamOutputData>,
    pub common: Option<&'a Sm50ShaderCommonData>,
    pub pixel: Option<&'a Sm50ShaderPsoPixelShaderData>,
    pub ia_layout: Option<&'a Sm50ShaderIaInputLayoutData>,
    pub gs_pass_through: Option<&'a Sm50ShaderGsPassThroughData>,
    pub geometry: Option<&'a Sm50ShaderPsoGeometryShaderData>,
    pub tessellator: Option<&'a Sm50ShaderPsoTessellatorData>,
    pub root_signature: Option<&'a Sm50ShaderRootSignatureData>,
    pub root_signature2: Option<&'a Sm50ShaderRootSignatureData>,
}

impl Sm50IaInputElement {
    pub const STEP_FUNCTION_SHIFT: u32 = 0;
    pub const STEP_FUNCTION_WIDTH: u32 = 1;
    pub const STEP_RATE_SHIFT: u32 = 1;
    pub const STEP_RATE_WIDTH: u32 = 31;

    /// 0 per vertex, 1 per instance.
    #[must_use]
    pub const fn step_function(&self) -> u32 {
        bitfield(
            self.bits,
            Self::STEP_FUNCTION_SHIFT,
            Self::STEP_FUNCTION_WIDTH,
        )
    }

    #[must_use]
    pub const fn step_rate(&self) -> u32 {
        bitfield(self.bits, Self::STEP_RATE_SHIFT, Self::STEP_RATE_WIDTH)
    }
}

impl ArgChain<'_> {
    /// The chain that starts at `node`, which may be null (no arguments).
    ///
    /// # Safety
    ///
    /// Every node of the chain is a live structure of the type its `type` field names (any for a
    /// type the header does not define), stays live and unwritten for `'a`, and the chain ends
    /// in a null `next`.
    #[must_use]
    pub const unsafe fn walk(mut node: *const Sm50ShaderCompilationArgumentData) -> Self {
        let mut chain = Self {
            stream_output: None,
            common: None,
            pixel: None,
            ia_layout: None,
            gs_pass_through: None,
            geometry: None,
            tessellator: None,
            root_signature: None,
            root_signature2: None,
        };
        while !node.is_null() {
            // SAFETY: a non-null node is a live header (the caller's contract).
            let header = unsafe { &*node };
            // Each arm's node is a live structure of the type its `type` names (the caller's).
            match header.r#type {
                SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.stream_output, node) };
                }
                SM50_SHADER_COMMON => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.common, node) };
                }
                SM50_SHADER_PSO_PIXEL_SHADER => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.pixel, node) };
                }
                SM50_SHADER_IA_INPUT_LAYOUT => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.ia_layout, node) };
                }
                SM50_SHADER_GS_PASS_THROUGH => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.gs_pass_through, node) };
                }
                SM50_SHADER_PSO_GEOMETRY_SHADER => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.geometry, node) };
                }
                SM50_SHADER_PSO_TESSELLATOR => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.tessellator, node) };
                }
                SM50_SHADER_ROOT_SIGNATURE => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.root_signature, node) };
                }
                SM50_SHADER_ROOT_SIGNATURE2 => {
                    // SAFETY: as above.
                    unsafe { keep_first(&mut chain.root_signature2, node) };
                }
                _ => {}
            }
            node = header.next.cast_const().cast();
        }
        chain
    }
}

impl DxilEntryInfo {
    /// No stage, no elements, no resources: a shader whose entry is not known.
    pub const EMPTY: Self = Self {
        is_dxil: 0,
        stage: 0,
        num_inputs: 0,
        num_outputs: 0,
        num_resources: 0,
        inputs: ptr::null(),
        outputs: ptr::null(),
        resources: ptr::null(),
    };
}

/// Keeps `node` in `slot` unless an earlier node of its type is there.
///
/// # Safety
///
/// `node` is a live `T` while `slot`'s reference is.
const unsafe fn keep_first<T>(
    slot: &mut Option<&T>,
    node: *const Sm50ShaderCompilationArgumentData,
) {
    if slot.is_none() {
        // SAFETY: `node` is a live `T` (the caller's contract).
        *slot = unsafe { node.cast::<T>().as_ref() };
    }
}

/// Bits `shift`..`shift + width` of `bits`.
const fn bitfield(bits: u32, shift: u32, width: u32) -> u32 {
    (bits >> shift) & (u32::MAX >> (32 - width))
}

#[cfg(test)]
mod tests;
