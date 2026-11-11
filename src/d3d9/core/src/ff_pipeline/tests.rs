use mtld3d_types::{
    D3DFVF_DIFFUSE, D3DFVF_SPECULAR, D3DFVF_XYZRHW, RENDER_STATE_COUNT, render_state_defaults,
};

use super::{FfPipelineKey, ff_source, up_byte_count};
use crate::{
    convert::{ff_vs_layout_from_elements, fvf_to_elements},
    ff_state::FfState,
};

/// The key of a default-state `XYZRHW | DIFFUSE` draw, 20 bytes a vertex, every colour written.
fn base() -> FfPipelineKey {
    let rs: [u32; RENDER_STATE_COUNT] = render_state_defaults();
    let ff = FfState::new();
    let fvf = D3DFVF_XYZRHW | D3DFVF_DIFFUSE;
    let layout = ff_vs_layout_from_elements(&fvf_to_elements(fvf).0);
    FfPipelineKey {
        vs: ff.build_vs_key(&rs, layout, 0, [0; 8]),
        ps: ff.build_ps_key(&rs, 0),
        variant: ff.variant_key(&rs, true, false),
        fvf,
        stride: 20,
        write_mask: 0xf,
    }
}

/// One field of a key changed.
type Change = fn(&mut FfPipelineKey);

/// Each field of the key reaches what its pipeline is built from.
///
/// Two keys differing in one field give different sources, so a key that left a pipeline input
/// out would bind one pipeline for two; equal keys give equal sources.
#[test]
fn ff_pipeline_key_tracks_every_input() {
    let base_source = ff_source(&base());
    assert_eq!(ff_source(&base()), base_source);
    let changes: [(&str, Change); 7] = [
        ("vs (tokens)", |k| k.vs.tex_coord_count = 1),
        ("vs (clip planes)", |k| k.vs.clip_plane_count = 1),
        ("ps", |k| k.ps.specular_add = true),
        ("variant", |k| k.variant.alpha_func = 5),
        ("fvf", |k| k.fvf |= D3DFVF_SPECULAR),
        ("stride", |k| k.stride = 24),
        ("write_mask", |k| k.write_mask = 0x7),
    ];
    for (field, change) in changes {
        let mut key = base();
        change(&mut key);
        assert_ne!(key, base(), "{field}");
        assert_ne!(ff_source(&key), base_source, "{field}");
    }
}

/// A `stride * count` past `u32` is refused, on every host: no `DrawPrimitiveUP` passes 4 GiB.
#[test]
fn up_byte_count_refuses_an_overflow() {
    assert_eq!(up_byte_count(20, 3), Some(60));
    assert_eq!(up_byte_count(u32::MAX, 1), usize::try_from(u32::MAX).ok());
    assert_eq!(up_byte_count(u32::MAX, 2), None);
    assert_eq!(up_byte_count(0x1_0000, 0x1_0000), None);
}
