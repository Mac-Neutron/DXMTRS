//! Compile: one shader's function, and the pass-through vertex function of a pixel shader.

use core::ffi::CStr;

use crate::{abi::ArgChain, api::Error, shader::Shader};

/// Compiles `shader` as the function `name`, with the chain's arguments, into a metallib.
///
/// # Errors
///
/// What the translation refuses, or "not ported".
pub fn compile(shader: &Shader, args: &ArgChain<'_>, name: &CStr) -> Result<Vec<u8>, Error> {
    let _ = (shader, args, name);
    Err(Error::not_ported())
}

/// A vertex function whose outputs are `pixel`'s inputs, so the pixel shader runs alone.
///
/// The test tools put a pixel shader in a pipeline with it.
///
/// # Errors
///
/// Not a DXIL pixel shader, or "not ported".
pub fn passthrough_vertex(pixel: &Shader) -> Result<Vec<u8>, Error> {
    let _ = pixel;
    Err(Error::not_ported())
}
