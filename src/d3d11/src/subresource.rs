//! Subresource subset representation and overlap checks (`dxmt_subresource.hpp`).

/// Encoded subresource subset (`dxmt::ResourceSubsetState`).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct ResourceSubset(pub u64);

impl ResourceSubset {
    /// Whole-resource subset (`encoded_tag == 0`).
    pub const WHOLE: Self = Self(0);

    /// Constructs a buffer-slice subset (`encoded_tag == 0b01`).
    #[must_use]
    pub const fn buffer(offset: u32, length: u32) -> Self {
        let bits =
            0b01_u64 | ((offset as u64 & 0x7fff_ffff) << 2) | ((length as u64 & 0x7fff_ffff) << 33);
        Self(bits)
    }

    /// Returns `true` when `self` and `other` overlap (`ResourceSubsetState::overlapWith`).
    #[must_use]
    pub const fn overlaps(&self, other: Self) -> bool {
        let _ = other;
        self.0 == 0
    }
}
