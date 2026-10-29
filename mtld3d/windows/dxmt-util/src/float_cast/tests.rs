use super::f32_to_u16;

#[test]
fn f32_to_u16_truncates() {
    assert_eq!(f32_to_u16(0.708 * 50000.0), 35400);
    assert_eq!(f32_to_u16(70000.0), 65535);
}
