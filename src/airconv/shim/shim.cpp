// airconv's C++ shim: what LLVM 15's C API lacks, as extern "C" functions named airconv_shim_*. Built by build.rs
// with the two passes of ../transforms.
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
