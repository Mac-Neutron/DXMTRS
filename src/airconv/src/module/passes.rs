//! airconv's two function passes, which the pass pipeline runs where its text names them.
//!
//! Ported from DXMT's `lower_16bit_texread.cpp` and `simdgroup_implicit_membarrier.cpp`. LLVM's
//! pass manager calls each one per function through the callbacks of [`table`]
//! (`LLVMAIRRunPasses`). The walks, the insert positions and the value names are the C++'s, so a
//! module leaves the pipeline as it left DXMT's.

use core::{
    ffi::{CStr, c_void},
    ptr,
};

use crate::llvm::{sys, value_name};

/// The 16-bit read pass's name in the pipeline's text, as LLVM printed the C++ class.
pub const LOWER_16BIT_TEXREAD: &str = "air::Lower16BitTexReadPass";
/// The barrier pass's name in the pipeline's text, as LLVM printed the C++ class.
pub const SIMDGROUP_IMPLICIT_MEMBARRIER: &str = "air::SimdgroupImplicitMemBarrierPass";

/// `MemFlags::Threadgroup`, the barrier's first argument.
const THREADGROUP_MEM_FLAGS: u64 = 2;
/// The address space of threadgroup memory.
const THREADGROUP_ADDRESS_SPACE: u32 = 3;

/// The two passes, as the pipeline's parser is told of them.
#[must_use]
pub fn table() -> [sys::AirFunctionPass; 2] {
    [
        sys::AirFunctionPass {
            name: LOWER_16BIT_TEXREAD.as_ptr().cast(),
            name_len: LOWER_16BIT_TEXREAD.len(),
            run: run_lower_16bit_texread,
            ctx: ptr::null_mut(),
        },
        sys::AirFunctionPass {
            name: SIMDGROUP_IMPLICIT_MEMBARRIER.as_ptr().cast(),
            name_len: SIMDGROUP_IMPLICIT_MEMBARRIER.len(),
            run: run_simdgroup_implicit_membarrier,
            ctx: ptr::null_mut(),
        },
    ]
}

extern "C" fn run_lower_16bit_texread(
    _ctx: *mut c_void,
    function: sys::ValueRef,
    _analyses: sys::AirFunctionAnalysesRef,
) -> sys::Bool {
    lower_16bit_texread(function).into()
}

extern "C" fn run_simdgroup_implicit_membarrier(
    _ctx: *mut c_void,
    function: sys::ValueRef,
    analyses: sys::AirFunctionAnalysesRef,
) -> sys::Bool {
    simdgroup_implicit_membarrier(function, analyses).into()
}

/// The type of `value`, a live value of the function a pass was handed.
fn type_of(value: sys::ValueRef) -> sys::TypeRef {
    // SAFETY: a live value.
    unsafe { sys::type_of(value) }
}

/// Operand `index` of `user`, an instruction that has it.
fn operand(user: sys::ValueRef, index: u32) -> sys::ValueRef {
    // SAFETY: a live instruction with more than `index` operands.
    unsafe { sys::get_operand(user, index) }
}

/// The constant `value` of the integer type `ty`.
fn const_int(ty: sys::TypeRef, value: u64) -> sys::ValueRef {
    // SAFETY: an integer type of a live context.
    unsafe { sys::const_int(ty, value, 0) }
}

/// Calls `visit` on each instruction of `function`, block by block.
///
/// An instruction's successor is read after its visit, as a C++ range-`for` reads it: one that
/// `visit` inserted right behind the visited instruction is visited too.
fn for_each_instruction(function: sys::ValueRef, mut visit: impl FnMut(sys::ValueRef)) {
    // SAFETY: a live function, as its pass manager handed it over.
    let mut block = unsafe { sys::get_first_basic_block(function) };
    while !block.is_null() {
        // SAFETY: a block of the function.
        let mut inst = unsafe { sys::get_first_instruction(block) };
        while !inst.is_null() {
            visit(inst);
            // SAFETY: `visit` erases nothing, so `inst` is still in its block.
            inst = unsafe { sys::get_next_instruction(inst) };
        }
        // SAFETY: as above; no pass here adds or removes a block.
        block = unsafe { sys::get_next_basic_block(block) };
    }
}

/// The users of `value`, in its use list's order.
fn users(value: sys::ValueRef) -> Vec<sys::ValueRef> {
    let mut out = Vec::new();
    // SAFETY: a live value.
    let mut use_ = unsafe { sys::get_first_use(value) };
    while !use_.is_null() {
        // SAFETY: a use of the list being walked, which nothing changes meanwhile.
        out.push(unsafe { sys::get_user(use_) });
        // SAFETY: as above.
        use_ = unsafe { sys::get_next_use(use_) };
    }
    out
}

/// Whether `ty` is a fixed vector of `count` integers `bits` wide.
fn is_int_vector(ty: sys::TypeRef, count: u32, bits: u32) -> bool {
    // SAFETY: a type of a live context.
    if unsafe { sys::get_type_kind(ty) } != sys::VECTOR_TYPE_KIND {
        return false;
    }
    // SAFETY: a fixed vector type, checked above.
    if unsafe { sys::get_vector_size(ty) } != count {
        return false;
    }
    // SAFETY: as above.
    let element = unsafe { sys::get_element_type(ty) };
    // SAFETY: a type of a live context.
    if unsafe { sys::get_type_kind(element) } != sys::INTEGER_TYPE_KIND {
        return false;
    }
    // SAFETY: an integer type, checked above.
    unsafe { sys::get_int_type_width(element) == bits }
}

/// Whether `user` is an `extractelement` at a constant lane.
fn is_constant_lane_extract(user: sys::ValueRef) -> bool {
    // SAFETY: a live user.
    if unsafe { sys::is_a_extract_element_inst(user) }.is_null() {
        return false;
    }
    // SAFETY: a live value, the extract's lane.
    !unsafe { sys::is_a_constant_int(operand(user, 1)) }.is_null()
}

/// `name` followed by `suffix`, as a C string (LLVM's C builders take one).
///
/// A name holding a NUL ends there, which no value of the translator's has.
fn suffixed(name: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut out = [name, suffix, b"\0"].concat();
    if let Some(nul) = out.iter().position(|&byte| byte == 0) {
        out.truncate(nul + 1);
    }
    out
}

/// The 32-bit lane of `source` that 16-bit lane `lane` lies in, narrowed to that half.
///
/// Built before `extract` and named after it: the lane's extract, shifted right by 16 for an
/// odd `lane` (`<its name>.lshr16`), truncated to `i16_ty` (`<extract's name>.trunc_to_i16`).
fn narrowed_lane(
    builder: sys::BuilderRef,
    i64_ty: sys::TypeRef,
    i16_ty: sys::TypeRef,
    source: sys::ValueRef,
    extract: sys::ValueRef,
    lane: u64,
) -> sys::ValueRef {
    let name = suffixed(&value_name(extract), b".trunc_to_i16");
    // SAFETY: the builder inserts before `extract`, an instruction of the function, and takes
    // its debug location, as `IRBuilder<> B(EE)` does.
    unsafe { sys::position_builder_before(builder, extract) };
    // The lane is an `i64`, as `CreateExtractElement(Value *, uint64_t)` makes it.
    let index = const_int(i64_ty, lane / 2);
    // SAFETY: a positioned builder; a vector of the function and a constant lane inside it; a
    // NUL-terminated name.
    let wide = unsafe { sys::build_extract_element(builder, source, index, c"".as_ptr()) };
    let half = if lane % 2 == 1 {
        let shift = const_int(type_of(wide), 16);
        let name = suffixed(&value_name(wide), b".lshr16");
        // SAFETY: a positioned builder; two values of one integer type; a NUL-terminated name.
        unsafe { sys::build_l_shr(builder, wide, shift, name.as_ptr().cast()) }
    } else {
        wide
    };
    // SAFETY: a positioned builder; an `i32` narrowed to `i16`; a NUL-terminated name.
    unsafe { sys::build_trunc(builder, half, i16_ty, name.as_ptr().cast()) }
}

/// `Lower16BitTexReadPass`: a 16-bit lane read from a `<4 x i32>` seen as `<8 x i16>`.
///
/// Each `extractelement` at a constant lane of a `bitcast <4 x i32> to <8 x i16>` becomes the
/// 32-bit lane's extract, shifted right by 16 for an odd lane and truncated; the extract and,
/// once nothing uses it, the cast are erased. Answers whether the function changed.
fn lower_16bit_texread(function: sys::ValueRef) -> bool {
    let mut changed = false;
    let mut to_erase = Vec::new();
    // SAFETY: the type of a live function.
    let cx = unsafe { sys::get_type_context(type_of(function)) };
    // SAFETY: a live context; the pass's own builder, freed below.
    let builder = unsafe { sys::create_builder_in_context(cx) };
    // SAFETY: a live context.
    let i64_ty = unsafe { sys::int_type_in_context(cx, 64) };
    for_each_instruction(function, |cast| {
        // SAFETY: an instruction of the function.
        if unsafe { sys::is_a_bit_cast_inst(cast) }.is_null() {
            return;
        }
        let dest_ty = type_of(cast);
        let source = operand(cast, 0);
        if !is_int_vector(type_of(source), 4, 32) || !is_int_vector(dest_ty, 8, 16) {
            return;
        }
        // Collected first: the replacements below change the cast's use list.
        let extracts: Vec<_> = users(cast)
            .into_iter()
            .filter(|&user| is_constant_lane_extract(user))
            .collect();
        if extracts.is_empty() {
            return;
        }
        // SAFETY: a vector type's element type.
        let i16_ty = unsafe { sys::get_element_type(dest_ty) };
        for extract in extracts {
            // SAFETY: a constant integer, checked when the extract was collected.
            let lane = unsafe { sys::const_int_get_z_ext_value(operand(extract, 1)) };
            if lane >= 8 {
                continue;
            }
            let narrowed = narrowed_lane(builder, i64_ty, i16_ty, source, extract, lane);
            // SAFETY: two values of one type; `extract` stays in its block until the walk ends.
            unsafe { sys::replace_all_uses_with(extract, narrowed) };
            to_erase.push(extract);
            changed = true;
        }
        to_erase.push(cast);
    });
    for inst in to_erase {
        // SAFETY: an instruction still in the function (each is listed once).
        let unused = unsafe { sys::get_first_use(inst) }.is_null();
        if unused {
            // SAFETY: as above; nothing uses it, tested after the erasures before it.
            unsafe { sys::instruction_erase_from_parent(inst) };
        }
    }
    // SAFETY: the builder made above, freed once.
    unsafe { sys::dispose_builder(builder) };
    changed
}

/// Declares (or finds) `air.simdgroup.barrier` in `module`, a live module.
///
/// `void (i32, i32)`, convergent, nounwind and willreturn, as `AIRBuilder::CreateBarrier`
/// declares it. Answers the function and its type.
#[must_use]
pub fn declare_simdgroup_barrier(module: sys::ModuleRef) -> (sys::ValueRef, sys::TypeRef) {
    const NAME: &CStr = c"air.simdgroup.barrier";
    // SAFETY: a live module.
    let cx = unsafe { sys::get_module_context(module) };
    // SAFETY: a live context.
    let i32_ty = unsafe { sys::int_type_in_context(cx, 32) };
    // SAFETY: as above.
    let void = unsafe { sys::void_type_in_context(cx) };
    let mut params = [i32_ty, i32_ty];
    // SAFETY: types of one context; `params` holds the two parameter types.
    let ty = unsafe { sys::function_type(void, params.as_mut_ptr(), 2, 0) };
    // SAFETY: a live module; a NUL-terminated name.
    let found = unsafe { sys::get_named_function(module, NAME.as_ptr()) };
    if !found.is_null() {
        return (found, ty);
    }
    // SAFETY: as above; `ty` is a function type of the module's context.
    let function = unsafe { sys::add_function(module, NAME.as_ptr(), ty) };
    for name in [&b"convergent"[..], b"nounwind", b"willreturn"] {
        // SAFETY: the name holds its length.
        let kind =
            unsafe { sys::get_enum_attribute_kind_for_name(name.as_ptr().cast(), name.len()) };
        // SAFETY: a live context; `kind` is one of LLVM's enum attributes.
        let attribute = unsafe { sys::create_enum_attribute(cx, kind, 0) };
        // SAFETY: the function made above and an attribute of its context.
        unsafe { sys::add_attribute_at_index(function, sys::ATTRIBUTE_FUNCTION_INDEX, attribute) };
    }
    (function, ty)
}

/// The address space of `inst`'s operand `index`: a load's (0) or a store's (1) pointer.
fn pointer_address_space(inst: sys::ValueRef, index: u32) -> u32 {
    // SAFETY: a pointer type, the type of a load's or a store's pointer operand.
    unsafe { sys::get_pointer_address_space(type_of(operand(inst, index))) }
}

/// Whether `inst` is a store to threadgroup memory.
fn is_threadgroup_store(inst: sys::ValueRef) -> bool {
    // SAFETY: an instruction of the function.
    let is_store = !unsafe { sys::is_a_store_inst(inst) }.is_null();
    is_store && pointer_address_space(inst, 1) == THREADGROUP_ADDRESS_SPACE
}

/// Builds `air.simdgroup.barrier(2, 1)` right after `store`.
///
/// The builder is put at the end of the store's block and then, as a store never ends one,
/// before the instruction after it, whose debug location it takes (`IRBuilder<> builder(BB)`,
/// `SetInsertPoint(next)`).
fn fence(builder: sys::BuilderRef, module: sys::ModuleRef, store: sys::ValueRef) {
    let (barrier, ty) = declare_simdgroup_barrier(module);
    // SAFETY: an instruction in a block.
    let block = unsafe { sys::get_instruction_parent(store) };
    // SAFETY: a builder of the module's context and a block of it.
    unsafe { sys::position_builder_at_end(builder, block) };
    // SAFETY: an instruction in a block.
    let next = unsafe { sys::get_next_instruction(store) };
    if !next.is_null() {
        // SAFETY: as above, and an instruction of that block.
        unsafe { sys::position_builder_before(builder, next) };
    }
    // SAFETY: a live module.
    let cx = unsafe { sys::get_module_context(module) };
    // SAFETY: a live context.
    let i32_ty = unsafe { sys::int_type_in_context(cx, 32) };
    let mut args = [
        const_int(i32_ty, THREADGROUP_MEM_FLAGS),
        const_int(i32_ty, 1),
    ];
    // SAFETY: a positioned builder; `barrier` is of type `ty`, which takes the two `i32`s; a
    // NUL-terminated name.
    unsafe { sys::build_call2(builder, ty, barrier, args.as_mut_ptr(), 2, c"".as_ptr()) };
}

/// `SimdgroupImplicitMemBarrierPass`: a barrier after a threadgroup store a kernel reads back.
///
/// In a module with `!air.kernel`, each load from threadgroup memory whose `MemorySSA`
/// definition is a store to threadgroup memory gets `air.simdgroup.barrier(2, 1)` right after
/// that store, once a store: games expect threadgroup memory to be coherent within a
/// simdgroup, which Metal promises only across a barrier. Only that simplest case is checked,
/// as in DXMT. Answers whether the function changed.
///
/// One case differs from the C++: a load defined by the function's entry (no write before it).
/// DXMT passed that definition's null instruction to `dyn_cast` and crashed; here the load is
/// skipped.
fn simdgroup_implicit_membarrier(
    function: sys::ValueRef,
    analyses: sys::AirFunctionAnalysesRef,
) -> bool {
    const KERNEL: &[u8] = b"air.kernel";
    // SAFETY: a live function.
    let module = unsafe { sys::get_global_parent(function) };
    // SAFETY: a live module; the name holds its length.
    let kernel = unsafe { sys::get_named_metadata(module, KERNEL.as_ptr().cast(), KERNEL.len()) };
    if kernel.is_null() {
        return false;
    }
    let mut loads = Vec::new();
    for_each_instruction(function, |inst| {
        // SAFETY: an instruction of the function.
        let is_load = !unsafe { sys::is_a_load_inst(inst) }.is_null();
        if is_load && pointer_address_space(inst, 0) == THREADGROUP_ADDRESS_SPACE {
            loads.push(inst);
        }
    });
    let mut fenced = Vec::new();
    let mut builder: sys::BuilderRef = ptr::null_mut();
    for load in loads {
        // SAFETY: the analyses of this run and an instruction of `function`. The barriers
        // inserted below leave the analysis as it was computed, as in the C++, which held one
        // `MemorySSA` for the whole pass.
        let store = unsafe { sys::air_get_memory_def_instruction(analyses, function, load) };
        if store.is_null() || fenced.contains(&store) || !is_threadgroup_store(store) {
            continue;
        }
        fenced.push(store);
        if builder.is_null() {
            // SAFETY: a live module.
            let cx = unsafe { sys::get_module_context(module) };
            // SAFETY: a live context; the pass's own builder, freed below.
            builder = unsafe { sys::create_builder_in_context(cx) };
        }
        fence(builder, module, store);
    }
    if !builder.is_null() {
        // SAFETY: the builder made above, freed once.
        unsafe { sys::dispose_builder(builder) };
    }
    !fenced.is_empty()
}
