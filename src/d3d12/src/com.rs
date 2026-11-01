//! COM base types, `DeviceChild` ownership, companion vtables, `slot!`, and D3D12 IIDs.

use core::{ffi::c_void, ops::Deref, ptr, ptr::NonNull};

pub use dxmt_util::{
    com::{
        ComObject, ComObjectWithInitialRef, DXGI_ERROR_MORE_DATA, DXGI_ERROR_NOT_FOUND, E_FAIL,
        E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_OUTOFMEMORY, E_POINTER, Guid, Hresult,
        IID_IUNKNOWN, Object, S_FALSE, S_OK, add_ref, qi_log, query, release, vtbl, write_out,
    },
    private_data::PrivateData,
};

use crate::abi::vtbl::IUnknownVtbl;

/// `DXGI_ERROR_INVALID_CALL`.
pub const DXGI_ERROR_INVALID_CALL: Hresult = 0x887a_0001_u32.cast_signed();
/// `DXGI_ERROR_UNSUPPORTED`.
pub const DXGI_ERROR_UNSUPPORTED: Hresult = 0x887a_0004_u32.cast_signed();
/// `DXGI_ERROR_ALREADY_EXISTS`.
pub const DXGI_ERROR_ALREADY_EXISTS: Hresult = 0x887a_0036_u32.cast_signed();

/// `ID3D10Blob` (`d3dcommon.h`).
pub const IID_ID3D10_BLOB: Guid = Guid::new(
    0x8ba5_fb08,
    0x5195,
    0x40e2,
    [0xac, 0x58, 0x0d, 0x98, 0x9c, 0x3a, 0x01, 0x02],
);

/// `ID3D12Object`.
pub const IID_ID3D12_OBJECT: Guid = Guid::new(
    0xc4fe_c28f,
    0x7966,
    0x4e95,
    [0x9f, 0x94, 0xf4, 0x31, 0xcb, 0x56, 0xc3, 0xb8],
);
/// `ID3D12DeviceChild`.
pub const IID_ID3D12_DEVICE_CHILD: Guid = Guid::new(
    0x905d_b94b,
    0xa00c,
    0x4140,
    [0x9d, 0xf5, 0x2b, 0x64, 0xca, 0x9e, 0xa3, 0x57],
);
/// `ID3D12Pageable`.
pub const IID_ID3D12_PAGEABLE: Guid = Guid::new(
    0x63ee_58fb,
    0x1268,
    0x4835,
    [0x86, 0xda, 0xf0, 0x08, 0xce, 0x62, 0xf0, 0xd6],
);
/// `ID3D12RootSignature`.
pub const IID_ID3D12_ROOT_SIGNATURE: Guid = Guid::new(
    0xc54a_6b66,
    0x72df,
    0x4ee8,
    [0x8b, 0xe5, 0xa9, 0x46, 0xa1, 0x42, 0x92, 0x14],
);
/// `ID3D12RootSignatureDeserializer`.
pub const IID_ID3D12_ROOT_SIGNATURE_DESERIALIZER: Guid = Guid::new(
    0x34ab_647b,
    0x3cc8,
    0x46ac,
    [0x84, 0x1b, 0xc0, 0x96, 0x56, 0x45, 0xc0, 0x46],
);
/// `ID3D12VersionedRootSignatureDeserializer`.
pub const IID_ID3D12_VERSIONED_ROOT_SIGNATURE_DESERIALIZER: Guid = Guid::new(
    0x7f91_ce67,
    0x090c,
    0x4bb7,
    [0xb7, 0x8e, 0xed, 0x8f, 0xf2, 0xe3, 0x1d, 0xa0],
);
/// `ID3D12Heap`.
pub const IID_ID3D12_HEAP: Guid = Guid::new(
    0x6b3b_2502,
    0x6e51,
    0x45b3,
    [0x90, 0xee, 0x98, 0x84, 0x26, 0x5e, 0x8d, 0xf3],
);
/// `ID3D12Heap1`.
pub const IID_ID3D12_HEAP1: Guid = Guid::new(
    0x572f_7389,
    0x2168,
    0x49e3,
    [0x96, 0x93, 0xd6, 0xdf, 0x58, 0x71, 0xbf, 0x6d],
);
/// `ID3D12Resource`.
pub const IID_ID3D12_RESOURCE: Guid = Guid::new(
    0x6964_42be,
    0xa72e,
    0x4059,
    [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
);
/// `ID3D12Resource1`.
pub const IID_ID3D12_RESOURCE1: Guid = Guid::new(
    0x9d5e_227a,
    0x4430,
    0x4161,
    [0x88, 0xb3, 0x3e, 0xca, 0x6b, 0xb1, 0x6e, 0x19],
);
/// `ID3D12Resource2`.
pub const IID_ID3D12_RESOURCE2: Guid = Guid::new(
    0xbe36_ec3b,
    0xea85,
    0x4aeb,
    [0xa4, 0x5a, 0xe9, 0xd7, 0x64, 0x04, 0xa4, 0x95],
);
/// `ID3D12CommandAllocator`.
pub const IID_ID3D12_COMMAND_ALLOCATOR: Guid = Guid::new(
    0x6102_dee4,
    0xaf59,
    0x4b09,
    [0xb9, 0x99, 0xb4, 0x4d, 0x73, 0xf0, 0x9b, 0x24],
);
/// `ID3D12Fence`.
pub const IID_ID3D12_FENCE: Guid = Guid::new(
    0x0a75_3dcf,
    0xc4d8,
    0x4b91,
    [0xad, 0xf6, 0xbe, 0x5a, 0x60, 0xd9, 0x5a, 0x76],
);
/// `ID3D12Fence1`.
pub const IID_ID3D12_FENCE1: Guid = Guid::new(
    0x4336_85fe,
    0xe22b,
    0x4ca0,
    [0xa8, 0xdb, 0xb5, 0xb4, 0xf4, 0xdd, 0x0e, 0x4a],
);
/// `ID3D12PipelineState`.
pub const IID_ID3D12_PIPELINE_STATE: Guid = Guid::new(
    0x765a_30f3,
    0xf624,
    0x4c6f,
    [0xa8, 0x28, 0xac, 0xe9, 0x48, 0x62, 0x24, 0x45],
);
/// `ID3D12DescriptorHeap`.
pub const IID_ID3D12_DESCRIPTOR_HEAP: Guid = Guid::new(
    0x8efb_471d,
    0x616c,
    0x4f49,
    [0x90, 0xf7, 0x12, 0x7b, 0xb7, 0x63, 0xfa, 0x51],
);
/// `ID3D12QueryHeap`.
pub const IID_ID3D12_QUERY_HEAP: Guid = Guid::new(
    0x0d96_58ae,
    0xed45,
    0x469e,
    [0xa6, 0x1d, 0x97, 0x0e, 0xc5, 0x83, 0xca, 0xb4],
);
/// `ID3D12CommandSignature`.
pub const IID_ID3D12_COMMAND_SIGNATURE: Guid = Guid::new(
    0xc36a_797c,
    0xec80,
    0x4f0a,
    [0x89, 0x85, 0xa7, 0xb2, 0x47, 0x50, 0x82, 0xd1],
);
/// `ID3D12CommandList`.
pub const IID_ID3D12_COMMAND_LIST: Guid = Guid::new(
    0x7116_d91c,
    0xe7e4,
    0x47ce,
    [0xb8, 0xc6, 0xec, 0x81, 0x68, 0xf4, 0x37, 0xe5],
);
/// `ID3D12GraphicsCommandList`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST: Guid = Guid::new(
    0x5b16_0d0f,
    0xac1b,
    0x4185,
    [0x8b, 0xa8, 0xb3, 0xae, 0x42, 0xa5, 0xa4, 0x55],
);
/// `ID3D12GraphicsCommandList1`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST1: Guid = Guid::new(
    0x5531_03fb,
    0x1fe7,
    0x4557,
    [0xbb, 0x38, 0x94, 0x6d, 0x7d, 0x0e, 0x7c, 0xa7],
);
/// `ID3D12GraphicsCommandList2`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST2: Guid = Guid::new(
    0x38c3_e585,
    0xff17,
    0x412c,
    [0x91, 0x50, 0x4f, 0xc6, 0xf9, 0xd7, 0x2a, 0x28],
);
/// `ID3D12GraphicsCommandList3`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST3: Guid = Guid::new(
    0x6fda_83a7,
    0xb84c,
    0x4e38,
    [0x9a, 0xc8, 0xc7, 0xbd, 0x22, 0x01, 0x6b, 0x3d],
);
/// `ID3D12GraphicsCommandList4`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST4: Guid = Guid::new(
    0x8754_318e,
    0xd3a9,
    0x4541,
    [0x98, 0xcf, 0x64, 0x5b, 0x50, 0xdc, 0x48, 0x74],
);
/// `ID3D12GraphicsCommandList5`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST5: Guid = Guid::new(
    0x5505_0859,
    0x4024,
    0x474c,
    [0x87, 0xf5, 0x64, 0x72, 0xea, 0xee, 0x44, 0xea],
);
/// `ID3D12GraphicsCommandList6`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST6: Guid = Guid::new(
    0xc382_7890,
    0xe548,
    0x4cfa,
    [0x96, 0xcf, 0x56, 0x89, 0xa9, 0x37, 0x0f, 0x80],
);
/// `ID3D12GraphicsCommandList7`.
pub const IID_ID3D12_GRAPHICS_COMMAND_LIST7: Guid = Guid::new(
    0xdd17_1223,
    0x8b61,
    0x4769,
    [0x90, 0xe3, 0x16, 0x0c, 0xcd, 0xe4, 0xe2, 0xc1],
);
/// `ID3D12CommandQueue`.
pub const IID_ID3D12_COMMAND_QUEUE: Guid = Guid::new(
    0x0ec8_70a6,
    0x5d7e,
    0x4c22,
    [0x8c, 0xfc, 0x5b, 0xaa, 0xe0, 0x76, 0x16, 0xed],
);
/// `ID3D12PipelineLibrary`.
pub const IID_ID3D12_PIPELINE_LIBRARY: Guid = Guid::new(
    0xc642_26a8,
    0x9201,
    0x46af,
    [0xb4, 0xcc, 0x53, 0xfb, 0x9f, 0xf7, 0x41, 0x4f],
);
/// `ID3D12PipelineLibrary1`.
pub const IID_ID3D12_PIPELINE_LIBRARY1: Guid = Guid::new(
    0x80ea_bf42,
    0x2568,
    0x4e5e,
    [0xbd, 0x82, 0xc3, 0x7f, 0x86, 0x96, 0x1d, 0xc3],
);
/// `ID3D12Device`.
pub const IID_ID3D12_DEVICE: Guid = Guid::new(
    0x1898_19f1,
    0x1db6,
    0x4b57,
    [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7],
);
/// `ID3D12Device1`.
pub const IID_ID3D12_DEVICE1: Guid = Guid::new(
    0x77ac_ce80,
    0x638e,
    0x4e65,
    [0x88, 0x95, 0xc1, 0xf2, 0x33, 0x86, 0x86, 0x3e],
);
/// `ID3D12Device2`.
pub const IID_ID3D12_DEVICE2: Guid = Guid::new(
    0x30ba_a41e,
    0xb15b,
    0x475c,
    [0xa0, 0xbb, 0x1a, 0xf5, 0xc5, 0xb6, 0x43, 0x28],
);
/// `ID3D12Device3`.
pub const IID_ID3D12_DEVICE3: Guid = Guid::new(
    0x81da_dc15,
    0x2bad,
    0x4392,
    [0x93, 0xc5, 0x10, 0x13, 0x45, 0xc4, 0xaa, 0x98],
);
/// `ID3D12Device4`.
pub const IID_ID3D12_DEVICE4: Guid = Guid::new(
    0xe865_df17,
    0xa9ee,
    0x46f9,
    [0xa4, 0x63, 0x30, 0x98, 0x31, 0x5a, 0xa2, 0xe5],
);
/// `ID3D12Device5`.
pub const IID_ID3D12_DEVICE5: Guid = Guid::new(
    0x8b4f_173b,
    0x2fea,
    0x4b80,
    [0x8f, 0x58, 0x43, 0x07, 0x19, 0x1a, 0xb9, 0x5d],
);
/// `ID3D12Device6`.
pub const IID_ID3D12_DEVICE6: Guid = Guid::new(
    0xc70b_221b,
    0x40e4,
    0x4a17,
    [0x89, 0xaf, 0x02, 0x5a, 0x07, 0x27, 0xa6, 0xdc],
);
/// `ID3D12Device7`.
pub const IID_ID3D12_DEVICE7: Guid = Guid::new(
    0x5c01_4b53,
    0x68a1,
    0x4b9b,
    [0x8b, 0xd1, 0xdd, 0x60, 0x46, 0xb9, 0x35, 0x8b],
);
/// `ID3D12Device8`.
pub const IID_ID3D12_DEVICE8: Guid = Guid::new(
    0x9218_e6bb,
    0xf944,
    0x4f7e,
    [0xa7, 0x5c, 0xb1, 0xb2, 0xc7, 0xb7, 0x01, 0xf3],
);
/// `ID3D12Device9`.
pub const IID_ID3D12_DEVICE9: Guid = Guid::new(
    0x4c80_e962,
    0xf032,
    0x4f60,
    [0xbc, 0x9e, 0xeb, 0xc2, 0xcf, 0xa1, 0xd8, 0x3c],
);
/// `ID3D12Device10`.
pub const IID_ID3D12_DEVICE10: Guid = Guid::new(
    0x517f_8718,
    0xaa66,
    0x49f9,
    [0xb0, 0x2b, 0xa7, 0xab, 0x89, 0xc0, 0x60, 0x31],
);

/// `IDXGIObject`.
pub const IID_IDXGI_OBJECT: Guid = Guid::new(
    0xaec2_2fb8,
    0x76f3,
    0x4639,
    [0x9b, 0xe0, 0x28, 0xeb, 0x43, 0xa6, 0x7a, 0x2e],
);
/// `IDXGIDeviceSubObject`.
pub const IID_IDXGI_DEVICE_SUB_OBJECT: Guid = Guid::new(
    0x3d3e_0379,
    0xf9de,
    0x4d58,
    [0xbb, 0x6c, 0x18, 0xd6, 0x29, 0x92, 0xf1, 0xa6],
);
/// `IDXGISwapChain`.
pub const IID_IDXGI_SWAP_CHAIN: Guid = Guid::new(
    0x310d_36a0,
    0xd2e7,
    0x4c0a,
    [0xaa, 0x04, 0x6a, 0x9d, 0x23, 0xb8, 0x88, 0x6a],
);
/// `IDXGISwapChain1`.
pub const IID_IDXGI_SWAP_CHAIN1: Guid = Guid::new(
    0x790a_45f7,
    0x0d42,
    0x4876,
    [0x98, 0x3a, 0x0a, 0x55, 0xcf, 0xe6, 0xf4, 0xaa],
);
/// `IDXGISwapChain2`.
pub const IID_IDXGI_SWAP_CHAIN2: Guid = Guid::new(
    0xa8be_2ac4,
    0x199f,
    0x4946,
    [0xb3, 0x31, 0x79, 0x59, 0x9f, 0xb9, 0x8d, 0xe7],
);
/// `IDXGISwapChain3`.
pub const IID_IDXGI_SWAP_CHAIN3: Guid = Guid::new(
    0x94d9_9bdb,
    0xf1f8,
    0x4ab0,
    [0xb2, 0x36, 0x7d, 0xa0, 0x17, 0x0e, 0xda, 0xb1],
);
/// `IDXGISwapChain4`.
pub const IID_IDXGI_SWAP_CHAIN4: Guid = Guid::new(
    0x3d58_5d5a,
    0xbd4a,
    0x489e,
    [0xb1, 0xf4, 0x3d, 0xbc, 0xb6, 0x45, 0x2f, 0xfb],
);
/// `IDXGIFactory1`.
pub const IID_IDXGI_FACTORY1: Guid = Guid::new(
    0x770a_ae78,
    0xf26f,
    0x4dba,
    [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87],
);
/// `IDXGIAdapter1`.
pub const IID_IDXGI_ADAPTER1: Guid = Guid::new(
    0x2903_8f61,
    0x3839,
    0x4626,
    [0x91, 0xfd, 0x08, 0x68, 0x79, 0x01, 0x1a, 0x05],
);

/// `WKPDID_D3DDebugObjectNameW` (`d3d12_device_child.hpp:26`).
pub const WKPDID_D3D_DEBUG_OBJECT_NAME_W: Guid = Guid::new(
    0x4cca_5fd8,
    0x921f,
    0x42c8,
    [0x85, 0x66, 0x70, 0xca, 0xf2, 0xa9, 0xb7, 0x41],
);

/// Zeroes `*out` when `out` is non-null (`InitReturnPtr` in `com_pointer.hpp`).
///
/// # Safety
///
/// `out` must be null or point to a writable `*mut c_void`.
pub const unsafe fn init_out_ptr(out: *mut *mut c_void) {
    if !out.is_null() {
        // SAFETY: `out` is non-null and the caller guarantees it points to a writable slot.
        unsafe { out.write_unaligned(ptr::null_mut()) };
    }
}

/// Returns `true` when `ptr` is non-null and its first pointer-sized field equals `expected_vtbl`.
///
/// # Safety
///
/// `ptr` must be null or point to a readable vtable pointer.
#[must_use]
pub unsafe fn has_vtbl<V>(ptr: *const c_void, expected_vtbl: &'static V) -> bool {
    if ptr.is_null() {
        return false;
    }
    // SAFETY: `ptr` is non-null and the caller guarantees its vtable slot is readable.
    let vtbl_ptr = unsafe { ptr.cast::<*const V>().read_unaligned() };
    ptr::eq(vtbl_ptr, expected_vtbl)
}

/// Casts a non-null COM object pointer `ptr` to `&'a T` when its vtable matches `expected_vtbl`.
///
/// # Safety
///
/// `ptr` must be null or a valid COM object pointer, and any live object whose vtable pointer is
/// `expected_vtbl` must be a valid `T` outliving `'a`.
#[must_use]
pub unsafe fn cast_com<'a, T, V>(ptr: *const c_void, expected_vtbl: &'static V) -> Option<&'a T> {
    // SAFETY: forwarded from caller.
    if unsafe { has_vtbl(ptr, expected_vtbl) } {
        // SAFETY: only `T` instances carry `expected_vtbl` at offset 0.
        Some(unsafe { &*ptr.cast::<T>() })
    } else {
        None
    }
}

/// Adjusts a primary COM object pointer `this` forward by `ext_offset` bytes.
///
/// # Safety
///
/// `this` must be a non-null pointer to `T`, and `ext_offset` must be `offset_of!(T, <ext_vtbl>)`.
#[must_use]
pub const unsafe fn to_companion<T>(this: *mut T, ext_offset: usize) -> *mut c_void {
    // SAFETY: `ext_offset` is within the same allocation `T`.
    unsafe { this.cast::<c_void>().byte_add(ext_offset) }
}

/// Adjusts a companion interface pointer `ext_this` backward by `ext_offset` bytes.
///
/// # Safety
///
/// `ext_this` must point to the companion vtable field at byte offset `ext_offset` inside a live
/// `T`.
#[must_use]
pub const unsafe fn from_companion<T>(ext_this: *mut c_void, ext_offset: usize) -> *mut T {
    // SAFETY: `ext_this` was produced by adding `ext_offset` to a `*mut T` within the same
    // allocation.
    unsafe { ext_this.byte_sub(ext_offset).cast::<T>() }
}

/// A heap-allocated COM implementation struct backed by [`ComObject`].
pub trait ComRefCounted: Sized {
    /// Returns the object's two-count reference counter.
    fn com(&self) -> &ComObject;
}

/// Increments the public reference count of `this` (`AddRef`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>`.
pub unsafe fn add_ref_com<T: ComRefCounted>(this: *mut T) -> u32 {
    // SAFETY: `this` is a live `T`.
    let obj = unsafe { &*this };
    obj.com().add_ref()
}

/// Decrements the public reference count of `this` (`Release`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>` whose caller owns one public reference.
pub unsafe fn release_com<T: ComRefCounted>(this: *mut T) -> u32 {
    // SAFETY: `this` is a live `T`.
    let (remaining, free) = unsafe { (*this).com().release_and_free() };
    if free {
        // SAFETY: the last private reference was just released and `DESTROYING_BIT` is set.
        unsafe { drop(Box::from_raw(this)) };
    }
    remaining
}

/// Increments the public reference count of `this` through a companion pointer at `OFFSET`.
///
/// # Safety
///
/// `this` must point to the companion vtable field at byte offset `OFFSET` inside a live `Box<T>`.
pub unsafe fn companion_add_ref<T: ComRefCounted, const OFFSET: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at byte offset `OFFSET` inside a live `T`.
    let base = unsafe { from_companion::<T>(this, OFFSET) };
    // SAFETY: `base` points to a live `Box<T>`.
    unsafe { add_ref_com(base) }
}

/// Decrements the public reference count of `this` through a companion pointer at `OFFSET`.
///
/// # Safety
///
/// `this` must point to the companion vtable field at byte offset `OFFSET` inside a live `Box<T>`
/// whose caller owns one public reference.
pub unsafe fn companion_release<T: ComRefCounted, const OFFSET: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is at byte offset `OFFSET` inside a live `T`.
    let base = unsafe { from_companion::<T>(this, OFFSET) };
    // SAFETY: `base` points to a live `Box<T>`.
    unsafe { release_com(base) }
}

/// Increments the private reference count of `this` (`AddRefPrivate`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>`.
pub unsafe fn add_ref_private_com<T: ComRefCounted>(this: *mut T) {
    // SAFETY: `this` is a live `T`.
    let obj = unsafe { &*this };
    obj.com().add_ref_private();
}

/// Decrements the private reference count of `this` (`ReleasePrivate`).
///
/// # Safety
///
/// `this` must point to a live `Box<T>` whose caller owns one private reference.
pub unsafe fn release_private_com<T: ComRefCounted>(this: *mut T) {
    // SAFETY: `this` is a live `T`.
    let free = unsafe { (*this).com().release_private() };
    if free {
        // SAFETY: the last private reference was just released and `DESTROYING_BIT` is set.
        unsafe { drop(Box::from_raw(this)) };
    }
}

/// Smart pointer holding a public reference to a [`ComRefCounted`] COM object (`Com<T>`).
pub struct ComHold<T: ComRefCounted> {
    ptr: NonNull<T>,
}

// SAFETY: `T` is a COM object with atomic reference counts; sharing or sending `ComHold<T>`
// requires `T: Send + Sync`.
unsafe impl<T: ComRefCounted + Send + Sync> Send for ComHold<T> {}
// SAFETY: see `Send` above.
unsafe impl<T: ComRefCounted + Send + Sync> Sync for ComHold<T> {}

impl<T: ComRefCounted> ComHold<T> {
    /// Acquires a new public reference to `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a live `Box<T>`.
    #[must_use]
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        // SAFETY: `ptr` points to a live `Box<T>`.
        unsafe { add_ref_com(ptr.as_ptr()) };
        Self { ptr }
    }

    /// Raw pointer to the held `T`.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: ComRefCounted> Clone for ComHold<T> {
    fn clone(&self) -> Self {
        // SAFETY: `self.ptr` is kept alive by `self`.
        unsafe { Self::new(self.ptr) }
    }
}

impl<T: ComRefCounted> Deref for ComHold<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `self` holds a live public reference to `self.ptr`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ComRefCounted> Drop for ComHold<T> {
    fn drop(&mut self) {
        // SAFETY: `self` owns one public reference acquired in `new`.
        unsafe { release_com(self.ptr.as_ptr()) };
    }
}

/// Smart pointer holding a private reference to a [`ComRefCounted`] COM object (`Com<T, false>`).
///
/// Used by placed resources (`heap_`) and command lists (`allocator_`, `pso_*`, `rootsig_*`) so an
/// internal hold keeps the target allocation alive after its last public `Release`.
pub struct PrivateHold<T: ComRefCounted> {
    ptr: NonNull<T>,
}

// SAFETY: `T` is a COM object with atomic reference counts; sharing or sending `PrivateHold<T>`
// requires `T: Send + Sync`.
unsafe impl<T: ComRefCounted + Send + Sync> Send for PrivateHold<T> {}
// SAFETY: see `Send` above.
unsafe impl<T: ComRefCounted + Send + Sync> Sync for PrivateHold<T> {}

impl<T: ComRefCounted> PrivateHold<T> {
    /// Acquires a new private reference to `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` must point to a live `Box<T>`.
    #[must_use]
    pub unsafe fn new(ptr: NonNull<T>) -> Self {
        // SAFETY: `ptr` points to a live `Box<T>`.
        unsafe { add_ref_private_com(ptr.as_ptr()) };
        Self { ptr }
    }

    /// Raw pointer to the held `T`.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

impl<T: ComRefCounted> Clone for PrivateHold<T> {
    fn clone(&self) -> Self {
        // SAFETY: `self.ptr` is kept alive by `self`.
        unsafe { Self::new(self.ptr) }
    }
}

impl<T: ComRefCounted> Deref for PrivateHold<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: `self` holds a live private reference to `self.ptr`.
        unsafe { self.ptr.as_ref() }
    }
}

impl<T: ComRefCounted> Drop for PrivateHold<T> {
    fn drop(&mut self) {
        // SAFETY: `self` owns one private reference acquired in `new`.
        unsafe { release_private_com(self.ptr.as_ptr()) };
    }
}

/// Stores `name` under [`WKPDID_D3D_DEBUG_OBJECT_NAME_W`] in `private_data`.
///
/// # Safety
///
/// `name` must be null or point to a NUL-terminated `u16` sequence.
pub unsafe fn set_name_private_data(private_data: &PrivateData, name: *const u16) -> Hresult {
    if name.is_null() {
        // SAFETY: null `data` erases the entry.
        return unsafe { private_data.set_data(WKPDID_D3D_DEBUG_OBJECT_NAME_W, 0, ptr::null()) };
    }
    let mut len = 0usize;
    loop {
        // SAFETY: `name` is non-null and `len` has not yet reached the terminating NUL.
        let ch_ptr = unsafe { name.add(len) };
        // SAFETY: `ch_ptr` points within the caller-supplied NUL-terminated UTF-16 string.
        if unsafe { ch_ptr.read_unaligned() } == 0 {
            break;
        }
        len += 1;
    }
    let byte_len = u32::try_from((len + 1) * size_of::<u16>()).unwrap_or(u32::MAX);
    // SAFETY: `name` points to `byte_len` readable bytes.
    unsafe { private_data.set_data(WKPDID_D3D_DEBUG_OBJECT_NAME_W, byte_len, name.cast()) }
}

/// Common header for `MTLD3D12DeviceChild` classes (`d3d12_device_child.hpp`).
#[repr(C)]
pub struct DeviceChild<D: ComRefCounted> {
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: ComHold<D>,
}

impl<D: ComRefCounted> DeviceChild<D> {
    /// Creates a `DeviceChild` header holding a reference to `device` (`device_->AddRef()`).
    ///
    /// # Safety
    ///
    /// `device` must point to a live `Box<D>`.
    #[must_use]
    pub unsafe fn new(device: NonNull<D>) -> Self {
        // SAFETY: `device` is a live `Box<D>`.
        let device = unsafe { ComHold::new(device) };
        Self {
            com: ComObject::new(),
            private_data: PrivateData::default(),
            device,
        }
    }

    /// `ID3D12Object::GetPrivateData`.
    ///
    /// # Safety
    ///
    /// `guid` and `data_size` must be valid pointers or null; `data` must point to `*data_size`
    /// writable bytes when non-null.
    pub unsafe fn get_private_data(
        &self,
        guid: *const c_void,
        data_size: *mut u32,
        data: *mut c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: forwarded from COM slot with caller's pointers.
        unsafe { self.private_data.get_data(guid, data_size, data) }
    }

    /// `ID3D12Object::SetPrivateData`.
    ///
    /// # Safety
    ///
    /// `guid` must be valid or null; `data` must point to `data_size` readable bytes when
    /// non-null.
    pub unsafe fn set_private_data(
        &self,
        guid: *const c_void,
        data_size: u32,
        data: *const c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: forwarded from COM slot with caller's pointers.
        unsafe { self.private_data.set_data(guid, data_size, data) }
    }

    /// `ID3D12Object::SetPrivateDataInterface`.
    ///
    /// # Safety
    ///
    /// `guid` must be valid or null; `unknown` must be null or a valid `IUnknown` pointer.
    pub unsafe fn set_private_data_interface(
        &self,
        guid: *const c_void,
        unknown: *const c_void,
    ) -> Hresult {
        // SAFETY: `guid` is null or 16 readable bytes.
        let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
            return E_INVALIDARG;
        };
        // SAFETY: forwarded from COM slot with caller's pointers.
        unsafe { self.private_data.set_interface(guid, unknown.cast_mut()) }
    }

    /// `ID3D12Object::SetName` (`d3d12_device_child.hpp:50-56`).
    ///
    /// # Safety
    ///
    /// `name` must be null or point to a NUL-terminated UTF-16 string.
    pub unsafe fn set_name(&self, name: *const u16) -> Hresult {
        // SAFETY: forwarded from COM slot with caller's pointer.
        unsafe { set_name_private_data(&self.private_data, name) }
    }

    /// `ID3D12DeviceChild::GetDevice`: forwards to the parent device's `QueryInterface`.
    ///
    /// # Safety
    ///
    /// `D`'s first field must be a vtable pointer starting with [`IUnknownVtbl`], and `riid` /
    /// `out` must be valid `QueryInterface` arguments.
    pub unsafe fn get_device(&self, riid: *const c_void, out: *mut *mut c_void) -> Hresult {
        let dev_ptr = self.device.as_ptr().cast::<c_void>();
        // SAFETY: `D` is a COM object whose first field is a vtable starting with `IUnknownVtbl`.
        let unk = unsafe { vtbl::<IUnknownVtbl>(dev_ptr) };
        // SAFETY: `dev_ptr` is live (`self.device` holds a reference) and `riid`/`out` are
        // forwarded from the caller.
        unsafe { (unk.query_interface)(dev_ptr, riid, out) }
    }
}

/// Stack reservation for D3D12 worker threads (`dxmt::thread` in `util/thread.hpp:55`, P16).
pub const THREAD_STACK_SIZE: usize = 0x10_0000;

/// Spawns a worker thread named `name` with [`THREAD_STACK_SIZE`] and `AbortOnPanic` (P16, P20).
///
/// # Panics
///
/// Panics if the operating system fails to spawn the thread.
pub fn spawn_thread<F>(name: &str, f: F) -> std::thread::JoinHandle<()>
where
    F: FnOnce() + Send + 'static,
{
    std::thread::Builder::new()
        .name(name.to_owned())
        .stack_size(THREAD_STACK_SIZE)
        .spawn(move || {
            let _guard = dxmt_core::ffi::AbortOnPanic;
            f();
        })
        .expect("failed to spawn D3D12 worker thread")
}

/// Defines an `unsafe extern "system-unwind" fn` COM vtable slot thunk (P9, P20).
#[macro_export]
macro_rules! slot {
    ($scope:literal, ($($arg:ident : $arg_ty:ty),* $(,)?) -> $ret:ty => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) -> $ret {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $crate::scope!($scope);
            $body
        }
        thunk
    }};
    ($scope:literal, ($($arg:ident : $arg_ty:ty),* $(,)?) => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $crate::scope!($scope);
            $body;
        }
        thunk
    }};
    (($($arg:ident : $arg_ty:ty),* $(,)?) -> $ret:ty => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) -> $ret {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $body
        }
        thunk
    }};
    (($($arg:ident : $arg_ty:ty),* $(,)?) => $body:expr) => {{
        unsafe extern "system-unwind" fn thunk($($arg: $arg_ty),*) {
            let _guard = ::dxmt_core::ffi::AbortOnPanic;
            $body;
        }
        thunk
    }};
}

#[cfg(test)]
mod tests;
