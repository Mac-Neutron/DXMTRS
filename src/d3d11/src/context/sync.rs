//! Context query dispatch, `Flush`, `Flush1`, `Signal`, `Wait` and context annotations.

use core::{ffi::c_void, ptr};
use std::sync::Arc;

use dxmt_core::query::{TimestampQuery, VisibilityQuery};
use dxmt_util::com::hresult::DXGI_ERROR_INVALID_CALL;
use winemetal_sys::Obj;

use super::{
    Context, Immediate, Mode, deferred::DeferredIssuedQuery, immediate::push_encoder_trace,
};
use crate::{
    abi::records::{D3d11QueryDataPipelineStatistics, D3d11QueryDataTimestampDisjoint},
    com::{E_INVALIDARG, Hresult, S_FALSE, S_OK},
    fence::Fence,
    query::Query,
};

/// `D3D11_DEVICE_CONTEXT_IMMEDIATE`.
pub const D3D11_DEVICE_CONTEXT_IMMEDIATE: u32 = 0;
/// `D3D11_DEVICE_CONTEXT_DEFERRED`.
pub const D3D11_DEVICE_CONTEXT_DEFERRED: u32 = 1;
/// `D3D11_ASYNC_GETDATA_DONOTFLUSH`.
pub const D3D11_ASYNC_GETDATA_DONOTFLUSH: u32 = 0x1;

/// `D3D11_QUERY_EVENT`.
pub const D3D11_QUERY_EVENT: u32 = 0;
/// `D3D11_QUERY_OCCLUSION`.
pub const D3D11_QUERY_OCCLUSION: u32 = 1;
/// `D3D11_QUERY_TIMESTAMP`.
pub const D3D11_QUERY_TIMESTAMP: u32 = 2;
/// `D3D11_QUERY_TIMESTAMP_DISJOINT`.
pub const D3D11_QUERY_TIMESTAMP_DISJOINT: u32 = 3;
/// `D3D11_QUERY_PIPELINE_STATISTICS`.
pub const D3D11_QUERY_PIPELINE_STATISTICS: u32 = 4;
/// `D3D11_QUERY_OCCLUSION_PREDICATE`.
pub const D3D11_QUERY_OCCLUSION_PREDICATE: u32 = 5;

const CONTEXT_QUERY_MAGIC: u64 = 0x4433_4431_3151_5259;

/// Lifecycle state of a [`ContextQuery`].
#[derive(Default, Debug, PartialEq, Eq)]
pub enum QueryState {
    #[default]
    Initial,
    Building,
    Issued,
    Signaled,
}

/// Self-contained query state used by context query dispatch and standalone context tests.
pub struct ContextQuery {
    magic: u64,
    pub query_type: u32,
    pub misc_flags: u32,
    pub state: QueryState,
    pub seq: u64,
    pub visibility: Option<Arc<VisibilityQuery>>,
    pub timestamp: Option<Arc<TimestampQuery>>,
}

impl ContextQuery {
    /// Creates a new [`ContextQuery`] for `query_type` and `misc_flags`.
    #[must_use]
    pub const fn new(query_type: u32, misc_flags: u32) -> Self {
        Self {
            magic: CONTEXT_QUERY_MAGIC,
            query_type,
            misc_flags,
            state: QueryState::Initial,
            seq: 0,
            visibility: None,
            timestamp: None,
        }
    }

    /// Returns a raw `*mut c_void` suitable for passing to `Begin`, `End` and `GetData`.
    #[must_use]
    pub const fn as_async_ptr(&mut self) -> *mut c_void {
        ptr::from_mut(self).cast::<c_void>()
    }

    /// Recovers `&mut ContextQuery` if `ptr` points to a live [`ContextQuery`].
    pub(super) fn from_ptr(ptr: *mut c_void) -> Option<&'static mut Self> {
        if ptr.is_null() || !(ptr as usize).is_multiple_of(align_of::<Self>()) {
            return None;
        }
        // SAFETY: `ptr` is non-null and aligned; magic field sits at offset 0.
        let magic = unsafe { ptr.cast::<u64>().read() };
        if magic != CONTEXT_QUERY_MAGIC {
            return None;
        }
        // SAFETY: Magic header verified `ptr` points to a `ContextQuery`.
        let cq = unsafe { &mut *ptr.cast::<Self>() };
        if cq.magic != CONTEXT_QUERY_MAGIC {
            return None;
        }
        Some(cq)
    }

    /// Expected output byte size for `GetData`.
    #[must_use]
    pub fn data_size(&self) -> u32 {
        match self.query_type {
            D3D11_QUERY_EVENT | D3D11_QUERY_OCCLUSION_PREDICATE => 4,
            D3D11_QUERY_OCCLUSION | D3D11_QUERY_TIMESTAMP => 8,
            D3D11_QUERY_TIMESTAMP_DISJOINT => {
                u32::try_from(size_of::<D3d11QueryDataTimestampDisjoint>()).unwrap_or(0)
            }
            D3D11_QUERY_PIPELINE_STATISTICS => {
                u32::try_from(size_of::<D3d11QueryDataPipelineStatistics>()).unwrap_or(0)
            }
            _ => 0,
        }
    }
}

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::Begin`.
    pub fn begin(&mut self, r#async: *mut c_void) {
        if r#async.is_null() {
            return;
        }
        if let Some(def) = self.as_deferred_mut()
            && !def.mode.open_queries.contains(&r#async)
        {
            def.mode.open_queries.push(r#async);
        }
        if let Some(q) = Query::from_iface(r#async) {
            let prev_state = q.state();
            q.begin_query();
            if matches!(
                q.kind(),
                crate::query::Kind::Occlusion { predicate: false }
                    | crate::query::Kind::OcclusionPredicate
            ) && prev_state != crate::query::State::Building
            {
                if let Some(imm) = self.as_immediate_mut() {
                    if let Some(vq) = q.visibility().cloned() {
                        if imm.is_live() {
                            Immediate::emit_st(imm, move |enc| {
                                enc.begin_visibility_query(Arc::clone(&vq));
                            });
                        } else {
                            let seq = imm.mode.current_seq();
                            let offset = imm.mode.visibility_offset;
                            vq.begin(seq, offset);
                            imm.mode
                                .encoder_mut()
                                .begin_visibility_query(Arc::clone(&vq));
                        }
                    }
                } else if let Some(def) = self.as_deferred_mut() {
                    let query_id = u32::try_from(def.mode.visibility_queries.len()).unwrap_or(0);
                    let vq = Arc::new(VisibilityQuery::new());
                    def.mode.visibility_queries.push(vq);
                    def.mode
                        .active_visibility_offsets
                        .insert(r#async as usize, query_id);
                    def.mode.commands_mut().emit_fn(move |enc| {
                        if let Some(active) = enc.current_deferred_visibility_query(query_id) {
                            enc.begin_visibility_query(active);
                        }
                    });
                }
            }
            return;
        }
        let Some(cq) = ContextQuery::from_ptr(r#async) else {
            return;
        };
        if let Some(imm) = self.as_immediate_mut() {
            match cq.query_type {
                D3D11_QUERY_OCCLUSION | D3D11_QUERY_OCCLUSION_PREDICATE => {
                    let seq = imm.mode.current_seq();
                    let offset = imm.mode.visibility_offset;
                    let vq = Arc::new(VisibilityQuery::new());
                    vq.begin(seq, offset);
                    imm.mode
                        .encoder_mut()
                        .begin_visibility_query(Arc::clone(&vq));
                    cq.visibility = Some(vq);
                    cq.state = QueryState::Building;
                }
                D3D11_QUERY_EVENT | D3D11_QUERY_TIMESTAMP => {}
                _ => {
                    cq.state = QueryState::Building;
                }
            }
        } else if let Some(def) = self.as_deferred_mut() {
            cq.state = QueryState::Building;
            if matches!(
                cq.query_type,
                D3D11_QUERY_OCCLUSION | D3D11_QUERY_OCCLUSION_PREDICATE
            ) {
                let query_id = u32::try_from(def.mode.visibility_queries.len()).unwrap_or(0);
                let vq = Arc::new(VisibilityQuery::new());
                def.mode.visibility_queries.push(vq);
                def.mode
                    .active_visibility_offsets
                    .insert(r#async as usize, query_id);
                def.mode.commands_mut().emit_fn(move |enc| {
                    if let Some(active) = enc.current_deferred_visibility_query(query_id) {
                        enc.begin_visibility_query(active);
                    }
                });
            }
        }
    }

    /// `ID3D11DeviceContext::End`.
    pub fn end(&mut self, r#async: *mut c_void) {
        if r#async.is_null() {
            return;
        }
        if let Some(def) = self.as_deferred_mut() {
            def.mode.open_queries.retain(|&p| p != r#async);
        }
        if let Some(q) = Query::from_iface(r#async) {
            if let Some(imm) = self.as_immediate_mut() {
                match q.kind() {
                    crate::query::Kind::Event | crate::query::Kind::TimestampDisjoint => {
                        if let Some(dev) = super::immediate::live_device(imm.device) {
                            if imm.mode.has_dirty_op || imm.has_open_pass() {
                                let event_seq = dev.core_mut().queue_mut().next_event_seq();
                                q.end_at(event_seq);
                                imm.finish_pass(true);
                                Immediate::emit_op(imm, move |enc| {
                                    enc.signal_queue_event(event_seq);
                                });
                            } else {
                                let event_seq = dev.core().queue().current_event_seq();
                                q.end_at(event_seq);
                            }
                        } else {
                            let seq = imm.mode.current_seq();
                            imm.mode.has_dirty_op = true;
                            q.end_at(seq);
                        }
                    }
                    crate::query::Kind::Timestamp => {
                        q.end_at(0);
                        if let Some(tq) = q.timestamp().cloned() {
                            if imm.is_live() {
                                imm.finish_pass(true);
                                Immediate::emit_op(imm, move |enc| {
                                    enc.sample_timestamp_query(Arc::clone(&tq));
                                });
                            } else {
                                imm.mode.has_dirty_op = true;
                                imm.mode
                                    .encoder_mut()
                                    .sample_timestamp_query(Arc::clone(&tq));
                            }
                        }
                    }
                    crate::query::Kind::Occlusion { predicate: false }
                    | crate::query::Kind::OcclusionPredicate => {
                        let was_building = q.state() == crate::query::State::Building;
                        q.end_at(0);
                        if was_building && let Some(vq) = q.visibility().cloned() {
                            if imm.is_live() {
                                imm.mode.has_dirty_op = true;
                                Immediate::emit_st(imm, move |enc| {
                                    enc.end_visibility_query(Arc::clone(&vq));
                                });
                            } else {
                                let seq = imm.mode.current_seq();
                                let offset = imm.mode.visibility_offset;
                                vq.end(seq, offset);
                                imm.mode.encoder_mut().end_visibility_query(Arc::clone(&vq));
                                imm.mode.visibility_offset += 1;
                                imm.mode.active_visibility_queries.push((
                                    seq,
                                    imm.mode.visibility_offset,
                                    Arc::clone(&vq),
                                ));
                                imm.mode.has_dirty_op = true;
                            }
                        }
                    }
                    _ => {
                        q.end_at(0);
                    }
                }
            } else if let Some(def) = self.as_deferred_mut() {
                q.end_at(0);
                def.mode.has_promoted_flush = true;
                match q.kind() {
                    crate::query::Kind::Occlusion { predicate: false }
                    | crate::query::Kind::OcclusionPredicate => {
                        if let Some(query_id) = def
                            .mode
                            .active_visibility_offsets
                            .remove(&(r#async as usize))
                        {
                            let vq = Arc::clone(&def.mode.visibility_queries[query_id as usize]);
                            def.mode.commands_mut().emit_fn(move |enc| {
                                if let Some(active) =
                                    enc.current_deferred_visibility_query(query_id)
                                {
                                    enc.end_visibility_query(active);
                                }
                            });
                            def.mode
                                .reissued_queries
                                .push(DeferredIssuedQuery::Visibility {
                                    query_ptr: r#async as usize,
                                    vq,
                                });
                        }
                    }
                    crate::query::Kind::Timestamp => {
                        if let Some(tq) = q.timestamp().cloned() {
                            def.mode
                                .reissued_queries
                                .push(DeferredIssuedQuery::Timestamp {
                                    query_ptr: r#async as usize,
                                    tq,
                                });
                        }
                    }
                    _ => {
                        def.mode.reissued_queries.push(DeferredIssuedQuery::Event {
                            query_ptr: r#async as usize,
                        });
                    }
                }
            }
            return;
        }
        let Some(cq) = ContextQuery::from_ptr(r#async) else {
            if let Some(def) = self.as_deferred_mut() {
                def.mode.reissued_queries.push(DeferredIssuedQuery::Event {
                    query_ptr: r#async as usize,
                });
            }
            return;
        };
        if let Some(imm) = self.as_immediate_mut() {
            let seq = imm.mode.current_seq();
            imm.mode.has_dirty_op = true;
            match cq.query_type {
                D3D11_QUERY_OCCLUSION | D3D11_QUERY_OCCLUSION_PREDICATE => {
                    if cq.state != QueryState::Building || cq.visibility.is_none() {
                        let offset = imm.mode.visibility_offset;
                        let vq = Arc::new(VisibilityQuery::new());
                        vq.begin(seq, offset);
                        imm.mode
                            .encoder_mut()
                            .begin_visibility_query(Arc::clone(&vq));
                        cq.visibility = Some(vq);
                    }
                    if let Some(vq) = cq.visibility.as_ref() {
                        let offset = imm.mode.visibility_offset;
                        vq.end(seq, offset);
                        imm.mode.encoder_mut().end_visibility_query(Arc::clone(vq));
                        imm.mode.visibility_offset += 1;
                        imm.mode.active_visibility_queries.push((
                            seq,
                            imm.mode.visibility_offset,
                            Arc::clone(vq),
                        ));
                    }
                    cq.seq = seq;
                    cq.state = QueryState::Issued;
                }
                D3D11_QUERY_TIMESTAMP => {
                    let tq = Arc::new(TimestampQuery::new());
                    imm.mode
                        .encoder_mut()
                        .sample_timestamp_query(Arc::clone(&tq));
                    cq.timestamp = Some(tq);
                    cq.seq = seq;
                    cq.state = QueryState::Issued;
                }
                _ => {
                    cq.seq = seq;
                    cq.state = QueryState::Issued;
                }
            }
        } else if let Some(def) = self.as_deferred_mut() {
            match cq.query_type {
                D3D11_QUERY_OCCLUSION | D3D11_QUERY_OCCLUSION_PREDICATE => {
                    let query_id = if let Some(id) = def
                        .mode
                        .active_visibility_offsets
                        .remove(&(r#async as usize))
                    {
                        id
                    } else {
                        let id = u32::try_from(def.mode.visibility_queries.len()).unwrap_or(0);
                        let vq = Arc::new(VisibilityQuery::new());
                        def.mode.visibility_queries.push(vq);
                        def.mode.commands_mut().emit_fn(move |enc| {
                            if let Some(active) = enc.current_deferred_visibility_query(id) {
                                enc.begin_visibility_query(active);
                            }
                        });
                        id
                    };
                    let vq = Arc::clone(&def.mode.visibility_queries[query_id as usize]);
                    def.mode.commands_mut().emit_fn(move |enc| {
                        if let Some(active) = enc.current_deferred_visibility_query(query_id) {
                            enc.end_visibility_query(active);
                        }
                    });
                    def.mode
                        .reissued_queries
                        .push(DeferredIssuedQuery::Visibility {
                            query_ptr: r#async as usize,
                            vq,
                        });
                    cq.state = QueryState::Issued;
                }
                D3D11_QUERY_TIMESTAMP => {
                    let tq = Arc::new(TimestampQuery::new());
                    let tq_cmd = Arc::clone(&tq);
                    def.mode.commands_mut().emit_fn(move |enc| {
                        enc.sample_timestamp_query(Arc::clone(&tq_cmd));
                    });
                    def.mode
                        .reissued_queries
                        .push(DeferredIssuedQuery::Timestamp {
                            query_ptr: r#async as usize,
                            tq,
                        });
                    cq.state = QueryState::Issued;
                }
                _ => {
                    def.mode.reissued_queries.push(DeferredIssuedQuery::Event {
                        query_ptr: r#async as usize,
                    });
                    cq.state = QueryState::Issued;
                }
            }
        }
    }

    /// `ID3D11DeviceContext::GetData`.
    pub fn get_data(
        &mut self,
        r#async: *mut c_void,
        data: *mut c_void,
        data_size: u32,
        get_data_flags: u32,
    ) -> Hresult {
        if r#async.is_null() || self.as_deferred().is_some() {
            return E_INVALIDARG;
        }
        if let Some(q) = Query::from_iface(r#async) {
            let coherent_seq = self.as_immediate().map_or(0, |imm| {
                super::immediate::live_device(imm.device).map_or_else(
                    || imm.mode.coherent_seq(),
                    |dev| dev.core().queue().signaled_event_seq(),
                )
            });
            let hr = q.data_at(coherent_seq, data, data_size);
            if hr == S_FALSE
                && ((get_data_flags & D3D11_ASYNC_GETDATA_DONOTFLUSH) == 0 || q.is_stalled())
            {
                self.flush();
            }
            return hr;
        }
        let Some(cq) = ContextQuery::from_ptr(r#async) else {
            return E_INVALIDARG;
        };
        let Some(imm) = self.as_immediate_mut() else {
            return E_INVALIDARG;
        };
        if !matches!(cq.state, QueryState::Issued | QueryState::Signaled) {
            return DXGI_ERROR_INVALID_CALL;
        }
        let expected = cq.data_size();
        if data_size != 0 && (data.is_null() || data_size < expected) {
            return E_INVALIDARG;
        }
        if imm.mode.coherent_seq() < cq.seq {
            if (get_data_flags & D3D11_ASYNC_GETDATA_DONOTFLUSH) == 0 {
                imm.commit_internal(None);
            }
            return S_FALSE;
        }
        cq.state = QueryState::Signaled;
        if !data.is_null() && data_size >= expected {
            match cq.query_type {
                D3D11_QUERY_EVENT => {
                    // SAFETY: `data` was checked for null and `data_size >= 4`.
                    unsafe {
                        data.cast::<i32>().write_unaligned(1);
                    }
                }
                D3D11_QUERY_OCCLUSION_PREDICATE => {
                    let count = cq
                        .visibility
                        .as_ref()
                        .and_then(|vq| vq.value())
                        .unwrap_or(0);
                    let passed = i32::from(count != 0);
                    // SAFETY: `data` was checked for null and `data_size >= 4`.
                    unsafe {
                        data.cast::<i32>().write_unaligned(passed);
                    }
                }
                D3D11_QUERY_OCCLUSION => {
                    let count = cq
                        .visibility
                        .as_ref()
                        .and_then(|vq| vq.value())
                        .unwrap_or(0);
                    // SAFETY: `data` was checked for null and `data_size >= 8`.
                    unsafe {
                        data.cast::<u64>().write_unaligned(count);
                    }
                }
                D3D11_QUERY_TIMESTAMP => {
                    let ts = cq.timestamp.as_ref().and_then(|tq| tq.value()).unwrap_or(0);
                    // SAFETY: `data` was checked for null and `data_size >= 8`.
                    unsafe {
                        data.cast::<u64>().write_unaligned(ts);
                    }
                }
                D3D11_QUERY_TIMESTAMP_DISJOINT => {
                    let out = D3d11QueryDataTimestampDisjoint {
                        frequency: 1_000_000_000,
                        disjoint: 0,
                    };
                    // SAFETY: `data` was checked for null and `data_size >= expected`.
                    unsafe {
                        data.cast::<D3d11QueryDataTimestampDisjoint>()
                            .write_unaligned(out);
                    }
                }
                D3D11_QUERY_PIPELINE_STATISTICS => {
                    let out = D3d11QueryDataPipelineStatistics {
                        ia_vertices: 0,
                        ia_primitives: 0,
                        vs_invocations: 0,
                        gs_invocations: 0,
                        gs_primitives: 0,
                        c_invocations: 0,
                        c_primitives: 0,
                        ps_invocations: 0,
                        hs_invocations: 0,
                        ds_invocations: 0,
                        cs_invocations: 0,
                    };
                    // SAFETY: `data` was checked for null and `data_size >= expected`.
                    unsafe {
                        data.cast::<D3d11QueryDataPipelineStatistics>()
                            .write_unaligned(out);
                    }
                }
                _ => {}
            }
        }
        S_OK
    }

    /// `ID3D11DeviceContext::Flush`.
    pub fn flush(&mut self) {
        if let Some(imm) = self.as_immediate_mut() {
            imm.prepare_flush();
            imm.commit_internal(None);
        }
    }

    /// `ID3D11DeviceContext1::Flush1`.
    pub fn flush1(&mut self, _context_type: u32, event: *mut c_void) {
        if let Some(imm) = self.as_immediate_mut() {
            imm.prepare_flush();
            let ev = if event.is_null() { None } else { Some(event) };
            imm.commit_internal(ev);
        }
    }

    /// `ID3D11DeviceContext::GetType`.
    #[must_use]
    pub fn get_type(&self) -> u32 {
        if self.as_deferred().is_some() {
            D3D11_DEVICE_CONTEXT_DEFERRED
        } else {
            D3D11_DEVICE_CONTEXT_IMMEDIATE
        }
    }

    /// `ID3D11DeviceContext::GetContextFlags`.
    #[must_use]
    pub fn get_context_flags(&self) -> u32 {
        self.as_deferred().map_or(0, |def| def.mode.context_flags)
    }

    /// `ID3DUserDefinedAnnotation::GetStatus`.
    #[must_use]
    pub const fn is_annotation_enabled(&self) -> i32 {
        0
    }

    /// `ID3DUserDefinedAnnotation::SetMarker`.
    pub const fn set_marker_int(&mut self, label: *const c_void, data: i32) {
        let _ = (label, data);
    }

    /// `ID3DUserDefinedAnnotation::BeginEvent`.
    pub const fn begin_event_int(&mut self, label: *const c_void, data: i32) {
        let _ = (label, data);
    }

    /// `ID3DUserDefinedAnnotation::EndEvent`.
    pub const fn end_event(&mut self) {}

    /// `ID3D11DeviceContext1::SetHardwareProtectionState`.
    pub fn set_hardware_protection_state(&mut self, hw_protection_enable: i32) {
        let enabled = hw_protection_enable != 0;
        if let Some(imm) = self.as_immediate_mut() {
            imm.mode.hw_protection_enabled = enabled;
        } else if let Some(def) = self.as_deferred_mut() {
            def.mode.hw_protection_enabled = enabled;
        }
    }

    /// `ID3D11DeviceContext1::GetHardwareProtectionState`.
    pub fn get_hardware_protection_state(&self, hw_protection_enable: *mut i32) {
        if hw_protection_enable.is_null() {
            return;
        }
        let enabled = match (self.as_immediate(), self.as_deferred()) {
            (Some(imm), _) => imm.mode.hw_protection_enabled,
            (None, Some(def)) => def.mode.hw_protection_enabled,
            (None, None) => false,
        };
        write_hw_protection(hw_protection_enable, enabled);
    }

    /// `ID3D11DeviceContext4::Signal`.
    pub fn signal(&mut self, fence: *mut c_void, value: u64) -> Hresult {
        if fence.is_null() || self.as_deferred().is_some() {
            return E_INVALIDARG;
        }
        let (raw_ev, keep_ev) = retain_fence_event(fence);
        self.finish_pass(false);
        M::emit_op(self, move |enc| {
            let _keep = &keep_ev;
            let ev = if raw_ev > 0x1000 {
                // SAFETY: `raw_ev` is kept alive by `keep_ev`.
                unsafe { Obj::retain(raw_ev) }
            } else {
                // SAFETY: `raw_ev` is a non-zero test sentinel handle.
                unsafe { Obj::adopt(raw_ev) }
            };
            if let Some(ev) = ev {
                enc.signal_event(ev, value);
            }
            push_encoder_trace(enc, format!("SignalEvent({value})"));
        });
        if self.is_live() {
            self.flush();
        }
        S_OK
    }

    /// `ID3D11DeviceContext4::Wait`.
    pub fn wait(&mut self, fence: *mut c_void, value: u64) -> Hresult {
        if fence.is_null() || self.as_deferred().is_some() {
            return E_INVALIDARG;
        }
        let (raw_ev, keep_ev) = retain_fence_event(fence);
        if self.is_live() {
            self.flush();
        } else {
            self.finish_pass(false);
        }
        M::emit_op(self, move |enc| {
            let _keep = &keep_ev;
            let ev = if raw_ev > 0x1000 {
                // SAFETY: `raw_ev` is kept alive by `keep_ev`.
                unsafe { Obj::retain(raw_ev) }
            } else {
                // SAFETY: `raw_ev` is a non-zero test sentinel handle.
                unsafe { Obj::adopt(raw_ev) }
            };
            if let Some(ev) = ev {
                enc.wait_event(ev, value);
            }
            push_encoder_trace(enc, format!("WaitEvent({value})"));
        });
        S_OK
    }
}

fn retain_fence_event(fence: *mut c_void) -> (u64, Option<Obj>) {
    if let Some(f) = Fence::from_iface(fence) {
        let ev = f.shared_event();
        let raw = ev.as_ref().map_or(1, Obj::handle);
        return (raw, ev);
    }
    // SAFETY: Standalone tests pass a pointer to a raw `u64` event handle.
    let raw = unsafe { (*fence.cast::<u64>()).max(1) };
    let ev = if raw > 0x1000 {
        // SAFETY: `raw` is a live `MTLSharedEvent`.
        unsafe { Obj::retain(raw) }
    } else {
        None
    };
    (raw, ev)
}

fn write_hw_protection(out: *mut i32, enabled: bool) {
    // SAFETY: `out` was verified non-null by `get_hardware_protection_state`.
    unsafe {
        out.write_unaligned(i32::from(enabled));
    }
}

#[cfg(test)]
mod tests;
