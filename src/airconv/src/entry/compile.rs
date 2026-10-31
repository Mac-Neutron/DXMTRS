//! Compile: one shader's function, and the pass-through vertex function of a pixel shader.

use core::ffi::CStr;

use airconv_ir::{shader_info::ShaderFlags, tokens::program_type};

use crate::{abi::ArgChain, api::Error, dxbc, llvm::Context, module, shader::Shader, writer};

/// Compiles `shader` as the function `name`, with the chain's arguments, into a metallib.
///
/// `SM50Compile`: a context and a module per compile, the stage's lowering (`convertDXBC`), the
/// Metal helpers it calls, the pass pipeline, then the metallib. With the feature `oracle` the
/// optimized module's pointers are checked against the facts opaque IR keeps.
///
/// # Errors
///
/// What the translation refuses, the C++'s texts for a stage that is not compiled alone, or
/// "not ported" (DXIL shaders, until their lowering lands).
pub fn compile(shader: &Shader, args: &ArgChain<'_>, name: &CStr) -> Result<Vec<u8>, Error> {
    let dxbc = match shader {
        Shader::Dxbc(dxbc) => dxbc,
        // DXIL lowering (X2) replaces this arm.
        Shader::Dxil(_) => return Err(Error::not_ported()),
    };
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    match dxbc.ir.program_type {
        program_type::COMPUTE => dxbc::cs::convert(&dxbc.ir, name.to_bytes(), &m, args)?,
        program_type::PIXEL => dxbc::ps::convert(&dxbc.ir, name.to_bytes(), &m, args)?,
        program_type::VERTEX => dxbc::vs::convert(&dxbc.ir, name.to_bytes(), &m, args)?,
        program_type::HULL | program_type::DOMAIN => {
            return Err(Error::new(
                b"Hull and domain shader cannot be independently converted.",
            ));
        }
        program_type::GEOMETRY => {
            return Err(Error::new(
                b"Geometry shader cannot be independently converted.",
            ));
        }
        _ => return Err(Error::new(b"Not supported shader type")),
    }
    let flags = dxbc.ir.cfg.info.flags;
    module::link_helpers(
        &m,
        flags.contains(ShaderFlags::USE_MSAD),
        flags.contains(ShaderFlags::USE_SAMPLEPOS),
        false,
    )?;
    module::optimize(&m);
    #[cfg(feature = "oracle")]
    crate::oracle::derive(&m)?;
    Ok(writer::write(&m))
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
