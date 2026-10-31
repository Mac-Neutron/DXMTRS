//! DXMT's `d3d12.dll` and `d3d12.dylib`: the Rust linked into the C++ front end.
//!
//! The static library carries DXMT's core, whose `extern "C"` functions the C++ calls: nothing
//! else names the core here, and without the `use` below the archive would hold none of them.
//! The modules are D3D12's pure helpers, written ahead of the front end's port.

use dxmt_core as _;

pub mod abi;
pub mod barrier;
pub mod feature;
pub mod pipeline_stream;
pub mod resource_math;
pub mod root_signature;
pub mod sampler;
