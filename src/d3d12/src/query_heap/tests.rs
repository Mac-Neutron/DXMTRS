use core::{
    ffi::c_void,
    mem::{align_of, offset_of, size_of},
    ptr,
};

use crate::{
    abi::objects::{
        D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP, D3D12_QUERY_HEAP_TYPE_OCCLUSION,
        D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS, D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS1,
        D3D12_QUERY_HEAP_TYPE_SO_STATISTICS, D3D12_QUERY_HEAP_TYPE_TIMESTAMP,
        D3D12_QUERY_HEAP_TYPE_VIDEO_DECODE_STATISTICS, QueryDataPipelineStatistics,
        QueryDataSoStatistics, QueryHeapDesc,
    },
    com::{E_INVALIDARG, IID_ID3D12_QUERY_HEAP},
    query_heap,
};

#[test]
fn desc_as_cpp() {
    assert_eq!(size_of::<QueryHeapDesc>(), 12);
    assert_eq!(align_of::<QueryHeapDesc>(), 4);
    assert_eq!(offset_of!(QueryHeapDesc, r#type), 0);
    assert_eq!(offset_of!(QueryHeapDesc, count), 4);
    assert_eq!(offset_of!(QueryHeapDesc, node_mask), 8);

    assert_eq!(size_of::<QueryDataPipelineStatistics>(), 88);
    assert_eq!(align_of::<QueryDataPipelineStatistics>(), 8);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, ia_vertices), 0);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, ia_primitives), 8);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, vs_invocations), 16);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, gs_invocations), 24);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, gs_primitives), 32);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, c_invocations), 40);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, c_primitives), 48);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, ps_invocations), 56);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, hs_invocations), 64);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, ds_invocations), 72);
    assert_eq!(offset_of!(QueryDataPipelineStatistics, cs_invocations), 80);

    assert_eq!(size_of::<QueryDataSoStatistics>(), 16);
    assert_eq!(align_of::<QueryDataSoStatistics>(), 8);
    assert_eq!(offset_of!(QueryDataSoStatistics, num_primitives_written), 0);
    assert_eq!(
        offset_of!(QueryDataSoStatistics, primitives_storage_needed),
        8
    );

    assert_eq!(D3D12_QUERY_HEAP_TYPE_OCCLUSION, 0);
    assert_eq!(D3D12_QUERY_HEAP_TYPE_TIMESTAMP, 1);
    assert_eq!(D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS, 2);
    assert_eq!(D3D12_QUERY_HEAP_TYPE_SO_STATISTICS, 3);
    assert_eq!(D3D12_QUERY_HEAP_TYPE_VIDEO_DECODE_STATISTICS, 4);
    assert_eq!(D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP, 5);
    assert_eq!(D3D12_QUERY_HEAP_TYPE_PIPELINE_STATISTICS1, 7);

    // Null descriptor is rejected with E_INVALIDARG.
    // SAFETY: Testing `CreateQueryHeap` null-descriptor validation.
    let hr = unsafe {
        query_heap::create_query_heap(
            ptr::null_mut(),
            ptr::null(),
            ptr::from_ref(&IID_ID3D12_QUERY_HEAP).cast::<c_void>(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr, E_INVALIDARG);
}
