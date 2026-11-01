//! D3D11 immediate and deferred device contexts (`d3d11_context.hpp`, `d3d11_context_impl.cpp`).
//!
//! [`Context<M>`] is generic over [`Mode`] ([`Immediate`] and [`Deferred`]), sharing state,
//! encoding and transfer logic across both context kinds.

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{dynamic::DynamicBuffer, encoder::Encoder, resource::BufferAllocation};
use dxmt_util::private_data::PrivateData;

use crate::{
    abi::{
        ext::{IMTLD3D11ContextExt2Vtbl, IMTLD3D11DeviceContextVtbl},
        vtbl::ID3DUserDefinedAnnotationVtbl,
    },
    com::{E_NOINTERFACE, E_NOTIMPL, Hresult, Managed},
    state::Defaults,
};

pub mod binding_set;
pub mod clear;
pub mod command_list;
pub mod context_state;
pub mod copy;
pub mod deferred;
pub mod dispatch;
pub mod draw;
pub mod encode;
pub mod ext;
pub mod getters;
pub mod gs_ts;
pub mod immediate;
pub mod map;
pub mod mips;
pub mod pipeline_select;
pub mod resolve;
pub mod setters;
pub mod state;
pub mod sync;
pub mod update;
pub mod vtbl;

pub use deferred::Deferred;
pub use encode::CommandBufferState;
pub use immediate::Immediate;

/// Execution mode of [`Context<M>`] ([`Immediate`] or [`Deferred`]).
pub trait Mode: Sized + 'static {
    /// Emits a state-binding closure onto the current chunk or command list (`EmitST`).
    fn emit_st(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static);

    /// Emits a draw/dispatch/blit closure onto the current chunk or command list (`EmitOP`).
    fn emit_op(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static);

    /// Retains `t` until the chunk or command list retires.
    fn retain<T: Send + 'static>(ctx: &mut Context<Self>, t: T);

    /// Allocates `size` bytes aligned to `align` in the argument-buffer / CPU heap.
    fn alloc(ctx: &mut Context<Self>, size: usize, align: usize) -> (*mut u8, u64);

    /// Allocates a temporary staging slice `(buffer, offset)` of `size` bytes aligned to `align`.
    fn staging(ctx: &mut Context<Self>, size: u64, align: u64) -> (u64, u64);

    /// Current buffer allocation for `dynamic` in this context (`GetDynamicBufferLatestName`).
    fn dynamic_name(ctx: &mut Context<Self>, dynamic: &DynamicBuffer) -> Arc<BufferAllocation>;
}

/// Shared D3D11 context (`MTLD3D11DeviceContextImplBase`).
#[repr(C)]
pub struct Context<M: Mode> {
    pub vtbl: *const IMTLD3D11DeviceContextVtbl,
    pub ext_vtbl: *const IMTLD3D11ContextExt2Vtbl,
    pub annotation_vtbl: *const ID3DUserDefinedAnnotationVtbl,
    pub refs: Managed,
    pub private_data: PrivateData,
    pub device: *mut c_void,
    pub state: state::ContextState,
    pub cmdbuf_state: CommandBufferState,
    pub dirty: state::DirtyFlags,
    pub defaults: Defaults,
    pub mode: M,
}

impl<M: Mode> Context<M> {
    /// Recovers `&mut Context<M>` from a primary `ID3D11DeviceContext*` pointer.
    ///
    /// # Safety
    ///
    /// `this` is a live `Context<M>` interface pointer.
    pub unsafe fn from_iface<'a>(this: *mut c_void) -> &'a mut Self {
        // SAFETY: `vtbl` is the first field of `#[repr(C)] Context<M>`.
        unsafe { &mut *this.cast::<Self>() }
    }

    /// `IUnknown::QueryInterface`.
    pub const fn query_interface(&mut self, riid: *const c_void, out: *mut *mut c_void) -> Hresult {
        let _ = (riid, out);
        E_NOINTERFACE
    }

    /// `IUnknown::AddRef`.
    pub fn add_ref(&self) -> u32 {
        // SAFETY: `self.device` is the parent device pointer or null in unit tests.
        unsafe { self.refs.add_ref(self.device) }
    }

    /// `IUnknown::Release`.
    pub fn release(&self) -> u32 {
        // SAFETY: `self.device` is the parent device pointer or null in unit tests.
        unsafe { self.refs.release(self.device).0 }
    }

    /// `ID3D11DeviceChild::GetDevice`.
    pub const fn get_device(&self, out: *mut *mut c_void) {
        let _ = out;
    }

    /// `ID3D11DeviceChild::GetPrivateData`.
    pub const fn get_private_data(
        &self,
        guid: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> Hresult {
        let _ = (guid, data_size, data);
        E_NOTIMPL
    }

    /// `ID3D11DeviceChild::SetPrivateData`.
    pub const fn set_private_data(
        &mut self,
        guid: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> Hresult {
        let _ = (guid, data_size, data);
        E_NOTIMPL
    }

    /// `ID3D11DeviceChild::SetPrivateDataInterface`.
    pub const fn set_private_data_interface(
        &mut self,
        guid: *const c_void,
        object: *const c_void,
    ) -> Hresult {
        let _ = (guid, object);
        E_NOTIMPL
    }
}
