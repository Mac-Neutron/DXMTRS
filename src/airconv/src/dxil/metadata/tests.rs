use super::*;
use crate::llvm::{Builder, Context};

/// The error's text of a read that must fail.
fn text<T>(result: Result<T, Error>) -> Vec<u8> {
    match result {
        Ok(_) => panic!("read"),
        Err(e) => e.text().to_vec(),
    }
}

/// A module whose named metadata `dx.shaderModel` and `dx.entryPoints` are the nodes given.
fn module(
    cx: &Context,
    model: Option<sys::MetadataRef>,
    point: Option<sys::MetadataRef>,
) -> Module<'_> {
    let m = Module::new(cx, c"t");
    for (name, node) in [(c"dx.shaderModel", model), (c"dx.entryPoints", point)] {
        if let Some(node) = node {
            add(&m, name, node);
        }
    }
    m
}

/// Adds `node` to the module's named metadata `name`.
fn add(m: &Module<'_>, name: &CStr, node: sys::MetadataRef) {
    // SAFETY: the module's context is live and `node` its metadata.
    let value = unsafe { sys::metadata_as_value(m.cx().raw(), node) };
    // SAFETY: a live module; a NUL-terminated name; `value` wraps a node.
    unsafe { sys::add_named_metadata_operand(m.raw(), name.as_ptr(), value) };
}

fn int(cx: &Context, v: u64) -> sys::MetadataRef {
    cx.const_int(cx.tcx().int(32), v, false).as_metadata()
}

#[test]
fn missing_metadata_names_what_is_missing() {
    let cx = Context::new();
    let model = cx.md_node(&[cx.md_string(b"cs"), int(&cx, 6), int(&cx, 0)]);
    assert_eq!(
        text(read_entry(&module(&cx, None, None))),
        b"DXIL: dx.shaderModel missing"
    );
    assert_eq!(
        text(read_entry(&module(&cx, Some(model), None))),
        b"DXIL: dx.entryPoints missing"
    );
    let short = cx.md_node(&[cx.md_string(b"main"), int(&cx, 0), int(&cx, 0)]);
    assert_eq!(
        text(read_entry(&module(&cx, Some(model), Some(short)))),
        b"DXIL: entry point fields missing"
    );
    let no_function = cx.md_node(&[
        cx.md_string(b"main"),
        cx.md_string(b"main"),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    ]);
    assert_eq!(
        text(read_entry(&module(&cx, Some(model), Some(no_function)))),
        b"DXIL: entry function missing"
    );
}

#[test]
fn metadata_the_cxx_crashed_on_is_malformed() {
    let cx = Context::new();
    // A string where the shader model's major version belongs, and a shader model of one operand:
    // the C++'s `mdconst::extract` and unchecked `getOperand` read garbage there.
    for model in [
        cx.md_node(&[cx.md_string(b"cs"), cx.md_string(b"six"), int(&cx, 0)]),
        cx.md_node(&[cx.md_string(b"cs")]),
    ] {
        assert_eq!(text(read_entry(&module(&cx, Some(model), None))), MALFORMED);
    }
    // A signatures node without its output list.
    let m = Module::new(&cx, c"t");
    let f = m
        .add_function(b"main", cx.tcx().function(cx.tcx().void(), &[], false))
        .unwrap_or_else(|_| panic!("add_function"));
    let model = cx.md_node(&[cx.md_string(b"cs"), int(&cx, 6), int(&cx, 0)]);
    let sigs = cx.md_node(&[ptr::null_mut()]);
    let point = cx.md_node(&[
        f.val().as_metadata(),
        cx.md_string(b"main"),
        sigs,
        ptr::null_mut(),
        ptr::null_mut(),
    ]);
    add(&m, c"dx.shaderModel", model);
    add(&m, c"dx.entryPoints", point);
    assert_eq!(text(read_entry(&m)), MALFORMED);
}

#[test]
fn entry_properties_read_as_the_cxx() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let f = m
        .add_function(b"csmain", cx.tcx().function(cx.tcx().void(), &[], false))
        .unwrap_or_else(|_| panic!("add_function"));
    let model = cx.md_node(&[cx.md_string(b"cs"), int(&cx, 6), int(&cx, 8)]);
    // NumThreads (8, 4, 2), then WaveSizeRange (16, 0: the minimum alone).
    let props = cx.md_node(&[
        int(&cx, 4),
        cx.md_node(&[int(&cx, 8), int(&cx, 4), int(&cx, 2)]),
        int(&cx, 23),
        cx.md_node(&[int(&cx, 16), int(&cx, 0), int(&cx, 0)]),
    ]);
    let point = cx.md_node(&[
        f.val().as_metadata(),
        cx.md_string(b"csmain"),
        ptr::null_mut(),
        ptr::null_mut(),
        props,
    ]);
    add(&m, c"dx.shaderModel", model);
    add(&m, c"dx.entryPoints", point);
    let entry = read_entry(&m).unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())));
    assert_eq!(
        (entry.kind, entry.sm_major, entry.sm_minor, entry.name),
        (shader_kind::COMPUTE, 6, 8, b"csmain".to_vec())
    );
    assert_eq!(entry.numthreads, [8, 4, 2]);
    assert_eq!((entry.wave_size_min, entry.wave_size_max), (16, 16));
    assert!(entry.inputs.is_empty() && entry.resources.is_empty());
}

#[test]
fn a_handle_that_annotates_itself_is_refused() {
    let cx = Context::new();
    let m = Module::new(&cx, c"t");
    let tcx = cx.tcx();
    let i32 = tcx.int(32);
    let annotate = m
        .add_function(
            b"dx.op.annotateHandle",
            tcx.function(i32, &[i32, i32, i32], false),
        )
        .unwrap_or_else(|_| panic!("add_function"));
    let main = m
        .add_function(b"main", tcx.function(tcx.void(), &[], false))
        .unwrap_or_else(|_| panic!("add_function"));
    let b = Builder::new(&cx);
    b.position_at_end(main.append_block(&cx, c"entry").unwrap());
    let zero = cx.const_int(i32, 0, false);
    let call = b.call(annotate, &[cx.const_int(i32, 216, false), zero, zero]);
    assert_eq!(op_code(call.raw()), op::ANNOTATE_HANDLE);
    // SAFETY: a call of three arguments; it becomes its own handle (the C++ recursed forever).
    unsafe { sys::set_operand(call.raw(), 1, call.raw()) };
    assert_eq!(
        text(resolve_handle(&EntryInfo::default(), call.raw())),
        b"DXIL: resource handle annotates itself"
    );
    // A handle that is no call.
    assert_eq!(
        text(resolve_handle(&EntryInfo::default(), zero.raw())),
        b"DXIL: resource handle through phi/select"
    );
}
