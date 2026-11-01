//! Fixed-capacity slot array with raw-pointer redundancy check (`dxmt_binding_set.hpp`).

/// A fixed-capacity binding array of `N` slots (`dxmt::BindingSet`).
pub struct BindingSet<T, const N: usize> {
    slots: [Option<T>; N],
    bound_mask: u128,
}

impl<T, const N: usize> Default for BindingSet<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> BindingSet<T, N> {
    /// An empty binding set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [const { None }; N],
            bound_mask: 0,
        }
    }

    /// Bitmask of bound slots (`bound_mask`).
    #[must_use]
    pub const fn bound_mask(&self) -> u128 {
        self.bound_mask
    }

    /// Reference to the item bound at `slot`.
    #[must_use]
    pub fn get(&self, slot: usize) -> Option<&T> {
        self.slots.get(slot).and_then(Option::as_ref)
    }

    /// Clears all slots.
    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = None;
        }
        self.bound_mask = 0;
    }
}
