//! The module's AIR version and language metadata, and a function's entry in the module.
//!
//! Ported from `setup_metal_version` (`dxbc_converter.cpp:201-232`) and the converters'
//! `getOrInsertNamedMetadata(<stage>)->addOperand(function_metadata)`. Named metadata prints in
//! the order it was made: the converters set the version right after `CreateFunction` and add the
//! function's entry last.

use core::ffi::CStr;

use super::signature::Entry;
use crate::{
    abi::SM50_SHADER_METAL_320,
    llvm::{Module, sys},
};

/// `setup_metal_version`: the AIR and Metal versions the module is written for.
///
/// AIR 2.7, Metal 3.2 and macOS 15's triple for `SM50_SHADER_METAL_320`; AIR 2.6 and Metal 3.1
/// for any other version, the module's triple kept.
pub fn set_metal_version(m: &Module<'_>, metal_version: u32) {
    let cx = m.cx();
    let i32_ty = cx.tcx().int(32);
    let int = |value| cx.const_int(i32_ty, value, false).as_metadata();
    let metal_320 = metal_version == SM50_SHADER_METAL_320;
    let (air_minor, metal_minor) = if metal_320 { (7, 2) } else { (6, 1) };
    add(
        m,
        c"air.version",
        cx.md_node(&[int(2), int(air_minor), int(0)]),
    );
    let language = [cx.md_string(b"Metal"), int(3), int(metal_minor), int(0)];
    add(m, c"air.language_version", cx.md_node(&language));
    if metal_320 {
        // SAFETY: a live module; a NUL-terminated triple.
        unsafe { sys::set_target(m.raw(), c"air64-apple-macosx15.0.0".as_ptr()) };
    }
}

/// Adds a function's entry to its stage's list.
///
/// The list is `air.vertex`, `air.fragment`, `air.kernel`, `air.object` or `air.mesh`.
pub fn add_entry<'cx>(m: &Module<'cx>, stage: &CStr, entry: Entry<'cx>) {
    add(m, stage, entry.node());
}

/// `getOrInsertNamedMetadata(name)->addOperand(node)`, `node` metadata of the module's context.
fn add(m: &Module<'_>, name: &CStr, node: sys::MetadataRef) {
    let cx = m.cx();
    // SAFETY: the context is live and `node` its metadata.
    let value = unsafe { sys::metadata_as_value(cx.raw(), node) };
    // SAFETY: a live module; a NUL-terminated name; `value` wraps a metadata node.
    unsafe { sys::add_named_metadata_operand(m.raw(), name.as_ptr(), value) };
}
