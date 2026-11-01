//! Temporaries, the immediate constant buffer and threadgroup memory.
//!
//! [`Resources`] is the C++'s `io_binding_map` (`A/dxbc_converter.hpp`): the registers, buffers
//! and special values a compile's prologue, body and epilogue share. Its four maps are
//! `FxHashMap`s looked up by register, never iterated into IR. [`setup_tgsm`],
//! [`setup_temp_register`] and [`setup_immediate_constant_buffer`] are
//! `A/dxbc_converter.cpp:95-199`, called in each stage's order.

use core::ffi::CStr;

use airconv_ir::shader_info::ShaderInfo;
use rustc_hash::FxHashMap;

use crate::{
    air::ty::AirTypes,
    api::Error,
    llvm::{Builder, Module, Ty, Val, name, sys},
};

/// The address space of threadgroup memory.
const THREADGROUP: u32 = 3;
/// The address space of constants.
const CONSTANT: u32 = 2;

/// A register file as `i32` and as `float` (the same memory).
#[derive(Clone, Copy, Default)]
pub struct RegisterFile<'cx> {
    pub ptr_int4: Option<Val<'cx>>,
    pub ptr_float4: Option<Val<'cx>>,
}

/// An indexable temp: `vec_size` components per register, as `i32`.
#[derive(Clone, Copy)]
pub struct IndexableRegisterFile<'cx> {
    pub ptr_int_vec: Val<'cx>,
    pub vec_size: u32,
}

/// A hull shader phase's own temporaries.
#[derive(Default)]
pub struct PhaseTemp<'cx> {
    pub temp: RegisterFile<'cx>,
    pub indexable_temp_map: FxHashMap<u32, IndexableRegisterFile<'cx>>,
}

/// A pixel shader input an `eval_*` interpolates: its interpolant and whether it is perspective.
#[derive(Clone, Copy)]
pub struct Interpolant<'cx> {
    pub handle: Val<'cx>,
    pub perspective: bool,
}

/// What a compile's prologue, body and epilogue share (`io_binding_map`).
///
/// A value no stage set is `None`, the C++'s null.
#[derive(Default)]
pub struct Resources<'cx> {
    pub icb: Option<Val<'cx>>,
    pub icb_float: Option<Val<'cx>>,
    /// Each threadgroup buffer's structure stride (0 raw) and its global.
    pub tgsm_map: FxHashMap<u32, (u32, Val<'cx>)>,
    pub interpolant_map: FxHashMap<u32, Interpolant<'cx>>,

    pub input: RegisterFile<'cx>,
    pub output: RegisterFile<'cx>,
    pub temp: RegisterFile<'cx>,
    pub indexable_temp_map: FxHashMap<u32, IndexableRegisterFile<'cx>>,
    pub phases: Vec<PhaseTemp<'cx>>,
    pub patch_constant_output: RegisterFile<'cx>,
    pub input_element_count: u32,
    pub output_element_count: u32,

    // Special registers (input).
    pub thread_id_arg: Option<Val<'cx>>,
    pub thread_group_id_arg: Option<Val<'cx>>,
    pub thread_id_in_group_arg: Option<Val<'cx>>,
    pub thread_id_in_group_flat_arg: Option<Val<'cx>>,
    pub coverage_mask_arg: Option<Val<'cx>>,
    pub domain: Option<Val<'cx>>,
    pub patch_id: Option<Val<'cx>>,
    pub thread_id_in_patch: Option<Val<'cx>>,
    pub gs_instance_id: Option<Val<'cx>>,

    // Special registers (output).
    pub depth_output_reg: Option<Val<'cx>>,
    pub stencil_ref_reg: Option<Val<'cx>>,
    pub coverage_mask_reg: Option<Val<'cx>>,
    pub cmp_exch_temp: Option<Val<'cx>>,

    // Tessellation's buffers.
    pub hull_cp_passthrough_type: Option<Ty>,
    pub hull_cp_passthrough_src: Option<Val<'cx>>,
    pub hull_cp_passthrough_dst: Option<Val<'cx>>,

    // Looked up often.
    pub vertex_id: Option<Val<'cx>>,
    pub vertex_id_with_base: Option<Val<'cx>>,
    pub instance_id: Option<Val<'cx>>,
    pub instance_id_with_base: Option<Val<'cx>>,
    pub base_vertex_id: Option<Val<'cx>>,
    pub base_instance_id: Option<Val<'cx>>,
    pub vertex_buffer_table: Option<Val<'cx>>,

    // The geometry pipeline's mesh.
    pub mesh: Option<Val<'cx>>,
}

/// The immediate constant buffer (`setup_immediate_constant_buffer`).
///
/// The global `icb` and its `float` view; nothing for a shader without one.
///
/// # Errors
///
/// None in practice: only the global's name check.
pub fn setup_immediate_constant_buffer<'cx>(
    info: &ShaderInfo,
    res: &mut Resources<'cx>,
    types: &AirTypes,
    m: &Module<'cx>,
    b: &Builder<'cx>,
) -> Result<(), Error> {
    if info.imm_constant_buffer_data.is_empty() {
        return Ok(());
    }
    let cx = m.cx();
    let tcx = cx.tcx();
    let len = airconv_ir::binding::count_u32(info.imm_constant_buffer_data.len());
    let ty = tcx.array(types.int4, len);
    let elements: Vec<Val<'cx>> = info
        .imm_constant_buffer_data
        .iter()
        .map(|data| {
            let ints = data.map(|v| cx.const_int(types.int, v.into(), false));
            cx.const_vector(&ints)
        })
        .collect();
    let data = cx.const_array(types.int4, &elements);
    let icb = global(m, ty, c"icb", CONSTANT, data, true)?;
    res.icb = Some(icb);
    res.icb_float = Some(b.bitcast(icb, tcx.ptr(tcx.array(types.float4, len), CONSTANT)));
    Ok(())
}

/// Each threadgroup buffer as the global `g<id>`, in id order (`setup_tgsm`).
///
/// # Errors
///
/// None in practice: only the global's name check.
pub fn setup_tgsm<'cx>(
    info: &ShaderInfo,
    res: &mut Resources<'cx>,
    types: &AirTypes,
    m: &Module<'cx>,
) -> Result<(), Error> {
    let cx = m.cx();
    for (&id, tgsm) in &info.tgsm_map {
        let ty = cx.tcx().array(types.int, tgsm.size_in_uint);
        let global_name = name(format!("g{id}").as_bytes())?;
        let h = global(m, ty, &global_name, THREADGROUP, cx.undef(ty), false)?;
        let stride = if tgsm.structured { tgsm.stride } else { 0 };
        res.tgsm_map.insert(id, (stride, h));
    }
    Ok(())
}

/// The temporaries' allocas (`setup_temp_register`).
///
/// The shader's, each phase's with its indexable temps, the shader's indexable temps, then the
/// compare-exchange slot.
pub fn setup_temp_register<'cx>(
    info: &ShaderInfo,
    res: &mut Resources<'cx>,
    types: &AirTypes,
    b: &Builder<'cx>,
) {
    res.temp = register_file(b, types, info.temp_register_count.wrapping_mul(4));
    for phase in &info.phases {
        let mut phase_temp = PhaseTemp {
            temp: register_file(b, types, phase.temp_register_count.wrapping_mul(4)),
            ..PhaseTemp::default()
        };
        for (&idx, &(count, mask)) in &phase.indexable_temp_register_counts {
            phase_temp
                .indexable_temp_map
                .insert(idx, indexable(b, types, count, mask));
        }
        res.phases.push(phase_temp);
    }
    for (&idx, &(count, mask)) in &info.indexable_temp_register_counts {
        res.indexable_temp_map
            .insert(idx, indexable(b, types, count, mask));
    }
    if info
        .flags
        .contains(airconv_ir::shader_info::ShaderFlags::USE_CMP_EXCH)
    {
        res.cmp_exch_temp = Some(b.alloca(types.int, c""));
    }
}

/// A vertex or pixel shader's input and output registers, each an array of `<4 x i32>`.
///
/// The allocas and their `<4 x float>` views, input first, with their register counts.
pub fn setup_io_registers<'cx>(
    res: &mut Resources<'cx>,
    types: &AirTypes,
    b: &Builder<'cx>,
    max_input_register: u32,
    max_output_register: u32,
) {
    let tcx = b.cx().tcx();
    let vec4_file = |count| {
        let ptr_int4 = b.alloca(tcx.array(types.int4, count), c"");
        let ptr_float4 = b.bitcast(ptr_int4, tcx.ptr(tcx.array(types.float4, count), 0));
        RegisterFile {
            ptr_int4: Some(ptr_int4),
            ptr_float4: Some(ptr_float4),
        }
    };
    res.input = vec4_file(max_input_register);
    res.input_element_count = max_input_register;
    res.output = vec4_file(max_output_register);
    res.output_element_count = max_output_register;
}

/// `len` components as an `i32` alloca and its `float` view.
fn register_file<'cx>(b: &Builder<'cx>, types: &AirTypes, len: u32) -> RegisterFile<'cx> {
    let tcx = b.cx().tcx();
    let ptr_int4 = b.alloca(tcx.array(types.int, len), c"");
    let ptr_float4 = b.bitcast(ptr_int4, tcx.ptr(tcx.array(types.float, len), 0));
    RegisterFile {
        ptr_int4: Some(ptr_int4),
        ptr_float4: Some(ptr_float4),
    }
}

/// An indexable temp of `count` registers, as many components as `mask`'s highest bit.
fn indexable<'cx>(
    b: &Builder<'cx>,
    types: &AirTypes,
    count: u32,
    mask: u32,
) -> IndexableRegisterFile<'cx> {
    let tcx = b.cx().tcx();
    let vec_size = mask.bit_width();
    let len = count.wrapping_mul(vec_size);
    let ptr_int_vec = b.alloca(tcx.array(types.int, len), c"");
    let _ = b.bitcast(ptr_int_vec, tcx.ptr(tcx.array(types.float, len), 0));
    IndexableRegisterFile {
        ptr_int_vec,
        vec_size,
    }
}

/// An internal global of `ty` in `addrspace` (`new GlobalVariable`, then `setAlignment`).
///
/// Initialized with `init`, aligned to 4.
fn global<'cx>(
    m: &Module<'cx>,
    ty: Ty,
    global_name: &CStr,
    addrspace: u32,
    init: Val<'cx>,
    constant: bool,
) -> Result<Val<'cx>, Error> {
    let g = m.add_global(global_name.to_bytes(), ty, addrspace)?;
    let raw = g.raw();
    // SAFETY: the global just made.
    unsafe { sys::set_linkage(raw, sys::INTERNAL_LINKAGE) };
    // SAFETY: the global and a constant of its value type, of one context.
    unsafe { sys::set_initializer(raw, init.raw()) };
    // SAFETY: the global just made.
    unsafe { sys::set_global_constant(raw, constant.into()) };
    // SAFETY: the global just made.
    unsafe { sys::set_alignment(raw, 4) };
    Ok(g)
}
