pub mod bounded_cache;
pub mod crumb;
pub mod fatal;
pub mod ffi_boundary;
pub mod ftol;
pub mod identity;
mod log_filter;
pub mod mtl;
pub mod perf;
pub mod shader_create;
pub mod trig;
pub mod tsc;

pub use dxmt_util::log::paths as log_paths;
// The once-macros live in dxmt-util; every `mtld3d_shared::log_once_*!` call compiles unchanged.
pub use dxmt_util::{
    log_once_debug_by, log_once_info, log_once_info_by, log_once_trace_by, log_once_warn,
    log_once_warn_by,
};
pub use ffi_boundary::{
    InPtr, InPtrMut, OutPtr, ValueIn, VtableThis, slice_from_caller, slice_to_caller,
};
pub use log_filter::{init_logger, init_logger_to, init_logger_to_filter, resolved_log_filter};
