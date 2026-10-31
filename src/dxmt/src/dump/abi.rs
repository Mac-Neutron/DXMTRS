//! `d3d12_dxil_dump.hpp`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::dump::dxmt_dump_on;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save_once;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> bool = crate::dump::dxmt_dump_make_folder;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, usize) = crate::dump::dxmt_dump_append_pipeline;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::dump::dxmt_dump_on;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save_once;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> bool = crate::dump::dxmt_dump_make_folder;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, usize) = crate::dump::dxmt_dump_append_pipeline;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::dump::dxmt_dump_on;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save_once;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> bool = crate::dump::dxmt_dump_make_folder;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, usize) = crate::dump::dxmt_dump_append_pipeline;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::dump::dxmt_dump_on;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save_once;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> bool = crate::dump::dxmt_dump_make_folder;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char, usize) = crate::dump::dxmt_dump_append_pipeline;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn() -> bool = crate::dump::dxmt_dump_on;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*const core::ffi::c_char, *const core::ffi::c_void, usize) = crate::dump::dxmt_dump_save_once;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*const core::ffi::c_char) -> bool = crate::dump::dxmt_dump_make_folder;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*const core::ffi::c_char, usize) = crate::dump::dxmt_dump_append_pipeline;
