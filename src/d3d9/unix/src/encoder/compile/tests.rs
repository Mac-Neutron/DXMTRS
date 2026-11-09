use std::{
    sync::{Arc, Weak, mpsc},
    thread,
    time::Duration,
};

use mtld3d_core::{async_compile::TicketSource, dxso::parse};

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
    assert_eq!(air.air_programs, Some((vs.vs_id, ps.ps_id)));
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

/// A failed AIR library or pipeline sends its programs' later pairs to MSL (ruling C3b-3).
#[test]
fn failed_air_pipeline_demotes_the_pair() {
    use std::sync::atomic::AtomicU64;

    use mtld3d_core::{config::Mtld3dConfig, gpu_caps::GpuCaps};
    use mtld3d_shared::{clock_calibration::ClockCalibration, record_handle::DeviceRecordHandle};

    let device = crate::mtl::objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
    let coherent = AtomicU64::new(0);
    let retained = AtomicU64::new(0);
    let context = crate::encoder_service::EncoderContext {
        device_handle: device_handle(&device),
        record_handle: DeviceRecordHandle::NULL,
        coherent_seq_ptr: core::ptr::from_ref(&coherent) as u64,
        upload_coherent_seq_ptr: 0,
        failed_submit_seq_ptr: 0,
        retained_bytes_ptr: core::ptr::from_ref(&retained) as u64,
        paced_present_seq_ptr: 0,
        largest_free_word_ptr: 0,
        guest_page_bytes_ptr: 0,
        guest_pool_bytes_ptr: 0,
    };
    let mut enc = FrameEncoder::new(
        GpuCaps::apple_silicon_default(),
        Arc::new(Mtld3dConfig {
            shader_backend: ShaderBackend::Air,
            ..Mtld3dConfig::default()
        }),
        None,
        Arc::new(ClockCalibration::new()),
        &context,
        super::super::NativeSubmitSpawner,
    )
    .expect("an encoder on the default device");
    enc.program_cache = programs();
    let (vs, ps) = source_records();
    let pair = |enc: &mut FrameEncoder| {
        enc.pair_backend(
            VsSourceView::Programmable(&vs),
            PsSourceView::Programmable(&ps),
            &VariantKey::default(),
        )
    };
    assert_eq!(pair(&mut enc), ShaderBackend::Air);
    let (snapshot, vertex_attrs) = match pipeline_job(None).job {
        CompileJob::Pipeline(job) => (job.snapshot, job.vertex_attrs),
        CompileJob::Library(_) => unreachable!("pipeline_job queues a pipeline"),
    };
    let shader = PairShaderId {
        is_programmable: true,
        hash: 1,
    };
    enc.install_pipeline(PipelineOutcome {
        key: pipeline_state::key_from_snapshot(&snapshot, &vertex_attrs),
        snapshot,
        vs: shader,
        ps: shader,
        air_programs: Some((vs.vs_id, ps.ps_id)),
        sibling_of: None,
        handle: None,
        total_ns: 0,
        persist_ns: 0,
        native: PipelineTimings::new(),
        persist_error: None,
    });
    assert_eq!(
        pair(&mut enc),
        ShaderBackend::Msl,
        "a failed AIR pipeline demotes"
    );

    // A library: a fresh rule, then the PS's AIR library fails.
    enc.pair_rule = libraries::PairRule::new(ShaderBackend::Air);
    assert_eq!(pair(&mut enc), ShaderBackend::Air);
    enc.install_library(LibraryOutcome {
        input: LibraryInput::ProgrammablePs {
            ps_id: ps.ps_id,
            program: Arc::clone(&enc.program_cache[&ps.ps_id]),
            variant: VariantKey::default(),
            backend: ShaderBackend::Air,
        },
        reference: ShaderRecordRef::new(CachedKind::AirSm2Ps, 1),
        handles: None,
        emitted: false,
        bucket: None,
        elapsed: Duration::ZERO,
        total_ns: 0,
        emit_ns: 0,
        persist_ns: 0,
        native: ShaderTimings::new(),
        persist_error: None,
    });
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

/// An AIR pair's recipe is written only when the job has the build's key; other pairs' always.
#[test]
fn an_air_recipe_is_not_written_without_a_build_key() {
    let air = Some((
        ProgramId::from_shader_reply(1),
        ProgramId::from_shader_reply(2),
    ));
    assert!(!recipe_persists(air, None));
    assert!(recipe_persists(air, Some([7u8; 32])));
    assert!(recipe_persists(None, None));
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
