//! DXSO (D3D9 shader bytecode) parsing, shader keys, and semantic linking.
//!
//! Platform-independent pure Rust. Re-exports the `airconv_ir::dxso` parser
//! and IR types alongside the D3D9 fixed-function key types, sampler metadata
//! helpers, and SM3 semantic linkage helpers.

mod emit;
mod ff;
mod link;

use airconv_ir::dxso::{ir, opcode};

/// `log` target used by every call inside this module.
///
/// Consumed via the `log` crate macros (`warn!(target: LOG_TARGET, ...)`) and
/// by `log_once_warn!`. No logger is registered here; `mtld3d-core` is an rlib
/// statically linked into `d3d9.dll`, which owns the `env_logger` init; tests
/// without a registered logger simply no-op.
pub const LOG_TARGET: &str = "mtld3d::dxso";

pub use airconv_ir::dxso::{
    CmpFunc, DeclUsage, Declaration, DxsoError, DxsoProgram, DxsoWarning, RegKind, ShaderType,
    TextureType, operand_token_count, parse,
    spec::{VariantFlags, VariantKey, VsSamplerKinds},
};
pub use emit::{
    bound_sampler_type, declared_ps_samplers, declared_vs_samplers, explicit_lod_samplers,
};
pub use ff::{
    FF_PASSTHROUGH_ATTR_BASE, FF_VS_PALETTE_BASE_ROW, FfPsKey, FfStage, FfStageFlags,
    FfStageResult, FfVsFlags, FfVsKey, MAX_VERTEX_BLEND_MATRIX_INDEX, ff_attr_index_for_semantic,
    is_bump_env_op, tci_entry, tt_count, tt_projected,
};
pub use link::{LinkInputs, MAX_LINKED_INPUTS, SemanticSet, decl_passthrough_code};

/// The line a [`DxsoWarning`] logs.
fn warning_text(warning: DxsoWarning) -> String {
    match warning {
        DxsoWarning::LinkedInputsOverflow => format!(
            "dxso: PS declares more than {MAX_LINKED_INPUTS} extra input semantics → \
             the rest read zero"
        ),
        DxsoWarning::ColorOutBeyondTargets(index) => format!(
            "dxso: PS writes oC{index}, beyond the four D3D9 render targets → write ignored"
        ),
    }
}

/// Log a warning the parser's data functions returned.
///
/// Under [`LOG_TARGET`]: the overflow once, each colour-output index once.
pub fn log_warning(warning: DxsoWarning) {
    match warning {
        DxsoWarning::LinkedInputsOverflow => {
            mtld3d_shared::log_once_warn!(target: LOG_TARGET, "{}", warning_text(warning));
        }
        DxsoWarning::ColorOutBeyondTargets(index) => {
            mtld3d_shared::log_once_warn_by!(
                target: LOG_TARGET,
                key: u64::from(index),
                "{}",
                warning_text(warning)
            );
        }
    }
}

/// [`LinkInputs::ps_inputs`], its warning logged.
#[must_use]
pub fn link_inputs(ps: &DxsoProgram) -> LinkInputs {
    let (inputs, warning) = LinkInputs::ps_inputs(ps);
    warning.into_iter().for_each(log_warning);
    inputs
}

/// [`DxsoProgram::color_out_mask`], its warnings logged.
#[must_use]
pub fn color_out_mask(ps: &DxsoProgram) -> u8 {
    let (mask, warnings) = ps.color_out_mask();
    warnings.into_iter().for_each(log_warning);
    mask
}

#[cfg(test)]
mod tests;
