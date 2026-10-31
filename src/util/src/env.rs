//! The program's name and environment, read the way `util/util_env.cpp` reads them.
//!
//! Natively (the Mac dylibs) the executable has no path: every name is empty, as the C++'s `TODO`.

/// `GetModuleFileNameW`'s buffer length in the C++ (`MAX_PATH`).
#[cfg(windows)]
const MAX_PATH: u32 = 260;

/// The executable's path as the wide characters `GetModuleFileNameW(NULL, buf, 260)` wrote.
#[cfg(windows)]
fn module_file_name() -> Vec<u16> {
    unsafe extern "system" {
        fn GetModuleFileNameW(module: *mut core::ffi::c_void, name: *mut u16, size: u32) -> u32;
    }
    let mut name = [0u16; MAX_PATH as usize + 1];
    // SAFETY: `name` has room for the 260 units asked for and a NUL; a null module is the process's exe.
    let len = unsafe { GetModuleFileNameW(core::ptr::null_mut(), name.as_mut_ptr(), MAX_PATH) };
    name.get(..usize::try_from(len).unwrap_or(0))
        .unwrap_or_default()
        .to_vec()
}

#[cfg(not(windows))]
const fn module_file_name() -> Vec<u16> {
    Vec::new()
}

/// The path in `units`, up to its length and its first NUL (the C++ reads a C string).
fn path_of(units: &[u16]) -> String {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    String::from_utf16_lossy(units.split_at(end).0)
}

/// The part of `path` after its last backslash.
#[must_use]
pub fn name_of(path: &str) -> &str {
    path.rsplit_once('\\').map_or(path, |(_, name)| name)
}

/// `name` without the extension `matchFileExtension(name, "exe")` accepts.
///
/// The text after the last dot only has to be a prefix of `exe`, in either case: `.EXE`, `.ex`,
/// `.e` and a bare trailing `.` all go.
#[must_use]
pub fn base_of(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if "exe".starts_with(ext.to_ascii_lowercase().as_str()) => stem,
        _ => name,
    }
}

/// The cache name: `var` (`DXMT_CACHE_EXE`) unless unset or empty, else `name`.
fn cache_name(var: Option<String>, name: &str) -> String {
    var.filter(|value| !value.is_empty())
        .unwrap_or_else(|| name.to_owned())
}

/// The full path of the executable, cut at 260 wide characters.
#[must_use]
pub fn exe_path() -> String {
    path_of(&module_file_name())
}

/// The executable's file name, extension included.
#[must_use]
pub fn exe_name() -> String {
    name_of(&exe_path()).to_owned()
}

/// The executable's file name without its `.exe`.
#[must_use]
pub fn exe_base_name() -> String {
    base_of(name_of(&exe_path())).to_owned()
}

/// The name DXMT's caches are kept under.
///
/// `DXMT_CACHE_EXE` when set (`dxmt-replay.exe` fills a game's caches), else the exe's name.
#[must_use]
pub fn cache_exe_name() -> String {
    cache_name(std::env::var("DXMT_CACHE_EXE").ok(), &exe_name())
}

#[cfg(test)]
mod tests;
