//! The tessellation pipeline: the vertex-hull function and the domain function.

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

/// Compiles `vertex` and `hull` as one function, `name`, into a metallib.
///
/// `SM50CompileTessellationPipelineHull`: the vertex-hull lowering, the Metal helpers the hull
/// shader calls (msad, sample positions) and the tessellation helpers, then the pass pipeline.
///
/// # Errors
///
/// Refusals from stage checks, bitcode loading, lowering, helper linking, or verification.
pub fn compile_hull(
    vertex: &Shader,
    hull: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let cx = Context::new();
    let (Shader::Dxbc(vs), Shader::Dxbc(hs)) = (vertex, hull) else {
        let m =
            dxil::converter::convert_tessellation_hull(vertex, hull, name.to_bytes(), &cx, args)?;
        let vs_flags = super::geometry::flags(vertex);
        let hs_flags = super::geometry::flags(hull);
        return finish_dxil(
            &m,
            vs_flags.contains(ShaderFlags::USE_MSAD) || hs_flags.contains(ShaderFlags::USE_MSAD),
            vs_flags.contains(ShaderFlags::USE_SAMPLEPOS)
                || hs_flags.contains(ShaderFlags::USE_SAMPLEPOS),
            shader_kind::HULL,
        );
    };
    let m = module::initialize(&cx, c"shader.air");
    dxbc::ts::convert_hull(
        Stage::Dxbc(&vs.ir),
        Stage::Dxbc(&hs.ir),
        |conv, epilogue| blocks::convert(conv, &vs.ir.cfg.blocks, epilogue),
        |conv, epilogue| blocks::convert(conv, &hs.ir.cfg.blocks, epilogue),
        name.to_bytes(),
        &m,
        args,
    )?;
    finish(&m, hs.ir.cfg.info.flags)
}

/// Compiles `domain`, fed by `hull`, as the function `name` into a metallib.
///
/// `SM50CompileTessellationPipelineDomain`: the domain lowering, the Metal helpers the domain
/// shader calls and the tessellation helpers, then the pass pipeline.
///
/// # Errors
///
/// Refusals from stage checks, bitcode loading, lowering, helper linking, or verification.
pub fn compile_domain(
    hull: &Shader,
    domain: &Shader,
    args: &ArgChain<'_>,
    name: &CStr,
) -> Result<Vec<u8>, Error> {
    let cx = Context::new();
    let (Shader::Dxbc(hs), Shader::Dxbc(ds)) = (hull, domain) else {
        let m =
            dxil::converter::convert_tessellation_domain(domain, hull, name.to_bytes(), &cx, args)?;
        let ds_flags = super::geometry::flags(domain);
        return finish_dxil(
            &m,
            ds_flags.contains(ShaderFlags::USE_MSAD),
            ds_flags.contains(ShaderFlags::USE_SAMPLEPOS),
            shader_kind::DOMAIN,
        );
    };
    let m = module::initialize(&cx, c"shader.air");
    dxbc::ts::convert_domain(
        Stage::Dxbc(&ds.ir),
        Stage::Dxbc(&hs.ir),
        name.to_bytes(),
        &m,
        args,
        |conv, epilogue| blocks::convert(conv, &ds.ir.cfg.blocks, epilogue),
    )?;
    finish(&m, ds.ir.cfg.info.flags)
}

/// Link `msad`, `samplepos`, and tessellation Metal helpers, strip fast-math, and emit metallib.
fn finish_dxil(m: &Module<'_>, msad: bool, samplepos: bool, kind: u32) -> Result<Vec<u8>, Error> {
    let refs = module::link_helpers(m, msad, samplepos, true)?;
    dxil::converter::strip_fast_math(m, kind);
    module::optimize(m)?;
    writer::write(m, &refs)
}

/// The helpers `flags` ask for and the tessellation ones, the passes, the metallib.
fn finish(m: &Module<'_>, flags: ShaderFlags) -> Result<Vec<u8>, Error> {
    let refs = module::link_helpers(
        m,
        flags.contains(ShaderFlags::USE_MSAD),
        flags.contains(ShaderFlags::USE_SAMPLEPOS),
        true,
    )?;
    module::optimize(m)?;
    writer::write(m, &refs)
}
