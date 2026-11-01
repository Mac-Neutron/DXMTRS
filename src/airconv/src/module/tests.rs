//! Tests of module setup, the helpers, the shim's pipeline and the writer's entry extraction.

use core::{ffi::CStr, ptr};

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
