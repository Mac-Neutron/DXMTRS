//! The LLVM 23 C API, `AIRWriter.h` and the shim's functions, declared by hand.
//!
//! Each declaration is the prototype of `llvm-c/{Core,Analysis,Linker,AIRWriter}.h` in the install
//! `AIRCONV_LLVM` names, under its snake-case name with the C name as `link_name` (`LLVMBuildGEP2`
//! is `build_gep2`); enumerators are constants named the same way (`LLVMIntEQ` is `INT_EQ`).
//! Only the deprecated forms are left out: the typed `2` forms (`LLVMBuildLoad2`, …) and the
//! length-taking ones (`LLVMSetValueName2`, `LLVMMDStringInContext2`) are what the translator
//! calls. The list is every function a later part of the translator uses; one that is missing is
//! added here, never declared where it is called. The shim's functions (`shim/shim.cpp`, named
//! `airconv_shim_*`) cover what this API lacks on LLVM 23.

use core::{
    ffi::{c_char, c_int, c_longlong, c_uint, c_ulonglong, c_void},
    marker::{PhantomData, PhantomPinned},
};

macro_rules! opaque {
    ($($(#[$doc:meta])* $name:ident => $handle:ident;)*) => {$(
        $(#[$doc])*
        #[repr(C)]
        pub struct $name {
            _data: [u8; 0],
            _marker: PhantomData<(*mut u8, PhantomPinned)>,
        }
        #[doc = concat!("A pointer to an [`", stringify!($name), "`], as the C API passes it.")]
        pub type $handle = *mut $name;
    )*};
}

opaque! {
    /// `LLVMContext`.
    Context => ContextRef;
    /// `Module`.
    Module => ModuleRef;
    /// `Type`.
    Type => TypeRef;
    /// `Value`.
    Value => ValueRef;
    /// `BasicBlock`.
    BasicBlock => BasicBlockRef;
    /// `Metadata`.
    Metadata => MetadataRef;
    /// `NamedMDNode`.
    NamedMdNode => NamedMDNodeRef;
    /// `IRBuilder<>`.
    Builder => BuilderRef;
    /// `MemoryBuffer`.
    MemoryBuffer => MemoryBufferRef;
    /// `Attribute`.
    Attribute => AttributeRef;
    /// `DiagnosticInfo`.
    DiagnosticInfo => DiagnosticInfoRef;
    /// `Use`.
    Use => UseRef;
}

/// An entry of `LLVMInstructionGetAllMetadataOtherThanDebugLoc`'s array.
#[repr(C)]
pub struct ValueMetadataEntry {
    _data: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// `LLVMBool`: 0 is false.
pub type Bool = c_int;
/// `LLVMOpcode`.
pub type Opcode = c_uint;
/// `LLVMTypeKind`.
pub type TypeKind = c_uint;
/// `LLVMLinkage`.
pub type Linkage = c_uint;
/// `LLVMIntPredicate`.
pub type IntPredicate = c_uint;
/// `LLVMRealPredicate`.
pub type RealPredicate = c_uint;
/// `LLVMAtomicOrdering`.
pub type AtomicOrdering = c_uint;
/// `LLVMAtomicRMWBinOp`.
pub type AtomicRMWBinOp = c_uint;
/// `LLVMDiagnosticSeverity`.
pub type DiagnosticSeverity = c_uint;
/// `LLVMVerifierFailureAction`: only `RETURN_STATUS_ACTION` neither prints nor aborts.
pub type VerifierFailureAction = c_uint;
/// `LLVMAttributeIndex`: the return value, the function, or a parameter from 1.
pub type AttributeIndex = c_uint;
/// `LLVMDiagnosticHandler`.
pub type DiagnosticHandler = Option<unsafe extern "C" fn(DiagnosticInfoRef, *mut c_void)>;

/// `LLVMAttributeFunctionIndex` (-1 as an unsigned).
pub const ATTRIBUTE_FUNCTION_INDEX: AttributeIndex = AttributeIndex::MAX;

/// `LLVMAIRValueType`: a value and its typed type (`AIRWriter.h`).
#[repr(C)]
pub struct AirValueType {
    /// The value (`Function *`, `GlobalVariable *` or `Value *`).
    pub value: ValueRef,
    /// Its typed type, built with [`air_typed_pointer_type`].
    pub ty: TypeRef,
}

/// `LLVMAIRStructBody`: an identified struct and its typed literal body (`AIRWriter.h`).
#[repr(C)]
pub struct AirStructBody {
    /// The identified struct type in the module's context.
    pub struct_ty: TypeRef,
    /// The typed literal struct body, with pointers built by [`air_typed_pointer_type`].
    pub body: TypeRef,
}

/// `LLVMAIRTypes`: the typed side table for [`write_air_bitcode`] (`AIRWriter.h`).
#[repr(C)]
pub struct AirTypes {
    /// Functions, globals and pointer values with their typed types.
    pub values: *const AirValueType,
    /// The number of entries in `values`.
    pub num_values: usize,
    /// Identified struct bodies whose fields hold pointers.
    pub structs: *const AirStructBody,
    /// The number of entries in `structs`.
    pub num_structs: usize,
    /// Typed bitcode buffers (`air_*.air` helpers) read for typed signatures and bodies.
    pub typed_refs: *const MemoryBufferRef,
    /// The number of buffers in `typed_refs`.
    pub num_typed_refs: usize,
}

/// `LLVMAIRWriteStats`: per-module counts from [`write_air_bitcode`] (`AIRWriter.h`).
#[repr(C)]
pub struct AirWriteStats {
    /// `bitcast` instructions inserted at pointer-type boundaries.
    pub inst_casts: u32,
    /// Constant-expression `bitcast`s made for globals and initializers.
    pub const_casts: u32,
    /// Pointer values typed from a use or `i8*` fallback.
    pub use_typed: u32,
    /// LLVM 23-only attributes dropped or weakened.
    pub attrs_dropped: u32,
    /// LLVM 23-only instruction flags (`nneg`, `disjoint`, `samesign`, `nuw`, `nusw`) dropped.
    pub flags_dropped: u32,
    /// Named symbols and structs filled from `typed_refs`.
    pub refs_filled: u32,
}

#[cfg(target_arch = "aarch64")]
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<AirValueType>() == 16);
    assert!(offset_of!(AirValueType, value) == 0);
    assert!(offset_of!(AirValueType, ty) == 8);

    assert!(size_of::<AirStructBody>() == 16);
    assert!(offset_of!(AirStructBody, struct_ty) == 0);
    assert!(offset_of!(AirStructBody, body) == 8);

    assert!(size_of::<AirTypes>() == 48);
    assert!(offset_of!(AirTypes, values) == 0);
    assert!(offset_of!(AirTypes, num_values) == 8);
    assert!(offset_of!(AirTypes, structs) == 16);
    assert!(offset_of!(AirTypes, num_structs) == 24);
    assert!(offset_of!(AirTypes, typed_refs) == 32);
    assert!(offset_of!(AirTypes, num_typed_refs) == 40);

    assert!(size_of::<AirWriteStats>() == 24);
    assert!(offset_of!(AirWriteStats, inst_casts) == 0);
    assert!(offset_of!(AirWriteStats, const_casts) == 4);
    assert!(offset_of!(AirWriteStats, use_typed) == 8);
    assert!(offset_of!(AirWriteStats, attrs_dropped) == 12);
    assert!(offset_of!(AirWriteStats, flags_dropped) == 16);
    assert!(offset_of!(AirWriteStats, refs_filled) == 20);

    let _ = (air_typed_pointer_type, write_air_bitcode);
};

// `LLVMOpcode`
pub const ADD: Opcode = 8;
pub const F_ADD: Opcode = 9;
pub const MUL: Opcode = 12;
pub const F_MUL: Opcode = 13;
pub const AND: Opcode = 23;
pub const OR: Opcode = 24;
pub const XOR: Opcode = 25;
#[cfg(all(test, feature = "oracle"))]
pub const Z_EXT: Opcode = 31;

// `LLVMTypeKind`
pub const VOID_TYPE_KIND: TypeKind = 0;
pub const HALF_TYPE_KIND: TypeKind = 1;
pub const FLOAT_TYPE_KIND: TypeKind = 2;
pub const DOUBLE_TYPE_KIND: TypeKind = 3;
pub const LABEL_TYPE_KIND: TypeKind = 7;
pub const INTEGER_TYPE_KIND: TypeKind = 8;
pub const FUNCTION_TYPE_KIND: TypeKind = 9;
pub const STRUCT_TYPE_KIND: TypeKind = 10;
pub const ARRAY_TYPE_KIND: TypeKind = 11;
pub const POINTER_TYPE_KIND: TypeKind = 12;
pub const VECTOR_TYPE_KIND: TypeKind = 13;
pub const METADATA_TYPE_KIND: TypeKind = 14;
pub const SCALABLE_VECTOR_TYPE_KIND: TypeKind = 17;

// `LLVMLinkage`
pub const INTERNAL_LINKAGE: Linkage = 8;

// `LLVMIntPredicate`
pub const INT_EQ: IntPredicate = 32;
pub const INT_NE: IntPredicate = 33;
pub const INT_UGT: IntPredicate = 34;
pub const INT_UGE: IntPredicate = 35;
pub const INT_ULT: IntPredicate = 36;
pub const INT_ULE: IntPredicate = 37;
pub const INT_SGT: IntPredicate = 38;
pub const INT_SGE: IntPredicate = 39;
pub const INT_SLT: IntPredicate = 40;

// `LLVMRealPredicate`
pub const REAL_OEQ: RealPredicate = 1;
pub const REAL_OGT: RealPredicate = 2;
pub const REAL_OGE: RealPredicate = 3;
pub const REAL_OLT: RealPredicate = 4;
pub const REAL_UNO: RealPredicate = 8;
pub const REAL_UNE: RealPredicate = 14;

// `LLVMAtomicOrdering`
pub const ATOMIC_ORDERING_MONOTONIC: AtomicOrdering = 2;

// `LLVMAtomicRMWBinOp`
pub const ATOMIC_RMW_BIN_OP_XCHG: AtomicRMWBinOp = 0;
pub const ATOMIC_RMW_BIN_OP_ADD: AtomicRMWBinOp = 1;
pub const ATOMIC_RMW_BIN_OP_SUB: AtomicRMWBinOp = 2;
pub const ATOMIC_RMW_BIN_OP_AND: AtomicRMWBinOp = 3;
pub const ATOMIC_RMW_BIN_OP_NAND: AtomicRMWBinOp = 4;
pub const ATOMIC_RMW_BIN_OP_OR: AtomicRMWBinOp = 5;
pub const ATOMIC_RMW_BIN_OP_XOR: AtomicRMWBinOp = 6;
pub const ATOMIC_RMW_BIN_OP_MAX: AtomicRMWBinOp = 7;
pub const ATOMIC_RMW_BIN_OP_MIN: AtomicRMWBinOp = 8;
pub const ATOMIC_RMW_BIN_OP_U_MAX: AtomicRMWBinOp = 9;
pub const ATOMIC_RMW_BIN_OP_U_MIN: AtomicRMWBinOp = 10;
pub const ATOMIC_RMW_BIN_OP_F_ADD: AtomicRMWBinOp = 11;
pub const ATOMIC_RMW_BIN_OP_F_SUB: AtomicRMWBinOp = 12;
pub const ATOMIC_RMW_BIN_OP_F_MAX: AtomicRMWBinOp = 13;
pub const ATOMIC_RMW_BIN_OP_F_MIN: AtomicRMWBinOp = 14;

// `LLVMDiagnosticSeverity`
pub const DS_ERROR: DiagnosticSeverity = 0;
pub const DS_WARNING: DiagnosticSeverity = 1;
pub const DS_NOTE: DiagnosticSeverity = 3;

// `LLVMVerifierFailureAction`
pub const RETURN_STATUS_ACTION: VerifierFailureAction = 2;

unsafe extern "C" {

    // Context, diagnostics, messages
    #[link_name = "LLVMContextCreate"]
    pub fn context_create() -> ContextRef;
    #[link_name = "LLVMContextDispose"]
    pub fn context_dispose(c: ContextRef);
    #[link_name = "LLVMContextSetDiagnosticHandler"]
    pub fn context_set_diagnostic_handler(
        c: ContextRef,
        handler: DiagnosticHandler,
        diagnostic_context: *mut c_void,
    );
    #[link_name = "LLVMGetDiagInfoDescription"]
    pub fn get_diag_info_description(di: DiagnosticInfoRef) -> *mut c_char;
    #[link_name = "LLVMGetDiagInfoSeverity"]
    pub fn get_diag_info_severity(di: DiagnosticInfoRef) -> DiagnosticSeverity;
    #[link_name = "LLVMGetMDKindIDInContext"]
    pub fn get_md_kind_id_in_context(c: ContextRef, name: *const c_char, s_len: c_uint) -> c_uint;
    #[link_name = "LLVMDisposeMessage"]
    pub fn dispose_message(message: *mut c_char);

    // Attributes
    #[link_name = "LLVMGetEnumAttributeKindForName"]
    pub fn get_enum_attribute_kind_for_name(name: *const c_char, s_len: usize) -> c_uint;
    #[link_name = "LLVMCreateEnumAttribute"]
    pub fn create_enum_attribute(c: ContextRef, kind_id: c_uint, val: u64) -> AttributeRef;
    #[link_name = "LLVMCreateStringAttribute"]
    pub fn create_string_attribute(
        c: ContextRef,
        k: *const c_char,
        k_length: c_uint,
        v: *const c_char,
        v_length: c_uint,
    ) -> AttributeRef;

    // Modules
    #[link_name = "LLVMModuleCreateWithNameInContext"]
    pub fn module_create_with_name_in_context(module_id: *const c_char, c: ContextRef)
    -> ModuleRef;
    #[link_name = "LLVMDisposeModule"]
    pub fn dispose_module(m: ModuleRef);
    #[link_name = "LLVMSetSourceFileName"]
    pub fn set_source_file_name(m: ModuleRef, name: *const c_char, len: usize);
    #[link_name = "LLVMSetDataLayout"]
    pub fn set_data_layout(m: ModuleRef, data_layout_str: *const c_char);
    #[link_name = "LLVMSetTarget"]
    pub fn set_target(m: ModuleRef, triple: *const c_char);
    #[cfg(test)]
    #[link_name = "LLVMPrintModuleToString"]
    pub fn print_module_to_string(m: ModuleRef) -> *mut c_char;
    #[link_name = "LLVMGetTypeByName2"]
    pub fn get_type_by_name2(c: ContextRef, name: *const c_char) -> TypeRef;
    #[link_name = "LLVMGetNamedFunction"]
    pub fn get_named_function(m: ModuleRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMAddFunction"]
    pub fn add_function(m: ModuleRef, name: *const c_char, function_ty: TypeRef) -> ValueRef;
    #[link_name = "LLVMGetFirstFunction"]
    pub fn get_first_function(m: ModuleRef) -> ValueRef;
    #[link_name = "LLVMGetNextFunction"]
    pub fn get_next_function(fn_: ValueRef) -> ValueRef;
    #[cfg(test)]
    #[link_name = "LLVMAddGlobal"]
    pub fn add_global(m: ModuleRef, ty: TypeRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMAddGlobalInAddressSpace"]
    pub fn add_global_in_address_space(
        m: ModuleRef,
        ty: TypeRef,
        name: *const c_char,
        address_space: c_uint,
    ) -> ValueRef;
    #[link_name = "LLVMGetNamedGlobal"]
    pub fn get_named_global(m: ModuleRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMGetFirstGlobal"]
    pub fn get_first_global(m: ModuleRef) -> ValueRef;
    #[link_name = "LLVMGetNextGlobal"]
    pub fn get_next_global(global_var: ValueRef) -> ValueRef;
    #[link_name = "LLVMDeleteGlobal"]
    pub fn delete_global(global_var: ValueRef);
    #[link_name = "LLVMGetFirstNamedMetadata"]
    pub fn get_first_named_metadata(m: ModuleRef) -> NamedMDNodeRef;
    #[link_name = "LLVMGetNextNamedMetadata"]
    pub fn get_next_named_metadata(named_md_node: NamedMDNodeRef) -> NamedMDNodeRef;
    #[cfg(test)]
    #[link_name = "LLVMGetNamedMetadata"]
    pub fn get_named_metadata(m: ModuleRef, name: *const c_char, name_len: usize)
    -> NamedMDNodeRef;
    #[link_name = "LLVMGetNamedMetadataName"]
    pub fn get_named_metadata_name(named_md: NamedMDNodeRef, name_len: *mut usize)
    -> *const c_char;
    #[link_name = "LLVMGetNamedMetadataNumOperands"]
    pub fn get_named_metadata_num_operands(m: ModuleRef, name: *const c_char) -> c_uint;
    #[link_name = "LLVMGetNamedMetadataOperands"]
    pub fn get_named_metadata_operands(m: ModuleRef, name: *const c_char, dest: *mut ValueRef);
    #[link_name = "LLVMAddNamedMetadataOperand"]
    pub fn add_named_metadata_operand(m: ModuleRef, name: *const c_char, val: ValueRef);

    // Types
    #[link_name = "LLVMGetTypeKind"]
    pub fn get_type_kind(ty: TypeRef) -> TypeKind;
    #[cfg(any(test, feature = "oracle"))]
    #[link_name = "LLVMPrintTypeToString"]
    pub fn print_type_to_string(val: TypeRef) -> *mut c_char;
    #[cfg(test)]
    #[link_name = "LLVMInt1TypeInContext"]
    pub fn int1_type_in_context(c: ContextRef) -> TypeRef;
    #[cfg(test)]
    #[link_name = "LLVMInt32TypeInContext"]
    pub fn int32_type_in_context(c: ContextRef) -> TypeRef;
    #[cfg(test)]
    #[link_name = "LLVMInt64TypeInContext"]
    pub fn int64_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMIntTypeInContext"]
    pub fn int_type_in_context(c: ContextRef, num_bits: c_uint) -> TypeRef;
    #[link_name = "LLVMGetIntTypeWidth"]
    pub fn get_int_type_width(integer_ty: TypeRef) -> c_uint;
    #[link_name = "LLVMHalfTypeInContext"]
    pub fn half_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMFloatTypeInContext"]
    pub fn float_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMDoubleTypeInContext"]
    pub fn double_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMFunctionType"]
    pub fn function_type(
        return_type: TypeRef,
        param_types: *mut TypeRef,
        param_count: c_uint,
        is_var_arg: Bool,
    ) -> TypeRef;
    #[link_name = "LLVMIsFunctionVarArg"]
    pub fn is_function_var_arg(function_ty: TypeRef) -> Bool;
    #[link_name = "LLVMGetReturnType"]
    pub fn get_return_type(function_ty: TypeRef) -> TypeRef;
    #[link_name = "LLVMCountParamTypes"]
    pub fn count_param_types(function_ty: TypeRef) -> c_uint;
    #[link_name = "LLVMGetParamTypes"]
    pub fn get_param_types(function_ty: TypeRef, dest: *mut TypeRef);
    #[link_name = "LLVMStructTypeInContext"]
    pub fn struct_type_in_context(
        c: ContextRef,
        element_types: *mut TypeRef,
        element_count: c_uint,
        packed: Bool,
    ) -> TypeRef;
    #[link_name = "LLVMStructCreateNamed"]
    pub fn struct_create_named(c: ContextRef, name: *const c_char) -> TypeRef;
    #[link_name = "LLVMGetStructName"]
    pub fn get_struct_name(ty: TypeRef) -> *const c_char;
    #[link_name = "LLVMStructSetBody"]
    pub fn struct_set_body(
        struct_ty: TypeRef,
        element_types: *mut TypeRef,
        element_count: c_uint,
        packed: Bool,
    );
    #[link_name = "LLVMCountStructElementTypes"]
    pub fn count_struct_element_types(struct_ty: TypeRef) -> c_uint;
    #[link_name = "LLVMGetStructElementTypes"]
    pub fn get_struct_element_types(struct_ty: TypeRef, dest: *mut TypeRef);
    #[link_name = "LLVMIsPackedStruct"]
    pub fn is_packed_struct(struct_ty: TypeRef) -> Bool;
    #[link_name = "LLVMIsOpaqueStruct"]
    pub fn is_opaque_struct(struct_ty: TypeRef) -> Bool;
    #[link_name = "LLVMIsLiteralStruct"]
    pub fn is_literal_struct(struct_ty: TypeRef) -> Bool;
    #[link_name = "LLVMGetElementType"]
    pub fn get_element_type(ty: TypeRef) -> TypeRef;
    #[link_name = "LLVMArrayType"]
    pub fn array_type(element_type: TypeRef, element_count: c_uint) -> TypeRef;
    #[link_name = "LLVMGetArrayLength"]
    pub fn get_array_length(array_ty: TypeRef) -> c_uint;
    #[link_name = "LLVMPointerType"]
    pub fn pointer_type(element_type: TypeRef, address_space: c_uint) -> TypeRef;
    #[link_name = "LLVMPointerTypeInContext"]
    pub fn pointer_type_in_context(c: ContextRef, address_space: c_uint) -> TypeRef;
    #[link_name = "LLVMGetPointerAddressSpace"]
    pub fn get_pointer_address_space(pointer_ty: TypeRef) -> c_uint;
    #[link_name = "LLVMVectorType"]
    pub fn vector_type(element_type: TypeRef, element_count: c_uint) -> TypeRef;
    #[link_name = "LLVMGetVectorSize"]
    pub fn get_vector_size(vector_ty: TypeRef) -> c_uint;
    #[link_name = "LLVMVoidTypeInContext"]
    pub fn void_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMLabelTypeInContext"]
    pub fn label_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMMetadataTypeInContext"]
    pub fn metadata_type_in_context(c: ContextRef) -> TypeRef;

    // Values
    #[link_name = "LLVMTypeOf"]
    pub fn type_of(val: ValueRef) -> TypeRef;
    #[link_name = "LLVMGetValueName2"]
    pub fn get_value_name2(val: ValueRef, length: *mut usize) -> *const c_char;
    #[link_name = "LLVMSetValueName2"]
    pub fn set_value_name2(val: ValueRef, name: *const c_char, name_len: usize);
    #[link_name = "LLVMPrintValueToString"]
    pub fn print_value_to_string(val: ValueRef) -> *mut c_char;
    #[link_name = "LLVMReplaceAllUsesWith"]
    pub fn replace_all_uses_with(old_val: ValueRef, new_val: ValueRef);
    #[link_name = "LLVMIsAConstant"]
    pub fn is_a_constant(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantExpr"]
    pub fn is_a_constant_expr(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantInt"]
    pub fn is_a_constant_int(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAGlobalObject"]
    pub fn is_a_global_object(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFunction"]
    pub fn is_a_function(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAUndefValue"]
    pub fn is_a_undef_value(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAInstruction"]
    pub fn is_a_instruction(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsACallInst"]
    pub fn is_a_call_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAIntrinsicInst"]
    pub fn is_a_intrinsic_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFCmpInst"]
    pub fn is_af_cmp_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAGetElementPtrInst"]
    pub fn is_a_get_element_ptr_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAPHINode"]
    pub fn is_aphi_node(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAStoreInst"]
    pub fn is_a_store_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAReturnInst"]
    pub fn is_a_return_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAAllocaInst"]
    pub fn is_a_alloca_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsABitCastInst"]
    pub fn is_a_bit_cast_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAExtractValueInst"]
    pub fn is_a_extract_value_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsALoadInst"]
    pub fn is_a_load_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAAtomicCmpXchgInst"]
    pub fn is_a_atomic_cmp_xchg_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAAtomicRMWInst"]
    pub fn is_a_atomic_rmw_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAMDNode"]
    pub fn is_amd_node(val: ValueRef) -> ValueRef;

    // Uses and operands
    #[link_name = "LLVMGetFirstUse"]
    pub fn get_first_use(val: ValueRef) -> UseRef;
    #[link_name = "LLVMGetNextUse"]
    pub fn get_next_use(u: UseRef) -> UseRef;
    #[link_name = "LLVMGetUser"]
    pub fn get_user(u: UseRef) -> ValueRef;
    #[link_name = "LLVMGetNumOperands"]
    pub fn get_num_operands(val: ValueRef) -> c_int;
    #[link_name = "LLVMGetOperand"]
    pub fn get_operand(val: ValueRef, index: c_uint) -> ValueRef;
    #[link_name = "LLVMSetOperand"]
    pub fn set_operand(user: ValueRef, index: c_uint, val: ValueRef);

    // Constants
    #[link_name = "LLVMConstNull"]
    pub fn const_null(ty: TypeRef) -> ValueRef;
    #[link_name = "LLVMConstAllOnes"]
    pub fn const_all_ones(ty: TypeRef) -> ValueRef;
    #[link_name = "LLVMGetUndef"]
    pub fn get_undef(ty: TypeRef) -> ValueRef;
    #[link_name = "LLVMGetPoison"]
    pub fn get_poison(ty: TypeRef) -> ValueRef;
    #[link_name = "LLVMIsNull"]
    pub fn is_null(val: ValueRef) -> Bool;
    #[link_name = "LLVMConstInt"]
    pub fn const_int(int_ty: TypeRef, n: c_ulonglong, sign_extend: Bool) -> ValueRef;
    #[link_name = "LLVMConstReal"]
    pub fn const_real(real_ty: TypeRef, n: f64) -> ValueRef;
    #[link_name = "LLVMConstIntGetZExtValue"]
    pub fn const_int_get_z_ext_value(constant_val: ValueRef) -> c_ulonglong;
    #[link_name = "LLVMConstIntGetSExtValue"]
    pub fn const_int_get_s_ext_value(constant_val: ValueRef) -> c_longlong;
    #[link_name = "LLVMConstArray"]
    pub fn const_array(
        element_ty: TypeRef,
        constant_vals: *mut ValueRef,
        length: c_uint,
    ) -> ValueRef;
    #[link_name = "LLVMConstVector"]
    pub fn const_vector(scalar_constant_vals: *mut ValueRef, size: c_uint) -> ValueRef;
    #[link_name = "LLVMGetAggregateElement"]
    pub fn get_aggregate_element(c: ValueRef, idx: c_uint) -> ValueRef;
    #[link_name = "LLVMConstInBoundsGEP2"]
    pub fn const_in_bounds_gep2(
        ty: TypeRef,
        constant_val: ValueRef,
        constant_indices: *mut ValueRef,
        num_indices: c_uint,
    ) -> ValueRef;

    // Globals
    #[link_name = "LLVMGetGlobalParent"]
    pub fn get_global_parent(global: ValueRef) -> ModuleRef;
    #[link_name = "LLVMIsDeclaration"]
    pub fn is_declaration(global: ValueRef) -> Bool;
    #[link_name = "LLVMGetLinkage"]
    pub fn get_linkage(global: ValueRef) -> Linkage;
    #[link_name = "LLVMSetLinkage"]
    pub fn set_linkage(global: ValueRef, linkage: Linkage);
    #[link_name = "LLVMGlobalGetValueType"]
    pub fn global_get_value_type(global: ValueRef) -> TypeRef;
    #[link_name = "LLVMGetAlignment"]
    pub fn get_alignment(v: ValueRef) -> c_uint;
    #[link_name = "LLVMSetAlignment"]
    pub fn set_alignment(v: ValueRef, bytes: c_uint);
    #[link_name = "LLVMGetInitializer"]
    pub fn get_initializer(global_var: ValueRef) -> ValueRef;
    #[link_name = "LLVMSetInitializer"]
    pub fn set_initializer(global_var: ValueRef, constant_val: ValueRef);
    #[link_name = "LLVMIsGlobalConstant"]
    pub fn is_global_constant(global_var: ValueRef) -> Bool;
    #[link_name = "LLVMSetGlobalConstant"]
    pub fn set_global_constant(global_var: ValueRef, is_constant: Bool);

    // Functions and parameters
    #[link_name = "LLVMDeleteFunction"]
    pub fn delete_function(fn_: ValueRef);
    #[link_name = "LLVMGetIntrinsicID"]
    pub fn get_intrinsic_id(fn_: ValueRef) -> c_uint;
    #[link_name = "LLVMLookupIntrinsicID"]
    pub fn lookup_intrinsic_id(name: *const c_char, name_len: usize) -> c_uint;
    #[link_name = "LLVMGetIntrinsicDeclaration"]
    pub fn get_intrinsic_declaration(
        mod_: ModuleRef,
        id: c_uint,
        param_types: *mut TypeRef,
        param_count: usize,
    ) -> ValueRef;
    #[link_name = "LLVMIntrinsicGetType"]
    pub fn intrinsic_get_type(
        ctx: ContextRef,
        id: c_uint,
        param_types: *mut TypeRef,
        param_count: usize,
    ) -> TypeRef;
    #[link_name = "LLVMAddAttributeAtIndex"]
    pub fn add_attribute_at_index(f: ValueRef, idx: AttributeIndex, a: AttributeRef);
    #[link_name = "LLVMGetStringAttributeAtIndex"]
    pub fn get_string_attribute_at_index(
        f: ValueRef,
        idx: AttributeIndex,
        k: *const c_char,
        k_len: c_uint,
    ) -> AttributeRef;
    #[link_name = "LLVMRemoveStringAttributeAtIndex"]
    pub fn remove_string_attribute_at_index(
        f: ValueRef,
        idx: AttributeIndex,
        k: *const c_char,
        k_len: c_uint,
    );
    #[link_name = "LLVMCountParams"]
    pub fn count_params(fn_: ValueRef) -> c_uint;
    #[link_name = "LLVMGetParam"]
    pub fn get_param(fn_: ValueRef, index: c_uint) -> ValueRef;

    // Basic blocks
    #[link_name = "LLVMGetBasicBlockParent"]
    pub fn get_basic_block_parent(bb: BasicBlockRef) -> ValueRef;
    #[link_name = "LLVMGetBasicBlockTerminator"]
    pub fn get_basic_block_terminator(bb: BasicBlockRef) -> ValueRef;
    #[link_name = "LLVMGetFirstBasicBlock"]
    pub fn get_first_basic_block(fn_: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetLastBasicBlock"]
    pub fn get_last_basic_block(fn_: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetNextBasicBlock"]
    pub fn get_next_basic_block(bb: BasicBlockRef) -> BasicBlockRef;
    #[link_name = "LLVMGetEntryBasicBlock"]
    pub fn get_entry_basic_block(fn_: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMAppendBasicBlockInContext"]
    pub fn append_basic_block_in_context(
        c: ContextRef,
        fn_: ValueRef,
        name: *const c_char,
    ) -> BasicBlockRef;
    #[link_name = "LLVMInsertBasicBlockInContext"]
    pub fn insert_basic_block_in_context(
        c: ContextRef,
        bb: BasicBlockRef,
        name: *const c_char,
    ) -> BasicBlockRef;
    #[link_name = "LLVMMoveBasicBlockAfter"]
    pub fn move_basic_block_after(bb: BasicBlockRef, move_pos: BasicBlockRef);
    #[link_name = "LLVMGetFirstInstruction"]
    pub fn get_first_instruction(bb: BasicBlockRef) -> ValueRef;

    // Instructions
    #[link_name = "LLVMHasMetadata"]
    pub fn has_metadata(val: ValueRef) -> c_int;
    #[link_name = "LLVMGetMetadata"]
    pub fn get_metadata(val: ValueRef, kind_id: c_uint) -> ValueRef;
    #[link_name = "LLVMSetMetadata"]
    pub fn set_metadata(val: ValueRef, kind_id: c_uint, node: ValueRef);
    #[link_name = "LLVMInstructionGetAllMetadataOtherThanDebugLoc"]
    pub fn instruction_get_all_metadata_other_than_debug_loc(
        instr: ValueRef,
        num_entries: *mut usize,
    ) -> *mut ValueMetadataEntry;
    #[link_name = "LLVMValueMetadataEntriesGetKind"]
    pub fn value_metadata_entries_get_kind(
        entries: *mut ValueMetadataEntry,
        index: c_uint,
    ) -> c_uint;
    #[link_name = "LLVMDisposeValueMetadataEntries"]
    pub fn dispose_value_metadata_entries(entries: *mut ValueMetadataEntry);
    #[link_name = "LLVMGetInstructionParent"]
    pub fn get_instruction_parent(inst: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetNextInstruction"]
    pub fn get_next_instruction(inst: ValueRef) -> ValueRef;
    #[link_name = "LLVMInstructionEraseFromParent"]
    pub fn instruction_erase_from_parent(inst: ValueRef);
    #[link_name = "LLVMGetNumArgOperands"]
    pub fn get_num_arg_operands(instr: ValueRef) -> c_uint;
    #[link_name = "LLVMAddCallSiteAttribute"]
    pub fn add_call_site_attribute(c: ValueRef, idx: AttributeIndex, a: AttributeRef);
    #[link_name = "LLVMGetCalledFunctionType"]
    pub fn get_called_function_type(c: ValueRef) -> TypeRef;
    #[link_name = "LLVMGetCalledValue"]
    pub fn get_called_value(instr: ValueRef) -> ValueRef;
    #[link_name = "LLVMSetSuccessor"]
    pub fn set_successor(term: ValueRef, i: c_uint, block: BasicBlockRef);
    #[link_name = "LLVMIsInBounds"]
    pub fn is_in_bounds(gep: ValueRef) -> Bool;
    #[link_name = "LLVMGetGEPSourceElementType"]
    pub fn get_gep_source_element_type(gep: ValueRef) -> TypeRef;
    #[link_name = "LLVMAddIncoming"]
    pub fn add_incoming(
        phi_node: ValueRef,
        incoming_values: *mut ValueRef,
        incoming_blocks: *mut BasicBlockRef,
        count: c_uint,
    );
    #[link_name = "LLVMGetNumIndices"]
    pub fn get_num_indices(inst: ValueRef) -> c_uint;
    #[link_name = "LLVMGetIndices"]
    pub fn get_indices(inst: ValueRef) -> *const c_uint;
    #[link_name = "LLVMSetVolatile"]
    pub fn set_volatile(memory_access_inst: ValueRef, is_volatile: Bool);
    #[link_name = "LLVMSetOrdering"]
    pub fn set_ordering(memory_access_inst: ValueRef, ordering: AtomicOrdering);
    #[link_name = "LLVMGetAtomicRMWBinOp"]
    pub fn get_atomic_rmw_bin_op(atomic_rmw_inst: ValueRef) -> AtomicRMWBinOp;

    // The builder
    #[link_name = "LLVMCreateBuilderInContext"]
    pub fn create_builder_in_context(c: ContextRef) -> BuilderRef;
    #[link_name = "LLVMDisposeBuilder"]
    pub fn dispose_builder(builder: BuilderRef);
    #[link_name = "LLVMPositionBuilderBefore"]
    pub fn position_builder_before(builder: BuilderRef, instr: ValueRef);
    #[link_name = "LLVMPositionBuilderAtEnd"]
    pub fn position_builder_at_end(builder: BuilderRef, block: BasicBlockRef);
    #[link_name = "LLVMGetInsertBlock"]
    pub fn get_insert_block(builder: BuilderRef) -> BasicBlockRef;
    #[link_name = "LLVMBuildRetVoid"]
    pub fn build_ret_void(b: BuilderRef) -> ValueRef;
    #[link_name = "LLVMBuildRet"]
    pub fn build_ret(b: BuilderRef, v: ValueRef) -> ValueRef;
    #[link_name = "LLVMBuildBr"]
    pub fn build_br(b: BuilderRef, dest: BasicBlockRef) -> ValueRef;
    #[link_name = "LLVMBuildCondBr"]
    pub fn build_cond_br(
        b: BuilderRef,
        if_: ValueRef,
        then: BasicBlockRef,
        else_: BasicBlockRef,
    ) -> ValueRef;
    #[link_name = "LLVMBuildSwitch"]
    pub fn build_switch(
        b: BuilderRef,
        v: ValueRef,
        else_: BasicBlockRef,
        num_cases: c_uint,
    ) -> ValueRef;
    #[link_name = "LLVMAddCase"]
    pub fn add_case(switch: ValueRef, on_val: ValueRef, dest: BasicBlockRef);
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildUnreachable"]
    pub fn build_unreachable(b: BuilderRef) -> ValueRef;
    #[link_name = "LLVMBuildAdd"]
    pub fn build_add(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildFAdd"]
    pub fn build_f_add(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildSub"]
    pub fn build_sub(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildFSub"]
    pub fn build_f_sub(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildMul"]
    pub fn build_mul(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildFMul"]
    pub fn build_f_mul(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildUDiv"]
    pub fn build_u_div(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildSDiv"]
    pub fn build_s_div(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFDiv"]
    pub fn build_f_div(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildURem"]
    pub fn build_u_rem(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildSRem"]
    pub fn build_s_rem(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildFRem"]
    pub fn build_f_rem(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildShl"]
    pub fn build_shl(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildLShr"]
    pub fn build_l_shr(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildAShr"]
    pub fn build_a_shr(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildAnd"]
    pub fn build_and(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildOr"]
    pub fn build_or(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildXor"]
    pub fn build_xor(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildBinOp"]
    pub fn build_bin_op(
        b: BuilderRef,
        op: Opcode,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildNeg"]
    pub fn build_neg(b: BuilderRef, v: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildFNeg"]
    pub fn build_f_neg(b: BuilderRef, v: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildNot"]
    pub fn build_not(b: BuilderRef, v: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildAlloca"]
    pub fn build_alloca(b: BuilderRef, ty: TypeRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildLoad2"]
    pub fn build_load2(
        b: BuilderRef,
        ty: TypeRef,
        pointer_val: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildStore"]
    pub fn build_store(b: BuilderRef, val: ValueRef, ptr: ValueRef) -> ValueRef;
    #[link_name = "LLVMBuildGEP2"]
    pub fn build_gep2(
        b: BuilderRef,
        ty: TypeRef,
        pointer: ValueRef,
        indices: *mut ValueRef,
        num_indices: c_uint,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildInBoundsGEP2"]
    pub fn build_in_bounds_gep2(
        b: BuilderRef,
        ty: TypeRef,
        pointer: ValueRef,
        indices: *mut ValueRef,
        num_indices: c_uint,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildStructGEP2"]
    pub fn build_struct_gep2(
        b: BuilderRef,
        ty: TypeRef,
        pointer: ValueRef,
        idx: c_uint,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildTrunc"]
    pub fn build_trunc(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildZExt"]
    pub fn build_z_ext(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildSExt"]
    pub fn build_s_ext(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildFPToUI"]
    pub fn build_fp_to_ui(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildFPToSI"]
    pub fn build_fp_to_si(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildUIToFP"]
    pub fn build_ui_to_fp(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildSIToFP"]
    pub fn build_si_to_fp(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFPTrunc"]
    pub fn build_fp_trunc(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFPExt"]
    pub fn build_fp_ext(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildPtrToInt"]
    pub fn build_ptr_to_int(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildIntToPtr"]
    pub fn build_int_to_ptr(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildBitCast"]
    pub fn build_bit_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildAddrSpaceCast"]
    pub fn build_addr_space_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildCast"]
    pub fn build_cast(
        b: BuilderRef,
        op: Opcode,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildPointerCast"]
    pub fn build_pointer_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildIntCast2"]
    pub fn build_int_cast2(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        is_signed: Bool,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFPCast"]
    pub fn build_fp_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildICmp"]
    pub fn build_i_cmp(
        b: BuilderRef,
        op: IntPredicate,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFCmp"]
    pub fn build_f_cmp(
        b: BuilderRef,
        op: RealPredicate,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildPhi"]
    pub fn build_phi(b: BuilderRef, ty: TypeRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildCall2"]
    pub fn build_call2(
        b: BuilderRef,
        ty: TypeRef,
        fn_: ValueRef,
        args: *mut ValueRef,
        num_args: c_uint,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildSelect"]
    pub fn build_select(
        b: BuilderRef,
        if_: ValueRef,
        then: ValueRef,
        else_: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildExtractElement"]
    pub fn build_extract_element(
        b: BuilderRef,
        vec_val: ValueRef,
        index: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildInsertElement"]
    pub fn build_insert_element(
        b: BuilderRef,
        vec_val: ValueRef,
        elt_val: ValueRef,
        index: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildShuffleVector"]
    pub fn build_shuffle_vector(
        b: BuilderRef,
        v1: ValueRef,
        v2: ValueRef,
        mask: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildExtractValue"]
    pub fn build_extract_value(
        b: BuilderRef,
        agg_val: ValueRef,
        index: c_uint,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildInsertValue"]
    pub fn build_insert_value(
        b: BuilderRef,
        agg_val: ValueRef,
        elt_val: ValueRef,
        index: c_uint,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFreeze"]
    pub fn build_freeze(b: BuilderRef, val: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildIsNull"]
    pub fn build_is_null(b: BuilderRef, val: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildIsNotNull"]
    pub fn build_is_not_null(b: BuilderRef, val: ValueRef, name: *const c_char) -> ValueRef;
    #[cfg(all(test, feature = "oracle"))]
    #[link_name = "LLVMBuildAtomicRMW"]
    pub fn build_atomic_rmw(
        b: BuilderRef,
        op: AtomicRMWBinOp,
        ptr: ValueRef,
        val: ValueRef,
        ordering: AtomicOrdering,
        single_thread: Bool,
    ) -> ValueRef;
    #[link_name = "LLVMBuildAtomicCmpXchg"]
    pub fn build_atomic_cmp_xchg(
        b: BuilderRef,
        ptr: ValueRef,
        cmp: ValueRef,
        new: ValueRef,
        success_ordering: AtomicOrdering,
        failure_ordering: AtomicOrdering,
        single_thread: Bool,
    ) -> ValueRef;

    // Memory buffers
    #[link_name = "LLVMGetBufferStart"]
    pub fn get_buffer_start(mem_buf: MemoryBufferRef) -> *const c_char;
    #[link_name = "LLVMGetBufferSize"]
    pub fn get_buffer_size(mem_buf: MemoryBufferRef) -> usize;
    #[link_name = "LLVMDisposeMemoryBuffer"]
    pub fn dispose_memory_buffer(mem_buf: MemoryBufferRef);

    // Metadata
    #[link_name = "LLVMMDStringInContext2"]
    pub fn md_string_in_context2(c: ContextRef, str: *const c_char, s_len: usize) -> MetadataRef;
    #[link_name = "LLVMMDNodeInContext2"]
    pub fn md_node_in_context2(c: ContextRef, m_ds: *mut MetadataRef, count: usize) -> MetadataRef;
    #[link_name = "LLVMMetadataAsValue"]
    pub fn metadata_as_value(c: ContextRef, md: MetadataRef) -> ValueRef;
    #[link_name = "LLVMValueAsMetadata"]
    pub fn value_as_metadata(val: ValueRef) -> MetadataRef;
    #[link_name = "LLVMGetMDString"]
    pub fn get_md_string(v: ValueRef, length: *mut c_uint) -> *const c_char;
    #[link_name = "LLVMGetMDNodeNumOperands"]
    pub fn get_md_node_num_operands(v: ValueRef) -> c_uint;
    #[link_name = "LLVMGetMDNodeOperands"]
    pub fn get_md_node_operands(v: ValueRef, dest: *mut ValueRef);

    // The verifier
    #[link_name = "LLVMVerifyModule"]
    pub fn verify_module(
        m: ModuleRef,
        action: VerifierFailureAction,
        out_message: *mut *mut c_char,
    ) -> Bool;

    // The linker (the shim links the helpers; this form, with no flag, is for tests)
    #[cfg(test)]
    #[link_name = "LLVMLinkModules2"]
    pub fn link_modules2(dest: ModuleRef, src: ModuleRef) -> Bool;

    // The AIR bitcode writer (`llvm-c/AIRWriter.h`)
    #[link_name = "LLVMAIRTypedPointerType"]
    pub fn air_typed_pointer_type(element_type: TypeRef, address_space: c_uint) -> TypeRef;
    #[link_name = "LLVMWriteAIRBitcode"]
    pub fn write_air_bitcode(
        m: ModuleRef,
        types: *const AirTypes,
        out_buf: *mut MemoryBufferRef,
        stats: *mut AirWriteStats,
        out_error: *mut *mut c_char,
    ) -> Bool;
}

// The shim (`shim/shim.cpp`): what LLVM 23's C API lacks. Each is `extern "C"` in C++ built
// without exceptions, so nothing unwinds out of it.
unsafe extern "C" {
    /// Links LLVM's core and the barrier pass through the pass's own declaration: answers 1.
    #[cfg(test)]
    pub fn airconv_shim_probe() -> c_int;

    /// Sets the builder's fast-math flags (`IRBuilder::setFastMathFlags`).
    ///
    /// Every floating-point instruction it makes from now on carries them; `flags` holds
    /// `FastMathFlags`' bits.
    pub fn airconv_shim_set_fast_math_flags(builder: BuilderRef, flags: c_uint);
    /// The fast-math flags of `inst` (`FPMathOperator::getFastMathFlags`).
    ///
    /// 0 for a value that is not a floating-point operation.
    pub fn airconv_shim_get_fast_math_flags(inst: ValueRef) -> c_uint;
    /// Replaces the fast-math flags of `inst` (`Instruction::copyFastMathFlags`).
    ///
    /// A value that is not a floating-point operation is left alone.
    pub fn airconv_shim_copy_fast_math_flags(inst: ValueRef, flags: c_uint);
    /// Whether `value` carries fast-math flags (`isa<FPMathOperator>`).
    pub fn airconv_shim_is_fp_math_operator(value: ValueRef) -> Bool;

    /// The bitcode of `module` with its module hash, in a buffer the caller disposes.
    ///
    /// `WriteBitcodeToFile(M, OS, false, nullptr, true)`.
    pub fn airconv_shim_write_bitcode(module: ModuleRef) -> MemoryBufferRef;
    /// Parses `size` bytes of bitcode into a module of `context` (`parseBitcodeFile`).
    ///
    /// The buffer is named by `name`; the module is read in full. When `dxil_layout` is non-zero,
    /// its `DataLayout` callback drops the `i8:32` component. Answers 0 and the module in `*out`,
    /// or 1 and `toString` of the error in `*error` (`*error_len` bytes, freed with
    /// `dispose_message`).
    pub fn airconv_shim_parse_bitcode(
        context: ContextRef,
        data: *const c_char,
        size: usize,
        name: *const c_char,
        name_len: usize,
        dxil_layout: Bool,
        out: *mut ModuleRef,
        error: *mut *mut c_char,
        error_len: *mut usize,
    ) -> Bool;
    /// Links `src`, which it takes, into `dest`: only what `dest` uses.
    ///
    /// `Linker::linkModules` with `LinkOnlyNeeded`; answers 1 on an error, which the context's
    /// handler has seen.
    pub fn airconv_shim_link_only_needed(dest: ModuleRef, src: ModuleRef) -> Bool;
    /// Erases the named metadata `name` of `module` if it has it.
    pub fn airconv_shim_erase_named_metadata(
        module: ModuleRef,
        name: *const c_char,
        name_len: usize,
    );
    /// Runs airconv's pass pipeline on `module`, its two passes included.
    pub fn airconv_shim_run_passes(module: ModuleRef);
    /// Prints the pass pipeline (`ModulePassManager::printPipeline`) to `*out` (`*len` bytes).
    ///
    /// The buffer is freed with `dispose_message` and is built with or without the two passes.
    #[cfg(test)]
    pub fn airconv_shim_pipeline_text(air_passes: c_int, out: *mut *mut c_char, len: *mut usize);
    /// `Attribute::getWithMemoryEffects` for the given location mask and `ModRefInfo`.
    ///
    /// `argmem_only` selects the location mask (`0`: all, `1`: `ArgMem`, `2`: `InaccessibleMem`,
    /// `3`: both) and `mod_ref` is `ModRefInfo` (`0`: none, `1`: `Ref`, `2`: `Mod`, `3`: `ModRef`).
    pub fn airconv_shim_memory_attribute(
        context: ContextRef,
        argmem_only: c_int,
        mod_ref: c_int,
    ) -> AttributeRef;
    /// `Attribute::getWithCaptureInfo(CaptureInfo::none())`: `captures(none)`.
    pub fn airconv_shim_captures_none(context: ContextRef) -> AttributeRef;
    /// Sets the module's SDK version (`Module::setSDKVersion`).
    pub fn airconv_shim_set_sdk_version(module: ModuleRef, major: c_uint, minor: c_uint);
    /// Declares (or finds) `air.simdgroup.barrier` in `module` as the barrier pass does.
    ///
    /// Answers the function.
    #[cfg(test)]
    pub fn airconv_shim_declare_simdgroup_barrier(module: ModuleRef) -> ValueRef;

    /// Splits `block` before `before` (`BasicBlock::splitBasicBlock`).
    ///
    /// A null `before` splits at the end. Answers the new block.
    pub fn airconv_shim_split_block(block: BasicBlockRef, before: ValueRef) -> BasicBlockRef;
    /// `SplitBlockAndInsertIfThen(cond, split_before, unreachable)`.
    ///
    /// Answers the terminator of the new conditional block.
    pub fn airconv_shim_split_block_and_insert_if_then(
        cond: ValueRef,
        split_before: ValueRef,
        unreachable: Bool,
    ) -> ValueRef;
    /// The constant expression `expr` as an instruction (`ConstantExpr::getAsInstruction`).
    ///
    /// It is inserted before `insert_before`.
    pub fn airconv_shim_get_as_instruction(expr: ValueRef, insert_before: ValueRef) -> ValueRef;
    /// Destroys the constant `constant`, which nothing uses (`Constant::destroyConstant`).
    pub fn airconv_shim_destroy_constant(constant: ValueRef);
    /// Renames the identified structs of `module` named `from…` to `to…` (`setName`).
    ///
    /// The rest of each name, after `from`, follows `to`.
    pub fn airconv_shim_rename_struct(
        module: ModuleRef,
        from: *const c_char,
        from_len: usize,
        to: *const c_char,
        to_len: usize,
    );
    /// `buildAtomicRMWValue`: the value `op` stores, from `loaded` and `value`.
    ///
    /// It is built at the builder's position.
    pub fn airconv_shim_build_atomic_rmw_value(
        builder: BuilderRef,
        op: AtomicRMWBinOp,
        loaded: ValueRef,
        value: ValueRef,
    ) -> ValueRef;
    /// The instruction of `function` whose local slot number is `slot`, or null.
    ///
    /// The number is `ModuleSlotTracker`'s, as the printer numbers unnamed values.
    pub fn airconv_shim_local_slot(function: ValueRef, slot: c_uint) -> ValueRef;
}
