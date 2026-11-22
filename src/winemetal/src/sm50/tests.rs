//! Each `SM50*` export against a Unix side that records what it was called with.

use core::{
    any::Any,
    cell::RefCell,
    ffi::{c_char, c_void},
    ptr,
};

use winemetal_wire::{
    Sm50CompileGeometryPipelineGeometryParams, Sm50CompileGeometryPipelineVertexParams,
    Sm50CompileParams, Sm50CompileTessellationPipelineDomainParams,
    Sm50CompileTessellationPipelineHullParams, Sm50DestroyBitcodeParams, Sm50DestroyParams,
    Sm50FreeErrorParams, Sm50GetArgumentsInfoParams, Sm50GetCompiledBitcodeParams,
    Sm50GetErrorMessageParams, Sm50InitializeParams, slot,
};

use super::{
    SM50Compile, SM50CompileGeometryPipelineGeometry, SM50CompileGeometryPipelineVertex,
    SM50CompileTessellationPipelineDomain, SM50CompileTessellationPipelineHull, SM50Destroy,
    SM50DestroyBitcode, SM50FreeError, SM50GetArgumentsInfo, SM50GetCompiledBitcode,
    SM50GetErrorMessage, SM50Initialize,
};

/// A Unix-call handler, as `winemetal.so`'s table holds them.
type Handler = unsafe extern "C-unwind" fn(*mut c_void) -> i32;

/// The Unix side of the calling test: what it answers, and the last call it saw.
struct Unix {
    /// The status the next call answers.
    status: i32,
    /// What the next call writes into its params before it answers.
    ///
    /// A `fn(&mut T)` of the slot's own params type.
    reply: Option<Box<dyn Any>>,
    /// The last call: its slot, and its params as they arrived.
    seen: Option<(u32, Box<dyn Any>)>,
}

thread_local! {
    /// The calling test's Unix side.
    ///
    /// A handler is reached through the C table below with the params as its one argument, so
    /// it has no object to record into; a thread-local keeps the tests, which run on a thread
    /// each, out of each other's record.
    static UNIX: RefCell<Unix> = const { RefCell::new(Unix { status: 0, reply: None, seen: None }) };
}

/// The handler of `SLOT`, whose params are a `T`: records them, then answers as scripted.
unsafe extern "C-unwind" fn record<T: Copy + 'static, const SLOT: u32>(args: *mut c_void) -> i32 {
    // SAFETY: the thunk of `SLOT` passes a pointer to its own `T`.
    let params = unsafe { &mut *args.cast::<T>() };
    UNIX.with_borrow_mut(|unix| {
        unix.seen = Some((SLOT, Box::new(*params)));
        if let Some(reply) = unix
            .reply
            .as_ref()
            .and_then(|reply| reply.downcast_ref::<fn(&mut T)>())
        {
            reply(params);
        }
        unix.status
    })
}

/// `winemetal.so`'s call table with a recording handler in each SM50 slot.
const fn table() -> [Option<Handler>; 165] {
    let mut table: [Option<Handler>; 165] = [None; 165];
    table[slot::SM50_INITIALIZE as usize] =
        Some(record::<Sm50InitializeParams, { slot::SM50_INITIALIZE }>);
    table[slot::SM50_DESTROY as usize] = Some(record::<Sm50DestroyParams, { slot::SM50_DESTROY }>);
    table[slot::SM50_COMPILE as usize] = Some(record::<Sm50CompileParams, { slot::SM50_COMPILE }>);
    table[slot::SM50_GET_COMPILED_BITCODE as usize] =
        Some(record::<Sm50GetCompiledBitcodeParams, { slot::SM50_GET_COMPILED_BITCODE }>);
    table[slot::SM50_DESTROY_BITCODE as usize] =
        Some(record::<Sm50DestroyBitcodeParams, { slot::SM50_DESTROY_BITCODE }>);
    table[slot::SM50_GET_ERROR_MESSAGE as usize] =
        Some(record::<Sm50GetErrorMessageParams, { slot::SM50_GET_ERROR_MESSAGE }>);
    table[slot::SM50_FREE_ERROR as usize] =
        Some(record::<Sm50FreeErrorParams, { slot::SM50_FREE_ERROR }>);
    table[slot::SM50_COMPILE_GEOMETRY_PIPELINE_VERTEX as usize] = Some(
        record::<
            Sm50CompileGeometryPipelineVertexParams,
            { slot::SM50_COMPILE_GEOMETRY_PIPELINE_VERTEX },
        >,
    );
    table[slot::SM50_COMPILE_GEOMETRY_PIPELINE_GEOMETRY as usize] = Some(
        record::<
            Sm50CompileGeometryPipelineGeometryParams,
            { slot::SM50_COMPILE_GEOMETRY_PIPELINE_GEOMETRY },
        >,
    );
    table[slot::SM50_COMPILE_TESSELLATION_HULL as usize] = Some(
        record::<Sm50CompileTessellationPipelineHullParams, { slot::SM50_COMPILE_TESSELLATION_HULL }>,
    );
    table[slot::SM50_COMPILE_TESSELLATION_DOMAIN as usize] = Some(
        record::<
            Sm50CompileTessellationPipelineDomainParams,
            { slot::SM50_COMPILE_TESSELLATION_DOMAIN },
        >,
    );
    table[slot::SM50_GET_ARGUMENTS_INFO as usize] =
        Some(record::<Sm50GetArgumentsInfoParams, { slot::SM50_GET_ARGUMENTS_INFO }>);
    table
}

/// The table the thunks call through natively, which `winemetal.so` defines in a real image.
#[unsafe(export_name = "__wine_unix_call_funcs")]
static UNIX_CALL_FUNCS: [Option<Handler>; 165] = table();

/// Scripts the next call: its status, and what it writes into its `T`.
fn script<T: 'static>(status: i32, reply: fn(&mut T)) {
    UNIX.with_borrow_mut(|unix| {
        unix.status = status;
        unix.seen = None;
        unix.reply = Some(Box::new(reply));
    });
}

/// The params of the last call, which reached `slot`.
fn seen<T: Copy + 'static>(slot: u32) -> T {
    let (at, params) = UNIX
        .with_borrow_mut(|unix| unix.seen.take())
        .expect("a Unix call was made");
    assert_eq!(at, slot, "the slot the thunk called");
    *params.downcast::<T>().expect("the slot's own params type")
}

/// A pointer that is only ever compared, at `addr`.
const fn at<T>(addr: usize) -> *mut T {
    ptr::without_provenance_mut(addr)
}

#[test]
fn initialize_passes_its_arguments_and_answers_ret() {
    script(0, |p: &mut Sm50InitializeParams| p.ret = 7);
    // SAFETY: the recording Unix side dereferences none of the pointers.
    let ret = unsafe { SM50Initialize(at(0x10), 0x20, at(0x30), at(0x40), at(0x50)) };
    let p: Sm50InitializeParams = seen(slot::SM50_INITIALIZE);
    assert_eq!(ret, 7);
    assert_eq!(
        (p.bytecode, p.bytecode_size),
        (at::<c_void>(0x10).cast_const(), 0x20)
    );
    assert_eq!(
        (p.shader, p.reflection, p.error),
        (at(0x30), at(0x40), at(0x50))
    );
    assert_eq!(p.ret, 0, "ret is zero until the handler writes it");

    script(1, |p: &mut Sm50InitializeParams| p.ret = 7);
    // SAFETY: as above.
    let ret = unsafe { SM50Initialize(at(0x10), 0x20, at(0x30), at(0x40), at(0x50)) };
    assert_eq!(ret, -1, "a failed Unix call answers -1 whatever ret holds");
}

#[test]
fn compile_passes_its_arguments_and_answers_ret() {
    script(0, |p: &mut Sm50CompileParams| p.ret = -3);
    // SAFETY: the recording Unix side dereferences none of the pointers.
    let ret = unsafe { SM50Compile(0x1_0000_0002, at(0x20), at(0x30), at(0x40), at(0x50)) };
    let p: Sm50CompileParams = seen(slot::SM50_COMPILE);
    assert_eq!(ret, -3);
    assert_eq!(
        (p.shader, p.args, p.func_name),
        (0x1_0000_0002, at(0x20), at::<c_char>(0x30).cast_const())
    );
    assert_eq!((p.bitcode, p.error), (at(0x40), at(0x50)));

    script(-1, |p: &mut Sm50CompileParams| p.ret = 0);
    // SAFETY: as above.
    let ret = unsafe { SM50Compile(1, at(0x20), at(0x30), at(0x40), at(0x50)) };
    assert_eq!(ret, -1);
}

#[test]
fn the_tessellation_pair_keeps_each_shader_in_its_field() {
    script(0, |p: &mut Sm50CompileTessellationPipelineHullParams| {
        p.ret = 5;
    });
    // SAFETY: the recording Unix side dereferences none of the pointers.
    let ret = unsafe {
        SM50CompileTessellationPipelineHull(0xa1, 0xa2, at(0x20), at(0x30), at(0x40), at(0x50))
    };
    let p: Sm50CompileTessellationPipelineHullParams = seen(slot::SM50_COMPILE_TESSELLATION_HULL);
    assert_eq!(
        (ret, p.vertex, p.hull, p.hull_args),
        (5, 0xa1, 0xa2, at(0x20))
    );
    assert_eq!(
        (p.func_name, p.bitcode, p.error),
        (at::<c_char>(0x30).cast_const(), at(0x40), at(0x50))
    );

    script(0, |p: &mut Sm50CompileTessellationPipelineDomainParams| {
        p.ret = 6;
    });
    // SAFETY: as above.
    let ret = unsafe {
        SM50CompileTessellationPipelineDomain(0xb1, 0xb2, at(0x21), at(0x31), at(0x41), at(0x51))
    };
    let p: Sm50CompileTessellationPipelineDomainParams =
        seen(slot::SM50_COMPILE_TESSELLATION_DOMAIN);
    assert_eq!(
        (ret, p.hull, p.domain, p.domain_args),
        (6, 0xb1, 0xb2, at(0x21))
    );
    assert_eq!(
        (p.func_name, p.bitcode, p.error),
        (at::<c_char>(0x31).cast_const(), at(0x41), at(0x51))
    );

    script(2, |p: &mut Sm50CompileTessellationPipelineHullParams| {
        p.ret = 5;
    });
    // SAFETY: as above.
    let hull = unsafe {
        SM50CompileTessellationPipelineHull(0xa1, 0xa2, at(0x20), at(0x30), at(0x40), at(0x50))
    };
    script(2, |p: &mut Sm50CompileTessellationPipelineDomainParams| {
        p.ret = 6;
    });
    // SAFETY: as above.
    let domain = unsafe {
        SM50CompileTessellationPipelineDomain(0xb1, 0xb2, at(0x21), at(0x31), at(0x41), at(0x51))
    };
    assert_eq!((hull, domain), (-1, -1));
}

#[test]
fn the_geometry_pair_keeps_each_shader_in_its_field() {
    script(0, |p: &mut Sm50CompileGeometryPipelineVertexParams| {
        p.ret = 8;
    });
    // SAFETY: the recording Unix side dereferences none of the pointers.
    let ret = unsafe {
        SM50CompileGeometryPipelineVertex(0xc1, 0xc2, at(0x20), at(0x30), at(0x40), at(0x50))
    };
    let p: Sm50CompileGeometryPipelineVertexParams =
        seen(slot::SM50_COMPILE_GEOMETRY_PIPELINE_VERTEX);
    assert_eq!(
        (ret, p.vertex, p.geometry, p.vertex_args),
        (8, 0xc1, 0xc2, at(0x20))
    );
    assert_eq!(
        (p.func_name, p.bitcode, p.error),
        (at::<c_char>(0x30).cast_const(), at(0x40), at(0x50))
    );

    script(0, |p: &mut Sm50CompileGeometryPipelineGeometryParams| {
        p.ret = 9;
    });
    // SAFETY: as above.
    let ret = unsafe {
        SM50CompileGeometryPipelineGeometry(0xd1, 0xd2, at(0x21), at(0x31), at(0x41), at(0x51))
    };
    let p: Sm50CompileGeometryPipelineGeometryParams =
        seen(slot::SM50_COMPILE_GEOMETRY_PIPELINE_GEOMETRY);
    assert_eq!(
        (ret, p.vertex, p.geometry, p.geometry_args),
        (9, 0xd1, 0xd2, at(0x21))
    );
    assert_eq!(
        (p.func_name, p.bitcode, p.error),
        (at::<c_char>(0x31).cast_const(), at(0x41), at(0x51))
    );

    script(3, |p: &mut Sm50CompileGeometryPipelineVertexParams| {
        p.ret = 8;
    });
    // SAFETY: as above.
    let vertex = unsafe {
        SM50CompileGeometryPipelineVertex(0xc1, 0xc2, at(0x20), at(0x30), at(0x40), at(0x50))
    };
    script(3, |p: &mut Sm50CompileGeometryPipelineGeometryParams| {
        p.ret = 9;
    });
    // SAFETY: as above.
    let geometry = unsafe {
        SM50CompileGeometryPipelineGeometry(0xd1, 0xd2, at(0x21), at(0x31), at(0x41), at(0x51))
    };
    assert_eq!((vertex, geometry), (-1, -1));
}

#[test]
fn the_error_message_answers_the_size_the_handler_wrote() {
    script(0, |p: &mut Sm50GetErrorMessageParams| p.ret_size = 12);
    // SAFETY: the recording Unix side dereferences none of the pointers.
    let size = unsafe { SM50GetErrorMessage(0xe1, at(0x20), 0x30) };
    let p: Sm50GetErrorMessageParams = seen(slot::SM50_GET_ERROR_MESSAGE);
    assert_eq!(
        (size, p.error, p.buffer, p.buffer_size, p.ret_size),
        (12, 0xe1, at(0x20), 0x30, 0)
    );

    script(1, |_: &mut Sm50GetErrorMessageParams| {});
    // SAFETY: as above.
    let size = unsafe { SM50GetErrorMessage(0xe1, at(0x20), 0x30) };
    assert_eq!(
        size, 0,
        "a handler that writes nothing leaves the size at 0"
    );
}

#[test]
fn the_calls_that_answer_nothing_pass_their_arguments() {
    script(1, |_: &mut Sm50DestroyParams| {});
    // SAFETY: the recording Unix side dereferences none of the pointers.
    unsafe { SM50Destroy(0xf1) };
    assert_eq!(seen::<Sm50DestroyParams>(slot::SM50_DESTROY).shader, 0xf1);

    script(1, |_: &mut Sm50DestroyBitcodeParams| {});
    // SAFETY: as above.
    unsafe { SM50DestroyBitcode(0xf2) };
    assert_eq!(
        seen::<Sm50DestroyBitcodeParams>(slot::SM50_DESTROY_BITCODE).bitcode,
        0xf2
    );

    script(1, |_: &mut Sm50FreeErrorParams| {});
    // SAFETY: as above.
    unsafe { SM50FreeError(0xf3) };
    assert_eq!(
        seen::<Sm50FreeErrorParams>(slot::SM50_FREE_ERROR).error,
        0xf3
    );

    script(0, |_: &mut Sm50GetCompiledBitcodeParams| {});
    // SAFETY: as above.
    unsafe { SM50GetCompiledBitcode(0xf4, at(0x20)) };
    let p: Sm50GetCompiledBitcodeParams = seen(slot::SM50_GET_COMPILED_BITCODE);
    assert_eq!((p.bitcode, p.data_out), (0xf4, at(0x20)));

    script(0, |_: &mut Sm50GetArgumentsInfoParams| {});
    // SAFETY: as above.
    unsafe { SM50GetArgumentsInfo(0xf5, at(0x20), at(0x30)) };
    let p: Sm50GetArgumentsInfoParams = seen(slot::SM50_GET_ARGUMENTS_INFO);
    assert_eq!(
        (p.shader, p.constant_buffers, p.arguments),
        (0xf5, at(0x20), at(0x30))
    );
}
