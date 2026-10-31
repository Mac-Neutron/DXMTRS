//! DXMT's core, which `d3d11.dll` and `d3d12.dll` (and their dylibs) share.
//!
//! Each DLL's static library (`src/d3d11`, `src/d3d12`) links its own copy. Until a front end is
//! in Rust, DXMT's C++ reaches the core's live pieces through the `extern "C"` functions each
//! seam's header declares.

pub mod arena;
pub mod capture;
pub mod counter;
pub mod deptrack;
pub mod dump;
pub mod dynamic;
pub mod emulated;
pub mod encoder;
pub mod format;
pub mod hud;
pub mod info;
pub mod initializer;
pub mod library;
pub mod log;
pub mod presenter;
pub mod query;
pub mod queue;
pub mod residency;
pub mod resource;
pub mod ring;
pub mod sampler;
pub mod scaler;
pub mod shader_cache;
pub mod staging;
pub mod stats;
