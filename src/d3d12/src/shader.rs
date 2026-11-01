//! RAII wrappers around `sm50_shader_t`, `sm50_bitcode_t`, and `sm50_error_t`
//!
//! (`d3d12_pipeline.hpp:30-113`).

use winemetal_sys::sm50::{SM50Destroy, SM50DestroyBitcode, SM50FreeError, error_message};

use crate::com::{E_FAIL, E_NOTIMPL, Hresult};

/// Owned `sm50_shader_t` handle (`SM50Shader`, `d3d12_pipeline.hpp:30-47`).
pub struct Sm50Shader(pub u64);

impl Sm50Shader {
    /// Creates an empty handle.
    #[must_use]
    pub const fn null() -> Self {
        Self(0)
    }
}

impl Drop for Sm50Shader {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: `self.0` was returned by `SM50Initialize` and is owned by `self`.
            unsafe { SM50Destroy(self.0) };
        }
    }
}

/// Owned `sm50_bitcode_t` handle (`SM50ShaderBitcode`, `d3d12_pipeline.hpp:49-66`).
pub struct Sm50Bitcode(pub u64);

impl Sm50Bitcode {
    /// Creates an empty handle.
    #[must_use]
    pub const fn null() -> Self {
        Self(0)
    }
}

impl Drop for Sm50Bitcode {
    fn drop(&mut self) {
        if self.0 != 0 {
            // SAFETY: `self.0` was returned by `SM50Compile*` and is owned by `self`.
            unsafe { SM50DestroyBitcode(self.0) };
        }
    }
}

/// Owned `sm50_error_t` handle (`SM50Error`, `d3d12_pipeline.hpp:68-85`).
pub struct Sm50Error(pub u64);

impl Sm50Error {
    /// Creates an empty handle.
    #[must_use]
    pub const fn null() -> Self {
        Self(0)
    }

    /// Returns the error string from `SM50GetErrorMessage` (`d3d12_pipeline.hpp:83`).
    #[must_use]
    pub fn message(&self) -> String {
        // SAFETY: `self.0` is a live `sm50_error_t` handle or `0`.
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

/// Logs an airconv error and maps `"DXIL:"`-prefixed messages to `E_NOTIMPL`, else `E_FAIL`
///
/// (`ShaderCompileFailed`, `d3d12_pipeline.hpp:108-113`).
#[must_use]
pub fn shader_compile_failed(stage: &str, error: &Sm50Error) -> Hresult {
    let msg = error.message();
    crate::err!("Failed to compile {stage} shader: {msg}");
    if msg.starts_with("DXIL:") {
        E_NOTIMPL
    } else {
        E_FAIL
    }
}
