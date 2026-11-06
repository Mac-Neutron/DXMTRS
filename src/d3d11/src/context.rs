//! D3D11 immediate and deferred device contexts (`d3d11_context.hpp`, `d3d11_context_impl.cpp`).
//!
//! [`Context<M>`] is generic over [`Mode`] ([`Immediate`] and [`Deferred`]), sharing state,
//! encoding and transfer logic across both context kinds.

use core::ffi::c_void;
use std::sync::Arc;

use dxmt_core::{dynamic::DynamicBuffer, encoder::Encoder, resource::BufferAllocation};
use dxmt_util::{
    com::{Qi, QiMiss, qi_table},
    private_data::PrivateData,
};

use crate::{
    abi::{
        IID_ID3D_USER_DEFINED_ANNOTATION, IID_ID3D11_DEVICE_CHILD, IID_ID3D11_DEVICE_CONTEXT,
        IID_ID3D11_DEVICE_CONTEXT_1, IID_ID3D11_DEVICE_CONTEXT_2, IID_ID3D11_DEVICE_CONTEXT_3,
        IID_ID3D11_DEVICE_CONTEXT_4,
        ext::{
            IID_D3D11_CONTEXT_EXT, IID_D3D11_CONTEXT_EXT1, IID_D3D11_CONTEXT_EXT2,
            IMTLD3D11ContextExt2Vtbl, IMTLD3D11DeviceContextVtbl,
        },
        vtbl::ID3DUserDefinedAnnotationVtbl,
    },
    com::{E_NOINTERFACE, Guid, Hresult, IID_IUNKNOWN, Managed, iface_of},
    state::Defaults,
};

const IID_IMTL_D3D11_DEVICE_CONTEXT: Guid = Guid::new(
    0x3a3f_085a,
    0xd0fe,
    0x4324,
    [0xb0, 0xae, 0xfe, 0x04, 0xde, 0x18, 0x57, 0x1c],
);

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

    /// Acquires the device multithread lock before dereferencing `this`, when applicable.
    #[must_use]
    fn lock_iface(this: *mut c_void) -> Option<crate::multithread::DeviceLockGuard<'static>> {
        let _ = this;
        None
    }
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
    pub(super) fn query_interface(
        &mut self,
        riid: *const c_void,
        out: *mut *mut c_void,
    ) -> Hresult {
        let (context, ext, annotation) = (
            iface_of(&self.vtbl),
            iface_of(&self.ext_vtbl),
            iface_of(&self.annotation_vtbl),
        );
        // `IMTLD3D11DeviceContextExt2` is not offered on Windows.
        let ext_iids: &[Guid] = if cfg!(not(target_os = "windows")) {
            &[
                IID_D3D11_CONTEXT_EXT,
                IID_D3D11_CONTEXT_EXT1,
                IID_D3D11_CONTEXT_EXT2,
            ]
        } else {
            &[IID_D3D11_CONTEXT_EXT, IID_D3D11_CONTEXT_EXT1]
        };
        // Only the immediate context is multithread-protected.
        let (multithread_iids, multithread): (&[Guid], *mut c_void) = match self.as_immediate_mut()
        {
            Some(imm) => (
                &[crate::abi::IID_ID3D11_MULTITHREAD],
                imm.mode.multithread.as_iface(),
            ),
            None => (&[], core::ptr::null_mut()),
        };
        let qi = Qi {
            null_riid: E_NOINTERFACE,
            silent: &[],
            log: &crate::LOG,
            miss: QiMiss::Once {
                object: &IID_ID3D11_DEVICE_CONTEXT_4,
                name: "D3D11DeviceContext",
            },
        };
        // SAFETY: the application's `riid` and `out`, null or valid.
        unsafe {
            qi_table(
                riid,
                out,
                &[
                    (
                        &[
                            IID_IUNKNOWN,
                            IID_ID3D11_DEVICE_CHILD,
                            IID_ID3D11_DEVICE_CONTEXT,
                            IID_ID3D11_DEVICE_CONTEXT_1,
                            IID_ID3D11_DEVICE_CONTEXT_2,
                            IID_ID3D11_DEVICE_CONTEXT_3,
                            IID_ID3D11_DEVICE_CONTEXT_4,
                            IID_IMTL_D3D11_DEVICE_CONTEXT,
                        ],
                        context,
                    ),
                    (ext_iids, ext),
                    (&[IID_ID3D_USER_DEFINED_ANNOTATION], annotation),
                    (multithread_iids, multithread),
                ],
                || {
                    self.add_ref();
                },
                &qi,
            )
        }
    }

    /// `IUnknown::AddRef`.
    pub fn add_ref(&self) -> u32 {
        // SAFETY: `self.device` is the parent device pointer or null in unit tests.
        unsafe { self.refs.add_ref(self.device) }
    }

    /// `IUnknown::Release`.
    pub fn release(&self) -> u32 {
        // SAFETY: `self.device` is the parent device pointer or null in unit tests.
        let (rem, destroy) = unsafe { self.refs.release(self.device) };
        if destroy {
            // SAFETY: `self` was heap-allocated via `Box::into_raw` and its final reference dropped.
            drop(unsafe { Box::from_raw(core::ptr::from_ref(self).cast_mut()) });
        }
        rem
    }

    /// `ID3D11DeviceChild::GetDevice`.
    pub(super) fn get_device(&self, out: *mut *mut c_void) {
        if out.is_null() {
            return;
        }
        // SAFETY: `self.device` is a live COM device pointer or null.
        unsafe {
            crate::com::add_ref(self.device);
        }
        // SAFETY: `out` was verified non-null.
        unsafe {
            out.write_unaligned(self.device);
        }
    }

    /// `ID3D11DeviceChild::GetPrivateData`.
    pub(super) fn get_private_data(
        &self,
        guid: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> Hresult {
        // SAFETY: the application's `guid`, `data_size` and `data`, forwarded.
        unsafe { self.private_data.get_ffi(guid, data_size, data) }
    }

    /// `ID3D11DeviceChild::SetPrivateData`.
    pub(super) fn set_private_data(
        &mut self,
        guid: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> Hresult {
        // SAFETY: the application's `guid` and `data`, forwarded.
        unsafe { self.private_data.set_ffi(guid, data_size, data) }
    }

    /// `ID3D11DeviceChild::SetPrivateDataInterface`.
    pub(super) fn set_private_data_interface(
        &mut self,
        guid: *const c_void,
        object: *const c_void,
    ) -> Hresult {
        // SAFETY: the application's `guid` and `object`, forwarded.
        unsafe { self.private_data.set_interface_ffi(guid, object) }
    }
}
