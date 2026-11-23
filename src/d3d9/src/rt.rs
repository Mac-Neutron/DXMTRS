//! Direct3D 9 runtime built on DXMT's core (`dxmt-core`).
//!
//! `d3d9.dll` keeps its COM objects and API state and records through `crate::rt` into
//! `dxmt-core`'s chunks, as `d3d11.dll` does: closures the core's encoder runs on its encode
//! thread, plus the core's hazard tracking, optimizer, queue and presenter. Host tests compile
//! `d3d9` without the `front-end` feature and run this runtime over `dxmt-fakes`.

pub mod device;
#[cfg(feature = "guest-vm")]
pub mod guest_pages;
pub mod stats;

pub use device::close_shader_cache_on_detach;
pub use stats::flush_stats_on_detach;
