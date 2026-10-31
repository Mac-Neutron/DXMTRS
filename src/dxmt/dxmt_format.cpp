#include "dxmt_format.hpp"
#include "util_error.hpp"
#include <cstdint>
#include <vector>

// MacNeutron (DXMTRS step 5a): the format tables are Rust, in src/dxmt/src/format.rs. Everything here forwards to
// the dxmt_format_* functions declared in dxmt_format.hpp.

namespace dxmt {

void
FormatCapabilityInspector::Inspect(WMT::Device device) {
  std::vector<dxmt_format_capability> list(128);
  size_t count = dxmt_format_capabilities(device, list.data(), list.size());
  if (count == SIZE_MAX)
    throw MTLD3DError("Invalid MTLDevice");
  if (count > list.size()) {
    list.resize(count);
    count = dxmt_format_capabilities(device, list.data(), list.size());
  }
  for (size_t i = 0; i < count; i++)
    textureCapabilities[(WMTPixelFormat)list[i].format] = (FormatCapability)list[i].caps;
}

WMTPixelFormat
Forget_sRGB(WMTPixelFormat format) {
  return dxmt_format_forget_srgb(format);
}

WMTPixelFormat
Recall_sRGB(WMTPixelFormat format) {
  return dxmt_format_recall_srgb(format);
}

bool
IsBlockCompressionFormat(WMTPixelFormat format) {
  return dxmt_format_is_bc(format);
}

uint32_t
DepthStencilPlanarFlags(WMTPixelFormat format) {
  return dxmt_format_depth_stencil_planar_flags(format);
}

int32_t
MTLQueryDXGIFormat(WMT::Device device, uint32_t format, MTL_DXGI_FORMAT_DESC &description) {
  return dxmt_format_query_dxgi(format, &description);
}

uint32_t
MTLGetTexelSize(WMTPixelFormat format) {
  return dxmt_format_texel_size(format);
}

WMTPixelFormat
MTLGetUnsignedIntegerFormat(WMTPixelFormat format) {
  return dxmt_format_unsigned_integer(format);
}

bool
IsUnorm8RenderTargetFormat(WMTPixelFormat format) {
  return dxmt_format_is_unorm8_rt(format);
}

bool
IsIntegerFormat(WMTPixelFormat format) {
  return dxmt_format_is_integer(format);
}

void
SanitizeRTVClearColor(WMTPixelFormat format, WMTClearColor &color) {
  dxmt_format_sanitize_clear_color(format, &color);
}

} // namespace dxmt
