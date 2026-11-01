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
/// DXIL shader), the pass pipeline, then the metallib.
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
            return finish(&m, d.ir.info.flags, Some(d.ir.entry.kind));
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
    finish(&m, dxbc.ir.cfg.info.flags, None)
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
    finish(&m, ShaderFlags::default(), None)
}

/// The Metal helpers `flags` ask for linked, the pass pipeline run and the module written.
///
/// A DXIL shader (its kind `dxil_kind`) keeps DXIL's float policy in the helpers.
fn finish(m: &Module<'_>, flags: ShaderFlags, dxil_kind: Option<u32>) -> Result<Vec<u8>, Error> {
    let msad = flags.contains(ShaderFlags::USE_MSAD);
    let samplepos = flags.contains(ShaderFlags::USE_SAMPLEPOS);
    let refs = module::link_helpers(m, msad, samplepos, false)?;
    if let Some(kind) = dxil_kind
        && (msad || samplepos)
    {
        dxil::converter::strip_fast_math(m, kind);
    }
    module::optimize(m);
    writer::write(m, &refs)
}
