//! winemetal's Unix-call table.
//!
//! `winemetal.so` loaded once, its 64-bit `__wine_unix_call_funcs` slots called with winemetal's
//! own parameter structs (`winemetal_thunks.h`). A 32-bit process ([`mark_wow64`]) calls its
//! `__wine_unix_call_wow64_funcs` instead, whose entries for the slots the facade calls take the
//! same structs, SM50's aside: in the VM's guest they mark the process 32-bit, so the memory
//! winemetal hands out lies below 2 GiB, where the i386 PE side writes into it (D3D9 M2b). That
//! table's SM50 entries read a 32-bit PE caller's `*_params32`, and this crate lays out SM50's
//! structs by its own pointer width (`sm50.rs`), so a 64-bit build calls them through the 64-bit
//! table even when marked (`takes_table64`).
//!
//! The library is the one beside the image holding this crate (`mtld3d.so` in Wine's
//! `lib/wine/<arch>-unix`), or `MTLD3D_WINEMETAL` when that names one (tests). Under Wine the
//! libraries it links (`ntdll.so`, `winemac.so` on macOS) are already loaded; outside Wine a test
//! folder holds stand-ins beside it.

use core::ffi::{CStr, c_char, c_int, c_void};
use std::sync::{
    LazyLock,
    atomic::{AtomicBool, Ordering},
};

use crate::wmt;

type Slot = unsafe extern "C" fn(*mut c_void) -> i32;

/// The loaded table.
///
/// Process-wide, as the library is (one `dlopen` handle per process, never closed).
pub struct Table {
    funcs: *const Slot,
    /// `__wine_unix_call_funcs`: `funcs` but in a marked 64-bit build.
    funcs64: *const Slot,
    wow64: bool,
}

// SAFETY: the table is an immutable array of function pointers in a library that is never unloaded.
unsafe impl Send for Table {}
// SAFETY: as above; every slot is callable from any thread (winemetal's own contract).
unsafe impl Sync for Table {}

static TABLE: LazyLock<Result<Table, String>> = LazyLock::new(Table::load);

/// Whether the process is 32-bit.
///
/// The resource is process-wide (one 32-bit address space) and a machine fact, latched once:
/// every entry of mtld3d.so's own WoW64 table sets it first, before any Metal call, and the table
/// reads it once when it loads.
static WOW64: AtomicBool = AtomicBool::new(false);

/// Marks the process 32-bit: the table loads winemetal's WoW64 entries.
///
/// Called before the table's first use.
pub fn mark_wow64() {
    // Every entry of a 32-bit process's table marks: a read, but for the first.
    if !WOW64.load(Ordering::Relaxed) {
        WOW64.store(true, Ordering::Relaxed);
    }
}

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
    fn dladdr(addr: *const c_void, info: *mut DlInfo) -> c_int;
}

#[repr(C)]
struct DlInfo {
    dli_fname: *const c_char,
    dli_fbase: *mut c_void,
    dli_sname: *const c_char,
    dli_saddr: *mut c_void,
}

#[cfg(target_vendor = "apple")]
const RTLD_NOW: c_int = 0x2;
#[cfg(not(target_vendor = "apple"))]
const RTLD_NOW: c_int = 0x2;

fn last_error() -> String {
    // SAFETY: dlerror returns null or a NUL-terminated string valid until the next dl call on this thread.
    let e = unsafe { dlerror() };
    if e.is_null() {
        "unknown dlopen error".to_owned()
    } else {
        unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
    }
}

/// The path of `winemetal.so` to load: `MTLD3D_WINEMETAL`, else the one beside this image.
fn library_path() -> Result<String, String> {
    if let Some(path) = std::env::var_os("MTLD3D_WINEMETAL") {
        return Ok(path.to_string_lossy().into_owned());
    }
    let mut info = DlInfo {
        dli_fname: core::ptr::null(),
        dli_fbase: core::ptr::null_mut(),
        dli_sname: core::ptr::null(),
        dli_saddr: core::ptr::null_mut(),
    };
    // SAFETY: dladdr only reads the address, a function of this image.
    if unsafe { dladdr(library_path as *const c_void, &raw mut info) } == 0
        || info.dli_fname.is_null()
    {
        return Err("dladdr found no image for mtld3d's facade".to_owned());
    }
    // SAFETY: dli_fname is a NUL-terminated path owned by the dynamic loader.
    let image = unsafe { CStr::from_ptr(info.dli_fname) }
        .to_string_lossy()
        .into_owned();
    let dir = image
        .rsplit_once('/')
        .map_or(".", |(dir, _)| dir)
        .to_owned();
    Ok(format!("{dir}/winemetal.so"))
}

impl Table {
    /// The table, loaded at the first call.
    ///
    /// # Errors
    /// The library or its table can't be loaded; the message says why.
    pub fn get() -> Result<&'static Self, &'static str> {
        TABLE.as_ref().map_err(String::as_str)
    }

    fn load() -> Result<Self, String> {
        let path = library_path()?;
        let c_path = std::ffi::CString::new(path.clone())
            .map_err(|_| format!("{path}: a NUL in the path"))?;
        // SAFETY: a plain dlopen of a path; the handle is kept for the process's life.
        let handle = unsafe { dlopen(c_path.as_ptr(), RTLD_NOW) };
        if handle.is_null() {
            return Err(format!("dlopen {path}: {}", last_error()));
        }
        let wow64 = WOW64.load(Ordering::Relaxed);
        let symbol = if wow64 {
            c"__wine_unix_call_wow64_funcs"
        } else {
            c"__wine_unix_call_funcs"
        };
        // SAFETY: a symbol lookup in the handle just opened.
        let funcs = unsafe { dlsym(handle, symbol.as_ptr()) };
        if funcs.is_null() {
            return Err(format!(
                "{path} has no {}: {}",
                symbol.to_string_lossy(),
                last_error()
            ));
        }
        let funcs64 = if wow64 && WIDE {
            // SAFETY: a symbol lookup in the handle just opened.
            let funcs64 = unsafe { dlsym(handle, c"__wine_unix_call_funcs".as_ptr()) };
            if funcs64.is_null() {
                return Err(format!(
                    "{path} has no __wine_unix_call_funcs: {}",
                    last_error()
                ));
            }
            funcs64
        } else {
            funcs
        };
        log::info!(target: "mtld3d::mtl", "winemetal: {path} ({})", symbol.to_string_lossy());
        Ok(Self {
            funcs: funcs.cast(),
            funcs64: funcs64.cast(),
            wow64,
        })
    }

    /// The address of slot `slot`'s function, for a test that checks the table against its symbols.
    #[must_use]
    pub fn slot_address(&self, slot: u32) -> usize {
        // SAFETY: callers ask only for slots the facade uses, all inside the table.
        unsafe { *self.funcs_for(slot).add(slot as usize) as usize }
    }

    /// Calls slot `slot` with `params`, winemetal's parameter struct for it; its NTSTATUS.
    ///
    /// # Safety `params` is that slot's struct, valid for reads and writes, and every pointer and
    /// handle in it is valid for the call.
    pub unsafe fn call<P>(&self, slot: u32, params: &mut P) -> i32 {
        // SAFETY: the caller's contract; the slot index is one the facade uses (inside the table).
        unsafe { (*self.funcs_for(slot).add(slot as usize))((params as *mut P).cast()) }
    }

    /// The table slot `slot` is called through.
    const fn funcs_for(&self, slot: u32) -> *const Slot {
        if takes_table64(slot, self.wow64, WIDE) {
            self.funcs64
        } else {
            self.funcs
        }
    }
}

/// Whether this build's pointers are 64-bit.
const WIDE: bool = size_of::<usize>() == 8;

/// Whether slot `slot` goes through winemetal's 64-bit table.
///
/// In a process marked `wow64`, by a build whose pointers are `wide`. A marked 64-bit build (new
/// `WoW64`'s Unix side) still calls SM50's slots through the 64-bit table: the `WoW64` table
/// converts them from a 32-bit caller's structs, and `sm50.rs` lays them out 64-bit. Skipping the
/// `WoW64` entry is safe. In the VM's guest those entries differ from the 64-bit ones only in
/// setting the process's 32-bit latch, which is process-wide and which the `WoW64` calls that made
/// the device, before any compile, already set; and SM50's output is read only here, on the 64-bit
/// Unix side, so it need not lie below 2 GiB. On macOS the `WoW64` table is the 64-bit one with
/// the converters in place. An i686 build's own table is the 32-bit one and its structs are
/// 32-bit, so it keeps it.
const fn takes_table64(slot: u32, wow64: bool, wide: bool) -> bool {
    !wow64 || (wide && matches!(slot, wmt::SM50_INITIALIZE..=wmt::SM50_FREE_ERROR))
}

/// Calls slot `slot` of the loaded table.
///
/// Panicking if winemetal isn't loadable (every facade object comes from it, so a call without it
/// is a bug in the order of things).
///
/// # Safety
/// As [`Table::call`].
pub unsafe fn call<P>(slot: u32, params: &mut P) -> i32 {
    let table = Table::get().unwrap_or_else(|e| panic!("winemetal is not loaded: {e}"));
    // SAFETY: the caller's contract.
    unsafe { table.call(slot, params) }
}

#[cfg(test)]
mod tests;
