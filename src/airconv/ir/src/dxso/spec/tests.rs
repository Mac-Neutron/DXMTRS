//! The specialization types' layout and the wire codec `StageSpec` carries across the VM boundary.

use core::mem::{offset_of, size_of};

use super::{
    SpecError, StageSpec, VARIANT_KEY_WIRE, VS_SPEC_WIRE, VariantFlags, VariantKey, VsSamplerKinds,
    VsSpec,
};

/// `VariantKey`'s layout as `emit.rs` had it before the move: the C ABI of `#[repr(C)]` fields.
#[test]
fn variant_key_layout_is_the_emitters() {
    assert_eq!(size_of::<VariantKey>(), 22);
    assert_eq!(VARIANT_KEY_WIRE, 22);
    assert_eq!(offset_of!(VariantKey, alpha_func), 0);
    assert_eq!(offset_of!(VariantKey, fog_mode), 1);
    assert_eq!(offset_of!(VariantKey, fog_table_mode), 2);
    assert_eq!(offset_of!(VariantKey, linked_input_mask), 3);
    assert_eq!(offset_of!(VariantKey, depth_sampler_mask), 4);
    assert_eq!(offset_of!(VariantKey, depth_fetch_mask), 6);
    assert_eq!(offset_of!(VariantKey, fetch4_mask), 8);
    assert_eq!(offset_of!(VariantKey, fetch4_alpha_mask), 10);
    assert_eq!(offset_of!(VariantKey, raw_depth_red_mask), 12);
    assert_eq!(offset_of!(VariantKey, volume_sampler_mask), 14);
    assert_eq!(offset_of!(VariantKey, cube_sampler_mask), 16);
    assert_eq!(offset_of!(VariantKey, tt_projected_mask), 18);
    assert_eq!(offset_of!(VariantKey, color_out_mask), 19);
    assert_eq!(offset_of!(VariantKey, sample_mask), 20);
    assert_eq!(offset_of!(VariantKey, flags), 21);
    assert_eq!(size_of::<VsSpec>(), 6);
    assert_eq!(VS_SPEC_WIRE, 6);
    assert_eq!(offset_of!(VsSpec, provided_input_mask), 0);
    assert_eq!(offset_of!(VsSpec, clip_plane_count), 2);
    assert_eq!(offset_of!(VsSpec, sampler_kinds), 3);
}

fn ps_key() -> VariantKey {
    VariantKey {
        alpha_func: 5,
        fog_mode: 4,
        fog_table_mode: 3,
        linked_input_mask: 0xA5,
        depth_sampler_mask: 0x1234,
        depth_fetch_mask: 0x0F0F,
        fetch4_mask: 0x00FF,
        fetch4_alpha_mask: 0x8001,
        raw_depth_red_mask: 0x4002,
        volume_sampler_mask: 0x2004,
        cube_sampler_mask: 0x1008,
        tt_projected_mask: 0x81,
        color_out_mask: 0b1011,
        sample_mask: 0x0F,
        flags: VariantFlags::all(),
    }
}

fn vs_spec() -> VsSpec {
    VsSpec {
        provided_input_mask: 0xBEEF,
        clip_plane_count: 6,
        sampler_kinds: VsSamplerKinds {
            volume_mask: 0b0101,
            cube_mask: 0b1010,
            lod_table: true,
        },
    }
}

#[test]
fn wire_round_trips() {
    for spec in [
        StageSpec::Vs(vs_spec()),
        StageSpec::Ps(ps_key()),
        StageSpec::Vs(VsSpec::default()),
        StageSpec::Ps(VariantKey::default()),
    ] {
        let (stage, vs, ps) = spec.to_wire();
        assert_eq!(StageSpec::from_wire(stage, &vs, &ps), Ok(spec));
    }
    // The unused half is zero, and each field sits at its offset.
    let (stage, vs, ps) = StageSpec::Vs(vs_spec()).to_wire();
    assert_eq!((stage, ps), (0, [0; VARIANT_KEY_WIRE]));
    assert_eq!(vs, [0xEF, 0xBE, 6, 0b0101, 0b1010, 1]);
    let (stage, vs, ps) = StageSpec::Ps(ps_key()).to_wire();
    assert_eq!((stage, vs), (1, [0; VS_SPEC_WIRE]));
    assert_eq!(ps[3], 0xA5);
    assert_eq!(ps[4..6], [0x34, 0x12]);
    assert_eq!(ps[21], 0xFF);
}

#[test]
fn lod_table_2_is_refused() {
    let (stage, mut vs, ps) = StageSpec::Vs(vs_spec()).to_wire();
    vs[offset_of!(VsSpec, sampler_kinds) + offset_of!(VsSamplerKinds, lod_table)] = 2;
    assert_eq!(
        StageSpec::from_wire(stage, &vs, &ps),
        Err(SpecError::LodTable(2))
    );
}

/// A value `emit.rs` has no case for is refused.
///
/// Every `VariantFlags` bit has a meaning, so no flag byte is among them.
#[test]
fn unknown_variant_values_are_refused() {
    let (_, vs, ps) = StageSpec::Ps(ps_key()).to_wire();
    assert_eq!(StageSpec::from_wire(2, &vs, &ps), Err(SpecError::Stage(2)));
    for (at, bad, err) in [
        (0, 9, SpecError::AlphaFunc(9)),
        (1, 5, SpecError::FogMode(5)),
        (2, 4, SpecError::FogTableMode(4)),
    ] {
        let mut ps = ps;
        ps[at] = bad;
        assert_eq!(StageSpec::from_wire(1, &vs, &ps), Err(err));
    }
    assert_eq!(VariantFlags::all().bits(), 0xFF);
}
