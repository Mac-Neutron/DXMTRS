//! DXIL's metadata: the entry point, its signatures and resources, and the `dx.op` calls.
//!
//! Ported from `A/dxil/dxil_metadata.cpp` ([`read_entry`], [`op_code`], `ModuleCallsOp` as
//! [`called_ops`]) and the two handle readers of `A/dxil/dxil_lower.cpp` that Initialize's usage
//! scan needs ([`resolve_handle`], [`heap_handle`]). Where the C++ read a node's operand without
//! checking that it exists, or took an operand for an integer without checking
//! (`mdconst::extract`, which crashes on anything else), the read answers [`MALFORMED`]; where it
//! checked, it skips as the C++ did. Every value here is one of the parsed module's, read through
//! the C API's walkers.

use core::{ffi::CStr, ptr, slice};
use std::collections::BTreeSet;

use crate::{
    api::Error,
    dxil::types::{
        EntryInfo, MALFORMED_WAVE_SIZE, Resource, ResourceFlags, SignatureElement, low32, op,
        resource_class, resource_kind, shader_kind,
    },
    llvm::{Module, sys},
};

/// A metadata node the C++ read past.
///
/// It lacks an operand the C++ read, or holds an operand that is not an integer where the C++
/// took one.
pub const MALFORMED: &[u8] = b"DXIL: malformed metadata";

/// A handle's resource (`HandleInfo`): its class, range ID, register and declaration.
///
/// `index` is the absolute register, as airconv's binding maps take it (null when the call
/// lacks that argument); `resource` the declaration's place in the entry's resources.
pub struct HandleInfo {
    pub cls: u32,
    pub range: u32,
    pub index: sys::ValueRef,
    pub resource: usize,
}

/// `ReadEntry`: the shader model and the first entry point.
///
/// The entry point's function, signatures, resources and properties.
///
/// # Errors
///
/// `DXIL: <what> missing` where the C++ said so, [`MALFORMED`] where it read past a node.
pub fn read_entry(m: &Module<'_>) -> Result<EntryInfo, Error> {
    let mut entry = EntryInfo::default();
    let Some(&model) = named(m, c"dx.shaderModel").first() else {
        return Err(missing(b"dx.shaderModel"));
    };
    let model = operands(model).ok_or_else(malformed)?;
    entry.kind = match string(at(&model, 0)?).as_slice() {
        b"ps" => shader_kind::PIXEL,
        b"vs" => shader_kind::VERTEX,
        b"gs" => shader_kind::GEOMETRY,
        b"hs" => shader_kind::HULL,
        b"ds" => shader_kind::DOMAIN,
        b"cs" => shader_kind::COMPUTE,
        b"lib" => shader_kind::LIBRARY,
        _ => shader_kind::INVALID,
    };
    entry.sm_major = low32(int(at(&model, 1)?)?);
    entry.sm_minor = low32(int(at(&model, 2)?)?);

    let Some(&point) = named(m, c"dx.entryPoints").first() else {
        return Err(missing(b"dx.entryPoints"));
    };
    // !{void ()* @main, !"main", !sigs, !resources, !props}
    let point = operands(point).ok_or_else(malformed)?;
    let [function, _, sigs, res, props, ..] = point[..] else {
        return Err(missing(b"entry point fields"));
    };
    // SAFETY: null or a value of the module (a metadata operand).
    if function.is_null() || unsafe { sys::is_a_function(function) }.is_null() {
        return Err(missing(b"entry function"));
    }
    entry.name = value_name(function);
    if let Some(sigs) = operands(sigs) {
        entry.inputs = read_signature(operands(at(&sigs, 0)?))?;
        entry.outputs = read_signature(operands(at(&sigs, 1)?))?;
    }
    if let Some(res) = operands(res) {
        for (i, cls) in [
            resource_class::SRV,
            resource_class::UAV,
            resource_class::CBUFFER,
            resource_class::SAMPLER,
        ]
        .into_iter()
        .enumerate()
        {
            read_resources(operands(at(&res, i)?), cls, &mut entry.resources)?;
        }
    }
    if let Some(props) = operands(props) {
        read_properties(&props, &mut entry)?;
    }
    Ok(entry)
}

/// `OpCode`: a `dx.op` call's opcode (its first argument), `u32::MAX` for any other call.
///
/// The callee is the function called directly with its own type (`getCalledFunction`).
#[must_use]
pub fn op_code(call: sys::ValueRef) -> u32 {
    // SAFETY: a call of the module.
    let callee = unsafe { sys::get_called_value(call) };
    // SAFETY: null or a value of the module.
    if callee.is_null() || unsafe { sys::is_a_function(callee) }.is_null() {
        return u32::MAX;
    }
    // SAFETY: a function of the module and the call.
    let same_type = unsafe { sys::global_get_value_type(callee) }
        == unsafe { sys::get_called_function_type(call) };
    if !same_type || !value_name(callee).starts_with(b"dx.op.") || arg_count(call) == 0 {
        return u32::MAX;
    }
    const_u32(arg(call, 0), u32::MAX)
}

/// The module's `dx.op.*` functions, in its order.
#[must_use]
pub fn dx_op_functions(m: &Module<'_>) -> Vec<sys::ValueRef> {
    // SAFETY: a live module.
    let first = unsafe { sys::get_first_function(m.raw()) };
    // SAFETY: a function of the module.
    walk(first, |f| unsafe { sys::get_next_function(f) })
        .filter(|&f| value_name(f).starts_with(b"dx.op."))
        .collect()
}

/// The calls among `value`'s users, in its use list's order (`users()`), once per use.
#[must_use]
pub fn call_users(value: sys::ValueRef) -> Vec<sys::ValueRef> {
    // SAFETY: a value of the module.
    let first = unsafe { sys::get_first_use(value) };
    let mut calls = Vec::new();
    let mut next = first;
    while !next.is_null() {
        // SAFETY: a use of the module.
        let user = unsafe { sys::get_user(next) };
        if is_call(user) {
            calls.push(user);
        }
        // SAFETY: as above.
        next = unsafe { sys::get_next_use(next) };
    }
    calls
}

/// Every opcode the module's `dx.op` calls pass (`ModuleCallsOp` for any opcode at once).
#[must_use]
pub fn called_ops(m: &Module<'_>) -> BTreeSet<u32> {
    dx_op_functions(m)
        .into_iter()
        .flat_map(call_users)
        .map(op_code)
        .collect()
}

/// `HeapHandle`: an `annotateHandle` of a `createHandleFromHeap`.
///
/// That call and the `ResourceProperties` `{p0, p1}`; `None` for any other handle.
#[must_use]
pub fn heap_handle(annotate: sys::ValueRef) -> Option<(sys::ValueRef, [u32; 2])> {
    if arg_count(annotate) < 3 {
        return None;
    }
    let heap = arg(annotate, 1);
    let props = arg(annotate, 2);
    // SAFETY: a value of the module, not null (a call's argument).
    let constant = unsafe { sys::is_a_constant(props) };
    if !is_call(heap) || op_code(heap) != op::CREATE_HANDLE_FROM_HEAP || constant.is_null() {
        return None;
    }
    let second = aggregate_element(props, 1)?;
    let first = aggregate_element(props, 0).unwrap_or(ptr::null_mut());
    Some((heap, [const_u32(first, 0), const_u32(second, 0)]))
}

/// `ResolveHandle`: a `%dx.types.Handle` to its resource, through `annotateHandle`.
///
/// Only the handle's defining calls are read, so Initialize's usage scan uses it too. A missing
/// argument reads as no value (the C++ read the callee or past the operands); a handle that
/// annotates itself is refused where the C++ recursed forever.
///
/// # Errors
///
/// The C++'s `DXIL: …` refusals.
pub fn resolve_handle(entry: &EntryInfo, handle: sys::ValueRef) -> Result<HandleInfo, Error> {
    let mut seen = Vec::new();
    let mut handle = handle;
    loop {
        if !is_call(handle) {
            return Err(Error::new(b"DXIL: resource handle through phi/select"));
        }
        match op_code(handle) {
            // (216, handle, %dx.types.ResourceProperties)
            op::ANNOTATE_HANDLE => {
                if let Some((heap, props)) = heap_handle(handle) {
                    // (218, i32 index, i1 samplerHeap, i1 nonUniform): the index is the register.
                    let (place, resource) = entry
                        .resources
                        .iter()
                        .enumerate()
                        .find(|(_, r)| {
                            r.flags.contains(ResourceFlags::HEAP) && r.heap_props == props
                        })
                        .ok_or_else(|| {
                            Error::new(b"DXIL: createHandleFromHeap names an undeclared heap range")
                        })?;
                    return Ok(HandleInfo {
                        cls: resource.cls,
                        range: resource.id,
                        index: arg(heap, 1),
                        resource: place,
                    });
                }
                if seen.contains(&handle) {
                    return Err(Error::new(b"DXIL: resource handle annotates itself"));
                }
                seen.push(handle);
                handle = arg(handle, 1);
            }
            // (57, i8 class, i32 rangeId, i32 index, i1 nonUniform)
            op::CREATE_HANDLE => {
                let cls = const_u32(arg(handle, 1), 0);
                let id = const_u32(arg(handle, 2), 0);
                let place = entry
                    .resources
                    .iter()
                    .position(|r| r.cls == cls && r.id == id)
                    .ok_or_else(|| {
                        Error::new(b"DXIL: createHandle names an undeclared resource")
                    })?;
                return Ok(HandleInfo {
                    cls,
                    range: id,
                    index: arg(handle, 3),
                    resource: place,
                });
            }
            // (217, %dx.types.ResBind {i32 LB, i32 UB, i32 space, i8 class}, i32 index, i1)
            op::CREATE_HANDLE_FROM_BINDING => return from_binding(entry, handle),
            op::CREATE_HANDLE_FROM_HEAP => {
                return Err(Error::new(
                    b"DXIL: createHandleFromHeap without annotateHandle",
                ));
            }
            _ => {
                return Err(Error::new(b"DXIL: resource handle from an unrecognised op"));
            }
        }
    }
}

/// `ConstantU32`: a constant integer's low 32 bits, else `otherwise`.
#[must_use]
pub fn const_u32(v: sys::ValueRef, otherwise: u32) -> u32 {
    // SAFETY: null checked first; else a value of the module.
    if v.is_null() || unsafe { sys::is_a_constant_int(v) }.is_null() {
        return otherwise;
    }
    // SAFETY: a constant integer.
    let [b0, b1, b2, b3, ..] = unsafe { sys::const_int_get_z_ext_value(v) }.to_le_bytes();
    u32::from_le_bytes([b0, b1, b2, b3])
}

/// Whether `v` is a call instruction (null is none).
#[must_use]
pub fn is_call(v: sys::ValueRef) -> bool {
    // SAFETY: null checked first; else a value of the module.
    !v.is_null() && !unsafe { sys::is_a_call_inst(v) }.is_null()
}

/// The `i`th argument of a call, null past its arguments.
#[must_use]
pub fn arg(call: sys::ValueRef, i: u32) -> sys::ValueRef {
    if i >= arg_count(call) {
        return ptr::null_mut();
    }
    // SAFETY: a call of the module with more than `i` arguments.
    unsafe { sys::get_operand(call, i) }
}

/// A value's name, any bytes.
#[must_use]
pub fn value_name(v: sys::ValueRef) -> Vec<u8> {
    let mut len = 0;
    // SAFETY: a value of the module; `len` is writable.
    let name = unsafe { sys::get_value_name2(v, &raw mut len) };
    if name.is_null() {
        return Vec::new();
    }
    // SAFETY: LLVM answers `len` bytes at `name`, alive while the value keeps its name.
    unsafe { slice::from_raw_parts(name.cast::<u8>(), len) }.to_vec()
}

/// The values from `first` along `next` until null.
pub fn walk(
    first: sys::ValueRef,
    next: impl Fn(sys::ValueRef) -> sys::ValueRef,
) -> impl Iterator<Item = sys::ValueRef> {
    core::iter::successors((!first.is_null()).then_some(first), move |&v| {
        let n = next(v);
        (!n.is_null()).then_some(n)
    })
}

fn missing(what: &[u8]) -> Error {
    Error::new(&[&b"DXIL: "[..], what, b" missing"].concat())
}

fn malformed() -> Error {
    Error::new(MALFORMED)
}

/// `createHandleFromBinding`'s resource.
///
/// The declared range of its class and space that holds its lower bound.
fn from_binding(entry: &EntryInfo, call: sys::ValueRef) -> Result<HandleInfo, Error> {
    let bind = arg(call, 1);
    // SAFETY: null checked first; else a value of the module.
    if bind.is_null() || unsafe { sys::is_a_constant(bind) }.is_null() {
        return Err(Error::new(b"DXIL: non-constant createHandleFromBinding"));
    }
    let field = |i| const_u32(aggregate_element(bind, i).unwrap_or(ptr::null_mut()), 0);
    let (lower_bound, space, cls) = (field(0), field(2), field(3));
    let (place, resource) = entry
        .resources
        .iter()
        .enumerate()
        .find(|(_, r)| {
            r.cls == cls
                && r.space == space
                && lower_bound >= r.lower_bound
                && (r.size == u32::MAX || lower_bound - r.lower_bound < r.size)
        })
        .ok_or_else(|| Error::new(b"DXIL: createHandleFromBinding names an undeclared resource"))?;
    Ok(HandleInfo {
        cls,
        range: resource.id,
        index: arg(call, 2),
        resource: place,
    })
}

/// `Constant::getAggregateElement`: element `i` of an aggregate or vector constant.
///
/// `None` past its elements or for a constant of another type (where the release C++ read an
/// undef scalar as a structure).
fn aggregate_element(c: sys::ValueRef, i: u32) -> Option<sys::ValueRef> {
    // SAFETY: a constant of the module.
    let ty = unsafe { sys::type_of(c) };
    // SAFETY: its type.
    let kind = unsafe { sys::get_type_kind(ty) };
    if !matches!(
        kind,
        sys::STRUCT_TYPE_KIND
            | sys::ARRAY_TYPE_KIND
            | sys::VECTOR_TYPE_KIND
            | sys::SCALABLE_VECTOR_TYPE_KIND
    ) {
        return None;
    }
    // SAFETY: an aggregate or vector constant.
    let element = unsafe { sys::get_aggregate_element(c, i) };
    (!element.is_null()).then_some(element)
}

fn arg_count(call: sys::ValueRef) -> u32 {
    // SAFETY: a call of the module.
    unsafe { sys::get_num_arg_operands(call) }
}

/// The operands of the named metadata `name` (each a node), none when the module has none.
fn named(m: &Module<'_>, name: &CStr) -> Vec<sys::ValueRef> {
    // SAFETY: a live module; a NUL-terminated name.
    let count = unsafe { sys::get_named_metadata_num_operands(m.raw(), name.as_ptr()) };
    let mut nodes = vec![ptr::null_mut(); count as usize];
    // SAFETY: as above; `nodes` holds one slot per operand.
    unsafe { sys::get_named_metadata_operands(m.raw(), name.as_ptr(), nodes.as_mut_ptr()) };
    nodes
}

/// The operands of the metadata node `node`, `None` for anything else.
///
/// The C++'s `dyn_cast_or_null<MDNode>`.
///
/// A constant operand comes back as the constant, other metadata as a metadata value, an empty
/// slot as null (`LLVMGetMDNodeOperands`).
fn operands(node: sys::ValueRef) -> Option<Vec<sys::ValueRef>> {
    // SAFETY: null checked first; else a value of the module (a metadata walk's answer).
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

/// Operand `i`, [`MALFORMED`] past the node's operands.
fn at(operands: &[sys::ValueRef], i: usize) -> Result<sys::ValueRef, Error> {
    operands.get(i).copied().ok_or_else(malformed)
}

/// The metadata string's bytes, empty for anything else (the C++'s `String`).
fn string(v: sys::ValueRef) -> Vec<u8> {
    if v.is_null() {
        return Vec::new();
    }
    let mut len = 0;
    // SAFETY: a value of the module; `len` is writable (null for a value that is no string).
    let bytes = unsafe { sys::get_md_string(v, &raw mut len) };
    if bytes.is_null() {
        return Vec::new();
    }
    // SAFETY: an `MDString` answers its `len` bytes, alive as long as its context.
    unsafe { slice::from_raw_parts(bytes.cast::<u8>(), len as usize) }.to_vec()
}

/// An integer operand of any width, sign-extended (the C++'s `Int`).
///
/// # Errors
///
/// [`MALFORMED`] for anything but a constant integer.
fn int(v: sys::ValueRef) -> Result<i64, Error> {
    // SAFETY: null checked first; else a value of the module.
    if v.is_null() || unsafe { sys::is_a_constant_int(v) }.is_null() {
        return Err(malformed());
    }
    // SAFETY: a constant integer.
    Ok(unsafe { sys::const_int_get_s_ext_value(v) })
}

/// `ReadSignature`: a signature's elements.
///
/// `!{i32 ID, !"Name", i8 CompType, i8 SemanticKind, !{i32 SemIdx...}, i8 Interp, i32 Rows, i8
/// Cols, i32 StartRow, i8 StartCol, ...}` per element; an element of fewer than 10 operands is
/// skipped.
fn read_signature(list: Option<Vec<sys::ValueRef>>) -> Result<Vec<SignatureElement>, Error> {
    let mut elements = Vec::new();
    for op in list.unwrap_or_default() {
        let Some(e) = operands(op) else { continue };
        let [
            id,
            name,
            ty,
            kind,
            indices,
            interpolation,
            rows,
            cols,
            start_row,
            start_col,
            ..,
        ] = e[..]
        else {
            continue;
        };
        let mut semantic_indices = Vec::new();
        for i in operands(indices).unwrap_or_default() {
            semantic_indices.push(low32(int(i)?));
        }
        let s = SignatureElement {
            id: low32(int(id)?),
            name: string(name),
            ty: low32(int(ty)?),
            kind: low32(int(kind)?),
            semantic_indices,
            interpolation: low32(int(interpolation)?),
            rows: low32(int(rows)?),
            cols: low32(int(cols)?),
            start_row: low32(int(start_row)?).cast_signed(),
            start_col: low32(int(start_col)?).cast_signed(),
        };
        elements.push(pad_indices(s)?);
    }
    Ok(elements)
}

/// The semantic indices padded with 0 to the element's rows.
///
/// # Errors
///
/// [`MALFORMED`] when the rows do not fit in memory (the C++'s `resize` aborted).
fn pad_indices(mut s: SignatureElement) -> Result<SignatureElement, Error> {
    let rows = usize::try_from(s.rows).map_err(|_| malformed())?;
    if s.semantic_indices.len() < rows {
        s.semantic_indices
            .try_reserve_exact(rows - s.semantic_indices.len())
            .map_err(|_| malformed())?;
        s.semantic_indices.resize(rows, 0);
    }
    Ok(s)
}

/// `ReadResources`: one class's list; a record of fewer than 7 operands is skipped.
fn read_resources(
    list: Option<Vec<sys::ValueRef>>,
    cls: u32,
    out: &mut Vec<Resource>,
) -> Result<(), Error> {
    for op in list.unwrap_or_default() {
        let Some(m) = operands(op) else { continue };
        if m.len() < 7 {
            continue;
        }
        let mut r = Resource {
            id: low32(int(at(&m, 0)?)?),
            cls,
            kind: 0,
            space: low32(int(at(&m, 3)?)?),
            lower_bound: low32(int(at(&m, 4)?)?),
            // -1 (unbounded) becomes `u32::MAX`.
            size: low32(int(at(&m, 5)?)?),
            element_type: 0,
            stride: 0,
            cbuffer_size: 0,
            flags: ResourceFlags::NONE,
            heap_props: [0; 2],
        };
        match cls {
            // ..., i32 kind, i32 sampleCount, !extra
            resource_class::SRV => {
                r.kind = low32(int(at(&m, 6)?)?);
                if m.len() > 8 {
                    read_extra(operands(at(&m, 8)?), &mut r)?;
                }
            }
            // ..., i32 kind, i1 globallyCoherent, i1 hasCounter, i1 ROV, !extra
            resource_class::UAV => {
                r.kind = low32(int(at(&m, 6)?)?);
                for (i, flag) in [
                    (7, ResourceFlags::GLOBALLY_COHERENT),
                    (8, ResourceFlags::HAS_COUNTER),
                    (9, ResourceFlags::RASTERIZER_ORDERED),
                ] {
                    r.flags.set(flag, int(at(&m, i)?)? != 0);
                }
                if m.len() > 10 {
                    read_extra(operands(at(&m, 10)?), &mut r)?;
                }
            }
            // ..., i32 sizeInBytes, !extra
            resource_class::CBUFFER => {
                r.kind = resource_kind::CBUFFER;
                r.cbuffer_size = low32(int(at(&m, 6)?)?);
            }
            // ..., i32 samplerType (1 = comparison), !extra
            _ => {
                r.kind = resource_kind::SAMPLER;
                r.flags
                    .set(ResourceFlags::COMPARISON_SAMPLER, int(at(&m, 6)?)? == 1);
            }
        }
        out.push(r);
    }
    Ok(())
}

/// `ReadResourceExtra`: tag/value pairs, tag 0 the element type, tag 1 the structured stride.
fn read_extra(extra: Option<Vec<sys::ValueRef>>, r: &mut Resource) -> Result<(), Error> {
    for &[tag, value] in extra.unwrap_or_default().as_chunks::<2>().0 {
        match int(tag)? {
            0 => r.element_type = low32(int(value)?),
            1 => r.stride = low32(int(value)?),
            _ => {}
        }
    }
    Ok(())
}

/// The entry's properties, tag/value pairs.
///
/// `NumThreads` (4), `GSState` (1), `WaveSize` (11) and `WaveSizeRange` (23).
fn read_properties(props: &[sys::ValueRef], entry: &mut EntryInfo) -> Result<(), Error> {
    for &[tag, value] in props.as_chunks::<2>().0 {
        let tag = int(tag)?;
        let n = operands(value);
        match (tag, n) {
            (4, Some(n)) => {
                for (slot, &d) in entry.numthreads.iter_mut().zip(&n) {
                    *slot = low32(int(d)?);
                }
            }
            // input primitive, max vertices, streams, topology, instances
            (1, Some(n)) if n.len() >= 5 => {
                entry.gs_input_primitive = low32(int(at(&n, 0)?)?);
                entry.gs_max_vertex_count = low32(int(at(&n, 1)?)?);
                entry.gs_stream_mask = low32(int(at(&n, 2)?)?);
                entry.gs_output_topology = low32(int(at(&n, 3)?)?);
                entry.gs_instances = low32(int(at(&n, 4)?)?);
            }
            // WaveSize: N; WaveSizeRange: min, max (0: min alone), preferred. A node too short is
            // refused by Initialize, not skipped.
            (11 | 23, n) => match (tag, n.as_deref()) {
                (11, Some([size, ..])) => {
                    entry.wave_size_min = low32(int(*size)?);
                    entry.wave_size_max = entry.wave_size_min;
                }
                (23, Some([min, max, ..])) => {
                    entry.wave_size_min = low32(int(*min)?);
                    let max = int(*max)?;
                    entry.wave_size_max = if max == 0 {
                        entry.wave_size_min
                    } else {
                        low32(max)
                    };
                }
                _ => {
                    entry.wave_size_min = MALFORMED_WAVE_SIZE;
                    entry.wave_size_max = 0;
                }
            },
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
