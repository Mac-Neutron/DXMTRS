//! Immediate context mode (`Immediate`) and `IMTLD3D11DeviceContext` (`d3d11_context_imm.cpp`).

use core::{
    any::TypeId,
    ffi::c_void,
    mem::ManuallyDrop,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(test)]
use std::sync::LazyLock;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use dxmt_core::{
    arena::{BlockPool, Commands},
    dynamic::DynamicBuffer,
    encoder::Encoder,
    query::VisibilityQuery,
    resource::BufferAllocation,
    ring::{DEFERRED_BLOCK, RingBump},
    staging::Staging,
};
use dxmt_util::private_data::PrivateData;
#[cfg(test)]
use rustc_hash::FxHashMap;
use winemetal_sys::{MTLSharedEvent_setWin32EventAtValue, Obj};

use super::{
    Context, Mode,
    encode::CommandBufferState,
    state::DirtyFlags,
    vtbl::{IMMEDIATE_ANNOTATION_VTBL, IMMEDIATE_EXT_VTBL, IMMEDIATE_VTBL},
};
use crate::{com::Managed, resource, state::Defaults};

type InFlightCallback = Box<dyn FnOnce(u64) + Send>;

/// Per-encoder trace lines the unit tests read back; the DLL builds none of it.
#[cfg(test)]
static ENCODER_TRACES: LazyLock<Mutex<FxHashMap<usize, Vec<String>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// Appends a trace record associated with `enc`.
#[cfg(test)]
pub fn push_encoder_trace(enc: &Encoder, entry: impl Into<String>) {
    let key = ptr::from_ref(enc).addr();
    let mut map = dxmt_util::sync::lock(&ENCODER_TRACES);
    map.entry(key).or_default().push(entry.into());
}

/// Returns a snapshot of the trace records recorded on `enc`.
#[cfg(test)]
#[must_use]
pub fn encoder_trace(enc: &Encoder) -> Vec<String> {
    let key = ptr::from_ref(enc).addr();
    let map = dxmt_util::sync::lock(&ENCODER_TRACES);
    map.get(&key).cloned().unwrap_or_default()
}

/// Clears trace records associated with `enc`.
#[cfg(test)]
pub fn clear_encoder_trace(enc: &Encoder) {
    let key = ptr::from_ref(enc).addr();
    let mut map = dxmt_util::sync::lock(&ENCODER_TRACES);
    map.remove(&key);
}

/// Immediate execution mode for [`Context<Immediate>`].
#[derive(Default)]
pub struct Immediate {
    pub has_dirty_op: bool,
    pub ignore_no_wait: bool,
    pub cmdbuf_seq: u64,
    pub coherent_seq: Arc<AtomicU64>,
    pub visibility_offset: u64,
    pub encoder: Option<Encoder>,
    pub commands: Option<Commands>,
    pub retained: Vec<Arc<dyn Send + Sync>>,
    pub staging_allocator: Option<RingBump>,
    pub active_stagings: Vec<(u64, Arc<Staging>)>,
    pub active_visibility_queries: Vec<(u64, u64, Arc<VisibilityQuery>)>,
    pub signaled_win32_events: Vec<(u64, usize)>,
    pub event_log: Vec<&'static str>,
    pub commit_count: u64,
    pub in_flight_callbacks: VecDeque<(u64, InFlightCallback)>,
    pub hw_protection_enabled: bool,
    pub multithread: crate::multithread::Multithread,
    pub local_mutex: crate::multithread::DeviceMutex,
}

impl Immediate {
    /// Returns the active command buffer sequence number, initializing chunk 1 if needed.
    pub fn current_seq(&mut self) -> u64 {
        if self.cmdbuf_seq == 0 {
            self.cmdbuf_seq = 1;
            let _ = self.encoder_mut();
        }
        self.cmdbuf_seq
    }

    /// Returns the active command buffer sequence number without mutating state.
    #[must_use]
    pub const fn current_seq_peek(&self) -> u64 {
        if self.cmdbuf_seq == 0 {
            1
        } else {
            self.cmdbuf_seq
        }
    }

    /// Returns the last committed command buffer sequence number.
    #[must_use]
    pub const fn last_committed_seq(&self) -> u64 {
        if self.cmdbuf_seq <= 1 {
            self.commit_count
        } else {
            self.cmdbuf_seq - 1
        }
    }

    /// Returns the highest GPU-completed sequence number.
    #[must_use]
    pub fn coherent_seq(&self) -> u64 {
        self.coherent_seq.load(Ordering::Acquire)
    }

    /// Returns a mutable reference to the active [`Encoder`], initializing it on first use.
    pub(super) fn encoder_mut(&mut self) -> &mut Encoder {
        let seq = if self.cmdbuf_seq == 0 {
            self.cmdbuf_seq = 1;
            1
        } else {
            self.cmdbuf_seq
        };
        self.encoder.get_or_insert_with(|| {
            // SAFETY: Non-zero sentinel handle wrapped in `ManuallyDrop` so `Obj_release` never runs.
            let fake_dev = ManuallyDrop::new(unsafe { Obj::adopt(1) }.expect("non-zero"));
            let heap = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
            let mut enc = Encoder::new(&fake_dev, heap);
            enc.begin_chunk(seq);
            enc
        })
    }

    /// Returns a mutable reference to the arena [`Commands`] buffer.
    pub(super) fn commands_mut(&mut self) -> &mut Commands {
        let block = usize::try_from(DEFERRED_BLOCK).unwrap_or(0x10_0000);
        self.commands
            .get_or_insert_with(|| Commands::new(Arc::new(BlockPool::new(block))))
    }
}

#[cfg(test)]
impl Drop for Immediate {
    fn drop(&mut self) {
        if let Some(enc) = self.encoder.as_ref() {
            clear_encoder_trace(enc);
        }
    }
}

/// Resolves `&mut Device` when `device` points to a live Metal-backed [`crate::device::Device`].
pub(super) fn live_device(device: *mut c_void) -> Option<&'static mut crate::device::Device> {
    let dev = crate::device::Device::from_iface(device)?;
    if dev.core().device().handle() > 0x1000 {
        // SAFETY: `from_iface` verified `device` is a live `Device`.
        Some(unsafe { &mut *device.cast::<crate::device::Device>() })
    } else {
        None
    }
}

/// Resolves `&resource::Common` from a resource pointer.
///
/// Uses `resource::common` when linked with the resource table, and falls back to casting
/// `resource` as `*const resource::Common` in standalone unit tests.
pub(super) fn resource_common<'a>(resource: *mut c_void) -> Option<&'a resource::Common> {
    if resource.is_null() {
        return None;
    }
    if let Some(common) = resource::common(resource) {
        return Some(common);
    }
    // SAFETY: Standalone context tests pass a valid pointer to `resource::Common`.
    Some(unsafe { &*resource.cast::<resource::Common>() })
}

impl Mode for Immediate {
    fn emit_st(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        if let Some(dev) = live_device(ctx.device) {
            dev.core_mut().queue_mut().chunk().emit(f);
            return;
        }
        let _ = ctx.mode.current_seq();
        f(ctx.mode.encoder_mut());
    }

    fn emit_op(ctx: &mut Context<Self>, f: impl Fn(&mut Encoder) + Send + 'static) {
        if let Some(dev) = live_device(ctx.device) {
            ctx.mode.has_dirty_op = true;
            dev.core_mut().queue_mut().chunk().emit(f);
            return;
        }
        let _ = ctx.mode.current_seq();
        ctx.mode.has_dirty_op = true;
        f(ctx.mode.encoder_mut());
    }

    fn retain<T: Send + 'static>(ctx: &mut Context<Self>, t: T) {
        if let Some(dev) = live_device(ctx.device) {
            dev.core_mut().queue_mut().chunk().retain(t);
            return;
        }
        ctx.mode.retained.push(Arc::new(Mutex::new(Some(t))));
    }

    fn alloc(ctx: &mut Context<Self>, size: usize, align: usize) -> (*mut u8, u64) {
        let len = size.max(1);
        let alignment = align.max(1);
        if let Some(dev) = live_device(ctx.device) {
            let ptr = dev
                .core_mut()
                .queue_mut()
                .chunk()
                .allocate_cpu(len, alignment)
                .as_ptr();
            return (ptr, ptr as usize as u64);
        }
        if let Ok(layout) = core::alloc::Layout::from_size_align(len, alignment) {
            let ptr = ctx.mode.commands_mut().allocate(layout).as_ptr();
            (ptr, ptr as usize as u64)
        } else {
            (ptr::null_mut(), 0)
        }
    }

    fn staging(ctx: &mut Context<Self>, size: u64, align: u64) -> (u64, u64) {
        if let Some(dev) = live_device(ctx.device) {
            let (block, offset) = dev
                .core_mut()
                .queue_mut()
                .allocate_staging_buffer(size.max(1), align.max(1));
            return (block.buffer, offset);
        }
        let seq = ctx.mode.current_seq();
        let coherent = ctx.mode.coherent_seq();
        if let Some(ring) = ctx.mode.staging_allocator.as_mut() {
            let (block, offset) = ring.allocate(seq, coherent, size.max(1), align.max(1));
            return (block.gpu_address + offset, offset);
        }
        let (ptr, gpu_addr) = Self::alloc(
            ctx,
            usize::try_from(size.max(1)).unwrap_or(1),
            usize::try_from(align.max(1)).unwrap_or(1),
        );
        let _ = ptr;
        (gpu_addr, 0)
    }

    fn dynamic_name(ctx: &mut Context<Self>, dynamic: &DynamicBuffer) -> Arc<BufferAllocation> {
        let _ = ctx;
        dynamic.immediate_name()
    }

    fn lock_iface(this: *mut c_void) -> Option<crate::multithread::DeviceLockGuard<'static>> {
        if this.is_null() {
            return None;
        }
        // SAFETY: `this` points to a live `Context<Immediate>`; `multithread.mutex` is initialized once at creation.
        let mutex_ptr = unsafe { (*this.cast::<Context<Self>>()).mode.multithread.mutex };
        if mutex_ptr.is_null() {
            return None;
        }
        // SAFETY: `mutex_ptr` points to the owning device's `DeviceMutex` (or `local_mutex`), valid for the call.
        Some(unsafe { (*mutex_ptr).lock_guard() })
    }
}

const fn write_so_target(targets: *mut *mut c_void, index: usize) {
    // SAFETY: Caller provides an array of at least `index + 1` target slots.
    let slot = unsafe { targets.add(index) };
    // SAFETY: `slot` is within the caller's `targets` array.
    unsafe {
        slot.write_unaligned(ptr::null_mut());
    }
}

const fn write_so_offset(offsets: *mut u32, index: usize) {
    // SAFETY: Caller provides an array of at least `index + 1` offset slots.
    let slot = unsafe { offsets.add(index) };
    // SAFETY: `slot` is within the caller's `offsets` array.
    unsafe {
        slot.write_unaligned(0);
    }
}

/// The results of a visibility query that retired with nothing counted, up to any real length.
const RETIRED_QUERY_ZEROS: [u64; 256] = [0; 256];

impl Context<Immediate> {
    /// Allocates and initializes a new immediate context (`MTLD3D11ImmediateContext`).
    #[must_use]
    pub fn new_immediate(device: *mut c_void) -> Box<Self> {
        let refs = Managed::new();
        refs.add_ref_private();
        let mut ctx = Box::new(Self {
            vtbl: &raw const IMMEDIATE_VTBL,
            ext_vtbl: &raw const IMMEDIATE_EXT_VTBL,
            annotation_vtbl: &raw const IMMEDIATE_ANNOTATION_VTBL,
            refs,
            private_data: PrivateData::default(),
            device,
            state: super::state::ContextState::default(),
            cmdbuf_state: CommandBufferState::Idle,
            dirty: DirtyFlags::all(),
            defaults: Defaults::new(device),
            mode: Immediate::default(),
        });
        ctx.mode.ignore_no_wait =
            crate::device::CONFIG.get_bool("d3d11.ignoreMapFlagNoWait", false);
        if live_device(device).is_none() {
            let _ = ctx.mode.current_seq();
        }
        let outer = crate::com::iface_of(&ctx.vtbl);
        let mutex_ptr = crate::device::Device::from_iface(device)
            .map_or(&raw const ctx.mode.local_mutex, |dev| &raw const dev.mutex);
        ctx.mode.multithread.init(outer, mutex_ptr);
        ctx
    }

    /// Active command queue sequence number (`CurrentSeqId`).
    pub fn active_seq(&mut self) -> u64 {
        if let Some(dev) = live_device(self.device) {
            dev.core().queue().current_seq()
        } else {
            self.mode.current_seq()
        }
    }

    /// Highest GPU-completed command queue sequence number (`CoherentSeqId`).
    #[must_use]
    pub fn active_coherent_seq(&self) -> u64 {
        live_device(self.device).map_or_else(
            || self.mode.coherent_seq(),
            |dev| dev.core().queue().coherent_seq(),
        )
    }

    /// Commits the current command buffer chunk and optionally signals `win32_event` afterwards.
    pub fn commit_internal(&mut self, win32_event: Option<*mut c_void>) {
        self.finish_pass(false);
        if let Some(dev) = live_device(self.device) {
            if self.mode.has_dirty_op {
                dev.core_mut().queue_mut().commit();
                self.mode.has_dirty_op = false;
                self.mode.commit_count += 1;
                self.dirty = DirtyFlags::all();
            }
            if let Some(ev) = win32_event
                && !ev.is_null()
            {
                let q = dev.core().queue();
                // SAFETY: `q.shared_event()` and `q.shared_event_listener()` are live Metal handles.
                unsafe {
                    MTLSharedEvent_setWin32EventAtValue(
                        q.shared_event(),
                        q.shared_event_listener(),
                        ev,
                        q.current_event_seq(),
                    );
                }
            }
            return;
        }
        if !self.mode.has_dirty_op && win32_event.is_none() {
            return;
        }
        if crate::device::Device::from_iface(self.device).is_some() {
            // SAFETY: `from_iface` verified `self.device` is a live `Device`.
            let dev_mut = unsafe { &mut *self.device.cast::<crate::device::Device>() };
            dev_mut.core_mut().queue_mut().commit();
        }
        let committed_seq = self.mode.current_seq();
        self.mode.encoder_mut().signal_queue_event(committed_seq);
        let mut chunk_retained = Vec::new();
        self.mode.encoder_mut().end_chunk(&mut chunk_retained);
        self.mode.retained.append(&mut chunk_retained);
        if let Some(cmds) = self.mode.commands.as_mut() {
            cmds.reset();
        }
        self.mode.has_dirty_op = false;
        self.mode.commit_count += 1;
        self.mode.event_log.push("commit");
        self.dirty = DirtyFlags::all();

        let next_seq = committed_seq + 1;
        self.mode.cmdbuf_seq = next_seq;
        self.mode.visibility_offset = 0;
        self.mode.encoder = None;
        let _ = self.mode.encoder_mut();

        if let Some(ev) = win32_event {
            self.mode.event_log.push("set_event");
            self.mode
                .signaled_win32_events
                .push((committed_seq, ev as usize));
        }
    }

    /// Waits for `target_seq` to finish on the GPU, retiring completed resources and callbacks.
    pub fn wait_for_seq(&mut self, target_seq: u64) {
        if target_seq == 0 {
            return;
        }
        if let Some(dev) = live_device(self.device) {
            let current = dev.core().queue().current_seq();
            if target_seq >= current
                && (self.mode.has_dirty_op || self.cmdbuf_state != CommandBufferState::Idle)
            {
                self.commit_internal(None);
            }
            dev.core().queue().wait_cpu(target_seq);
            return;
        }
        let current = self.mode.current_seq();
        if target_seq >= current
            && (self.mode.has_dirty_op || self.cmdbuf_state != CommandBufferState::Idle)
        {
            self.commit_internal(None);
        }
        self.mode
            .coherent_seq
            .fetch_max(target_seq, Ordering::Release);

        if let Some(ring) = self.mode.staging_allocator.as_mut() {
            ring.free_blocks(target_seq);
        }

        let mut i = 0;
        while i < self.mode.active_visibility_queries.len() {
            if self.mode.active_visibility_queries[i].0 <= target_seq {
                let (seq, count, vq) = self.mode.active_visibility_queries.swap_remove(i);
                let len = usize::try_from(count).unwrap_or(0).max(1);
                // A retired query adds nothing: zeros of its length, from a constant unless it
                // is longer than any real one.
                if let Some(zeros) = RETIRED_QUERY_ZEROS.get(..len) {
                    vq.issue(seq, zeros);
                } else {
                    vq.issue(seq, &vec![0u64; len]);
                }
            } else {
                i += 1;
            }
        }

        self.mode
            .active_stagings
            .retain(|(seq, _)| *seq > target_seq);

        while let Some((seq, _)) = self.mode.in_flight_callbacks.front() {
            if *seq <= target_seq {
                if let Some((_, cb)) = self.mode.in_flight_callbacks.pop_front() {
                    cb(target_seq);
                }
            } else {
                break;
            }
        }
    }

    /// Registers a completion callback to run once the GPU completes `seq`.
    pub fn register_in_flight(&mut self, seq: u64, cb: impl FnOnce(u64) + Send + 'static) {
        if self.mode.coherent_seq() >= seq {
            cb(self.mode.coherent_seq());
        } else {
            self.mode.in_flight_callbacks.push_back((seq, Box::new(cb)));
        }
    }
}

impl<M: Mode> Context<M> {
    /// Whether this context is backed by a live Metal device.
    #[must_use]
    pub(super) fn is_live(&self) -> bool {
        live_device(self.device).is_some()
    }

    /// Downcasts `&self` to `&Context<Immediate>` when `M` is [`Immediate`].
    #[must_use]
    pub fn as_immediate(&self) -> Option<&Context<Immediate>> {
        if TypeId::of::<M>() == TypeId::of::<Immediate>() {
            // SAFETY: `TypeId` equality guarantees `Self` is `Context<Immediate>`.
            Some(unsafe { &*(ptr::from_ref(self).cast::<Context<Immediate>>()) })
        } else {
            None
        }
    }

    /// Downcasts `&mut self` to `&mut Context<Immediate>` when `M` is [`Immediate`].
    #[must_use]
    pub fn as_immediate_mut(&mut self) -> Option<&mut Context<Immediate>> {
        if TypeId::of::<M>() == TypeId::of::<Immediate>() {
            // SAFETY: `TypeId` equality guarantees `Self` is `Context<Immediate>`.
            Some(unsafe { &mut *(ptr::from_mut(self).cast::<Context<Immediate>>()) })
        } else {
            None
        }
    }

    /// Starts a render pass on the context and records the pass transition in the encoder.
    pub fn begin_render_pass(&mut self) {
        if self.cmdbuf_state == CommandBufferState::RenderEncoderActive {
            return;
        }
        self.finish_pass(false);
        self.cmdbuf_state = CommandBufferState::RenderEncoderActive;
        M::emit_st(self, |enc| {
            enc.start_render_pass(0, 0, 0, None);
            #[cfg(test)]
            push_encoder_trace(enc, "RenderPass:start");
        });
    }

    /// Starts a compute pass on the context and records the pass transition in the encoder.
    pub fn begin_compute_pass(&mut self) {
        if self.cmdbuf_state == CommandBufferState::ComputeEncoderActive {
            return;
        }
        self.finish_pass(false);
        self.cmdbuf_state = CommandBufferState::ComputeEncoderActive;
        M::emit_st(self, |enc| {
            enc.start_compute_pass(None);
            #[cfg(test)]
            push_encoder_trace(enc, "ComputePass:start");
        });
    }

    /// Ends any active encoder pass and transitions `cmdbuf_state` to `Idle`.
    pub fn finish_pass(&mut self, promote_flush: bool) {
        let was_active = matches!(
            self.cmdbuf_state,
            CommandBufferState::RenderEncoderActive
                | CommandBufferState::ComputeEncoderActive
                | CommandBufferState::BlitEncoderActive
        );
        self.invalidate_current_pass(promote_flush);
        if was_active {
            M::emit_st(self, |enc| {
                if enc.pass().is_some() {
                    enc.end_pass();
                }
                #[cfg(test)]
                push_encoder_trace(enc, "Pass:end");
            });
        }
    }

    /// `IMTLD3D11DeviceContext::WaitUntilGPUIdle`.
    pub fn wait_until_gpu_idle(&mut self) {
        if let Some(imm) = self.as_immediate_mut() {
            if crate::device::Device::from_iface(imm.device).is_some() {
                // SAFETY: `from_iface` verified `imm.device` is a live `Device`.
                let dev = unsafe { &mut *imm.device.cast::<crate::device::Device>() };
                let seq = dev.core().queue().current_seq();
                imm.finish_pass(false);
                dev.core_mut().queue_mut().commit();
                imm.mode.has_dirty_op = false;
                dev.core().queue().wait_cpu(seq);
                if dev.core().device().handle() > 0x1000 {
                    return;
                }
            }
            if imm.mode.has_dirty_op || imm.cmdbuf_state != CommandBufferState::Idle {
                imm.commit_internal(None);
            }
            let target = imm.mode.last_committed_seq();
            imm.wait_for_seq(target);
        }
    }

    /// `IMTLD3D11DeviceContext::PrepareFlush`.
    pub fn prepare_flush(&mut self) {
        if let Some(imm) = self.as_immediate_mut() {
            imm.finish_pass(true);
            imm.mode.has_dirty_op = true;
        }
    }

    /// `IMTLD3D11DeviceContext::Commit`.
    pub fn commit(&mut self) {
        if let Some(imm) = self.as_immediate_mut() {
            imm.commit_internal(None);
        }
    }

    /// `IMTLD3D11DeviceContext::SOGetTargetsWithOffsets`.
    pub fn so_get_targets_with_offsets(
        &self,
        num_buffers: u32,
        targets: *mut *mut c_void,
        offsets: *mut u32,
    ) {
        let count = (num_buffers as usize).min(4);
        for i in 0..count {
            if !targets.is_null() {
                write_so_target(targets, i);
            }
            if !offsets.is_null() {
                write_so_offset(offsets, i);
            }
        }
    }

    /// `IMTLD3D11DeviceContext::AcquireSync`.
    pub fn acquire_sync(&mut self, sync_resource: *mut c_void, key: u64) {
        let Some(common) = resource_common(sync_resource) else {
            return;
        };
        let Some(km) = common.keyed_mutex.clone() else {
            return;
        };
        let raw_ev = km.shared_event().max(1);
        let live = self.is_live();
        let _ = km.acquire(
            |val| {
                self.finish_pass(true);
                M::emit_op(self, move |enc| {
                    let ev = if raw_ev > 0x1000 {
                        // SAFETY: `raw_ev` is a live `MTLSharedEvent`.
                        unsafe { Obj::retain(raw_ev) }
                    } else {
                        // SAFETY: `raw_ev` is a non-zero test sentinel handle.
                        unsafe { Obj::adopt(raw_ev) }
                    };
                    if let Some(ev) = ev {
                        enc.wait_event(ev, val);
                    }
                    #[cfg(test)]
                    push_encoder_trace(enc, format!("WaitEvent({val})"));
                });
                if live {
                    self.flush();
                }
            },
            key,
            u32::MAX,
        );
    }

    /// `IMTLD3D11DeviceContext::ReleaseSync`.
    pub fn release_sync(&mut self, sync_resource: *mut c_void, key: u64) {
        let Some(common) = resource_common(sync_resource) else {
            return;
        };
        let Some(km) = common.keyed_mutex.clone() else {
            return;
        };
        let raw_ev = km.shared_event().max(1);
        let live = self.is_live();
        let _ = km.release(
            |val| {
                self.finish_pass(true);
                M::emit_op(self, move |enc| {
                    let ev = if raw_ev > 0x1000 {
                        // SAFETY: `raw_ev` is a live `MTLSharedEvent`.
                        unsafe { Obj::retain(raw_ev) }
                    } else {
                        // SAFETY: `raw_ev` is a non-zero test sentinel handle.
                        unsafe { Obj::adopt(raw_ev) }
                    };
                    if let Some(ev) = ev {
                        enc.signal_event(ev, val);
                    }
                    #[cfg(test)]
                    push_encoder_trace(enc, format!("SignalEvent({val})"));
                });
                if live {
                    self.flush();
                }
            },
            key,
        );
    }
}

#[cfg(test)]
mod tests;
