//! LLVM 15 through its C API: the declarations, the handles and one `Ty` for every type.
//!
//! A [`Context`] is one compile's LLVM context, with typed pointers (the AIR Metal reads) and a
//! diagnostic handler, so nothing LLVM reports ends the process: an error is kept for
//! [`Context::take_error`], which every parse, link and verify checks, a warning or a note is
//! written to stderr as LLVM would have written it, a remark is dropped (LLVM filters remarks
//! when it prints itself). The handles ([`Module`], [`Func`], [`Block`], [`Val`]) borrow their
//! context, so none outlives it; every value carries its [`Ty`]. Each wrapper makes one C call.

pub mod builder;
pub mod sys;
pub mod ty;

use core::{
    cell::RefCell,
    ffi::{CStr, c_char, c_void},
    marker::PhantomData,
    mem::ManuallyDrop,
    slice,
};
use std::{
    ffi::CString,
    io::{self, Write as _},
};

pub use builder::{Builder, FastMath, FastMathGuard};
pub use ty::{Ty, TyCx, TyKind};

use crate::api::Error;

/// A NUL-terminated copy of a name the C API takes as a C string.
///
/// A name holding a NUL is P15's error; the length-taking forms ([`Val::set_name`],
/// [`Context::md_string`]) need no copy.
///
/// # Errors
///
/// `Invalid DXBC bytecode: a name holds a NUL`.
pub fn name(bytes: &[u8]) -> Result<CString, Error> {
    CString::new(bytes).map_err(|_| Error::new(b"Invalid DXBC bytecode: a name holds a NUL"))
}

/// The bytes of a message LLVM made (`char *` it hands over), disposed of here; empty for null.
///
/// # Safety
///
/// `message` is null or a NUL-terminated message the C API allocated, used by nothing else.
unsafe fn take_message(message: *mut c_char) -> Vec<u8> {
    if message.is_null() {
        return Vec::new();
    }
    // SAFETY: a NUL-terminated message (the caller's contract).
    let bytes = unsafe { CStr::from_ptr(message) }.to_bytes().to_vec();
    // SAFETY: the C API's message, freed once.
    unsafe { sys::dispose_message(message) };
    bytes
}

/// What the diagnostic handler keeps: the first error.
#[derive(Default)]
struct Diagnostics {
    error: RefCell<Option<Vec<u8>>>,
    /// The lines written to stderr, for the tests.
    #[cfg(test)]
    printed: RefCell<Vec<Vec<u8>>>,
}

/// The context's diagnostic handler (P5).
///
/// It records the first error, writes warnings and notes to stderr with LLVM's prefix and drops
/// remarks. It never unwinds: a panic here aborts.
extern "C" fn diagnose(info: sys::DiagnosticInfoRef, state: *mut c_void) {
    // SAFETY: LLVM passes the diagnostic it is reporting.
    let severity = unsafe { sys::get_diag_info_severity(info) };
    let prefix: &[u8] = match severity {
        sys::DS_ERROR => b"error",
        sys::DS_WARNING => b"warning",
        sys::DS_NOTE => b"note",
        _ => return,
    };
    // SAFETY: as above; the description is a message the caller disposes.
    let message = unsafe { sys::get_diag_info_description(info) };
    // SAFETY: the description just made, disposed of once.
    let text = unsafe { take_message(message) };
    // SAFETY: `state` is the `Diagnostics` the context's owner made for this handler, alive as
    // long as the context.
    let diagnostics = unsafe { &*state.cast::<Diagnostics>() };
    if severity == sys::DS_ERROR {
        diagnostics.error.borrow_mut().get_or_insert(text);
        return;
    }
    let line = [prefix, b": ", &text, b"\n"].concat();
    let _ = io::stderr().lock().write_all(&line);
    #[cfg(test)]
    diagnostics.printed.borrow_mut().push(line);
}

/// One compile's LLVM context, with its types ([`TyCx`]).
pub struct Context {
    raw: sys::ContextRef,
    tcx: TyCx,
    /// The handler's state: a box made in [`Context::new`], freed after the context in `drop`.
    diagnostics: *mut Diagnostics,
}

impl Context {
    /// A context with typed pointers and the diagnostic handler installed.
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: creating a context has no precondition.
        let raw = unsafe { sys::context_create() };
        // SAFETY: `raw` is the live context just made.
        unsafe { sys::context_set_opaque_pointers(raw, 0) };
        let diagnostics = Box::into_raw(Box::<Diagnostics>::default());
        // SAFETY: `raw` is live; `diagnostics` stays allocated until after the context is
        // disposed of (`drop`), and the handler only reads through it.
        unsafe { sys::context_set_diagnostic_handler(raw, Some(diagnose), diagnostics.cast()) };
        Self {
            raw,
            tcx: TyCx::new(raw),
            diagnostics,
        }
    }

    /// The `LLVMContextRef`.
    #[must_use]
    pub const fn raw(&self) -> sys::ContextRef {
        self.raw
    }

    /// The context's types.
    #[must_use]
    pub const fn tcx(&self) -> &TyCx {
        &self.tcx
    }

    fn diagnostics(&self) -> &Diagnostics {
        // SAFETY: the box made in `new`, freed only in `drop`.
        unsafe { &*self.diagnostics }
    }

    /// The first error LLVM reported since the last call, as P15's `LLVM error: <description>`.
    #[must_use]
    pub fn take_error(&self) -> Option<Error> {
        let text = self.diagnostics().error.borrow_mut().take()?;
        Some(Error::new(&[&b"LLVM error: "[..], &text].concat()))
    }

    /// The lines the handler wrote to stderr so far.
    #[cfg(test)]
    pub fn printed(&self) -> Vec<Vec<u8>> {
        self.diagnostics().printed.borrow().clone()
    }

    /// The value `raw` of type `ty`, borrowing this context.
    ///
    /// # Safety
    ///
    /// `raw` is a live value of this context and `ty` (of its table) is its type.
    #[must_use]
    pub const unsafe fn val(&self, raw: sys::ValueRef, ty: Ty) -> Val<'_> {
        // SAFETY: the caller's contract.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant `value` of the integer type `ty` (`ConstantInt::get`).
    ///
    /// Its low bits are taken; `sign_extend` widens it by its sign past 64 bits.
    #[must_use]
    pub fn const_int(&self, ty: Ty, value: u64, sign_extend: bool) -> Val<'_> {
        // SAFETY: `ty` lowers to an integer type of this context.
        let raw = unsafe { sys::const_int(self.tcx.lower(ty), value, sign_extend.into()) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant `value` of the floating-point type `ty`, rounded to it (`ConstantFP::get`).
    #[must_use]
    pub fn const_real(&self, ty: Ty, value: f64) -> Val<'_> {
        // SAFETY: `ty` lowers to a floating-point type of this context.
        let raw = unsafe { sys::const_real(self.tcx.lower(ty), value) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The zero of `ty` (`Constant::getNullValue`).
    #[must_use]
    pub fn null(&self, ty: Ty) -> Val<'_> {
        // SAFETY: `ty` lowers to a type of this context.
        let raw = unsafe { sys::const_null(self.tcx.lower(ty)) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// Every bit set, of the integer or integer vector `ty` (`Constant::getAllOnesValue`).
    #[must_use]
    pub fn all_ones(&self, ty: Ty) -> Val<'_> {
        // SAFETY: `ty` lowers to an integer type (or a vector of them) of this context.
        let raw = unsafe { sys::const_all_ones(self.tcx.lower(ty)) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// `undef` of `ty`.
    #[must_use]
    pub fn undef(&self, ty: Ty) -> Val<'_> {
        // SAFETY: `ty` lowers to a type of this context.
        let raw = unsafe { sys::get_undef(self.tcx.lower(ty)) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// `poison` of `ty`.
    #[must_use]
    pub fn poison(&self, ty: Ty) -> Val<'_> {
        // SAFETY: `ty` lowers to a type of this context.
        let raw = unsafe { sys::get_poison(self.tcx.lower(ty)) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant vector of `elements`, constants of one type (`ConstantVector::get`).
    #[must_use]
    pub fn const_vector<'cx>(&'cx self, elements: &[Val<'cx>]) -> Val<'cx> {
        let element = elements.first().map_or_else(|| self.tcx.void(), |e| e.ty);
        let ty = self.tcx.vector(element, len_u32(elements.len()));
        let mut raws: Vec<sys::ValueRef> = elements.iter().map(|e| e.raw).collect();
        // SAFETY: constants of this context; `raws` holds its length.
        let raw = unsafe { sys::const_vector(raws.as_mut_ptr(), len_u32(raws.len())) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant array of `elements`, constants of `element` (`ConstantArray::get`).
    #[must_use]
    pub fn const_array<'cx>(&'cx self, element: Ty, elements: &[Val<'cx>]) -> Val<'cx> {
        let ty = self.tcx.array(element, len_u32(elements.len()));
        let mut raws: Vec<sys::ValueRef> = elements.iter().map(|e| e.raw).collect();
        // SAFETY: `element` lowers to a type of this context, `raws` holds constants of it.
        let raw = unsafe {
            sys::const_array(
                self.tcx.lower(element),
                raws.as_mut_ptr(),
                len_u32(raws.len()),
            )
        };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant of the literal struct of `fields`' types (`ConstantStruct::getAnon`).
    #[must_use]
    pub fn const_struct<'cx>(&'cx self, fields: &[Val<'cx>], packed: bool) -> Val<'cx> {
        let types: Vec<Ty> = fields.iter().map(|f| f.ty).collect();
        let ty = self.tcx.literal_struct(&types, packed);
        let mut raws: Vec<sys::ValueRef> = fields.iter().map(|f| f.raw).collect();
        // SAFETY: constants of this context; `raws` holds its length.
        let raw = unsafe {
            sys::const_struct_in_context(
                self.raw,
                raws.as_mut_ptr(),
                len_u32(raws.len()),
                packed.into(),
            )
        };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant of the identified struct `ty` with `fields` (`ConstantStruct::get`).
    #[must_use]
    pub fn const_named_struct<'cx>(&'cx self, ty: Ty, fields: &[Val<'cx>]) -> Val<'cx> {
        let mut raws: Vec<sys::ValueRef> = fields.iter().map(|f| f.raw).collect();
        // SAFETY: `ty` lowers to a struct of this context, `raws` holds constants of its fields.
        let raw = unsafe {
            sys::const_named_struct(self.tcx.lower(ty), raws.as_mut_ptr(), len_u32(raws.len()))
        };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The metadata string of `bytes`, any bytes (`MDString::get`).
    #[must_use]
    pub fn md_string(&self, bytes: &[u8]) -> sys::MetadataRef {
        // SAFETY: the context is live; `bytes` holds its length.
        unsafe { sys::md_string_in_context2(self.raw, bytes.as_ptr().cast(), bytes.len()) }
    }

    /// The metadata tuple of `operands` (`MDTuple::get`); a null operand is an empty slot.
    #[must_use]
    pub fn md_node(&self, operands: &[sys::MetadataRef]) -> sys::MetadataRef {
        let mut raws = operands.to_vec();
        // SAFETY: metadata of this context; `raws` holds its length.
        unsafe { sys::md_node_in_context2(self.raw, raws.as_mut_ptr(), raws.len()) }
    }

    /// The enum attribute `name` (`nounwind`, `readnone`, …) with `value` (`Attribute::get`).
    ///
    /// `None` for a name LLVM does not know.
    #[must_use]
    pub fn enum_attribute(&self, name: &[u8], value: u64) -> Option<sys::AttributeRef> {
        // SAFETY: `name` holds its length.
        let kind =
            unsafe { sys::get_enum_attribute_kind_for_name(name.as_ptr().cast(), name.len()) };
        // SAFETY: the context is live, `kind` an attribute kind LLVM named.
        (kind != 0).then(|| unsafe { sys::create_enum_attribute(self.raw, kind, value) })
    }
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the context is live and owned here; every handle that borrows it is gone.
        unsafe { sys::context_dispose(self.raw) };
        // SAFETY: the box made in `new`; the context that called the handler with it is gone.
        drop(unsafe { Box::from_raw(self.diagnostics) });
    }
}

/// A module of a context, owned until it is dropped or handed over ([`Module::into_raw`]).
pub struct Module<'cx> {
    raw: sys::ModuleRef,
    cx: &'cx Context,
}

impl<'cx> Module<'cx> {
    /// An empty module named `name` (`Module(name, context)`).
    #[must_use]
    pub fn new(cx: &'cx Context, name: &CStr) -> Self {
        // SAFETY: the context is live; `name` is NUL-terminated.
        let raw = unsafe { sys::module_create_with_name_in_context(name.as_ptr(), cx.raw) };
        Self { raw, cx }
    }

    /// Takes the module `raw` (a parsed one).
    ///
    /// # Safety
    ///
    /// `raw` is a live module of `cx` that nothing else owns.
    #[must_use]
    pub const unsafe fn from_raw(cx: &'cx Context, raw: sys::ModuleRef) -> Self {
        Self { raw, cx }
    }

    /// The `LLVMModuleRef`.
    #[must_use]
    pub const fn raw(&self) -> sys::ModuleRef {
        self.raw
    }

    /// The context.
    #[must_use]
    pub const fn cx(&self) -> &'cx Context {
        self.cx
    }

    /// Gives the module up (a link that takes it); the caller disposes of it.
    #[must_use]
    pub fn into_raw(self) -> sys::ModuleRef {
        ManuallyDrop::new(self).raw
    }

    /// The module's text (`Module::print`).
    #[must_use]
    pub fn print(&self) -> Vec<u8> {
        // SAFETY: the module is live; the answer is a message disposed of below.
        let text = unsafe { sys::print_module_to_string(self.raw) };
        // SAFETY: the message just made.
        unsafe { take_message(text) }
    }

    /// A new function `name` of the function type `ty`, external (`Function::Create`).
    ///
    /// # Errors
    ///
    /// P15's error for a name holding a NUL.
    pub fn add_function(&self, name: &[u8], ty: Ty) -> Result<Func<'cx>, Error> {
        let c_name = self::name(name)?;
        let tcx = self.cx.tcx();
        // SAFETY: the module is live, `ty` lowers to a function type of its context.
        let raw = unsafe { sys::add_function(self.raw, c_name.as_ptr(), tcx.lower(ty)) };
        // SAFETY: a function of this context of type `ty`, in address space 0 (AIR's data layout
        // names no program address space).
        Ok(unsafe { Func::new(raw, ty, tcx.ptr(ty, 0)) })
    }

    /// `Module::getOrInsertFunction`: the global named `name`, called as the function type `ty`.
    ///
    /// When there is none, a new external declaration with `attrs` (`(index, attribute)`, not
    /// given to an `llvm.` intrinsic, which has its own); a bitcast of it when it has
    /// another type.
    ///
    /// # Errors
    ///
    /// P15's error for a name holding a NUL.
    pub fn get_or_insert_function(
        &self,
        name: &[u8],
        ty: Ty,
        attrs: &[(sys::AttributeIndex, sys::AttributeRef)],
    ) -> Result<Func<'cx>, Error> {
        let c_name = self::name(name)?;
        let tcx = self.cx.tcx();
        // SAFETY: the module is live; `c_name` is NUL-terminated.
        let mut found = unsafe { sys::get_named_function(self.raw, c_name.as_ptr()) };
        if found.is_null() {
            // SAFETY: as above.
            found = unsafe { sys::get_named_global(self.raw, c_name.as_ptr()) };
        }
        if found.is_null() {
            let func = self.add_function(name, ty)?;
            if !name.starts_with(b"llvm.") {
                for &(index, attr) in attrs {
                    // SAFETY: a function just made; `attr` an attribute of its context.
                    unsafe { sys::add_attribute_at_index(func.val.raw, index, attr) };
                }
            }
            return Ok(func);
        }
        // SAFETY: `found` is a global of this module.
        let found_ty = unsafe { sys::type_of(found) };
        // SAFETY: a global's type is a pointer type of this context.
        let addrspace = unsafe { sys::get_pointer_address_space(found_ty) };
        let ptr_ty = tcx.ptr(ty, addrspace);
        let raw = if found_ty == tcx.lower(ptr_ty) {
            found
        } else {
            // SAFETY: a global of this context, cast to a pointer type of it.
            unsafe { sys::const_bit_cast(found, tcx.lower(ptr_ty)) }
        };
        // SAFETY: `raw` is the global or its bitcast, of type `ptr_ty`.
        Ok(unsafe { Func::new(raw, ty, ptr_ty) })
    }
}

impl Drop for Module<'_> {
    fn drop(&mut self) {
        // SAFETY: the module is live and owned here.
        unsafe { sys::dispose_module(self.raw) };
    }
}

/// A value of a context, with its type.
#[derive(Clone, Copy)]
pub struct Val<'cx> {
    raw: sys::ValueRef,
    ty: Ty,
    cx: PhantomData<&'cx Context>,
}

impl Val<'_> {
    /// The value `raw` of type `ty`.
    ///
    /// # Safety
    ///
    /// `raw` is a live value of the context the lifetime borrows, and `ty` (of that context's
    /// table) is its type.
    #[must_use]
    pub const unsafe fn new(raw: sys::ValueRef, ty: Ty) -> Self {
        Self {
            raw,
            ty,
            cx: PhantomData,
        }
    }

    /// The `LLVMValueRef`.
    #[must_use]
    pub const fn raw(self) -> sys::ValueRef {
        self.raw
    }

    /// The value's type.
    #[must_use]
    pub const fn ty(self) -> Ty {
        self.ty
    }

    /// The value's name, any bytes (`Value::getName`).
    #[must_use]
    pub fn name(self) -> Vec<u8> {
        let mut len = 0;
        // SAFETY: a live value; `len` is writable.
        let name = unsafe { sys::get_value_name2(self.raw, &raw mut len) };
        if name.is_null() {
            return Vec::new();
        }
        // SAFETY: LLVM answers `len` bytes at `name`, alive while the value keeps its name.
        unsafe { slice::from_raw_parts(name.cast::<u8>(), len) }.to_vec()
    }

    /// Names the value, any bytes (`Value::setName`; a constant takes no name).
    pub fn set_name(self, name: &[u8]) {
        // SAFETY: a live value; `name` holds its length.
        unsafe { sys::set_value_name2(self.raw, name.as_ptr().cast(), name.len()) };
    }

    /// The value's text (`Value::print`).
    #[must_use]
    pub fn print(self) -> Vec<u8> {
        // SAFETY: a live value; the answer is a message disposed of below.
        let text = unsafe { sys::print_value_to_string(self.raw) };
        // SAFETY: the message just made.
        unsafe { take_message(text) }
    }

    /// The constant integer's value, zero-extended, or `None` for anything else.
    #[must_use]
    pub fn const_u64(self) -> Option<u64> {
        // SAFETY: a live value.
        if unsafe { sys::is_a_constant_int(self.raw) }.is_null() {
            return None;
        }
        // SAFETY: a constant integer.
        Some(unsafe { sys::const_int_get_z_ext_value(self.raw) })
    }

    /// The value as metadata (`ValueAsMetadata::get`).
    #[must_use]
    pub fn as_metadata(self) -> sys::MetadataRef {
        // SAFETY: a live value.
        unsafe { sys::value_as_metadata(self.raw) }
    }
}

/// A callee: a function type and the value called through it (LLVM's `FunctionCallee`).
///
/// The value is the function or a bitcast of it.
#[derive(Clone, Copy)]
pub struct Func<'cx> {
    fn_ty: Ty,
    val: Val<'cx>,
}

impl<'cx> Func<'cx> {
    /// The callee `raw` of the function type `fn_ty`, of the pointer type `ptr_ty`.
    ///
    /// # Safety
    ///
    /// `raw` is a live function of the context the lifetime borrows, or a constant cast of one,
    /// of type `ptr_ty`, a pointer to `fn_ty`.
    #[must_use]
    pub const unsafe fn new(raw: sys::ValueRef, fn_ty: Ty, ptr_ty: Ty) -> Self {
        Self {
            fn_ty,
            // SAFETY: the caller's contract.
            val: unsafe { Val::new(raw, ptr_ty) },
        }
    }

    /// The function type calls go through.
    #[must_use]
    pub const fn fn_ty(self) -> Ty {
        self.fn_ty
    }

    /// The callee value.
    #[must_use]
    pub const fn val(self) -> Val<'cx> {
        self.val
    }

    /// Whether the callee is the function itself, not a cast of it.
    #[must_use]
    pub fn is_function(self) -> bool {
        // SAFETY: a live value.
        !unsafe { sys::is_a_function(self.val.raw) }.is_null()
    }

    /// The parameter `i` of the function, typed by the function type.
    ///
    /// `None` past the last parameter or for a cast callee.
    #[must_use]
    pub fn param(self, tcx: &TyCx, i: u32) -> Option<Val<'cx>> {
        let ty = *tcx.params(self.fn_ty)?.get(i as usize)?;
        if !self.is_function() {
            return None;
        }
        // SAFETY: a function with more than `i` parameters (its type's).
        let raw = unsafe { sys::get_param(self.val.raw, i) };
        // SAFETY: a parameter of this function, of its type's parameter type.
        Some(unsafe { Val::new(raw, ty) })
    }

    /// A new block named `name` at the end of the function; `None` for a cast callee.
    #[must_use]
    pub fn append_block(self, cx: &'cx Context, name: &CStr) -> Option<Block<'cx>> {
        if !self.is_function() {
            return None;
        }
        // SAFETY: the context and the function are live; `name` is NUL-terminated.
        let raw =
            unsafe { sys::append_basic_block_in_context(cx.raw, self.val.raw, name.as_ptr()) };
        // SAFETY: a block of this context.
        Some(unsafe { Block::new(raw) })
    }
}

/// A basic block of a context.
#[derive(Clone, Copy)]
pub struct Block<'cx> {
    raw: sys::BasicBlockRef,
    cx: PhantomData<&'cx Context>,
}

impl<'cx> Block<'cx> {
    /// The block `raw`.
    ///
    /// # Safety
    ///
    /// `raw` is a live block of the context the lifetime borrows.
    #[must_use]
    pub const unsafe fn new(raw: sys::BasicBlockRef) -> Self {
        Self {
            raw,
            cx: PhantomData,
        }
    }

    /// The `LLVMBasicBlockRef`.
    #[must_use]
    pub const fn raw(self) -> sys::BasicBlockRef {
        self.raw
    }

    /// A new block named `name` before this one (`BasicBlock::Create(…, InsertBefore)`).
    #[must_use]
    pub fn insert_before(self, cx: &'cx Context, name: &CStr) -> Self {
        // SAFETY: the context and the block are live; `name` is NUL-terminated.
        let raw = unsafe { sys::insert_basic_block_in_context(cx.raw, self.raw, name.as_ptr()) };
        // SAFETY: a block of this context.
        unsafe { Self::new(raw) }
    }
}

/// A count the C API takes as `unsigned`: operand and element lists stay far below 2^32.
fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
