//! NGX's parameter map, laid out as DXMT's `nvngx.hpp` declares it.
//!
//! Games call it through its vtable: eight setters, eight getters and `Reset`, in the order the
//! NGX SDK they were built against calls them. A value keeps the type its setter stored, and a
//! getter of another type answers only the conversions DXMT's `nvngx_parameter.hpp` allows.

use crate::float_cast::{f32_to_i32, f32_to_u32, f64_to_f32, i32_to_f32, u32_to_f32};
use crate::ngx::{FAIL, OK};
use core::ffi::{CStr, c_char, c_void};
use std::collections::BTreeMap;

/// A stored value, by the setter that stored it.
pub enum Value {
    Ptr(*mut c_void),
    D3D12(*mut c_void),
    D3D11(*mut c_void),
    I32(i32),
    U32(u32),
    F64(f64),
    F32(f32),
    U64(u64),
}

impl Value {
    /// As `void *`: a pointer of either resource type, or a `u64`'s bits.
    fn as_ptr(&self) -> Option<*mut c_void> {
        match *self {
            Self::Ptr(p) | Self::D3D12(p) | Self::D3D11(p) => Some(p),
            Self::U64(v) => Some(core::ptr::with_exposed_provenance_mut(usize::try_from(v).ok()?)),
            _ => None,
        }
    }

    /// As `ID3D12Resource *`.
    const fn as_d3d12(&self) -> Option<*mut c_void> {
        match *self {
            Self::D3D12(p) | Self::Ptr(p) => Some(p),
            _ => None,
        }
    }

    /// As `ID3D11Resource *`.
    const fn as_d3d11(&self) -> Option<*mut c_void> {
        match *self {
            Self::D3D11(p) | Self::Ptr(p) => Some(p),
            _ => None,
        }
    }

    /// As `int`: an `unsigned long long`'s low 32 bits, as C++'s cast keeps them.
    fn as_i32(&self) -> Option<i32> {
        match *self {
            Self::I32(v) => Some(v),
            Self::U32(v) => Some(v.cast_signed()),
            Self::F32(v) => Some(f32_to_i32(v)),
            Self::U64(v) => Some(low32(v).cast_signed()),
            _ => None,
        }
    }

    /// As `unsigned int`.
    fn as_u32(&self) -> Option<u32> {
        match *self {
            Self::U32(v) => Some(v),
            Self::I32(v) => Some(v.cast_unsigned()),
            Self::F32(v) => Some(f32_to_u32(v)),
            Self::U64(v) => Some(low32(v)),
            _ => None,
        }
    }

    /// As `double`.
    fn as_f64(&self) -> Option<f64> {
        match *self {
            Self::F64(v) => Some(v),
            Self::F32(v) => Some(f64::from(v)),
            _ => None,
        }
    }

    /// As `float`.
    const fn as_f32(&self) -> Option<f32> {
        match *self {
            Self::F32(v) => Some(v),
            Self::U32(v) => Some(u32_to_f32(v)),
            Self::I32(v) => Some(i32_to_f32(v)),
            Self::F64(v) => Some(f64_to_f32(v)),
            _ => None,
        }
    }

    /// As `unsigned long long`: an `int` sign-extended, as C++'s cast does.
    fn as_u64(&self) -> Option<u64> {
        match *self {
            Self::U64(v) => Some(v),
            Self::Ptr(p) => u64::try_from(p.expose_provenance()).ok(),
            Self::I32(v) => Some(i64::from(v).cast_unsigned()),
            Self::U32(v) => Some(u64::from(v)),
            _ => None,
        }
    }
}

/// The low 32 bits of `v`, which C++'s cast to a 32-bit integer keeps.
fn low32(v: u64) -> u32 {
    u32::try_from(v & 0xffff_ffff).unwrap_or(0)
}

/// The map: its vtable first, as a C++ object with virtual functions.
#[repr(C)]
pub struct Parameters {
    pub vtbl: *const ParametersVtbl,
    values: BTreeMap<Vec<u8>, Value>,
}

type Set<T> = unsafe extern "system" fn(*mut Parameters, *const c_char, T);
type Get<T> = unsafe extern "system" fn(*const Parameters, *const c_char, *mut T) -> u32;

/// The vtable, slot by slot in `nvngx.hpp`'s order (no destructor slot).
#[repr(C)]
pub struct ParametersVtbl {
    pub set_ptr: Set<*mut c_void>,
    pub set_d3d12: Set<*mut c_void>,
    pub set_d3d11: Set<*mut c_void>,
    pub set_i32: Set<i32>,
    pub set_u32: Set<u32>,
    pub set_f64: Set<f64>,
    pub set_f32: Set<f32>,
    pub set_u64: Set<u64>,
    pub get_ptr: Get<*mut c_void>,
    pub get_d3d12: Get<*mut c_void>,
    pub get_f64: Get<f64>,
    pub get_u32: Get<u32>,
    pub get_i32: Get<i32>,
    pub get_d3d11: Get<*mut c_void>,
    pub get_f32: Get<f32>,
    pub get_u64: Get<u64>,
    pub reset: unsafe extern "system" fn(*mut Parameters),
}

/// Stores `value` under `name` (a C string) in `this`.
///
/// # Safety
///
/// `this` is a live map from `Parameters::allocate` and `name` a NUL-terminated string, or null.
unsafe fn store(this: *mut Parameters, name: *const c_char, value: Value) {
    if this.is_null() || name.is_null() {
        return;
    }
    // SAFETY: the caller's: a NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) };
    // SAFETY: the caller's: a live map, which nothing else borrows during the game's call.
    let map = unsafe { &mut *this };
    map.set(name, value);
}

/// Writes `name`'s value, converted by `convert`, to `out`.
///
/// NGX's OK, or FAIL when the name is absent or its value doesn't convert.
///
/// # Safety
///
/// As `store`, and `out` writable or null.
unsafe fn fetch<T>(this: *const Parameters, name: *const c_char, out: *mut T, convert: fn(&Value) -> Option<T>) -> u32 {
    if this.is_null() || name.is_null() || out.is_null() {
        return FAIL;
    }
    // SAFETY: the caller's: a NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) };
    // SAFETY: the caller's: a live map.
    let map = unsafe { &*this };
    map.values.get(name.to_bytes()).and_then(convert).map_or(FAIL, |value| {
        // SAFETY: the caller's: `out` is writable.
        unsafe { out.write(value) };
        OK
    })
}

macro_rules! setter {
    ($name:ident, $t:ty, $variant:ident) => {
        unsafe extern "system" fn $name(this: *mut Parameters, name: *const c_char, value: $t) {
            // SAFETY: the game passes a map it allocated through NGX and a C string.
            unsafe { store(this, name, Value::$variant(value)) }
        }
    };
}

macro_rules! getter {
    ($name:ident, $t:ty, $convert:ident) => {
        unsafe extern "system" fn $name(this: *const Parameters, name: *const c_char, out: *mut $t) -> u32 {
            // SAFETY: as the setters', and the game's `out`.
            unsafe { fetch(this, name, out, Value::$convert) }
        }
    };
}

setter!(set_ptr, *mut c_void, Ptr);
setter!(set_d3d12, *mut c_void, D3D12);
setter!(set_d3d11, *mut c_void, D3D11);
setter!(set_i32, i32, I32);
setter!(set_u32, u32, U32);
setter!(set_f64, f64, F64);
setter!(set_f32, f32, F32);
setter!(set_u64, u64, U64);
getter!(get_ptr, *mut c_void, as_ptr);
getter!(get_d3d12, *mut c_void, as_d3d12);
getter!(get_f64, f64, as_f64);
getter!(get_u32, u32, as_u32);
getter!(get_i32, i32, as_i32);
getter!(get_d3d11, *mut c_void, as_d3d11);
getter!(get_f32, f32, as_f32);
getter!(get_u64, u64, as_u64);

unsafe extern "system" fn reset(this: *mut Parameters) {
    if !this.is_null() {
        // SAFETY: the game passes a map it allocated through NGX.
        unsafe { (*this).values.clear() };
    }
}

static VTBL: ParametersVtbl = ParametersVtbl {
    set_ptr,
    set_d3d12,
    set_d3d11,
    set_i32,
    set_u32,
    set_f64,
    set_f32,
    set_u64,
    get_ptr,
    get_d3d12,
    get_f64,
    get_u32,
    get_i32,
    get_d3d11,
    get_f32,
    get_u64,
    reset,
};

impl Parameters {
    /// A new, empty map, owned by whoever `destroy`s it.
    pub fn allocate() -> *mut Self {
        Box::into_raw(Box::new(Self { vtbl: &raw const VTBL, values: BTreeMap::new() }))
    }

    /// Frees a map `allocate` made; null does nothing.
    pub fn destroy(this: *mut Self) {
        if !this.is_null() {
            // SAFETY: `this` came from `allocate`, and NGX's DestroyParameters is its last use.
            drop(unsafe { Box::from_raw(this) });
        }
    }

    /// Stores `value` under `name`.
    pub fn set(&mut self, name: &CStr, value: Value) {
        self.values.insert(name.to_bytes().to_vec(), value);
    }

    /// `name`'s value as `unsigned int`.
    pub fn u32(&self, name: &CStr) -> Option<u32> {
        self.values.get(name.to_bytes()).and_then(Value::as_u32)
    }

    /// `name`'s value as `int`.
    pub fn i32(&self, name: &CStr) -> Option<i32> {
        self.values.get(name.to_bytes()).and_then(Value::as_i32)
    }

    /// `name`'s value as `float`.
    pub fn f32(&self, name: &CStr) -> Option<f32> {
        self.values.get(name.to_bytes()).and_then(Value::as_f32)
    }

    /// `name`'s value as `ID3D12Resource *`, whichever pointer setter the game used.
    pub fn d3d12(&self, name: &CStr) -> Option<*mut c_void> {
        self.values.get(name.to_bytes()).and_then(Value::as_d3d12)
    }

    /// `name`'s value as `ID3D11Resource *`.
    pub fn d3d11(&self, name: &CStr) -> Option<*mut c_void> {
        self.values.get(name.to_bytes()).and_then(Value::as_d3d11)
    }

    /// `name`'s value as `void *`.
    pub fn ptr(&self, name: &CStr) -> Option<*mut c_void> {
        self.values.get(name.to_bytes()).and_then(Value::as_ptr)
    }
}

#[cfg(test)]
mod tests;
