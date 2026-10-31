//! `dxmt_format.hpp`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut crate::format::MtlDxgiFormatDesc) -> i32 = crate::format::dxmt_format_query_dxgi;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_forget_srgb;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_recall_srgb;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_bc;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_depth_stencil_planar_flags;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_texel_size;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_unsigned_integer;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_unorm8_rt;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_integer;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut winemetal_wire::WMTClearColor) = crate::format::dxmt_format_sanitize_clear_color;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut crate::format::DxmtFormatCapability, usize) -> usize = crate::format::dxmt_format_capabilities;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut crate::format::MtlDxgiFormatDesc) -> i32 = crate::format::dxmt_format_query_dxgi;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_forget_srgb;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_recall_srgb;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_bc;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_depth_stencil_planar_flags;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_texel_size;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_unsigned_integer;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_unorm8_rt;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_integer;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut winemetal_wire::WMTClearColor) = crate::format::dxmt_format_sanitize_clear_color;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut crate::format::DxmtFormatCapability, usize) -> usize = crate::format::dxmt_format_capabilities;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut crate::format::MtlDxgiFormatDesc) -> i32 = crate::format::dxmt_format_query_dxgi;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_forget_srgb;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_recall_srgb;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_bc;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_depth_stencil_planar_flags;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_texel_size;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_unsigned_integer;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_unorm8_rt;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_integer;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut winemetal_wire::WMTClearColor) = crate::format::dxmt_format_sanitize_clear_color;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut crate::format::DxmtFormatCapability, usize) -> usize = crate::format::dxmt_format_capabilities;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut crate::format::MtlDxgiFormatDesc) -> i32 = crate::format::dxmt_format_query_dxgi;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_forget_srgb;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_recall_srgb;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_bc;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_depth_stencil_planar_flags;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_texel_size;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> u32 = crate::format::dxmt_format_unsigned_integer;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_unorm8_rt;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> bool = crate::format::dxmt_format_is_integer;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32, *mut winemetal_wire::WMTClearColor) = crate::format::dxmt_format_sanitize_clear_color;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u64, *mut crate::format::DxmtFormatCapability, usize) -> usize = crate::format::dxmt_format_capabilities;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32, *mut crate::format::MtlDxgiFormatDesc) -> i32 = crate::format::dxmt_format_query_dxgi;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> u32 = crate::format::dxmt_format_forget_srgb;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> u32 = crate::format::dxmt_format_recall_srgb;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> bool = crate::format::dxmt_format_is_bc;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> u32 = crate::format::dxmt_format_depth_stencil_planar_flags;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> u32 = crate::format::dxmt_format_texel_size;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> u32 = crate::format::dxmt_format_unsigned_integer;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> bool = crate::format::dxmt_format_is_unorm8_rt;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> bool = crate::format::dxmt_format_is_integer;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32, *mut winemetal_wire::WMTClearColor) = crate::format::dxmt_format_sanitize_clear_color;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u64, *mut crate::format::DxmtFormatCapability, usize) -> usize = crate::format::dxmt_format_capabilities;
