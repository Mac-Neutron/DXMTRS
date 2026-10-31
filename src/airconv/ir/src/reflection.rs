//! A shader's reflection, as `MTL_SHADER_REFLECTION` reports it.
//!
//! [`Reflection`] holds the fields `SM50Initialize` writes for a DXBC shader
//! (`A/dxbc_converter.cpp:1252-1329`): the common ones and the stage's part of the union, which a
//! stage writes alone. The geometry shader's pass-through test (`CheckGSBBIsPassThrough`,
//! `CheckGSSignatureIsPassThrough`, `:900-980`) and the semantic kind `DXILGetEntry` gives a DXBC
//! element (`A/dxil/dxil_entry.cpp:69-93`) are here too.

use crate::{
    container::Error,
    ir::{BasicBlock, Target},
    signature::{OutputSignature, Signature},
    tokens::sb_name,
};

/// A geometry shader whose blocks branch in a cycle without leaving (the C++ recursed forever).
pub const GS_LOOPS_FOREVER: Error =
    Error::fail("Invalid DXBC bytecode: a geometry shader whose blocks loop forever");

/// What `SM50Initialize` writes into a DXBC shader's `MTL_SHADER_REFLECTION`.
#[derive(Debug, PartialEq)]
pub struct Reflection {
    /// 29 with constant buffers, else `u32::MAX`.
    pub constant_buffer_table_bind_index: u32,
    /// 30 with other arguments, else `u32::MAX`.
    pub argument_buffer_bind_index: u32,
    pub num_constant_buffers: u32,
    pub num_arguments: u32,
    pub stage: StageReflection,
    pub constant_buffer_slot_mask: u16,
    pub sampler_slot_mask: u16,
    pub uav_slot_mask: u64,
    /// SRV slots 64 and up, one bit each from slot 64.
    pub srv_slot_mask_hi: u64,
    pub srv_slot_mask_lo: u64,
    pub num_output_element: u32,
    pub argument_table_qwords: u32,
    /// 1 when the shader declares a UAV.
    pub writes_uav: u32,
}

/// The stage's part of the reflection: the union and, for a hull shader, `ThreadsPerPatch`.
#[derive(Debug, PartialEq)]
pub enum StageReflection {
    /// A vertex shader, or a stage the C++ writes no part for.
    None,
    Compute {
        threadgroup_size: [u32; 3],
    },
    Hull {
        threads_per_patch: u32,
        partition: u32,
        max_factor: f32,
        output_primitive: u32,
    },
    Domain {
        max_potential_tess_factor: u32,
    },
    /// A geometry shader's part.
    ///
    /// `pass_through` is the union's first word as bytes: `MTL_GEOMETRY_SHADER_PASS_THROUGH`'s
    /// four registers and components, or `u32::MAX`'s when the shader is no pass-through.
    Geometry {
        pass_through: [u8; 4],
        primitive: u32,
    },
    Pixel {
        valid_render_targets: u32,
        has_coverage_output: u32,
    },
}

/// `CheckGSBBIsPassThrough` from the entry: whether the blocks it reaches are empty.
///
/// Whether the blocks the entry reaches by unconditional branches hold no instruction and the
/// last returns.
///
/// The C++ meant to let moves, emits and cuts pass, but its overloads for them take mutable
/// references while `PackedVariantList::any` hands each instruction as a `const` one, so only
/// its catch-all overload is viable and any instruction fails the test. The port keeps what
/// the C++ answers.
///
/// # Errors
///
/// [`GS_LOOPS_FOREVER`] when the branches cycle.
pub fn gs_blocks_pass_through(blocks: &[BasicBlock]) -> Result<bool, Error> {
    let mut at = blocks.first();
    // A walk longer than the list has revisited a block.
    for _ in 0..=blocks.len() {
        let Some(block) = at else { return Ok(false) };
        if !block.instructions.is_empty() {
            return Ok(false);
        }
        match block.target {
            Target::Return => return Ok(true),
            Target::UnconditionalBranch(next) => at = blocks.get(next.index()),
            _ => return Ok(false),
        }
    }
    Err(GS_LOOPS_FOREVER)
}

/// `CheckGSSignatureIsPassThrough`: the pass-through data.
///
/// `None` when the outputs are not the inputs passed on.
/// Each output must match an input on its register with a mask the input's covers, and by
/// name (case-insensitive) and index unless it is the render target or viewport array index,
/// whose register and first component the data records (255 when there is none). A component
/// of an empty mask is 32, as ARM64 counts it.
#[must_use]
pub fn gs_signature_pass_through(input: &Signature, output: &OutputSignature) -> Option<[u8; 4]> {
    let mut data = [255u8; 4];
    if output.num_streams() > 1 || output.rasterized_stream() != 0 {
        return None;
    }
    let outputs = &output.stream(0)?.parameters;
    if outputs.len() > input.parameters.len() {
        return None;
    }
    for out in outputs {
        let found = input
            .parameters
            .iter()
            .find(|i| out.register == i.register && (out.mask & i.mask) == out.mask)?;
        let first = u32::from(out.mask).trailing_zeros();
        match out.system_value {
            sb_name::RENDER_TARGET_ARRAY_INDEX => {
                data[0] = low_byte(out.register);
                data[1] = low_byte(first);
            }
            sb_name::VIEWPORT_ARRAY_INDEX => {
                data[2] = low_byte(out.register);
                data[3] = low_byte(first);
            }
            _ => {
                if out.semantic_index != found.semantic_index
                    || !out.semantic_name.eq_ignore_ascii_case(&found.semantic_name)
                {
                    return None;
                }
            }
        }
    }
    Some(data)
}

/// The kind `DXILGetEntry` gives a DXBC element (`SemanticOf`).
///
/// Its system value's DXIL `SemanticKind`, or by name the depth and stencil outputs, else 0
/// (`Arbitrary`).
/// The names are compared case-insensitively; a pixel shader's `SV_Target` is 0 too.
#[must_use]
pub fn semantic_kind(system_value: u32, name: &[u8]) -> u32 {
    match system_value {
        sb_name::POSITION => 3,
        sb_name::CLIP_DISTANCE => 6,
        sb_name::CULL_DISTANCE => 7,
        sb_name::RENDER_TARGET_ARRAY_INDEX => 4,
        sb_name::VIEWPORT_ARRAY_INDEX => 5,
        sb_name::VERTEX_ID => 1,
        sb_name::PRIMITIVE_ID => 10,
        sb_name::INSTANCE_ID => 2,
        sb_name::IS_FRONT_FACE => 13,
        sb_name::SAMPLE_INDEX => 12,
        _ => [
            (&b"sv_depth"[..], 17),
            (b"sv_depthlessequal", 18),
            (b"sv_depthgreaterequal", 19),
            (b"sv_stencilref", 20),
        ]
        .into_iter()
        .find_map(|(n, kind)| name.eq_ignore_ascii_case(n).then_some(kind))
        .unwrap_or(0),
    }
}

/// The low byte, as the C++'s assignment to a `uint8_t` keeps it.
const fn low_byte(x: u32) -> u8 {
    let [low, ..] = x.to_le_bytes();
    low
}

#[cfg(test)]
mod tests;
