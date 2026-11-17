//! Tests of module setup, the helpers, the shim's pipeline and the writer's entry extraction.

use core::{ffi::CStr, ptr};
use std::ffi::CString;

use airconv_ir::{
    container::Container,
    dxil_part,
    metallib::{FunctionEntry, FunctionKind, Patch, VertexAttribute},
};

use super::{Layout, MSAD, SAMPLEPOS, TESSELLATION, initialize, link_helpers, optimize, parse};
use crate::{
    api::Error,
    llvm::{Builder, Context, FastMath, Module, sys},
    writer,
};

/// A helper's bitcode, a function of it, one its link leaves out, and `link_helpers`' flags.
type HelperRow = (
    &'static [u8],
    &'static CStr,
    Option<&'static CStr>,
    [bool; 3],
);

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

fn function(m: &Module<'_>, name: &CStr) -> sys::ValueRef {
    // SAFETY: a live module; a NUL-terminated name.
    unsafe { sys::get_named_function(m.raw(), name.as_ptr()) }
}

/// The verifier's complaint about `m`, empty when it has none.
fn verify(m: &Module<'_>) -> String {
    let mut message = ptr::null_mut();
    // SAFETY: a live module; `message` is writable.
    let broken =
        unsafe { sys::verify_module(m.raw(), sys::RETURN_STATUS_ACTION, &raw mut message) };
    let text = if message.is_null() {
        String::new()
    } else {
        // SAFETY: the verifier's NUL-terminated message.
        let text = unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: the verifier's message, freed once.
        unsafe { sys::dispose_message(message) };
        text
    };
    if broken == 0 {
        String::new()
    } else {
        format!("broken: {text}")
    }
}

/// Appends the metadata node `node` to the named metadata `name`.
fn add_named(m: &Module<'_>, name: &CStr, node: sys::MetadataRef) {
    // SAFETY: the module's context is live and `node` its metadata.
    let value = unsafe { sys::metadata_as_value(m.cx().raw(), node) };
    // SAFETY: a live module; a NUL-terminated name; `value` wraps a node.
    unsafe { sys::add_named_metadata_operand(m.raw(), name.as_ptr(), value) };
}

#[test]
fn helpers_parse_and_link() {
    let rows: [HelperRow; 3] = [
        (
            MSAD,
            c"dxmt.msad.i32",
            Some(c"dxmt.msad.v4i32"),
            [true, false, false],
        ),
        (
            SAMPLEPOS,
            c"dxmt.sample_pos.i32",
            None,
            [false, true, false],
        ),
        (
            TESSELLATION,
            c"dxmt.get_domain_patch_index",
            Some(c"dxmt.generate_workload.quad.even"),
            [false, false, true],
        ),
    ];
    for (air, symbol, left_out, [msad, samplepos, tessellation]) in rows {
        let cx = Context::new();
        let tcx = cx.tcx();
        let fn_ty = {
            let helper = ok(parse(&cx, air, b"", b"", Layout::Keep));
            let f = function(&helper, symbol);
            assert!(!f.is_null(), "{symbol:?}");
            // SAFETY: a function of the helper.
            let raw = unsafe { sys::global_get_value_type(f) };
            // SAFETY: a type of this context.
            unsafe { tcx.lift(raw) }.unwrap()
        };
        let m = initialize(&cx, c"shader.air");
        let user = ok(m.add_function(b"user", tcx.function(tcx.void(), &[], false)));
        let b = Builder::new(&cx);
        b.position_at_end(user.append_block(&cx, c"entry").unwrap());
        let helper_fn = ok(m.get_or_insert_function(symbol.to_bytes(), fn_ty, &[]));
        let args: Vec<_> = tcx
            .params(fn_ty)
            .unwrap()
            .iter()
            .map(|&p| cx.null(p))
            .collect();
        let _ = b.call(helper_fn, &args);
        b.ret_void();

        let refs = ok(link_helpers(&m, msad, samplepos, tessellation));
        assert_eq!(refs, vec![air]);
        // Silent, as the C++'s link was (the baseline's stderr is empty): the helper's module
        // flags, which would warn about its SDK version, are gone before it links.
        assert!(cx.printed().is_empty(), "{:?}", cx.printed());

        let linked = function(&m, symbol);
        // SAFETY: a function of the module (checked non-null first).
        assert!(!linked.is_null() && unsafe { sys::is_declaration(linked) } == 0);
        if let Some(left_out) = left_out {
            assert!(function(&m, left_out).is_null(), "{left_out:?} was linked");
        }
        // SAFETY: a live module; a NUL-terminated name.
        let flags =
            unsafe { sys::get_named_metadata_num_operands(m.raw(), c"llvm.module.flags".as_ptr()) };
        assert_eq!(flags, 10, "the helper's module flags were linked");
        for name in [
            &b"air.compile_options"[..],
            b"air.version",
            b"air.language_version",
            b"air.source_file_name",
            b"llvm.ident",
        ] {
            // SAFETY: a live module; `name` holds its length.
            let md = unsafe { sys::get_named_metadata(m.raw(), name.as_ptr().cast(), name.len()) };
            assert!(md.is_null(), "{}", String::from_utf8_lossy(name));
        }
        assert_eq!(verify(&m), "");
        // The pass pipeline runs on it and leaves it valid.
        optimize(&m);
        assert_eq!(verify(&m), "");
        assert!(cx.take_error().is_none());
    }
}

#[test]
fn function_entries_reads_a_patch() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = initialize(&cx, c"shader.air");
    let i32_ty = tcx.int(32);
    let int = |v: u64| cx.const_int(i32_ty, v, false).as_metadata();
    let s = |bytes: &[u8]| cx.md_string(bytes);
    let entry = |name: &[u8]| {
        ok(m.add_function(name, tcx.function(tcx.void(), &[], false)))
            .val()
            .as_metadata()
    };
    // An `air.vertex_input` at location 261 (the C++ kept its low byte) and another input.
    let inputs = cx.md_node(&[
        cx.md_node(&[
            int(0),
            s(b"air.vertex_input"),
            s(b"air.location_index"),
            int(261),
            int(1),
            s(b"air.arg_type_name"),
            s(b"float4"),
            s(b"air.arg_name"),
            s(b"in0"),
        ]),
        cx.md_node(&[
            int(1),
            s(b"air.vertex_id"),
            s(b"air.arg_type_name"),
            s(b"uint"),
        ]),
    ]);
    let patch = |kind: &[u8], count: u64| {
        cx.md_node(&[
            s(b"air.patch"),
            s(kind),
            s(b"air.patch_control_point"),
            int(count),
        ])
    };
    let empty = cx.md_node(&[]);
    // A fragment function listed first, with the same inputs and patch: it carries neither.
    add_named(
        &m,
        c"air.fragment",
        cx.md_node(&[entry(b"f"), empty, inputs, patch(b"triangle", 3)]),
    );
    add_named(
        &m,
        c"air.vertex",
        cx.md_node(&[entry(b"v"), empty, inputs, patch(b"triangle", 3)]),
    );
    add_named(
        &m,
        c"air.vertex",
        cx.md_node(&[entry(b"q"), empty, empty, patch(b"quad", 4)]),
    );
    // A three-operand tuple is no patch.
    let short = cx.md_node(&[s(b"air.patch"), s(b"triangle"), int(3)]);
    add_named(
        &m,
        c"air.vertex",
        cx.md_node(&[entry(b"n"), empty, empty, short]),
    );

    let vertex = |name: &[u8], vertex_attributes, patch| FunctionEntry {
        name: name.to_vec(),
        kind: FunctionKind::Vertex,
        vertex_attributes,
        patch,
    };
    let expected = vec![
        vertex(
            b"v",
            vec![VertexAttribute {
                attribute: 5,
                name: b"in0".to_vec(),
                data_type: 0x06,
            }],
            Some(Patch::Triangle),
        ),
        vertex(b"q", Vec::new(), Some(Patch::Quad)),
        vertex(b"n", Vec::new(), None),
        FunctionEntry {
            name: b"f".to_vec(),
            kind: FunctionKind::Fragment,
            vertex_attributes: Vec::new(),
            patch: None,
        },
    ];
    assert_eq!(writer::function_entries(&m), expected);
}

#[test]
fn shim_fast_math_flags() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = initialize(&cx, c"shader.air");
    let f = ok(m.add_function(b"f", tcx.function(tcx.float(), &[tcx.float()], false)));
    let b = Builder::new(&cx);
    b.position_at_end(f.append_block(&cx, c"entry").unwrap());
    let x = f.param(tcx, 0).unwrap();
    // `Converter::UseFastMath`'s pre-raster flags, set until the guard goes.
    let flags = FastMath::APPROX_FUNC.with(FastMath::NO_SIGNED_ZEROS);
    let sum = {
        let _guard = b.fast_math_guard(flags);
        b.fadd(x, x)
    };
    let plain = b.fadd(sum, x);
    let int = b.add(
        cx.const_int(tcx.int(32), 1, false),
        cx.const_int(tcx.int(32), 2, false),
    );
    let _ = b.ret(plain);
    let print = |v: crate::llvm::Val<'_>| String::from_utf8(v.print()).unwrap();
    assert!(
        print(sum).contains("fadd nsz afn float %0, %0"),
        "{}",
        print(sum)
    );
    assert!(print(plain).contains("= fadd float"), "{}", print(plain));
    // SAFETY: an instruction of this context.
    let sum_flags = unsafe { sys::airconv_shim_get_fast_math_flags(sum.raw()) };
    assert_eq!(sum_flags, flags.bits());
    // SAFETY: as above.
    let plain_flags = unsafe { sys::airconv_shim_get_fast_math_flags(plain.raw()) };
    assert_eq!(plain_flags, 0);
    // SAFETY: as above; the flags replace the instruction's.
    unsafe { sys::airconv_shim_copy_fast_math_flags(sum.raw(), FastMath::NO_NANS.bits()) };
    assert!(print(sum).contains("fadd nnan float"), "{}", print(sum));
    // SAFETY: as above.
    unsafe { sys::airconv_shim_copy_fast_math_flags(sum.raw(), 0x7f) };
    assert!(print(sum).contains("fadd fast float"), "{}", print(sum));
    // SAFETY: a value of this context.
    let sum_is_fp = unsafe { sys::airconv_shim_is_fp_math_operator(sum.raw()) };
    assert_eq!(sum_is_fp, 1);
    // SAFETY: as above (a folded constant: not a floating-point operation).
    let int_is_fp = unsafe { sys::airconv_shim_is_fp_math_operator(int.raw()) };
    assert_eq!(int_is_fp, 0);
}

#[test]
fn dxil_layout_override_drops_i8_32() {
    let dxbc = include_bytes!("../../ir/tests/data/cache.cs.dxil");
    let container = Container::parse(dxbc).unwrap();
    let part = dxil_part::find(&container).unwrap().unwrap();
    let bitcode = part.bitcode;

    let cx = Context::new();
    let err = parse(&cx, bitcode, b"", b"", Layout::Keep)
        .err()
        .expect("raw DXIL layout must be rejected by LLVM 23's reader");
    let text = String::from_utf8_lossy(err.text());
    assert!(text.contains("i8"), "{text}");

    let cx = Context::new();
    let m = ok(parse(&cx, bitcode, b"", b"", Layout::Dxil));
    let printed = String::from_utf8(m.print()).unwrap();
    assert!(!printed.contains("i8:32"), "{printed}");
}

fn pipeline_text(air_passes: i32) -> String {
    let mut ptr: *mut std::ffi::c_char = std::ptr::null_mut();
    let mut len: usize = 0;
    // SAFETY: `ptr` and `len` are valid out-pointers.
    unsafe { sys::airconv_shim_pipeline_text(air_passes, &raw mut ptr, &raw mut len) };
    // SAFETY: `airconv_shim_pipeline_text` wrote `len` bytes at `ptr`.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    // SAFETY: `ptr` was allocated by `airconv_shim_pipeline_text` as a message buffer.
    unsafe { sys::dispose_message(ptr) };
    s
}

const WS_P15_OPT_PRINTED: &str = concat!(
    "annotation2metadata,forceattrs,inferattrs,",
    "function<eager-inv>(",
    "lower-expect,",
    "simplifycfg<bonus-inst-threshold=1;no-forward-switch-cond;no-switch-range-to-icmp;",
    "no-switch-to-arithmetic;no-switch-to-lookup;keep-loops;no-hoist-common-insts;",
    "no-hoist-loads-stores-with-cond-faulting;no-sink-common-insts;speculate-blocks;",
    "simplify-cond-branch;no-speculate-unpredictables>,",
    "sroa<modify-cfg>,",
    "early-cse<>",
    "),",
    "always-inline,ipsccp,globalopt,function(mem2reg),deadargelim,",
    "function<eager-inv>(",
    "instcombine<max-iterations=1;no-verify-fixpoint>,",
    "simplifycfg<bonus-inst-threshold=1;no-forward-switch-cond;switch-range-to-icmp;",
    "no-switch-to-arithmetic;no-switch-to-lookup;keep-loops;no-hoist-common-insts;",
    "no-hoist-loads-stores-with-cond-faulting;no-sink-common-insts;speculate-blocks;",
    "simplify-cond-branch;no-speculate-unpredictables>",
    ")"
);

#[test]
fn shim_pipeline_equals_ws_p15() {
    assert_eq!(pipeline_text(0), WS_P15_OPT_PRINTED);
}

#[test]
fn shim_pipeline_places_the_two_passes() {
    let expected = format!(
        "{},air::Lower16BitTexReadPass,air::SimdgroupImplicitMemBarrierPass)",
        WS_P15_OPT_PRINTED.strip_suffix(')').unwrap()
    );
    assert_eq!(pipeline_text(1), expected);
}

/// Parses the textual IR `text` into a module of `cx`.
fn parse_ir<'cx>(cx: &'cx Context, text: &CStr) -> Module<'cx> {
    let mut out = ptr::null_mut();
    let mut message = ptr::null_mut();
    // SAFETY: `text` is NUL-terminated and outlives the buffer; the name is NUL-terminated.
    let buffer = unsafe {
        sys::create_memory_buffer_with_memory_range(
            text.as_ptr(),
            text.count_bytes(),
            c"ir".as_ptr(),
            1,
        )
    };
    // SAFETY: a live context and buffer; the two outputs are writable.
    let failed =
        unsafe { sys::parse_ir_in_context2(cx.raw(), buffer, &raw mut out, &raw mut message) };
    // SAFETY: the buffer made above, freed once; the reader did not take it.
    unsafe { sys::dispose_memory_buffer(buffer) };
    if failed != 0 {
        // SAFETY: the reader's NUL-terminated message.
        let text = unsafe { CStr::from_ptr(message) }.to_string_lossy();
        panic!("{text}");
    }
    // SAFETY: the reader's module, of `cx`, handed over.
    unsafe { Module::from_raw(cx, out) }
}

/// The module `text` after the pass pipeline, printed.
fn optimized(text: &CStr) -> String {
    let cx = Context::new();
    let m = parse_ir(&cx, text);
    optimize(&m);
    assert_eq!(verify(&m), "");
    String::from_utf8(m.print()).unwrap()
}

/// 16-bit reads of a 32-bit vector, each result kept alive by a store or a return.
///
/// Alive, so that no earlier pass of the pipeline removes it: three constant lanes (low half,
/// high half, the last lane), a constant lane beside a variable one (the cast must stay for the
/// variable one), and a cast nothing extracts from.
const READ16: &CStr = c"
define void @constant_lanes(<4 x i32> %v, ptr %out) {
  %c = bitcast <4 x i32> %v to <8 x i16>
  %e0 = extractelement <8 x i16> %c, i32 0
  %e1 = extractelement <8 x i16> %c, i32 1
  %e7 = extractelement <8 x i16> %c, i64 7
  store i16 %e0, ptr %out, align 2
  %p1 = getelementptr i16, ptr %out, i64 1
  store i16 %e1, ptr %p1, align 2
  %p2 = getelementptr i16, ptr %out, i64 2
  store i16 %e7, ptr %p2, align 2
  ret void
}

define i16 @variable_lane(<4 x i32> %v, i32 %i) {
  %c = bitcast <4 x i32> %v to <8 x i16>
  %e3 = extractelement <8 x i16> %c, i32 3
  %ev = extractelement <8 x i16> %c, i32 %i
  %s = add i16 %e3, %ev
  ret i16 %s
}

define <8 x i16> @cast_alone(<4 x i32> %v) {
  %c = bitcast <4 x i32> %v to <8 x i16>
  ret <8 x i16> %c
}
";

const READ16_OPTIMIZED: &str = r#"; ModuleID = 'ir'
source_filename = "ir"

define void @constant_lanes(<4 x i32> %v, ptr %out) local_unnamed_addr {
  %1 = extractelement <4 x i32> %v, i64 0
  %e0.trunc_to_i16 = trunc i32 %1 to i16
  %2 = extractelement <4 x i32> %v, i64 0
  %.lshr161 = lshr i32 %2, 16
  %e1.trunc_to_i16 = trunc i32 %.lshr161 to i16
  %3 = extractelement <4 x i32> %v, i64 3
  %.lshr16 = lshr i32 %3, 16
  %e7.trunc_to_i16 = trunc i32 %.lshr16 to i16
  store i16 %e0.trunc_to_i16, ptr %out, align 2
  %p1 = getelementptr i8, ptr %out, i64 2
  store i16 %e1.trunc_to_i16, ptr %p1, align 2
  %p2 = getelementptr i8, ptr %out, i64 4
  store i16 %e7.trunc_to_i16, ptr %p2, align 2
  ret void
}

define i16 @variable_lane(<4 x i32> %v, i32 %i) local_unnamed_addr {
  %c = bitcast <4 x i32> %v to <8 x i16>
  %1 = extractelement <4 x i32> %v, i64 1
  %.lshr16 = lshr i32 %1, 16
  %e3.trunc_to_i16 = trunc i32 %.lshr16 to i16
  %ev = extractelement <8 x i16> %c, i32 %i
  %s = add i16 %e3.trunc_to_i16, %ev
  ret i16 %s
}

define <8 x i16> @cast_alone(<4 x i32> %v) local_unnamed_addr {
  %c = bitcast <4 x i32> %v to <8 x i16>
  ret <8 x i16> %c
}
"#;

#[test]
fn the_pipeline_lowers_16_bit_reads_of_a_32_bit_vector() {
    assert_eq!(optimized(READ16), READ16_OPTIMIZED);
}

/// Threadgroup memory (address space 3) read after it is written.
///
/// Each load is from an address the pipeline cannot prove equal to the store's: two loads after
/// one store (one barrier, after the store), a second store and its load (a second barrier), a
/// load after a call, a load after a store to other memory, and a load of other memory after a
/// threadgroup store (none for the three).
const BARRIER: &str = "
declare void @opaque()

define void @two_loads_one_store(ptr addrspace(3) %a, ptr addrspace(3) %b, ptr addrspace(3) %c, ptr %out, i32 %x) {
  store i32 %x, ptr addrspace(3) %a, align 4
  %l1 = load i32, ptr addrspace(3) %b, align 4
  %l2 = load i32, ptr addrspace(3) %c, align 4
  %s = add i32 %l1, %l2
  store i32 %s, ptr addrspace(3) %a, align 4
  %l3 = load i32, ptr addrspace(3) %b, align 4
  store i32 %l3, ptr %out, align 4
  ret void
}

define i32 @load_after_a_call(ptr addrspace(3) %b) {
  call void @opaque()
  %l = load i32, ptr addrspace(3) %b, align 4
  ret i32 %l
}

define i32 @load_after_another_store(ptr %p, ptr addrspace(3) %b, i32 %x) {
  store i32 %x, ptr %p, align 4
  %l = load i32, ptr addrspace(3) %b, align 4
  ret i32 %l
}

define i32 @another_load_after_the_store(ptr %p, ptr addrspace(3) %b, i32 %x) {
  store i32 %x, ptr addrspace(3) %b, align 4
  %l = load i32, ptr %p, align 4
  ret i32 %l
}
";

const BARRIER_OPTIMIZED: &str = r#"; ModuleID = 'ir'
source_filename = "ir"

declare void @opaque() local_unnamed_addr

define void @two_loads_one_store(ptr addrspace(3) %a, ptr addrspace(3) %b, ptr addrspace(3) %c, ptr %out, i32 %x) local_unnamed_addr {
  store i32 %x, ptr addrspace(3) %a, align 4
  call void @air.simdgroup.barrier(i32 2, i32 1)
  %l1 = load i32, ptr addrspace(3) %b, align 4
  %l2 = load i32, ptr addrspace(3) %c, align 4
  %s = add i32 %l1, %l2
  store i32 %s, ptr addrspace(3) %a, align 4
  call void @air.simdgroup.barrier(i32 2, i32 1)
  %l3 = load i32, ptr addrspace(3) %b, align 4
  store i32 %l3, ptr %out, align 4
  ret void
}

define i32 @load_after_a_call(ptr addrspace(3) %b) local_unnamed_addr {
  call void @opaque()
  %l = load i32, ptr addrspace(3) %b, align 4
  ret i32 %l
}

define i32 @load_after_another_store(ptr %p, ptr addrspace(3) %b, i32 %x) local_unnamed_addr {
  store i32 %x, ptr %p, align 4
  %l = load i32, ptr addrspace(3) %b, align 4
  ret i32 %l
}

define i32 @another_load_after_the_store(ptr %p, ptr addrspace(3) %b, i32 %x) local_unnamed_addr {
  store i32 %x, ptr addrspace(3) %b, align 4
  %l = load i32, ptr %p, align 4
  ret i32 %l
}

; Function Attrs: convergent nounwind willreturn
declare void @air.simdgroup.barrier(i32, i32) #0

attributes #0 = { convergent nounwind willreturn }

!air.kernel = !{!0}

!0 = !{}
"#;

#[test]
fn the_pipeline_fences_threadgroup_stores_a_kernel_reads_back() {
    let kernel = CString::new(format!("{BARRIER}\n!air.kernel = !{{!0}}\n!0 = !{{}}\n")).unwrap();
    assert_eq!(optimized(&kernel), BARRIER_OPTIMIZED);
}

#[test]
fn the_pipeline_fences_nothing_outside_a_kernel() {
    let plain = CString::new(BARRIER).unwrap();
    let printed = optimized(&plain);
    assert!(printed.contains("@two_loads_one_store"), "{printed}");
    assert!(!printed.contains("air.simdgroup.barrier"), "{printed}");
}

/// The first attribute group of `printed`, without its braces.
fn first_attribute_group(printed: &str) -> &str {
    let (_, rest) = printed.split_once("attributes #0 = { ").expect(printed);
    rest.split_once(" }").expect(printed).0
}

#[test]
fn memory_attributes_print_as_llvm_23_writes_them() {
    // Rows of four: the location mask's selector (all, argument memory, inaccessible memory, both),
    // each with `ModRefInfo` none, read, write, read-write.
    let expected = [
        "memory(none)",
        "memory(read)",
        "memory(write)",
        "memory(readwrite)",
        "memory(none)",
        "memory(argmem: read)",
        "memory(argmem: write)",
        "memory(argmem: readwrite)",
        "memory(none)",
        "memory(inaccessiblemem: read)",
        "memory(inaccessiblemem: write)",
        "memory(inaccessiblemem: readwrite)",
        "memory(none)",
        "memory(argmem: read, inaccessiblemem: read)",
        "memory(argmem: write, inaccessiblemem: write)",
        "memory(argmem: readwrite, inaccessiblemem: readwrite)",
    ];
    let printed: Vec<String> = (0..16)
        .map(|row: i32| {
            let cx = Context::new();
            let m = Module::new(&cx, c"m");
            let tcx = cx.tcx();
            let f = ok(m.add_function(b"f", tcx.function(tcx.void(), &[], false)));
            // SAFETY: the context is live.
            let attribute =
                unsafe { sys::airconv_shim_memory_attribute(cx.raw(), row / 4, row % 4) };
            // SAFETY: a function of this context and an attribute of it.
            unsafe {
                sys::add_attribute_at_index(
                    f.val().raw(),
                    sys::ATTRIBUTE_FUNCTION_INDEX,
                    attribute,
                );
            };
            let text = String::from_utf8(m.print()).unwrap();
            first_attribute_group(&text).to_owned()
        })
        .collect();
    assert_eq!(printed, expected);
}

#[test]
fn a_set_up_module_carries_sdk_version_15_0() {
    let cx = Context::new();
    let m = initialize(&cx, c"m");
    let printed = String::from_utf8(m.print()).unwrap();
    assert!(
        printed.contains("!{i32 2, !\"SDK Version\", [2 x i32] [i32 15, i32 0]}"),
        "{printed}"
    );
}
