use super::{Compression, Flags, Settings};

fn with(vars: &[(&str, &str)]) -> Settings {
    Settings::from_vars(|name| {
        vars.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_owned())
    })
}

#[test]
fn unset_variables_give_the_defaults() {
    let s = with(&[]);
    assert_eq!(s.d3d11_compression, Compression::Default);
    assert_eq!(s.d3d12_compression, Compression::Default);
    assert_eq!(
        s.flags,
        Flags::PRIVATE_HEAP
            | Flags::MERGE
            | Flags::DEFER_RESOLVES
            | Flags::SM6
            | Flags::INDIRECT_WITHOUT_ICB
    );
    assert_eq!(s.capture_frame, None);
    assert_eq!((s.mtl_capture_enabled, s.capture_executable), (None, None));
}

#[test]
fn on_by_default_flags_turn_off_only_for_zero() {
    let s = with(&[
        ("DXMT_D3D12_PRIVATE", "0"),
        ("DXMT_D3D12_MERGE", "0"),
        ("DXMT_D3D12_SM6", "0"),
        ("DXMT_D3D12_DEFER_RESOLVES", "no"),
        ("DXMT_D3D12_INDIRECT", "icb"),
    ]);
    assert_eq!(s.flags, Flags::DEFER_RESOLVES);
}

#[test]
fn off_by_default_flags_turn_on_only_for_one() {
    assert!(with(&[("DXMT_STATS", "1")]).enabled(Flags::STATS));
    assert!(!with(&[("DXMT_STATS", "true")]).enabled(Flags::STATS));
    assert!(with(&[("DXMT_D3D12_MEMORYLESS", "1")]).enabled(Flags::MEMORYLESS));
    assert!(!with(&[("DXMT_D3D12_ADVERTISE_NUMA", "0")]).enabled(Flags::ADVERTISE_NUMA));
}

#[test]
fn compression_modes() {
    assert_eq!(
        with(&[("DXMT_D3D11_COMPRESSION", "0")]).d3d11_compression,
        Compression::Off
    );
    assert_eq!(
        with(&[("DXMT_D3D12_COMPRESSION", "rt")]).d3d12_compression,
        Compression::RenderTargets
    );
    assert_eq!(
        with(&[("DXMT_D3D12_COMPRESSION", "1")]).d3d12_compression,
        Compression::Default
    );
    assert_eq!(
        with(&[("DXMT_D3D12_COMPRESSION", "")]).d3d12_compression,
        Compression::Default
    );
}

/// The strict parse core always had: the trimmed value is a number above 0, or no capture.
#[test]
fn capture_frame_parse_is_strict() {
    assert_eq!(
        with(&[("DXMT_CAPTURE_FRAME", " 12 ")]).capture_frame,
        Some(12)
    );
    for value in ["0", "-1", "12abc", "", "abc"] {
        let s = with(&[("DXMT_CAPTURE_FRAME", value)]);
        assert_eq!(s.capture_frame, None, "{value:?}");
        assert!(s.enabled(Flags::CAPTURE_FRAME_SET));
    }
}
