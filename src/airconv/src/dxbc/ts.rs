//! The tessellation pipeline's functions: vertex-hull and domain.
//!
//! The vertex-hull function is an object function, the domain function a mesh function.
//!
//! [`convert_hull`] is `convert_dxbc_vertex_hull_shader` (`A/dxbc_converter_ts.cpp:178-723`): the
//! vertex and hull shaders run in one threadgroup, the vertex shader's outputs passing through
//! threadgroup memory; the hull's patch constants and tessellation factors go into the payload
//! with the workloads the domain function consumes. [`convert_domain`] is
//! `convert_dxbc_tesselator_domain_shader` (`:725-1056`): each thread tessellates its workloads'
//! vertices, runs the domain shader and writes the mesh. Both keep the C++'s order of values,
//! blocks and globals, which the module's text shows.

use core::ffi::{CStr, c_ulonglong};

use airconv_ir::{
    binding::{
        AddressSpace, BufferBinding, FunctionInput, MemoryAccess, MeshOutputTopology,
        MeshPrimitiveOutput, MeshVertexOutput, MslScalar, MslType, MslValue, MslVector, count_u32,
    },
    estimators::{self, next_pow2},
    handlers::MeshOutputHandler,
    initialize::DxbcShader,
    shader_info::ShaderFlags,
    tokens::{program_type, tessellator},
};

use super::{
    binding_rootsig::{root_signature_bytes, setup_binding_rootsig, setup_bindings},
    binding_sm50::setup_binding_table2,
    blocks,
    converter::{BindingMap, Converter, OutputPrimitive, Partitioning, invalid, req},
    replay::{
        self, Options, input_layout, pop_mesh_output_position,
        pop_mesh_output_render_target_array_index, pop_mesh_output_vertex_data,
        pop_mesh_output_viewport_array_index,
    },
    resources::{Resources, setup_immediate_constant_buffer, setup_temp_register},
};
use crate::{
    abi::{
        ArgChain, SM50_BINDING_INDEX_ARGUMENT_TABLE2, SM50_BINDING_INDEX_CONSTANT_BUFFER2,
        SM50_BINDING_INDEX_DRAW_ARGUMENTS, SM50_BINDING_INDEX_INDEX_BUFFER,
        SM50_BINDING_INDEX_ROOT_ARGUMENTS, SM50_BINDING_INDEX_STATIC_SAMPLERS,
        SM50_SHADER_METAL_310,
    },
    air::{
        builder::{AirBuilder, FpBinOp, MemFlags},
        metadata::{add_entry, set_metal_version},
        signature::{Handles, create_function},
        ty::AirTypes,
    },
    api::Error,
    llvm::{Builder, Module, Ty, TyCx, Val, sys},
};

/// A pass-through geometry shader's register for no output.
const NO_REGISTER: u8 = 255;
/// The address space of threadgroup memory.
const THREADGROUP: u32 = 3;
/// The address space of an object function's payload.
const PAYLOAD: u32 = 6;
/// `sizeof(TessMeshWorkload)`: one workload's record in the payload.
const WORKLOAD_SIZE: u32 = 56;
/// The number of tessellation factors a hull shader writes at most (`tess_factors`).
const FACTORS: usize = 6;

/// Lowers `vertex` and `hull` into `m` as the object function `name`.
///
/// # Errors
///
/// An unreadable root signature, a hull shader whose control points pass through but whose
/// vertex shader writes fewer registers, a payload over 4 GiB, and what the lowering refuses.
pub fn convert_hull(
    vertex: &DxbcShader,
    hull: &DxbcShader,
    name: &[u8],
    m: &Module<'_>,
    args: &ArgChain<'_>,
) -> Result<(), Error> {
    if vertex.program_type != program_type::VERTEX || hull.program_type != program_type::HULL {
        return Err(wrong_stage());
    }
    let cx = m.cx();
    let tcx = cx.tcx();
    // The vertex stage's own signature only defines outputs the object function does not need,
    // and the hull's defines nothing yet.
    let mut desc = hull.io.func_signature.clone();
    let vertex_info = &vertex.cfg.info;
    let hull_info = &hull.cfg.info;
    let decls = &hull.cfg.decls;

    let max_vertex_input = vertex.io.max_input_register;
    let max_vertex_output = vertex
        .output
        .rasterized()
        .parameters
        .iter()
        .fold(0, |max, out| max.max(out.register.wrapping_add(1)));
    let max_hull_output = hull.io.max_output_register;
    let max_patch_constant_output_register = hull.io.max_patch_constant_output_register;
    let control_points_in = decls.input_control_point_count;
    let control_points_out = decls.output_control_point_count;

    let (final_max_tess_factor, factor_int) = estimators::final_max_tess_factor(
        decls.max_tessellation_factor,
        decls.tessellation_partition,
        args.tessellator.map(|t| t.max_potential_tess_factor),
    );

    let (metal_version, shader_flags) = args
        .common
        .map_or((SM50_SHADER_METAL_310, 0), |c| (c.metal_version, c.flags));
    // SAFETY: the chain's layout (`SM50Compile`'s contract).
    let ia_layout = args.ia_layout.map(|ia| unsafe { input_layout(ia) });
    let is_indexed_draw = args.ia_layout.is_some_and(|ia| ia.index_buffer_format > 0);

    let mut vertex_res = Resources::default();
    let mut hull_res = Resources::default();
    let types = AirTypes::new(tcx);
    let mut handles = Handles::default();

    // The vertex shader's outputs stay in threadgroup memory, so its handlers define no output.
    let vertex_replay = replay::record(
        &vertex.io.signature_handlers,
        &mut desc,
        &Options {
            ia_layout,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: true,
        },
    )?;
    let hull_replay = replay::record(
        &hull.io.signature_handlers,
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    )?;

    // The vertex stage's own root signature (the chain's second) and argument slots.
    let vertex_binding: Box<dyn BindingMap<'_> + '_> = match args.root_signature2 {
        Some(rootsig) => {
            // SAFETY: the chain's root signature (`SM50Compile`'s contract).
            let bytecode = unsafe { root_signature_bytes(rootsig) };
            Box::new(setup_binding_rootsig(
                vertex_info,
                &mut desc,
                m,
                &mut handles,
                program_type::VERTEX,
                bytecode,
                (
                    SM50_BINDING_INDEX_ROOT_ARGUMENTS,
                    SM50_BINDING_INDEX_STATIC_SAMPLERS,
                ),
            )?)
        }
        None => Box::new(setup_binding_table2(
            vertex,
            &mut desc,
            m,
            &mut handles,
            SM50_BINDING_INDEX_CONSTANT_BUFFER2,
            SM50_BINDING_INDEX_ARGUMENT_TABLE2,
        )?),
    };
    let hull_binding = setup_bindings(hull, &mut desc, m, &mut handles, args)?;

    // 32 threads make up a threadgroup: `next_pow2` is never 0.
    let threads_per_patch = next_pow2(decls.hull_maximum_threads_per_patch);
    let patches_per_threadgroup = 32 / threads_per_patch;
    let patch_per_group = next_pow2(patches_per_threadgroup);

    let hs = HullTypes::new(
        tcx,
        &types,
        max_hull_output,
        control_points_out,
        patch_per_group,
        hull.io.patch_constant_scalars.len(),
    );
    let hs_pcout_per_patch = tcx.array(types.int4, max_patch_constant_output_register);
    let hs_pcout_per_group = tcx.array(hs_pcout_per_patch, patch_per_group);
    let hs_pcout_per_patch_float = tcx.array(types.float4, max_patch_constant_output_register);

    let max_workload_count =
        estimators::max_potential_workload_count(factor_int, decls.tessellation_domain);
    desc.use_max_mesh_work_group_size(max_workload_count);

    let payload_ty = hs.payload_type(tcx, &types, max_workload_count, patch_per_group);
    let payload_size = alloc_size(m, payload_ty)?;

    let payload_idx = desc.define_input(FunctionInput::Payload { size: payload_size });
    let thread_id_idx = desc.define_input(FunctionInput::ThreadPositionInThreadgroup);
    // (batched_patch_id_start, instance_id, 0)
    let tg_id_idx = desc.define_input(FunctionInput::ThreadgroupPositionInGrid);
    desc.define_input(FunctionInput::MeshGridProperties);
    let (draw_ty, draw_name) = if is_indexed_draw {
        (types.dxmt_draw_indexed_arguments, "draw_indexed_arguments")
    } else {
        (types.dxmt_draw_arguments, "draw_arguments")
    };
    let draw_argument_idx = desc.define_input(FunctionInput::Buffer(BufferBinding {
        buffer_size: None,
        location_index: SM50_BINDING_INDEX_DRAW_ARGUMENTS,
        array_size: 0,
        memory_access: MemoryAccess::Read,
        address_space: AddressSpace::Constant,
        ty: MslType::Struct {
            name: draw_name.into(),
            ty: handles.add_ty(draw_ty),
        },
        arg_name: "draw_arguments".into(),
        raster_order_group: None,
    }));
    let index_buffer_idx = args
        .ia_layout
        .filter(|ia| ia.index_buffer_format > 0)
        .map(|ia| {
            desc.define_input(FunctionInput::Buffer(BufferBinding {
                buffer_size: None,
                location_index: SM50_BINDING_INDEX_INDEX_BUFFER,
                array_size: 0,
                memory_access: MemoryAccess::Read,
                address_space: AddressSpace::Device,
                ty: MslType::Value(MslValue::Scalar(if ia.index_buffer_format == 1 {
                    MslScalar::Ushort
                } else {
                    MslScalar::Uint
                })),
                arg_name: String::new(),
                raster_order_group: None,
            }))
        });

    let (function, entry) = create_function(m, &desc, name, true, &handles)?;
    let no_block = || invalid("a block of no function");
    let block = |label: &CStr| function.append_block(cx, label).ok_or_else(no_block);
    let entry_bb = block(c"entry")?;
    let vertex_stage_end = block(c"vertex_end")?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, m, &types, shader_flags);
    let param = |index| {
        function
            .param(tcx, index)
            .ok_or_else(|| invalid("a tessellation input of no argument"))
    };

    setup_temp_register(vertex_info, &mut vertex_res, &types, &b);
    setup_temp_register(hull_info, &mut hull_res, &types, &b);
    setup_immediate_constant_buffer(vertex_info, &mut vertex_res, &types, m, &b)?;
    setup_immediate_constant_buffer(hull_info, &mut hull_res, &types, m, &b)?;

    let output_reg_per_point = tcx.array(types.int4, max_vertex_output);
    let output_reg_per_patch = tcx.array(output_reg_per_point, control_points_in);
    let output_reg_per_group = tcx.array(output_reg_per_patch, patch_per_group);
    let output_reg_per_point_float = tcx.array(types.float4, max_vertex_output);
    let output_reg_per_patch_float = tcx.array(output_reg_per_point_float, control_points_in);

    let vertex_out = threadgroup_global(
        m,
        output_reg_per_group,
        c"vertex_out_hull_in",
        cx.undef(output_reg_per_group),
    )?;

    let thread_position_in_group = param(thread_id_idx)?;
    let control_point_id_in_patch = b.extract_element_u64(thread_position_in_group, 0);
    let patch_offset_in_group = b.extract_element_u64(thread_position_in_group, 1);

    let instance_id;
    let batched_patch_start;
    let patch_id;
    let patch_count;
    {
        // The vertex shader's zone.
        let epilogue_bb = block(c"epilogue_vertex")?;
        set_metal_version(m, metal_version);
        let active = block(c"active_vertex")?;

        let threadgroup_position_in_grid = param(tg_id_idx)?;
        batched_patch_start = b.mul(
            b.extract_element_u64(threadgroup_position_in_grid, 0),
            air.get_int(patches_per_threadgroup),
        );
        patch_id = b.add(batched_patch_start, patch_offset_in_group);
        instance_id = b.extract_element_u64(threadgroup_position_in_grid, 1);
        let control_point_index = b.add(
            b.mul(patch_id, air.get_int(control_points_in)),
            control_point_id_in_patch,
        );

        let draw_arguments = b.load(draw_ty, param(draw_argument_idx)?);
        patch_count = b.udiv(
            b.extract_value(draw_arguments, 0),
            air.get_int(control_points_in),
        );

        let input = b.alloca(tcx.array(types.int4, max_vertex_input), c"");
        vertex_res.input.ptr_int4 = Some(input);
        vertex_res.input.ptr_float4 =
            Some(b.bitcast(input, tcx.ptr(tcx.array(types.float4, max_vertex_input), 0)));
        vertex_res.input_element_count = max_vertex_input;

        // Of type `output_reg_per_point`.
        let output = b.gep(
            output_reg_per_group,
            vertex_out,
            &[
                air.get_int(0),
                patch_offset_in_group,
                control_point_id_in_patch,
            ],
        );
        vertex_res.output.ptr_int4 = Some(output);
        vertex_res.output.ptr_float4 =
            Some(b.bitcast(output, tcx.ptr(output_reg_per_point_float, THREADGROUP)));
        vertex_res.output_element_count = max_vertex_output;

        let in_patch = b.icmp(
            sys::INT_ULT,
            control_point_id_in_patch,
            air.get_int(control_points_in),
        );
        let patch_is_live = b.icmp(sys::INT_ULT, patch_id, patch_count);
        b.cond_br(
            b.logical_and(in_patch, patch_is_live),
            active,
            vertex_stage_end,
        );
        b.position_at_end(active);

        let vertex_id = if let Some(index_buffer_idx) = index_buffer_idx {
            let start_index = b.extract_value(draw_arguments, 2);
            let index_buffer = param(index_buffer_idx)?;
            let element_ty = tcx
                .pointee(index_buffer.ty())
                .ok_or_else(|| invalid("the index buffer is not a pointer"))?;
            let index = b.load(
                element_ty,
                b.gep(
                    element_ty,
                    index_buffer,
                    &[b.add(start_index, control_point_index)],
                ),
            );
            b.zext(index, types.int)
        } else {
            control_point_index
        };
        vertex_res.vertex_id = Some(vertex_id);
        let base_vertex_id = b.extract_value(draw_arguments, if is_indexed_draw { 3 } else { 2 });
        vertex_res.base_vertex_id = Some(base_vertex_id);
        vertex_res.instance_id = Some(instance_id);
        vertex_res.vertex_id_with_base = Some(b.add(vertex_id, base_vertex_id));
        let base_instance_id = b.extract_value(draw_arguments, if is_indexed_draw { 4 } else { 3 });
        vertex_res.base_instance_id = Some(base_instance_id);
        vertex_res.instance_id_with_base = Some(b.add(instance_id, base_instance_id));

        vertex_replay.prologue(&air, function, &mut vertex_res)?;
        let conv = Converter {
            air: &air,
            ir: &b,
            function,
            binding: vertex_binding.as_ref(),
            res: &vertex_res,
            types: &types,
            pso_sample_mask: u32::MAX,
            shader_type: vertex.program_type,
            metal_version,
            geometry: None,
        };
        let real_entry = blocks::convert(&conv, &vertex.cfg.blocks, epilogue_bb)?;
        b.br(real_entry);

        b.position_at_end(epilogue_bb);
        // The function returns void and the output registers are handled directly.
        vertex_replay.epilogue(&air, &vertex_res, None, u32::MAX)?;
        b.br(vertex_stage_end);
        b.position_at_end(vertex_stage_end);

        air.create_barrier(MemFlags::THREADGROUP, false)?;
    }

    {
        // The hull shader's zone.
        let epilogue_bb = block(c"epilogue_hull")?;
        let payload = b.bitcast(param(payload_idx)?, tcx.ptr(payload_ty, PAYLOAD));

        let input = b.gep(
            output_reg_per_group,
            vertex_out,
            &[air.get_int(0), patch_offset_in_group],
        );
        hull_res.input.ptr_int4 = Some(input);
        hull_res.input.ptr_float4 =
            Some(b.bitcast(input, tcx.ptr(output_reg_per_patch_float, THREADGROUP)));
        hull_res.input_element_count = max_vertex_output;
        hull_res.instance_id = Some(instance_id);
        hull_res.patch_id = Some(patch_id);
        let patch_is_live = b.icmp(sys::INT_ULT, patch_id, patch_count);
        // 32 so all phases are effectively skipped.
        let thread_id_in_patch =
            b.select(patch_is_live, control_point_id_in_patch, air.get_int(32));
        hull_res.thread_id_in_patch = Some(thread_id_in_patch);

        if hull_info
            .flags
            .contains(ShaderFlags::NO_CONTROL_POINT_PHASE_PASSTHROUGH)
        {
            if hull_info
                .flags
                .contains(ShaderFlags::OUTPUT_CONTROL_POINT_READ)
            {
                let control_point_phase_out = threadgroup_global(
                    m,
                    hs.per_group,
                    c"hull_control_point_phase_out",
                    cx.undef(hs.per_group),
                )?;
                hull_res.output.ptr_int4 = Some(b.gep(
                    hs.per_group,
                    control_point_phase_out,
                    &[air.get_int(0), patch_offset_in_group],
                ));
                hull_res.hull_cp_passthrough_dst = Some(b.gep(
                    payload_ty,
                    payload,
                    &[
                        air.get_int(0),
                        air.get_int(0),
                        patch_offset_in_group,
                        thread_id_in_patch,
                    ],
                ));
                hull_res.hull_cp_passthrough_src = Some(b.gep(
                    hs.per_group,
                    control_point_phase_out,
                    &[air.get_int(0), patch_offset_in_group, thread_id_in_patch],
                ));
                hull_res.hull_cp_passthrough_type = Some(hs.per_point);
            } else {
                // The field of control points.
                hull_res.output.ptr_int4 = Some(b.gep(
                    payload_ty,
                    payload,
                    &[air.get_int(0), air.get_int(0), patch_offset_in_group],
                ));
            }
            let output = req(hull_res.output.ptr_int4, "no hull output register file")?;
            let space = tcx
                .addrspace(output.ty())
                .ok_or_else(|| invalid("a register file is not a pointer"))?;
            hull_res.output.ptr_float4 =
                Some(b.bitcast(output, tcx.ptr(hs.per_patch_float, space)));
            hull_res.output_element_count = max_hull_output;
        } else {
            // Since there is no control point phase.
            if max_vertex_output < max_hull_output {
                return Err(Error::new(
                    b"Hull shader has control point phase pass-through, but vertex shader \
                      output signature doesn't match hull shader input signature.",
                ));
            }
            hull_res.output.ptr_float4 = hull_res.input.ptr_float4;
            hull_res.output.ptr_int4 = hull_res.input.ptr_int4;
            hull_res.output_element_count = hull_res.input_element_count;

            hull_res.hull_cp_passthrough_dst = Some(b.gep(
                payload_ty,
                payload,
                &[
                    air.get_int(0),
                    air.get_int(0),
                    patch_offset_in_group,
                    thread_id_in_patch,
                ],
            ));
            hull_res.hull_cp_passthrough_src = Some(b.gep(
                output_reg_per_group,
                vertex_out,
                &[air.get_int(0), patch_offset_in_group, thread_id_in_patch],
            ));
            hull_res.hull_cp_passthrough_type = Some(hs.per_point);
        }

        if max_patch_constant_output_register != 0 {
            // All instances write to the same output (threadgroup memory).
            let patch_constant_out = threadgroup_global(
                m,
                hs_pcout_per_group,
                c"hull_patch_constant_out",
                cx.undef(hs_pcout_per_group),
            )?;
            let output = b.gep(
                hs_pcout_per_group,
                patch_constant_out,
                &[air.get_int(0), patch_offset_in_group],
            );
            hull_res.patch_constant_output.ptr_int4 = Some(output);
            hull_res.patch_constant_output.ptr_float4 =
                Some(b.bitcast(output, tcx.ptr(hs_pcout_per_patch_float, THREADGROUP)));
        }

        hull_replay.prologue(&air, function, &mut hull_res)?;
        let conv = Converter {
            air: &air,
            ir: &b,
            function,
            binding: hull_binding.as_ref(),
            res: &hull_res,
            types: &types,
            pso_sample_mask: u32::MAX,
            shader_type: hull.program_type,
            metal_version,
            geometry: None,
        };
        let real_entry = blocks::convert(&conv, &hull.cfg.blocks, epilogue_bb)?;
        b.br(real_entry);

        b.position_at_end(epilogue_bb);

        // Populate the patch constant output.
        let write_patch_constant = block(c"write_patch_constant")?;
        let dispatch_mesh = block(c"dispatch_mesh")?;
        let real_return = block(c"real_return")?;

        let first_thread = b.icmp(sys::INT_EQ, thread_id_in_patch, air.get_int(0));
        b.cond_br(first_thread, write_patch_constant, real_return);

        b.position_at_end(write_patch_constant);
        hull_replay.epilogue(&air, &hull_res, None, u32::MAX)?;

        let mut tess_factors = [air.get_float(0.0); FACTORS];
        let max_tess_factor_value = air.get_float(final_max_tess_factor);
        for (i, scalar) in (0_u32..).zip(&hull.io.patch_constant_scalars) {
            let dst_ptr = b.gep(
                payload_ty,
                payload,
                &[
                    air.get_int(0),        // struct
                    air.get_int(1),        // pcout
                    patch_offset_in_group, // patch_id
                    air.get_int(i),
                ],
            );
            let constants = req(
                hull_res.patch_constant_output.ptr_int4,
                "no patch constant register file",
            )?;
            let src_ptr = b.gep(
                hs_pcout_per_patch,
                constants,
                &[
                    air.get_int(0),
                    air.get_int(scalar.reg.into()),
                    air.get_int(scalar.component.into()),
                ],
            );
            let factor = usize::try_from(scalar.tess_factor_index)
                .ok()
                .and_then(|factor| tess_factors.get_mut(factor));
            if let Some(factor) = factor {
                let value = b.bitcast(b.load(types.int, src_ptr), types.float);
                let clamped =
                    air.create_fp_bin_op(FpBinOp::Fmin, value, max_tess_factor_value, true)?;
                *factor = clamped;
                b.store(b.bitcast(clamped, types.int), dst_ptr);
            } else {
                b.store(b.load(types.int, src_ptr), dst_ptr);
            }
        }

        let workload_count =
            threadgroup_global(m, types.int, c"workload_count", cx.undef(types.int))?;
        // Clear the value.
        air.create_atomic_rmw(sys::ATOMIC_RMW_BIN_OP_AND, workload_count, air.get_int(0))?;

        let partitioning = partitioning(decls.tessellation_partition);
        match decls.tessellation_domain {
            // TESS TODO: the C++ makes no workload for lines.
            tessellator::DOMAIN_ISOLINE => {}
            tessellator::DOMAIN_QUAD => {
                let data = b.gep(
                    payload_ty,
                    payload,
                    &[air.get_int(0), air.get_int(3), air.get_int(0)],
                );
                let [out0, out1, out2, out3, inner0, inner1] = tess_factors;
                conv.hull_generate_workload(
                    true,
                    &partitioning,
                    &[
                        patch_offset_in_group,
                        workload_count,
                        data,
                        inner0,
                        inner1,
                        out0,
                        out1,
                        out2,
                        out3,
                    ],
                )?;
            }
            _ => {
                let data = b.gep(
                    payload_ty,
                    payload,
                    &[air.get_int(0), air.get_int(3), air.get_int(0)],
                );
                let [out0, out1, out2, inner, ..] = tess_factors;
                conv.hull_generate_workload(
                    false,
                    &partitioning,
                    &[
                        patch_offset_in_group,
                        workload_count,
                        data,
                        inner,
                        out0,
                        out1,
                        out2,
                    ],
                )?;
            }
        }
        let first_patch = b.icmp(sys::INT_EQ, patch_offset_in_group, air.get_int(0));
        b.cond_br(first_patch, dispatch_mesh, real_return);

        b.position_at_end(dispatch_mesh);

        let meshgroup_to_dispatch =
            b.insert_element_u64(air.get_int3(1, 1, 1), b.load(types.int, workload_count), 0);
        air.create_set_mesh_properties(meshgroup_to_dispatch)?;
        b.store(
            batched_patch_start,
            b.gep(payload_ty, payload, &[air.get_int(0), air.get_int(2)]),
        );
        b.br(real_return);
        b.position_at_end(real_return);
    }

    air.create_barrier(MemFlags::THREADGROUP, false)?;
    b.ret_void();

    add_entry(m, c"air.object", entry);
    Ok(())
}

/// Lowers `domain`, fed by `hull`, into `m` as the mesh function `name`.
///
/// # Errors
///
/// An unreadable root signature, a payload over 4 GiB, and what the lowering refuses.
pub fn convert_domain(
    domain: &DxbcShader,
    hull: &DxbcShader,
    name: &[u8],
    m: &Module<'_>,
    args: &ArgChain<'_>,
) -> Result<(), Error> {
    if domain.program_type != program_type::DOMAIN || hull.program_type != program_type::HULL {
        return Err(wrong_stage());
    }
    let cx = m.cx();
    let tcx = cx.tcx();
    let mut desc = domain.io.func_signature.clone();
    let info = &domain.cfg.info;
    let decls = &hull.cfg.decls;

    let max_input_register = domain.io.max_input_register;
    let max_output_register = domain.io.max_output_register;
    let rasterization_disabled = args
        .gs_pass_through
        .is_some_and(|pt| pt.rasterization_disabled != 0);
    // SAFETY: both members of the union are four bytes of plain integers.
    let pass_through = args.gs_pass_through.map(|pt| unsafe { pt.u.data });
    let (metal_version, shader_flags) = args
        .common
        .map_or((SM50_SHADER_METAL_310, 0), |c| (c.metal_version, c.flags));

    let (_, factor_int) = estimators::final_max_tess_factor(
        decls.max_tessellation_factor,
        decls.tessellation_partition,
        args.tessellator.map(|t| t.max_potential_tess_factor),
    );

    let mut res = Resources::default();
    let types = AirTypes::new(tcx);
    let mut handles = Handles::default();

    // The domain shader's handlers are built into no prologue or epilogue: the C++ only lets
    // them define into the signature.
    replay::record(
        &domain.io.signature_handlers,
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    )?;

    let binding = setup_bindings(domain, &mut desc, m, &mut handles, args)?;

    let rta = pass_through
        .filter(|pt| pt.render_target_array_index_reg != NO_REGISTER)
        .map(|pt| {
            desc.define_mesh_primitive_output(MeshPrimitiveOutput::RenderTargetArrayIndex);
            (
                pt.render_target_array_index_reg,
                pt.render_target_array_index_component,
            )
        });
    let va = pass_through
        .filter(|pt| pt.viewport_array_index_reg != NO_REGISTER)
        .map(|pt| {
            desc.define_mesh_primitive_output(MeshPrimitiveOutput::ViewportArrayIndex);
            (
                pt.viewport_array_index_reg,
                pt.viewport_array_index_component,
            )
        });

    let thread_id_idx = desc.define_input(FunctionInput::ThreadPositionInThreadgroup);
    let tg_id_idx = desc.define_input(FunctionInput::ThreadgroupPositionInGrid);

    // 32 threads make up a threadgroup: `next_pow2` is never 0.
    let threads_per_patch = next_pow2(decls.hull_maximum_threads_per_patch);
    let patch_per_group = next_pow2(32 / threads_per_patch);

    let hs = HullTypes::new(
        tcx,
        &types,
        hull.io.max_output_register,
        decls.output_control_point_count,
        patch_per_group,
        hull.io.patch_constant_scalars.len(),
    );

    // n segments has n + 1 points.
    let max_edge_point = factor_int.wrapping_add(1);
    let max_workload_count =
        estimators::max_potential_workload_count(factor_int, decls.tessellation_domain);
    let payload_ty = hs.payload_type(tcx, &types, max_workload_count, patch_per_group);
    let payload_size = alloc_size(m, payload_ty)?;

    let payload_idx = desc.define_input(FunctionInput::Payload { size: payload_size });

    let mut point_rasterization = false;
    match decls.tessellator_output_primitive {
        tessellator::OUTPUT_POINT => {
            desc.define_input(FunctionInput::Mesh {
                vertex_count: max_edge_point.wrapping_mul(2),
                primitive_count: max_edge_point.wrapping_mul(2),
                topology: MeshOutputTopology::Point,
            });
            // TESS TODO: not necessary... if combined with GS/SO.
            point_rasterization = true;
        }
        tessellator::OUTPUT_LINE => {
            // TESS TODO.
            desc.define_input(FunctionInput::Mesh {
                vertex_count: max_edge_point,
                primitive_count: max_edge_point.wrapping_sub(1),
                topology: MeshOutputTopology::Line,
            });
        }
        _ => {
            let count = max_edge_point
                .wrapping_add(2)
                .wrapping_sub(max_edge_point & 1)
                .wrapping_mul(2)
                .wrapping_add(1);
            desc.define_input(FunctionInput::Mesh {
                vertex_count: count,
                primitive_count: count,
                topology: MeshOutputTopology::Triangle,
            });
        }
    }

    if point_rasterization {
        desc.define_mesh_vertex_output(MeshVertexOutput::PointSize);
    }
    if !domain.io.clip_distance_scalars.is_empty() {
        desc.define_mesh_vertex_output(MeshVertexOutput::ClipDistance {
            count: domain.io.clip_distance_scalars.len(),
        });
    }

    let (function, entry) = create_function(m, &desc, name, rasterization_disabled, &handles)?;
    let no_block = || invalid("a block of no function");
    let block = |label: &CStr| function.append_block(cx, label).ok_or_else(no_block);
    let entry_bb = block(c"entry")?;
    let vertex_start = block(c"vertex_start")?;
    let vertex_emit = block(c"vertex_emit")?;
    let vertex_end = block(c"vertex_end")?;
    let generate_primitive_pre = block(c"generate_primitive_pre")?;
    let generate_primitive = block(c"generate_primitive")?;
    let real_return = block(c"real_return")?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, m, &types, shader_flags);
    let param = |index| {
        function
            .param(tcx, index)
            .ok_or_else(|| invalid("a tessellation input of no argument"))
    };

    set_metal_version(m, metal_version);
    setup_temp_register(info, &mut res, &types, &b);
    setup_immediate_constant_buffer(info, &mut res, &types, m, &b)?;

    let workload_index = b.extract_element_u64(param(tg_id_idx)?, 0);
    let thread_index = b.extract_element_u64(param(thread_id_idx)?, 0);

    let payload = b.bitcast(param(payload_idx)?, tcx.ptr(payload_ty, PAYLOAD));
    let data = b.gep(
        payload_ty,
        payload,
        &[air.get_int(0), air.get_int(3), air.get_int(0)],
    );

    // The tessellation helpers read no register: they run before the registers are set.
    let no_resources = Resources::default();
    let helpers = Converter {
        air: &air,
        ir: &b,
        function,
        binding: binding.as_ref(),
        res: &no_resources,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: domain.program_type,
        metal_version,
        geometry: None,
    };

    let batched_patch_start = b.load(
        types.int,
        b.gep(payload_ty, payload, &[air.get_int(0), air.get_int(2)]),
    );
    let patch_index = helpers.domain_get_patch_index(workload_index, data)?;
    res.patch_id = Some(b.add(batched_patch_start, patch_index));

    let input = b.gep(
        payload_ty,
        payload,
        &[
            air.get_int(0), // struct
            air.get_int(0), // control points
            patch_index,
        ],
    );
    res.input.ptr_int4 = Some(input);
    res.input.ptr_float4 = Some(b.bitcast(input, tcx.ptr(hs.per_patch_float, PAYLOAD)));
    res.input_element_count = hull.io.max_output_register;

    let constants_ty = tcx.array(types.int4, max_input_register);
    let constants = b.alloca(constants_ty, c"");
    res.patch_constant_output.ptr_int4 = Some(constants);
    res.patch_constant_output.ptr_float4 = Some(b.bitcast(
        constants,
        tcx.ptr(tcx.array(types.float4, max_input_register), 0),
    ));

    let output_ty = tcx.array(types.int4, max_output_register);
    let output = b.alloca(output_ty, c"");
    let output_float_ty = tcx.array(types.float4, max_output_register);
    let output_float = b.bitcast(output, tcx.ptr(output_float_ty, 0));
    res.output.ptr_int4 = Some(output);
    res.output.ptr_float4 = Some(output_float);
    res.output_element_count = max_output_register;

    // Set up the patch constant registers.
    for (i, scalar) in (0_u32..).zip(&hull.io.patch_constant_scalars) {
        if u32::from(scalar.reg) >= max_input_register {
            continue;
        }
        let src_ptr = b.gep(
            payload_ty,
            payload,
            &[
                air.get_int(0), // struct
                air.get_int(1), // pcout
                patch_index,
                air.get_int(i),
            ],
        );
        let dst_ptr = b.gep(
            constants_ty,
            constants,
            &[
                air.get_int(0),
                air.get_int(scalar.reg.into()),
                air.get_int(scalar.component.into()),
            ],
        );
        b.store(b.load(types.int, src_ptr), dst_ptr);
    }

    b.br(vertex_start);

    b.position_at_end(vertex_start);

    let phi_thread_index_base = b.phi(types.int);
    b.add_incoming(phi_thread_index_base, air.get_int(0), entry_bb);

    let actual_thread_index = b.add(phi_thread_index_base, thread_index);

    let (location, active, iterate) = helpers.domain_get_location(
        workload_index,
        actual_thread_index,
        data,
        &partitioning(decls.tessellation_partition),
    )?;

    // It accidentally works on quad as well.
    {
        let domain_bary = cx.undef(air.float_ty(3));
        let bary_u = b.extract_element_u64(location, 0);
        let bary_v = b.extract_element_u64(location, 1);
        let bary_w = b.fsub(air.get_float(1.0), b.fadd(bary_u, bary_v));
        let domain_bary = b.insert_element_u64(domain_bary, bary_u, 0);
        let domain_bary = b.insert_element_u64(domain_bary, bary_v, 1);
        res.domain = Some(b.insert_element_u64(domain_bary, bary_w, 2));
    }

    let conv = Converter {
        binding: binding.as_ref(),
        res: &res,
        ..helpers
    };
    let real_entry = blocks::convert(&conv, &domain.cfg.blocks, vertex_emit)?;
    b.cond_br(active, real_entry, vertex_end);

    b.position_at_end(vertex_emit);

    let vertex_id = actual_thread_index;
    let primitive_id = actual_thread_index;
    for handler in &domain.io.mesh_output_handlers {
        match *handler {
            MeshOutputHandler::Position { reg, .. } => {
                pop_mesh_output_position(&air, &res, reg, vertex_id)?;
            }
            MeshOutputHandler::RenderTargetArrayIndex { reg, mask } => {
                pop_mesh_output_render_target_array_index(&air, &res, reg, mask, primitive_id)?;
            }
            MeshOutputHandler::ViewportArrayIndex { reg, mask } => {
                pop_mesh_output_viewport_array_index(&air, &res, reg, mask, primitive_id)?;
            }
            MeshOutputHandler::VertexData {
                reg,
                mask,
                index,
                ty,
            } => {
                pop_mesh_output_vertex_data(
                    &air,
                    &res,
                    reg,
                    mask,
                    index,
                    vertex_id,
                    value_ty(tcx, ty),
                )?;
            }
        }
    }

    for (i, scalar) in (0_u32..).zip(&domain.io.clip_distance_scalars) {
        if u32::from(scalar.reg) >= max_output_register {
            continue;
        }
        let src_ptr = b.gep(
            output_float_ty,
            output_float,
            &[
                air.get_int(0),
                air.get_int(scalar.reg.into()),
                air.get_int(scalar.component.into()),
            ],
        );
        air.create_set_mesh_clip_distance(vertex_id, air.get_int(i), b.load(types.float, src_ptr))?;
    }

    // The pass-through geometry shader's array indices, read from their output registers.
    let index_source = |(reg, component): (u8, u8)| {
        let src_ptr = b.gep(
            output_ty,
            output,
            &[
                air.get_int(0),
                air.get_int(reg.into()),
                air.get_int(component.into()),
            ],
        );
        b.load(types.int, src_ptr)
    };
    if let Some(source) = rta {
        air.create_set_mesh_render_target_array_index(primitive_id, index_source(source))?;
    }
    if let Some(source) = va {
        air.create_set_mesh_viewport_array_index(primitive_id, index_source(source))?;
    }

    if point_rasterization {
        air.create_set_mesh_point_size(vertex_id, air.get_float(1.0))?;
    }

    b.br(vertex_end);

    b.position_at_end(vertex_end);
    b.add_incoming(
        phi_thread_index_base,
        b.add(phi_thread_index_base, air.get_int(32)),
        vertex_end,
    );
    b.cond_br(iterate, vertex_start, generate_primitive_pre);

    b.position_at_end(generate_primitive_pre);
    let first_thread = b.icmp(sys::INT_EQ, thread_index, air.get_int(0));
    b.cond_br(first_thread, generate_primitive, real_return);
    b.position_at_end(generate_primitive);

    conv.domain_generate_primitives(
        workload_index,
        data,
        &output_primitive(decls.tessellator_output_primitive),
    )?;

    b.br(real_return);
    b.position_at_end(real_return);
    b.ret_void();

    add_entry(m, c"air.mesh", entry);
    Ok(())
}

/// The refusal of a pipeline whose shaders are of other stages.
///
/// The C++ read their stage's fields whatever the stages were.
fn wrong_stage() -> Error {
    invalid("a tessellation pipeline of shaders of other stages")
}

/// The hull shader's register and payload array types, made for one threadgroup shape.
///
/// The types of `convert_dxbc_vertex_hull_shader` and `convert_dxbc_tesselator_domain_shader`,
/// which build the same payload.
struct HullTypes {
    /// `[max_hull_output x <4 x i32>]`: one control point.
    per_point: Ty,
    /// `[patches in a group x [control points x per_point]]`.
    per_group: Ty,
    /// `[control points x [max_hull_output x <4 x float>]]`: one patch, as floats.
    per_patch_float: Ty,
    /// `[patches in a group x [patch constant scalars x i32]]`.
    pcout_per_group_scalar: Ty,
}

impl HullTypes {
    fn new(
        tcx: &TyCx,
        types: &AirTypes,
        max_output_register: u32,
        output_control_point_count: u32,
        patch_per_group: u32,
        patch_constant_scalars: usize,
    ) -> Self {
        let per_point = tcx.array(types.int4, max_output_register);
        let per_patch = tcx.array(per_point, output_control_point_count);
        let per_point_float = tcx.array(types.float4, max_output_register);
        let pcout_per_patch_scalar = tcx.array(types.int, count_u32(patch_constant_scalars));
        Self {
            per_point,
            per_group: tcx.array(per_patch, patch_per_group),
            per_patch_float: tcx.array(per_point_float, output_control_point_count),
            pcout_per_group_scalar: tcx.array(pcout_per_patch_scalar, patch_per_group),
        }
    }

    /// The identified structure `payload`.
    ///
    /// Its fields are the control points, the patch constants, the batched patch start and the
    /// workloads.
    fn payload_type(
        &self,
        tcx: &TyCx,
        types: &AirTypes,
        max_workload_count: u32,
        patch_per_group: u32,
    ) -> Ty {
        let workload_words = max_workload_count
            .wrapping_mul(patch_per_group)
            .wrapping_mul(WORKLOAD_SIZE)
            / 4;
        tcx.create_struct(
            c"payload",
            Some((
                &[
                    self.per_group,
                    self.pcout_per_group_scalar,
                    types.int,
                    tcx.array(types.int, workload_words),
                ],
                false,
            )),
        )
    }
}

/// `get_partitioning`.
const fn partitioning(partition: u32) -> Partitioning {
    match estimators::partitioning(partition) {
        estimators::Partitioning::Integer => Partitioning::Integer,
        estimators::Partitioning::Pow2 => Partitioning::Pow2,
        estimators::Partitioning::FractionalOdd => Partitioning::FractionalOdd,
        estimators::Partitioning::FractionalEven => Partitioning::FractionalEven,
    }
}

/// `get_output_primitive`.
const fn output_primitive(primitive: u32) -> OutputPrimitive {
    match estimators::output_primitive(primitive) {
        estimators::OutputPrimitive::Point => OutputPrimitive::Point,
        estimators::OutputPrimitive::Line => OutputPrimitive::Line,
        estimators::OutputPrimitive::Triangle => OutputPrimitive::Triangle,
        estimators::OutputPrimitive::TriangleCcw => OutputPrimitive::TriangleCcw,
    }
}

/// An `MSLScalerOrVectorType` as its LLVM type (`air::get_llvm_type`).
///
/// Declared here, not in lane a's `air/signature.rs` where it is private: a request to make it
/// public is ledgered.
fn value_ty(tcx: &TyCx, value: MslValue) -> Ty {
    let (scalar, dimension) = match value {
        MslValue::Scalar(scalar) => (scalar, 1),
        MslValue::Vector(MslVector { dimension, scalar }) => (scalar, dimension),
    };
    let element = match scalar {
        MslScalar::Float => tcx.float(),
        MslScalar::Int | MslScalar::Uint => tcx.int(32),
        MslScalar::Bool => tcx.int(1),
        MslScalar::Half => tcx.half(),
        MslScalar::Ushort => tcx.int(16),
        MslScalar::Ulong => tcx.int(64),
    };
    if dimension == 1 {
        element
    } else {
        tcx.vector(element, dimension)
    }
}

/// An internal threadgroup global of `ty`, 4-aligned (`new GlobalVariable`, `setAlignment`).
///
/// Declared here, not in lane d's `resources.rs` where it is private: a request to make it public
/// is ledgered.
fn threadgroup_global<'cx>(
    m: &Module<'cx>,
    ty: Ty,
    global_name: &CStr,
    init: Val<'cx>,
) -> Result<Val<'cx>, Error> {
    let tcx = m.cx().tcx();
    // SAFETY: a live module, a type of its context and a NUL-terminated name.
    let raw = unsafe {
        sys::add_global_in_address_space(m.raw(), tcx.lower(ty), global_name.as_ptr(), THREADGROUP)
    };
    if raw.is_null() {
        return Err(invalid("a global was not made"));
    }
    // SAFETY: the global just made.
    unsafe { sys::set_linkage(raw, sys::INTERNAL_LINKAGE) };
    // SAFETY: the global and a constant of its value type, of one context.
    unsafe { sys::set_initializer(raw, init.raw()) };
    // SAFETY: the global just made.
    unsafe { sys::set_alignment(raw, 4) };
    // SAFETY: a global of this context, a pointer to `ty` in the threadgroup space.
    Ok(unsafe { m.cx().val(raw, tcx.ptr(ty, THREADGROUP)) })
}

/// `llvm-c/Target.h`'s `LLVMTargetDataRef`.
#[repr(C)]
struct TargetData {
    _data: [u8; 0],
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}

// The two functions `getTypeAllocSize` takes, which `llvm::sys` does not declare yet: declared
// here as `dxil/initialize.rs` does, until lane l moves them there (the ledger's request).
unsafe extern "C" {
    #[link_name = "LLVMGetModuleDataLayout"]
    fn get_module_data_layout(m: sys::ModuleRef) -> *mut TargetData;
    #[link_name = "LLVMABISizeOfType"]
    fn abi_size_of_type(td: *mut TargetData, ty: sys::TypeRef) -> c_ulonglong;
}

/// `DataLayout::getTypeAllocSize` of `ty`, which the C++ truncated to a `uint32_t`.
///
/// # Errors
///
/// A type of 4 GiB or more.
fn alloc_size(m: &Module<'_>, ty: Ty) -> Result<u32, Error> {
    let raw = m.cx().tcx().lower(ty);
    // SAFETY: a live module; the answer is its layout, never disposed of.
    let layout = unsafe { get_module_data_layout(m.raw()) };
    // SAFETY: the module's layout and a sized type of its context.
    let size = unsafe { abi_size_of_type(layout, raw) };
    u32::try_from(size).map_err(|_| invalid("a payload of 4 GiB or more"))
}
