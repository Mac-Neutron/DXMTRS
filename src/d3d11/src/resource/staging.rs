//! Staging buffer and staging texture implementations (`d3d11_resource_staging.cpp`).

use super::Common;

/// Staging resource (`MTLD3D11StagingBuffer`, `MTLD3D11StagingTexture`).
pub struct StagingResource {
    pub common: Common,
}
