use core::{
    alloc::{GlobalAlloc, Layout},
    cell::Cell,
    ffi::c_void,
    mem::size_of,
    ptr,
};
use std::alloc::System;

struct CountingAlloc;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: delegates every allocation call directly to `std::alloc::System`.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNTING.try_with(|on| {
            if on.get() {
                let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
            }
        });
        // SAFETY: forwarded to `System::alloc` with caller's layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let _ = COUNTING.try_with(|on| {
            if on.get() {
                let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
            }
        });
        // SAFETY: forwarded to `System::alloc_zeroed` with caller's layout.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = COUNTING.try_with(|on| {
            if on.get() {
                let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
            }
        });
        // SAFETY: forwarded to `System::realloc` with caller's arguments.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded to `System::dealloc` with caller's arguments.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL_ALLOC: CountingAlloc = CountingAlloc;

pub fn count_allocs<F: FnOnce()>(f: F) -> usize {
    ALLOCS.with(|c| c.set(0));
    COUNTING.with(|on| on.set(true));
    f();
    COUNTING.with(|on| on.set(false));
    ALLOCS.with(Cell::get)
}

#[cfg(not(windows))]
#[unsafe(export_name = "wmt_native_not_implemented")]
extern "C" fn fake_wmt_native_not_implemented() {}

#[unsafe(export_name = "NSObject_retain")]
extern "C-unwind" fn fake_nsobject_retain(_object: u64) {}

#[unsafe(export_name = "NSObject_release")]
extern "C-unwind" fn fake_nsobject_release(_object: u64) {}

#[unsafe(export_name = "NSAutoreleasePool_alloc_init")]
extern "C-unwind" fn fake_autorelease_pool_alloc_init() -> u64 {
    1
}

use super::{
    D3D_FEATURE_LEVEL_11_0, D3D12CreateDevice, D3D12GetDebugInterface, D3D12GetInterface,
    abi::{
        ext::{
            D3D12CommandListExtVtbl, D3D12DeviceExtVtbl, D3D12TemporalScalerVtbl,
            IMTLSwapChainFactoryVtbl,
        },
        vtbl::{
            ID3D10BlobVtbl, ID3D12CommandAllocatorVtbl, ID3D12CommandQueueVtbl,
            ID3D12CommandSignatureVtbl, ID3D12DescriptorHeapVtbl, ID3D12Device10Vtbl,
            ID3D12Fence1Vtbl, ID3D12GraphicsCommandList7Vtbl, ID3D12Heap1Vtbl,
            ID3D12PipelineLibrary1Vtbl, ID3D12PipelineStateVtbl, ID3D12QueryHeapVtbl,
            ID3D12Resource2Vtbl, ID3D12RootSignatureDeserializerVtbl, ID3D12RootSignatureVtbl,
            ID3D12VersionedRootSignatureDeserializerVtbl,
        },
    },
    com::{E_INVALIDARG, E_NOINTERFACE, Guid, IID_ID3D12_DEVICE, IID_IUNKNOWN},
};

#[test]
fn exports_answer_as_cpp_without_a_device() {
    let iid = IID_IUNKNOWN;
    let clsid = Guid::new(1, 2, 3, [4, 5, 6, 7, 8, 9, 10, 11]);
    let mut out: *mut c_void = ptr::dangling_mut();

    // SAFETY: `iid` and `out` are valid stack pointers.
    let hr = unsafe { D3D12GetDebugInterface(&raw const iid, &raw mut out) };
    assert_eq!(hr, E_NOINTERFACE);

    // SAFETY: `clsid`, `iid`, and `out` are valid stack pointers.
    let hr = unsafe { D3D12GetInterface(&raw const clsid, &raw const iid, &raw mut out) };
    assert_eq!(hr, E_NOINTERFACE);

    // Feature level below `D3D_FEATURE_LEVEL_11_0` (`0xb000`) returns `E_INVALIDARG` before
    // touching the adapter or creating a device (`d3d12.cpp:38-39`).
    let dev_iid = IID_ID3D12_DEVICE;
    let mut dev_out: *mut c_void = ptr::null_mut();
    // SAFETY: feature level `< D3D_FEATURE_LEVEL_11_0` returns early.
    let hr = unsafe {
        D3D12CreateDevice(
            ptr::null_mut(),
            D3D_FEATURE_LEVEL_11_0 - 0x100,
            &raw const dev_iid,
            &raw mut dev_out,
        )
    };
    assert_eq!(hr, E_INVALIDARG);
}

#[test]
fn vtables_have_the_cpp_slot_counts() {
    const PTR: usize = size_of::<usize>();
    assert_eq!(size_of::<ID3D10BlobVtbl>(), 5 * PTR);
    assert_eq!(size_of::<ID3D12RootSignatureVtbl>(), 8 * PTR);
    assert_eq!(size_of::<ID3D12RootSignatureDeserializerVtbl>(), 4 * PTR);
    assert_eq!(
        size_of::<ID3D12VersionedRootSignatureDeserializerVtbl>(),
        5 * PTR
    );
    assert_eq!(size_of::<ID3D12Heap1Vtbl>(), 10 * PTR);
    assert_eq!(size_of::<ID3D12Resource2Vtbl>(), 17 * PTR);
    assert_eq!(size_of::<ID3D12CommandAllocatorVtbl>(), 9 * PTR);
    assert_eq!(size_of::<ID3D12Fence1Vtbl>(), 12 * PTR);
    assert_eq!(size_of::<ID3D12PipelineStateVtbl>(), 9 * PTR);
    assert_eq!(size_of::<ID3D12PipelineLibrary1Vtbl>(), 14 * PTR);
    assert_eq!(size_of::<ID3D12DescriptorHeapVtbl>(), 11 * PTR);
    assert_eq!(size_of::<ID3D12QueryHeapVtbl>(), 8 * PTR);
    assert_eq!(size_of::<ID3D12CommandSignatureVtbl>(), 8 * PTR);
    assert_eq!(size_of::<ID3D12GraphicsCommandList7Vtbl>(), 81 * PTR);
    assert_eq!(size_of::<ID3D12CommandQueueVtbl>(), 19 * PTR);
    assert_eq!(size_of::<ID3D12Device10Vtbl>(), 79 * PTR);
    assert_eq!(size_of::<D3D12DeviceExtVtbl>(), 5 * PTR);
    assert_eq!(size_of::<D3D12CommandListExtVtbl>(), 4 * PTR);
    assert_eq!(size_of::<D3D12TemporalScalerVtbl>(), 3 * PTR);
    assert_eq!(size_of::<IMTLSwapChainFactoryVtbl>(), 4 * PTR);
}
