use super::{
    ClearHistory, ClearPlanes, FEED_MEMORY_FRAMES, mark_kept_reads_generic, may_skip_draw,
};

/// One clear is not enough; a clear in each of two consecutive frames is.
#[test]
fn an_attachment_is_regenerated_after_two_consecutive_cleared_frames() {
    let mut history: ClearHistory<u32> = ClearHistory::new();
    let rt = 0x100;
    history.record_key(rt, 0, ClearPlanes::COLOR);
    assert!(
        !history.regenerated(rt, 0, ClearPlanes::COLOR),
        "a first clear may open a one-off render"
    );
    history.begin_frame();
    assert!(!history.regenerated(rt, 0, ClearPlanes::COLOR));
    history.record_key(rt, 0, ClearPlanes::COLOR);
    history.record_key(rt, 0, ClearPlanes::COLOR);
    assert!(history.regenerated(rt, 0, ClearPlanes::COLOR));
    history.begin_frame();
    assert!(
        !history.regenerated(rt, 0, ClearPlanes::COLOR),
        "not before this frame's own clear"
    );
    history.record_key(rt, 0, ClearPlanes::COLOR);
    assert!(history.regenerated(rt, 0, ClearPlanes::COLOR));
}

/// A frame without the clear breaks the streak and the entry is dropped.
#[test]
fn a_skipped_clear_restarts_the_streak() {
    let mut history: ClearHistory<u32> = ClearHistory::new();
    let rt = 0x100;
    history.record_key(rt, 0, ClearPlanes::COLOR);
    history.begin_frame();
    history.begin_frame();
    history.record_key(rt, 0, ClearPlanes::COLOR);
    assert!(!history.regenerated(rt, 0, ClearPlanes::COLOR));
    history.begin_frame();
    history.record_key(rt, 0, ClearPlanes::COLOR);
    assert!(history.regenerated(rt, 0, ClearPlanes::COLOR));
    history.begin_frame();
    history.begin_frame();
    assert!(history.is_empty(), "stale attachments are forgotten");
}

/// Planes and subresources are tracked apart.
#[test]
fn planes_and_subresources_are_separate() {
    let mut history: ClearHistory<u32> = ClearHistory::new();
    let ds = 0x200;
    for _ in 0..2 {
        history.begin_frame();
        history.record_key(ds, 0, ClearPlanes::DEPTH);
        history.record_key(ds, 1, ClearPlanes::DEPTH | ClearPlanes::STENCIL);
    }
    assert!(history.regenerated(ds, 0, ClearPlanes::DEPTH));
    assert!(
        !history.regenerated(ds, 0, ClearPlanes::STENCIL),
        "a depth-only clear leaves stencil kept"
    );
    assert!(history.regenerated(ds, 1, ClearPlanes::STENCIL));
    assert!(!history.regenerated(ds, 2, ClearPlanes::DEPTH));
}

/// A destroyed texture's key names a new one, which inherits none of its clears.
#[test]
fn a_forgotten_texture_starts_over_at_its_address() {
    let mut history: ClearHistory<u32> = ClearHistory::new();
    let rt = 0x300;
    history.record_key(rt, 0, ClearPlanes::COLOR);
    history.begin_frame();
    history.record_key(rt, 0, ClearPlanes::COLOR);
    assert!(history.regenerated(rt, 0, ClearPlanes::COLOR));
    history.forget(rt);
    history.record_key(rt, 0, ClearPlanes::COLOR);
    assert!(
        !history.regenerated(rt, 0, ClearPlanes::COLOR),
        "a one-off target at a reused address is not rebuilt every frame"
    );
    assert!(history.tracks(rt));
}

/// A `Reset` forgets every texture at once.
#[test]
fn clearing_forgets_every_texture() {
    let mut history: ClearHistory<u32> = ClearHistory::new();
    let rt = 0x300;
    let ds = 0x400;
    for _ in 0..2 {
        history.begin_frame();
        history.record_key(rt, 0, ClearPlanes::COLOR);
        history.record_key(ds, 0, ClearPlanes::DEPTH);
    }
    history.mark_feeds_persistent_key(rt);
    history.clear();
    assert!(history.is_empty());
    assert!(!history.regenerated(rt, 0, ClearPlanes::COLOR));
    assert!(!history.regenerated(ds, 0, ClearPlanes::DEPTH));
    assert!(!history.feeds_persistent(rt));
}

/// A read into kept content holds for many frames after it, then lapses.
#[test]
fn feeding_kept_content_lasts_the_feed_memory_then_lapses() {
    let mut history: ClearHistory<u32> = ClearHistory::new();
    let scratch = 0x500;
    assert!(!history.feeds_persistent(scratch));
    assert!(!history.tracks(scratch));
    assert!(
        history.mark_feeds_persistent_key(scratch),
        "a first mark is new"
    );
    assert!(
        !history.mark_feeds_persistent_key(scratch),
        "a repeat is not"
    );
    assert!(history.feeds_persistent(scratch));
    assert!(!history.tracks(scratch), "a read is not a clear");
    for _ in 0..FEED_MEMORY_FRAMES {
        history.begin_frame();
    }
    assert!(
        history.feeds_persistent(scratch),
        "a periodic kept read keeps its mark between reads"
    );
    history.begin_frame();
    assert!(!history.feeds_persistent(scratch));
    assert!(history.is_empty(), "and the texture is forgotten");
}

#[test]
fn skip_requires_every_touched_plane_rebuilt() {
    assert!(may_skip_draw(&[true], Some(true), None));
    assert!(!may_skip_draw(&[true, false], Some(true), None));
    assert!(!may_skip_draw(&[true], Some(false), None));
    assert!(!may_skip_draw(&[true], Some(true), Some(false)));
    assert!(may_skip_draw(&[], Some(true), Some(true)));
}

/// Generic `ClearHistory<K>` and `mark_kept_reads_generic` propagate kept marks across custom keys.
#[test]
fn generic_clear_history_and_mark_kept_reads() {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    enum Key {
        BackBuffer,
        Tex(u32),
    }

    let mut history: ClearHistory<Key> = ClearHistory::new();
    for _ in 0..2 {
        history.begin_frame();
        history.record_key(Key::Tex(1), 0, ClearPlanes::COLOR);
        history.record_key(Key::Tex(2), 0, ClearPlanes::COLOR);
    }
    assert!(history.regenerated(Key::Tex(1), 0, ClearPlanes::COLOR));
    assert!(history.regenerated(Key::Tex(2), 0, ClearPlanes::COLOR));

    // Pass 0 writes Tex(2) and reads Tex(1); Pass 1 writes BackBuffer (persistent) and reads Tex(2).
    let reads = [(0, Key::Tex(1)), (1, Key::Tex(2))];
    mark_kept_reads_generic(&reads, &mut history, |h, pass| match pass {
        0 => !h.regenerated(Key::Tex(2), 0, ClearPlanes::COLOR) || h.feeds_persistent(Key::Tex(2)),
        1 => !h.regenerated(Key::BackBuffer, 0, ClearPlanes::COLOR),
        _ => true,
    });
    assert!(history.feeds_persistent(Key::Tex(2)));
    assert!(history.feeds_persistent(Key::Tex(1)));
}
