#pragma once

#include "winemetal.h"

namespace dxmt {

// MacNeutron (DXMTRS step 5a): the scalers are Rust (src/dxmt/src/scaler.rs).
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

} // namespace dxmt
