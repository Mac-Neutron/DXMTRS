//! DXMT's `d3d11.dll` and `d3d11.dylib`: the Rust linked into the C++ front end.
//!
//! The static library carries DXMT's core, whose `extern "C"` functions the C++ calls: nothing
//! else names the core here, and without the `use` below the archive would hold none of them.

use dxmt_core as _;
