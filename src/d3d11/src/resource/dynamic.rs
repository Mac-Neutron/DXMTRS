//! Dynamic buffer and dynamic texture implementations (`d3d11_texture_dynamic.cpp`).

use std::sync::Arc;

use dxmt_core::{
    dynamic::DynamicBuffer,
    resource::{Buffer, BufferAllocation, BufferAllocationFlags},
};
use winemetal_sys::Obj;

use super::Common;

/// Dynamic resource wrapper (`MTLD3D11DynamicBuffer`, `MTLD3D11DynamicTexture`).
pub struct DynamicResource {
    pub common: Common,
}

/// Allocates a [`Buffer`] and its initial [`BufferAllocation`], returning a [`DynamicBuffer`].
#[must_use]
pub fn allocate_dynamic_buffer(
    device: &Obj,
    byte_width: u32,
    flags: BufferAllocationFlags,
) -> Option<(Arc<Buffer>, Arc<DynamicBuffer>)> {
    let buffer = Arc::new(Buffer::new(u64::from(byte_width), device));
    let alloc = buffer.allocate(flags);
    let _ = buffer.rename(alloc);
    let dynamic = Arc::new(DynamicBuffer::new(Arc::clone(&buffer), flags)?);
    Some((buffer, dynamic))
}

/// Performs an immediate-context `D3D11_MAP_WRITE_DISCARD` step on `dynamic`.
///
/// Matches `MapDynamicBuffer` in `d3d11_context_imm.cpp:120-135`: returns
/// `(allocation, suballocation, mapped_ptr)` and whether a new allocation was renamed (`true` when
/// `next_suballocation` found no next one and a new allocation became the immediate name).
#[must_use]
pub fn discard_map_dynamic_buffer(
    dynamic: &DynamicBuffer,
    current_seq: u64,
    coherent_seq: u64,
) -> (Arc<BufferAllocation>, u32, *mut u8, bool) {
    if let Some((alloc, subidx, ptr)) = dynamic.next_suballocation() {
        (alloc, subidx, ptr, false)
    } else {
        let alloc = dynamic.allocate(coherent_seq);
        dynamic.update_immediate_name(current_seq, Arc::clone(&alloc), 0, false);
        let ptr = dynamic.immediate_mapped_memory();
        (alloc, 0, ptr, true)
    }
}
