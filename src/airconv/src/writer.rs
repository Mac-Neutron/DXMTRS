//! The metallib writer's glue: a module's function list and its bitcode, packed.
//!
//! Ported from `metallib_writer.cpp`: [`function_entries`] reads the function list from the
//! module's `air.*` named metadata as `MetallibWriter::Write` walked it, and [`write()`] packs
//! it with the module's bitcode (`airconv_ir::metallib::pack`, which writes the container).

use core::{ffi::CStr, ptr, slice};

use airconv_ir::metallib::{self, FunctionEntry, FunctionKind, Patch, VertexAttribute};

use crate::{
    api::Error,
    llvm::{Module, sys, value_name},
};

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

/// Writes `m` as a metallib (`LLVMWriteAIRBitcode` and `airconv_ir::metallib::pack`).
///
/// `refs` are the bitcode slices of the Metal helpers linked into `m` (P6).
///
/// # Errors
///
/// `AIR writer: <message>` when a type cannot be lowered for the writer or `LLVMWriteAIRBitcode`
/// fails (P7).
pub fn write(m: &Module<'_>, refs: &[&[u8]]) -> Result<Vec<u8>, Error> {
    #[cfg(feature = "oracle")]
    {
        let (metallib, stats, stale) = write_with_stats(m, refs)?;
        if let Some(dir) = std::env::var_os("AIRCONV_ORACLE_DIR").filter(|d| !d.is_empty()) {
            write_oracle_files(std::path::Path::new(&dir), m, &metallib, &stats, stale);
        }
        Ok(metallib)
    }
    #[cfg(not(feature = "oracle"))]
    {
        let (metallib, _, _) = write_with_stats(m, refs)?;
        Ok(metallib)
    }
}

/// Builds P8's type table for `m`.
///
/// Returns recorded functions and globals whose lowered type still matches their value type in `m`,
/// identified struct bodies, and the count of stale recorded names.
fn build_table(
    m: &Module<'_>,
) -> Result<(Vec<sys::AirValueType>, Vec<sys::AirStructBody>, u32), Error> {
    let cx = m.cx();
    let tcx = cx.tcx();
    let mut values = Vec::new();
    // SAFETY: a live module.
    let mut f = unsafe { sys::get_first_function(m.raw()) };
    while !f.is_null() {
        let name = value_name(f);
        if let Some(ty) = cx.recorded(&name) {
            // SAFETY: `f` is a function of `m`.
            let want = unsafe { sys::global_get_value_type(f) };
            if tcx.lower(ty) == want {
                values.push(sys::AirValueType {
                    value: f,
                    ty: tcx.typed(ty)?,
                });
            }
        }
        // SAFETY: `f` is a function of `m`.
        f = unsafe { sys::get_next_function(f) };
    }
    // SAFETY: a live module.
    let mut g = unsafe { sys::get_first_global(m.raw()) };
    while !g.is_null() {
        let name = value_name(g);
        if let Some(ty) = cx.recorded(&name) {
            // SAFETY: `g` is a global of `m`.
            let want = unsafe { sys::global_get_value_type(g) };
            if tcx.lower(ty) == want {
                values.push(sys::AirValueType {
                    value: g,
                    ty: tcx.typed(ty)?,
                });
            }
        }
        // SAFETY: `g` is a global of `m`.
        g = unsafe { sys::get_next_global(g) };
    }
    let stale = u32::try_from(cx.recorded_len().saturating_sub(values.len())).unwrap_or(u32::MAX);
    let structs = tcx.struct_bodies()?;
    Ok((values, structs, stale))
}

fn write_with_stats(
    m: &Module<'_>,
    refs: &[&[u8]],
) -> Result<(Vec<u8>, sys::AirWriteStats, u32), Error> {
    let (values, structs, stale) = build_table(m)?;
    let ref_bufs: Vec<sys::MemoryBufferRef> = refs
        .iter()
        .map(|r| {
            // SAFETY: `r` holds `r.len()` bytes; `c""` is NUL-terminated; `0` requires no NUL
            // terminator in `r`.
            unsafe {
                sys::create_memory_buffer_with_memory_range(
                    r.as_ptr().cast(),
                    r.len(),
                    c"".as_ptr(),
                    0,
                )
            }
        })
        .collect();
    let types = sys::AirTypes {
        values: values.as_ptr(),
        num_values: values.len(),
        structs: structs.as_ptr(),
        num_structs: structs.len(),
        typed_refs: ref_bufs.as_ptr(),
        num_typed_refs: ref_bufs.len(),
    };
    let mut buffer = ptr::null_mut();
    let mut stats = sys::AirWriteStats {
        inst_casts: 0,
        const_casts: 0,
        use_typed: 0,
        attrs_dropped: 0,
        flags_dropped: 0,
        refs_filled: 0,
    };
    let mut err_msg = ptr::null_mut();
    // SAFETY: `m` is a live module; `types` references live slices and memory buffers; the
    // out-pointers are writable.
    let failed = unsafe {
        sys::write_air_bitcode(
            m.raw(),
            &raw const types,
            &raw mut buffer,
            &raw mut stats,
            &raw mut err_msg,
        )
    } != 0;
    for buf in ref_bufs {
        // SAFETY: each memory buffer was created above and is no longer needed.
        unsafe { sys::dispose_memory_buffer(buf) };
    }
    if failed {
        let msg = if err_msg.is_null() {
            Vec::new()
        } else {
            // SAFETY: `write_air_bitcode` returns a NUL-terminated message on failure.
            let bytes = unsafe { CStr::from_ptr(err_msg) }.to_bytes().to_vec();
            // SAFETY: the message was allocated by `write_air_bitcode`.
            unsafe { sys::dispose_message(err_msg) };
            bytes
        };
        return Err(Error::new(&[&b"AIR writer: "[..], &msg].concat()));
    }
    // SAFETY: the buffer just made.
    let start = unsafe { sys::get_buffer_start(buffer) };
    // SAFETY: as above.
    let size = unsafe { sys::get_buffer_size(buffer) };
    // SAFETY: the buffer holds `size` bytes at `start` until it is disposed of.
    let bitcode = unsafe { slice::from_raw_parts(start.cast::<u8>(), size) }.to_vec();
    // SAFETY: the buffer, disposed of once.
    unsafe { sys::dispose_memory_buffer(buffer) };
    let metallib = metallib::pack(&bitcode, &function_entries(m));
    Ok((metallib, stats, stale))
}

#[cfg(feature = "oracle")]
fn write_oracle_files(
    dir: &std::path::Path,
    m: &Module<'_>,
    metallib: &[u8],
    stats: &sys::AirWriteStats,
    stale: u32,
) {
    use core::fmt::Write as _;
    let digest = airconv_ir::sha256::digest(metallib);
    let mut sha = String::with_capacity(64);
    for byte in digest {
        let _ = write!(sha, "{byte:02x}");
    }
    let line = format!(
        "stats inst-casts={} const-casts={} use-typed={} attrs-dropped={} flags-dropped={} refs={} stale={}\n",
        stats.inst_casts,
        stats.const_casts,
        stats.use_typed,
        stats.attrs_dropped,
        stats.flags_dropped,
        stats.refs_filled,
        stale,
    );
    let _ = std::fs::write(dir.join(format!("{sha}.log")), line);
    let _ = std::fs::write(dir.join(format!("{sha}.post.ll")), m.print());
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

#[cfg(test)]
mod tests;
