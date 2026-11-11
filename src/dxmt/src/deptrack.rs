//! Dependency tracking: the reads and writes each encoder makes of each resource.
//!
//! Every encoder (one Metal pass, or the vertex half of a render pass) takes an id from a counter
//! that starts at [`PARITY_LANE`], and fence `id % PARITY_LANE` of a pool of that many marks its
//! end. A [`FenceSet`] is a set of those fences. A [`GenericAccessTracker`] per buffer
//! suballocation or texture subresource remembers the last write and the reads since, and adds
//! to the accessing encoder's set the fences it must wait for, or the barrier it needs when the
//! earlier access is its own. [`FenceLocalityCheck`] then drops the waits another wait implies.
//!
//! Behaviour is the C++'s (`dxmt_deptrack.*`), but for one fix (DXMTRS C4): a render half's write
//! waits for the target's last write only when it is recent (under [`LANE`] ids back), as every
//! other wait does. The C++ always waited for it; that strong bit, of a write 128 or more ids old,
//! masked the lane of the weak wait that ordered a read of something written exactly 64 ids back.
//! Written ahead for the switches of `d3d11.dll` and `d3d12.dll`, which are its first callers.

/// Fences per parity: the bits of one word.
pub const LANE: u64 = 64;
/// Generations of [`LANE`] fences a [`FenceSet`] holds.
pub const PARITY: u64 = 4;
/// Fences in the pool, and the first encoder id.
pub const PARITY_LANE: u64 = PARITY * LANE;
/// A barrier bit: a read after a write, or a write after a read.
pub const BARRIER_RW: u8 = 1;
/// A barrier bit: an unordered-access write after a write.
pub const BARRIER_WAW: u8 = 1 << 1;
const WORDS: usize = 4;
const LANES: usize = 64;
const SLOTS: usize = WORDS * LANES;
const _: () = assert!(WORDS as u64 == PARITY && LANES as u64 == LANE);

/// For each fence, the [`LANE`] encoders before it: what a wait on it implies.
static WEAK_FENCE_MASK: [FenceSet; SLOTS] = weak_fence_mask();

bitflags::bitflags! {
    /// How an encoder uses a resource: the C++'s `ResourceAccess` bits.
    #[derive(Clone, Copy)]
    pub struct Access: u8 {
        const READ = 1;
        const WRITE = 1 << 1;
        /// An unordered-access view's use, whose writes need a barrier even after a write.
        const UAV = 1 << 2;
    }
}

/// A set of fences: bit `id % LANE` of word `(id / LANE) % PARITY` for encoder `id`.
pub struct FenceSet([u64; WORDS]);

impl FenceSet {
    pub const EMPTY: Self = Self([0; WORDS]);

    /// The set of encoder `id`'s fence alone.
    #[must_use]
    pub const fn of(id: u64) -> Self {
        let mut set = Self::EMPTY;
        set.set(id);
        set
    }

    pub const fn set(&mut self, id: u64) {
        self.0[parity(id)] |= bit(id);
    }

    pub const fn unset(&mut self, id: u64) {
        self.0[parity(id)] &= !bit(id);
    }

    #[must_use]
    pub const fn test(&self, id: u64) -> bool {
        self.0[parity(id)] & bit(id) != 0
    }

    /// Adds encoder `id`'s fence; true when it was there already.
    pub const fn test_and_set(&mut self, id: u64) -> bool {
        let word = &mut self.0[parity(id)];
        if *word & bit(id) != 0 {
            return true;
        }
        *word |= bit(id);
        false
    }

    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.0.iter().zip(&other.0).any(|(a, b)| a & b != 0)
    }

    #[must_use]
    pub fn contains(&self, other: &Self) -> bool {
        self.0.iter().zip(&other.0).all(|(a, b)| a & b == *b)
    }

    pub fn merge(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a |= b;
        }
    }

    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self(core::array::from_fn(|p| self.0[p] | other.0[p]))
    }

    pub fn subtract(&mut self, other: &Self) {
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a &= !b;
        }
    }

    /// Merges `other`'s fences whose lane is not in `mask`.
    pub fn merge_with_lane_mask_off(&mut self, other: &Self, mask: u64) {
        for (a, b) in self.0.iter_mut().zip(&other.0) {
            *a |= b & !mask;
        }
    }

    /// The lanes any generation has.
    #[must_use]
    pub fn lane_mask(&self) -> u64 {
        self.0.iter().fold(0, |m, w| m | w)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lane_mask() == 0
    }

    /// Calls `f` with each fence's slot in the pool, in slot order.
    pub fn for_each(&self, mut f: impl FnMut(usize)) {
        for (p, &word) in self.0.iter().enumerate() {
            let mut lanes = word;
            while lanes != 0 {
                f(slot(p, lanes.trailing_zeros()));
                lanes &= lanes - 1;
            }
        }
    }

    /// Calls `on_prior` with each of `prior`'s slots and `f` with each of this set's others.
    pub fn for_each_with_prior(
        &self,
        prior: &Self,
        mut on_prior: impl FnMut(usize),
        mut f: impl FnMut(usize),
    ) {
        for (p, (&word, &prior_word)) in self.0.iter().zip(&prior.0).enumerate() {
            let (mut lanes, mut prior_lanes) = (word, prior_word);
            while lanes | prior_lanes != 0 {
                let lane = (lanes | prior_lanes).trailing_zeros();
                let b = 1u64 << lane;
                if prior_lanes & b != 0 {
                    on_prior(slot(p, lane));
                } else {
                    f(slot(p, lane));
                }
                lanes &= !b;
                prior_lanes &= !b;
            }
        }
    }

    /// Adds the [`LANE`] encoders before the one at `parity` and `lane` of the next generation.
    const fn fill_generation_before(&mut self, parity: u64, lane: u64) {
        let last = (parity + PARITY + (PARITY - 1)) * LANE + lane;
        let mut offset = 0;
        while offset < LANE {
            self.set(last - offset);
            offset += 1;
        }
    }
}

/// The barriers an encoder needs inside itself, each a set of `BARRIER_*` bits.
#[derive(Default)]
pub struct BarrierState {
    /// Between two accesses of one stage: a compute encoder's, or a fragment's after a fragment's.
    pub general: u8,
    pub pre_raster: u8,
    pub fragment_after_pre_raster: u8,
    pub pre_raster_after_fragment: u8,
}

/// The last write and the reads since of one buffer suballocation or texture subresource.
pub struct GenericAccessTracker {
    shared: TrackingSet,
    exclusive: u64,
    flags: TrackerFlags,
}

impl Default for GenericAccessTracker {
    fn default() -> Self {
        Self {
            shared: TrackingSet {
                ids: [0; LANES],
                cursor: 0,
            },
            exclusive: 0,
            flags: TrackerFlags::empty(),
        }
    }
}

impl GenericAccessTracker {
    /// A compute (or blit) encoder `id` reads.
    pub fn access_shared(&mut self, id: u64, wait: &mut FenceSet, barriers: &mut BarrierState) {
        if self.exclusive == id {
            if self.flags.contains(TrackerFlags::SHARED) {
                return;
            }
            self.flags.insert(TrackerFlags::SHARED);
            barriers.general |= BARRIER_RW;
            return;
        }
        debug_assert!(self.exclusive < id, "accesses come in encoder order");
        if self.shared.is_last_access(id) {
            return;
        }
        self.shared.add(id);
        self.wait_recent_write(id, wait);
    }

    /// A compute (or blit) encoder `id` writes.
    pub fn access_exclusive(
        &mut self,
        id: u64,
        wait: &mut FenceSet,
        barriers: &mut BarrierState,
        uav: bool,
    ) {
        self.flags.remove(TrackerFlags::SHARED);
        if self.exclusive == id {
            barriers.general |= write_barrier(uav);
            return;
        }
        if self.shared.is_last_access(id) {
            barriers.general |= BARRIER_RW;
        }
        self.shared.enumerate(id, |r| wait.set(r));
        self.shared.clear();
        self.wait_recent_write(id, wait);
        self.exclusive = id;
    }

    /// The vertex half `id` of a render encoder reads; its fragment half is `id + 1`.
    pub fn access_shared_pre_raster(
        &mut self,
        id: u64,
        wait: &mut FenceSet,
        barriers: &mut BarrierState,
    ) {
        if self.exclusive == id + 1 {
            if self.flags.contains(TrackerFlags::SHARED_PRE_RASTER) {
                return;
            }
            self.flags.insert(TrackerFlags::SHARED_PRE_RASTER);
            if self
                .flags
                .contains(TrackerFlags::LAST_WRITE_FROM_PRE_RASTER)
            {
                barriers.pre_raster |= BARRIER_RW;
            } else {
                barriers.pre_raster_after_fragment |= BARRIER_RW;
            }
            return;
        }
        if self.exclusive == id {
            if self.flags.contains(TrackerFlags::SHARED_PRE_RASTER) {
                return;
            }
            self.flags.insert(TrackerFlags::SHARED_PRE_RASTER);
            barriers.pre_raster |= BARRIER_RW;
            return;
        }
        debug_assert!(self.exclusive < id, "accesses come in encoder order");
        if self.shared.is_last_access(id + 1) {
            if self.flags.contains(TrackerFlags::SHARED_PRE_RASTER) {
                return;
            }
            self.flags.insert(TrackerFlags::SHARED_PRE_RASTER);
            self.wait_recent_write(id, wait);
            return;
        }
        if self.shared.is_last_access(id) {
            return;
        }
        self.shared.add(id);
        self.wait_recent_write(id, wait);
        self.flags.insert(TrackerFlags::SHARED_PRE_RASTER);
        self.flags.remove(TrackerFlags::SHARED);
    }

    /// The vertex half `id` of a render encoder writes; its fragment half is `id + 1`.
    pub fn access_exclusive_pre_raster(
        &mut self,
        id: u64,
        wait: &mut FenceSet,
        barriers: &mut BarrierState,
        uav: bool,
    ) {
        let last = self.exclusive;
        self.exclusive = id;
        if last == id + 1 {
            let from_pre_raster = self
                .flags
                .contains(TrackerFlags::LAST_WRITE_FROM_PRE_RASTER);
            self.flags.insert(TrackerFlags::LAST_WRITE_FROM_PRE_RASTER);
            if !self.reads_since_write() {
                if from_pre_raster {
                    barriers.pre_raster |= write_barrier(uav);
                } else {
                    barriers.pre_raster_after_fragment |= write_barrier(uav);
                }
                return;
            }
            self.end_reads(
                &mut barriers.pre_raster,
                &mut barriers.pre_raster_after_fragment,
            );
            return;
        }
        if last == id {
            if !self.reads_since_write() {
                barriers.pre_raster |= write_barrier(uav);
            }
            self.end_reads(
                &mut barriers.pre_raster,
                &mut barriers.pre_raster_after_fragment,
            );
            return;
        }
        self.shared.enumerate(id, |r| wait.set(r));
        // Only a recent write, as every other wait: an older one is ordered through the weak
        // waits, and a strong bit for it would mask the lane of a write 64 ids back (DXMTRS C4).
        if id.wrapping_sub(last) < LANE {
            wait.set(last);
        }
        if self.shared.is_last_access(id + 1) || self.shared.is_last_access(id) {
            self.end_reads(
                &mut barriers.pre_raster,
                &mut barriers.pre_raster_after_fragment,
            );
            self.shared.clear();
            return;
        }
        self.shared.clear();
        self.flags
            .remove(TrackerFlags::SHARED | TrackerFlags::SHARED_PRE_RASTER);
    }

    /// The fragment half `id` of a render encoder reads; its vertex half is `id - 1`.
    pub fn access_shared_fragment(
        &mut self,
        id: u64,
        wait: &mut FenceSet,
        barriers: &mut BarrierState,
    ) {
        if self.exclusive == id || self.exclusive == id - 1 {
            if self.flags.contains(TrackerFlags::SHARED) {
                return;
            }
            self.flags.insert(TrackerFlags::SHARED);
            if self.flags.contains(TrackerFlags::SHARED_PRE_RASTER) {
                // The vertex half's read already waited behind a barrier: implicit.
                return;
            }
            if self.exclusive == id - 1
                || self
                    .flags
                    .contains(TrackerFlags::LAST_WRITE_FROM_PRE_RASTER)
            {
                barriers.fragment_after_pre_raster |= BARRIER_RW;
            } else {
                barriers.general |= BARRIER_RW;
            }
            return;
        }
        debug_assert!(self.exclusive < id - 1, "accesses come in encoder order");
        if self.shared.is_last_access(id) {
            return;
        }
        let vertex_read_last = self.shared.is_last_access(id - 1);
        self.wait_recent_write(id, wait);
        self.shared.add(id);
        self.flags.insert(TrackerFlags::SHARED);
        if !vertex_read_last {
            self.flags.remove(TrackerFlags::SHARED_PRE_RASTER);
        }
    }

    /// The fragment half `id` of a render encoder writes; its vertex half is `id - 1`.
    pub fn access_exclusive_fragment(
        &mut self,
        id: u64,
        wait: &mut FenceSet,
        barriers: &mut BarrierState,
        uav: bool,
    ) {
        let last = self.exclusive;
        self.exclusive = id;
        if last == id {
            let from_pre_raster = self
                .flags
                .contains(TrackerFlags::LAST_WRITE_FROM_PRE_RASTER);
            self.flags.remove(TrackerFlags::LAST_WRITE_FROM_PRE_RASTER);
            if !self.reads_since_write() {
                if from_pre_raster {
                    barriers.fragment_after_pre_raster |= write_barrier(uav);
                } else {
                    barriers.general |= write_barrier(uav);
                }
                return;
            }
            self.end_reads(
                &mut barriers.fragment_after_pre_raster,
                &mut barriers.general,
            );
            return;
        }
        self.flags.remove(TrackerFlags::LAST_WRITE_FROM_PRE_RASTER);
        if last == id - 1 {
            if !self.reads_since_write() {
                barriers.fragment_after_pre_raster |= write_barrier(uav);
                return;
            }
            self.end_reads(
                &mut barriers.fragment_after_pre_raster,
                &mut barriers.general,
            );
            return;
        }
        self.shared.enumerate(id, |r| wait.set(r));
        // Only a recent write, as every other wait: an older one is ordered through the weak
        // waits, and a strong bit for it would mask the lane of a write 64 ids back (DXMTRS C4).
        if id.wrapping_sub(last) < LANE {
            wait.set(last);
        }
        if self.shared.is_last_access(id) || self.shared.is_last_access(id - 1) {
            self.end_reads(
                &mut barriers.fragment_after_pre_raster,
                &mut barriers.general,
            );
            self.shared.clear();
            return;
        }
        self.shared.clear();
        self.flags
            .remove(TrackerFlags::SHARED | TrackerFlags::SHARED_PRE_RASTER);
    }

    /// Waits for the last write when it is recent enough that its fence is not reused yet.
    const fn wait_recent_write(&self, id: u64, wait: &mut FenceSet) {
        if id.wrapping_sub(self.exclusive) < LANE {
            wait.set(self.exclusive);
        }
    }

    const fn reads_since_write(&self) -> bool {
        self.flags
            .intersects(TrackerFlags::SHARED.union(TrackerFlags::SHARED_PRE_RASTER))
    }

    /// Ends the reads since the last write, each kind with its barrier before the new write.
    fn end_reads(&mut self, after_pre_raster_read: &mut u8, after_fragment_read: &mut u8) {
        if self.flags.contains(TrackerFlags::SHARED_PRE_RASTER) {
            self.flags.remove(TrackerFlags::SHARED_PRE_RASTER);
            *after_pre_raster_read |= BARRIER_RW;
        }
        if self.flags.contains(TrackerFlags::SHARED) {
            self.flags.remove(TrackerFlags::SHARED);
            *after_fragment_read |= BARRIER_RW;
        }
    }
}

/// The waits each recent encoder made, to drop the waits an earlier wait already implies.
pub struct FenceLocalityCheck {
    summary: Box<[FenceSet; SLOTS]>,
}

impl Default for FenceLocalityCheck {
    fn default() -> Self {
        Self {
            summary: Box::new([FenceSet::EMPTY; SLOTS]),
        }
    }
}

impl FenceLocalityCheck {
    /// The fewest of `strong`'s fences (plus `id - 1`'s when implicit) encoder `id` must wait for.
    ///
    /// A wait implies every wait of the encoder it waits for and the [`LANE`] encoders before
    /// that encoder's fence generation (whose fences it overwrites), so those are dropped. A
    /// render encoder's fragment half passes `implicit_pre_raster_wait`: its own vertex half
    /// counts as waited without being returned.
    pub fn collect_and_simplify_waits(
        &mut self,
        mut strong: FenceSet,
        id: u64,
        implicit_pre_raster_wait: bool,
    ) -> FenceSet {
        if implicit_pre_raster_wait {
            strong.set(id - 1);
        }
        let mut full = FenceSet(strong.0);
        full.merge_with_lane_mask_off(&WEAK_FENCE_MASK[index(id)], strong.lane_mask());
        let mut minimal = FenceSet::EMPTY;
        let mut accessible = FenceSet::EMPTY;
        for offset in 1..PARITY_LANE {
            let prev = id - offset;
            if full.test(prev) && !accessible.test_and_set(prev) {
                minimal.set(prev);
            }
            if accessible.test(prev) {
                accessible.merge(&self.summary[index(prev)]);
            }
            if accessible.contains(&full) {
                break;
            }
        }
        self.summary[index(id)] = full;
        if implicit_pre_raster_wait {
            minimal.unset(id - 1);
        }
        minimal
    }
}

bitflags::bitflags! {
    /// What a tracker knows of the accesses since the last write.
    struct TrackerFlags: u8 {
        /// A compute or fragment read.
        const SHARED = 1;
        const SHARED_PRE_RASTER = 1 << 1;
        const LAST_WRITE_FROM_PRE_RASTER = 1 << 2;
    }
}

/// The last [`LANE`] encoders that read, newest at `cursor`; 0 is no encoder.
struct TrackingSet {
    ids: [u64; LANES],
    cursor: usize,
}

impl TrackingSet {
    fn add(&mut self, id: u64) {
        debug_assert!(self.ids[self.cursor] <= id, "reads come in encoder order");
        if self.ids[self.cursor] == id {
            return;
        }
        self.cursor = (self.cursor + 1) % self.ids.len();
        self.ids[self.cursor] = id;
    }

    const fn is_last_access(&self, id: u64) -> bool {
        self.ids[self.cursor] == id
    }

    const fn clear(&mut self) {
        self.ids[self.cursor] = 0;
    }

    /// Calls `f` with each read by one of the [`LANE`] encoders before `id`, newest first.
    fn enumerate(&self, id: u64, mut f: impl FnMut(u64)) {
        debug_assert!(id > LANE, "encoder ids start at PARITY_LANE");
        let n = self.ids.len();
        for i in 0..n {
            let read = self.ids[(self.cursor + n - i) % n];
            if read >= id {
                debug_assert!(
                    read - id <= 1,
                    "only the encoder's own fragment half reads later"
                );
                continue;
            }
            if read > id.wrapping_sub(LANE) {
                f(read);
                continue;
            }
            break;
        }
    }
}

const fn parity(id: u64) -> usize {
    low_byte(id / LANE % PARITY)
}

const fn bit(id: u64) -> u64 {
    1 << (id % LANE)
}

/// Encoder `id`'s slot in the fence pool.
const fn index(id: u64) -> usize {
    low_byte(id % PARITY_LANE)
}

const fn slot(parity: usize, lane: u32) -> usize {
    parity * LANES + low_byte(lane as u64)
}

/// `v`, below 256, as an index.
const fn low_byte(v: u64) -> usize {
    v.to_le_bytes()[0] as usize
}

/// The barrier bit a write needs after a write in the same encoder.
const fn write_barrier(uav: bool) -> u8 {
    if uav { BARRIER_WAW } else { BARRIER_RW }
}

const fn weak_fence_mask() -> [FenceSet; SLOTS] {
    let mut table = [FenceSet::EMPTY; SLOTS];
    let mut i = 0;
    while i < SLOTS {
        table[i].fill_generation_before((i / LANES) as u64, (i % LANES) as u64);
        i += 1;
    }
    table
}

#[cfg(test)]
mod tests;
