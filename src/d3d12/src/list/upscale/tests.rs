use core::sync::atomic::{AtomicBool, Ordering};

use super::consume_reset;

#[test]
fn explicit_reset_short_circuits_fresh_flag() {
    let fresh = AtomicBool::new(true);
    assert_eq!(consume_reset(1, &fresh), 1);
    assert!(fresh.load(Ordering::SeqCst));
    assert_eq!(consume_reset(0, &fresh), 1);
    assert!(!fresh.load(Ordering::SeqCst));
    assert_eq!(consume_reset(0, &fresh), 0);
}
