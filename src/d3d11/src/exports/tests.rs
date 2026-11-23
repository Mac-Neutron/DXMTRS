use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicBool, Ordering},
};

use dxmt_util::{
    com::{E_INVALIDARG, E_NOINTERFACE, Hresult, S_OK},
    dxgi_vtbl::IDXGIObjectVtbl,
};

use super::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_NULL, D3D_DRIVER_TYPE_REFERENCE,
    D3D_DRIVER_TYPE_SOFTWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_DRIVER_TYPE_WARP, d3d11_create_device,
    d3d11_create_device_and_swap_chain, validate_driver_type,
};
use crate::abi::records::{DxgiModeDesc, DxgiRational, DxgiSampleDesc, DxgiSwapChainDesc};

#[test]
fn validate_driver_type_matches_d3d11_rules() {
    // Null adapter: UNKNOWN, SOFTWARE with null module, and out-of-range fail.
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_UNKNOWN, false),
        E_INVALIDARG
    );
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_SOFTWARE, false),
        E_INVALIDARG
    );
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_WARP + 1, false),
        E_INVALIDARG
    );
    assert_eq!(validate_driver_type(false, u32::MAX, false), E_INVALIDARG);

    // Null adapter: HARDWARE, REFERENCE, NULL, SOFTWARE with module, and WARP are accepted.
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_HARDWARE, false),
        S_OK
    );
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_REFERENCE, false),
        S_OK
    );
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_NULL, false),
        S_OK
    );
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_SOFTWARE, true),
        S_OK
    );
    assert_eq!(
        validate_driver_type(false, D3D_DRIVER_TYPE_WARP, false),
        S_OK
    );

    // Non-null adapter: only UNKNOWN with null software is valid.
    assert_eq!(
        validate_driver_type(true, D3D_DRIVER_TYPE_UNKNOWN, false),
        S_OK
    );
    assert_eq!(
        validate_driver_type(true, D3D_DRIVER_TYPE_UNKNOWN, true),
        E_INVALIDARG
    );
    assert_eq!(
        validate_driver_type(true, D3D_DRIVER_TYPE_HARDWARE, false),
        E_INVALIDARG
    );
    assert_eq!(
        validate_driver_type(true, D3D_DRIVER_TYPE_WARP, false),
        E_INVALIDARG
    );
}

#[test]
fn create_device_rejects_invalid_driver_types() {
    let fake_adapter = ptr::without_provenance_mut(0x1000);
    let fake_software = ptr::without_provenance_mut(0x2000);
    let dummy_sc_desc = DxgiSwapChainDesc {
        buffer_desc: DxgiModeDesc {
            width: 64,
            height: 64,
            refresh_rate: DxgiRational {
                numerator: 60,
                denominator: 1,
            },
            format: 28,
            scanline_ordering: 0,
            scaling: 0,
        },
        sample_desc: DxgiSampleDesc {
            count: 1,
            quality: 0,
        },
        buffer_usage: 0x20,
        buffer_count: 2,
        output_window: ptr::null_mut(),
        windowed: 1,
        swap_effect: 0,
        flags: 0,
    };

    let invalid_cases = [
        (ptr::null_mut(), D3D_DRIVER_TYPE_UNKNOWN, ptr::null_mut()),
        (ptr::null_mut(), D3D_DRIVER_TYPE_SOFTWARE, ptr::null_mut()),
        (ptr::null_mut(), D3D_DRIVER_TYPE_WARP + 1, ptr::null_mut()),
        (ptr::null_mut(), 99, ptr::null_mut()),
        (fake_adapter, D3D_DRIVER_TYPE_HARDWARE, ptr::null_mut()),
        (fake_adapter, D3D_DRIVER_TYPE_REFERENCE, ptr::null_mut()),
        (fake_adapter, D3D_DRIVER_TYPE_UNKNOWN, fake_software),
    ];

    for (adapter, driver_type, software) in invalid_cases {
        let mut dev: *mut c_void = ptr::without_provenance_mut(0xDEAD);
        let mut ctx: *mut c_void = ptr::without_provenance_mut(0xBEEF);
        let mut fl = 0x1234_u32;
        // SAFETY: all out-pointers are valid locals and invalid driver-type args reject early.
        let hr = unsafe {
            d3d11_create_device(
                adapter,
                driver_type,
                software,
                0,
                ptr::null(),
                0,
                7,
                &raw mut dev,
                &raw mut fl,
                &raw mut ctx,
            )
        };
        assert_eq!(hr, E_INVALIDARG);
        assert!(dev.is_null());
        assert!(ctx.is_null());
        assert_eq!(fl, 0);

        let mut sc: *mut c_void = ptr::without_provenance_mut(0xCAFE);
        dev = ptr::without_provenance_mut(0xDEAD);
        ctx = ptr::without_provenance_mut(0xBEEF);
        fl = 0x1234;
        // SAFETY: all out-pointers and `dummy_sc_desc` are valid locals.
        let hr_sc = unsafe {
            d3d11_create_device_and_swap_chain(
                adapter,
                driver_type,
                software,
                0,
                ptr::null(),
                0,
                7,
                &raw const dummy_sc_desc,
                &raw mut sc,
                &raw mut dev,
                &raw mut fl,
                &raw mut ctx,
            )
        };
        assert_eq!(hr_sc, E_INVALIDARG);
        assert!(sc.is_null());
        assert!(dev.is_null());
        assert!(ctx.is_null());
        assert_eq!(fl, 0);
    }
}

#[test]
fn non_null_adapter_with_unknown_driver_type_reaches_adapter_parent_check() {
    static CALLED_GET_PARENT: AtomicBool = AtomicBool::new(false);

    unsafe extern "system-unwind" fn fake_qi(
        _this: *mut c_void,
        _riid: *const c_void,
        out: *mut *mut c_void,
    ) -> Hresult {
        if !out.is_null() {
            // SAFETY: `out` was checked non-null.
            unsafe { *out = ptr::null_mut() };
        }
        E_INVALIDARG
    }

    const unsafe extern "system-unwind" fn fake_add_ref(_this: *mut c_void) -> u32 {
        2
    }

    const unsafe extern "system-unwind" fn fake_release(_this: *mut c_void) -> u32 {
        1
    }

    const unsafe extern "system-unwind" fn fake_noop_set(
        _this: *mut c_void,
        _guid: *const c_void,
        _size: u32,
        _data: *const c_void,
    ) -> Hresult {
        S_OK
    }

    const unsafe extern "system-unwind" fn fake_noop_set_iface(
        _this: *mut c_void,
        _guid: *const c_void,
        _obj: *const c_void,
    ) -> Hresult {
        S_OK
    }

    const unsafe extern "system-unwind" fn fake_noop_get(
        _this: *mut c_void,
        _guid: *const c_void,
        _size: *mut u32,
        _data: *mut c_void,
    ) -> Hresult {
        E_INVALIDARG
    }

    unsafe extern "system-unwind" fn fake_get_parent(
        _this: *mut c_void,
        _riid: *const c_void,
        out: *mut *mut c_void,
    ) -> Hresult {
        CALLED_GET_PARENT.store(true, Ordering::SeqCst);
        if !out.is_null() {
            // SAFETY: `out` was checked non-null.
            unsafe { *out = ptr::null_mut() };
        }
        E_NOINTERFACE
    }

    static FAKE_ADAPTER_VTBL: IDXGIObjectVtbl = IDXGIObjectVtbl {
        query_interface: fake_qi,
        add_ref: fake_add_ref,
        release: fake_release,
        set_private_data: fake_noop_set,
        set_private_data_interface: fake_noop_set_iface,
        get_private_data: fake_noop_get,
        get_parent: fake_get_parent,
    };

    #[repr(C)]
    struct FakeAdapter {
        vtbl: *const IDXGIObjectVtbl,
    }

    let mut adapter = FakeAdapter {
        vtbl: &raw const FAKE_ADAPTER_VTBL,
    };
    CALLED_GET_PARENT.store(false, Ordering::SeqCst);
    let mut dev: *mut c_void = ptr::null_mut();
    // SAFETY: `adapter` has a valid `IDXGIObjectVtbl` and `dev` is a writable local.
    let hr = unsafe {
        d3d11_create_device(
            (&raw mut adapter).cast::<c_void>(),
            D3D_DRIVER_TYPE_UNKNOWN,
            ptr::null_mut(),
            0,
            ptr::null(),
            0,
            7,
            &raw mut dev,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr, E_INVALIDARG);
    assert!(dev.is_null());
    assert!(CALLED_GET_PARENT.load(Ordering::SeqCst));
}
