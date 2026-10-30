//! Shared events, the shared-event listener, fences and Mach ports.

use core::ffi::{c_char, c_int, c_void};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{NSObject, ProtocolObject};
use objc2::{AnyThread, extern_class, extern_methods};
use objc2_metal::{
    MTLDevice, MTLEvent, MTLFence, MTLResource, MTLSharedEvent, MTLTexture, MTLTextureDescriptor,
};
use winemetal_wire::{
    UnixcallBootstrap, UnixcallGenericObjNoret, UnixcallGenericObjObjRet, UnixcallGenericObjRet,
    UnixcallGenericObjUint64Noret, UnixcallGenericObjUint64Ret, UnixcallMtldeviceNewsharedeventwithmachport,
    UnixcallMtldeviceNewtexture, UnixcallMtlsharedeventCreatemachport, UnixcallMtlsharedeventSetevent,
    UnixcallMtlsharedeventSeteventatvalues, UnixcallMtlsharedeventWaituntilsignaledvalue, WMTTextureInfo,
};

use crate::ffi::{AbortOnPanic, STATUS_SUCCESS, STATUS_UNSUCCESSFUL, mem_mut, mem_slice, params};
use crate::float_cast::{u64_to_usize, usize_to_u32};
use crate::handle::{obj, ret_owned};
use crate::texture::fill_texture_descriptor;

use self::listener::{Listener, Owner};

pub mod listener;

/// `TASK_BOOTSTRAP_PORT`: `task_get_bootstrap_port`'s special port.
const TASK_BOOTSTRAP_PORT: c_int = 4;

/// `MACH_PORT_RIGHT_SEND`.
const MACH_PORT_RIGHT_SEND: u32 = 0;

/// `KERN_SUCCESS`.
const KERN_SUCCESS: c_int = 0;

unsafe extern "C" {
    /// `mach_task_self()`: this task's port, set before any image's initializer runs.
    #[link_name = "mach_task_self_"]
    safe static MACH_TASK_SELF: u32;

    fn task_get_special_port(task: u32, which: c_int, port: *mut u32) -> c_int;

    /// The bootstrap server's private registration, as the C declared it.
    fn bootstrap_register2(bp: u32, name: *const c_char, sp: u32, flags: c_int) -> c_int;

    fn bootstrap_look_up(bp: u32, name: *const c_char, sp: *mut u32) -> c_int;

    fn mach_port_deallocate(task: u32, name: u32) -> c_int;

    fn mach_port_mod_refs(task: u32, name: u32, right: u32, delta: i32) -> c_int;
}

#[cfg(not(feature = "native"))]
unsafe extern "C" {
    /// Wine's `NtSetEvent`, from `ntdll.so`.
    #[link_name = "NtSetEvent"]
    fn nt_set_event(handle: *mut c_void, prev_state: *mut c_void) -> i32;
}

#[cfg(feature = "native")]
unsafe extern "C" {
    /// `mn_win32_set_event` (`src/nativemetal/win32_event.c`): sets an event or calls a relay.
    fn wmt_native_set_event(handle: *mut c_void) -> c_int;

    /// `mn_win32_ref`: one more reference to a native event.
    fn wmt_native_ref(handle: *mut c_void);

    /// `mn_win32_unref`: drops a reference to a native event; the last frees it.
    fn wmt_native_unref(handle: *mut c_void) -> c_int;
}

/// Slot 15, `MTLDevice_newSharedEvent`: a new shared event, +1.
///
/// # Safety
///
/// `args` is slot 15's params block; its handle is null or an `MTLDevice`.
#[unsafe(export_name = "_MTLDevice_newSharedEvent")]
pub unsafe extern "C-unwind" fn mtl_device_new_shared_event(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.handle) };
    p.ret = ret_owned(device.and_then(MTLDevice::newSharedEvent));
    STATUS_SUCCESS
}

/// Slot 16, `MTLSharedEvent_signaledValue`.
///
/// # Safety
///
/// `args` is slot 16's params block; its handle is null or an `MTLSharedEvent`.
#[unsafe(export_name = "_MTLSharedEvent_signaledValue")]
pub unsafe extern "C-unwind" fn mtl_shared_event_signaled_value(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Ret>(args) };
    // SAFETY: as above.
    let event = unsafe { obj::<ProtocolObject<dyn MTLSharedEvent>>(p.handle) };
    p.ret = event.map_or(0, MTLSharedEvent::signaledValue);
    STATUS_SUCCESS
}

/// Slot 103, `MTLSharedEvent_signalValue`: sets the event's value.
///
/// # Safety
///
/// `args` is slot 103's params block; its handle is null or an object (another class raises).
#[unsafe(export_name = "_MTLSharedEvent_signalValue")]
pub unsafe extern "C-unwind" fn mtl_shared_event_signal_value(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjUint64Noret>(args) };
    // SAFETY: as above.
    if let Some(event) = unsafe { obj::<ProtocolObject<dyn MTLSharedEvent>>(p.handle) } {
        event.setSignaledValue(p.arg);
    }
    STATUS_SUCCESS
}

/// Slot 104, `MTLSharedEvent_setWin32EventAtValue`: sets a Win32 event when the value is reached.
///
/// The set runs on the listener's thread (slot 109's). Natively the event is held from here to
/// the set. A null listener registers nothing (the C read through it).
///
/// # Safety
///
/// `args` is slot 104's params block: a null or live shared event, a null or live listener.
#[unsafe(export_name = "_MTLSharedEvent_setWin32EventAtValue")]
pub unsafe extern "C-unwind" fn mtl_shared_event_set_win32_event_at_value(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlsharedeventSetevent>(args) };
    let Some(listener) = Listener::from_handle(p.shared_event_listener) else {
        return STATUS_SUCCESS;
    };
    let nt_event = p.event_handle;
    hold(nt_event);
    // SAFETY: the caller's contract.
    let Some(event) = (unsafe { obj::<ProtocolObject<dyn MTLSharedEvent>>(p.shared_event) }) else {
        return STATUS_SUCCESS;
    };
    // SAFETY: the caller's listener, owned by its handle.
    let owner = unsafe { Owner::new(listener) };
    let block = RcBlock::new(move |_: NonNull<ProtocolObject<dyn MTLSharedEvent>>, _: u64| {
        let _guard = AbortOnPanic;
        owner.notify(move || set_and_drop(nt_event), || drop_hold(nt_event));
    });
    // SAFETY: as above.
    let events = unsafe { listener.as_ref() }.events();
    // SAFETY: a live block, which Metal copies.
    unsafe { event.notifyListener_atValue_block(events, p.value, RcBlock::as_ptr(&block)) };
    STATUS_SUCCESS
}

/// Slot 105, `MTLDevice_newFence`: a new fence, +1.
///
/// # Safety
///
/// `args` is slot 105's params block; its handle is null or an `MTLDevice`.
#[unsafe(export_name = "_MTLDevice_newFence")]
pub unsafe extern "C-unwind" fn mtl_device_new_fence(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.handle) };
    p.ret = ret_owned::<ProtocolObject<dyn MTLFence>>(device.and_then(MTLDevice::newFence));
    STATUS_SUCCESS
}

/// Slot 106, `MTLDevice_newEvent`: a new event, +1.
///
/// # Safety
///
/// `args` is slot 106's params block; its handle is null or an `MTLDevice`.
#[unsafe(export_name = "_MTLDevice_newEvent")]
pub unsafe extern "C-unwind" fn mtl_device_new_event(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjObjRet>(args) };
    // SAFETY: as above.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.handle) };
    p.ret = ret_owned::<ProtocolObject<dyn MTLEvent>>(device.and_then(MTLDevice::newEvent));
    STATUS_SUCCESS
}

/// Slot 108, `SharedEventListener_create`: a new listener (not an object).
///
/// # Safety
///
/// `args` is slot 108's params block.
#[unsafe(export_name = "_SharedEventListener_create")]
pub unsafe extern "C-unwind" fn shared_event_listener_create(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjRet>(args) };
    p.ret = Listener::create();
    STATUS_SUCCESS
}

/// Slot 109, `SharedEventListener_start`: runs the listener's loop until its destroy.
///
/// # Safety
///
/// `args` is slot 109's params block; its handle is null or a listener slot 108 made.
#[unsafe(export_name = "_SharedEventListener_start")]
pub unsafe extern "C-unwind" fn shared_event_listener_start(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    if let Some(listener) = Listener::from_handle(p.handle) {
        // SAFETY: as above.
        unsafe { Listener::start(listener) };
    }
    STATUS_SUCCESS
}

/// Slot 110, `SharedEventListener_destroy`.
///
/// # Safety
///
/// `args` is slot 110's params block; its handle is null or a listener slot 108 made.
#[unsafe(export_name = "_SharedEventListener_destroy")]
pub unsafe extern "C-unwind" fn shared_event_listener_destroy(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallGenericObjNoret>(args) };
    if let Some(listener) = Listener::from_handle(p.handle) {
        // SAFETY: as above.
        unsafe { Listener::destroy(listener) };
    }
    STATUS_SUCCESS
}

/// Slot 120, `MTLDevice_newSharedTexture`: a texture other processes can open, +1.
///
/// With the info's `mach_port` 0 it makes one from the info and writes its port; with a port it
/// opens that texture and writes its whole description. A null info writes nothing (the C read
/// through it).
///
/// # Safety
///
/// `args` is slot 120's params block: a null or live device, a null or live info.
#[unsafe(export_name = "_MTLDevice_newSharedTexture")]
pub unsafe extern "C-unwind" fn mtl_device_new_shared_texture(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewtexture>(args) };
    // SAFETY: as above.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    // SAFETY: as above.
    let Some(info) = (unsafe { mem_mut::<WMTTextureInfo>(p.info) }) else {
        return STATUS_SUCCESS;
    };
    if info.mach_port != 0 {
        let handle = PrivateSharedTextureHandle::init_with_mach_port(
            PrivateSharedTextureHandle::alloc(),
            info.mach_port,
        );
        let texture = device.and_then(|d| d.new_shared_texture_with_handle(handle.as_deref()));
        extract_texture_descriptor(texture.as_deref(), info);
        info.gpu_resource_id = texture.as_ref().map_or(0, |t| t.gpuResourceID().to_raw());
        p.ret = ret_owned(texture);
    } else {
        let desc = MTLTextureDescriptor::init(MTLTextureDescriptor::alloc());
        fill_texture_descriptor(&desc, info);
        let texture = device.and_then(|d| d.newSharedTextureWithDescriptor(&desc));
        let handle = texture.as_ref().and_then(|t| t.newSharedTextureHandle());
        info.gpu_resource_id = texture.as_ref().map_or(0, |t| t.gpuResourceID().to_raw());
        // Adds a reference to the texture's IOSurface.
        info.mach_port = handle.map_or(0, |h| {
            // SAFETY: the same object under a second declaration of its own class.
            let h = unsafe { &*Retained::as_ptr(&h).cast::<PrivateSharedTextureHandle>() };
            h.create_mach_port()
        });
        p.ret = ret_owned(texture);
    }
    STATUS_SUCCESS
}

/// Slot 121, `WMTBootstrapRegister`: registers the port under the name with the bootstrap server.
///
/// `STATUS_UNSUCCESSFUL` when the server can't be reached or refuses.
///
/// # Safety
///
/// `args` is slot 121's params block.
#[unsafe(export_name = "_WMTBootstrapRegister")]
pub unsafe extern "C-unwind" fn wmt_bootstrap_register(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallBootstrap>(args) };
    let Some(bp) = bootstrap_port() else {
        return STATUS_UNSUCCESSFUL;
    };
    // SAFETY: the params' 128-byte name, as the C passed it.
    let kr = unsafe { bootstrap_register2(bp, p.name.as_ptr().cast(), p.mach_port, 0) };
    // SAFETY: the bootstrap port's send right this call got.
    unsafe { mach_port_deallocate(MACH_TASK_SELF, bp) };
    if kr == KERN_SUCCESS { STATUS_SUCCESS } else { STATUS_UNSUCCESSFUL }
}

/// Slot 122, `WMTBootstrapLookUp`: the port registered under the name.
///
/// `STATUS_UNSUCCESSFUL` when the server can't be reached (nothing written) or has no such name
/// (`mach_port` still written).
///
/// # Safety
///
/// `args` is slot 122's params block.
#[unsafe(export_name = "_WMTBootstrapLookUp")]
pub unsafe extern "C-unwind" fn wmt_bootstrap_look_up(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallBootstrap>(args) };
    let Some(bp) = bootstrap_port() else {
        return STATUS_UNSUCCESSFUL;
    };
    let mut port = 0;
    // SAFETY: the params' 128-byte name, as the C passed it, and a port to write.
    let kr = unsafe { bootstrap_look_up(bp, p.name.as_ptr().cast(), &raw mut port) };
    // SAFETY: the bootstrap port's send right this call got.
    unsafe { mach_port_deallocate(MACH_TASK_SELF, bp) };
    p.mach_port = port;
    if kr == KERN_SUCCESS { STATUS_SUCCESS } else { STATUS_UNSUCCESSFUL }
}

/// Slot 123, `MTLSharedEvent_createMachPort`: a send right to the event's port.
///
/// The handle's own right goes with the handle: the caller gets one more.
///
/// # Safety
///
/// `args` is slot 123's params block; its event is null or an `MTLSharedEvent`.
#[unsafe(export_name = "_MTLSharedEvent_createMachPort")]
pub unsafe extern "C-unwind" fn mtl_shared_event_create_mach_port(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlsharedeventCreatemachport>(args) };
    // SAFETY: as above.
    let event = unsafe { obj::<ProtocolObject<dyn MTLSharedEvent>>(p.event) };
    let handle = event.map(MTLSharedEvent::newSharedEventHandle);
    let port = handle.as_ref().map_or(0, |h| {
        // SAFETY: the same object under a second declaration of its own class.
        let h = unsafe { &*Retained::as_ptr(h).cast::<PrivateSharedEventHandle>() };
        h.event_port()
    });
    // SAFETY: a port name in this task (0 when the event was nil, as the C passed it).
    unsafe { mach_port_mod_refs(MACH_TASK_SELF, port, MACH_PORT_RIGHT_SEND, 1) };
    p.ret_mach_port = port;
    drop(handle);
    STATUS_SUCCESS
}

/// Slot 124, `MTLDevice_newSharedEventWithMachPort`: the shared event a port names, +1.
///
/// # Safety
///
/// `args` is slot 124's params block; its device is null or an `MTLDevice`.
#[unsafe(export_name = "_MTLDevice_newSharedEventWithMachPort")]
pub unsafe extern "C-unwind" fn mtl_device_new_shared_event_with_mach_port(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtldeviceNewsharedeventwithmachport>(args) };
    // SAFETY: as above.
    let device = unsafe { obj::<ProtocolObject<dyn MTLDevice>>(p.device) };
    p.ret_event = ret_owned(device.and_then(|d| d.new_shared_event_with_mach_port(p.mach_port)));
    STATUS_SUCCESS
}

/// Slot 126, `MTLSharedEvent_waitUntilSignaledValue`: 1 when the value was reached in time.
///
/// # Safety
///
/// `args` is slot 126's params block; its event is null or an `MTLSharedEvent`.
#[unsafe(export_name = "_MTLSharedEvent_waitUntilSignaledValue")]
pub unsafe extern "C-unwind" fn mtl_shared_event_wait_until_signaled_value(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlsharedeventWaituntilsignaledvalue>(args) };
    // SAFETY: as above.
    let event = unsafe { obj::<ProtocolObject<dyn MTLSharedEvent>>(p.event) };
    let reached = event.is_some_and(|e| e.waitUntilSignaledValue_timeoutMS(p.value, p.timeout_ms));
    p.ret_timeout = u8::from(reached);
    STATUS_SUCCESS
}

/// Slot 146, `MTLSharedEvent_setWin32EventAtValues`: a Win32 event set once `needed` are reached.
///
/// The set runs on the listener's thread. Natively the event is held from here to the set, and
/// forever when the count never reaches `needed`. Null arrays with a count, or a null listener
/// with a count, register nothing (the C read through them).
///
/// # Safety
///
/// `args` is slot 146's params block: `count` events (each null or a live shared event) and
/// values, a null or live listener.
#[unsafe(export_name = "_MTLSharedEvent_setWin32EventAtValues")]
pub unsafe extern "C-unwind" fn mtl_shared_event_set_win32_event_at_values(args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let p = unsafe { params::<UnixcallMtlsharedeventSeteventatvalues>(args) };
    // SAFETY: as above.
    let events = unsafe { mem_slice::<u64>(p.shared_events, u64::from(p.count)) };
    // SAFETY: as above.
    let values = unsafe { mem_slice::<u64>(p.values, u64::from(p.count)) };
    let (Some(events), Some(values)) = (events, values) else {
        return STATUS_SUCCESS;
    };
    let nt_event = p.event_handle;
    let needed = Arc::new(AtomicU32::new(p.needed));
    hold(nt_event);
    let Some(listener) = Listener::from_handle(p.shared_event_listener) else {
        return STATUS_SUCCESS;
    };
    for (&event, &value) in events.iter().zip(values) {
        // SAFETY: the caller's contract.
        let Some(event) = (unsafe { obj::<ProtocolObject<dyn MTLSharedEvent>>(event) }) else {
            continue;
        };
        // SAFETY: the caller's listener, owned by its handle.
        let owner = unsafe { Owner::new(listener) };
        let needed = Arc::clone(&needed);
        let block = RcBlock::new(move |_: NonNull<ProtocolObject<dyn MTLSharedEvent>>, _: u64| {
            let _guard = AbortOnPanic;
            if needed.fetch_sub(1, Ordering::AcqRel) == 1 {
                owner.notify(move || set_and_drop(nt_event), || drop_hold(nt_event));
            }
        });
        // SAFETY: as above.
        let listener_events = unsafe { listener.as_ref() }.events();
        // SAFETY: a live block, which Metal copies.
        unsafe { event.notifyListener_atValue_block(listener_events, value, RcBlock::as_ptr(&block)) };
    }
    STATUS_SUCCESS
}

extern_class!(
    /// `MTLSharedTextureHandle`, re-declared for its private Mach port methods.
    #[unsafe(super(NSObject))]
    #[name = "MTLSharedTextureHandle"]
    struct PrivateSharedTextureHandle;
);

impl PrivateSharedTextureHandle {
    extern_methods!(
        /// `-initWithMachPort:`: the handle of the texture a port from `createMachPort` names.
        #[unsafe(method(initWithMachPort:))]
        #[unsafe(method_family = init)]
        fn init_with_mach_port(this: Allocated<Self>, port: u32) -> Option<Retained<Self>>;

        /// `-createMachPort`: a send right to the texture's port.
        #[unsafe(method(createMachPort))]
        #[unsafe(method_family = none)]
        fn create_mach_port(&self) -> u32;
    );
}

extern_class!(
    /// `MTLSharedEventHandle`, re-declared for its private `eventPort`.
    #[unsafe(super(NSObject))]
    #[name = "MTLSharedEventHandle"]
    struct PrivateSharedEventHandle;
);

impl PrivateSharedEventHandle {
    extern_methods!(
        /// `-eventPort`: the event's port, a send right the handle owns.
        #[unsafe(method(eventPort))]
        #[unsafe(method_family = none)]
        fn event_port(&self) -> u32;
    );
}

/// `MTLDevice`'s shared-object sends, re-declared.
trait SharedDevice {
    fn new_shared_event_with_mach_port(
        &self,
        port: u32,
    ) -> Option<Retained<ProtocolObject<dyn MTLSharedEvent>>>;

    fn new_shared_texture_with_handle(
        &self,
        handle: Option<&PrivateSharedTextureHandle>,
    ) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
}

impl SharedDevice for ProtocolObject<dyn MTLDevice> {
    extern_methods!(
        /// `-newSharedEventWithMachPort:` (private): the event a port from `eventPort` names.
        #[unsafe(method(newSharedEventWithMachPort:))]
        #[unsafe(method_family = new)]
        fn new_shared_event_with_mach_port(
            &self,
            port: u32,
        ) -> Option<Retained<ProtocolObject<dyn MTLSharedEvent>>>;

        /// `-newSharedTextureWithHandle:`, its handle sent as nil when `init` gave none.
        #[unsafe(method(newSharedTextureWithHandle:))]
        #[unsafe(method_family = new)]
        fn new_shared_texture_with_handle(
            &self,
            handle: Option<&PrivateSharedTextureHandle>,
        ) -> Option<Retained<ProtocolObject<dyn MTLTexture>>>;
    );
}

/// Writes `texture`'s description into `info`: every field but the port and the resource id.
///
/// A nil texture writes zeros, as the C's sends to nil did.
fn extract_texture_descriptor(
    texture: Option<&ProtocolObject<dyn MTLTexture>>,
    info: &mut WMTTextureInfo,
) {
    let [kind, format, width, height, depth, array_length, mips, samples, usage, options] =
        texture.map_or([0; 10], |t| {
            [
                t.textureType().0,
                t.pixelFormat().0,
                t.width(),
                t.height(),
                t.depth(),
                t.arrayLength(),
                t.mipmapLevelCount(),
                t.sampleCount(),
                t.usage().0,
                t.resourceOptions().0,
            ]
        });
    info.pixel_format = usize_to_u32(format);
    info.width = usize_to_u32(width);
    info.height = usize_to_u32(height);
    info.depth = usize_to_u32(depth);
    info.array_length = usize_to_u32(array_length);
    info.bits = texture_bits(kind, mips, samples, usage);
    info.options = options as u64;
    info.flags = 0;
}

/// `WMTTextureInfo`'s four 8-bit fields packed, each cut to its width as the C's bit-fields.
fn texture_bits(kind: usize, mips: usize, samples: usize, usage: usize) -> u32 {
    let field = |v: usize, shift: u32, width: u32| (usize_to_u32(v) & ((1 << width) - 1)) << shift;
    field(kind, WMTTextureInfo::TYPE_SHIFT, WMTTextureInfo::TYPE_WIDTH)
        | field(mips, WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT, WMTTextureInfo::MIPMAP_LEVEL_COUNT_WIDTH)
        | field(samples, WMTTextureInfo::SAMPLE_COUNT_SHIFT, WMTTextureInfo::SAMPLE_COUNT_WIDTH)
        | field(usage, WMTTextureInfo::USAGE_SHIFT, WMTTextureInfo::USAGE_WIDTH)
}

/// The bootstrap server's port (a send right the caller deallocates), `None` when unreachable.
fn bootstrap_port() -> Option<u32> {
    let mut bp = 0;
    // SAFETY: this task's port and a port to write.
    let kr = unsafe { task_get_special_port(MACH_TASK_SELF, TASK_BOOTSTRAP_PORT, &raw mut bp) };
    (kr == KERN_SUCCESS).then_some(bp)
}

/// A Win32 event handle as the pointer the setters take.
const fn event_ptr(handle: u64) -> *mut c_void {
    core::ptr::with_exposed_provenance_mut(u64_to_usize(handle))
}

/// Holds an event from its registration to its set.
///
/// Natively one more reference, as the Windows kernel holds it, so the program may close it first.
#[cfg(feature = "native")]
fn hold(handle: u64) {
    // SAFETY: any value: a native event is checked by its kind.
    unsafe { wmt_native_ref(event_ptr(handle)) };
}

/// Holds an event from its registration: nothing under Wine.
#[cfg(not(feature = "native"))]
const fn hold(_handle: u64) {}

/// Sets an event on the listener's thread, then drops its hold.
#[cfg(feature = "native")]
fn set_and_drop(handle: u64) {
    let _guard = AbortOnPanic;
    // SAFETY: any value: a native event is checked by its kind.
    unsafe { wmt_native_set_event(event_ptr(handle)) };
    drop_hold(handle);
}

/// Sets an event on the listener's thread: Wine's `NtSetEvent`.
#[cfg(not(feature = "native"))]
fn set_and_drop(handle: u64) {
    let _guard = AbortOnPanic;
    // SAFETY: the guest's handle, which Wine checks.
    unsafe { nt_set_event(event_ptr(handle), core::ptr::null_mut()) };
}

/// Drops an event's hold without setting it.
#[cfg(feature = "native")]
fn drop_hold(handle: u64) {
    // SAFETY: any value: a native event is checked by its kind.
    unsafe { wmt_native_unref(event_ptr(handle)) };
}

/// Drops an event's hold without setting it: nothing under Wine.
#[cfg(not(feature = "native"))]
const fn drop_hold(_handle: u64) {}

#[cfg(test)]
mod tests;
