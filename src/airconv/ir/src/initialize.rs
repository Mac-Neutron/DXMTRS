//! DXBC Initialize: a shader's reflection, control flow and handlers from its container.
//!
//! [`dxbc`] is `SM50Initialize`'s DXBC path (`A/dxbc_converter.cpp:1032-1250`) over a parsed
//! container that holds no DXIL part: the program and its signatures, the control-flow walk with
//! the signature handlers, then the binding tables and the reflection's argument tables.
//! [`DxbcShader::reflect`] is the rest (`:1252-1329`), which the C++ ran only for a caller that
//! asked for the reflection: a hull shader's threadgroup and factor are fitted to the
//! tessellation pipeline there, and refused when they cannot be.

use crate::{
    binding::{
        AddressSpace, ArgumentTable, BindingType, MemoryAccess, MslScalar, MslType, MslValue,
        ShaderArgument, argument_index, count_u32, flag, msl_scalar, texture_kind,
    },
    cfg::{self, Cfg},
    container::{Container, Error, fourcc},
    estimators::{self, HullCounts},
    handlers::{Recorder, SignatureState},
    reflection::{self, Reflection, StageReflection},
    shader_info::{ResourceType, ShaderFlags, UavFlags},
    signature::{self, OutputSignature, Signature},
    tokens::{Program, program_type},
};

/// A container with neither an `SHEX` nor an `SHDR` part.
pub const NO_SHADER_BLOB: Error = Error::fail("Invalid DXBC bytecode: shader blob not found");
/// Any failure to read the input signature, in the C++'s words.
pub const NO_INPUT_SIGNATURE: Error =
    Error::fail("Invalid DXBC bytecode: input signature not found");
/// Any failure to read the output signature, in the C++'s words.
pub const NO_OUTPUT_SIGNATURE: Error =
    Error::fail("Invalid DXBC bytecode: output signature not found");
/// A view whose elements are doubles, which no texture holds (the C++ fell off its end).
pub const DOUBLE_TEXTURE: Error = Error::fail("Invalid DXBC bytecode: a texture of doubles");
/// A hull shader whose patch needs more than 32 threads.
pub const THREADGROUP_TOO_LARGE: Error =
    Error::fail("Threadgroup size of tessellation pipeline is too large.");
/// A hull shader whose payload exceeds 16 KiB even for one patch at factor 1.
pub const PAYLOAD_TOO_LARGE: Error =
    Error::fail("Payload size of tessellation pipeline is too large.");

/// The largest payload of a tessellation threadgroup (Metal's object payload).
const MAX_PAYLOAD: u64 = 16384;
/// The largest mesh a domain shader may output.
const MAX_MESH: u64 = 32768;
/// The constant buffer table's and the argument table's buffer indices.
const CONSTANT_BUFFER_TABLE: u32 = 29;
const ARGUMENT_TABLE: u32 = 30;

/// An initialized DXBC shader (`SM50ShaderInternal` without DXIL).
#[derive(Debug)]
pub struct DxbcShader {
    /// `D3D10_SB_TOKENIZED_PROGRAM_TYPE`.
    pub program_type: u32,
    /// The blocks, the declarations and the declared scalars.
    ///
    /// The [`crate::shader_info::ShaderInfo`] has its argument indices set; a hull shader's
    /// scalars are fitted by [`DxbcShader::reflect`].
    pub cfg: Cfg,
    pub input: Signature,
    pub output: OutputSignature,
    /// The function signature, the handlers, the register counts and scalars.
    pub io: SignatureState,
    pub binding_table: ArgumentTable,
    pub binding_table_cbuffer: ArgumentTable,
    /// `SM50GetArgumentsInfo`'s two tables: the constant buffers, then every other argument.
    pub args_cbuffer: Vec<ShaderArgument>,
    pub args: Vec<ShaderArgument>,
    masks: SlotMasks,
}

/// Which D3D slots the shader binds, one bit each.
#[derive(Debug, Default)]
struct SlotMasks {
    cbuffer: u16,
    sampler: u16,
    uav: u64,
    srv_hi: u64,
    srv_lo: u64,
}

/// `SM50Initialize`'s DXBC path, up to the reflection.
///
/// # Errors
///
/// [`NO_SHADER_BLOB`], [`NO_INPUT_SIGNATURE`], [`NO_OUTPUT_SIGNATURE`], [`DOUBLE_TEXTURE`], and
/// the program's, the walk's and the handlers' errors.
pub fn dxbc(container: &Container<'_>) -> Result<DxbcShader, Error> {
    let code = container
        .find(fourcc::SHEX)
        .or_else(|| container.find(fourcc::SHDR))
        .ok_or(NO_SHADER_BLOB)?;
    let input = signature::input(container).map_err(|_| NO_INPUT_SIGNATURE)?;
    let output = signature::output_v5(container).map_err(|_| NO_OUTPUT_SIGNATURE)?;
    let program = Program::new(code)?;
    let program_type = program.program_type();

    let (cfg, mut io) = {
        let empty = Signature::default();
        let stream0 = output.stream(0).unwrap_or(&empty);
        let mut recorder = Recorder::new(program_type, &input, stream0);
        let cfg = cfg::read(&program, &mut recorder)?;
        (cfg, recorder.state)
    };
    if cfg.decls.early_fragment_tests {
        io.func_signature.use_early_fragment_tests();
    }
    io.func_signature
        .use_max_work_group_size(cfg.decls.max_workgroup_size);

    let mut shader = DxbcShader {
        program_type,
        cfg,
        input,
        output,
        io,
        binding_table: ArgumentTable::default(),
        binding_table_cbuffer: ArgumentTable::default(),
        args_cbuffer: Vec::new(),
        args: Vec::new(),
        masks: SlotMasks::default(),
    };
    shader.bind()?;

    if program_type == program_type::HULL
        && !shader
            .cfg
            .info
            .flags
            .contains(ShaderFlags::NO_CONTROL_POINT_PHASE_PASSTHROUGH)
    {
        // The control points pass through: the hull outputs its inputs' registers.
        let max = &mut shader.io.max_output_register;
        for p in &shader.input.parameters {
            *max = (*max).max(p.register.wrapping_add(1));
        }
    }
    Ok(shader)
}

impl DxbcShader {
    /// The reflection `SM50Initialize` writes, the C++'s `if (pRefl)` block.
    ///
    /// For a hull shader it also fits the threadgroup and the maximum factor to the tessellation
    /// pipeline and keeps them, as the C++ wrote them back to the shader; the C++ ran none of it
    /// for a caller that passed no reflection.
    ///
    /// # Errors
    ///
    /// For a hull shader, [`THREADGROUP_TOO_LARGE`] and [`PAYLOAD_TOO_LARGE`]; and
    /// [`reflection::GS_LOOPS_FOREVER`].
    pub fn reflect(&mut self) -> Result<Reflection, Error> {
        let stage = match self.program_type {
            program_type::COMPUTE => StageReflection::Compute {
                threadgroup_size: self.cfg.decls.threadgroup_size,
            },
            program_type::HULL => self.fit_hull()?,
            program_type::DOMAIN => {
                let max_output_register = self.io.max_output_register;
                let max_potential_tess_factor = (1..=64)
                    .take_while(|&f| estimators::mesh_size(max_output_register, f) <= MAX_MESH)
                    .last()
                    .unwrap_or(1);
                StageReflection::Domain {
                    max_potential_tess_factor,
                }
            }
            program_type::GEOMETRY => {
                let m = &self.masks;
                let bound = m.cbuffer != 0
                    || m.sampler != 0
                    || m.uav != 0
                    || m.srv_hi != 0
                    || m.srv_lo != 0;
                let data = if bound || !reflection::gs_blocks_pass_through(&self.cfg.blocks)? {
                    None
                } else {
                    reflection::gs_signature_pass_through(&self.input, &self.output)
                };
                StageReflection::Geometry {
                    pass_through: data.unwrap_or([0xff; 4]),
                    primitive: self.cfg.decls.gs_input_primitive,
                }
            }
            program_type::PIXEL => StageReflection::Pixel {
                valid_render_targets: self.io.pso_valid_output_reg_mask,
                has_coverage_output: self.io.ps_has_coverage_output,
            },
            _ => StageReflection::None,
        };
        let bind_index = |table: &[ShaderArgument], index| {
            if table.is_empty() { u32::MAX } else { index }
        };
        Ok(Reflection {
            constant_buffer_table_bind_index: bind_index(&self.args_cbuffer, CONSTANT_BUFFER_TABLE),
            argument_buffer_bind_index: bind_index(&self.args, ARGUMENT_TABLE),
            num_constant_buffers: count_u32(self.args_cbuffer.len()),
            num_arguments: count_u32(self.args.len()),
            stage,
            constant_buffer_slot_mask: self.masks.cbuffer,
            sampler_slot_mask: self.masks.sampler,
            uav_slot_mask: self.masks.uav,
            srv_slot_mask_hi: self.masks.srv_hi,
            srv_slot_mask_lo: self.masks.srv_lo,
            num_output_element: self.io.max_output_register,
            argument_table_qwords: count_u32(self.binding_table.len()),
            writes_uav: u32::from(!self.cfg.info.uav_map.is_empty()),
        })
    }

    /// The hull shader's estimates, fitted.
    ///
    /// Patches per threadgroup halved, then the factor lowered by 2, until the payload fits.
    fn fit_hull(&mut self) -> Result<StageReflection, Error> {
        let threads_per_patch =
            estimators::next_pow2(self.cfg.decls.hull_maximum_threads_per_patch);
        if threads_per_patch > 32 {
            return Err(THREADGROUP_TOO_LARGE);
        }
        let counts = HullCounts {
            partition: self.cfg.decls.tessellation_partition,
            domain: self.cfg.decls.tessellation_domain,
            max_output_register: self.io.max_output_register,
            output_control_point_count: self.cfg.decls.output_control_point_count,
            patch_constant_scalars: self.io.patch_constant_scalars.len(),
        };
        // `next_pow2` answers a power of two, never 0.
        let mut patch_per_group = 32 / threads_per_patch;
        let mut factor = self.cfg.decls.max_tessellation_factor;
        while estimators::payload_size(&counts, factor, patch_per_group) > MAX_PAYLOAD {
            if patch_per_group != 1 {
                patch_per_group >>= 1;
            } else if factor > 1.0 {
                // `std::max(factor - 2.0f, 1.0f)`. A factor too large for the step to change it
                // would loop forever in the C++.
                let lowered = factor - 2.0;
                if lowered >= factor {
                    return Err(PAYLOAD_TOO_LARGE);
                }
                factor = if lowered < 1.0 { 1.0 } else { lowered };
            } else {
                return Err(PAYLOAD_TOO_LARGE);
            }
        }
        let threads_per_patch = 32 / patch_per_group;
        self.cfg.decls.hull_maximum_threads_per_patch = threads_per_patch;
        self.cfg.decls.max_tessellation_factor = factor;
        Ok(StageReflection::Hull {
            threads_per_patch,
            partition: self.cfg.decls.tessellation_partition,
            max_factor: factor,
            output_primitive: self.cfg.decls.tessellator_output_primitive,
        })
    }

    /// The four binding loops (`:1079-1236`).
    ///
    /// Each declared resource's fields in the binding tables, its reflection entry and its slot's
    /// mask bit, in slot order.
    fn bind(&mut self) -> Result<(), Error> {
        let info = &mut self.cfg.info;
        let table = &mut self.binding_table;
        let masks = &mut self.masks;

        for (&slot, cbv) in &mut info.cbuffer_map {
            cbv.arg_index = self.binding_table_cbuffer.define_buffer(
                format!("cb{slot}"),
                AddressSpace::Constant,
                MemoryAccess::Read,
                MslType::Value(MslValue::UINT4),
                argument_index(BindingType::ConstantBuffer, slot),
                None,
            );
            self.args_cbuffer.push(ShaderArgument {
                ty: BindingType::ConstantBuffer,
                slot,
                flags: flag::BUFFER | flag::READ_ACCESS,
                structure_ptr_offset: cbv.arg_index,
            });
            masks.cbuffer |= mask16(slot);
        }
        for (&slot, sampler) in &mut info.sampler_map {
            let location = argument_index(BindingType::Sampler, slot);
            sampler.arg_index = table.define_sampler(format!("s{slot}"), location);
            sampler.arg_cube_index =
                table.define_sampler(format!("cube_s{slot}"), location.wrapping_add(1));
            sampler.arg_metadata_index =
                table.define_integer64(format!("meta_s{slot}"), location.wrapping_add(2));
            self.args.push(ShaderArgument {
                ty: BindingType::Sampler,
                slot,
                flags: 0,
                structure_ptr_offset: sampler.arg_index,
            });
            masks.sampler |= mask16(slot);
        }
        for (&slot, srv) in &mut info.srv_map {
            let location = argument_index(BindingType::Srv, slot);
            let access = if srv.sampled {
                MemoryAccess::Sample
            } else {
                MemoryAccess::Read
            };
            srv.arg_index = match texture_kind(&srv.resource_type, srv.compared) {
                Some(kind) => table.define_texture(
                    format!("t{slot}"),
                    kind,
                    access,
                    msl_scalar(&srv.scaler_type).ok_or(DOUBLE_TEXTURE)?,
                    location,
                    None,
                ),
                // `device`: `constant` has size and alignment limits.
                None => table.define_buffer(
                    format!("t{slot}"),
                    AddressSpace::Device,
                    MemoryAccess::Read,
                    MslType::Value(MslValue::Scalar(MslScalar::Uint)),
                    location,
                    None,
                ),
            };
            srv.arg_metadata_index =
                table.define_integer64(format!("meta_t{slot}"), location.wrapping_add(1));
            let mut flags = view_flags(&srv.resource_type);
            if srv.read || srv.sampled || srv.compared {
                flags |= flag::READ_ACCESS;
            }
            self.args.push(ShaderArgument {
                ty: BindingType::Srv,
                slot,
                flags,
                structure_ptr_offset: srv.arg_index,
            });
            // ARM64 shifts by the count mod 64, as the C++'s `1ULL << …` ran.
            if slot & 64 != 0 {
                masks.srv_hi |= 1u64.wrapping_shl(slot.wrapping_sub(64));
            } else {
                masks.srv_lo |= 1u64.wrapping_shl(slot);
            }
        }
        for (&slot, uav) in &mut info.uav_map {
            let (read, written) = (
                uav.flags.contains(UavFlags::READ),
                uav.flags.contains(UavFlags::WRITTEN),
            );
            let access = match (written, read) {
                (true, true) => MemoryAccess::ReadWrite,
                (true, false) => MemoryAccess::Write,
                (false, _) => MemoryAccess::Read,
            };
            let order = uav.flags.contains(UavFlags::RASTERIZER_ORDER).then_some(1);
            let location = argument_index(BindingType::Uav, slot);
            uav.arg_index = match texture_kind(&uav.resource_type, false) {
                Some(kind) => table.define_texture(
                    format!("u{slot}"),
                    kind,
                    access,
                    msl_scalar(&uav.scaler_type).ok_or(DOUBLE_TEXTURE)?,
                    location,
                    order,
                ),
                None => table.define_buffer(
                    format!("u{slot}"),
                    AddressSpace::Device,
                    access,
                    MslType::Value(MslValue::Scalar(MslScalar::Uint)),
                    location,
                    order,
                ),
            };
            uav.arg_metadata_index =
                table.define_integer64(format!("meta_u{slot}"), location.wrapping_add(1));
            let counter = uav.flags.contains(UavFlags::WITH_COUNTER);
            if counter {
                uav.arg_counter_index = table.define_buffer(
                    format!("counter{slot}"),
                    AddressSpace::Device,
                    MemoryAccess::ReadWrite,
                    MslType::Value(MslValue::Scalar(MslScalar::Uint)),
                    location.wrapping_add(2),
                    order,
                );
            }
            let mut flags = view_flags(&uav.resource_type);
            if read {
                flags |= flag::READ_ACCESS;
            }
            if written {
                flags |= flag::WRITE_ACCESS;
            }
            if counter {
                flags |= flag::UAV_COUNTER;
            }
            self.args.push(ShaderArgument {
                ty: BindingType::Uav,
                slot,
                flags,
                structure_ptr_offset: uav.arg_index,
            });
            masks.uav |= 1u64.wrapping_shl(slot);
        }
        Ok(())
    }
}

/// A view's reflected kind: a buffer, a typed buffer, or a texture (an array one marked).
const fn view_flags(ty: &ResourceType) -> u32 {
    match ty {
        ResourceType::NonApplicable => flag::ELEMENT_WIDTH | flag::BUFFER,
        ResourceType::TextureBuffer => flag::TBUFFER_OFFSET | flag::TEXTURE,
        ResourceType::Texture1DArray
        | ResourceType::Texture2DArray
        | ResourceType::Texture2DMultisampledArray
        | ResourceType::TextureCubeArray => {
            flag::TEXTURE_MINLOD_CLAMP | flag::TEXTURE | flag::TEXTURE_ARRAY
        }
        _ => flag::TEXTURE_MINLOD_CLAMP | flag::TEXTURE,
    }
}

/// `1 << slot` into a 16-bit mask: an `int` shift ARM64 takes mod 32, its low half kept.
const fn mask16(slot: u32) -> u16 {
    let [low, high, ..] = 1u32.wrapping_shl(slot).to_le_bytes();
    u16::from_le_bytes([low, high])
}

#[cfg(test)]
mod tests;
