use super::*;

/// Vectors printed by a C program over `winemetal.h`.
///
/// Each is a format, a requested swizzle, `ORIGINAL_FORMAT` and `DECODE_FORMAT_COMPONENT_SWIZZLE`
/// per component, for `WMTPixelFormatBGRX8Unorm`, `WMTPixelFormatR32X8X32` and
/// `WMTPixelFormatBGRA4Unorm`.
const VECTORS: [(u32, [u8; 4], usize, [u8; 4]); 9] = [
    (8272, [2, 3, 4, 5], 80, [2, 3, 4, 1]),
    (8272, [5, 0, 1, 2], 80, [5, 0, 1, 4]),
    (8272, [4, 4, 3, 1], 80, [4, 4, 3, 3]),
    (860_420, [2, 3, 4, 5], 260, [2, 0, 0, 1]),
    (860_420, [5, 0, 1, 2], 260, [5, 3, 3, 4]),
    (860_420, [4, 4, 3, 1], 260, [4, 1, 5, 3]),
    (2_404_394, [2, 3, 4, 5], 42, [3, 4, 5, 2]),
    (2_404_394, [5, 0, 1, 2], 42, [0, 1, 2, 5]),
    (2_404_394, [4, 4, 3, 1], 42, [5, 5, 4, 4]),
];

#[test]
fn pixel_format_strips_swizzle() {
    for (format, [r, g, b, a], metal, decoded) in VECTORS {
        assert_eq!(pixel_format(format), MTLPixelFormat(metal), "{format}");
        let s = swizzle(format, WMTTextureSwizzleChannels { r, g, b, a });
        assert_eq!(
            [s.red.0, s.green.0, s.blue.0, s.alpha.0],
            decoded,
            "{format} {r} {g} {b} {a}"
        );
    }
}
