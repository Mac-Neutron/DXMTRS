//! Display modes: the list an output answers and the closest match to a mode.
//!
//! `dxgi_output.cpp`'s mode logic, pure: the output hands in the window system's modes and its
//! current one. A refresh rate with a zero denominator (a VM host's hook can answer one) is
//! skipped by the rate filter and sorts last, where the C++ divides by zero.

use crate::{
    LOG,
    abi::{
        DXGI_ENUM_MODES_INTERLACED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        DXGI_FORMAT_B8G8R8X8_UNORM, DXGI_FORMAT_B8G8R8X8_UNORM_SRGB, DXGI_FORMAT_R8G8B8A8_UNORM,
        DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM,
        DXGI_FORMAT_R10G10B10A2_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_UNKNOWN,
        DXGI_MODE_SCALING_UNSPECIFIED, DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE,
        DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST,
        ModeDesc1, Rational,
    },
    wsi::WsiMode,
};

/// The bits per pixel of a monitor showing `format`: `GetMonitorFormatBpp`.
///
/// An unknown format is logged and counts as 32.
#[must_use]
pub fn monitor_format_bpp(format: u32) -> u32 {
    match format {
        DXGI_FORMAT_R8G8B8A8_UNORM
        | DXGI_FORMAT_B8G8R8A8_UNORM
        | DXGI_FORMAT_B8G8R8X8_UNORM
        | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
        | DXGI_FORMAT_B8G8R8X8_UNORM_SRGB
        | DXGI_FORMAT_R10G10B10A2_UNORM
        | DXGI_FORMAT_R10G10B10_XR_BIAS_A2_UNORM => 32,
        DXGI_FORMAT_R16G16B16A16_FLOAT => 64,
        _ => {
            LOG.warn(&format!("GetMonitorFormatBpp: Unknown format: {format}"));
            32
        }
    }
}

/// Whether `format` is one a swap chain's back buffer can have.
///
/// The six formats DXMT's format table flags `MTL_DXGI_FORMAT_BACKBUFFER`, whatever the device.
#[must_use]
pub const fn is_back_buffer_format(format: u32) -> bool {
    matches!(
        format,
        DXGI_FORMAT_R16G16B16A16_FLOAT
            | DXGI_FORMAT_R10G10B10A2_UNORM
            | DXGI_FORMAT_R8G8B8A8_UNORM
            | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB
            | DXGI_FORMAT_B8G8R8A8_UNORM
            | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB
    )
}

/// The DXGI mode of a window-system mode: `ConvertDisplayMode`.
///
/// Its format is always `R8G8B8A8_UNORM_SRGB`, as the C++ has it.
#[must_use]
pub const fn from_wsi(mode: &WsiMode) -> ModeDesc1 {
    ModeDesc1 {
        width: mode.width,
        height: mode.height,
        refresh_rate: mode.refresh,
        format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        scanline_ordering: if mode.interlaced {
            DXGI_MODE_SCANLINE_ORDER_UPPER_FIELD_FIRST
        } else {
            DXGI_MODE_SCANLINE_ORDER_PROGRESSIVE
        },
        scaling: DXGI_MODE_SCALING_UNSPECIFIED,
        stereo: 0,
    }
}

/// The modes of `raw` an output lists for `format` and the `DXGI_ENUM_MODES` `flags`, sorted.
///
/// None for a format no back buffer has (`UNKNOWN` among them). An interlaced mode is listed only
/// with `DXGI_ENUM_MODES_INTERLACED`, and only modes at the format's bits per pixel, each with
/// `format`. The order is width, height, then whole refresh rate.
#[must_use]
pub fn mode_list(raw: &[WsiMode], format: u32, flags: u32) -> Vec<ModeDesc1> {
    if !is_back_buffer_format(format) {
        return Vec::new();
    }
    let mut list: Vec<ModeDesc1> = raw
        .iter()
        .filter(|mode| !mode.interlaced || flags & DXGI_ENUM_MODES_INTERLACED != 0)
        .filter(|mode| mode.bits_per_pixel == monitor_format_bpp(format))
        .map(|mode| ModeDesc1 {
            format,
            ..from_wsi(mode)
        })
        .collect();
    list.sort_by_key(sort_key);
    list
}

/// `GetDisplayModeList1`'s copy of `list` into a buffer of `room` modes: the count it answers.
///
/// `put(i, mode)` writes the first `room` modes. None is `DXGI_ERROR_MORE_DATA`, which leaves the
/// caller's count as it was; else the count becomes the list's length, and the buffer's tail is
/// left as it was.
#[must_use]
pub fn copy_out(
    list: Vec<ModeDesc1>,
    room: u32,
    mut put: impl FnMut(usize, ModeDesc1),
) -> Option<u32> {
    let count = u32::try_from(list.len()).unwrap_or(u32::MAX);
    let fits = usize::try_from(room).unwrap_or(usize::MAX);
    for (i, mode) in list.into_iter().take(fits).enumerate() {
        put(i, mode);
    }
    (count <= room).then_some(count)
}

/// The mode of `modes` closest to `target`; the output's `current` mode fills what it leaves open.
///
/// `FindClosestMatchingMode1`'s two filters: `target`'s, then a mode with `target`'s stereo flag
/// and, where `target` leaves them unspecified or zero, `current`'s scanline order, scaling,
/// format, size and refresh rate. None when no mode is left.
#[must_use]
pub fn find_closest(
    mut modes: Vec<ModeDesc1>,
    target: &ModeDesc1,
    current: &ModeDesc1,
) -> Option<ModeDesc1> {
    let no_size = target.width == 0;
    let no_rate = target.refresh_rate.numerator == 0 || target.refresh_rate.denominator == 0;
    let default = ModeDesc1 {
        width: if no_size { current.width } else { 0 },
        height: if no_size { current.height } else { 0 },
        refresh_rate: if no_rate {
            current.refresh_rate
        } else {
            Rational::default()
        },
        format: if target.format == DXGI_FORMAT_UNKNOWN {
            current.format
        } else {
            DXGI_FORMAT_UNKNOWN
        },
        scanline_ordering: if target.scanline_ordering == DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED {
            current.scanline_ordering
        } else {
            DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED
        },
        scaling: if target.scaling == DXGI_MODE_SCALING_UNSPECIFIED {
            current.scaling
        } else {
            DXGI_MODE_SCALING_UNSPECIFIED
        },
        stereo: target.stereo,
    };
    filter(&mut modes, target);
    filter(&mut modes, &default);
    modes.into_iter().next()
}

/// The refresh rate of `hz` hertz as Win32's modes give it: `{hz * 1000, 1000}`, wrapping as C.
#[must_use]
pub const fn refresh_from_hz(hz: u32) -> Rational {
    Rational {
        numerator: hz.wrapping_mul(1000),
        denominator: 1000,
    }
}

/// `FilterModesByDesc`: keeps the modes of `modes` that match `target`.
///
/// A mode of another stereo flag goes, and so does one of another scanline order, scaling or
/// format when `target` names one that some mode has. Then, if `target` has a width, the modes
/// nearest it in size stay, and if it has a refresh rate, the modes nearest it in rate.
fn filter(modes: &mut Vec<ModeDesc1>, target: &ModeDesc1) {
    let scanline = target.scanline_ordering != DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED
        && modes
            .iter()
            .any(|mode| mode.scanline_ordering == target.scanline_ordering);
    let scaling = target.scaling != DXGI_MODE_SCALING_UNSPECIFIED
        && modes.iter().any(|mode| mode.scaling == target.scaling);
    let format = target.format != DXGI_FORMAT_UNKNOWN
        && modes.iter().any(|mode| mode.format == target.format);
    modes.retain(|mode| {
        mode.stereo == target.stereo
            && (!scanline || mode.scanline_ordering == target.scanline_ordering)
            && (!scaling || mode.scaling == target.scaling)
            && (!format || mode.format == target.format)
    });
    if target.width != 0 {
        let nearest = modes
            .iter()
            .map(|mode| size_diff(target, mode))
            .fold(u32::MAX, u32::min);
        modes.retain(|mode| size_diff(target, mode) == nearest);
    }
    if target.refresh_rate.numerator != 0 && target.refresh_rate.denominator != 0 {
        let nearest = u64::from(min_rate_diff(modes, target));
        modes.retain(|mode| rate_diff(target, mode) == Some(nearest));
    }
}

/// How far `mode`'s size is from `target`'s: the C++'s `abs(int32_t(a - b))` per side, summed.
const fn size_diff(target: &ModeDesc1, mode: &ModeDesc1) -> u32 {
    let wide = target
        .width
        .wrapping_sub(mode.width)
        .cast_signed()
        .unsigned_abs();
    let high = target
        .height
        .wrapping_sub(mode.height)
        .cast_signed()
        .unsigned_abs();
    wide.wrapping_add(high)
}

/// How far `mode`'s refresh rate is from `target`'s, in `target`'s units.
///
/// None for a mode whose denominator is zero.
fn rate_diff(target: &ModeDesc1, mode: &ModeDesc1) -> Option<u64> {
    // Two `u32`s: the product fits.
    let scaled =
        u64::from(mode.refresh_rate.numerator) * u64::from(target.refresh_rate.denominator);
    let rate = scaled.checked_div(u64::from(mode.refresh_rate.denominator))?;
    let apart = rate.wrapping_sub(u64::from(target.refresh_rate.numerator));
    Some(apart.cast_signed().unsigned_abs())
}

/// The smallest `rate_diff` of `modes`, as the C++ keeps it.
///
/// A `uint32_t` assigned from the `uint64_t` minimum: its low 32 bits, `0xFFFF_FFFF` when no mode
/// has a rate.
fn min_rate_diff(modes: &[ModeDesc1], target: &ModeDesc1) -> u32 {
    let nearest = modes
        .iter()
        .filter_map(|mode| rate_diff(target, mode))
        .fold(u64::MAX, u64::min);
    u32::try_from(nearest & u64::from(u32::MAX)).unwrap_or(u32::MAX)
}

/// Where a mode sorts: by width, height, then whole refresh rate; a zero denominator last.
const fn sort_key(mode: &ModeDesc1) -> (bool, u32, u32, u32) {
    match mode
        .refresh_rate
        .numerator
        .checked_div(mode.refresh_rate.denominator)
    {
        Some(rate) => (false, mode.width, mode.height, rate),
        None => (true, mode.width, mode.height, 0),
    }
}

#[cfg(test)]
mod tests;
