//! How D3D12 answers a failed compile (`d3d12_pipeline.hpp:108-113`).
//!
//! airconv's owned handles (`d3d12_pipeline.hpp:30-85`) are `dxmt_core::shader`'s.

pub use dxmt_core::shader::{Sm50Bitcode, Sm50Error, Sm50Shader};

use crate::com::{E_FAIL, E_NOTIMPL, Hresult};

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
