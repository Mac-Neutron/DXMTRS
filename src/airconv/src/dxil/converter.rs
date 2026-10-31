//! The DXIL converter: a module's blocks moved into the AIR function.
//!
//! Ported from `A/dxil/dxil_converter.cpp`. [`convert`] (`ConvertDXIL`) parses the shader's
//! bitcode into the compile's context, builds the AIR entry function as the DXBC compute,
//! vertex and pixel compiles do (signature handlers, bindings, IO registers, prologue), moves
//! the DXIL entry's blocks into it, lowers every `dx.op` call (`lower.rs`), and leaves the module
//! without what only DXIL tools read. [`convert_geometry_pipeline`]
//! (`ConvertDXILGeometryPipeline`) gives the DXBC geometry pipeline's two functions a DXIL body.
//! [`strip_fast_math`] applies DXIL's float policy after the Metal helpers are linked.

use airconv_ir::binding::{FunctionInput, FunctionOutput};

use super::{
    initialize::DxilShader,
    lower::{Lowering, erase_terminator, has_uses, lift, walk_blocks},
    metadata::{op_code, value_name},
    probe,
    types::{EntryInfo, ResourceFlags, resource_class, shader_kind},
};
use crate::{
    abi::{
        ArgChain, SM50_BINDING_INDEX_ROOT_ARGUMENTS, SM50_BINDING_INDEX_STATIC_SAMPLERS,
        SM50_SHADER_METAL_310,
    },
    air::{
        builder::{AirBuilder, MemFlags},
        metadata::{add_entry, set_metal_version},
        signature::{Handles, create_function},
        ty::AirTypes,
    },
    api::Error,
    dxbc::{
        binding_rootsig::{root_signature_bytes, root_signature_flags, setup_binding_rootsig},
        binding_sm50::Sm50Bindings,
        converter::{BindingMap, Converter, invalid, req},
        gs::{self, Stage},
        replay::{Options, input_layout, zero_return},
        resources::{Resources, setup_io_registers},
    },
    llvm::{self, Block, Builder, Context, FastMath, Module, Val, sys},
    module,
    shader::Shader,
};

/// The function's attribute index (`~0U`).
const FN: sys::AttributeIndex = sys::ATTRIBUTE_FUNCTION_INDEX;
/// The constant address space, where `static const` arrays move.
const CONSTANT: u32 = 2;
/// The threadgroup address space.
const THREADGROUP: u32 = 3;
/// `CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED` and `SAMPLER_HEAP_DIRECTLY_INDEXED`.
const VIEW_HEAP_INDEXED: u32 = 0x400;
const SAMPLER_HEAP_INDEXED: u32 = 0x800;
/// The function-level fast math Metal's compiler gives the helpers.
const FAST_MATH_ATTRIBUTES: [&[u8]; 5] = [
    b"unsafe-fp-math",
    b"no-nans-fp-math",
    b"no-infs-fp-math",
    b"no-signed-zeros-fp-math",
    b"approx-func-fp-math",
];

/// `SM50Compile`'s DXIL branch: the shader's AIR module, ready for airconv's passes.
///
/// `ConvertDXIL`: from the parsed module on, as the DXBC compute shader's compile, except that
/// the body comes from DXIL.
///
/// # Errors
///
/// The C++'s `DXIL: …` refusals (a geometry shader alone, heap flags, the entry function, the
/// lowering's, the module's verification), the reader's, and what the handlers and bindings
/// refuse.
pub fn convert<'cx>(
    shader: &DxilShader,
    name: &[u8],
    cx: &'cx Context,
    args: &ArgChain<'_>,
) -> Result<Module<'cx>, Error> {
    let entry = &shader.entry;
    if entry.kind == shader_kind::GEOMETRY {
        return Err(Error::new(
            b"DXIL: a geometry shader compiles only with its vertex shader",
        ));
    }
    check_heap_flags(entry, args)?;
    let (m, dxil_main) = load(shader, cx)?;

    let tcx = cx.tcx();
    let mut desc = shader.io.func_signature.clone();
    let (metal_version, shader_flags) = args
        .common
        .map_or((SM50_SHADER_METAL_310, 0), |c| (c.metal_version, c.flags));
    // SAFETY: the chain's layout (`SM50Compile`'s contract).
    let ia_layout = args.ia_layout.map(|ia| unsafe { input_layout(ia) });
    let pso_sample_mask = args.pixel.map_or(u32::MAX, |p| p.sample_mask);
    let vertex = entry.kind == shader_kind::VERTEX;
    let pixel = entry.kind == shader_kind::PIXEL;

    let replay = shader.io.record(
        &mut desc,
        &Options {
            ia_layout,
            pixel: args.pixel,
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    )?;
    // As the DXBC pixel shader's compile.
    let sample_mask_out = (pixel && pso_sample_mask != u32::MAX)
        .then(|| desc.define_output(FunctionOutput::CoverageMask));
    // Vertex shaders: the IDs as the DXBC vertex shader's compile defines them.
    let ids = vertex.then(|| {
        (
            desc.define_input(FunctionInput::VertexId),
            desc.define_input(FunctionInput::BaseVertex),
            desc.define_input(FunctionInput::InstanceId),
            desc.define_input(FunctionInput::BaseInstance),
        )
    });
    let mut handles = Handles::default();
    let binding = setup_bindings(shader, &mut desc, &m, &mut handles, args)?;

    let (function, function_entry) = create_function(&m, &desc, name, false, &handles)?;
    let no_block = || invalid("a block of no function");
    let entry_bb = function.append_block(cx, c"entry").ok_or_else(no_block)?;
    let epilogue_bb = function
        .append_block(cx, c"epilogue")
        .ok_or_else(no_block)?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let types = AirTypes::new(tcx);
    let air = AirBuilder::new(&b, &m, &types, shader_flags);
    set_metal_version(&m, metal_version);

    let mut res = Resources::default();
    if let Some((vertex_idx, base_vertex_idx, instance_idx, base_instance_idx)) = ids {
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
    }
    if vertex || pixel {
        // The register files `loadInput`/`storeOutput` address (element row, column).
        let (inputs, outputs) = (shader.io.max_input_register, shader.io.max_output_register);
        setup_io_registers(&mut res, &types, &b, inputs.max(1), outputs.max(1));
        res.input_element_count = inputs;
        res.output_element_count = outputs;
    }
    replay.prologue(&air, function, &mut res)?;
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: binding.as_ref(),
        res: &res,
        types: &types,
        pso_sample_mask,
        shader_type: shader.program_type,
        metal_version,
        geometry: None,
    };
    let body = lower_body(shader, &conv, &m, dxil_main, epilogue_bb, None)?;
    b.br(body);

    b.position_at_end(epilogue_bb);
    let mut value = replay.epilogue(&air, &res, zero_return(cx, function), pso_sample_mask)?;
    if let Some(index) = sample_mask_out
        && res.coverage_mask_reg.is_none()
    {
        let v = value.ok_or_else(|| invalid("an output of a function that returns void"))?;
        value = Some(b.insert_value(v, air.get_int(pso_sample_mask), index));
    }
    value.map_or_else(|| b.ret_void(), |v| b.ret(v));

    let stage = if vertex {
        c"air.vertex"
    } else if pixel {
        c"air.fragment"
    } else {
        c"air.kernel"
    };
    add_entry(&m, stage, function_entry);
    finish(&m, dxil_main)?;
    Ok(m)
}

/// A geometry pipeline function with a DXIL body (`ConvertDXILGeometryPipeline`).
///
/// `object`: the vertex shader as the object function (`SM50CompileGeometryPipelineVertex`),
/// else the geometry shader as the mesh function (`…Geometry`), whose inputs are relinked to
/// the vertex shader's outputs by semantic (as the varyings between stages).
///
/// # Errors
///
/// `DXIL: a geometry pipeline needs a DXIL vertex and geometry shader`, then as [`convert`].
pub fn convert_geometry_pipeline<'cx>(
    object: bool,
    vertex: &Shader,
    geometry: &Shader,
    name: &[u8],
    cx: &'cx Context,
    args: &ArgChain<'_>,
) -> Result<Module<'cx>, Error> {
    let (Shader::Dxil(vs), Shader::Dxil(gs)) = (vertex, geometry) else {
        return Err(Error::new(
            b"DXIL: a geometry pipeline needs a DXIL vertex and geometry shader",
        ));
    };
    let (vs, gs) = (&vs.ir, &gs.ir);
    if vs.entry.kind != shader_kind::VERTEX || gs.entry.kind != shader_kind::GEOMETRY {
        return Err(Error::new(
            b"DXIL: a geometry pipeline needs a DXIL vertex and geometry shader",
        ));
    }
    let stage = if object { vs } else { gs };
    check_heap_flags(&stage.entry, args)?;
    let (m, dxil_main) = load(stage, cx)?;
    let vertex_outputs = (!object).then_some(&vs.entry);
    let body = |conv: &Converter<'_, 'cx>, epilogue: Block<'cx>| {
        lower_body(stage, conv, &m, dxil_main, epilogue, vertex_outputs)
    };
    if object {
        gs::convert_vertex(Stage::Dxil(vs), Stage::Dxil(gs), name, &m, args, body)?;
    } else {
        gs::convert_geometry(Stage::Dxil(gs), Stage::Dxil(vs), name, &m, args, body)?;
    }
    finish(&m, dxil_main)?;
    Ok(m)
}

/// A DXIL stage's bindings: the chain's root signature's when it has one, else D3D11's tables.
///
/// `setup_binding_rootsig` or `setup_binding_table2` at their default slots; a DXIL shader has
/// no D3D11 argument table, so the latter defines nothing.
///
/// # Errors
///
/// An unreadable root signature and what the binding setup refuses.
pub fn setup_bindings<'s, 'cx>(
    shader: &'s DxilShader,
    desc: &mut airconv_ir::binding::FunctionSignatureDesc,
    m: &Module<'_>,
    handles: &mut Handles,
    args: &ArgChain<'_>,
) -> Result<Box<dyn BindingMap<'cx> + 's>, Error> {
    let info = &shader.info;
    Ok(match args.root_signature {
        Some(rootsig) => {
            // SAFETY: the chain's root signature (`SM50Compile`'s contract).
            let bytecode = unsafe { root_signature_bytes(rootsig) };
            Box::new(setup_binding_rootsig(
                info,
                desc,
                m,
                handles,
                shader.program_type,
                bytecode,
                (
                    SM50_BINDING_INDEX_ROOT_ARGUMENTS,
                    SM50_BINDING_INDEX_STATIC_SAMPLERS,
                ),
            )?)
        }
        None => Box::new(Sm50Bindings {
            constant_buffer_table_index: u32::MAX,
            binding_table_index: u32::MAX,
            constant_buffers: &info.cbuffer_map,
            samplers: &info.sampler_map,
            srvs: &info.srv_map,
            uavs: &info.uav_map,
        }),
    })
}

/// DXIL's float policy on every function `m` defines, their fast-math attributes dropped.
///
/// `StripFastMath`: run after airconv's Metal helpers (msad, sample positions) are linked,
/// which Metal compiled with fast math. `kind` is the DXIL stage.
pub fn strip_fast_math(m: &Module<'_>, kind: u32) {
    // SAFETY: a live module.
    let first = unsafe { sys::get_first_function(m.raw()) };
    for function in super::metadata::walk(first, |f| {
        // SAFETY: a function of the module.
        unsafe { sys::get_next_function(f) }
    }) {
        // SAFETY: a function of the module.
        if unsafe { sys::is_declaration(function) } != 0 {
            continue;
        }
        for attribute in FAST_MATH_ATTRIBUTES {
            // SAFETY: a function; the name holds its length.
            unsafe {
                sys::remove_string_attribute_at_index(
                    function,
                    FN,
                    attribute.as_ptr().cast(),
                    u32::try_from(attribute.len()).unwrap_or(0),
                );
            }
        }
        // SAFETY: a defined function.
        for bb in walk_blocks(unsafe { sys::get_first_basic_block(function) }) {
            for_each_instruction(bb, |inst| strip_fast_math_of(inst, pre_raster(kind)));
        }
    }
}

/// `MoveConstantsToConstantSpace`: DXIL's `static const` arrays into constant space.
///
/// DXIL keeps dynamically indexed `static const` arrays as address space 0 globals, which Metal
/// cannot link ("Undefined symbols"); they belong in constant space, as airconv's immediate
/// constant buffers. Mutable statics are not supported.
///
/// # Errors
///
/// `DXIL: static (non-const) global <name> not supported` (the name's bytes as they are, P15),
/// and the uses the move cannot follow.
fn move_constants_to_constant_space(m: &Module<'_>) -> Result<(), Error> {
    let cx = m.cx();
    // SAFETY: a live module.
    let mut global = unsafe { sys::get_first_global(m.raw()) };
    while !global.is_null() {
        let gv = global;
        // SAFETY: a global of the module.
        global = unsafe { sys::get_next_global(gv) };
        let space = addrspace_of(gv);
        let name = value_name(gv);
        if space != 0 || name.starts_with(b"llvm.") {
            continue;
        }
        // SAFETY: a global variable.
        let initializer = unsafe { sys::get_initializer(gv) };
        // SAFETY: as above.
        if unsafe { sys::is_global_constant(gv) } == 0 || initializer.is_null() {
            return Err(Error::new(
                &[
                    &b"DXIL: static (non-const) global "[..],
                    &name,
                    b" not supported",
                ]
                .concat(),
            ));
        }
        // SAFETY: a global variable of the module.
        let value_ty = unsafe { sys::global_get_value_type(gv) };
        // SAFETY: a live module; a type of its context; the empty name.
        let moved =
            unsafe { sys::add_global_in_address_space(m.raw(), value_ty, c"".as_ptr(), CONSTANT) };
        // SAFETY: the global just made, and the old one's linkage and initializer, of one module.
        unsafe {
            sys::set_global_constant(moved, 1);
        }
        // SAFETY: a global of the module.
        let linkage = unsafe { sys::get_linkage(gv) };
        // SAFETY: the global just made.
        unsafe { sys::set_linkage(moved, linkage) };
        // SAFETY: as above.
        unsafe { sys::set_initializer(moved, initializer) };
        // `takeName`: the old global gives its name up first, so the new one has it exactly.
        // SAFETY: two globals of the module; the names hold their lengths.
        unsafe { sys::set_value_name2(gv, c"".as_ptr(), 0) };
        // SAFETY: as above.
        unsafe { sys::set_value_name2(moved, name.as_ptr().cast(), name.len()) };
        // SAFETY: a global of the module.
        let align = unsafe { sys::get_alignment(gv) };
        // SAFETY: the global just made; an alignment of 0 is none, as `MaybeAlign` takes it.
        unsafe { sys::set_alignment(moved, align) };
        retarget(cx, gv, moved)?;
        // SAFETY: a global nothing uses now.
        unsafe { sys::delete_global(gv) };
    }
    Ok(())
}

/// The shader's module, set up for AIR, and its DXIL entry function, renamed (`Load`).
///
/// The caller may ask for the entry's own name (`main`), so it becomes `dxil.entry`.
fn load<'cx>(shader: &DxilShader, cx: &'cx Context) -> Result<(Module<'cx>, sys::ValueRef), Error> {
    let m = module::parse(
        cx,
        &shader.bitcode,
        b"dxil",
        b"DXIL: can't read the bitcode: ",
        module::Layout::Dxil,
    )?;
    let entry_name = llvm::name(&shader.entry.name)?;
    // SAFETY: a live module; a NUL-terminated name.
    let dxil_main = unsafe { sys::get_named_function(m.raw(), entry_name.as_ptr()) };
    if dxil_main.is_null() {
        return Err(Error::new(b"DXIL: entry function missing"));
    }
    let renamed = b"dxil.entry";
    // SAFETY: a function of the module; the name holds its length.
    unsafe { sys::set_value_name2(dxil_main, renamed.as_ptr().cast(), renamed.len()) };
    // The AIR triple, data layout, SDK version and module flags.
    module::set_up(&m);
    Ok((m, dxil_main))
}

/// A shader that indexes a descriptor heap needs its root signature to say so (`CheckHeapFlags`).
///
/// Only then do the heap bases follow its parameters in the root arguments (spec 3.4). The
/// view heap's flag is checked first.
fn check_heap_flags(entry: &EntryInfo, args: &ArgChain<'_>) -> Result<(), Error> {
    let (mut views, mut samplers) = (false, false);
    for r in &entry.resources {
        if r.flags.contains(ResourceFlags::HEAP) {
            if r.cls == resource_class::SAMPLER {
                samplers = true;
            } else {
                views = true;
            }
        }
    }
    if !views && !samplers {
        return Ok(());
    }
    let Some(rootsig) = args.root_signature else {
        return Err(Error::new(
            b"DXIL: ResourceDescriptorHeap needs a root signature",
        ));
    };
    // SAFETY: the chain's root signature (`SM50Compile`'s contract).
    let flags = root_signature_flags(unsafe { root_signature_bytes(rootsig) });
    if views && flags & VIEW_HEAP_INDEXED == 0 {
        return Err(Error::new(
            b"DXIL: ResourceDescriptorHeap needs root signature flag CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED (0x400)",
        ));
    }
    if samplers && flags & SAMPLER_HEAP_INDEXED == 0 {
        return Err(Error::new(
            b"DXIL: SamplerDescriptorHeap needs root signature flag SAMPLER_HEAP_DIRECTLY_INDEXED (0x800)",
        ));
    }
    Ok(())
}

/// The DXIL entry's blocks moved into the function being built, lowered (`LowerBody`).
///
/// They go to the end of the function, its returns become branches to `epilogue`, and every
/// `dx.op` call is lowered. Answers the block that enters the body. The builder's position is
/// kept (the caller branches to the body from where its prologue ended; the prologue may have
/// added blocks, e.g. vertex pulls).
fn lower_body<'cx>(
    dxil: &DxilShader,
    conv: &Converter<'_, 'cx>,
    m: &Module<'cx>,
    dxil_main: sys::ValueRef,
    epilogue: Block<'cx>,
    vertex_outputs: Option<&EntryInfo>,
) -> Result<Block<'cx>, Error> {
    let cx = m.cx();
    probe::apply(dxil, cx, dxil_main)?;
    let ip = conv
        .ir
        .insert_block()
        .ok_or_else(|| invalid("an unpositioned builder"))?;
    // SAFETY: the DXIL entry function, defined.
    let body = unsafe { sys::get_entry_basic_block(dxil_main) };
    // SAFETY: a block of this context.
    let body = unsafe { Block::new(body) };
    // SAFETY: as above.
    let blocks: Vec<_> = walk_blocks(unsafe { sys::get_first_basic_block(dxil_main) }).collect();
    let function = conv.function.val().raw();
    for &bb in &blocks {
        // SAFETY: the function being built has blocks (its entry at least).
        let last = unsafe { sys::get_last_basic_block(function) };
        // SAFETY: `bb` a block of the DXIL entry, moved after the function's last (`splice` of
        // the whole list, in order).
        unsafe { sys::move_basic_block_after(bb, last) };
    }
    // Collected first: lowering erases them.
    let mut calls = Vec::new();
    // 64-bit groupshared atomics (Task 14c).
    let mut gs64 = Vec::new();
    let pre_raster = pre_raster(dxil.entry.kind);
    let lift = |raw| lift(cx, raw);
    for &bb in &blocks {
        // SAFETY: a block of the function being built.
        let ret = unsafe { sys::get_basic_block_terminator(bb) };
        // SAFETY: null checked first; else an instruction.
        if !ret.is_null() && !unsafe { sys::is_a_return_inst(ret) }.is_null() {
            // `BranchInst::Create(epilogue, ret)`: before the return, which then goes.
            let to_epilogue = Builder::new(cx);
            to_epilogue.position_before(lift(ret)?);
            to_epilogue.br(epilogue);
            // SAFETY: the block's return, replaced by the branch before it.
            unsafe { sys::instruction_erase_from_parent(ret) };
        }
        let mut scan = Ok(());
        for_each_instruction(bb, |inst| {
            strip_fast_math_of(inst, pre_raster);
            if scan.is_err() {
                return;
            }
            if super::metadata::is_call(inst) && op_code(inst) != u32::MAX {
                match lift(inst) {
                    Ok(call) => calls.push(call),
                    Err(e) => scan = Err(e),
                }
            }
            if is_groupshared64(inst) {
                match lift(inst) {
                    Ok(atomic) => gs64.push(atomic),
                    Err(e) => scan = Err(e),
                }
            }
        });
        scan?;
    }
    let lowering = Lowering {
        conv,
        dxil,
        entry: &dxil.entry,
        vertex_outputs,
    };
    for &call in &calls {
        // SAFETY: each call once; `calls` is not read after this loop.
        unsafe { lowering.lower(call) }?;
    }
    if dxil.gs_locks != 0 {
        let [x, y, z] = dxil.entry.numthreads;
        let group = x.wrapping_mul(y).wrapping_mul(z);
        let locks = zero_group_shared_locks(conv, m, body, dxil.gs_locks, group)?;
        for &inst in &gs64 {
            // SAFETY: each atomic once; `gs64` is not read after this loop.
            unsafe { lowering.group_shared64(inst, locks) }?;
        }
    }
    // SAFETY: the function being built; `calls`, which holds its handle-producing calls, is not
    // read again.
    unsafe { Lowering::cleanup(function) };
    conv.ir.position_at_end(ip);
    Ok(body)
}

/// The 64-bit groupshared atomics' lock array, zeroed at the body's start (Task 14c).
///
/// `ZeroGroupSharedLocks`: an `[n x i32] addrspace(3)` global, as TGSM is; thread i (its
/// flattened index in the group) clears slots i, i + group, ..., then one threadgroup barrier,
/// before any user code (the entry block is uniform).
fn zero_group_shared_locks<'cx>(
    conv: &Converter<'_, 'cx>,
    m: &Module<'cx>,
    body: Block<'cx>,
    n: u32,
    group: u32,
) -> Result<Val<'cx>, Error> {
    let cx = m.cx();
    let tcx = cx.tcx();
    let b = conv.ir;
    let i32_ty = tcx.int(32);
    let ty = tcx.array(i32_ty, n);
    // SAFETY: a live module, a type of its context and a NUL-terminated name.
    let raw = unsafe {
        sys::add_global_in_address_space(m.raw(), tcx.lower(ty), c"gs_locks".as_ptr(), THREADGROUP)
    };
    // SAFETY: the global just made.
    unsafe { sys::set_linkage(raw, sys::INTERNAL_LINKAGE) };
    // SAFETY: the global and an undef of its value type.
    unsafe { sys::set_initializer(raw, cx.undef(ty).raw()) };
    // SAFETY: the global just made.
    unsafe { sys::set_alignment(raw, 4) };
    // SAFETY: a global of this context, a pointer to `ty` in the threadgroup address space.
    let locks = unsafe { cx.val(raw, tcx.ptr(ty, THREADGROUP)) };
    // The body's code moves on; `body` stays the entry.
    // SAFETY: the body's entry block, which holds instructions.
    let first = unsafe { sys::get_first_instruction(body.raw()) };
    // SAFETY: as above; `first` is its first instruction.
    let rest = unsafe { sys::airconv_shim_split_block(body.raw(), first) };
    // SAFETY: the block the split made, of this context.
    let rest = unsafe { Block::new(rest) };
    // SAFETY: `body`'s terminator is the branch the split just made, held by nothing.
    unsafe { erase_terminator(body) };
    let looped = rest.insert_before(cx, c"");
    let after = rest.insert_before(cx, c"");
    let first = req(
        conv.res.thread_id_in_group_flat_arg,
        "no flattened thread id",
    )?;
    b.position_at_end(body);
    let int = |v| cx.const_int(i32_ty, v, false);
    b.cond_br(b.icmp(sys::INT_ULT, first, int(n.into())), looped, after);
    b.position_at_end(looped);
    let i = b.phi(i32_ty);
    b.add_incoming(i, first, body);
    let slot = b.inbounds_gep(ty, locks, &[int(0), i]);
    b.aligned_store(int(0), slot, 4);
    let next = b.add(i, int(group.into()));
    b.add_incoming(i, next, looped);
    b.cond_br(b.icmp(sys::INT_ULT, next, int(n.into())), looped, after);
    b.position_at_end(after);
    conv.air.create_barrier(MemFlags::THREADGROUP, false)?;
    b.br(rest);
    Ok(locks)
}

/// What is left of DXIL removed once the AIR function is built, and the module checked.
///
/// `Finish`: the emptied entry and the unused `dx.op` declarations go, `static const` arrays
/// move to constant space, DXIL's metadata and type names go, then the verifier.
fn finish(m: &Module<'_>, dxil_main: sys::ValueRef) -> Result<(), Error> {
    // SAFETY: the DXIL entry function, emptied of its blocks.
    unsafe { sys::delete_function(dxil_main) };
    // SAFETY: a live module.
    let mut function = unsafe { sys::get_first_function(m.raw()) };
    while !function.is_null() {
        let this = function;
        // SAFETY: a function of the module.
        function = unsafe { sys::get_next_function(this) };
        if value_name(this).starts_with(b"dx.op.") && !has_uses(this) {
            // SAFETY: a function nothing uses.
            unsafe { sys::delete_function(this) };
        }
    }
    move_constants_to_constant_space(m)?;
    strip_dxil(m);
    let mut message = core::ptr::null_mut();
    // SAFETY: a live module; the message is written once and disposed of below.
    let broken =
        unsafe { sys::verify_module(m.raw(), sys::RETURN_STATUS_ACTION, &raw mut message) } != 0;
    let text = if message.is_null() {
        Vec::new()
    } else {
        // SAFETY: the verifier's NUL-terminated message.
        let bytes = unsafe { core::ffi::CStr::from_ptr(message) }
            .to_bytes()
            .to_vec();
        // SAFETY: the verifier's message, freed once.
        unsafe { sys::dispose_message(message) };
        bytes
    };
    if let Some(error) = m.cx().take_error() {
        return Err(error);
    }
    if broken {
        return Err(Error::new(
            &[&b"DXIL: invalid module after lowering: "[..], &text].concat(),
        ));
    }
    Ok(())
}

/// What only DXIL tools read removed: named metadata, instruction metadata, DXIL's type names.
///
/// `StripDXIL`: the `dx.*` and `llvm.ident` named metadata, every attachment but the debug
/// location, and `dx.types.*` struct names, which become `dxil.*`.
fn strip_dxil(m: &Module<'_>) {
    let mut named = Vec::new();
    // SAFETY: a live module.
    let mut node = unsafe { sys::get_first_named_metadata(m.raw()) };
    while !node.is_null() {
        let mut len = 0;
        // SAFETY: a named metadata node of the module; `len` is writable.
        let name = unsafe { sys::get_named_metadata_name(node, &raw mut len) };
        let name = if name.is_null() {
            Vec::new()
        } else {
            // SAFETY: LLVM answers `len` bytes at `name`.
            unsafe { core::slice::from_raw_parts(name.cast::<u8>(), len) }.to_vec()
        };
        if name.starts_with(b"dx.") || name == b"llvm.ident" {
            named.push(name);
        }
        // SAFETY: as above.
        node = unsafe { sys::get_next_named_metadata(node) };
    }
    for name in named {
        // SAFETY: a live module; the name holds its length.
        unsafe {
            sys::airconv_shim_erase_named_metadata(m.raw(), name.as_ptr().cast(), name.len());
        };
    }
    // SAFETY: a live module.
    let first = unsafe { sys::get_first_function(m.raw()) };
    for function in super::metadata::walk(first, |f| {
        // SAFETY: a function of the module.
        unsafe { sys::get_next_function(f) }
    }) {
        // SAFETY: a function of the module (none for a declaration).
        for bb in walk_blocks(unsafe { sys::get_first_basic_block(function) }) {
            for_each_instruction(bb, drop_metadata);
        }
    }
    let (from, to) = (b"dx.types.", b"dxil.");
    // SAFETY: a live module; the names hold their lengths.
    unsafe {
        sys::airconv_shim_rename_struct(
            m.raw(),
            from.as_ptr().cast(),
            from.len(),
            to.as_ptr().cast(),
            to.len(),
        );
    }
}

/// Every use of the pointer `from` pointed at `to`, the same object in another address space.
///
/// `Retarget`: GEPs, bitcasts and loads are rebuilt or repointed; constant expressions become
/// instructions first. The new GEPs and bitcasts are instructions, as the C++'s constructors
/// made them, never folded into constants (a builder would fold one of a global).
fn retarget(cx: &Context, from: sys::ValueRef, to: sys::ValueRef) -> Result<(), Error> {
    for_each_user(from, |user| {
        // SAFETY: a user of a value of the module.
        if unsafe { sys::is_a_constant_expr(user) }.is_null() {
            return Ok(());
        }
        for_each_user(user, |ce_user| {
            // SAFETY: as above.
            if unsafe { sys::is_a_instruction(ce_user) }.is_null() {
                return Err(Error::new(
                    b"DXIL: a static const array in a constant initializer",
                ));
            }
            // SAFETY: a constant expression and the instruction it is expanded before.
            let expanded = unsafe { sys::airconv_shim_get_as_instruction(user, ce_user) };
            replace_uses_of_with(ce_user, user, expanded);
            Ok(())
        })?;
        // SAFETY: a constant expression nothing uses now.
        unsafe { sys::airconv_shim_destroy_constant(user) };
        Ok(())
    })?;
    let space = addrspace_of(to);
    for_each_user(from, |user| {
        // SAFETY: a user of a value of the module (each check below likewise).
        let gep = !unsafe { sys::is_a_get_element_ptr_inst(user) }.is_null();
        // SAFETY: as above.
        let cast = !unsafe { sys::is_a_bit_cast_inst(user) }.is_null();
        // SAFETY: as above.
        let load = !unsafe { sys::is_a_load_inst(user) }.is_null();
        // SAFETY: as above.
        let intrinsic = !unsafe { sys::is_a_intrinsic_inst(user) }.is_null();
        if gep {
            let moved = rebuilt(cx, user, to, |b, placeholder| {
                // SAFETY: a GEP instruction.
                let source = unsafe { sys::get_gep_source_element_type(user) };
                // SAFETY: as above: its operands after the pointer are its indices.
                let count = unsafe { sys::get_num_operands(user) };
                let mut indices: Vec<sys::ValueRef> = (1..u32::try_from(count).unwrap_or(0))
                    // SAFETY: as above.
                    .map(|i| unsafe { sys::get_operand(user, i) })
                    .collect();
                // SAFETY: a GEP; the builder is positioned before it.
                let inbounds = unsafe { sys::is_in_bounds(user) } != 0;
                let build = if inbounds {
                    sys::build_in_bounds_gep2
                } else {
                    sys::build_gep2
                };
                // SAFETY: the builder, the pointer and the indices are of one context;
                // `indices` holds its length.
                unsafe {
                    build(
                        b,
                        source,
                        placeholder,
                        indices.as_mut_ptr(),
                        u32::try_from(indices.len()).unwrap_or(0),
                        c"".as_ptr(),
                    )
                }
            });
            retarget(cx, user, moved)?;
            // SAFETY: a GEP nothing uses now.
            unsafe { sys::instruction_erase_from_parent(user) };
        } else if cast {
            let moved = rebuilt(cx, user, to, |b, placeholder| {
                // SAFETY: a bitcast of the module.
                let destination = unsafe { sys::type_of(user) };
                // SAFETY: a bitcast's destination is a pointer type.
                let element = unsafe { sys::get_element_type(destination) };
                // SAFETY: a type of the module's context.
                let ty = unsafe { sys::pointer_type(element, space) };
                // SAFETY: the builder and the pointer are of one context.
                unsafe { sys::build_bit_cast(b, placeholder, ty, c"".as_ptr()) }
            });
            retarget(cx, user, moved)?;
            // SAFETY: a bitcast nothing uses now.
            unsafe { sys::instruction_erase_from_parent(user) };
            if !has_uses(moved) {
                // It only fed lifetime markers.
                // SAFETY: an unused instruction.
                unsafe { sys::instruction_erase_from_parent(moved) };
            }
        } else if load {
            // SAFETY: a load, whose pointer is operand 0.
            unsafe { sys::set_operand(user, 0, to) };
        } else if intrinsic && is_lifetime_marker(user) {
            // DXC hoists a local array of constants to a global (<name>.hca) and keeps the
            // array's lifetime markers on it (Palworld's NeighborOffset, SM 6.6): they mean
            // nothing on a constant.
            // SAFETY: a call nothing uses.
            unsafe { sys::instruction_erase_from_parent(user) };
        } else {
            return Err(Error::new(
                b"DXIL: a static const array used other than by loads",
            ));
        }
        Ok(())
    })
}

/// A GEP or bitcast rebuilt before `old` over `to`, named as `old` (an instruction).
///
/// `build` makes it over a `freeze` of `to`, which no builder folds, and the operand is then
/// set to `to` itself: the C++'s constructors never fold.
fn rebuilt(
    cx: &Context,
    old: sys::ValueRef,
    to: sys::ValueRef,
    build: impl FnOnce(sys::BuilderRef, sys::ValueRef) -> sys::ValueRef,
) -> sys::ValueRef {
    let b = Builder::new(cx);
    // SAFETY: `old` is an instruction of the module, in a block.
    unsafe { sys::position_builder_before(b.raw(), old) };
    // SAFETY: the builder and `to` are of one context.
    let placeholder = unsafe { sys::build_freeze(b.raw(), to, c"".as_ptr()) };
    let moved = build(b.raw(), placeholder);
    // SAFETY: the instruction just made, whose operand 0 is the placeholder, of `to`'s type.
    unsafe { sys::set_operand(moved, 0, to) };
    // SAFETY: the placeholder, unused now.
    unsafe { sys::instruction_erase_from_parent(placeholder) };
    let name = value_name(old);
    // SAFETY: an instruction; the name holds its length.
    unsafe { sys::set_value_name2(moved, name.as_ptr().cast(), name.len()) };
    moved
}

/// `user.replaceUsesOfWith(from, to)`: every operand of `user` that is `from` becomes `to`.
fn replace_uses_of_with(user: sys::ValueRef, from: sys::ValueRef, to: sys::ValueRef) {
    // SAFETY: an instruction of the module.
    let count = unsafe { sys::get_num_operands(user) };
    for i in 0..u32::try_from(count).unwrap_or(0) {
        // SAFETY: as above, with more than `i` operands.
        if unsafe { sys::get_operand(user, i) } == from {
            // SAFETY: as above; `to` has `from`'s type.
            unsafe { sys::set_operand(user, i, to) };
        }
    }
}

/// Whether the call `inst` is `llvm.lifetime.start` or `.end` (`isLifetimeStartOrEnd`).
fn is_lifetime_marker(inst: sys::ValueRef) -> bool {
    // SAFETY: an intrinsic call.
    let callee = unsafe { sys::get_called_value(inst) };
    // SAFETY: the callee of an intrinsic call is a function.
    let id = unsafe { sys::get_intrinsic_id(callee) };
    let lookup = |name: &[u8]| {
        // SAFETY: the name holds its length.
        unsafe { sys::lookup_intrinsic_id(name.as_ptr().cast(), name.len()) }
    };
    id != 0 && (id == lookup(b"llvm.lifetime.start") || id == lookup(b"llvm.lifetime.end"))
}

/// Calls `f` with each user of `value`, the next use taken before (`make_early_inc_range`).
fn for_each_user(
    value: sys::ValueRef,
    mut f: impl FnMut(sys::ValueRef) -> Result<(), Error>,
) -> Result<(), Error> {
    // SAFETY: a live value.
    let mut next_use = unsafe { sys::get_first_use(value) };
    while !next_use.is_null() {
        // SAFETY: a use of the value.
        let user = unsafe { sys::get_user(next_use) };
        // SAFETY: as above.
        next_use = unsafe { sys::get_next_use(next_use) };
        f(user)?;
    }
    Ok(())
}

/// Calls `f` with each instruction of `bb`, in order.
fn for_each_instruction(bb: sys::BasicBlockRef, mut f: impl FnMut(sys::ValueRef)) {
    // SAFETY: a live block.
    let mut inst = unsafe { sys::get_first_instruction(bb) };
    while !inst.is_null() {
        f(inst);
        // SAFETY: an instruction of the block.
        inst = unsafe { sys::get_next_instruction(inst) };
    }
}

/// Every metadata attachment of `inst` but its debug location dropped.
///
/// `dropUnknownNonDebugMetadata`.
fn drop_metadata(inst: sys::ValueRef) {
    let mut count = 0;
    // SAFETY: an instruction; `count` is writable.
    let entries =
        unsafe { sys::instruction_get_all_metadata_other_than_debug_loc(inst, &raw mut count) };
    if entries.is_null() {
        return;
    }
    let kinds: Vec<u32> = (0..u32::try_from(count).unwrap_or(0))
        // SAFETY: `count` entries.
        .map(|i| unsafe { sys::value_metadata_entries_get_kind(entries, i) })
        .collect();
    // SAFETY: the entries, freed once.
    unsafe { sys::dispose_value_metadata_entries(entries) };
    for kind in kinds {
        // SAFETY: an instruction and one of its attachment kinds; null removes it.
        unsafe { sys::set_metadata(inst, kind, core::ptr::null_mut()) };
    }
}

/// A 64-bit `atomicrmw` or `cmpxchg` on threadgroup memory.
fn is_groupshared64(inst: sys::ValueRef) -> bool {
    // SAFETY: an instruction of the module.
    let rmw = !unsafe { sys::is_a_atomic_rmw_inst(inst) }.is_null();
    // SAFETY: as above.
    let cx = !unsafe { sys::is_a_atomic_cmp_xchg_inst(inst) }.is_null();
    if !rmw && !cx {
        return false;
    }
    // The value is operand 1 of an atomicrmw, the new value operand 2 of a cmpxchg.
    let value = if rmw { 1 } else { 2 };
    // SAFETY: an atomic's pointer operand.
    let ptr = unsafe { sys::get_operand(inst, 0) };
    // SAFETY: an atomic's value operand.
    let operand = unsafe { sys::get_operand(inst, value) };
    // SAFETY: a value of the module.
    let ty = unsafe { sys::type_of(operand) };
    // SAFETY: a type of the module's context.
    let integer = unsafe { sys::get_type_kind(ty) } == sys::INTEGER_TYPE_KIND;
    // SAFETY: an integer type.
    addrspace_of(ptr) == THREADGROUP && integer && unsafe { sys::get_int_type_width(ty) } == 64
}

/// Whether a stage runs before the rasterizer (`PreRaster`).
///
/// DXC marks most float math `fast`. D3D keeps NaN and infinity, so no stage assumes them
/// away, and compares keep no fast flag at all (Metal's compiler treats any as leave to ignore
/// NaN). As airconv's DXBC path, pre-raster stages neither reassociate, fuse nor use
/// reciprocals, so a depth prepass and a base pass compute the same positions. A non-precise
/// `FMad` still becomes `air.fma` there: an explicit fma is the same in every shader the same
/// DXIL is lowered into.
const fn pre_raster(kind: u32) -> bool {
    matches!(
        kind,
        shader_kind::VERTEX | shader_kind::GEOMETRY | shader_kind::HULL | shader_kind::DOMAIN
    )
}

/// One instruction's fast-math flags under DXIL's float policy (`StripFastMath`).
fn strip_fast_math_of(inst: sys::ValueRef, pre_raster: bool) {
    // SAFETY: an instruction of the module.
    if !unsafe { sys::is_af_cmp_inst(inst) }.is_null() {
        // SAFETY: a compare, which carries fast-math flags.
        unsafe { sys::airconv_shim_copy_fast_math_flags(inst, FastMath::NONE.bits()) };
        return;
    }
    // SAFETY: an instruction of the module.
    if unsafe { sys::airconv_shim_is_fp_math_operator(inst) } == 0 {
        return;
    }
    // SAFETY: a floating-point operation.
    let flags = unsafe { sys::airconv_shim_get_fast_math_flags(inst) };
    let mut dropped = FastMath::NO_NANS.with(FastMath::NO_INFS);
    if pre_raster {
        dropped = dropped
            .with(FastMath::ALLOW_REASSOC)
            .with(FastMath::ALLOW_CONTRACT)
            .with(FastMath::ALLOW_RECIPROCAL);
    }
    let kept = flags & !dropped.bits();
    // SAFETY: as above.
    unsafe { sys::airconv_shim_copy_fast_math_flags(inst, kept) };
}

/// The address space of the pointer value `v`.
fn addrspace_of(v: sys::ValueRef) -> u32 {
    // SAFETY: a value of the module.
    let ty = unsafe { sys::type_of(v) };
    // SAFETY: a pointer type.
    unsafe { sys::get_pointer_address_space(ty) }
}

#[cfg(test)]
mod tests;
