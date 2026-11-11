//! Direct3D 9 on DXMT's core (`runtime.mode = core`, DXMTRS C4).
//!
//! `d3d9.dll` keeps its COM objects and API state; under the core runtime it records through this
//! crate into `dxmt-core`'s chunks, as `d3d11.dll` does: closures the core's encoder runs on its
//! encode thread, the core's hazard tracking, optimizer, queue and presenter. Linked into
//! `d3d9.dll` by its `core-runtime` feature (meson's `d3d9_core_runtime`). Its tests run the
//! recording over `dxmt-fakes`' winemetal fakes, in a cargo command of their own.

pub mod device;
#[cfg(feature = "guest-vm")]
pub mod guest_pages;
