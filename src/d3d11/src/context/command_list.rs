//! `ID3D11CommandList`, pool, `FinishCommandList` and `ExecuteCommandList`.

use core::{
    any::Any,
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::sync::{Arc, Mutex, PoisonError};

use dxmt_core::{
    arena::CommandList as ArenaCommandList, query::VisibilityQuery, ring::DEFERRED_BLOCK,
    staging::Staging,
};
use dxmt_util::private_data::PrivateData;

use super::{
    Context, Immediate, Mode,
    deferred::{DeferredDynamicBuffer, DeferredDynamicTexture, DeferredIssuedQuery},
    state::DirtyFlags,
    sync::{ContextQuery, DXGI_ERROR_INVALID_CALL, QueryState},
};
use crate::{
    abi::vtbl::ID3D11CommandListVtbl,
    com::{
        E_INVALIDARG, E_NOINTERFACE, Guid, Hresult, IID_IUNKNOWN, Managed, S_OK, container_of,
        iface_of,
    },
    device::Device,
    query::Query,
};

const COMMAND_LIST_MAGIC: u64 = 0x4433_4431_3143_4D44;

/// `IID_ID3D11DeviceChild`: `1841e5c8-16b0-489b-bcc8-44cfb0d5deae`.
pub const IID_ID3D11_DEVICE_CHILD: Guid = Guid::new(
    0x1841_e5c8,
    0x16b0,
    0x489b,
    [0xbc, 0xc8, 0x44, 0xcf, 0xb0, 0xd5, 0xde, 0xae],
);

/// `IID_ID3D11CommandList`: `a24bc4d1-769e-43f7-8013-98ff566c18e2`.
pub const IID_ID3D11_COMMAND_LIST: Guid = Guid::new(
    0xa24b_c4d1,
    0x769e,
    0x43f7,
    [0x80, 19, 0x98, 0xff, 0x56, 0x6c, 0x18, 0xe2],
);

/// Device-owned pool of reusable [`CommandList`] instances (`MTLD3D11CommandListPool`).
#[derive(Default)]
pub struct CommandListPool {
    free_lists: Mutex<Vec<usize>>,
    recycled_total: AtomicUsize,
}

impl CommandListPool {
    /// Acquires a [`CommandList`] from the pool or allocates a fresh one.
    #[must_use]
    pub fn acquire(&self, device: *mut c_void, context_flags: u32) -> Box<CommandList> {
        let popped = self
            .free_lists
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        if let Some(raw) = popped {
            // SAFETY: `raw` was produced by `Box::into_raw` in `recycle`.
            let mut list = unsafe { Box::from_raw(raw as *mut CommandList) };
            list.reset_for_reuse(device, context_flags);
            return list;
        }
        CommandList::new_boxed(device, context_flags)
    }

    /// Resets `list` and returns it to the pool for future reuse.
    pub fn recycle(&self, mut list: Box<CommandList>) {
        list.clear_payload();
        self.recycled_total.fetch_add(1, Ordering::Relaxed);
        let raw = Box::into_raw(list) as usize;
        let mut free = self
            .free_lists
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        free.push(raw);
    }

    /// Number of free command lists currently sitting in the pool.
    #[must_use]
    pub fn free_count(&self) -> usize {
        self.free_lists
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Total number of command list recycle operations completed on this pool.
    #[must_use]
    pub fn recycled_total(&self) -> usize {
        self.recycled_total.load(Ordering::Relaxed)
    }
}

impl Drop for CommandListPool {
    fn drop(&mut self) {
        let mut free = self
            .free_lists
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        for raw in free.drain(..) {
            // SAFETY: `raw` was produced by `Box::into_raw` in `recycle`.
            let _ = unsafe { Box::from_raw(raw as *mut CommandList) };
        }
    }
}

/// Recorded D3D11 command list (`MTLD3D11CommandList`).
#[repr(C)]
pub struct CommandList {
    pub vtbl: *const ID3D11CommandListVtbl,
    magic: u64,
    pub refs: Managed,
    pub private_data: PrivateData,
    pub device: *mut c_void,
    pub context_flags: u32,
    pub commands: ArenaCommandList,
    pub retained: Vec<Box<dyn Any + Send>>,
    pub dynamic_buffers: Vec<DeferredDynamicBuffer>,
    pub dynamic_buffer_history: Vec<DeferredDynamicBuffer>,
    pub dynamic_textures: Vec<DeferredDynamicTexture>,
    pub dynamic_texture_history: Vec<DeferredDynamicTexture>,
    pub written_stagings: Vec<Arc<Staging>>,
    pub visibility_queries: Vec<Arc<VisibilityQuery>>,
    pub reissued_queries: Vec<DeferredIssuedQuery>,
    pub has_promoted_flush: bool,
    pub execution_count: AtomicU64,
    pub recycled_allocations: Arc<AtomicUsize>,
}

static COMMAND_LIST_VTBL: ID3D11CommandListVtbl = ID3D11CommandListVtbl {
    query_interface: cmdlist_query_interface,
    add_ref: cmdlist_add_ref,
    release: cmdlist_release,
    get_device: cmdlist_get_device,
    get_private_data: cmdlist_get_private_data,
    set_private_data: cmdlist_set_private_data,
    set_private_data_interface: cmdlist_set_private_data_interface,
    get_context_flags: cmdlist_get_context_flags,
};

impl CommandList {
    /// Allocates a new boxed [`CommandList`].
    #[must_use]
    pub fn new_boxed(device: *mut c_void, context_flags: u32) -> Box<Self> {
        let block = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
        let refs = Managed::new();
        // SAFETY: `device` is null or a valid COM device pointer.
        unsafe {
            refs.add_ref(ptr::null_mut());
        }
        Box::new(Self {
            vtbl: &raw const COMMAND_LIST_VTBL,
            magic: COMMAND_LIST_MAGIC,
            refs,
            private_data: PrivateData::default(),
            device,
            context_flags,
            commands: ArenaCommandList::new(block),
            retained: Vec::new(),
            dynamic_buffers: Vec::new(),
            dynamic_buffer_history: Vec::new(),
            dynamic_textures: Vec::new(),
            dynamic_texture_history: Vec::new(),
            written_stagings: Vec::new(),
            visibility_queries: Vec::new(),
            reissued_queries: Vec::new(),
            has_promoted_flush: false,
            execution_count: AtomicU64::new(0),
            recycled_allocations: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Recovers `&CommandList` from an `ID3D11CommandList*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        let raw = container_of!(iface, Self, vtbl);
        // SAFETY: `raw` points to the `CommandList` embedding `vtbl`.
        let this = unsafe { &*raw };
        if this.magic != COMMAND_LIST_MAGIC {
            return None;
        }
        Some(this)
    }

    fn reset_for_reuse(&mut self, device: *mut c_void, context_flags: u32) {
        self.clear_payload();
        self.device = device;
        self.context_flags = context_flags;
        // SAFETY: Null device pointer for internal pool reuse refcount.
        unsafe {
            self.refs.add_ref(ptr::null_mut());
        }
    }

    fn clear_payload(&mut self) {
        let block = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
        self.commands = ArenaCommandList::new(block);
        self.retained.clear();
        self.dynamic_buffers.clear();
        self.dynamic_buffer_history.clear();
        self.dynamic_textures.clear();
        self.dynamic_texture_history.clear();
        self.written_stagings.clear();
        self.visibility_queries.clear();
        self.reissued_queries.clear();
        self.has_promoted_flush = false;
    }
}

unsafe extern "system-unwind" fn cmdlist_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    if ppv_object.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `ppv_object` was checked for null above.
    unsafe {
        ppv_object.write_unaligned(ptr::null_mut());
    }
    if riid.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `riid` was checked for null above.
    let iid = unsafe { riid.cast::<Guid>().read_unaligned() };
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    if iid == IID_IUNKNOWN || iid == IID_ID3D11_DEVICE_CHILD || iid == IID_ID3D11_COMMAND_LIST {
        // SAFETY: `list.device` is the parent device or null.
        unsafe {
            list.refs.add_ref(list.device);
        }
        let iface = iface_of(&list.vtbl);
        // SAFETY: `ppv_object` was checked for null above.
        unsafe {
            ppv_object.write_unaligned(iface);
        }
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn cmdlist_add_ref(this: *mut c_void) -> u32 {
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    // SAFETY: `list.device` is the parent device or null.
    unsafe { list.refs.add_ref(list.device) }
}

unsafe extern "system-unwind" fn cmdlist_release(this: *mut c_void) -> u32 {
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of a live `CommandList`.
    let list = unsafe { &*raw };
    let dev = list.device;
    // SAFETY: `dev` is the parent device or null.
    let (remaining, destroy) = unsafe { list.refs.release(dev) };
    if destroy {
        // SAFETY: Both public and private refcounts reached zero.
        let _ = unsafe { Box::from_raw(raw) };
    }
    remaining
}

unsafe extern "system-unwind" fn cmdlist_get_device(
    this: *mut c_void,
    pp_device: *mut *mut c_void,
) {
    if pp_device.is_null() {
        return;
    }
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    if let Some(dev) = Device::from_iface(list.device) {
        let _ = dev;
    }
    // SAFETY: `pp_device` was checked for null above.
    unsafe {
        pp_device.write_unaligned(list.device);
    }
}

unsafe extern "system-unwind" fn cmdlist_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    p_data_size: *mut u32,
    p_data: *mut c_void,
) -> Hresult {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    // SAFETY: `guid` was checked for null above.
    let g = unsafe { guid.cast::<Guid>().read_unaligned() };
    // SAFETY: Forwards caller's `p_data_size` and `p_data` pointers to `PrivateData::get_data`.
    unsafe { list.private_data.get_data(g, p_data_size, p_data) }
}

unsafe extern "system-unwind" fn cmdlist_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    p_data: *const c_void,
) -> Hresult {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    // SAFETY: `guid` was checked for null above.
    let g = unsafe { guid.cast::<Guid>().read_unaligned() };
    // SAFETY: Forwards caller's `p_data` and `data_size` to `PrivateData::set_data`.
    unsafe { list.private_data.set_data(g, data_size, p_data) }
}

unsafe extern "system-unwind" fn cmdlist_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    p_data: *const c_void,
) -> Hresult {
    if guid.is_null() {
        return E_INVALIDARG;
    }
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    // SAFETY: `guid` was checked for null above.
    let g = unsafe { guid.cast::<Guid>().read_unaligned() };
    // SAFETY: Forwards caller's `p_data` interface pointer to `PrivateData::set_interface`.
    unsafe { list.private_data.set_interface(g, p_data.cast_mut()) }
}

unsafe extern "system-unwind" fn cmdlist_get_context_flags(this: *mut c_void) -> u32 {
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    list.context_flags
}

const fn write_cmdlist_out(out: *mut *mut c_void, val: *mut c_void) {
    // SAFETY: `out` was verified non-null by `finish_command_list`.
    unsafe {
        out.write_unaligned(val);
    }
}

/// Re-issues a deferred query onto `imm` during `ExecuteCommandList`.
fn reissue_deferred_query(imm: &mut Context<Immediate>, query: &DeferredIssuedQuery) {
    let seq = imm.mode.current_seq();
    imm.mode.has_dirty_op = true;
    match query {
        DeferredIssuedQuery::Event { query_ptr } => {
            if let Some(cq) = ContextQuery::from_ptr(*query_ptr as *mut c_void) {
                cq.seq = seq;
                cq.state = QueryState::Issued;
            } else if let Some(q) = Query::from_iface(*query_ptr as *mut c_void) {
                q.end(ptr::from_mut(imm).cast());
            }
        }
        DeferredIssuedQuery::Visibility { query_ptr, vq } => {
            let offset = imm.mode.visibility_offset;
            vq.begin(seq, offset);
            vq.end(seq, offset);
            imm.mode.visibility_offset += 1;
            imm.mode.active_visibility_queries.push((
                seq,
                imm.mode.visibility_offset,
                Arc::clone(vq),
            ));
            if let Some(cq) = ContextQuery::from_ptr(*query_ptr as *mut c_void) {
                cq.seq = seq;
                cq.visibility = Some(Arc::clone(vq));
                cq.state = QueryState::Issued;
            }
        }
        DeferredIssuedQuery::Timestamp { query_ptr, tq } => {
            if let Some(cq) = ContextQuery::from_ptr(*query_ptr as *mut c_void) {
                cq.seq = seq;
                cq.timestamp = Some(Arc::clone(tq));
                cq.state = QueryState::Issued;
            }
        }
    }
}

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::FinishCommandList`.
    pub fn finish_command_list(
        &mut self,
        restore_deferred_context_state: i32,
        command_list: *mut *mut c_void,
    ) -> Hresult {
        if command_list.is_null() {
            return E_INVALIDARG;
        }
        write_cmdlist_out(command_list, ptr::null_mut());
        if self.as_immediate().is_some() {
            return DXGI_ERROR_INVALID_CALL;
        }
        let saved_state = super::state::ContextState {
            sample_mask: self.state.sample_mask,
            stencil_ref: self.state.stencil_ref,
            blend_factor: self.state.blend_factor,
            topology: self.state.topology,
        };

        // End all queries left open on this deferred context before sealing the command list.
        let open_queries = if let Some(def) = self.as_deferred_mut() {
            mem::take(&mut def.mode.open_queries)
        } else {
            Vec::new()
        };
        for q in open_queries {
            self.end(q);
        }

        self.finish_pass(false);

        let Some(def) = self.as_deferred_mut() else {
            return DXGI_ERROR_INVALID_CALL;
        };

        let block = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
        let mut list = CommandList::new_boxed(def.device, def.mode.context_flags);
        list.commands = def
            .mode
            .commands
            .take()
            .unwrap_or_else(|| ArenaCommandList::new(block));
        def.mode.commands = Some(ArenaCommandList::new(block));
        list.retained = mem::take(&mut def.mode.retained);
        list.dynamic_buffers = def
            .mode
            .dynamic_buffers
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        list.dynamic_buffer_history = mem::take(&mut def.mode.dynamic_buffer_history);
        list.dynamic_textures = def
            .mode
            .dynamic_textures
            .drain()
            .map(|(_, entry)| entry)
            .collect();
        list.dynamic_texture_history = mem::take(&mut def.mode.dynamic_texture_history);
        list.written_stagings = mem::take(&mut def.mode.written_stagings);
        list.visibility_queries = mem::take(&mut def.mode.visibility_queries);
        def.mode.active_visibility_offsets.clear();
        list.reissued_queries = mem::take(&mut def.mode.reissued_queries);
        list.has_promoted_flush = mem::replace(&mut def.mode.has_promoted_flush, false);

        if restore_deferred_context_state == 0 {
            def.state = super::state::ContextState {
                sample_mask: 0xffff_ffff,
                ..super::state::ContextState::default()
            };
            def.clear_state();
        } else {
            def.state = saved_state;
            def.dirty = DirtyFlags::all();
        }

        let raw = Box::into_raw(list);
        // SAFETY: `raw` was just allocated by `Box::into_raw`.
        let iface = unsafe { iface_of(&(*raw).vtbl) };
        write_cmdlist_out(command_list, iface);
        S_OK
    }

    /// `ID3D11DeviceContext::ExecuteCommandList`.
    pub fn execute_command_list(&mut self, command_list: *mut c_void, restore_context_state: i32) {
        let Some(list) = CommandList::from_iface(command_list) else {
            return;
        };
        let saved_state = super::state::ContextState {
            sample_mask: self.state.sample_mask,
            stencil_ref: self.state.stencil_ref,
            blend_factor: self.state.blend_factor,
            topology: self.state.topology,
        };

        self.finish_pass(list.has_promoted_flush);

        if let Some(imm) = self.as_immediate_mut() {
            let seq = imm.mode.current_seq();
            imm.mode.has_dirty_op = true;

            for staging in &list.written_stagings {
                staging.use_copy_destination(seq);
                imm.mode.active_stagings.push((seq, Arc::clone(staging)));
            }

            for query in &list.reissued_queries {
                reissue_deferred_query(imm, query);
            }

            for entry in &list.dynamic_buffers {
                entry
                    .dynamic
                    .update_immediate_name(seq, Arc::clone(&entry.allocation), 0, true);
            }
            for entry in &list.dynamic_textures {
                entry
                    .dynamic
                    .update_immediate_name(seq, Arc::clone(&entry.allocation), true);
            }

            let vq_ptr = if list.visibility_queries.is_empty() {
                ptr::null()
            } else {
                list.visibility_queries.as_ptr()
            };
            let enc = imm.mode.encoder_mut();
            if !vq_ptr.is_null() {
                enc.push_deferred_visibility_queries(vq_ptr);
            }
            list.commands.run(enc);
            if !vq_ptr.is_null() {
                enc.pop_deferred_visibility_queries();
            }

            list.execution_count.fetch_add(1, Ordering::Relaxed);
            list.refs.add_ref_private();
            let raw_list = ptr::from_ref(list) as usize;
            let dyn_buf_history = list.dynamic_buffer_history.clone();
            let dyn_tex_history = list.dynamic_texture_history.clone();
            let recycled_counter = Arc::clone(&list.recycled_allocations);

            imm.register_in_flight(seq, move |completed_seq| {
                for entry in dyn_buf_history {
                    entry.dynamic.recycle(completed_seq, entry.allocation);
                    recycled_counter.fetch_add(1, Ordering::Relaxed);
                }
                for entry in dyn_tex_history {
                    entry.dynamic.recycle(completed_seq, entry.allocation);
                    recycled_counter.fetch_add(1, Ordering::Relaxed);
                }
                let raw = raw_list as *mut CommandList;
                // SAFETY: `add_ref_private` above keeps `raw` alive until this callback runs.
                let destroy = unsafe { (*raw).refs.release_private() };
                if destroy {
                    // SAFETY: Both public and private refcounts reached zero.
                    let _ = unsafe { Box::from_raw(raw) };
                }
            });
        } else if let Some(def) = self.as_deferred_mut() {
            if list.has_promoted_flush {
                def.mode.has_promoted_flush = true;
            }
            for staging in &list.written_stagings {
                def.mode.record_staging_write(staging);
            }
            for query in &list.reissued_queries {
                def.mode.reissued_queries.push(query.clone());
            }
            let raw_list = ptr::from_ref(list) as usize;
            list.refs.add_ref_private();
            def.mode.commands_mut().emit_fn(move |enc| {
                // SAFETY: `raw_list` is kept alive by `add_ref_private`.
                let l = unsafe { &*(raw_list as *const CommandList) };
                l.commands.run(enc);
            });
        }

        self.finish_pass(list.has_promoted_flush);

        if restore_context_state == 0 {
            self.state = super::state::ContextState {
                sample_mask: 0xffff_ffff,
                ..super::state::ContextState::default()
            };
            self.clear_state();
        } else {
            self.state = saved_state;
            self.dirty = DirtyFlags::all();
        }
    }
}

#[cfg(test)]
mod tests;
