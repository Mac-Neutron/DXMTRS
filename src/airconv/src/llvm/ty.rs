//! `Ty`, every type the translator makes, interned per compile and lowered to LLVM's.
//!
//! A [`Ty`] is a handle into its context's [`TyCx`]; two equal handles are one type, as LLVM's
//! uniqued types are. The table knows each type's parts, so a pointer's pointee is a `Ty` read,
//! never asked of LLVM, and [`TyCx::lower`]'s table is filled in one place, the only one that
//! makes a pointer type: under LLVM 15's typed pointers `pointee*`, and only it changes when the
//! pointers go opaque.

use core::{cell::RefCell, ffi::CStr, ptr};

use rustc_hash::FxHashMap;

use super::sys;

/// A type of one context, as an index into its [`TyCx`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Ty(u32);

/// What a [`Ty`] is.
///
/// A struct is known by its number in the context's table: identified structs (named, or made
/// by `StructType::create`) each have their own, literal structs one per field list.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum TyKind {
    /// `void`.
    Void,
    /// An integer of this many bits.
    Int(u32),
    /// `half`.
    Half,
    /// `float`.
    Float,
    /// `double`.
    Double,
    /// A fixed vector of this many elements.
    Vector(Ty, u32),
    /// An array of this many elements.
    Array(Ty, u32),
    /// A struct, by its number in the table.
    Struct(u32),
    /// A function type.
    Function {
        /// The return type.
        ret: Ty,
        /// The parameter types.
        params: Box<[Ty]>,
        /// Whether it takes more arguments than `params`.
        vararg: bool,
    },
    /// A pointer.
    Ptr {
        /// What it points at.
        pointee: Ty,
        /// Its address space.
        addrspace: u32,
    },
    /// A basic block's type.
    Label,
    /// `metadata`.
    Metadata,
}

/// One struct of the table: its LLVM type and, once it has a body, its fields.
struct StructInfo {
    raw: sys::TypeRef,
    fields: Option<Box<[Ty]>>,
}

#[derive(Default)]
struct Table {
    kinds: Vec<TyKind>,
    raws: Vec<sys::TypeRef>,
    interned: FxHashMap<TyKind, Ty>,
    structs: Vec<StructInfo>,
    literal: FxHashMap<(Box<[Ty]>, bool), Ty>,
    by_raw: FxHashMap<sys::TypeRef, Ty>,
}

/// The types of one LLVM context: every [`Ty`] made, with the LLVM type it lowers to.
///
/// The table grows behind a shared reference, so a builder that holds `&TyCx` can make the
/// types its instructions need (a GEP's result, an alloca's pointer).
pub struct TyCx {
    cx: sys::ContextRef,
    table: RefCell<Table>,
}

impl TyCx {
    /// The table of the live context `cx`, which outlives it.
    pub(super) fn new(cx: sys::ContextRef) -> Self {
        Self {
            cx,
            table: RefCell::default(),
        }
    }

    /// The LLVM type `ty` lowers to (null for a `Ty` of another table).
    #[must_use]
    pub fn lower(&self, ty: Ty) -> sys::TypeRef {
        let table = self.table.borrow();
        table
            .raws
            .get(index(ty))
            .copied()
            .unwrap_or(ptr::null_mut())
    }

    /// What `ty` is.
    #[must_use]
    pub fn kind(&self, ty: Ty) -> TyKind {
        let table = self.table.borrow();
        table.kinds.get(index(ty)).cloned().unwrap_or(TyKind::Void)
    }

    /// `void`.
    #[must_use]
    pub fn void(&self) -> Ty {
        self.intern(TyKind::Void)
    }

    /// An integer of `bits` bits.
    #[must_use]
    pub fn int(&self, bits: u32) -> Ty {
        self.intern(TyKind::Int(bits))
    }

    /// `half`.
    #[must_use]
    pub fn half(&self) -> Ty {
        self.intern(TyKind::Half)
    }

    /// `float`.
    #[must_use]
    pub fn float(&self) -> Ty {
        self.intern(TyKind::Float)
    }

    /// `double`.
    #[must_use]
    pub fn double(&self) -> Ty {
        self.intern(TyKind::Double)
    }

    /// A vector of `len` elements of `element`.
    #[must_use]
    pub fn vector(&self, element: Ty, len: u32) -> Ty {
        self.intern(TyKind::Vector(element, len))
    }

    /// An array of `len` elements of `element`.
    #[must_use]
    pub fn array(&self, element: Ty, len: u32) -> Ty {
        self.intern(TyKind::Array(element, len))
    }

    /// A pointer to `pointee` in `addrspace`.
    #[must_use]
    pub fn ptr(&self, pointee: Ty, addrspace: u32) -> Ty {
        self.intern(TyKind::Ptr { pointee, addrspace })
    }

    /// A function type.
    #[must_use]
    pub fn function(&self, ret: Ty, params: &[Ty], vararg: bool) -> Ty {
        self.intern(TyKind::Function {
            ret,
            params: params.into(),
            vararg,
        })
    }

    /// A basic block's type.
    #[must_use]
    pub fn label(&self) -> Ty {
        self.intern(TyKind::Label)
    }

    /// `metadata`.
    #[must_use]
    pub fn metadata(&self) -> Ty {
        self.intern(TyKind::Metadata)
    }

    /// The literal struct of `fields` (`StructType::get`).
    #[must_use]
    pub fn literal_struct(&self, fields: &[Ty], packed: bool) -> Ty {
        let key: (Box<[Ty]>, bool) = (fields.into(), packed);
        if let Some(&ty) = self.table.borrow().literal.get(&key) {
            return ty;
        }
        let mut raws = self.lower_all(fields);
        // SAFETY: the context is live, each element a type of it; `raws` holds its length.
        let raw = unsafe {
            sys::struct_type_in_context(
                self.cx,
                raws.as_mut_ptr(),
                count(raws.len()),
                packed.into(),
            )
        };
        let (ty, _) = self.add_struct(raw, Some(key.0.clone()));
        self.table.borrow_mut().literal.insert(key, ty);
        ty
    }

    /// The identified struct named `name`, made opaque when the context has none.
    ///
    /// `StructType::getTypeByName`, then `StructType::create`.
    #[must_use]
    pub fn named_struct(&self, name: &CStr) -> Ty {
        // SAFETY: the context is live; `name` is NUL-terminated.
        let found = unsafe { sys::get_type_by_name2(self.cx, name.as_ptr()) };
        if !found.is_null() {
            // SAFETY: `found` is a type of this context.
            if let Some(ty) = unsafe { self.lift(found) } {
                return ty;
            }
        }
        self.create_struct(name, None)
    }

    /// A new identified struct (`StructType::create`).
    ///
    /// Named `name` (LLVM adds a suffix to a name in use; an empty name leaves it unnamed), with
    /// `body`'s fields and packing, or opaque.
    #[must_use]
    pub fn create_struct(&self, name: &CStr, body: Option<(&[Ty], bool)>) -> Ty {
        // SAFETY: the context is live; `name` is NUL-terminated.
        let raw = unsafe { sys::struct_create_named(self.cx, name.as_ptr()) };
        let (ty, _) = self.add_struct(raw, None);
        if let Some((fields, packed)) = body {
            self.set_body(ty, fields, packed);
        }
        ty
    }

    /// Gives the identified struct `ty` its body (`StructType::setBody`).
    ///
    /// Anything else is left alone.
    pub fn set_body(&self, ty: Ty, fields: &[Ty], packed: bool) {
        let TyKind::Struct(id) = self.kind(ty) else {
            return;
        };
        let mut raws = self.lower_all(fields);
        let mut table = self.table.borrow_mut();
        let Some(info) = table.structs.get_mut(id as usize) else {
            return;
        };
        // SAFETY: `info.raw` is a struct of this context, each element a type of it; `raws`
        // holds its length.
        unsafe {
            sys::struct_set_body(
                info.raw,
                raws.as_mut_ptr(),
                count(raws.len()),
                packed.into(),
            );
        }
        info.fields = Some(fields.into());
    }

    /// The `Ty` of the LLVM type `raw`, made from its parts.
    ///
    /// For a parsed module's types, an intrinsic's. `None` for a kind the translator never makes
    /// (an opaque pointer, a token, `bfloat`, a scalable vector, x86's).
    ///
    /// # Safety
    ///
    /// `raw` is a type of this table's context.
    #[must_use]
    pub unsafe fn lift(&self, raw: sys::TypeRef) -> Option<Ty> {
        // SAFETY: the caller's contract.
        self.lift_raw(unsafe { RawTy::new(raw) })
    }

    /// The pointee of the pointer `ty`.
    #[must_use]
    pub fn pointee(&self, ty: Ty) -> Option<Ty> {
        match self.kind(ty) {
            TyKind::Ptr { pointee, .. } => Some(pointee),
            _ => None,
        }
    }

    /// The address space of the pointer `ty`.
    #[must_use]
    pub fn addrspace(&self, ty: Ty) -> Option<u32> {
        match self.kind(ty) {
            TyKind::Ptr { addrspace, .. } => Some(addrspace),
            _ => None,
        }
    }

    /// The element of the vector or array `ty`.
    #[must_use]
    pub fn element(&self, ty: Ty) -> Option<Ty> {
        match self.kind(ty) {
            TyKind::Vector(element, _) | TyKind::Array(element, _) => Some(element),
            _ => None,
        }
    }

    /// The number of elements of the vector or array `ty`.
    #[must_use]
    pub fn count(&self, ty: Ty) -> Option<u32> {
        match self.kind(ty) {
            TyKind::Vector(_, n) | TyKind::Array(_, n) => Some(n),
            _ => None,
        }
    }

    /// The return type of the function type `ty`.
    #[must_use]
    pub fn ret(&self, ty: Ty) -> Option<Ty> {
        match self.kind(ty) {
            TyKind::Function { ret, .. } => Some(ret),
            _ => None,
        }
    }

    /// The parameter types of the function type `ty`.
    #[must_use]
    pub fn params(&self, ty: Ty) -> Option<Box<[Ty]>> {
        match self.kind(ty) {
            TyKind::Function { params, .. } => Some(params),
            _ => None,
        }
    }

    /// The fields of the struct `ty`; `None` for an opaque one or another kind.
    #[must_use]
    pub fn fields(&self, ty: Ty) -> Option<Box<[Ty]>> {
        let TyKind::Struct(id) = self.kind(ty) else {
            return None;
        };
        let table = self.table.borrow();
        table.structs.get(id as usize)?.fields.clone()
    }

    /// The field `i` of the struct `ty`.
    #[must_use]
    pub fn field(&self, ty: Ty, i: u32) -> Option<Ty> {
        self.fields(ty)?.get(i as usize).copied()
    }

    /// The bits of the integer `ty`.
    #[must_use]
    pub fn int_bits(&self, ty: Ty) -> Option<u32> {
        match self.kind(ty) {
            TyKind::Int(bits) => Some(bits),
            _ => None,
        }
    }

    /// The bits of `ty`'s scalar, a vector's element or `ty` (`Type::getScalarSizeInBits`).
    ///
    /// 0 for anything but an integer or a floating-point type.
    #[must_use]
    pub fn scalar_bits(&self, ty: Ty) -> u32 {
        let scalar = match self.kind(ty) {
            TyKind::Vector(element, _) => self.kind(element),
            kind => kind,
        };
        match scalar {
            TyKind::Int(bits) => bits,
            TyKind::Half => 16,
            TyKind::Float => 32,
            TyKind::Double => 64,
            _ => 0,
        }
    }

    /// The type a GEP over `source` with `indices` points at (`getIndexedType`).
    ///
    /// The first index steps over the pointer, each other one into an aggregate, a struct's by
    /// its constant (`None`: not a constant).
    #[must_use]
    pub fn indexed(&self, source: Ty, indices: &[Option<u64>]) -> Option<Ty> {
        let mut ty = source;
        for index in indices.iter().skip(1) {
            ty = match self.kind(ty) {
                TyKind::Struct(_) => self.field(ty, u32::try_from((*index)?).ok()?)?,
                TyKind::Array(element, _) | TyKind::Vector(element, _) => element,
                _ => return None,
            };
        }
        Some(ty)
    }

    fn lift_raw(&self, raw: RawTy) -> Option<Ty> {
        if let Some(&ty) = self.table.borrow().by_raw.get(&raw.0) {
            return Some(ty);
        }
        let ty = match raw.kind() {
            sys::VOID_TYPE_KIND => self.void(),
            sys::HALF_TYPE_KIND => self.half(),
            sys::FLOAT_TYPE_KIND => self.float(),
            sys::DOUBLE_TYPE_KIND => self.double(),
            sys::LABEL_TYPE_KIND => self.label(),
            sys::METADATA_TYPE_KIND => self.metadata(),
            sys::INTEGER_TYPE_KIND => self.int(raw.int_width()),
            sys::VECTOR_TYPE_KIND => self.vector(self.lift_raw(raw.element())?, raw.vector_len()),
            sys::ARRAY_TYPE_KIND => self.array(self.lift_raw(raw.element())?, raw.array_len()),
            sys::POINTER_TYPE_KIND if !raw.is_opaque_pointer() => {
                self.ptr(self.lift_raw(raw.element())?, raw.addrspace())
            }
            sys::FUNCTION_TYPE_KIND => {
                let ret = self.lift_raw(raw.ret())?;
                let params = self.lift_all(raw.params())?;
                self.function(ret, &params, raw.is_vararg())
            }
            sys::STRUCT_TYPE_KIND if raw.is_literal_struct() => {
                let fields = self.lift_all(raw.fields())?;
                self.literal_struct(&fields, raw.is_packed())
            }
            sys::STRUCT_TYPE_KIND => {
                // Registered before its fields, which may point back at it.
                let (ty, id) = self.add_struct(raw.0, None);
                if !raw.is_opaque_struct() {
                    let fields = self.lift_all(raw.fields())?;
                    if let Some(info) = self.table.borrow_mut().structs.get_mut(id as usize) {
                        info.fields = Some(fields.into());
                    }
                }
                ty
            }
            _ => return None,
        };
        Some(ty)
    }

    fn lift_all(&self, raws: Vec<RawTy>) -> Option<Vec<Ty>> {
        raws.into_iter().map(|raw| self.lift_raw(raw)).collect()
    }

    fn lower_all(&self, tys: &[Ty]) -> Vec<sys::TypeRef> {
        tys.iter().map(|&ty| self.lower(ty)).collect()
    }

    fn add_struct(&self, raw: sys::TypeRef, fields: Option<Box<[Ty]>>) -> (Ty, u32) {
        let mut table = self.table.borrow_mut();
        let id = count(table.structs.len());
        table.structs.push(StructInfo { raw, fields });
        let ty = push(&mut table, TyKind::Struct(id), raw);
        table.by_raw.insert(raw, ty);
        (ty, id)
    }

    fn intern(&self, kind: TyKind) -> Ty {
        if let Some(&ty) = self.table.borrow().interned.get(&kind) {
            return ty;
        }
        let raw = self.make(&kind);
        let mut table = self.table.borrow_mut();
        let ty = push(&mut table, kind.clone(), raw);
        table.interned.insert(kind, ty);
        table.by_raw.entry(raw).or_insert(ty);
        ty
    }

    /// The LLVM type of `kind`, whose parts are in the table.
    ///
    /// The only place a pointer type is made.
    fn make(&self, kind: &TyKind) -> sys::TypeRef {
        let cx = self.cx;
        match *kind {
            // SAFETY: the context is live.
            TyKind::Void => unsafe { sys::void_type_in_context(cx) },
            // SAFETY: as above.
            TyKind::Int(bits) => unsafe { sys::int_type_in_context(cx, bits) },
            // SAFETY: as above.
            TyKind::Half => unsafe { sys::half_type_in_context(cx) },
            // SAFETY: as above.
            TyKind::Float => unsafe { sys::float_type_in_context(cx) },
            // SAFETY: as above.
            TyKind::Double => unsafe { sys::double_type_in_context(cx) },
            // SAFETY: as above.
            TyKind::Label => unsafe { sys::label_type_in_context(cx) },
            // SAFETY: as above.
            TyKind::Metadata => unsafe { sys::metadata_type_in_context(cx) },
            // SAFETY: the element is a type of this context (the table's).
            TyKind::Vector(element, n) => unsafe { sys::vector_type(self.lower(element), n) },
            // SAFETY: as above.
            TyKind::Array(element, n) => unsafe { sys::array_type(self.lower(element), n) },
            // SAFETY: the pointee is a type of this context (the table's).
            TyKind::Ptr { pointee, addrspace } => unsafe {
                sys::pointer_type(self.lower(pointee), addrspace)
            },
            TyKind::Function {
                ret,
                ref params,
                vararg,
            } => {
                let mut raws = self.lower_all(params);
                // SAFETY: the parts are types of this context; `raws` holds its length.
                unsafe {
                    sys::function_type(
                        self.lower(ret),
                        raws.as_mut_ptr(),
                        count(raws.len()),
                        vararg.into(),
                    )
                }
            }
            TyKind::Struct(id) => {
                let table = self.table.borrow();
                table
                    .structs
                    .get(id as usize)
                    .map_or(ptr::null_mut(), |info| info.raw)
            }
        }
    }
}

/// A live LLVM type of the table's context, read through the C API.
#[derive(Clone, Copy)]
struct RawTy(sys::TypeRef);

impl RawTy {
    /// # Safety
    ///
    /// `raw` is a live type, of the context of the table that reads it.
    const unsafe fn new(raw: sys::TypeRef) -> Self {
        Self(raw)
    }

    fn kind(self) -> sys::TypeKind {
        // SAFETY: a live type (`new`'s contract).
        unsafe { sys::get_type_kind(self.0) }
    }

    fn int_width(self) -> u32 {
        // SAFETY: a live integer type (checked by the caller's kind).
        unsafe { sys::get_int_type_width(self.0) }
    }

    fn element(self) -> Self {
        // SAFETY: a live vector, array or typed pointer (the caller's kind); its element is a
        // type of the same context.
        Self(unsafe { sys::get_element_type(self.0) })
    }

    fn vector_len(self) -> u32 {
        // SAFETY: a live vector type.
        unsafe { sys::get_vector_size(self.0) }
    }

    fn array_len(self) -> u32 {
        // SAFETY: a live array type.
        unsafe { sys::get_array_length(self.0) }
    }

    fn is_opaque_pointer(self) -> bool {
        // SAFETY: a live pointer type.
        unsafe { sys::pointer_type_is_opaque(self.0) != 0 }
    }

    fn addrspace(self) -> u32 {
        // SAFETY: a live pointer type.
        unsafe { sys::get_pointer_address_space(self.0) }
    }

    fn ret(self) -> Self {
        // SAFETY: a live function type; its return type is a type of the same context.
        Self(unsafe { sys::get_return_type(self.0) })
    }

    fn params(self) -> Vec<Self> {
        // SAFETY: a live function type.
        let n = unsafe { sys::count_param_types(self.0) };
        let mut raws = vec![ptr::null_mut(); n as usize];
        // SAFETY: `raws` has room for every parameter type.
        unsafe { sys::get_param_types(self.0, raws.as_mut_ptr()) };
        raws.into_iter().map(Self).collect()
    }

    fn is_vararg(self) -> bool {
        // SAFETY: a live function type.
        unsafe { sys::is_function_var_arg(self.0) != 0 }
    }

    fn is_literal_struct(self) -> bool {
        // SAFETY: a live struct type.
        unsafe { sys::is_literal_struct(self.0) != 0 }
    }

    fn is_opaque_struct(self) -> bool {
        // SAFETY: a live struct type.
        unsafe { sys::is_opaque_struct(self.0) != 0 }
    }

    fn is_packed(self) -> bool {
        // SAFETY: a live struct type.
        unsafe { sys::is_packed_struct(self.0) != 0 }
    }

    fn fields(self) -> Vec<Self> {
        // SAFETY: a live struct type.
        let n = unsafe { sys::count_struct_element_types(self.0) };
        let mut raws = vec![ptr::null_mut(); n as usize];
        // SAFETY: `raws` has room for every element type.
        unsafe { sys::get_struct_element_types(self.0, raws.as_mut_ptr()) };
        raws.into_iter().map(Self).collect()
    }
}

fn push(table: &mut Table, kind: TyKind, raw: sys::TypeRef) -> Ty {
    let ty = Ty(count(table.kinds.len()));
    table.kinds.push(kind);
    table.raws.push(raw);
    ty
}

const fn index(ty: Ty) -> usize {
    ty.0 as usize
}

/// A length the C API takes as `unsigned`: the table and a type's parts stay far below 2^32.
fn count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}
