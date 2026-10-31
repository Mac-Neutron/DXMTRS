//! The shader translator's thunks: the `SM50*` calls with their parameter blocks.
//!
//! Each handler passes its block's fields, as they are, to airconv's function of the same name
//! (the `airconv` crate, linked into this library) and writes the answer where the C did: `ret`
//! for a call that answers a status, `ret_size` for an error's text, nothing for the rest. The
//! pointers in a block are the caller's, at their natural width, and go to airconv unread. The
//! 32-bit callers' converters stay C (`sm50_wow64.c`), which the `WoW64` table names; they call
//! airconv's C exports, which this library carries.

use core::ffi::c_void;

use airconv::abi::{
    MtlShaderReflection, MtlSm50ShaderArgument, Sm50CompiledBitcode,
    Sm50ShaderCompilationArgumentData,
};
use winemetal_wire::{
    Sm50CompileGeometryPipelineGeometryParams, Sm50CompileGeometryPipelineVertexParams,
    Sm50CompileParams, Sm50CompileTessellationPipelineDomainParams,
    Sm50CompileTessellationPipelineHullParams, Sm50DestroyBitcodeParams, Sm50DestroyParams,
    Sm50FreeErrorParams, Sm50GetArgumentsInfoParams, Sm50GetCompiledBitcodeParams,
    Sm50GetErrorMessageParams, Sm50InitializeParams,
};

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

/// Slot 75: frees a shader.
///
/// # Safety
///
/// `args` points to the caller's `sm50_destroy_params`, whose shader is what `SM50Destroy` takes.
#[unsafe(export_name = "thunk_SM50Destroy")]
pub unsafe extern "C-unwind" fn sm50_destroy(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50DestroyParams>(args) };
    // SAFETY: the caller's shader, forwarded as the C forwarded it.
    unsafe { airconv::sm50_destroy(handle(p.shader)) };
    STATUS_SUCCESS
}

/// Slot 76: a shader's bitcode under its compilation arguments.
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

/// Slot 77: a bitcode's bytes and size, written where `data_out` points.
///
/// # Safety
///
/// `args` points to the caller's `sm50_get_compiled_bitcode_params`, whose fields are what
/// `SM50GetCompiledBitcode` takes.
#[unsafe(export_name = "thunk_SM50GetCompiledBitcode")]
pub unsafe extern "C-unwind" fn sm50_get_compiled_bitcode(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50GetCompiledBitcodeParams>(args) };
    // SAFETY: the caller's handle and pointer, forwarded as the C forwarded them.
    unsafe {
        airconv::sm50_get_compiled_bitcode(
            handle(p.bitcode),
            p.data_out.cast::<Sm50CompiledBitcode>(),
        );
    };
    STATUS_SUCCESS
}

/// Slot 78: frees a bitcode.
///
/// # Safety
///
/// `args` points to the caller's `sm50_destroy_bitcode_params`, whose bitcode is what
/// `SM50DestroyBitcode` takes.
#[unsafe(export_name = "thunk_SM50DestroyBitcode")]
pub unsafe extern "C-unwind" fn sm50_destroy_bitcode(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50DestroyBitcodeParams>(args) };
    // SAFETY: the caller's bitcode, forwarded as the C forwarded it.
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
    // SAFETY: the caller's error and buffer, forwarded as the C forwarded them.
    p.ret_size =
        unsafe { airconv::sm50_get_error_message(handle(p.error), p.buffer, p.buffer_size) };
    STATUS_SUCCESS
}

/// Slot 80: frees an error.
///
/// # Safety
///
/// `args` points to the caller's `sm50_free_error_params`, whose error is what `SM50FreeError`
/// takes.
#[unsafe(export_name = "thunk_SM50FreeError")]
pub unsafe extern "C-unwind" fn sm50_free_error(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<Sm50FreeErrorParams>(args) };
    // SAFETY: the caller's error, forwarded as the C forwarded it.
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
