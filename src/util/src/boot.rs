//! What every DXMT Rust DLL declares at its root: its log, allocator and `type_info` vtable.

pub use crate::dxmt_dll;

/// Declares a DLL's root statics: `LOG`, then the parts listed after it.
///
/// `LOG` is the [`Logger`](crate::log::Logger) writing `<exe>_<file>`; a test build's goes to
/// `no_test_log/<file>`, which never opens.
///
/// - `allocator`: `snmalloc` as the global allocator of the native dylib (macOS, not in tests);
///   the calling crate depends on `snmalloc-rs`.
/// - `type_info`: the `type_info` vtable under its MSVC decorated name, on Windows.
/// - `pe_rt`: what Rust's `-msvc` code needs from MSVC's runtime on x86 and x86-64 (`_fltused`,
///   `__chkstk`, `__tls_array`); every DLL and program names it once, at its root.
///
/// `dxmt_dll!(pub log "d3d12.log", allocator, type_info);` gives `pub static LOG`; leave out
/// `pub` for a private one. The `cfg`s are the calling crate's: none of them is a feature.
#[macro_export]
macro_rules! dxmt_dll {
    (@allocator) => {
        /// Native Rust allocations in this DLL's dylib use `snmalloc`.
        #[cfg(all(target_os = "macos", not(test)))]
        #[global_allocator]
        static ALLOCATOR: ::snmalloc_rs::SnMalloc = ::snmalloc_rs::SnMalloc;
    };
    (@type_info) => {
        /// The `type_info` vtable under its MSVC decorated name.
        ///
        /// Rust's `std::thread` `catch_unwind` emits a C++ type descriptor
        /// (`__rust_panic_type_info`) whose first field points at this vtable, so the name has to
        /// resolve; llvm-mingw's CRT does not define it. Nothing reads through it: every profile
        /// sets `panic = "abort"`, and the C++ frame handler (`__CxxFrameHandler3`) matches a
        /// thrown type by descriptor address and decorated name, never through the vtable.
        #[cfg(windows)]
        #[unsafe(export_name = "??_7type_info@@6B@")]
        pub static TYPE_INFO_VFTABLE: [usize; 1] = [0];
    };
    (@pe_rt) => {
        /// MSVC's `_fltused`, which code that uses floating point references.
        ///
        /// MSVC's CRT defines it and llvm-mingw's does not; only its presence matters, so it
        /// holds nothing and never changes.
        #[cfg(all(windows, any(target_arch = "x86", target_arch = "x86_64")))]
        #[unsafe(export_name = "_fltused")]
        pub static FLTUSED: i32 = 0;

        // The stack probe. MSVC's `__chkstk` and mingw's `___chkstk_ms` both touch the pages
        // below `rsp` and leave it alone, so a jump keeps the caller's frame as `__chkstk` would.
        #[cfg(all(windows, target_arch = "x86_64"))]
        ::core::arch::global_asm!(".globl __chkstk", "__chkstk:", "jmp ___chkstk_ms");

        // MSVC's `_chkstk` and compiler-rt's `___chkstk` both probe and then lower `esp` by
        // `eax`. The `-msvc` target reads the TEB's TLS array at the absolute `__tls_array`
        // (the TEB's `ThreadLocalStoragePointer`), which MSVC's CRT defines.
        #[cfg(all(windows, target_arch = "x86"))]
        ::core::arch::global_asm!(
            ".globl __chkstk",
            "__chkstk:",
            "jmp ___chkstk",
            ".globl __tls_array",
            ".set __tls_array, 0x2c",
        );
    };
    ($vis:vis log $file:literal $(, $part:ident)* $(,)?) => {
        /// This DLL's log file (DXMT's `Logger::s_instance`).
        ///
        /// Process-wide: one log file per DLL, opened by the first message that passes the level,
        /// and the exports have no object to hold it.
        $vis static LOG: $crate::log::Logger = $crate::log::Logger::new(if cfg!(test) {
            concat!("no_test_log/", $file)
        } else {
            $file
        });
        $($crate::dxmt_dll!(@$part);)*
    };
}

#[cfg(test)]
mod tests;
