//! The gamma ramp an output holds and the presenter reads with no lock.
//!
//! `DXMTGammaRamp` (`dxmt_presenter.hpp`) as the Rust DXGI output stores it and the presenter
//! reads it on the Present thread (`dxmt_presenter.cpp`, `changeGammaRamp`).

use core::{
    mem::offset_of,
    sync::atomic::{AtomicU32, AtomicU64},
};

/// `DXMT_GAMMA_CP_COUNT`: the points of `GammaRamp`'s curves.
pub const GAMMA_CP_COUNT: usize = 1024;

/// `DXMTGammaRamp`: an output's gamma curves, which the presenter reads with no lock.
///
/// The curves are `f32`s by their bits. A writer (`SetGammaControl`, through `&self`) stores
/// every point Relaxed, then bumps `version` with Release; the presenter compares `version` with
/// the last it saw and copies the curves when it moved.
#[repr(C)]
pub struct GammaRamp {
    pub red: [AtomicU32; GAMMA_CP_COUNT],
    pub green: [AtomicU32; GAMMA_CP_COUNT],
    pub blue: [AtomicU32; GAMMA_CP_COUNT],
    pub version: AtomicU64,
}

const _: () = {
    assert!(size_of::<GammaRamp>() == 12296 && align_of::<GammaRamp>() == 8);
    assert!(offset_of!(GammaRamp, red) == 0);
    assert!(offset_of!(GammaRamp, green) == 4096);
    assert!(offset_of!(GammaRamp, blue) == 8192);
    assert!(offset_of!(GammaRamp, version) == 12288);
};

#[cfg(test)]
mod tests;
