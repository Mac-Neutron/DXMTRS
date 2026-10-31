//! The tessellation pipeline: the vertex-hull function and the domain function.

use core::ffi::CStr;

use crate::{abi::ArgChain, api::Error, shader::Shader};

/// Compiles `vertex` and `hull` as one function, `name`, into a metallib.
///
/// # Errors
///
/// What the translation refuses, or "not ported".
pub fn compile_hull(
    vertex: &Shader,
    hull: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let _ = (vertex, hull, args, name);
    Err(Error::not_ported())
}

/// Compiles `domain`, fed by `hull`, as the function `name` into a metallib.
///
/// # Errors
///
/// What the translation refuses, or "not ported".
pub fn compile_domain(
    hull: &Shader,
    domain: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let _ = (hull, domain, args, name);
    Err(Error::not_ported())
}
