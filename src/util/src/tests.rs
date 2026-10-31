use super::luid_from_registry_id;

#[test]
fn luid_swaps_bytes() {
    assert_eq!(
        luid_from_registry_id(0x0102_0304_0506_0708),
        0x0807_0605_0403_0201_u64.to_ne_bytes()
    );
}
