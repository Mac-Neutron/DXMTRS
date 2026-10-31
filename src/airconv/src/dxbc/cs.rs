//! The compute shader's function.
//!
//! [`convert`] is `convert_dxbc_compute_shader` (`A/dxbc_converter.cpp:474-575`), in its order:
//! the bindings (a root signature's when the chain has one, else D3D11's tables), threadgroup
//! memory, the function, the version metadata, the temporaries and the immediate constant
//! buffer, the thread ids the signature handlers ask for, the blocks, and the entry under
//! `air.kernel`.

use airconv_ir::{handlers::SignatureHandler, initialize::DxbcShader};

use super::{
    binding_rootsig::setup_bindings,
    blocks,
    converter::Converter,
    resources::{Resources, setup_immediate_constant_buffer, setup_temp_register, setup_tgsm},
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

/// Lowers the compute shader `shader` into `m` as the kernel `name`.
///
/// # Errors
///
/// An unreadable root signature, a handler no compute shader records, and what the lowering
/// refuses.
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
    let (metal_version, shader_flags) = args
        .common
        .map_or((SM50_SHADER_METAL_310, 0), |c| (c.metal_version, c.flags));

    let mut res = Resources::default();
    let types = AirTypes::new(tcx);
    let mut handles = Handles::default();

    let binding = setup_bindings(shader, &mut desc, m, &mut handles, args)?;
    setup_tgsm(info, &mut res, &types, m)?;

    let (function, entry) = create_function(m, &desc, name, false, &handles)?;
    let no_block = || Error::new(b"invalid operation: a block of no function.");
    let entry_bb = function.append_block(cx, c"entry").ok_or_else(no_block)?;
    let epilogue_bb = function
        .append_block(cx, c"epilogue")
        .ok_or_else(no_block)?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, m, &types, shader_flags);

    set_metal_version(m, metal_version);
    setup_temp_register(info, &mut res, &types, &b);
    setup_immediate_constant_buffer(info, &mut res, &types, m, &b)?;

    // The prologue: the thread ids the handlers recorded.
    for handler in &shader.io.signature_handlers {
        let (slot, index) = match *handler {
            SignatureHandler::ThreadId { index } => (&mut res.thread_id_arg, index),
            SignatureHandler::ThreadGroupId { index } => (&mut res.thread_group_id_arg, index),
            SignatureHandler::ThreadIdInGroup { index } => (&mut res.thread_id_in_group_arg, index),
            SignatureHandler::ThreadIdInGroupFlattened { index } => {
                (&mut res.thread_id_in_group_flat_arg, index)
            }
            _ => {
                return Err(Error::new(
                    b"invalid operation: a compute shader with another stage's signature.",
                ));
            }
        };
        *slot = Some(
            function
                .param(tcx, index)
                .ok_or_else(|| Error::new(b"invalid operation: a handler of no argument."))?,
        );
    }

    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: binding.as_ref(),
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: shader.program_type,
        metal_version,
        geometry: None,
    };
    let real_entry = blocks::convert(&conv, &shader.cfg.blocks, epilogue_bb)?;
    b.br(real_entry);
    b.position_at_end(epilogue_bb);
    b.ret_void();

    add_entry(m, c"air.kernel", entry);
    Ok(())
}
