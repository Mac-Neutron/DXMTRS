use super::*;

#[test]
fn anisotropy_reads_like_atoi() {
    let cases = [
        (Some(c"8"), 8),
        (Some(c"8x"), 8),
        (Some(c" 4"), 4),
        (Some(c"0"), 0),
        (Some(c"17"), 0),
        (Some(c"-3"), 0),
        (Some(c"abc"), 0),
        // atoi is (int)strtol: 2^32 + 8 wraps to 8.
        (Some(c"4294967304"), 8),
        (None, 0),
    ];
    for (v, n) in cases {
        assert_eq!(parse_max_anisotropy(v), n, "{v:?}");
    }
}

#[test]
fn presenter_only_for_1() {
    for v in [None, Some(c""), Some(c"0"), Some(c"11"), Some(c"1 ")] {
        assert!(!presenter_wanted(v), "{v:?}");
    }
    assert!(presenter_wanted(Some(c"1")));
}
