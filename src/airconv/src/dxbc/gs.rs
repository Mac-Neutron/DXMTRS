//! The geometry pipeline's two functions.
//!
//! [`convert_vertex`] is `convert_dxbc_vertex_for_geometry_shader`
//! (`A/dxbc_converter_gs.cpp:521-883`): the object function, which runs the vertex shader for
//! one warp of vertices into the payload and tells the mesh function which primitives are
//! complete. [`convert_geometry`] is
//! `convert_dxbc_geometry_shader` (`:100-518`): the mesh function, which reads a warp's vertices
//! back from the payload, runs the geometry shader on each primitive and writes its `emit`s and
//! `cut`s into the mesh. Both take the body of the shader as a callback (`ShaderBody`), so the
//! DXIL pipeline can lower its own blocks inside the same function.
//!
//! A warp holds `warp_vertex_count` vertices, which make `warp_primitive_count` primitives of
//! `vertex_per_primitive` vertices each ([`warp_counts`]); a strip shares vertices between
//! primitives.

use core::ffi::CStr;

use airconv_ir::{
    binding::{
        AddressSpace, BufferBinding, FunctionInput, MemoryAccess, MeshOutputTopology,
        MeshVertexOutput, MslScalar, MslType, MslValue, MslVector,
    },
    handlers::{MeshOutputHandler, ScalarInfo},
    initialize::DxbcShader,
    tokens::program_type,
};

use super::{
    binding_rootsig::setup_bindings,
    converter::{Converter, GeometryOutput, invalid, req},
    replay::{self, Options, input_layout},
    resources::{Resources, setup_immediate_constant_buffer, setup_temp_register},
};
use crate::{
    abi::{
        ArgChain, SM50_BINDING_INDEX_DRAW_ARGUMENTS, SM50_BINDING_INDEX_INDEX_BUFFER,
        SM50_SHADER_METAL_310,
    },
    air::{
        builder::{AirBuilder, MemFlags},
        metadata::{add_entry, set_metal_version},
        signature::{Handles, create_function},
        ty::AirTypes,
    },
    api::Error,
    llvm::{Block, Builder, Module, Ty, TyCx, Val, sys},
};

/// The payload's size in bytes, as DXMT's D3D11 declares it.
const PAYLOAD_SIZE: u32 = 16256;
/// The threadgroup address space, where the object function's valid vertex mask lives.
const THREADGROUP: u32 = 3;
/// Metal's object-data address space, where the payload lives.
const OBJECT_DATA: u32 = 6;

/// `D3D10_SB_PRIMITIVE`.
mod primitive {
    pub const POINT: u32 = 1;
    pub const LINE: u32 = 2;
    pub const TRIANGLE: u32 = 3;
    pub const LINE_ADJ: u32 = 6;
    pub const TRIANGLE_ADJ: u32 = 7;
    pub const PATCH_1: u32 = 8;
}

/// `D3D10_SB_PRIMITIVE_TOPOLOGY`, the geometry shader's output.
mod topology {
    pub const POINT_LIST: u32 = 1;
    pub const LINE_STRIP: u32 = 3;
    pub const TRIANGLE_STRIP: u32 = 5;
}

/// What a shader body lowers its blocks with: the converter and the epilogue block to branch to.
///
/// Answers the body's entry block (`ShaderBody`).
pub trait ShaderBody<'cx>:
    FnOnce(&Converter<'_, 'cx>, Block<'cx>) -> Result<Block<'cx>, Error>
{
}

impl<'cx, F> ShaderBody<'cx> for F where
    F: FnOnce(&Converter<'_, 'cx>, Block<'cx>) -> Result<Block<'cx>, Error>
{
}

/// A warp's vertices, primitives, and vertices per primitive.
///
/// `get_vertex_primitive_count_in_warp`.
///
/// `strip` shares vertices between the primitives of a line or triangle strip. An unknown
/// primitive answers zeros.
#[must_use]
pub const fn warp_counts(primitive: u32, strip: bool) -> (u32, u32, u32) {
    match (primitive, strip) {
        (primitive::LINE, true) => (31, 31, 2),
        (primitive::TRIANGLE, true) => (30, 30, 3),
        (primitive::LINE_ADJ, true) => (29, 29, 4),
        (primitive::TRIANGLE_ADJ, true) => (28, 14, 6),
        // A list, a point and a patch of the same vertex count share their counts.
        // The patches: one to 16 control points have their own counts, 17 to 32 one patch a warp.
        (primitive::POINT | 8, _) => (32, 32, 1),
        (primitive::LINE, false) | (9, _) => (32, 16, 2),
        (primitive::TRIANGLE, false) | (10, _) => (30, 10, 3),
        (primitive::LINE_ADJ, false) | (11, _) => (32, 8, 4),
        (12, _) => (30, 6, 5),
        (primitive::TRIANGLE_ADJ, false) | (13, _) => (30, 5, 6),
        (14, _) => (28, 4, 7),
        (15, _) => (32, 4, 8),
        (16, _) => (27, 3, 9),
        (17, _) => (30, 3, 10),
        (18, _) => (22, 2, 11),
        (19, _) => (24, 2, 12),
        (20, _) => (26, 2, 13),
        (21, _) => (28, 2, 14),
        (22, _) => (30, 2, 15),
        (23, _) => (32, 2, 16),
        (24..=39, _) => (
            primitive - primitive::PATCH_1 + 1,
            1,
            primitive - primitive::PATCH_1 + 1,
        ),
        _ => (0, 0, 0),
    }
}

/// Lowers `geometry` into `m` as the mesh function `name`, fed by `vertex`'s outputs.
///
/// `body` lowers the shader's blocks (DXBC: [`super::blocks::convert`]).
///
/// # Errors
///
/// An unreadable root signature, an output topology or (for a strip) an input primitive no
/// mesh function handles, and what the lowering refuses.
pub fn convert_geometry<'cx>(
    geometry: &DxbcShader,
    vertex: &DxbcShader,
    name: &[u8],
    m: &Module<'cx>,
    args: &ArgChain<'_>,
    body: impl ShaderBody<'cx>,
) -> Result<(), Error> {
    if geometry.program_type != program_type::GEOMETRY
        || vertex.program_type != program_type::VERTEX
    {
        return Err(invalid("a geometry pipeline of shaders of other stages"));
    }
    let cx = m.cx();
    let tcx = cx.tcx();
    let mut desc = geometry.io.func_signature.clone();
    let info = &geometry.cfg.info;
    let io = &geometry.io;
    let decls = &geometry.cfg.decls;
    let is_strip = args.geometry.is_some_and(|pso| pso.strip_topology != 0);
    let (metal_version, shader_flags) = args
        .common
        .map_or((SM50_SHADER_METAL_310, 0), |c| (c.metal_version, c.flags));
    let max_output_register = io.max_output_register;
    let vertex_registers = vertex.io.max_output_register;

    let mut res = Resources::default();
    let types = AirTypes::new(tcx);
    let mut handles = Handles::default();

    let replay = replay::record(
        &io.signature_handlers,
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    )?;
    let binding = setup_bindings(geometry, &mut desc, m, &mut handles, args)?;

    let output_topology = match decls.gs_output_topology {
        topology::TRIANGLE_STRIP => MeshOutputTopology::Triangle,
        topology::LINE_STRIP => MeshOutputTopology::Line,
        topology::POINT_LIST => MeshOutputTopology::Point,
        _ => return Err(Error::new(b"unsupported geometry shader output topology")),
    };

    let max_vertex_out = decls.gs_max_vertex_output;
    let payload_idx = desc.define_input(FunctionInput::Payload { size: PAYLOAD_SIZE });
    // It is intended to declare a bit more max primitive count.
    let mesh_idx = desc.define_input(FunctionInput::Mesh {
        vertex_count: max_vertex_out,
        primitive_count: max_vertex_out,
        topology: output_topology.clone(),
    });
    let tg_in_grid_idx = desc.define_input(FunctionInput::ThreadgroupPositionInGrid);
    if !io.clip_distance_scalars.is_empty() {
        desc.define_mesh_vertex_output(MeshVertexOutput::ClipDistance {
            count: io.clip_distance_scalars.len(),
        });
    }

    let (function, entry) = create_function(m, &desc, name, false, &handles)?;
    let no_block = || invalid("a block of no function");
    let entry_bb = function.append_block(cx, c"entry").ok_or_else(no_block)?;
    let active_bb = function.append_block(cx, c"active").ok_or_else(no_block)?;
    let epilogue_bb = function
        .append_block(cx, c"epilogue")
        .ok_or_else(no_block)?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, m, &types, shader_flags);
    set_metal_version(m, metal_version);

    let param = |index| {
        function
            .param(tcx, index)
            .ok_or_else(|| invalid("a mesh input of no argument"))
    };
    let (_, warp_primitive_count, vertex_per_primitive) =
        warp_counts(decls.gs_input_primitive, is_strip);
    let tg_position_in_grid = param(tg_in_grid_idx)?;
    let primitive_id_in_warp = b.extract_element_u64(tg_position_in_grid, 0);
    res.gs_instance_id = Some(b.extract_element_u64(tg_position_in_grid, 1));

    let zero = air.get_int(0);
    let one = air.get_int(1);
    let two = air.get_int(2);
    let next_write_vertex = b.alloca(types.int, c"next_write_vertex");
    b.store(zero, next_write_vertex);
    let vertex_offset = b.alloca(types.int, c"vertex_offset");
    b.store(zero, vertex_offset);
    let primitive_count = b.alloca(types.int, c"primitive_count");
    b.store(zero, primitive_count);

    res.mesh = Some(param(mesh_idx)?);

    let payload = param(payload_idx)?;
    let valid_primitive_mask = b.load(types.int, b.const_inbounds_gep1_32(types.int, payload, 1));
    let warp_id = b.load(types.int, b.const_inbounds_gep1_32(types.int, payload, 2));
    res.patch_id = Some(b.add(
        b.mul(warp_id, air.get_int(warp_primitive_count)),
        primitive_id_in_warp,
    ));

    // The input registers: one array of the vertex shader's outputs for each vertex of the
    // primitive.
    let input_per_vertex_ty = tcx.array(types.int4, vertex_registers);
    let input_per_vertex_float_ty = tcx.array(types.float4, vertex_registers);
    let input_ty = tcx.array(input_per_vertex_ty, vertex_per_primitive);
    let input_ptr_int4 = b.alloca(input_ty, c"");
    res.input.ptr_int4 = Some(input_ptr_int4);
    res.input.ptr_float4 = Some(b.bitcast(
        input_ptr_int4,
        tcx.ptr(
            tcx.array(input_per_vertex_float_ty, vertex_per_primitive),
            0,
        ),
    ));
    res.input_element_count = vertex_registers;

    let output_ptr_int4 = b.alloca(tcx.array(types.int4, max_output_register), c"");
    res.output.ptr_int4 = Some(output_ptr_int4);
    res.output.ptr_float4 = Some(b.bitcast(
        output_ptr_int4,
        tcx.ptr(tcx.array(types.float4, max_output_register), 0),
    ));
    res.output_element_count = max_output_register;

    setup_temp_register(info, &mut res, &types, &b);
    setup_immediate_constant_buffer(info, &mut res, &types, m, &b)?;

    let shifted = b.shl(one, primitive_id_in_warp);
    let active = b.icmp(sys::INT_NE, b.and(valid_primitive_mask, shifted), zero);
    b.cond_br(active, active_bb, epilogue_bb);
    b.position_at_end(active_bb);

    let vertices_ptr = b.bitcast(
        b.const_inbounds_gep1_32(types.int, payload, 4),
        tcx.ptr(types.int4, OBJECT_DATA),
    );
    let load_vertex = |vertex_index: Val<'cx>, vid: u32| {
        for reg in 0..vertex_registers {
            let index = b.add(
                b.mul(vertex_index, air.get_int(vertex_registers)),
                air.get_int(reg),
            );
            let src = b.gep(types.int4, vertices_ptr, &[index]);
            let value = b.load(types.int4, src);
            let dst = b.gep(
                input_ty,
                input_ptr_int4,
                &[zero, air.get_int(vid), air.get_int(reg)],
            );
            b.store(value, dst);
        }
    };

    if is_strip && decls.gs_input_primitive != primitive::POINT {
        let leading = primitive_id_in_warp;
        match decls.gs_input_primitive {
            primitive::TRIANGLE => {
                // primitive n: {n, n + 1 + (n & 1), n + 2 - (n & 1)}; the first triangle of the
                // warp is assumed to be at the right winding (primitive restart can be broken).
                let odd_bit = b.and(leading, air.get_int(1));
                load_vertex(leading, 0);
                load_vertex(b.add(b.add(leading, air.get_int(1)), odd_bit), 1);
                load_vertex(b.sub(b.add(leading, air.get_int(2)), odd_bit), 2);
            }
            primitive::LINE => {
                // primitive n: {n, n + 1}
                load_vertex(leading, 0);
                load_vertex(b.add(leading, air.get_int(1)), 1);
            }
            primitive::LINE_ADJ => {
                // primitive n: {n, n + 1, n + 2, n + 3}
                load_vertex(leading, 0);
                load_vertex(b.add(leading, air.get_int(1)), 1);
                load_vertex(b.add(leading, air.get_int(2)), 2);
                load_vertex(b.add(leading, air.get_int(3)), 3);
            }
            primitive::TRIANGLE_ADJ => {
                // For 16 vertices and 6 primitives:
                //   primitive 0: {0, (-2), 2, 6, 4, 3}
                //   primitive 1: {2, 5, 6, 8, 4, 0}
                //   primitive n (inner): {2n, -, 2(n + 1 + (n & 1)), -, 2(n + 2 - (n & 1)), -}
                //   primitive n (outer 1): {-, is_even(n) ? 2n - 2 : 2n + 3, -, -, -, -} mod 17
                //   primitive n (outer 2): {-, -, -, 6 + 2n, -, -} mod 15
                //   primitive n (outer 3): {-, -, -, -, -, is_even(n) ? 2n + 3 : 2n - 2}
                let double = |v: Val<'cx>| b.mul(two, v);
                let odd_bit = b.and(leading, one);
                let twice = double(leading);
                load_vertex(twice, 0);
                load_vertex(double(b.add(b.add(leading, one), odd_bit)), 2);
                load_vertex(double(b.sub(b.add(leading, two), odd_bit)), 4);
                let twice_plus_3 = b.add(twice, air.get_int(3));
                let twice_minus_2 = b.sub(twice, two);
                let is_odd = b.icmp(sys::INT_EQ, odd_bit, one);
                load_vertex(
                    b.urem(
                        b.select(is_odd, twice_plus_3, twice_minus_2),
                        air.get_int(33),
                    ),
                    1,
                );
                load_vertex(b.urem(b.add(twice, air.get_int(6)), air.get_int(31)), 3);
                load_vertex(b.select(is_odd, twice_minus_2, twice_plus_3), 5);
            }
            other => {
                return Err(Error::new(
                    format!("unhandled geometry shader input primitive strip: {other}").as_bytes(),
                ));
            }
        }
    } else {
        let leading = b.mul(air.get_int(vertex_per_primitive), primitive_id_in_warp);
        for vid in 0..vertex_per_primitive {
            load_vertex(b.add(leading, air.get_int(vid)), vid);
        }
    }

    replay.prologue(&air, function, &mut res)?;
    let output = MeshOutput {
        air: &air,
        res: &res,
        topology: output_topology,
        next_write_vertex,
        vertex_offset,
        primitive_count,
        handlers: &io.mesh_output_handlers,
        clip_distances: &io.clip_distance_scalars,
        max_output_register,
    };
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: binding.as_ref(),
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: geometry.program_type,
        metal_version,
        geometry: Some(&output),
    };
    let real_entry = body(&conv, epilogue_bb)?;
    b.br(real_entry);
    b.position_at_end(epilogue_bb);

    replay.epilogue(&air, &res, None, u32::MAX)?;
    output.cut()?;
    air.create_set_mesh_primitive_count(b.load(types.int, primitive_count))?;
    b.ret_void();

    add_entry(m, c"air.mesh", entry);
    Ok(())
}

/// Lowers `vertex` into `m` as the object function `name`, feeding `geometry`'s mesh function.
///
/// `body` lowers the shader's blocks (DXBC: [`super::blocks::convert`]).
///
/// # Errors
///
/// An unreadable root signature, a vertex attribute or draw argument the lowering cannot make,
/// a strip's input primitive no object function handles, and what the lowering refuses.
pub fn convert_vertex<'cx>(
    vertex: &DxbcShader,
    geometry: &DxbcShader,
    name: &[u8],
    m: &Module<'cx>,
    args: &ArgChain<'_>,
    body: impl ShaderBody<'cx>,
) -> Result<(), Error> {
    if geometry.program_type != program_type::GEOMETRY
        || vertex.program_type != program_type::VERTEX
    {
        return Err(invalid("a geometry pipeline of shaders of other stages"));
    }
    let cx = m.cx();
    let tcx = cx.tcx();
    let mut desc = vertex.io.func_signature.clone();
    let info = &vertex.cfg.info;
    let io = &vertex.io;
    let input_primitive = geometry.cfg.decls.gs_input_primitive;
    let is_strip = args.geometry.is_some_and(|pso| pso.strip_topology != 0);
    // SAFETY: the chain's layout (`SM50Compile`'s contract).
    let ia_layout = args.ia_layout.map(|ia| unsafe { input_layout(ia) });
    let index_buffer_format = args.ia_layout.map_or(0, |ia| ia.index_buffer_format);
    let (metal_version, shader_flags) = args
        .common
        .map_or((SM50_SHADER_METAL_310, 0), |c| (c.metal_version, c.flags));
    let max_input_register = io.max_input_register;
    let max_output_register = io.max_output_register;

    let is_triadj_strip = is_strip && input_primitive == primitive::TRIANGLE_ADJ;
    let is_indexed_draw = index_buffer_format > 0;

    let mut res = Resources::default();
    let types = AirTypes::new(tcx);
    let mut handles = Handles::default();

    let replay = replay::record(
        &io.signature_handlers,
        &mut desc,
        &Options {
            ia_layout,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: true,
        },
    )?;
    let binding = setup_bindings(vertex, &mut desc, m, &mut handles, args)?;

    let payload_idx = desc.define_input(FunctionInput::Payload { size: PAYLOAD_SIZE });
    // (warp_size, 1, 1)
    let thread_id_idx = desc.define_input(FunctionInput::ThreadPositionInThreadgroup);
    // (warp_count, instance_count, 1), warp_count = ceil(index_count / warp_size)
    let tg_id_idx = desc.define_input(FunctionInput::ThreadgroupPositionInGrid);
    desc.define_input(FunctionInput::MeshGridProperties);
    let (draw_name, draw_ty) = if is_indexed_draw {
        ("draw_indexed_arguments", types.dxmt_draw_indexed_arguments)
    } else {
        ("draw_arguments", types.dxmt_draw_arguments)
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
    let index_buffer_idx = is_indexed_draw.then(|| {
        desc.define_input(FunctionInput::Buffer(BufferBinding {
            buffer_size: None,
            location_index: SM50_BINDING_INDEX_INDEX_BUFFER,
            array_size: 0,
            memory_access: MemoryAccess::Read,
            address_space: AddressSpace::Device,
            ty: MslType::Value(if index_buffer_format == 1 {
                MslValue::Scalar(MslScalar::Ushort)
            } else {
                MslValue::UINT
            }),
            arg_name: String::new(),
            raster_order_group: None,
        }))
    });

    let (function, entry) = create_function(m, &desc, name, true, &handles)?;
    let no_block = || invalid("a block of no function");
    let entry_bb = function.append_block(cx, c"entry").ok_or_else(no_block)?;
    let epilogue_bb = function
        .append_block(cx, c"epilogue")
        .ok_or_else(no_block)?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, m, &types, shader_flags);
    set_metal_version(m, metal_version);

    let index_check_bb = function
        .append_block(cx, c"index_check")
        .ok_or_else(no_block)?;
    let active_bb = function.append_block(cx, c"active").ok_or_else(no_block)?;
    let will_dispatch_bb = function
        .append_block(cx, c"will_dispatch")
        .ok_or_else(no_block)?;
    let dispatch_bb = function
        .append_block(cx, c"dispatch")
        .ok_or_else(no_block)?;
    let return_bb = function.append_block(cx, c"return").ok_or_else(no_block)?;

    let param = |index| {
        function
            .param(tcx, index)
            .ok_or_else(|| invalid("an object input of no argument"))
    };
    let payload = param(payload_idx)?;
    let thread_position_in_group = param(thread_id_idx)?;
    let warp_vertex_id = b.extract_element_u64(thread_position_in_group, 0);

    let threadgroup_position_in_grid = param(tg_id_idx)?;
    let warp_id = b.extract_element_u64(threadgroup_position_in_grid, 0);
    let instance_id = b.extract_element_u64(threadgroup_position_in_grid, 1);

    let draw_arguments = b.load(draw_ty, param(draw_argument_idx)?);
    let vertex_count = b.extract_value(draw_arguments, 0);

    let input_ptr_int4 = b.alloca(tcx.array(types.int4, max_input_register), c"");
    res.input.ptr_int4 = Some(input_ptr_int4);
    res.input.ptr_float4 = Some(b.bitcast(
        input_ptr_int4,
        tcx.ptr(tcx.array(types.float4, max_input_register), 0),
    ));
    res.input_element_count = max_input_register;

    let (warp_vertex_count, warp_primitive_count, vertex_per_primitive) =
        warp_counts(input_primitive, is_strip);

    // The output registers are the payload's, one array of them for each thread of the warp.
    let payload_output_ptr = b.const_inbounds_gep1_32(types.int, payload, 4);
    let int4_regs_ptr = tcx.ptr(tcx.array(types.int4, max_output_register), OBJECT_DATA);
    let int4_regs = b.bitcast(payload_output_ptr, int4_regs_ptr);
    let int4_regs = b.gep(
        tcx.pointee(int4_regs_ptr)
            .ok_or_else(|| invalid("a register array is not a pointer"))?,
        int4_regs,
        &[warp_vertex_id],
    );
    let float4_regs_ptr = tcx.ptr(tcx.array(types.float4, max_output_register), OBJECT_DATA);
    let float4_regs = b.bitcast(payload_output_ptr, float4_regs_ptr);
    let float4_regs = b.gep(
        tcx.pointee(float4_regs_ptr)
            .ok_or_else(|| invalid("a register array is not a pointer"))?,
        float4_regs,
        &[warp_vertex_id],
    );
    res.output.ptr_int4 = Some(int4_regs);
    res.output.ptr_float4 = Some(float4_regs);
    res.output_element_count = max_output_register;

    let valid_vertex_mask = threadgroup_int(m, types.int, c"valid_vertex_mask")?;

    setup_temp_register(info, &mut res, &types, &b);
    setup_immediate_constant_buffer(info, &mut res, &types, m, &b)?;

    let mut global_index_id = b.add(
        b.mul(warp_id, air.get_int(warp_vertex_count)),
        warp_vertex_id,
    );

    if is_triadj_strip {
        // The 2nd and 32nd vertices are not read, but the 33rd and the -2nd (or 1st) are.
        let one = air.get_int(1);
        let signed_max = |a: Val<'cx>, c: Val<'cx>| b.select(b.icmp(sys::INT_SLT, a, c), c, a);
        let signed_min = |a: Val<'cx>, c: Val<'cx>| b.select(b.icmp(sys::INT_SLT, a, c), a, c);

        let is_second_thread = b.icmp(sys::INT_EQ, warp_vertex_id, one);
        global_index_id = b.select(
            is_second_thread,
            b.add(global_index_id, air.get_int(31)),
            global_index_id,
        );
        let is_last_thread = b.icmp(sys::INT_EQ, warp_vertex_id, air.get_int(31));
        let lower = signed_max(one, b.sub(global_index_id, air.get_int(33)));
        // May degenerate.
        let upper = signed_min(global_index_id, b.sub(vertex_count, one));
        global_index_id = b.select(is_last_thread, lower, upper);
    }

    // Explicit initialization.
    air.create_atomic_rmw(
        sys::ATOMIC_RMW_BIN_OP_AND,
        valid_vertex_mask,
        air.get_int(0),
    )?;

    b.cond_br(
        b.icmp(sys::INT_ULT, global_index_id, vertex_count),
        index_check_bb,
        will_dispatch_bb,
    );
    b.position_at_end(index_check_bb);

    if let Some(index_idx) = index_buffer_idx {
        let start_index = b.extract_value(draw_arguments, 2);
        let index_buffer = param(index_idx)?;
        let element_ty = tcx
            .pointee(index_buffer.ty())
            .ok_or_else(|| invalid("an index buffer is not a pointer"))?;
        let at = b.gep(
            element_ty,
            index_buffer,
            &[b.add(start_index, global_index_id)],
        );
        let vertex_id = b.load(element_ty, at);
        // So 0xFFFF is mapped to 0xFFFFFFFF, and other values are zero-extended.
        let widened = b.zext(
            b.add(vertex_id, cx.const_int(element_ty, 1, false)),
            types.int,
        );
        let id = b.sub(widened, cx.const_int(types.int, 1, false));
        res.vertex_id = Some(id);
        b.cond_br(
            b.icmp(sys::INT_NE, air.get_int(u32::MAX), id),
            active_bb,
            will_dispatch_bb,
        );
    } else {
        res.vertex_id = Some(global_index_id);
        b.br(active_bb);
    }

    b.position_at_end(active_bb);

    air.create_atomic_rmw(
        sys::ATOMIC_RMW_BIN_OP_OR,
        valid_vertex_mask,
        b.shl(air.get_int(1), warp_vertex_id),
    )?;

    let vertex_id = req(res.vertex_id, "no vertex id")?;
    let base_vertex_id = b.extract_value(draw_arguments, if is_indexed_draw { 3 } else { 2 });
    res.base_vertex_id = Some(base_vertex_id);
    res.instance_id = Some(instance_id);
    res.vertex_id_with_base = Some(b.add(vertex_id, base_vertex_id));
    let base_instance_id = b.extract_value(draw_arguments, if is_indexed_draw { 4 } else { 3 });
    res.base_instance_id = Some(base_instance_id);
    res.instance_id_with_base = Some(b.add(instance_id, base_instance_id));

    replay.prologue(&air, function, &mut res)?;
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: binding.as_ref(),
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: vertex.program_type,
        metal_version,
        geometry: None,
    };
    let real_entry = body(&conv, epilogue_bb)?;
    b.br(real_entry);

    b.position_at_end(epilogue_bb);
    replay.epilogue(&air, &res, None, u32::MAX)?;

    b.br(will_dispatch_bb);
    b.position_at_end(will_dispatch_bb);

    air.create_barrier(MemFlags::THREADGROUP, false)?;

    b.cond_br(
        b.icmp(sys::INT_EQ, warp_vertex_id, air.get_int(0)),
        dispatch_bb,
        return_bb,
    );
    b.position_at_end(dispatch_bb);

    // The dispatch phase: the valid primitive mask, the instance and the warp for the mesh
    // function, and the mesh grid.
    let mut valid_primitive_mask = air.get_int(0);
    let valid_vertex_result = b.load(types.int, valid_vertex_mask);

    for primitive_id in 0..warp_primitive_count {
        let mut vertices_mask = 0_u32;
        if is_strip && input_primitive != primitive::POINT {
            match input_primitive {
                primitive::TRIANGLE => vertices_mask = 0b111_u32.wrapping_shl(primitive_id),
                primitive::LINE => vertices_mask = 0b11_u32.wrapping_shl(primitive_id),
                primitive::LINE_ADJ => vertices_mask = 0b1111_u32.wrapping_shl(primitive_id),
                primitive::TRIANGLE_ADJ => {
                    let odd = primitive_id & 1;
                    let twice = primitive_id << 1;
                    let bit = |at: u32| 1_u32.wrapping_shl(at);
                    vertices_mask |= bit(2_u32.wrapping_mul(primitive_id));
                    vertices_mask |= bit(2_u32.wrapping_mul(primitive_id + 1 + odd));
                    vertices_mask |= bit(2_u32.wrapping_mul(primitive_id + 2 - odd));
                    let plus_3 = twice.wrapping_add(3);
                    let minus_2 = twice.wrapping_sub(2);
                    vertices_mask |= bit(if odd == 1 { plus_3 } else { minus_2 } % 33);
                    vertices_mask |= bit((6 + 2 * primitive_id) % 31);
                    vertices_mask |= bit(if odd == 1 { minus_2 } else { plus_3 });
                }
                other => {
                    return Err(Error::new(
                        format!("unhandled geometry shader input primitive strip: {other}")
                            .as_bytes(),
                    ));
                }
            }
        } else {
            // A primitive is valid if all of its vertices are.
            for vid_in_prim in 0..vertex_per_primitive {
                vertices_mask |=
                    1_u32.wrapping_shl(vid_in_prim + vertex_per_primitive * primitive_id);
            }
        }
        let primitive_valid = b.icmp(
            sys::INT_EQ,
            b.and(air.get_int(vertices_mask), valid_vertex_result),
            air.get_int(vertices_mask),
        );
        valid_primitive_mask = b.or(
            valid_primitive_mask,
            b.shl(
                b.zext(primitive_valid, types.int),
                air.get_int(primitive_id),
            ),
        );
    }

    b.store(instance_id, b.const_inbounds_gep1_32(types.int, payload, 0));
    b.store(
        valid_primitive_mask,
        b.const_inbounds_gep1_32(types.int, payload, 1),
    );
    b.store(warp_id, b.const_inbounds_gep1_32(types.int, payload, 2));

    air.create_set_mesh_properties(air.get_int3(
        warp_primitive_count,
        geometry.cfg.decls.gs_instance_count,
        1,
    ))?;

    b.br(return_bb);
    b.position_at_end(return_bb);
    b.ret_void();

    add_entry(m, c"air.object", entry);
    Ok(())
}

/// A geometry shader's `emit` and `cut`: its vertices and primitives into the mesh.
///
/// The three allocas count the next vertex to write in the current strip, the vertices of the
/// strips already cut and their primitives. A strip's vertices are written once; the mesh's
/// indices say how they join.
struct MeshOutput<'a, 'cx> {
    air: &'a AirBuilder<'a, 'cx>,
    res: &'a Resources<'cx>,
    topology: MeshOutputTopology,
    next_write_vertex: Val<'cx>,
    vertex_offset: Val<'cx>,
    primitive_count: Val<'cx>,
    handlers: &'a [MeshOutputHandler],
    clip_distances: &'a [ScalarInfo],
    max_output_register: u32,
}

impl GeometryOutput for MeshOutput<'_, '_> {
    fn emit(&self) -> Result<(), Error> {
        let air = self.air;
        let b = air.builder();
        let t = air.types();
        let zero = air.get_int(0);
        let one = air.get_int(1);
        let two = air.get_int(2);
        match self.topology {
            MeshOutputTopology::Triangle => {
                let write = b.load(t.int, self.next_write_vertex);
                let offset = b.load(t.int, self.vertex_offset);
                let vertex = b.add(offset, write);

                let count = b.load(t.int, self.primitive_count);
                let primitive = b.add(count, write);

                let even_winding = b.zext(b.icmp(sys::INT_EQ, zero, b.and(write, one)), t.int);

                self.output_data(vertex, primitive)?;
                self.clip_distances(vertex)?;

                let triple = b.mul(primitive, air.get_int(3));
                air.create_set_mesh_index(b.add(triple, zero), vertex)?;
                air.create_set_mesh_index(
                    b.add(triple, one),
                    b.sub(b.add(vertex, two), even_winding),
                )?;
                air.create_set_mesh_index(
                    b.add(triple, two),
                    b.add(b.add(vertex, one), even_winding),
                )?;

                b.store(b.add(one, write), self.next_write_vertex);
            }
            MeshOutputTopology::Line => {
                let write = b.load(t.int, self.next_write_vertex);
                b.store(b.add(one, write), self.next_write_vertex);

                let offset = b.load(t.int, self.vertex_offset);
                let vertex = b.add(offset, write);

                let count = b.load(t.int, self.primitive_count);
                let primitive = b.add(count, write);

                self.output_data(vertex, primitive)?;
                self.clip_distances(vertex)?;

                let double = b.mul(primitive, two);
                air.create_set_mesh_index(b.add(double, zero), vertex)?;
                air.create_set_mesh_index(b.add(double, one), b.add(vertex, one))?;
            }
            MeshOutputTopology::Point => {
                // Only one accumulator to maintain.
                let write = b.load(t.int, self.next_write_vertex);
                b.store(b.add(one, write), self.next_write_vertex);

                self.output_data(write, write)?;
                self.clip_distances(write)?;
                air.create_set_mesh_index(write, write)?;
            }
        }
        Ok(())
    }

    fn cut(&self) -> Result<(), Error> {
        // A point has nothing to cut.
        let primitive_size = match self.topology {
            MeshOutputTopology::Triangle => 2,
            MeshOutputTopology::Line => 1,
            MeshOutputTopology::Point => return Ok(()),
        };
        let air = self.air;
        let b = air.builder();
        let t = air.types();
        let zero = air.get_int(0);

        let write = b.load(t.int, self.next_write_vertex);
        b.store(zero, self.next_write_vertex);

        let has_valid_primitive = b.icmp(sys::INT_UGT, write, air.get_int(primitive_size));
        let add_primitive_count = b.select(
            has_valid_primitive,
            b.sub(write, air.get_int(primitive_size)),
            zero,
        );
        let add_vertex_count = b.select(has_valid_primitive, write, zero);
        b.store(
            b.add(b.load(t.int, self.vertex_offset), add_vertex_count),
            self.vertex_offset,
        );
        b.store(
            b.add(b.load(t.int, self.primitive_count), add_primitive_count),
            self.primitive_count,
        );
        Ok(())
    }
}

impl<'cx> MeshOutput<'_, 'cx> {
    /// The geometry shader's output registers into the mesh vertex and primitive (the handlers).
    fn output_data(&self, vertex: Val<'cx>, primitive: Val<'cx>) -> Result<(), Error> {
        let tcx = self.air.builder().cx().tcx();
        for handler in self.handlers {
            match *handler {
                MeshOutputHandler::Position { reg, .. } => {
                    replay::pop_mesh_output_position(self.air, self.res, reg, vertex)?;
                }
                MeshOutputHandler::RenderTargetArrayIndex { reg, mask } => {
                    replay::pop_mesh_output_render_target_array_index(
                        self.air, self.res, reg, mask, primitive,
                    )?;
                }
                MeshOutputHandler::ViewportArrayIndex { reg, mask } => {
                    replay::pop_mesh_output_viewport_array_index(
                        self.air, self.res, reg, mask, primitive,
                    )?;
                }
                MeshOutputHandler::VertexData {
                    reg,
                    mask,
                    index,
                    ty,
                } => {
                    replay::pop_mesh_output_vertex_data(
                        self.air,
                        self.res,
                        reg,
                        mask,
                        index,
                        vertex,
                        value_ty(tcx, ty),
                    )?;
                }
            }
        }
        Ok(())
    }

    /// The clip distances of the output registers into the mesh vertex.
    fn clip_distances(&self, vertex: Val<'cx>) -> Result<(), Error> {
        let air = self.air;
        let b = air.builder();
        let t = air.types();
        let tcx = b.cx().tcx();
        let output = req(self.res.output.ptr_float4, "no output register file")?;
        let file_ty = tcx.array(t.float4, self.max_output_register);
        for (index, scalar) in (0_u32..).zip(self.clip_distances) {
            if u32::from(scalar.reg) >= self.max_output_register {
                continue;
            }
            let src = b.gep(
                file_ty,
                output,
                &[
                    air.get_int(0),
                    air.get_int(scalar.reg.into()),
                    air.get_int(scalar.component.into()),
                ],
            );
            air.create_set_mesh_clip_distance(vertex, air.get_int(index), b.load(t.float, src))?;
        }
        Ok(())
    }
}

/// An internal zero-initialized `i32` global in the threadgroup address space.
///
/// `new GlobalVariable`: it has no alignment of its own.
fn threadgroup_int<'cx>(m: &Module<'cx>, ty: Ty, global_name: &CStr) -> Result<Val<'cx>, Error> {
    let cx = m.cx();
    let tcx = cx.tcx();
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
    unsafe { sys::set_initializer(raw, cx.null(ty).raw()) };
    // SAFETY: the global just made.
    unsafe { sys::set_global_constant(raw, 0) };
    // SAFETY: a global of this context, a pointer to `ty` in the threadgroup address space.
    Ok(unsafe { cx.val(raw, tcx.ptr(ty, THREADGROUP)) })
}

/// The LLVM type of an MSL scalar or vector.
///
/// `value_ty` of `air::signature`, which keeps it private.
fn value_ty(tcx: &TyCx, v: MslValue) -> Ty {
    let scalar_ty = |s: MslScalar| match s {
        MslScalar::Float => tcx.float(),
        MslScalar::Int | MslScalar::Uint => tcx.int(32),
        MslScalar::Bool => tcx.int(1),
        MslScalar::Half => tcx.half(),
        MslScalar::Ushort => tcx.int(16),
        MslScalar::Ulong => tcx.int(64),
    };
    match v {
        MslValue::Vector(MslVector { dimension, scalar }) if dimension != 1 => {
            tcx.vector(scalar_ty(scalar), dimension)
        }
        MslValue::Scalar(scalar) | MslValue::Vector(MslVector { scalar, .. }) => scalar_ty(scalar),
    }
}

#[cfg(test)]
mod tests;
