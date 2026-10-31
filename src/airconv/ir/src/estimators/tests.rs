//! The estimates against the C++'s.
//!
//! Read once from the baseline's sources by a scratch program that initialized C5's
//! `patch.hs.dxbc` and C6's `tessq.hs.dxbc` and `tessl.hs.dxbc` without a reflection and called
//! `estimate_payload_size`, `get_integer_factor` and `get_max_potential_workload_count` on each.

use super::*;

/// `patch.hs`: integer partitioning and a tri domain.
///
/// One output register, 3 output control points, 4 patch constant scalars.
const PATCH_HS: HullCounts = HullCounts {
    partition: 1,
    domain: 2,
    max_output_register: 1,
    output_control_point_count: 3,
    patch_constant_scalars: 4,
};

/// `tessq.hs`: fractional odd, quad domain, 2 output registers, 4 control points, 6 scalars.
const TESSQ_HS: HullCounts = HullCounts {
    partition: 3,
    domain: 3,
    max_output_register: 2,
    output_control_point_count: 4,
    patch_constant_scalars: 6,
};

#[test]
fn hull_estimators_match() {
    let payloads = [
        (32, 88_068, 121_348),
        (16, 44_036, 60_676),
        (8, 22_020, 30_340),
        (4, 11_012, 15_172),
        (2, 5_508, 7_588),
        (1, 2_756, 3_796),
    ];
    for (patches, patch_hs, tessq_hs) in payloads {
        assert_eq!(
            payload_size(&PATCH_HS, 64.0, patches),
            patch_hs,
            "patch.hs, {patches} patches"
        );
        assert_eq!(
            payload_size(&TESSQ_HS, 64.0, patches),
            tessq_hs,
            "tessq.hs, {patches} patches"
        );
    }
    // (factor, integer factor and workloads of patch.hs, of tessq.hs, integer factor of tessl.hs)
    let factors = [
        (64.0, (64, 48), (63, 65), 64),
        (63.0, (63, 49), (63, 65), 64),
        (16.5, (17, 13), (17, 17), 16),
        (3.0, (3, 4), (3, 5), 4),
        (1.0, (1, 1), (1, 1), 1),
        (0.5, (1, 1), (1, 1), 1),
    ];
    for (factor, (patch, patch_work), (tessq, tessq_work), tessl) in factors {
        assert_eq!(integer_factor(factor, 1), patch, "integer {factor}");
        assert_eq!(
            max_potential_workload_count(patch, 2),
            patch_work,
            "tri {factor}"
        );
        assert_eq!(integer_factor(factor, 3), tessq, "fractional odd {factor}");
        assert_eq!(
            max_potential_workload_count(tessq, 3),
            tessq_work,
            "quad {factor}"
        );
        assert_eq!(integer_factor(factor, 2), tessl, "pow2 {factor}");
        assert_eq!(
            max_potential_workload_count(tessl, 1),
            0,
            "isoline {factor}"
        );
    }
    // A factor of 0 wraps to 2^32 - 1 before the division; the quad's count saturates.
    let counts = [
        (0, 3_221_225_472, u32::MAX),
        (1, 1, 1),
        (2, 3, 4),
        (7, 7, 9),
        (64, 48, 64),
    ];
    for (factor, tri, quad) in counts {
        assert_eq!(max_potential_workload_count(factor, 2), tri, "tri {factor}");
        assert_eq!(
            max_potential_workload_count(factor, 3),
            quad,
            "quad {factor}"
        );
    }
    assert_eq!((next_pow2(3), next_pow2(4), next_pow2(2)), (4, 4, 2));
}

#[test]
fn unknown_domain_and_partitioning_answer_zero() {
    assert_eq!(max_potential_workload_count(64, 4), 0);
    assert_eq!(integer_factor(64.0, 5), 0);
    assert_eq!(integer_factor(64.0, 0), 0);
    assert_eq!(integer_factor(f32::NAN, 1), 0);
    // The C++'s `1 << 32` runs as ARM64 shifts it.
    assert_eq!((next_pow2(0), next_pow2(0x8000_0001)), (1, 1));
}
