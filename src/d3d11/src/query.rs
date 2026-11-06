//! D3D11 queries and predicates (`MTLD3DQueryBase`, `d3d11_query.hpp`, `d3d11_query.cpp`).

use core::{ffi::c_void, fmt, ptr};
use std::sync::{Arc, Mutex, MutexGuard};

use dxmt_core::query::{TimestampQuery, VisibilityQuery};
pub use dxmt_util::com::hresult::DXGI_ERROR_INVALID_CALL;
use dxmt_util::{
    com::{Qi, QiMiss, qi_table},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
    sync::lock,
};

use crate::{
    abi::{
        IID_ID3D10_DEVICE_CHILD, IID_ID3D10_QUERY, IID_ID3D11_DEVICE_CHILD, IID_ID3D11_QUERY,
        IID_ID3D11_QUERY_1,
        records::{
            D3d11QueryDataPipelineStatistics, D3d11QueryDataSoStatistics,
            D3d11QueryDataTimestampDisjoint, D3d11QueryDesc, D3d11QueryDesc1,
        },
        vtbl::{ID3D10QueryVtbl, ID3D11Query1Vtbl},
    },
    com::{
        ComObject, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, Hresult, IID_IUNKNOWN, S_FALSE, S_OK,
    },
    device::Device,
};

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
/// `D3D11_QUERY_SO_STATISTICS`.
pub const D3D11_QUERY_SO_STATISTICS: u32 = 6;
/// `D3D11_QUERY_SO_OVERFLOW_PREDICATE`.
pub const D3D11_QUERY_SO_OVERFLOW_PREDICATE: u32 = 7;
/// `D3D11_QUERY_SO_STATISTICS_STREAM0`.
pub const D3D11_QUERY_SO_STATISTICS_STREAM0: u32 = 8;
/// `D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM0`.
pub const D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM0: u32 = 9;
/// `D3D11_QUERY_SO_STATISTICS_STREAM1`.
pub const D3D11_QUERY_SO_STATISTICS_STREAM1: u32 = 10;
/// `D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM1`.
pub const D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM1: u32 = 11;
/// `D3D11_QUERY_SO_STATISTICS_STREAM2`.
pub const D3D11_QUERY_SO_STATISTICS_STREAM2: u32 = 12;
/// `D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM2`.
pub const D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM2: u32 = 13;
/// `D3D11_QUERY_SO_STATISTICS_STREAM3`.
pub const D3D11_QUERY_SO_STATISTICS_STREAM3: u32 = 14;
/// `D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM3`.
pub const D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM3: u32 = 15;

/// `D3D11_QUERY_MISC_PREDICATEHINT`.
pub const D3D11_QUERY_MISC_PREDICATEHINT: u32 = 0x1;

/// Stall threshold before `GetData` flushes (`kEventStallThreshold`).
pub const EVENT_STALL_THRESHOLD: u32 = 64;

/// Timestamp frequency in Hz (`1_000_000_000`).
pub const TIMESTAMP_FREQUENCY_HZ: u64 = 1_000_000_000;

/// Query state machine (`QueryState` in `d3d11_query.hpp`).
pub enum State {
    Building,
    Issued,
    Signaled,
}

impl Clone for State {
    fn clone(&self) -> Self {
        match self {
            Self::Building => Self::Building,
            Self::Issued => Self::Issued,
            Self::Signaled => Self::Signaled,
        }
    }
}

impl PartialEq for State {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Building, Self::Building)
                | (Self::Issued, Self::Issued)
                | (Self::Signaled, Self::Signaled)
        )
    }
}

impl Eq for State {}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Building => f.write_str("Building"),
            Self::Issued => f.write_str("Issued"),
            Self::Signaled => f.write_str("Signaled"),
        }
    }
}

/// Polling state of an event or timestamp-disjoint query (`EventState` in `d3d11_query.hpp`).
pub enum EventState {
    Pending,
    Signaled,
    Stall,
    Invalid,
}

impl Clone for EventState {
    fn clone(&self) -> Self {
        match self {
            Self::Pending => Self::Pending,
            Self::Signaled => Self::Signaled,
            Self::Stall => Self::Stall,
            Self::Invalid => Self::Invalid,
        }
    }
}

impl PartialEq for EventState {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (Self::Pending, Self::Pending)
                | (Self::Signaled, Self::Signaled)
                | (Self::Stall, Self::Stall)
                | (Self::Invalid, Self::Invalid)
        )
    }
}

impl Eq for EventState {}

impl fmt::Debug for EventState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => f.write_str("Pending"),
            Self::Signaled => f.write_str("Signaled"),
            Self::Stall => f.write_str("Stall"),
            Self::Invalid => f.write_str("Invalid"),
        }
    }
}

/// D3D11 query kind (`D3D11_QUERY`).
pub enum Kind {
    Event,
    Occlusion { predicate: bool },
    Timestamp,
    TimestampDisjoint,
    PipelineStatistics,
    SoStatistics { stream: Option<u32> },
    OcclusionPredicate,
}

impl Clone for Kind {
    fn clone(&self) -> Self {
        match *self {
            Self::Event => Self::Event,
            Self::Occlusion { predicate } => Self::Occlusion { predicate },
            Self::Timestamp => Self::Timestamp,
            Self::TimestampDisjoint => Self::TimestampDisjoint,
            Self::PipelineStatistics => Self::PipelineStatistics,
            Self::SoStatistics { stream } => Self::SoStatistics { stream },
            Self::OcclusionPredicate => Self::OcclusionPredicate,
        }
    }
}

impl PartialEq for Kind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Event, Self::Event)
            | (Self::Timestamp, Self::Timestamp)
            | (Self::TimestampDisjoint, Self::TimestampDisjoint)
            | (Self::PipelineStatistics, Self::PipelineStatistics)
            | (Self::OcclusionPredicate, Self::OcclusionPredicate) => true,
            (Self::Occlusion { predicate: a }, Self::Occlusion { predicate: b }) => a == b,
            (Self::SoStatistics { stream: a }, Self::SoStatistics { stream: b }) => a == b,
            _ => false,
        }
    }
}

impl Eq for Kind {}

impl fmt::Debug for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Event => f.write_str("Event"),
            Self::Occlusion { predicate } => f
                .debug_struct("Occlusion")
                .field("predicate", predicate)
                .finish(),
            Self::Timestamp => f.write_str("Timestamp"),
            Self::TimestampDisjoint => f.write_str("TimestampDisjoint"),
            Self::PipelineStatistics => f.write_str("PipelineStatistics"),
            Self::SoStatistics { stream } => f
                .debug_struct("SoStatistics")
                .field("stream", stream)
                .finish(),
            Self::OcclusionPredicate => f.write_str("OcclusionPredicate"),
        }
    }
}

struct QueryInner {
    state: State,
    has_been_issued: bool,
    should_be_signaled_at: u64,
    stall_counter: u32,
    accumulated: u64,
    timestamp_issued: Option<u64>,
    deferred_visibility: Option<Arc<VisibilityQuery>>,
}

static QUERY_VTBL: ID3D11Query1Vtbl = ID3D11Query1Vtbl {
    query_interface: query_query_interface,
    add_ref: query_add_ref,
    release: query_release,
    get_device: query_get_device,
    get_private_data: get_private_data::<Query>,
    set_private_data: set_private_data::<Query>,
    set_private_data_interface: set_private_data_interface::<Query>,
    get_data_size: query_get_data_size,
    get_desc: query_get_desc,
    get_desc1: query_get_desc1,
};

/// `ID3D11Query1` / `ID3D11Predicate` implementation.
#[repr(C)]
pub struct Query {
    vtbl: *const ID3D11Query1Vtbl,
    refs: ComObject,
    device: *mut c_void,
    private_data: PrivateData,
    desc: D3d11QueryDesc1,
    kind: Kind,
    is_predicate: bool,
    data_size: u32,
    inner: Mutex<QueryInner>,
    visibility: Option<Arc<VisibilityQuery>>,
    timestamp: Option<Arc<TimestampQuery>>,
    d3d10: crate::d3d10::Wrapper<ID3D10QueryVtbl>,
}

const _: () = assert!(size_of::<i32>() == 4);
const _: () = assert!(size_of::<u64>() == 8);
const _: () = assert!(size_of::<D3d11QueryDataTimestampDisjoint>() == 16);
const _: () = assert!(size_of::<D3d11QueryDataPipelineStatistics>() == 88);
const _: () = assert!(size_of::<D3d11QueryDataSoStatistics>() == 16);

/// Returns `(Kind, data_size)` for `query_type`, or `None` if `query_type > 15`.
#[must_use]
pub const fn data_size_for(query_type: u32) -> Option<(Kind, u32)> {
    match query_type {
        D3D11_QUERY_EVENT => Some((Kind::Event, 4)),
        D3D11_QUERY_OCCLUSION => Some((Kind::Occlusion { predicate: false }, 8)),
        D3D11_QUERY_TIMESTAMP => Some((Kind::Timestamp, 8)),
        D3D11_QUERY_TIMESTAMP_DISJOINT => Some((Kind::TimestampDisjoint, 16)),
        D3D11_QUERY_PIPELINE_STATISTICS => Some((Kind::PipelineStatistics, 88)),
        D3D11_QUERY_OCCLUSION_PREDICATE => Some((Kind::OcclusionPredicate, 4)),
        D3D11_QUERY_SO_STATISTICS => Some((Kind::SoStatistics { stream: None }, 16)),
        D3D11_QUERY_SO_OVERFLOW_PREDICATE
        | D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM0
        | D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM1
        | D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM2
        | D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM3 => {
            Some((Kind::Occlusion { predicate: true }, 4))
        }
        D3D11_QUERY_SO_STATISTICS_STREAM0 => Some((Kind::SoStatistics { stream: Some(0) }, 16)),
        D3D11_QUERY_SO_STATISTICS_STREAM1 => Some((Kind::SoStatistics { stream: Some(1) }, 16)),
        D3D11_QUERY_SO_STATISTICS_STREAM2 => Some((Kind::SoStatistics { stream: Some(2) }, 16)),
        D3D11_QUERY_SO_STATISTICS_STREAM3 => Some((Kind::SoStatistics { stream: Some(3) }, 16)),
        _ => None,
    }
}

impl Query {
    fn new_raw(device: *mut c_void, desc: D3d11QueryDesc1, is_predicate: bool) -> *mut Self {
        let (kind, data_size) = data_size_for(desc.query).expect("validated query type");
        // SAFETY: `device` is null (in unit tests) or a valid COM device pointer.
        unsafe { crate::com::add_ref(device) };
        let immediate_ctx =
            Device::from_iface(device).map_or(ptr::null_mut(), |d| d.immediate().cast::<c_void>());
        let visibility = matches!(
            kind,
            Kind::Occlusion { predicate: false } | Kind::OcclusionPredicate
        )
        .then(|| Arc::new(VisibilityQuery::new()));
        let timestamp = (kind == Kind::Timestamp).then(|| Arc::new(TimestampQuery::new()));
        let has_been_issued = !matches!(kind, Kind::Event | Kind::TimestampDisjoint);
        let mut boxed = Box::new(Self {
            vtbl: &raw const QUERY_VTBL,
            refs: ComObject::with_initial_ref(),
            device,
            private_data: PrivateData::default(),
            desc,
            kind,
            is_predicate,
            data_size,
            inner: Mutex::new(QueryInner {
                state: State::Signaled,
                has_been_issued,
                should_be_signaled_at: 0,
                stall_counter: 0,
                accumulated: 0,
                timestamp_issued: None,
                deferred_visibility: None,
            }),
            visibility,
            timestamp,
            d3d10: crate::d3d10::Wrapper::new(
                &crate::d3d10::QUERY_VTBL,
                ptr::null_mut(),
                immediate_ctx,
            ),
        });
        let raw = &raw mut *boxed;
        boxed.d3d10.d3d11 = raw.cast::<c_void>();
        Box::into_raw(boxed)
    }

    fn lock(&self) -> MutexGuard<'_, QueryInner> {
        lock(&self.inner)
    }

    /// Recovers `&Query` from an `ID3D11Asynchronous*` / `ID3D11Query*` / `ID3D11Predicate*`.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: `iface` is non-null and its first word holds the COM vtable pointer.
        let vtbl = unsafe { *iface.cast::<*const ID3D11Query1Vtbl>() };
        if !ptr::eq(vtbl, &raw const QUERY_VTBL) {
            return None;
        }
        // SAFETY: `vtbl` matches `QUERY_VTBL`, so `iface` points to a live `Query`.
        Some(unsafe { &*iface.cast::<Self>() })
    }

    /// Returns the query kind.
    #[must_use]
    pub const fn kind(&self) -> &Kind {
        &self.kind
    }

    /// Returns the current query state.
    #[must_use]
    pub fn state(&self) -> State {
        self.lock().state.clone()
    }

    /// `ID3D11Asynchronous::GetDataSize`.
    #[must_use]
    pub const fn data_size(&self) -> u32 {
        self.data_size
    }

    /// Returns the underlying core [`VisibilityQuery`] for occlusion queries.
    #[must_use]
    pub const fn visibility(&self) -> Option<&Arc<VisibilityQuery>> {
        self.visibility.as_ref()
    }

    /// Returns the underlying core [`TimestampQuery`] for timestamp queries.
    #[must_use]
    pub const fn timestamp(&self) -> Option<&Arc<TimestampQuery>> {
        self.timestamp.as_ref()
    }

    /// Begins a query bracket (`ID3D11DeviceContext::Begin`).
    pub fn begin(&self, _ctx: *mut c_void) {
        self.begin_query();
    }

    /// Begins this query (`ID3D11DeviceContext::Begin`).
    pub fn begin_query(&self) {
        let mut inner = self.lock();
        match self.kind {
            Kind::Event | Kind::Timestamp | Kind::TimestampDisjoint => {}
            Kind::Occlusion { predicate: false } | Kind::OcclusionPredicate => {
                if inner.state != State::Building {
                    inner.state = State::Building;
                    inner.deferred_visibility = None;
                    if let Some(vis) = &self.visibility {
                        vis.reset();
                    }
                }
            }
            Kind::Occlusion { predicate: true }
            | Kind::PipelineStatistics
            | Kind::SoStatistics { .. } => {
                inner.state = State::Building;
            }
        }
    }

    /// Ends or issues a query (`ID3D11DeviceContext::End`).
    pub fn end(&self, _ctx: *mut c_void) {
        self.end_at(0);
    }

    /// Ends or issues this query at sequence `seq` (`ID3D11DeviceContext::End`).
    pub fn end_at(&self, seq: u64) {
        let mut inner = self.lock();
        match self.kind {
            Kind::Event | Kind::TimestampDisjoint => {
                inner.state = State::Issued;
                inner.has_been_issued = true;
                inner.should_be_signaled_at = seq;
                inner.stall_counter = 0;
            }
            Kind::Timestamp => {
                inner.state = State::Issued;
                inner.has_been_issued = true;
                inner.timestamp_issued = None;
            }
            Kind::Occlusion { predicate: false } | Kind::OcclusionPredicate => match inner.state {
                State::Signaled => {
                    inner.accumulated = 0;
                }
                State::Issued => {}
                State::Building => {
                    inner.state = State::Issued;
                    inner.has_been_issued = true;
                }
            },
            Kind::Occlusion { predicate: true }
            | Kind::PipelineStatistics
            | Kind::SoStatistics { .. } => {
                inner.state = State::Issued;
                inner.has_been_issued = true;
            }
        }
    }

    /// Attaches a deferred command list's [`VisibilityQuery`] (`DoDeferredQuery`).
    pub fn do_deferred_query(&self, query: Arc<VisibilityQuery>) {
        let mut inner = self.lock();
        inner.accumulated = 0;
        inner.state = State::Issued;
        inner.has_been_issued = true;
        inner.deferred_visibility = Some(query);
    }

    /// Directly issues a timestamp value to this query.
    pub fn issue_timestamp(&self, timestamp: u64) {
        let mut inner = self.lock();
        inner.timestamp_issued = Some(timestamp);
    }

    /// Checks the event state of an `Event` or `TimestampDisjoint` query (`CheckEventState`).
    #[must_use]
    pub fn check_event_state(&self, coherent_seq: u64) -> EventState {
        let mut inner = self.lock();
        Self::step_event_state(&mut inner, coherent_seq)
    }

    /// Whether the event query has hit `EVENT_STALL_THRESHOLD` and requires a forced flush.
    #[must_use]
    pub fn is_stalled(&self) -> bool {
        self.lock().stall_counter == u32::MAX
    }

    const fn step_event_state(inner: &mut QueryInner, coherent_seq: u64) -> EventState {
        if !inner.has_been_issued {
            return EventState::Invalid;
        }
        if inner.should_be_signaled_at <= coherent_seq {
            inner.state = State::Signaled;
            inner.stall_counter = 0;
            return EventState::Signaled;
        }
        if inner.stall_counter != u32::MAX {
            if inner.stall_counter < EVENT_STALL_THRESHOLD {
                inner.stall_counter += 1;
                return EventState::Pending;
            }
            inner.stall_counter = u32::MAX;
        }
        EventState::Stall
    }

    /// Reads query results (`ID3D11DeviceContext::GetData`).
    pub fn data(
        &self,
        _ctx: *mut c_void,
        data: *mut c_void,
        data_size: u32,
        _flags: u32,
    ) -> Hresult {
        self.data_at(0, data, data_size)
    }

    /// Reads query results at `coherent_seq` (`ID3D11DeviceContext::GetData`).
    pub fn data_at(&self, coherent_seq: u64, data: *mut c_void, data_size: u32) -> Hresult {
        self.data_at_impl(coherent_seq, data, data_size)
    }

    fn data_at_impl(&self, coherent_seq: u64, data: *mut c_void, data_size: u32) -> Hresult {
        if data_size != 0 && (data.is_null() || data_size != self.data_size) {
            return E_INVALIDARG;
        }
        let write_out = !data.is_null() && data_size == self.data_size;
        let mut inner = self.lock();
        if inner.state == State::Building {
            return DXGI_ERROR_INVALID_CALL;
        }
        let accumulated;
        match self.kind {
            Kind::Event => {
                let st = Self::step_event_state(&mut inner, coherent_seq);
                drop(inner);
                match st {
                    EventState::Invalid => DXGI_ERROR_INVALID_CALL,
                    EventState::Signaled => {
                        if write_out {
                            // SAFETY: `data` is non-null and `data_size == 4`.
                            unsafe { data.cast::<i32>().write_unaligned(1) };
                        }
                        S_OK
                    }
                    EventState::Pending | EventState::Stall => {
                        if write_out {
                            // SAFETY: `data` is non-null and `data_size == 4`.
                            unsafe { data.cast::<i32>().write_unaligned(0) };
                        }
                        S_FALSE
                    }
                }
            }
            Kind::TimestampDisjoint => {
                let st = Self::step_event_state(&mut inner, coherent_seq);
                drop(inner);
                match st {
                    EventState::Invalid => DXGI_ERROR_INVALID_CALL,
                    EventState::Signaled => {
                        if write_out {
                            let out = D3d11QueryDataTimestampDisjoint {
                                frequency: TIMESTAMP_FREQUENCY_HZ,
                                disjoint: 0,
                            };
                            // SAFETY: `data` is non-null and `data_size == 16`.
                            unsafe {
                                data.cast::<D3d11QueryDataTimestampDisjoint>()
                                    .write_unaligned(out);
                            }
                        }
                        S_OK
                    }
                    EventState::Pending | EventState::Stall => {
                        if write_out {
                            let out = D3d11QueryDataTimestampDisjoint {
                                frequency: TIMESTAMP_FREQUENCY_HZ,
                                disjoint: 0,
                            };
                            // SAFETY: `data` is non-null and `data_size == 16`.
                            unsafe {
                                data.cast::<D3d11QueryDataTimestampDisjoint>()
                                    .write_unaligned(out);
                            }
                        }
                        S_FALSE
                    }
                }
            }
            Kind::Occlusion { predicate: false } | Kind::OcclusionPredicate => {
                if inner.state != State::Signaled {
                    let vis_val = inner
                        .deferred_visibility
                        .as_ref()
                        .or(self.visibility.as_ref())
                        .and_then(|v| v.value());
                    let Some(val) = vis_val else {
                        return S_FALSE;
                    };
                    inner.accumulated = val;
                    inner.state = State::Signaled;
                    inner.deferred_visibility = None;
                    if let Some(vis) = &self.visibility {
                        vis.reset();
                    }
                }
                accumulated = inner.accumulated;
                drop(inner);
                if write_out {
                    if self.kind == Kind::OcclusionPredicate {
                        let passed = i32::from(accumulated != 0);
                        // SAFETY: `data` is non-null and `data_size == 4`.
                        unsafe { data.cast::<i32>().write_unaligned(passed) };
                    } else {
                        // SAFETY: `data` is non-null and `data_size == 8`.
                        unsafe { data.cast::<u64>().write_unaligned(accumulated) };
                    }
                }
                S_OK
            }
            Kind::Timestamp => {
                if inner.state != State::Signaled {
                    let ts_val = inner
                        .timestamp_issued
                        .or_else(|| self.timestamp.as_ref().and_then(|t| t.value()));
                    let Some(val) = ts_val else {
                        return S_FALSE;
                    };
                    inner.accumulated = val;
                    inner.state = State::Signaled;
                    inner.timestamp_issued = None;
                }
                accumulated = inner.accumulated;
                drop(inner);
                if write_out {
                    // SAFETY: `data` is non-null and `data_size == 8`.
                    unsafe { data.cast::<u64>().write_unaligned(accumulated) };
                }
                S_OK
            }
            Kind::PipelineStatistics => {
                inner.state = State::Signaled;
                drop(inner);
                if write_out {
                    let zero = D3d11QueryDataPipelineStatistics {
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
                    // SAFETY: `data` is non-null and `data_size == 88`.
                    unsafe {
                        data.cast::<D3d11QueryDataPipelineStatistics>()
                            .write_unaligned(zero);
                    }
                }
                S_OK
            }
            Kind::SoStatistics { .. } => {
                inner.state = State::Signaled;
                drop(inner);
                if write_out {
                    let zero = D3d11QueryDataSoStatistics {
                        num_primitives_written: 0,
                        primitives_storage_needed: 0,
                    };
                    // SAFETY: `data` is non-null and `data_size == 16`.
                    unsafe {
                        data.cast::<D3d11QueryDataSoStatistics>()
                            .write_unaligned(zero);
                    }
                }
                S_OK
            }
            Kind::Occlusion { predicate: true } => {
                inner.state = State::Signaled;
                drop(inner);
                if write_out {
                    // SAFETY: `data` is non-null and `data_size == 4`.
                    unsafe { data.cast::<i32>().write_unaligned(0) };
                }
                S_OK
            }
        }
    }

    /// Alias of [`Self::data_at`].
    pub fn get_data(&self, coherent_seq: u64, data: *mut c_void, data_size: u32) -> Hresult {
        self.data_at_impl(coherent_seq, data, data_size)
    }
}

impl Drop for Query {
    fn drop(&mut self) {
        // SAFETY: `self.device` was retained in `Query::new_raw`.
        unsafe { crate::com::release(self.device) };
    }
}

fn create_query_impl(
    device: *mut c_void,
    desc: *const D3d11QueryDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null and writable.
        unsafe { *out = ptr::null_mut() };
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let d = unsafe { *desc };
    let desc1 = D3d11QueryDesc1 {
        query: d.query,
        misc_flags: d.misc_flags,
        context_type: 0,
    };
    create_query_internal(device, desc1, false, out)
}

/// `ID3D11Device::CreateQuery`.
pub fn create_query(
    device: *mut c_void,
    desc: *const D3d11QueryDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_query_impl(device, desc, out)
}

fn create_query1_impl(
    device: *mut c_void,
    desc: *const D3d11QueryDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null and writable.
        unsafe { *out = ptr::null_mut() };
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` is non-null.
    let d1 = unsafe { *desc };
    create_query_internal(device, d1, false, out)
}

/// `ID3D11Device3::CreateQuery1`.
pub fn create_query1(
    device: *mut c_void,
    desc: *const D3d11QueryDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_query1_impl(device, desc, out)
}

fn create_predicate_impl(
    device: *mut c_void,
    desc: *const D3d11QueryDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_query_impl(device, desc, out)
}

/// `ID3D11Device::CreatePredicate`.
pub fn create_predicate(
    device: *mut c_void,
    desc: *const D3d11QueryDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_predicate_impl(device, desc, out)
}

fn create_query_internal(
    device: *mut c_void,
    desc: D3d11QueryDesc1,
    is_predicate: bool,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = is_predicate;
    if out.is_null() {
        return S_FALSE;
    }
    if desc.query > D3D11_QUERY_OCCLUSION_PREDICATE {
        dxmt_util::err!(
            crate::LOG,
            "CreateQuery1: query type not implemented: {}",
            desc.query
        );
        return E_NOTIMPL;
    }
    if data_size_for(desc.query).is_none()
        || (desc.misc_flags & !D3D11_QUERY_MISC_PREDICATEHINT) != 0
        || desc.context_type > 4
    {
        return E_INVALIDARG;
    }
    let query = Query::new_raw(device, desc, false);
    // SAFETY: `out` is non-null.
    unsafe { *out = query.cast::<c_void>() };
    S_OK
}

unsafe extern "system-unwind" fn query_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `Query`.
    let q = unsafe { &mut *this.cast::<Query>() };
    let qi = Qi {
        null_riid: E_NOINTERFACE,
        silent: &[],
        log: &crate::LOG,
        miss: QiMiss::OnceSplit {
            object: &IID_ID3D11_QUERY,
            name: "D3D11Query::QueryInterface",
        },
    };
    // SAFETY: the application's `riid` and `ppv_object`, null or valid.
    unsafe {
        qi_table(
            riid,
            ppv_object,
            &[
                (
                    &[
                        IID_IUNKNOWN,
                        IID_ID3D11_DEVICE_CHILD,
                        IID_ID3D11_QUERY,
                        IID_ID3D11_QUERY_1,
                    ],
                    this,
                ),
                (
                    &[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_QUERY],
                    (&raw mut q.d3d10).cast::<c_void>(),
                ),
            ],
            || {
                let _ = q.refs.add_ref();
            },
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn query_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Query`.
    unsafe { (*this.cast::<Query>()).refs.add_ref() }
}

unsafe extern "system-unwind" fn query_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Query`.
    let (rem, free) = unsafe { (*this.cast::<Query>()).refs.release_and_free() };
    if free {
        // SAFETY: the last reference was released; `this` was allocated via `Box::into_raw`.
        drop(unsafe { Box::from_raw(this.cast::<Query>()) });
    }
    rem
}

unsafe extern "system-unwind" fn query_get_device(this: *mut c_void, pp_device: *mut *mut c_void) {
    if pp_device.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Query`.
    let dev = unsafe { (*this.cast::<Query>()).device };
    // SAFETY: `dev` is null or a valid COM pointer.
    unsafe { crate::com::add_ref(dev) };
    // SAFETY: `pp_device` is non-null.
    unsafe { *pp_device = dev };
}

impl HasPrivateData for Query {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `Query`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

unsafe extern "system-unwind" fn query_get_data_size(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Query`.
    unsafe { (*this.cast::<Query>()).data_size }
}

unsafe extern "system-unwind" fn query_get_desc(this: *mut c_void, p_desc: *mut c_void) {
    if p_desc.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Query`.
    let d1 = unsafe { (*this.cast::<Query>()).desc };
    let d = D3d11QueryDesc {
        query: d1.query,
        misc_flags: d1.misc_flags,
    };
    // SAFETY: `p_desc` is non-null and points to a `D3d11QueryDesc`.
    unsafe { p_desc.cast::<D3d11QueryDesc>().write_unaligned(d) };
}

unsafe extern "system-unwind" fn query_get_desc1(this: *mut c_void, p_desc1: *mut c_void) {
    if p_desc1.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Query`.
    let d1 = unsafe { (*this.cast::<Query>()).desc };
    // SAFETY: `p_desc1` is non-null and points to a `D3d11QueryDesc1`.
    unsafe { p_desc1.cast::<D3d11QueryDesc1>().write_unaligned(d1) };
}

#[cfg(test)]
mod tests;
