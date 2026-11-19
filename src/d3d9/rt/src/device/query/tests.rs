use std::sync::{Arc, Mutex};

use dxmt_core::query::VisibilityQuery;
use dxmt_fakes::take_calls;
use mtld3d_core::{ids::TextureId, stretch_rect::BlitDecode};
use winemetal_sys::wire::{WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_RENDER_COMMAND_SET_VISIBILITY_MODE};

use super::{
    Occlusion,
    OcclusionAnswer::{Count, Never, Pending},
    Span,
};
use crate::device::{
    CoreDevice, DrawData, IndexData, Indices, StretchQuad, TargetRef, TextureKind,
    fixture::{core_on_fakes, ff_binds, key, passes, returns_within_10s, target_desc, triangle},
};

/// Draws the fixture's triangle into the bound target's pass.
fn draw(core: &mut CoreDevice) {
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
}

/// The bracket `occlusion` last began.
fn bracket(occlusion: &Occlusion) -> Arc<VisibilityQuery> {
    match &occlusion.span {
        Span::Open(query) | Span::Closed { query, .. } => Arc::clone(query),
        Span::Never | Span::Empty => panic!("no bracket begun"),
    }
}

/// The chunk being recorded.
fn current(core: &CoreDevice) -> u64 {
    core.device().queue().current_seq()
}

/// The render passes `device` encoded since the last take, each as its lines.
fn taken_passes(device: u64) -> Vec<Vec<String>> {
    let calls = take_calls(device);
    let found = passes(&calls);
    found.into_iter().map(<[String]>::to_vec).collect()
}

/// A visibility-mode record as the fakes print it: its type alone, not the mode it sets.
fn mode_line() -> String {
    format!("  render {WMT_RENDER_COMMAND_SET_VISIBILITY_MODE}")
}

/// The visibility-mode records `pass` holds, and how many of them come before its first draw.
fn modes(pass: &[String]) -> (usize, usize) {
    let mode = mode_line();
    let first = pass
        .iter()
        .position(|c| c.starts_with("  render 19 ") || c.starts_with("  render 20 "))
        .unwrap_or(pass.len());
    let count = |lines: &[String]| lines.iter().filter(|c| **c == mode).count();
    (count(pass), count(&pass[..first]))
}

/// Level 0 of a new 64x64 BGRA8 render-target texture, its creation drained and taken.
fn target(core: &mut CoreDevice, device: u64) -> TargetRef {
    let id = TextureId::new_unique();
    let desc = target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, TextureKind::Tex2d, 64, 1, 4);
    core.texture_create(id, &desc);
    core.drain();
    let _ = take_calls(device);
    TargetRef::Texture {
        id,
        level: 0,
        face: None,
    }
}

/// A point-sampled copy of a 16x16 rect of `src` onto a 16x16 rect of `dst`.
const fn copy(src: TargetRef, dst: TargetRef) -> StretchQuad {
    StretchQuad {
        src,
        src_rect: [0, 0, 16, 16],
        src_view: WMT_PIXEL_FORMAT_BGRA8_UNORM,
        dst,
        dst_rect: [0, 0, 16, 16],
        linear: false,
        decode: BlitDecode::None.uniform(),
    }
}

/// Draws `data` and drains: the visibility-mode records of the one pass `device` then encoded.
///
/// As [`modes`] counts them: all of them, and those before the pass's first draw.
fn draw_modes(core: &mut CoreDevice, device: u64, data: &DrawData<'_>) -> (usize, usize) {
    assert_eq!(core.draw(&key(0xf), data), Ok(()));
    core.drain();
    let found = taken_passes(device);
    assert_eq!(found.len(), 1, "{found:#?}");
    modes(&found[0])
}

/// A `log` sink that keeps the text of every error line.
struct Errors(Mutex<Vec<String>>);

impl log::Log for Errors {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Error
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.0
                .lock()
                .expect("the error sink")
                .push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

/// An EVENT whose chunk the queue retired is done, and the answer commits nothing.
#[test]
fn an_event_in_a_retired_chunk_is_done_without_a_commit() {
    let mut core = core_on_fakes(0x470);
    let seq = core.event_stamp();
    assert_ne!(seq, 0);
    draw(&mut core);
    core.drain();
    let now = current(&core);
    assert!(seq < now);
    assert!(core.event_done(seq));
    assert!(core.event_done(0), "never issued");
    assert_eq!(current(&core), now);
}

/// An EVENT in the chunk being recorded commits it, a chunk with a draw or an empty one.
///
/// The commit is the one the poll makes (the chunk one up); once the queue retired the chunk
/// the EVENT is done, with no commit more. Whether the first poll already answers true depends
/// on the queue's finish thread, so it is not asserted.
#[test]
fn an_event_in_the_recording_chunk_commits_it() {
    returns_within_10s(|| {
        let mut core = core_on_fakes(0x471);
        core.drain();
        for drawn in [true, false] {
            if drawn {
                draw(&mut core);
            } else {
                assert!(!core.dirty && !core.render_open, "an empty chunk");
            }
            let seq = core.event_stamp();
            assert_eq!(seq, current(&core));
            let _ = core.event_done(seq);
            assert_eq!(current(&core), seq + 1, "drawn {drawn}");
            core.drain();
            assert!(core.event_done(seq), "drawn {drawn}");
            assert_eq!(current(&core), seq + 1, "drawn {drawn}");
        }
    });
}

/// A poll answers `Never` before any issue and `Pending` while open; without the flag, no commit.
///
/// Open: pending with the flag and without, nothing committed. Ended: pending without the flag,
/// nothing committed; with it, the END's chunk is committed and waited for, and the count is
/// the fakes' zeroed result.
#[test]
fn a_poll_without_the_flag_commits_nothing() {
    let mut core = core_on_fakes(0x472);
    let mut query = Occlusion::default();
    assert_eq!(core.occlusion_poll(&query, true), Never);
    assert_eq!(core.occlusion_poll(&query, false), Never);
    core.occlusion_begin(&mut query);
    draw(&mut core);
    let now = current(&core);
    assert_eq!(core.occlusion_poll(&query, true), Pending);
    assert_eq!(core.occlusion_poll(&query, false), Pending);
    assert_eq!(current(&core), now);
    core.occlusion_end(&mut query);
    assert_eq!(core.occlusion_poll(&query, false), Pending);
    assert_eq!(current(&core), now);
    assert_eq!(core.occlusion_poll(&query, true), Count(0));
    assert_eq!(current(&core), now + 1);
}

/// A bracket whose END sits alone in its chunk is read back once that chunk retired.
///
/// BEGIN and a draw in one chunk, committed; the END in the next, which writes no result, so
/// dxmt-core makes no read-back for it. The poll with the flag answers the count all the same.
#[test]
fn an_end_alone_in_its_chunk_is_read_back() {
    let mut core = core_on_fakes(0x473);
    let mut query = Occlusion::default();
    core.occlusion_begin(&mut query);
    draw(&mut core);
    core.commit();
    core.occlusion_end(&mut query);
    assert_eq!(core.occlusion_poll(&query, true), Count(0));
}

/// BEGIN twice ends the first bracket and begins a fresh one; each begin has its end.
///
/// The second bracket is another `VisibilityQuery`; the first one's END ran in the chunk both
/// were recorded in, and the device's open count is back at 0 after the second END. A later
/// draw pass with no query active prints one visibility-mode record, the pass's own: a begin
/// left without its end would add the draw's counting record.
#[test]
fn a_second_begin_ends_the_first_bracket() {
    const DEVICE: u64 = 0x474;
    let mut core = core_on_fakes(DEVICE);
    let mut query = Occlusion::default();
    let seq = core.event_stamp();
    core.occlusion_begin(&mut query);
    let first = bracket(&query);
    draw(&mut core);
    core.occlusion_begin(&mut query);
    let second = bracket(&query);
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(core.open_brackets, Some(1));
    draw(&mut core);
    core.occlusion_end(&mut query);
    assert_eq!(core.open_brackets, Some(0));
    assert_eq!(core.occlusion_poll(&query, true), Count(0));
    assert_eq!(first.end_at(), seq);
    assert_eq!(second.end_at(), seq);
    let _ = take_calls(DEVICE);
    draw(&mut core);
    core.drain();
    let found = taken_passes(DEVICE);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(modes(&found[0]), (1, 1), "{found:#?}");
}

/// A release while open records the END; a release of a bracket that is not open records nothing.
///
/// The open count is back at 0 and the END ran in its chunk; a later draw pass prints the pass's
/// own visibility-mode record alone.
#[test]
fn a_release_while_open_ends_the_bracket() {
    const DEVICE: u64 = 0x475;
    let mut core = core_on_fakes(DEVICE);
    let mut query = Occlusion::default();
    core.occlusion_begin(&mut query);
    let seq = core.event_stamp();
    draw(&mut core);
    core.occlusion_release(&mut query);
    assert_eq!(core.open_brackets, Some(0));
    let ended = bracket(&query);
    core.drain();
    assert_eq!(ended.end_at(), seq);
    core.occlusion_release(&mut query);
    assert!(!core.dirty, "a second release records nothing");
    let _ = take_calls(DEVICE);
    draw(&mut core);
    core.drain();
    let found = taken_passes(DEVICE);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(modes(&found[0]), (1, 1), "{found:#?}");
}

/// An END before any BEGIN records nothing and answers 0, with the flag or without.
#[test]
fn an_end_first_answers_0_with_nothing_recorded() {
    let mut core = core_on_fakes(0x476);
    core.drain();
    let now = current(&core);
    let mut query = Occlusion::default();
    core.occlusion_end(&mut query);
    assert!(!core.dirty);
    assert_eq!(core.open_brackets, None);
    assert_eq!(core.occlusion_poll(&query, false), Count(0));
    assert_eq!(core.occlusion_poll(&query, true), Count(0));
    core.occlusion_end(&mut query);
    assert!(!core.dirty);
    assert_eq!(current(&core), now);
}

/// A second END records nothing and keeps the result.
#[test]
fn a_second_end_keeps_the_result() {
    let mut core = core_on_fakes(0x477);
    let mut query = Occlusion::default();
    core.occlusion_begin(&mut query);
    draw(&mut core);
    core.occlusion_end(&mut query);
    core.drain();
    core.occlusion_end(&mut query);
    assert!(!core.dirty);
    assert_eq!(core.open_brackets, Some(0));
    assert_eq!(core.occlusion_poll(&query, false), Count(0));
}

/// A device dropped with brackets open logs one error line that counts them.
///
/// The one test of this binary that sets a logger, and the one that leaves a bracket open.
#[test]
fn a_device_dropped_with_brackets_open_logs_them() {
    const LINE: &str = "d3d9 core: 2 occlusion queries' END was not recorded";
    let errors: &'static Errors = Box::leak(Box::new(Errors(Mutex::new(Vec::new()))));
    log::set_logger(errors).expect("the only logger");
    log::set_max_level(log::LevelFilter::Error);
    let mut core = core_on_fakes(0x478);
    let (mut a, mut b) = (Occlusion::default(), Occlusion::default());
    core.occlusion_begin(&mut a);
    core.occlusion_begin(&mut b);
    drop(core);
    let logged = errors
        .0
        .lock()
        .expect("the error sink")
        .iter()
        .filter(|line| *line == LINE)
        .count();
    assert_eq!(logged, 1);
}

/// From the device's first BEGIN on, a draw pass starts with counting off; both draw records bump.
///
/// Counts of visibility-mode records in each pass, and of those before its first draw: none
/// before any BEGIN; with a query active the pass's own and the draw's counting record (the
/// indexed draw's too); with none active after the END, the pass's own alone.
#[test]
fn a_draw_pass_starts_with_counting_off_once_a_query_began() {
    const DEVICE: u64 = 0x479;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = take_calls(DEVICE);
    let binds = ff_binds();
    let list = [0_u8, 0, 1, 0, 2, 0];
    let indexed = DrawData {
        indices: Some(Indices {
            data: IndexData::Bytes(&list),
            index_type: 0,
            count: 3,
        }),
        ..triangle(&binds)
    };
    let mut query = Occlusion::default();
    let plain = triangle(&binds);
    // Before any BEGIN.
    assert_eq!(draw_modes(&mut core, DEVICE, &plain), (0, 0));
    core.occlusion_begin(&mut query);
    assert_eq!(draw_modes(&mut core, DEVICE, &plain), (2, 2));
    core.occlusion_end(&mut query);
    core.occlusion_begin(&mut query);
    assert_eq!(draw_modes(&mut core, DEVICE, &indexed), (2, 2));
    core.occlusion_end(&mut query);
    // No query active.
    assert_eq!(draw_modes(&mut core, DEVICE, &plain), (1, 1));
}

/// The `StretchRect` quad's pass starts with counting off once the device began a query.
///
/// None before any BEGIN; one, before its draw, after one. Merged behind a counting draw on its
/// destination, the quad's own record comes after that draw and before the quad's.
#[test]
fn the_quads_pass_starts_with_counting_off_once_a_query_began() {
    const DEVICE: u64 = 0x47a;
    let mut core = core_on_fakes(DEVICE);
    let src = target(&mut core, DEVICE);
    let dst = target(&mut core, DEVICE);
    let mut query = Occlusion::default();
    for (what, want) in [("before any BEGIN", (0, 0)), ("after a BEGIN", (1, 1))] {
        assert!(core.stretch(&copy(src, dst)), "{what}");
        core.drain();
        let found = taken_passes(DEVICE);
        assert_eq!(found.len(), 1, "{what}: {found:#?}");
        assert_eq!(modes(&found[0]), want, "{what}: {found:#?}");
        core.occlusion_begin(&mut query);
        core.occlusion_end(&mut query);
    }
    assert!(core.set_target(dst));
    core.occlusion_begin(&mut query);
    draw(&mut core);
    assert!(core.stretch(&copy(src, dst)));
    core.occlusion_end(&mut query);
    core.drain();
    let found = taken_passes(DEVICE);
    assert_eq!(found.len(), 1, "merged: {found:#?}");
    let pass = &found[0];
    let at = |prefix: &str| -> Vec<usize> {
        pass.iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with(prefix))
            .map(|(i, _)| i)
            .collect()
    };
    let records = at(&mode_line());
    let draws = at("  render 19 ");
    assert_eq!((records.len(), draws.len()), (3, 2), "{pass:#?}");
    assert!(
        records[1] < draws[0] && draws[0] < records[2] && records[2] < draws[1],
        "{pass:#?}"
    );
}

/// The clear quads record no visibility mode, a query active or not.
///
/// A draw, a rect Clear and a draw with a query active: the Clear's pass between the two draw
/// passes prints no visibility-mode record.
#[test]
fn the_clear_quads_record_no_visibility_mode() {
    const DEVICE: u64 = 0x47b;
    let mut core = core_on_fakes(DEVICE);
    core.drain();
    let _ = take_calls(DEVICE);
    let mut query = Occlusion::default();
    core.occlusion_begin(&mut query);
    draw(&mut core);
    core.clear_rects(0, &[[1, 2, 3, 4]], [0.0, 1.0, 0.0, 1.0], false);
    draw(&mut core);
    core.occlusion_end(&mut query);
    core.drain();
    let found = taken_passes(DEVICE);
    assert_eq!(found.len(), 3, "{found:#?}");
    let counts: Vec<_> = found.iter().map(Vec::as_slice).map(modes).collect();
    assert_eq!(counts, [(2, 2), (0, 0), (2, 2)], "{found:#?}");
}
