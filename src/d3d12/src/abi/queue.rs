//! C-layout mirror of `D3D12_COMMAND_QUEUE_DESC`.
//!
//! The structure and constants as `d3d12.h` defines them. Every enumeration is its `u32` (or `i32`
//! for priority) and the fields are the C's in snake case. The layout is asserted at the end
//! against clang's record layouts on every target.

use core::mem::offset_of;

/// `D3D12_COMMAND_LIST_TYPE_DIRECT`.
pub const D3D12_COMMAND_LIST_TYPE_DIRECT: u32 = 0;
/// `D3D12_COMMAND_LIST_TYPE_BUNDLE`.
pub const D3D12_COMMAND_LIST_TYPE_BUNDLE: u32 = 1;
/// `D3D12_COMMAND_LIST_TYPE_COMPUTE`.
pub const D3D12_COMMAND_LIST_TYPE_COMPUTE: u32 = 2;
/// `D3D12_COMMAND_LIST_TYPE_COPY`.
pub const D3D12_COMMAND_LIST_TYPE_COPY: u32 = 3;
/// `D3D12_COMMAND_LIST_TYPE_VIDEO_DECODE`.
pub const D3D12_COMMAND_LIST_TYPE_VIDEO_DECODE: u32 = 4;
/// `D3D12_COMMAND_LIST_TYPE_VIDEO_PROCESS`.
pub const D3D12_COMMAND_LIST_TYPE_VIDEO_PROCESS: u32 = 5;
/// `D3D12_COMMAND_LIST_TYPE_VIDEO_ENCODE`.
pub const D3D12_COMMAND_LIST_TYPE_VIDEO_ENCODE: u32 = 6;

/// `D3D12_COMMAND_QUEUE_PRIORITY_NORMAL`.
pub const D3D12_COMMAND_QUEUE_PRIORITY_NORMAL: i32 = 0;
/// `D3D12_COMMAND_QUEUE_PRIORITY_HIGH`.
pub const D3D12_COMMAND_QUEUE_PRIORITY_HIGH: i32 = 100;
/// `D3D12_COMMAND_QUEUE_PRIORITY_GLOBAL_REALTIME`.
pub const D3D12_COMMAND_QUEUE_PRIORITY_GLOBAL_REALTIME: i32 = 10000;

/// `D3D12_COMMAND_QUEUE_FLAG_NONE`.
pub const D3D12_COMMAND_QUEUE_FLAG_NONE: u32 = 0;
/// `D3D12_COMMAND_QUEUE_FLAG_DISABLE_GPU_TIMEOUT`.
pub const D3D12_COMMAND_QUEUE_FLAG_DISABLE_GPU_TIMEOUT: u32 = 0x1;

/// `D3D12_COMMAND_QUEUE_DESC`.
#[repr(C)]
pub struct CommandQueueDesc {
    pub r#type: u32,
    pub priority: i32,
    pub flags: u32,
    pub node_mask: u32,
}

impl Clone for CommandQueueDesc {
    fn clone(&self) -> Self {
        Self {
            r#type: self.r#type,
            priority: self.priority,
            flags: self.flags,
            node_mask: self.node_mask,
        }
    }
}

const _: () = {
    assert!(size_of::<CommandQueueDesc>() == 16 && align_of::<CommandQueueDesc>() == 4);
    assert!(offset_of!(CommandQueueDesc, r#type) == 0);
    assert!(offset_of!(CommandQueueDesc, priority) == 4);
    assert!(offset_of!(CommandQueueDesc, flags) == 8);
    assert!(offset_of!(CommandQueueDesc, node_mask) == 12);
};
