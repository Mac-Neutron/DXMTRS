use core::{
    ffi::{c_char, c_void},
    ptr,
};

use super::*;
use crate::shader::Shader;

/// A handle to a new shader, as `SM50Initialize` would answer one.
fn shader() -> *mut c_void {
    Box::into_raw(Box::new(Shader)).cast()
}

/// The text of an error handle, read through `SM50GetErrorMessage`; frees the error.
fn message(error: *mut c_void) -> Vec<u8> {
    let mut buffer = [0u8; 256];
    // SAFETY: `error` is a live error handle and `buffer` holds its 256 bytes.
    let len = unsafe { sm50_get_error_message(error, buffer.as_mut_ptr().cast(), buffer.len()) };
    // SAFETY: `error` came from an export and is freed once.
    unsafe { sm50_free_error(error) };
    buffer[..len].to_vec()
}

/// One status export's answer: its status, then the error's text.
fn answer(call: impl FnOnce(*mut *mut c_void) -> i32) -> (i32, Vec<u8>) {
    let mut error = ptr::dangling_mut::<c_void>();
    let status = call(&raw mut error);
    assert!(!error.is_null(), "an error answer sets *ppError");
    (status, message(error))
}

#[test]
fn every_export_answers_not_ported() {
    let (vs, hs, gs, ps) = (shader(), shader(), shader(), shader());
    let name = c"main".as_ptr();
    let mut out = ptr::null_mut::<c_void>();
    let out = &raw mut out;
    let bytes = [0u8; 32];
    let not_ported = (1, b"not ported".to_vec());

    // Every call below: the handles are live shaders, `bytes` holds its length, the chains are
    // null, `name` is a C string, and `out` and the error are writable.
    let answers = [
        // SAFETY: as above.
        answer(|e| unsafe {
            sm50_initialize(bytes.as_ptr().cast(), bytes.len(), out, ptr::null_mut(), e)
        }),
        // SAFETY: as above.
        answer(|e| unsafe { sm50_compile(vs, ptr::null_mut(), name, out, e) }),
        // SAFETY: as above.
        answer(|e| unsafe {
            sm50_compile_tessellation_pipeline_hull(vs, hs, ptr::null_mut(), name, out, e)
        }),
        // SAFETY: as above.
        answer(|e| unsafe {
            sm50_compile_tessellation_pipeline_domain(hs, vs, ptr::null_mut(), name, out, e)
        }),
        // SAFETY: as above.
        answer(|e| unsafe {
            sm50_compile_geometry_pipeline_vertex(vs, gs, ptr::null_mut(), name, out, e)
        }),
        // SAFETY: as above.
        answer(|e| unsafe {
            sm50_compile_geometry_pipeline_geometry(vs, gs, ptr::null_mut(), name, out, e)
        }),
        // SAFETY: as above.
        answer(|e| unsafe { dxil_compile_pass_through_vertex(ps, out, e) }),
    ];
    for (i, got) in answers.iter().enumerate() {
        assert_eq!(got, &not_ported, "export {i}");
    }
    // SAFETY: nothing was written to `out`.
    assert!(unsafe { *out }.is_null());
    for handle in [vs, hs, gs, ps] {
        // SAFETY: each handle is a live shader, destroyed once.
        unsafe { sm50_destroy(handle) };
    }
}

#[test]
fn null_out_pointer_is_refused_as_the_cxx() {
    let vs = shader();
    // SAFETY: the out pointers are null, which the exports refuse before reading anything.
    let initialize =
        // SAFETY: as above.
        answer(|e| unsafe { sm50_initialize(ptr::null(), 0, ptr::null_mut(), ptr::null_mut(), e) });
    assert_eq!(initialize, (1, b"ppShader can not be null".to_vec()));
    // SAFETY: as above, `vs` is a live shader.
    let compile =
        // SAFETY: as above.
        answer(|e| unsafe { sm50_compile(vs, ptr::null_mut(), ptr::null(), ptr::null_mut(), e) });
    assert_eq!(compile, (1, b"ppBitcode can not be null".to_vec()));
    // SAFETY: `vs` is a live shader, destroyed once.
    unsafe { sm50_destroy(vs) };
}

#[test]
fn get_error_message_size_zero_writes_nothing() {
    let error = Box::into_raw(Box::new(Error::not_ported())).cast::<c_void>();
    let mut buffer = [0xaau8; 8];
    // SAFETY: `error` is a live error handle; a size of 0 reads no buffer.
    let len = unsafe { sm50_get_error_message(error, buffer.as_mut_ptr().cast(), 0) };
    assert_eq!((len, buffer), (0, [0xaa; 8]));

    // A short buffer takes what fits and its NUL.
    // SAFETY: `buffer` holds 4 bytes.
    let len = unsafe { sm50_get_error_message(error, buffer.as_mut_ptr().cast::<c_char>(), 4) };
    assert_eq!((len, &buffer[..5]), (3, &b"not\0\xaa"[..]));
    // SAFETY: `error` is freed once.
    unsafe { sm50_free_error(error) };
}

#[test]
fn error_text_keeps_non_utf8_bytes() {
    let error = Box::into_raw(Box::new(Error::new(b"x\xff"))).cast::<c_void>();
    assert_eq!(message(error), b"x\xff");
}
