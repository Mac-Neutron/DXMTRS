//! The geometry pipeline: the vertex (object) function and the geometry (mesh) function.

use core::ffi::CStr;

use crate::{abi::ArgChain, api::Error, shader::Shader};

/// Compiles `vertex`, feeding `geometry`, as the function `name` into a metallib.
///
/// # Errors
///
/// What the translation refuses, or "not ported".
pub fn compile_gs_vertex(
    vertex: &Shader,
    geometry: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let _ = (vertex, geometry, args, name);
    Err(Error::not_ported())
}

/// Compiles `geometry`, fed by `vertex`, as the function `name` into a metallib.
///
/// # Errors
///
/// What the translation refuses, or "not ported".
pub fn compile_gs(
    vertex: &Shader,
    geometry: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let _ = (vertex, geometry, args, name);
    Err(Error::not_ported())
}
