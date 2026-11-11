//! The process's runtime mode (DXMTRS C4): the first device's `runtime.mode` wins.
//!
//! One process runs one runtime: `mtld3d.so`'s or DXMT's core. The first `Direct3DCreate9(Ex)`
//! latches its mode; a later interface asking for the other one is refused, so no device of one
//! runtime ever meets an object of the other.

use core::sync::atomic::{AtomicU8, Ordering};

use crate::config::RuntimeMode;

/// A process-wide latch of [`RuntimeMode`].
pub struct RuntimeLatch(AtomicU8);

const UNSET: u8 = 0;

const fn code(mode: RuntimeMode) -> u8 {
    match mode {
        RuntimeMode::Mtld3d => 1,
        RuntimeMode::Core => 2,
    }
}

const fn mode(code: u8) -> Option<RuntimeMode> {
    match code {
        1 => Some(RuntimeMode::Mtld3d),
        2 => Some(RuntimeMode::Core),
        _ => None,
    }
}

impl RuntimeLatch {
    /// A latch nothing has set.
    #[must_use]
    pub const fn new() -> Self {
        Self(AtomicU8::new(UNSET))
    }

    /// The latched mode, if any.
    #[must_use]
    pub fn get(&self) -> Option<RuntimeMode> {
        mode(self.0.load(Ordering::Acquire))
    }

    /// Latches `wanted` unless a mode is latched already.
    ///
    /// # Errors
    /// The latched mode, when it is not `wanted`.
    pub fn latch(&self, wanted: RuntimeMode) -> Result<RuntimeMode, RuntimeMode> {
        match self
            .0
            .compare_exchange(UNSET, code(wanted), Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Ok(wanted),
            Err(held) => match mode(held) {
                Some(m) if m == wanted => Ok(m),
                Some(m) => Err(m),
                None => Ok(wanted),
            },
        }
    }
}

impl Default for RuntimeLatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
