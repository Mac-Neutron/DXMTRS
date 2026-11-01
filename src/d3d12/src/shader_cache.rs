//! D3D12 shader translation cache (`d3d12_shader_cache.{hpp,cpp}`): `CachedShader`,
//!
//! `HashCompileArgs`, `CompileFunction`, and `LogShaderCacheCounters`.

use std::sync::Arc;

use winemetal_sys::{
    Obj,
    sm50::{MtlShaderReflection, Sm50ShaderCompilationArgumentData},
};

use crate::{com::Hresult, shader::Sm50Shader};

/// 20-byte SHA-1 digest used for shader and root-signature cache keys (`dxmt::sha1_digest`).
pub type Sha1Digest = [u8; 20];

/// Stage compilation mode (`FunctionKind`, `d3d12_shader_cache.hpp:55`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionKind {
    Shader,
    GeometryVertex,
    GeometryMesh,
}

/// One shader stage's bytecode, digest, reflection, and lazily-parsed `SM50Shader`
///
/// (`CachedShader`, `d3d12_shader_cache.hpp:31-50`).
pub struct CachedShader {
    pub bytecode: Vec<u8>,
    pub digest: Sha1Digest,
    pub shader: Option<Sm50Shader>,
    pub reflected: bool,
    pub reflection: MtlShaderReflection,
}

impl CachedShader {
    /// Checks the container and computes its SHA-1 digest (`Initialize`,
    ///
    /// `d3d12_shader_cache.cpp:245`).
    ///
    /// # Errors
    ///
    /// Returns an `Hresult` error when `bytecode` is not a valid DXBC/DXIL container.
    pub fn new(_bytecode: &[u8]) -> Result<Arc<Self>, Hresult> {
        crate::not_implemented!("shader_cache::CachedShader::new")
    }

    /// The SHA-1 digest of the shader bytecode.
    #[must_use]
    pub const fn digest(&self) -> &Sha1Digest {
        &self.digest
    }

    /// Reads (or computes and caches) the shader's [`MtlShaderReflection`] (`Reflection`,
    ///
    /// `d3d12_shader_cache.cpp:277`).
    pub fn reflection(&mut self, _out: &mut MtlShaderReflection) -> Hresult {
        crate::not_implemented!("shader_cache::CachedShader::reflection")
    }

    /// Lazily parses the `sm50_shader_t` handle (`Parse`, `d3d12_shader_cache.cpp:265`).
    ///
    /// # Errors
    ///
    /// Returns an `Hresult` error (`E_NOTIMPL` on `"DXIL:"`, else `E_FAIL`) when parsing fails.
    pub fn parse(&mut self) -> Result<u64, Hresult> {
        crate::not_implemented!("shader_cache::CachedShader::parse")
    }
}

/// Hashes an `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain (`HashCompileArgs`,
///
/// `d3d12_shader_cache.cpp:122-243`).
///
/// # Safety
///
/// `args` must be null or a valid null-terminated `SM50_SHADER_COMPILATION_ARGUMENT_DATA` linked
/// list whose inner array pointers are valid for their declared counts.
#[must_use]
pub unsafe fn hash_compile_args(
    _args: *const Sm50ShaderCompilationArgumentData,
) -> Option<Sha1Digest> {
    crate::not_implemented!("shader_cache::hash_compile_args")
}

/// Translates or fetches a cached Metal function (`CompileFunction`,
///
/// `d3d12_shader_cache.cpp:305-419`).
///
/// # Errors
///
/// Returns an `Hresult` error (`E_NOTIMPL` on `"DXIL:"`, else `E_FAIL`) if translation or Metal
/// compilation fails.
///
/// # Safety
///
/// `args` must be null or a valid `SM50_SHADER_COMPILATION_ARGUMENT_DATA` linked list.
pub unsafe fn compile_function(
    _device: &Obj,
    _kind: FunctionKind,
    _first: &mut CachedShader,
    _second: Option<&mut CachedShader>,
    _args: *mut Sm50ShaderCompilationArgumentData,
    _name: &str,
    _stage: &str,
) -> Result<Obj, Hresult> {
    crate::not_implemented!("shader_cache::compile_function")
}

/// Logs `"d3d12 shader cache: functions <h> hit <m> missed, reflections <h> hit <m> missed"` if any
///
/// lookup occurred (`LogShaderCacheCounters`, `d3d12_shader_cache.cpp:421`).
pub fn log_counters() {
    let (): () = crate::not_implemented!("shader_cache::log_counters");
}
