# airconv's LLVM C++ API against LLVM 23's C API (DXMTRS Phase 0, E2)

What `src/airconv` (C++, LLVM 15) uses, and what a Rust translator on `llvm-sys` (LLVM 23's C API) calls instead.
Counted with `grep -rhoE '\bCreate[A-Z]…\(|\bllvm::…'` over `src/airconv`; airconv's own builders (`CreateSample`,
`CreateTextureQuery`, `CreateSetMesh*` and the like, about 40 names) wrap the calls below and port with the translator.

| airconv uses | C API | Shim |
|---|---|---|
| IRBuilder integer and float arithmetic (`CreateAdd` 116, `Sub`, `Mul`, `UDiv`, `URem`, `And`, `Or`, `Xor`, `Shl`, `LShr`, `AShr`, `Not`, `Neg`, `FAdd`, `FSub`, `FMul`, `FDiv`, `FNeg`, `BinOp`) | `LLVMBuild*` of the same names | — |
| Vectors and aggregates (`InsertElement` 109, `ExtractElement`, `ShuffleVector`, `ExtractValue`, `InsertValue`, `VectorSplat`) | `LLVMBuildInsertElement`, `…ExtractElement`, `…ShuffleVector`, `…ExtractValue`, `…InsertValue`; a splat is insert + shuffle | — |
| Memory (`Load` 86, `Store`, `AlignedLoad/Store`, `GEP` 77, `InBoundsGEP`, `StructGEP`, `Alloca`) | `LLVMBuildLoad2`, `LLVMBuildStore`, `LLVMSetAlignment`, `LLVMBuildGEP2`, `LLVMBuildInBoundsGEP2`, `LLVMBuildStructGEP2`, `LLVMBuildAlloca` | — |
| Casts (`BitCast` 70, `Trunc`, `ZExt`, `SExt`, `ZExtOrTrunc`, `FPExt`, `FPTrunc`, `FPCast`, `UIToFP`, `PtrToInt`, `PointerCast`, `BitOrPointerCast`) | `LLVMBuild*` of the same names (`ZExtOrTrunc` is a width test choosing `ZExt` or `Trunc`) | — |
| Compares and select (`ICmp*`, `FCmp*`, `IsNull`, `IsNotNull`, `IsNaN`, `Select` 52, `LogicalAnd`) | `LLVMBuildICmp`, `LLVMBuildFCmp` (`IsNaN` is `fcmp uno x, x`), `LLVMBuildIsNull`, `LLVMBuildIsNotNull`, `LLVMBuildSelect` | — |
| Control flow (`Br`, `CondBr`, `Switch`, `PHI`, `Ret`, `RetVoid`, `BasicBlock::Create`, `BranchInst::Create`) | `LLVMBuildBr`, `…CondBr`, `…Switch`, `…Phi`, `…Ret`, `…RetVoid`, `LLVMAppendBasicBlockInContext` | — |
| `SplitBlockAndInsertIfThen` (3) | none | rewritten in Rust: split with `LLVMBuildBr` into a new block, then a `CondBr` |
| Calls and intrinsics (`Call` 57, `FMA`, `BinaryIntrinsic`, `uadd`/`usub` with overflow) | `LLVMBuildCall2`, `LLVMLookupIntrinsicID`, `LLVMGetIntrinsicDeclaration` | — |
| Atomics (`AtomicRMW` 13, all `AtomicRMWInst` ops, `AtomicCmpXchg`, `AtomicOrdering`) | `LLVMBuildAtomicRMW`, `LLVMBuildAtomicCmpXchg`, `LLVMSetOrdering` | — |
| Fast-math flags (`FastMathFlags`, `FPMathOperator`) | `LLVMSetFastMathFlags` | — |
| Constants (`ConstantInt`, `ConstantFP` incl. infinity and −0, `UndefValue`, `PoisonValue`, `getNullValue`, `getAllOnesValue`, `ConstantAggregateZero`, `ConstantVector`, `ConstantDataVector`, `ConstantArray`, in-bounds GEP expressions, `APInt` limits) | `LLVMConstInt`, `LLVMConstReal`, `LLVMGetUndef`, `LLVMGetPoison`, `LLVMConstNull`, `LLVMConstAllOnes`, `LLVMConstVector`, `LLVMConstArray2`, `LLVMConstInBoundsGEP2`; the limits computed in Rust | — |
| Types (`Type`, `ArrayType`, `StructType` named and literal, `FixedVectorType`, `FunctionType`, `PointerType`) | `LLVMInt*TypeInContext`, `LLVMArrayType2`, `LLVMStructCreateNamed`, `LLVMStructSetBody`, `LLVMStructTypeInContext`, `LLVMVectorType`, `LLVMFunctionType`, `LLVMPointerTypeInContext` | — |
| Globals, functions, linkage, thread-local mode, attributes (`GlobalVariable`, `Function`, `Attribute::get`, `AttributeList`, `NoUnwind`, `WillReturn`, `ReadNone`, `MustProgress`, `Convergent`) | `LLVMAddGlobalInAddressSpace`, `LLVMSetLinkage`, `LLVMSetThreadLocalMode`, `LLVMAddFunction`, `LLVMCreateEnumAttribute`, `LLVMCreateStringAttribute`, `LLVMAddAttributeAtIndex` | — |
| Metadata (`MDNode`, `MDString`, `NamedMDNode`, `MDOperand`, `Metadata`) | `LLVMMDNodeInContext2`, `LLVMMDStringInContext2`, `LLVMValueAsMetadata`, `LLVMAddNamedMetadataOperand` | — |
| Module, context, data layout, bitcode in (`Module`, `LLVMContext`, `DataLayout`, `BitcodeReader`, `MemoryBufferRef`) | `LLVMModuleCreateWithNameInContext`, `LLVMContextCreate`, `LLVMSetDataLayout`, `LLVMSetTarget`, `LLVMParseBitcodeInContext2` | — |
| `Linker::linkModules` (1) | `LLVMLinkModules2` (`llvm-c/Linker.h`) | — |
| New pass manager pipeline (`PassManager`, `OptimizationLevel`, `LoopAnalysisManager`) | `LLVMRunPasses` with a pipeline string and `LLVMPassBuilderOptions` | — |
| `StandardInstrumentations`, `PassInstrumentationCallbacks`, `ModuleSlotTracker` (1 each) | none | dropped: debug printing only |
| `Verifier` | `LLVMVerifyModule` | — |
| Bitcode out (`BitcodeWriter`, through `MetallibWriter`) | `LLVMWriteBitcodeToMemoryBuffer` writes opaque pointers, which Metal refuses (E2) | **the AIR writer**: our LLVM fork's typed-pointer writer, one C entry point taking the module and the translator's pointee types |
| `Error`, `Expected`, `Optional`, `SmallVector`, `ArrayRef`, `StringRef` | `LLVMErrorRef` where the C API returns one; the rest are Rust's own types | — |

Shims: one C entry point (the AIR writer, in our llvm-project fork, C++). `SplitBlockAndInsertIfThen` is a few lines of
Rust; the instrumentation and slot tracker were debug aids and go.
