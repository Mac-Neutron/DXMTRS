#pragma once

#include "Metal.hpp"
#include <atomic>

namespace dxmt {

// MacNeutron (DXMTRS step 5a): the scalers are Rust (src/dxmt/src/scaler.rs); the classes below keep their reference
// counts (and the temporal scaler its description) and forward the rest to these functions.
// src/dxmt/src/scaler/abi.rs, generated from this block by tools/winemetal-layouts.sh --seam-abi, asserts each Rust
// export against its declaration on every target.
extern "C" {
/* dxmt-abi begin */
/* a scaler, then its fence (each +1, held by the returned object; either null when Metal made none); device is
 * borrowed */
void *dxmt_spatial_scaler_create(obj_handle_t device, const struct WMTFXSpatialScalerInfo *info);
void *dxmt_temporal_scaler_create(obj_handle_t device, const struct WMTFXTemporalScalerInfo *info);
void dxmt_scaler_destroy(void *s);
/* +0, null when Metal made none */
obj_handle_t dxmt_scaler_object(void *s);
obj_handle_t dxmt_scaler_fence(void *s);
/* dxmt-abi end */
}

class SpatialScaler {
public:
  void incRef();
  void decRef();

  SpatialScaler(WMT::Device device, const WMTFXSpatialScalerInfo &info);
  ~SpatialScaler();

  WMT::FXSpatialScaler scaler() {
    return {dxmt_scaler_object(rust_)};
  }

  WMT::Fence fence() {
    return {dxmt_scaler_fence(rust_)};
  }

private:
  void *rust_;
  std::atomic<uint32_t> refcount_;
};

class TemporalScaler {
public:
  void incRef();
  void decRef();

  TemporalScaler(WMT::Device device, const WMTFXTemporalScalerInfo &info);
  ~TemporalScaler();

  // MacNeutron: 1 when only its last holder has it (D3D12's pool of released upscalers)
  uint32_t refCount() const {
    return refcount_.load(std::memory_order_acquire);
  }

  WMT::FXTemporalScaler scaler() {
    return {dxmt_scaler_object(rust_)};
  }

  WMT::Fence fence() {
    return {dxmt_scaler_fence(rust_)};
  }

  // MacNeutron: what it was made for (frame generation pairs an interpolator with it).
  const WMTFXTemporalScalerInfo &info() const {
    return info_;
  }

private:
  WMTFXTemporalScalerInfo info_;
  void *rust_;
  std::atomic<uint32_t> refcount_;
};

} // namespace dxmt
