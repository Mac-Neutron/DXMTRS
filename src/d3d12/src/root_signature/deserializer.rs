//! `ID3D12RootSignatureDeserializer` and `ID3D12VersionedRootSignatureDeserializer`
//!
//! (`d3d12_root_signature.cpp:178-345`).

use core::{ffi::c_void, mem::ManuallyDrop, ptr};

use airconv_ir::{
    container::{Code, Container},
    root_signature::{
        Descriptor, Payload, Range, RootSignature as IrRootSignature, StaticSampler, from_container,
    },
};
use dxmt_util::com::ComObject;

use crate::{
    abi::{
        root_signature::{
            D3D_ROOT_SIGNATURE_VERSION_1_0, D3D_ROOT_SIGNATURE_VERSION_1_1, DescriptorRange,
            DescriptorRange1, RootConstants, RootDescriptor, RootDescriptor1, RootDescriptorTable,
            RootDescriptorTable1, RootParameter, RootParameter1, RootParameter1Payload,
            RootParameterPayload, RootSignatureDesc, RootSignatureDesc1, StaticSamplerDesc,
            VersionedDesc, VersionedRootSignatureDesc,
        },
        vtbl::{ID3D12RootSignatureDeserializerVtbl, ID3D12VersionedRootSignatureDeserializerVtbl},
    },
    com::{
        self, ComRefCounted, E_FAIL, E_INVALIDARG, E_OUTOFMEMORY, Guid, Hresult,
        IID_ID3D12_ROOT_SIGNATURE_DESERIALIZER, IID_ID3D12_VERSIONED_ROOT_SIGNATURE_DESERIALIZER,
        IID_IUNKNOWN, S_OK,
    },
    slot,
};

/// Owned 1.0 and 1.1 `D3D12_VERSIONED_ROOT_SIGNATURE_DESC` trees with stable heap backing arrays.
pub struct ConvertedRootSignature {
    _samplers: Box<[StaticSamplerDesc]>,
    _ranges_1_0: Box<[Box<[DescriptorRange]>]>,
    _params_1_0: Box<[RootParameter]>,
    _ranges_1_1: Box<[Box<[DescriptorRange1]>]>,
    _params_1_1: Box<[RootParameter1]>,
    pub desc_1_0: VersionedRootSignatureDesc,
    pub desc_1_1: VersionedRootSignatureDesc,
}

impl ConvertedRootSignature {
    /// Converts a deserialized `airconv_ir` root signature into owned 1.0 and 1.1 ABI structs.
    #[must_use]
    pub fn from_ir(ir: &IrRootSignature) -> Self {
        let samplers: Box<[StaticSamplerDesc]> = ir
            .static_samplers
            .iter()
            .map(convert_static_sampler)
            .collect();
        let samplers_ptr = if samplers.is_empty() {
            ptr::null()
        } else {
            samplers.as_ptr()
        };
        let num_samplers = u32::try_from(samplers.len()).unwrap_or(u32::MAX);

        let mut ranges_1_0 = Vec::new();
        let mut params_1_0 = Vec::with_capacity(ir.v1_0.parameters.len());
        for param in &ir.v1_0.parameters {
            let payload = match &param.payload {
                Payload::Table(ranges) => {
                    let converted: Box<[DescriptorRange]> =
                        ranges.iter().map(convert_range_0).collect();
                    let ptr = if converted.is_empty() {
                        ptr::null()
                    } else {
                        converted.as_ptr()
                    };
                    let count = u32::try_from(converted.len()).unwrap_or(u32::MAX);
                    ranges_1_0.push(converted);
                    RootParameterPayload {
                        descriptor_table: ManuallyDrop::new(RootDescriptorTable {
                            num_descriptor_ranges: count,
                            p_descriptor_ranges: ptr,
                        }),
                    }
                }
                Payload::Constants(c) => RootParameterPayload {
                    constants: ManuallyDrop::new(RootConstants {
                        shader_register: c.register,
                        register_space: c.space,
                        num_32bit_values: c.values,
                    }),
                },
                Payload::Cbv(Descriptor {
                    register, space, ..
                })
                | Payload::Srv(Descriptor {
                    register, space, ..
                })
                | Payload::Uav(Descriptor {
                    register, space, ..
                }) => RootParameterPayload {
                    descriptor: ManuallyDrop::new(RootDescriptor {
                        shader_register: *register,
                        register_space: *space,
                    }),
                },
                Payload::Unknown(_) => RootParameterPayload {
                    constants: ManuallyDrop::new(RootConstants {
                        shader_register: 0,
                        register_space: 0,
                        num_32bit_values: 0,
                    }),
                },
            };
            params_1_0.push(RootParameter {
                parameter_type: param.parameter_type(),
                payload,
                shader_visibility: param.visibility,
            });
        }
        let ranges_1_0 = ranges_1_0.into_boxed_slice();
        let params_1_0 = params_1_0.into_boxed_slice();
        let params_1_0_ptr = if params_1_0.is_empty() {
            ptr::null()
        } else {
            params_1_0.as_ptr()
        };
        let num_params_1_0 = u32::try_from(params_1_0.len()).unwrap_or(u32::MAX);

        let mut ranges_1_1 = Vec::new();
        let mut params_1_1 = Vec::with_capacity(ir.v1_1.parameters.len());
        for param in &ir.v1_1.parameters {
            let payload = match &param.payload {
                Payload::Table(ranges) => {
                    let converted: Box<[DescriptorRange1]> =
                        ranges.iter().map(convert_range_1).collect();
                    let ptr = if converted.is_empty() {
                        ptr::null()
                    } else {
                        converted.as_ptr()
                    };
                    let count = u32::try_from(converted.len()).unwrap_or(u32::MAX);
                    ranges_1_1.push(converted);
                    RootParameter1Payload {
                        descriptor_table: ManuallyDrop::new(RootDescriptorTable1 {
                            num_descriptor_ranges: count,
                            p_descriptor_ranges: ptr,
                        }),
                    }
                }
                Payload::Constants(c) => RootParameter1Payload {
                    constants: ManuallyDrop::new(RootConstants {
                        shader_register: c.register,
                        register_space: c.space,
                        num_32bit_values: c.values,
                    }),
                },
                Payload::Cbv(Descriptor {
                    register,
                    space,
                    flags,
                })
                | Payload::Srv(Descriptor {
                    register,
                    space,
                    flags,
                })
                | Payload::Uav(Descriptor {
                    register,
                    space,
                    flags,
                }) => RootParameter1Payload {
                    descriptor: ManuallyDrop::new(RootDescriptor1 {
                        shader_register: *register,
                        register_space: *space,
                        flags: *flags,
                    }),
                },
                Payload::Unknown(_) => RootParameter1Payload {
                    constants: ManuallyDrop::new(RootConstants {
                        shader_register: 0,
                        register_space: 0,
                        num_32bit_values: 0,
                    }),
                },
            };
            params_1_1.push(RootParameter1 {
                parameter_type: param.parameter_type(),
                payload,
                shader_visibility: param.visibility,
            });
        }
        let ranges_1_1 = ranges_1_1.into_boxed_slice();
        let params_1_1 = params_1_1.into_boxed_slice();
        let params_1_1_ptr = if params_1_1.is_empty() {
            ptr::null()
        } else {
            params_1_1.as_ptr()
        };
        let num_params_1_1 = u32::try_from(params_1_1.len()).unwrap_or(u32::MAX);

        let desc_1_0 = VersionedRootSignatureDesc {
            version: D3D_ROOT_SIGNATURE_VERSION_1_0,
            desc: VersionedDesc {
                desc_1_0: ManuallyDrop::new(RootSignatureDesc {
                    num_parameters: num_params_1_0,
                    p_parameters: params_1_0_ptr,
                    num_static_samplers: num_samplers,
                    p_static_samplers: samplers_ptr,
                    flags: ir.v1_0.flags,
                }),
            },
        };
        let desc_1_1 = VersionedRootSignatureDesc {
            version: D3D_ROOT_SIGNATURE_VERSION_1_1,
            desc: VersionedDesc {
                desc_1_1: ManuallyDrop::new(RootSignatureDesc1 {
                    num_parameters: num_params_1_1,
                    p_parameters: params_1_1_ptr,
                    num_static_samplers: num_samplers,
                    p_static_samplers: samplers_ptr,
                    flags: ir.v1_1.flags,
                }),
            },
        };

        Self {
            _samplers: samplers,
            _ranges_1_0: ranges_1_0,
            _params_1_0: params_1_0,
            _ranges_1_1: ranges_1_1,
            _params_1_1: params_1_1,
            desc_1_0,
            desc_1_1,
        }
    }
}

const fn convert_static_sampler(s: &StaticSampler) -> StaticSamplerDesc {
    StaticSamplerDesc {
        filter: s.filter,
        address_u: s.address_u,
        address_v: s.address_v,
        address_w: s.address_w,
        mip_lod_bias: s.mip_lod_bias,
        max_anisotropy: s.max_anisotropy,
        comparison_func: s.comparison,
        border_color: s.border_color,
        min_lod: s.min_lod,
        max_lod: s.max_lod,
        shader_register: s.register,
        register_space: s.space,
        shader_visibility: s.visibility,
    }
}

const fn convert_range_0(r: &Range) -> DescriptorRange {
    DescriptorRange {
        range_type: r.range_type,
        num_descriptors: r.num_descriptors,
        base_shader_register: r.base_register,
        register_space: r.space,
        offset_in_descriptors_from_table_start: r.offset,
    }
}

const fn convert_range_1(r: &Range) -> DescriptorRange1 {
    DescriptorRange1 {
        range_type: r.range_type,
        num_descriptors: r.num_descriptors,
        base_shader_register: r.base_register,
        register_space: r.space,
        flags: r.flags,
        offset_in_descriptors_from_table_start: r.offset,
    }
}

/// Validates a DXBC container and deserializes its `RTS0` chunk.
///
/// (`d3d12_root_signature.cpp:289-302`).
///
/// # Errors
///
/// `E_FAIL` when the container header or `RTS0` chunk is invalid; `E_INVALIDARG` or
/// `E_OUTOFMEMORY` when `RTS0` deserialization refuses the record layout.
pub fn parse_rts0_slice(bytes: &[u8]) -> Result<IrRootSignature, Hresult> {
    let declared = Container::declared_size(bytes)
        .filter(|&n| n <= bytes.len())
        .ok_or(E_FAIL)?;
    let container = Container::parse(&bytes[..declared]).map_err(|_| E_FAIL)?;
    let rts0 = from_container(&container).map_err(|_| E_FAIL)?;
    IrRootSignature::deserialize(rts0).map_err(|err| match err.code {
        Code::Fail => E_FAIL,
        Code::InvalidArg => E_INVALIDARG,
        Code::OutOfMemory => E_OUTOFMEMORY,
    })
}

#[repr(C)]
struct RootSignatureDeserializer {
    vtbl: &'static ID3D12RootSignatureDeserializerVtbl,
    com: ComObject,
    converted: ConvertedRootSignature,
}

impl ComRefCounted for RootSignatureDeserializer {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

unsafe fn add_ref_deserializer(this: *mut c_void) {
    // SAFETY: `this` is a live `RootSignatureDeserializer`.
    unsafe { com::add_ref_com(this.cast::<RootSignatureDeserializer>()) };
}

static DESERIALIZER_VTBL: ID3D12RootSignatureDeserializerVtbl =
    ID3D12RootSignatureDeserializerVtbl {
        query_interface: slot!(
            (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
                // SAFETY: `this` is a live `RootSignatureDeserializer`.
                unsafe {
                    super::object::query_interface(
                        this,
                        riid,
                        ppv_object,
                        &[IID_IUNKNOWN, IID_ID3D12_ROOT_SIGNATURE_DESERIALIZER],
                        add_ref_deserializer,
                        false,
                    )
                }
            }
        ),
        add_ref: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `RootSignatureDeserializer`.
            unsafe { com::add_ref_com(this.cast::<RootSignatureDeserializer>()) }
        }),
        release: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `RootSignatureDeserializer`.
            unsafe { com::release_com(this.cast::<RootSignatureDeserializer>()) }
        }),
        get_root_signature_desc: slot!(
            "rootsig.GetRootSignatureDesc",
            (this: *mut c_void) -> *const c_void => {
                // SAFETY: `this` is a live `RootSignatureDeserializer`.
                let converted = unsafe { &(*this.cast::<RootSignatureDeserializer>()).converted };
                // SAFETY: `converted.desc_1_0` is initialized as version 1.0.
                let desc_1_0: &RootSignatureDesc = unsafe { &converted.desc_1_0.desc.desc_1_0 };
                ptr::from_ref(desc_1_0).cast()
            }
        ),
    };

#[repr(C)]
struct VersionedRootSignatureDeserializer {
    vtbl: &'static ID3D12VersionedRootSignatureDeserializerVtbl,
    com: ComObject,
    unconverted_version: u32,
    converted: ConvertedRootSignature,
}

impl ComRefCounted for VersionedRootSignatureDeserializer {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

unsafe fn add_ref_versioned_deserializer(this: *mut c_void) {
    // SAFETY: `this` is a live `VersionedRootSignatureDeserializer`.
    unsafe { com::add_ref_com(this.cast::<VersionedRootSignatureDeserializer>()) };
}

static VERSIONED_DESERIALIZER_VTBL: ID3D12VersionedRootSignatureDeserializerVtbl =
    ID3D12VersionedRootSignatureDeserializerVtbl {
        query_interface: slot!(
            (this: *mut c_void, riid: *const c_void, ppv_object: *mut *mut c_void) -> Hresult => {
                // SAFETY: `this` is a live `VersionedRootSignatureDeserializer`.
                unsafe {
                    super::object::query_interface(
                        this,
                        riid,
                        ppv_object,
                        &[IID_IUNKNOWN, IID_ID3D12_VERSIONED_ROOT_SIGNATURE_DESERIALIZER],
                        add_ref_versioned_deserializer,
                        false,
                    )
                }
            }
        ),
        add_ref: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `VersionedRootSignatureDeserializer`.
            unsafe { com::add_ref_com(this.cast::<VersionedRootSignatureDeserializer>()) }
        }),
        release: slot!((this: *mut c_void) -> u32 => {
            // SAFETY: `this` is a live `VersionedRootSignatureDeserializer`.
            unsafe { com::release_com(this.cast::<VersionedRootSignatureDeserializer>()) }
        }),
        get_root_signature_desc_at_version: slot!(
            "rootsig.GetRootSignatureDescAtVersion",
            (this: *mut c_void, convert_to_version: u32, pp_desc: *const *mut c_void) -> Hresult => {
                let pp_desc = pp_desc.cast_mut();
                if pp_desc.is_null() {
                    return E_INVALIDARG;
                }
                // SAFETY: `this` is a live `VersionedRootSignatureDeserializer`.
                let deser = unsafe { &*this.cast::<VersionedRootSignatureDeserializer>() };
                match convert_to_version {
                    D3D_ROOT_SIGNATURE_VERSION_1_0 => {
                        // SAFETY: `pp_desc` is a non-null writable out-pointer.
                        unsafe { *pp_desc = ptr::from_ref(&deser.converted.desc_1_0).cast_mut().cast() };
                        S_OK
                    }
                    D3D_ROOT_SIGNATURE_VERSION_1_1 => {
                        // SAFETY: `pp_desc` is a non-null writable out-pointer.
                        unsafe { *pp_desc = ptr::from_ref(&deser.converted.desc_1_1).cast_mut().cast() };
                        S_OK
                    }
                    _ => E_INVALIDARG,
                }
            }
        ),
        get_unconverted_root_signature_desc: slot!(
            "rootsig.GetUnconvertedRootSignatureDesc",
            (this: *mut c_void) -> *const c_void => {
                // SAFETY: `this` is a live `VersionedRootSignatureDeserializer`.
                let deser = unsafe { &*this.cast::<VersionedRootSignatureDeserializer>() };
                if deser.unconverted_version == D3D_ROOT_SIGNATURE_VERSION_1_0 {
                    ptr::from_ref(&deser.converted.desc_1_0).cast()
                } else {
                    ptr::from_ref(&deser.converted.desc_1_1).cast()
                }
            }
        ),
    };

/// `D3D12CreateRootSignatureDeserializer` (`d3d12_root_signature.cpp:331-345`).
///
/// # Safety
///
/// `src_data`, `deserializer_interface`, and `deserializer` are caller-supplied pointers.
pub unsafe fn create_deserializer(
    src_data: *const c_void,
    src_data_size_in_bytes: usize,
    deserializer_interface: *const Guid,
    deserializer: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `deserializer` is a caller out-pointer (or null).
    unsafe { com::init_out_ptr(deserializer) };
    if src_data.is_null() || deserializer.is_null() {
        return E_INVALIDARG;
    }
    let bytes = if src_data_size_in_bytes == 0 {
        &[][..]
    } else {
        // SAFETY: `src_data` is non-null and holds `src_data_size_in_bytes` bytes.
        unsafe { core::slice::from_raw_parts(src_data.cast::<u8>(), src_data_size_in_bytes) }
    };
    let ir = match parse_rts0_slice(bytes) {
        Ok(ir) => ir,
        Err(hr) => return hr,
    };
    let raw = Box::into_raw(Box::new(RootSignatureDeserializer {
        vtbl: &DESERIALIZER_VTBL,
        com: ComObject::new(),
        converted: ConvertedRootSignature::from_ir(&ir),
    }));
    // SAFETY: `raw` is a valid `RootSignatureDeserializer` and `deserializer` is non-null.
    let hr = unsafe {
        (DESERIALIZER_VTBL.query_interface)(raw.cast(), deserializer_interface.cast(), deserializer)
    };
    if hr < 0 {
        // SAFETY: `QueryInterface` did not increment `raw`'s refcount; reclaim the allocation.
        drop(unsafe { Box::from_raw(raw) });
    }
    hr
}

/// `D3D12CreateVersionedRootSignatureDeserializer` (`d3d12_root_signature.cpp:312-328`).
///
/// # Safety
///
/// `src_data`, `deserializer_interface`, and `deserializer` are caller-supplied pointers.
pub unsafe fn create_versioned_deserializer(
    src_data: *const c_void,
    src_data_size_in_bytes: usize,
    deserializer_interface: *const Guid,
    deserializer: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `deserializer` is a caller out-pointer (or null).
    unsafe { com::init_out_ptr(deserializer) };
    if src_data.is_null() || deserializer.is_null() {
        return E_INVALIDARG;
    }
    let bytes = if src_data_size_in_bytes == 0 {
        &[][..]
    } else {
        // SAFETY: `src_data` is non-null and holds `src_data_size_in_bytes` bytes.
        unsafe { core::slice::from_raw_parts(src_data.cast::<u8>(), src_data_size_in_bytes) }
    };
    let ir = match parse_rts0_slice(bytes) {
        Ok(ir) => ir,
        Err(hr) => return hr,
    };
    let raw = Box::into_raw(Box::new(VersionedRootSignatureDeserializer {
        vtbl: &VERSIONED_DESERIALIZER_VTBL,
        com: ComObject::new(),
        unconverted_version: ir.version,
        converted: ConvertedRootSignature::from_ir(&ir),
    }));
    // SAFETY: `raw` is a valid `VersionedRootSignatureDeserializer` and `deserializer` is non-null.
    let hr = unsafe {
        (VERSIONED_DESERIALIZER_VTBL.query_interface)(
            raw.cast(),
            deserializer_interface.cast(),
            deserializer,
        )
    };
    if hr < 0 {
        // SAFETY: `QueryInterface` did not increment `raw`'s refcount; reclaim the allocation.
        drop(unsafe { Box::from_raw(raw) });
    }
    hr
}

/// `D3D12SerializeRootSignature` (`d3d12_root_signature.cpp:229-243`).
///
/// # Safety
///
/// `root_signature`, `blob`, and `error_blob` are caller-supplied pointers.
pub unsafe fn serialize_root_signature(
    root_signature: *const c_void,
    version: u32,
    blob: *mut *mut c_void,
    error_blob: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `error_blob` is a caller out-pointer (or null).
    unsafe { com::init_out_ptr(error_blob) };
    if blob.is_null() || version != D3D_ROOT_SIGNATURE_VERSION_1_0 || root_signature.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `root_signature` is non-null; read unaligned as application structs carry no alignment guarantee.
    let desc = unsafe { root_signature.cast::<RootSignatureDesc>().read_unaligned() };
    // SAFETY: caller guarantees the arrays referenced by `desc` are readable for their counts.
    match unsafe { super::serialize(&desc, version) } {
        // SAFETY: `blob` is non-null and writable.
        Ok(bytes) => unsafe { crate::blob::create_blob(bytes, blob) },
        Err(hr) => hr,
    }
}

/// `D3D12SerializeVersionedRootSignature` (`d3d12_root_signature.cpp:178-226`).
///
/// # Safety
///
/// `root_signature`, `blob`, and `error_blob` are caller-supplied pointers.
pub unsafe fn serialize_versioned_root_signature(
    root_signature: *const c_void,
    blob: *mut *mut c_void,
    error_blob: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `error_blob` is a caller out-pointer (or null).
    unsafe { com::init_out_ptr(error_blob) };
    if blob.is_null() || root_signature.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `root_signature` is non-null; read unaligned as application structs carry no alignment guarantee.
    let desc = unsafe {
        root_signature
            .cast::<VersionedRootSignatureDesc>()
            .read_unaligned()
    };
    // SAFETY: caller guarantees the arrays referenced by `desc` are readable for their counts.
    match unsafe { super::serialize_versioned(&desc) } {
        // SAFETY: `blob` is non-null and writable.
        Ok(bytes) => unsafe { crate::blob::create_blob(bytes, blob) },
        Err(hr) => hr,
    }
}
