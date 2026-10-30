#include "simdgroup_implicit_membarrier.hpp"
#include "llvm/ADT/DenseSet.h"
#include "llvm/Analysis/LoopAnalysisManager.h"
#include "llvm/Analysis/MemorySSA.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Instruction.h"
#include "llvm/IR/Instructions.h"

namespace llvm::air {

// `air.simdgroup.barrier(i32 flags, i32 1)`, declared as AIRBuilder::CreateBarrier(flags, true) declares it
// (nt/air_builder.cpp), so this pass needs nothing else from AIRBuilder.
FunctionCallee
declareSimdgroupBarrier(Module &M) {
  auto &Context = M.getContext();
  auto Attrs = AttributeList::get(
      Context, {{~0U, Attribute::get(Context, Attribute::AttrKind::Convergent)},
                {~0U, Attribute::get(Context, Attribute::AttrKind::NoUnwind)},
                {~0U, Attribute::get(Context, Attribute::AttrKind::WillReturn)}}
  );
  auto I32 = Type::getInt32Ty(Context);
  return M.getOrInsertFunction(
      "air.simdgroup.barrier", FunctionType::get(Type::getVoidTy(Context), {I32, I32}, false), Attrs
  );
}

PreservedAnalyses
SimdgroupImplicitMemBarrierPass::run(Function &F, FunctionAnalysisManager &AM) {
  if (!F.getParent()->getNamedMetadata("air.kernel"))
    return PreservedAnalyses::all();

  bool Changed = false;

  SmallVector<LoadInst *, 4> ToCheck;
  SmallDenseSet<Instruction *, 4> Fenced;

  auto &MemSSA = AM.getResult<MemorySSAAnalysis>(F).getMSSA();

  for (auto &BB : F) {
    for (auto &Inst : BB) {
      if (auto Load = llvm::dyn_cast<LoadInst>(&Inst)) {
        if (Load->getPointerAddressSpace() == 3)
          ToCheck.push_back(Load);
      }
    }
  }

  for (auto &Load : ToCheck) {
    auto MA = MemSSA.getMemoryAccess(Load);
    auto DefAccess = MA->getDefiningAccess();
    /**
    * We are only checking the simplest case: MemoryDef is a store to threadgroup memory
    * It's not complete, but good enough for all games I known requiring such warp-coherency behavior
    * FIXME: uniformity can be a issue because we are using `simdgroup_barrier`
    */
    if (auto Def = llvm::dyn_cast<MemoryDef>(DefAccess)) {
      auto DefInst = Def->getMemoryInst();
      if (Fenced.contains(DefInst))
        continue;
      if (auto Store = llvm::dyn_cast<StoreInst>(DefInst)) {
        if (Store->getPointerAddressSpace() == 3) {
          Fenced.insert(DefInst);
          IRBuilder<> builder(Store->getParent());
          if (Store->getNextNode())
            builder.SetInsertPoint(Store->getNextNode());
          // I expect atomic_thread_fence() to work, but no luck
          constexpr uint32_t ThreadgroupMemFlags = 2; // MemFlags::Threadgroup
          builder.CreateCall(
              declareSimdgroupBarrier(*F.getParent()), {builder.getInt32(ThreadgroupMemFlags), builder.getInt32(1)}
          );
          Changed = true;
        }
      }
    }
  }

  return (Changed ? PreservedAnalyses::none() : PreservedAnalyses::all());
}
} // namespace llvm::air