//! Device-resident 1D, 2D and 3D texture implementations (`d3d11_texture.cpp`).

use super::Common;

/// Device texture (`MTLD3D11DeviceTexture`).
pub struct DeviceTexture {
    pub common: Common,
}
