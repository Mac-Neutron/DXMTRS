//! Initialize: a shader read from its container, its reflection written.

use crate::{
    abi::{DxilEntryInfo, MtlShaderReflection},
    api::Error,
    shader::Shader,
};

/// Reads a DXBC or DXIL container into a shader and writes its reflection to `refl`.
///
/// # Errors
///
/// The container's refusal, or "not ported".
///
/// # Safety
///
/// `refl` is null or points to writable memory for one reflection, which may be uninitialised:
/// only the stage's fields are written, nothing is read.
pub unsafe fn initialize(bytes: &[u8], refl: *mut MtlShaderReflection) -> Result<Shader, Error> {
    let _ = (bytes, refl);
    Err(Error::not_ported())
}

/// The shader's entry point as `DXILGetEntry` answers it.
#[must_use]
pub const fn entry_info(shader: &Shader) -> DxilEntryInfo {
    let _ = shader;
    DxilEntryInfo::EMPTY
}
