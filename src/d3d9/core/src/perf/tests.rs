//! Unit tests for API-thread perf timers and exclusive-time accounting.

use super::*;

#[test]
fn exclusive_exit_single_timer_books_full_elapsed() {
    let (self_time, restored) = exclusive_exit(200, 0, 0, 0);
    assert_eq!(self_time, 200);
    assert_eq!(restored, 0);
}

#[test]
fn exclusive_exit_nested_pair_no_double_count() {
    let (inner_self, inner_restored) = exclusive_exit(50, 0, 0, 1);
    assert_eq!(inner_self, 50);
    assert_eq!(inner_restored, 50);
    let (outer_self, outer_restored) = exclusive_exit(200, inner_restored, 0, 0);
    assert_eq!(outer_self, 150);
    assert_eq!(outer_restored, 0);
    assert_eq!(inner_self + outer_self, 200);
}

#[test]
fn exclusive_exit_two_siblings_under_parent() {
    let (c1_self, acc) = exclusive_exit(30, 0, 0, 1);
    let (c2_self, acc) = exclusive_exit(40, 0, acc, 1);
    let (p_self, p_restored) = exclusive_exit(100, acc, 0, 0);
    assert_eq!((c1_self, c2_self, p_self), (30, 40, 30));
    assert_eq!(p_restored, 0);
    assert_eq!(c1_self + c2_self + p_self, 100);
}

#[test]
fn exclusive_exit_saturates_when_children_exceed_elapsed() {
    let (self_time, _) = exclusive_exit(10, 25, 0, 1);
    assert_eq!(self_time, 0);
}

/// A final Release may destroy its device inside a child Release's timer.
#[test]
fn api_timer_storage_outlives_nested_device_release() {
    let mut storage = ApiPerfStorage::new();
    let weak = Rc::downgrade(&storage.state);
    let outer = ApiTimer::new(Some(&storage), ApiCategory::Texture, true);
    let inner = ApiTimer::new(Some(&storage), ApiCategory::Device, true);
    storage.state_mut().bump_texture_rename();
    assert_eq!(storage.state.borrow().timer_depth, 2);
    drop(storage);
    assert_eq!(weak.strong_count(), 2);
    drop(inner);
    {
        let retained = weak.upgrade().expect("outer timer owns the state");
        let state = retained.borrow();
        assert_eq!(state.timer_depth, 1);
        assert_eq!(state.counters.texture_renames, 1);
        assert_eq!(
            state.counters.api_call_counts_by_category[ApiCategory::Device as usize],
            1
        );
    }
    drop(outer);
    assert!(weak.upgrade().is_none(), "last timer frees counter storage");
}

#[test]
fn api_timer_storage_outlives_direct_device_release() {
    let storage = ApiPerfStorage::new();
    let weak = Rc::downgrade(&storage.state);
    let timer = ApiTimer::new(Some(&storage), ApiCategory::Device, true);
    drop(storage);
    assert_eq!(weak.strong_count(), 1);
    drop(timer);
    assert!(weak.upgrade().is_none());
}

#[test]
fn api_timer_nested_writeback_balances_depth_and_counts() {
    let storage = ApiPerfStorage::new();
    let outer = ApiTimer::new(Some(&storage), ApiCategory::Surface, true);
    let inner = ApiTimer::new(Some(&storage), ApiCategory::Texture, true);
    drop(inner);
    drop(outer);
    let state = storage.state.borrow();
    assert_eq!(state.timer_depth, 0);
    assert_eq!(state.active_child_cycles, 0);
    assert_eq!(
        state.counters.api_call_counts_by_category[ApiCategory::Surface as usize],
        1
    );
    assert_eq!(
        state.counters.api_call_counts_by_category[ApiCategory::Texture as usize],
        1
    );
}

#[test]
fn api_timer_disabled_does_not_retain_storage() {
    // Unit tests install no logger, so the runtime gate remains disabled.
    assert!(!perf_enabled());
    let storage = ApiPerfStorage::new();
    let weak = Rc::downgrade(&storage.state);
    let timer = ApiTimer::start_device(Some(&storage), DeviceSubCategory::Misc);
    assert_eq!(storage.state.borrow().timer_depth, 0);
    assert_eq!(weak.strong_count(), 1);
    drop(storage);
    assert!(weak.upgrade().is_none());
    drop(timer);
}

#[test]
fn api_perf_storage_without_timers_is_reclaimed() {
    let storage = ApiPerfStorage::new();
    let weak = Rc::downgrade(&storage.state);
    drop(storage);
    assert!(weak.upgrade().is_none());
}

/// In an untimed frame a timer only counts its call; a `Frame` call's timer reads the clock.
#[test]
fn untimed_frame_counts_every_call_and_times_only_the_frame_calls() {
    let storage = ApiPerfStorage::new();
    storage.state.borrow_mut().untimed_frames = 5;
    let counted = ApiTimer::frame_gated(Some(&storage), ApiCategory::Device, false);
    assert!(!counted.clock, "an untimed frame reads no clock");
    assert_eq!(storage.state.borrow().timer_depth, 0);
    drop(counted);
    let frame = ApiTimer::frame_gated(Some(&storage), ApiCategory::Device, true);
    assert!(frame.clock, "a Frame call reads the clock in every frame");
    assert_eq!(storage.state.borrow().timer_depth, 1);
    drop(frame);
    let state = storage.state.borrow();
    assert_eq!(
        state.counters.api_call_counts_by_category[ApiCategory::Device as usize],
        2,
        "both calls counted"
    );
    assert_eq!(state.timer_depth, 0);
}
