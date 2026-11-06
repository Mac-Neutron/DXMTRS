//! Deferred context mode (`Deferred`) and `CreateDeferredContext*` (`d3d11_context_def.cpp`).

use core::{
    any::{Any, TypeId},
    ffi::c_void,
    ptr,
};
use std::sync::{Arc, Mutex};

use dxmt_core::{
    arena::CommandList as ArenaCommandList,
    dynamic::{DynamicBuffer, DynamicLinearTexture},
    encoder::Encoder,
    query::{TimestampQuery, VisibilityQuery},
    resource::{BufferAllocation, TextureAllocation},
    ring::{DEFERRED_BLOCK, RingBump},
    staging::Staging,
};
use dxmt_util::private_data::PrivateData;
use rustc_hash::FxHashMap;

use super::{
    Context, Mode,
    encode::CommandBufferState,
    state::DirtyFlags,
    vtbl::{DEFERRED_ANNOTATION_VTBL, DEFERRED_EXT_VTBL, DEFERRED_VTBL},
};
use crate::{
    abi::ext::IMTLD3D11DeviceContextVtbl,
    com::{E_INVALIDARG, Hresult, Managed, S_OK, iface_of},
    state::Defaults,
};

struct AlignedBlock {
    ptr: usize,
    layout: core::alloc::Layout,
}

impl Drop for AlignedBlock {
    fn drop(&mut self) {
        // SAFETY: `self.ptr` was allocated with `std::alloc::alloc_zeroed(self.layout)`.
        unsafe {
            std::alloc::dealloc(self.ptr as *mut u8, self.layout);
        }
    }
}

/// Per-command-list dynamic buffer allocation recorded by a deferred context.
pub struct DeferredDynamicBuffer {
    pub dynamic: Arc<DynamicBuffer>,
    pub allocation: Arc<BufferAllocation>,
}

impl Clone for DeferredDynamicBuffer {
    fn clone(&self) -> Self {
        Self {
            dynamic: Arc::clone(&self.dynamic),
            allocation: Arc::clone(&self.allocation),
        }
    }
}

/// Per-command-list dynamic texture allocation recorded by a deferred context.
pub struct DeferredDynamicTexture {
    pub dynamic: Arc<DynamicLinearTexture>,
    pub allocation: Arc<TextureAllocation>,
}

impl Clone for DeferredDynamicTexture {
    fn clone(&self) -> Self {
        Self {
            dynamic: Arc::clone(&self.dynamic),
            allocation: Arc::clone(&self.allocation),
        }
    }
}

/// Query operation recorded in a deferred context to be re-issued on `ExecuteCommandList`.
pub enum DeferredIssuedQuery {
    Event {
        query_ptr: usize,
    },
    Visibility {
        query_ptr: usize,
        vq: Arc<VisibilityQuery>,
    },
    Timestamp {
        query_ptr: usize,
        tq: Arc<TimestampQuery>,
    },
}

impl Clone for DeferredIssuedQuery {
    fn clone(&self) -> Self {
        match self {
            Self::Event { query_ptr } => Self::Event {
                query_ptr: *query_ptr,
            },
            Self::Visibility { query_ptr, vq } => Self::Visibility {
                query_ptr: *query_ptr,
                vq: Arc::clone(vq),
            },
            Self::Timestamp { query_ptr, tq } => Self::Timestamp {
                query_ptr: *query_ptr,
                tq: Arc::clone(tq),
            },
        }
    }
}

/// Deferred execution mode for [`Context<Deferred>`].
#[derive(Default)]
pub struct Deferred {
    pub context_flags: u32,
    pub commands: Option<ArenaCommandList>,
    pub retained: Vec<Box<dyn Any + Send>>,
    pub staging_allocator: Option<RingBump>,
    pub dynamic_buffers: FxHashMap<usize, DeferredDynamicBuffer>,
    pub dynamic_textures: FxHashMap<usize, DeferredDynamicTexture>,
    pub dynamic_buffer_history: Vec<DeferredDynamicBuffer>,
    pub dynamic_texture_history: Vec<DeferredDynamicTexture>,
    pub written_stagings: Vec<Arc<Staging>>,
    pub read_stagings: Vec<Arc<Staging>>,
    pub visibility_queries: Vec<Arc<VisibilityQuery>>,
    pub active_visibility_offsets: FxHashMap<usize, u32>,
    pub open_queries: Vec<*mut c_void>,
    pub reissued_queries: Vec<DeferredIssuedQuery>,
    pub has_promoted_flush: bool,
    pub hw_protection_enabled: bool,
}

impl Deferred {
    /// Returns a mutable reference to the active [`ArenaCommandList`].
    pub fn commands_mut(&mut self) -> &mut ArenaCommandList {
        let block = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
        self.commands
            .get_or_insert_with(|| ArenaCommandList::new(block))
    }

    /// Looks up or allocates the current command list's [`BufferAllocation`] for `dynamic`.
    pub fn get_or_alloc_dynamic_buffer(
        &mut self,
        dynamic: &Arc<DynamicBuffer>,
    ) -> Arc<BufferAllocation> {
        let key = Arc::as_ptr(dynamic) as usize;
        if let Some(entry) = self.dynamic_buffers.get(&key) {
            return Arc::clone(&entry.allocation);
        }
        let allocation = dynamic.allocate(0);
        let entry = DeferredDynamicBuffer {
            dynamic: Arc::clone(dynamic),
            allocation: Arc::clone(&allocation),
        };
        self.dynamic_buffer_history.push(entry.clone());
        self.dynamic_buffers.insert(key, entry);
        allocation
    }

    /// Allocates a new [`BufferAllocation`] for `dynamic` on `D3D11_MAP_WRITE_DISCARD`.
    pub fn rotate_dynamic_buffer(&mut self, dynamic: &Arc<DynamicBuffer>) -> Arc<BufferAllocation> {
        let key = Arc::as_ptr(dynamic) as usize;
        let allocation = dynamic.allocate(0);
        let entry = DeferredDynamicBuffer {
            dynamic: Arc::clone(dynamic),
            allocation: Arc::clone(&allocation),
        };
        self.dynamic_buffer_history.push(entry.clone());
        self.dynamic_buffers.insert(key, entry);
        allocation
    }

    /// Looks up or allocates the current command list's [`TextureAllocation`] for `dynamic`.
    pub fn get_or_alloc_dynamic_texture(
        &mut self,
        dynamic: &Arc<DynamicLinearTexture>,
    ) -> Arc<TextureAllocation> {
        let key = Arc::as_ptr(dynamic) as usize;
        if let Some(entry) = self.dynamic_textures.get(&key) {
            return Arc::clone(&entry.allocation);
        }
        let allocation = dynamic.allocate(0);
        let entry = DeferredDynamicTexture {
            dynamic: Arc::clone(dynamic),
            allocation: Arc::clone(&allocation),
        };
        self.dynamic_texture_history.push(entry.clone());
        self.dynamic_textures.insert(key, entry);
        allocation
    }

    /// Allocates a new [`TextureAllocation`] for `dynamic` on `D3D11_MAP_WRITE_DISCARD`.
    pub fn rotate_dynamic_texture(
        &mut self,
        dynamic: &Arc<DynamicLinearTexture>,
    ) -> Arc<TextureAllocation> {
        let key = Arc::as_ptr(dynamic) as usize;
        let allocation = dynamic.allocate(0);
        let entry = DeferredDynamicTexture {
            dynamic: Arc::clone(dynamic),
            allocation: Arc::clone(&allocation),
        };
        self.dynamic_texture_history.push(entry.clone());
        self.dynamic_textures.insert(key, entry);
        allocation
    }

    /// Records that `staging` was written to as a copy destination in this command list.
    pub fn record_staging_write(&mut self, staging: &Arc<Staging>) {
        let ptr = Arc::as_ptr(staging);
        if !self
            .written_stagings
            .iter()
            .any(|existing| ptr::eq(Arc::as_ptr(existing), ptr))
        {
            self.written_stagings.push(Arc::clone(staging));
        }
    }

    /// Records that `staging` was read from as a copy source in this command list.
    pub fn record_staging_read(&mut self, staging: &Arc<Staging>) {
        let ptr = Arc::as_ptr(staging);
        if !self
            .read_stagings
            .iter()
            .any(|existing| ptr::eq(Arc::as_ptr(existing), ptr))
        {
            self.read_stagings.push(Arc::clone(staging));
        }
    }
}

impl Mode for Deferred {
    fn emit_st(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        ctx.mode.commands_mut().emit_fn(f);
    }

    fn emit_op(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        ctx.mode.commands_mut().emit_fn(f);
    }

    fn retain<T: Send + 'static>(ctx: &mut Context<Self>, t: T) {
        ctx.mode.retained.push(Box::new(Mutex::new(Some(t))));
    }

    fn alloc(ctx: &mut Context<Self>, size: usize, align: usize) -> (*mut u8, u64) {
        let len = size.max(1);
        let alignment = align.max(1);
        let Ok(layout) = core::alloc::Layout::from_size_align(len, alignment) else {
            return (ptr::null_mut(), 0);
        };
        // SAFETY: `layout` has non-zero size (`len >= 1`).
        let raw = unsafe { std::alloc::alloc_zeroed(layout) };
        if raw.is_null() {
            return (ptr::null_mut(), 0);
        }
        ctx.mode.retained.push(Box::new(AlignedBlock {
            ptr: raw as usize,
            layout,
        }));
        (raw, raw as usize as u64)
    }

    fn staging(ctx: &mut Context<Self>, size: u64, align: u64) -> (u64, u64) {
        if let Some(ring) = ctx.mode.staging_allocator.as_mut() {
            let (block, offset) = ring.allocate(0, 0, size.max(1), align.max(1));
            if block.buffer != 0 {
                return (block.buffer, offset);
            }
            return (block.gpu_address + offset, offset);
        }
        let (_, gpu_addr) = Self::alloc(
            ctx,
            usize::try_from(size.max(1)).unwrap_or(1),
            usize::try_from(align.max(1)).unwrap_or(1),
        );
        (gpu_addr, 0)
    }

    fn dynamic_name(ctx: &mut Context<Self>, dynamic: &DynamicBuffer) -> Arc<BufferAllocation> {
        let key = ptr::from_ref(dynamic) as usize;
        if let Some(entry) = ctx.mode.dynamic_buffers.get(&key) {
            return Arc::clone(&entry.allocation);
        }
        dynamic.allocate(0)
    }
}

impl Context<Deferred> {
    /// Allocates and initializes a new deferred context (`MTLD3D11DeferredContext`).
    #[must_use]
    pub(super) fn new_deferred(device: *mut c_void, context_flags: u32) -> Box<Self> {
        let block = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
        let staging_allocator = super::immediate::live_device(device).map(|dev| {
            RingBump::new(
                dev.core().device(),
                dxmt_core::resource::BufferAllocationFlags::CPU_WRITE_COMBINED,
                dxmt_core::ring::RingConfig {
                    block: DEFERRED_BLOCK,
                    lifetime: 0,
                },
            )
        });
        let mode = Deferred {
            context_flags,
            commands: Some(ArenaCommandList::new(block)),
            staging_allocator,
            ..Deferred::default()
        };
        let refs = Managed::new();
        // SAFETY: `device` is null or a valid COM device pointer.
        unsafe {
            refs.add_ref(device);
        }
        Box::new(Self {
            vtbl: (&raw const DEFERRED_VTBL).cast::<IMTLD3D11DeviceContextVtbl>(),
            ext_vtbl: &raw const DEFERRED_EXT_VTBL,
            annotation_vtbl: &raw const DEFERRED_ANNOTATION_VTBL,
            refs,
            private_data: PrivateData::default(),
            device,
            state: super::state::ContextState::default(),
            cmdbuf_state: CommandBufferState::Idle,
            encode: super::encode::EncodeSession::default(),
            dirty: DirtyFlags::all(),
            defaults: Defaults::new(device),
            mode,
        })
    }

    /// Records a staging buffer copy destination on this deferred context.
    pub fn record_staging_copy_destination(&mut self, staging: &Arc<Staging>) {
        self.mode.record_staging_write(staging);
    }
}

impl<M: Mode> Context<M> {
    /// Downcasts `&self` to `&Context<Deferred>` when `M` is [`Deferred`].
    #[must_use]
    pub fn as_deferred(&self) -> Option<&Context<Deferred>> {
        if TypeId::of::<M>() == TypeId::of::<Deferred>() {
            // SAFETY: `TypeId` equality guarantees `Self` is `Context<Deferred>`.
            Some(unsafe { &*(ptr::from_ref(self).cast::<Context<Deferred>>()) })
        } else {
            None
        }
    }

    /// Downcasts `&mut self` to `&mut Context<Deferred>` when `M` is [`Deferred`].
    #[must_use]
    pub fn as_deferred_mut(&mut self) -> Option<&mut Context<Deferred>> {
        if TypeId::of::<M>() == TypeId::of::<Deferred>() {
            // SAFETY: `TypeId` equality guarantees `Self` is `Context<Deferred>`.
            Some(unsafe { &mut *(ptr::from_mut(self).cast::<Context<Deferred>>()) })
        } else {
            None
        }
    }
}

const fn write_deferred_out(out: *mut *mut c_void, iface: *mut c_void) {
    // SAFETY: `out` was verified non-null by `create_deferred`.
    unsafe {
        out.write_unaligned(iface);
    }
}

/// Creates a deferred context (`ID3D11Device::CreateDeferredContext*`).
pub fn create_deferred(device: *mut c_void, context_flags: u32, out: *mut *mut c_void) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    let ctx = Box::into_raw(Context::new_deferred(device, context_flags));
    // SAFETY: `ctx` was just allocated by `Box::into_raw`.
    let iface = unsafe { iface_of(&(*ctx).vtbl) };
    write_deferred_out(out, iface);
    S_OK
}

#[cfg(test)]
mod tests;
