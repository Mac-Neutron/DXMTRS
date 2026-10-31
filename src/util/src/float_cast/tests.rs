use super::{f32_to_u16, u64_to_f64};

#[test]
fn f32_to_u16_truncates() {
    assert_eq!(f32_to_u16(0.708 * 50000.0), 35400);
    assert_eq!(f32_to_u16(70000.0), 65535);
}

#[test]
fn u64_to_f64_rounds_to_nearest() {
    // 2^53 + 1 is halfway between two doubles: ties to even.
    assert_eq!(u64_to_f64((1 << 53) + 1), 9_007_199_254_740_992.0);
    assert_eq!(u64_to_f64(u64::MAX), 18_446_744_073_709_551_616.0);
}
