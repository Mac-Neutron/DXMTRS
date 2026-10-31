//! The exported functions, under the C names of `airconv_public.h` and `dxil/dxil_public.h`.
//!
//! The handles are pointer-sized, as the header's `sm50_ptr64_t` is on the Mac: a shader is a
//! `Box<Shader>`, a compiled metallib a `Box<Vec<u8>>`, an error a `Box<Error>`, each freed by its
//! destroy function. Every function that answers a status keeps the C++'s contract: `*ppError` is
//! set to null on entry (when `ppError` is not null), 0 answers success and fills the out handle,
//! 1 answers an error object in `*ppError` (dropped when `ppError` is null). The work is the entry
//! point's ([`crate::entry`]); here are only the pointers. The functions are `extern "C"`, never
//! `C-unwind`: a panic aborts here instead of unwinding into the caller.

use core::{
    ffi::{CStr, c_char, c_int, c_void},
    ptr, slice,
};

use crate::{
    abi::{
        ArgChain, DxilEntryInfo, MtlShaderReflection, MtlSm50ShaderArgument, Sm50CompiledBitcode,
        Sm50ShaderCompilationArgumentData,
    },
    entry,
    shader::Shader,
};

/// An error the translator answers: its text, as bytes.
///
/// The text is bytes, never a `String`: a guest's names, LLVM's messages and the verifier's text
/// need not be UTF-8, and `SM50GetErrorMessage` answers their exact length.
pub struct Error {
    text: Vec<u8>,
}

impl Error {
    /// An error with the text `text`.
    #[must_use]
    pub fn new(text: &[u8]) -> Self {
        Self {
            text: text.to_vec(),
        }
    }

    /// The answer of a function whose work has not been ported yet.
    #[must_use]
    pub fn not_ported() -> Self {
        Self::new(b"not ported")
    }

    /// The error's text.
    #[must_use]
    pub fn text(&self) -> &[u8] {
        &self.text
    }
}

/// `SM50Initialize`: a shader from a DXBC or DXIL container, with its reflection.
///
/// # Safety
///
/// `bytecode` is null or holds `size` bytes; `shader` is null or writable; `refl` is null or
/// writable for one reflection (written, never read); `error` is null or writable.
#[unsafe(export_name = "SM50Initialize")]
pub unsafe extern "C" fn sm50_initialize(
    bytecode: *const c_void,
    size: usize,
    shader: *mut *mut c_void,
    refl: *mut MtlShaderReflection,
    error: *mut *mut c_void,
) -> c_int {
    // SAFETY: `error` is null or writable (the caller's contract).
    unsafe { clear(error) };
    if shader.is_null() {
        // SAFETY: as above.
        return unsafe { refuse(error, Error::new(b"ppShader can not be null")) };
    }
    let bytes = if bytecode.is_null() {
        &[][..]
    } else {
        // SAFETY: a non-null `bytecode` holds `size` bytes (the caller's contract).
        unsafe { slice::from_raw_parts(bytecode.cast::<u8>(), size) }
    };
    // SAFETY: `refl` is null or writable for one reflection (the caller's contract).
    let initialized = unsafe { entry::initialize(bytes, refl) };
    // SAFETY: `shader` and `error` are writable (checked, and the caller's contract).
    unsafe { answer(initialized.map(Box::new), shader, error) }
}

/// `SM50Destroy`: frees a shader; null is no shader.
///
/// # Safety
///
/// `shader` is null or a live shader handle, not used again.
#[unsafe(export_name = "SM50Destroy")]
pub unsafe extern "C" fn sm50_destroy(shader: *mut c_void) {
    if !shader.is_null() {
        // SAFETY: a live handle is a `Box<Shader>` (the caller's contract).
        drop(unsafe { Box::from_raw(shader.cast::<Shader>()) });
    }
}

/// `SM50Compile`: one shader's function, named `name`, with the arguments of the chain `args`.
///
/// # Safety
///
/// `shader` is a live shader handle; `args` is null or a valid chain (see [`ArgChain::walk`]);
/// `name` is null or a C string; `bitcode` and `error` are null or writable.
#[unsafe(export_name = "SM50Compile")]
pub unsafe extern "C" fn sm50_compile(
    shader: *mut c_void,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: *const c_char,
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
) -> c_int {
    let compile = || {
        // SAFETY: a live shader handle (the caller's contract).
        let shader = unsafe { shader_ref(shader) };
        // SAFETY: a valid chain and a C string or null (the caller's contract).
        let (args, name) = unsafe { args_and_name(args, name) };
        entry::compile(shader, &args, name)
    };
    // SAFETY: `bitcode` and `error` are null or writable (the caller's contract).
    unsafe { compiled(bitcode, error, compile) }
}

/// `SM50GetCompiledBitcode`: where a compiled metallib's bytes are, and how many.
///
/// # Safety
///
/// `bitcode` is a live bitcode handle; `data` is writable.
#[unsafe(export_name = "SM50GetCompiledBitcode")]
pub unsafe extern "C" fn sm50_get_compiled_bitcode(
    bitcode: *mut c_void,
    data: *mut Sm50CompiledBitcode,
) {
    // SAFETY: a live handle is a `Box<Vec<u8>>` (the caller's contract).
    let bytes = unsafe { &*bitcode.cast::<Vec<u8>>() };
    let answer = Sm50CompiledBitcode {
        data: bytes.as_ptr().cast_mut().cast(),
        size: bytes.len() as u64,
    };
    // SAFETY: `data` is writable (the caller's contract).
    unsafe { data.write(answer) };
}

/// `SM50DestroyBitcode`: frees a compiled metallib; null is none.
///
/// # Safety
///
/// `bitcode` is null or a live bitcode handle, not used again.
#[unsafe(export_name = "SM50DestroyBitcode")]
pub unsafe extern "C" fn sm50_destroy_bitcode(bitcode: *mut c_void) {
    if !bitcode.is_null() {
        // SAFETY: a live handle is a `Box<Vec<u8>>` (the caller's contract).
        drop(unsafe { Box::from_raw(bitcode.cast::<Vec<u8>>()) });
    }
}

/// `SM50GetErrorMessage`: an error's text into `buffer`, NUL-terminated, cut to fit.
///
/// Answers the bytes written before the NUL. A `size` of 0 writes nothing and answers 0 (the C++
/// wrote past the buffer).
///
/// # Safety
///
/// `error` is a live error handle; `buffer` is writable for `size` bytes.
#[unsafe(export_name = "SM50GetErrorMessage")]
pub unsafe extern "C" fn sm50_get_error_message(
    error: *mut c_void,
    buffer: *mut c_char,
    size: usize,
) -> usize {
    if size == 0 {
        return 0;
    }
    // SAFETY: a live handle is a `Box<Error>` (the caller's contract).
    let text = unsafe { &*error.cast::<Error>() }.text();
    let len = text.len().min(size - 1);
    // SAFETY: `buffer` holds `size` > `len` bytes (the caller's contract); `text` holds `len`.
    let out = unsafe { slice::from_raw_parts_mut(buffer.cast::<u8>(), len + 1) };
    let (body, nul) = out.split_at_mut(len);
    body.copy_from_slice(text.get(..len).unwrap_or_default());
    nul.fill(0);
    len
}

/// `SM50FreeError`: frees an error; null is none.
///
/// # Safety
///
/// `error` is null or a live error handle, not used again.
#[unsafe(export_name = "SM50FreeError")]
pub unsafe extern "C" fn sm50_free_error(error: *mut c_void) {
    if !error.is_null() {
        // SAFETY: a live handle is a `Box<Error>` (the caller's contract).
        drop(unsafe { Box::from_raw(error.cast::<Error>()) });
    }
}

/// `SM50CompileTessellationPipelineHull`: the vertex and hull shaders as one function.
///
/// # Safety
///
/// As [`sm50_compile`], with two live shader handles.
#[unsafe(export_name = "SM50CompileTessellationPipelineHull")]
pub unsafe extern "C" fn sm50_compile_tessellation_pipeline_hull(
    vertex: *mut c_void,
    hull: *mut c_void,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: *const c_char,
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
) -> c_int {
    // SAFETY: as `sm50_compile`'s.
    unsafe {
        pipeline(
            vertex,
            hull,
            args,
            name,
            bitcode,
            error,
            entry::compile_hull,
        )
    }
}

/// `SM50CompileTessellationPipelineDomain`: the domain shader, fed by the hull shader.
///
/// # Safety
///
/// As [`sm50_compile`], with two live shader handles.
#[unsafe(export_name = "SM50CompileTessellationPipelineDomain")]
pub unsafe extern "C" fn sm50_compile_tessellation_pipeline_domain(
    hull: *mut c_void,
    domain: *mut c_void,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: *const c_char,
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
) -> c_int {
    // SAFETY: as `sm50_compile`'s.
    unsafe {
        pipeline(
            hull,
            domain,
            args,
            name,
            bitcode,
            error,
            entry::compile_domain,
        )
    }
}

/// `SM50CompileGeometryPipelineVertex`: the vertex shader, feeding the geometry shader.
///
/// # Safety
///
/// As [`sm50_compile`], with two live shader handles.
#[unsafe(export_name = "SM50CompileGeometryPipelineVertex")]
pub unsafe extern "C" fn sm50_compile_geometry_pipeline_vertex(
    vertex: *mut c_void,
    geometry: *mut c_void,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: *const c_char,
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
) -> c_int {
    // SAFETY: as `sm50_compile`'s.
    unsafe {
        pipeline(
            vertex,
            geometry,
            args,
            name,
            bitcode,
            error,
            entry::compile_gs_vertex,
        )
    }
}

/// `SM50CompileGeometryPipelineGeometry`: the geometry shader, fed by the vertex shader.
///
/// # Safety
///
/// As [`sm50_compile`], with two live shader handles.
#[unsafe(export_name = "SM50CompileGeometryPipelineGeometry")]
pub unsafe extern "C" fn sm50_compile_geometry_pipeline_geometry(
    vertex: *mut c_void,
    geometry: *mut c_void,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: *const c_char,
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
) -> c_int {
    // SAFETY: as `sm50_compile`'s.
    unsafe {
        pipeline(
            vertex,
            geometry,
            args,
            name,
            bitcode,
            error,
            entry::compile_gs,
        )
    }
}

/// `SM50GetArgumentsInfo`: copies the shader's constant-buffer and argument tables.
///
/// Each table goes to a non-null pointer, which holds as many entries as the reflection's
/// `NumConstantBuffers` or `NumArguments` said.
///
/// # Safety
///
/// `shader` is a live shader handle; each table pointer is null or writable for its entries.
#[unsafe(export_name = "SM50GetArgumentsInfo")]
pub unsafe extern "C" fn sm50_get_arguments_info(
    shader: *mut c_void,
    constant_buffers: *mut MtlSm50ShaderArgument,
    arguments: *mut MtlSm50ShaderArgument,
) {
    // SAFETY: a live shader handle (the caller's contract).
    let (cbs, args) = unsafe { shader_ref(shader) }.arguments();
    for (table, out) in [(cbs, constant_buffers), (args, arguments)] {
        if !out.is_null() {
            // SAFETY: `out` holds the table's entries (the caller's contract).
            unsafe { ptr::copy_nonoverlapping(table.as_ptr(), out, table.len()) };
        }
    }
}

/// `DXILCompilePassThroughVertex`: a vertex function for a DXIL pixel shader alone (test use).
///
/// # Safety
///
/// `pixel` is a live shader handle; `bitcode` is writable; `error` is null or writable.
#[unsafe(export_name = "DXILCompilePassThroughVertex")]
pub unsafe extern "C" fn dxil_compile_pass_through_vertex(
    pixel: *mut c_void,
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
) -> c_int {
    // SAFETY: `error` is null or writable (the caller's contract).
    unsafe { clear(error) };
    // SAFETY: a live shader handle (the caller's contract).
    let compiled = entry::passthrough_vertex(unsafe { shader_ref(pixel) });
    // SAFETY: `bitcode` and `error` are writable (the caller's contract).
    unsafe { answer(compiled.map(Box::new), bitcode, error) }
}

/// `DXILGetEntry`: the shader's entry point, its signature elements and resources (test use).
///
/// # Safety
///
/// `shader` is a live shader handle; `out` is writable.
#[unsafe(export_name = "DXILGetEntry")]
pub const unsafe extern "C" fn dxil_get_entry(shader: *mut c_void, out: *mut DxilEntryInfo) {
    // SAFETY: a live shader handle (the caller's contract).
    let info = entry::entry_info(unsafe { shader_ref(shader) });
    // SAFETY: `out` is writable (the caller's contract).
    unsafe { out.write(info) };
}

/// A two-shader compile's entry point.
type PipelineCompile = fn(&Shader, &Shader, &ArgChain<'_>, &CStr) -> Result<Vec<u8>, Error>;

/// A two-shader compile: `compile` on the two shaders, answered as [`compiled`] answers.
///
/// # Safety
///
/// As [`sm50_compile`], with `first` and `second` live shader handles.
unsafe fn pipeline(
    first: *mut c_void,
    second: *mut c_void,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: *const c_char,
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
    compile: PipelineCompile,
) -> c_int {
    let pipeline = || {
        // SAFETY: a live shader handle (the caller's contract).
        let first = unsafe { shader_ref(first) };
        // SAFETY: as above.
        let second = unsafe { shader_ref(second) };
        // SAFETY: a valid chain and a C string or null (the caller's contract).
        let (args, name) = unsafe { args_and_name(args, name) };
        compile(first, second, &args, name)
    };
    // SAFETY: `bitcode` and `error` are null or writable (the caller's contract).
    unsafe { compiled(bitcode, error, pipeline) }
}

/// A compile's answer, as the C++'s: a null `bitcode` is refused before `compile` runs.
///
/// # Safety
///
/// `bitcode` and `error` are null or writable.
unsafe fn compiled(
    bitcode: *mut *mut c_void,
    error: *mut *mut c_void,
    compile: impl FnOnce() -> Result<Vec<u8>, Error>,
) -> c_int {
    // SAFETY: `error` is null or writable (the caller's contract).
    unsafe { clear(error) };
    if bitcode.is_null() {
        // SAFETY: as above.
        return unsafe { refuse(error, Error::new(b"ppBitcode can not be null")) };
    }
    // SAFETY: `bitcode` is writable (checked, and the caller's contract); `error` as above.
    unsafe { answer(compile().map(Box::new), bitcode, error) }
}

/// A compile's argument chain and function name.
///
/// # Safety
///
/// `args` is null or a valid chain (see [`ArgChain::walk`]); `name` is null or a C string; both
/// outlive the answer.
const unsafe fn args_and_name<'a>(
    args: *const Sm50ShaderCompilationArgumentData,
    name: *const c_char,
) -> (ArgChain<'a>, &'a CStr) {
    // SAFETY: the caller's contract.
    let args = unsafe { ArgChain::walk(args) };
    // SAFETY: the caller's contract.
    (args, unsafe { c_str(name) })
}

/// `*error = null`, when `error` is not null.
///
/// # Safety
///
/// `error` is null or writable.
const unsafe fn clear(error: *mut *mut c_void) {
    if !error.is_null() {
        // SAFETY: a non-null `error` is writable (the caller's contract).
        unsafe { error.write(ptr::null_mut()) };
    }
}

/// Answers `result`: 0 with its value boxed into `*out`, or 1 with the error in `*error`.
///
/// # Safety
///
/// `out` is writable; `error` is null or writable.
unsafe fn answer<T>(
    result: Result<Box<T>, Error>,
    out: *mut *mut c_void,
    error: *mut *mut c_void,
) -> c_int {
    match result {
        Ok(value) => {
            // SAFETY: `out` is writable (the caller's contract).
            unsafe { out.write(Box::into_raw(value).cast()) };
            0
        }
        // SAFETY: `error` is null or writable (the caller's contract).
        Err(e) => unsafe { refuse(error, e) },
    }
}

/// Answers 1 with `e` in `*error`; drops it when `error` is null.
///
/// # Safety
///
/// `error` is null or writable.
unsafe fn refuse(error: *mut *mut c_void, e: Error) -> c_int {
    if !error.is_null() {
        // SAFETY: a non-null `error` is writable (the caller's contract).
        unsafe { error.write(Box::into_raw(Box::new(e)).cast()) };
    }
    1
}

/// The shader a handle owns.
///
/// # Safety
///
/// `shader` is a live shader handle, which outlives the reference.
const unsafe fn shader_ref<'a>(shader: *mut c_void) -> &'a Shader {
    // SAFETY: a live handle is a `Box<Shader>` (the caller's contract).
    unsafe { &*shader.cast::<Shader>() }
}

/// A function name: a null `name` is the empty name, as LLVM's `StringRef` takes the C++'s null.
///
/// # Safety
///
/// `name` is null or a C string, which outlives the reference.
const unsafe fn c_str<'a>(name: *const c_char) -> &'a CStr {
    if name.is_null() {
        c""
    } else {
        // SAFETY: a non-null `name` is a C string (the caller's contract).
        unsafe { CStr::from_ptr(name) }
    }
}

#[cfg(test)]
mod tests;
