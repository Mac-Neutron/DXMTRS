//! The four `dxgi.*` options, over DXMT's configuration.
//!
//! The reader is `dxmt_util::config`'s; this module holds the process's copy of what it read and
//! the options dxgi takes from it.

use std::sync::LazyLock;

use dxmt_util::config::Config;

use crate::LOG;

/// The process's configuration, read at its first use.
///
/// A machine fact, latched once and immutable after: the first `EnumAdapters1` reads it
/// (`config.cpp`'s `Config::getInstance`), never under the loader lock.
pub static CONFIG: LazyLock<Config> = LazyLock::new(|| Config::load(&LOG));

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
    ///
    /// An ID is exactly four hex digits; an empty description is none; `forceSDR` is `true` or
    /// `false` in any case, else off (`dxgi_options.cpp`).
    #[must_use]
    pub fn from_config(config: &Config) -> Self {
        let desc = config.get("dxgi.customDeviceDesc");
        Self {
            vendor_id: hex4(config.get("dxgi.customVendorId")),
            device_id: hex4(config.get("dxgi.customDeviceId")),
            device_desc: (!desc.is_empty()).then(|| desc.to_vec()),
            force_sdr: config.get_bool("dxgi.forceSDR", false),
        }
    }
}

/// `parsePciId`: exactly four hex digits, in either case.
fn hex4(value: &[u8]) -> Option<u16> {
    if value.len() != 4 {
        return None;
    }
    value.iter().try_fold(0u16, |id, &b| {
        let digit = char::from(b).to_digit(16)?;
        Some((id << 4) | u16::try_from(digit).ok()?)
    })
}

#[cfg(test)]
mod tests;
