use core::{
    cell::Cell,
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use dxmt_util::{
    com::{E_FAIL, E_NOINTERFACE, Guid, Hresult, IUnknownVtbl, S_OK, write_out},
    dxgi_vtbl::IDXGIFactory1Vtbl,
    ext::IID_IMTL_DXGI_ADAPTER,
};

use super::{Failure, IID_IDXGI_ADAPTER1, IID_IDXGI_FACTORY1, default_adapter, resolve};

/// `DXGI_ERROR_NOT_FOUND`, which `EnumAdapters` answers past the last adapter.
const NOT_FOUND: Hresult = 0x887a_0002_u32.cast_signed();

thread_local! {
    /// What this thread's `CreateDXGIFactory1` answers: its `HRESULT` and factory.
    static NEXT_FACTORY: Cell<(Hresult, *mut c_void)> = const { Cell::new((E_FAIL, ptr::null_mut())) };
}

/// The fake `CreateDXGIFactory1` the core's import resolves to in this test binary.
#[unsafe(no_mangle)]
extern "system" fn CreateDXGIFactory1(riid: *const Guid, out: *mut *mut c_void) -> Hresult {
    // SAFETY: the code under test passes a live IID.
    assert!(unsafe { Guid::read(riid) } == Some(IID_IDXGI_FACTORY1));
    let (hr, factory) = NEXT_FACTORY.get();
    if !factory.is_null() {
        // SAFETY: `factory` is a live `Fake` the test keeps.
        unsafe { fake(factory) }.refs.fetch_add(1, Ordering::SeqCst);
    }
    // SAFETY: `out` is the caller's local.
    unsafe { write_out(out, factory) };
    hr
}

/// A fake COM object: a factory (`enum` set) or an adapter (`answers` set).
#[repr(C)]
struct Fake {
    vtbl: *const c_void,
    refs: AtomicU32,
    /// An adapter's IIDs it answers with itself.
    answers: &'static [Guid],
    /// A factory's `EnumAdapters(0)`: its `HRESULT` and adapter.
    enum_answer: (Hresult, *mut c_void),
}

impl Fake {
    fn adapter(answers: &'static [Guid]) -> Self {
        Self {
            vtbl: ptr::from_ref(&ADAPTER_VTBL).cast(),
            refs: AtomicU32::new(1),
            answers,
            enum_answer: (S_OK, ptr::null_mut()),
        }
    }

    fn factory(enum_answer: (Hresult, *mut c_void)) -> Self {
        Self {
            vtbl: ptr::from_ref(&FACTORY_VTBL).cast(),
            refs: AtomicU32::new(0),
            answers: &[],
            enum_answer,
        }
    }

    fn this(&self) -> *mut c_void {
        ptr::from_ref(self).cast_mut().cast()
    }

    fn refs(&self) -> u32 {
        self.refs.load(Ordering::SeqCst)
    }
}

/// The fake behind `this`.
///
/// # Safety
///
/// `this` is a live `Fake`.
const unsafe fn fake<'a>(this: *mut c_void) -> &'a Fake {
    // SAFETY: the caller's.
    unsafe { &*this.cast::<Fake>() }
}

extern "system" fn adapter_qi(
    this: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: the slot's `this` is a live `Fake`.
    let adapter = unsafe { fake(this) };
    // SAFETY: the code under test passes a live IID.
    let riid = unsafe { Guid::read(riid) };
    if riid.is_some_and(|riid| adapter.answers.contains(&riid)) {
        adapter.refs.fetch_add(1, Ordering::SeqCst);
        // SAFETY: the caller's local.
        unsafe { write_out(out, this) };
        return S_OK;
    }
    E_NOINTERFACE
}

extern "system" fn adapter_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Fake`.
    unsafe { fake(this) }.refs.fetch_add(1, Ordering::SeqCst) + 1
}

extern "system" fn adapter_release(this: *mut c_void) -> u32 {
    // SAFETY: the slot's `this` is a live `Fake`.
    unsafe { fake(this) }.refs.fetch_sub(1, Ordering::SeqCst) - 1
}

static ADAPTER_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: adapter_qi,
    add_ref: adapter_add_ref,
    release: adapter_release,
};

extern "system-unwind" fn factory_add_ref(this: *mut c_void) -> u32 {
    adapter_add_ref(this)
}

extern "system-unwind" fn factory_release(this: *mut c_void) -> u32 {
    adapter_release(this)
}

extern "system-unwind" fn factory_enum(
    this: *mut c_void,
    index: u32,
    out: *mut *mut c_void,
) -> i32 {
    assert_eq!(index, 0);
    // SAFETY: the slot's `this` is a live `Fake`.
    let (hr, adapter) = unsafe { fake(this) }.enum_answer;
    if !adapter.is_null() {
        adapter_add_ref(adapter);
    }
    // SAFETY: the caller's local.
    unsafe { write_out(out, adapter) };
    hr
}

extern "system-unwind" fn no_qi(_: *mut c_void, _: *const c_void, _: *mut *mut c_void) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_set(
    _: *mut c_void,
    _: *const c_void,
    _: u32,
    _: *const c_void,
) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_set_iface(_: *mut c_void, _: *const c_void, _: *const c_void) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_get(
    _: *mut c_void,
    _: *const c_void,
    _: *mut u32,
    _: *mut c_void,
) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_window(_: *mut c_void, _: *mut c_void, _: u32) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_get_window(_: *mut c_void, _: *mut *mut c_void) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_swap_chain(
    _: *mut c_void,
    _: *mut c_void,
    _: *mut c_void,
    _: *mut *mut c_void,
) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_software(_: *mut c_void, _: *mut c_void, _: *mut *mut c_void) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_enum1(_: *mut c_void, _: u32, _: *mut *mut c_void) -> i32 {
    unreachable!()
}
extern "system-unwind" fn no_current(_: *mut c_void) -> i32 {
    unreachable!()
}

static FACTORY_VTBL: IDXGIFactory1Vtbl = IDXGIFactory1Vtbl {
    query_interface: no_qi,
    add_ref: factory_add_ref,
    release: factory_release,
    set_private_data: no_set,
    set_private_data_interface: no_set_iface,
    get_private_data: no_get,
    get_parent: no_qi,
    enum_adapters: factory_enum,
    make_window_association: no_window,
    get_window_association: no_get_window,
    create_swap_chain: no_swap_chain,
    create_software_adapter: no_software,
    enum_adapters1: no_enum1,
    is_current: no_current,
};

const DXMT: &[Guid] = &[IID_IDXGI_ADAPTER1, IID_IMTL_DXGI_ADAPTER];

#[test]
fn null_adapter_is_adapter_0_of_a_new_factory() {
    let adapter = Fake::adapter(DXMT);
    let factory = Fake::factory((S_OK, adapter.this()));
    NEXT_FACTORY.set((S_OK, factory.this()));
    // SAFETY: the fakes live to the end of the test.
    let mtl = unsafe { resolve(ptr::null_mut()) };
    assert_eq!(mtl, Ok(adapter.this()));
    // The factory and `EnumAdapters`' reference are given back; the answer keeps one.
    assert_eq!(factory.refs(), 0);
    assert_eq!(adapter.refs(), 2);
}

#[test]
fn default_adapter_keeps_the_factory_for_the_caller() {
    let adapter = Fake::adapter(DXMT);
    let factory = Fake::factory((S_OK, adapter.this()));
    NEXT_FACTORY.set((S_OK, factory.this()));
    // SAFETY: the fakes live to the end of the test.
    let pair = unsafe { default_adapter() };
    assert_eq!(pair, Ok((factory.this(), adapter.this())));
    assert_eq!(factory.refs(), 1);
    assert_eq!(adapter.refs(), 2);
}

#[test]
fn failures_carry_the_calls_answer_and_give_references_back() {
    NEXT_FACTORY.set((E_FAIL, ptr::null_mut()));
    // SAFETY: no object is involved.
    let mtl = unsafe { resolve(ptr::null_mut()) };
    assert_eq!(mtl, Err(Failure::Factory(E_FAIL)));
    // Success with no factory is a failure too, with the success code.
    NEXT_FACTORY.set((S_OK, ptr::null_mut()));
    // SAFETY: as above.
    assert_eq!(unsafe { default_adapter() }, Err(Failure::Factory(S_OK)));

    let factory = Fake::factory((NOT_FOUND, ptr::null_mut()));
    NEXT_FACTORY.set((S_OK, factory.this()));
    // SAFETY: the fake lives to the end of the test.
    let mtl = unsafe { resolve(ptr::null_mut()) };
    assert_eq!(mtl, Err(Failure::NoAdapter(NOT_FOUND)));
    assert_eq!(factory.refs(), 0);
}

#[test]
fn an_application_adapter_is_asked_for_dxmts_interface() {
    let adapter = Fake::adapter(DXMT);
    // SAFETY: the fake lives to the end of the test.
    assert_eq!(unsafe { resolve(adapter.this()) }, Ok(adapter.this()));
    // The `IDXGIAdapter1` reference is given back; the answer keeps one.
    assert_eq!(adapter.refs(), 2);

    // One that answers no `IDXGIAdapter1` is asked itself.
    let bare = Fake::adapter(&[IID_IMTL_DXGI_ADAPTER]);
    // SAFETY: as above.
    assert_eq!(unsafe { resolve(bare.this()) }, Ok(bare.this()));
    assert_eq!(bare.refs(), 2);

    let foreign = Fake::adapter(&[IID_IDXGI_ADAPTER1]);
    // SAFETY: as above.
    assert_eq!(unsafe { resolve(foreign.this()) }, Err(Failure::NotDxmt));
    assert_eq!(foreign.refs(), 1);
}
