#pragma once
#include "Metal.hpp"
#include "dxmt_command.hpp"
#include "rc/util_rc.hpp"
#include "thread.hpp"
#include "util_cpu_fence.hpp"
#include "winemetal.h"
#include <atomic>

namespace dxmt {

struct DXMTPresentMetadata {
  float edr_scale;
  float max_content_luminance;
  float max_display_luminance;
};

constexpr uint32_t DXMT_GAMMA_CP_COUNT = 1024;

struct DXMTGammaRamp {
  float red[DXMT_GAMMA_CP_COUNT];
  float green[DXMT_GAMMA_CP_COUNT];
  float blue[DXMT_GAMMA_CP_COUNT];
  uint64_t version;
};

class Presenter : public RcObject {
public:
  Presenter(
      WMT::Device device, WMT::MetalLayer layer, InternalCommandLibrary &lib, float scale_factor, uint8_t sample_count
  );
  ~Presenter();

  bool changeLayerProperties(
      WMTPixelFormat format, WMTColorSpace colorspace, double width, double height, uint8_t sample_count
  );

  bool changeLayerColorSpace(WMTColorSpace colorspace);

  void changeHDRMetadata(const WMTHDRMetadata *metadata);

  void changeGammaRamp(const DXMTGammaRamp *gamma_ramp);

  class PresentState {
  public:
    DXMTPresentMetadata metadata;
    uint64_t frame_id;
    Presenter *presenter;
    PresentState(DXMTPresentMetadata metadata, uint64_t frame_id, Presenter *presenter) :
        metadata(metadata),
        frame_id(frame_id),
        presenter(presenter) {}
    PresentState(const PresentState &copy) = delete;
    PresentState(PresentState &&move)  {
      metadata = move.metadata;
      frame_id = move.frame_id;
      presenter = move.presenter;
      move.presenter = nullptr;
    };
    ~PresentState() {
      if (presenter) {
        presenter->frame_presented_.signal(frame_id);
        presenter = nullptr;
      }
    };
  };

  PresentState synchronizeLayerProperties();

  WMT::Reference<WMT::MetalDrawable> encodeCommands(
      WMT::CommandBuffer cmdbuf, WMT::Texture backbuffer, DXMTPresentMetadata metadata,
      std::function<void(WMT::RenderCommandEncoder)> &&wait_fences,
      std::function<void(WMT::RenderCommandEncoder)> &&update_fences
  );

private:
  void buildRenderPipelineState(bool is_pq, bool with_hdr_metadata, bool is_ms, bool gamma_enable);
#ifdef DXMT_NATIVE
  // MacNeutron (the VM host): DXMT's presenter fetches its next drawable on a thread of its own, as the display recycles
  // drawables (Metal only hands one back at a refresh). The frame's present then finds one ready instead of waiting up
  // to a display frame on the game's vCPU (docs/research/2026-10-15-vm-route/README.md, M3's "upgrade"). Native DXMT on
  // macOS is not the VM host (it has no WSI hooks: winemac's window system), so this is off there.
  void drawableWorker();
  void requestDrawableLocked();
  void invalidateDrawable();
  WMT::Reference<WMT::MetalDrawable> acquireDrawable();
  bool prefetch_drawable_ = false;
  dxmt::thread drawable_thread_;
  dxmt::mutex drawable_mutex_;
  dxmt::condition_variable drawable_cv_;
  bool drawable_stop_ = false, drawable_wanted_ = false, drawable_busy_ = false;
  uint64_t drawable_generation_ = 0, prefetched_generation_ = 0;
  WMT::Reference<WMT::MetalDrawable> prefetched_drawable_;
#endif

  WMT::Device device_;
  WMT::MetalLayer layer_;
  InternalCommandLibrary &lib_;
  WMTLayerProps layer_props_;
  uint32_t sample_count_;
  WMTColorSpace colorspace_ = WMTColorSpaceSRGB;
  WMTHDRMetadata hdr_metadata_;
  bool has_hdr_metadata_ = false;
  WMTPixelFormat source_format_ = WMTPixelFormatInvalid;
  uint64_t display_setting_version_ = 0;
  WMTColorSpace display_colorspace_ = WMTColorSpaceSRGB;
  WMTHDRMetadata display_hdr_metadata_;
  WMTEDRValue display_edr_value_{0.0, 1.0};
  uint64_t gamma_version_ = 0;
  std::array<float, DXMT_GAMMA_CP_COUNT * 4> gamma_lut_rgba_;
  WMT::Reference<WMT::Texture> gamma_lut_texture_;
  WMT::Reference<WMT::RenderPipelineState> present_blit_;
  WMT::Reference<WMT::RenderPipelineState> present_scale_;
  std::atomic_flag pso_valid = 0;
  uint64_t frame_requested_ = 0;
  CpuFence frame_presented_ = 0;
};
} // namespace dxmt