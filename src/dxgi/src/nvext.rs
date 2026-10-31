//! DXMT's NVIDIA extension, on when `DXMT_ENABLE_NVEXT` is `1`.
//!
//! On PE the decision also writes or deletes the registry values NVIDIA's loaders read
//! (`dxgi.cpp`'s `InitializeVendorExtensionNV`), and i686 never turns it on. Natively the variable
//! alone decides: the keys are a VM host's guest's.

#[cfg(windows)]
use core::{ffi::c_void, ptr};
use std::sync::{
    Once,
    atomic::{AtomicBool, Ordering},
};

#[cfg(windows)]
use crate::{
    LOG,
    abi::{HKEY_LOCAL_MACHINE, Hkey, KEY_ALL_ACCESS, REG_DWORD, REG_SZ},
};

/// The keys `init` opens, under `HKEY_LOCAL_MACHINE`, in the C++'s order.
#[cfg(windows)]
const KEYS: [&str; 3] = [
    "SOFTWARE\\NVIDIA Corporation\\Global",
    "SYSTEM\\ControlSet001\\Services\\nvlddmkm",
    "SOFTWARE\\NVIDIA Corporation\\Global\\NGXCore",
];
/// The `REG_DWORD` value of the first two keys.
#[cfg(windows)]
const NVIDIA_VALUE: &str = "{41FCC608-8496-4DEF-B43E-7D9BD675A6FF}";
/// The `REG_SZ` value of the third key, and its text.
#[cfg(windows)]
const FULL_PATH: (&str, &str) = ("FullPath", "C:\\Windows\\System32");

#[cfg(windows)]
unsafe extern "system" {
    fn RegCreateKeyExW(
        key: Hkey,
        sub_key: *const u16,
        reserved: u32,
        class: *const u16,
        options: u32,
        desired: u32,
        security: *const c_void,
        result: *mut Hkey,
        disposition: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        key: Hkey,
        name: *const u16,
        reserved: u32,
        kind: u32,
        data: *const u8,
        size: u32,
    ) -> i32;
    fn RegDeleteValueW(key: Hkey, name: *const u16) -> i32;
    fn RegCloseKey(key: Hkey) -> i32;
}

/// Whether the extension is on: decided at the first call (`DllMain`'s on PE), then latched.
#[must_use]
pub fn enabled() -> bool {
    /// Whether the decision ran.
    ///
    /// The caller has no object to reach: `DllMain` and `DXGIGetDebugInterface1` ask (the C++'s
    /// `nvext_init`).
    static NVEXT: Once = Once::new();
    /// The decision (the C++'s `g_extension_enabled`), written once under `NVEXT`.
    ///
    /// The caller has no object to reach, as `NVEXT`.
    static NVEXT_ON: AtomicBool = AtomicBool::new(false);
    latch(&NVEXT, &NVEXT_ON, || init(&|name| std::env::var(name).ok()))
}

/// Runs `init` once under `once` and keeps its answer in `on`: the answer.
fn latch(once: &Once, on: &AtomicBool, init: impl FnOnce() -> bool) -> bool {
    // `call_once` returns after the first call finished, which orders its store before the load.
    once.call_once(|| on.store(init(), Ordering::Relaxed));
    on.load(Ordering::Relaxed)
}

/// The decision natively: `DXMT_ENABLE_NVEXT` is `1`, as `env` reads it.
#[cfg(not(windows))]
fn init(env: &impl Fn(&str) -> Option<String>) -> bool {
    env("DXMT_ENABLE_NVEXT").as_deref() == Some("1")
}

/// The decision on PE: the three keys open, then the variable sets or deletes the values.
///
/// A key that does not open turns the extension off, with no log line; the values' own results
/// are not looked at.
#[cfg(windows)]
fn init(env: &impl Fn(&str) -> Option<String>) -> bool {
    if cfg!(target_arch = "x86") {
        return false;
    }
    let mut keys = [ptr::null_mut(); 3];
    let opened = KEYS
        .iter()
        .zip(&mut keys)
        .all(|(path, key)| create_key(path, key));
    let on = opened && env("DXMT_ENABLE_NVEXT").as_deref() == Some("1");
    if opened {
        write_values(&keys, on);
    }
    for key in keys.into_iter().filter(|key| !key.is_null()) {
        // SAFETY: a key `create_key` opened, closed once.
        unsafe { RegCloseKey(key) };
    }
    on
}

/// Opens (or creates) `HKEY_LOCAL_MACHINE\<path>` into `key`: whether it did.
#[cfg(windows)]
fn create_key(path: &str, key: &mut Hkey) -> bool {
    let path = wide(path);
    // SAFETY: `path` is NUL-terminated; `key` is the caller's slot; class, security attributes
    // and disposition may be null.
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            path.as_ptr(),
            0,
            ptr::null(),
            0,
            KEY_ALL_ACCESS,
            ptr::null(),
            key,
            ptr::null_mut(),
        )
    };
    status == 0
}

/// Sets the three values (`on`, then logs it) or deletes them, in `keys`' order.
#[cfg(windows)]
fn write_values(keys: &[Hkey; 3], on: bool) {
    let nvidia = wide(NVIDIA_VALUE);
    let full_path = wide(FULL_PATH.0);
    let one = 1u32.to_ne_bytes();
    let system32: Vec<u8> = wide(FULL_PATH.1)
        .iter()
        .flat_map(|unit| unit.to_ne_bytes())
        .collect();
    let values: [(&[u16], u32, &[u8]); 3] = [
        (&nvidia, REG_DWORD, &one),
        (&nvidia, REG_DWORD, &one),
        (&full_path, REG_SZ, &system32),
    ];
    for (&key, (name, kind, data)) in keys.iter().zip(values) {
        if on {
            let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
            // SAFETY: an open key; `name` is NUL-terminated and `data` holds `size` bytes.
            unsafe { RegSetValueExW(key, name.as_ptr(), 0, kind, data.as_ptr(), size) };
        } else {
            // SAFETY: an open key; `name` is NUL-terminated.
            unsafe { RegDeleteValueW(key, name.as_ptr()) };
        }
    }
    if on {
        LOG.info("Vendor extension enabled: NVEXT");
    }
}

/// `text` as a NUL-terminated wide string.
#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests;
