use super::version_text;

#[test]
fn version_text_is_the_cpps() {
    // The feature levels `dxmt_info.cpp` names, then two it does not.
    let levels = [
        (0x9100, "9_1"),
        (0x9200, "9_2"),
        (0x9300, "9_3"),
        (0xa000, "10_0"),
        (0xa100, "10_1"),
        (0xb000, "11_0"),
        (0xb100, "11_1"),
        (0xc000, "12_0"),
        (0xc100, "12_1"),
        (0xb200, "UNKNOWN"),
        (0, "UNKNOWN"),
    ];
    for (level, text) in levels {
        assert_eq!(
            version_text(11, level, "58b99d7"),
            format!("DXMT D3D11 FL_{text} 58b99d7")
        );
    }
    assert_eq!(version_text(12, 0xc100, "v0.1"), "DXMT D3D12 FL_12_1 v0.1");
}
