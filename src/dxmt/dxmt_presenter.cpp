#include "dxmt_presenter.hpp"

// MacNeutron (DXMTRS step 5a): the presenter is Rust, in src/dxmt/src/presenter.rs. Everything here forwards to the
// dxmt_presenter_* functions declared in dxmt_presenter.hpp.

namespace dxmt {

Presenter::Presenter(
    WMT::Device device, WMT::MetalLayer layer, InternalCommandLibrary &lib, float scale_factor, uint8_t sample_count
) :
    rust_(dxmt_presenter_create(device, layer, lib.getLibrary(), scale_factor, sample_count)) {}

Presenter::~Presenter() {
  dxmt_presenter_destroy(rust_);
}

bool
Presenter::changeLayerProperties(
    WMTPixelFormat format, WMTColorSpace colorspace, double width, double height, uint8_t sample_count
) {
  return dxmt_presenter_change_layer_properties(rust_, format, colorspace, width, height, sample_count);
}

bool
Presenter::changeLayerColorSpace(WMTColorSpace colorspace) {
  return dxmt_presenter_change_layer_color_space(rust_, colorspace);
}

void
Presenter::changeHDRMetadata(const WMTHDRMetadata *metadata) {
  dxmt_presenter_change_hdr_metadata(rust_, metadata);
}

void
Presenter::changeGammaRamp(const DXMTGammaRamp *gamma_ramp) {
  dxmt_presenter_change_gamma_ramp(rust_, gamma_ramp);
}

Presenter::PresentState
Presenter::synchronizeLayerProperties() {
  DXMTPresentMetadata metadata;
  uint64_t frame_id = dxmt_presenter_synchronize(rust_, &metadata);
  return {metadata, frame_id, this};
}

WMT::Reference<WMT::MetalDrawable>
Presenter::encodeCommands(
    WMT::CommandBuffer cmdbuf, WMT::Texture backbuffer, DXMTPresentMetadata metadata,
    std::function<void(WMT::RenderCommandEncoder)> &&wait_fences,
    std::function<void(WMT::RenderCommandEncoder)> &&update_fences
) {
  // noexcept: a C++ exception ends in C++ and never unwinds through the Rust caller.
  struct Fences {
    std::function<void(WMT::RenderCommandEncoder)> &wait, &update;
  } fences{wait_fences, update_fences};
  dxmt_presenter_callbacks callbacks{
      [](void *ctx, obj_handle_t encoder) noexcept {
        static_cast<Fences *>(ctx)->wait(WMT::RenderCommandEncoder{encoder});
      },
      [](void *ctx, obj_handle_t encoder) noexcept {
        static_cast<Fences *>(ctx)->update(WMT::RenderCommandEncoder{encoder});
      },
      &fences,
  };
  return WMT::Reference<WMT::MetalDrawable>(dxmt_presenter_encode(rust_, cmdbuf, backbuffer, &metadata, &callbacks));
}

} // namespace dxmt
