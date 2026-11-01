//! Command-list submission finalization (`Close`/`Reset`, `d3d12_command_list.cpp:349-365`).

use crate::{com::Hresult, list::List};

/// Finalizes `list`'s [`Submission`](crate::encoder::Submission) on `Close`
///
/// (`d3d12_command_list.cpp:349`).
pub fn finish(_list: &mut List) -> Hresult {
    crate::not_implemented!("list::submission::finish")
}

/// Clears `list`'s [`Submission`](crate::encoder::Submission) on `Reset`
///
/// (`d3d12_command_list.cpp:355`).
pub fn reset(_list: &mut List) {
    let (): () = crate::not_implemented!("list::submission::reset");
}
