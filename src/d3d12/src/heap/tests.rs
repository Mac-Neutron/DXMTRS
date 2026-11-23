use core::{
    ffi::c_void,
    mem::{align_of, offset_of, size_of},
    ptr,
};

use winemetal_wire::{WMT_RESOURCE_STORAGE_MODE_PRIVATE, WMT_RESOURCE_STORAGE_MODE_SHARED};

use crate::{
    abi::{
        barrier::{
            D3D12_HEAP_TYPE_DEFAULT, D3D12_HEAP_TYPE_GPU_UPLOAD, D3D12_HEAP_TYPE_READBACK,
            D3D12_HEAP_TYPE_UPLOAD,
        },
        objects::{D3D12_HEAP_FLAG_ALLOW_DISPLAY, HeapDesc},
        resource::{
            D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE,
            D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT, D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER,
            D3D12_MEMORY_POOL_L0, D3D12_MEMORY_POOL_UNKNOWN, HeapProperties,
        },
    },
    com::{
        E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_OUTOFMEMORY, IID_ID3D12_HEAP, IID_ID3D12_HEAP1,
        IID_ID3D12_PAGEABLE, S_FALSE, S_OK,
    },
    heap::{self, HeapAllocChoice},
};

#[test]
fn native_cpu_heap_through_hooks() {
    assert_eq!(size_of::<HeapDesc>(), 48);
    assert_eq!(align_of::<HeapDesc>(), 8);
    assert_eq!(offset_of!(HeapDesc, size_in_bytes), 0);
    assert_eq!(offset_of!(HeapDesc, properties), 8);
    assert_eq!(offset_of!(HeapDesc, alignment), 32);
    assert_eq!(offset_of!(HeapDesc, flags), 40);

    let heap_props = |ty: u32| HeapProperties {
        r#type: ty,
        cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
        creation_node_mask: 1,
        visible_node_mask: 1,
    };

    let upload_desc = HeapDesc {
        size_in_bytes: 5000,
        properties: heap_props(D3D12_HEAP_TYPE_UPLOAD),
        alignment: 0,
        flags: 0,
    };

    // With memory hooks active, a CPU-visible heap plans a 64 KiB-aligned native CPU buffer (B2).
    assert_eq!(
        heap::plan_heap_allocation(&upload_desc, true, true),
        HeapAllocChoice::NativeCpuBuffer {
            aligned_size: 65536
        }
    );

    // Without memory hooks, the same upload heap falls back to a Shared MTLHeap.
    assert_eq!(
        heap::plan_heap_allocation(&upload_desc, true, false),
        HeapAllocChoice::MetalHeap {
            size: 65536,
            options: WMT_RESOURCE_STORAGE_MODE_SHARED,
        }
    );

    // A default heap with `private_on = true` uses a Private MTLHeap even when hooks are active.
    let default_desc = HeapDesc {
        properties: heap_props(D3D12_HEAP_TYPE_DEFAULT),
        ..upload_desc.copy()
    };
    assert_eq!(
        heap::plan_heap_allocation(&default_desc, true, true),
        HeapAllocChoice::MetalHeap {
            size: 65536,
            options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        }
    );

    // A readback heap without hooks uses Shared storage mode.
    let readback_desc = HeapDesc {
        properties: heap_props(D3D12_HEAP_TYPE_READBACK),
        ..upload_desc.copy()
    };
    assert_eq!(
        heap::plan_heap_allocation(&readback_desc, true, false),
        HeapAllocChoice::MetalHeap {
            size: 65536,
            options: WMT_RESOURCE_STORAGE_MODE_SHARED,
        }
    );

    // Construct a native CPU heap through the hooks fake and exercise its COM interface.
    let mut backing = vec![0u8; 65536];
    let backing_ptr = backing.as_mut_ptr().cast::<c_void>();
    let mut requested_size = 0u64;
    let raw = heap::create_cpu_heap_with_hook(&upload_desc, |aligned| {
        requested_size = aligned;
        (None, backing_ptr)
    })
    .expect("hooked CPU heap succeeds");
    assert_eq!(requested_size, 65536);
    assert!(!raw.is_null());

    // SAFETY: `raw` is a valid `HeapObject` pointer.
    let heap_ref = unsafe { heap::cast_heap(raw.cast()) }.expect("cast_heap recognizes HeapObject");
    assert_eq!(heap_ref.cpu_memory, backing_ptr);
    assert!(heap_ref.heap.is_none());

    // SAFETY: `raw` has `ID3D12HeapVtbl` at offset 0.
    let vtbl = unsafe { (*raw).vtbl };
    let mut out_desc = HeapDesc::empty();
    // SAFETY: `raw` and `out_desc` are valid for `GetDesc`.
    let ret_desc = unsafe { (vtbl.get_desc)(raw.cast(), (&raw mut out_desc).cast()) };
    assert_eq!(ret_desc, (&raw mut out_desc).cast());
    assert_eq!(out_desc.size_in_bytes, 5000);
    assert_eq!(
        out_desc.alignment,
        D3D12_DEFAULT_RESOURCE_PLACEMENT_ALIGNMENT
    );

    // QueryInterface answers ID3D12Heap and ID3D12Pageable, and rejects ID3D12Heap1.
    for iid in [IID_ID3D12_HEAP, IID_ID3D12_PAGEABLE] {
        let mut qi_out: *mut c_void = ptr::null_mut();
        // SAFETY: `raw` and `qi_out` are valid for `QueryInterface`.
        let hr_qi = unsafe {
            (vtbl.query_interface)(raw.cast(), ptr::from_ref(&iid).cast(), &raw mut qi_out)
        };
        assert_eq!(hr_qi, S_OK);
        assert_eq!(qi_out, raw.cast());
        // SAFETY: Release the queried reference.
        assert_eq!(unsafe { (vtbl.release)(qi_out) }, 1);
    }
    let mut qi_heap1: *mut c_void = ptr::dangling_mut();
    // SAFETY: `raw` and `qi_heap1` are valid for `QueryInterface`.
    let hr_heap1 = unsafe {
        (vtbl.query_interface)(
            raw.cast(),
            ptr::from_ref(&IID_ID3D12_HEAP1).cast(),
            &raw mut qi_heap1,
        )
    };
    assert_eq!(hr_heap1, E_NOINTERFACE);
    assert!(qi_heap1.is_null());

    // SAFETY: Final release destroys the `HeapObject`.
    assert_eq!(unsafe { (vtbl.release)(raw.cast()) }, 0);

    // When the hook fails to map memory (null pointer), creation returns E_OUTOFMEMORY.
    let oom = heap::create_cpu_heap_with_hook(&upload_desc, |_| (None, ptr::null_mut()));
    assert_eq!(oom, Err(E_OUTOFMEMORY));

    // Entry-point validation: ALLOW_DISPLAY flag is rejected with E_INVALIDARG; null out-pointer
    // on a valid descriptor returns S_FALSE; OpenExistingHeap* return E_NOTIMPL.
    let bad_display = HeapDesc {
        flags: D3D12_HEAP_FLAG_ALLOW_DISPLAY,
        ..upload_desc.copy()
    };
    // SAFETY: Testing `CreateHeap` validation with `D3D12_HEAP_FLAG_ALLOW_DISPLAY`.
    let hr_display = unsafe {
        heap::create_heap(
            ptr::null_mut(),
            (&raw const bad_display).cast(),
            ptr::from_ref(&IID_ID3D12_HEAP).cast(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_display, E_INVALIDARG);

    // SAFETY: Testing `CreateHeap` with valid descriptor and null out-pointer returning `S_FALSE`.
    let hr_probe = unsafe {
        heap::create_heap(
            ptr::null_mut(),
            (&raw const upload_desc).cast(),
            ptr::from_ref(&IID_ID3D12_HEAP).cast(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_probe, S_FALSE);

    // SAFETY: Testing `OpenExistingHeapFromAddress` returning `E_NOTIMPL`.
    let hr_addr = unsafe {
        heap::open_existing_heap_from_address(
            ptr::null_mut(),
            ptr::null(),
            ptr::from_ref(&IID_ID3D12_HEAP).cast(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_addr, E_NOTIMPL);

    // SAFETY: Testing `OpenExistingHeapFromFileMapping` returning `E_NOTIMPL`.
    let hr_map = unsafe {
        heap::open_existing_heap_from_file_mapping(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::from_ref(&IID_ID3D12_HEAP).cast(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_map, E_NOTIMPL);
}

#[test]
fn gpu_upload_heap_validation_and_storage() {
    let gpu_upload_props = HeapProperties {
        r#type: D3D12_HEAP_TYPE_GPU_UPLOAD,
        cpu_page_property: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        memory_pool_preference: D3D12_MEMORY_POOL_UNKNOWN,
        creation_node_mask: 1,
        visible_node_mask: 1,
    };
    assert!(heap::is_cpu_visible_heap(&gpu_upload_props));
    assert!(!heap::is_private_heap(&gpu_upload_props, true));
    assert_eq!(
        heap::validate_heap_properties(&gpu_upload_props, 0, false),
        S_OK
    );
    assert_eq!(
        heap::validate_heap_properties(&gpu_upload_props, 0, true),
        E_INVALIDARG
    );
    assert_eq!(
        heap::validate_heap_properties(
            &gpu_upload_props,
            D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER,
            false
        ),
        E_INVALIDARG
    );

    let bad_page = HeapProperties {
        cpu_page_property: D3D12_CPU_PAGE_PROPERTY_WRITE_COMBINE,
        ..gpu_upload_props
    };
    assert_eq!(
        heap::validate_heap_properties(&bad_page, 0, false),
        E_INVALIDARG
    );

    let bad_pool = HeapProperties {
        memory_pool_preference: D3D12_MEMORY_POOL_L0,
        ..gpu_upload_props
    };
    assert_eq!(
        heap::validate_heap_properties(&bad_pool, 0, false),
        E_INVALIDARG
    );

    let gpu_upload_desc = HeapDesc {
        size_in_bytes: 5000,
        properties: gpu_upload_props,
        alignment: 0,
        flags: 0,
    };
    assert_eq!(
        heap::plan_heap_allocation(&gpu_upload_desc, true, false),
        HeapAllocChoice::MetalHeap {
            size: 65536,
            options: WMT_RESOURCE_STORAGE_MODE_SHARED,
        }
    );
    assert_eq!(
        heap::plan_heap_allocation(&gpu_upload_desc, true, true),
        HeapAllocChoice::NativeCpuBuffer {
            aligned_size: 65536,
        }
    );

    // SAFETY: Testing `CreateHeap` probe with `GPU_UPLOAD` and null out-pointer returns `S_FALSE`.
    let hr_probe = unsafe {
        heap::create_heap(
            ptr::null_mut(),
            (&raw const gpu_upload_desc).cast(),
            ptr::from_ref(&IID_ID3D12_HEAP).cast(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_probe, S_FALSE);

    let mut numa_dev = crate::device::Device::new_dummy();
    numa_dev.advertise_numa = true;
    let numa_ptr: *mut crate::device::Device = &raw mut *numa_dev;
    // SAFETY: Testing `CreateHeap` with `GPU_UPLOAD` on a NUMA device rejects with `E_INVALIDARG`.
    let hr_numa = unsafe {
        heap::create_heap(
            numa_ptr.cast(),
            (&raw const gpu_upload_desc).cast(),
            ptr::from_ref(&IID_ID3D12_HEAP).cast(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_numa, E_INVALIDARG);
}
