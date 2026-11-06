use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use super::{
    ComObject, E_NOINTERFACE, Guid, Hresult, IID_IUNKNOWN, IUnknownVtbl, Managed, S_OK, TearOff,
    container_of, iface_of, write_out,
};

#[repr(C)]
struct FakeDevice {
    vtbl: *const IUnknownVtbl,
    refs: AtomicU32,
}

impl FakeDevice {
    const fn new() -> Self {
        Self {
            vtbl: &raw const FAKE_DEVICE_VTBL,
            refs: AtomicU32::new(1),
        }
    }

    const fn as_iface(&self) -> *mut c_void {
        iface_of(&self.vtbl)
    }

    fn count(&self) -> u32 {
        self.refs.load(Ordering::SeqCst)
    }

    unsafe extern "system" fn query_interface(
        this: *mut c_void,
        riid: *const Guid,
        out: *mut *mut c_void,
    ) -> Hresult {
        // SAFETY: `riid` is a valid GUID pointer in tests.
        let Some(iid) = (unsafe { Guid::read(riid) }) else {
            return E_NOINTERFACE;
        };
        if iid == IID_IUNKNOWN {
            // SAFETY: `this` is a `FakeDevice`.
            unsafe { Self::add_ref(this) };
            // SAFETY: `out` is a valid output pointer in tests.
            unsafe { write_out(out, this) };
            return S_OK;
        }
        E_NOINTERFACE
    }

    unsafe extern "system" fn add_ref(this: *mut c_void) -> u32 {
        let dev = container_of!(this, Self, vtbl);
        // SAFETY: `dev` is a live `FakeDevice`.
        let prev = unsafe { (*dev).refs.fetch_add(1, Ordering::SeqCst) };
        prev + 1
    }

    unsafe extern "system" fn release(this: *mut c_void) -> u32 {
        let dev = container_of!(this, Self, vtbl);
        // SAFETY: `dev` is a live `FakeDevice`.
        let prev = unsafe { (*dev).refs.fetch_sub(1, Ordering::SeqCst) };
        prev - 1
    }
}

static FAKE_DEVICE_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: FakeDevice::query_interface,
    add_ref: FakeDevice::add_ref,
    release: FakeDevice::release,
};

#[test]
fn com_object_deletes_at_private_zero() {
    let obj = ComObject::new();
    assert_eq!(obj.public_count(), 0);
    assert_eq!(obj.private_count(), 0);

    // First public ref takes one private ref.
    assert_eq!(obj.add_ref(), 1);
    assert_eq!(obj.public_count(), 1);
    assert_eq!(obj.private_count(), 1);

    // An internal bind holds an extra private ref; releasing the public ref drops public to 0
    // without freeing while the private ref remains.
    obj.add_ref_private();
    assert_eq!(obj.private_count(), 2);
    let (rem, free) = obj.release_and_free();
    assert_eq!(rem, 0);
    assert!(!free);
    assert_eq!(obj.private_count(), 1);

    // Releasing the last private ref signals deletion and sets the destroying guard bit.
    assert!(obj.release_private());
    assert_eq!(obj.private_count(), 0);
}

#[test]
fn managed_zero_is_valid_and_recycles() {
    let dev = FakeDevice::new();
    let managed = Managed::new();
    assert_eq!(managed.public_count(), 0);
    assert_eq!(managed.private_count(), 0);

    // Private 0 -> 1 -> 0 recycles without destroying, and the object can be reused from 0 again.
    managed.add_ref_private();
    assert_eq!(managed.private_count(), 1);
    assert!(managed.release_private());
    assert_eq!(managed.private_count(), 0);

    // Reuse from 0 via public AddRef -> Release.
    // SAFETY: `dev` is a live `FakeDevice`.
    let pub_after_add = unsafe { managed.add_ref(dev.as_iface()) };
    assert_eq!(pub_after_add, 1);
    assert_eq!(managed.private_count(), 1);

    // SAFETY: `dev` is a live `FakeDevice`.
    let (pub_after_rel, recycled) = unsafe { managed.release(dev.as_iface()) };
    assert_eq!(pub_after_rel, 0);
    assert!(recycled);
    assert_eq!(managed.public_count(), 0);
    assert_eq!(managed.private_count(), 0);
}

#[test]
fn first_public_ref_adds_device_ref() {
    let dev = FakeDevice::new();
    let managed = Managed::new();
    assert_eq!(dev.count(), 1);

    // 0 -> 1 public adds one ref to the parent device; 1 -> 2 does not.
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.add_ref(dev.as_iface()) }, 1);
    assert_eq!(dev.count(), 2);
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.add_ref(dev.as_iface()) }, 2);
    assert_eq!(dev.count(), 2);

    // Holding a private ref keeps `recycled == false` when public drops 1 -> 0, while still
    // releasing the parent device's ref on 1 -> 0 public.
    managed.add_ref_private();
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.release(dev.as_iface()) }, (1, false));
    assert_eq!(dev.count(), 2);
    // SAFETY: `dev` is a live `FakeDevice`.
    assert_eq!(unsafe { managed.release(dev.as_iface()) }, (0, false));
    assert_eq!(dev.count(), 1);
    assert_eq!(managed.private_count(), 1);
    assert!(managed.release_private());
}

#[repr(C)]
struct TearOffVtbl {
    query_interface:
        unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> Hresult,
    add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
}

static TEAR_OFF_VTBL: TearOffVtbl = TearOffVtbl {
    query_interface: TearOff::<()>::query_interface,
    add_ref: TearOff::<()>::add_ref,
    release: TearOff::<()>::release,
};

#[test]
fn tear_off_forwards() {
    let dev = FakeDevice::new();
    let tear_off = TearOff::new(&TEAR_OFF_VTBL, dev.as_iface());
    let iface = tear_off.as_iface();

    // SAFETY: `iface` is a live `TearOff`.
    let r1 = unsafe { (TEAR_OFF_VTBL.add_ref)(iface) };
    assert_eq!(r1, 2);
    assert_eq!(dev.count(), 2);

    let iid = IID_IUNKNOWN;
    let mut out = ptr::null_mut();
    // SAFETY: `iface` is a live `TearOff`, `iid` and `out` are valid pointers.
    let hr =
        unsafe { (TEAR_OFF_VTBL.query_interface)(iface, (&raw const iid).cast(), &raw mut out) };
    assert_eq!(hr, S_OK);
    assert_eq!(out, dev.as_iface());
    assert_eq!(dev.count(), 3);

    // SAFETY: `iface` is a live `TearOff`.
    assert_eq!(unsafe { (TEAR_OFF_VTBL.release)(iface) }, 2);
    // SAFETY: `iface` is a live `TearOff`.
    assert_eq!(unsafe { (TEAR_OFF_VTBL.release)(iface) }, 1);
    assert_eq!(dev.count(), 1);
}

// --- Reference counts across AddRef, QueryInterface and Release, one object of each model ---

mod refcount {
    use core::{ffi::c_void, ptr};

    use airconv_ir::container::{build, fourcc};

    use super::FakeDevice;
    use crate::{
        abi::{
            IID_ID3D10_DEVICE_CHILD, IID_ID3D10_INPUT_LAYOUT, IID_ID3D10_RASTERIZER_STATE,
            IID_ID3D11_DEVICE_CHILD, IID_ID3D11_INPUT_LAYOUT, IID_ID3D11_RASTERIZER_STATE_2,
            IID_ID3D11_TEXTURE_2D, IID_IDXGI_KEYED_MUTEX, IID_IDXGI_SURFACE,
            ext::IID_IMTL_MIN_LOD_CLAMPABLE,
            records::{D3d11BufferDesc, D3d11InputElementDesc, D3d11Texture2dDesc1},
            vtbl::{ID3D11ResourceVtbl, IDXGIResource1Vtbl, IDXGISurface2Vtbl},
        },
        com::{E_NOINTERFACE, E_POINTER, Guid, Hresult, IID_IUNKNOWN, IUnknownVtbl, S_OK, vtbl},
        input_layout::create_input_layout,
        resource::{Common, Resource, ResourceDesc, ResourceKind},
        state::{create_rasterizer_state2, default_rasterizer_desc2},
    };

    fn unknown<'a>(obj: *mut c_void) -> &'a IUnknownVtbl {
        // SAFETY: `obj` is a live COM object.
        unsafe { vtbl::<IUnknownVtbl>(obj) }
    }

    /// `QueryInterface(iid)` on `obj`: the answer and the pointer written (null on a miss).
    fn qi(obj: *mut c_void, iid: &Guid) -> (Hresult, *mut c_void) {
        let mut out = ptr::dangling_mut::<c_void>();
        // SAFETY: `obj` is a live COM object; `iid` and `out` are valid.
        let hr = unsafe { (unknown(obj).query_interface)(obj, iid, &raw mut out) };
        (hr, out)
    }

    fn add_ref(obj: *mut c_void) -> u32 {
        // SAFETY: `obj` is a live COM object.
        unsafe { (unknown(obj).add_ref)(obj) }
    }

    fn release(obj: *mut c_void) -> u32 {
        // SAFETY: `obj` is a live COM object the caller holds a reference to.
        unsafe { (unknown(obj).release)(obj) }
    }

    fn null_riid_and_out(obj: *mut c_void, null_riid: Hresult) {
        let mut out = ptr::dangling_mut::<c_void>();
        // SAFETY: a null `riid` is the case under test; `out` is valid.
        let hr = unsafe { (unknown(obj).query_interface)(obj, ptr::null(), &raw mut out) };
        assert_eq!(hr, null_riid);
        assert!(out.is_null());
        let iid = IID_IUNKNOWN;
        // SAFETY: a null `out` is the case under test; `iid` is valid.
        let hr = unsafe { (unknown(obj).query_interface)(obj, &raw const iid, ptr::null_mut()) };
        assert_eq!(hr, E_POINTER);
    }

    fn texture2d(misc_flags: u32) -> *mut Resource {
        Resource::into_raw(
            Common {
                misc_flags,
                ..Common::default()
            },
            ResourceKind::DeviceTexture,
            ResourceDesc::Texture2d(D3d11Texture2dDesc1 {
                array_size: 1,
                mip_levels: 1,
                ..D3d11Texture2dDesc1::default()
            }),
        )
    }

    #[test]
    fn resource_and_its_tear_offs_share_one_count() {
        let res = texture2d(0);
        let obj = res.cast::<c_void>();
        // SAFETY: `res` is live until its last release below.
        let count = || unsafe { (*res).refs.public_count() };
        assert_eq!(count(), 1);
        assert_eq!(add_ref(obj), 2);

        assert_eq!(qi(obj, &IID_IUNKNOWN), (S_OK, obj));
        assert_eq!(count(), 3);
        assert_eq!(qi(obj, &IID_ID3D11_TEXTURE_2D), (S_OK, obj));
        assert_eq!(count(), 4);

        // A one-subresource texture answers the DXGI surface from its tear-off, which counts on the outer.
        // SAFETY: `res` is live.
        let surface = unsafe { (*res).dxgi_surface.as_iface() };
        assert_eq!(qi(obj, &IID_IDXGI_SURFACE), (S_OK, surface));
        assert_eq!(count(), 5);
        assert_eq!(add_ref(surface), 6);
        assert_eq!(count(), 6);
        // The tear-off's own QueryInterface forwards to the outer, so its answer takes the outer's count.
        assert_eq!(qi(surface, &IID_IUNKNOWN), (S_OK, obj));
        assert_eq!(count(), 7);
        for expected in (1..7).rev() {
            assert_eq!(release(surface), expected);
        }
        assert_eq!(count(), 1);

        // The min-LOD tear-off of a device texture.
        // SAFETY: `res` is live.
        let min_lod = unsafe { (&raw const (*res).min_lod_vtbl).cast_mut().cast::<c_void>() };
        assert_eq!(qi(obj, &IID_IMTL_MIN_LOD_CLAMPABLE), (S_OK, min_lod));
        assert_eq!(count(), 2);
        assert_eq!(release(min_lod), 1);

        // Misses take no reference and write null: the keyed mutex without the flag, an unknown IID.
        assert_eq!(
            qi(obj, &IID_IDXGI_KEYED_MUTEX),
            (E_NOINTERFACE, ptr::null_mut())
        );
        let unknown = Guid::new(0x1111_1111, 0x2222, 0x3333, [0x44; 8]);
        assert_eq!(qi(obj, &unknown), (E_NOINTERFACE, ptr::null_mut()));
        assert_eq!(count(), 1);
        null_riid_and_out(obj, E_NOINTERFACE);
        assert_eq!(count(), 1);

        assert_eq!(release(obj), 0);
    }

    #[test]
    fn keyed_mutex_tear_off_counts_on_the_resource() {
        let res = texture2d(0x100);
        let obj = res.cast::<c_void>();
        // SAFETY: `res` is live until its last release below.
        let mutex = unsafe { (*res).keyed_mutex_tearoff.as_iface() };
        assert_eq!(qi(obj, &IID_IDXGI_KEYED_MUTEX), (S_OK, mutex));
        // SAFETY: `res` is live.
        assert_eq!(unsafe { (*res).refs.public_count() }, 2);
        assert_eq!(add_ref(mutex), 3);
        assert_eq!(release(mutex), 2);
        assert_eq!(release(mutex), 1);
        assert_eq!(release(obj), 0);
    }

    #[test]
    fn cached_state_object_lives_at_zero_and_holds_the_device_while_referenced() {
        let dev = FakeDevice::new();
        let dev_ptr = dev.as_iface();
        let mut desc = default_rasterizer_desc2();
        desc.depth_bias = 4321;
        let mut state = ptr::null_mut();
        assert_eq!(
            create_rasterizer_state2(dev_ptr, &raw const desc, &raw mut state),
            S_OK
        );
        assert_eq!(dev.count(), 2);

        // The 0 -> 1 transition took the device; further references do not.
        assert_eq!(qi(state, &IID_ID3D11_RASTERIZER_STATE_2), (S_OK, state));
        assert_eq!(qi(state, &IID_ID3D11_DEVICE_CHILD), (S_OK, state));
        assert_eq!(dev.count(), 2);
        // The D3D10 interface is the embedded wrapper, counted on the same object.
        let (hr, d3d10) = qi(state, &IID_ID3D10_RASTERIZER_STATE);
        assert_eq!(hr, S_OK);
        assert_ne!(d3d10, state);
        assert_eq!(qi(state, &IID_ID3D10_DEVICE_CHILD).1, d3d10);
        assert_eq!(dev.count(), 2);

        // Misses take nothing and write null.
        let unknown = Guid::new(0x1111_1111, 0x2222, 0x3333, [0x44; 8]);
        assert_eq!(qi(state, &unknown), (E_NOINTERFACE, ptr::null_mut()));
        assert_eq!(dev.count(), 2);
        null_riid_and_out(state, crate::com::E_INVALIDARG);

        // 1 (create) + 4 answers: releasing through either interface counts down; the last gives the device back.
        assert_eq!(release(d3d10), 4);
        assert_eq!(release(d3d10), 3);
        assert_eq!(release(state), 2);
        assert_eq!(release(state), 1);
        assert_eq!(dev.count(), 2);
        assert_eq!(release(state), 0);
        assert_eq!(dev.count(), 1);

        // The object stays in the cache at 0: a new create finds it and takes the device again.
        let mut again = ptr::null_mut();
        assert_eq!(
            create_rasterizer_state2(dev_ptr, &raw const desc, &raw mut again),
            S_OK
        );
        assert_eq!(again, state);
        assert_eq!(dev.count(), 2);
        assert_eq!(release(again), 0);
        assert_eq!(dev.count(), 1);
    }

    /// A `VSIA`-less minimum: an `ISGN` of one `POSITION` parameter.
    fn position_signature() -> Vec<u8> {
        let mut blob = Vec::new();
        blob.extend_from_slice(&1u32.to_le_bytes());
        blob.extend_from_slice(&8u32.to_le_bytes());
        blob.extend_from_slice(&32u32.to_le_bytes()); // name offset
        for field in [0u32, 0, 3, 0] {
            blob.extend_from_slice(&field.to_le_bytes());
        }
        blob.extend_from_slice(&[0x7, 0x7, 0, 0]);
        blob.extend_from_slice(b"POSITION\0\0\0\0");
        build(&[(fourcc::ISGN, &blob)]).expect("container fits in 32 bits")
    }

    #[test]
    fn managed_object_takes_the_device_on_the_first_public_reference_only() {
        let dev = FakeDevice::new();
        let dev_ptr = dev.as_iface();
        let element = D3d11InputElementDesc {
            semantic_name: c"POSITION".as_ptr(),
            semantic_index: 0,
            format: 6,
            input_slot: 0,
            aligned_byte_offset: 0,
            input_slot_class: 0,
            instance_data_step_rate: 0,
        };
        let dxbc = position_signature();
        let mut layout = ptr::null_mut();
        assert_eq!(
            create_input_layout(
                dev_ptr,
                &raw const element,
                1,
                dxbc.as_ptr().cast(),
                dxbc.len(),
                &raw mut layout,
            ),
            S_OK
        );
        assert_eq!(dev.count(), 2);

        assert_eq!(qi(layout, &IID_ID3D11_INPUT_LAYOUT), (S_OK, layout));
        assert_eq!(dev.count(), 2);
        let (hr, d3d10) = qi(layout, &IID_ID3D10_INPUT_LAYOUT);
        assert_eq!(hr, S_OK);
        assert_ne!(d3d10, layout);
        let unknown = Guid::new(0x1111_1111, 0x2222, 0x3333, [0x44; 8]);
        assert_eq!(qi(layout, &unknown), (E_NOINTERFACE, ptr::null_mut()));
        null_riid_and_out(layout, crate::com::E_INVALIDARG);
        assert_eq!(add_ref(layout), 4);

        assert_eq!(release(d3d10), 3);
        assert_eq!(release(layout), 2);
        assert_eq!(release(layout), 1);
        assert_eq!(dev.count(), 2);
        // The last release gives the device back and frees the object (both counts reach zero).
        assert_eq!(release(layout), 0);
        assert_eq!(dev.count(), 1);
    }

    #[test]
    fn dxgi_resource_usage_reports_the_uav_bit_as_the_sdk_has_it() {
        let res = Resource::into_raw(
            Common {
                bind_flags: 0x80 | 0x8,
                ..Common::default()
            },
            ResourceKind::DefaultBuffer,
            ResourceDesc::Buffer(D3d11BufferDesc::default()),
        );
        // SAFETY: `res` is live until the release below.
        let iface = unsafe { (*res).dxgi_resource.as_iface() };
        let mut usage = 0;
        // SAFETY: `iface` is the resource's live `IDXGIResource1` tear-off.
        let get_usage = unsafe { vtbl::<IDXGIResource1Vtbl>(iface) }.get_usage;
        // SAFETY: `iface` is live; `usage` is a valid out pointer.
        assert_eq!(unsafe { get_usage(iface, &raw mut usage) }, S_OK);
        // `DXGI_USAGE_UNORDERED_ACCESS` is `1 << 10` and `DXGI_USAGE_SHADER_INPUT` `1 << 4`.
        assert_eq!(usage, (1 << 10) | (1 << 4));
        assert_eq!(release(res.cast()), 0);
    }

    #[test]
    fn keyed_mutex_calls_without_a_mutex_answer_dxgi_invalid_call() {
        let common = Common::default();
        // `DXGI_ERROR_INVALID_CALL` is 0x887a0001.
        let invalid_call = 0x887a_0001_u32.cast_signed();
        assert_eq!(
            crate::keyed_mutex::acquire_sync_for_resource(&common, 0, 0),
            invalid_call
        );
        assert_eq!(
            crate::keyed_mutex::release_sync_for_resource(&common, 0),
            invalid_call
        );
    }

    #[test]
    fn private_data_is_one_store_for_a_resource_and_its_tear_offs() {
        let res = texture2d(0);
        let obj = res.cast::<c_void>();
        // SAFETY: `res` is live until the release below.
        let surface = unsafe { (*res).dxgi_surface.as_iface() };
        let guid = Guid::new(0x2222_2222, 0x3333, 0x4444, [0x55; 8]);
        let data = [7u8, 8, 9];
        // SAFETY: `obj` is a live resource; `guid` and `data` are valid.
        let set = unsafe { vtbl::<ID3D11ResourceVtbl>(obj) }.set_private_data;
        // SAFETY: as above.
        let stored = unsafe { set(obj, (&raw const guid).cast(), 3, data.as_ptr().cast()) };
        assert_eq!(stored, S_OK);
        // A null GUID is an invalid argument, not a fault.
        // SAFETY: as above, with the null GUID under test.
        let refused = unsafe { set(obj, ptr::null(), 3, data.as_ptr().cast()) };
        assert_eq!(refused, crate::com::E_INVALIDARG);

        // The surface tear-off reads what the resource stored.
        // SAFETY: `surface` is the live tear-off.
        let get = unsafe { vtbl::<IDXGISurface2Vtbl>(surface) }.get_private_data;
        let mut size = 3u32;
        let mut out = [0u8; 3];
        // SAFETY: `surface` is live; `guid`, `size` and `out` are valid.
        let hr = unsafe {
            get(
                surface,
                (&raw const guid).cast(),
                &raw mut size,
                out.as_mut_ptr().cast(),
            )
        };
        assert_eq!((hr, size, out), (S_OK, 3, data));
        assert_eq!(release(obj), 0);
    }
}
