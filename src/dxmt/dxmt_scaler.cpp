#include "dxmt_scaler.hpp"

namespace dxmt {

void
SpatialScaler::incRef() {
  refcount_.fetch_add(1u, std::memory_order_acquire);
}

void
SpatialScaler::decRef() {
  if (refcount_.fetch_sub(1u, std::memory_order_release) == 1u)
    delete this;
}

SpatialScaler::SpatialScaler(WMT::Device device, const WMTFXSpatialScalerInfo &info) :
    rust_(dxmt_spatial_scaler_create(device, &info)) {}

SpatialScaler::~SpatialScaler() {
  dxmt_scaler_destroy(rust_);
}

void
TemporalScaler::incRef() {
  refcount_.fetch_add(1u, std::memory_order_acquire);
}

void
TemporalScaler::decRef() {
  if (refcount_.fetch_sub(1u, std::memory_order_release) == 1u)
    delete this;
}

TemporalScaler::TemporalScaler(WMT::Device device, const WMTFXTemporalScalerInfo &info) :
    info_(info),
    rust_(dxmt_temporal_scaler_create(device, &info)) {}

TemporalScaler::~TemporalScaler() {
  dxmt_scaler_destroy(rust_);
}

} // namespace dxmt
