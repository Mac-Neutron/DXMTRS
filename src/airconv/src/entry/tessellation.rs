//! The tessellation pipeline: the vertex-hull function and the domain function.

use core::ffi::CStr;

use airconv_ir::shader_info::ShaderFlags;

use crate::{
    abi::ArgChain,
    api::Error,
    dxbc,
    llvm::{Context, Module},
    module,
    shader::Shader,
    writer,
};

/// Compiles `vertex` and `hull` as one function, `name`, into a metallib.
///
/// `SM50CompileTessellationPipelineHull`: the vertex-hull lowering, the Metal helpers the hull
/// shader calls (msad, sample positions) and the tessellation helpers, then the pass pipeline.
///
/// # Errors
///
/// What the translation refuses, or "not ported" (DXIL shaders, until their lowering lands).
pub fn compile_hull(
    vertex: &Shader,
    hull: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let (Shader::Dxbc(vertex), Shader::Dxbc(hull)) = (vertex, hull) else {
        // DXIL lowering (X2) replaces this arm.
        return Err(Error::not_ported());
    };
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    dxbc::ts::convert_hull(&vertex.ir, &hull.ir, name.to_bytes(), &m, args)?;
    finish(&m, hull.ir.cfg.info.flags)
}

/// Compiles `domain`, fed by `hull`, as the function `name` into a metallib.
///
/// `SM50CompileTessellationPipelineDomain`: the domain lowering, the Metal helpers the domain
/// shader calls and the tessellation helpers, then the pass pipeline.
///
/// # Errors
///
/// What the translation refuses, or "not ported" (DXIL shaders, until their lowering lands).
pub fn compile_domain(
    hull: &Shader,
    domain: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let (Shader::Dxbc(hull), Shader::Dxbc(domain)) = (hull, domain) else {
        // DXIL lowering (X2) replaces this arm.
        return Err(Error::not_ported());
    };
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    dxbc::ts::convert_domain(&domain.ir, &hull.ir, name.to_bytes(), &m, args)?;
    finish(&m, domain.ir.cfg.info.flags)
}

/// The helpers `flags` ask for and the tessellation ones, the passes, the metallib.
fn finish(m: &Module<'_>, flags: ShaderFlags) -> Result<Vec<u8>, Error> {
    module::link_helpers(
        m,
        flags.contains(ShaderFlags::USE_MSAD),
        flags.contains(ShaderFlags::USE_SAMPLEPOS),
        true,
    )?;
    module::optimize(m);
    #[cfg(feature = "oracle")]
    crate::oracle::derive(m)?;
    Ok(writer::write(m))
}
