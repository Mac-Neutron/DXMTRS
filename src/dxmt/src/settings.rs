//! The `DXMT_*` environment flags more than one DLL reads, parsed once.
//!
//! A flag is a fact about the machine and the launch, not about a device: the environment is read
//! the first time [`settings`] is called and the answer stays for the life of the DLL copy that
//! linked this core (`d3d11.dll` and `d3d12.dll` each link their own). No logger state lives here.

use std::sync::LazyLock;

/// How `DXMT_D3D11_COMPRESSION` and `DXMT_D3D12_COMPRESSION` ask for lossless texture compression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    /// `0`: no texture is compressed.
    Off,
    /// `rt`: textures are compressed, unordered access views excepted.
    RenderTargets,
    /// Unset or anything else: compressed, unordered access views of 32-bit formats excepted.
    Default,
}

impl Compression {
    /// The mode `value` names; an unset variable is `Default`.
    #[must_use]
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("0") => Self::Off,
            Some("rt") => Self::RenderTargets,
            _ => Self::Default,
        }
    }
}

bitflags::bitflags! {
    /// The on/off flags, each set as the unset variable's default says.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Flags: u16 {
        /// `DXMT_D3D12_PRIVATE`: on unless `0`. Heaps and committed resources in private storage.
        const PRIVATE_HEAP = 1;
        /// `DXMT_D3D12_MERGE`: on unless `0`. Adjacent command lists merge into one pass.
        const MERGE = 1 << 1;
        /// `DXMT_D3D12_DEFER_RESOLVES`: on unless `0`.
        const DEFER_RESOLVES = 1 << 2;
        /// `DXMT_D3D12_SM6`: on unless `0`. Shader model 6 is advertised.
        const SM6 = 1 << 3;
        /// `DXMT_D3D12_INDIRECT`: on unless `icb`. Indirect draws without a command buffer.
        const INDIRECT_WITHOUT_ICB = 1 << 4;
        /// `DXMT_D3D12_MEMORYLESS`: off unless `1`.
        const MEMORYLESS = 1 << 5;
        /// `DXMT_D3D12_TIMESTAMP_BLITS`: off unless `1`.
        const TIMESTAMP_BLITS = 1 << 6;
        /// `DXMT_D3D12_ADVERTISE_NUMA`: off unless `1`.
        const ADVERTISE_NUMA = 1 << 7;
        /// `DXMT_METALFX_SPATIAL_SWAPCHAIN`: off unless `1`.
        const METALFX_SPATIAL_SWAPCHAIN = 1 << 8;
        /// `DXMT_STATS`: off unless `1`.
        const STATS = 1 << 9;
        /// `DXMT_CAPTURE_FRAME` is set, whatever it says.
        const CAPTURE_FRAME_SET = 1 << 10;
    }
}

/// The settings: the flags and the values.
#[derive(Debug)]
pub struct Settings {
    /// The on/off flags.
    pub flags: Flags,
    /// `DXMT_D3D11_COMPRESSION`.
    pub d3d11_compression: Compression,
    /// `DXMT_D3D12_COMPRESSION`.
    pub d3d12_compression: Compression,
    /// `DXMT_CAPTURE_FRAME` as a frame number: strict, the value trimmed, and above 0.
    pub capture_frame: Option<u64>,
    /// `MTL_CAPTURE_ENABLED`, when set.
    pub mtl_capture_enabled: Option<String>,
    /// `DXMT_CAPTURE_EXECUTABLE`, when set.
    pub capture_executable: Option<String>,
}

impl Settings {
    /// The settings `var` answers, a lookup of the environment's variables.
    #[must_use]
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Self {
        let is = |name: &str, value: &str| var(name).as_deref() == Some(value);
        let capture_frame = var("DXMT_CAPTURE_FRAME");
        let table = [
            (Flags::PRIVATE_HEAP, !is("DXMT_D3D12_PRIVATE", "0")),
            (Flags::MERGE, !is("DXMT_D3D12_MERGE", "0")),
            (Flags::DEFER_RESOLVES, !is("DXMT_D3D12_DEFER_RESOLVES", "0")),
            (Flags::SM6, !is("DXMT_D3D12_SM6", "0")),
            (
                Flags::INDIRECT_WITHOUT_ICB,
                !is("DXMT_D3D12_INDIRECT", "icb"),
            ),
            (Flags::MEMORYLESS, is("DXMT_D3D12_MEMORYLESS", "1")),
            (
                Flags::TIMESTAMP_BLITS,
                is("DXMT_D3D12_TIMESTAMP_BLITS", "1"),
            ),
            (Flags::ADVERTISE_NUMA, is("DXMT_D3D12_ADVERTISE_NUMA", "1")),
            (
                Flags::METALFX_SPATIAL_SWAPCHAIN,
                is("DXMT_METALFX_SPATIAL_SWAPCHAIN", "1"),
            ),
            (Flags::STATS, is("DXMT_STATS", "1")),
            (Flags::CAPTURE_FRAME_SET, capture_frame.is_some()),
        ];
        let flags = table
            .into_iter()
            .filter(|(_, on)| *on)
            .fold(Flags::empty(), |all, (flag, _)| all | flag);
        Self {
            flags,
            d3d11_compression: Compression::parse(var("DXMT_D3D11_COMPRESSION").as_deref()),
            d3d12_compression: Compression::parse(var("DXMT_D3D12_COMPRESSION").as_deref()),
            capture_frame: crate::capture::capture_frame(capture_frame.as_deref()),
            mtl_capture_enabled: var("MTL_CAPTURE_ENABLED"),
            capture_executable: var("DXMT_CAPTURE_EXECUTABLE"),
        }
    }

    /// Whether `flag` is set.
    #[must_use]
    pub const fn enabled(&self, flag: Flags) -> bool {
        self.flags.contains(flag)
    }
}

/// The process's flags, read from the environment on first use.
#[must_use]
pub fn settings() -> &'static Settings {
    static SETTINGS: LazyLock<Settings> =
        LazyLock::new(|| Settings::from_vars(|name| std::env::var(name).ok()));
    &SETTINGS
}

#[cfg(test)]
mod tests;
