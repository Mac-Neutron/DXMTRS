//! Display modes: the list an output answers and the closest match to a mode; stubs for now.

use crate::{
    abi::{ModeDesc1, Rational},
    wsi::WsiMode,
};

/// The bits per pixel of a monitor showing `format`.
#[must_use]
pub const fn monitor_format_bpp(_format: u32) -> u32 {
    0
}

/// Whether `format` is one a swap chain's back buffer can have.
#[must_use]
pub const fn is_back_buffer_format(_format: u32) -> bool {
    false
}

/// The modes of `raw` an output lists for `format` and the `DXGI_ENUM_MODES` `flags`, sorted.
#[must_use]
pub const fn mode_list(_raw: &[WsiMode], _format: u32, _flags: u32) -> Vec<ModeDesc1> {
    Vec::new()
}

/// The mode of `modes` closest to `target`; the output's `current` mode fills what it leaves open.
#[must_use]
pub fn find_closest(
    modes: Vec<ModeDesc1>,
    _target: &ModeDesc1,
    _current: &ModeDesc1,
) -> Option<ModeDesc1> {
    drop(modes);
    None
}

/// The refresh rate of `hz` hertz as Win32's modes give it: `{hz * 1000, 1000}`.
#[must_use]
pub const fn refresh_from_hz(_hz: u32) -> Rational {
    Rational {
        numerator: 0,
        denominator: 0,
    }
}
