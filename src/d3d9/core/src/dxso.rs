//! DXSO (D3D9 shader bytecode) parsing and → MSL translation.
//!
//! Platform-independent pure Rust. The `parser` submodule turns a `&[u32]` of
//! DXSO bytecode into a structured `DxsoProgram`; the MSL emitters
//! (`emit_vs_programmable` / `emit_ps_programmable` for DXSO and `emit_vs_ff`
//! / `emit_ps_ff` for fixed-function) each produce a standalone translation
//! unit containing a single `mtld3d_vs` or `mtld3d_ps` entry point. A render
//! pipeline can mix any VS source with any PS source.

mod emit;
mod ff;
mod link;

use airconv_ir::dxso::{ir, opcode};

/// `log` target used by every call inside this module.
///
/// Consumed via the `log` crate macros (`warn!(target: LOG_TARGET, …)`) and
/// by `log_once_warn!`. No logger is registered here — mtld3d-core is an rlib
/// statically linked into `d3d9.dll`, which owns the `env_logger` init; tests
/// without a registered logger simply no-op.
pub const LOG_TARGET: &str = "mtld3d::dxso";

pub use airconv_ir::dxso::{
    CmpFunc, DeclUsage, Declaration, DxsoError, DxsoProgram, DxsoWarning, RegKind, ShaderType,
    TextureType, operand_token_count, parse,
    spec::{VariantFlags, VariantKey, VsSamplerKinds},
};
pub use emit::{
    DEFAULT_PS_ENTRY, DEFAULT_VS_ENTRY, EmitError, bound_sampler_type, declared_ps_samplers,
    emit_ps_programmable, emit_ps_programmable_named, emit_vs_programmable,
    emit_vs_programmable_named, explicit_lod_samplers,
};
pub use ff::{
    FF_PASSTHROUGH_ATTR_BASE, FF_VS_PALETTE_BASE_ROW, FfPsKey, FfStage, FfStageFlags,
    FfStageResult, FfVsFlags, FfVsKey, MAX_VERTEX_BLEND_MATRIX_INDEX, emit_ps_ff, emit_ps_ff_named,
    emit_vs_ff, emit_vs_ff_named, ff_attr_index_for_semantic, is_bump_env_op, tci_entry,
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
