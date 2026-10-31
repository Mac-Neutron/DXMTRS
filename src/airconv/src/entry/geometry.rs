//! The geometry pipeline: the vertex (object) function and the geometry (mesh) function.

use core::ffi::CStr;

use airconv_ir::shader_info::ShaderFlags;

use crate::{
    abi::ArgChain,
    api::Error,
    dxbc::{self, blocks, gs::Stage},
    dxil::{self, types::shader_kind},
    llvm::{Context, Module},
    module,
    shader::Shader,
    writer,
};

/// Compiles `vertex`, feeding `geometry`, as the function `name` into a metallib.
///
/// `SM50CompileGeometryPipelineVertex`: the object function, which runs the vertex shader for
/// a warp. A DXBC pipeline links the geometry shader's helpers, as the C++'s did; a DXIL one
/// (either shader DXIL) also the vertex shader's, under DXIL's float policy.
///
/// # Errors
///
/// What the translation refuses; a pipeline of one DXIL and one DXBC shader is the DXIL
/// front end's refusal.
pub fn compile_gs_vertex(
    vertex: &Shader,
    geometry: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let cx = Context::new();
    let (Shader::Dxbc(vs), Shader::Dxbc(gs)) = (vertex, geometry) else {
        let m = dxil::converter::convert_geometry_pipeline(
            true,
            vertex,
            geometry,
            name.to_bytes(),
            &cx,
            args,
        )?;
        let msad = flags(geometry).contains(ShaderFlags::USE_MSAD)
            || flags(vertex).contains(ShaderFlags::USE_MSAD);
        let samplepos = flags(geometry).contains(ShaderFlags::USE_SAMPLEPOS)
            || flags(vertex).contains(ShaderFlags::USE_SAMPLEPOS);
        return finish_dxil(&m, msad, samplepos);
    };
    let m = module::initialize(&cx, c"shader.air");
    dxbc::gs::convert_vertex(
        Stage::Dxbc(&vs.ir),
        Stage::Dxbc(&gs.ir),
        name.to_bytes(),
        &m,
        args,
        |conv, entry| blocks::convert(conv, &vs.ir.cfg.blocks, entry),
    )?;
    finish(&m, gs.ir.cfg.info.flags)
}

/// Compiles `geometry`, fed by `vertex`, as the function `name` into a metallib.
///
/// `SM50CompileGeometryPipelineGeometry`: the mesh function, which runs the geometry shader on
/// the vertices the object function left in the payload. It links the geometry shader's
/// helpers, under DXIL's float policy in a DXIL pipeline.
///
/// # Errors
///
/// What the translation refuses; a pipeline of one DXIL and one DXBC shader is the DXIL
/// front end's refusal.
pub fn compile_gs(
    vertex: &Shader,
    geometry: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let cx = Context::new();
    let (Shader::Dxbc(vs), Shader::Dxbc(gs)) = (vertex, geometry) else {
        let m = dxil::converter::convert_geometry_pipeline(
            false,
            vertex,
            geometry,
            name.to_bytes(),
            &cx,
            args,
        )?;
        let gs_flags = flags(geometry);
        return finish_dxil(
            &m,
            gs_flags.contains(ShaderFlags::USE_MSAD),
            gs_flags.contains(ShaderFlags::USE_SAMPLEPOS),
        );
    };
    let m = module::initialize(&cx, c"shader.air");
    dxbc::gs::convert_geometry(
        Stage::Dxbc(&gs.ir),
        Stage::Dxbc(&vs.ir),
        name.to_bytes(),
        &m,
        args,
        |conv, entry| blocks::convert(conv, &gs.ir.cfg.blocks, entry),
    )?;
    finish(&m, gs.ir.cfg.info.flags)
}

/// A DXIL pipeline's helpers linked, the passes run and the module written.
///
/// The helpers keep DXIL's float policy, the geometry stage's.
fn finish_dxil(m: &Module<'_>, msad: bool, samplepos: bool) -> Result<Vec<u8>, Error> {
    module::link_helpers(m, msad, samplepos, false)?;
    if msad || samplepos {
        dxil::converter::strip_fast_math(m, shader_kind::GEOMETRY);
    }
    module::optimize(m);
    Ok(writer::write(m))
}

/// A shader's helper flags, of either front end.
const fn flags(shader: &Shader) -> ShaderFlags {
    match shader {
        Shader::Dxbc(s) => s.ir.cfg.info.flags,
        Shader::Dxil(s) => s.ir.info.flags,
    }
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
    Ok(writer::write(m))
}
