//! Pure parts of tracking cleared targets and kept reads for async pipeline compilation.

use core::hash::Hash;

use rustc_hash::FxHashMap;

/// Presented frames a texture stays marked as feeding kept content after its last such read.
///
/// A read into kept content that happens every few frames or less (a
/// periodic impostor or environment-map refresh) must not lose its mark
/// between reads, so the mark outlasts many frames; a target that fed kept
/// content once waits for its builds rather than skip them for a long
/// while after, and forever while it keeps feeding. Ten seconds at 60 Hz.
pub const FEED_MEMORY_FRAMES: u64 = 600;

bitflags::bitflags! {
    /// The planes of one attachment a clear reached.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct ClearPlanes: u8 {
        /// A colour target.
        const COLOR = 1 << 0;
        /// The depth plane of a depth-stencil attachment.
        const DEPTH = 1 << 1;
        /// The stencil plane of a depth-stencil attachment, tracked apart from its depth.
        const STENCIL = 1 << 2;
    }
}

/// Whether a draw whose library or pipeline is still building may be left out of this frame.
///
/// Leaving a draw out is safe only when every attachment its result lands
/// in, or that later draws read it back through, is rebuilt from scratch
/// every frame and read only by work rebuilt every frame, so the frame after
/// the build lands shows the draw. Each argument answers that for one
/// attachment plane: `color` for every colour target the pass attaches (a
/// draw that writes only depth still shapes what a later depth-tested draw
/// writes into them), `depth` and `stencil` for the planes the draw tests or
/// writes, `None` for a plane it leaves alone. A target cleared once and
/// drawn once, at load, is exactly the one a skip would lose for good, so a
/// clear in this frame alone does not qualify; see
/// [`ClearHistory::regenerated`] and [`ClearHistory::feeds_persistent`].
#[must_use]
pub fn may_skip_draw(color: &[bool], depth: Option<bool>, stencil: Option<bool>) -> bool {
    color.iter().all(|&rebuilt| rebuilt)
        && depth.is_none_or(|rebuilt| rebuilt)
        && stencil.is_none_or(|rebuilt| rebuilt)
}

/// Per target, the recent frames that cleared its planes, and whether it feeds kept content.
///
/// Keyed by `K`. Each target keeps, per subresource (slice in the low half,
/// level in the high half for colour, or `slice | (level << 16)`) and plane,
/// the index of the last frame a whole clear reached it and of the frame before
/// that, which is all "cleared this frame and the one before" needs; a clear
/// of one face, level or plane says nothing about another. It also keeps the
/// last frame its content was read into something kept (a copy out of it into
/// a kept target, a pass into a kept target sampling it), which holds for
/// [`FEED_MEMORY_FRAMES`] after it. A target neither cleared in the current or
/// the previous frame nor read that way within that span is dropped when the
/// next frame begins, so the map holds only what was touched recently. Keys
/// may be reused on destruction, so a destroyed target is forgotten
/// ([`Self::forget`]) before its identity can name another.
pub struct ClearHistory<K> {
    /// Index of the current presented frame; starts at 1, so 0 means "never".
    frame: u64,
    textures: FxHashMap<K, TextureHistory>,
}

#[derive(Default)]
struct TextureHistory {
    /// `(subresource, plane bit, record)` per plane a recent clear reached.
    clears: Vec<(u32, u8, ClearRecord)>,
    /// Last frame the texture's content was read into kept content; 0 for never.
    fed_kept: u64,
}

struct ClearRecord {
    last: u64,
    before: u64,
}

impl<K: Copy + Eq + Hash> Default for ClearHistory<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Copy + Eq + Hash> ClearHistory<K> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            frame: 1,
            textures: FxHashMap::default(),
        }
    }

    /// Whether any target has a recent clear or read on record.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Start the next presented frame, forgetting targets no recent frame cleared or read.
    ///
    /// A mid-frame flush is not a new frame: the application's frame goes
    /// on, and so do its clears.
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.textures.retain(|_, history| {
            history
                .clears
                .retain(|(_, _, record)| record.last + 1 >= frame);
            !history.clears.is_empty() || history.fed_within(frame)
        });
    }

    /// Remember that `planes` of `key` at `subresource` were cleared whole this frame.
    pub fn record_key(&mut self, key: K, subresource: u32, planes: ClearPlanes) {
        let frame = self.frame;
        let history = self.textures.entry(key).or_default();
        for plane in planes.iter() {
            let bit = plane.bits();
            let position = history
                .clears
                .iter()
                .position(|(sub, plane, _)| *sub == subresource && *plane == bit);
            let index = position.unwrap_or_else(|| {
                history
                    .clears
                    .push((subresource, bit, ClearRecord { last: 0, before: 0 }));
                history.clears.len() - 1
            });
            let record = &mut history.clears[index].2;
            if record.last != frame {
                record.before = record.last;
                record.last = frame;
            }
        }
    }

    /// Whether `plane` of `key` at `subresource` was cleared whole in this frame and the last.
    ///
    /// Two consecutive frames are the evidence that the application rebuilds
    /// the attachment every frame: a clear in this frame alone is just as
    /// likely to open a one-off render whose draw a skip would lose.
    #[must_use]
    pub fn regenerated(&self, key: K, subresource: u32, plane: ClearPlanes) -> bool {
        let Some(history) = self.textures.get(&key) else {
            return false;
        };
        history
            .clears
            .iter()
            .find(|(sub, bit, _)| *sub == subresource && *bit == plane.bits())
            .is_some_and(|(_, _, record)| {
                record.last == self.frame && record.before != 0 && record.before + 1 == self.frame
            })
    }

    /// Whether `key` has a recent clear on record, so a draw into it could ever be skipped.
    #[must_use]
    pub fn tracks(&self, key: K) -> bool {
        self.textures
            .get(&key)
            .is_some_and(|history| !history.clears.is_empty())
    }

    /// Remember that `key`'s content was just read into something kept.
    ///
    /// A `StretchRect` out of it into a target that is not rebuilt every
    /// frame, or a pass into such a target sampling it. A draw left out of
    /// `key` would then be baked into that kept content, so none is for
    /// the next [`FEED_MEMORY_FRAMES`]. Answers whether the target was not
    /// marked before, so a caller propagating marks knows when to stop.
    pub fn mark_feeds_persistent_key(&mut self, key: K) -> bool {
        let frame = self.frame;
        let history = self.textures.entry(key).or_default();
        let newly = !history.fed_within(frame);
        history.fed_kept = frame;
        newly
    }

    /// Whether `key`'s content was read into kept content in the last [`FEED_MEMORY_FRAMES`].
    #[must_use]
    pub fn feeds_persistent(&self, key: K) -> bool {
        self.textures
            .get(&key)
            .is_some_and(|history| history.fed_within(self.frame))
    }

    /// Forget everything about `key`, which is being destroyed or resized.
    pub fn forget(&mut self, key: K) {
        self.textures.remove(&key);
    }

    /// Forget every target, at a device `Reset`.
    ///
    /// A `Reset` recreates the implicit surfaces and ends the application's
    /// frame without a `Present`, so no clear before it vouches for a frame
    /// after it.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}

impl TextureHistory {
    /// Whether a read into kept content happened within [`FEED_MEMORY_FRAMES`] of `frame`.
    const fn fed_within(&self, frame: u64) -> bool {
        self.fed_kept != 0 && self.fed_kept + FEED_MEMORY_FRAMES >= frame
    }
}

/// Propagate `feeds_persistent` marks across `reads` using `pass_is_kept` to judge each pass.
pub fn mark_kept_reads_generic<K: Copy + Eq + Hash>(
    reads: &[(usize, K)],
    history: &mut ClearHistory<K>,
    mut pass_is_kept: impl FnMut(&ClearHistory<K>, usize) -> bool,
) {
    if reads.is_empty() || history.is_empty() {
        return;
    }
    // Each round that marks something marks at least one more target, so
    // the reads bound the rounds.
    for _ in 0..=reads.len() {
        let mut marked = false;
        let mut verdict: Option<(usize, bool)> = None;
        for &(pass, key) in reads {
            if !history.tracks(key) {
                continue;
            }
            let kept = match verdict {
                Some((judged, kept)) if judged == pass => kept,
                _ => {
                    let kept = pass_is_kept(history, pass);
                    verdict = Some((pass, kept));
                    kept
                }
            };
            if kept {
                marked |= history.mark_feeds_persistent_key(key);
            }
        }
        if !marked {
            break;
        }
    }
}

#[cfg(test)]
mod tests;
