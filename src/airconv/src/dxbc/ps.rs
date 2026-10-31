//! The pixel shader's function.
//!
//! [`convert`] is `convert_dxbc_pixel_shader` (`A/dxbc_converter.cpp:315-473`), in its order:
//! the handlers' pass with the pipeline's pixel node (render target formats, dual-source
//! blending, depth output, unorm fix-up), the pipeline's sample mask as a coverage output, the
//! bindings, the function, its registers, the prologue, the blocks, the epilogue (the sample
//! mask inserted unless the shader writes its own coverage), and the entry under `air.fragment`.

use airconv_ir::{binding::FunctionOutput, initialize::DxbcShader};

use super::{
    binding_rootsig::setup_bindings,
    blocks,
    converter::{Converter, invalid},
    replay::{self, Options, zero_return},
    resources::{
        Resources, setup_immediate_constant_buffer, setup_io_registers, setup_temp_register,
    },
};
use crate::{
    abi::{ArgChain, SM50_SHADER_METAL_310},
    air::{
        builder::AirBuilder,
        metadata::{add_entry, set_metal_version},
        signature::{Handles, create_function},
        ty::AirTypes,
    },
    api::Error,
    llvm::{Builder, Module},
};

/// Lowers the pixel shader `shader` into `m` as the fragment function `name`.
///
/// # Errors
///
/// An unreadable root signature, an input or render target of an unknown component type, and
/// what the lowering refuses.
pub fn convert(
    shader: &DxbcShader,
    name: &[u8],
    m: &Module<'_>,
    args: &ArgChain<'_>,
) -> Result<(), Error> {
    let cx = m.cx();
    let tcx = cx.tcx();
    let mut desc = shader.io.func_signature.clone();
    let info = &shader.cfg.info;
    let io = &shader.io;
    let pso_sample_mask = args.pixel.map_or(u32::MAX, |p| p.sample_mask);
    let (metal_version, shader_flags) = args
        .common
        .map_or((SM50_SHADER_METAL_310, 0), |c| (c.metal_version, c.flags));

    let mut res = Resources::default();
    let types = AirTypes::new(tcx);
    let mut handles = Handles::default();

    let replay = replay::record(
        &io.signature_handlers,
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: args.pixel,
            pull_mode_reg_mask: info.pull_mode_reg_mask,
            skip_vertex_output: false,
        },
    )?;
    let sample_mask_out =
        (pso_sample_mask != u32::MAX).then(|| desc.define_output(FunctionOutput::CoverageMask));

    let binding = setup_bindings(shader, &mut desc, m, &mut handles, args)?;

    let (function, entry) = create_function(m, &desc, name, false, &handles)?;
    let no_block = || invalid("a block of no function");
    let entry_bb = function.append_block(cx, c"entry").ok_or_else(no_block)?;
    let epilogue_bb = function
        .append_block(cx, c"epilogue")
        .ok_or_else(no_block)?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, m, &types, shader_flags);

    set_metal_version(m, metal_version);

    setup_io_registers(
        &mut res,
        &types,
        &b,
        io.max_input_register,
        io.max_output_register,
    );
    setup_temp_register(info, &mut res, &types, &b);
    setup_immediate_constant_buffer(info, &mut res, &types, m, &b)?;

    replay.prologue(&air, function, &mut res)?;
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: binding.as_ref(),
        res: &res,
        types: &types,
        pso_sample_mask,
        shader_type: shader.program_type,
        metal_version,
        geometry: None,
    };
    let real_entry = blocks::convert(&conv, &shader.cfg.blocks, epilogue_bb)?;
    b.br(real_entry);

    b.position_at_end(epilogue_bb);
    let mut value = replay.epilogue(&air, &res, zero_return(cx, function), pso_sample_mask)?;
    if let Some(index) = sample_mask_out
        && res.coverage_mask_reg.is_none()
    {
        let v = value.ok_or_else(|| invalid("an output of a function that returns void"))?;
        value = Some(b.insert_value(v, air.get_int(pso_sample_mask), index));
    }
    value.map_or_else(|| b.ret_void(), |v| b.ret(v));

    add_entry(m, c"air.fragment", entry);
    Ok(())
}
