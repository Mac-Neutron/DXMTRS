//! D3D9 shaders through winemetal's SM50 slots.
//!
//! [`translate`] has the translator behind slots 74 to 80 (the Mac's own `winemetal.so`, or the
//! VM host's through the transport) make a metallib of a DXSO stage; [`new_library`] makes a
//! `MTLLibrary` of it (slots 114 and 25). The specialization travels as `SM50_SHADER_D3D9_DATA`,
//! bytes `airconv_ir::dxso::spec` builds and reads, so this crate needs no LLVM.
//!
//! The parameter blocks and the node are laid out at the process's own width: an x86-64 or arm64
//! process passes airconv's 64-bit layouts, an i686 one (old WoW64's `mtld3d.so` in the VM's
//! guest, whose `winemetal.so` is the WoW64 table) airconv's `*_params32` (`airconv_thunks.h`)
//! with 32-bit pointers, where `wmt.rs`'s structs hold 8-byte ones. The handles stay 64-bit.

use core::{
    ffi::{CStr, c_char, c_void},
    ptr,
};

use airconv_ir::dxso::spec::{SM50_SHADER_D3D9, StageSpec, VARIANT_KEY_WIRE, VS_SPEC_WIRE};

use crate::{
    meta,
    metal::{MTLDevice, MTLLibrary, handle_of},
    objc2::{self, Retained, runtime::ProtocolObject},
    table, wmt,
};

/// A library made of a metallib [`translate`] returned.
pub type Library = Retained<ProtocolObject<dyn MTLLibrary>>;

/// `SM50_SHADER_COMMON` and its Metal 3.1 target (AIR 2.6, as D3D10 to 12).
const SM50_SHADER_COMMON: u32 = 2;
const SM50_SHADER_METAL_310: u32 = 310;

/// Bytes of the error text asked for; longer is cut.
const ERROR_TEXT: usize = 4096;

/// `SM50_SHADER_COMMON_DATA`: 24 bytes on a 64-bit process, 16 on a 32-bit one.
#[repr(C)]
struct CommonData {
    next: *mut c_void,
    kind: u32,
    metal_version: u32,
    flags: u32,
}

/// `SM50_SHADER_D3D9_DATA`.
///
/// 48 bytes on a 64-bit process (`stage` at 12, `vs` at 16, `ps` at 22), 40 on a 32-bit one
/// (8, 12, 18).
#[repr(C)]
struct D3d9Data {
    next: *mut c_void,
    kind: u32,
    stage: u32,
    vs: [u8; VS_SPEC_WIRE],
    ps: [u8; VARIANT_KEY_WIRE],
}

const WIDE: bool = size_of::<usize>() == 8;
const _: () = {
    assert!(size_of::<CommonData>() == if WIDE { 24 } else { 16 });
    assert!(size_of::<D3d9Data>() == if WIDE { 48 } else { 40 });
    assert!(core::mem::offset_of!(D3d9Data, stage) == if WIDE { 12 } else { 8 });
    assert!(core::mem::offset_of!(D3d9Data, vs) == if WIDE { 16 } else { 12 });
    assert!(core::mem::offset_of!(D3d9Data, ps) == if WIDE { 22 } else { 18 });
    // airconv_thunks.h's params, and its `*_params32` (DXMT's i386 PE layout, 8-aligned) on a 32-bit process.
    assert!(size_of::<Initialize>() == if WIDE { 48 } else { 24 });
    assert!(size_of::<Compile>() == if WIDE { 48 } else { 32 });
    assert!(core::mem::offset_of!(Compile, args) == 8);
    assert!(size_of::<GetCompiledBitcode>() == 16);
    assert!(size_of::<GetErrorMessage>() == if WIDE { 32 } else { 24 });
    assert!(size_of::<CompiledBitcode>() == 16);
};

/// `sm50_initialize_params` (slot 74): 48 bytes, or `sm50_initialize_params32`'s 24.
#[repr(C, align(8))]
struct Initialize {
    bytecode: *const u8,
    bytecode_size: usize,
    shader: *mut u64,
    reflection: *mut c_void,
    error: *mut u64,
    ret: i32,
}

/// `sm50_compile_params` (slot 76): 48 bytes, or `sm50_compile_params32`'s 32.
#[repr(C, align(8))]
struct Compile {
    shader: u64,
    args: *mut c_void,
    func_name: *const c_char,
    bitcode: *mut u64,
    error: *mut u64,
    ret: i32,
}

/// `SM50_COMPILED_BITCODE`: `Data` is 64 bits on either width.
#[repr(C)]
struct CompiledBitcode {
    data: u64,
    size: u64,
}

/// `sm50_get_compiled_bitcode_params` (slot 77): 16 bytes.
#[repr(C, align(8))]
struct GetCompiledBitcode {
    bitcode: u64,
    data_out: *mut CompiledBitcode,
}

/// `sm50_get_error_message_params` (slot 79): 32 bytes, or the `32` form's 24.
#[repr(C, align(8))]
struct GetErrorMessage {
    error: u64,
    buffer: *mut u8,
    buffer_size: usize,
    ret_size: usize,
}

/// `unixcall_mtldevice_newlibrary` (slot 25).
#[repr(C)]
struct NewLibrary {
    device: u64,
    data: u64,
    ret_error: u64,
    ret_library: u64,
}

/// Gives a +1 handle back.
fn release(handle: u64) {
    if handle != 0 {
        let mut params = wmt::ObjNoRet { handle };
        // SAFETY: a +1 handle a slot returned, given up once.
        unsafe { table::call(wmt::RELEASE, &mut params) };
    }
}

/// The text of SM50 error `error`, which this frees and sets to 0.
fn take_error(error: &mut u64) -> String {
    if *error == 0 {
        return "no error text".to_owned();
    }
    let mut text = [0_u8; ERROR_TEXT];
    let mut params = GetErrorMessage {
        error: *error,
        buffer: text.as_mut_ptr(),
        buffer_size: text.len(),
        ret_size: 0,
    };
    // SAFETY: slot 79 writes at most `buffer_size` bytes into the buffer.
    let status = unsafe { table::call(wmt::SM50_GET_ERROR_MESSAGE, &mut params) };
    free_error(error);
    if status != 0 {
        return format!("error text unavailable (status {status:#x})");
    }
    let len = params.ret_size.min(text.len() - 1);
    String::from_utf8_lossy(text.get(..len).unwrap_or_default()).into_owned()
}

/// Gives error `error` back (slot 80) and sets it to 0; 0 is none.
fn free_error(error: &mut u64) {
    settle(error, |handle| {
        let mut params = wmt::ObjNoRet { handle };
        // SAFETY: slot 80 frees the error once.
        unsafe { table::call(wmt::SM50_FREE_ERROR, &mut params) };
    });
}

/// Hands the handle in `slot` to `give_back` and sets the slot to 0; 0 is none.
///
/// A later call on the same variable, such as the compile that leaves `*error` untouched, so never
/// gives the same handle back twice.
fn settle(slot: &mut u64, give_back: impl FnOnce(u64)) {
    let handle = core::mem::take(slot);
    if handle != 0 {
        give_back(handle);
    }
}

/// Gives shader `shader` back (slot 75).
fn destroy_shader(shader: u64) {
    if shader != 0 {
        let mut params = wmt::ObjNoRet { handle: shader };
        // SAFETY: slot 75 frees the shader once.
        unsafe { table::call(wmt::SM50_DESTROY, &mut params) };
    }
}

/// Gives bitcode `bitcode` back (slot 78).
fn destroy_bitcode(bitcode: u64) {
    if bitcode != 0 {
        let mut params = wmt::ObjNoRet { handle: bitcode };
        // SAFETY: slot 78 frees the bitcode once.
        unsafe { table::call(wmt::SM50_DESTROY_BITCODE, &mut params) };
    }
}

/// A metallib of the DXSO program `tokens`, as the stage `spec` says, its function named `name`.
///
/// `tokens` is the shader as D3D9 hands it over (`vs_2_0` and the rest, ending in the END token).
///
/// # Errors
/// The translator's text (or the transport's status) when it refuses the program or the
/// specialization.
pub fn translate(tokens: &[u32], spec: &StageSpec, name: &CStr) -> Result<Vec<u8>, String> {
    let (stage, vs, ps) = spec.to_wire();
    let mut shader = 0_u64;
    let mut error = 0_u64;
    let mut init = Initialize {
        bytecode: tokens.as_ptr().cast(),
        bytecode_size: size_of_val(tokens),
        shader: &raw mut shader,
        reflection: ptr::null_mut(),
        error: &raw mut error,
        ret: -1,
    };
    // SAFETY: slot 74 reads the tokens and writes the shader and error handles.
    let status = unsafe { table::call(wmt::SM50_INITIALIZE, &mut init) };
    if status != 0 || init.ret != 0 || shader == 0 {
        destroy_shader(shader);
        return Err(format!(
            "SM50Initialize (status {status:#x}, ret {}): {}",
            init.ret,
            take_error(&mut error)
        ));
    }
    free_error(&mut error);

    let mut d3d9 = D3d9Data {
        next: ptr::null_mut(),
        kind: SM50_SHADER_D3D9,
        stage,
        vs,
        ps,
    };
    let mut common = CommonData {
        next: (&raw mut d3d9).cast(),
        kind: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };
    let mut bitcode = 0_u64;
    let mut compile = Compile {
        shader,
        args: (&raw mut common).cast(),
        func_name: name.as_ptr(),
        bitcode: &raw mut bitcode,
        error: &raw mut error,
        ret: -1,
    };
    // SAFETY: slot 76 reads the chain and the name and writes the bitcode and error handles.
    let status = unsafe { table::call(wmt::SM50_COMPILE, &mut compile) };
    destroy_shader(shader);
    if status != 0 || compile.ret != 0 || bitcode == 0 {
        destroy_bitcode(bitcode);
        return Err(format!(
            "SM50Compile (status {status:#x}, ret {}): {}",
            compile.ret,
            take_error(&mut error)
        ));
    }
    free_error(&mut error);

    let mut out = CompiledBitcode { data: 0, size: 0 };
    let mut get = GetCompiledBitcode {
        bitcode,
        data_out: &raw mut out,
    };
    // SAFETY: slot 77 writes the bytes' address and length, valid until the bitcode is destroyed.
    let status = unsafe { table::call(wmt::SM50_GET_COMPILED_BITCODE, &mut get) };
    let bytes = if status == 0 && out.data != 0 {
        // SAFETY: `size` bytes at `data`, the bitcode's own, read before it is destroyed.
        Some(
            unsafe {
                core::slice::from_raw_parts(out.data as usize as *const u8, out.size as usize)
            }
            .to_vec(),
        )
    } else {
        None
    };
    destroy_bitcode(bitcode);
    bytes.ok_or_else(|| format!("SM50GetCompiledBitcode (status {status:#x}) gave no bytes"))
}

/// A library of the metallib `metallib` (what [`translate`] returned) on `device`.
///
/// `None` when winemetal makes none (the bytes are not a metallib Metal accepts).
pub fn new_library(device: &ProtocolObject<dyn MTLDevice>, metallib: &[u8]) -> Option<Library> {
    let mut data = wmt::ObjU64ObjRet {
        handle: metallib.as_ptr() as usize as u64,
        arg: metallib.len() as u64,
        ret: 0,
    };
    // SAFETY: slot 114 copies `arg` bytes from the address in `handle` into dispatch data.
    unsafe { table::call(wmt::DISPATCH_DATA_ALLOC_INIT, &mut data) };
    if data.ret == 0 {
        return None;
    }
    let device = handle_of(device);
    let mut params = NewLibrary {
        device,
        data: data.ret,
        ret_error: 0,
        ret_library: 0,
    };
    // SAFETY: slot 25 reads the device and the data, and returns +1 handles.
    unsafe { table::call(wmt::DEVICE_NEW_LIBRARY, &mut params) };
    release(data.ret);
    release(params.ret_error);
    let library = objc2::from_plus_one(params.ret_library)?;
    meta::with(params.ret_library, |e| e.device = device);
    Some(library)
}

#[cfg(test)]
mod tests;
