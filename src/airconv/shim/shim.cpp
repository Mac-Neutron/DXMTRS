// airconv's C++ shim: what LLVM 23's C API lacks, as extern "C" functions named airconv_shim_*. Built by build.rs
// with the two passes of ../transforms; src/llvm/sys.rs declares each function with what it does. Each body is the
// C++ translator's own call (airconv_context.cpp, metallib_writer.cpp, the DXIL converter), so it behaves as there.
#include <cstdlib>
#include <cstring>
#include <optional>
#include <string>

#include "llvm-c/Core.h"
#include "llvm/Bitcode/BitcodeReader.h"
#include "llvm/Bitcode/BitcodeWriter.h"
#include "llvm/IR/Attributes.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/FMF.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/ModuleSlotTracker.h"
#include "llvm/IR/Operator.h"
#include "llvm/Linker/Linker.h"
#include "llvm/Passes/PassBuilder.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/ModRef.h"
#include "llvm/Support/VersionTuple.h"
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
#include "llvm/Transforms/Utils/BasicBlockUtils.h"
#include "llvm/Transforms/Utils/LowerAtomic.h"
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

// Fast math. FastMathFlags' constructor from bits is private, so the bits go through its setters and getters; they
// are src/llvm/builder.rs's FastMath bits.
static_assert(FastMathFlags::AllowReassoc == 1 << 0 && FastMathFlags::NoNaNs == 1 << 1 &&
                  FastMathFlags::NoInfs == 1 << 2 && FastMathFlags::NoSignedZeros == 1 << 3 &&
                  FastMathFlags::AllowReciprocal == 1 << 4 && FastMathFlags::AllowContract == 1 << 5 &&
                  FastMathFlags::ApproxFunc == 1 << 6,
              "FastMath's bits in src/llvm/builder.rs");

static FastMathFlags FromBits(unsigned bits) {
  FastMathFlags flags;
  flags.setAllowReassoc(bits & FastMathFlags::AllowReassoc);
  flags.setNoNaNs(bits & FastMathFlags::NoNaNs);
  flags.setNoInfs(bits & FastMathFlags::NoInfs);
  flags.setNoSignedZeros(bits & FastMathFlags::NoSignedZeros);
  flags.setAllowReciprocal(bits & FastMathFlags::AllowReciprocal);
  flags.setAllowContract(bits & FastMathFlags::AllowContract);
  flags.setApproxFunc(bits & FastMathFlags::ApproxFunc);
  return flags;
}

static unsigned ToBits(FastMathFlags flags) {
  return (flags.allowReassoc() ? FastMathFlags::AllowReassoc : 0) | (flags.noNaNs() ? FastMathFlags::NoNaNs : 0) |
         (flags.noInfs() ? FastMathFlags::NoInfs : 0) | (flags.noSignedZeros() ? FastMathFlags::NoSignedZeros : 0) |
         (flags.allowReciprocal() ? FastMathFlags::AllowReciprocal : 0) |
         (flags.allowContract() ? FastMathFlags::AllowContract : 0) |
         (flags.approxFunc() ? FastMathFlags::ApproxFunc : 0);
}

extern "C" void airconv_shim_set_fast_math_flags(LLVMBuilderRef builder, unsigned flags) {
  unwrap(builder)->setFastMathFlags(FromBits(flags));
}

extern "C" unsigned airconv_shim_get_fast_math_flags(LLVMValueRef inst) {
  auto *op = dyn_cast<FPMathOperator>(unwrap(inst));
  return op ? ToBits(op->getFastMathFlags()) : 0;
}

extern "C" void airconv_shim_copy_fast_math_flags(LLVMValueRef inst, unsigned flags) {
  auto *i = dyn_cast<Instruction>(unwrap(inst));
  if (i && isa<FPMathOperator>(i))
    i->copyFastMathFlags(FromBits(flags));
}

extern "C" LLVMBool airconv_shim_is_fp_math_operator(LLVMValueRef value) { return isa<FPMathOperator>(unwrap(value)); }

// Bitcode, as MetallibWriter::Write and LoadModule read and write it.
extern "C" LLVMMemoryBufferRef airconv_shim_write_bitcode(LLVMModuleRef module) {
  SmallVector<char, 0> bitcode;
  raw_svector_ostream os(bitcode);
  WriteBitcodeToFile(*unwrap(module), os, false, nullptr, true);
  return wrap(MemoryBuffer::getMemBufferCopy(StringRef(bitcode.data(), bitcode.size())).release());
}

extern "C" LLVMBool airconv_shim_parse_bitcode(LLVMContextRef context, const char *data, size_t size,
                                               const char *name, size_t name_len, LLVMBool dxil_layout,
                                               LLVMModuleRef *out, char **error, size_t *error_len) {
  *out = nullptr;
  *error = nullptr;
  *error_len = 0;
  ParserCallbacks callbacks;
  if (dxil_layout) {
    callbacks.DataLayout = [](StringRef, StringRef layout) -> std::optional<std::string> {
      SmallVector<StringRef, 16> parts;
      layout.split(parts, '-');
      std::string kept;
      for (StringRef part : parts) {
        if (part == "i8:32")
          continue;
        if (!kept.empty())
          kept.push_back('-');
        kept.append(part.data(), part.size());
      }
      return kept;
    };
  }
  auto module = parseBitcodeFile(MemoryBufferRef(StringRef(data, size), StringRef(name, name_len)), *unwrap(context),
                                 callbacks);
  if (module) {
    *out = wrap(module->release());
    return 0;
  }
  // The text as bytes with its length (it need not be a C string); LLVMDisposeMessage frees it.
  std::string text = toString(module.takeError());
  if (auto *copy = static_cast<char *>(malloc(text.size() + 1))) {
    memcpy(copy, text.data(), text.size());
    copy[text.size()] = '\0';
    *error = copy;
    *error_len = text.size();
  }
  return 1;
}

// linkShader's link: errors reach the context's diagnostic handler.
extern "C" LLVMBool airconv_shim_link_only_needed(LLVMModuleRef dest, LLVMModuleRef src) {
  return Linker::linkModules(*unwrap(dest), std::unique_ptr<Module>(unwrap(src)), Linker::Flags::LinkOnlyNeeded);
}

extern "C" void airconv_shim_erase_named_metadata(LLVMModuleRef module, const char *name, size_t name_len) {
  if (auto *md = unwrap(module)->getNamedMetadata(StringRef(name, name_len)))
    unwrap(module)->eraseNamedMetadata(md);
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

extern "C" LLVMAttributeRef airconv_shim_memory_attribute(LLVMContextRef context, int argmem_only, int mod_ref) {
  auto mr = static_cast<ModRefInfo>(mod_ref & 3);
  MemoryEffects effects = (argmem_only == 1)   ? MemoryEffects::argMemOnly(mr)
                          : (argmem_only == 2) ? MemoryEffects::inaccessibleMemOnly(mr)
                          : (argmem_only == 3) ? MemoryEffects::inaccessibleOrArgMemOnly(mr)
                                               : MemoryEffects(mr);
  return wrap(Attribute::getWithMemoryEffects(*unwrap(context), effects));
}

extern "C" LLVMAttributeRef airconv_shim_captures_none(LLVMContextRef context) {
  return wrap(Attribute::getWithCaptureInfo(*unwrap(context), CaptureInfo::none()));
}

extern "C" void airconv_shim_set_sdk_version(LLVMModuleRef module, unsigned major, unsigned minor) {
  unwrap(module)->setSDKVersion(VersionTuple(major, minor));
}

extern "C" LLVMValueRef airconv_shim_declare_simdgroup_barrier(LLVMModuleRef module) {
  return wrap(air::declareSimdgroupBarrier(*unwrap(module)).getCallee());
}

// Blocks and instructions, for the DXIL converter.
extern "C" LLVMBasicBlockRef airconv_shim_split_block(LLVMBasicBlockRef block, LLVMValueRef before) {
  auto *bb = unwrap(block);
  return wrap(bb->splitBasicBlock(before ? unwrap<Instruction>(before)->getIterator() : bb->end()));
}

extern "C" LLVMValueRef airconv_shim_split_block_and_insert_if_then(LLVMValueRef cond, LLVMValueRef split_before,
                                                                    LLVMBool unreachable) {
  return wrap(SplitBlockAndInsertIfThen(unwrap(cond), unwrap<Instruction>(split_before), unreachable != 0));
}

extern "C" LLVMValueRef airconv_shim_get_as_instruction(LLVMValueRef expr, LLVMValueRef insert_before) {
  Instruction *inst = unwrap<ConstantExpr>(expr)->getAsInstruction();
  if (auto *before = cast_or_null<Instruction>(unwrap(insert_before)))
    inst->insertBefore(before->getIterator());
  return wrap(inst);
}

extern "C" void airconv_shim_destroy_constant(LLVMValueRef constant) { unwrap<Constant>(constant)->destroyConstant(); }

extern "C" void airconv_shim_rename_struct(LLVMModuleRef module, const char *from, size_t from_len, const char *to,
                                           size_t to_len) {
  StringRef prefix(from, from_len), replacement(to, to_len);
  for (auto *st : unwrap(module)->getIdentifiedStructTypes())
    if (st->getName().starts_with(prefix))
      st->setName(replacement.str() + st->getName().substr(prefix.size()).str());
}

// Core.cpp's mapping (static there); an operation the C++ translator did not handle answers BAD_BINOP.
static AtomicRMWInst::BinOp BinOp(LLVMAtomicRMWBinOp op) {
  switch (op) {
  case LLVMAtomicRMWBinOpXchg: return AtomicRMWInst::Xchg;
  case LLVMAtomicRMWBinOpAdd: return AtomicRMWInst::Add;
  case LLVMAtomicRMWBinOpSub: return AtomicRMWInst::Sub;
  case LLVMAtomicRMWBinOpAnd: return AtomicRMWInst::And;
  case LLVMAtomicRMWBinOpNand: return AtomicRMWInst::Nand;
  case LLVMAtomicRMWBinOpOr: return AtomicRMWInst::Or;
  case LLVMAtomicRMWBinOpXor: return AtomicRMWInst::Xor;
  case LLVMAtomicRMWBinOpMax: return AtomicRMWInst::Max;
  case LLVMAtomicRMWBinOpMin: return AtomicRMWInst::Min;
  case LLVMAtomicRMWBinOpUMax: return AtomicRMWInst::UMax;
  case LLVMAtomicRMWBinOpUMin: return AtomicRMWInst::UMin;
  case LLVMAtomicRMWBinOpFAdd: return AtomicRMWInst::FAdd;
  case LLVMAtomicRMWBinOpFSub: return AtomicRMWInst::FSub;
  case LLVMAtomicRMWBinOpFMax: return AtomicRMWInst::FMax;
  case LLVMAtomicRMWBinOpFMin: return AtomicRMWInst::FMin;
  default: return AtomicRMWInst::BAD_BINOP;
  }
}

// Null for an operation outside the C enum (buildAtomicRMWValue would reach llvm_unreachable).
extern "C" LLVMValueRef airconv_shim_build_atomic_rmw_value(LLVMBuilderRef builder, LLVMAtomicRMWBinOp op,
                                                            LLVMValueRef loaded, LLVMValueRef value) {
  auto bin_op = BinOp(op);
  if (bin_op == AtomicRMWInst::BAD_BINOP)
    return nullptr;
  return wrap(buildAtomicRMWValue(bin_op, *unwrap(builder), unwrap(loaded), unwrap(value)));
}

// ApplyProbe's lookup: the last instruction whose slot is `slot` ((int)~0u matches an unnumbered one, as there).
extern "C" LLVMValueRef airconv_shim_local_slot(LLVMValueRef function, unsigned slot) {
  auto *f = unwrap<Function>(function);
  ModuleSlotTracker slots(f->getParent());
  slots.incorporateFunction(*f);
  Instruction *found = nullptr;
  for (auto &block : *f)
    for (auto &inst : block)
      if (slots.getLocalSlot(&inst) == (int)slot)
        found = &inst;
  return wrap(found);
}
