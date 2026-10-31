//! airconv's half without LLVM: what the translator reads and packs before and after LLVM.
//!
//! The DXBC container and its signatures (ported from `libs/DXBCParser`, whose MIT notice the
//! ported files keep), root signatures, the DXIL part, the token decoder and control flow, the
//! signature handlers as data, the binding recorders, reflection, the hull estimators, DXBC
//! Initialize, and the metallib container with its SHA-256. It builds for every target the tree
//! builds for, so the PE crates can read DXBC with it too.
//!
//! Guest bytes never panic: a short or lying blob is an error, never an index past the end.
#![cfg_attr(
    not(test),
    deny(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic
    )
)]

pub mod binding;
pub mod cfg;
pub mod container;
pub mod decode;
pub mod dxil_part;
pub mod estimators;
mod float_cast;
pub mod handlers;
pub mod initialize;
pub mod ir;
pub mod metallib;
pub mod reflection;
pub mod root_signature;
pub mod sha256;
pub mod shader_info;
pub mod signature;
pub mod tokens;
