//! The type oracle: every value's type checked as it is made, and pointee types derived after.
//!
//! Two checks, built only with the feature `oracle` (the tests and the lane tools, never meson):
//!
//! - [`check()`], at each value the builder makes: the `Ty` attached to it lowers to the value's
//!   own LLVM type, or the compile fails with `oracle: <op> made <llvm type>, Ty says <ty>`. The
//!   builder keeps the first such text as its context's first error, so every check of the
//!   context answers it ([`Context::take_error`] as `LLVM error: oracle: …`, and [`derive()`]).
//! - [`derive()`], over a whole module after its passes: every pointer's pointee derived from facts
//!   opaque pointers keep, each fact compared with LLVM 15's typed pointee (a difference fails the
//!   compile), and each pointer no fact reaches printed as `oracle-untyped <function> <value>`.
//!   That list is what a writer of LLVM 23's opaque IR would have to infer.

use core::{
    ffi::{CStr, c_char},
    iter, slice,
};
use std::io::{self, Write as _};

use rustc_hash::FxHashMap;

use crate::{
    api::Error,
    llvm::{Context, Module, Val, sys},
};

/// The failure text of a value whose `Ty` does not lower to its LLVM type; `None` when it does.
///
/// `v` is what the builder of `cx` answered for `op`, with the type it attached.
#[must_use]
pub fn check(cx: &Context, op: &str, v: Val<'_>) -> Option<Vec<u8>> {
    let lowered = cx.tcx().lower(v.ty());
    let made = type_of(v.raw());
    (made != lowered).then(|| {
        [
            &b"oracle: "[..],
            op.as_bytes(),
            b" made ",
            &type_text(made),
            b", Ty says ",
            &type_text(lowered),
        ]
        .concat()
    })
}

/// Derives the pointee of every pointer of `m` and prints those no fact reaches.
///
/// The facts are those opaque pointers keep, read with LLVM 15's types: an alloca's type, a
/// global's value type, a GEP's source type (its base points at it) and indices (its result points
/// at the indexed type), a load's and a store's type (their address points at it), the same for
/// `atomicrmw` and `cmpxchg`, a call's function type (its callee points at it), and select and phi,
/// whose result and operands share one pointee, to a fixpoint. A cast follows nothing: under typed
/// pointers it changes the pointee. The values answered for are the globals, the functions, and
/// the arguments and instructions of every function with a body; each untyped one is printed to
/// stderr as `oracle-untyped <function> <instruction>` (an argument as `argument <i> <type>`).
///
/// # Errors
///
/// The context's first error (a creation failure of [`check()`] among them), or a fact that is not
/// LLVM's pointee: `oracle: <value> derived <type>, LLVM says <type>`.
pub fn derive(m: &Module<'_>) -> Result<(), Error> {
    let lines = untyped(m)?;
    let mut stderr = io::stderr().lock();
    for line in lines {
        let _ = stderr.write_all(&[&line[..], b"\n"].concat());
    }
    Ok(())
}

/// [`derive()`]'s lines, `oracle-untyped <function> <value>`, in the module's order.
fn untyped(m: &Module<'_>) -> Result<Vec<Vec<u8>>, Error> {
    if let Some(error) = m.cx().take_error() {
        return Err(error);
    }
    let mut d = Derivation::default();
    // The pointers answered for: (function, argument index, value).
    let mut walked: Vec<(sys::ValueRef, Option<usize>, sys::ValueRef)> = Vec::new();
    // Each select's and phi's result with its operands.
    let mut joins: Vec<Vec<sys::ValueRef>> = Vec::new();
    // SAFETY: the module is live.
    let first = unsafe { sys::get_first_global(m.raw()) };
    for global in chain(first, sys::get_next_global) {
        // SAFETY: a global of the module.
        d.fact(global, unsafe { sys::global_get_value_type(global) })?;
    }
    // SAFETY: the module is live.
    let first = unsafe { sys::get_first_function(m.raw()) };
    for function in chain(first, sys::get_next_function) {
        // SAFETY: a function of the module.
        d.fact(function, unsafe { sys::global_get_value_type(function) })?;
        // SAFETY: as above.
        if unsafe { sys::is_declaration(function) } != 0 {
            continue;
        }
        // SAFETY: a function with a body.
        let first = unsafe { sys::get_first_param(function) };
        for (i, arg) in chain(first, sys::get_next_param).enumerate() {
            if pointee(arg).is_some() {
                walked.push((function, Some(i), arg));
            }
        }
        for inst in instructions(function) {
            if pointee(inst).is_some() {
                walked.push((function, None, inst));
            }
            d.facts_of(inst, &mut joins)?;
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for join in &joins {
            let Some(ty) = join.iter().find_map(|v| d.pointee.get(v).copied()) else {
                continue;
            };
            for &v in join {
                changed |= d.fact(v, ty)?;
            }
        }
    }
    let lines = walked
        .into_iter()
        .filter(|(_, _, v)| !d.pointee.contains_key(v))
        .map(|(function, arg, v)| {
            let what = arg.map_or_else(
                || value_text(v),
                |i| [format!("argument {i} ").as_bytes(), &type_text(type_of(v))].concat(),
            );
            [&b"oracle-untyped "[..], &name(function), b" ", &what].concat()
        })
        .collect();
    Ok(lines)
}

/// The pointees derived so far, each one LLVM's.
#[derive(Default)]
struct Derivation {
    pointee: FxHashMap<sys::ValueRef, sys::TypeRef>,
}

impl Derivation {
    /// Records that `v` points at `ty`; answers whether `v` had no pointee yet.
    ///
    /// Anything but a typed pointer is left alone.
    fn fact(&mut self, v: sys::ValueRef, ty: sys::TypeRef) -> Result<bool, Error> {
        let Some(truth) = pointee(v) else {
            return Ok(false);
        };
        if ty != truth {
            return Err(Error::new(
                &[
                    &b"oracle: "[..],
                    &value_text(v),
                    b" derived ",
                    &type_text(ty),
                    b", LLVM says ",
                    &type_text(truth),
                ]
                .concat(),
            ));
        }
        Ok(self.pointee.insert(v, ty).is_none())
    }

    /// The facts the instruction `inst` states; a select or a phi of pointers joins `joins`.
    fn facts_of(
        &mut self,
        inst: sys::ValueRef,
        joins: &mut Vec<Vec<sys::ValueRef>>,
    ) -> Result<(), Error> {
        // SAFETY: a live instruction (as every call below on it).
        match unsafe { sys::get_instruction_opcode(inst) } {
            sys::ALLOCA => {
                // SAFETY: an alloca.
                self.fact(inst, unsafe { sys::get_allocated_type(inst) })?;
            }
            sys::GET_ELEMENT_PTR => {
                // SAFETY: a GEP.
                let source = unsafe { sys::get_gep_source_element_type(inst) };
                self.fact(operand(inst, 0), source)?;
                if let Some(ty) = indexed(inst, source) {
                    self.fact(inst, ty)?;
                }
            }
            sys::LOAD => {
                self.fact(operand(inst, 0), type_of(inst))?;
            }
            sys::STORE => {
                self.fact(operand(inst, 1), type_of(operand(inst, 0)))?;
            }
            sys::ATOMIC_RMW | sys::ATOMIC_CMP_XCHG => {
                self.fact(operand(inst, 0), type_of(operand(inst, 1)))?;
            }
            sys::CALL => {
                // SAFETY: a call.
                let callee = unsafe { sys::get_called_value(inst) };
                // SAFETY: as above.
                self.fact(callee, unsafe { sys::get_called_function_type(inst) })?;
            }
            sys::SELECT if pointee(inst).is_some() => {
                joins.push(vec![inst, operand(inst, 1), operand(inst, 2)]);
            }
            sys::PHI if pointee(inst).is_some() => {
                // SAFETY: a phi.
                let n = unsafe { sys::count_incoming(inst) };
                // SAFETY: a phi with `n` incoming values.
                let incoming = (0..n).map(|i| unsafe { sys::get_incoming_value(inst, i) });
                joins.push(iter::once(inst).chain(incoming).collect());
            }
            _ => {}
        }
        Ok(())
    }
}

/// The type a GEP over `source` points at (`getIndexedType`).
///
/// The first index steps over the base, each other one into an aggregate (a struct's by its
/// constant). `None` past a struct with no such field.
fn indexed(gep: sys::ValueRef, source: sys::TypeRef) -> Option<sys::TypeRef> {
    // SAFETY: a live GEP.
    let operands = u32::try_from(unsafe { sys::get_num_operands(gep) }).ok()?;
    let mut ty = source;
    for i in 2..operands {
        // SAFETY: a live type (the source and each type indexed from it).
        ty = match unsafe { sys::get_type_kind(ty) } {
            sys::STRUCT_TYPE_KIND => {
                let index = operand(gep, i);
                // SAFETY: a live value.
                if unsafe { sys::is_a_constant_int(index) }.is_null() {
                    return None;
                }
                // SAFETY: a constant integer.
                let field = u32::try_from(unsafe { sys::const_int_get_z_ext_value(index) }).ok()?;
                // SAFETY: a struct type.
                if field >= unsafe { sys::count_struct_element_types(ty) } {
                    return None;
                }
                // SAFETY: a struct type with more than `field` fields.
                unsafe { sys::struct_get_type_at_index(ty, field) }
            }
            // SAFETY: an array or a vector.
            sys::ARRAY_TYPE_KIND | sys::VECTOR_TYPE_KIND => unsafe { sys::get_element_type(ty) },
            _ => return None,
        };
    }
    Some(ty)
}

/// LLVM's pointee of `v`; `None` when `v` is not a typed pointer.
fn pointee(v: sys::ValueRef) -> Option<sys::TypeRef> {
    let ty = type_of(v);
    // SAFETY: a live type.
    if unsafe { sys::get_type_kind(ty) } != sys::POINTER_TYPE_KIND {
        return None;
    }
    // SAFETY: a pointer type.
    if unsafe { sys::pointer_type_is_opaque(ty) } != 0 {
        return None;
    }
    // SAFETY: a typed pointer.
    Some(unsafe { sys::get_element_type(ty) })
}

fn type_of(v: sys::ValueRef) -> sys::TypeRef {
    // SAFETY: a live value (each caller's, of the module being read).
    unsafe { sys::type_of(v) }
}

fn operand(user: sys::ValueRef, i: u32) -> sys::ValueRef {
    // SAFETY: a live user with more than `i` operands (each caller's opcode has them).
    unsafe { sys::get_operand(user, i) }
}

/// The values from `first` along `next` (a C API list), until null.
fn chain(
    first: sys::ValueRef,
    next: unsafe extern "C" fn(sys::ValueRef) -> sys::ValueRef,
) -> impl Iterator<Item = sys::ValueRef> {
    iter::successors((!first.is_null()).then_some(first), move |&v| {
        // SAFETY: a live element of the list `next` walks.
        let n = unsafe { next(v) };
        (!n.is_null()).then_some(n)
    })
}

/// Every instruction of the function `f`, block by block.
fn instructions(f: sys::ValueRef) -> impl Iterator<Item = sys::ValueRef> {
    // SAFETY: a live function with a body.
    let first = unsafe { sys::get_first_basic_block(f) };
    iter::successors((!first.is_null()).then_some(first), |&b| {
        // SAFETY: a live block of the function.
        let n = unsafe { sys::get_next_basic_block(b) };
        (!n.is_null()).then_some(n)
    })
    .flat_map(|b| {
        // SAFETY: a live block.
        let first = unsafe { sys::get_first_instruction(b) };
        chain(first, sys::get_next_instruction)
    })
}

/// The value's name, any bytes.
fn name(v: sys::ValueRef) -> Vec<u8> {
    let mut len = 0;
    // SAFETY: a live value; `len` is writable.
    let name = unsafe { sys::get_value_name2(v, &raw mut len) };
    if name.is_null() {
        return Vec::new();
    }
    // SAFETY: LLVM answers `len` bytes at `name`, alive while the value keeps its name.
    unsafe { slice::from_raw_parts(name.cast::<u8>(), len) }.to_vec()
}

/// The value's text (`Value::print`), without the indentation of an instruction.
fn value_text(v: sys::ValueRef) -> Vec<u8> {
    // SAFETY: a live value.
    let text = unsafe { sys::print_value_to_string(v) };
    // SAFETY: the message just made.
    unsafe { message(text) }.trim_ascii().to_vec()
}

/// The type's text, `?` for none.
fn type_text(ty: sys::TypeRef) -> Vec<u8> {
    if ty.is_null() {
        return b"?".to_vec();
    }
    // SAFETY: a live type.
    let text = unsafe { sys::print_type_to_string(ty) };
    // SAFETY: the message just made.
    unsafe { message(text) }
}

/// The bytes of a message the C API made, disposed of here.
///
/// # Safety
///
/// `text` is a NUL-terminated message the C API allocated, used by nothing else.
unsafe fn message(text: *mut c_char) -> Vec<u8> {
    // SAFETY: a NUL-terminated message (the caller's contract).
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
    // SAFETY: the C API's message, freed once.
    unsafe { sys::dispose_message(text) };
    bytes
}

#[cfg(test)]
mod tests;
