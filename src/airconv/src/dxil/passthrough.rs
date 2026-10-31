//! The pass-through vertex function of a DXIL pixel shader.
//!
//! Ported from `A/dxil/dxil_passthrough.cpp` (`BuildPassThroughVertex`): a vertex function whose
//! outputs are the pixel shader's arbitrary inputs, all zero, and the position (0, 0, 0, 1);
//! `dxil-translate` puts a pixel shader in a Metal pipeline with it.

use airconv_ir::binding::{FunctionOutput, FunctionSignatureDesc, MslValue};

use super::{
    signature::varying,
    types::{EntryInfo, semantic_kind},
};
use crate::{
    abi::SM50_SHADER_METAL_310,
    air::{
        metadata::{add_entry, set_metal_version},
        signature::{Handles, create_function},
    },
    api::Error,
    dxbc::converter::invalid,
    llvm::{Builder, Context, Module},
    module,
};

/// The vertex function `vs_passthrough` feeding `pixel`'s inputs, in a new AIR module of `cx`.
///
/// # Errors
///
/// An input name that is not UTF-8 (`varying`'s), what the function's creation refuses.
pub fn build<'cx>(pixel: &EntryInfo, cx: &'cx Context) -> Result<Module<'cx>, Error> {
    let m = module::initialize(cx, c"shader.air");
    set_metal_version(&m, SM50_SHADER_METAL_310);
    let mut desc = FunctionSignatureDesc::default();
    desc.define_output(FunctionOutput::Position {
        ty: MslValue::FLOAT4,
    });
    for e in &pixel.inputs {
        if e.kind == semantic_kind::ARBITRARY {
            for r in 0..e.rows {
                let (user, ty) = varying(e, r)?;
                desc.define_output(FunctionOutput::Vertex { user, ty });
            }
        }
    }
    let (function, entry) =
        create_function(&m, &desc, b"vs_passthrough", false, &Handles::default())?;
    let block = function
        .append_block(cx, c"entry")
        .ok_or_else(|| invalid("a block of no function"))?;
    let b = Builder::new(cx);
    b.position_at_end(block);
    let tcx = cx.tcx();
    let ret = tcx
        .ret(function.fn_ty())
        .ok_or_else(|| invalid("a function of no return type"))?;
    let zero = cx.null(ret);
    let f = |v| cx.const_real(tcx.float(), v);
    let position = cx.const_vector(&[f(0.0), f(0.0), f(0.0), f(1.0)]);
    b.ret(b.insert_value(zero, position, 0));
    add_entry(&m, c"air.vertex", entry);
    Ok(m)
}
