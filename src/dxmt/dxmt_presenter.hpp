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

// MacNeutron (DXMTRS step 5a): the presenter is Rust (src/dxmt/src/presenter.rs); the class below forwards to these
// functions. src/dxmt/src/presenter/abi.rs, generated from this block by tools/winemetal-layouts.sh --seam-abi, asserts
// each Rust export against its declaration on every target; a width written differently here fails that build.
extern "C" {
/* dxmt-abi begin */
struct DXMTPresentMetadata;
struct DXMTGammaRamp;

/* dxmt_presenter_encode calls each once, during the call, with ctx and the present pass's render encoder: wait before
 * the pass's commands, update after them */
struct dxmt_presenter_callbacks {
  void (*wait)(void *ctx, obj_handle_t encoder);
  void (*update)(void *ctx, obj_handle_t encoder);
  void *ctx;
};

/* device, layer and library are borrowed: they outlive the presenter */
void *dxmt_presenter_create(obj_handle_t device, obj_handle_t layer, obj_handle_t library, float scale_factor,
                            uint8_t sample_count);
void dxmt_presenter_destroy(void *p);
bool dxmt_presenter_change_layer_properties(void *p, enum WMTPixelFormat format, enum WMTColorSpace cs, double width,
                                            double height, uint8_t sample_count);
bool dxmt_presenter_change_layer_color_space(void *p, enum WMTColorSpace cs);
void dxmt_presenter_change_hdr_metadata(void *p, const struct WMTHDRMetadata *metadata);
/* ramp is the output's live gamma ramp (null for none), read through its atomics */
void dxmt_presenter_change_gamma_ramp(void *p, const struct DXMTGammaRamp *ramp);
/* returns the frame id; *out gets the frame's metadata */
uint64_t dxmt_presenter_synchronize(void *p, struct DXMTPresentMetadata *out);
/* PresentState's destructor: the frame is done */
void dxmt_presenter_frame_done(void *p, uint64_t frame_id);
/* returns the drawable, +1 */
obj_handle_t dxmt_presenter_encode(void *p, obj_handle_t cmdbuf, obj_handle_t backbuffer,
                                   const struct DXMTPresentMetadata *metadata,
                                   const struct dxmt_presenter_callbacks *callbacks);
/* dxmt-abi end */
}

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
        dxmt_presenter_frame_done(presenter->rust_, frame_id);
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
  void *rust_;
};
} // namespace dxmt