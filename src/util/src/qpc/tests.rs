use super::{now, qpc};

#[test]
fn native_is_uptime_raw_ns() {
    let (t0, freq0) = now();
    let (u0, ufreq0) = qpc();
    let (t1, freq1) = now();
    assert_eq!(freq0, 1_000_000_000);
    assert_eq!(ufreq0, 1_000_000_000);
    assert_eq!(freq1, 1_000_000_000);
    assert!(t0 > 0);
    assert!(u0 >= t0.cast_unsigned());
    assert!(t1.cast_unsigned() >= u0);
}
