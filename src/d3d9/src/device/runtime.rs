//! Which runtime records a device's frames (DXMTRS C4): mtld3d's, or DXMT's core.
//!
//! A device of mtld3d's runtime holds its encoder thread and its frame; a device of the core
//! runtime holds a `d3d9_rt::CoreDevice` and neither of those ([`Mtld3dOnly::absent`]). The
//! entry points the core runtime does not implement yet answer `D3DERR_NOTAVAILABLE` (or their
//! return type's nothing) with one warning (`refuse_under_core`), before they reach mtld3d's
//! recorder; [`Mtld3dOnly`] is the backstop behind them. In a build without `core-runtime` every
//! device is mtld3d's and [`Mtld3dOnly`] is its value.

use core::ops::{Deref, DerefMut};

/// A part of mtld3d's runtime a device holds: absent from a device of the core runtime.
///
/// Reaching it on such a device means an entry point missed its guard, which ends the process.
pub struct Mtld3dOnly<T>(
    #[cfg(not(feature = "core-runtime"))] T,
    #[cfg(feature = "core-runtime")] Option<T>,
);

impl<T> Mtld3dOnly<T> {
    /// mtld3d's `value`.
    pub const fn new(value: T) -> Self {
        #[cfg(not(feature = "core-runtime"))]
        return Self(value);
        #[cfg(feature = "core-runtime")]
        Self(Some(value))
    }

    /// The part, `None` on a device of the core runtime.
    #[cfg(feature = "core-runtime")]
    pub const fn get(&self) -> Option<&T> {
        self.0.as_ref()
    }

    /// The part a device of the core runtime does not have.
    #[cfg(feature = "core-runtime")]
    pub const fn absent() -> Self {
        Self(None)
    }
}

impl<T> Deref for Mtld3dOnly<T> {
    type Target = T;

    // The panic names the caller's line: the site that lacks its guard.
    #[track_caller]
    fn deref(&self) -> &T {
        #[cfg(not(feature = "core-runtime"))]
        return &self.0;
        #[cfg(feature = "core-runtime")]
        match self.0.as_ref() {
            Some(value) => value,
            None => missed_guard(),
        }
    }
}

impl<T> DerefMut for Mtld3dOnly<T> {
    #[track_caller]
    fn deref_mut(&mut self) -> &mut T {
        #[cfg(not(feature = "core-runtime"))]
        return &mut self.0;
        #[cfg(feature = "core-runtime")]
        match self.0.as_mut() {
            Some(value) => value,
            None => missed_guard(),
        }
    }
}

/// Logs, once per entry point, that the core runtime does not implement `name`.
#[cfg(feature = "core-runtime")]
#[cold]
#[inline(never)]
pub fn refuse_under_core(name: &'static str) {
    mtld3d_shared::log_once_warn_by!(
        target: crate::LOG_TARGET,
        key: name.as_ptr() as u64,
        "d3d9 core: {name} is not implemented under runtime.mode = core → D3DERR_NOTAVAILABLE"
    );
}

#[cfg(feature = "core-runtime")]
#[cold]
#[inline(never)]
#[track_caller]
fn missed_guard() -> ! {
    panic!(
        "d3d9 core: an entry point without its runtime.mode = core guard reached mtld3d's recorder \
         at {}",
        core::panic::Location::caller()
    )
}
