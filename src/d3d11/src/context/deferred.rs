//! Deferred context mode (`Deferred`) and `CreateDeferredContext*` (`d3d11_context_def.cpp`).

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::{dynamic::DynamicBuffer, encoder::Encoder, resource::BufferAllocation};

use super::{Context, Mode};
use crate::com::{E_NOTIMPL, Hresult};

/// Deferred execution mode for [`Context<Deferred>`].
#[derive(Default)]
pub struct Deferred {
    pub context_flags: u32,
}

impl Mode for Deferred {
    fn emit_st(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        let _ = (ctx, f);
    }

    fn emit_op(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        let _ = (ctx, f);
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

/// Creates a deferred context (`ID3D11Device::CreateDeferredContext*`).
pub const fn create_deferred(
    device: *mut c_void,
    context_flags: u32,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, context_flags, out);
    E_NOTIMPL
}
