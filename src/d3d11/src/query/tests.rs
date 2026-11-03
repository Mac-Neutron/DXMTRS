use core::{ffi::c_void, ptr};

use super::{
    D3D11_QUERY_EVENT, D3D11_QUERY_OCCLUSION, D3D11_QUERY_OCCLUSION_PREDICATE,
    D3D11_QUERY_PIPELINE_STATISTICS, D3D11_QUERY_SO_OVERFLOW_PREDICATE,
    D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM0, D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM1,
    D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM2, D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM3,
    D3D11_QUERY_SO_STATISTICS, D3D11_QUERY_SO_STATISTICS_STREAM0,
    D3D11_QUERY_SO_STATISTICS_STREAM1, D3D11_QUERY_SO_STATISTICS_STREAM2,
    D3D11_QUERY_SO_STATISTICS_STREAM3, D3D11_QUERY_TIMESTAMP, D3D11_QUERY_TIMESTAMP_DISJOINT,
    DXGI_ERROR_INVALID_CALL, EVENT_STALL_THRESHOLD, EventState, Kind, Query, State,
    TIMESTAMP_FREQUENCY_HZ, create_predicate, create_query, create_query1, data_size_for,
};
use crate::{
    abi::{
        IID_ID3D10_QUERY, IID_ID3D11_FENCE, IID_ID3D11_PREDICATE, IID_ID3D11_QUERY_1,
        records::{
            D3d11QueryDataPipelineStatistics, D3d11QueryDataSoStatistics,
            D3d11QueryDataTimestampDisjoint, D3d11QueryDesc, D3d11QueryDesc1,
        },
        vtbl::{ID3D11FenceVtbl, ID3D11Query1Vtbl, IUnknownVtbl},
    },
    com::{E_NOINTERFACE, E_NOTIMPL, IID_IUNKNOWN, S_FALSE, S_OK, vtbl},
    fence::{self, Fence},
};

unsafe fn release_ref(ptr: *mut c_void) -> u32 {
    // SAFETY: `ptr` is a live COM object pointer.
    let u = unsafe { vtbl::<IUnknownVtbl>(ptr) };
    // SAFETY: `ptr` is valid for `Release`.
    unsafe { (u.release)(ptr) }
}

fn make_query(query_type: u32) -> *mut c_void {
    let desc = D3d11QueryDesc {
        query: query_type,
        misc_flags: 0,
    };
    let mut out = ptr::null_mut();
    assert_eq!(
        create_query(ptr::null_mut(), &raw const desc, &raw mut out),
        S_OK
    );
    assert!(!out.is_null());
    out
}

#[test]
fn data_sizes_as_cpp() {
    let expected: &[(u32, u32, Kind)] = &[
        (D3D11_QUERY_EVENT, 4, Kind::Event),
        (
            D3D11_QUERY_OCCLUSION,
            8,
            Kind::Occlusion { predicate: false },
        ),
        (D3D11_QUERY_TIMESTAMP, 8, Kind::Timestamp),
        (D3D11_QUERY_TIMESTAMP_DISJOINT, 16, Kind::TimestampDisjoint),
        (
            D3D11_QUERY_PIPELINE_STATISTICS,
            88,
            Kind::PipelineStatistics,
        ),
        (D3D11_QUERY_OCCLUSION_PREDICATE, 4, Kind::OcclusionPredicate),
        (
            D3D11_QUERY_SO_STATISTICS,
            16,
            Kind::SoStatistics { stream: None },
        ),
        (
            D3D11_QUERY_SO_OVERFLOW_PREDICATE,
            4,
            Kind::Occlusion { predicate: true },
        ),
        (
            D3D11_QUERY_SO_STATISTICS_STREAM0,
            16,
            Kind::SoStatistics { stream: Some(0) },
        ),
        (
            D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM0,
            4,
            Kind::Occlusion { predicate: true },
        ),
        (
            D3D11_QUERY_SO_STATISTICS_STREAM1,
            16,
            Kind::SoStatistics { stream: Some(1) },
        ),
        (
            D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM1,
            4,
            Kind::Occlusion { predicate: true },
        ),
        (
            D3D11_QUERY_SO_STATISTICS_STREAM2,
            16,
            Kind::SoStatistics { stream: Some(2) },
        ),
        (
            D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM2,
            4,
            Kind::Occlusion { predicate: true },
        ),
        (
            D3D11_QUERY_SO_STATISTICS_STREAM3,
            16,
            Kind::SoStatistics { stream: Some(3) },
        ),
        (
            D3D11_QUERY_SO_OVERFLOW_PREDICATE_STREAM3,
            4,
            Kind::Occlusion { predicate: true },
        ),
    ];

    for (query_type, expected_size, expected_kind) in expected {
        assert_eq!(
            data_size_for(*query_type),
            Some((expected_kind.clone(), *expected_size))
        );
        if *query_type <= D3D11_QUERY_OCCLUSION_PREDICATE {
            let raw = make_query(*query_type);
            let q = Query::from_iface(raw).expect("valid Query");
            assert_eq!(q.kind(), expected_kind);
            assert_eq!(q.data_size(), *expected_size);
            // SAFETY: `raw` is a live `ID3D11Query1` COM pointer.
            let vtbl_ref = unsafe { vtbl::<ID3D11Query1Vtbl>(raw) };
            // SAFETY: `raw` is a valid `Query` instance.
            let vtbl_size = unsafe { (vtbl_ref.get_data_size)(raw) };
            assert_eq!(vtbl_size, *expected_size);
            // SAFETY: `raw` was returned by `create_query` with refcount 1.
            assert_eq!(unsafe { release_ref(raw) }, 0);
        } else {
            let unimpl_desc = D3d11QueryDesc {
                query: *query_type,
                misc_flags: 0,
            };
            let mut unimpl_out = ptr::null_mut();
            assert_eq!(
                create_query(ptr::null_mut(), &raw const unimpl_desc, &raw mut unimpl_out),
                E_NOTIMPL
            );
        }
    }

    assert_eq!(data_size_for(16), None);
    let bad_desc = D3d11QueryDesc {
        query: 16,
        misc_flags: 0,
    };
    let mut bad_out = ptr::null_mut();
    assert_eq!(
        create_query(ptr::null_mut(), &raw const bad_desc, &raw mut bad_out),
        E_NOTIMPL
    );
}

#[test]
fn occlusion_sums_readbacks() {
    let raw = make_query(D3D11_QUERY_OCCLUSION);
    let q = Query::from_iface(raw).expect("valid Query");

    // Starts signaled with accumulated 0.
    assert_eq!(q.state(), State::Signaled);
    let mut count = 99_u64;
    assert_eq!(q.data_at(0, (&raw mut count).cast::<c_void>(), 8), S_OK);
    assert_eq!(count, 0);

    // Begin transitions to Building; GetData while Building returns DXGI_ERROR_INVALID_CALL.
    q.begin_query();
    assert_eq!(q.state(), State::Building);
    assert_eq!(
        q.data_at(0, (&raw mut count).cast::<c_void>(), 8),
        DXGI_ERROR_INVALID_CALL
    );

    let vis = q.visibility().expect("occlusion query has VisibilityQuery");
    vis.begin(1, 1);
    vis.end(2, 2);
    q.end_at(2);
    assert_eq!(q.state(), State::Issued);

    // Chunk 1 issues [100, 10, 20] -> covers [1..3] = 10 + 20 = 30. Not yet at seq_end=2.
    vis.issue(1, &[100, 10, 20]);
    assert_eq!(q.data_at(1, (&raw mut count).cast::<c_void>(), 8), S_FALSE);

    // Chunk 2 issues [5, 7, 999] -> covers [0..2] = 5 + 7 = 12. Total = 42.
    vis.issue(2, &[5, 7, 999]);
    assert_eq!(q.data_at(2, (&raw mut count).cast::<c_void>(), 8), S_OK);
    assert_eq!(count, 42);
    assert_eq!(q.state(), State::Signaled);

    // Subsequent GetData while Signaled returns the accumulated value again.
    let mut count2 = 0_u64;
    assert_eq!(q.data_at(2, (&raw mut count2).cast::<c_void>(), 8), S_OK);
    assert_eq!(count2, 42);

    // Predicate query reports BOOL (1 when non-zero, 0 when zero).
    let pred_desc = D3d11QueryDesc {
        query: D3D11_QUERY_OCCLUSION_PREDICATE,
        misc_flags: 0,
    };
    let mut pred_raw = ptr::null_mut();
    assert_eq!(
        create_predicate(ptr::null_mut(), &raw const pred_desc, &raw mut pred_raw),
        S_OK
    );
    let pred = Query::from_iface(pred_raw).expect("valid predicate Query");
    pred.begin_query();
    let pvis = pred.visibility().expect("predicate has VisibilityQuery");
    pvis.begin(3, 0);
    pvis.end(3, 2);
    pred.end_at(3);
    pvis.issue(3, &[0, 9]);
    let mut flag = 0_i32;
    assert_eq!(pred.data_at(3, (&raw mut flag).cast::<c_void>(), 4), S_OK);
    assert_eq!(flag, 1);

    // SAFETY: `pred_raw` and `raw` are live COM pointers with refcount 1.
    assert_eq!(unsafe { release_ref(pred_raw) }, 0);
    // SAFETY: `raw` is a live COM pointer with refcount 1.
    assert_eq!(unsafe { release_ref(raw) }, 0);
}

#[test]
fn event_and_disjoint_and_dummy_queries() {
    let ev_raw = make_query(D3D11_QUERY_EVENT);
    let ev = Query::from_iface(ev_raw).expect("event query");
    let mut done = -1_i32;
    // Before End(), Event query is unissued -> Invalid -> DXGI_ERROR_INVALID_CALL.
    assert_eq!(ev.check_event_state(0), EventState::Invalid);
    assert_eq!(
        ev.data_at(0, (&raw mut done).cast::<c_void>(), 4),
        DXGI_ERROR_INVALID_CALL
    );

    ev.end_at(10);
    assert_eq!(ev.state(), State::Issued);
    for _ in 0..EVENT_STALL_THRESHOLD {
        assert_eq!(ev.check_event_state(5), EventState::Pending);
    }
    assert_eq!(ev.check_event_state(5), EventState::Stall);
    assert_eq!(ev.data_at(5, (&raw mut done).cast::<c_void>(), 4), S_FALSE);
    assert_eq!(done, 0);

    assert_eq!(ev.data_at(10, (&raw mut done).cast::<c_void>(), 4), S_OK);
    assert_eq!(done, 1);
    assert_eq!(ev.state(), State::Signaled);

    // TimestampDisjoint query.
    let dj_raw = make_query(D3D11_QUERY_TIMESTAMP_DISJOINT);
    let dj = Query::from_iface(dj_raw).expect("disjoint query");
    dj.end_at(4);
    let mut dj_data = D3d11QueryDataTimestampDisjoint {
        frequency: 0,
        disjoint: 1,
    };
    assert_eq!(
        dj.data_at(3, (&raw mut dj_data).cast::<c_void>(), 16),
        S_FALSE
    );
    assert_eq!(dj.data_at(4, (&raw mut dj_data).cast::<c_void>(), 16), S_OK);
    assert_eq!(dj_data.frequency, TIMESTAMP_FREQUENCY_HZ);
    assert_eq!(dj_data.disjoint, 0);

    // Timestamp query.
    let ts_raw = make_query(D3D11_QUERY_TIMESTAMP);
    let ts = Query::from_iface(ts_raw).expect("timestamp query");
    ts.end_at(1);
    let mut ts_val = 0_u64;
    assert_eq!(
        ts.data_at(1, (&raw mut ts_val).cast::<c_void>(), 8),
        S_FALSE
    );
    ts.issue_timestamp(123_456_789);
    assert_eq!(ts.data_at(1, (&raw mut ts_val).cast::<c_void>(), 8), S_OK);
    assert_eq!(ts_val, 123_456_789);

    // Pipeline statistics dummy query zero-fills and returns S_OK; SO statistics returns E_NOTIMPL.
    let ps_raw = make_query(D3D11_QUERY_PIPELINE_STATISTICS);
    let ps = Query::from_iface(ps_raw).expect("pipeline stats query");
    let mut ps_data = D3d11QueryDataPipelineStatistics {
        ia_vertices: 1,
        ia_primitives: 2,
        vs_invocations: 3,
        gs_invocations: 4,
        gs_primitives: 5,
        c_invocations: 6,
        c_primitives: 7,
        ps_invocations: 8,
        hs_invocations: 9,
        ds_invocations: 10,
        cs_invocations: 11,
    };
    assert_eq!(ps.data_at(0, (&raw mut ps_data).cast::<c_void>(), 88), S_OK);
    assert_eq!(ps_data.ia_vertices, 0);
    assert_eq!(ps_data.cs_invocations, 0);

    let so_desc = D3d11QueryDesc {
        query: D3D11_QUERY_SO_STATISTICS,
        misc_flags: 0,
    };
    let mut so_raw = ptr::null_mut();
    assert_eq!(
        create_query(ptr::null_mut(), &raw const so_desc, &raw mut so_raw),
        E_NOTIMPL
    );
    let so_data = D3d11QueryDataSoStatistics {
        num_primitives_written: 7,
        primitives_storage_needed: 8,
    };
    assert_eq!(so_data.num_primitives_written, 7);
    assert_eq!(so_data.primitives_storage_needed, 8);

    // QueryInterface & GetDesc1 checks.
    let desc1 = D3d11QueryDesc1 {
        query: D3D11_QUERY_OCCLUSION_PREDICATE,
        misc_flags: 1,
        context_type: 2,
    };
    let mut q1_raw = ptr::null_mut();
    assert_eq!(
        create_query1(ptr::null_mut(), &raw const desc1, &raw mut q1_raw),
        S_OK
    );
    // SAFETY: `q1_raw` is a live `ID3D11Query1`.
    let q1_vtbl = unsafe { vtbl::<ID3D11Query1Vtbl>(q1_raw) };
    let mut got_desc1 = D3d11QueryDesc1 {
        query: 0,
        misc_flags: 0,
        context_type: 0,
    };
    // SAFETY: `q1_raw` and `got_desc1` are valid.
    unsafe { (q1_vtbl.get_desc1)(q1_raw, (&raw mut got_desc1).cast::<c_void>()) };
    assert_eq!(got_desc1.query, D3D11_QUERY_OCCLUSION_PREDICATE);
    assert_eq!(got_desc1.misc_flags, 1);
    assert_eq!(got_desc1.context_type, 2);

    let mut qi_out = ptr::null_mut();
    let iid_q1 = IID_ID3D11_QUERY_1;
    assert_eq!(
        // SAFETY: `q1_raw` is a live `Query`.
        unsafe {
            (q1_vtbl.query_interface)(
                q1_raw,
                (&raw const iid_q1).cast::<c_void>(),
                &raw mut qi_out,
            )
        },
        S_OK
    );
    // SAFETY: `qi_out` was addref'd by `query_interface`.
    assert_eq!(unsafe { release_ref(qi_out) }, 1);
    // Non-predicate created via CreateQuery1 does not expose ID3D11Predicate.
    let iid_pred = IID_ID3D11_PREDICATE;
    assert_eq!(
        // SAFETY: `q1_raw` is a live `Query`.
        unsafe {
            (q1_vtbl.query_interface)(
                q1_raw,
                (&raw const iid_pred).cast::<c_void>(),
                &raw mut qi_out,
            )
        },
        E_NOINTERFACE
    );
    // D3D10 query interface is exposed.
    let iid_d3d10_q = IID_ID3D10_QUERY;
    assert_eq!(
        // SAFETY: `q1_raw` is a live `Query`.
        unsafe {
            (q1_vtbl.query_interface)(
                q1_raw,
                (&raw const iid_d3d10_q).cast::<c_void>(),
                &raw mut qi_out,
            )
        },
        S_OK
    );
    assert!(!qi_out.is_null());
    // SAFETY: `q1_raw` had its refcount incremented for `d3d10` interface.
    assert_eq!(unsafe { release_ref(q1_raw) }, 1);

    // Fence creation & signaling check.
    let mut fence_raw = ptr::null_mut();
    let iid_fence = IID_ID3D11_FENCE;
    assert_eq!(
        fence::create(
            ptr::null_mut(),
            7,
            0,
            &raw const iid_fence,
            &raw mut fence_raw
        ),
        S_OK
    );
    let f = Fence::from_iface(fence_raw).expect("valid Fence");
    assert_eq!(f.completed_value(), 7);
    f.signal(19);
    // SAFETY: `fence_raw` is a live `ID3D11Fence`.
    let f_vtbl = unsafe { vtbl::<ID3D11FenceVtbl>(fence_raw) };
    // SAFETY: `fence_raw` is valid.
    assert_eq!(unsafe { (f_vtbl.get_completed_value)(fence_raw) }, 19);
    assert_eq!(
        // SAFETY: `fence_raw` is valid.
        unsafe { (f_vtbl.set_event_on_completion)(fence_raw, 19, ptr::null_mut()) },
        S_OK
    );
    let mut unk = ptr::null_mut();
    let iid_unk = IID_IUNKNOWN;
    assert_eq!(
        // SAFETY: `fence_raw` is valid.
        unsafe {
            (f_vtbl.query_interface)(
                fence_raw,
                (&raw const iid_unk).cast::<c_void>(),
                &raw mut unk,
            )
        },
        S_OK
    );
    // SAFETY: `unk` and `fence_raw` are valid COM pointers.
    assert_eq!(unsafe { release_ref(unk) }, 1);
    // SAFETY: `fence_raw` has 1 remaining reference.
    assert_eq!(unsafe { release_ref(fence_raw) }, 0);

    // SAFETY: release remaining test queries.
    assert_eq!(unsafe { release_ref(q1_raw) }, 0);
    // SAFETY: `ps_raw` is live.
    assert_eq!(unsafe { release_ref(ps_raw) }, 0);
    // SAFETY: `ts_raw` is live.
    assert_eq!(unsafe { release_ref(ts_raw) }, 0);
    // SAFETY: `dj_raw` is live.
    assert_eq!(unsafe { release_ref(dj_raw) }, 0);
    // SAFETY: `ev_raw` is live.
    assert_eq!(unsafe { release_ref(ev_raw) }, 0);
}
