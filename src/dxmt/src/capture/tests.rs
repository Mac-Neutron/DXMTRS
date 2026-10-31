use super::{CaptureState, NextAction, enabled_by};
use crate::log::take_captured;

#[test]
fn capture_states_as_cpp() {
    // Not enabled: nothing is ever scheduled or started.
    let mut frozen = CaptureState::new(false);
    frozen.schedule(3);
    assert_eq!(frozen.next_action(3), NextAction::Nothing);
    assert!(!frozen.should_capture_next(true));

    // Idle does nothing until a frame is scheduled; a second schedule is ignored.
    let mut c = CaptureState::new(true);
    assert_eq!(c.next_action(0), NextAction::Nothing);
    c.schedule(5);
    c.schedule(9);
    for frame in 0..5 {
        assert_eq!(c.next_action(frame), NextAction::Nothing, "frame {frame}");
    }
    assert_eq!(c.next_action(5), NextAction::StartCapture);
    // The capture runs through its frame and stops on the next one.
    c.schedule(7);
    assert_eq!(c.next_action(5), NextAction::Nothing);
    assert_eq!(c.next_action(6), NextAction::StopCapture);
    assert_eq!(c.next_action(7), NextAction::Nothing);

    // Idle again: a frame past the scheduled one does not start it.
    c.schedule(0);
    assert_eq!(c.next_action(0), NextAction::StartCapture);
    assert_eq!(c.next_action(1), NextAction::StopCapture);
    c.schedule(4);
    assert_eq!(c.next_action(6), NextAction::Nothing);
    assert_eq!(c.next_action(4), NextAction::StartCapture);
}

#[test]
fn enabling_warns_once() {
    let _ = take_captured();
    let _off = CaptureState::new(false);
    assert_eq!(take_captured(), Vec::<(u32, String)>::new());
    let _on = CaptureState::new(true);
    assert_eq!(take_captured(), [(3, "DXMT capture enabled".to_owned())]);
}

#[test]
fn f10_fires_once_per_press_and_only_when_idle() {
    let mut c = CaptureState::new(true);
    let presses = [false, true, true, true, false, true, true, true, false];
    let fired: Vec<bool> = presses.map(|down| c.should_capture_next(down)).to_vec();
    assert_eq!(
        fired,
        [false, true, false, false, false, true, false, false, false]
    );

    // While a capture is scheduled the key is not looked at, so its state is not remembered.
    c.schedule(1);
    assert!(!c.should_capture_next(true));
    assert_eq!(c.next_action(1), NextAction::StartCapture);
    assert_eq!(c.next_action(2), NextAction::StopCapture);
    // Still held when the capture ends: the press counts, as it was never seen.
    assert!(c.should_capture_next(true));
}

#[test]
fn enabled_as_cpp() {
    assert!(enabled_by(Some("1"), "game", Some("game")));
    assert!(!enabled_by(Some("0"), "game", Some("game")));
    assert!(!enabled_by(Some("11"), "game", Some("game")));
    assert!(!enabled_by(None, "game", Some("game")));
    assert!(!enabled_by(Some("1"), "game", Some("other")));
    assert!(!enabled_by(Some("1"), "game", None));
    // The native dylibs have no program name, which an unset target also reads as.
    assert!(enabled_by(Some("1"), "", None));
    assert!(enabled_by(Some("1"), "", Some("")));
}
