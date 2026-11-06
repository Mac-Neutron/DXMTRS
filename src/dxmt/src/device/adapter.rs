//! The adapter a D3D device is made on: the application's, or adapter 0 of a new DXGI factory.
//!
//! `D3D11CreateDevice` and `D3D12CreateDevice` both fall back to that default adapter, and D3D12
//! then asks it for DXMT's `IMTLDXGIAdapter`. Each front end logs its own lines and maps a
//! [`Failure`] to its own `HRESULT`, so the answers stay theirs.

use core::{ffi::c_void, ptr};

use dxmt_util::{
    com::{
        self, Guid, Hresult,
        iid::{IID_IDXGI_ADAPTER1, IID_IDXGI_FACTORY1},
    },
    dxgi_vtbl::IDXGIFactory1Vtbl,
    ext::IID_IMTL_DXGI_ADAPTER,
};

unsafe extern "system" {
    fn CreateDXGIFactory1(riid: *const Guid, factory: *mut *mut c_void) -> Hresult;
}

/// Why no adapter came out.
#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    /// `CreateDXGIFactory1` answered this, or success with no factory.
    Factory(Hresult),
    /// The new factory's `EnumAdapters(0)` answered this, or success with no adapter.
    NoAdapter(Hresult),
    /// The adapter does not answer `IMTLDXGIAdapter`: not DXMT's.
    NotDxmt,
}

/// Adapter 0 of a new DXGI factory: the factory and the adapter, a reference to each.
///
/// `CreateDXGIFactory1`, then `EnumAdapters(0)`; both references are the caller's.
/// When the factory has no adapter, its reference is given back before the answer.
///
/// # Errors
///
/// [`Failure::Factory`] or [`Failure::NoAdapter`], with the call's `HRESULT`.
///
/// # Safety
///
/// `CreateDXGIFactory1` resolves to DXGI's (DXMT's `dxgi.dll` or `dxgi.dylib`).
pub unsafe fn default_adapter() -> Result<(*mut c_void, *mut c_void), Failure> {
    let mut factory = ptr::null_mut();
    // SAFETY: the IID is a live GUID and `factory` a local the call writes.
    let hr = unsafe { CreateDXGIFactory1(&IID_IDXGI_FACTORY1, &raw mut factory) };
    if hr < 0 || factory.is_null() {
        return Err(Failure::Factory(hr));
    }
    // SAFETY: `factory` is a live `IDXGIFactory1`.
    let slots = unsafe { com::vtbl::<IDXGIFactory1Vtbl>(factory) };
    let mut adapter = ptr::null_mut();
    // SAFETY: `factory` is live and `adapter` a local the call writes.
    let hr = unsafe { (slots.enum_adapters)(factory, 0, &raw mut adapter) };
    if hr < 0 || adapter.is_null() {
        // SAFETY: the reference `CreateDXGIFactory1` gave.
        unsafe { com::release(factory) };
        return Err(Failure::NoAdapter(hr));
    }
    Ok((factory, adapter))
}

/// `adapter`'s `IMTLDXGIAdapter`, a reference for the caller (`D3D12CreateDevice`'s order).
///
/// A null `adapter` is [`default_adapter`]'s, whose factory is given back at once. Otherwise the
/// adapter's `IDXGIAdapter1` is asked first, and the adapter itself when it answers none. Every
/// reference taken on the way is given back.
///
/// # Errors
///
/// [`default_adapter`]'s, or [`Failure::NotDxmt`].
///
/// # Safety
///
/// `adapter` is null or a live COM object; `CreateDXGIFactory1` as [`default_adapter`].
pub unsafe fn resolve(adapter: *mut c_void) -> Result<*mut c_void, Failure> {
    let (dxgi_adapter, owned) = if adapter.is_null() {
        // SAFETY: the caller's.
        let (factory, adapter) = unsafe { default_adapter() }?;
        // SAFETY: the factory's reference, which nothing here needs.
        unsafe { com::release(factory) };
        (adapter, true)
    } else {
        // SAFETY: the caller's: `adapter` is a live COM object.
        unsafe { com::query(adapter, &IID_IDXGI_ADAPTER1) }.map_or((adapter, false), |a| (a, true))
    };
    // SAFETY: `dxgi_adapter` is live: the caller's, or a reference taken above.
    let mtl_adapter = unsafe { com::query(dxgi_adapter, &IID_IMTL_DXGI_ADAPTER) };
    if owned {
        // SAFETY: the reference taken above.
        unsafe { com::release(dxgi_adapter) };
    }
    mtl_adapter.ok_or(Failure::NotDxmt)
}

#[cfg(test)]
mod tests;
