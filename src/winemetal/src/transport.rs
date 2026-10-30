//! Wine's unix-call handshake and calls, as `winemetal_thunks.c` and `main.c` made them.

use core::{
    ffi::{c_char, c_void},
    mem::MaybeUninit,
};

use winemetal_wire::{self as wire, MemPtr};

// winecrt0's (libwinecrt0.a): the Unix library's handle, which `__wine_init_unix_call` sets, and the dispatcher, a
// stub that finds ntdll's on first use.
unsafe extern "C" {
    static __wine_unixlib_handle: u64;
    static __wine_unix_call_dispatcher: unsafe extern "system" fn(u64, u32, *mut c_void) -> i32;
}

unsafe extern "system" {
    fn __wine_init_unix_call() -> i32;
    fn DisableThreadLibraryCalls(module: *mut c_void) -> i32;
}

/// `DllMain`: on process attach, no thread notifications and the Unix side loaded.
///
/// False when the Unix side does not load.
#[unsafe(export_name = "DllMain")]
pub extern "system" fn dll_main(instance: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason != DLL_PROCESS_ATTACH {
        return 1;
    }
    attach(instance)
}

/// `DLL_PROCESS_ATTACH`'s work.
///
/// Private, so that the exported `dll_main` passes no pointer argument to an unsafe call itself.
fn attach(instance: *mut c_void) -> i32 {
    // SAFETY: `instance` is the module handle the loader passed to `DllMain`.
    unsafe { DisableThreadLibraryCalls(instance) };
    // SAFETY: winecrt0's handshake, made once, on process attach, as Wine's unix-call ABI asks.
    let status = unsafe { __wine_init_unix_call() };
    i32::from(status == 0)
}

/// `WINE_UNIX_CALL`: winemetal.so's handler `slot` called with `args`; its status.
pub fn unix_call(slot: u32, args: *mut c_void) -> i32 {
    // SAFETY: winecrt0's handle: read only, set by `DllMain` before any thunk runs.
    let handle = unsafe { __wine_unixlib_handle };
    // SAFETY: winecrt0's dispatcher pointer: always a function (its lazy stub until the first call patches it).
    let dispatcher = unsafe { __wine_unix_call_dispatcher };
    // SAFETY: `args` is the params structure of `slot` (or the caller's own pointer, which `WMTForward` passes on).
    unsafe { dispatcher(handle, slot, args) }
}

/// A params structure of the wire: integers, floats, `MemPtr`s and arrays of them.
///
/// # Safety
///
/// All-zero bytes are a value of the type.
pub unsafe trait Params: Copy {}

// SAFETY: `obj_handle_t`, the params of slots 0 and 1.
unsafe impl Params for u64 {}

macro_rules! params {
    ($($t:ident),* $(,)?) => {$(
        // SAFETY: a `#[repr(C)]` wire mirror of integers, floats, `MemPtr`s and arrays of them.
        unsafe impl Params for wire::$t {}
    )*};
}

params!(
    UnixcallBootstrap,
    UnixcallCacheAllocInit,
    UnixcallCacheGet,
    UnixcallCacheSet,
    UnixcallCreateMetalViewFromHwnd,
    UnixcallDispatchdataCopybytes,
    UnixcallEnumerate,
    UnixcallGenericObjCmdNoret,
    UnixcallGenericObjConstptrNoret,
    UnixcallGenericObjNoret,
    UnixcallGenericObjObjDoubleNoret,
    UnixcallGenericObjObjNoret,
    UnixcallGenericObjObjObjNoret,
    UnixcallGenericObjObjObjUint64Ret,
    UnixcallGenericObjObjRet,
    UnixcallGenericObjObjUint64Noret,
    UnixcallGenericObjPtrNoret,
    UnixcallGenericObjRet,
    UnixcallGenericObjUint64Noret,
    UnixcallGenericObjUint64ObjRet,
    UnixcallGenericObjUint64Ret,
    UnixcallGenericObjUint64Uint64Ret,
    UnixcallGetOsVersion,
    UnixcallMtlbinaryarchiveSerialize,
    UnixcallMtlbufferNewtexture,
    UnixcallMtlbufferUpdatecontents,
    UnixcallMtlcapturemanagerStartcapture,
    UnixcallMtlcommandbufferBlitcommandencoderwithsamplebuffers,
    UnixcallMtlcommandbufferComputecommandencoderwithsamplebuffers,
    UnixcallMtlcommandbufferFrameInterpolate,
    UnixcallMtlcommandbufferSpatialScale,
    UnixcallMtlcommandbufferTemporalScale,
    UnixcallMtlcountersamplebufferNewtimestampbuffer,
    UnixcallMtlcountersamplebufferResolvecounterrange,
    UnixcallMtldeviceFxtemporalscalerScale,
    UnixcallMtldeviceHeapbuffersizeandalign,
    UnixcallMtldeviceHeaptexturesizeandalign,
    UnixcallMtldeviceNewbinaryarchive,
    UnixcallMtldeviceNewbuffer,
    UnixcallMtldeviceNewcomputepso,
    UnixcallMtldeviceNewdepthstencilstate,
    UnixcallMtldeviceNewfxframeinterpolator,
    UnixcallMtldeviceNewfxspatialscaler,
    UnixcallMtldeviceNewfxtemporalscaler,
    UnixcallMtldeviceNewheap,
    UnixcallMtldeviceNewicb,
    UnixcallMtldeviceNewlibrary,
    UnixcallMtldeviceNewlibrarywithsource,
    UnixcallMtldeviceNewlibrarywithsourceOptions,
    UnixcallMtldeviceNewmeshrenderpso,
    UnixcallMtldeviceNewrenderpso,
    UnixcallMtldeviceNewrenderpsoVertex,
    UnixcallMtldeviceNewresidencyset,
    UnixcallMtldeviceNewsamplerstate,
    UnixcallMtldeviceNewsharedeventwithmachport,
    UnixcallMtldeviceNewtexture,
    UnixcallMtldeviceSampletimestamps,
    UnixcallMtlheapNewbuffer,
    UnixcallMtlheapNewtexture,
    UnixcallMtllibraryNewfunctionWithConstants,
    UnixcallMtlresidencysetAddallocations,
    UnixcallMtlsharedeventCreatemachport,
    UnixcallMtlsharedeventSetevent,
    UnixcallMtlsharedeventSeteventatvalues,
    UnixcallMtlsharedeventWaituntilsignaledvalue,
    UnixcallMtltextureNewtextureview,
    UnixcallMtltextureReplaceregion,
    UnixcallNsstringGetcstring,
    UnixcallNsstringString,
    UnixcallQueryDisplaySetting,
    UnixcallQueryDisplaySettingForLayer,
    UnixcallSetmetalcachepath,
    UnixcallUpdateDisplaySetting,
    UnixcallWmtmemory,
);

/// Calls handler `slot` with a zeroed `T` that `fill` writes first.
///
/// The `T` as the handler left it, and the status.
pub fn call_status<T: Params>(slot: u32, fill: impl FnOnce(&mut T)) -> (T, i32) {
    // Zeroed in place: `MaybeUninit::zeroed` returns its zeros by value, and that copy can drop a padding byte.
    let mut params = MaybeUninit::<T>::uninit();
    // SAFETY: `params` is valid for a write of one `T`.
    unsafe { params.as_mut_ptr().write_bytes(0, 1) };
    // SAFETY: all-zero bytes are a `T` (`Params`).
    let params = unsafe { params.assume_init_mut() };
    fill(params);
    let status = unix_call(slot, (&raw mut *params).cast());
    (*params, status)
}

/// [`call_status`] without the status, which the C thunks dropped.
pub fn call<T: Params>(slot: u32, fill: impl FnOnce(&mut T)) -> T {
    call_status(slot, fill).0
}

/// `PtrToUInt64`: `p`'s address, zero-extended.
pub fn addr<T>(p: *const T) -> u64 {
    p.expose_provenance() as u64
}

/// `WMT_MEMPTR_SET`: `p`'s address.
#[cfg(target_pointer_width = "64")]
pub fn mem_ptr<T>(p: *const T) -> MemPtr {
    MemPtr { ptr: addr(p) }
}

/// `WMT_MEMPTR_SET`: `p`'s address, and a high half of 0.
#[cfg(target_pointer_width = "32")]
pub fn mem_ptr<T>(p: *const T) -> MemPtr {
    MemPtr {
        ptr: u32::from_ne_bytes(p.expose_provenance().to_ne_bytes()),
        high: 0,
    }
}

/// A `MemPtr`'s pointer (`.ptr`; a 32-bit process ignores the high half, as the C did).
pub const fn ptr_of(m: MemPtr) -> *mut c_void {
    core::ptr::with_exposed_provenance_mut(usize::from_ne_bytes(m.ptr.to_ne_bytes()))
}

/// C's conversion of a `uint64_t` to a `uint32_t`: the low half.
pub const fn low_half(value: u64) -> u32 {
    let [b0, b1, b2, b3, ..] = value.to_le_bytes();
    u32::from_le_bytes([b0, b1, b2, b3])
}

/// `if (out) *out = v;`
///
/// # Safety
///
/// `out` is null or valid for a write of a `T`.
pub unsafe fn write_if<T>(out: *mut T, v: T) {
    if !out.is_null() {
        // SAFETY: not null, so valid for the write (the caller's promise).
        unsafe { out.write(v) };
    }
}

/// `strncpy(field, name, 127)`: `name` up to its NUL or 127 bytes.
///
/// The rest of the zeroed field stays zero.
///
/// # Safety
///
/// `name` is readable up to its NUL or its 127th byte.
pub unsafe fn copy_name(field: &mut [u8; 128], name: *const c_char) {
    for (i, byte) in field.iter_mut().take(127).enumerate() {
        // SAFETY: the caller's string, read up to its NUL or 127 bytes (this loop's bound).
        let c = unsafe { name.cast::<u8>().wrapping_add(i).read() };
        if c == 0 {
            break;
        }
        *byte = c;
    }
}
