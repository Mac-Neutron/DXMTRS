//! DXMT's `winemetal.dll` in a Wine build: each `winemetal.h` function a thunk to `winemetal.so`.
//!
//! A thunk fills its call's params structure (zeroed, then each field the C thunk wrote), makes
//! the Unix call of its slot through Wine's dispatcher and hands back what the handler wrote: the
//! return value and the out-pointers the C copied back. Statuses are dropped, as a release build
//! of the C dropped them, except where the C looked: the bootstrap calls answer whether theirs was
//! 0 and `WMTForward` passes its own through. `DllMain` loads the Unix side. The `SM50*` exports
//! stay C (`airconv_thunks.c`); the native `winemetal.dylib` keeps the C thunks.

/// winemetal.dll's exports.
///
/// Each `fn` becomes an unmangled `pub unsafe extern "C" fn` of its C name and signature.
macro_rules! exports {
    ($(fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)? $body:block)*) => {$(
        /// # Safety
        ///
        /// The C function's contract: what its pointers point at is valid for the Unix side,
        /// and an out-pointer is valid for a write unless the function checks it for null.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name($($arg: $ty),*) $(-> $ret)? $body
    )*};
}

mod access;
mod cache;
mod capture;
mod commands;
mod device;
mod display;
mod events;
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
pub use guest::*;
pub use metalfx::*;
pub use objects::*;
pub use pipelines::*;
pub use resources::*;
