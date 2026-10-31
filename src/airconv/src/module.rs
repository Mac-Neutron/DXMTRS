//! A module's setup, its Metal helpers and the pass pipeline.
//!
//! Ported from `airconv_context.cpp`: [`initialize`] (and [`set_up`], for a parsed module) gives a
//! module AIR's source name, triple, data layout, SDK version and module flags; [`link_helpers`]
//! links the Metal helpers a shader calls (msad, sample positions, tessellation), compiled to AIR
//! by `build.rs`; [`optimize`] runs airconv's pass pipeline through the shim. [`parse`] reads
//! bitcode as the C++'s `parseBitcodeFile` calls did.

use core::{ffi::CStr, ptr, slice};

use crate::{
    api::Error,
    llvm::{Context, Module, sys},
};

/// The Metal helpers as AIR bitcode, `shaders/*.metal` compiled by `build.rs`.
const MSAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/air_msad.air"));
const SAMPLEPOS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/air_samplepos.air"));
const TESSELLATION: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/air_tessellation.air"));

/// The named metadata a helper's module loses before it is linked (`linkShader`).
const HELPER_METADATA: [&[u8]; 6] = [
    b"air.compile_options",
    b"air.version",
    b"air.language_version",
    b"air.source_file_name",
    b"llvm.ident",
    b"llvm.module.flags",
];

const SOURCE_FILE_NAME: &[u8] = b"airconv_generated.metal";
const TRIPLE: &CStr = c"air64-apple-macosx14.0.0";
const DATA_LAYOUT: &CStr = c"e-p:64:64:64-i1:8:8-i8:8:8-i16:16:16-i32:32:32-i64:64:64-f32:32:32-f64:64:64-v16:16:16-v24:32:32-v32:32:32-v48:64:64-v64:64:64-v96:128:128-v128:128:128-v192:256:256-v256:256:256-v512:512:512-v1024:1024:1024-n8:16:32";

/// `Module::ModFlagBehavior`'s `Error` and `Max` (LLVM's C enum has no `Max`).
const FLAG_ERROR: u64 = 1;
const FLAG_MAX: u64 = 7;

/// The module flags after the SDK version, in `initializeModule`'s order: behaviour, key, value.
const MODULE_FLAGS: [(u64, &[u8], u64); 9] = [
    (FLAG_ERROR, b"wchar_size", 4),
    (FLAG_MAX, b"frame-pointer", 2),
    (FLAG_MAX, b"air.max_device_buffers", 31),
    (FLAG_MAX, b"air.max_constant_buffers", 31),
    (FLAG_MAX, b"air.max_threadgroup_buffers", 31),
    (FLAG_MAX, b"air.max_textures", 128),
    (FLAG_MAX, b"air.max_read_write_textures", 8),
    (FLAG_MAX, b"air.max_samplers", 16),
    // Varyings of different widths link (DXIL's are at their signature's width): as Metal Shader
    // Converter's modules.
    (FLAG_MAX, b"agx.allow_mismatched_component_counts", 1),
];

/// An empty module named `name`, set up for AIR (`Module(name, context)`, then [`set_up`]).
#[must_use]
pub fn initialize<'cx>(cx: &'cx Context, name: &CStr) -> Module<'cx> {
    let m = Module::new(cx, name);
    set_up(&m);
    m
}

/// `initializeModule`: AIR's source name, triple, data layout, SDK version 15.0 and nine flags.
///
/// The flags are added after any the module has, each as `Module::addModuleFlag` makes it.
pub fn set_up(m: &Module<'_>) {
    // SAFETY: a live module; the name holds its length.
    unsafe {
        sys::set_source_file_name(
            m.raw(),
            SOURCE_FILE_NAME.as_ptr().cast(),
            SOURCE_FILE_NAME.len(),
        );
    };
    // SAFETY: a live module; a NUL-terminated triple.
    unsafe { sys::set_target(m.raw(), TRIPLE.as_ptr()) };
    // SAFETY: a live module; a NUL-terminated data layout LLVM parses.
    unsafe { sys::set_data_layout(m.raw(), DATA_LAYOUT.as_ptr()) };
    // SAFETY: a live module.
    unsafe { sys::airconv_shim_set_sdk_version(m.raw(), 15, 0) };
    let cx = m.cx();
    let i32_ty = cx.tcx().int(32);
    for (behavior, key, value) in MODULE_FLAGS {
        let flag = cx.md_node(&[
            cx.const_int(i32_ty, behavior, false).as_metadata(),
            cx.md_string(key),
            cx.const_int(i32_ty, value, false).as_metadata(),
        ]);
        // SAFETY: the context is live and `flag` its metadata.
        let flag = unsafe { sys::metadata_as_value(cx.raw(), flag) };
        // SAFETY: a live module; a NUL-terminated name; `flag` wraps a metadata node.
        unsafe { sys::add_named_metadata_operand(m.raw(), c"llvm.module.flags".as_ptr(), flag) };
    }
}

/// How [`parse`] handles the bitcode module's data layout.
#[derive(Clone, Copy)]
pub enum Layout {
    /// Keep the bitcode module's data layout as written.
    Keep,
    /// Remove DXIL's `i8:32` component so LLVM 23's reader accepts the layout (P5).
    Dxil,
}

/// Parses `bitcode` into a module of `cx` (`parseBitcodeFile`), its buffer named `name`.
///
/// # Errors
///
/// An error LLVM reported to the context while reading (P5's `LLVM error: …`, where the C++
/// exited), or else `prefix` followed by the reader's message (`toString` of its error).
pub fn parse<'cx>(
    cx: &'cx Context,
    bitcode: &[u8],
    name: &[u8],
    prefix: &[u8],
    layout: Layout,
) -> Result<Module<'cx>, Error> {
    let mut out = ptr::null_mut();
    let mut text = ptr::null_mut();
    let mut len = 0;
    let dxil_layout = matches!(layout, Layout::Dxil).into();
    // SAFETY: the context is live; `bitcode` and `name` hold their lengths; the three outputs are
    // writable.
    let failed = unsafe {
        sys::airconv_shim_parse_bitcode(
            cx.raw(),
            bitcode.as_ptr().cast(),
            bitcode.len(),
            name.as_ptr().cast(),
            name.len(),
            dxil_layout,
            &raw mut out,
            &raw mut text,
            &raw mut len,
        )
    } != 0;
    // SAFETY: the shim answers null or a module of `cx` it hands over.
    let module = (!out.is_null()).then(|| unsafe { Module::from_raw(cx, out) });
    let message = if text.is_null() {
        Vec::new()
    } else {
        // SAFETY: the shim answers `len` bytes at `text`.
        let bytes = unsafe { slice::from_raw_parts(text.cast::<u8>(), len) }.to_vec();
        // SAFETY: the shim's message, freed once.
        unsafe { sys::dispose_message(text) };
        bytes
    };
    if let Some(error) = cx.take_error() {
        return Err(error);
    }
    match module {
        Some(module) if !failed => Ok(module),
        _ => Err(Error::new(&[prefix, &message].concat())),
    }
}

/// Links the Metal helpers the module calls, each only for what it uses (`linkShader`).
///
/// # Errors
///
/// P15's `LLVM error: …` when a helper does not parse or link (the C++ printed a parse failure
/// and went on without the helper, and exited on a link error).
pub fn link_helpers(
    m: &Module<'_>,
    msad: bool,
    samplepos: bool,
    tessellation: bool,
) -> Result<(), Error> {
    for (wanted, air) in [
        (msad, MSAD),
        (samplepos, SAMPLEPOS),
        (tessellation, TESSELLATION),
    ] {
        if wanted {
            link_helper(m, air)?;
        }
    }
    Ok(())
}

/// `runOptimizationPasses`: airconv's pass pipeline, its two passes included.
pub fn optimize(m: &Module<'_>) {
    // SAFETY: a live module.
    unsafe { sys::airconv_shim_run_passes(m.raw()) };
}

fn link_helper(m: &Module<'_>, air: &[u8]) -> Result<(), Error> {
    let cx = m.cx();
    let helper = parse(cx, air, b"", b"LLVM error: ", Layout::Keep)?;
    for name in HELPER_METADATA {
        // SAFETY: a live module; `name` holds its length.
        unsafe {
            sys::airconv_shim_erase_named_metadata(helper.raw(), name.as_ptr().cast(), name.len());
        }
    }
    // SAFETY: both modules are live and of `cx`; the link takes the helper.
    let failed = unsafe { sys::airconv_shim_link_only_needed(m.raw(), helper.into_raw()) } != 0;
    match cx.take_error() {
        Some(error) => Err(error),
        None if failed => Err(Error::new(b"LLVM error: a Metal helper does not link")),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
