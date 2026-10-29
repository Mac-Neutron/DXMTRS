// air-roundtrip (DXMTRS Phase 0, E2): reads a metallib airconv wrote, and either extracts its module's bitcode or
// writes the module back through airconv's MetallibWriter with the LLVM this tool is built against.
//   air-roundtrip extract <in.metallib> <out.bc>
//   air-roundtrip rewrite <in.metallib> <out.metallib>
// Built against LLVM 17 or later it writes opaque pointers; against an older LLVM, opaque ones too unless
// AIR_ROUNDTRIP_TYPED is defined (the typed-pointer fallback of the DXMTRS spec, §4).
#include "metallib_writer.hpp"
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
std::string_view Module(const std::string &file) {
  auto at = file.find(std::string_view("\xde\xc0\x17\x0b", 4));
  if (at == std::string::npos || file.size() - at < 20)
    return {};
  uint32_t header[5];
  memcpy(header, file.data() + at, sizeof(header));
  if (uint64_t(header[2]) + header[3] > file.size() - at)
    return {};
  return std::string_view(file).substr(at, header[2] + header[3]);
}

} // namespace

int main(int argc, char **argv) {
  if (argc != 4 || (strcmp(argv[1], "extract") && strcmp(argv[1], "rewrite"))) {
    fprintf(stderr, "usage: air-roundtrip extract|rewrite <in.metallib> <out>\n");
    return 2;
  }
  std::ifstream in(argv[2], std::ios::binary);
  std::string file((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
  auto bytes = Module(file);
  if (bytes.empty())
    return fail(std::string("no bitcode module in ") + argv[2]);
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
  std::string broken;
  llvm::raw_string_ostream why(broken);
  if (llvm::verifyModule(**module, &why))
    return fail("the module doesn't verify: " + why.str());
  dxmt::metallib::MetallibWriter().Write(**module, out);
  return 0;
}
