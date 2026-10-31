//! The shader cache: translated shaders stored per build and per executable.
//!
//! A [`Store`] is the pair of winemetal objects over one SQLite file: a writer that creates the
//! database and a reader of it. The C++ makes one per Metal version, owns it through the
//! `dxmt_shader_cache_*` functions its header `dxmt_shader_cache.hpp` declares, and frees it with
//! its own destructor. Each object is used under a mutex of its own.

mod abi;

use core::{
    ffi::{c_char, c_void},
    mem::ManuallyDrop,
};
use std::{
    ffi::CString,
    sync::{Mutex, PoisonError},
};

use dxmt_util::env::cache_exe_name;
use winemetal_sys::{
    CacheReader_alloc_init, CacheReader_get, CacheWriter_alloc_init, CacheWriter_set, Obj,
};

/// The translation cache of one Metal version: a writer and a reader, each there if it opened.
pub struct Store {
    // Fields drop in declaration order, and the C++ released the reader before the writer.
    reader: Option<Mutex<Obj>>,
    writer: Option<Mutex<Obj>>,
}

impl Store {
    /// The store the environment asks for.
    ///
    /// It reads `DXMT_SHADER_CACHE` (`0` turns the cache off), `DXMT_SHADER_CACHE_PATH` (an
    /// absolute folder) and `DXMT_CACHE_EXE`. `version` is the table version
    /// `ShaderCacheVersion` computes: a different one drops the table of another build when the
    /// cache opens.
    #[must_use]
    pub fn new(metal_version: u32, version: u64) -> Self {
        let cache = std::env::var("DXMT_SHADER_CACHE").ok();
        let folder = std::env::var("DXMT_SHADER_CACHE_PATH").ok();
        let file = path(
            cache.as_deref(),
            folder.as_deref(),
            &cache_exe_name(),
            metal_version,
        );
        Self::open(file.as_deref(), version)
    }

    /// Opens the database at `file`, or nothing when `None`.
    fn open(file: Option<&str>, version: u64) -> Self {
        let Some(file) = file.and_then(|file| CString::new(file).ok()) else {
            return Self {
                reader: None,
                writer: None,
            };
        };
        // The writer first: it creates the database and its table, and a read-only reader opened
        // on a cold cache would be null for the whole process.
        // SAFETY: `file` is a NUL-terminated path that outlives the call.
        let writer = unsafe { CacheWriter_alloc_init(file.as_ptr().cast::<c_char>(), version) };
        // SAFETY: as above.
        let reader = unsafe { CacheReader_alloc_init(file.as_ptr().cast::<c_char>(), version) };
        // SAFETY: an `alloc_init` handle is null or carries the one reference given here.
        let writer = unsafe { Obj::adopt(writer) };
        // SAFETY: as above.
        let reader = unsafe { Obj::adopt(reader) };
        Self {
            reader: reader.map(Mutex::new),
            writer: writer.map(Mutex::new),
        }
    }

    /// Whether the cache can be read.
    #[must_use]
    pub const fn has_reader(&self) -> bool {
        self.reader.is_some()
    }

    /// Whether the cache can be written.
    #[must_use]
    pub const fn has_writer(&self) -> bool {
        self.writer.is_some()
    }

    /// The data stored under `key`, if the cache can be read and holds it.
    #[must_use]
    pub fn read(&self, key: &[u8]) -> Option<Obj> {
        let reader = self
            .reader
            .as_ref()?
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `reader` is a live reader handle, `key` is readable for its length.
        let data = unsafe {
            CacheReader_get(
                reader.handle(),
                key.as_ptr().cast::<c_void>(),
                key.len() as u64,
            )
        };
        // SAFETY: a `get` handle is null or carries the one reference given here.
        unsafe { Obj::adopt(data) }
    }

    /// Stores `data` under `key`, if the cache can be written.
    pub fn write(&self, key: &[u8], data: &Obj) {
        let Some(writer) = &self.writer else {
            return;
        };
        let writer = writer.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: `writer` is a live writer handle, `key` is readable for its length, `data` is live.
        unsafe {
            CacheWriter_set(
                writer.handle(),
                key.as_ptr().cast::<c_void>(),
                key.len() as u64,
                data.handle(),
            );
        }
    }
}

/// The cache file's name for the environment's three values, or `None` when `cache` is `0`.
///
/// `cache` is `DXMT_SHADER_CACHE`, `folder` is `DXMT_SHADER_CACHE_PATH` and `cache_exe` is the
/// executable's cache name; unset and empty values are the same. The text is the C++'s: built
/// with `/` and not with `Path::join`, which would put a `\` in a path on Windows and so name
/// another file than the Mac's.
#[must_use]
pub fn path(
    cache: Option<&str>,
    folder: Option<&str>,
    cache_exe: &str,
    metal_version: u32,
) -> Option<String> {
    if cache == Some("0") {
        return None;
    }
    let mut folder = match folder {
        Some(folder) if folder.starts_with('/') => folder.to_owned(),
        _ => format!("dxmt/{cache_exe}/"),
    };
    if !folder.ends_with('/') {
        folder.push('/');
    }
    Some(format!("{folder}shaders_{metal_version}.db"))
}

/// Aborts the process when dropped during a panic.
///
/// Natively the exports are `"C-unwind"`, so that an Objective-C exception from winemetal reaches
/// the caller's `@catch`; a Rust panic must not follow it into C++.
#[cfg(not(windows))]
struct AbortOnPanic;

#[cfg(not(windows))]
impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort();
        }
    }
}

/// Defines a C export: `extern "C"` on Windows, an aborting `extern "C-unwind"` natively.
macro_rules! export {
    ($(#[$meta:meta])* fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)? $body:block) => {
        $(#[$meta])*
        #[cfg(windows)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($($arg: $ty),*) $(-> $ret)? $body

        $(#[$meta])*
        #[cfg(not(windows))]
        #[unsafe(no_mangle)]
        pub unsafe extern "C-unwind" fn $name($($arg: $ty),*) $(-> $ret)? {
            let _guard = AbortOnPanic;
            $body
        }
    };
}

export! {
    /// A store for `metal_version`, writer opened first, then reader.
    ///
    /// # Safety
    ///
    /// None; the store is freed by [`dxmt_shader_cache_destroy`].
    fn dxmt_shader_cache_create(metal_version: u32, version: u64) -> *mut c_void {
        Box::into_raw(Box::new(Store::new(metal_version, version))).cast::<c_void>()
    }
}

export! {
    /// Releases the reader, then the writer, and frees the store.
    ///
    /// # Safety
    ///
    /// `store` is a store [`dxmt_shader_cache_create`] returned and nothing has destroyed.
    fn dxmt_shader_cache_destroy(store: *mut c_void) {
        // SAFETY: the caller's promise: a live box, given back here.
        drop(unsafe { Box::from_raw(store.cast::<Store>()) });
    }
}

export! {
    /// Whether the cache can be read.
    ///
    /// # Safety
    ///
    /// `store` is a live store from [`dxmt_shader_cache_create`].
    fn dxmt_shader_cache_has_reader(store: *mut c_void) -> bool {
        // SAFETY: the caller's promise.
        unsafe { &*store.cast::<Store>() }.has_reader()
    }
}

export! {
    /// Whether the cache can be written.
    ///
    /// # Safety
    ///
    /// `store` is a live store from [`dxmt_shader_cache_create`].
    fn dxmt_shader_cache_has_writer(store: *mut c_void) -> bool {
        // SAFETY: the caller's promise.
        unsafe { &*store.cast::<Store>() }.has_writer()
    }
}

export! {
    /// The data stored under the `len` bytes at `key` as a handle with its own reference, or 0.
    ///
    /// # Safety
    ///
    /// `store` is a live store from [`dxmt_shader_cache_create`]; `key` is readable for `len`
    /// bytes.
    fn dxmt_shader_cache_read(store: *mut c_void, key: *const c_void, len: u64) -> u64 {
        let Ok(len) = usize::try_from(len) else {
            return 0;
        };
        // SAFETY: the caller's promise.
        let store = unsafe { &*store.cast::<Store>() };
        // SAFETY: the caller's promise.
        let key = unsafe { core::slice::from_raw_parts(key.cast::<u8>(), len) };
        // The reference goes to the caller, so the `Obj` must not release it.
        store
            .read(key)
            .map_or(0, |data| ManuallyDrop::new(data).handle())
    }
}

export! {
    /// Stores `data` under the `len` bytes at `key`; the caller keeps its reference to `data`.
    ///
    /// # Safety
    ///
    /// `store` is a live store from [`dxmt_shader_cache_create`]; `key` is readable for `len`
    /// bytes; `data` is a live object's handle.
    fn dxmt_shader_cache_write(store: *mut c_void, key: *const c_void, len: u64, data: u64) {
        let Ok(len) = usize::try_from(len) else {
            return;
        };
        // SAFETY: the caller's promise.
        let store = unsafe { &*store.cast::<Store>() };
        // SAFETY: the caller's promise.
        let key = unsafe { core::slice::from_raw_parts(key.cast::<u8>(), len) };
        // SAFETY: the caller's promise; the caller's reference is not ours, so it is never dropped.
        let data = ManuallyDrop::new(unsafe { Obj::adopt(data) });
        if let Some(data) = data.as_ref() {
            store.write(key, data);
        }
    }
}

#[cfg(test)]
mod tests;
