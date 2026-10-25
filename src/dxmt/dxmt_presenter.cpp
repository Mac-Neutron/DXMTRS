
#include <algorithm>
#include "Metal.hpp"
#include "dxmt_format.hpp"
#include "dxmt_presenter.hpp"
#include "util_likely.hpp"


namespace dxmt {

Presenter::Presenter(WMT::Device device, WMT::MetalLayer layer, InternalCommandLibrary &lib, float scale_factor, uint8_t sample_count) :
    device_(device),
    layer_(layer),
    lib_(lib),
    sample_count_(sample_count) {
  layer_.getProps(layer_props_);
  layer_props_.device = device;
  layer_props_.opaque = true;
  layer_props_.display_sync_enabled = false;
  layer_props_.framebuffer_only = false; // how strangely setting it true results in worse performance
  layer_props_.contents_scale = layer_props_.contents_scale * scale_factor;

  WMTTextureInfo texture_info{};
  texture_info.type = WMTTextureType2D;
  texture_info.pixel_format = WMTPixelFormatRGBA32Float;
  texture_info.usage = WMTTextureUsageShaderRead;
  texture_info.options = WMTResourceStorageModeShared;
  texture_info.width = DXMT_GAMMA_CP_COUNT;
  texture_info.height = 1;
  texture_info.depth = 1;
  texture_info.mipmap_level_count = 1;
  texture_info.sample_count = 1;
  texture_info.array_length = 1;
  gamma_lut_texture_ = device.newTexture(texture_info);
#ifdef DXMT_NATIVE
  // Direct3D forwarding's VM host sets the WSI hooks before any device; the native build without them (a test, or
  // macOS) keeps the synchronous acquire.
  prefetch_drawable_ = WMTGetWsiHooks() != nullptr;
  if (prefetch_drawable_) {
    drawable_thread_ = dxmt::thread([this] { drawableWorker(); });
    if (!drawable_thread_.joinable())
      prefetch_drawable_ = false;
  }
#endif
}

Presenter::~Presenter() {
#ifdef DXMT_NATIVE
  if (drawable_thread_.joinable()) {
    {
      std::unique_lock<dxmt::mutex> lock(drawable_mutex_);
      drawable_stop_ = true;
    }
    drawable_cv_.notify_all();
    drawable_thread_.join();
  }
#endif
}

#ifdef DXMT_NATIVE
// The drawable the layer hands out is autoreleased: retain it in the worker's pool (WMT::Reference does), and hand it
// to the present that asked while the display's next refresh is still to come.
void Presenter::drawableWorker() {
  for (;;) {
    std::unique_lock<dxmt::mutex> lock(drawable_mutex_);
    drawable_cv_.wait(lock, [&] { return drawable_stop_ || drawable_wanted_; });
    if (drawable_stop_)
      return;
    drawable_wanted_ = false;
    drawable_busy_ = true;
    uint64_t generation = drawable_generation_;
    lock.unlock();
    WMT::Reference<WMT::MetalDrawable> drawable;
    {
      auto pool = WMT::MakeAutoreleasePool();
      drawable = WMT::Reference<WMT::MetalDrawable>(layer_.nextDrawable());
    }
    lock.lock();
    if (!drawable) { // an occluded layer's timeout: not a spin for the worker to try again at once
      lock.unlock();
      std::this_thread::sleep_for(std::chrono::milliseconds(1));
      lock.lock();
    }
    drawable_busy_ = false;
    if (!drawable_stop_ && generation == drawable_generation_ && !prefetched_drawable_) {
      prefetched_drawable_ = std::move(drawable);
      prefetched_generation_ = generation;
    }
    drawable_cv_.notify_all();
  }
}

// Under drawable_mutex_.
void Presenter::requestDrawableLocked() {
  if (drawable_stop_ || drawable_wanted_ || drawable_busy_ || prefetched_drawable_)
    return;
  drawable_wanted_ = true;
  drawable_cv_.notify_all();
}

// A layer property change (size, format, color space) makes a prefetched drawable the old one: drop it, and the
// worker's next stale result with it.
void Presenter::invalidateDrawable() {
  std::unique_lock<dxmt::mutex> lock(drawable_mutex_);
  drawable_generation_++;
  prefetched_drawable_ = nullptr;
  drawable_cv_.notify_all();
}

WMT::Reference<WMT::MetalDrawable> Presenter::acquireDrawable() {
  for (;;) {
    std::unique_lock<dxmt::mutex> lock(drawable_mutex_);
    if (drawable_stop_)
      return {};
    if (prefetched_drawable_ && prefetched_generation_ == drawable_generation_) {
      WMT::Reference<WMT::MetalDrawable> drawable = std::move(prefetched_drawable_);
      prefetched_drawable_ = nullptr;
      requestDrawableLocked();
      return drawable;
    }
    prefetched_drawable_ = nullptr;
    requestDrawableLocked();
    drawable_cv_.wait(lock, [&] { return prefetched_drawable_ || drawable_stop_; });
  }
}
#endif

bool
Presenter::changeLayerProperties(
    WMTPixelFormat format, WMTColorSpace colorspace, double width, double height, uint8_t sample_count
) {
  bool should_invalidated = colorspace_ != colorspace;
  should_invalidated |= source_format_ != format;
  source_format_ = format;
  should_invalidated |= sample_count_ != sample_count;
  sample_count_ = sample_count;
  layer_props_.pixel_format = Forget_sRGB(format);
  layer_props_.drawable_height = height;
  layer_props_.drawable_width = width;
  colorspace_ = colorspace;
  if (should_invalidated)
    pso_valid.clear(); // defer changes
  else
    layer_.setProps(layer_props_);
#ifdef DXMT_NATIVE
  invalidateDrawable();
#endif
  return should_invalidated;
}

bool
Presenter::changeLayerColorSpace(WMTColorSpace colorspace) {
  bool should_invalidated = colorspace_ != colorspace;
  colorspace_ = colorspace;
  if (should_invalidated)
    pso_valid.clear();
#ifdef DXMT_NATIVE
  invalidateDrawable();
#endif
  return should_invalidated;
}

void
Presenter::changeHDRMetadata(const WMTHDRMetadata *metadata) {
  if (metadata) {
    has_hdr_metadata_ = true;
    hdr_metadata_ = *metadata;
    pso_valid.clear();
  } else {
    if (has_hdr_metadata_) {
      has_hdr_metadata_ = false;
      pso_valid.clear();
    }
  }
}

void
Presenter::changeGammaRamp(const DXMTGammaRamp *gamma_ramp) {
  if (!gamma_ramp && gamma_version_ != 0) {
    gamma_version_ = 0;
    pso_valid.clear();
    return;
  }
  if (gamma_ramp && gamma_version_ != gamma_ramp->version) {
    gamma_version_ = gamma_ramp->version;
    for (uint32_t i = 0; i < DXMT_GAMMA_CP_COUNT; i++) {
      gamma_lut_rgba_[i * 4 + 0] = std::clamp(gamma_ramp->red[i], 0.f, 1.f);
      gamma_lut_rgba_[i * 4 + 1] = std::clamp(gamma_ramp->green[i], 0.f, 1.f);
      gamma_lut_rgba_[i * 4 + 2] = std::clamp(gamma_ramp->blue[i], 0.f, 1.f);
      gamma_lut_rgba_[i * 4 + 3] = 1.0f;
    }
    gamma_lut_texture_.replaceRegion(
          {0, 0, 0}, {DXMT_GAMMA_CP_COUNT, 1, 1}, 0, 0,
          gamma_lut_rgba_.data(),
          DXMT_GAMMA_CP_COUNT * sizeof(float) * 4,
          0);
    pso_valid.clear();
  }
}

Presenter::PresentState
Presenter::synchronizeLayerProperties() {
  uint64_t display_setting_version = 0;

  WMTQueryDisplaySettingForLayer(
      layer_.handle, &display_setting_version, &display_colorspace_, &display_hdr_metadata_, &display_edr_value_
  );

  if (display_setting_version != display_setting_version_) {
    display_setting_version_ = display_setting_version;
    pso_valid.clear();
  }

  auto final_colorspace = display_setting_version_ > 0 ? display_colorspace_ : colorspace_;
  auto is_hdr = WMT_COLORSPACE_IS_HDR(final_colorspace);
  auto hdr_metadata = display_setting_version_ > 0 ? &display_hdr_metadata_
                      : has_hdr_metadata_          ? &hdr_metadata_
                                                   : nullptr;
  if (unlikely(!pso_valid.test_and_set())) {
    frame_presented_.wait(frame_requested_);
    buildRenderPipelineState(final_colorspace == WMTColorSpaceHDR_PQ, is_hdr && hdr_metadata != nullptr, sample_count_ > 1, gamma_version_ != 0);
    layer_.setProps(layer_props_);
    layer_.setColorSpace(final_colorspace);
#ifdef DXMT_NATIVE
    invalidateDrawable();
#endif
  }

  DXMTPresentMetadata metadata;

  metadata.edr_scale = 1.0;
  metadata.max_content_luminance = 10000;
  metadata.max_display_luminance = display_edr_value_.maximum_potential_edr_color_component_value * 100;

  if (is_hdr) {
    metadata.edr_scale = display_edr_value_.maximum_edr_color_component_value /
                display_edr_value_.maximum_potential_edr_color_component_value;

    if (hdr_metadata) {
      metadata.max_content_luminance = std::max<uint32_t>(
          {(uint32_t)metadata.max_display_luminance, hdr_metadata->max_content_light_level,
           hdr_metadata->max_mastering_luminance, hdr_metadata->max_frame_average_light_level}
      );
    }
  }

  if (final_colorspace == WMTColorSpaceHDR_scRGB)
    metadata.edr_scale *= 0.8;

  return {metadata, ++frame_requested_, this};
}

WMT::Reference<WMT::MetalDrawable>
Presenter::encodeCommands(
    WMT::CommandBuffer cmdbuf, WMT::Texture backbuffer, DXMTPresentMetadata metadata,
    std::function<void(WMT::RenderCommandEncoder)> &&wait_fences,
    std::function<void(WMT::RenderCommandEncoder)> &&update_fences
) {
#ifdef DXMT_NATIVE
  auto drawable = prefetch_drawable_ ? acquireDrawable() : WMT::Reference<WMT::MetalDrawable>(layer_.nextDrawable());
#else
  WMT::Reference<WMT::MetalDrawable> drawable = layer_.nextDrawable();
#endif

  WMTRenderPassInfo info;
  WMT::InitializeRenderPassInfo(info);
  info.colors[0].load_action = WMTLoadActionDontCare;
  info.colors[0].store_action = WMTStoreActionStore;
  info.colors[0].texture = drawable.texture();
  auto encoder = cmdbuf.renderCommandEncoder(info);
  wait_fences(encoder);
  encoder.setFragmentTexture(backbuffer, 0);
  encoder.setFragmentTexture(gamma_lut_texture_, 1);

  double width = layer_props_.drawable_width;
  double height = layer_props_.drawable_height;

  encoder.setFragmentBytes(&metadata, sizeof(metadata), 0);
  if (backbuffer.width() == (uint64_t)width && backbuffer.height() == (uint64_t)height) {
    encoder.setRenderPipelineState(present_blit_);
  } else {
    encoder.setRenderPipelineState(present_scale_);
  }
  encoder.setViewport({0, 0, width, height, 0, 1});
  encoder.drawPrimitives(WMTPrimitiveTypeTriangle, 0, 3);
  update_fences(encoder);
  encoder.endEncoding();

  return drawable;
}

void
Presenter::buildRenderPipelineState(bool is_pq, bool with_hdr_metadata, bool is_ms, bool gamma_enable) {
  auto pool = WMT::MakeAutoreleasePool();

  auto library = lib_.getLibrary();

  uint32_t true_data = true, false_data = false;
  WMTFunctionConstant constants[6];
  constants[0].data.set(&true_data);
  constants[0].type = WMTDataTypeBool;
  constants[0].index = kPresentFCIndex_BackbufferSizeMatched;
  constants[1].data.set(is_pq ? &true_data : &false_data);
  constants[1].type = WMTDataTypeBool;
  constants[1].index = kPresentFCIndex_HDRPQ;
  constants[2].data.set(with_hdr_metadata ? &true_data : &false_data);
  constants[2].type = WMTDataTypeBool;
  constants[2].index = kPresentFCIndex_WithHDRMetadata;
  constants[3].data.set(Is_sRGBVariant(source_format_) ? &true_data : &false_data);
  constants[3].type = WMTDataTypeBool;
  constants[3].index = kPresentFCIndex_BackbufferIsSRGB;
  constants[4].data.set(is_ms ? &true_data : &false_data);
  constants[4].type = WMTDataTypeBool;
  constants[4].index = kPresentFCIndex_BackbufferIsMS;
  constants[5].data.set(gamma_enable ? &true_data : &false_data);
  constants[5].type  = WMTDataTypeBool;
  constants[5].index = kPresentFCIndex_GammaEnabled;

  WMT::Reference<WMT::Error> error;
  auto vs_present_quad = library.newFunction("vs_present_quad");
  auto fs_present_quad = library.newFunctionWithConstants("fs_present_quad", constants, std::size(constants), error);

  constants[0].data.set(&false_data);
  auto fs_present_quad_scaled =
      library.newFunctionWithConstants("fs_present_quad", constants, std::size(constants), error);
  {
    WMTRenderPipelineInfo present_pipeline;
    WMT::InitializeRenderPipelineInfo(present_pipeline);
    present_pipeline.colors[0].pixel_format = layer_props_.pixel_format;
    present_pipeline.vertex_function = vs_present_quad;
    present_pipeline.fragment_function = fs_present_quad;
    present_blit_ = device_.newRenderPipelineState(present_pipeline, error);
    present_pipeline.fragment_function = fs_present_quad_scaled;
    present_scale_ = device_.newRenderPipelineState(present_pipeline, error);
  }
}

} // namespace dxmt