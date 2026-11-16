//! `win32_native_event.h`'s `dxmt-abi` block, asserted against the Rust exports.
//!
//! Written by `tools/winemetal-layouts.sh --seam-abi` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_event;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_semaphore;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, i32, *mut i32) -> i32 = crate::native_win32::wmt_native_release_semaphore;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32) -> u32 = crate::native_win32::wmt_native_wait_for_single_object;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void, u32) -> i32 = crate::native_win32::wmt_native_duplicate_handle;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_open_thread;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_close_handle;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_set_event;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::native_win32::wmt_native_ref;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_unref;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::native_win32::wmt_native_not_implemented;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::native_win32::MTLD3DNotImplemented;
#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u32 = crate::native_win32::wmt_native_handle_count;

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_event;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_semaphore;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, i32, *mut i32) -> i32 = crate::native_win32::wmt_native_release_semaphore;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32) -> u32 = crate::native_win32::wmt_native_wait_for_single_object;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void, u32) -> i32 = crate::native_win32::wmt_native_duplicate_handle;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_open_thread;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_close_handle;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_set_event;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::native_win32::wmt_native_ref;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_unref;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::native_win32::wmt_native_not_implemented;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::native_win32::MTLD3DNotImplemented;
#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u32 = crate::native_win32::wmt_native_handle_count;

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_event;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_semaphore;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, i32, *mut i32) -> i32 = crate::native_win32::wmt_native_release_semaphore;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32) -> u32 = crate::native_win32::wmt_native_wait_for_single_object;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void, u32) -> i32 = crate::native_win32::wmt_native_duplicate_handle;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_open_thread;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_close_handle;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_set_event;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::native_win32::wmt_native_ref;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_unref;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::native_win32::wmt_native_not_implemented;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::native_win32::MTLD3DNotImplemented;
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u32 = crate::native_win32::wmt_native_handle_count;

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_event;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_semaphore;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, i32, *mut i32) -> i32 = crate::native_win32::wmt_native_release_semaphore;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, u32) -> u32 = crate::native_win32::wmt_native_wait_for_single_object;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void, u32) -> i32 = crate::native_win32::wmt_native_duplicate_handle;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(u32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_open_thread;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_close_handle;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_set_event;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) = crate::native_win32::wmt_native_ref;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_unref;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn() = crate::native_win32::wmt_native_not_implemented;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn() -> bool = crate::native_win32::MTLD3DNotImplemented;
#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: unsafe extern "C" fn(*mut core::ffi::c_void) -> u32 = crate::native_win32::wmt_native_handle_count;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_event;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(i32, i32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_create_semaphore;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, i32, *mut i32) -> i32 = crate::native_win32::wmt_native_release_semaphore;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, u32) -> u32 = crate::native_win32::wmt_native_wait_for_single_object;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void, *mut *mut core::ffi::c_void, u32) -> i32 = crate::native_win32::wmt_native_duplicate_handle;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(u32) -> *mut core::ffi::c_void = crate::native_win32::wmt_native_open_thread;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_close_handle;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_set_event;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) = crate::native_win32::wmt_native_ref;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> i32 = crate::native_win32::wmt_native_unref;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn() = crate::native_win32::wmt_native_not_implemented;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn() -> bool = crate::native_win32::MTLD3DNotImplemented;
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: unsafe extern "C-unwind" fn(*mut core::ffi::c_void) -> u32 = crate::native_win32::wmt_native_handle_count;

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: () = {
    assert!(crate::native_win32::MN_WIN32_EVENT == 0x6e65_7665);
    assert!(crate::native_win32::MN_WIN32_SEMAPHORE == 0x6d65_7373);
    assert!(crate::native_win32::MN_WIN32_FILE == 0x656c_6966);
    assert!(crate::native_win32::MN_WIN32_THREAD == 0x6472_6874);
    assert!(crate::native_win32::MN_WIN32_RELAY == 0x7961_6c72);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, kind) == 0);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, refs) == 4);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, m) == 8);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, c) == 72);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, count) == 120);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, max) == 124);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, manual) == 128);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, fd) == 132);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, tid) == 136);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, relay) == 144);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, relay_end) == 152);
    assert!(core::mem::offset_of!(crate::native_win32::MnWin32Handle, relay_ctx) == 160);
    assert!(size_of::<crate::native_win32::MnWin32Handle>() == 168);
};
