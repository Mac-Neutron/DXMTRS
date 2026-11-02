//! Unit tests for `Device`, `DxgiDevice`, feature-level selection, and drop ordering.

use core::{ffi::c_void, ptr};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use dxmt_core::device::Device as CoreDevice;
use dxmt_util::com::{
    E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, IID_IUNKNOWN, IUnknownVtbl, S_OK, qi_log, release,
};
use winemetal_sys::{
    Obj,
    wire::{
        WMT_COMMAND_BUFFER_STATUS_COMPLETED, WMTGPU_FAMILY_APPLE7, WMTGPU_FAMILY_APPLE8,
        WMTSamplerInfo,
    },
};

use super::{
    D3D11_CREATE_DEVICE_D3D10_INTERNAL, DEFAULT_FEATURE_LEVELS, DxgiDevice, create_with_core,
    max_feature_level_from_config, select_feature_level,
};
use crate::abi::{
    IID_ID3D10_DEVICE, IID_ID3D10_DEVICE1, IID_ID3D10_MULTITHREAD, IID_ID3D11_DEVICE,
    IID_ID3D11_DEVICE_1, IID_ID3D11_DEVICE_2, IID_ID3D11_DEVICE_3, IID_ID3D11_DEVICE_4,
    IID_ID3D11_DEVICE_5, IID_ID3D11_MULTITHREAD, IID_IDXGI_DEVICE, IID_IDXGI_DEVICE_1,
    IID_IDXGI_DEVICE_2, IID_IDXGI_DEVICE_3, IID_IDXGI_OBJECT,
    ext::{
        IID_D3D11_CONTEXT_EXT, IID_D3D11_DEVICE_EXT, IID_IMTL_DXGI_DEVICE,
        IID_IMTL_SWAP_CHAIN_FACTORY,
    },
    records::D3d11BufferDesc,
    vtbl::ID3D11Device5Vtbl,
};

#[unsafe(export_name = "NSAutoreleasePool_alloc_init")]
extern "C-unwind" fn fake_autorelease_pool() -> u64 {
    0x7001
}

#[unsafe(export_name = "WMTGetOSVersion")]
extern "C-unwind" fn fake_get_os_version(
    major_out: *mut u64,
    minor_out: *mut u64,
    patch_out: *mut u64,
) {
    // SAFETY: `CoreDevice::new` passes a valid `major_out` pointer.
    unsafe {
        *major_out = 15;
    }
    // SAFETY: `CoreDevice::new` passes a valid `minor_out` pointer.
    unsafe {
        *minor_out = 0;
    }
    // SAFETY: `CoreDevice::new` passes a valid `patch_out` pointer.
    unsafe {
        *patch_out = 0;
    }
}

#[unsafe(export_name = "MTLDevice_supportsFamily")]
extern "C-unwind" fn fake_supports_family(_device: u64, family: u32) -> bool {
    matches!(family, WMTGPU_FAMILY_APPLE7 | WMTGPU_FAMILY_APPLE8)
}

#[unsafe(export_name = "MTLDevice_supportsBCTextureCompression")]
extern "C-unwind" fn fake_supports_bc(_device: u64) -> bool {
    true
}

#[unsafe(export_name = "MTLDevice_hasUnifiedMemory")]
extern "C-unwind" fn fake_has_unified_memory(_device: u64) -> bool {
    true
}

#[unsafe(export_name = "MTLDevice_supportsTextureSampleCount")]
extern "C-unwind" fn fake_supports_sample_count(_device: u64, sample_count: u8) -> bool {
    matches!(sample_count, 1 | 2 | 4 | 8)
}

#[unsafe(export_name = "MTLSharedEvent_signaledValue")]
extern "C-unwind" fn fake_shared_event_signaled_value(_event: u64) -> u64 {
    0
}

#[unsafe(export_name = "MTLDevice_setShouldMaximizeConcurrentCompilation")]
extern "C-unwind" fn fake_set_maximize_concurrent(_device: u64, _value: bool) {}

#[unsafe(export_name = "MTLDevice_newCommandQueue")]
extern "C-unwind" fn fake_new_command_queue(_device: u64, _max_cmdbufs: u64) -> u64 {
    0x7100
}

#[unsafe(export_name = "MTLDevice_newSharedEvent")]
extern "C-unwind" fn fake_new_shared_event(_device: u64) -> u64 {
    0x7101
}

#[unsafe(export_name = "MTLDevice_newEvent")]
extern "C-unwind" fn fake_new_event(_device: u64) -> u64 {
    0x7102
}

#[unsafe(export_name = "MTLDevice_newSamplerState")]
extern "C-unwind" fn fake_new_sampler_state(_device: u64, info: *mut WMTSamplerInfo) -> u64 {
    // SAFETY: `Encoder::new` passes a valid local `WMTSamplerInfo`.
    unsafe { (*info).gpu_resource_id = 0x7200 };
    0x7201
}

#[unsafe(export_name = "MTLDevice_newFence")]
extern "C-unwind" fn fake_new_fence(_device: u64) -> u64 {
    0x7300
}

#[unsafe(export_name = "MTLCommandQueue_commandBuffer")]
extern "C-unwind" fn fake_command_buffer(_queue: u64) -> u64 {
    0x7500
}

#[unsafe(export_name = "MTLCommandBuffer_encodeSignalEvent")]
extern "C-unwind" fn fake_encode_signal_event(_cmdbuf: u64, _event: u64, _value: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_encodeWaitForEvent")]
extern "C-unwind" fn fake_encode_wait_for_event(_cmdbuf: u64, _event: u64, _value: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_commit")]
extern "C-unwind" fn fake_commit(_cmdbuf: u64) {}

#[unsafe(export_name = "MTLCommandBuffer_status")]
extern "C-unwind" fn fake_cmdbuf_status(_cmdbuf: u64) -> u64 {
    WMT_COMMAND_BUFFER_STATUS_COMPLETED
}

#[unsafe(export_name = "MTLCommandBuffer_waitUntilCompleted")]
extern "C-unwind" fn fake_cmdbuf_wait(_cmdbuf: u64) {}

#[unsafe(export_name = "SharedEventListener_create")]
extern "C-unwind" fn fake_listener_create() -> u64 {
    1
}

#[unsafe(export_name = "SharedEventListener_start")]
extern "C-unwind" fn fake_listener_start(_listener: u64) {}

#[unsafe(export_name = "SharedEventListener_destroy")]
extern "C-unwind" fn fake_listener_destroy(_listener: u64) {}

fn new_test_device(feature_level: u32, flags: u32) -> *mut DxgiDevice {
    // SAFETY: `0xD1` is a non-zero test handle backed by the fake `NSObject_retain`/`release`.
    let mtl = unsafe { Obj::adopt(0xD1) }.expect("non-zero handle");
    let core = CoreDevice::new(mtl, 0);
    create_with_core(ptr::null_mut(), 0, core, feature_level, flags)
}

#[test]
fn feature_level_from_config() {
    assert_eq!(max_feature_level_from_config("", false), 0xb000);
    assert_eq!(max_feature_level_from_config("", true), 0xb100);
    assert_eq!(max_feature_level_from_config("12_1", true), 0xc100);
    assert_eq!(max_feature_level_from_config("12_0", true), 0xc000);
    assert_eq!(max_feature_level_from_config("11_1", false), 0xb100);
    assert_eq!(max_feature_level_from_config("11_0", true), 0xb000);
    assert_eq!(max_feature_level_from_config("10_1", true), 0xa100);
    assert_eq!(max_feature_level_from_config("10_0", true), 0xa000);
    assert_eq!(max_feature_level_from_config("9_3", true), 0x9300);
    assert_eq!(max_feature_level_from_config("9_2", true), 0x9200);
    assert_eq!(max_feature_level_from_config("9_1", true), 0x9100);
    assert_eq!(max_feature_level_from_config("bogus", false), 0xb000);
    assert_eq!(max_feature_level_from_config("bogus", true), 0xb100);

    let max_fl = max_feature_level_from_config("10_1", true);
    assert_eq!(
        select_feature_level(&DEFAULT_FEATURE_LEVELS, max_fl),
        Some(0xa100)
    );
    assert_eq!(select_feature_level(&[0xffff], max_fl), None);
}

#[test]
fn apple7_allows_11_1() {
    let max_no_a7 = max_feature_level_from_config("", false);
    assert_eq!(select_feature_level(&[0xb100], max_no_a7), None);
    assert_eq!(
        select_feature_level(&[0xb100, 0xb000], max_no_a7),
        Some(0xb000)
    );

    let max_a7 = max_feature_level_from_config("", true);
    assert_eq!(
        select_feature_level(&[0xb100, 0xb000], max_a7),
        Some(0xb100)
    );
    assert_eq!(
        select_feature_level(&DEFAULT_FEATURE_LEVELS, max_a7),
        Some(0xb000)
    );
}

#[test]
fn d3d10_flag_selects_d3d10() {
    let raw_10 = new_test_device(0xb000, D3D11_CREATE_DEVICE_D3D10_INTERNAL | 0x20);
    // SAFETY: `raw_10` is a valid `DxgiDevice` pointer.
    let dev_10 = unsafe { &mut (*raw_10).d3d11 };
    assert_eq!(dev_10.direct_x_version(), 10);
    let iface_10 = ptr::from_mut(dev_10).cast::<c_void>();
    // SAFETY: `dev_10.vtbl` is a valid `ID3D11Device5Vtbl` pointer.
    let get_flags_10 = unsafe { (*dev_10.vtbl).get_creation_flags };
    // SAFETY: Calling `get_creation_flags` on valid `Device` pointer.
    let flags_10 = unsafe { get_flags_10(iface_10) };
    assert_eq!(flags_10, 0x20);
    // SAFETY: Releasing the initial ref on `raw_10`.
    unsafe {
        release(raw_10.cast::<c_void>());
    }

    let raw_11 = new_test_device(0xb000, 0x20);
    // SAFETY: `raw_11` is a valid `DxgiDevice` pointer.
    let dev_11 = unsafe { &mut (*raw_11).d3d11 };
    assert_eq!(dev_11.direct_x_version(), 11);
    let iface_11 = ptr::from_mut(dev_11).cast::<c_void>();
    // SAFETY: `dev_11.vtbl` is a valid `ID3D11Device5Vtbl` pointer.
    let get_flags_11 = unsafe { (*dev_11.vtbl).get_creation_flags };
    // SAFETY: Calling `get_creation_flags` on valid `Device` pointer.
    let flags_11 = unsafe { get_flags_11(iface_11) };
    assert_eq!(flags_11, 0x20);
    // SAFETY: Releasing the initial ref on `raw_11`.
    unsafe {
        release(raw_11.cast::<c_void>());
    }
}

#[test]
fn qi_set_as_cpp() {
    let raw = new_test_device(0xb000, 0);
    let root_iface = raw.cast::<c_void>();
    // SAFETY: `raw` is a valid `DxgiDevice` pointer.
    let d3d11_iface = unsafe { (&raw mut (*raw).d3d11).cast::<c_void>() };
    // SAFETY: `d3d11_iface` is a valid `Device` pointer.
    let d3d11_vtbl_ptr = unsafe { *d3d11_iface.cast::<*const ID3D11Device5Vtbl>() };
    // SAFETY: `d3d11_vtbl_ptr` points to the static `ID3D11Device5Vtbl`.
    let d3d11_vtbl = unsafe { &*d3d11_vtbl_ptr };

    // Null output pointer -> E_POINTER.
    // SAFETY: Testing `query_interface` with null `out`.
    let hr_null = unsafe {
        (d3d11_vtbl.query_interface)(
            d3d11_iface,
            ptr::from_ref(&IID_IUNKNOWN).cast(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_null, E_POINTER);

    let positive_iids: &[Guid] = &[
        IID_IUNKNOWN,
        IID_ID3D11_DEVICE,
        IID_ID3D11_DEVICE_1,
        IID_ID3D11_DEVICE_2,
        IID_ID3D11_DEVICE_3,
        IID_ID3D11_DEVICE_4,
        IID_ID3D11_DEVICE_5,
        IID_IDXGI_OBJECT,
        IID_IDXGI_DEVICE,
        IID_IDXGI_DEVICE_1,
        IID_IDXGI_DEVICE_2,
        IID_IDXGI_DEVICE_3,
        IID_IMTL_DXGI_DEVICE,
        IID_D3D11_DEVICE_EXT,
        IID_IMTL_SWAP_CHAIN_FACTORY,
        IID_ID3D11_MULTITHREAD,
        IID_ID3D10_MULTITHREAD,
        IID_ID3D10_DEVICE,
        IID_ID3D10_DEVICE1,
    ];

    for iid in positive_iids {
        let mut out: *mut c_void = ptr::null_mut();
        // SAFETY: Querying a positive IID on `d3d11_iface`.
        let hr = unsafe {
            (d3d11_vtbl.query_interface)(d3d11_iface, ptr::from_ref(iid).cast(), &raw mut out)
        };
        assert_eq!(hr, S_OK, "expected S_OK for positive IID {iid}");
        assert!(!out.is_null(), "expected non-null pointer for IID {iid}");

        // Verify COM identity: querying IID_IUnknown on `out` must return `root_iface`.
        let mut unk: *mut c_void = ptr::null_mut();
        // SAFETY: `out` is a valid COM interface pointer with an `IUnknownVtbl` header.
        let out_vtbl = unsafe { *out.cast::<*const IUnknownVtbl>() };
        // SAFETY: `out_vtbl` is a valid `IUnknownVtbl`.
        let qi = unsafe { (*out_vtbl).query_interface };
        // SAFETY: Querying `IID_IUNKNOWN` on `out`.
        let hr_unk = unsafe { qi(out, ptr::from_ref(&IID_IUNKNOWN), &raw mut unk) };
        assert_eq!(hr_unk, S_OK);
        assert_eq!(unk, root_iface, "COM identity mismatch for IID {iid}");

        // SAFETY: Releasing `unk`.
        unsafe {
            release(unk);
        }
        // SAFETY: `out_vtbl` is a valid `IUnknownVtbl`.
        let rel = unsafe { (*out_vtbl).release };
        // SAFETY: Releasing `out` and checking the remaining reference count.
        let rc = unsafe { rel(out) };
        assert_eq!(rc, 1, "refcount imbalance after releasing IID {iid}");
    }

    let iid_dxgi_device4 = Guid::new(
        0x95b4_f95f,
        0xd8da,
        0x4ca4,
        [0x9e, 0xe6, 0x3b, 0x76, 0xd5, 0x96, 0x8a, 0x10],
    );
    let iid_bogus = Guid::new(
        0xdead_beef,
        0x0000,
        0x1111,
        [0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55],
    );
    // Pre-register the non-silent negative IIDs with `qi_log` so host tests do not write `_d3d11.log`.
    let _ = qi_log(&IID_IMTL_DXGI_DEVICE, &iid_dxgi_device4);
    let _ = qi_log(&IID_IMTL_DXGI_DEVICE, &iid_bogus);
    let negative_iids: &[Guid] = &[
        iid_dxgi_device4,
        super::IID_ID3D11_DEBUG,
        super::IID_ID3D11_INFO_QUEUE,
        IID_D3D11_CONTEXT_EXT,
        super::IID_RENDERDOC,
        super::IID_PIX,
        super::IID_GPA,
        iid_bogus,
    ];

    for iid in negative_iids {
        let mut out: *mut c_void = ptr::without_provenance_mut(0x1234);
        // SAFETY: Querying a negative IID on `d3d11_iface`.
        let hr = unsafe {
            (d3d11_vtbl.query_interface)(d3d11_iface, ptr::from_ref(iid).cast(), &raw mut out)
        };
        assert_eq!(hr, E_NOINTERFACE, "expected E_NOINTERFACE for IID {iid}");
        assert!(out.is_null(), "expected null out pointer for IID {iid}");
    }

    // SAFETY: Calling `release` on `d3d11_iface` to release the final reference.
    let final_rc = unsafe { (d3d11_vtbl.release)(d3d11_iface) };
    assert_eq!(final_rc, 0);
}

#[test]
fn drop_drains_before_caches() {
    let raw = new_test_device(0xb000, 0);
    let chunk_executed = Arc::new(AtomicBool::new(false));
    let drained_before_caches = Arc::new(AtomicBool::new(false));

    // SAFETY: `raw` is a valid `DxgiDevice` pointer uniquely owned by this test.
    let dev = unsafe { &mut (*raw).d3d11 };
    let target_seq = dev.core().queue().current_seq();

    let chunk_executed_worker = Arc::clone(&chunk_executed);
    dev.core_mut().queue_mut().chunk().emit(move |_| {
        chunk_executed_worker.store(true, Ordering::SeqCst);
    });

    let chunk_executed_check = Arc::clone(&chunk_executed);
    let drained_flag = Arc::clone(&drained_before_caches);
    dev.on_drop_after_drain = Some(Box::new(move |core, _caches| {
        assert!(
            chunk_executed_check.load(Ordering::SeqCst),
            "command queue chunk must execute before StateCaches drop"
        );
        assert!(
            core.queue().coherent_seq() >= target_seq,
            "command queue coherent_seq must reach target_seq before StateCaches drop"
        );
        drained_flag.store(true, Ordering::SeqCst);
    }));

    // SAFETY: Releasing the sole reference drops `DxgiDevice` and runs `Device::drop`.
    unsafe {
        release(raw.cast::<c_void>());
    }
    assert!(drained_before_caches.load(Ordering::SeqCst));
}

#[test]
fn invalid_usage_is_invalid_arg() {
    let raw = new_test_device(0xb000, 0);
    // SAFETY: `raw` is a valid `DxgiDevice` pointer.
    let d3d11_iface = unsafe { (&raw mut (*raw).d3d11).cast::<c_void>() };
    // SAFETY: `d3d11_iface` is a valid `Device` pointer.
    let vtbl_ptr = unsafe { *d3d11_iface.cast::<*const ID3D11Device5Vtbl>() };
    // SAFETY: `vtbl_ptr` points to the static `ID3D11Device5Vtbl`.
    let vtbl = unsafe { &*vtbl_ptr };

    let bad_desc = D3d11BufferDesc {
        byte_width: 64,
        usage: 99,
        bind_flags: 1,
        cpu_access_flags: 0,
        misc_flags: 0,
        structure_byte_stride: 0,
    };
    let mut out_buf: *mut c_void = ptr::without_provenance_mut(0x5678);
    // SAFETY: Calling `create_buffer` through the `ID3D11Device5` vtable with invalid `usage`.
    let hr = unsafe {
        (vtbl.create_buffer)(
            d3d11_iface,
            (&raw const bad_desc).cast(),
            ptr::null(),
            &raw mut out_buf,
        )
    };
    assert_eq!(hr, E_INVALIDARG);
    assert!(out_buf.is_null());

    // SAFETY: Releasing the device.
    unsafe {
        release(raw.cast::<c_void>());
    }
}
