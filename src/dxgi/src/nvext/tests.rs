use core::cell::Cell;

use super::*;

#[cfg(not(windows))]
#[test]
fn native_reads_the_variable_once() {
    let reads = Cell::new(0);
    let env = |name: &str| {
        assert_eq!(name, "DXMT_ENABLE_NVEXT");
        reads.set(reads.get() + 1);
        Some("1".to_owned())
    };
    let (once, on) = (Once::new(), AtomicBool::new(false));
    assert!(latch(&once, &on, || init(&env)));
    assert!(latch(&once, &on, || init(&env)));
    assert_eq!(reads.get(), 1);
    for value in [None, Some("0"), Some("true"), Some("1 "), Some("")] {
        assert!(!init(&|_: &str| value.map(str::to_owned)));
    }
}
