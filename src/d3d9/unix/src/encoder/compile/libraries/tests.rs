use mtld3d_core::{
    draw_data::ShaderSourceFlags,
    dxso::{FfStage, FfVsFlags, VariantFlags},
};

use super::{super::tests::programs, *};

fn handles(value: u64) -> StageLibHandles {
    // SAFETY: opaque test values; the index and memo only copy and compare handles.
    let library = unsafe { MetalHandle::new(value) };
    // SAFETY: as above.
    let func = unsafe { MetalHandle::new(value + 1) };
    StageLibHandles { library, func }
}

const fn raw_pair((vs, ps): (StageLibHandles, StageLibHandles)) -> (u64, u64) {
    (vs.library.raw(), ps.library.raw())
}

const fn programmable_vs(id: u64) -> ProgrammableVsSource {
    ProgrammableVsSource {
        vs_id: ProgramId::from_shader_reply(id),
        max_const_used: 0,
        provided_input_mask: 1,
        flags: ShaderSourceFlags::empty(),
        clip_plane_count: 0,
        sampler_kinds: VsSamplerKinds {
            volume_mask: 0,
            cube_mask: 0,
            lod_table: false,
        },
        reserved: [0; 7],
    }
}

const fn programmable_ps(id: u64) -> ProgrammablePsSource {
    ProgrammablePsSource {
        ps_id: ProgramId::from_shader_reply(id),
        max_const_used: 0,
        flags: ShaderSourceFlags::empty(),
        color_out_mask: 1,
        reserved: [0; 4],
    }
}

fn fixed_vs(fog_mode: u8) -> FixedVsSource {
    FixedVsSource {
        key: FfVsKey {
            reserved: 0,
            flags: FfVsFlags::HAS_NORMAL,
            input_tex_coord_count: 1,
            tex_coord_count: 1,
            light_active_mask: 0,
            light_directional_mask: 0,
            light_spot_mask: 0,
            diffuse_source: 0,
            ambient_source: 0,
            specular_source: 0,
            emissive_source: 0,
            fog_mode,
            tci: [0; 8],
            tex_coord_dims: [2; 8],
            tt_flags: [0; 8],
            vertex_blend_count: 0,
            declared_weights_count: 0,
            clip_plane_count: 0,
            passthrough: [0; 8],
        },
        max_row_count: 8,
        reserved: [0; 6],
    }
}

fn fixed_ps(specular_add: bool) -> FixedPsSource {
    FixedPsSource {
        key: FfPsKey {
            stages: [FfStage::default(); 8],
            specular_add,
            tt_projected_mask: 0,
        },
        sampled_stage_mask: 1,
        constant_rows: 0,
        bump_env: false,
        reserved: [0; 2],
    }
}

fn variant(alpha_func: u8) -> VariantKey {
    VariantKey {
        alpha_func,
        ..VariantKey::default()
    }
}

/// A programmable VS and PS whose libraries are both built.
fn built_pair(
    libraries: &mut StageLibraries,
    vs: &ProgrammableVsSource,
    ps: &ProgrammablePsSource,
) {
    libraries.record_vs(
        VsSourceView::Programmable(vs),
        ShaderBackend::Msl,
        Some(handles(10)),
    );
    libraries.record_ps(
        PsSourceView::Programmable(ps),
        variant(0),
        ShaderBackend::Msl,
        Some(handles(20)),
    );
}

#[test]
fn repeated_records_answer_from_the_memo() {
    let mut libraries = StageLibraries::default();
    let vs = programmable_vs(1);
    let ps = programmable_ps(2);
    built_pair(&mut libraries, &vs, &ps);
    let (vs_view, ps_view) = (
        VsSourceView::Programmable(&vs),
        PsSourceView::Programmable(&ps),
    );
    assert_eq!(
        libraries.memo.vs_record, 0,
        "recording leaves the memo empty"
    );
    let first = libraries
        .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
        .map(raw_pair);
    assert_eq!(first, Some((10, 20)));
    assert_eq!(libraries.memo.vs_record, vs_record(vs_view));
    assert_eq!(libraries.memo.ps_record, ps_record(ps_view));
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        first,
        "a draw naming the same records gets the same libraries"
    );
}

#[test]
fn a_rebound_shader_probes_the_index_for_its_own_record() {
    let mut libraries = StageLibraries::default();
    let first_vs = programmable_vs(1);
    let second_vs = programmable_vs(3);
    let ps = programmable_ps(2);
    built_pair(&mut libraries, &first_vs, &ps);
    libraries.record_vs(
        VsSourceView::Programmable(&second_vs),
        ShaderBackend::Msl,
        Some(handles(30)),
    );
    let ps_view = PsSourceView::Programmable(&ps);
    for (vs, expected) in [(&first_vs, 10), (&second_vs, 30), (&first_vs, 10)] {
        assert_eq!(
            libraries
                .lookup_ready(
                    VsSourceView::Programmable(vs),
                    VsSourceView::Programmable(vs),
                    ps_view,
                    variant(0),
                    ShaderBackend::Msl
                )
                .map(raw_pair),
            Some((expected, 20)),
            "each bound shader answers with its own library"
        );
    }
    // A snapshot that re-sends an unchanged shader puts an equal record at a
    // new address: the index answers for it, and the memo follows it.
    let resent = programmable_vs(1);
    let view = VsSourceView::Programmable(&resent);
    assert_eq!(
        libraries
            .lookup_ready(view, view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        Some((10, 20))
    );
    assert_eq!(libraries.memo.vs_record, vs_record(view));
}

#[test]
fn a_changed_variant_probes_the_pixel_index() {
    let mut libraries = StageLibraries::default();
    let vs = programmable_vs(1);
    let ps = programmable_ps(2);
    built_pair(&mut libraries, &vs, &ps);
    let ps_view = PsSourceView::Programmable(&ps);
    libraries.record_ps(ps_view, variant(4), ShaderBackend::Msl, Some(handles(40)));
    let vs_view = VsSourceView::Programmable(&vs);
    for (key, expected) in [(variant(0), 20), (variant(4), 40), (variant(0), 20)] {
        assert_eq!(
            libraries
                .lookup_ready(vs_view, vs_view, ps_view, key, ShaderBackend::Msl)
                .map(raw_pair),
            Some((10, expected)),
            "the pixel library follows the draw's variant on an unchanged record"
        );
    }
    let flagged = VariantKey {
        flags: VariantFlags::SRGB_WRITE,
        ..variant(0)
    };
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, flagged, ShaderBackend::Msl)
            .map(|_| ()),
        None,
        "a variant that was never built is no memo hit"
    );
}

#[test]
fn a_rebuild_or_an_adopted_library_replaces_the_memoised_handles() {
    let mut libraries = StageLibraries::default();
    let vs = programmable_vs(1);
    let ps = programmable_ps(2);
    built_pair(&mut libraries, &vs, &ps);
    let (vs_view, ps_view) = (
        VsSourceView::Programmable(&vs),
        PsSourceView::Programmable(&ps),
    );
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        Some((10, 20))
    );
    // A finished worker build lands under the owned key.
    libraries.record_programmable_vs(
        programmable_vs_key(&vs, ShaderBackend::Msl),
        Some(handles(50)),
    );
    assert_eq!(
        libraries.memo.vs_record, 0,
        "an index write forgets the memo"
    );
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        Some((50, 20))
    );
    // A library bridged from the prewarmed cache lands under the draw's source.
    libraries.record_ps(ps_view, variant(0), ShaderBackend::Msl, Some(handles(60)));
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        Some((50, 60))
    );
    // A failed rebuild is an answer too: the draw goes to the slow path.
    libraries.record_programmable_ps(ps.ps_id, variant(0), ShaderBackend::Msl, None);
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(|_| ()),
        None
    );
    assert_eq!(libraries.memo.ps_record, 0, "a failure is never memoised");
}

#[test]
fn a_recycled_record_address_misses_after_the_packet_boundary() {
    let mut libraries = StageLibraries::default();
    let ps = programmable_ps(2);
    let mut vs = programmable_vs(1);
    let recycled = programmable_vs(7);
    built_pair(&mut libraries, &vs, &ps);
    libraries.record_vs(
        VsSourceView::Programmable(&recycled),
        ShaderBackend::Msl,
        Some(handles(70)),
    );
    let ps_view = PsSourceView::Programmable(&ps);
    let address = vs_record(VsSourceView::Programmable(&vs));
    assert_eq!(
        libraries
            .lookup_ready(
                VsSourceView::Programmable(&vs),
                VsSourceView::Programmable(&vs),
                ps_view,
                variant(0),
                ShaderBackend::Msl
            )
            .map(raw_pair),
        Some((10, 20))
    );
    // The next packet writes another shader's record where this one was.
    libraries.begin_packet();
    vs.vs_id = recycled.vs_id;
    let view = VsSourceView::Programmable(&vs);
    assert_eq!(
        vs_record(view),
        address,
        "the new record reuses the address"
    );
    assert_eq!(
        libraries
            .lookup_ready(view, view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        Some((70, 20)),
        "the recycled address answers for the record now stored there"
    );
}

#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "the VS library memo answers what the index holds")]
fn a_recycled_record_without_the_packet_boundary_trips_the_memo_check() {
    let mut libraries = StageLibraries::default();
    let ps = programmable_ps(2);
    let mut vs = programmable_vs(1);
    built_pair(&mut libraries, &vs, &ps);
    let ps_view = PsSourceView::Programmable(&ps);
    let _ = libraries.lookup_ready(
        VsSourceView::Programmable(&vs),
        VsSourceView::Programmable(&vs),
        ps_view,
        variant(0),
        ShaderBackend::Msl,
    );
    vs.vs_id = ProgramId::from_shader_reply(9);
    let _ = libraries.lookup_ready(
        VsSourceView::Programmable(&vs),
        VsSourceView::Programmable(&vs),
        ps_view,
        variant(0),
        ShaderBackend::Msl,
    );
}

#[test]
fn a_device_reset_forgets_the_memo_and_the_failures() {
    let mut libraries = StageLibraries::default();
    let vs = programmable_vs(1);
    let ps = programmable_ps(2);
    let failed = programmable_vs(8);
    built_pair(&mut libraries, &vs, &ps);
    libraries.record_vs(
        VsSourceView::Programmable(&failed),
        ShaderBackend::Msl,
        None,
    );
    let (vs_view, ps_view) = (
        VsSourceView::Programmable(&vs),
        PsSourceView::Programmable(&ps),
    );
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        Some((10, 20))
    );
    libraries.forget_failures();
    assert_eq!((libraries.memo.vs_record, libraries.memo.ps_record), (0, 0));
    assert!(
        libraries
            .lookup_vs(VsSourceView::Programmable(&failed), ShaderBackend::Msl)
            .is_none(),
        "the failed key is built once more"
    );
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(raw_pair),
        Some((10, 20)),
        "built libraries survive the reset"
    );
    libraries.clear();
    assert_eq!((libraries.memo.vs_record, libraries.memo.ps_record), (0, 0));
    assert_eq!(
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), ShaderBackend::Msl)
            .map(|_| ()),
        None
    );
}

#[test]
fn fixed_function_records_are_tagged_and_keyed_by_content() {
    let mut libraries = StageLibraries::default();
    let vs = fixed_vs(0);
    let fogged = fixed_vs(3);
    let ps = fixed_ps(false);
    libraries.record_ff_vs(vs.key.clone(), Some(handles(80)));
    libraries.record_vs(
        VsSourceView::FixedFunction(&fogged),
        ShaderBackend::Msl,
        Some(handles(82)),
    );
    libraries.record_ff_ps(ps.key.clone(), variant(0), Some(handles(90)));
    let ps_view = PsSourceView::FixedFunction(&ps);
    let view = VsSourceView::FixedFunction(&vs);
    assert_eq!(
        vs_record(view) & 1,
        1,
        "fixed-function records carry the tag bit"
    );
    assert_eq!(ps_record(ps_view) & 1, 1);
    for (source, expected) in [(&vs, 80), (&fogged, 82), (&vs, 80)] {
        assert_eq!(
            libraries
                .lookup_ready(
                    VsSourceView::FixedFunction(source),
                    VsSourceView::FixedFunction(source),
                    ps_view,
                    variant(0),
                    ShaderBackend::Msl
                )
                .map(raw_pair),
            Some((expected, 90))
        );
    }
    let specular = fixed_ps(true);
    assert_eq!(
        libraries
            .lookup_ready(
                view,
                view,
                PsSourceView::FixedFunction(&specular),
                variant(0),
                ShaderBackend::Msl
            )
            .map(|_| ()),
        None,
        "an unbuilt fixed-function pixel key goes to the slow path"
    );
}

#[test]
fn every_variant_field_reaches_the_compared_words() {
    let base = VariantKey::default();
    let changed: [VariantKey; 15] = [
        VariantKey {
            alpha_func: 1,
            ..base
        },
        VariantKey {
            fog_mode: 1,
            ..base
        },
        VariantKey {
            fog_table_mode: 1,
            ..base
        },
        VariantKey {
            linked_input_mask: 1,
            ..base
        },
        VariantKey {
            depth_sampler_mask: 0x8000,
            ..base
        },
        VariantKey {
            depth_fetch_mask: 0x8000,
            ..base
        },
        VariantKey {
            fetch4_mask: 0x8000,
            ..base
        },
        VariantKey {
            fetch4_alpha_mask: 0x8000,
            ..base
        },
        VariantKey {
            raw_depth_red_mask: 0x8000,
            ..base
        },
        VariantKey {
            volume_sampler_mask: 0x8000,
            ..base
        },
        VariantKey {
            cube_sampler_mask: 0x8000,
            ..base
        },
        VariantKey {
            tt_projected_mask: 0x80,
            ..base
        },
        VariantKey {
            color_out_mask: 0x80,
            ..base
        },
        VariantKey {
            sample_mask: 0x80,
            ..base
        },
        VariantKey {
            flags: VariantFlags::all(),
            ..base
        },
    ];
    for (index, left) in changed.iter().enumerate() {
        assert_ne!(
            variant_words(left),
            variant_words(&base),
            "field {index} is compared"
        );
        for right in &changed[index + 1..] {
            assert_ne!(
                variant_words(left),
                variant_words(right),
                "fields do not overlap"
            );
        }
    }
    let memo = LibraryMemo {
        ps_record: 8,
        ps_variant: changed[14],
        ..LibraryMemo::default()
    };
    assert!(memo.holds_ps(8, &changed[14]));
    assert!(!memo.holds_ps(8, &base));
    assert!(!memo.holds_ps(16, &changed[14]));
}

#[test]
fn lod_table_copies_at_one_address_answer_for_their_own_snapshot_records() {
    // A draw keyed for the vertex LOD table passes a copy of its snapshot
    // record that lives in the same local slot for every such draw, so the
    // memo identity must come from the snapshot record, and must tell the
    // record's two keys apart.
    let mut libraries = StageLibraries::default();
    let first = programmable_vs(1);
    let second = programmable_vs(3);
    let ps = programmable_ps(2);
    let ps_view = PsSourceView::Programmable(&ps);
    libraries.record_ps(ps_view, variant(0), ShaderBackend::Msl, Some(handles(20)));
    libraries.record_vs(
        VsSourceView::Programmable(&first.with_lod_table()),
        ShaderBackend::Msl,
        Some(handles(10)),
    );
    libraries.record_vs(
        VsSourceView::Programmable(&second.with_lod_table()),
        ShaderBackend::Msl,
        Some(handles(30)),
    );
    libraries.record_vs(
        VsSourceView::Programmable(&first),
        ShaderBackend::Msl,
        Some(handles(50)),
    );

    let mut slot = first.with_lod_table();
    let answer = |libraries: &mut StageLibraries, keyed: &ProgrammableVsSource, record| {
        libraries
            .lookup_ready(
                VsSourceView::Programmable(keyed),
                VsSourceView::Programmable(record),
                ps_view,
                variant(0),
                ShaderBackend::Msl,
            )
            .map(raw_pair)
    };
    assert_eq!(answer(&mut libraries, &slot, &first), Some((10, 20)));
    slot = second.with_lod_table();
    assert_eq!(
        answer(&mut libraries, &slot, &second),
        Some((30, 20)),
        "the second shader's copy at the first one's address gets its own library"
    );
    assert_eq!(answer(&mut libraries, &first, &first), Some((50, 20)));
    slot = first.with_lod_table();
    assert_eq!(
        answer(&mut libraries, &slot, &first),
        Some((10, 20)),
        "one record keyed with and without the table is two memo identities"
    );
    assert_ne!(
        vs_memo_identity(
            VsSourceView::Programmable(&slot),
            VsSourceView::Programmable(&first)
        ),
        vs_memo_identity(
            VsSourceView::Programmable(&first),
            VsSourceView::Programmable(&first)
        )
    );
}

#[test]
fn pair_backend_is_msl_under_msl() {
    let mut rule = PairRule::new(ShaderBackend::Msl);
    let (vs, ps) = (programmable_vs(1), programmable_ps(2));
    assert_eq!(
        rule.pair_backend(
            &programs(),
            VsSourceView::Programmable(&vs),
            PsSourceView::Programmable(&ps),
            &variant(0)
        ),
        ShaderBackend::Msl
    );
    assert!(
        rule.vs_lowers.is_empty() && rule.ps_lowers.is_empty(),
        "nothing is lowered under msl"
    );
}

#[test]
fn pair_backend_air_for_two_programmable_stages() {
    let mut rule = PairRule::new(ShaderBackend::Air);
    let (vs, ps) = (programmable_vs(1), programmable_ps(2));
    assert_eq!(
        rule.pair_backend(
            &programs(),
            VsSourceView::Programmable(&vs),
            PsSourceView::Programmable(&ps),
            &variant(0)
        ),
        ShaderBackend::Air
    );
}

#[test]
fn pair_backend_msl_when_either_stage_is_ff() {
    let mut rule = PairRule::new(ShaderBackend::Air);
    let programs = programs();
    let (vs, ps) = (programmable_vs(1), programmable_ps(2));
    let (fixed_vertex, fixed_pixel) = (fixed_vs(0), fixed_ps(false));
    for (vs, ps) in [
        (
            VsSourceView::FixedFunction(&fixed_vertex),
            PsSourceView::Programmable(&ps),
        ),
        (
            VsSourceView::Programmable(&vs),
            PsSourceView::FixedFunction(&fixed_pixel),
        ),
        (
            VsSourceView::FixedFunction(&fixed_vertex),
            PsSourceView::FixedFunction(&fixed_pixel),
        ),
    ] {
        assert_eq!(
            rule.pair_backend(&programs, vs, ps, &variant(0)),
            ShaderBackend::Msl
        );
    }
    assert!(
        rule.vs_lowers.is_empty() && rule.ps_lowers.is_empty(),
        "a fixed-function stage decides before any lowering"
    );
}

#[test]
fn pair_backend_msl_when_a_program_failed() {
    let programs = programs();
    let (vs, ps) = (programmable_vs(1), programmable_ps(2));
    for failed in [vs.vs_id, ps.ps_id] {
        let mut rule = PairRule::new(ShaderBackend::Air);
        assert!(rule.fail(failed));
        assert!(!rule.fail(failed), "a program fails once");
        assert_eq!(
            rule.pair_backend(
                &programs,
                VsSourceView::Programmable(&vs),
                PsSourceView::Programmable(&ps),
                &variant(0)
            ),
            ShaderBackend::Msl
        );
    }
}

#[test]
fn pair_backend_msl_when_a_lowering_refuses() {
    let mut rule = PairRule::new(ShaderBackend::Air);
    let (vs, ps) = (programmable_vs(1), programmable_ps(3));
    assert_eq!(
        rule.pair_backend(
            &programs(),
            VsSourceView::Programmable(&vs),
            PsSourceView::Programmable(&ps),
            &variant(0)
        ),
        ShaderBackend::Msl
    );
    assert_eq!(rule.ps_lowers.get(&(ps.ps_id, variant(0))), Some(&false));
}

#[test]
fn a_refusing_program_is_logged_once_across_stage_keys() {
    let mut rule = PairRule::new(ShaderBackend::Air);
    let (vs, ps) = (programmable_vs(1), programmable_ps(3));
    for v in [0, 4] {
        rule.pair_backend(
            &programs(),
            VsSourceView::Programmable(&vs),
            PsSourceView::Programmable(&ps),
            &variant(v),
        );
    }
    assert_eq!(rule.ps_lowers.len(), 2, "two stage keys lowered");
    assert_eq!(rule.refused.len(), 1);
    assert!(!log_refusal(&mut rule.refused, "PS", ps.ps_id, &"again"));
}

#[test]
fn air_lowering_is_memoized_per_stage_key() {
    let mut rule = PairRule::new(ShaderBackend::Air);
    let programs = programs();
    let (vs, ps) = (programmable_vs(1), programmable_ps(2));
    let backend = |rule: &mut PairRule, variant: VariantKey| {
        rule.pair_backend(
            &programs,
            VsSourceView::Programmable(&vs),
            PsSourceView::Programmable(&ps),
            &variant,
        )
    };
    assert_eq!(backend(&mut rule, variant(0)), ShaderBackend::Air);
    assert_eq!(backend(&mut rule, variant(0)), ShaderBackend::Air);
    assert_eq!((rule.vs_lowers.len(), rule.ps_lowers.len()), (1, 1));
    assert_eq!(backend(&mut rule, variant(4)), ShaderBackend::Air);
    assert_eq!(
        (rule.vs_lowers.len(), rule.ps_lowers.len()),
        (1, 2),
        "a new PS variant lowers once more, the VS key does not"
    );
    // The memo answers instead of lowering again.
    rule.ps_lowers.insert((ps.ps_id, variant(0)), false);
    assert_eq!(backend(&mut rule, variant(0)), ShaderBackend::Msl);
}

#[test]
fn memo_keys_keep_backends_apart() {
    let mut libraries = StageLibraries::default();
    let vs = programmable_vs(1);
    let ps = programmable_ps(2);
    let (vs_view, ps_view) = (
        VsSourceView::Programmable(&vs),
        PsSourceView::Programmable(&ps),
    );
    built_pair(&mut libraries, &vs, &ps);
    let lookup = |libraries: &mut StageLibraries, backend| {
        libraries
            .lookup_ready(vs_view, vs_view, ps_view, variant(0), backend)
            .map(raw_pair)
    };
    assert_eq!(lookup(&mut libraries, ShaderBackend::Msl), Some((10, 20)));
    assert_eq!(
        lookup(&mut libraries, ShaderBackend::Air),
        None,
        "the memo of an MSL pair does not answer an AIR one"
    );
    libraries.record_vs(vs_view, ShaderBackend::Air, Some(handles(30)));
    libraries.record_ps(ps_view, variant(0), ShaderBackend::Air, Some(handles(40)));
    for (backend, expected) in [
        (ShaderBackend::Air, (30, 40)),
        (ShaderBackend::Msl, (10, 20)),
        (ShaderBackend::Air, (30, 40)),
    ] {
        assert_eq!(lookup(&mut libraries, backend), Some(expected));
    }
}

#[test]
fn an_air_failure_leaves_the_msl_index_unknown() {
    let mut libraries = StageLibraries::default();
    let vs = programmable_vs(1);
    let ps = programmable_ps(2);
    let (vs_view, ps_view) = (
        VsSourceView::Programmable(&vs),
        PsSourceView::Programmable(&ps),
    );
    libraries.record_vs(vs_view, ShaderBackend::Air, None);
    libraries.record_ps(ps_view, variant(0), ShaderBackend::Air, None);
    assert!(matches!(
        libraries.lookup_vs(vs_view, ShaderBackend::Air),
        Some(None)
    ));
    assert!(libraries.lookup_vs(vs_view, ShaderBackend::Msl).is_none());
    assert!(matches!(
        libraries.lookup_ps(ps_view, variant(0), ShaderBackend::Air),
        Some(None)
    ));
    assert!(
        libraries
            .lookup_ps(ps_view, variant(0), ShaderBackend::Msl)
            .is_none()
    );
}
