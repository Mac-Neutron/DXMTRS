#pragma once

#include "Metal.hpp"
#include <map>

namespace dxmt {
enum class FormatCapability : int {
  None = 0,
  Atomic = 0x1,
  Filter = 0x2,
  Write = 0x4,
  Color = 0x8,
  Blend = 0x10,
  MSAA = 0x20,
  Sparse = 0x40,
  Resolve = 0x80,
  DepthStencil = 0x100,
  TextureBufferRead = 0x200,
  TextureBufferWrite = 0x400,
  TextureBufferReadWrite = 0x800
};

class FormatCapabilityInspector {
public:
  std::map<WMTPixelFormat, FormatCapability> textureCapabilities{};
  void Inspect(WMT::Device device);
};

WMTPixelFormat Forget_sRGB(WMTPixelFormat format);
WMTPixelFormat Recall_sRGB(WMTPixelFormat format);

inline bool
Is_sRGBVariant(WMTPixelFormat format) {
  return Forget_sRGB(format) != format;
}

bool IsBlockCompressionFormat(WMTPixelFormat format);

uint32_t DepthStencilPlanarFlags(WMTPixelFormat format);

enum MTL_DXGI_FORMAT_FLAG {
  MTL_DXGI_FORMAT_TYPELESS = 1,
  MTL_DXGI_FORMAT_BC = 2,
  MTL_DXGI_FORMAT_BACKBUFFER = 4,
  MTL_DXGI_FORMAT_DEPTH_PLANER = 16,
  MTL_DXGI_FORMAT_STENCIL_PLANER = 32,
  MTL_DXGI_FORMAT_EMULATED_D24 = 256,
  MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL = 512,
};

struct MTL_DXGI_FORMAT_DESC {
  WMTPixelFormat PixelFormat;
  WMTAttributeFormat AttributeFormat;
  union {
    uint32_t BytesPerTexel;
    uint32_t BlockSize;
  };
  uint16_t PlanarCount;
  uint16_t Flag;
};

int32_t MTLQueryDXGIFormat(WMT::Device device, uint32_t format, MTL_DXGI_FORMAT_DESC &description);

uint32_t MTLGetTexelSize(WMTPixelFormat format);

WMTPixelFormat MTLGetUnsignedIntegerFormat(WMTPixelFormat format);

bool IsUnorm8RenderTargetFormat(WMTPixelFormat format);

bool IsIntegerFormat(WMTPixelFormat format);

void SanitizeRTVClearColor(WMTPixelFormat format, WMTClearColor &color);

// MacNeutron (DXMTRS step 5a): the functions above forward to the Rust core (src/dxmt/src/format.rs), which exports
// these. src/dxmt/src/format/abi.rs, generated from this block by tools/winemetal-layouts.sh --seam-abi, asserts each
// Rust export against its declaration on every target; a width written differently here fails that build.
extern "C" {
/* dxmt-abi begin */
struct MTL_DXGI_FORMAT_DESC;

struct dxmt_format_capability {
  uint32_t format;
  uint32_t caps;
};

/* HRESULT of MTLQueryDXGIFormat; *out is always written (E_FAIL leaves the C++'s defaults) */
int32_t dxmt_format_query_dxgi(uint32_t dxgi_format, struct MTL_DXGI_FORMAT_DESC *out);
enum WMTPixelFormat dxmt_format_forget_srgb(enum WMTPixelFormat format);
enum WMTPixelFormat dxmt_format_recall_srgb(enum WMTPixelFormat format);
bool dxmt_format_is_bc(enum WMTPixelFormat format);
uint32_t dxmt_format_depth_stencil_planar_flags(enum WMTPixelFormat format);
uint32_t dxmt_format_texel_size(enum WMTPixelFormat format);
enum WMTPixelFormat dxmt_format_unsigned_integer(enum WMTPixelFormat format);
bool dxmt_format_is_unorm8_rt(enum WMTPixelFormat format);
bool dxmt_format_is_integer(enum WMTPixelFormat format);
void dxmt_format_sanitize_clear_color(enum WMTPixelFormat format, struct WMTClearColor *color);
/* Writes min(count, capacity) entries sorted by format and returns count; SIZE_MAX for a device that is neither Apple7
 * nor Mac2, for which FormatCapabilityInspector::Inspect throws */
size_t dxmt_format_capabilities(obj_handle_t device, struct dxmt_format_capability *out, size_t capacity);
/* dxmt-abi end */
}

} // namespace dxmt