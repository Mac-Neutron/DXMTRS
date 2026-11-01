//! `IDXGIKeyedMutex` tear-off and `KeyedMutex` cross-process sync (`dxmt_keyed_mutex.cpp`).

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use winemetal_sys::{
    MTLDevice_newSharedEvent, MTLDevice_newSharedEventWithMachPort, MTLSharedEvent_createMachPort,
    Obj, WMTBootstrapLookUp, WMTBootstrapRegister,
};

use crate::{
    com::{E_FAIL, Hresult, S_OK},
    context::{Immediate, Mode},
    device::Device,
    kmt,
    resource::Common,
};

const DXGI_ERROR_INVALID_CALL: Hresult = -0x7789_ffff;

/// `D3D11_RESOURCE_MISC_SHARED`.
pub const D3D11_RESOURCE_MISC_SHARED: u32 = 0x2;

/// `D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX`.
pub const D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX: u32 = 0x100;

/// `D3D11_RESOURCE_MISC_SHARED_NTHANDLE`.
pub const D3D11_RESOURCE_MISC_SHARED_NTHANDLE: u32 = 0x800;

/// Reference-counted handle to a [`KeyedMutex`].
pub type KeyedMutexRef = Arc<KeyedMutex>;

/// Cross-process keyed mutex over D3DKMT and `WMT::SharedEvent` (`dxmt::KeyedMutex`).
#[derive(Default)]
pub struct KeyedMutex {
    owned: AtomicBool,
    fence_value: AtomicU64,
    global_kmt: u32,
    local_kmt: u32,
    event: Option<Obj>,
}

impl KeyedMutex {
    /// Creates a new cross-process keyed mutex on `device` (`KeyedMutex::create`).
    #[must_use]
    pub fn create(device: &Obj) -> Option<KeyedMutexRef> {
        // SAFETY: `device.handle()` is a live `MTLDevice` handle.
        let raw_ev = unsafe { MTLDevice_newSharedEvent(device.handle()) };
        // SAFETY: `MTLDevice_newSharedEvent` returns a +1 retained `MTLSharedEvent` or 0.
        let event = unsafe { Obj::adopt(raw_ev) }?;
        // SAFETY: `event.handle()` is a live `MTLSharedEvent` handle.
        let mach_port = unsafe { MTLSharedEvent_createMachPort(event.handle()) };
        if mach_port == 0 {
            return None;
        }
        let mach_port_name = kmt::make_unique_shared_name();
        // SAFETY: `mach_port_name` is a 54-byte NUL-terminated ASCII string.
        let registered = unsafe { WMTBootstrapRegister(mach_port_name.as_ptr().cast(), mach_port) };
        if !registered {
            return None;
        }
        let (global_kmt, local_kmt) = kmt::create_keyed_mutex(&mach_port_name)?;
        Some(Arc::new(Self {
            owned: AtomicBool::new(false),
            fence_value: AtomicU64::new(0),
            global_kmt,
            local_kmt,
            event: Some(event),
        }))
    }

    /// Imports an existing cross-process keyed mutex from `global_kmt` (`KeyedMutex::import`).
    #[must_use]
    pub fn import(device: &Obj, global_kmt: u32) -> Option<KeyedMutexRef> {
        let (local_kmt, mach_port_name) = kmt::open_keyed_mutex(global_kmt)?;
        let mut mach_port = 0u32;
        // SAFETY: `mach_port_name` is a 54-byte NUL-terminated buffer; `mach_port` is a valid out ptr.
        let looked_up =
            unsafe { WMTBootstrapLookUp(mach_port_name.as_ptr().cast(), &raw mut mach_port) };
        if !looked_up {
            kmt::destroy_keyed_mutex(local_kmt);
            return None;
        }
        // SAFETY: `device.handle()` is a live `MTLDevice` handle and `mach_port` came from bootstrap.
        let raw_ev = unsafe { MTLDevice_newSharedEventWithMachPort(device.handle(), mach_port) };
        // SAFETY: `MTLDevice_newSharedEventWithMachPort` returns a +1 retained `MTLSharedEvent` or 0.
        let event = unsafe { Obj::adopt(raw_ev) };
        Some(Arc::new(Self {
            owned: AtomicBool::new(false),
            fence_value: AtomicU64::new(0),
            global_kmt,
            local_kmt,
            event,
        }))
    }

    /// Returns the global D3DKMT handle.
    #[must_use]
    pub const fn global_handle(&self) -> u32 {
        self.global_kmt
    }

    /// Returns the `WMT::SharedEvent` handle.
    #[must_use]
    pub fn shared_event(&self) -> u64 {
        self.event.as_ref().map_or(0, Obj::handle)
    }

    /// Returns the retained `Obj` of the shared event, if any.
    #[must_use]
    pub const fn event_obj(&self) -> Option<&Obj> {
        self.event.as_ref()
    }

    /// Acquires the keyed mutex for `key` within `milliseconds`.
    pub fn acquire(&self, func_acquire: impl FnOnce(u64), key: u64, milliseconds: u32) -> Hresult {
        if self.owned.load(Ordering::Acquire) {
            return E_FAIL;
        }
        let fence = match kmt::acquire_keyed_mutex(self.local_kmt, key, milliseconds) {
            Ok(f) => f,
            Err(hr) => return hr,
        };
        func_acquire(fence);
        self.fence_value.store(fence, Ordering::Relaxed);
        self.owned.store(true, Ordering::Release);
        S_OK
    }

    /// Releases the keyed mutex for `key`.
    pub fn release(&self, func_release: impl FnOnce(u64), key: u64) -> Hresult {
        if !self.owned.load(Ordering::Acquire) {
            return E_FAIL;
        }
        let next_fence = self.fence_value.load(Ordering::Relaxed).wrapping_add(1);
        func_release(next_fence);
        if let Err(hr) = kmt::release_keyed_mutex(self.local_kmt, key, next_fence) {
            return hr;
        }
        self.owned.store(false, Ordering::Release);
        S_OK
    }
}

impl Drop for KeyedMutex {
    fn drop(&mut self) {
        if self.local_kmt != 0 {
            kmt::destroy_keyed_mutex(self.local_kmt);
            self.local_kmt = 0;
        }
        self.global_kmt = 0;
    }
}

/// Executes `IDXGIKeyedMutex::AcquireSync` for a resource's [`Common`].
#[must_use]
pub fn acquire_sync_for_resource(common: &Common, key: u64, milliseconds: u32) -> Hresult {
    let dev = Device::from_iface(common.device());
    if let Some(d) = dev {
        d.mutex.lock();
    }
    let hr = common.keyed_mutex().map_or(DXGI_ERROR_INVALID_CALL, |km| {
        let km_ev = Arc::clone(km);
        km.acquire(
            |fence| {
                if let Some(d) = dev {
                    let imm = d.immediate();
                    if !imm.is_null() {
                        // SAFETY: `imm` is the device's live immediate context under `d.mutex`.
                        let ctx = unsafe { &mut *imm };
                        Immediate::emit_op(ctx, move |enc| {
                            // SAFETY: `km_ev` keeps the `MTLSharedEvent` alive.
                            if let Some(ev) = unsafe { Obj::retain(km_ev.shared_event()) } {
                                enc.wait_event(ev, fence);
                            }
                        });
                        ctx.flush();
                    }
                }
            },
            key,
            milliseconds,
        )
    });
    if let Some(d) = dev {
        d.mutex.unlock();
    }
    hr
}

/// Executes `IDXGIKeyedMutex::ReleaseSync` for a resource's [`Common`].
#[must_use]
pub fn release_sync_for_resource(common: &Common, key: u64) -> Hresult {
    let dev = Device::from_iface(common.device());
    if let Some(d) = dev {
        d.mutex.lock();
    }
    let hr = common.keyed_mutex().map_or(DXGI_ERROR_INVALID_CALL, |km| {
        let km_ev = Arc::clone(km);
        km.release(
            |fence| {
                if let Some(d) = dev {
                    let imm = d.immediate();
                    if !imm.is_null() {
                        // SAFETY: `imm` is the device's live immediate context under `d.mutex`.
                        let ctx = unsafe { &mut *imm };
                        Immediate::emit_op(ctx, move |enc| {
                            // SAFETY: `km_ev` keeps the `MTLSharedEvent` alive.
                            if let Some(ev) = unsafe { Obj::retain(km_ev.shared_event()) } {
                                enc.signal_event(ev, fence);
                            }
                        });
                        ctx.flush();
                    }
                }
            },
            key,
        )
    });
    if let Some(d) = dev {
        d.mutex.unlock();
    }
    hr
}
