//! Unit tests for the shader record kind and the content keys.
//!
//! `CachedKind` survives its wire byte and maps shader-model majors, and `ff_key_hash`
//! gives equal keys equal hashes.

use super::*;

#[test]
fn cached_kind_round_trips_via_byte() {
    for k in [
        CachedKind::FfVs,
        CachedKind::FfPs,
        CachedKind::Sm1Vs,
        CachedKind::Sm1Ps,
        CachedKind::Sm2Vs,
        CachedKind::Sm2Ps,
        CachedKind::Sm3Vs,
        CachedKind::Sm3Ps,
    ] {
        assert_eq!(CachedKind::from_byte(k as u8), Some(k));
    }
}

#[test]
fn air_kinds_round_trip_and_name() {
    let pairs = [
        (
            CachedKind::Sm1Vs,
            CachedKind::AirSm1Vs,
            10,
            "mtld3d_vs_air_sm1_000000ab",
        ),
        (
            CachedKind::Sm1Ps,
            CachedKind::AirSm1Ps,
            11,
            "mtld3d_ps_air_sm1_000000ab",
        ),
        (
            CachedKind::Sm2Vs,
            CachedKind::AirSm2Vs,
            12,
            "mtld3d_vs_air_sm2_000000ab",
        ),
        (
            CachedKind::Sm2Ps,
            CachedKind::AirSm2Ps,
            13,
            "mtld3d_ps_air_sm2_000000ab",
        ),
        (
            CachedKind::Sm3Vs,
            CachedKind::AirSm3Vs,
            14,
            "mtld3d_vs_air_sm3_000000ab",
        ),
        (
            CachedKind::Sm3Ps,
            CachedKind::AirSm3Ps,
            15,
            "mtld3d_ps_air_sm3_000000ab",
        ),
    ];
    for (msl, air, byte, name) in pairs {
        assert_eq!(air as u8, byte);
        assert_eq!(CachedKind::from_byte(byte), Some(air));
        assert_eq!(msl.air(), Some(air));
        assert_eq!(air.air(), None);
        assert!(air.is_air() && !msl.is_air());
        assert_eq!(air.msl_twin(), msl);
        assert_eq!(msl.msl_twin(), msl);
        assert!(air.is_programmable());
        assert_eq!(
            (air.is_vertex(), air.is_pixel()),
            (msl.is_vertex(), msl.is_pixel())
        );
        assert_eq!(air.compile_bucket(), msl.compile_bucket());
        assert_eq!(air.entry_name(0xAB), name);
    }
    assert_eq!(CachedKind::from_byte(16), None);
}

#[test]
fn air_ff_kinds_round_trip_bytes() {
    assert_eq!(CachedKind::AirFfVs as u8, 8);
    assert_eq!(CachedKind::AirFfPs as u8, 9);
    assert_eq!(CachedKind::from_byte(8), Some(CachedKind::AirFfVs));
    assert_eq!(CachedKind::from_byte(9), Some(CachedKind::AirFfPs));
    assert_eq!(
        CachedKind::AirFfVs.entry_name(0xAB),
        "mtld3d_vs_air_ff_000000ab"
    );
    assert_eq!(
        CachedKind::AirFfPs.entry_name(0xAB),
        "mtld3d_ps_air_ff_000000ab"
    );
}

#[test]
fn air_ff_kinds_twin_ff_kinds() {
    assert_eq!(CachedKind::FfVs.air(), Some(CachedKind::AirFfVs));
    assert_eq!(CachedKind::FfPs.air(), Some(CachedKind::AirFfPs));
    assert_eq!(CachedKind::AirFfVs.air(), None);
    assert_eq!(CachedKind::AirFfPs.air(), None);
    assert_eq!(CachedKind::AirFfVs.msl_twin(), CachedKind::FfVs);
    assert_eq!(CachedKind::AirFfPs.msl_twin(), CachedKind::FfPs);
    assert_eq!(CachedKind::FfVs.msl_twin(), CachedKind::FfVs);
    assert_eq!(
        CachedKind::AirFfVs.compile_bucket(),
        CachedKind::FfVs.compile_bucket()
    );
    assert_eq!(
        CachedKind::AirFfPs.compile_bucket(),
        CachedKind::FfPs.compile_bucket()
    );
}

#[test]
fn air_ff_kinds_are_air_and_not_programmable() {
    for k in [CachedKind::AirFfVs, CachedKind::AirFfPs] {
        assert!(k.is_air());
        assert!(!k.is_programmable());
    }
    assert!(!CachedKind::FfVs.is_air());
    assert!(!CachedKind::FfPs.is_air());
}

#[test]
fn air_ff_kinds_are_vertex_or_pixel() {
    assert!(CachedKind::AirFfVs.is_vertex() && !CachedKind::AirFfVs.is_pixel());
    assert!(CachedKind::AirFfPs.is_pixel() && !CachedKind::AirFfPs.is_vertex());
}

#[test]
fn from_programmable_maps_supported_majors() {
    assert_eq!(
        CachedKind::from_programmable(1, false),
        Some(CachedKind::Sm1Vs)
    );
    assert_eq!(
        CachedKind::from_programmable(2, true),
        Some(CachedKind::Sm2Ps)
    );
    assert_eq!(
        CachedKind::from_programmable(3, false),
        Some(CachedKind::Sm3Vs)
    );
    assert_eq!(CachedKind::from_programmable(0, false), None);
    assert_eq!(CachedKind::from_programmable(4, true), None);
}

#[test]
fn ff_key_hash_is_stable() {
    let a = (1u32, 2u32, 3u32);
    let b = (1u32, 2u32, 3u32);
    assert_eq!(ff_key_hash(&a), ff_key_hash(&b));
}
