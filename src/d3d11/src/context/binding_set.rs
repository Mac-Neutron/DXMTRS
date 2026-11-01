//! Fixed-capacity slot array with raw-pointer redundancy check (`dxmt_binding_set.hpp`).

/// A fixed-capacity binding array of `N` slots (`dxmt::BindingSet`).
pub struct BindingSet<T, const N: usize> {
    slots: [Option<T>; N],
    bound_mask: u128,
    dirty_mask: u128,
}

impl<T, const N: usize> Default for BindingSet<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone, const N: usize> Clone for BindingSet<T, N> {
    fn clone(&self) -> Self {
        Self {
            slots: self.slots.clone(),
            bound_mask: self.bound_mask,
            dirty_mask: self.dirty_mask,
        }
    }
}

impl<T, const N: usize> BindingSet<T, N> {
    const fn bit(slot: usize) -> u128 {
        if slot < N && slot < 128 {
            1_u128 << slot
        } else {
            0
        }
    }

    const fn all_mask() -> u128 {
        if N >= 128 {
            u128::MAX
        } else {
            (1_u128 << N) - 1
        }
    }

    /// An empty binding set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: [const { None }; N],
            bound_mask: 0,
            dirty_mask: 0,
        }
    }

    /// Bitmask of bound slots (`bound_mask`).
    #[must_use]
    pub const fn bound_mask(&self) -> u128 {
        self.bound_mask
    }

    /// Bitmask of dirty slots (`dirty_mask`).
    #[must_use]
    pub const fn dirty_mask(&self) -> u128 {
        self.dirty_mask
    }

    /// Returns `true` if any slot is currently bound (`any_bound`).
    #[must_use]
    pub const fn any_bound(&self) -> bool {
        self.bound_mask != 0
    }

    /// Returns `true` if any slot is marked dirty (`any_dirty`).
    #[must_use]
    pub const fn any_dirty(&self) -> bool {
        self.dirty_mask != 0
    }

    /// Returns `true` if `slot` is currently bound (`test_bound`).
    #[must_use]
    pub const fn test_bound(&self, slot: usize) -> bool {
        (self.bound_mask & Self::bit(slot)) != 0
    }

    /// Returns `true` if `slot` is marked dirty (`test_dirty`).
    #[must_use]
    pub const fn test_dirty(&self, slot: usize) -> bool {
        (self.dirty_mask & Self::bit(slot)) != 0
    }

    /// Clears all dirty bits (`clear_dirty()`).
    pub const fn clear_dirty(&mut self) {
        self.dirty_mask = 0;
    }

    /// Clears the dirty bit for `slot` (`clear_dirty(slot)`).
    pub const fn clear_dirty_slot(&mut self, slot: usize) {
        self.dirty_mask &= !Self::bit(slot);
    }

    /// Marks every slot dirty (`set_dirty()`).
    pub const fn set_all_dirty(&mut self) {
        self.dirty_mask = Self::all_mask();
    }

    /// Marks `slot` dirty (`set_dirty(slot)`).
    pub const fn set_dirty(&mut self, slot: usize) {
        self.dirty_mask |= Self::bit(slot);
    }

    /// Reference to the item bound at `slot`.
    #[must_use]
    pub fn get(&self, slot: usize) -> Option<&T> {
        self.slots.get(slot).and_then(Option::as_ref)
    }

    /// Mutable reference to the item bound at `slot`.
    #[must_use]
    pub fn get_mut(&mut self, slot: usize) -> Option<&mut T> {
        self.slots.get_mut(slot).and_then(Option::as_mut)
    }

    /// Binds `slot` with redundancy detection (`dxmt::BindingSet::bind`).
    ///
    /// Returns `Some((true, entry))` when `slot` was already bound and `same(existing)`
    /// holds (leaving `dirty_mask` unchanged), or `Some((false, entry))` when `make()` was
    /// stored and `slot` was marked bound and dirty.
    pub fn bind_with(
        &mut self,
        slot: usize,
        same: impl FnOnce(&T) -> bool,
        make: impl FnOnce() -> T,
    ) -> Option<(bool, &mut T)> {
        if slot >= N {
            return None;
        }
        let is_same = self.test_bound(slot) && self.slots[slot].as_ref().is_some_and(same);
        if is_same {
            return self.slots[slot].as_mut().map(|entry| (true, entry));
        }
        self.slots[slot] = Some(make());
        let bit = Self::bit(slot);
        self.bound_mask |= bit;
        self.dirty_mask |= bit;
        self.slots[slot].as_mut().map(|entry| (false, entry))
    }

    /// Binds `value` at `slot`, returning `true` if the slot changed (`!replacement`).
    pub fn bind(&mut self, slot: usize, value: T) -> bool
    where
        T: PartialEq,
    {
        if slot >= N {
            return false;
        }
        if self.test_bound(slot) && self.slots[slot].as_ref() == Some(&value) {
            return false;
        }
        self.slots[slot] = Some(value);
        let bit = Self::bit(slot);
        self.bound_mask |= bit;
        self.dirty_mask |= bit;
        true
    }

    /// Unbinds `slot`, returning `true` if `slot` was previously bound (`unbind`).
    pub fn unbind(&mut self, slot: usize) -> bool {
        if slot >= N || !self.test_bound(slot) {
            return false;
        }
        self.slots[slot] = None;
        let bit = Self::bit(slot);
        self.bound_mask &= !bit;
        self.dirty_mask &= !bit;
        true
    }

    /// Clears all slots.
    pub fn clear(&mut self) {
        for slot in &mut self.slots {
            *slot = None;
        }
        self.bound_mask = 0;
        self.dirty_mask = 0;
    }
}

#[cfg(test)]
mod tests;
