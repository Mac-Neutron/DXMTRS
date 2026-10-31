use core::mem::{align_of, offset_of, size_of};

use super::*;

#[test]
fn layout_is_12296() {
    assert_eq!(size_of::<GammaRamp>(), 12296);
    assert_eq!(align_of::<GammaRamp>(), 8);
    assert_eq!(offset_of!(GammaRamp, red), 0);
    assert_eq!(offset_of!(GammaRamp, green), 4096);
    assert_eq!(offset_of!(GammaRamp, blue), 8192);
    assert_eq!(offset_of!(GammaRamp, version), 12288);
    assert_eq!(GAMMA_CP_COUNT, 1024);
}
