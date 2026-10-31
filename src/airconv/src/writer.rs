//! The metallib writer's glue: a module's function list and its bitcode, packed.
//!
//! Ported from `metallib_writer.cpp`: [`function_entries`] reads the function list from the
//! module's `air.*` named metadata as `MetallibWriter::Write` walked it, and [`write()`] packs
//! it with the module's bitcode (`airconv_ir::metallib::pack`, which writes the container).

use core::{ffi::CStr, ptr, slice};

use airconv_ir::metallib::{self, FunctionEntry, FunctionKind, Patch, VertexAttribute};

use crate::llvm::{Module, sys};

/// The named metadata listing each kind's functions, in the C++'s order.
///
/// `pack` writes the functions in the order it is given them.
const KINDS: [(&CStr, MakeKind); 5] = [
    (c"air.vertex", || FunctionKind::Vertex),
    (c"air.fragment", || FunctionKind::Fragment),
    (c"air.kernel", || FunctionKind::Kernel),
    (c"air.object", || FunctionKind::Object),
    (c"air.mesh", || FunctionKind::Mesh),
];

/// The kind of the functions a named metadata lists (`FunctionKind` is not `Copy`).
type MakeKind = fn() -> FunctionKind;

/// The module's function list, from its `air.*` metadata.
///
/// The lists are read as `air.vertex`, `air.fragment`, `air.kernel`, `air.object` and
/// `air.mesh`. Only a vertex function carries vertex attributes (its `air.vertex_input`s) and a
/// patch (an `air.patch` tuple as its fourth operand), as in the C++. An entry the translator
/// never writes (no function, no input list) is left out or read as empty, where the C++ would
/// have crashed.
#[must_use]
pub fn function_entries(m: &Module<'_>) -> Vec<FunctionEntry> {
    let mut entries = Vec::new();
    for (list, kind) in KINDS {
        for function in named_metadata(m, list) {
            let Some(operands) = node_operands(function) else {
                continue;
            };
            let Some(name) = operands.first().and_then(|&f| function_name(f)) else {
                continue;
            };
            let mut entry = FunctionEntry {
                name,
                kind: kind(),
                vertex_attributes: Vec::new(),
                patch: None,
            };
            if matches!(entry.kind, FunctionKind::Vertex) {
                entry.patch = patch(&operands);
                entry.vertex_attributes = operands
                    .get(2)
                    .and_then(|&inputs| node_operands(inputs))
                    .map(|inputs| inputs.into_iter().filter_map(vertex_input).collect())
                    .unwrap_or_default();
            }
            entries.push(entry);
        }
    }
    entries
}

/// The module as a metallib: its bitcode and its function list.
///
/// The bitcode carries a module hash: `WriteBitcodeToFile(M, OS, false, nullptr, true)`.
#[must_use]
pub fn write(m: &Module<'_>) -> Vec<u8> {
    // SAFETY: a live module; the answer is a buffer disposed of below.
    let buffer = unsafe { sys::airconv_shim_write_bitcode(m.raw()) };
    // SAFETY: the buffer just made.
    let start = unsafe { sys::get_buffer_start(buffer) };
    // SAFETY: as above.
    let size = unsafe { sys::get_buffer_size(buffer) };
    // SAFETY: the buffer holds `size` bytes at `start` until it is disposed of.
    let bitcode = unsafe { slice::from_raw_parts(start.cast::<u8>(), size) }.to_vec();
    // SAFETY: the buffer, disposed of once.
    unsafe { sys::dispose_memory_buffer(buffer) };
    metallib::pack(&bitcode, &function_entries(m))
}

/// The operands of the named metadata `name` (each a node, as a metadata value).
fn named_metadata(m: &Module<'_>, name: &CStr) -> Vec<sys::ValueRef> {
    // SAFETY: a live module; a NUL-terminated name (0 when the module has none).
    let count = unsafe { sys::get_named_metadata_num_operands(m.raw(), name.as_ptr()) };
    let mut nodes = vec![ptr::null_mut(); count as usize];
    // SAFETY: as above; `nodes` holds one slot per operand.
    unsafe { sys::get_named_metadata_operands(m.raw(), name.as_ptr(), nodes.as_mut_ptr()) };
    nodes
}

/// The operands of the metadata node `node`, `None` for anything else.
///
/// A constant operand comes back as the constant, any other metadata as a metadata value, an
/// empty slot as null (`LLVMGetMDNodeOperands`). The C++ asked for an `MDTuple`; the C API tells
/// a node from a string, and the translator writes no node but tuples.
fn node_operands(node: sys::ValueRef) -> Option<Vec<sys::ValueRef>> {
    // SAFETY: null or a live value (a metadata walk's answer).
    if node.is_null() || unsafe { sys::is_amd_node(node) }.is_null() {
        return None;
    }
    // SAFETY: a metadata node.
    let count = unsafe { sys::get_md_node_num_operands(node) };
    let mut values = vec![ptr::null_mut(); count as usize];
    // SAFETY: as above; `values` holds one slot per operand.
    unsafe { sys::get_md_node_operands(node, values.as_mut_ptr()) };
    Some(values)
}

/// The bytes of the metadata string `value`, `None` for anything else.
fn md_string(value: sys::ValueRef) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    let mut len = 0;
    // SAFETY: a live value; `len` is writable (null answered for a value that is not a string).
    let bytes = unsafe { sys::get_md_string(value, &raw mut len) };
    // SAFETY: an `MDString` answers its `len` bytes, alive as long as its context.
    (!bytes.is_null())
        .then(|| unsafe { slice::from_raw_parts(bytes.cast::<u8>(), len as usize) }.to_vec())
}

/// The name of the function `value`, `None` for anything else.
fn function_name(value: sys::ValueRef) -> Option<Vec<u8>> {
    // SAFETY: a live value (a metadata operand).
    if value.is_null() || unsafe { sys::is_a_function(value) }.is_null() {
        return None;
    }
    let mut len = 0;
    // SAFETY: a function; `len` is writable.
    let name = unsafe { sys::get_value_name2(value, &raw mut len) };
    if name.is_null() {
        return Some(Vec::new());
    }
    // SAFETY: LLVM answers `len` bytes at `name`, alive while the function keeps its name.
    Some(unsafe { slice::from_raw_parts(name.cast::<u8>(), len) }.to_vec())
}

/// The patch of a vertex function, `None` when it has none.
///
/// It is a four-operand tuple `!{!"air.patch", !"<type>", …}` as the function's fourth operand:
/// a triangle if `<type>` says so and a quad otherwise.
fn patch(operands: &[sys::ValueRef]) -> Option<Patch> {
    let tuple = node_operands(*operands.get(3)?)?;
    if tuple.len() != 4 || md_string(*tuple.first()?)? != b"air.patch" {
        return None;
    }
    Some(Patch::from_air_name(&md_string(*tuple.get(1)?)?))
}

/// The vertex attribute an `air.vertex_input` element describes, `None` for another input.
///
/// Its location (operand 3) as the C++'s `(uint8_t)` cast keeps it, its type name (6) and its
/// argument name (8).
fn vertex_input(element: sys::ValueRef) -> Option<VertexAttribute> {
    let element = node_operands(element)?;
    if md_string(*element.get(1)?)? != b"air.vertex_input" {
        return None;
    }
    let location = *element.get(3)?;
    // SAFETY: a live value (a metadata operand), null checked first.
    if location.is_null() || unsafe { sys::is_a_constant_int(location) }.is_null() {
        return None;
    }
    // SAFETY: a constant integer.
    let [attribute, ..] = unsafe { sys::const_int_get_z_ext_value(location) }.to_le_bytes();
    Some(VertexAttribute {
        attribute,
        data_type: VertexAttribute::data_type_for(&md_string(*element.get(6)?)?),
        name: md_string(*element.get(8)?)?,
    })
}
