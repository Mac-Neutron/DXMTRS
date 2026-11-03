#pragma once
#include "winemetal.h"

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

// MacNeutron (DXMTRS step 5a): the presenter is Rust (src/dxmt/src/presenter.rs).
// src/dxmt/src/presenter/abi.rs, generated from this block by tools/winemetal-layouts.sh --seam-abi, asserts
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

} // namespace dxmt
