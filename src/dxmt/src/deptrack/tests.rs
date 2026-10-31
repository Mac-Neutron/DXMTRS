use core::{cell::RefCell, fmt::Write};

use super::*;

/// What the C++ answered to the script [`replay`] runs.
///
/// A C++ program running the same script over `dxmt_deptrack.cpp` printed it (twice, equal).
const GOLDEN: &str = include_str!("golden.txt");

/// The oracle's xorshift64, drawn in the oracle's order.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    const fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn words(s: &FenceSet) -> String {
    let w = s.0;
    format!("{:x}:{:x}:{:x}:{:x}", w[0], w[1], w[2], w[3])
}

fn rand_set(rng: &mut Rng, id: u64) -> FenceSet {
    let mut s = FenceSet::EMPTY;
    for _ in 0..rng.below(4) {
        s.set(id - 1 - rng.below(200));
    }
    s
}

/// The oracle's script, printed as the oracle prints it.
fn replay() -> String {
    let mut out = String::new();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    out.push_str("== fenceset\n");
    let mut sets = [FenceSet::EMPTY, FenceSet::EMPTY];
    for _ in 0..256 {
        let (op, s) = (rng.below(14), rng.below(2));
        let [first, second] = &mut sets;
        let (me, other) = if s == 0 {
            (first, &*second)
        } else {
            (second, &*first)
        };
        match op {
            0 => {
                let id = rng.below(1024);
                me.set(id);
                write!(out, "set {s} {id}").unwrap();
            }
            1 => {
                let id = rng.below(1024);
                me.unset(id);
                write!(out, "unset {s} {id}").unwrap();
            }
            2 => {
                let id = rng.below(1024);
                write!(out, "test {s} {id} {}", u8::from(me.test(id))).unwrap();
            }
            3 => {
                let id = rng.below(1024);
                write!(out, "tas {s} {id} {}", u8::from(me.test_and_set(id))).unwrap();
            }
            4 => {
                let (p, l) = (rng.below(4), rng.below(64));
                me.fill_generation_before(p, l);
                write!(out, "fill {s} {p} {l}").unwrap();
            }
            5 => write!(out, "inter {s} {}", u8::from(me.intersects(other))).unwrap(),
            6 => write!(out, "contains {s} {}", u8::from(me.contains(other))).unwrap(),
            7 => {
                me.merge(other);
                write!(out, "merge {s}").unwrap();
            }
            8 => write!(out, "union {s} {}", words(&me.union(other))).unwrap(),
            9 => {
                me.subtract(other);
                write!(out, "subtract {s}").unwrap();
            }
            10 => {
                let m = rng.next();
                me.merge_with_lane_mask_off(other, m);
                write!(out, "mlmo {s} {m:x}").unwrap();
            }
            11 => write!(out, "lanemask {s} {:x}", me.lane_mask()).unwrap(),
            12 => {
                write!(out, "reset {s} {}", u8::from(me.is_empty())).unwrap();
                *me = FenceSet::EMPTY;
            }
            _ => {
                write!(out, "each {s}").unwrap();
                me.for_each(|slot| write!(out, " {slot}").unwrap());
                out.push_str(" |");
                let seen = RefCell::new(String::new());
                me.for_each_with_prior(
                    other,
                    |slot| write!(seen.borrow_mut(), " p{slot}").unwrap(),
                    |slot| write!(seen.borrow_mut(), " f{slot}").unwrap(),
                );
                out.push_str(&seen.into_inner());
            }
        }
        writeln!(out, " => {} {}", words(&sets[0]), words(&sets[1])).unwrap();
    }

    out.push_str("== tracker\n");
    let mut trackers: Vec<GenericAccessTracker> =
        (0..8).map(|_| GenericAccessTracker::default()).collect();
    let mut next_id = 256;
    for _ in 0..120 {
        if rng.below(16) == 0 {
            next_id += rng.below(100);
        }
        let render = rng.below(3) == 1;
        let vid = next_id;
        next_id += 1 + u64::from(render);
        let id = next_id - 1;
        let (mut wait, mut wait_v, mut b) =
            (FenceSet::EMPTY, FenceSet::EMPTY, BarrierState::default());
        for _ in 0..=rng.below(6) {
            let (t, w) = (rng.below(8), rng.below(3));
            let st = if render { rng.below(2) } else { 2 };
            let tr = &mut trackers[usize::try_from(t).unwrap()];
            match (st, w) {
                (2, 0) => tr.access_shared(id, &mut wait, &mut b),
                (2, _) => tr.access_exclusive(id, &mut wait, &mut b, w == 2),
                (0, 0) => tr.access_shared_pre_raster(vid, &mut wait_v, &mut b),
                (0, _) => tr.access_exclusive_pre_raster(vid, &mut wait_v, &mut b, w == 2),
                (_, 0) => tr.access_shared_fragment(id, &mut wait, &mut b),
                _ => tr.access_exclusive_fragment(id, &mut wait, &mut b, w == 2),
            }
            writeln!(
                out,
                "e{vid} t{t} {st} {w} => {} {} {} {} {} {}",
                words(&wait),
                words(&wait_v),
                b.general,
                b.pre_raster,
                b.fragment_after_pre_raster,
                b.pre_raster_after_fragment
            )
            .unwrap();
        }
    }

    out.push_str("== locality\n");
    let mut check = FenceLocalityCheck::default();
    let mut next_id = 256;
    for _ in 0..160 {
        if rng.below(16) == 0 {
            next_id += rng.below(300);
        }
        if rng.below(2) == 1 {
            let (vid, fid) = (next_id, next_id + 1);
            next_id += 2;
            let sv = rand_set(&mut rng, vid);
            let r1 = check.collect_and_simplify_waits(sv, vid, false);
            let sf = rand_set(&mut rng, fid);
            let r2 = check.collect_and_simplify_waits(sf, fid, true);
            writeln!(out, "r {vid} {} {}", words(&r1), words(&r2)).unwrap();
        } else {
            let id = next_id;
            next_id += 1;
            let s = rand_set(&mut rng, id);
            let r = check.collect_and_simplify_waits(s, id, false);
            writeln!(out, "c {id} {}", words(&r)).unwrap();
        }
    }
    out
}

/// One `== <name>` section of a replay.
fn section<'a>(text: &'a str, name: &str) -> &'a str {
    let start = text.find(&format!("== {name}\n")).unwrap();
    let rest = &text[start..];
    let end = rest[3..].find("== ").map_or(rest.len(), |e| e + 3);
    &rest[..end]
}

fn assert_section_as_cpp(name: &str) {
    let ours = replay();
    let (ours, cpp) = (section(&ours, name), section(GOLDEN, name));
    for (n, (o, c)) in ours.lines().zip(cpp.lines()).enumerate() {
        assert_eq!(o, c, "{name} line {n}");
    }
    assert_eq!(ours.lines().count(), cpp.lines().count(), "{name} lines");
}

#[test]
fn fenceset_ops_as_cpp() {
    assert_section_as_cpp("fenceset");
}

#[test]
fn trackers_as_cpp() {
    assert_section_as_cpp("tracker");
}

#[test]
fn locality_as_cpp() {
    assert_section_as_cpp("locality");
}

#[test]
fn weak_mask_is_the_generation_before() {
    // Fence 300 (parity 0, lane 44 of generation 4): the LANE encoders up to 7 * LANE + 44.
    let w = &WEAK_FENCE_MASK[index(300)];
    let last = 7 * LANE + 44;
    for id in last + 1 - LANE..=last {
        assert!(w.test(id), "{id}");
    }
    assert_eq!(w.0.iter().map(|x| x.count_ones()).sum::<u32>(), 64);
}

#[test]
fn tracker_read_after_write_waits() {
    let mut t = GenericAccessTracker::default();
    let mut b = BarrierState::default();
    let mut w = FenceSet::EMPTY;
    t.access_exclusive(256, &mut w, &mut b, false);
    assert!(w.is_empty(), "a first write waits for nothing");

    let mut w = FenceSet::EMPTY;
    t.access_shared(257, &mut w, &mut b);
    assert_eq!(w.0, FenceSet::of(256).0, "a read waits for the write");

    let mut w = FenceSet::EMPTY;
    t.access_exclusive(258, &mut w, &mut b, false);
    assert!(
        w.test(257) && w.test(256),
        "a write waits for the read and the write"
    );

    let mut w = FenceSet::EMPTY;
    t.access_shared(258 + LANE, &mut w, &mut b);
    assert!(w.is_empty(), "a write LANE encoders back has finished");

    let (mut w, mut b) = (FenceSet::EMPTY, BarrierState::default());
    t.access_exclusive(400, &mut w, &mut b, false);
    t.access_shared(400, &mut w, &mut b);
    assert_eq!(
        b.general, BARRIER_RW,
        "a read after its own encoder's write is a barrier"
    );
    t.access_exclusive(400, &mut w, &mut b, true);
    assert_eq!(b.general, BARRIER_RW | BARRIER_WAW);

    let (mut wv, mut b) = (FenceSet::EMPTY, BarrierState::default());
    t.access_exclusive_fragment(403, &mut w, &mut b, false);
    t.access_shared_pre_raster(404, &mut wv, &mut b);
    t.access_shared_fragment(405, &mut w, &mut b);
    assert!(
        wv.test(403),
        "the vertex half waits for the earlier fragment write"
    );
    assert_eq!(
        b.fragment_after_pre_raster, 0,
        "the vertex half's read covers the fragment's"
    );
}

#[test]
fn locality_check_drops_local_waits() {
    let mut check = FenceLocalityCheck::default();
    // A chain: each encoder waits for the one before it.
    for id in 256..1000 {
        let _ = check.collect_and_simplify_waits(FenceSet::of(id - 1), id, false);
    }
    let mut both = FenceSet::of(998);
    both.set(999);
    let waits = check.collect_and_simplify_waits(both, 1000, false);
    assert_eq!(waits.0, FenceSet::of(999).0, "999 waited for 998 already");
    // A render encoder: its vertex half 1001 waits for 1000, so its fragment half needs nothing.
    let vertex = check.collect_and_simplify_waits(FenceSet::of(1000), 1001, false);
    assert_eq!(vertex.0, FenceSet::of(1000).0);
    let fragment = check.collect_and_simplify_waits(FenceSet::of(1000), 1002, true);
    assert!(
        fragment.is_empty(),
        "the vertex half is waited for without a fence"
    );
}
