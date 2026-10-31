//! DXMT's shader translator: DXBC and DXIL to Metal's AIR, behind `airconv_public.h`.
//!
//! The crate exports the header's functions under their C names (`SM50Initialize`, …) and, for
//! the test tools, `DXILCompilePassThroughVertex` and `DXILGetEntry` of `dxil/dxil_public.h`.
//! Translation runs on LLVM 15 through its C API, declared here by hand, and a C++ shim for what
//! that API lacks (`shim/shim.cpp`, with the two passes of `transforms/`), both built by
//! `build.rs` against the install `AIRCONV_LLVM` names. LLVM's libraries are linked by the final
//! image, never bundled into this crate.
//!
//! Guest bytes never panic and never end the process: an exported function answers an error.
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

pub mod abi;
pub mod air;
mod api;
pub mod dxbc;
pub mod dxil;
mod entry;
pub mod llvm;
pub mod module;
#[cfg(feature = "oracle")]
pub mod oracle;
pub mod shader;
pub mod writer;

pub use api::{
    Error, dxil_compile_pass_through_vertex, dxil_get_entry, sm50_compile,
    sm50_compile_geometry_pipeline_geometry, sm50_compile_geometry_pipeline_vertex,
    sm50_compile_tessellation_pipeline_domain, sm50_compile_tessellation_pipeline_hull,
    sm50_destroy, sm50_destroy_bitcode, sm50_free_error, sm50_get_arguments_info,
    sm50_get_compiled_bitcode, sm50_get_error_message, sm50_initialize,
};

#[cfg(test)]
mod tests;
