//! DXMT's winemetal: the Unix side of its Unix calls, over Metal.
//!
//! A static library linked into the Wine build's `winemetal.so` and, with the `native` feature,
//! the native `winemetal.dylib`. Each handler is exported under its C name, so the C table of
//! handlers reaches it until the tables move here too ([`table`]). The handlers are grouped as
//! winemetal's slots are ([`g1`] to [`g10`]); the modules before them are what the groups share.

pub mod ffi;
pub mod float_cast;
pub mod format;
pub mod handle;
pub mod init;
pub mod table;
pub mod texture;

pub mod g1;
pub mod g2;
pub mod g3;
pub mod g4;
pub mod g5;
pub mod g6;
pub mod g7;
pub mod g8;
pub mod g9;
pub mod g10;
