use std::sync::PoisonError;

use dxmt_fakes as _;
use winemetal_sys::Obj;

use super::{CoreDevice, CoreOptions, queue_profile, queue_profile_for, select_device};
use crate::rt::device::fixture::{
    FAKE_GLOBAL_LOCK, capture_logs, ff_binds, key, returns_within_10s, triangle,
};

/// A 32-bit process's rings place 1 MiB blocks; a 64-bit one keeps D3D11's 4 MiB.
#[test]
fn queue_profile_places_1mib_blocks_on_32_bit() {
    let narrow = queue_profile_for(32);
    assert_eq!(narrow.staging_block, 0x10_0000);
    assert_eq!(narrow.gpu_heap, 0x10_0000);
    assert_eq!(narrow.chunks, 32);
    let wide = queue_profile_for(64);
    assert_eq!(wide.staging_block, 0x40_0000);
    assert_eq!(wide.gpu_heap, 0x40_0000);
    assert_eq!(queue_profile().staging_block, wide.staging_block);
}

/// The profile a core device is made with reaches its queue's rings.
#[test]
fn core_device_comes_up_on_the_fakes() {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { Obj::adopt(0xd9) }.expect("a non-zero handle");
    let core = CoreDevice::new(mtl, queue_profile_for(32));
    let queue = core.device().queue();
    assert_eq!(queue.staging_ring_config().block, 0x10_0000);
    assert_eq!(queue.config().gpu_heap, 0x10_0000);
    assert_eq!(queue.config().chunks, 32);
    assert_eq!(core.device().device().handle(), 0xd9);
}

/// The device whose registry ID `GetDeviceInfo` reported; the first one when none has it.
#[test]
fn select_device_takes_the_reported_registry_id() {
    let second = dxmt_fakes::FAKE_DEVICES[1];
    let picked = select_device(dxmt_fakes::fake_registry_id(second)).expect("a device");
    assert_eq!(picked.handle(), second);
    let fallback = select_device(0x5eed).expect("a device");
    assert_eq!(fallback.handle(), dxmt_fakes::FAKE_DEVICES[0]);
}

#[test]
fn command_buffer_error_latches_core_device_has_error() {
    crate::rt::device::fixture::returns_within_10s(|| {
        const DEVICE: u64 = 0x520;
        let mut core = crate::rt::device::fixture::core_on_fakes(DEVICE);
        assert!(!core.has_error());

        core.clear(0, [0.0, 0.0, 0.0, 1.0], false);
        core.drain();
        assert!(!core.has_error());

        dxmt_fakes::fail_next_command_buffer(DEVICE);
        core.clear(0, [0.0, 0.0, 0.0, 1.0], false);
        core.drain();
        assert!(core.has_error());

        core.clear(0, [0.0, 0.0, 0.0, 1.0], false);
        core.present(true, 0.0, 0.0, true);
        core.drain();
        assert!(core.has_error());
    });
}

#[test]
fn shader_cache_prewarm_populates_pipelines_and_avoids_compile_miss() {
    let _guard = FAKE_GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    dxmt_core::shader_cache::close_all();
    dxmt_fakes::reset_shader_cache();

    let k1 = key(0xf);
    let k2 = key(0x7);
    let binds = ff_binds();

    // Session 1 populates SQLite stage cache and flushes D3D9REC1 recipes on Drop.
    {
        // SAFETY: made-up handle for dxmt-fakes.
        let mtl1 = unsafe { Obj::adopt(0x521) }.expect("non-zero handle");
        let mut core1 = CoreDevice::with_options(mtl1, queue_profile(), CoreOptions::SHADER_CACHE);
        core1.attach(1, 64, 32, 1);
        assert_eq!(core1.draw(&k1, &triangle(&binds)), Ok(()));
        assert_eq!(core1.draw(&k2, &triangle(&binds)), Ok(()));
        core1.drain();
    }
    dxmt_core::shader_cache::close_all();
    let _ = dxmt_fakes::take_calls(0x522);

    // Session 2 loads the 2 saved recipes, prewarms both pipelines, and draws with zero PSO builds.
    let ((), lines) = capture_logs(|| {
        // SAFETY: made-up handle for dxmt-fakes.
        let mtl2 = unsafe { Obj::adopt(0x522) }.expect("non-zero handle");
        let mut core2 = CoreDevice::with_options(mtl2, queue_profile(), CoreOptions::SHADER_CACHE);
        core2.attach(1, 64, 32, 1);
        core2.wait_prewarm();
        assert_eq!(core2.pipelines_len(), 2);

        let prewarm_calls = dxmt_fakes::take_calls(0x522);
        let prewarm_psos = prewarm_calls
            .iter()
            .filter(|c| *c == "MTLDevice_newRenderPipelineStateWithVertexDescriptor")
            .count();
        assert_eq!(prewarm_psos, 2, "{prewarm_calls:#?}");

        assert_eq!(core2.draw(&k1, &triangle(&binds)), Ok(()));
        assert_eq!(core2.draw(&k2, &triangle(&binds)), Ok(()));
        core2.drain();

        let draw_calls = dxmt_fakes::take_calls(0x522);
        let draw_psos = draw_calls
            .iter()
            .filter(|c| *c == "MTLDevice_newRenderPipelineStateWithVertexDescriptor")
            .count();
        assert_eq!(draw_psos, 0, "{draw_calls:#?}");
    });

    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("d3d9 core: shader cache opened (metal ")),
        "{lines:#?}"
    );
    let summary = lines
        .iter()
        .find(|l| l.starts_with("d3d9 core: shader cache summary: "))
        .expect("summary log on drop");
    assert!(summary.contains("recipes_loaded=2 "), "{summary}");
    assert!(summary.contains("prewarmed=2"), "{summary}");
}

#[test]
fn core_device_drop_drains_cancels_prewarm_and_logs_summary() {
    let _guard = FAKE_GLOBAL_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    dxmt_core::shader_cache::close_all();
    dxmt_fakes::reset_shader_cache();

    returns_within_10s(|| {
        let ((), lines) = capture_logs(|| {
            // SAFETY: made-up handle for dxmt-fakes.
            let mtl = unsafe { Obj::adopt(0x523) }.expect("non-zero handle");
            let opts = CoreOptions::SHADER_CACHE
                | CoreOptions::ASYNC_COMPILE
                | CoreOptions::DISCARD_BACK_BUFFER;
            let mut core = CoreDevice::with_options(mtl, queue_profile(), opts);
            core.attach(1, 64, 32, 1);
            let binds = ff_binds();
            core.clear(0, [0.0, 0.0, 0.0, 1.0], false);
            assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
            assert_eq!(core.draw(&key(0x7), &triangle(&binds)), Ok(()));
            // Drop immediately without manual drain to verify ordered teardown.
        });
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("d3d9 core: shader cache opened (metal ")),
            "{lines:#?}"
        );
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("d3d9 core: shader cache summary: ")),
            "{lines:#?}"
        );
    });
}
