//! Mapping from a D3D9 `D3DPRESENT_INTERVAL_*` to the pacing a device asks of its layer.
//!
//! The layer paces presents by a minimum duration alone. An interval
//! contributes the vsync request, and a divided interval a frame-rate ceiling
//! on top of it; the user's `present.maxFps` rides the same ceiling.

use mtld3d_types::{
    D3DMULTISAMPLE_NONE, D3DPRESENT_BACK_BUFFERS_MAX, D3DPRESENT_BACK_BUFFERS_MAX_EX,
    D3DPRESENT_INTERVAL_DEFAULT, D3DPRESENT_INTERVAL_FOUR, D3DPRESENT_INTERVAL_IMMEDIATE,
    D3DPRESENT_INTERVAL_ONE, D3DPRESENT_INTERVAL_THREE, D3DPRESENT_INTERVAL_TWO,
    D3DPRESENT_PARAMETERS, D3DPRESENTFLAG_LOCKABLE_BACKBUFFER, D3DSWAPEFFECT_COPY,
    D3DSWAPEFFECT_DISCARD, D3DSWAPEFFECT_FLIPEX,
};

/// The side of an additional swap chain whose request and window both leave it zero.
pub const EMPTY_CLIENT_BACK_BUFFER_EXTENT: u32 = 8;

/// Result of mapping a `D3DPRESENT_INTERVAL_*` to the vsync request.
///
/// `Fallthrough` carries the same boolean as a supported choice but
/// signals the caller to fire a `log_once_warn_by!` keyed on the raw
/// input: a bit pattern that names no interval takes this path and runs at
/// display rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplaySync {
    On,
    Off,
    Fallthrough,
}

impl DisplaySync {
    #[must_use]
    pub const fn enabled(self) -> bool {
        matches!(self, Self::On | Self::Fallthrough)
    }
}

/// The pacing a device hands its layer: the vsync request and the frame-rate ceiling.
///
/// `max_fps` is in Hz and `0` means no ceiling. It is the effective ceiling
/// ([`effective_max_fps`]), so a divided interval and `present.maxFps` are
/// already folded into it and the unix side sees one number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerPacing {
    pub display_sync: bool,
    pub max_fps: u32,
}

/// Validate the swap-effect, back-buffer-count and presentation-interval fields.
///
/// The `CreateDevice` and `Reset` contract; `false` is the call's
/// `D3DERR_INVALIDCALL`. A plain device takes the swap effects DISCARD, FLIP
/// and COPY and up to [`D3DPRESENT_BACK_BUFFERS_MAX`] back buffers; an
/// extended device also takes OVERLAY and FLIPEX and up to
/// [`D3DPRESENT_BACK_BUFFERS_MAX_EX`]. On both, swap effect 0 is refused,
/// COPY allows one back buffer (a requested 0 resolves to 1), and the
/// interval is one of DEFAULT, ONE, TWO, THREE, FOUR and IMMEDIATE.
#[must_use]
pub const fn present_params_are_valid(pp: &D3DPRESENT_PARAMETERS, extended: bool) -> bool {
    let (highest_swap_effect, max_back_buffers) = if extended {
        (D3DSWAPEFFECT_FLIPEX, D3DPRESENT_BACK_BUFFERS_MAX_EX)
    } else {
        (D3DSWAPEFFECT_COPY, D3DPRESENT_BACK_BUFFERS_MAX)
    };
    if pp.swap_effect < D3DSWAPEFFECT_DISCARD || pp.swap_effect > highest_swap_effect {
        return false;
    }
    if pp.swap_effect == D3DSWAPEFFECT_COPY && pp.back_buffer_count > 1 {
        return false;
    }
    if pp.back_buffer_count > max_back_buffers {
        return false;
    }
    matches!(
        pp.presentation_interval,
        D3DPRESENT_INTERVAL_DEFAULT
            | D3DPRESENT_INTERVAL_ONE
            | D3DPRESENT_INTERVAL_TWO
            | D3DPRESENT_INTERVAL_THREE
            | D3DPRESENT_INTERVAL_FOUR
            | D3DPRESENT_INTERVAL_IMMEDIATE
    )
}

/// Validate the presentation flags against the back buffer's multisample type.
///
/// The `CreateDevice`, `CreateAdditionalSwapChain` and `Reset` contract on
/// both kinds of device; `false` is the call's `D3DERR_INVALIDCALL`. A
/// multisampled surface cannot be locked, so `D3DPRESENTFLAG_LOCKABLE_BACKBUFFER`
/// together with any `MultiSampleType` but `D3DMULTISAMPLE_NONE`
/// (`D3DMULTISAMPLE_NONMASKABLE` included) asks for a back buffer that cannot
/// exist. A `Reset` checks this apart from [`present_params_are_valid`]: it is
/// refused where the new swap chain is built, after a plain device's state
/// returned to its defaults, so the device then needs another `Reset`.
#[must_use]
pub const fn present_flags_are_valid(pp: &D3DPRESENT_PARAMETERS) -> bool {
    pp.flags & D3DPRESENTFLAG_LOCKABLE_BACKBUFFER == 0
        || pp.multi_sample_type == D3DMULTISAMPLE_NONE
}

#[must_use]
pub const fn display_sync_for(interval: u32) -> DisplaySync {
    match interval {
        D3DPRESENT_INTERVAL_DEFAULT
        | D3DPRESENT_INTERVAL_ONE
        | D3DPRESENT_INTERVAL_TWO
        | D3DPRESENT_INTERVAL_THREE
        | D3DPRESENT_INTERVAL_FOUR => DisplaySync::On,
        D3DPRESENT_INTERVAL_IMMEDIATE => DisplaySync::Off,
        _ => DisplaySync::Fallthrough,
    }
}

/// How many refresh periods one present of `interval` spans.
///
/// `TWO`, `THREE` and `FOUR` answer 2, 3 and 4. Every other value answers 1:
/// display rate for `DEFAULT` and `ONE`, and nothing to divide for
/// `IMMEDIATE` or a pattern that names no interval.
#[must_use]
pub const fn interval_divisor(interval: u32) -> u32 {
    match interval {
        D3DPRESENT_INTERVAL_TWO => 2,
        D3DPRESENT_INTERVAL_THREE => 3,
        D3DPRESENT_INTERVAL_FOUR => 4,
        _ => 1,
    }
}

/// Whether two intervals ask the layer for the same pacing.
///
/// `DEFAULT` and `ONE` do, as does a pattern that names no interval, which runs
/// at display rate.
#[must_use]
pub const fn intervals_pace_alike(a: u32, b: u32) -> bool {
    display_sync_for(a).enabled() == display_sync_for(b).enabled()
        && interval_divisor(a) == interval_divisor(b)
}

/// The frame-rate ceiling in Hz for `interval` on a `refresh_hz` mode, `0` for none.
///
/// A divided interval is a ceiling of `refresh_hz / N`, and the result is the
/// lower of that and `configured` (`present.maxFps`), where `0` on either
/// side means that side sets no ceiling. The quotient rounds up: the ceiling
/// becomes a minimum present duration, and one a little shorter than `N`
/// refresh periods still lands on the `N`th, while one a little longer would
/// slip to the period after it. An undivided interval, or a mode whose rate
/// is unknown (`refresh_hz == 0`), leaves `configured` as it is.
#[must_use]
pub const fn effective_max_fps(interval: u32, refresh_hz: u32, configured: u32) -> u32 {
    let divisor = interval_divisor(interval);
    if divisor == 1 || refresh_hz == 0 {
        return configured;
    }
    let divided = refresh_hz.div_ceil(divisor);
    if configured == 0 || divided < configured {
        divided
    } else {
        configured
    }
}

/// The pacing `interval` asks for on a `refresh_hz` mode under a `configured` ceiling.
#[must_use]
pub const fn layer_pacing_for(interval: u32, refresh_hz: u32, configured: u32) -> LayerPacing {
    LayerPacing {
        display_sync: display_sync_for(interval).enabled(),
        max_fps: effective_max_fps(interval, refresh_hz, configured),
    }
}

/// The pacing change a `Reset` leaves queued, given the pacing the layer holds.
///
/// `held` is the value the layer was last handed and `want` the value the
/// `Reset` resolved to. The answer is the whole queue rather than a delta:
/// `None` when the two agree, because the layer already paces the way the
/// guest asked for, so nothing has to go out and anything still queued is
/// dropped rather than written back as the value the layer never left.
#[must_use]
pub const fn queued_pacing(held: LayerPacing, want: LayerPacing) -> Option<LayerPacing> {
    if want.display_sync == held.display_sync && want.max_fps == held.max_fps {
        None
    } else {
        Some(want)
    }
}

/// Render and drawable extents paired for present routing and settle tracking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PresentGeometry {
    pub src: (usize, usize),
    pub dst: (usize, usize),
}

impl PresentGeometry {
    /// Whether presenting `src` into `dst` enlarges without shrinking either axis.
    ///
    /// Returns `true` when neither axis shrinks and at least one is strictly larger.
    #[must_use]
    pub const fn enlarges(self) -> bool {
        self.src.0 <= self.dst.0
            && self.src.1 <= self.dst.1
            && (self.src.0 < self.dst.0 || self.src.1 < self.dst.1)
    }
}

/// Consecutive presents at one geometry before `MetalFX` is worth building.
///
/// The drawable follows the layer every present while the back buffer follows guest `WM_SIZE`, so
/// a live window drag produces a different enlargement on nearly every frame. Waiting for the
/// geometry to hold still avoids building and leaking transient spatial scalers.
pub const SETTLED_PRESENTS: u32 = 30;

/// Advance the settle counter for `geometry`, and say whether it has settled.
///
/// A change of geometry restarts the count, so settled means [`SETTLED_PRESENTS`] presents in a
/// row at the same pair rather than that many presents in total.
pub const fn geometry_settled(
    seen: &mut Option<(PresentGeometry, u32)>,
    geometry: PresentGeometry,
) -> bool {
    match seen {
        Some((last, streak))
            if last.src.0 == geometry.src.0
                && last.src.1 == geometry.src.1
                && last.dst.0 == geometry.dst.0
                && last.dst.1 == geometry.dst.1 =>
        {
            *streak = streak.saturating_add(1);
            *streak >= SETTLED_PRESENTS
        }
        _ => {
            *seen = Some((geometry, 1));
            SETTLED_PRESENTS <= 1
        }
    }
}

#[cfg(test)]
mod tests;
