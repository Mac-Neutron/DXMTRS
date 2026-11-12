//! Synchronous shader validation and metadata extraction.
//!
//! The caller owns failure reporting and HRESULT translation. Successful programs are parsed
//! once and remain owned by the runtime that performed creation.

use mtld3d_shared::shader_create::{ShaderInputSemantic, ShaderStage, ShaderUsage};

use crate::{
    dxso::{
        Declaration, DxsoError, DxsoProgram, LinkInputs, RegKind, SemanticSet, ShaderType,
        color_out_mask, declared_ps_samplers, link_inputs, parse,
    },
    ids::ProgramId,
};

/// Validated program and the metadata needed by the API wrapper.
pub struct ParsedShader {
    pub program: DxsoProgram,
    pub id: ProgramId,
    pub max_const_used: u32,
    pub usage: ShaderUsage,
    pub color_out_mask: u8,
    pub input_semantics: Vec<ShaderInputSemantic>,
}

/// Creation failures translated to INVALIDCALL by the API boundary.
#[derive(Debug)]
pub enum ShaderCreateError {
    Parse(DxsoError),
    WrongStage,
    InvalidPixelInput,
    ConstantRegisterLimit,
}

/// Parse once, validate the requested stage, and extract synchronous API metadata.
///
/// # Panics
///
/// Panics if the parser emits a semantic usage index wider than its four-bit DXSO field.
///
/// # Errors
///
/// Returns the parser error or the first stage-specific validation failure. Pixel input
/// validation precedes constant-register validation, matching the API creation contract.
pub fn parse_shader(
    stage: &ShaderStage,
    tokens: &[u32],
) -> Result<ParsedShader, ShaderCreateError> {
    let program = parse(tokens).map_err(ShaderCreateError::Parse)?;
    let expected = match stage {
        ShaderStage::Vertex => ShaderType::Vertex,
        ShaderStage::Pixel => ShaderType::Pixel,
    };
    if program.shader_type != expected {
        return Err(ShaderCreateError::WrongStage);
    }
    if expected == ShaderType::Pixel && program.has_invalid_pixel_input_decl() {
        return Err(ShaderCreateError::InvalidPixelInput);
    }
    if program.violates_constant_register_limits() {
        return Err(ShaderCreateError::ConstantRegisterLimit);
    }
    let max_const_used = program.max_const_reg().map_or(0, |m| u32::from(m) + 1);
    let mut usage = ShaderUsage::empty();
    usage.set(ShaderUsage::INT_CONST, program.uses_dynamic_int_constants());
    usage.set(
        ShaderUsage::BOOL_CONST,
        program.uses_dynamic_bool_constants(),
    );
    usage.set(
        ShaderUsage::RELATIVE_CONST,
        program.uses_relative_const_addressing(),
    );
    let mut input_semantics = Vec::new();
    let color_out_mask = match stage {
        ShaderStage::Vertex => {
            for decl in &program.declarations {
                if let Declaration::Semantic {
                    usage,
                    usage_index,
                    reg,
                    ..
                } = decl
                    && reg.kind == RegKind::Input
                {
                    input_semantics.push(ShaderInputSemantic {
                        usage: *usage,
                        usage_index: u8::try_from(*usage_index)
                            .expect("DXSO usage index is a four-bit field"),
                        register_index: reg.index,
                    });
                }
            }
            0
        }
        ShaderStage::Pixel => {
            usage.set(ShaderUsage::AUTOMATIC_FOG, program.major < 3);
            usage.set(ShaderUsage::BUMP_ENV, program.uses_bump_env());
            usage.set(
                ShaderUsage::LINKED_INPUTS,
                !link_inputs(&program).is_empty(),
            );
            color_out_mask(&program)
        }
    };
    Ok(ParsedShader {
        id: ProgramId::from_tokens(tokens),
        program,
        max_const_used,
        usage,
        color_out_mask,
        input_semantics,
    })
}

/// What the core runtime's draws need of a program, derived once at creation.
///
/// What mtld3d's encoder derives when it registers a program (`register_program`), for a device
/// that has no encoder: the core keys and refuses its draws on these.
pub struct CoreShaderMeta {
    /// [`Self::samples`] and [`Self::bump_env`].
    pub flags: CoreMetaFlags,
    /// A `ps_3_0`'s extra input semantics; empty for any other program.
    pub link_inputs: LinkInputs,
    /// A `vs_3_0`'s extra output semantics; empty for any other program.
    pub link_outputs: SemanticSet,
    /// The sampler slots the program declares (`dcl_<dim> sN`, SM1's sampled stages), s0-s15.
    pub sampler_mask: u16,
    /// The declared slots sampled at an explicit level (`texldl`), whose clamp the shader applies.
    pub explicit_lod_mask: u16,
}

bitflags::bitflags! {
    /// What a program uses that the core runtime's draws refuse.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct CoreMetaFlags: u8 {
        /// The program declares a sampler (SM1: samples a stage), which the core cannot bind yet.
        const SAMPLES = 1 << 0;
        /// The program reads the bump-env rows (`bem`, `texbem`, `texbeml`).
        const BUMP_ENV = 1 << 1;
    }
}

impl CoreShaderMeta {
    /// The program declares a sampler (SM1: samples a stage).
    #[must_use]
    pub const fn samples(&self) -> bool {
        self.flags.contains(CoreMetaFlags::SAMPLES)
    }

    /// The program reads the bump-env rows.
    #[must_use]
    pub const fn bump_env(&self) -> bool {
        self.flags.contains(CoreMetaFlags::BUMP_ENV)
    }
}

/// The [`CoreShaderMeta`] of `program`.
#[must_use]
pub fn core_meta(program: &DxsoProgram) -> CoreShaderMeta {
    let mut flags = CoreMetaFlags::empty();
    flags.set(
        CoreMetaFlags::SAMPLES,
        !declared_ps_samplers(program).is_empty(),
    );
    flags.set(CoreMetaFlags::BUMP_ENV, program.uses_bump_env());
    let sampler_mask = declared_ps_samplers(program)
        .keys()
        .filter(|&&slot| slot < 16)
        .fold(0u16, |mask, &slot| mask | 1 << slot);
    CoreShaderMeta {
        flags,
        link_inputs: link_inputs(program),
        link_outputs: SemanticSet::vs_outputs(program),
        sampler_mask,
        explicit_lod_mask: crate::dxso::explicit_lod_samplers(program) & sampler_mask,
    }
}

#[cfg(test)]
mod tests;
