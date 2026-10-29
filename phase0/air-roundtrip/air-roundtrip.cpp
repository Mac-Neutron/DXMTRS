// air-roundtrip (DXMTRS Phase 0, E2): reads a metallib airconv wrote, and either extracts its module's bitcode or
// writes the module back through airconv's MetallibWriter with the LLVM this tool is built against.
//   air-roundtrip extract <in.metallib> <out.bc>
//   air-roundtrip rewrite <in.metallib> <out.metallib>
//   air-roundtrip dis <in.metallib> <out.ll>  (the module as this LLVM reads it, as text)
// Built against LLVM 17 or later it writes opaque pointers; against an older LLVM, opaque ones too unless
// AIR_ROUNDTRIP_TYPED is defined (the typed-pointer fallback of the DXMTRS spec, §4). With AIR_ROUNDTRIP_DXIL_WRITER
// (LLVM 23 with its DirectX target; metallib_writer.cpp compiled with WriteBitcodeToFile renamed, build.sh --dxil),
// the module is written by LLVM's DXIL bitcode writer instead: typed pointers inferred from the opaque IR
// (PointerTypeAnalysis), in its old (3.7) record format.
#include "metallib_writer.hpp"
#ifdef AIR_ROUNDTRIP_DXIL_WRITER
#include "DXILWriter/DXILBitcodeWriter.h"
#include "DirectXIRPasses/DXILAttributes.h"
#include "DirectXIRPasses/DXILDebugInfo.h"
#include "DirectXIRPasses/PointerTypeAnalysis.h"
#include "llvm/IR/IRBuilder.h"
#include "llvm/IR/Instructions.h"
#include "llvm/Bitcode/BitcodeWriter.h"
#endif
#include "llvm/Bitcode/BitcodeReader.h"
#include "llvm/Config/llvm-config.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/Verifier.h"
#include "llvm/Support/Error.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/raw_ostream.h"
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iterator>
#include <string>
#include <string_view>

namespace {

int fail(const std::string &what) {
  fprintf(stderr, "air-roundtrip: %s\n", what.c_str());
  return 1;
}

// The module's bytes: LLVM's bitcode wrapper (magic, version, offset, size, cputype) and the bitcode it frames.
std::string_view ModuleBytes(const std::string &file) {
  auto at = file.find(std::string_view("\xde\xc0\x17\x0b", 4));
  if (at == std::string::npos || file.size() - at < 20)
    return {};
  uint32_t header[5];
  memcpy(header, file.data() + at, sizeof(header));
  if (uint64_t(header[2]) + header[3] > file.size() - at)
    return {};
  return std::string_view(file).substr(at, header[2] + header[3]);
}

uint32_t cpu_type = ~0u; // the input's wrapper header's, for the DXIL writer's output

} // namespace

#ifdef AIR_ROUNDTRIP_DXIL_WRITER
namespace {

using namespace llvm;

// DXILPrepare's step before the DXIL writer: where a load, store or GEP uses a pointer as another type than the one
// inferred for it, a no-op bitcast in between, which the writer writes as a typed bitcast.
Value *Cast(IRBuilder<> &builder, const dxil::PointerTypeMap &types, Value *pointer, Type *type) {
  if (type->isPointerTy()) // a pointer access: the analysis types the pointer from the loaded or stored one
    return nullptr;
  if (auto it = types.find(pointer); it != types.end() && cast<TypedPointerType>(it->second)->getElementType() == type)
    return nullptr;
  Type *value_type = pointer->getType();
  if (auto *global = dyn_cast<GlobalVariable>(pointer))
    value_type = global->getValueType();
  if (auto *alloca = dyn_cast<AllocaInst>(pointer))
    value_type = alloca->getAllocatedType();
  if (auto *array = dyn_cast<ArrayType>(value_type); array && array->getElementType() == type)
    return nullptr;
  return builder.Insert(CastInst::Create(Instruction::BitCast, pointer,
                                         builder.getPtrTy(pointer->getType()->getPointerAddressSpace())));
}

void Prepare(Module &module) {
  // DXILPrepare's other step: the enum attributes the writer's old record format has no code for (it would write kind
  // 0). AIR's string attributes stay.
  const AttributeMask &newer = dxil::getNonDXILAttributeMask();
  auto strip = [&](auto &target, size_t args) {
    target.removeFnAttrs(newer);
    target.removeRetAttrs(newer);
    for (size_t i = 0; i < args; i++)
      target.removeParamAttrs(i, newer);
  };
  auto types = dxil::PointerTypeAnalysis::run(module);
  for (auto &function : module) {
    strip(function, function.arg_size());
    for (auto &block : function)
      for (auto &inst : block)
        if (auto *call = dyn_cast<CallBase>(&inst))
          strip(*call, call->arg_size());
  }
  // A pointer select's or phi's operands must have its type exactly (bitcode types it by them): an operand typed
  // otherwise goes through a no-op bitcast, and a null through inttoptr 0, which the analysis types by its one user
  // (the writer would type a bare null i8* in address space 0).
  auto operand = [&](Instruction *at, Value *value, Type *target) -> Value * {
    if (isa<ConstantPointerNull>(value))
      return CastInst::Create(Instruction::IntToPtr, ConstantInt::get(Type::getInt64Ty(module.getContext()), 0),
                              value->getType(), "", at->getIterator());
    auto it = types.find(value);
    if (it != types.end() && it->second == target)
      return nullptr;
    return CastInst::Create(Instruction::BitCast, value, value->getType(), "", at->getIterator());
  };
  for (auto &function : module)
    for (auto &block : function)
      for (auto &inst : make_early_inc_range(block)) {
        if (!inst.getType()->isPointerTy())
          continue;
        auto it = types.find(&inst);
        Type *target = it == types.end() ? nullptr : it->second;
        if (auto *select = dyn_cast<SelectInst>(&inst)) {
          for (unsigned i : {1u, 2u})
            if (auto *v = operand(select, select->getOperand(i), target))
              select->setOperand(i, v);
        } else if (auto *phi = dyn_cast<PHINode>(&inst)) {
          for (unsigned i = 0; i < phi->getNumIncomingValues(); i++)
            if (auto *v = operand(phi->getIncomingBlock(i)->getTerminator(), phi->getIncomingValue(i), target))
              phi->setIncomingValue(i, v);
        }
      }
  for (auto &function : module)
    for (auto &block : function) {
      IRBuilder<> builder(&block);
      for (auto &inst : make_early_inc_range(block)) {
        builder.SetInsertPoint(&inst);
        if (auto *load = dyn_cast<LoadInst>(&inst)) {
          if (auto *p = Cast(builder, types, load->getPointerOperand(), load->getType()))
            load->setOperand(0, p);
        } else if (auto *store = dyn_cast<StoreInst>(&inst)) {
          if (auto *p = Cast(builder, types, store->getPointerOperand(), store->getValueOperand()->getType()))
            store->setOperand(1, p);
        } else if (auto *gep = dyn_cast<GetElementPtrInst>(&inst)) {
          if (auto *p = Cast(builder, types, gep->getPointerOperand(), gep->getSourceElementType()))
            gep->setOperand(0, p);
        }
      }
    }
}

// AIR_ROUNDTRIP_DEBUG: what the writer will still see as a mismatch (the analysis rerun on the prepared module).
void Mismatches(Module &module) {
  auto types = dxil::PointerTypeAnalysis::run(module);
  auto check = [&](Instruction &inst, Value *pointer, Type *type) {
    auto it = types.find(pointer);
    Type *pointee = it == types.end() ? nullptr : cast<TypedPointerType>(it->second)->getElementType();
    if (pointee != type) {
      errs() << "mismatch: " << inst << "\n  pointer: " << *pointer << "\n  pointee: ";
      if (pointee) errs() << *pointee; else errs() << "(none)";
      errs() << "\n";
    }
  };
  for (auto &function : module)
    for (auto &block : function)
      for (auto &inst : block) {
        if (auto *load = dyn_cast<LoadInst>(&inst))
          check(inst, load->getPointerOperand(), load->getType());
        else if (auto *store = dyn_cast<StoreInst>(&inst))
          check(inst, store->getPointerOperand(), store->getValueOperand()->getType());
      }
}

} // namespace

namespace llvm {
// metallib_writer.cpp's WriteBitcodeToFile, renamed to this by build.sh --dxil: the DXIL writer leaves a Darwin
// triple's wrapper header zeroed, so it is filled here as WriteBitcodeToFile fills it (and padded to 16 bytes).
void air_roundtrip_write_bitcode(const Module &M, raw_ostream &Out, bool, const ModuleSummaryIndex *, bool,
                                 ModuleHash *) {
  SmallVector<char, 0> buffer;
  raw_svector_ostream os(buffer);
  dxil::WriteDXILToFile(M, os, dxil::DXILDebugInfoMap());
  uint32_t header[5] = {0x0b17c0de, 0, 20, uint32_t(buffer.size() - 20), cpu_type};
  memcpy(buffer.data(), header, sizeof(header));
  while (buffer.size() & 15)
    buffer.push_back(0);
  Out.write(buffer.data(), buffer.size());
}
} // namespace llvm
#endif

int main(int argc, char **argv) {
  if (argc != 4 || (strcmp(argv[1], "extract") && strcmp(argv[1], "rewrite") && strcmp(argv[1], "dis"))) {
    fprintf(stderr, "usage: air-roundtrip extract|rewrite|dis <in.metallib> <out>\n");
    return 2;
  }
  std::ifstream in(argv[2], std::ios::binary);
  std::string file((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
  auto bytes = ModuleBytes(file);
  if (bytes.empty())
    return fail(std::string("no bitcode module in ") + argv[2]);
  memcpy(&cpu_type, bytes.data() + 16, sizeof(cpu_type));
  std::error_code ec;
  llvm::raw_fd_ostream out(argv[3], ec);
  if (ec)
    return fail(std::string("can't write ") + argv[3] + ": " + ec.message());
  if (!strcmp(argv[1], "extract")) {
    out << llvm::StringRef(bytes.data(), bytes.size());
    return 0;
  }
  llvm::LLVMContext context;
#if LLVM_VERSION_MAJOR < 17
#ifdef AIR_ROUNDTRIP_TYPED
  context.setOpaquePointers(false);
#else
  context.setOpaquePointers(true);
#endif
#endif
  auto buffer = llvm::MemoryBuffer::getMemBuffer(llvm::StringRef(bytes.data(), bytes.size()), "", false);
  auto module = llvm::parseBitcodeFile(buffer->getMemBufferRef(), context);
  if (!module)
    return fail("reading: " + llvm::toString(module.takeError()));
  if (!strcmp(argv[1], "dis")) {
    (*module)->print(out, nullptr);
    return 0;
  }
  std::string broken;
  llvm::raw_string_ostream why(broken);
  if (llvm::verifyModule(**module, &why))
    return fail("the module doesn't verify: " + why.str());
#ifdef AIR_ROUNDTRIP_DXIL_WRITER
  Prepare(**module);
  if (getenv("AIR_ROUNDTRIP_DEBUG"))
    Mismatches(**module);
  if (const char *drop = getenv("AIR_ROUNDTRIP_DROP")) { // bisecting a reader error: the n-th load or store goes
    int n = atoi(drop);
    for (auto &function : **module)
      for (auto &block : function)
        for (auto &inst : llvm::make_early_inc_range(block))
          if ((llvm::isa<llvm::LoadInst>(inst) || llvm::isa<llvm::StoreInst>(inst)) && n-- == 0) {
            llvm::errs() << "dropped: " << inst << "\n";
            inst.replaceAllUsesWith(llvm::PoisonValue::get(inst.getType()));
            inst.eraseFromParent();
          }
  }
#endif
  dxmt::metallib::MetallibWriter().Write(**module, out);
  return 0;
}
