//! `sm50-abi.h`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_void, usize, *mut u64, *mut winemetal_sys::sm50::MtlShaderReflection, *mut u64) -> i32 = winemetal_sys::sm50::SM50Initialize;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50Destroy;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50Compile;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50CompiledBitcode) = winemetal_sys::sm50::SM50GetCompiledBitcode;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50DestroyBitcode;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut core::ffi::c_char, usize) -> usize = winemetal_sys::sm50::SM50GetErrorMessage;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50FreeError;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineHull;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineDomain;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineVertex;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineGeometry;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::MtlSm50ShaderArgument, *mut winemetal_sys::sm50::MtlSm50ShaderArgument) = winemetal_sys::sm50::SM50GetArgumentsInfo;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_void, usize, *mut u64, *mut winemetal_sys::sm50::MtlShaderReflection, *mut u64) -> i32 = winemetal_sys::sm50::SM50Initialize;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50Destroy;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50Compile;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50CompiledBitcode) = winemetal_sys::sm50::SM50GetCompiledBitcode;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50DestroyBitcode;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut core::ffi::c_char, usize) -> usize = winemetal_sys::sm50::SM50GetErrorMessage;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50FreeError;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineHull;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineDomain;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineVertex;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineGeometry;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::MtlSm50ShaderArgument, *mut winemetal_sys::sm50::MtlSm50ShaderArgument) = winemetal_sys::sm50::SM50GetArgumentsInfo;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_void, usize, *mut u64, *mut winemetal_sys::sm50::MtlShaderReflection, *mut u64) -> i32 = winemetal_sys::sm50::SM50Initialize;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50Destroy;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50Compile;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50CompiledBitcode) = winemetal_sys::sm50::SM50GetCompiledBitcode;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50DestroyBitcode;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut core::ffi::c_char, usize) -> usize = winemetal_sys::sm50::SM50GetErrorMessage;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50FreeError;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineHull;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineDomain;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineVertex;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineGeometry;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::MtlSm50ShaderArgument, *mut winemetal_sys::sm50::MtlSm50ShaderArgument) = winemetal_sys::sm50::SM50GetArgumentsInfo;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_void, usize, *mut u64, *mut winemetal_sys::sm50::MtlShaderReflection, *mut u64) -> i32 = winemetal_sys::sm50::SM50Initialize;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50Destroy;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50Compile;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::Sm50CompiledBitcode) = winemetal_sys::sm50::SM50GetCompiledBitcode;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50DestroyBitcode;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut core::ffi::c_char, usize) -> usize = winemetal_sys::sm50::SM50GetErrorMessage;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64) = winemetal_sys::sm50::SM50FreeError;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineHull;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineDomain;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineVertex;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineGeometry;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut winemetal_sys::sm50::MtlSm50ShaderArgument, *mut winemetal_sys::sm50::MtlSm50ShaderArgument) = winemetal_sys::sm50::SM50GetArgumentsInfo;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*const core::ffi::c_void, usize, *mut u64, *mut winemetal_sys::sm50::MtlShaderReflection, *mut u64) -> i32 = winemetal_sys::sm50::SM50Initialize;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64) = winemetal_sys::sm50::SM50Destroy;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50Compile;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, *mut winemetal_sys::sm50::Sm50CompiledBitcode) = winemetal_sys::sm50::SM50GetCompiledBitcode;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64) = winemetal_sys::sm50::SM50DestroyBitcode;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, *mut core::ffi::c_char, usize) -> usize = winemetal_sys::sm50::SM50GetErrorMessage;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64) = winemetal_sys::sm50::SM50FreeError;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineHull;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileTessellationPipelineDomain;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineVertex;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, u64, *mut winemetal_sys::sm50::Sm50ShaderCompilationArgumentData, *const core::ffi::c_char, *mut u64, *mut u64) -> i32 = winemetal_sys::sm50::SM50CompileGeometryPipelineGeometry;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, *mut winemetal_sys::sm50::MtlSm50ShaderArgument, *mut winemetal_sys::sm50::MtlSm50ShaderArgument) = winemetal_sys::sm50::SM50GetArgumentsInfo;
