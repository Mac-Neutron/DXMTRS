//! Immediate context mode (`Immediate`) and `IMTLD3D11DeviceContext` (`d3d11_context_imm.cpp`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::{dynamic::DynamicBuffer, encoder::Encoder, resource::BufferAllocation};

use super::{Context, Mode};

/// Immediate execution mode for [`Context<Immediate>`].
#[derive(Default)]
pub struct Immediate {
    pub has_dirty_op: bool,
}

impl Mode for Immediate {
    fn emit_st(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        let _ = (ctx, f);
    }

    fn emit_op(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        ctx.mode.has_dirty_op = true;
        let _ = f;
    }

    fn retain<T: Send + 'static>(ctx: &mut Context<Self>, t: T) {
        let _ = (ctx, t);
    }

    fn alloc(ctx: &mut Context<Self>, size: usize, align: usize) -> (*mut u8, u64) {
        let _ = (ctx, size, align);
        (ptr::null_mut(), 0)
    }

    fn staging(ctx: &mut Context<Self>, size: u64, align: u64) -> (u64, u64) {
        let _ = (ctx, size, align);
        (0, 0)
    }

    fn dynamic_name(ctx: &mut Context<Self>, dynamic: &DynamicBuffer) -> Arc<BufferAllocation> {
        let _ = ctx;
        dynamic.immediate_name()
    }
}

impl<M: Mode> Context<M> {
    /// `IMTLD3D11DeviceContext::WaitUntilGPUIdle`.
    pub const fn wait_until_gpu_idle(&mut self) {}

    /// `IMTLD3D11DeviceContext::PrepareFlush`.
    pub const fn prepare_flush(&mut self) {}

    /// `IMTLD3D11DeviceContext::Commit`.
    pub const fn commit(&mut self) {}

    /// `IMTLD3D11DeviceContext::SOGetTargetsWithOffsets`.
    pub const fn so_get_targets_with_offsets(
        &self,
        num_buffers: u32,
        targets: *mut *mut c_void,
        offsets: *mut u32,
    ) {
        let _ = (num_buffers, targets, offsets);
    }

    /// `IMTLD3D11DeviceContext::AcquireSync`.
    pub const fn acquire_sync(&mut self, sync_resource: *mut c_void, key: u64) {
        let _ = (sync_resource, key);
    }

    /// `IMTLD3D11DeviceContext::ReleaseSync`.
    pub const fn release_sync(&mut self, sync_resource: *mut c_void, key: u64) {
        let _ = (sync_resource, key);
    }
}
