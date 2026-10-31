//! `d3d12_stats.hpp`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(bool) = crate::stats::dxmt_stats_init;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_exit;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> u32 = crate::stats::dxmt_stat_id;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_count;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u64 = crate::stats::dxmt_stat_begin;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_end;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_frame;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u32 = crate::stats::dxmt_stats_next_pass;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(bool) = crate::stats::dxmt_stats_init;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_exit;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> u32 = crate::stats::dxmt_stat_id;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_count;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u64 = crate::stats::dxmt_stat_begin;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_end;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_frame;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u32 = crate::stats::dxmt_stats_next_pass;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(bool) = crate::stats::dxmt_stats_init;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_exit;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> u32 = crate::stats::dxmt_stat_id;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_count;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u64 = crate::stats::dxmt_stat_begin;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_end;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_frame;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u32 = crate::stats::dxmt_stats_next_pass;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(bool) = crate::stats::dxmt_stats_init;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_exit;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*const core::ffi::c_char) -> u32 = crate::stats::dxmt_stat_id;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_count;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u64 = crate::stats::dxmt_stat_begin;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, u64) = crate::stats::dxmt_stat_end;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::stats::dxmt_stats_frame;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn() -> u32 = crate::stats::dxmt_stats_next_pass;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(bool) = crate::stats::dxmt_stats_init;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn() = crate::stats::dxmt_stats_exit;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*const core::ffi::c_char) -> u32 = crate::stats::dxmt_stat_id;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32, u64) = crate::stats::dxmt_stat_count;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn() -> u64 = crate::stats::dxmt_stat_begin;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32, u64) = crate::stats::dxmt_stat_end;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn() = crate::stats::dxmt_stats_frame;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn() -> u32 = crate::stats::dxmt_stats_next_pass;
