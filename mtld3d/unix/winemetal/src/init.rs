//! The image's constructors: the forced maximum anisotropy and the `MetalFX` presenter's load.
//!
//! Both run when the image loads, before any handler: `DXMT_MAX_ANISOTROPY` is read into
//! [`FORCED_MAX_ANISOTROPY`], then, with `MACNEUTRON_PRESENT=1`, `libmacneutron-present.dylib` is
//! loaded from beside the image. Wine's loader runs under the hardened runtime, which ignores
//! `DYLD_INSERT_LIBRARIES`, so the image loads the presenter itself, once in each process.

use core::ffi::CStr;
use core::sync::atomic::{AtomicU32, Ordering};
use std::io::Write;

/// `DXMT_MAX_ANISOTROPY=<1..16>`: the floor on a sampler's `maxAnisotropy`, 0 for none.
///
/// For trilinear, non-comparison samplers. Process-wide (§State lives on an object): one
/// environment, read once when the image loads. Exported under the C's name for the C sampler
/// handler.
#[unsafe(export_name = "forced_max_anisotropy")]
pub static FORCED_MAX_ANISOTROPY: AtomicU32 = AtomicU32::new(0);

/// `DXMT_MAX_ANISOTROPY`'s value: `atoi`'s reading of `v` when it is 1 to 16, else 0.
#[must_use]
pub fn parse_max_anisotropy(v: Option<&CStr>) -> u32 {
    // SAFETY: a NUL-terminated string.
    let n = v.map_or(0, |v| unsafe { libc::atoi(v.as_ptr()) });
    u32::try_from(n).ok().filter(|n| (1..=16).contains(n)).unwrap_or(0)
}

/// Whether `MACNEUTRON_PRESENT`'s value `v` asks for the presenter: exactly `"1"`.
#[must_use]
pub fn presenter_wanted(v: Option<&CStr>) -> bool {
    v == Some(c"1")
}

/// The image's constructors, in the C's order. Not state: dyld's list of what to run at load.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CONSTRUCTORS: [extern "C" fn(); 2] = [read_max_anisotropy, load_presenter];

extern "C" fn read_max_anisotropy() {
    let n = parse_max_anisotropy(env(c"DXMT_MAX_ANISOTROPY"));
    FORCED_MAX_ANISOTROPY.store(n, Ordering::Relaxed);
}

/// Loads the presenter from beside this image, or writes the C's line to stderr.
///
/// `dlopen` resolves `@loader_path` from the image that calls it, found by its return address:
/// its result is tested here, so the call is never a tail call, which would return to dyld and
/// resolve the path from there.
extern "C" fn load_presenter() {
    if !presenter_wanted(env(c"MACNEUTRON_PRESENT")) {
        return;
    }
    let path = c"@loader_path/libmacneutron-present.dylib";
    // SAFETY: a NUL-terminated path.
    let image = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if !image.is_null() {
        return;
    }
    // SAFETY: after a failed dlopen on this thread: null or a NUL-terminated message.
    let why = unsafe { libc::dlerror() };
    let why = if why.is_null() {
        c"(null)"
    } else {
        // SAFETY: as above.
        unsafe { CStr::from_ptr(why) }
    };
    let mut line = b"winemetal: can't load the MetalFX presenter: ".to_vec();
    line.extend_from_slice(why.to_bytes());
    line.push(b'\n');
    // The C's fprintf ignored a failed write too.
    let _ = std::io::stderr().write_all(&line);
}

/// An environment variable's value, `None` when unset.
fn env(name: &CStr) -> Option<&'static CStr> {
    // SAFETY: a NUL-terminated name; nothing in the image changes the environment.
    let v = unsafe { libc::getenv(name.as_ptr()) };
    if v.is_null() {
        return None;
    }
    // SAFETY: getenv's value is a NUL-terminated string the environment keeps.
    Some(unsafe { CStr::from_ptr(v) })
}

#[cfg(test)]
mod tests;
