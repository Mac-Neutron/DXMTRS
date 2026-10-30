//! The shader caches (SQLite) and Metal's own shader cache path.
//!
//! Slots 115 to 118 open a reader or a writer over a SQLite file and look a blob up in it or put
//! one in; slot 119 points Metal's own shader cache at a folder. The database is a seam (D18): a
//! table `cache_<version>` of `key BLOB PRIMARY KEY, value BLOB NOT NULL`, every other `cache_*`
//! table dropped (then `VACUUM`) by a writer of a new version, WAL with `synchronous=NORMAL`,
//! an exclusive `flock` on `<db>-lock` while a writer opens and migrates, a reader opened
//! `READONLY|NOMUTEX` and `INSERT OR REPLACE`. A relative path lives under the user cache folder.
//!
//! A reader and a writer are Objective-C objects (slot 1 releases them like any handle), of
//! classes defined here: `CacheReader` and `CacheWriter`, or with the `native` feature
//! `DXMTNativeCacheReader` and `DXMTNativeCacheWriter`, since the Wine image's classes load beside
//! them in the VM host. Both register when the image loads, as the C's `@implementation`s did.
//! Each holds one connection and one prepared statement, used by one thread at a time.
//!
//! Slots 116 and 118 send the object a message rather than calling into it, so a handle of the
//! wrong class raises `unrecognized selector`, as the C's sends did.

mod sqlite;

use core::{
    ffi::{CStr, c_char, c_void},
    ptr::{self, NonNull},
};

use dispatch2::{DispatchData, DispatchRetained};
use objc2::{
    AnyThread, ClassType, DefinedClass, Message, define_class, extern_class, extern_methods,
    rc::{Allocated, PartialInit, Retained},
    runtime::{AnyObject, NSObject},
};
use objc2_foundation::{NSFileManager, NSString, NSUTF8StringEncoding, ns_string};
use sqlite::{Db, Stmt};
use winemetal_wire::{
    MemPtr, UnixcallCacheAllocInit, UnixcallCacheGet, UnixcallCacheSet, UnixcallSetmetalcachepath,
};

use crate::{
    ffi::{self, AbortOnPanic, STATUS_SUCCESS},
    float_cast::{i32_to_usize, u64_to_usize},
    handle,
};

/// `PATH_MAX`: the buffer `confstr` fills with the user cache folder.
const PATH_MAX: usize = libc::PATH_MAX.cast_unsigned() as usize;

/// What a cache object holds: its statement, then its connection.
///
/// A statement and its connection drop in this order, as the C's `dealloc` finalized then closed.
pub struct Conn {
    stmt: Stmt,
    db: Db,
}

/// Defines the reader and writer classes under the names of this build.
macro_rules! cache_classes {
    ($reader:literal, $writer:literal) => {
        define_class!(
            /// A shader cache opened for reading: a connection and its lookup statement.
            #[unsafe(super(NSObject))]
            #[name = $reader]
            #[ivars = Conn]
            pub struct CacheReader;

            impl CacheReader {
                /// The value for the `length` bytes at `key`: a new dispatch data object, or null.
                ///
                /// # Safety
                ///
                /// A non-null `key` points to `length` readable bytes.
                #[unsafe(method(getKey:length:))]
                unsafe fn get(&self, key: *const c_void, length: usize) -> *mut AnyObject {
                    // SAFETY: the caller's contract.
                    unsafe { self.lookup(key, length) }
                }
            }
        );

        define_class!(
            /// A shader cache opened for writing: a connection and its insert statement.
            #[unsafe(super(NSObject))]
            #[name = $writer]
            #[ivars = Conn]
            pub struct CacheWriter;

            impl CacheWriter {
                /// Stores the dispatch data `value` under the `length` bytes at `key`.
                ///
                /// # Safety
                ///
                /// A non-null `key` points to `length` readable bytes; `value` is null or a
                /// dispatch data object.
                #[unsafe(method(setKey:length:value:))]
                unsafe fn set(&self, key: *const c_void, length: usize, value: *const AnyObject) {
                    // SAFETY: the caller's contract.
                    unsafe { self.store(key, length, value) }
                }
            }
        );
    };
}

#[cfg(not(feature = "native"))]
cache_classes!("CacheReader", "CacheWriter");
#[cfg(feature = "native")]
cache_classes!("DXMTNativeCacheReader", "DXMTNativeCacheWriter");

extern_class!(
    /// Any object a cache handle names: the receiver of the cache messages, whatever its class.
    ///
    /// The send goes through the Objective-C runtime, so an object that is not a cache raises.
    #[unsafe(super(NSObject))]
    #[name = "NSObject"]
    struct CacheObject;
);

impl CacheObject {
    extern_methods!(
        /// `-getKey:length:`, the reader's lookup.
        #[unsafe(method(getKey:length:))]
        unsafe fn get_key(&self, key: *const c_void, length: usize) -> *mut AnyObject;

        /// `-setKey:length:value:`, the writer's store.
        #[unsafe(method(setKey:length:value:))]
        unsafe fn set_key(&self, key: *const c_void, length: usize, value: *const AnyObject);

        /// `-init` on an object whose instance variables are set, as a raw pointer.
        #[unsafe(method(init))]
        #[unsafe(method_family = none)]
        unsafe fn init_raw(this: *mut Self) -> *mut Self;
    );
}

extern_class!(
    /// `NSData`, re-declared to wrap a key over a pointer that may be null.
    ///
    /// The C built one of these around every key, before any send: Foundation raises for a length
    /// above 2^63, and an empty one answers a null `bytes` whatever the pointer.
    #[unsafe(super(NSObject))]
    #[name = "NSData"]
    struct KeyData;
);

impl KeyData {
    extern_methods!(
        /// `-initWithBytesNoCopy:length:freeWhenDone:`, which raises for an absurd length.
        #[unsafe(method(initWithBytesNoCopy:length:freeWhenDone:))]
        unsafe fn init_no_copy(
            this: Allocated<Self>,
            bytes: *const c_void,
            length: usize,
            free_when_done: bool,
        ) -> Option<Retained<Self>>;

        /// `-bytes`.
        #[unsafe(method(bytes))]
        fn bytes(&self) -> *const c_void;

        /// `-length`.
        #[unsafe(method(length))]
        fn length(&self) -> usize;
    );

    /// Wraps the `length` bytes at `key` without copying them, as the C did.
    ///
    /// `None` if Foundation answers nil.
    ///
    /// # Safety
    ///
    /// A non-null `key` points to `length` readable bytes while the result lives.
    unsafe fn wrap(key: *const c_void, length: usize) -> Option<Retained<Self>> {
        // SAFETY: the caller's contract; the bytes are not copied, nor freed.
        unsafe { Self::init_no_copy(Self::alloc(), key, length, false) }
    }

    /// The bytes and length of a key, `(null, 0)` for no object (a message to nil).
    fn parts(this: Option<&Self>) -> (*const c_void, usize) {
        this.map_or((ptr::null(), 0), |this| (this.bytes(), this.length()))
    }
}

extern_class!(
    /// `NSString`, re-declared to send `-stringByAppendingPathComponent:` a nil component.
    ///
    /// The C's `initWithCString:` answers nil for invalid UTF-8, and its path then goes to this
    /// message as nil (which answers the receiver).
    #[unsafe(super(NSObject))]
    #[name = "NSString"]
    struct PathString;
);

impl PathString {
    extern_methods!(
        /// `-stringByAppendingPathComponent:`, nil allowed.
        #[unsafe(method(stringByAppendingPathComponent:))]
        fn append_component(&self, component: Option<&NSString>) -> Retained<NSString>;
    );
}

// `NSLog` takes a format and the arguments it names.
unsafe extern "C-unwind" {
    fn NSLog(format: &NSString, ...);
}

// Metal's own shader cache folder (private API, as the C declared them).
unsafe extern "C-unwind" {
    fn MTLSetShaderCachePath(path: Option<&NSString>);
    fn MTLGetShaderCachePath() -> *const NSString;
}

/// `NSLog(text)`: `text` holds no format specifier.
fn log(text: &str) {
    let format = NSString::from_str(text);
    // SAFETY: a format without arguments.
    unsafe { NSLog(&format) };
}

/// `NSLog(format, message)`: `format` holds one `%s`.
fn log_message(format: &str, message: &CStr) {
    let format = NSString::from_str(format);
    // SAFETY: the format takes one C string.
    unsafe { NSLog(&format, message.as_ptr()) };
}

/// `NSLog(format, object)`: `format` holds one `%@`.
fn log_object(format: &str, object: &NSString) {
    let format = NSString::from_str(format);
    // SAFETY: the format takes one object.
    unsafe { NSLog(&format, ptr::from_ref(object)) };
}

/// A statement's text as a C string; every text this file builds holds no NUL.
fn sql(text: String) -> std::ffi::CString {
    std::ffi::CString::new(text).unwrap_or_default()
}

/// `table`'s name for cache `version`: `cache_<version>`.
fn table_name(version: u64) -> String {
    format!("cache_{version}")
}

/// The address a `WMTMemoryPointer` holds, as a pointer.
const fn mem_ptr(p: MemPtr) -> *const c_void {
    ptr::with_exposed_provenance(u64_to_usize(p.ptr))
}

/// `[[NSString alloc] initWithCString:p encoding:NSUTF8StringEncoding]`: nil for invalid UTF-8.
///
/// A null `p` is nil as well, where the C faulted.
fn utf8_string(p: MemPtr) -> Option<Retained<NSString>> {
    // SAFETY: a NUL-terminated string the caller keeps for the call, or null.
    let text = unsafe { ffi::mem_cstr(p) }?;
    let text = NonNull::new(text.as_ptr().cast_mut())?;
    // SAFETY: a valid NUL-terminated string.
    unsafe { NSString::initWithCString_encoding(NSString::alloc(), text, NSUTF8StringEncoding) }
}

/// The file system representation of `path`, valid while `path` and the autorelease pool live.
fn fs_path(path: &NSString) -> &CStr {
    // SAFETY: a NUL-terminated string `path` owns.
    unsafe { CStr::from_ptr(path.fileSystemRepresentation().as_ptr()) }
}

/// `path` resolved: absolute as is, else under the user cache folder, and its folder made.
///
/// The folder is the parent of the file (`path_is_file`) or `path` itself. `None` when the cache
/// folder is unknown, or `path` is nil and the folder's name is not UTF-8.
fn resolve_cache_dir(path: Option<&NSString>, path_is_file: bool) -> Option<Retained<NSString>> {
    let resolved = if path.is_some_and(|path| path.hasPrefix(ns_string!("/"))) {
        path.map(NSString::retain)
    } else {
        let mut buf: [c_char; PATH_MAX] = [0; PATH_MAX];
        // SAFETY: a buffer of its own length.
        let len =
            unsafe { libc::confstr(libc::_CS_DARWIN_USER_CACHE_DIR, buf.as_mut_ptr(), buf.len()) };
        if len == 0 {
            return None;
        }
        // SAFETY: `confstr` NUL-terminates what it writes, truncated or not.
        let base = unsafe { NSString::stringWithUTF8String(NonNull::new(buf.as_mut_ptr())?) };
        base.map(|base| {
            // SAFETY: two declarations of `NSString`.
            let base = unsafe { &*ptr::from_ref(&*base).cast::<PathString>() };
            base.append_component(path)
        })
    }?;
    let folder = if path_is_file {
        resolved.stringByDeletingLastPathComponent()
    } else {
        resolved.clone()
    };
    // The C ignored the result: a failure shows when the database opens.
    // SAFETY: no attributes.
    let _ = unsafe {
        NSFileManager::defaultManager()
            .createDirectoryAtPath_withIntermediateDirectories_attributes_error(&folder, true, None)
    };
    Some(resolved)
}

/// An open `flock` on a file, taken exclusive; unlocked and closed on drop.
struct FileLock(libc::c_int);

impl FileLock {
    /// Opens (creating) `path` and takes `LOCK_EX`, waiting for the holder; `None` if no open.
    fn exclusive(path: &CStr) -> Option<Self> {
        // SAFETY: a NUL-terminated path.
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CREAT, 0o666_u32) };
        if fd < 0 {
            return None;
        }
        // SAFETY: an open descriptor. The C ignored the result.
        unsafe { libc::flock(fd, libc::LOCK_EX) };
        Some(Self(fd))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // SAFETY: the descriptor `exclusive` opened.
        unsafe { libc::flock(self.0, libc::LOCK_UN) };
        // SAFETY: as above, closed once.
        unsafe { libc::close(self.0) };
    }
}

/// A cache object of class `T` holding `conn`, +1: its instance variables set, `-init` sent.
fn construct<T>(conn: Conn) -> Option<Retained<T>>
where
    T: DefinedClass<Ivars = Conn> + ClassType<ThreadKind = dyn AnyThread>,
{
    let allocated: Allocated<T> = T::alloc();
    let mut partial: PartialInit<T> = allocated.set_ivars(conn);
    let raw = PartialInit::as_mut_ptr(&mut partial);
    // The reference passes to the pointer; `init` on an NSObject subclass answers the receiver.
    core::mem::forget(partial);
    // SAFETY: an allocated object of an NSObject subclass whose instance variables are set.
    let raw = unsafe { CacheObject::init_raw(raw.cast()) };
    // SAFETY: the +1 reference `alloc` gave, passed through `init`.
    unsafe { Retained::from_raw(raw.cast()) }
}

impl CacheReader {
    /// Opens the database `path` names for reading `cache_<version>`; nil, logged, on failure.
    fn open(path: Option<&NSString>, version: u64) -> Option<Retained<Self>> {
        let Some(db_path) = resolve_cache_dir(path, true) else {
            log("[CacheReader] Failed to resolve cache path");
            return None;
        };
        let table = table_name(version);
        let db = match Db::open(
            fs_path(&db_path),
            sqlite::OPEN_READONLY | sqlite::OPEN_NOMUTEX,
        ) {
            Ok(db) => db,
            Err(why) => {
                log_message("[CacheReader] Failed to open DB: %s", &why);
                return None;
            }
        };
        let stmt = match db.prepare(&sql(format!("SELECT value FROM {table} WHERE key = ?;"))) {
            Ok(stmt) => stmt,
            Err(why) => {
                log_message("[CacheReader] Failed to prepare SELECT: %s", &why);
                return None;
            }
        };
        construct(Conn { stmt, db })
    }

    /// The value under the `length` bytes at `key` as a new dispatch data object, null on a miss.
    ///
    /// # Safety
    ///
    /// A non-null `key` points to `length` readable bytes.
    unsafe fn lookup(&self, key: *const c_void, length: usize) -> *mut AnyObject {
        let stmt = &self.ivars().stmt;
        stmt.rewind();
        // SAFETY: the caller's contract; the statement is reset before this returns.
        unsafe { stmt.bind_blob(1, key, length as u64) };
        let found = if stmt.step() == sqlite::ROW {
            let (data, len) = stmt.column_blob(0);
            copy_to_dispatch_data(data, i32_to_usize(len))
        } else {
            ptr::null_mut()
        };
        stmt.reset();
        found
    }
}

impl CacheWriter {
    /// Opens (creating) the database `path` names for writing `cache_<version>`, migrating it.
    ///
    /// Under the exclusive lock on `<db>-lock`, the other `cache_*` tables go, and `VACUUM` gives
    /// their space back. Nil, with a log line, on failure.
    fn open(path: Option<&NSString>, version: u64) -> Option<Retained<Self>> {
        let Some(db_path) = resolve_cache_dir(path, true) else {
            log("[CacheWriter] Failed to resolve cache path");
            return None;
        };
        let lock_path = db_path.stringByAppendingString(ns_string!("-lock"));
        let Some(lock) = FileLock::exclusive(fs_path(&lock_path)) else {
            log_object(
                "[CacheWriter] Failed to open file for locking %@",
                &lock_path,
            );
            return None;
        };
        let flags = sqlite::OPEN_READWRITE | sqlite::OPEN_CREATE | sqlite::OPEN_NOMUTEX;
        let db = match Db::open(fs_path(&db_path), flags) {
            Ok(db) => db,
            Err(why) => {
                log_message("[CacheWriter] Failed to open DB: %s", &why);
                return None;
            }
        };

        // The results are ignored: a failure shows in the statements below.
        let _ = db.exec(c"PRAGMA journal_mode=WAL;");
        let _ = db.exec(c"PRAGMA synchronous=NORMAL;");
        let table = table_name(version);
        let create = format!(
            "CREATE TABLE IF NOT EXISTS {table} (key BLOB PRIMARY KEY, value BLOB NOT NULL);"
        );
        if let Err(why) = db.exec(&sql(create)) {
            log_message("[CacheWriter] Failed to create table: %s", &why);
        }
        drop_other_versions(&db, &table);

        drop(lock);
        let stmt = match db.prepare(&sql(format!(
            "INSERT OR REPLACE INTO {table} (key, value) VALUES (?, ?);"
        ))) {
            Ok(stmt) => stmt,
            Err(why) => {
                log_message("[CacheWriter] Failed to prepare INSERT: %s", &why);
                return None;
            }
        };
        construct(Conn { stmt, db })
    }

    /// Stores the contents of the dispatch data `value` under the `length` bytes at `key`.
    ///
    /// A null `value` stores nothing (the insert is refused, as for an empty value).
    ///
    /// # Safety
    ///
    /// A non-null `key` points to `length` readable bytes; `value` is null or a dispatch data
    /// object.
    unsafe fn store(&self, key: *const c_void, length: usize, value: *const AnyObject) {
        let conn = self.ivars();
        conn.stmt.rewind();
        // SAFETY: the caller's contract; the statement is reset before this returns.
        unsafe { conn.stmt.bind_blob(1, key, length as u64) };

        let mut bytes = ptr::null();
        let mut size = 0_usize;
        // SAFETY: the caller's contract: null or a live dispatch data object.
        let value = unsafe { value.cast::<DispatchData>().as_ref() };
        // SAFETY: the places are valid; `bytes` stays valid until `flat` is dropped.
        let flat = value.map(|value| unsafe { value.map(&raw mut bytes, &raw mut size) });
        // SAFETY: `size` bytes at `bytes` (or none), alive until `flat` drops after the step.
        unsafe { conn.stmt.bind_blob(2, bytes, size as u64) };

        if conn.stmt.step() != sqlite::DONE {
            log_message("[CacheWriter] Failed to insert: %s", &conn.db.message());
        }
        drop(flat);
        conn.stmt.reset();
    }
}

/// Drops every `cache_*` table but `keep`, then `VACUUM`s if there was one.
fn drop_other_versions(db: &Db, keep: &str) {
    let mut stale = Vec::new();
    if let Ok(list) =
        db.prepare(c"SELECT name FROM sqlite_master WHERE type = 'table' AND name GLOB 'cache_*';")
    {
        while list.step() == sqlite::ROW {
            // A name that is not UTF-8 is left alone (the C raised on it).
            let name = list.column_text(0).and_then(|name| name.to_str().ok());
            if let Some(name) = name.filter(|name| *name != keep) {
                stale.push(name.to_owned());
            }
        }
    }
    for name in &stale {
        let _ = db.exec(&sql(format!("DROP TABLE \"{name}\";")));
    }
    if !stale.is_empty() {
        let _ = db.exec(c"VACUUM;");
    }
}

/// A new dispatch data object holding a copy of the `len` bytes at `data`, as a +1 object pointer.
fn copy_to_dispatch_data(data: *const c_void, len: usize) -> *mut AnyObject {
    // A null buffer is the empty object, whatever the length.
    let len = if data.is_null() { 0 } else { len };
    let buffer = NonNull::new(data.cast_mut()).unwrap_or(NonNull::dangling());
    // SAFETY: `len` readable bytes at `buffer` (none when empty); the default destructor (null)
    // copies them, so the object outlives them.
    let copy = unsafe { DispatchData::new(buffer, len, None, ptr::null_mut()) };
    DispatchRetained::into_raw(copy).as_ptr().cast()
}

/// Slot 115: opens a cache reader over the file at `path`.
///
/// # Safety
///
/// `obj` is a live `unixcall_cache_alloc_init` block.
#[unsafe(export_name = "_CacheReader_alloc_init")]
pub unsafe extern "C-unwind" fn cache_reader_alloc_init(obj: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let params = unsafe { ffi::params::<UnixcallCacheAllocInit>(obj) };
    let path = utf8_string(params.path);
    params.ret_cache = handle::ret_owned(CacheReader::open(path.as_deref(), params.version));
    STATUS_SUCCESS
}

/// Slot 116: looks `key` up in a cache reader: the value as a new dispatch data object, else 0.
///
/// # Safety
///
/// `obj` is a live `unixcall_cache_get` block.
#[unsafe(export_name = "_CacheReader_get")]
pub unsafe extern "C-unwind" fn cache_reader_get(obj: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let params = unsafe { ffi::params::<UnixcallCacheGet>(obj) };
    // The C wrapped the key before the send, so an absurd length raises even for a nil handle.
    // SAFETY: the caller's key, readable for its length; `key` outlives the send.
    let key = unsafe { KeyData::wrap(mem_ptr(params.key), u64_to_usize(params.key_length)) };
    let (bytes, length) = KeyData::parts(key.as_deref());
    // SAFETY: a non-zero handle names a live object.
    let reader = unsafe { handle::obj::<CacheObject>(params.cache) };
    params.ret_data = reader.map_or(0, |reader| {
        // SAFETY: `bytes` and `length` are the key's; the object answers `getKey:length:` or the
        // send raises.
        let data = unsafe { reader.get_key(bytes, length) };
        // The object is +1: the dispatch data the lookup made.
        handle::ret_passthrough(data)
    });
    STATUS_SUCCESS
}

/// Slot 117: opens a cache writer over the file at `path`, migrating it to `version`.
///
/// # Safety
///
/// `obj` is a live `unixcall_cache_alloc_init` block.
#[unsafe(export_name = "_CacheWriter_alloc_init")]
pub unsafe extern "C-unwind" fn cache_writer_alloc_init(obj: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let params = unsafe { ffi::params::<UnixcallCacheAllocInit>(obj) };
    let path = utf8_string(params.path);
    params.ret_cache = handle::ret_owned(CacheWriter::open(path.as_deref(), params.version));
    STATUS_SUCCESS
}

/// Slot 118: stores a value under `key` in a cache writer.
///
/// # Safety
///
/// `obj` is a live `unixcall_cache_set` block.
#[unsafe(export_name = "_CacheWriter_set")]
pub unsafe extern "C-unwind" fn cache_writer_set(obj: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let params = unsafe { ffi::params::<UnixcallCacheSet>(obj) };
    // The C wrapped the key before the send, so an absurd length raises even for a nil handle.
    // SAFETY: the caller's key, readable for its length; `key` outlives the send.
    let key = unsafe { KeyData::wrap(mem_ptr(params.key), u64_to_usize(params.key_length)) };
    let (bytes, length) = KeyData::parts(key.as_deref());
    // SAFETY: a non-zero handle names a live object.
    let writer = unsafe { handle::obj::<CacheObject>(params.cache) };
    if let Some(writer) = writer {
        let value = ptr::with_exposed_provenance::<AnyObject>(u64_to_usize(params.value_data));
        // SAFETY: `bytes` and `length` are the key's and `value` the caller's; the object answers
        // `setKey:length:value:` or the send raises.
        unsafe { writer.set_key(bytes, length, value) };
    }
    STATUS_SUCCESS
}

/// Slot 119: points Metal's shader cache at the folder `path` names; 1 if Metal then reports it.
///
/// # Safety
///
/// `obj` is a live `unixcall_setmetalcachepath` block.
#[unsafe(export_name = "_WMTSetMetalShaderCachePath")]
pub unsafe extern "C-unwind" fn set_metal_shader_cache_path(obj: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    // SAFETY: the caller's contract.
    let params = unsafe { ffi::params::<UnixcallSetmetalcachepath>(obj) };
    let path = utf8_string(params.path);
    let resolved = resolve_cache_dir(path.as_deref(), false);
    // SAFETY: Metal's setter takes a path or nil.
    unsafe { MTLSetShaderCachePath(resolved.as_deref()) };
    // SAFETY: Metal's getter takes nothing.
    let current = unsafe { MTLGetShaderCachePath() };
    // SAFETY: its path (not retained), or nil.
    let current = unsafe { current.as_ref() };
    // A nil on either side is not equal, as `isEqualToString:` answers.
    let same = current
        .zip(resolved.as_deref())
        .is_some_and(|(current, resolved)| current.isEqualToString(resolved));
    params.ret_success = u64::from(same);
    STATUS_SUCCESS
}

/// The image's constructor for the cache classes: registers both when the image loads.
///
/// The C's `@implementation`s registered with the image. A probe looks the classes up by name
/// before a first call, and an image beside this one (the native dylib in the VM host) registers
/// its own names. Not state: dyld's list of what to run at load.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static REGISTER_CLASSES: extern "C" fn() = register_classes;

extern "C" fn register_classes() {
    let _guard = AbortOnPanic;
    let _ = CacheReader::class();
    let _ = CacheWriter::class();
}

#[cfg(test)]
mod tests;
