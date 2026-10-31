//! The translator's entry points, one per export, each in the file of the work that fills it.
//!
//! Until its work lands an entry point answers "not ported".

mod compile;
mod geometry;
mod initialize;
mod tessellation;

pub use compile::{compile, passthrough_vertex};
pub use geometry::{compile_gs, compile_gs_vertex};
pub use initialize::{entry_info, initialize};
pub use tessellation::{compile_domain, compile_hull};

#[cfg(test)]
mod tests;
