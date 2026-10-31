//! The geometry pipeline: the vertex (object) function and the geometry (mesh) function.

use core::ffi::CStr;

use airconv_ir::shader_info::ShaderFlags;

use crate::{
    abi::ArgChain,
    api::Error,
    dxbc::{self, blocks},
    llvm::{Context, Module},
    module,
    shader::Shader,
    writer,
};

/// Compiles `vertex`, feeding `geometry`, as the function `name` into a metallib.
///
/// `SM50CompileGeometryPipelineVertex`: the object function, which runs the vertex shader for
/// a warp. The helpers it links are the geometry shader's, as the C++'s were.
///
/// # Errors
///
/// What the translation refuses, or "not ported" (DXIL shaders, until their lowering lands).
pub fn compile_gs_vertex(
    vertex: &Shader,
    geometry: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let (Shader::Dxbc(vs), Shader::Dxbc(gs)) = (vertex, geometry) else {
        // DXIL lowering (X2) replaces this arm.
        return Err(Error::not_ported());
    };
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    dxbc::gs::convert_vertex(&vs.ir, &gs.ir, name.to_bytes(), &m, args, |conv, entry| {
        blocks::convert(conv, &vs.ir.cfg.blocks, entry)
    })?;
    finish(&m, gs.ir.cfg.info.flags)
}

/// Compiles `geometry`, fed by `vertex`, as the function `name` into a metallib.
///
/// `SM50CompileGeometryPipelineGeometry`: the mesh function, which runs the geometry shader on
/// the vertices the object function left in the payload.
///
/// # Errors
///
/// What the translation refuses, or "not ported" (DXIL shaders, until their lowering lands).
pub fn compile_gs(
    vertex: &Shader,
    geometry: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let (Shader::Dxbc(vs), Shader::Dxbc(gs)) = (vertex, geometry) else {
        // DXIL lowering (X2) replaces this arm.
        return Err(Error::not_ported());
    };
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    dxbc::gs::convert_geometry(&gs.ir, &vs.ir, name.to_bytes(), &m, args, |conv, entry| {
        blocks::convert(conv, &gs.ir.cfg.blocks, entry)
    })?;
    finish(&m, gs.ir.cfg.info.flags)
}

/// The geometry shader's helpers linked, the passes run and the module written.
fn finish(m: &Module<'_>, flags: ShaderFlags) -> Result<Vec<u8>, Error> {
    module::link_helpers(
        m,
        flags.contains(ShaderFlags::USE_MSAD),
        flags.contains(ShaderFlags::USE_SAMPLEPOS),
        false,
    )?;
    module::optimize(m);
    #[cfg(feature = "oracle")]
    crate::oracle::derive(m)?;
    Ok(writer::write(m))
}
