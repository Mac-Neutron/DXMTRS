//! Dynamic buffer and dynamic texture implementations (`d3d11_texture_dynamic.cpp`).

use super::Common;

/// Dynamic resource wrapper (`MTLD3D11DynamicBuffer`, `MTLD3D11DynamicTexture`).
pub struct DynamicResource {
    pub common: Common,
}
