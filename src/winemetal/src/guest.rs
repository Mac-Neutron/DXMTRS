//! The VM guest's own calls.
//!
//! Every build defines them; only the guest's `winemetal-guest.def` exports them.

use core::ffi::c_void;

use winemetal_wire::{UnixcallWmtmemory, slot};

use crate::transport::{call, mem_ptr, ptr_of, unix_call};

exports! {
    fn WMTMemoryAlloc(length: u64) -> *mut c_void {
        ptr_of(call(slot::WMT_MEMORY_ALLOC, |p: &mut UnixcallWmtmemory| p.length = length).ptr)
    }

    fn WMTMemoryFree(ptr: *mut c_void) {
        call(slot::WMT_MEMORY_FREE, |p: &mut UnixcallWmtmemory| p.ptr = mem_ptr(ptr));
    }

    fn WMTForward(call: u32, params: *mut c_void) -> u32 {
        // STATUS_INVALID_PARAMETER: winemetal's own calls aren't forwarded, and none is past 155.
        if call != slot::WMT_FORWARD {
            return 0xC000_000D;
        }
        unix_call(call, params).cast_unsigned()
    }
}
