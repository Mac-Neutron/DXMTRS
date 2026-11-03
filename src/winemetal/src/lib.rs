//! DXMT's `winemetal.dll` (Wine) and `winemetal.dylib`'s C entry points (native).
//!
//! Each `winemetal.h` function is a thunk to its Unix-call slot. A thunk fills its call's params
//! structure (zeroed, then each field the C thunk wrote), makes the Unix call of its slot (through
//! Wine's dispatcher on Windows, or directly through `__wine_unix_call_funcs` natively) and hands
//! back what the handler wrote: the return value and the out-pointers the C copied back. Statuses
//! are dropped, as a release build of the C dropped them, except where the C looked: the bootstrap
//! calls answer whether theirs was 0 and `WMTForward` passes its own through. On Windows,
//! `DllMain` loads the Unix side; the `SM50*` exports stay C (`airconv_thunks.c`).

/// `winemetal.h`'s exported entry points.
///
/// Each `fn` becomes an unmangled `pub unsafe extern "C"` (Windows) or `"C-unwind"` (native) `fn`
/// of its C name and signature.
macro_rules! exports {
    ($(fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)? $body:block)*) => {$(
        /// # Safety
        ///
        /// The C function's contract: what its pointers point at is valid for the Unix side,
        /// and an out-pointer is valid for a write unless the function checks it for null.
        #[cfg(windows)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($($arg: $ty),*) $(-> $ret)? $body

        /// # Safety
        ///
        /// The C function's contract: what its pointers point at is valid for the Unix side,
        /// and an out-pointer is valid for a write unless the function checks it for null.
        #[cfg(not(windows))]
        #[unsafe(no_mangle)]
        pub unsafe extern "C-unwind" fn $name($($arg: $ty),*) $(-> $ret)? $body
    )*};
}

mod access;
mod cache;
mod capture;
mod commands;
mod device;
mod display;
mod events;
#[cfg(windows)]
mod guest;
mod metalfx;
mod objects;
mod pipelines;
#[cfg(windows)]
mod protos;
mod resources;
mod transport;

// The exports at the crate root, where `protos` names them as `winemetal.h` does.
pub use access::*;
pub use cache::*;
pub use capture::*;
pub use commands::*;
pub use device::*;
pub use display::*;
pub use events::*;
#[cfg(windows)]
pub use guest::*;
pub use metalfx::*;
pub use objects::*;
pub use pipelines::*;
pub use resources::*;
