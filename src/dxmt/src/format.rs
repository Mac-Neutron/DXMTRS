//! Formats: the DXGI table, the pixel-format helpers and each device's format capabilities.

/// The linear `WMTPixelFormat` of an sRGB one; any other format comes back as given.
///
/// The signature the presenter codes against; the format table's port (step 5a, S3) gives it
/// its body, and until then every format comes back as given.
#[must_use]
pub const fn forget_srgb(format: u32) -> u32 {
    format
}
