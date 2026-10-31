//! The vertex shader's function, with stream output.
//!
//! [`convert`] is `convert_dxbc_vertex_shader` (`A/dxbc_converter.cpp:576-860`), in its order:
//! the handlers' pass (vertex pulling through the chain's input layout, else stage-in
//! attributes; no output with rasterization off), the output signature's user varyings, the
//! stream output's and the vertex and instance ids' inputs, the clip distances, the bindings,
//! the pass-through geometry shader's outputs, the function, its registers, the prologue, the
//! blocks, then the epilogue: the outputs, the stream output's stores, the clip distances and
//! the pass-through outputs, and the entry under `air.vertex`.

use airconv_ir::{
    binding::{
        AddressSpace, BufferBinding, FunctionInput, FunctionOutput, MemoryAccess, MslType,
        MslValue, count_u32,
    },
    handlers::{attribute_name, msl_value},
    initialize::DxbcShader,
};

use super::{
    binding_rootsig::setup_bindings,
    blocks,
    converter::{Converter, invalid, req},
    replay::{self, Options, input_layout, zero_return},
    resources::{
        Resources, setup_immediate_constant_buffer, setup_io_registers, setup_temp_register,
    },
};
use crate::{
    abi::{
        ArgChain, SM50_BINDING_INDEX_STREAM_OUTPUT0, SM50_SHADER_METAL_310,
        Sm50ShaderEmulateVertexStreamOutputData, Sm50StreamOutputElement,
    },
    air::{
        builder::AirBuilder,
        metadata::{add_entry, set_metal_version},
        signature::{Handles, create_function},
        ty::AirTypes,
    },
    api::Error,
    llvm::{Builder, Func, Module, sys},
};

/// A pass-through geometry shader's register for no output.
const NO_REGISTER: u8 = 255;
/// The constant address space, where the stream output table lives.
const CONSTANT: u32 = 2;

/// Lowers the vertex shader `shader` into `m` as the vertex function `name`.
///
/// # Errors
///
/// An unreadable root signature, an output of an unknown component type, and what the
/// lowering refuses.
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
    let vertex_so = args.stream_output;
    // SAFETY: the chain's layout (`SM50Compile`'s contract).
    let ia_layout = args.ia_layout.map(|ia| unsafe { input_layout(ia) });
    let rasterization_disabled = args
        .gs_pass_through
        .is_some_and(|pt| pt.rasterization_disabled != 0)
        || vertex_so.is_some();
    // SAFETY: both members of the union are four bytes of plain integers.
    let pass_through = args.gs_pass_through.map(|pt| unsafe { pt.u.data });
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
            ia_layout,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: rasterization_disabled,
        },
    )?;
    for out in &shader.output.rasterized().parameters {
        if out.system_value != 0 {
            continue;
        }
        desc.define_output(FunctionOutput::Vertex {
            user: attribute_name(out),
            ty: msl_value(out.component_type)?,
        });
    }
    let so_inputs = vertex_so.map(|_| {
        let base_vertex = desc.define_input(FunctionInput::BaseVertex);
        let vertex_id = desc.define_input(FunctionInput::VertexId);
        let table = desc.define_input(FunctionInput::Buffer(BufferBinding {
            buffer_size: None,
            location_index: SM50_BINDING_INDEX_STREAM_OUTPUT0,
            array_size: 0,
            memory_access: MemoryAccess::Read,
            address_space: AddressSpace::Constant,
            ty: MslType::Value(MslValue::UINT),
            arg_name: "stream_outputs".into(),
            raster_order_group: None,
        }));
        (base_vertex, vertex_id, table)
    });

    let vertex_idx = desc.define_input(FunctionInput::VertexId);
    let base_vertex_idx = desc.define_input(FunctionInput::BaseVertex);
    let instance_idx = desc.define_input(FunctionInput::InstanceId);
    let base_instance_idx = desc.define_input(FunctionInput::BaseInstance);
    let clip_count = io.clip_distance_scalars.len();
    let clip_distance_out = (clip_count != 0)
        .then(|| desc.define_output(FunctionOutput::ClipDistance { count: clip_count }));

    let binding = setup_bindings(shader, &mut desc, m, &mut handles, args)?;

    let rta_out = pass_through
        .filter(|pt| pt.render_target_array_index_reg != NO_REGISTER)
        .map(|pt| {
            let index = desc.define_output(FunctionOutput::RenderTargetArrayIndex);
            (
                index,
                pt.render_target_array_index_reg,
                pt.render_target_array_index_component,
            )
        });
    let va_out = pass_through
        .filter(|pt| pt.viewport_array_index_reg != NO_REGISTER)
        .map(|pt| {
            let index = desc.define_output(FunctionOutput::ViewportArrayIndex);
            (
                index,
                pt.viewport_array_index_reg,
                pt.viewport_array_index_component,
            )
        });

    let (function, entry) = create_function(m, &desc, name, rasterization_disabled, &handles)?;
    let no_block = || invalid("a block of no function");
    let entry_bb = function.append_block(cx, c"entry").ok_or_else(no_block)?;
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
            .ok_or_else(|| invalid("a vertex input of no argument"))
    };
    let vertex_id_with_base = param(vertex_idx)?;
    let base_vertex_id = param(base_vertex_idx)?;
    let instance_id_with_base = param(instance_idx)?;
    let base_instance_id = param(base_instance_idx)?;
    res.vertex_id_with_base = Some(vertex_id_with_base);
    res.base_vertex_id = Some(base_vertex_id);
    res.instance_id_with_base = Some(instance_id_with_base);
    res.base_instance_id = Some(base_instance_id);
    res.vertex_id = Some(b.sub(vertex_id_with_base, base_vertex_id));
    res.instance_id = Some(b.sub(instance_id_with_base, base_instance_id));

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
        pso_sample_mask: u32::MAX,
        shader_type: shader.program_type,
        metal_version,
        geometry: None,
    };
    let real_entry = blocks::convert(&conv, &shader.cfg.blocks, epilogue_bb)?;
    b.br(real_entry);

    b.position_at_end(epilogue_bb);
    let outputs = replay.epilogue(&air, &res, zero_return(cx, function), u32::MAX)?;
    let mut value = if let (Some(so), Some(inputs)) = (vertex_so, so_inputs) {
        stream_output(&air, function, &res, so, inputs)?;
        // The stream output's step answers no value: the function returns void.
        None
    } else {
        outputs
    };

    // A void function (rasterization disabled) returns no outputs, so no clip distances either.
    if let (Some(v), Some(out)) = (value, clip_distance_out) {
        let output = req(res.output.ptr_float4, "no output register file")?;
        let file_ty = tcx
            .pointee(output.ty())
            .ok_or_else(|| invalid("a register array is not a pointer"))?;
        let clip_ty = tcx.array(types.float, count_u32(clip_count));
        let mut clip_distances = cx.null(clip_ty);
        for (i, scalar) in (0_u32..).zip(&io.clip_distance_scalars) {
            let indices = [
                air.get_int(0),
                air.get_int(scalar.reg.into()),
                air.get_int(scalar.component.into()),
            ];
            let src = b.gep(file_ty, output, &indices);
            clip_distances = b.insert_value(clip_distances, b.load(types.float, src), i);
        }
        value = Some(b.insert_value(v, clip_distances, out));
    }

    match value {
        None => {
            b.ret_void();
        }
        Some(mut v) => {
            for (index, reg, component) in [rta_out, va_out].into_iter().flatten() {
                let output = req(res.output.ptr_int4, "no output register file")?;
                let file_ty = tcx
                    .pointee(output.ty())
                    .ok_or_else(|| invalid("a register array is not a pointer"))?;
                let indices = [
                    air.get_int(0),
                    air.get_int(reg.into()),
                    air.get_int(component.into()),
                ];
                let src = b.gep(file_ty, output, &indices);
                v = b.insert_value(v, b.load(types.int, src), index);
            }
            b.ret(v);
        }
    }

    add_entry(m, c"air.vertex", entry);
    Ok(())
}

/// Each stream output element's register component stored to buffer 0 at its vertex's place.
///
/// The emulated stream output's epilogue: `inputs` are the base vertex, vertex id and stream
/// output table arguments.
fn stream_output<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    res: &Resources<'cx>,
    so: &Sm50ShaderEmulateVertexStreamOutputData,
    inputs: (u32, u32, u32),
) -> Result<(), Error> {
    let b = air.builder();
    let t = air.types();
    let tcx = b.cx().tcx();
    let (bv, vid, so_table) = inputs;
    let param = |index| {
        function
            .param(tcx, index)
            .ok_or_else(|| invalid("a stream output input of no argument"))
    };
    let base_vertex = param(bv)?;
    let vertex_id = param(vid)?;
    let entry_ty = t.dxmt_stream_output_buffer_entry;
    let entries = b.bitcast(param(so_table)?, tcx.ptr(entry_ty, CONSTANT));
    let slot0_entry = b.load(entry_ty, b.const_inbounds_gep1_32(entry_ty, entries, 0));
    let slot0 = b.extract_value(slot0_entry, 0);
    let adjusted_vertex_id = b.sub(vertex_id, base_vertex);
    let output = req(res.output.ptr_int4, "no output register file")?;
    let output_regs = b.bit_or_pointer_cast(output, tcx.ptr(t.int, 0));
    // SAFETY: the chain's stream output (`SM50Compile`'s contract).
    for element in unsafe { elements(so) } {
        if element.reg_id == u32::MAX {
            continue;
        }
        let at = element
            .reg_id
            .wrapping_mul(4)
            .wrapping_add(element.component);
        let ptr = b.const_gep1_32(t.int, output_regs, at);
        // The C++ read past `strides` for a slot above 3.
        let stride = so
            .strides
            .get(element.output_slot as usize)
            .copied()
            .ok_or_else(|| invalid("a stream output slot above 3"))?;
        let target_offset = b.add(
            b.mul(adjusted_vertex_id, air.get_int(stride)),
            air.get_int(element.offset),
        );
        let two = b.cx().const_int(t.int, 2, false);
        let target_ptr = b.gep(t.int, slot0, &[b.lshr(target_offset, two)]);
        let store = b.store(b.load(t.int, ptr), target_ptr);
        // SAFETY: the store just made.
        unsafe { sys::set_volatile(store.raw(), 1) };
    }
    Ok(())
}

/// The stream output's elements.
///
/// # Safety
///
/// `so.elements` points to `so.num_elements` live elements that live as long as `so`
/// (`SM50Compile`'s contract for its argument chain), or `so.num_elements` is 0.
const unsafe fn elements(
    so: &Sm50ShaderEmulateVertexStreamOutputData,
) -> &[Sm50StreamOutputElement] {
    if so.num_elements == 0 || so.elements.is_null() {
        return &[];
    }
    // SAFETY: the caller's contract.
    unsafe { core::slice::from_raw_parts(so.elements, so.num_elements as usize) }
}
