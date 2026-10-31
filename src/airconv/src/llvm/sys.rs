//! The LLVM 15 C API and the shim's functions, declared by hand with their C signatures.
//!
//! Each declaration is the prototype of `llvm-c/{Core,Analysis,Linker}.h` in the install
//! `AIRCONV_LLVM` names, under its snake-case name with the C name as `link_name` (`LLVMBuildGEP2`
//! is `build_gep2`); enumerators are constants named the same way (`LLVMIntEQ` is `INT_EQ`).
//! Only the deprecated forms are left out: the typed `2` forms (`LLVMBuildLoad2`, …) and the
//! length-taking ones (`LLVMSetValueName2`, `LLVMMDStringInContext2`) are what the translator
//! calls. The list is every function a later part of the translator uses; one that is missing is
//! added here, never declared where it is called. The shim's functions (`shim/shim.cpp`, named
//! `airconv_shim_*`) cover what this API lacks on LLVM 15.

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
/// `LLVMUnnamedAddr`.
pub type UnnamedAddr = c_uint;
/// `LLVMValueKind`.
pub type ValueKind = c_uint;
/// `LLVMIntPredicate`.
pub type IntPredicate = c_uint;
/// `LLVMRealPredicate`.
pub type RealPredicate = c_uint;
/// `LLVMThreadLocalMode`.
pub type ThreadLocalMode = c_uint;
/// `LLVMAtomicOrdering`.
pub type AtomicOrdering = c_uint;
/// `LLVMAtomicRMWBinOp`.
pub type AtomicRMWBinOp = c_uint;
/// `LLVMDiagnosticSeverity`.
pub type DiagnosticSeverity = c_uint;
/// `LLVMModuleFlagBehavior`.
pub type ModuleFlagBehavior = c_uint;
/// `LLVMVerifierFailureAction`: only `RETURN_STATUS_ACTION` neither prints nor aborts.
pub type VerifierFailureAction = c_uint;
/// `LLVMAttributeIndex`: the return value, the function, or a parameter from 1.
pub type AttributeIndex = c_uint;
/// `LLVMDiagnosticHandler`.
pub type DiagnosticHandler = Option<unsafe extern "C" fn(DiagnosticInfoRef, *mut c_void)>;

/// `LLVMAttributeReturnIndex`.
pub const ATTRIBUTE_RETURN_INDEX: AttributeIndex = 0;
/// `LLVMAttributeFunctionIndex` (-1 as an unsigned).
pub const ATTRIBUTE_FUNCTION_INDEX: AttributeIndex = AttributeIndex::MAX;

// `LLVMOpcode`
pub const RET: Opcode = 1;
pub const BR: Opcode = 2;
pub const SWITCH: Opcode = 3;
pub const INDIRECT_BR: Opcode = 4;
pub const INVOKE: Opcode = 5;
pub const UNREACHABLE: Opcode = 7;
pub const CALL_BR: Opcode = 67;
pub const F_NEG: Opcode = 66;
pub const ADD: Opcode = 8;
pub const F_ADD: Opcode = 9;
pub const SUB: Opcode = 10;
pub const F_SUB: Opcode = 11;
pub const MUL: Opcode = 12;
pub const F_MUL: Opcode = 13;
pub const U_DIV: Opcode = 14;
pub const S_DIV: Opcode = 15;
pub const F_DIV: Opcode = 16;
pub const U_REM: Opcode = 17;
pub const S_REM: Opcode = 18;
pub const F_REM: Opcode = 19;
pub const SHL: Opcode = 20;
pub const L_SHR: Opcode = 21;
pub const A_SHR: Opcode = 22;
pub const AND: Opcode = 23;
pub const OR: Opcode = 24;
pub const XOR: Opcode = 25;
pub const ALLOCA: Opcode = 26;
pub const LOAD: Opcode = 27;
pub const STORE: Opcode = 28;
pub const GET_ELEMENT_PTR: Opcode = 29;
pub const TRUNC: Opcode = 30;
pub const Z_EXT: Opcode = 31;
pub const S_EXT: Opcode = 32;
pub const FP_TO_UI: Opcode = 33;
pub const FP_TO_SI: Opcode = 34;
pub const UI_TO_FP: Opcode = 35;
pub const SI_TO_FP: Opcode = 36;
pub const FP_TRUNC: Opcode = 37;
pub const FP_EXT: Opcode = 38;
pub const PTR_TO_INT: Opcode = 39;
pub const INT_TO_PTR: Opcode = 40;
pub const BIT_CAST: Opcode = 41;
pub const ADDR_SPACE_CAST: Opcode = 60;
pub const I_CMP: Opcode = 42;
pub const F_CMP: Opcode = 43;
pub const PHI: Opcode = 44;
pub const CALL: Opcode = 45;
pub const SELECT: Opcode = 46;
pub const USER_OP1: Opcode = 47;
pub const USER_OP2: Opcode = 48;
pub const VA_ARG: Opcode = 49;
pub const EXTRACT_ELEMENT: Opcode = 50;
pub const INSERT_ELEMENT: Opcode = 51;
pub const SHUFFLE_VECTOR: Opcode = 52;
pub const EXTRACT_VALUE: Opcode = 53;
pub const INSERT_VALUE: Opcode = 54;
pub const FREEZE: Opcode = 68;
pub const FENCE: Opcode = 55;
pub const ATOMIC_CMP_XCHG: Opcode = 56;
pub const ATOMIC_RMW: Opcode = 57;
pub const RESUME: Opcode = 58;
pub const LANDING_PAD: Opcode = 59;
pub const CLEANUP_RET: Opcode = 61;
pub const CATCH_RET: Opcode = 62;
pub const CATCH_PAD: Opcode = 63;
pub const CLEANUP_PAD: Opcode = 64;
pub const CATCH_SWITCH: Opcode = 65;

// `LLVMTypeKind`
pub const VOID_TYPE_KIND: TypeKind = 0;
pub const HALF_TYPE_KIND: TypeKind = 1;
pub const FLOAT_TYPE_KIND: TypeKind = 2;
pub const DOUBLE_TYPE_KIND: TypeKind = 3;
pub const X86_FP80_TYPE_KIND: TypeKind = 4;
pub const FP128_TYPE_KIND: TypeKind = 5;
pub const PPC_FP128_TYPE_KIND: TypeKind = 6;
pub const LABEL_TYPE_KIND: TypeKind = 7;
pub const INTEGER_TYPE_KIND: TypeKind = 8;
pub const FUNCTION_TYPE_KIND: TypeKind = 9;
pub const STRUCT_TYPE_KIND: TypeKind = 10;
pub const ARRAY_TYPE_KIND: TypeKind = 11;
pub const POINTER_TYPE_KIND: TypeKind = 12;
pub const VECTOR_TYPE_KIND: TypeKind = 13;
pub const METADATA_TYPE_KIND: TypeKind = 14;
pub const X86_MMX_TYPE_KIND: TypeKind = 15;
pub const TOKEN_TYPE_KIND: TypeKind = 16;
pub const SCALABLE_VECTOR_TYPE_KIND: TypeKind = 17;
pub const B_FLOAT_TYPE_KIND: TypeKind = 18;
pub const X86_AMX_TYPE_KIND: TypeKind = 19;

// `LLVMLinkage`
pub const EXTERNAL_LINKAGE: Linkage = 0;
pub const AVAILABLE_EXTERNALLY_LINKAGE: Linkage = 1;
pub const LINK_ONCE_ANY_LINKAGE: Linkage = 2;
pub const LINK_ONCE_ODR_LINKAGE: Linkage = 3;
pub const LINK_ONCE_ODR_AUTO_HIDE_LINKAGE: Linkage = 4;
pub const WEAK_ANY_LINKAGE: Linkage = 5;
pub const WEAK_ODR_LINKAGE: Linkage = 6;
pub const APPENDING_LINKAGE: Linkage = 7;
pub const INTERNAL_LINKAGE: Linkage = 8;
pub const PRIVATE_LINKAGE: Linkage = 9;
pub const DLL_IMPORT_LINKAGE: Linkage = 10;
pub const DLL_EXPORT_LINKAGE: Linkage = 11;
pub const EXTERNAL_WEAK_LINKAGE: Linkage = 12;
pub const GHOST_LINKAGE: Linkage = 13;
pub const COMMON_LINKAGE: Linkage = 14;
pub const LINKER_PRIVATE_LINKAGE: Linkage = 15;
pub const LINKER_PRIVATE_WEAK_LINKAGE: Linkage = 16;

// `LLVMUnnamedAddr`
pub const NO_UNNAMED_ADDR: UnnamedAddr = 0;
pub const LOCAL_UNNAMED_ADDR: UnnamedAddr = 1;
pub const GLOBAL_UNNAMED_ADDR: UnnamedAddr = 2;

// `LLVMValueKind`
pub const ARGUMENT_VALUE_KIND: ValueKind = 0;
pub const BASIC_BLOCK_VALUE_KIND: ValueKind = 1;
pub const MEMORY_USE_VALUE_KIND: ValueKind = 2;
pub const MEMORY_DEF_VALUE_KIND: ValueKind = 3;
pub const MEMORY_PHI_VALUE_KIND: ValueKind = 4;
pub const FUNCTION_VALUE_KIND: ValueKind = 5;
pub const GLOBAL_ALIAS_VALUE_KIND: ValueKind = 6;
pub const GLOBAL_I_FUNC_VALUE_KIND: ValueKind = 7;
pub const GLOBAL_VARIABLE_VALUE_KIND: ValueKind = 8;
pub const BLOCK_ADDRESS_VALUE_KIND: ValueKind = 9;
pub const CONSTANT_EXPR_VALUE_KIND: ValueKind = 10;
pub const CONSTANT_ARRAY_VALUE_KIND: ValueKind = 11;
pub const CONSTANT_STRUCT_VALUE_KIND: ValueKind = 12;
pub const CONSTANT_VECTOR_VALUE_KIND: ValueKind = 13;
pub const UNDEF_VALUE_VALUE_KIND: ValueKind = 14;
pub const CONSTANT_AGGREGATE_ZERO_VALUE_KIND: ValueKind = 15;
pub const CONSTANT_DATA_ARRAY_VALUE_KIND: ValueKind = 16;
pub const CONSTANT_DATA_VECTOR_VALUE_KIND: ValueKind = 17;
pub const CONSTANT_INT_VALUE_KIND: ValueKind = 18;
pub const CONSTANT_FP_VALUE_KIND: ValueKind = 19;
pub const CONSTANT_POINTER_NULL_VALUE_KIND: ValueKind = 20;
pub const CONSTANT_TOKEN_NONE_VALUE_KIND: ValueKind = 21;
pub const METADATA_AS_VALUE_VALUE_KIND: ValueKind = 22;
pub const INLINE_ASM_VALUE_KIND: ValueKind = 23;
pub const INSTRUCTION_VALUE_KIND: ValueKind = 24;
pub const POISON_VALUE_VALUE_KIND: ValueKind = 25;

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
pub const INT_SLE: IntPredicate = 41;

// `LLVMRealPredicate`
pub const REAL_PREDICATE_FALSE: RealPredicate = 0;
pub const REAL_OEQ: RealPredicate = 1;
pub const REAL_OGT: RealPredicate = 2;
pub const REAL_OGE: RealPredicate = 3;
pub const REAL_OLT: RealPredicate = 4;
pub const REAL_OLE: RealPredicate = 5;
pub const REAL_ONE: RealPredicate = 6;
pub const REAL_ORD: RealPredicate = 7;
pub const REAL_UNO: RealPredicate = 8;
pub const REAL_UEQ: RealPredicate = 9;
pub const REAL_UGT: RealPredicate = 10;
pub const REAL_UGE: RealPredicate = 11;
pub const REAL_ULT: RealPredicate = 12;
pub const REAL_ULE: RealPredicate = 13;
pub const REAL_UNE: RealPredicate = 14;
pub const REAL_PREDICATE_TRUE: RealPredicate = 15;

// `LLVMThreadLocalMode`
pub const NOT_THREAD_LOCAL: ThreadLocalMode = 0;
pub const GENERAL_DYNAMIC_TLS_MODEL: ThreadLocalMode = 1;
pub const LOCAL_DYNAMIC_TLS_MODEL: ThreadLocalMode = 2;
pub const INITIAL_EXEC_TLS_MODEL: ThreadLocalMode = 3;
pub const LOCAL_EXEC_TLS_MODEL: ThreadLocalMode = 4;

// `LLVMAtomicOrdering`
pub const ATOMIC_ORDERING_NOT_ATOMIC: AtomicOrdering = 0;
pub const ATOMIC_ORDERING_UNORDERED: AtomicOrdering = 1;
pub const ATOMIC_ORDERING_MONOTONIC: AtomicOrdering = 2;
pub const ATOMIC_ORDERING_ACQUIRE: AtomicOrdering = 4;
pub const ATOMIC_ORDERING_RELEASE: AtomicOrdering = 5;
pub const ATOMIC_ORDERING_ACQUIRE_RELEASE: AtomicOrdering = 6;
pub const ATOMIC_ORDERING_SEQUENTIALLY_CONSISTENT: AtomicOrdering = 7;

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
pub const DS_REMARK: DiagnosticSeverity = 2;
pub const DS_NOTE: DiagnosticSeverity = 3;

// `LLVMModuleFlagBehavior`
pub const MODULE_FLAG_BEHAVIOR_ERROR: ModuleFlagBehavior = 0;
pub const MODULE_FLAG_BEHAVIOR_WARNING: ModuleFlagBehavior = 1;
pub const MODULE_FLAG_BEHAVIOR_REQUIRE: ModuleFlagBehavior = 2;
pub const MODULE_FLAG_BEHAVIOR_OVERRIDE: ModuleFlagBehavior = 3;
pub const MODULE_FLAG_BEHAVIOR_APPEND: ModuleFlagBehavior = 4;
pub const MODULE_FLAG_BEHAVIOR_APPEND_UNIQUE: ModuleFlagBehavior = 5;

// `LLVMVerifierFailureAction`
pub const ABORT_PROCESS_ACTION: VerifierFailureAction = 0;
pub const PRINT_MESSAGE_ACTION: VerifierFailureAction = 1;
pub const RETURN_STATUS_ACTION: VerifierFailureAction = 2;

unsafe extern "C" {

    // Context, diagnostics, messages
    #[link_name = "LLVMContextCreate"]
    pub fn context_create() -> ContextRef;
    #[link_name = "LLVMContextDispose"]
    pub fn context_dispose(c: ContextRef);
    #[link_name = "LLVMContextSetOpaquePointers"]
    pub fn context_set_opaque_pointers(c: ContextRef, opaque_pointers: Bool);
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
    #[link_name = "LLVMGetLastEnumAttributeKind"]
    pub fn get_last_enum_attribute_kind() -> c_uint;
    #[link_name = "LLVMCreateEnumAttribute"]
    pub fn create_enum_attribute(c: ContextRef, kind_id: c_uint, val: u64) -> AttributeRef;
    #[link_name = "LLVMGetEnumAttributeKind"]
    pub fn get_enum_attribute_kind(a: AttributeRef) -> c_uint;
    #[link_name = "LLVMGetEnumAttributeValue"]
    pub fn get_enum_attribute_value(a: AttributeRef) -> u64;
    #[link_name = "LLVMCreateTypeAttribute"]
    pub fn create_type_attribute(c: ContextRef, kind_id: c_uint, type_ref: TypeRef)
    -> AttributeRef;
    #[link_name = "LLVMGetTypeAttributeValue"]
    pub fn get_type_attribute_value(a: AttributeRef) -> TypeRef;
    #[link_name = "LLVMCreateStringAttribute"]
    pub fn create_string_attribute(
        c: ContextRef,
        k: *const c_char,
        k_length: c_uint,
        v: *const c_char,
        v_length: c_uint,
    ) -> AttributeRef;
    #[link_name = "LLVMGetStringAttributeKind"]
    pub fn get_string_attribute_kind(a: AttributeRef, length: *mut c_uint) -> *const c_char;
    #[link_name = "LLVMGetStringAttributeValue"]
    pub fn get_string_attribute_value(a: AttributeRef, length: *mut c_uint) -> *const c_char;
    #[link_name = "LLVMIsEnumAttribute"]
    pub fn is_enum_attribute(a: AttributeRef) -> Bool;
    #[link_name = "LLVMIsStringAttribute"]
    pub fn is_string_attribute(a: AttributeRef) -> Bool;
    #[link_name = "LLVMIsTypeAttribute"]
    pub fn is_type_attribute(a: AttributeRef) -> Bool;

    // Modules
    #[link_name = "LLVMModuleCreateWithNameInContext"]
    pub fn module_create_with_name_in_context(module_id: *const c_char, c: ContextRef)
    -> ModuleRef;
    #[link_name = "LLVMDisposeModule"]
    pub fn dispose_module(m: ModuleRef);
    #[link_name = "LLVMGetModuleContext"]
    pub fn get_module_context(m: ModuleRef) -> ContextRef;
    #[link_name = "LLVMGetSourceFileName"]
    pub fn get_source_file_name(m: ModuleRef, len: *mut usize) -> *const c_char;
    #[link_name = "LLVMSetSourceFileName"]
    pub fn set_source_file_name(m: ModuleRef, name: *const c_char, len: usize);
    #[link_name = "LLVMGetDataLayoutStr"]
    pub fn get_data_layout_str(m: ModuleRef) -> *const c_char;
    #[link_name = "LLVMSetDataLayout"]
    pub fn set_data_layout(m: ModuleRef, data_layout_str: *const c_char);
    #[link_name = "LLVMGetTarget"]
    pub fn get_target(m: ModuleRef) -> *const c_char;
    #[link_name = "LLVMSetTarget"]
    pub fn set_target(m: ModuleRef, triple: *const c_char);
    #[link_name = "LLVMAddModuleFlag"]
    pub fn add_module_flag(
        m: ModuleRef,
        behavior: ModuleFlagBehavior,
        key: *const c_char,
        key_len: usize,
        val: MetadataRef,
    );
    #[link_name = "LLVMGetModuleFlag"]
    pub fn get_module_flag(m: ModuleRef, key: *const c_char, key_len: usize) -> MetadataRef;
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
    #[link_name = "LLVMGetLastFunction"]
    pub fn get_last_function(m: ModuleRef) -> ValueRef;
    #[link_name = "LLVMGetNextFunction"]
    pub fn get_next_function(fn_: ValueRef) -> ValueRef;
    #[link_name = "LLVMGetPreviousFunction"]
    pub fn get_previous_function(fn_: ValueRef) -> ValueRef;
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
    #[link_name = "LLVMGetLastGlobal"]
    pub fn get_last_global(m: ModuleRef) -> ValueRef;
    #[link_name = "LLVMGetNextGlobal"]
    pub fn get_next_global(global_var: ValueRef) -> ValueRef;
    #[link_name = "LLVMGetPreviousGlobal"]
    pub fn get_previous_global(global_var: ValueRef) -> ValueRef;
    #[link_name = "LLVMDeleteGlobal"]
    pub fn delete_global(global_var: ValueRef);
    #[link_name = "LLVMGetFirstNamedMetadata"]
    pub fn get_first_named_metadata(m: ModuleRef) -> NamedMDNodeRef;
    #[link_name = "LLVMGetLastNamedMetadata"]
    pub fn get_last_named_metadata(m: ModuleRef) -> NamedMDNodeRef;
    #[link_name = "LLVMGetNextNamedMetadata"]
    pub fn get_next_named_metadata(named_md_node: NamedMDNodeRef) -> NamedMDNodeRef;
    #[link_name = "LLVMGetPreviousNamedMetadata"]
    pub fn get_previous_named_metadata(named_md_node: NamedMDNodeRef) -> NamedMDNodeRef;
    #[link_name = "LLVMGetNamedMetadata"]
    pub fn get_named_metadata(m: ModuleRef, name: *const c_char, name_len: usize)
    -> NamedMDNodeRef;
    #[link_name = "LLVMGetOrInsertNamedMetadata"]
    pub fn get_or_insert_named_metadata(
        m: ModuleRef,
        name: *const c_char,
        name_len: usize,
    ) -> NamedMDNodeRef;
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
    #[link_name = "LLVMTypeIsSized"]
    pub fn type_is_sized(ty: TypeRef) -> Bool;
    #[link_name = "LLVMGetTypeContext"]
    pub fn get_type_context(ty: TypeRef) -> ContextRef;
    #[link_name = "LLVMPrintTypeToString"]
    pub fn print_type_to_string(val: TypeRef) -> *mut c_char;
    #[link_name = "LLVMInt1TypeInContext"]
    pub fn int1_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMInt8TypeInContext"]
    pub fn int8_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMInt16TypeInContext"]
    pub fn int16_type_in_context(c: ContextRef) -> TypeRef;
    #[link_name = "LLVMInt32TypeInContext"]
    pub fn int32_type_in_context(c: ContextRef) -> TypeRef;
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
    #[link_name = "LLVMStructGetTypeAtIndex"]
    pub fn struct_get_type_at_index(struct_ty: TypeRef, i: c_uint) -> TypeRef;
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
    #[link_name = "LLVMPointerTypeIsOpaque"]
    pub fn pointer_type_is_opaque(ty: TypeRef) -> Bool;
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
    #[link_name = "LLVMGetValueKind"]
    pub fn get_value_kind(val: ValueRef) -> ValueKind;
    #[link_name = "LLVMGetValueName2"]
    pub fn get_value_name2(val: ValueRef, length: *mut usize) -> *const c_char;
    #[link_name = "LLVMSetValueName2"]
    pub fn set_value_name2(val: ValueRef, name: *const c_char, name_len: usize);
    #[link_name = "LLVMPrintValueToString"]
    pub fn print_value_to_string(val: ValueRef) -> *mut c_char;
    #[link_name = "LLVMReplaceAllUsesWith"]
    pub fn replace_all_uses_with(old_val: ValueRef, new_val: ValueRef);
    #[link_name = "LLVMIsConstant"]
    pub fn is_constant(val: ValueRef) -> Bool;
    #[link_name = "LLVMIsUndef"]
    pub fn is_undef(val: ValueRef) -> Bool;
    #[link_name = "LLVMIsPoison"]
    pub fn is_poison(val: ValueRef) -> Bool;
    #[link_name = "LLVMIsAArgument"]
    pub fn is_a_argument(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsABasicBlock"]
    pub fn is_a_basic_block(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAUser"]
    pub fn is_a_user(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstant"]
    pub fn is_a_constant(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantAggregateZero"]
    pub fn is_a_constant_aggregate_zero(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantArray"]
    pub fn is_a_constant_array(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantDataSequential"]
    pub fn is_a_constant_data_sequential(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantDataArray"]
    pub fn is_a_constant_data_array(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantDataVector"]
    pub fn is_a_constant_data_vector(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantExpr"]
    pub fn is_a_constant_expr(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantFP"]
    pub fn is_a_constant_fp(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantInt"]
    pub fn is_a_constant_int(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantPointerNull"]
    pub fn is_a_constant_pointer_null(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantStruct"]
    pub fn is_a_constant_struct(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAConstantVector"]
    pub fn is_a_constant_vector(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAGlobalValue"]
    pub fn is_a_global_value(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAGlobalObject"]
    pub fn is_a_global_object(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFunction"]
    pub fn is_a_function(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAGlobalVariable"]
    pub fn is_a_global_variable(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAUndefValue"]
    pub fn is_a_undef_value(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAPoisonValue"]
    pub fn is_a_poison_value(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAInstruction"]
    pub fn is_a_instruction(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAUnaryOperator"]
    pub fn is_a_unary_operator(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsABinaryOperator"]
    pub fn is_a_binary_operator(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsACallInst"]
    pub fn is_a_call_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAIntrinsicInst"]
    pub fn is_a_intrinsic_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsACmpInst"]
    pub fn is_a_cmp_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFCmpInst"]
    pub fn is_af_cmp_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAICmpInst"]
    pub fn is_ai_cmp_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAExtractElementInst"]
    pub fn is_a_extract_element_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAGetElementPtrInst"]
    pub fn is_a_get_element_ptr_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAInsertElementInst"]
    pub fn is_a_insert_element_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAInsertValueInst"]
    pub fn is_a_insert_value_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAPHINode"]
    pub fn is_aphi_node(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsASelectInst"]
    pub fn is_a_select_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAShuffleVectorInst"]
    pub fn is_a_shuffle_vector_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAStoreInst"]
    pub fn is_a_store_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsABranchInst"]
    pub fn is_a_branch_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAReturnInst"]
    pub fn is_a_return_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsASwitchInst"]
    pub fn is_a_switch_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAUnreachableInst"]
    pub fn is_a_unreachable_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAUnaryInstruction"]
    pub fn is_a_unary_instruction(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAAllocaInst"]
    pub fn is_a_alloca_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsACastInst"]
    pub fn is_a_cast_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAAddrSpaceCastInst"]
    pub fn is_a_addr_space_cast_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsABitCastInst"]
    pub fn is_a_bit_cast_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFPExtInst"]
    pub fn is_afp_ext_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFPToSIInst"]
    pub fn is_afp_to_si_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFPToUIInst"]
    pub fn is_afp_to_ui_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFPTruncInst"]
    pub fn is_afp_trunc_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAIntToPtrInst"]
    pub fn is_a_int_to_ptr_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAPtrToIntInst"]
    pub fn is_a_ptr_to_int_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsASExtInst"]
    pub fn is_as_ext_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsASIToFPInst"]
    pub fn is_asi_to_fp_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsATruncInst"]
    pub fn is_a_trunc_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAUIToFPInst"]
    pub fn is_aui_to_fp_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAZExtInst"]
    pub fn is_az_ext_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAExtractValueInst"]
    pub fn is_a_extract_value_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsALoadInst"]
    pub fn is_a_load_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFreezeInst"]
    pub fn is_a_freeze_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAAtomicCmpXchgInst"]
    pub fn is_a_atomic_cmp_xchg_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAAtomicRMWInst"]
    pub fn is_a_atomic_rmw_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAFenceInst"]
    pub fn is_a_fence_inst(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAMDNode"]
    pub fn is_amd_node(val: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsAMDString"]
    pub fn is_amd_string(val: ValueRef) -> ValueRef;

    // Uses and operands
    #[link_name = "LLVMGetFirstUse"]
    pub fn get_first_use(val: ValueRef) -> UseRef;
    #[link_name = "LLVMGetNextUse"]
    pub fn get_next_use(u: UseRef) -> UseRef;
    #[link_name = "LLVMGetUser"]
    pub fn get_user(u: UseRef) -> ValueRef;
    #[link_name = "LLVMGetUsedValue"]
    pub fn get_used_value(u: UseRef) -> ValueRef;
    #[link_name = "LLVMGetNumOperands"]
    pub fn get_num_operands(val: ValueRef) -> c_int;
    #[link_name = "LLVMGetOperand"]
    pub fn get_operand(val: ValueRef, index: c_uint) -> ValueRef;
    #[link_name = "LLVMGetOperandUse"]
    pub fn get_operand_use(val: ValueRef, index: c_uint) -> UseRef;
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
    #[link_name = "LLVMConstPointerNull"]
    pub fn const_pointer_null(ty: TypeRef) -> ValueRef;
    #[link_name = "LLVMConstInt"]
    pub fn const_int(int_ty: TypeRef, n: c_ulonglong, sign_extend: Bool) -> ValueRef;
    #[link_name = "LLVMConstIntOfArbitraryPrecision"]
    pub fn const_int_of_arbitrary_precision(
        int_ty: TypeRef,
        num_words: c_uint,
        words: *const u64,
    ) -> ValueRef;
    #[link_name = "LLVMConstReal"]
    pub fn const_real(real_ty: TypeRef, n: f64) -> ValueRef;
    #[link_name = "LLVMConstIntGetZExtValue"]
    pub fn const_int_get_z_ext_value(constant_val: ValueRef) -> c_ulonglong;
    #[link_name = "LLVMConstIntGetSExtValue"]
    pub fn const_int_get_s_ext_value(constant_val: ValueRef) -> c_longlong;
    #[link_name = "LLVMConstRealGetDouble"]
    pub fn const_real_get_double(constant_val: ValueRef, loses_info: *mut Bool) -> f64;
    #[link_name = "LLVMConstStringInContext"]
    pub fn const_string_in_context(
        c: ContextRef,
        str: *const c_char,
        length: c_uint,
        dont_null_terminate: Bool,
    ) -> ValueRef;
    #[link_name = "LLVMIsConstantString"]
    pub fn is_constant_string(c: ValueRef) -> Bool;
    #[link_name = "LLVMGetAsString"]
    pub fn get_as_string(c: ValueRef, length: *mut usize) -> *const c_char;
    #[link_name = "LLVMConstStructInContext"]
    pub fn const_struct_in_context(
        c: ContextRef,
        constant_vals: *mut ValueRef,
        count: c_uint,
        packed: Bool,
    ) -> ValueRef;
    #[link_name = "LLVMConstNamedStruct"]
    pub fn const_named_struct(
        struct_ty: TypeRef,
        constant_vals: *mut ValueRef,
        count: c_uint,
    ) -> ValueRef;
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
    #[link_name = "LLVMGetConstOpcode"]
    pub fn get_const_opcode(constant_val: ValueRef) -> Opcode;
    #[link_name = "LLVMConstBitCast"]
    pub fn const_bit_cast(constant_val: ValueRef, to_type: TypeRef) -> ValueRef;
    #[link_name = "LLVMConstPointerCast"]
    pub fn const_pointer_cast(constant_val: ValueRef, to_type: TypeRef) -> ValueRef;
    #[link_name = "LLVMConstAddrSpaceCast"]
    pub fn const_addr_space_cast(constant_val: ValueRef, to_type: TypeRef) -> ValueRef;
    #[link_name = "LLVMConstPtrToInt"]
    pub fn const_ptr_to_int(constant_val: ValueRef, to_type: TypeRef) -> ValueRef;
    #[link_name = "LLVMConstIntToPtr"]
    pub fn const_int_to_ptr(constant_val: ValueRef, to_type: TypeRef) -> ValueRef;
    #[link_name = "LLVMConstGEP2"]
    pub fn const_gep2(
        ty: TypeRef,
        constant_val: ValueRef,
        constant_indices: *mut ValueRef,
        num_indices: c_uint,
    ) -> ValueRef;
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
    #[link_name = "LLVMGetSection"]
    pub fn get_section(global: ValueRef) -> *const c_char;
    #[link_name = "LLVMSetSection"]
    pub fn set_section(global: ValueRef, section: *const c_char);
    #[link_name = "LLVMGetUnnamedAddress"]
    pub fn get_unnamed_address(global: ValueRef) -> UnnamedAddr;
    #[link_name = "LLVMSetUnnamedAddress"]
    pub fn set_unnamed_address(global: ValueRef, unnamed_addr: UnnamedAddr);
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
    #[link_name = "LLVMIsThreadLocal"]
    pub fn is_thread_local(global_var: ValueRef) -> Bool;
    #[link_name = "LLVMSetThreadLocal"]
    pub fn set_thread_local(global_var: ValueRef, is_thread_local: Bool);
    #[link_name = "LLVMGetThreadLocalMode"]
    pub fn get_thread_local_mode(global_var: ValueRef) -> ThreadLocalMode;
    #[link_name = "LLVMSetThreadLocalMode"]
    pub fn set_thread_local_mode(global_var: ValueRef, mode: ThreadLocalMode);
    #[link_name = "LLVMIsGlobalConstant"]
    pub fn is_global_constant(global_var: ValueRef) -> Bool;
    #[link_name = "LLVMSetGlobalConstant"]
    pub fn set_global_constant(global_var: ValueRef, is_constant: Bool);
    #[link_name = "LLVMIsExternallyInitialized"]
    pub fn is_externally_initialized(global_var: ValueRef) -> Bool;
    #[link_name = "LLVMSetExternallyInitialized"]
    pub fn set_externally_initialized(global_var: ValueRef, is_ext_init: Bool);

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
    #[link_name = "LLVMIntrinsicIsOverloaded"]
    pub fn intrinsic_is_overloaded(id: c_uint) -> Bool;
    #[link_name = "LLVMGetFunctionCallConv"]
    pub fn get_function_call_conv(fn_: ValueRef) -> c_uint;
    #[link_name = "LLVMSetFunctionCallConv"]
    pub fn set_function_call_conv(fn_: ValueRef, cc: c_uint);
    #[link_name = "LLVMAddAttributeAtIndex"]
    pub fn add_attribute_at_index(f: ValueRef, idx: AttributeIndex, a: AttributeRef);
    #[link_name = "LLVMGetAttributeCountAtIndex"]
    pub fn get_attribute_count_at_index(f: ValueRef, idx: AttributeIndex) -> c_uint;
    #[link_name = "LLVMGetAttributesAtIndex"]
    pub fn get_attributes_at_index(f: ValueRef, idx: AttributeIndex, attrs: *mut AttributeRef);
    #[link_name = "LLVMGetEnumAttributeAtIndex"]
    pub fn get_enum_attribute_at_index(
        f: ValueRef,
        idx: AttributeIndex,
        kind_id: c_uint,
    ) -> AttributeRef;
    #[link_name = "LLVMGetStringAttributeAtIndex"]
    pub fn get_string_attribute_at_index(
        f: ValueRef,
        idx: AttributeIndex,
        k: *const c_char,
        k_len: c_uint,
    ) -> AttributeRef;
    #[link_name = "LLVMRemoveEnumAttributeAtIndex"]
    pub fn remove_enum_attribute_at_index(f: ValueRef, idx: AttributeIndex, kind_id: c_uint);
    #[link_name = "LLVMRemoveStringAttributeAtIndex"]
    pub fn remove_string_attribute_at_index(
        f: ValueRef,
        idx: AttributeIndex,
        k: *const c_char,
        k_len: c_uint,
    );
    #[link_name = "LLVMCountParams"]
    pub fn count_params(fn_: ValueRef) -> c_uint;
    #[link_name = "LLVMGetParams"]
    pub fn get_params(fn_: ValueRef, params: *mut ValueRef);
    #[link_name = "LLVMGetParam"]
    pub fn get_param(fn_: ValueRef, index: c_uint) -> ValueRef;
    #[link_name = "LLVMGetParamParent"]
    pub fn get_param_parent(inst: ValueRef) -> ValueRef;
    #[link_name = "LLVMGetFirstParam"]
    pub fn get_first_param(fn_: ValueRef) -> ValueRef;
    #[link_name = "LLVMGetNextParam"]
    pub fn get_next_param(arg: ValueRef) -> ValueRef;

    // Basic blocks
    #[link_name = "LLVMBasicBlockAsValue"]
    pub fn basic_block_as_value(bb: BasicBlockRef) -> ValueRef;
    #[link_name = "LLVMValueIsBasicBlock"]
    pub fn value_is_basic_block(val: ValueRef) -> Bool;
    #[link_name = "LLVMValueAsBasicBlock"]
    pub fn value_as_basic_block(val: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetBasicBlockName"]
    pub fn get_basic_block_name(bb: BasicBlockRef) -> *const c_char;
    #[link_name = "LLVMGetBasicBlockParent"]
    pub fn get_basic_block_parent(bb: BasicBlockRef) -> ValueRef;
    #[link_name = "LLVMGetBasicBlockTerminator"]
    pub fn get_basic_block_terminator(bb: BasicBlockRef) -> ValueRef;
    #[link_name = "LLVMCountBasicBlocks"]
    pub fn count_basic_blocks(fn_: ValueRef) -> c_uint;
    #[link_name = "LLVMGetBasicBlocks"]
    pub fn get_basic_blocks(fn_: ValueRef, basic_blocks: *mut BasicBlockRef);
    #[link_name = "LLVMGetFirstBasicBlock"]
    pub fn get_first_basic_block(fn_: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetLastBasicBlock"]
    pub fn get_last_basic_block(fn_: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetNextBasicBlock"]
    pub fn get_next_basic_block(bb: BasicBlockRef) -> BasicBlockRef;
    #[link_name = "LLVMGetPreviousBasicBlock"]
    pub fn get_previous_basic_block(bb: BasicBlockRef) -> BasicBlockRef;
    #[link_name = "LLVMGetEntryBasicBlock"]
    pub fn get_entry_basic_block(fn_: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMInsertExistingBasicBlockAfterInsertBlock"]
    pub fn insert_existing_basic_block_after_insert_block(builder: BuilderRef, bb: BasicBlockRef);
    #[link_name = "LLVMAppendExistingBasicBlock"]
    pub fn append_existing_basic_block(fn_: ValueRef, bb: BasicBlockRef);
    #[link_name = "LLVMCreateBasicBlockInContext"]
    pub fn create_basic_block_in_context(c: ContextRef, name: *const c_char) -> BasicBlockRef;
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
    #[link_name = "LLVMDeleteBasicBlock"]
    pub fn delete_basic_block(bb: BasicBlockRef);
    #[link_name = "LLVMRemoveBasicBlockFromParent"]
    pub fn remove_basic_block_from_parent(bb: BasicBlockRef);
    #[link_name = "LLVMMoveBasicBlockBefore"]
    pub fn move_basic_block_before(bb: BasicBlockRef, move_pos: BasicBlockRef);
    #[link_name = "LLVMMoveBasicBlockAfter"]
    pub fn move_basic_block_after(bb: BasicBlockRef, move_pos: BasicBlockRef);
    #[link_name = "LLVMGetFirstInstruction"]
    pub fn get_first_instruction(bb: BasicBlockRef) -> ValueRef;
    #[link_name = "LLVMGetLastInstruction"]
    pub fn get_last_instruction(bb: BasicBlockRef) -> ValueRef;

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
    #[link_name = "LLVMValueMetadataEntriesGetMetadata"]
    pub fn value_metadata_entries_get_metadata(
        entries: *mut ValueMetadataEntry,
        index: c_uint,
    ) -> MetadataRef;
    #[link_name = "LLVMDisposeValueMetadataEntries"]
    pub fn dispose_value_metadata_entries(entries: *mut ValueMetadataEntry);
    #[link_name = "LLVMGetInstructionParent"]
    pub fn get_instruction_parent(inst: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetNextInstruction"]
    pub fn get_next_instruction(inst: ValueRef) -> ValueRef;
    #[link_name = "LLVMGetPreviousInstruction"]
    pub fn get_previous_instruction(inst: ValueRef) -> ValueRef;
    #[link_name = "LLVMInstructionRemoveFromParent"]
    pub fn instruction_remove_from_parent(inst: ValueRef);
    #[link_name = "LLVMInstructionEraseFromParent"]
    pub fn instruction_erase_from_parent(inst: ValueRef);
    #[link_name = "LLVMGetInstructionOpcode"]
    pub fn get_instruction_opcode(inst: ValueRef) -> Opcode;
    #[link_name = "LLVMGetICmpPredicate"]
    pub fn get_i_cmp_predicate(inst: ValueRef) -> IntPredicate;
    #[link_name = "LLVMGetFCmpPredicate"]
    pub fn get_f_cmp_predicate(inst: ValueRef) -> RealPredicate;
    #[link_name = "LLVMInstructionClone"]
    pub fn instruction_clone(inst: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsATerminatorInst"]
    pub fn is_a_terminator_inst(inst: ValueRef) -> ValueRef;
    #[link_name = "LLVMGetNumArgOperands"]
    pub fn get_num_arg_operands(instr: ValueRef) -> c_uint;
    #[link_name = "LLVMGetInstructionCallConv"]
    pub fn get_instruction_call_conv(instr: ValueRef) -> c_uint;
    #[link_name = "LLVMSetInstructionCallConv"]
    pub fn set_instruction_call_conv(instr: ValueRef, cc: c_uint);
    #[link_name = "LLVMAddCallSiteAttribute"]
    pub fn add_call_site_attribute(c: ValueRef, idx: AttributeIndex, a: AttributeRef);
    #[link_name = "LLVMGetCallSiteAttributeCount"]
    pub fn get_call_site_attribute_count(c: ValueRef, idx: AttributeIndex) -> c_uint;
    #[link_name = "LLVMGetCallSiteAttributes"]
    pub fn get_call_site_attributes(c: ValueRef, idx: AttributeIndex, attrs: *mut AttributeRef);
    #[link_name = "LLVMGetCallSiteEnumAttribute"]
    pub fn get_call_site_enum_attribute(
        c: ValueRef,
        idx: AttributeIndex,
        kind_id: c_uint,
    ) -> AttributeRef;
    #[link_name = "LLVMGetCalledFunctionType"]
    pub fn get_called_function_type(c: ValueRef) -> TypeRef;
    #[link_name = "LLVMGetCalledValue"]
    pub fn get_called_value(instr: ValueRef) -> ValueRef;
    #[link_name = "LLVMIsTailCall"]
    pub fn is_tail_call(call_inst: ValueRef) -> Bool;
    #[link_name = "LLVMSetTailCall"]
    pub fn set_tail_call(call_inst: ValueRef, is_tail_call: Bool);
    #[link_name = "LLVMGetNumSuccessors"]
    pub fn get_num_successors(term: ValueRef) -> c_uint;
    #[link_name = "LLVMGetSuccessor"]
    pub fn get_successor(term: ValueRef, i: c_uint) -> BasicBlockRef;
    #[link_name = "LLVMSetSuccessor"]
    pub fn set_successor(term: ValueRef, i: c_uint, block: BasicBlockRef);
    #[link_name = "LLVMIsConditional"]
    pub fn is_conditional(branch: ValueRef) -> Bool;
    #[link_name = "LLVMGetCondition"]
    pub fn get_condition(branch: ValueRef) -> ValueRef;
    #[link_name = "LLVMSetCondition"]
    pub fn set_condition(branch: ValueRef, cond: ValueRef);
    #[link_name = "LLVMGetSwitchDefaultDest"]
    pub fn get_switch_default_dest(switch_instr: ValueRef) -> BasicBlockRef;
    #[link_name = "LLVMGetAllocatedType"]
    pub fn get_allocated_type(alloca: ValueRef) -> TypeRef;
    #[link_name = "LLVMIsInBounds"]
    pub fn is_in_bounds(gep: ValueRef) -> Bool;
    #[link_name = "LLVMSetIsInBounds"]
    pub fn set_is_in_bounds(gep: ValueRef, in_bounds: Bool);
    #[link_name = "LLVMGetGEPSourceElementType"]
    pub fn get_gep_source_element_type(gep: ValueRef) -> TypeRef;
    #[link_name = "LLVMAddIncoming"]
    pub fn add_incoming(
        phi_node: ValueRef,
        incoming_values: *mut ValueRef,
        incoming_blocks: *mut BasicBlockRef,
        count: c_uint,
    );
    #[link_name = "LLVMCountIncoming"]
    pub fn count_incoming(phi_node: ValueRef) -> c_uint;
    #[link_name = "LLVMGetIncomingValue"]
    pub fn get_incoming_value(phi_node: ValueRef, index: c_uint) -> ValueRef;
    #[link_name = "LLVMGetIncomingBlock"]
    pub fn get_incoming_block(phi_node: ValueRef, index: c_uint) -> BasicBlockRef;
    #[link_name = "LLVMGetNumIndices"]
    pub fn get_num_indices(inst: ValueRef) -> c_uint;
    #[link_name = "LLVMGetIndices"]
    pub fn get_indices(inst: ValueRef) -> *const c_uint;
    #[link_name = "LLVMGetVolatile"]
    pub fn get_volatile(memory_access_inst: ValueRef) -> Bool;
    #[link_name = "LLVMSetVolatile"]
    pub fn set_volatile(memory_access_inst: ValueRef, is_volatile: Bool);
    #[link_name = "LLVMGetOrdering"]
    pub fn get_ordering(memory_access_inst: ValueRef) -> AtomicOrdering;
    #[link_name = "LLVMSetOrdering"]
    pub fn set_ordering(memory_access_inst: ValueRef, ordering: AtomicOrdering);
    #[link_name = "LLVMGetAtomicRMWBinOp"]
    pub fn get_atomic_rmw_bin_op(atomic_rmw_inst: ValueRef) -> AtomicRMWBinOp;
    #[link_name = "LLVMIsAtomicSingleThread"]
    pub fn is_atomic_single_thread(atomic_inst: ValueRef) -> Bool;
    #[link_name = "LLVMGetCmpXchgSuccessOrdering"]
    pub fn get_cmp_xchg_success_ordering(cmp_xchg_inst: ValueRef) -> AtomicOrdering;
    #[link_name = "LLVMGetCmpXchgFailureOrdering"]
    pub fn get_cmp_xchg_failure_ordering(cmp_xchg_inst: ValueRef) -> AtomicOrdering;
    #[link_name = "LLVMGetNumMaskElements"]
    pub fn get_num_mask_elements(shuffle_vector_inst: ValueRef) -> c_uint;
    #[link_name = "LLVMGetUndefMaskElem"]
    pub fn get_undef_mask_elem() -> c_int;
    #[link_name = "LLVMGetMaskValue"]
    pub fn get_mask_value(shuffle_vector_inst: ValueRef, elt: c_uint) -> c_int;

    // The builder
    #[link_name = "LLVMCreateBuilderInContext"]
    pub fn create_builder_in_context(c: ContextRef) -> BuilderRef;
    #[link_name = "LLVMDisposeBuilder"]
    pub fn dispose_builder(builder: BuilderRef);
    #[link_name = "LLVMPositionBuilder"]
    pub fn position_builder(builder: BuilderRef, block: BasicBlockRef, instr: ValueRef);
    #[link_name = "LLVMPositionBuilderBefore"]
    pub fn position_builder_before(builder: BuilderRef, instr: ValueRef);
    #[link_name = "LLVMPositionBuilderAtEnd"]
    pub fn position_builder_at_end(builder: BuilderRef, block: BasicBlockRef);
    #[link_name = "LLVMGetInsertBlock"]
    pub fn get_insert_block(builder: BuilderRef) -> BasicBlockRef;
    #[link_name = "LLVMClearInsertionPosition"]
    pub fn clear_insertion_position(builder: BuilderRef);
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
    #[link_name = "LLVMBuildUnreachable"]
    pub fn build_unreachable(b: BuilderRef) -> ValueRef;
    #[link_name = "LLVMBuildAdd"]
    pub fn build_add(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildNSWAdd"]
    pub fn build_nsw_add(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildNUWAdd"]
    pub fn build_nuw_add(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFAdd"]
    pub fn build_f_add(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildSub"]
    pub fn build_sub(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildNSWSub"]
    pub fn build_nsw_sub(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildNUWSub"]
    pub fn build_nuw_sub(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildFSub"]
    pub fn build_f_sub(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildMul"]
    pub fn build_mul(b: BuilderRef, lhs: ValueRef, rhs: ValueRef, name: *const c_char) -> ValueRef;
    #[link_name = "LLVMBuildNSWMul"]
    pub fn build_nsw_mul(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildNUWMul"]
    pub fn build_nuw_mul(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
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
    #[link_name = "LLVMBuildExactUDiv"]
    pub fn build_exact_u_div(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildSDiv"]
    pub fn build_s_div(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildExactSDiv"]
    pub fn build_exact_s_div(
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
    #[link_name = "LLVMBuildSRem"]
    pub fn build_s_rem(
        b: BuilderRef,
        lhs: ValueRef,
        rhs: ValueRef,
        name: *const c_char,
    ) -> ValueRef;
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
    #[link_name = "LLVMBuildFPToUI"]
    pub fn build_fp_to_ui(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
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
    #[link_name = "LLVMBuildAddrSpaceCast"]
    pub fn build_addr_space_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildZExtOrBitCast"]
    pub fn build_z_ext_or_bit_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildSExtOrBitCast"]
    pub fn build_s_ext_or_bit_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
    #[link_name = "LLVMBuildTruncOrBitCast"]
    pub fn build_trunc_or_bit_cast(
        b: BuilderRef,
        val: ValueRef,
        dest_ty: TypeRef,
        name: *const c_char,
    ) -> ValueRef;
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
    #[link_name = "LLVMBuildFence"]
    pub fn build_fence(
        b: BuilderRef,
        ordering: AtomicOrdering,
        single_thread: Bool,
        name: *const c_char,
    ) -> ValueRef;
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
    #[link_name = "LLVMCreateMemoryBufferWithMemoryRange"]
    pub fn create_memory_buffer_with_memory_range(
        input_data: *const c_char,
        input_data_length: usize,
        buffer_name: *const c_char,
        requires_null_terminator: Bool,
    ) -> MemoryBufferRef;
    #[link_name = "LLVMCreateMemoryBufferWithMemoryRangeCopy"]
    pub fn create_memory_buffer_with_memory_range_copy(
        input_data: *const c_char,
        input_data_length: usize,
        buffer_name: *const c_char,
    ) -> MemoryBufferRef;
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
    #[link_name = "LLVMVerifyFunction"]
    pub fn verify_function(fn_: ValueRef, action: VerifierFailureAction) -> Bool;

    // The linker (the shim links the helpers; this form, with no flag, is for tests)
    #[link_name = "LLVMLinkModules2"]
    pub fn link_modules2(dest: ModuleRef, src: ModuleRef) -> Bool;
}

// The shim (`shim/shim.cpp`): what LLVM 15's C API lacks. Each is `extern "C"` in C++ built
// without exceptions, so nothing unwinds out of it.
unsafe extern "C" {
    /// Links LLVM's core and the barrier pass through the pass's own declaration: answers 1.
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
    /// The buffer is named by `name`; the module is read in full. Answers 0 and the module in
    /// `*out`, or 1 and `toString` of the error in `*error` (`*error_len` bytes, freed with
    /// `dispose_message`).
    pub fn airconv_shim_parse_bitcode(
        context: ContextRef,
        data: *const c_char,
        size: usize,
        name: *const c_char,
        name_len: usize,
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
    /// Sets the module's SDK version (`Module::setSDKVersion`).
    pub fn airconv_shim_set_sdk_version(module: ModuleRef, major: c_uint, minor: c_uint);
    /// Declares (or finds) `air.simdgroup.barrier` in `module` as the barrier pass does.
    ///
    /// Answers the function.
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
