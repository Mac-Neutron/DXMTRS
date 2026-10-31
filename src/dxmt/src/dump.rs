//! The DXIL dump folder (`DXMT_DXIL_DUMP`): a game's shaders and pipelines, written as files.
//!
//! `d3d12_dxil_dump.cpp`'s file writing: the folder is the variable's value as Wine names it (a
//! Mac path gets Wine's `Z:` drive), made on first use and kept when that fails; a file is
//! `<folder>\<name>`, written under `<name>.tmp` and renamed, so a game killed mid-write never
//! leaves a capture under its final name. Natively the path is mapped as `win32_native.h`'s file
//! shims map it. Any failure only means a file isn't written: capturing never changes what the
//! game gets. The performance build has none of it.
#![cfg(not(feature = "perf-build"))]

use std::{
    ffi::{OsStr, OsString, c_char, c_void},
    path::Path,
    sync::{LazyLock, Mutex, PoisonError},
};

use crate::ffi::{c_bytes, c_str};

mod abi;
mod platform;

/// The capture folder in Wine's form, `None` when `DXMT_DXIL_DUMP` is unset or empty.
///
/// A static: the process's one capture folder, a fact latched at first use and immutable after
/// (`CaptureFolder`'s function-local static).
static FOLDER: LazyLock<Option<OsString>> =
    LazyLock::new(|| open(std::env::var_os("DXMT_DXIL_DUMP")));

/// Whether capture mode is on (`DXILCaptureMode`).
#[must_use]
pub fn on() -> bool {
    FOLDER.is_some()
}

/// The capture folder as DXMT names it (`Z:` and backslashes natively too), `None` when off.
#[must_use]
pub fn folder() -> Option<&'static Path> {
    FOLDER.as_deref().map(Path::new)
}

/// Saves `data` as `<folder>\<name>`, replacing an earlier file of that name (`SaveCapture`).
pub fn save(name: &str, data: &[u8]) {
    if let Some(folder) = FOLDER.as_deref() {
        save_in(folder, name, data);
    }
}

/// Saves `data` as `<folder>\<name>` unless a file of that name is there (`SaveOnce`).
pub fn save_once(name: &str, data: &[u8]) {
    if let Some(folder) = FOLDER.as_deref() {
        save_once_in(folder, name, data);
    }
}

/// Creates `<folder>\<name>`: false only when it was already there (`MakeCaptureFolder`).
///
/// True when capture is off and on any other error, as the C++: F9 numbers its folders with
/// `while (!MakeCaptureFolder(dir))` under the dumps mutex, which a folder that cannot be made
/// must not turn into an endless loop.
#[must_use]
pub fn make_folder(name: &str) -> bool {
    make_folder_in(FOLDER.as_deref(), name)
}

/// Appends `line` and a newline to `<folder>\pipelines.txt` (`LogPipeline`).
pub fn append_pipeline(line: &str) {
    append_line(line.as_bytes());
}

crate::seam_export! {
    /// `DXILCaptureMode`.
    fn dxmt_dump_on() -> bool {
        on()
    }
}

crate::seam_export! {
    /// `SaveCapture`.
    ///
    /// # Safety
    ///
    /// `name` is null or a NUL-terminated string; `data` is null or `len` readable bytes.
    unsafe fn dxmt_dump_save(name: *const c_char, data: *const c_void, len: usize) {
        // SAFETY: the caller's contract.
        let name = unsafe { c_str(name) };
        // SAFETY: the caller's contract.
        let data = unsafe { c_bytes(data, len) };
        if let (Some(name), Some(data)) = (name, data) {
            save(&name, data);
        }
    }
}

crate::seam_export! {
    /// `SaveOnce`.
    ///
    /// # Safety
    ///
    /// `name` is null or a NUL-terminated string; `data` is null or `len` readable bytes.
    unsafe fn dxmt_dump_save_once(name: *const c_char, data: *const c_void, len: usize) {
        // SAFETY: the caller's contract.
        let name = unsafe { c_str(name) };
        // SAFETY: the caller's contract.
        let data = unsafe { c_bytes(data, len) };
        if let (Some(name), Some(data)) = (name, data) {
            save_once(&name, data);
        }
    }
}

crate::seam_export! {
    /// `MakeCaptureFolder`.
    ///
    /// # Safety
    ///
    /// `name` is null or a NUL-terminated string.
    unsafe fn dxmt_dump_make_folder(name: *const c_char) -> bool {
        // SAFETY: the caller's contract.
        unsafe { c_str(name) }.is_none_or(|name| make_folder(&name))
    }
}

crate::seam_export! {
    /// `LogPipeline`.
    ///
    /// # Safety
    ///
    /// `line` is null or `len` readable bytes.
    unsafe fn dxmt_dump_append_pipeline(line: *const c_char, len: usize) {
        // SAFETY: the caller's contract.
        if let Some(line) = unsafe { c_bytes(line.cast(), len) } {
            append_line(line);
        }
    }
}

/// The folder `value` names, made if it isn't there; `None` for no value or an empty one.
///
/// A failure to make it keeps the folder: capture mode (and the SM6 answers it brings) stay on,
/// and only the captures go missing.
fn open(value: Option<OsString>) -> Option<OsString> {
    let folder = wine_folder(value?)?;
    platform::create_dir(&folder);
    Some(folder)
}

/// `value` as Wine names it: a Mac path (a leading `/`) gets Wine's `Z:` drive.
fn wine_folder(value: OsString) -> Option<OsString> {
    match value.as_encoded_bytes().first() {
        None => None,
        Some(b'/') => {
            let mut folder = OsString::from("Z:");
            folder.push(value);
            Some(folder)
        }
        Some(_) => Some(value),
    }
}

/// `<folder>\<name>`.
fn join(folder: &OsStr, name: &str) -> OsString {
    let mut path = folder.to_owned();
    path.push("\\");
    path.push(name);
    path
}

/// `path` with `.tmp` added.
fn temporary(path: &OsStr) -> OsString {
    let mut temp = path.to_owned();
    temp.push(".tmp");
    temp
}

fn save_in(folder: &OsStr, name: &str, data: &[u8]) {
    let path = join(folder, name);
    let temp = temporary(&path);
    if platform::write(&temp, data) && !platform::rename(&temp, &path, true) {
        platform::remove(&temp);
    }
}

/// `save_in` unless the file is there.
///
/// The rename of a file that appeared meanwhile (another thread saving the same shader) fails
/// on Windows and the earlier capture is kept; natively it replaces it, with the same bytes,
/// since a name is a hash of its content.
fn save_once_in(folder: &OsStr, name: &str, data: &[u8]) {
    let path = join(folder, name);
    if platform::exists(&path) {
        return;
    }
    let temp = temporary(&path);
    if platform::write(&temp, data) && !platform::rename(&temp, &path, false) {
        platform::remove(&temp);
    }
}

fn make_folder_in(folder: Option<&OsStr>, name: &str) -> bool {
    folder.is_none_or(|folder| platform::create_dir(&join(folder, name)))
}

fn append_line(line: &[u8]) {
    /// One writer of `pipelines.txt` at a time: the file is the process's one pipeline list.
    static WRITER: Mutex<()> = Mutex::new(());
    let Some(folder) = FOLDER.as_deref() else {
        return;
    };
    let _writer = WRITER.lock().unwrap_or_else(PoisonError::into_inner);
    append_in(folder, line);
}

fn append_in(folder: &OsStr, line: &[u8]) {
    let mut text = Vec::with_capacity(line.len() + 1);
    text.extend_from_slice(line);
    text.push(b'\n');
    platform::append(&join(folder, "pipelines.txt"), &text);
}

// The tests read macOS's paths and printf: they run on the host.
#[cfg(all(test, target_os = "macos"))]
mod tests;
