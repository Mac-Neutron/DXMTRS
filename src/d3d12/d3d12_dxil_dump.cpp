#include "d3d12_dxil_dump.hpp"
#include "util_env.hpp"
#include <windows.h>
#include "util_win32_compat.h"
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <unordered_map>

namespace dxmt {

namespace {

uint32_t read32(const uint8_t *p) {
  uint32_t value;
  memcpy(&value, p, 4);
  return value;
}

// The stage in the DXIL part's program header, or nullptr for DXBC or a malformed container.
const char *DXILStage(const uint8_t *blob, size_t size) {
  static const char *const stages[] = {"ps", "vs", "gs", "hs", "ds", "cs", "lib", "raygen",
                                       "intersection", "anyhit", "closesthit", "miss", "callable", "ms", "as", "node"};
  if (size < 32 || memcmp(blob, "DXBC", 4))
    return nullptr;
  uint32_t parts = read32(blob + 28);
  for (uint64_t i = 0; i < parts && 32 + 4 * (i + 1) <= size; i++) {
    uint64_t offset = read32(blob + 32 + 4 * i);
    if (offset + 12 > size || memcmp(blob + offset, "DXIL", 4))
      continue;
    uint32_t kind = read32(blob + offset + 8) >> 16;
    return kind < sizeof(stages) / sizeof(*stages) ? stages[kind] : "unknown";
  }
  return nullptr;
}

} // namespace

bool
SM6Caps() {
  static const bool on = DXILCaptureMode() || env::getEnvVar("DXMT_D3D12_SM6") != "0";
  return on;
}

uint64_t CaptureHash(const void *data, size_t size) {
  auto bytes = static_cast<const uint8_t *>(data);
  uint64_t hash = 0xcbf29ce484222325ull;
  for (size_t i = 0; i < size; i++)
    hash = (hash ^ bytes[i]) * 0x100000001b3ull;
  return hash;
}

namespace {

// Saves `blob` as <folder>\<name> unless a file of that name is there (see DumpDXIL).
void SaveOnce(const char *name, const void *blob, size_t size) {
#ifndef DXMT_PERF_BUILD // never called there: capture mode is off
  dxmt_dump_save_once(name, blob, size);
#endif
}

} // namespace

void DumpDXIL(const D3D12_SHADER_BYTECODE &Bytecode) {
  if (!DXILCaptureMode() || !Bytecode.pShaderBytecode)
    return;
  auto blob = static_cast<const uint8_t *>(Bytecode.pShaderBytecode);
  const char *stage = DXILStage(blob, Bytecode.BytecodeLength);
  if (!stage)
    return;
  char name[64];
  snprintf(name, sizeof(name), "%s-%016llx.dxil", stage, (unsigned long long)CaptureHash(blob, Bytecode.BytecodeLength));
  SaveOnce(name, blob, Bytecode.BytecodeLength);
}

void DumpRootSignature(const void *blob, size_t size) {
  if (!DXILCaptureMode() || !blob)
    return;
  char name[64];
  snprintf(name, sizeof(name), "rs-%016llx.bin", (unsigned long long)CaptureHash(blob, size));
  SaveOnce(name, blob, size);
}

namespace {
std::mutex pipeline_names_mutex;
std::unordered_map<uint64_t, std::string> pipeline_names;
} // namespace

void NamePipeline(uint64_t pso, const std::string &name) {
  if (!DXILCaptureMode() || !pso)
    return;
  std::lock_guard<std::mutex> lock(pipeline_names_mutex);
  pipeline_names[pso] = name;
}

std::string PipelineName(uint64_t pso) {
  std::lock_guard<std::mutex> lock(pipeline_names_mutex);
  auto found = pipeline_names.find(pso);
  if (found != pipeline_names.end())
    return found->second;
  char text[32];
  snprintf(text, sizeof(text), "pso-%llx", (unsigned long long)pso);
  return text;
}

} // namespace dxmt
