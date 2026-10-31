//! The file calls the dump folder makes, on paths in Wine's form.
//!
//! On Windows they are the kernel32 calls `d3d12_dxil_dump.cpp` made, with its flags; natively the
//! path is mapped as `win32_native.h`'s `mn_win32_utf8` maps it (a leading `Z:` drive dropped,
//! each backslash a slash) and the files get its modes (0644, folders 0755).

#[cfg(windows)]
use core::ffi::c_void;
use std::ffi::OsStr;
#[cfg(not(windows))]
use std::fs::{DirBuilder, OpenOptions};
#[cfg(not(windows))]
use std::io::{ErrorKind, Write};
#[cfg(not(windows))]
use std::os::unix::ffi::{OsStrExt, OsStringExt};
#[cfg(not(windows))]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(not(windows))]
use std::path::PathBuf;

#[cfg(windows)]
const GENERIC_WRITE: u32 = 0x4000_0000;
#[cfg(windows)]
const FILE_APPEND_DATA: u32 = 0x0004;
#[cfg(windows)]
const FILE_SHARE_READ: u32 = 0x0001;
#[cfg(windows)]
const CREATE_ALWAYS: u32 = 2;
#[cfg(windows)]
const OPEN_ALWAYS: u32 = 4;
#[cfg(windows)]
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
#[cfg(windows)]
const INVALID_FILE_ATTRIBUTES: u32 = u32::MAX;
#[cfg(windows)]
const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
#[cfg(windows)]
const ERROR_ALREADY_EXISTS: u32 = 183;

#[cfg(windows)]
unsafe extern "system" {
    fn GetFileAttributesW(path: *const u16) -> u32;
    fn CreateFileW(
        path: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: *mut c_void,
    ) -> *mut c_void;
    fn WriteFile(
        file: *mut c_void,
        data: *const c_void,
        len: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn MoveFileW(from: *const u16, to: *const u16) -> i32;
    fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    fn DeleteFileW(path: *const u16) -> i32;
    fn CreateDirectoryW(path: *const u16, security: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

/// Whether anything is at `path` (`GetFileAttributesW`).
#[cfg(windows)]
pub fn exists(path: &OsStr) -> bool {
    let path = wide(path);
    // SAFETY: `path` is NUL-terminated and lives across the call.
    unsafe { GetFileAttributesW(path.as_ptr()) != INVALID_FILE_ATTRIBUTES }
}

/// Writes `data` as the file `path`, replacing one there; false when it fails.
///
/// A file made but not fully written is deleted; one that could not be made is left alone.
#[cfg(windows)]
pub fn write(path: &OsStr, data: &[u8]) -> bool {
    let path = wide(path);
    let null = core::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated and lives across the call; the other pointers are null.
    let file = unsafe {
        CreateFileW(
            path.as_ptr(),
            GENERIC_WRITE,
            0,
            null,
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            null,
        )
    };
    if is_invalid(file) {
        return false;
    }
    let ok = write_all(file, data);
    // SAFETY: `file` is the handle CreateFileW opened, closed once.
    unsafe { CloseHandle(file) };
    if !ok {
        // SAFETY: `path` is NUL-terminated and lives across the call.
        unsafe { DeleteFileW(path.as_ptr()) };
    }
    ok
}

/// Renames `from` to `to`, replacing a file there when `replace`; false when it fails.
#[cfg(windows)]
pub fn rename(from: &OsStr, to: &OsStr, replace: bool) -> bool {
    let (from, to) = (wide(from), wide(to));
    if replace {
        // SAFETY: both paths are NUL-terminated and live across the call.
        unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_REPLACE_EXISTING) != 0 }
    } else {
        // SAFETY: both paths are NUL-terminated and live across the call.
        unsafe { MoveFileW(from.as_ptr(), to.as_ptr()) != 0 }
    }
}

/// Deletes the file `path`, if it can.
#[cfg(windows)]
pub fn remove(path: &OsStr) {
    let path = wide(path);
    // SAFETY: `path` is NUL-terminated and lives across the call.
    unsafe { DeleteFileW(path.as_ptr()) };
}

/// Makes the folder `path`: false only when something was already there.
#[cfg(windows)]
pub fn create_dir(path: &OsStr) -> bool {
    let path = wide(path);
    // SAFETY: `path` is NUL-terminated and lives across the call; no security attributes.
    let made = unsafe { CreateDirectoryW(path.as_ptr(), core::ptr::null_mut()) } != 0;
    // SAFETY: reads the calling thread's last error, which CreateDirectoryW just set.
    made || unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
}

/// Appends `data` to the file `path`, made if it isn't there; a failure writes nothing.
#[cfg(windows)]
pub fn append(path: &OsStr, data: &[u8]) {
    let path = wide(path);
    let null = core::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated and lives across the call; the other pointers are null.
    let file = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_APPEND_DATA,
            FILE_SHARE_READ,
            null,
            OPEN_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            null,
        )
    };
    if is_invalid(file) {
        return;
    }
    write_all(file, data);
    // SAFETY: `file` is the handle CreateFileW opened, closed once.
    unsafe { CloseHandle(file) };
}

/// Whether anything is at `path` (`stat`).
#[cfg(not(windows))]
pub fn exists(path: &OsStr) -> bool {
    std::fs::metadata(host(path)).is_ok()
}

/// Writes `data` as the file `path`, replacing one there; false when it fails.
///
/// A file made but not fully written is deleted; one that could not be made is left alone.
#[cfg(not(windows))]
pub fn write(path: &OsStr, data: &[u8]) -> bool {
    let path = host(path);
    let Ok(mut file) = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(&path)
    else {
        return false;
    };
    let ok = file.write_all(data).is_ok();
    drop(file);
    if !ok {
        let _ = std::fs::remove_file(&path);
    }
    ok
}

/// Renames `from` to `to`, replacing a file there; false when it fails.
///
/// `rename(2)` always replaces: a capture that appeared meanwhile has the same bytes, its name
/// being their hash.
#[cfg(not(windows))]
pub fn rename(from: &OsStr, to: &OsStr, _replace: bool) -> bool {
    std::fs::rename(host(from), host(to)).is_ok()
}

/// Deletes the file `path`, if it can.
#[cfg(not(windows))]
pub fn remove(path: &OsStr) {
    let _ = std::fs::remove_file(host(path));
}

/// Makes the folder `path`: false only when something was already there.
#[cfg(not(windows))]
pub fn create_dir(path: &OsStr) -> bool {
    DirBuilder::new()
        .mode(0o755)
        .create(host(path))
        .err()
        .is_none_or(|error| error.kind() != ErrorKind::AlreadyExists)
}

/// Appends `data` to the file `path`, made if it isn't there; a failure writes nothing.
#[cfg(not(windows))]
pub fn append(path: &OsStr, data: &[u8]) {
    if let Ok(mut file) = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o644)
        .open(host(path))
    {
        let _ = file.write_all(data);
    }
}

/// A leading `Z:` (either case) before a slash or backslash goes, and every backslash is a slash.
#[cfg(not(windows))]
pub fn host_bytes(path: &[u8]) -> Vec<u8> {
    let rest = match path {
        [b'Z' | b'z', b':', rest @ ..] if matches!(rest.first(), Some(b'/' | b'\\')) => rest,
        _ => path,
    };
    rest.iter()
        .map(|&byte| if byte == b'\\' { b'/' } else { byte })
        .collect()
}

/// `path` NUL-terminated in UTF-16.
#[cfg(windows)]
fn wide(path: &OsStr) -> Vec<u16> {
    path.encode_wide().chain(core::iter::once(0)).collect()
}

/// Whether `CreateFileW` failed (`INVALID_HANDLE_VALUE`).
#[cfg(windows)]
fn is_invalid(file: *mut c_void) -> bool {
    file.addr() == usize::MAX
}

/// One `WriteFile` of all of `data`: true when every byte went.
#[cfg(windows)]
fn write_all(file: *mut c_void, data: &[u8]) -> bool {
    let Ok(len) = u32::try_from(data.len()) else {
        return false;
    };
    let mut written = 0;
    // SAFETY: `file` is open for writing, `data` holds `len` bytes, `written` is a live u32.
    let ok = unsafe {
        WriteFile(
            file,
            data.as_ptr().cast(),
            len,
            &raw mut written,
            core::ptr::null_mut(),
        )
    };
    ok != 0 && written == len
}

/// The host path of a path in Wine's form: `win32_native.h`'s `mn_win32_utf8`.
#[cfg(not(windows))]
fn host(path: &OsStr) -> PathBuf {
    PathBuf::from(std::ffi::OsString::from_vec(host_bytes(path.as_bytes())))
}
