//! Compile: one shader's function, and the pass-through vertex function of a pixel shader.

use core::ffi::CStr;

use airconv_ir::{shader_info::ShaderFlags, tokens::program_type};

use crate::{
    abi::ArgChain,
    api::Error,
    dxbc, dxil,
    dxil::types::shader_kind,
    llvm::{Context, Module},
    module,
    shader::Shader,
    writer,
};

/// Compiles `shader` as the function `name`, with the chain's arguments, into a metallib.
///
/// `SM50Compile`: a context and a module per compile, the stage's lowering (`convertDXBC`, or
/// the DXIL front end's module), the Metal helpers it calls (under DXIL's float policy for a
/// DXIL shader), the pass pipeline, then the metallib. With the feature `oracle` the optimized
/// module's pointers are checked against the facts opaque IR keeps.
///
/// # Errors
///
/// What the translation refuses, and the C++'s texts for a stage that is not compiled alone.
pub fn compile(shader: &Shader, args: &ArgChain<'_>, name: &CStr) -> Result<Vec<u8>, Error> {
    let dxbc = match shader {
        Shader::Dxbc(dxbc) => dxbc,
        Shader::Dxil(d) => {
            let cx = Context::new();
            let m = dxil::converter::convert(&d.ir, name.to_bytes(), &cx, args)?;
            let flags = d.ir.info.flags;
            let msad = flags.contains(ShaderFlags::USE_MSAD);
            let samplepos = flags.contains(ShaderFlags::USE_SAMPLEPOS);
            module::link_helpers(&m, msad, samplepos, false)?;
            if msad || samplepos {
                dxil::converter::strip_fast_math(&m, d.ir.entry.kind);
            }
            return finish(&m);
        }
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
    finish(&m)
}

/// A vertex function whose outputs are `pixel`'s inputs, so the pixel shader runs alone.
///
/// The test tools put a pixel shader in a pipeline with it.
///
/// # Errors
///
/// `DXIL: not a DXIL pixel shader`, and an input name that is not UTF-8.
pub fn passthrough_vertex(pixel: &Shader) -> Result<Vec<u8>, Error> {
    let entry = match pixel {
        Shader::Dxil(d) if d.ir.entry.kind == shader_kind::PIXEL => &d.ir.entry,
        _ => return Err(Error::new(b"DXIL: not a DXIL pixel shader")),
    };
    let cx = Context::new();
    let m = dxil::passthrough::build(entry, &cx)?;
    finish(&m)
}

/// The pass pipeline run and the module written (checked first with the feature `oracle`).
fn finish(m: &Module<'_>) -> Result<Vec<u8>, Error> {
    module::optimize(m);
    #[cfg(feature = "oracle")]
    crate::oracle::derive(m)?;
    Ok(writer::write(m))
}
