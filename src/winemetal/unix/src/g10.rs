//! The shader translator's thunks: the `SM50*` calls with their parameter blocks.
//!
//! Each handler passes its block's fields, as they are, to airconv's function of the same name
//! (the `airconv` crate, linked into this library) and writes the answer where the C did: `ret`
//! for a call that answers a status, `ret_size` for an error's text, nothing for the rest. The
//! pointers in a block are the caller's, at their natural width, and go to airconv unread. In the
//! Wine build (`not(feature = "native")`), the `thunk32_SM50*` handlers widen a 32-bit caller's
//! parameter blocks and `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain before calling `airconv`.

use core::ffi::c_void;
#[cfg(not(feature = "native"))]
use core::{ffi::c_char, ptr};

use airconv::abi::{
    MtlShaderReflection, MtlSm50ShaderArgument, Sm50CompiledBitcode,
    Sm50ShaderCompilationArgumentData,
};
#[cfg(not(feature = "native"))]
use airconv::abi::{
    SM50_SHADER_COMMON, SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT, SM50_SHADER_GS_PASS_THROUGH,
    SM50_SHADER_IA_INPUT_LAYOUT, SM50_SHADER_PSO_GEOMETRY_SHADER, SM50_SHADER_PSO_PIXEL_SHADER,
    SM50_SHADER_PSO_TESSELLATOR, SM50_SHADER_ROOT_SIGNATURE, SM50_SHADER_ROOT_SIGNATURE2,
    Sm50ShaderCommonData, Sm50ShaderEmulateVertexStreamOutputData, Sm50ShaderGsPassThroughData,
    Sm50ShaderGsPassThroughDataUnion, Sm50ShaderIaInputLayoutData, Sm50ShaderPsoGeometryShaderData,
    Sm50ShaderPsoPixelShaderData, Sm50ShaderPsoTessellatorData, Sm50ShaderRootSignatureData,
};
use winemetal_wire::{
    Sm50CompileGeometryPipelineGeometryParams, Sm50CompileGeometryPipelineVertexParams,
    Sm50CompileParams, Sm50CompileTessellationPipelineDomainParams,
    Sm50CompileTessellationPipelineHullParams, Sm50DestroyBitcodeParams, Sm50DestroyParams,
    Sm50FreeErrorParams, Sm50GetArgumentsInfoParams, Sm50GetCompiledBitcodeParams,
    Sm50GetErrorMessageParams, Sm50InitializeParams,
};
#[cfg(not(feature = "native"))]
use winemetal_wire::{
    Sm50CompileGeometryPipelineGeometryParams32, Sm50CompileGeometryPipelineVertexParams32,
    Sm50CompileParams32, Sm50CompileTessellationPipelineDomainParams32,
    Sm50CompileTessellationPipelineHullParams32, Sm50GetArgumentsInfoParams32,
    Sm50GetCompiledBitcodeParams32, Sm50GetErrorMessageParams32, Sm50InitializeParams32,
};

#[cfg(not(feature = "native"))]
use crate::float_cast::{u64_to_u32, usize_to_u32};
use crate::{
    ffi::{AbortOnPanic, STATUS_SUCCESS, params},
    float_cast::u64_to_usize,
};

/// A shader, bitcode or error handle (`sm50_shader_t`, …) as airconv takes it.
///
/// A `void *` on a 64-bit target, which the wire carries as a `u64`.
const fn handle(v: u64) -> *mut c_void {
    core::ptr::with_exposed_provenance_mut(u64_to_usize(v))
}

/// Where airconv writes a handle: the caller's `u64`.
///
/// A `u64` has a pointer's size and alignment on the 64-bit targets this library builds for.
const fn slot(p: *mut u64) -> *mut *mut c_void {
    p.cast()
}

/// Slot 74: a shader from DXBC bytecode, with its reflection.
///
/// # Safety
///
/// `args` points to the caller's `sm50_initialize_params`, whose pointers are what
/// `SM50Initialize` takes.
#[unsafe(export_name = "thunk_SM50Initialize")]
pub unsafe extern "C-unwind" fn sm50_initialize(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50InitializeParams>(args) };
    // SAFETY: the caller's pointers, forwarded as the C forwarded them.
    p.ret = unsafe {
        airconv::sm50_initialize(
            p.bytecode,
            p.bytecode_size,
            slot(p.shader),
            p.reflection.cast::<MtlShaderReflection>(),
            slot(p.error),
        )
    };
    STATUS_SUCCESS
}

/// Slot 75: frees a shader `SM50Initialize` made.
///
/// # Safety
///
/// `args` points to a valid `sm50_destroy_params` whose `shader` is a live shader handle or zero.
#[unsafe(export_name = "thunk_SM50Destroy")]
pub unsafe extern "C-unwind" fn sm50_destroy(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50DestroyParams>(args) };
    // SAFETY: the caller's handle.
    unsafe { airconv::sm50_destroy(handle(p.shader)) };
    STATUS_SUCCESS
}

/// Slot 76: compiles a shader to Metal bitcode.
///
/// # Safety
///
/// `args` points to the caller's `sm50_compile_params`, whose fields are what `SM50Compile` takes.
#[unsafe(export_name = "thunk_SM50Compile")]
pub unsafe extern "C-unwind" fn sm50_compile(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileParams>(args) };
    // SAFETY: the caller's handle and pointers, forwarded as the C forwarded them.
    p.ret = unsafe {
        airconv::sm50_compile(
            handle(p.shader),
            p.args.cast::<Sm50ShaderCompilationArgumentData>(),
            p.func_name,
            slot(p.bitcode),
            slot(p.error),
        )
    };
    STATUS_SUCCESS
}

/// Slot 77: a compiled bitcode's bytes and length, written into `data_out`.
///
/// # Safety
///
/// `args` points to the caller's `sm50_get_compiled_bitcode_params`, with a live `bitcode` and a
/// valid `SM50_COMPILED_BITCODE` at `data_out`.
#[unsafe(export_name = "thunk_SM50GetCompiledBitcode")]
pub unsafe extern "C-unwind" fn sm50_get_compiled_bitcode(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50GetCompiledBitcodeParams>(args) };
    // SAFETY: the caller's handle and output pointer.
    unsafe {
        airconv::sm50_get_compiled_bitcode(
            handle(p.bitcode),
            p.data_out.cast::<Sm50CompiledBitcode>(),
        );
    };
    STATUS_SUCCESS
}

/// Slot 78: frees a bitcode `SM50Compile*` made.
///
/// # Safety
///
/// `args` points to a valid `sm50_destroy_bitcode_params` whose `bitcode` is a live bitcode handle
/// or zero.
#[unsafe(export_name = "thunk_SM50DestroyBitcode")]
pub unsafe extern "C-unwind" fn sm50_destroy_bitcode(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50DestroyBitcodeParams>(args) };
    // SAFETY: the caller's handle.
    unsafe { airconv::sm50_destroy_bitcode(handle(p.bitcode)) };
    STATUS_SUCCESS
}

/// Slot 79: an error's text, into the caller's buffer; `ret_size` is airconv's answer.
///
/// # Safety
///
/// `args` points to the caller's `sm50_get_error_message_params`, whose fields are what
/// `SM50GetErrorMessage` takes.
#[unsafe(export_name = "thunk_SM50GetErrorMessage")]
pub unsafe extern "C-unwind" fn sm50_get_error_message(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50GetErrorMessageParams>(args) };
    // SAFETY: the caller's handle and buffer.
    p.ret_size =
        unsafe { airconv::sm50_get_error_message(handle(p.error), p.buffer, p.buffer_size) };
    STATUS_SUCCESS
}

/// Slot 80: frees an error `SM50Initialize` or `SM50Compile*` made.
///
/// # Safety
///
/// `args` points to a valid `sm50_free_error_params` whose `error` is a live error handle or zero.
#[unsafe(export_name = "thunk_SM50FreeError")]
pub unsafe extern "C-unwind" fn sm50_free_error(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50FreeErrorParams>(args) };
    // SAFETY: the caller's handle.
    unsafe { airconv::sm50_free_error(handle(p.error)) };
    STATUS_SUCCESS
}

/// Slot 81: the vertex stage of a geometry pipeline.
///
/// # Safety
///
/// `args` points to the caller's `sm50_compile_geometry_pipeline_vertex_params`, whose fields are
/// what `SM50CompileGeometryPipelineVertex` takes.
#[unsafe(export_name = "thunk_SM50CompileGeometryPipelineVertex")]
pub unsafe extern "C-unwind" fn sm50_compile_geometry_pipeline_vertex(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileGeometryPipelineVertexParams>(args) };
    // SAFETY: the caller's handles and pointers, forwarded as the C forwarded them.
    p.ret = unsafe {
        airconv::sm50_compile_geometry_pipeline_vertex(
            handle(p.vertex),
            handle(p.geometry),
            p.vertex_args.cast::<Sm50ShaderCompilationArgumentData>(),
            p.func_name,
            slot(p.bitcode),
            slot(p.error),
        )
    };
    STATUS_SUCCESS
}

/// Slot 82: the geometry stage of a geometry pipeline.
///
/// # Safety
///
/// `args` points to the caller's `sm50_compile_geometry_pipeline_geometry_params`, whose fields
/// are what `SM50CompileGeometryPipelineGeometry` takes.
#[unsafe(export_name = "thunk_SM50CompileGeometryPipelineGeometry")]
pub unsafe extern "C-unwind" fn sm50_compile_geometry_pipeline_geometry(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileGeometryPipelineGeometryParams>(args) };
    // SAFETY: the caller's handles and pointers, forwarded as the C forwarded them.
    p.ret = unsafe {
        airconv::sm50_compile_geometry_pipeline_geometry(
            handle(p.vertex),
            handle(p.geometry),
            p.geometry_args.cast::<Sm50ShaderCompilationArgumentData>(),
            p.func_name,
            slot(p.bitcode),
            slot(p.error),
        )
    };
    STATUS_SUCCESS
}

/// Slot 84: the hull stage of a tessellation pipeline.
///
/// # Safety
///
/// `args` points to the caller's `sm50_compile_tessellation_pipeline_hull_params`, whose fields
/// are what `SM50CompileTessellationPipelineHull` takes.
#[unsafe(export_name = "thunk_SM50CompileTessellationPipelineHull")]
pub unsafe extern "C-unwind" fn sm50_compile_tessellation_pipeline_hull(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileTessellationPipelineHullParams>(args) };
    // SAFETY: the caller's handles and pointers, forwarded as the C forwarded them.
    p.ret = unsafe {
        airconv::sm50_compile_tessellation_pipeline_hull(
            handle(p.vertex),
            handle(p.hull),
            p.hull_args.cast::<Sm50ShaderCompilationArgumentData>(),
            p.func_name,
            slot(p.bitcode),
            slot(p.error),
        )
    };
    STATUS_SUCCESS
}

/// Slot 85: the domain stage of a tessellation pipeline.
///
/// # Safety
///
/// `args` points to the caller's `sm50_compile_tessellation_pipeline_domain_params`, whose fields
/// are what `SM50CompileTessellationPipelineDomain` takes.
#[unsafe(export_name = "thunk_SM50CompileTessellationPipelineDomain")]
pub unsafe extern "C-unwind" fn sm50_compile_tessellation_pipeline_domain(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileTessellationPipelineDomainParams>(args) };
    // SAFETY: the caller's handles and pointers, forwarded as the C forwarded them.
    p.ret = unsafe {
        airconv::sm50_compile_tessellation_pipeline_domain(
            handle(p.hull),
            handle(p.domain),
            p.domain_args.cast::<Sm50ShaderCompilationArgumentData>(),
            p.func_name,
            slot(p.bitcode),
            slot(p.error),
        )
    };
    STATUS_SUCCESS
}

/// Slot 88: a shader's constant-buffer and argument tables, into the caller's two arrays.
///
/// # Safety
///
/// `args` points to the caller's `sm50_get_arguments_info_params`, whose fields are what
/// `SM50GetArgumentsInfo` takes.
#[unsafe(export_name = "thunk_SM50GetArgumentsInfo")]
pub unsafe extern "C-unwind" fn sm50_get_arguments_info(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50GetArgumentsInfoParams>(args) };
    // SAFETY: the caller's shader and arrays, forwarded as the C forwarded them.
    unsafe {
        airconv::sm50_get_arguments_info(
            handle(p.shader),
            p.constant_buffers.cast::<MtlSm50ShaderArgument>(),
            p.arguments.cast::<MtlSm50ShaderArgument>(),
        );
    };
    STATUS_SUCCESS
}

#[cfg(not(feature = "native"))]
const fn ptr32<T>(addr: u32) -> *mut T {
    ptr::with_exposed_provenance_mut(addr as usize)
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct MtlShaderBitcode32 {
    data: u32,
    size: u32,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderCompilationArgumentData32 {
    next: u32,
    r#type: u32,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderCommonData32 {
    next: u32,
    r#type: u32,
    metal_version: u32,
    flags: u32,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderIaInputLayoutData32 {
    next: u32,
    r#type: u32,
    index_buffer_format: u32,
    slot_mask: u32,
    num_elements: u32,
    elements: u32,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderPsoPixelShaderData32 {
    next: u32,
    r#type: u32,
    sample_mask: u32,
    dual_source_blending: u8,
    disable_depth_output: u8,
    unorm_output_reg_mask: u32,
    pixel_formats: [u32; 8],
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderEmulateVertexStreamOutputData32 {
    next: u32,
    r#type: u32,
    num_output_slots: u32,
    num_elements: u32,
    strides: [u32; 4],
    elements: u32,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderGsPassThroughData32 {
    next: u32,
    r#type: u32,
    data_encoded: u32,
    rasterization_disabled: u8,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderPsoGeometryShaderData32 {
    next: u32,
    r#type: u32,
    strip_topology: u8,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderPsoTessellatorData32 {
    next: u32,
    r#type: u32,
    max_potential_tess_factor: u32,
}

#[cfg(not(feature = "native"))]
#[repr(C)]
struct Sm50ShaderRootSignatureData32 {
    next: u32,
    r#type: u32,
    bytecode: u32,
    bytecode_length: u32,
}

#[cfg(not(feature = "native"))]
const _: () = {
    assert!(size_of::<MtlShaderBitcode32>() == 8);
    assert!(size_of::<Sm50ShaderCompilationArgumentData32>() == 8);
    assert!(size_of::<Sm50ShaderCommonData32>() == 16);
    assert!(size_of::<Sm50ShaderIaInputLayoutData32>() == 24);
    assert!(size_of::<Sm50ShaderPsoPixelShaderData32>() == 52);
    assert!(size_of::<Sm50ShaderEmulateVertexStreamOutputData32>() == 36);
    assert!(size_of::<Sm50ShaderGsPassThroughData32>() == 16);
    assert!(size_of::<Sm50ShaderPsoGeometryShaderData32>() == 12);
    assert!(size_of::<Sm50ShaderPsoTessellatorData32>() == 12);
    assert!(size_of::<Sm50ShaderRootSignatureData32>() == 16);
};

#[cfg(not(feature = "native"))]
enum ChainNode {
    Header(Box<Sm50ShaderCompilationArgumentData>),
    Common(Box<Sm50ShaderCommonData>),
    IaInputLayout(Box<Sm50ShaderIaInputLayoutData>),
    Pixel(Box<Sm50ShaderPsoPixelShaderData>),
    StreamOutput(Box<Sm50ShaderEmulateVertexStreamOutputData>),
    GsPassThrough(Box<Sm50ShaderGsPassThroughData>),
    Geometry(Box<Sm50ShaderPsoGeometryShaderData>),
    Tessellator(Box<Sm50ShaderPsoTessellatorData>),
    RootSignature(Box<Sm50ShaderRootSignatureData>),
}

#[cfg(not(feature = "native"))]
impl ChainNode {
    fn as_header_mut(&mut self) -> *mut Sm50ShaderCompilationArgumentData {
        match self {
            Self::Header(b) => ptr::from_mut::<Sm50ShaderCompilationArgumentData>(&mut **b),
            Self::Common(b) => ptr::from_mut::<Sm50ShaderCommonData>(&mut **b).cast(),
            Self::IaInputLayout(b) => ptr::from_mut::<Sm50ShaderIaInputLayoutData>(&mut **b).cast(),
            Self::Pixel(b) => ptr::from_mut::<Sm50ShaderPsoPixelShaderData>(&mut **b).cast(),
            Self::StreamOutput(b) => {
                ptr::from_mut::<Sm50ShaderEmulateVertexStreamOutputData>(&mut **b).cast()
            }
            Self::GsPassThrough(b) => ptr::from_mut::<Sm50ShaderGsPassThroughData>(&mut **b).cast(),
            Self::Geometry(b) => ptr::from_mut::<Sm50ShaderPsoGeometryShaderData>(&mut **b).cast(),
            Self::Tessellator(b) => ptr::from_mut::<Sm50ShaderPsoTessellatorData>(&mut **b).cast(),
            Self::RootSignature(b) => ptr::from_mut::<Sm50ShaderRootSignatureData>(&mut **b).cast(),
        }
    }
}

/// Widens a 32-bit `SM50_SHADER_COMPILATION_ARGUMENT_DATA` linked list into owned 64-bit nodes.
///
/// # Safety
///
/// `head32` must be zero or the 32-bit address of a null-terminated chain of valid 32-bit argument
/// structures matching their `type` tag.
#[cfg(not(feature = "native"))]
unsafe fn walk_chain(mut cur32: u32) -> (*mut Sm50ShaderCompilationArgumentData, Vec<ChainNode>) {
    let mut nodes: Vec<ChainNode> = Vec::new();
    while cur32 != 0 {
        let hdr_ptr = ptr32::<Sm50ShaderCompilationArgumentData32>(cur32).cast_const();
        // SAFETY: `cur32` is non-zero and points to a valid 32-bit argument header.
        let hdr = unsafe { &*hdr_ptr };
        let next32 = hdr.next;
        let node = match hdr.r#type {
            SM50_SHADER_COMMON => {
                let d_ptr = ptr32::<Sm50ShaderCommonData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid `SM50_SHADER_COMMON_DATA_32`.
                let d = unsafe { &*d_ptr };
                ChainNode::Common(Box::new(Sm50ShaderCommonData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    metal_version: d.metal_version,
                    flags: d.flags,
                }))
            }
            SM50_SHADER_IA_INPUT_LAYOUT => {
                let d_ptr = ptr32::<Sm50ShaderIaInputLayoutData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid `SM50_SHADER_IA_INPUT_LAYOUT_DATA_32`.
                let d = unsafe { &*d_ptr };
                ChainNode::IaInputLayout(Box::new(Sm50ShaderIaInputLayoutData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    index_buffer_format: d.index_buffer_format,
                    slot_mask: d.slot_mask,
                    num_elements: d.num_elements,
                    elements: ptr32(d.elements),
                }))
            }
            SM50_SHADER_PSO_PIXEL_SHADER => {
                let d_ptr = ptr32::<Sm50ShaderPsoPixelShaderData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid `SM50_SHADER_PSO_PIXEL_SHADER_DATA_32`.
                let d = unsafe { &*d_ptr };
                ChainNode::Pixel(Box::new(Sm50ShaderPsoPixelShaderData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    sample_mask: d.sample_mask,
                    dual_source_blending: d.dual_source_blending,
                    disable_depth_output: d.disable_depth_output,
                    unorm_output_reg_mask: d.unorm_output_reg_mask,
                    pixel_formats: d.pixel_formats,
                }))
            }
            SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT => {
                let d_ptr = ptr32::<Sm50ShaderEmulateVertexStreamOutputData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid stream-output 32-bit node.
                let d = unsafe { &*d_ptr };
                ChainNode::StreamOutput(Box::new(Sm50ShaderEmulateVertexStreamOutputData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    num_output_slots: d.num_output_slots,
                    num_elements: d.num_elements,
                    strides: d.strides,
                    elements: ptr32(d.elements),
                }))
            }
            SM50_SHADER_GS_PASS_THROUGH => {
                let d_ptr = ptr32::<Sm50ShaderGsPassThroughData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid `SM50_SHADER_GS_PASS_THROUGH_DATA_32`.
                let d = unsafe { &*d_ptr };
                ChainNode::GsPassThrough(Box::new(Sm50ShaderGsPassThroughData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    u: Sm50ShaderGsPassThroughDataUnion {
                        data_encoded: d.data_encoded,
                    },
                    rasterization_disabled: d.rasterization_disabled,
                }))
            }
            SM50_SHADER_PSO_GEOMETRY_SHADER => {
                let d_ptr = ptr32::<Sm50ShaderPsoGeometryShaderData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid `SM50_SHADER_PSO_GEOMETRY_SHADER_DATA_32`.
                let d = unsafe { &*d_ptr };
                ChainNode::Geometry(Box::new(Sm50ShaderPsoGeometryShaderData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    strip_topology: d.strip_topology,
                }))
            }
            SM50_SHADER_PSO_TESSELLATOR => {
                let d_ptr = ptr32::<Sm50ShaderPsoTessellatorData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid `SM50_SHADER_PSO_TESSELLATOR_DATA_32`.
                let d = unsafe { &*d_ptr };
                ChainNode::Tessellator(Box::new(Sm50ShaderPsoTessellatorData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    max_potential_tess_factor: d.max_potential_tess_factor,
                }))
            }
            SM50_SHADER_ROOT_SIGNATURE | SM50_SHADER_ROOT_SIGNATURE2 => {
                let d_ptr = ptr32::<Sm50ShaderRootSignatureData32>(cur32).cast_const();
                // SAFETY: `hdr.type` identifies a valid `SM50_SHADER_ROOT_SIGNATURE_DATA_32`.
                let d = unsafe { &*d_ptr };
                ChainNode::RootSignature(Box::new(Sm50ShaderRootSignatureData {
                    next: ptr::null_mut(),
                    r#type: d.r#type,
                    bytecode: ptr32::<c_void>(d.bytecode).cast_const(),
                    bytecode_length: d.bytecode_length as usize,
                }))
            }
            _ => ChainNode::Header(Box::new(Sm50ShaderCompilationArgumentData {
                next: ptr::null_mut(),
                r#type: hdr.r#type,
            })),
        };
        nodes.push(node);
        cur32 = next32;
    }
    let mut next_ptr: *mut Sm50ShaderCompilationArgumentData = ptr::null_mut();
    for node in nodes.iter_mut().rev() {
        let cur_ptr = node.as_header_mut();
        // SAFETY: `cur_ptr` points to the boxed node's leading `Sm50ShaderCompilationArgumentData`.
        unsafe { (*cur_ptr).next = next_ptr.cast() };
        next_ptr = cur_ptr;
    }
    (next_ptr, nodes)
}

/// Slot 74 (`WoW64`): widens 32-bit `sm50_initialize_params32` and calls `SM50Initialize`.
///
/// # Safety
///
/// `args` points to a valid `sm50_initialize_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50Initialize")]
pub unsafe extern "C-unwind" fn sm50_initialize_32(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50InitializeParams32>(args) };
    // SAFETY: widened 32-bit pointers forwarded to `airconv::sm50_initialize`.
    p.ret = unsafe {
        airconv::sm50_initialize(
            ptr32::<c_void>(p.bytecode).cast_const(),
            p.bytecode_size as usize,
            slot(ptr32::<u64>(p.shader)),
            ptr32::<MtlShaderReflection>(p.reflection),
            slot(ptr32::<u64>(p.error)),
        )
    };
    STATUS_SUCCESS
}

/// Slot 76 (`WoW64`): widens 32-bit `sm50_compile_params32` and calls `SM50Compile`.
///
/// # Safety
///
/// `args` points to a valid `sm50_compile_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50Compile")]
pub unsafe extern "C-unwind" fn sm50_compile_32(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileParams32>(args) };
    // SAFETY: `p.args` is a 32-bit argument chain from the caller.
    let (chain, _nodes) = unsafe { walk_chain(p.args) };
    // SAFETY: widened 32-bit pointers and chain forwarded to `airconv::sm50_compile`.
    p.ret = unsafe {
        airconv::sm50_compile(
            handle(p.shader),
            chain,
            ptr32::<c_char>(p.func_name).cast_const(),
            slot(ptr32::<u64>(p.bitcode)),
            slot(ptr32::<u64>(p.error)),
        )
    };
    STATUS_SUCCESS
}

/// Slot 77 (`WoW64`): widens 32-bit `sm50_get_compiled_bitcode_params32`.
///
/// # Safety
///
/// `args` points to a valid `sm50_get_compiled_bitcode_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50GetCompiledBitcode")]
pub unsafe extern "C-unwind" fn sm50_get_compiled_bitcode_32(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50GetCompiledBitcodeParams32>(args) };
    let mut data64 = Sm50CompiledBitcode {
        data: ptr::null_mut(),
        size: 0,
    };
    // SAFETY: `p.bitcode` is a live bitcode handle and `data64` is valid output storage.
    unsafe { airconv::sm50_get_compiled_bitcode(handle(p.bitcode), &raw mut data64) };
    let out32 = ptr32::<MtlShaderBitcode32>(p.data_out);
    // SAFETY: `out32` is the caller's 32-bit `MTL_SHADER_BITCODE32` output pointer.
    let out_ref = unsafe { &mut *out32 };
    out_ref.data = usize_to_u32(data64.data.expose_provenance());
    out_ref.size = u64_to_u32(data64.size);
    STATUS_SUCCESS
}

/// Slot 79 (`WoW64`): widens 32-bit `sm50_get_error_message_params32`.
///
/// # Safety
///
/// `args` points to a valid `sm50_get_error_message_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50GetErrorMessage")]
pub unsafe extern "C-unwind" fn sm50_get_error_message_32(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50GetErrorMessageParams32>(args) };
    // SAFETY: widened 32-bit buffer pointer forwarded to `airconv::sm50_get_error_message`.
    let n = unsafe {
        airconv::sm50_get_error_message(
            handle(p.error),
            ptr32::<c_char>(p.buffer),
            p.buffer_size as usize,
        )
    };
    p.ret_size = usize_to_u32(n);
    STATUS_SUCCESS
}

/// Slot 81 (`WoW64`): widens 32-bit `sm50_compile_geometry_pipeline_vertex_params32`.
///
/// # Safety
///
/// `args` points to a valid `sm50_compile_geometry_pipeline_vertex_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50CompileGeometryPipelineVertex")]
pub unsafe extern "C-unwind" fn sm50_compile_geometry_pipeline_vertex_32(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileGeometryPipelineVertexParams32>(args) };
    // SAFETY: `p.vertex_args` is a 32-bit argument chain from the caller.
    let (chain, _nodes) = unsafe { walk_chain(p.vertex_args) };
    // SAFETY: widened 32-bit pointers and chain forwarded to `airconv`.
    p.ret = unsafe {
        airconv::sm50_compile_geometry_pipeline_vertex(
            handle(p.vertex),
            handle(p.geometry),
            chain,
            ptr32::<c_char>(p.func_name).cast_const(),
            slot(ptr32::<u64>(p.bitcode)),
            slot(ptr32::<u64>(p.error)),
        )
    };
    STATUS_SUCCESS
}

/// Slot 82 (`WoW64`): widens 32-bit `sm50_compile_geometry_pipeline_geometry_params32`.
///
/// # Safety
///
/// `args` points to a valid `sm50_compile_geometry_pipeline_geometry_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50CompileGeometryPipelineGeometry")]
pub unsafe extern "C-unwind" fn sm50_compile_geometry_pipeline_geometry_32(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileGeometryPipelineGeometryParams32>(args) };
    // SAFETY: `p.geometry_args` is a 32-bit argument chain from the caller.
    let (chain, _nodes) = unsafe { walk_chain(p.geometry_args) };
    // SAFETY: widened 32-bit pointers and chain forwarded to `airconv`.
    p.ret = unsafe {
        airconv::sm50_compile_geometry_pipeline_geometry(
            handle(p.vertex),
            handle(p.geometry),
            chain,
            ptr32::<c_char>(p.func_name).cast_const(),
            slot(ptr32::<u64>(p.bitcode)),
            slot(ptr32::<u64>(p.error)),
        )
    };
    STATUS_SUCCESS
}

/// Slot 84 (`WoW64`): widens 32-bit `sm50_compile_tessellation_pipeline_hull_params32`.
///
/// # Safety
///
/// `args` points to a valid `sm50_compile_tessellation_pipeline_hull_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50CompileTessellationPipelineHull")]
pub unsafe extern "C-unwind" fn sm50_compile_tessellation_pipeline_hull_32(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileTessellationPipelineHullParams32>(args) };
    // SAFETY: `p.hull_args` is a 32-bit argument chain from the caller.
    let (chain, _nodes) = unsafe { walk_chain(p.hull_args) };
    // SAFETY: widened 32-bit pointers and chain forwarded to `airconv`.
    p.ret = unsafe {
        airconv::sm50_compile_tessellation_pipeline_hull(
            handle(p.vertex),
            handle(p.hull),
            chain,
            ptr32::<c_char>(p.func_name).cast_const(),
            slot(ptr32::<u64>(p.bitcode)),
            slot(ptr32::<u64>(p.error)),
        )
    };
    STATUS_SUCCESS
}

/// Slot 85 (`WoW64`): widens 32-bit `sm50_compile_tessellation_pipeline_domain_params32`.
///
/// # Safety
///
/// `args` points to a valid `sm50_compile_tessellation_pipeline_domain_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50CompileTessellationPipelineDomain")]
pub unsafe extern "C-unwind" fn sm50_compile_tessellation_pipeline_domain_32(
    args: *mut c_void,
) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50CompileTessellationPipelineDomainParams32>(args) };
    // SAFETY: `p.domain_args` is a 32-bit argument chain from the caller.
    let (chain, _nodes) = unsafe { walk_chain(p.domain_args) };
    // SAFETY: widened 32-bit pointers and chain forwarded to `airconv`.
    p.ret = unsafe {
        airconv::sm50_compile_tessellation_pipeline_domain(
            handle(p.hull),
            handle(p.domain),
            chain,
            ptr32::<c_char>(p.func_name).cast_const(),
            slot(ptr32::<u64>(p.bitcode)),
            slot(ptr32::<u64>(p.error)),
        )
    };
    STATUS_SUCCESS
}

/// Slot 88 (`WoW64`): widens 32-bit `sm50_get_arguments_info_params32`.
///
/// # Safety
///
/// `args` points to a valid `sm50_get_arguments_info_params32`.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "thunk32_SM50GetArgumentsInfo")]
pub unsafe extern "C-unwind" fn sm50_get_arguments_info_32(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50GetArgumentsInfoParams32>(args) };
    // SAFETY: widened 32-bit array pointers forwarded to `airconv::sm50_get_arguments_info`.
    unsafe {
        airconv::sm50_get_arguments_info(
            handle(p.shader),
            ptr32::<MtlSm50ShaderArgument>(p.constant_buffers),
            ptr32::<MtlSm50ShaderArgument>(p.arguments),
        );
    };
    STATUS_SUCCESS
}
