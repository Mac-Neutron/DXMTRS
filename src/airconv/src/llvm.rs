//! LLVM 23 through its C API: the declarations, the handles and one `Ty` for every type.
//!
//! A [`Context`] is one compile's LLVM context, with a diagnostic handler, so nothing LLVM
//! reports ends the process: an error is kept for [`Context::take_error`], which every parse, link
//! and verify checks, a warning or a note is written to stderr as LLVM would have written it, a
//! remark is dropped (LLVM filters remarks when it prints itself). The handles ([`Module`],
//! [`Func`], [`Block`], [`Val`]) borrow their context, so none outlives it; every value carries
//! its [`Ty`]. Each wrapper makes one C call.
//!
//! A constant maker checks its type's kind ([`Context::const_int`] an integer, …): another kind
//! answers `undef` and an error the compile reports, since a DXIL `dx.op` declared with other
//! types can steer one. Beyond that the wrappers check what their C calls need only as far as
//! their types carry it, by design: a builder method takes operands of the kinds its instruction
//! needs, as the C++'s `IRBuilder` did. LLVM is built without assertions, so a wrong kind there is
//! undefined behaviour; the `oracle` feature checks every value's type as it is made, in the tests
//! and the lane tools. A [`Val`], [`Func`] or [`Block`] borrows the context, not its module or
//! instruction: it is live while what it names is. A function that erases what a `Val` or
//! `Block` it is given names is an `unsafe fn` (DXIL's `Lowering::lower`, `group_shared64`,
//! `erase_terminator`). The functions that take raw `sys` handles expect live ones of the
//! compile's context, as LLVM's walkers and this module's makers answer them; the crate's
//! modules are public, but other crates call only its exports and `abi`.

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
    collections::BTreeMap,
    ffi::CString,
    io::{self, Write as _},
};

pub use builder::{Builder, FastMath};
pub use ty::{Ty, TyCx, TyKind};

use crate::api::Error;

/// A NUL-terminated copy of a name the C API takes as a C string.
///
/// A name holding a NUL is P15's error; the length-taking forms (`Val::set_name`,
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

/// One compile's LLVM context, with its types ([`TyCx`]) and recorded symbol types (P8).
pub struct Context {
    raw: sys::ContextRef,
    tcx: TyCx,
    recorded: RefCell<BTreeMap<Vec<u8>, Ty>>,
    /// The handler's state: a box made in [`Context::new`], freed after the context in `drop`.
    diagnostics: *mut Diagnostics,
}

impl Context {
    /// A context with the diagnostic handler installed.
    #[must_use]
    pub fn new() -> Self {
        // SAFETY: creating a context has no precondition.
        let raw = unsafe { sys::context_create() };
        let diagnostics = Box::into_raw(Box::<Diagnostics>::default());
        // SAFETY: `raw` is live; `diagnostics` stays allocated until after the context is
        // disposed of (`drop`), and the handler only reads through it.
        unsafe { sys::context_set_diagnostic_handler(raw, Some(diagnose), diagnostics.cast()) };
        Self {
            raw,
            tcx: TyCx::new(raw),
            recorded: RefCell::default(),
            diagnostics,
        }
    }

    /// Records the function or global `name` with its [`Ty`] (P8).
    pub fn record(&self, name: &[u8], ty: Ty) {
        if !name.is_empty() {
            self.recorded.borrow_mut().insert(name.to_vec(), ty);
        }
    }

    /// The recorded [`Ty`] of the function or global `name`, or `None` if it was not recorded.
    #[must_use]
    pub fn recorded(&self, name: &[u8]) -> Option<Ty> {
        self.recorded.borrow().get(name).copied()
    }

    /// The number of recorded function and global names in this context.
    #[must_use]
    pub fn recorded_len(&self) -> usize {
        self.recorded.borrow().len()
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
        let _ = self.raw;
        // SAFETY: the caller's contract.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant `value` of the integer type `ty` (`ConstantInt::get`).
    ///
    /// Its low bits are taken; `sign_extend` widens it by its sign past 64 bits. A `ty` of
    /// another kind answers `undef` and fails the compile (the module doc).
    #[must_use]
    pub fn const_int(&self, ty: Ty, value: u64, sign_extend: bool) -> Val<'_> {
        if !matches!(self.tcx.kind(ty), TyKind::Int(_)) {
            return self.wrong_kind(ty);
        }
        // SAFETY: `ty` lowers to an integer type of this context.
        let raw = unsafe { sys::const_int(self.tcx.lower(ty), value, sign_extend.into()) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The constant `value` of the floating-point type `ty`, rounded to it (`ConstantFP::get`).
    ///
    /// `ty` may be a vector of one. A `ty` of another kind answers `undef` and fails the compile
    /// (the module doc).
    #[must_use]
    pub fn const_real(&self, ty: Ty, value: f64) -> Val<'_> {
        if !self.is_fp(self.scalar(ty)) {
            return self.wrong_kind(ty);
        }
        // SAFETY: `ty` lowers to a floating-point type (or a vector of one) of this context.
        let raw = unsafe { sys::const_real(self.tcx.lower(ty), value) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// The zero of `ty` (`Constant::getNullValue`).
    ///
    /// `void`, `Unknown`, a function type, a label and `metadata` have none: they answer `undef`
    /// and fail the compile (the module doc).
    #[must_use]
    pub fn null(&self, ty: Ty) -> Val<'_> {
        if matches!(
            self.tcx.kind(ty),
            TyKind::Void
                | TyKind::Unknown
                | TyKind::Function { .. }
                | TyKind::Label
                | TyKind::Metadata
        ) {
            return self.wrong_kind(ty);
        }
        // SAFETY: `ty` lowers to a type of this context that has a zero.
        let raw = unsafe { sys::const_null(self.tcx.lower(ty)) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// Every bit set, of the integer or floating-point `ty` (`Constant::getAllOnesValue`).
    ///
    /// `ty` may be a vector of one. A `ty` of another kind answers `undef` and fails the compile
    /// (the module doc).
    #[must_use]
    pub fn all_ones(&self, ty: Ty) -> Val<'_> {
        let scalar = self.scalar(ty);
        if !matches!(self.tcx.kind(scalar), TyKind::Int(_)) && !self.is_fp(scalar) {
            return self.wrong_kind(ty);
        }
        // SAFETY: `ty` lowers to an integer or floating-point type (or a vector of one) of this
        // context.
        let raw = unsafe { sys::const_all_ones(self.tcx.lower(ty)) };
        // SAFETY: a constant of this context, of type `ty`.
        unsafe { Val::new(raw, ty) }
    }

    /// A constant maker's answer for a `ty` its call does not take: `undef` of `ty`, and an error.
    ///
    /// LLVM, built without assertions, would read `ty` as the kind it expects. The compile's next
    /// [`Self::take_error`] reports the error (`LLVM error: a constant of the wrong type`). The
    /// translator's own types never reach it; a DXIL `dx.op` declared with other types can.
    fn wrong_kind(&self, ty: Ty) -> Val<'_> {
        self.diagnostics()
            .error
            .borrow_mut()
            .get_or_insert_with(|| b"a constant of the wrong type".to_vec());
        self.undef(ty)
    }

    /// The element type of the vector `ty`, else `ty`.
    fn scalar(&self, ty: Ty) -> Ty {
        match self.tcx.kind(ty) {
            TyKind::Vector(element, _) => element,
            _ => ty,
        }
    }

    /// Whether `ty` is `half`, `float` or `double`.
    fn is_fp(&self, ty: Ty) -> bool {
        matches!(
            self.tcx.kind(ty),
            TyKind::Half | TyKind::Float | TyKind::Double
        )
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

    /// The enum attribute `name` (`nounwind`, `willreturn`, …) with `value` (`Attribute::get`).
    ///
    /// # Errors
    ///
    /// `LLVM error: no attribute <name>` for a name LLVM 23 does not know.
    pub fn enum_attribute(&self, name: &[u8], value: u64) -> Result<sys::AttributeRef, Error> {
        // SAFETY: `name` holds its length.
        let kind =
            unsafe { sys::get_enum_attribute_kind_for_name(name.as_ptr().cast(), name.len()) };
        if kind == 0 {
            return Err(Error::new(
                &[&b"LLVM error: no attribute "[..], name].concat(),
            ));
        }
        // SAFETY: the context is live, `kind` an attribute kind LLVM named.
        Ok(unsafe { sys::create_enum_attribute(self.raw, kind, value) })
    }

    /// Converts an attribute table `(index, name)` into LLVM 23 attributes.
    ///
    /// Upgrades function-level memory attributes and parameter `nocapture` as `BitcodeReader`
    /// does (P20).
    ///
    /// # Errors
    ///
    /// `LLVM error: no attribute <name>` when `name` is neither a known memory/capture
    /// attribute nor an LLVM 23 enum attribute.
    pub fn attributes15(
        &self,
        attrs: &[(sys::AttributeIndex, &[u8])],
    ) -> Result<Vec<(sys::AttributeIndex, sys::AttributeRef)>, Error> {
        let mut out = Vec::with_capacity(attrs.len());
        let mut has_mem = false;
        let mut loc_mask = 7;
        let mut mod_ref = 3;
        for &(index, attr_name) in attrs {
            if index == sys::ATTRIBUTE_FUNCTION_INDEX {
                match attr_name {
                    b"readnone" => {
                        has_mem = true;
                        mod_ref &= 0;
                    }
                    b"readonly" => {
                        has_mem = true;
                        mod_ref &= 1;
                    }
                    b"writeonly" => {
                        has_mem = true;
                        mod_ref &= 2;
                    }
                    b"argmemonly" => {
                        has_mem = true;
                        loc_mask &= 1;
                    }
                    b"inaccessiblememonly" => {
                        has_mem = true;
                        loc_mask &= 2;
                    }
                    b"inaccessiblemem_or_argmemonly" => {
                        has_mem = true;
                        loc_mask &= 3;
                    }
                    _ => out.push((index, self.enum_attribute(attr_name, 0)?)),
                }
            } else if attr_name == b"nocapture" {
                // SAFETY: the context is live.
                out.push((index, unsafe { sys::airconv_shim_captures_none(self.raw) }));
            } else {
                out.push((index, self.enum_attribute(attr_name, 0)?));
            }
        }
        if has_mem {
            let (argmem_only, mr) = match loc_mask {
                0 => (0, 0),
                1..=3 => (loc_mask, mod_ref),
                _ => (0, mod_ref),
            };
            // SAFETY: the context is live.
            let mem = unsafe { sys::airconv_shim_memory_attribute(self.raw, argmem_only, mr) };
            out.push((sys::ATTRIBUTE_FUNCTION_INDEX, mem));
        }
        Ok(out)
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
        self.cx.record(&value_name(raw), ty);
        // SAFETY: a function of this context of type `ty`, in address space 0 (AIR's data layout
        // names no program address space).
        Ok(unsafe { Func::new(raw, ty, tcx.ptr(ty, 0)) })
    }

    /// A new global `name` of the value type `ty` in `addrspace` (`new GlobalVariable`).
    ///
    /// # Errors
    ///
    /// P15's error for a name holding a NUL, or when the global could not be created.
    pub fn add_global(&self, name: &[u8], ty: Ty, addrspace: u32) -> Result<Val<'cx>, Error> {
        let c_name = self::name(name)?;
        let tcx = self.cx.tcx();
        // SAFETY: the module is live, `ty` lowers to a type of its context, `c_name` is
        // NUL-terminated.
        let raw = unsafe {
            sys::add_global_in_address_space(self.raw, tcx.lower(ty), c_name.as_ptr(), addrspace)
        };
        if raw.is_null() {
            return Err(Error::new(b"invalid operation: a global was not made."));
        }
        self.cx.record(&value_name(raw), ty);
        // SAFETY: a global of this context, a pointer to `ty` in `addrspace`.
        let val = unsafe { self.cx.val(raw, tcx.ptr(ty, addrspace)) };
        #[cfg(feature = "oracle")]
        if let Some(text) = crate::oracle::check(self.cx, "add_global", val) {
            self.cx.diagnostics().error.borrow_mut().get_or_insert(text);
        }
        Ok(val)
    }

    /// The module's text (`Module::print`).
    #[cfg(any(test, feature = "oracle"))]
    #[must_use]
    pub fn print(&self) -> Vec<u8> {
        // SAFETY: the module is live; the answer is a message disposed of below.
        let text = unsafe { sys::print_module_to_string(self.raw) };
        // SAFETY: the message just made.
        unsafe { take_message(text) }
    }

    /// `Module::getOrInsertFunction`: the global named `name`, called as the function type `ty`.
    ///
    /// When there is none, a new external declaration with `attrs` (`(index, name)`, not given to
    /// an `llvm.` intrinsic, which has its own); a bitcast of it when it has another type.
    ///
    /// # Errors
    ///
    /// P15's error for a name holding a NUL, or an unknown attribute name in `attrs`.
    pub fn get_or_insert_function(
        &self,
        name: &[u8],
        ty: Ty,
        attrs: &[(sys::AttributeIndex, &[u8])],
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
            let resolved = if name.starts_with(b"llvm.") {
                Vec::new()
            } else {
                self.cx.attributes15(attrs)?
            };
            let func = self.add_function(name, ty)?;
            for (index, attr) in resolved {
                // SAFETY: a function just made; `attr` an attribute of its context.
                unsafe { sys::add_attribute_at_index(func.val.raw, index, attr) };
            }
            return Ok(func);
        }
        // SAFETY: `found` is a global of this module.
        let found_ty = unsafe { sys::type_of(found) };
        // SAFETY: a global's type is a pointer type of this context.
        let addrspace = unsafe { sys::get_pointer_address_space(found_ty) };
        let ptr_ty = tcx.ptr(ty, addrspace);
        // SAFETY: `found` is the global, of type `ptr_ty`.
        Ok(unsafe { Func::new(found, ty, ptr_ty) })
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

/// The name of `value` (`LLVMGetValueName2`).
pub fn value_name(value: sys::ValueRef) -> Vec<u8> {
    let mut len = 0;
    // SAFETY: a live value; `len` is writable.
    let name = unsafe { sys::get_value_name2(value, &raw mut len) };
    if name.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: LLVM answers `len` bytes at `name`, alive while the value keeps its name.
    unsafe { slice::from_raw_parts(name.cast::<u8>(), len) }.to_vec()
}

/// A count the C API takes as `unsigned`: operand and element lists stay far below 2^32.
fn len_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
