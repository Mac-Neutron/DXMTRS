// airconv's C++ shim: the pass pipeline and its two passes, as extern "C" functions named airconv_shim_*. Built by
// build.rs with the two passes of ../transforms; src/llvm/sys.rs declares each function with what it does. Each body
// is the C++ translator's own call (airconv_context.cpp), so it behaves as there.
#include <cstdlib>
#include <cstring>
#include <string>

#include "llvm-c/Core.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Transforms/IPO/AlwaysInliner.h"
#include "llvm/Transforms/IPO/Annotation2Metadata.h"
#include "llvm/Transforms/IPO/DeadArgumentElimination.h"
#include "llvm/Transforms/IPO/ForceFunctionAttrs.h"
#include "llvm/Transforms/IPO/GlobalOpt.h"
#include "llvm/Transforms/IPO/InferFunctionAttrs.h"
#include "llvm/Transforms/IPO/SCCP.h"
#include "llvm/Transforms/InstCombine/InstCombine.h"
#include "llvm/Transforms/Scalar/EarlyCSE.h"
#include "llvm/Transforms/Scalar/LowerExpectIntrinsic.h"
#include "llvm/Transforms/Scalar/SROA.h"
#include "llvm/Transforms/Scalar/SimplifyCFG.h"
#include "llvm/Transforms/Utils/Mem2Reg.h"

#include "../transforms/lower_16bit_texread.hpp"
#include "../transforms/simdgroup_implicit_membarrier.hpp"

using namespace llvm;

// A probe of the link: declares the barrier pass's intrinsic in a fresh module through the pass's own function, so a
// caller links LLVM's core and the pass. Answers 1 when the declaration exists.
extern "C" int airconv_shim_probe(void) {
  LLVMContext context;
  Module module("probe", context);
  air::declareSimdgroupBarrier(module);
  return module.getFunction("air.simdgroup.barrier") != nullptr;
}

static ModulePassManager BuildPipeline(int air_passes) {
  ModulePassManager MPM;

  // Convert @llvm.global.annotations to !annotation metadata.
  MPM.addPass(Annotation2MetadataPass());
  // Force any function attributes we want the rest of the pipeline to observe.
  MPM.addPass(ForceFunctionAttrsPass());
  // Do basic inference of function attributes from known properties of system libraries and other oracles.
  MPM.addPass(InferFunctionAttrsPass());

  {
    // Create an early function pass manager to cleanup the output of the frontend.
    FunctionPassManager EarlyFPM;
    // Lower llvm.expect to metadata before attempting transforms.
    EarlyFPM.addPass(LowerExpectIntrinsicPass());
    EarlyFPM.addPass(SimplifyCFGPass());
    EarlyFPM.addPass(SROAPass(SROAOptions::ModifyCFG));
    EarlyFPM.addPass(EarlyCSEPass());
    MPM.addPass(createModuleToFunctionPassAdaptor(std::move(EarlyFPM), true));
  }

  MPM.addPass(AlwaysInlinerPass());
  MPM.addPass(IPSCCPPass());
  MPM.addPass(GlobalOptPass());
  MPM.addPass(createModuleToFunctionPassAdaptor(PromotePass()));
  MPM.addPass(DeadArgumentEliminationPass());

  {
    // Create a small function pass pipeline to cleanup after all the global optimizations.
    FunctionPassManager GlobalCleanupPM;
    GlobalCleanupPM.addPass(InstCombinePass());
    GlobalCleanupPM.addPass(SimplifyCFGPass(SimplifyCFGOptions().convertSwitchRangeToICmp(true)));
    if (air_passes) {
      GlobalCleanupPM.addPass(air::Lower16BitTexReadPass());
      GlobalCleanupPM.addPass(air::SimdgroupImplicitMemBarrierPass());
    }
    MPM.addPass(createModuleToFunctionPassAdaptor(std::move(GlobalCleanupPM), true));
  }

  return MPM;
}

// runOptimizationPasses (airconv_context.cpp), without its poke of instcombine-infinite-loop-threshold: it set the
// NDEBUG default, 1000.
extern "C" void airconv_shim_run_passes(LLVMModuleRef module) {
  // Following optimization passes are picked from default LLVM pipelines
  // An unoptimized shader can make PSO compilation take a very long time
  // But a full optimization pipeline is also too heavy
  // So we choose some passes that really matter to run

  // These must be declared in this order so that they are destroyed in the correct order due to inter-analysis-manager
  // references.
  LoopAnalysisManager LAM;
  FunctionAnalysisManager FAM;
  CGSCCAnalysisManager CGAM;
  ModuleAnalysisManager MAM;

  PassInstrumentationCallbacks PIC;
  PassBuilder PB(nullptr, PipelineTuningOptions(), {}, &PIC);

  PB.registerModuleAnalyses(MAM);
  PB.registerCGSCCAnalyses(CGAM);
  PB.registerFunctionAnalyses(FAM);
  PB.registerLoopAnalyses(LAM);
  PB.crossRegisterProxies(LAM, FAM, CGAM, MAM);

  ModulePassManager MPM = BuildPipeline(1);
  MPM.run(*unwrap(module), MAM);
}

extern "C" void airconv_shim_pipeline_text(int air_passes, char **out, size_t *len) {
  *out = nullptr;
  *len = 0;
  PassInstrumentationCallbacks PIC;
  PassBuilder PB(nullptr, PipelineTuningOptions(), {}, &PIC);
  ModulePassManager MPM = BuildPipeline(air_passes);
  std::string text;
  raw_string_ostream os(text);
  MPM.printPipeline(os, [&PIC](StringRef ClassName) {
    auto PassName = PIC.getPassNameForClassName(ClassName);
    return PassName.empty() ? ClassName : PassName;
  });
  os.flush();
  if (auto *copy = static_cast<char *>(malloc(text.size() + 1))) {
    memcpy(copy, text.data(), text.size());
    copy[text.size()] = '\0';
    *out = copy;
    *len = text.size();
  }
}

extern "C" LLVMValueRef airconv_shim_declare_simdgroup_barrier(LLVMModuleRef module) {
  return wrap(air::declareSimdgroupBarrier(*unwrap(module)).getCallee());
}
