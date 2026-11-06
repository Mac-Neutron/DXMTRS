//! `ID3D11CommandList`, pool, `FinishCommandList` and `ExecuteCommandList`.

use core::{
    any::Any,
    ffi::c_void,
    mem, ptr,
    sync::atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::sync::{Arc, Mutex};

use dxmt_core::{
    arena::CommandList as ArenaCommandList, query::VisibilityQuery, ring::DEFERRED_BLOCK,
    staging::Staging,
};
use dxmt_util::{
    com::{Qi, QiMiss, hresult::DXGI_ERROR_INVALID_CALL, qi_table},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
    sync::lock,
};

use super::{
    Context, Immediate, Mode,
    deferred::{DeferredDynamicBuffer, DeferredDynamicTexture, DeferredIssuedQuery},
    state::DirtyFlags,
    sync::{ContextQuery, QueryState},
};
use crate::{
    abi::{IID_ID3D11_COMMAND_LIST, IID_ID3D11_DEVICE_CHILD, vtbl::ID3D11CommandListVtbl},
    com::{E_INVALIDARG, Hresult, IID_IUNKNOWN, Managed, S_OK, add_ref, container_of, iface_of},
    query::Query,
};

const COMMAND_LIST_MAGIC: u64 = 0x4433_4431_3143_4D44;

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
        let popped = lock(&self.free_lists).pop();
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
        lock(&self.free_lists).push(raw);
    }

    /// Number of free command lists currently sitting in the pool.
    #[must_use]
    pub fn free_count(&self) -> usize {
        lock(&self.free_lists).len()
    }

    /// Total number of command list recycle operations completed on this pool.
    #[must_use]
    pub fn recycled_total(&self) -> usize {
        self.recycled_total.load(Ordering::Relaxed)
    }
}

impl Drop for CommandListPool {
    fn drop(&mut self) {
        let mut free = lock(&self.free_lists);
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
    pub read_stagings: Vec<Arc<Staging>>,
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
    get_private_data: get_private_data::<CommandList>,
    set_private_data: set_private_data::<CommandList>,
    set_private_data_interface: set_private_data_interface::<CommandList>,
    get_context_flags: cmdlist_get_context_flags,
};

fn live_dev(device: *mut c_void) -> *mut c_void {
    if crate::device::Device::from_iface(device).is_some() {
        device
    } else {
        ptr::null_mut()
    }
}

impl CommandList {
    /// Allocates a new boxed [`CommandList`].
    #[must_use]
    pub fn new_boxed(device: *mut c_void, context_flags: u32) -> Box<Self> {
        let block = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
        let refs = Managed::new();
        // SAFETY: `live_dev(device)` is either null (in unit tests) or a live COM device pointer.
        unsafe {
            refs.add_ref(live_dev(device));
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
            read_stagings: Vec::new(),
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
        // SAFETY: `live_dev(device)` is either null or a live COM device pointer.
        unsafe {
            self.refs.add_ref(live_dev(device));
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
        self.read_stagings.clear();
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
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    let qi = Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &crate::LOG,
        miss: QiMiss::Once {
            object: &IID_ID3D11_COMMAND_LIST,
            name: "D3D11CommandList",
        },
    };
    let add_ref = || {
        // SAFETY: `live_dev(list.device)` is the live parent device or null.
        unsafe {
            list.refs.add_ref(live_dev(list.device));
        }
    };
    // SAFETY: the application's `riid` and `ppv_object`, null or valid.
    unsafe {
        qi_table(
            riid,
            ppv_object,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_ID3D11_DEVICE_CHILD,
                    IID_ID3D11_COMMAND_LIST,
                ],
                iface_of(&list.vtbl),
            )],
            add_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn cmdlist_add_ref(this: *mut c_void) -> u32 {
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of `CommandList`.
    let list = unsafe { &*raw };
    // SAFETY: `live_dev(list.device)` is the live parent device or null.
    unsafe { list.refs.add_ref(live_dev(list.device)) }
}

unsafe extern "system-unwind" fn cmdlist_release(this: *mut c_void) -> u32 {
    let raw = container_of!(this, CommandList, vtbl);
    // SAFETY: `this` is the `vtbl` pointer of a live `CommandList`.
    let list = unsafe { &*raw };
    let dev = live_dev(list.device);
    // SAFETY: `dev` is the live parent device or null.
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
    // SAFETY: `live_dev(list.device)` is the live parent device or null.
    unsafe {
        add_ref(live_dev(list.device));
    }
    // SAFETY: `pp_device` was checked for null above.
    unsafe {
        pp_device.write_unaligned(list.device);
    }
}

impl HasPrivateData for CommandList {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is the `vtbl` pointer of a live `CommandList`.
        &unsafe { &*container_of!(this, Self, vtbl) }.private_data
    }
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

struct LiveCommandListCleanup {
    device: usize,
    raw_list: usize,
    dyn_buf_history: Vec<DeferredDynamicBuffer>,
    dyn_tex_history: Vec<DeferredDynamicTexture>,
    recycled_counter: Arc<AtomicUsize>,
}

impl Drop for LiveCommandListCleanup {
    fn drop(&mut self) {
        let completed_seq = super::immediate::live_device(self.device as *mut c_void)
            .map_or(0, |d| d.core().queue().coherent_seq());
        for entry in self.dyn_buf_history.drain(..) {
            entry.dynamic.recycle(completed_seq, entry.allocation);
            self.recycled_counter.fetch_add(1, Ordering::Relaxed);
        }
        for entry in self.dyn_tex_history.drain(..) {
            entry.dynamic.recycle(completed_seq, entry.allocation);
            self.recycled_counter.fetch_add(1, Ordering::Relaxed);
        }
        let raw = self.raw_list as *mut CommandList;
        // SAFETY: `add_ref_private` in `execute_command_list` keeps `raw` alive until drop.
        let destroy = unsafe { (*raw).refs.release_private() };
        if destroy {
            // SAFETY: Both public and private refcounts reached zero.
            let _ = unsafe { Box::from_raw(raw) };
        }
    }
}

/// Re-issues a deferred query onto `imm` during `ExecuteCommandList`.
fn reissue_deferred_query(imm: &mut Context<Immediate>, query: &DeferredIssuedQuery) {
    let seq = imm.active_seq();
    imm.mode.has_dirty_op = true;
    match query {
        DeferredIssuedQuery::Event { query_ptr } => {
            if let Some(cq) = ContextQuery::from_ptr(*query_ptr as *mut c_void) {
                cq.seq = seq;
                cq.state = QueryState::Issued;
            } else if Query::from_iface(*query_ptr as *mut c_void).is_some() {
                imm.end(*query_ptr as *mut c_void);
            }
        }
        DeferredIssuedQuery::Visibility { query_ptr, vq } => {
            if !imm.is_live() {
                let offset = imm.mode.visibility_offset;
                vq.begin(seq, offset);
                vq.end(seq, offset);
                imm.mode.visibility_offset += 1;
                imm.mode.active_visibility_queries.push((
                    seq,
                    imm.mode.visibility_offset,
                    Arc::clone(vq),
                ));
            }
            if let Some(cq) = ContextQuery::from_ptr(*query_ptr as *mut c_void) {
                cq.seq = seq;
                cq.visibility = Some(Arc::clone(vq));
                cq.state = QueryState::Issued;
            } else if let Some(q) = Query::from_iface(*query_ptr as *mut c_void) {
                q.do_deferred_query(Arc::clone(vq));
            }
        }
        DeferredIssuedQuery::Timestamp { query_ptr, tq } => {
            if let Some(cq) = ContextQuery::from_ptr(*query_ptr as *mut c_void) {
                cq.seq = seq;
                cq.timestamp = Some(Arc::clone(tq));
                cq.state = QueryState::Issued;
            } else if Query::from_iface(*query_ptr as *mut c_void).is_some() {
                imm.end(*query_ptr as *mut c_void);
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
        list.read_stagings = mem::take(&mut def.mode.read_stagings);
        list.visibility_queries = mem::take(&mut def.mode.visibility_queries);
        def.mode.active_visibility_offsets.clear();
        list.reissued_queries = mem::take(&mut def.mode.reissued_queries);
        list.has_promoted_flush = mem::replace(&mut def.mode.has_promoted_flush, false);

        if restore_deferred_context_state == 0 {
            def.clear_state();
        } else {
            def.dirty = DirtyFlags::all();
            def.restore_encoding_context_state();
        }

        let raw = Box::into_raw(list);
        // SAFETY: `raw` was just allocated by `Box::into_raw`.
        let iface = unsafe { iface_of(&(*raw).vtbl) };
        write_cmdlist_out(command_list, iface);
        S_OK
    }

    /// `ID3D11DeviceContext::ExecuteCommandList`.
    pub fn execute_command_list(&mut self, command_list: *mut c_void, restore_context_state: i32) {
        if self.as_deferred().is_some() {
            crate::LOG.err("ExecuteCommandList: not supported on deferred context");
            #[cfg(all(not(windows), not(test)))]
            {
                // SAFETY: Recording not-implemented call for native probe diagnostic.
                unsafe {
                    winemetal_sys::wmt_native_not_implemented();
                }
            }
            return;
        }
        let Some(list) = CommandList::from_iface(command_list) else {
            return;
        };

        self.finish_pass(list.has_promoted_flush);

        if let Some(imm) = self.as_immediate_mut() {
            let seq = imm.active_seq();
            imm.mode.has_dirty_op = true;

            for staging in &list.read_stagings {
                staging.use_copy_source(seq);
            }
            for staging in &list.written_stagings {
                staging.use_copy_destination(seq);
                if !imm.is_live() {
                    imm.mode.active_stagings.push((seq, Arc::clone(staging)));
                }
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

            for vq in &list.visibility_queries {
                vq.reset();
            }

            list.execution_count.fetch_add(1, Ordering::Relaxed);
            list.refs.add_ref_private();
            let raw_list = ptr::from_ref(list) as usize;
            let dyn_buf_history = list.dynamic_buffer_history.clone();
            let dyn_tex_history = list.dynamic_texture_history.clone();
            let recycled_counter = Arc::clone(&list.recycled_allocations);

            if let Some(dev) = super::immediate::live_device(imm.device) {
                let vq_list = list.visibility_queries.clone();
                dev.core_mut().queue_mut().chunk().emit(move |enc| {
                    // SAFETY: `raw_list` is kept alive by `LiveCommandListCleanup` in the chunk.
                    let l = unsafe { &*(raw_list as *const CommandList) };
                    let vq_ptr = if vq_list.is_empty() {
                        ptr::null()
                    } else {
                        vq_list.as_ptr()
                    };
                    if !vq_ptr.is_null() {
                        enc.push_deferred_visibility_queries(vq_ptr);
                    }
                    l.commands.run(enc);
                    if !vq_ptr.is_null() {
                        enc.pop_deferred_visibility_queries();
                    }
                });
                dev.core_mut()
                    .queue_mut()
                    .chunk()
                    .retain(LiveCommandListCleanup {
                        device: imm.device as usize,
                        raw_list,
                        dyn_buf_history,
                        dyn_tex_history,
                        recycled_counter,
                    });
            } else {
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
            }

            for query in &list.reissued_queries {
                reissue_deferred_query(imm, query);
            }
        }

        self.finish_pass(list.has_promoted_flush);

        if restore_context_state == 0 {
            self.clear_state();
        } else {
            self.dirty = DirtyFlags::all();
            self.restore_encoding_context_state();
        }
    }
}

#[cfg(test)]
mod tests;
