use super::*;

#[test]
fn build_alpha_ref_bytes_skips_when_disabled_or_always() {
    let mut variant = VariantKey::default();
    assert_eq!(build_alpha_ref_bytes(variant, 0.5), ([0; 4], 0));
    variant.alpha_func = 8;
    assert_eq!(build_alpha_ref_bytes(variant, 0.5), ([0; 4], 0));
    variant.alpha_func = 5;
    assert_eq!(
        build_alpha_ref_bytes(variant, 0.5),
        (0.5_f32.to_le_bytes(), 4)
    );
}
