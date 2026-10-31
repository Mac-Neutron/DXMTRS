//! DXMT's configuration as dxgi reads it, and the four `dxgi.*` options; stubs for now.

use std::sync::LazyLock;

/// The configuration: the user's file and `DXMT_CONFIG`, then the exe's built-in profile.
pub struct Config {}

/// The process's configuration, read at its first use.
///
/// A machine fact, latched once and immutable after: the first `EnumAdapters1` reads it
/// (`config.cpp`'s `Config::getInstance`), never under the loader lock.
pub static CONFIG: LazyLock<Config> = LazyLock::new(|| Config {});

/// The `dxgi.*` options, as `DxgiOptions` holds them.
pub struct Options {
    /// `dxgi.customVendorId`.
    pub vendor_id: Option<u16>,
    /// `dxgi.customDeviceId`.
    pub device_id: Option<u16>,
    /// `dxgi.customDeviceDesc`, its bytes as the file or the variable gave them.
    pub device_desc: Option<Vec<u8>>,
    /// `dxgi.forceSDR`.
    pub force_sdr: bool,
}

impl Options {
    /// The options `config` sets.
    #[must_use]
    pub const fn from_config(_config: &Config) -> Self {
        Self {
            vendor_id: None,
            device_id: None,
            device_desc: None,
            force_sdr: false,
        }
    }
}
