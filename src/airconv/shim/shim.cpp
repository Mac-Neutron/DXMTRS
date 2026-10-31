// airconv's C++ shim: what LLVM 15's C API lacks, as extern "C" functions named airconv_shim_*. Built by build.rs
// with the two passes of ../transforms. src/llvm/sys.rs declares each function with what it does; until its body is
// written a function does nothing and answers null, 0, or (parse and link) a failure.
#include "llvm-c/Core.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"

#include "../transforms/simdgroup_implicit_membarrier.hpp"

// A probe of the link: declares the barrier pass's intrinsic in a fresh module through the pass's own function, so a
// caller links LLVM's core and the pass. Answers 1 when the declaration exists.
extern "C" int airconv_shim_probe(void) {
  llvm::LLVMContext context;
  llvm::Module module("probe", context);
  llvm::air::declareSimdgroupBarrier(module);
  return module.getFunction("air.simdgroup.barrier") != nullptr;
}

extern "C" void airconv_shim_set_fast_math_flags(LLVMBuilderRef, unsigned) {}

extern "C" unsigned airconv_shim_get_fast_math_flags(LLVMValueRef) { return 0; }

extern "C" void airconv_shim_copy_fast_math_flags(LLVMValueRef, unsigned) {}

extern "C" LLVMBool airconv_shim_is_fp_math_operator(LLVMValueRef) { return 0; }

extern "C" LLVMMemoryBufferRef airconv_shim_write_bitcode(LLVMModuleRef) { return nullptr; }

extern "C" LLVMBool airconv_shim_parse_bitcode(LLVMContextRef, const char *, size_t, const char *, size_t,
                                               LLVMModuleRef *out, char **error, size_t *error_len) {
  *out = nullptr;
  *error = nullptr;
  *error_len = 0;
  return 1;
}

extern "C" LLVMBool airconv_shim_link_only_needed(LLVMModuleRef, LLVMModuleRef src) {
  LLVMDisposeModule(src);
  return 1;
}

extern "C" void airconv_shim_erase_named_metadata(LLVMModuleRef, const char *, size_t) {}

extern "C" void airconv_shim_run_passes(LLVMModuleRef) {}

extern "C" void airconv_shim_set_sdk_version(LLVMModuleRef, unsigned, unsigned) {}

extern "C" LLVMValueRef airconv_shim_declare_simdgroup_barrier(LLVMModuleRef) { return nullptr; }

extern "C" LLVMBasicBlockRef airconv_shim_split_block(LLVMBasicBlockRef, LLVMValueRef) { return nullptr; }

extern "C" LLVMValueRef airconv_shim_split_block_and_insert_if_then(LLVMValueRef, LLVMValueRef, LLVMBool) {
  return nullptr;
}

extern "C" LLVMValueRef airconv_shim_get_as_instruction(LLVMValueRef, LLVMValueRef) { return nullptr; }

extern "C" void airconv_shim_destroy_constant(LLVMValueRef) {}

extern "C" void airconv_shim_rename_struct(LLVMModuleRef, const char *, size_t, const char *, size_t) {}

extern "C" LLVMValueRef airconv_shim_build_atomic_rmw_value(LLVMBuilderRef, LLVMAtomicRMWBinOp, LLVMValueRef,
                                                            LLVMValueRef) {
  return nullptr;
}

extern "C" LLVMValueRef airconv_shim_local_slot(LLVMValueRef, unsigned) { return nullptr; }
