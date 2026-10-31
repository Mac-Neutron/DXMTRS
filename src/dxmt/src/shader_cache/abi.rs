//! `dxmt_shader_cache.hpp`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) -> *mut core::ffi::c_void = crate::shader_cache::dxmt_shader_cache_create;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::shader_cache::dxmt_shader_cache_destroy;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_reader;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_writer;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64) -> u64 = crate::shader_cache::dxmt_shader_cache_read;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64, u64) = crate::shader_cache::dxmt_shader_cache_write;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) -> *mut core::ffi::c_void = crate::shader_cache::dxmt_shader_cache_create;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::shader_cache::dxmt_shader_cache_destroy;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_reader;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_writer;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64) -> u64 = crate::shader_cache::dxmt_shader_cache_read;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64, u64) = crate::shader_cache::dxmt_shader_cache_write;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) -> *mut core::ffi::c_void = crate::shader_cache::dxmt_shader_cache_create;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::shader_cache::dxmt_shader_cache_destroy;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_reader;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_writer;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64) -> u64 = crate::shader_cache::dxmt_shader_cache_read;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64, u64) = crate::shader_cache::dxmt_shader_cache_write;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) -> *mut core::ffi::c_void = crate::shader_cache::dxmt_shader_cache_create;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::shader_cache::dxmt_shader_cache_destroy;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_reader;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_writer;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64) -> u64 = crate::shader_cache::dxmt_shader_cache_read;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64, u64) = crate::shader_cache::dxmt_shader_cache_write;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32, u64) -> *mut core::ffi::c_void = crate::shader_cache::dxmt_shader_cache_create;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) = crate::shader_cache::dxmt_shader_cache_destroy;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_reader;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> bool = crate::shader_cache::dxmt_shader_cache_has_writer;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64) -> u64 = crate::shader_cache::dxmt_shader_cache_read;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, *const core::ffi::c_void, u64, u64) = crate::shader_cache::dxmt_shader_cache_write;
