//! Tests of module setup, the helpers and the writer against the C++ baseline.
//!
//! The baseline tests read `AIRCONV_STEP4_BASE`, step 4's frozen C++ outputs: unset, each prints
//! why and passes; set, a missing folder or file fails it.

use core::{ffi::CStr, ptr};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use airconv_ir::metallib::{self, FunctionEntry, FunctionKind, Patch, VertexAttribute};

use super::{MSAD, SAMPLEPOS, TESSELLATION, initialize, link_helpers, optimize, parse};
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

/// The baseline folder, or `None` (and why) when `AIRCONV_STEP4_BASE` is unset.
fn base(skipped: &str) -> Option<PathBuf> {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: {skipped}");
        return None;
    };
    Some(PathBuf::from(base))
}

fn sorted(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

/// Every metallib of the baseline's `*.out` folders (each run's metallibs), in order.
fn metallibs(base: &Path) -> Vec<PathBuf> {
    let files: Vec<PathBuf> = sorted(base)
        .into_iter()
        .filter(|out| out.is_dir() && out.extension().is_some_and(|ext| ext == "out"))
        .flat_map(|out| sorted(&out))
        .filter(|path| path.extension().is_some_and(|ext| ext == "metallib"))
        .collect();
    assert!(!files.is_empty(), "no metallib under {}", base.display());
    files
}

/// The text of a metallib, `<run>.dis/<name>.ll` beside `<run>.out/<name>.metallib`.
fn text_of(metallib: &Path) -> Vec<u8> {
    let out = metallib.parent().unwrap();
    let ll = out
        .with_extension("dis")
        .join(Path::new(metallib.file_name().unwrap()).with_extension("ll"));
    fs::read(&ll).unwrap_or_else(|e| panic!("{}: {e}", ll.display()))
}

/// The module flags of a module's text: the `!llvm.module.flags` line and `!0` .. `!9`.
fn flag_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| {
            line.starts_with("!llvm.module.flags = ")
                || (0..10).any(|i| line.starts_with(&format!("!{i} = ")))
        })
        .collect()
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
fn initialize_prints_the_baseline_header() {
    let Some(base) = base("the module header is not compared") else {
        return;
    };
    let baseline = fs::read_to_string(base.join("C1.dis/compute.cs.dxil.ll")).unwrap();
    let cx = Context::new();
    let m = initialize(&cx, c"shader.air");
    let text = String::from_utf8(m.print()).unwrap();
    let mut lines = text.lines();
    // The baseline was printed from a module read with no identifier.
    assert_eq!(lines.next(), Some("; ModuleID = 'shader.air'"));
    let header: Vec<&str> = lines.take_while(|line| !line.is_empty()).collect();
    let baseline_header: Vec<&str> = baseline.lines().take_while(|l| !l.is_empty()).collect();
    assert_eq!(header.len(), 3, "{text}");
    assert_eq!(header, baseline_header);
    assert_eq!(flag_lines(&text).len(), 11, "{text}");
    assert_eq!(flag_lines(&text), flag_lines(&baseline));
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
            let helper = ok(parse(&cx, air, b"", b""));
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

        ok(link_helpers(&m, msad, samplepos, tessellation));
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
fn writer_packs_baseline() {
    let Some(base) = base("no baseline metallib is packed") else {
        return;
    };
    let (mut files, mut functions, mut with_inputs) = (0, 0, 0);
    for path in metallibs(&base) {
        let bytes = fs::read(&path).unwrap();
        let (bitcode, _) =
            metallib::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let cx = Context::new();
        let m = ok(parse(&cx, &bitcode, b"", b""));
        let entries = writer::function_entries(&m);
        assert!(
            metallib::pack(&bitcode, &entries) == bytes,
            "{}",
            path.display()
        );
        files += 1;
        functions += entries.len();
        with_inputs += entries
            .iter()
            .filter(|e| !e.vertex_attributes.is_empty())
            .count();
    }
    assert!(with_inputs > 0, "no baseline function has vertex inputs");
    eprintln!("packed {files} metallibs: {functions} functions, {with_inputs} with vertex inputs");
}

#[test]
fn writer_round_trip_keeps_the_text() {
    let Some(base) = base("no baseline metallib is written again") else {
        return;
    };
    let (mut files, mut fixed, mut same) = (0, 0, 0);
    for path in metallibs(&base) {
        let bytes = fs::read(&path).unwrap();
        let (bitcode, entries) =
            metallib::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let cx = Context::new();
        let m = ok(parse(&cx, &bitcode, b"", b""));
        let written = writer::write(&m);
        let (rewritten, rewritten_entries) =
            metallib::parse(&written).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(rewritten_entries == entries, "{}: tags", path.display());
        // Each read in a context of its own, as the tool read each metallib: a second module of
        // one context would rename its structs (`%argument_buffer_struct.0`).
        let again: Vec<Vec<u8>> = (0..2)
            .map(|_| {
                let cx = Context::new();
                let again = ok(parse(&cx, &rewritten, b"", b""));
                assert!(again.print() == text_of(&path), "{}: text", path.display());
                writer::write(&again)
            })
            .collect();
        // The same bytes read twice write the same bytes. Writing what was read is not always the
        // bytes read: LLVM orders a function's value symbols as its symbol table's buckets, which
        // the reader fills in another order, so a module can take a few rounds, or never settle.
        assert!(
            again[0] == again[1],
            "{}: two reads write two metallibs",
            path.display()
        );
        fixed += usize::from(again[0] == written);
        same += usize::from(written == bytes);
        files += 1;
    }
    // Some modules write back as the C++ wrote them: the writer's options are the C++'s (a module
    // hash, no use-list order); a different option would change every one.
    assert!(same > 0, "no metallib writes back as the C++ wrote it");
    eprintln!(
        "wrote {files} metallibs again: {same} as the C++ wrote them, {fixed} the same bytes on a \
         second round"
    );
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
