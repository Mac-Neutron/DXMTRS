//! The SQLite calls the shader caches make, over a connection and a statement that close on drop.
//!
//! Raw FFI on the system library (D1), one operation to an `unsafe` block. A [`Db`] and a [`Stmt`]
//! finalize and close when dropped, so a failed open in the middle of a cache's setup leaves
//! nothing behind; a cache object holds its statement ahead of its connection, which drops the
//! statement first, as the C's `dealloc` finalized before it closed.

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

/// `SQLITE_OPEN_READONLY`.
pub const OPEN_READONLY: c_int = 0x0000_0001;
/// `SQLITE_OPEN_READWRITE`.
pub const OPEN_READWRITE: c_int = 0x0000_0002;
/// `SQLITE_OPEN_CREATE`.
pub const OPEN_CREATE: c_int = 0x0000_0004;
/// `SQLITE_OPEN_NOMUTEX`: one thread at a time uses the connection, the cache object's contract.
pub const OPEN_NOMUTEX: c_int = 0x0000_8000;
/// `SQLITE_ROW`.
pub const ROW: c_int = 100;
/// `SQLITE_DONE`.
pub const DONE: c_int = 101;

const OK: c_int = 0;

/// `sqlite3`, opaque.
#[repr(C)]
struct Sqlite3 {
    opaque: [u8; 0],
}

/// `sqlite3_stmt`, opaque.
#[repr(C)]
struct Sqlite3Stmt {
    opaque: [u8; 0],
}

// The system library, which the images link with `-lsqlite3`; the attribute links it into the
// unit tests' executable too.
#[link(name = "sqlite3")]
unsafe extern "C" {
    fn sqlite3_open_v2(filename: *const c_char, db: *mut *mut Sqlite3, flags: c_int, vfs: *const c_char) -> c_int;
    fn sqlite3_close(db: *mut Sqlite3) -> c_int;
    fn sqlite3_errmsg(db: *mut Sqlite3) -> *const c_char;
    fn sqlite3_exec(
        db: *mut Sqlite3,
        sql: *const c_char,
        callback: Option<unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char, *mut *mut c_char) -> c_int>,
        arg: *mut c_void,
        errmsg: *mut *mut c_char,
    ) -> c_int;
    fn sqlite3_free(p: *mut c_void);
    fn sqlite3_prepare_v2(
        db: *mut Sqlite3,
        sql: *const c_char,
        nbyte: c_int,
        stmt: *mut *mut Sqlite3Stmt,
        tail: *mut *const c_char,
    ) -> c_int;
    fn sqlite3_step(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_reset(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_clear_bindings(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_finalize(stmt: *mut Sqlite3Stmt) -> c_int;
    fn sqlite3_bind_blob64(
        stmt: *mut Sqlite3Stmt,
        index: c_int,
        data: *const c_void,
        len: u64,
        destructor: Option<unsafe extern "C" fn(*mut c_void)>,
    ) -> c_int;
    fn sqlite3_column_blob(stmt: *mut Sqlite3Stmt, column: c_int) -> *const c_void;
    fn sqlite3_column_bytes(stmt: *mut Sqlite3Stmt, column: c_int) -> c_int;
    fn sqlite3_column_text(stmt: *mut Sqlite3Stmt, column: c_int) -> *const u8;
}

/// SQLite's last message for `db` (null: out of memory), copied before another call replaces it.
fn message(db: *mut Sqlite3) -> CString {
    // SAFETY: `sqlite3_errmsg` takes a connection or null and answers a NUL-terminated string.
    let text = unsafe { sqlite3_errmsg(db) };
    // SAFETY: a NUL-terminated string, valid until the next call on this connection.
    unsafe { CStr::from_ptr(text) }.to_owned()
}

/// An open connection, closed on drop.
pub struct Db(NonNull<Sqlite3>);

impl Db {
    /// Opens `path` with `flags`: the connection, or SQLite's message for the failure.
    pub fn open(path: &CStr, flags: c_int) -> Result<Self, CString> {
        let mut db = ptr::null_mut();
        // SAFETY: a NUL-terminated path, a place for the handle, the default VFS.
        let rc = unsafe { sqlite3_open_v2(path.as_ptr(), &raw mut db, flags, ptr::null()) };
        match NonNull::new(db) {
            Some(db) if rc == OK => Ok(Self(db)),
            _ => {
                let why = message(db);
                // SAFETY: the handle `sqlite3_open_v2` left, closed once (null is a no-op).
                unsafe { sqlite3_close(db) };
                Err(why)
            }
        }
    }

    /// Runs `sql`, ignoring any rows: `Err` with SQLite's message on a failure.
    pub fn exec(&self, sql: &CStr) -> Result<(), CString> {
        let mut text = ptr::null_mut();
        // SAFETY: an open connection, a NUL-terminated statement, no callback, a place for the message.
        let rc = unsafe { sqlite3_exec(self.0.as_ptr(), sql.as_ptr(), None, ptr::null_mut(), &raw mut text) };
        if rc == OK {
            return Ok(());
        }
        let why = if text.is_null() {
            c"(null)".to_owned()
        } else {
            // SAFETY: a NUL-terminated string `sqlite3_exec` allocated.
            unsafe { CStr::from_ptr(text) }.to_owned()
        };
        // SAFETY: `sqlite3_exec`'s message, freed once (null is a no-op).
        unsafe { sqlite3_free(text.cast()) };
        Err(why)
    }

    /// Prepares `sql`: the statement, or SQLite's message for the failure.
    ///
    /// The statement must be dropped before the connection.
    pub fn prepare(&self, sql: &CStr) -> Result<Stmt, CString> {
        let mut stmt = ptr::null_mut();
        // SAFETY: an open connection, a NUL-terminated statement, a place for the handle, no tail.
        let rc = unsafe { sqlite3_prepare_v2(self.0.as_ptr(), sql.as_ptr(), -1, &raw mut stmt, ptr::null_mut()) };
        match NonNull::new(stmt) {
            Some(stmt) if rc == OK => Ok(Stmt(stmt)),
            _ => {
                // SAFETY: a statement a failed prepare left (null in practice), finalized once.
                unsafe { sqlite3_finalize(stmt) };
                Err(message(self.0.as_ptr()))
            }
        }
    }

    /// SQLite's last message on this connection.
    pub fn message(&self) -> CString {
        message(self.0.as_ptr())
    }
}

impl Drop for Db {
    fn drop(&mut self) {
        // SAFETY: the connection `open` made, closed once, after its statements.
        unsafe { sqlite3_close(self.0.as_ptr()) };
    }
}

/// A prepared statement, finalized on drop.
pub struct Stmt(NonNull<Sqlite3Stmt>);

impl Stmt {
    /// `sqlite3_reset` then `sqlite3_clear_bindings`: ready to bind again.
    pub fn rewind(&self) {
        self.reset();
        // SAFETY: a live statement.
        unsafe { sqlite3_clear_bindings(self.0.as_ptr()) };
    }

    /// `sqlite3_reset`.
    pub fn reset(&self) {
        // SAFETY: a live statement.
        unsafe { sqlite3_reset(self.0.as_ptr()) };
    }

    /// Binds parameter `index` (1-based) to `len` bytes at `data`, or to NULL when `data` is null.
    ///
    /// This is `sqlite3_bind_blob64` with `SQLITE_STATIC`.
    ///
    /// # Safety
    ///
    /// A non-null `data` points to `len` readable bytes until the statement has been stepped and
    /// reset (or rewound).
    pub unsafe fn bind_blob(&self, index: c_int, data: *const c_void, len: u64) {
        // SAFETY: a live statement; the caller's contract for the bytes; no destructor (static).
        unsafe { sqlite3_bind_blob64(self.0.as_ptr(), index, data, len, None) };
    }

    /// `sqlite3_step`: `ROW`, `DONE` or an error code.
    pub fn step(&self) -> c_int {
        // SAFETY: a live statement.
        unsafe { sqlite3_step(self.0.as_ptr()) }
    }

    /// The current row's column `column` as a blob: its address and byte count.
    ///
    /// The address is null for an empty or NULL value; both are valid until the next step or reset.
    pub fn column_blob(&self, column: c_int) -> (*const c_void, c_int) {
        // The blob first, then its size: SQLite's conversion order.
        // SAFETY: a live statement on a row.
        let data = unsafe { sqlite3_column_blob(self.0.as_ptr(), column) };
        // SAFETY: as above.
        let len = unsafe { sqlite3_column_bytes(self.0.as_ptr(), column) };
        (data, len)
    }

    /// The current row's column `column` as text, `None` for NULL, valid until the next step.
    pub fn column_text(&self, column: c_int) -> Option<&CStr> {
        // SAFETY: a live statement on a row.
        let text = unsafe { sqlite3_column_text(self.0.as_ptr(), column) };
        if text.is_null() {
            return None;
        }
        // SAFETY: a NUL-terminated string, valid until the next step or reset, which `&self` outlasts.
        Some(unsafe { CStr::from_ptr(text.cast()) })
    }
}

impl Drop for Stmt {
    fn drop(&mut self) {
        // SAFETY: the statement `prepare` made, finalized once.
        unsafe { sqlite3_finalize(self.0.as_ptr()) };
    }
}
