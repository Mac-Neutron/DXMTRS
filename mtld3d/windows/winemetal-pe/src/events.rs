//! Shared events, fences, the shared-event listener and Mach ports.

use core::ffi::{c_char, c_void};

use winemetal_wire::{
    UnixcallBootstrap, UnixcallGenericObjNoret, UnixcallGenericObjObjRet, UnixcallGenericObjRet,
    UnixcallGenericObjUint64Noret, UnixcallGenericObjUint64Ret, UnixcallMtldeviceNewsharedeventwithmachport,
    UnixcallMtlsharedeventCreatemachport, UnixcallMtlsharedeventSetevent, UnixcallMtlsharedeventSeteventatvalues,
    UnixcallMtlsharedeventWaituntilsignaledvalue, slot,
};

use crate::transport::{addr, call, call_status, copy_name, mem_ptr};

exports! {
    fn MTLDevice_newSharedEvent(device: u64) -> u64 {
        call(slot::MTL_DEVICE_NEW_SHARED_EVENT, |p: &mut UnixcallGenericObjObjRet| p.handle = device).ret
    }

    fn MTLSharedEvent_signaledValue(event: u64) -> u64 {
        call(slot::MTL_SHARED_EVENT_SIGNALED_VALUE, |p: &mut UnixcallGenericObjUint64Ret| p.handle = event).ret
    }

    fn MTLSharedEvent_signalValue(event: u64, value: u64) {
        call(slot::MTL_SHARED_EVENT_SIGNAL_VALUE, |p: &mut UnixcallGenericObjUint64Noret| {
            p.handle = event;
            p.arg = value;
        });
    }

    fn MTLSharedEvent_setWin32EventAtValue(
        event: u64,
        shared_event_listener: u64,
        nt_event_handle: *mut c_void,
        at_value: u64,
    ) {
        call(slot::MTL_SHARED_EVENT_SET_WIN32_EVENT_AT_VALUE, |p: &mut UnixcallMtlsharedeventSetevent| {
            p.shared_event = event;
            p.shared_event_listener = shared_event_listener;
            p.event_handle = addr(nt_event_handle);
            p.value = at_value;
        });
    }

    fn MTLDevice_newFence(device: u64) -> u64 {
        call(slot::MTL_DEVICE_NEW_FENCE, |p: &mut UnixcallGenericObjObjRet| p.handle = device).ret
    }

    fn MTLDevice_newEvent(device: u64) -> u64 {
        call(slot::MTL_DEVICE_NEW_EVENT, |p: &mut UnixcallGenericObjObjRet| p.handle = device).ret
    }

    fn SharedEventListener_create() -> u64 {
        call(slot::SHARED_EVENT_LISTENER_CREATE, |_: &mut UnixcallGenericObjRet| ()).ret
    }

    fn SharedEventListener_start(event_queue: u64) {
        call(slot::SHARED_EVENT_LISTENER_START, |p: &mut UnixcallGenericObjNoret| p.handle = event_queue);
    }

    fn SharedEventListener_destroy(event_queue: u64) {
        call(slot::SHARED_EVENT_LISTENER_DESTROY, |p: &mut UnixcallGenericObjNoret| p.handle = event_queue);
    }

    fn WMTBootstrapRegister(name: *const c_char, mach_port: u32) -> bool {
        let (_, status) = call_status(slot::WMT_BOOTSTRAP_REGISTER, |p: &mut UnixcallBootstrap| {
            // SAFETY: `name` is a string (the function's contract).
            unsafe { copy_name(&mut p.name, name) };
            p.mach_port = mach_port;
        });
        status == 0
    }

    fn WMTBootstrapLookUp(name: *const c_char, mach_port: *mut u32) -> bool {
        let (p, status) = call_status(slot::WMT_BOOTSTRAP_LOOK_UP, |p: &mut UnixcallBootstrap| {
            // SAFETY: `name` is a string (the function's contract).
            unsafe { copy_name(&mut p.name, name) };
        });
        // SAFETY: valid for a write (the function's contract; the C never checked it).
        unsafe { mach_port.write(p.mach_port) };
        status == 0
    }

    fn MTLSharedEvent_createMachPort(event: u64) -> u32 {
        call(slot::MTL_SHARED_EVENT_CREATE_MACH_PORT, |p: &mut UnixcallMtlsharedeventCreatemachport| {
            p.event = event;
        })
        .ret_mach_port
    }

    fn MTLDevice_newSharedEventWithMachPort(device: u64, mach_port: u32) -> u64 {
        call(
            slot::MTL_DEVICE_NEW_SHARED_EVENT_WITH_MACH_PORT,
            |p: &mut UnixcallMtldeviceNewsharedeventwithmachport| {
                p.device = device;
                p.mach_port = mach_port;
            },
        )
        .ret_event
    }

    fn MTLSharedEvent_waitUntilSignaledValue(event: u64, value: u64, timeout: u64) -> bool {
        call(
            slot::MTL_SHARED_EVENT_WAIT_UNTIL_SIGNALED_VALUE,
            |p: &mut UnixcallMtlsharedeventWaituntilsignaledvalue| {
                p.event = event;
                p.value = value;
                p.timeout_ms = timeout;
            },
        )
        .ret_timeout
            != 0
    }

    fn MTLSharedEvent_setWin32EventAtValues(
        shared_event_listener: u64,
        shared_events: *const u64,
        values: *const u64,
        count: u32,
        needed: u32,
        nt_event_handle: *mut c_void,
    ) {
        call(slot::MTL_SHARED_EVENT_SET_WIN32_EVENT_AT_VALUES, |p: &mut UnixcallMtlsharedeventSeteventatvalues| {
            p.shared_event_listener = shared_event_listener;
            p.shared_events = mem_ptr(shared_events);
            p.values = mem_ptr(values);
            p.count = count;
            p.needed = needed;
            p.event_handle = addr(nt_event_handle);
        });
    }
}
