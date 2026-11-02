//! `ID3D12RootSignature` (`d3d12_root_signature.cpp:347-527`, `d3d12_device.hpp:184-205`).

use core::{
    ffi::c_void,
    mem::{ManuallyDrop, zeroed},
    ptr,
};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use airconv_ir::root_signature::Payload;
use dxmt_core::sampler::Sampler;
use dxmt_util::{com::ComObject, private_data::PrivateData};
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;
use winemetal_wire::WMTSamplerInfo;

use crate::{
    abi::{
        root_signature::{
            D3D12_DESCRIPTOR_RANGE_TYPE_CBV, D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
            D3D12_DESCRIPTOR_RANGE_TYPE_SRV, D3D12_DESCRIPTOR_RANGE_TYPE_UAV, RootSignatureDesc1,
        },
        sampler::StaticSamplerDesc as SamplerStaticSamplerDesc,
        vtbl::{ID3D12RootSignatureVtbl, IUnknownVtbl},
    },
    com::{
        self, ComRefCounted, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult,
        IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT, IID_ID3D12_ROOT_SIGNATURE, IID_IUNKNOWN, S_OK,
    },
    device::Device,
    root_signature::{
        QwordOffsets,
        deserializer::{ConvertedRootSignature, parse_rts0_slice},
    },
    shader_cache::Sha1Digest,
    slot,
};

const D3D12_MAX_ROOT_COST: usize = 64;

type SamplerList = Box<[Arc<Sampler>]>;
type LiveSamplerMap = FxHashMap<usize, SamplerList>;

static LIVE_SAMPLERS: LazyLock<Mutex<LiveSamplerMap>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// `MTLD3D12RootSignatureImpl` (`d3d12_device.hpp:184-205`, `d3d12_root_signature.cpp:347`).
#[repr(C)]
pub struct RootSignature {
    pub vtbl: &'static ID3D12RootSignatureVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub blob: Box<[u8]>,
    pub qword_offsets: QwordOffsets,
    pub static_samplers: Box<[u64]>,
    pub flags: u32,
    pub sha1: Sha1Digest,
}

impl ComRefCounted for RootSignature {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for RootSignature {
    fn drop(&mut self) {
        LIVE_SAMPLERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&ptr::from_ref(self).addr());
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained by `add_ref_com` when `RootSignature` was created.
            unsafe { com::release_com(self.device) };
        }
    }
}

impl RootSignature {
    /// Total qwords uploaded to the root-argument table per draw/dispatch (`UploadQwords`).
    #[must_use]
    pub const fn upload_qwords(&self) -> u32 {
        self.qword_offsets.total
    }

    /// Number of root parameter slots (`ParameterSlots`).
    #[must_use]
    pub fn parameter_slots(&self) -> u32 {
        u32::try_from(self.qword_offsets.parameters.len()).unwrap_or(0)
    }

    /// Qword offset of each root parameter slot (`SlotQwordOffsets`).
    #[must_use]
    pub fn slot_qword_offsets(&self) -> &[u32] {
        &self.qword_offsets.parameters
    }

    /// Qword offset of the directly-indexed heap pair (`~0u` when absent; `HeapQwordOffset`).
    #[must_use]
    pub const fn heap_qword_offset(&self) -> u32 {
        self.qword_offsets.heaps
    }

    /// Qword offset of the 64-bit atomic lock table (`LockQwordOffset`).
    #[must_use]
    pub const fn lock_qword_offset(&self) -> u32 {
        self.qword_offsets.lock
    }

    /// Encoded static sampler descriptors (4 qwords per static sampler).
    #[must_use]
    pub fn encoded_static_samplers(&self) -> &[u64] {
        &self.static_samplers
    }
}

/// Shared `rootsig.QueryInterface` implementation.
///
/// Used by `ID3D12RootSignature`, `ID3D12RootSignatureDeserializer`, and
/// `ID3D12VersionedRootSignatureDeserializer`.
///
/// # Safety
///
/// `this` must be a live COM object whose `ComObject` is reachable via `add_ref`; `riid` and
/// `ppv_object` are caller-supplied `QueryInterface` arguments.
#[inline(never)]
pub unsafe fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
    accepted: &[Guid],
    add_ref: unsafe fn(*mut c_void),
    warn_unknown: bool,
) -> Hresult {
    crate::scope!("rootsig.QueryInterface");
    if ppv_object.is_null() {
        return E_POINTER;
    }
    // SAFETY: `ppv_object` is non-null.
    unsafe { *ppv_object = ptr::null_mut() };
    // SAFETY: `riid` is an IID pointer when non-null.
    let Some(iid) = (unsafe { riid.cast::<Guid>().as_ref() }) else {
        return E_NOINTERFACE;
    };
    if accepted.contains(iid) {
        // SAFETY: `this` is a live COM object of the caller's type.
        unsafe { add_ref(this) };
        // SAFETY: `ppv_object` is non-null.
        unsafe { *ppv_object = this };
        return S_OK;
    }
    if warn_unknown {
        crate::warn!("D3D12RootSignature: Unknown interface query {iid}");
    }
    E_NOINTERFACE
}

unsafe fn add_ref_root_signature(this: *mut c_void) {
    // SAFETY: `this` is a live `RootSignature`.
    unsafe { com::add_ref_com(this.cast::<RootSignature>()) };
}

static ROOT_SIGNATURE_VTBL: ID3D12RootSignatureVtbl = ID3D12RootSignatureVtbl {
    query_interface: slot!(
        (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `RootSignature`.
            unsafe {
                query_interface(
                    this,
                    riid,
                    ppv_object,
                    &[
                        IID_IUNKNOWN,
                        IID_ID3D12_OBJECT,
                        IID_ID3D12_DEVICE_CHILD,
                        IID_ID3D12_ROOT_SIGNATURE,
                    ],
                    add_ref_root_signature,
                    true,
                )
            }
        }
    ),
    add_ref: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `RootSignature`.
        unsafe { com::add_ref_com(this.cast::<RootSignature>()) }
    }),
    release: slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `RootSignature`.
        unsafe { com::release_com(this.cast::<RootSignature>()) }
    }),
    get_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: `this` is a live `RootSignature`.
            let private_data = unsafe { &(*this.cast::<RootSignature>()).private_data };
            // SAFETY: `data_size` and `data` are caller pointers.
            unsafe { private_data.get_data(guid, data_size, data) }
        }
    ),
    set_private_data: slot!(
        (this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: `this` is a live `RootSignature`.
            let private_data = unsafe { &(*this.cast::<RootSignature>()).private_data };
            // SAFETY: `data` is a caller pointer.
            unsafe { private_data.set_data(guid, data_size, data) }
        }
    ),
    set_private_data_interface: slot!(
        (this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: `this` is a live `RootSignature`.
            let private_data = unsafe { &(*this.cast::<RootSignature>()).private_data };
            // SAFETY: `data` is a caller `IUnknown` pointer.
            unsafe { private_data.set_interface(guid, data.cast_mut()) }
        }
    ),
    set_name: slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: `this` is a live `RootSignature`.
        let private_data = unsafe { &(*this.cast::<RootSignature>()).private_data };
        // SAFETY: `name` is a caller UTF-16 string pointer.
        unsafe { com::set_name_private_data(private_data, name.cast()) }
    }),
    get_device: slot!(
        (this: *mut c_void, riid: *const c_void, ppv_device: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `RootSignature`.
            let dev_ptr = unsafe { (*this.cast::<RootSignature>()).device }.cast::<c_void>();
            if dev_ptr.is_null() {
                // SAFETY: `ppv_device` is a caller out-pointer (or null).
                unsafe { com::init_out_ptr(ppv_device) };
                return E_FAIL;
            }
            // SAFETY: `dev_ptr` is a live `Device` whose first field is a vtable starting with `IUnknownVtbl`.
            let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev_ptr) };
            // SAFETY: `dev_ptr` is live and `riid`/`ppv_device` are forwarded from the caller.
            unsafe { (unk.query_interface)(dev_ptr, riid, ppv_device) }
        }
    ),
};

/// Casts a non-null `ID3D12RootSignature` pointer if its vtable matches [`RootSignature`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_root_signature<'a>(raw: *mut c_void) -> Option<&'a RootSignature> {
    // SAFETY: `raw` is a readable COM object pointer if non-null; on match it is a `RootSignature`.
    unsafe {
        com::cast_com::<RootSignature, ID3D12RootSignatureVtbl>(raw, &ROOT_SIGNATURE_VTBL)
            .map(|ptr| ptr as &RootSignature)
    }
}

/// `ID3D12Device::CreateRootSignature` (`d3d12_device.cpp:829`, `d3d12_root_signature.cpp:508`).
///
/// # Safety
///
/// `this`, `blob`, `riid`, and `root_sig` are caller-supplied pointers.
pub unsafe fn create_root_signature(
    this: *mut c_void,
    _node_mask: u32,
    blob: *const c_void,
    length: usize,
    riid: *const c_void,
    root_sig: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateRootSignature");
    // SAFETY: `root_sig` is a caller out-pointer (or null).
    unsafe { com::init_out_ptr(root_sig) };
    if blob.is_null() || length == 0 {
        return E_FAIL;
    }
    // SAFETY: `blob` is non-null and holds `length` readable bytes.
    let bytes = unsafe { core::slice::from_raw_parts(blob.cast::<u8>(), length) };
    let ir = match parse_rts0_slice(bytes) {
        Ok(ir) => ir,
        Err(hr) => return hr,
    };

    if ir.v1_1.parameters.len() > D3D12_MAX_ROOT_COST {
        return E_INVALIDARG;
    }
    for param in &ir.v1_1.parameters {
        if let Payload::Table(ranges) = &param.payload {
            let mut is_normal_range = false;
            let mut is_sampler_range = false;
            for range in ranges {
                match range.range_type {
                    D3D12_DESCRIPTOR_RANGE_TYPE_SRV
                    | D3D12_DESCRIPTOR_RANGE_TYPE_UAV
                    | D3D12_DESCRIPTOR_RANGE_TYPE_CBV => {
                        is_normal_range = true;
                    }
                    D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER => {
                        is_sampler_range = true;
                    }
                    _ => return E_INVALIDARG,
                }
            }
            if is_normal_range && is_sampler_range {
                return E_INVALIDARG;
            }
        }
    }

    let converted = ConvertedRootSignature::from_ir(&ir);
    // SAFETY: `converted.desc_1_1` was initialized with the `desc_1_1` union arm.
    let desc_1_1: &RootSignatureDesc1 = unsafe { &converted.desc_1_1.desc.desc_1_1 };

    let sampler_count = usize::try_from(desc_1_1.num_static_samplers).unwrap_or(0);
    let mut static_samplers = Vec::with_capacity(sampler_count * 4);
    let mut live_samplers = Vec::with_capacity(sampler_count);
    if sampler_count > 0 {
        // SAFETY: `desc_1_1.p_static_samplers` points to `sampler_count` static sampler descriptors.
        let descs =
            unsafe { core::slice::from_raw_parts(desc_1_1.p_static_samplers, sampler_count) };
        let device_mtl = if this.is_null() {
            None
        } else {
            // SAFETY: `this` is a valid `Device` pointer when non-null.
            unsafe { (*this.cast::<Device>()).mtl.as_ref() }
        };
        // SAFETY: dummy handle `1` wrapped in `ManuallyDrop` is never released via `NSObject_release`.
        let fallback_mtl = unsafe { Obj::adopt(1) }.map(ManuallyDrop::new);
        let Some(mtl_ref) = device_mtl.or(fallback_mtl.as_deref()) else {
            return E_FAIL;
        };

        for sampler_desc in descs {
            // SAFETY: `WMTSamplerInfo` is a plain C struct of integer and float fields.
            let mut info: WMTSamplerInfo = unsafe { zeroed() };
            let abi_sampler = SamplerStaticSamplerDesc {
                filter: sampler_desc.filter,
                address_u: sampler_desc.address_u,
                address_v: sampler_desc.address_v,
                address_w: sampler_desc.address_w,
                mip_lod_bias: sampler_desc.mip_lod_bias,
                max_anisotropy: sampler_desc.max_anisotropy,
                comparison_func: sampler_desc.comparison_func,
                border_color: sampler_desc.border_color,
                min_lod: sampler_desc.min_lod,
                max_lod: sampler_desc.max_lod,
                shader_register: sampler_desc.shader_register,
                register_space: sampler_desc.register_space,
                shader_visibility: sampler_desc.shader_visibility,
            };
            crate::sampler::populate_static_sampler_info(&mut info, &abi_sampler, &mut |w| {
                crate::warn!("{w}");
            });
            if let Some(sampler) = Sampler::create(mtl_ref, &info, sampler_desc.mip_lod_bias) {
                static_samplers.push(sampler.gpu_resource_id());
                static_samplers.push(sampler.gpu_resource_id_cube());
                static_samplers.push(u64::from(sampler.lod_bias().to_bits()));
                static_samplers.push(0);
                live_samplers.push(sampler);
            } else {
                static_samplers.push(0);
                static_samplers.push(0);
                static_samplers.push(u64::from(sampler_desc.mip_lod_bias.to_bits()));
                static_samplers.push(0);
            }
        }
    }

    // SAFETY: `desc_1_1.p_parameters` points to `desc_1_1.num_parameters` valid `RootParameter1`s.
    let qword_offsets = match unsafe { super::qword_offsets(desc_1_1) } {
        Ok(offsets) => offsets,
        Err(hr) => return hr,
    };
    let sha1 = crate::shader_cache::sha1(bytes);
    crate::capture::dump_root_signature(bytes);

    let device = this.cast::<Device>();
    if !device.is_null() {
        // SAFETY: `device` is a live `Device` pointer.
        unsafe { com::add_ref_com(device) };
    }
    let raw = Box::into_raw(Box::new(RootSignature {
        vtbl: &ROOT_SIGNATURE_VTBL,
        com: ComObject::new(),
        private_data: PrivateData::default(),
        device,
        blob: bytes.into(),
        qword_offsets,
        static_samplers: static_samplers.into_boxed_slice(),
        flags: desc_1_1.flags,
        sha1,
    }));
    if !live_samplers.is_empty() {
        LIVE_SAMPLERS
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(raw.addr(), live_samplers.into_boxed_slice());
    }

    // SAFETY: `raw` is a valid `RootSignature`; `riid` and `root_sig` are caller pointers.
    let hr = unsafe { (ROOT_SIGNATURE_VTBL.query_interface)(raw.cast(), riid, root_sig) };
    if hr < 0 {
        // SAFETY: `QueryInterface` did not increment `raw`'s refcount; reclaim the allocation.
        drop(unsafe { Box::from_raw(raw) });
    }
    hr
}

#[cfg(test)]
mod tests;
