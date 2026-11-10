use std::{
    sync::{Arc, Weak, mpsc},
    thread,
    time::Duration,
};

use mtld3d_core::{
    async_compile::TicketSource,
    dxso::{FfStage, FfStageFlags, FfVsFlags, parse},
};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

/// `vs_2_0`: `dcl_position v0` / `mov oPos, v0`.
pub const VS_2_0: [u32; 8] = [
    0xFFFE_0200,
    0x0200_001F,
    0x8000_0000,
    0x900F_0000,
    0x0200_0001,
    0xC00F_0000,
    0x90E4_0000,
    0x0000_FFFF,
];

/// `ps_2_0`: `def c0, 1, 0, 0, 1` / `mov oC0, c0`.
pub const PS_2_0: [u32; 11] = [
    0xFFFF_0200,
    0x0500_0051,
    0xA00F_0000,
    0x3F80_0000,
    0,
    0,
    0x3F80_0000,
    0x0200_0001,
    0x800F_0800,
    0xA0E4_0000,
    0x0000_FFFF,
];

/// `ps_2_0`: `dcl t0` / `dcl_2d s16` / `texld r0, t0, s16` / `mov oC0, r0`.
///
/// It parses, and the translator refuses a sampler above s15.
pub const PS_S16: [u32; 15] = [
    0xFFFF_0200,
    0x0200_001F,
    0x8000_0000,
    0xB00F_0000,
    0x0200_001F,
    0x9000_0000,
    0xA00F_0810,
    0x0300_0042,
    0x800F_0000,
    0xB0E4_0000,
    0xA0E4_0810,
    0x0200_0001,
    0x800F_0800,
    0x80E4_0000,
    0x0000_FFFF,
];

/// Programs 1 (`VS_2_0`), 2 (`PS_2_0`) and 3 (`PS_S16`), as the encoder registers them.
pub fn programs() -> FxHashMap<ProgramId, Arc<DxsoProgram>> {
    [(1, &VS_2_0[..]), (2, &PS_2_0[..]), (3, &PS_S16[..])]
        .into_iter()
        .map(|(id, tokens)| {
            let program = mtld3d_core::dxso::parse(tokens).expect("the test program parses");
            (ProgramId::from_shader_reply(id), Arc::new(program))
        })
        .collect()
}

fn retained_job() -> (QueuedJob, Weak<DxsoProgram>) {
    let tokens = [
        0xFFFF_0200,
        0x0200_0001,
        0x800F_0800,
        0xA0E4_0000,
        0x0000_FFFF,
    ];
    let program = Arc::new(parse(&tokens).expect("valid shader"));
    let weak = Arc::downgrade(&program);
    let job = QueuedJob {
        job: CompileJob::Library(LibraryJob {
            input: LibraryInput::ProgrammablePs {
                ps_id: ProgramId::from_tokens(&tokens),
                program,
                variant: VariantKey::default(),
                backend: ShaderBackend::Msl,
            },
            reference: ShaderRecordRef::new(CachedKind::Sm2Ps, 1),
            device: MetalHandle::NULL,
            persist: false,
            cache_path: None,
            translator: None,
        }),
        enqueued_tsc: 0,
    };
    (job, weak)
}

/// Cancellation drops queued owners while a running job retains its owner through close and join.
#[test]
fn canceled_queue_and_blocked_worker_release_distinct_job_owners() {
    let queue = Arc::new(CompileQueue::new());
    let mut tickets = TicketSource::new();
    let running_ticket = tickets.issue();
    let (running, running_owner) = retained_job();
    queue.push(running_ticket, running);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let worker_queue = Arc::clone(&queue);
    let worker = thread::spawn(move || {
        worker_main_with(&worker_queue, |ticket, job| {
            assert_eq!(ticket, running_ticket, "only the first job may execute");
            entered_tx.send(()).expect("test is waiting for entry");
            release_rx
                .recv_timeout(WAIT)
                .expect("release blocked build");
            drop(job);
            true
        });
    });
    entered_rx
        .recv_timeout(WAIT)
        .expect("worker took first job");
    let canceled_ticket = tickets.issue();
    let (canceled, canceled_owner) = retained_job();
    queue.push(canceled_ticket, canceled);
    assert_eq!(running_owner.strong_count(), 1);
    assert_eq!(canceled_owner.strong_count(), 1);
    let (ticket, pending) = queue.take_any().expect("unstarted job can be canceled");
    assert_eq!(ticket, canceled_ticket);
    drop(pending);
    assert!(
        canceled_owner.upgrade().is_none(),
        "canceled job drops its sole owner"
    );
    assert!(queue.take_any().is_none());
    queue.close();
    assert!(
        !worker.is_finished(),
        "closing cannot end an executing build"
    );
    assert_eq!(
        running_owner.strong_count(),
        1,
        "running build still owns its input"
    );
    let (joining_tx, joining_rx) = mpsc::channel();
    let (joined_tx, joined_rx) = mpsc::channel();
    let joiner = thread::spawn(move || {
        joining_tx.send(()).expect("join starts");
        worker.join().expect("compile worker exits after release");
        joined_tx.send(()).expect("join completion observed");
    });
    joining_rx.recv_timeout(WAIT).expect("join attempt started");
    assert!(
        joined_rx.try_recv().is_err(),
        "join cannot complete before build release"
    );
    release_tx.send(()).expect("worker still awaits release");
    joined_rx
        .recv_timeout(WAIT)
        .expect("join completes after release");
    joiner.join().expect("join observer exits");
    assert!(
        running_owner.upgrade().is_none(),
        "running job drops its sole owner"
    );
    assert!(
        canceled_owner.upgrade().is_none(),
        "no canceled owner was revived"
    );
}

fn pipeline_job(sibling_of: Option<u64>) -> QueuedJob {
    let snapshot = PipelineSnapshot {
        vs_fn: MetalHandle::NULL,
        ps_fn: MetalHandle::NULL,
        vdecl_hash: 0,
        stream_layouts: [mtld3d_core::pipeline_state::StreamLayout::UNUSED;
            mtld3d_types::MAX_STREAMS as usize],
        color_format: mtld3d_shared::mtl::PixelFormat::Bgra8Unorm,
        attach: mtld3d_core::pipeline_state::PipelineAttachFlags::empty(),
        rs: mtld3d_core::pipeline_state::PipelineRsBits::default(),
        extra: mtld3d_core::pipeline_state::ExtraColorAttachments::NONE,
        ps_color_out_mask: 1,
        sample_count: 1,
    };
    let shader = PairShaderId {
        is_programmable: true,
        hash: 1,
    };
    QueuedJob {
        job: CompileJob::Pipeline(Box::new(PipelineJob {
            snapshot,
            vertex_attrs: Vec::new(),
            identity: PipelineIdentity {
                shader_refs: None,
                vs: shader,
                ps: shader,
                air_programs: None,
            },
            sibling_of,
            device: MetalHandle::NULL,
            persist: false,
            cache_path: None,
            translator: None,
        })),
        enqueued_tsc: 0,
    }
}

/// A primary pipeline starts ahead of queued libraries; a no-colour sibling queues with them.
#[test]
fn push_sends_primary_pipelines_ahead_and_siblings_with_the_libraries() {
    let queue = CompileQueue::new();
    let mut tickets = TicketSource::new();
    let library = tickets.issue();
    let sibling = tickets.issue();
    let primary = tickets.issue();
    queue.push(library, retained_job().0);
    queue.push(sibling, pipeline_job(Some(0x1000)));
    queue.push(primary, pipeline_job(None));
    let order: Vec<JobTicket> = core::iter::from_fn(|| queue.take_any())
        .map(|(ticket, _)| ticket)
        .collect();
    assert_eq!(order, [primary, library, sibling]);
}

fn source_records() -> (ProgrammableVsSource, ProgrammablePsSource) {
    let vs = ProgrammableVsSource {
        vs_id: ProgramId::from_shader_reply(1),
        max_const_used: 0,
        provided_input_mask: 1,
        flags: mtld3d_core::draw_data::ShaderSourceFlags::empty(),
        clip_plane_count: 0,
        sampler_kinds: VsSamplerKinds::default(),
        reserved: [0; 7],
    };
    let ps = ProgrammablePsSource {
        ps_id: ProgramId::from_shader_reply(2),
        max_const_used: 0,
        flags: mtld3d_core::draw_data::ShaderSourceFlags::empty(),
        color_out_mask: 1,
        reserved: [0; 4],
    };
    (vs, ps)
}

/// A lit fixed-function VS source: position, normal, diffuse and one coordinate set.
pub fn ff_vs_source() -> FixedVsSource {
    FixedVsSource {
        key: FfVsKey {
            flags: FfVsFlags::HAS_COLOR0 | FfVsFlags::HAS_NORMAL | FfVsFlags::LIGHTING_ENABLED,
            input_tex_coord_count: 1,
            tex_coord_count: 1,
            light_active_mask: 1,
            light_directional_mask: 1,
            light_spot_mask: 0,
            diffuse_source: 0,
            ambient_source: 0,
            specular_source: 0,
            emissive_source: 0,
            fog_mode: 0,
            tci: [0; 8],
            passthrough: [0; 8],
            tex_coord_dims: [2, 0, 0, 0, 0, 0, 0, 0],
            tt_flags: [0; 8],
            vertex_blend_count: 0,
            declared_weights_count: 0,
            clip_plane_count: 0,
            reserved: 0,
        },
        max_row_count: 8,
        reserved: [0; 6],
    }
}

/// A fixed-function PS source; `textured` modulates stage 0's texture into the diffuse colour.
pub fn ff_ps_source(textured: bool) -> FixedPsSource {
    use mtld3d_types::{
        D3DTA_DIFFUSE, D3DTA_TEXTURE, D3DTOP_DISABLE, D3DTOP_MODULATE, D3DTOP_SELECTARG1,
    };

    let code = |value: u32| u8::try_from(value).expect("a D3D9 code fits u8");
    let disabled = FfStage {
        color_op: code(D3DTOP_DISABLE),
        alpha_op: code(D3DTOP_DISABLE),
        ..FfStage::default()
    };
    let mut stages = [disabled; 8];
    stages[0] = if textured {
        FfStage {
            color_op: code(D3DTOP_MODULATE),
            color_arg1: code(D3DTA_TEXTURE),
            color_arg2: code(D3DTA_DIFFUSE),
            alpha_op: code(D3DTOP_MODULATE),
            alpha_arg1: code(D3DTA_TEXTURE),
            alpha_arg2: code(D3DTA_DIFFUSE),
            flags: FfStageFlags::HAS_TEXTURE,
        }
    } else {
        FfStage {
            color_op: code(D3DTOP_SELECTARG1),
            color_arg1: code(D3DTA_DIFFUSE),
            alpha_op: code(D3DTOP_SELECTARG1),
            alpha_arg1: code(D3DTA_DIFFUSE),
            ..FfStage::default()
        }
    };
    FixedPsSource {
        key: FfPsKey {
            stages,
            specular_add: false,
            tt_projected_mask: 0,
        },
        sampled_stage_mask: u16::from(textured),
        constant_rows: 0,
        bump_env: false,
        reserved: [0; 2],
    }
}

/// A programmable PS source of program `id`.
pub const fn ps_source(id: u64) -> ProgrammablePsSource {
    ProgrammablePsSource {
        ps_id: ProgramId::from_shader_reply(id),
        max_const_used: 0,
        flags: mtld3d_core::draw_data::ShaderSourceFlags::empty(),
        color_out_mask: 1,
        reserved: [0; 4],
    }
}

/// A fresh folder for one test's cache file, and the file's path in it.
fn temp_cache(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mtld3d-compile-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join("mtld3d_shaders.bin")
}

/// The records of a current cache file.
fn loaded(path: &std::path::Path) -> shader_cache::CacheRecords {
    match shader_cache::load(path).expect("readable") {
        shader_cache::CacheLoad::Current(records) => records,
        _ => panic!("the cache is current"),
    }
}

/// The recipe kinds of a pipeline identity.
fn recipe_kinds(identity: &PipelineIdentity) -> Option<(CachedKind, CachedKind)> {
    identity.shader_refs.map(|(vs, ps)| (vs.kind(), ps.kind()))
}

/// A fixed-function pair's recipe names the fixed-function kinds of its backend (ruling C3c-6).
#[test]
fn an_air_ff_recipe_names_kinds_8_and_9() {
    let programs = programs();
    let (vs, ps) = (ff_vs_source(), ff_ps_source(true));
    let shaders = |backend| ShaderRef {
        vs: VsSourceView::FixedFunction(&vs),
        ps: PsSourceView::FixedFunction(&ps),
        variant: VariantKey::default(),
        backend,
    };
    let air = pipeline_identity(&programs, &shaders(ShaderBackend::Air));
    assert_eq!(
        recipe_kinds(&air),
        Some((CachedKind::AirFfVs, CachedKind::AirFfPs))
    );
    assert_eq!(
        air.air_programs,
        Some((
            libraries::AirStage::FfVs(vs.key.clone()),
            libraries::AirStage::FfPs(ps.key.clone())
        )),
        "an AIR pair's stages can fail"
    );
    assert_eq!(recipe_kinds(&air.copied()), recipe_kinds(&air));
    let msl = pipeline_identity(&programs, &shaders(ShaderBackend::Msl));
    assert_eq!(
        recipe_kinds(&msl),
        Some((CachedKind::FfVs, CachedKind::FfPs))
    );
    assert!(msl.air_programs.is_none());
}

/// An AIR pair of a fixed-function VS and a programmable PS writes a recipe the reader keeps.
#[test]
fn an_ff_by_programmable_air_recipe_is_valid() {
    let ps_program = Arc::new(parse(&PS_2_0).expect("parses"));
    let ps = ProgrammablePsSource {
        ps_id: ProgramId::from_tokens(&PS_2_0),
        ..ps_source(0)
    };
    let mut programs = programs();
    programs.insert(ps.ps_id, Arc::clone(&ps_program));
    let vs = ff_vs_source();
    let shaders = ShaderRef {
        vs: VsSourceView::FixedFunction(&vs),
        ps: PsSourceView::Programmable(&ps),
        variant: VariantKey::default(),
        backend: ShaderBackend::Air,
    };
    let identity = pipeline_identity(&programs, &shaders);
    assert_eq!(
        recipe_kinds(&identity),
        Some((CachedKind::AirFfVs, CachedKind::AirSm2Ps))
    );
    let (vs_ref, ps_ref) = identity.shader_refs.expect("both kinds");
    let mut snapshot = match pipeline_job(None).job {
        CompileJob::Pipeline(job) => job.snapshot,
        CompileJob::Library(_) => unreachable!("pipeline_job queues a pipeline"),
    };
    snapshot.attach = mtld3d_core::pipeline_state::PipelineAttachFlags::HAS_COLOR_OUTPUT;
    // The reader keeps a recipe only beside both records; their metallibs are never loaded here.
    let records = [
        (
            vs_ref,
            shader_cache::ShaderSource::ff_vertex(&vs.key, &mtld3d_core::ff_build::vertex(&vs.key)),
        ),
        (
            ps_ref,
            shader_cache::ShaderSource::pixel(&ps_program, VariantKey::default()),
        ),
    ];
    let path = temp_cache("ff-recipe");
    let writer = shader_cache::CacheWriter::open(&path).expect("cache file");
    for (reference, source) in records {
        assert_eq!(source.disk_key(), reference.key());
        let entry = shader_cache::CacheEntry::new_air(
            reference.kind(),
            reference.key(),
            [7; 32],
            vec![0; 4],
            source,
        );
        writer.append_shader(&entry).expect("append record");
    }
    writer
        .append_pipeline(&PipelineRecipe::from_snapshot(
            vs_ref,
            ps_ref,
            &snapshot,
            &[],
        ))
        .expect("append recipe");
    let kept = loaded(&path);
    assert_eq!(
        kept.pipelines
            .iter()
            .map(|recipe| (recipe.vs().kind(), recipe.ps().kind()))
            .collect::<Vec<_>>(),
        [(CachedKind::AirFfVs, CachedKind::AirSm2Ps)],
        "one backend per recipe: the reader keeps it"
    );
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// The recipe of an AIR pair names AIR kinds, and the identity a deferred draw copies keeps them.
#[test]
fn pipeline_refs_of_an_air_pair_name_air_kinds() {
    let programs = programs();
    let (vs, ps) = source_records();
    let shaders = |backend| ShaderRef {
        vs: VsSourceView::Programmable(&vs),
        ps: PsSourceView::Programmable(&ps),
        variant: VariantKey::default(),
        backend,
    };
    let kinds =
        |identity: &PipelineIdentity| identity.shader_refs.map(|(vs, ps)| (vs.kind(), ps.kind()));
    let air = pipeline_identity(&programs, &shaders(ShaderBackend::Air));
    assert_eq!(
        kinds(&air),
        Some((CachedKind::AirSm2Vs, CachedKind::AirSm2Ps))
    );
    assert_eq!(
        air.air_programs,
        Some((
            libraries::AirStage::Program(vs.vs_id),
            libraries::AirStage::Program(ps.ps_id)
        ))
    );
    let deferred = air.copied();
    assert_eq!(kinds(&deferred), kinds(&air));
    assert_eq!(deferred.air_programs, air.air_programs);
    let msl = pipeline_identity(&programs, &shaders(ShaderBackend::Msl));
    assert_eq!(kinds(&msl), Some((CachedKind::Sm2Vs, CachedKind::Sm2Ps)));
    assert_eq!(msl.air_programs, None);
    assert_eq!(
        air.shader_refs.map(|(vs, ps)| (vs.key(), ps.key())),
        msl.shader_refs.map(|(vs, ps)| (vs.key(), ps.key())),
        "an AIR record keeps its MSL twin's key"
    );
}

type Device = crate::mtl::objc2::rc::Retained<
    crate::mtl::objc2::runtime::ProtocolObject<dyn crate::mtl::objc2_metal::MTLDevice>,
>;

fn device_handle(device: &Device) -> MetalHandle<MTLDeviceKind> {
    // SAFETY: the device's retain outlives every use of the handle in these tests.
    unsafe { MetalHandle::new(crate::mtl::objc2::rc::Retained::as_ptr(device) as u64) }
}

/// The snapshot and attributes of [`pipeline_job`], and the outcome of its failed build.
fn failed_pipeline(
    air_programs: Option<(libraries::AirStage, libraries::AirStage)>,
) -> PipelineOutcome {
    let (snapshot, vertex_attrs) = match pipeline_job(None).job {
        CompileJob::Pipeline(job) => (job.snapshot, job.vertex_attrs),
        CompileJob::Library(_) => unreachable!("pipeline_job queues a pipeline"),
    };
    let shader = PairShaderId {
        is_programmable: true,
        hash: 1,
    };
    PipelineOutcome {
        key: pipeline_state::key_from_snapshot(&snapshot, &vertex_attrs),
        snapshot,
        vs: shader,
        ps: shader,
        air_programs,
        sibling_of: None,
        handle: None,
        total_ns: 0,
        persist_ns: 0,
        native: PipelineTimings::new(),
        persist_error: None,
    }
}

/// The outcome of a library build that failed.
fn failed_library(input: LibraryInput, reference: ShaderRecordRef) -> LibraryOutcome {
    LibraryOutcome {
        input,
        reference,
        handles: None,
        emitted: false,
        bucket: None,
        elapsed: Duration::ZERO,
        total_ns: 0,
        emit_ns: 0,
        persist_ns: 0,
        native: ShaderTimings::new(),
        persist_error: None,
    }
}

/// A failed AIR library or pipeline sends its programs' later pairs to MSL (ruling C3b-3).
#[test]
fn failed_air_pipeline_demotes_the_pair() {
    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let mut enc = encoder(&device, ShaderBackend::Air, None);
    let (vs, ps) = source_records();
    let pair = |enc: &mut FrameEncoder| {
        enc.pair_backend(
            VsSourceView::Programmable(&vs),
            PsSourceView::Programmable(&ps),
            &VariantKey::default(),
        )
    };
    assert_eq!(pair(&mut enc), ShaderBackend::Air);
    enc.install_pipeline(failed_pipeline(Some((
        libraries::AirStage::Program(vs.vs_id),
        libraries::AirStage::Program(ps.ps_id),
    ))));
    assert_eq!(
        pair(&mut enc),
        ShaderBackend::Msl,
        "a failed AIR pipeline demotes"
    );

    // A library: a fresh rule, then the PS's AIR library fails.
    enc.pair_rule = libraries::PairRule::new(ShaderBackend::Air);
    assert_eq!(pair(&mut enc), ShaderBackend::Air);
    enc.install_library(failed_library(
        LibraryInput::ProgrammablePs {
            ps_id: ps.ps_id,
            program: Arc::clone(&enc.program_cache[&ps.ps_id]),
            variant: VariantKey::default(),
            backend: ShaderBackend::Air,
        },
        ShaderRecordRef::new(CachedKind::AirSm2Ps, 1),
    ));
    assert_eq!(
        pair(&mut enc),
        ShaderBackend::Msl,
        "a failed AIR library demotes"
    );
    assert!(
        enc.libraries
            .lookup_ps(
                PsSourceView::Programmable(&ps),
                VariantKey::default(),
                ShaderBackend::Msl
            )
            .is_none(),
        "the failure lands under the AIR key only"
    );
    enc.shutdown_cleanup();
}

/// A failed AIR pipeline of fixed-function stages demotes both keys; a failed FF library its own.
#[test]
fn an_air_ff_pipeline_failure_demotes_both_stages() {
    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let mut enc = encoder(&device, ShaderBackend::Air, None);
    let (app_vertex, app_pixel) = source_records();
    let (vs, ps) = (ff_vs_source(), ff_ps_source(true));
    let pairs = [
        (
            VsSourceView::FixedFunction(&vs),
            PsSourceView::FixedFunction(&ps),
        ),
        (
            VsSourceView::FixedFunction(&vs),
            PsSourceView::Programmable(&app_pixel),
        ),
        (
            VsSourceView::Programmable(&app_vertex),
            PsSourceView::FixedFunction(&ps),
        ),
    ];
    let backends = |enc: &mut FrameEncoder| {
        pairs.map(|(vs, ps)| enc.pair_backend(vs, ps, &VariantKey::default()))
    };
    assert_eq!(backends(&mut enc), [ShaderBackend::Air; 3]);
    enc.install_pipeline(failed_pipeline(Some((
        libraries::AirStage::FfVs(vs.key.clone()),
        libraries::AirStage::FfPs(ps.key.clone()),
    ))));
    assert_eq!(backends(&mut enc), [ShaderBackend::Msl; 3]);
    let programmable = enc.pair_backend(
        VsSourceView::Programmable(&app_vertex),
        PsSourceView::Programmable(&app_pixel),
        &VariantKey::default(),
    );
    assert_eq!(
        programmable,
        ShaderBackend::Air,
        "the programs did not fail"
    );

    enc.pair_rule = libraries::PairRule::new(ShaderBackend::Air);
    assert_eq!(backends(&mut enc), [ShaderBackend::Air; 3]);
    let ps_view = PsSourceView::FixedFunction(&ps);
    let disk_key = ps_view.disk_key(VariantKey::default());
    enc.install_library(failed_library(
        LibraryInput::FixedFunctionPs {
            key: ps.key.clone(),
            variant: VariantKey::default(),
            backend: ShaderBackend::Air,
            program: enc.pair_rule.ff_ps_program(&ps.key),
        },
        ShaderRecordRef::new(CachedKind::AirFfPs, disk_key),
    ));
    assert_eq!(
        backends(&mut enc),
        [ShaderBackend::Msl, ShaderBackend::Air, ShaderBackend::Msl],
        "the PS key's pairs demote, the VS key's other pair stays"
    );
    assert!(matches!(
        enc.libraries
            .lookup_ps(ps_view, VariantKey::default(), ShaderBackend::Air),
        Some(None)
    ));
    assert!(
        enc.libraries
            .lookup_ps(ps_view, VariantKey::default(), ShaderBackend::Msl)
            .is_none(),
        "the failure lands under the AIR index only"
    );
    enc.shutdown_cleanup();
}

/// An AIR pair's recipe is written only when the job has the build's key; other pairs' always.
#[test]
fn an_air_recipe_is_not_written_without_a_build_key() {
    assert!(!recipe_persists(true, None));
    assert!(recipe_persists(true, Some([7u8; 32])));
    assert!(recipe_persists(false, None));
}

/// An AIR library is built and its record written only when the job has the build's key.
#[test]
fn an_air_record_is_not_written_without_a_build_key() {
    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let dir = std::env::temp_dir().join(format!("mtld3d-air-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let program = Arc::new(parse(&VS_2_0).expect("parses"));
    // The record key is the source's, which the reader checks.
    let key =
        shader_cache::ShaderSource::vertex(&program, 1, 0, VsSamplerKinds::default()).disk_key();
    for translator in [None, Some([7u8; 32])] {
        let path = dir.join(format!("{}.bin", translator.is_some()));
        let _ = std::fs::remove_file(&path);
        let outcome = build_library(LibraryJob {
            input: LibraryInput::ProgrammableVs {
                vs_id: ProgramId::from_shader_reply(1),
                program: Arc::clone(&program),
                provided_input_mask: 1,
                clip_plane_count: 0,
                sampler_kinds: VsSamplerKinds::default(),
                backend: ShaderBackend::Air,
            },
            reference: ShaderRecordRef::new(CachedKind::AirSm2Vs, key),
            device: device_handle(&device),
            persist: true,
            cache_path: Some(path.clone()),
            translator,
        });
        let handles = outcome.handles.expect("the AIR library builds");
        assert!(outcome.emitted && outcome.persist_error.is_none());
        crate::metal::destroy_function(handles.func.raw());
        crate::metal::destroy_library(handles.library.raw());
        match (translator, shader_cache::load(&path).expect("readable")) {
            (None, shader_cache::CacheLoad::Missing) => {}
            (Some(key), shader_cache::CacheLoad::Current(records)) => {
                let [entry] = records.shaders.as_slice() else {
                    panic!("one record, got {}", records.shaders.len());
                };
                assert_eq!(entry.kind, CachedKind::AirSm2Vs);
                assert_eq!(entry.air().map(|(key, _)| *key), Some(key));
            }
            (translator, _) => panic!("unexpected cache state for key {translator:?}"),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// An encoder on `device` configured with `backend`, writing its cache at `cache_path` when given.
///
/// The context's counters are leaked: the encoder keeps their addresses.
fn encoder(
    device: &Device,
    backend: ShaderBackend,
    cache_path: Option<std::path::PathBuf>,
) -> FrameEncoder {
    use std::sync::atomic::AtomicU64;

    use mtld3d_core::{config::Mtld3dConfig, gpu_caps::GpuCaps};
    use mtld3d_shared::{clock_calibration::ClockCalibration, record_handle::DeviceRecordHandle};

    let coherent: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0)));
    let retained: &'static AtomicU64 = Box::leak(Box::new(AtomicU64::new(0)));
    let context = crate::encoder_service::EncoderContext {
        device_handle: device_handle(device),
        record_handle: DeviceRecordHandle::NULL,
        coherent_seq_ptr: core::ptr::from_ref(coherent) as u64,
        upload_coherent_seq_ptr: 0,
        failed_submit_seq_ptr: 0,
        retained_bytes_ptr: core::ptr::from_ref(retained) as u64,
        paced_present_seq_ptr: 0,
        largest_free_word_ptr: 0,
        guest_page_bytes_ptr: 0,
        guest_pool_bytes_ptr: 0,
    };
    let mut enc = FrameEncoder::new(
        GpuCaps::apple_silicon_default(),
        Arc::new(Mtld3dConfig {
            shader_backend: backend,
            ..Mtld3dConfig::default()
        }),
        cache_path,
        Arc::new(ClockCalibration::new()),
        &context,
        super::super::NativeSubmitSpawner,
    )
    .expect("an encoder on the default device");
    enc.program_cache = programs();
    enc
}

/// Under `air`, a fixed-function VS paired with a refusing PS is MSL and its record has no source.
#[test]
fn an_msl_ff_library_under_air_persists_sourceless() {
    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let path = temp_cache("ff-sourceless");
    let mut enc = encoder(&device, ShaderBackend::Air, Some(path.clone()));
    enc.ingest_warm_cache(crate::encoder::WarmCache::empty(), false);
    let (vs, refusing) = (ff_vs_source(), ps_source(3));
    let view = VsSourceView::FixedFunction(&vs);
    let backend = enc.pair_backend(
        view,
        PsSourceView::Programmable(&refusing),
        &VariantKey::default(),
    );
    assert_eq!(backend, ShaderBackend::Msl);
    assert!(!matches!(
        enc.resolve_vs_library(view, backend),
        mtld3d_core::async_compile::Resolution::Failed
    ));
    enc.finish_compiles(false);
    let key = view.disk_key();
    let msl = emit_vs_ff_named(&vs.key, &CachedKind::FfVs.entry_name(key));
    assert_eq!(
        loaded(&path).shaders,
        [shader_cache::CacheEntry::new(
            CachedKind::FfVs,
            key,
            msl,
            None
        )],
        "the record C3b wrote: MSL, no source"
    );
    enc.shutdown_cleanup();
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// Under `air`, a lowered fixed-function pair persists kinds 8 and 9 with their sources.
#[test]
fn an_air_ff_pair_through_the_encoder_persists_its_sources() {
    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let path = temp_cache("ff-air-encoder");
    let mut enc = encoder(&device, ShaderBackend::Air, Some(path.clone()));
    // A cargo build has no translator key; AIR records need one.
    enc.translator_key = Some([7; 32]);
    enc.ingest_warm_cache(crate::encoder::WarmCache::empty(), false);
    let (vs, ps) = (ff_vs_source(), ff_ps_source(true));
    let (vs_view, ps_view) = (
        VsSourceView::FixedFunction(&vs),
        PsSourceView::FixedFunction(&ps),
    );
    let variant = VariantKey::default();
    let backend = enc.pair_backend(vs_view, ps_view, &variant);
    assert_eq!(backend, ShaderBackend::Air);
    for failed in [
        matches!(
            enc.resolve_vs_library(vs_view, backend),
            mtld3d_core::async_compile::Resolution::Failed
        ),
        matches!(
            enc.resolve_ps_library(ps_view, variant, backend),
            mtld3d_core::async_compile::Resolution::Failed
        ),
    ] {
        assert!(!failed);
    }
    enc.finish_compiles(false);
    let mut rule = libraries::PairRule::new(ShaderBackend::Air);
    assert_eq!(
        rule.pair_backend(&programs(), vs_view, ps_view, &variant),
        ShaderBackend::Air
    );
    let vs_built = rule.ff_vs_program(&vs.key).expect("the VS lowered");
    let ps_built = rule.ff_ps_program(&ps.key).expect("the PS lowered");
    let want = [
        (
            CachedKind::AirFfVs,
            shader_cache::ShaderSource::ff_vertex(&vs.key, vs_built.program()),
        ),
        (
            CachedKind::AirFfPs,
            shader_cache::ShaderSource::ff_pixel(&ps.key, variant, ps_built.program()),
        ),
    ];
    let records = loaded(&path);
    // Worker completion order is not fixed: compare as a set.
    assert_eq!(records.shaders.len(), 2);
    for (kind, source) in &want {
        assert!(
            records
                .shaders
                .iter()
                .any(|entry| entry.kind == *kind && entry.source() == Some(source)),
            "a {kind:?} record with the program the rule built"
        );
    }
    enc.shutdown_cleanup();
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// Under `air`, a fixed-function stage the rule has no program for fails; no job is queued.
#[test]
fn an_air_ff_stage_without_a_program_fails_without_a_job() {
    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let mut enc = encoder(&device, ShaderBackend::Air, None);
    enc.ingest_warm_cache(crate::encoder::WarmCache::empty(), false);
    let (vs, ps) = (ff_vs_source(), ff_ps_source(true));
    assert!(matches!(
        enc.resolve_vs_library(VsSourceView::FixedFunction(&vs), ShaderBackend::Air),
        mtld3d_core::async_compile::Resolution::Failed
    ));
    assert!(matches!(
        enc.resolve_ps_library(
            PsSourceView::FixedFunction(&ps),
            VariantKey::default(),
            ShaderBackend::Air
        ),
        mtld3d_core::async_compile::Resolution::Failed
    ));
    enc.shutdown_cleanup();
}

/// Build both fixed-function stages of an AIR pair into one cache file, stamped `translator`.
///
/// Answers each stage's source and the file.
fn build_ff_air(
    name: &str,
    translator: Option<[u8; 32]>,
) -> ([shader_cache::ShaderSource; 2], std::path::PathBuf) {
    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let path = temp_cache(name);
    let mut rule = libraries::PairRule::new(ShaderBackend::Air);
    let (vs, ps) = (ff_vs_source(), ff_ps_source(true));
    let (vs_view, ps_view) = (
        VsSourceView::FixedFunction(&vs),
        PsSourceView::FixedFunction(&ps),
    );
    let variant = VariantKey::default();
    assert_eq!(
        rule.pair_backend(&programs(), vs_view, ps_view, &variant),
        ShaderBackend::Air
    );
    let vs_program = rule.ff_vs_program(&vs.key).expect("the VS lowered");
    let ps_program = rule.ff_ps_program(&ps.key).expect("the PS lowered");
    let sources = [
        shader_cache::ShaderSource::ff_vertex(&vs.key, vs_program.program()),
        shader_cache::ShaderSource::ff_pixel(&ps.key, variant, ps_program.program()),
    ];
    let jobs = [
        (
            LibraryInput::FixedFunctionVs {
                key: vs.key.clone(),
                backend: ShaderBackend::Air,
                program: Some(vs_program),
            },
            ShaderRecordRef::new(CachedKind::AirFfVs, vs_view.disk_key()),
        ),
        (
            LibraryInput::FixedFunctionPs {
                key: ps.key.clone(),
                variant,
                backend: ShaderBackend::Air,
                program: Some(ps_program),
            },
            ShaderRecordRef::new(CachedKind::AirFfPs, ps_view.disk_key(variant)),
        ),
    ];
    for (input, reference) in jobs {
        let outcome = build_library(LibraryJob {
            input,
            reference,
            device: device_handle(&device),
            persist: true,
            cache_path: Some(path.clone()),
            translator,
        });
        let handles = outcome.handles.expect("the AIR library builds");
        assert!(outcome.emitted && outcome.persist_error.is_none());
        assert_eq!(outcome.bucket, Some(CompileBucket::Ff));
        crate::metal::destroy_function(handles.func.raw());
        crate::metal::destroy_library(handles.library.raw());
    }
    (sources, path)
}

/// An AIR fixed-function library is written as kind 8 or 9 with the key and tokens it came from.
#[test]
fn an_air_ff_library_persists_with_its_source() {
    const KEY: [u8; 32] = [7; 32];
    let (sources, path) = build_ff_air("ff-air", Some(KEY));
    let records = loaded(&path);
    let kept: Vec<_> = records
        .shaders
        .iter()
        .map(|entry| (entry.kind, entry.source(), entry.air().map(|(key, _)| *key)))
        .collect();
    assert_eq!(
        kept,
        [
            (CachedKind::AirFfVs, Some(&sources[0]), Some(KEY)),
            (CachedKind::AirFfPs, Some(&sources[1]), Some(KEY)),
        ]
    );
    for entry in &records.shaders {
        assert_eq!(
            entry.source().map(shader_cache::ShaderSource::disk_key),
            Some(entry.key)
        );
    }
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

/// Without the build's translator key an AIR fixed-function library builds and writes nothing.
#[test]
fn a_keyless_air_ff_build_persists_no_record() {
    let (_, path) = build_ff_air("ff-keyless", None);
    assert!(matches!(
        shader_cache::load(&path).expect("readable"),
        shader_cache::CacheLoad::Missing
    ));
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}
