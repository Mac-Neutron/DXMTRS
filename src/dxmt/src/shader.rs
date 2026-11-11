//! Shader plumbing D3D11 and D3D12 share.
//!
//! airconv's owned handles and entry points, and a Metal function from a metallib. What a failure
//! logs, the cache keys and the compile arguments stay with each front end.

use core::ffi::{CStr, c_void};

use winemetal_sys::{
    DispatchData_alloc_init, MTLDevice_newLibrary, MTLLibrary_newFunction, Obj,
    sm50::{
        MtlShaderReflection, SM50Compile, SM50CompileGeometryPipelineGeometry,
        SM50CompileGeometryPipelineVertex, SM50CompileTessellationPipelineDomain,
        SM50CompileTessellationPipelineHull, SM50Destroy, SM50DestroyBitcode, SM50FreeError,
        SM50GetCompiledBitcode, SM50Initialize, Sm50CompiledBitcode,
        Sm50ShaderCompilationArgumentData, error_message,
    },
};

/// `SM50_SHADER_D3D9`, the argument type of a D3D9 stage's specialization (`airconv_public.h`).
pub const SM50_SHADER_D3D9: u32 = 10;
/// Bytes of the vertex half of `SM50_SHADER_D3D9_DATA` (airconv-ir's `VS_SPEC_WIRE`).
pub const D3D9_VS_WIRE: usize = 6;
/// Bytes of the pixel half of `SM50_SHADER_D3D9_DATA` (airconv-ir's `VARIANT_KEY_WIRE`).
pub const D3D9_PS_WIRE: usize = 22;

/// `SM50_SHADER_D3D9_DATA`: a D3D9 stage's specialization, a node of a compile's argument chain.
///
/// 48 bytes on a 64-bit process (`stage` at 12, `vs` at 16, `ps` at 22), 40 on a 32-bit one.
#[repr(C)]
pub struct Sm50ShaderD3d9Data {
    pub next: *mut c_void,
    /// [`SM50_SHADER_D3D9`].
    pub r#type: u32,
    /// 0 for a vertex shader, 1 for a pixel shader.
    pub stage: u32,
    pub vs: [u8; D3D9_VS_WIRE],
    pub ps: [u8; D3D9_PS_WIRE],
}

const _: () = {
    let wide = size_of::<usize>() == 8;
    assert!(size_of::<Sm50ShaderD3d9Data>() == if wide { 48 } else { 40 });
    assert!(core::mem::offset_of!(Sm50ShaderD3d9Data, stage) == if wide { 12 } else { 8 });
    assert!(core::mem::offset_of!(Sm50ShaderD3d9Data, vs) == if wide { 16 } else { 12 });
    assert!(core::mem::offset_of!(Sm50ShaderD3d9Data, ps) == if wide { 22 } else { 18 });
};

/// An owned `sm50_shader_t` handle; 0 is none (`SM50Shader`, `d3d12_pipeline.hpp:30-47`).
pub struct Sm50Shader(pub u64);

impl Drop for Sm50Shader {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: `self.0` was returned by `SM50Initialize` and is owned by `self`.
            unsafe { SM50Destroy(self.0) };
        }
    }
}

/// An owned `sm50_bitcode_t` handle; 0 is none (`SM50ShaderBitcode`, `d3d12_pipeline.hpp:49-66`).
pub struct Sm50Bitcode(pub u64);

impl Drop for Sm50Bitcode {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: `self.0` was returned by `SM50Compile*` and is owned by `self`.
            unsafe { SM50DestroyBitcode(self.0) };
        }
    }
}

/// An owned `sm50_error_t` handle; 0 is none (`SM50Error`, `d3d12_pipeline.hpp:68-85`).
///
/// The second field is an entry's own refusal of bytes it does not take, before airconv sees
/// them: [`initialize`]'s of a stream that is not DXBC, [`initialize_dxso`]'s of DXBC.
pub struct Sm50Error(pub u64, Option<&'static str>);

impl Sm50Error {
    /// The error's text (`SM50GetErrorMessage`).
    #[must_use]
    pub fn message(&self) -> String {
        if let Some(refusal) = self.1 {
            return refusal.to_owned();
        }
        if self.0 == 0 {
            return "no airconv error object".to_owned();
        }
        // SAFETY: `self.0` is a live `sm50_error_t` handle or 0.
        unsafe { error_message(self.0) }
    }
}

impl Drop for Sm50Error {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: `self.0` was returned by an `SM50*` call and is owned by `self`.
            unsafe { SM50FreeError(self.0) };
        }
    }
}

/// The airconv entry point that compiles a function, its two shaders in the C API's order.
pub enum Entry {
    /// `SM50Compile`: one shader; the second handle is unused.
    Shader,
    /// `SM50CompileTessellationPipelineHull`: the vertex shader, then the hull shader.
    TessellationHull,
    /// `SM50CompileTessellationPipelineDomain`: the hull shader, then the domain shader.
    TessellationDomain,
    /// `SM50CompileGeometryPipelineVertex`: the vertex shader, then the geometry shader.
    GeometryVertex,
    /// `SM50CompileGeometryPipelineGeometry`: the vertex shader, then the geometry shader.
    Geometry,
}

/// Parses DXBC `bytecode` into a shader, writing its reflection.
///
/// Input that does not open with `DXBC` is refused before airconv sees it: `SM50Initialize` also
/// reads D3D9's token streams, which D3D10-12 must not create shaders from.
///
/// # Errors
///
/// airconv's error when it refuses the bytecode; its handle may be 0, which is also the
/// refusal of a non-DXBC stream.
pub fn initialize(
    bytecode: &[u8],
    reflection: &mut MtlShaderReflection,
) -> Result<Sm50Shader, Sm50Error> {
    if !bytecode.starts_with(b"DXBC") {
        return Err(Sm50Error(0, Some("not a DXBC container")));
    }
    let mut shader = 0;
    let mut error = 0;
    // SAFETY: `bytecode` is valid for its length; the three outputs are writable.
    let status = unsafe {
        SM50Initialize(
            bytecode.as_ptr().cast::<c_void>(),
            bytecode.len(),
            &raw mut shader,
            reflection,
            &raw mut error,
        )
    };
    if status == 0 {
        Ok(Sm50Shader(shader))
    } else {
        Err(Sm50Error(error, None))
    }
}

/// Parses a D3D9 token stream (`bytecode`, DXSO) into a shader, with the node its compile needs.
///
/// `wire` is the stage's specialization as airconv-ir's `StageSpec::to_wire` gives it: the stage
/// (0 vertex, 1 pixel), the vertex half and the pixel half. The returned
/// [`Sm50ShaderD3d9Data`] goes into the argument chain [`compile`] is handed, after
/// `SM50_SHADER_COMMON`'s node, so [`compile`], [`compiled_data`] and [`load_function`] serve D3D9
/// as they serve D3D10-12. A DXSO shader has no reflection to write. DXBC is refused: D3D9 does
/// not create shaders from a D3D10-12 container.
///
/// # Errors
///
/// The refusal of a DXBC container, or airconv's error when it refuses the tokens.
pub fn initialize_dxso(
    bytecode: &[u8],
    wire: (u32, [u8; D3D9_VS_WIRE], [u8; D3D9_PS_WIRE]),
) -> Result<(Sm50Shader, Sm50ShaderD3d9Data), Sm50Error> {
    if bytecode.starts_with(b"DXBC") {
        return Err(Sm50Error(
            0,
            Some("a DXBC container, not a D3D9 token stream"),
        ));
    }
    let mut shader = 0;
    let mut error = 0;
    // SAFETY: `bytecode` is valid for its length; the shader and error outputs are writable and
    // the reflection is null, which airconv skips.
    let status = unsafe {
        SM50Initialize(
            bytecode.as_ptr().cast::<c_void>(),
            bytecode.len(),
            &raw mut shader,
            core::ptr::null_mut(),
            &raw mut error,
        )
    };
    if status != 0 {
        return Err(Sm50Error(error, None));
    }
    let (stage, vs, ps) = wire;
    let d3d9 = Sm50ShaderD3d9Data {
        next: core::ptr::null_mut(),
        r#type: SM50_SHADER_D3D9,
        stage,
        vs,
        ps,
    };
    Ok((Sm50Shader(shader), d3d9))
}

/// Compiles the function `name` through `entry`.
///
/// # Errors
///
/// airconv's error when the compile fails.
///
/// # Safety
///
/// `first`, and `second` for every entry but [`Entry::Shader`], are live shader handles; `args`
/// is null or a valid `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain.
pub unsafe fn compile(
    entry: &Entry,
    first: u64,
    second: u64,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: &CStr,
) -> Result<Sm50Bitcode, Sm50Error> {
    let (mut bitcode, mut error) = (0, 0);
    let (out, err, name) = (&raw mut bitcode, &raw mut error, name.as_ptr());
    let status = match entry {
        // SAFETY: the caller's contract; `out` and `err` are writable.
        Entry::Shader => unsafe { SM50Compile(first, args, name, out, err) },
        Entry::TessellationHull => {
            // SAFETY: as above.
            unsafe { SM50CompileTessellationPipelineHull(first, second, args, name, out, err) }
        }
        Entry::TessellationDomain => {
            // SAFETY: as above.
            unsafe { SM50CompileTessellationPipelineDomain(first, second, args, name, out, err) }
        }
        Entry::GeometryVertex => {
            // SAFETY: as above.
            unsafe { SM50CompileGeometryPipelineVertex(first, second, args, name, out, err) }
        }
        Entry::Geometry => {
            // SAFETY: as above.
            unsafe { SM50CompileGeometryPipelineGeometry(first, second, args, name, out, err) }
        }
    };
    if status == 0 {
        Ok(Sm50Bitcode(bitcode))
    } else {
        Err(Sm50Error(error, None))
    }
}

/// The compiled metallib's bytes as a data object, which the disk cache stores.
#[must_use]
pub fn compiled_data(bitcode: &Sm50Bitcode) -> Option<Obj> {
    let mut compiled = Sm50CompiledBitcode { data: 0, size: 0 };
    // SAFETY: `bitcode` holds a live bitcode handle; `compiled` is writable.
    unsafe { SM50GetCompiledBitcode(bitcode.0, &raw mut compiled) };
    // SAFETY: the bytes stay valid while `bitcode` lives, and the data object copies them.
    let data = unsafe { DispatchData_alloc_init(compiled.data, compiled.size) };
    // SAFETY: a new data object's handle carries its reference.
    unsafe { Obj::adopt(data) }
}

/// The library the metallib `data` holds, made on `device`.
///
/// `None` when Metal makes no library or reports an error, which is dropped.
#[must_use]
pub fn new_library(device: &Obj, data: &Obj) -> Option<Obj> {
    let mut error = 0;
    // SAFETY: `device` holds a live `MTLDevice` and `data` a data object; `error` is writable.
    let library = unsafe { MTLDevice_newLibrary(device.handle(), data.handle(), &raw mut error) };
    // SAFETY: a new error's handle carries its reference.
    let error = unsafe { Obj::adopt(error) };
    // SAFETY: a new library's handle carries its reference.
    let library = unsafe { Obj::adopt(library) };
    library.filter(|_| error.is_none())
}

/// The function `name` of `library`, if it has one.
#[must_use]
pub fn new_function(library: &Obj, name: &CStr) -> Option<Obj> {
    // SAFETY: `library` holds a live `MTLLibrary`; `name` is NUL-terminated.
    let function = unsafe { MTLLibrary_newFunction(library.handle(), name.as_ptr()) };
    // SAFETY: a new function's handle carries its reference.
    unsafe { Obj::adopt(function) }
}

/// What [`load_function`] could not make.
pub enum LoadError {
    Library,
    Function,
}

/// The function `name` of the metallib `data` (from the disk cache, or just compiled) on `device`.
///
/// # Errors
///
/// Which of the library and the function Metal did not make.
pub fn load_function(device: &Obj, data: &Obj, name: &CStr) -> Result<Obj, LoadError> {
    let library = new_library(device, data).ok_or(LoadError::Library)?;
    new_function(&library, name).ok_or(LoadError::Function)
}

#[cfg(test)]
mod tests;
