// air-vs-msl <manifest> <air-dir> <msl-dir> <salt> (DXMTRS Phase 0, E3): the build latency of each manifest pipeline from
// airconv's AIR (<air-dir>/<name>.dxil.metallib, newLibraryWithData) and from SPIRV-Cross's MSL (<msl-dir>/<name>.metal,
// newLibraryWithSource), library and pipeline state timed apart. Manifest lines (shaders.sh): `compute <cs>`,
// `vertex <vs>` (no fragment function) or `render <vs> <ps>`. Prints `<name> air <lib-ms> <pso-ms> msl <lib-ms> <pso-ms>`,
// `skip <name> <side> <why>` for a pipeline either path can't build, then `total air <ms> msl <ms> over <n>`: the sums
// over the pipelines both paths built. Metal's shader cache is per user and keyed by content: each MSL source gets a
// first line `// <salt>` (argv[4], new for every run), so its frontend compile, what AIR skips, is never a cache hit.
// The pipeline (backend) compile can still be one for either path.
#import <Metal/Metal.h>
#include <chrono>
#include <cstdio>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

namespace {

using Clock = std::chrono::steady_clock;
std::string salt; // main's argv[4]
double Ms(Clock::time_point a, Clock::time_point b) { return std::chrono::duration<double, std::milli>(b - a).count(); }

std::string Read(const std::string &path) {
  std::ifstream in(path, std::ios::binary);
  std::stringstream s;
  s << in.rdbuf();
  return s.str();
}

struct Build {
  double lib = 0, pso = 0;
  std::string error; // empty: built
};

// The vertex descriptor a vertex function's [[stage_in]] attributes need: each a 16-byte slot of buffer 29.
MTLVertexDescriptor *Descriptor(id<MTLFunction> vs) {
  if (!vs.vertexAttributes.count)
    return nil;
  auto *d = [MTLVertexDescriptor vertexDescriptor];
  for (MTLVertexAttribute *a in vs.vertexAttributes) {
    if (!a.active)
      continue;
    MTLVertexFormat f = MTLVertexFormatFloat4;
    switch (a.attributeType) {
    case MTLDataTypeFloat: f = MTLVertexFormatFloat; break;
    case MTLDataTypeFloat2: f = MTLVertexFormatFloat2; break;
    case MTLDataTypeFloat3: f = MTLVertexFormatFloat3; break;
    case MTLDataTypeInt: f = MTLVertexFormatInt; break;
    case MTLDataTypeInt2: f = MTLVertexFormatInt2; break;
    case MTLDataTypeInt3: f = MTLVertexFormatInt3; break;
    case MTLDataTypeInt4: f = MTLVertexFormatInt4; break;
    case MTLDataTypeUInt: f = MTLVertexFormatUInt; break;
    case MTLDataTypeUInt2: f = MTLVertexFormatUInt2; break;
    case MTLDataTypeUInt3: f = MTLVertexFormatUInt3; break;
    case MTLDataTypeUInt4: f = MTLVertexFormatUInt4; break;
    default: break;
    }
    d.attributes[a.attributeIndex].format = f;
    d.attributes[a.attributeIndex].offset = 16 * a.attributeIndex;
    d.attributes[a.attributeIndex].bufferIndex = 29;
  }
  d.layouts[29].stride = 16 * 32;
  return d;
}

// One path's pipeline: libraries from each name's file, then the pipeline state.
Build Pipeline(id<MTLDevice> device, bool air, const std::string &dir, const std::string &kind,
               const std::vector<std::string> &names) {
  Build b;
  std::vector<id<MTLFunction>> functions;
  auto t0 = Clock::now();
  for (auto &name : names) {
    NSError *err = nil;
    id<MTLLibrary> lib;
    if (air) {
      auto bytes = Read(dir + "/" + name + ".dxil.metallib");
      auto data = dispatch_data_create(bytes.data(), bytes.size(), nullptr, DISPATCH_DATA_DESTRUCTOR_DEFAULT);
      lib = [device newLibraryWithData:data error:&err];
    } else {
      auto source = "// " + salt + "\n" + Read(dir + "/" + name + ".metal");
      auto *options = [MTLCompileOptions new];
      options.languageVersion = MTLLanguageVersion3_2;
      lib = [device newLibraryWithSource:[NSString stringWithUTF8String:source.c_str()] options:options error:&err];
    }
    if (!lib || !lib.functionNames.count) {
      b.error = lib ? "no function" : err.localizedDescription.UTF8String;
      return b;
    }
    functions.push_back([lib newFunctionWithName:lib.functionNames.firstObject]);
  }
  auto t1 = Clock::now();
  NSError *err = nil;
  id pso;
  if (kind == "compute") {
    pso = [device newComputePipelineStateWithFunction:functions[0] error:&err];
  } else {
    auto *d = [MTLRenderPipelineDescriptor new];
    d.vertexFunction = functions[0];
    d.vertexDescriptor = Descriptor(functions[0]);
    d.inputPrimitiveTopology = MTLPrimitiveTopologyClassTriangle; // for a render target array index
    d.depthAttachmentPixelFormat = MTLPixelFormatDepth32Float;
    if (kind == "render") {
      d.fragmentFunction = functions[1];
      for (int i = 0; i < 8; i++)
        d.colorAttachments[i].pixelFormat = MTLPixelFormatRGBA32Float;
    }
    pso = [device newRenderPipelineStateWithDescriptor:d error:&err];
  }
  auto t2 = Clock::now();
  if (!pso) {
    b.error = err.localizedDescription.UTF8String;
    return b;
  }
  b.lib = Ms(t0, t1), b.pso = Ms(t1, t2);
  return b;
}

std::string FirstLine(const std::string &s) { return s.substr(0, s.find('\n')); }

} // namespace

int main(int argc, char **argv) {
  if (argc != 5) {
    fprintf(stderr, "usage: air-vs-msl <manifest> <air-dir> <msl-dir> <salt>\n");
    return 2;
  }
  salt = argv[4];
  id<MTLDevice> device = MTLCreateSystemDefaultDevice();
  std::ifstream manifest(argv[1]);
  std::string line;
  double air_total = 0, msl_total = 0;
  unsigned both = 0;
  while (std::getline(manifest, line)) {
    std::istringstream words(line);
    std::string kind, name;
    std::vector<std::string> names;
    words >> kind;
    while (words >> name)
      names.push_back(name);
    if (names.empty())
      continue;
    std::string label = names.back();
    Build a, m;
    @autoreleasepool {
      a = Pipeline(device, true, argv[2], kind, names);
      m = Pipeline(device, false, argv[3], kind, names);
    }
    if (!a.error.empty() || !m.error.empty()) {
      printf("skip %s %s %s\n", label.c_str(), a.error.empty() ? "msl" : "air",
             FirstLine(a.error.empty() ? m.error : a.error).c_str());
      continue;
    }
    printf("%s air %.2f %.2f msl %.2f %.2f\n", label.c_str(), a.lib, a.pso, m.lib, m.pso);
    air_total += a.lib + a.pso, msl_total += m.lib + m.pso, both++;
  }
  printf("total air %.1f msl %.1f over %u\n", air_total, msl_total, both);
  return both ? 0 : 1;
}
