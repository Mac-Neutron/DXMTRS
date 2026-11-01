//! Tests of the AIR bitcode writer, its type table (P8), error reporting (P7) and oracle stats.

use core::ffi::CStr;

use super::{build_table, write, write_with_stats};
use crate::{
    api::Error,
    llvm::{Builder, Context, sys},
    module,
};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

fn print_raw_type(raw: sys::TypeRef) -> String {
    // SAFETY: `raw` is a live `LLVMTypeRef`.
    let ptr = unsafe { sys::print_type_to_string(raw) };
    // SAFETY: `print_type_to_string` returns a NUL-terminated string.
    let text = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_owned();
    // SAFETY: `ptr` was allocated by `print_type_to_string`.
    unsafe { sys::dispose_message(ptr) };
    text
}

#[test]
fn table_has_an_entry_per_recorded_function_and_global() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"shader.air");
    let p_f = tcx.ptr(tcx.float(), 1);
    let fn_ty = tcx.function(tcx.void(), &[p_f], false);
    let f = ok(m.add_function(b"f", fn_ty));
    let b = Builder::new(&cx);
    b.position_at_end(f.append_block(&cx, c"entry").unwrap());
    let decl_ty = tcx.function(tcx.float(), &[tcx.float()], false);
    let decl = ok(m.get_or_insert_function(b"air.dfdx.f32", decl_ty, &[]));
    let g_ty = tcx.array(tcx.int(32), 4);
    let g = ok(m.add_global(b"g", g_ty, 3));
    // SAFETY: the global just made and its zero initializer.
    unsafe { sys::set_initializer(g.raw(), cx.null(g_ty).raw()) };
    b.ret_void();

    assert_eq!(cx.recorded(b"f"), Some(fn_ty));
    assert_eq!(cx.recorded(b"air.dfdx.f32"), Some(decl_ty));
    assert_eq!(cx.recorded(b"g"), Some(g_ty));

    let (values, structs, stale) = ok(build_table(&m));
    assert_eq!(stale, 0);
    assert!(structs.is_empty());
    assert_eq!(values.len(), 3);
    assert_eq!(values[0].value, f.val().raw());
    assert_eq!(values[0].ty, ok(tcx.typed(fn_ty)));
    assert_eq!(values[1].value, decl.val().raw());
    assert_eq!(values[1].ty, ok(tcx.typed(decl_ty)));
    assert_eq!(values[2].value, g.raw());
    assert_eq!(values[2].ty, ok(tcx.typed(g_ty)));

    let (metallib, stats, write_stale) = ok(write_with_stats(&m, &[]));
    assert_ne!(metallib, [] as [u8; 0]);
    assert_eq!(stats.use_typed, 0);
    assert_eq!(write_stale, 0);
}

#[test]
fn table_leaves_out_a_retyped_function() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"shader.air");
    let p_f = tcx.ptr(tcx.float(), 1);
    let i32_ty = tcx.int(32);

    let helper_ty = tcx.function(i32_ty, &[p_f, i32_ty], false);
    let helper = ok(m.add_function(b"helper", helper_ty));
    // SAFETY: the function just made.
    unsafe { sys::set_linkage(helper.val().raw(), sys::INTERNAL_LINKAGE) };
    let b = Builder::new(&cx);
    b.position_at_end(helper.append_block(&cx, c"entry").unwrap());
    let _ = b.ret(helper.param(tcx, 1).unwrap());

    let entry_ty = tcx.function(i32_ty, &[p_f, i32_ty], false);
    let entry = ok(m.add_function(b"entry", entry_ty));
    b.position_at_end(entry.append_block(&cx, c"entry").unwrap());
    let call = b.call(
        helper,
        &[entry.param(tcx, 0).unwrap(), entry.param(tcx, 1).unwrap()],
    );
    let _ = b.ret(call);

    module::optimize(&m);
    let (values, _, stale) = ok(build_table(&m));
    assert_eq!(stale, 1);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].value, entry.val().raw());

    let (metallib, stats, write_stale) = ok(write_with_stats(&m, &[]));
    assert_ne!(metallib, [] as [u8; 0]);
    assert_eq!(stats.use_typed, 0);
    assert_eq!(write_stale, 1);
}

#[test]
fn table_leaves_out_a_deleted_declaration() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"shader.air");
    let entry_ty = tcx.function(tcx.void(), &[], false);
    let entry = ok(m.add_function(b"entry", entry_ty));
    let b = Builder::new(&cx);
    b.position_at_end(entry.append_block(&cx, c"entry").unwrap());
    let decl_ty = tcx.function(tcx.float(), &[tcx.float()], false);
    let _ = ok(m.get_or_insert_function(b"air.dfdx.f32", decl_ty, &[]));
    b.ret_void();

    module::optimize(&m);
    let (values, _, stale) = ok(build_table(&m));
    assert_eq!(stale, 1);
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].value, entry.val().raw());
}

#[test]
fn struct_bodies_are_typed() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"shader.air");
    let p_f = tcx.ptr(tcx.float(), 1);
    let p_i = tcx.ptr(tcx.int(32), 2);
    let arg_buf = tcx.create_struct(c"argument_buffer_struct", Some((&[p_f, p_i], false)));
    // An opaque struct and a lifted struct holding `Unknown` get no body entry.
    let _opaque = tcx.create_struct(c"opaque_only", None);
    let _lifted_unknown = tcx.create_struct(
        c"lifted_unknown",
        Some((&[tcx.ptr(tcx.unknown(), 1)], false)),
    );

    let (_, structs, _) = ok(build_table(&m));
    assert_eq!(structs.len(), 1);
    assert_eq!(structs[0].struct_ty, tcx.lower(arg_buf));
    assert_eq!(
        print_raw_type(structs[0].body),
        "{ typedptr(float, 1), typedptr(i32, 2) }"
    );
}

#[test]
fn write_returns_the_writers_error_as_text() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"broken.air");
    let f = ok(m.add_function(b"f", tcx.function(tcx.void(), &[], false)));
    let _ = f.append_block(&cx, c"entry").unwrap();

    let err = write(&m, &[]).expect_err("a block without a terminator must fail verification");
    let text = String::from_utf8_lossy(err.text());
    assert!(text.starts_with("AIR writer: "), "{text}");
    assert!(text.contains("terminator"), "{text}");
}

#[test]
fn write_types_the_helpers_from_their_refs() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"shader.air");
    let i32_ty = tcx.int(32);
    let p3 = tcx.ptr(tcx.float(), 3);
    let p7 = tcx.ptr(tcx.int(8), 7);
    let f = ok(m.add_function(b"entry", tcx.function(tcx.void(), &[i32_ty, p3, p7], false)));
    let b = Builder::new(&cx);
    b.position_at_end(f.append_block(&cx, c"entry").unwrap());
    let helper_ty = tcx.function(tcx.void(), &[i32_ty, p3, p7], false);
    let helper = ok(m.get_or_insert_function(
        b"dxmt.domain_generate_primitives.triangle",
        helper_ty,
        &[(sys::ATTRIBUTE_FUNCTION_INDEX, b"nounwind")],
    ));
    let _ = b.call(
        helper,
        &[
            f.param(tcx, 0).unwrap(),
            f.param(tcx, 1).unwrap(),
            f.param(tcx, 2).unwrap(),
        ],
    );
    b.ret_void();

    let refs = ok(module::link_helpers(&m, false, false, true));
    assert_eq!(refs.len(), 1);
    module::optimize(&m);
    let (metallib, stats, _) = ok(write_with_stats(&m, &refs));
    assert_ne!(metallib, [] as [u8; 0]);
    assert!(stats.refs_filled > 0, "refs_filled={}", stats.refs_filled);
    assert_eq!(stats.use_typed, 0);
}

#[test]
fn const_vector_splat_is_written() {
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"shader.air");
    let f32_ty = tcx.float();
    let half = cx.const_real(f32_ty, 0.5);
    let splat = cx.const_vector(&[half, half, half]);
    let printed = String::from_utf8(splat.print()).unwrap();
    assert!(printed.contains("splat"), "{printed}");

    let v3f = tcx.vector(f32_ty, 3);
    let f = ok(m.add_function(b"entry", tcx.function(v3f, &[], false)));
    let b = Builder::new(&cx);
    b.position_at_end(f.append_block(&cx, c"entry").unwrap());
    let _ = b.ret(splat);

    let metallib = ok(write(&m, &[]));
    assert_ne!(metallib, [] as [u8; 0]);
}

#[cfg(feature = "oracle")]
#[test]
fn oracle_files_are_named_by_the_metallib() {
    use super::write_oracle_files;
    let cx = Context::new();
    let tcx = cx.tcx();
    let m = module::initialize(&cx, c"shader.air");
    let f = ok(m.add_function(b"entry", tcx.function(tcx.void(), &[], false)));
    let b = Builder::new(&cx);
    b.position_at_end(f.append_block(&cx, c"entry").unwrap());
    b.ret_void();

    let dir = std::env::temp_dir().join(format!("airconv-oracle-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let (metallib, stats, stale) = ok(write_with_stats(&m, &[]));
    write_oracle_files(&dir, &m, &metallib, &stats, stale);

    let digest = airconv_ir::sha256::digest(&metallib);
    let mut sha = String::with_capacity(64);
    for byte in digest {
        use core::fmt::Write as _;
        let _ = write!(sha, "{byte:02x}");
    }
    let log_path = dir.join(format!("{sha}.log"));
    let post_path = dir.join(format!("{sha}.post.ll"));
    let log = std::fs::read_to_string(&log_path).unwrap();
    assert_eq!(
        log,
        "stats inst-casts=0 const-casts=0 use-typed=0 attrs-dropped=0 flags-dropped=0 refs=0 stale=0\n"
    );
    let post = std::fs::read_to_string(&post_path).unwrap();
    assert!(post.contains("@entry()"), "{post}");

    let _ = std::fs::remove_dir_all(&dir);
}
