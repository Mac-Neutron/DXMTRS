//! The shader translator's thunks: the `SM50*` calls with their parameter blocks.
//!
//! Each handler passes its block's fields, as they are, to the `SM50*` function of the same name
//! (airconv, the static library the link provides) and writes the answer where the C did: `ret`
//! for a call that answers a status, `ret_size` for an error's text, nothing for the rest. The
//! pointers in a block are the caller's, at their natural width, and go to airconv unread. The
//! 32-bit callers' converters stay C (`sm50_wow64.c`), which the `WoW64` table names.

use core::ffi::{c_char, c_void};

use winemetal_wire::{
    Sm50CompileGeometryPipelineGeometryParams, Sm50CompileGeometryPipelineVertexParams,
    Sm50CompileParams, Sm50CompileTessellationPipelineDomainParams,
    Sm50CompileTessellationPipelineHullParams, Sm50DestroyBitcodeParams, Sm50DestroyParams,
    Sm50FreeErrorParams, Sm50GetArgumentsInfoParams, Sm50GetCompiledBitcodeParams,
    Sm50GetErrorMessageParams, Sm50InitializeParams,
};

use crate::ffi::{AbortOnPanic, STATUS_SUCCESS, params};

// airconv's API (`airconv_public.h`). A shader, bitcode or error (`sm50_shader_t`, …) is a
// `void *` on a 64-bit target, carried as the wire's `u64`; `sysv_abi` is the C ABI on the Mac.
// airconv builds without exceptions: nothing unwinds out of it.
unsafe extern "C" {
    fn SM50Initialize(
        bytecode: *const c_void,
        bytecode_size: usize,
        shader: *mut u64,
        reflection: *mut c_void,
        error: *mut u64,
    ) -> i32;
    fn SM50Destroy(shader: u64);
    fn SM50Compile(
        shader: u64,
        args: *mut c_void,
        func_name: *const c_char,
        bitcode: *mut u64,
        error: *mut u64,
    ) -> i32;
    fn SM50GetCompiledBitcode(bitcode: u64, data_out: *mut c_void);
    fn SM50DestroyBitcode(bitcode: u64);
    fn SM50GetErrorMessage(error: u64, buffer: *mut c_char, buffer_size: usize) -> usize;
    fn SM50FreeError(error: u64);
    fn SM50CompileTessellationPipelineHull(
        vertex: u64,
        hull: u64,
        hull_args: *mut c_void,
        func_name: *const c_char,
        bitcode: *mut u64,
        error: *mut u64,
    ) -> i32;
    fn SM50CompileTessellationPipelineDomain(
        hull: u64,
        domain: u64,
        domain_args: *mut c_void,
        func_name: *const c_char,
        bitcode: *mut u64,
        error: *mut u64,
    ) -> i32;
    fn SM50CompileGeometryPipelineVertex(
        vertex: u64,
        geometry: u64,
        vertex_args: *mut c_void,
        func_name: *const c_char,
        bitcode: *mut u64,
        error: *mut u64,
    ) -> i32;
    fn SM50CompileGeometryPipelineGeometry(
        vertex: u64,
        geometry: u64,
        geometry_args: *mut c_void,
        func_name: *const c_char,
        bitcode: *mut u64,
        error: *mut u64,
    ) -> i32;
    fn SM50GetArgumentsInfo(shader: u64, constant_buffers: *mut c_void, arguments: *mut c_void);
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
    p.ret = unsafe { SM50Initialize(p.bytecode, p.bytecode_size, p.shader, p.reflection, p.error) };
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
    unsafe { SM50Destroy(p.shader) };
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
    p.ret = unsafe { SM50Compile(p.shader, p.args, p.func_name, p.bitcode, p.error) };
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
    unsafe { SM50GetCompiledBitcode(p.bitcode, p.data_out) };
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
    unsafe { SM50DestroyBitcode(p.bitcode) };
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
    p.ret_size = unsafe { SM50GetErrorMessage(p.error, p.buffer, p.buffer_size) };
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
    unsafe { SM50FreeError(p.error) };
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
        SM50CompileGeometryPipelineVertex(
            p.vertex,
            p.geometry,
            p.vertex_args,
            p.func_name,
            p.bitcode,
            p.error,
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
        SM50CompileGeometryPipelineGeometry(
            p.vertex,
            p.geometry,
            p.geometry_args,
            p.func_name,
            p.bitcode,
            p.error,
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
        SM50CompileTessellationPipelineHull(
            p.vertex,
            p.hull,
            p.hull_args,
            p.func_name,
            p.bitcode,
            p.error,
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
        SM50CompileTessellationPipelineDomain(
            p.hull,
            p.domain,
            p.domain_args,
            p.func_name,
            p.bitcode,
            p.error,
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
    unsafe { SM50GetArgumentsInfo(p.shader, p.constant_buffers, p.arguments) };
    STATUS_SUCCESS
}
