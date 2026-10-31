#pragma once

#include "d3d12.h"
#include <cstddef>
#include <cstdint>
#include <string>

// The capture folder's file writing lives in DXMT's core (Rust, src/dxmt/src/dump.rs); these are its C entry points,
// which the forwards below call.
extern "C" {
/* dxmt-abi begin */
bool dxmt_dump_on(void);
void dxmt_dump_save(const char *name, const void *data, size_t len);
void dxmt_dump_save_once(const char *name, const void *data, size_t len);
bool dxmt_dump_make_folder(const char *name);
void dxmt_dump_append_pipeline(const char *line, size_t len);
/* dxmt-abi end */
}

namespace dxmt {

// MacNeutron: with DXMT_DXIL_DUMP=<folder>, saves each DXIL shader a pipeline is created from as
// <folder>/<stage>-<FNV-1a 64 of the blob, 16 hex>.dxil. DXBC shaders are ignored, existing files are kept, and
// write errors are ignored, so capturing never changes what the game gets.
void DumpDXIL(const D3D12_SHADER_BYTECODE &Bytecode);

// True when DXMT_DXIL_DUMP is set. The device then reports what Unreal Engine's SM6 check needs (feature level 12_1,
// shader model up to 6.7, binding tier 3, wave ops, 64-bit atomics), so SM6-only games get as far as creating their
// DXIL pipelines, which are captured; a pipeline using an op the translator lacks fails with E_NOTIMPL.
#ifdef DXMT_PERF_BUILD // MacNeutron's performance build never captures
inline bool DXILCaptureMode() { return false; }
#else
inline bool DXILCaptureMode() { return dxmt_dump_on(); }
#endif

// Report what Shader Model 6 games check for (feature level 12_1, SM 6.7, binding tier 3, wave ops, 64-bit atomics):
// by default, so Unreal Engine 5 games start. DXMT_D3D12_SM6=0 reports SM 5.1, feature level 11_1, binding tier 2,
// no wave ops and no 64-bit atomics again, for a game whose SM6 path fails; capture mode always reports SM6.
// (MacNeutron)
bool SM6Caps();

// Capture mode also saves each root signature as <folder>/rs-<FNV-1a 64>.bin, and appends one line per pipeline to
// <folder>/pipelines.txt naming its shaders' and root signature's hashes (see CaptureHash), for offline replays.
uint64_t CaptureHash(const void *data, size_t size);
void DumpRootSignature(const void *blob, size_t size);

#ifdef DXMT_PERF_BUILD
inline void LogPipeline(const std::string &) {}
inline void SaveCapture(const char *, const void *, size_t) {}
inline bool MakeCaptureFolder(const char *) { return true; }
#else
inline void LogPipeline(const std::string &line) { dxmt_dump_append_pipeline(line.data(), line.size()); }

// Capture mode: saves `data` as <folder>/<name>, replacing any earlier file of that name.
inline void SaveCapture(const char *name, const void *data, size_t size) { dxmt_dump_save(name, data, size); }
// Creates <capture folder>\<name> (MacNeutron: per-F9 dump folders). False: the folder was already there (true when
// capture is off and on any other error, so F9's loop over folder numbers ends).
inline bool MakeCaptureFolder(const char *name) { return dxmt_dump_make_folder(name); }
#endif

// Pixel history (MacNeutron capture mode): a render pipeline's description ("gfx vs=... ps=... blend0=..."), by its
// Metal pipeline state handle, for naming the draws that change a pixel.
void NamePipeline(uint64_t pso, const std::string &name);
std::string PipelineName(uint64_t pso);

} // namespace dxmt
