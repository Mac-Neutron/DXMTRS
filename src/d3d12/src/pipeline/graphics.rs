//! `MTLD3D12GraphicsPipelineState` (`d3d12_pipeline_graphics.cpp`, `d3d12_device.hpp:251-274`).

use core::{
    ffi::c_void,
    fmt::Write,
    hash::{Hash, Hasher},
    mem::zeroed,
    ptr,
};
use std::sync::{Arc, Mutex};

use airconv_ir::{
    container::{Code, Container, Error as IrError},
    signature::{self, Signature},
};
use dxmt_core::library::render_pipeline_info;
use dxmt_util::{com::ComObject, float_cast::i32_to_f32, private_data::PrivateData, sync::lock};
use rustc_hash::FxHasher;
use winemetal_sys::{
    MTLDevice_newDepthStencilState, MTLDevice_newMeshRenderPipelineState,
    MTLDevice_newRenderPipelineState, Obj,
    sm50::{
        MtlShaderReflection, SM50_BINDING_INDEX_DRAW_ARGUMENTS, SM50_BINDING_INDEX_ROOT_ARGUMENTS,
        SM50_BINDING_INDEX_STATIC_SAMPLERS, SM50_BINDING_INDEX_VERTEX_BUFFER,
        SM50_INDEX_BUFFER_FORMAT_NONE, SM50_SHADER_COMMON, SM50_SHADER_IA_INPUT_LAYOUT,
        SM50_SHADER_METAL_310, SM50_SHADER_PSO_GEOMETRY_SHADER, SM50_SHADER_PSO_PIXEL_SHADER,
        SM50_SHADER_ROOT_SIGNATURE, Sm50IaInputElement, Sm50ShaderCommonData,
        Sm50ShaderIaInputLayoutData, Sm50ShaderPsoGeometryShaderData, Sm50ShaderPsoPixelShaderData,
        Sm50ShaderRootSignatureData,
    },
};
use winemetal_wire::{
    MemPtr, WMT_BLEND_FACTOR_BLEND_ALPHA, WMT_BLEND_FACTOR_BLEND_COLOR,
    WMT_BLEND_FACTOR_DESTINATION_ALPHA, WMT_BLEND_FACTOR_DESTINATION_COLOR, WMT_BLEND_FACTOR_ONE,
    WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR,
    WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR,
    WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR,
    WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_ALPHA, WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_COLOR,
    WMT_BLEND_FACTOR_SOURCE_ALPHA, WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED,
    WMT_BLEND_FACTOR_SOURCE_COLOR, WMT_BLEND_FACTOR_SOURCE1_ALPHA, WMT_BLEND_FACTOR_SOURCE1_COLOR,
    WMT_BLEND_FACTOR_ZERO, WMT_BLEND_OPERATION_ADD, WMT_BLEND_OPERATION_MAX,
    WMT_BLEND_OPERATION_MIN, WMT_BLEND_OPERATION_REVERSE_SUBTRACT, WMT_BLEND_OPERATION_SUBTRACT,
    WMT_COLOR_WRITE_MASK_ALL, WMT_COLOR_WRITE_MASK_ALPHA, WMT_COLOR_WRITE_MASK_BLUE,
    WMT_COLOR_WRITE_MASK_GREEN, WMT_COLOR_WRITE_MASK_NONE, WMT_COLOR_WRITE_MASK_RED,
    WMT_COMPARE_FUNCTION_ALWAYS, WMT_COMPARE_FUNCTION_EQUAL, WMT_COMPARE_FUNCTION_GREATER,
    WMT_COMPARE_FUNCTION_GREATER_EQUAL, WMT_COMPARE_FUNCTION_LESS, WMT_COMPARE_FUNCTION_LESS_EQUAL,
    WMT_COMPARE_FUNCTION_NEVER, WMT_COMPARE_FUNCTION_NOT_EQUAL, WMT_CULL_MODE_BACK,
    WMT_CULL_MODE_FRONT, WMT_CULL_MODE_NONE, WMT_DEPTH_CLIP_MODE_CLAMP, WMT_DEPTH_CLIP_MODE_CLIP,
    WMT_LOGIC_OPERATION_CLEAR, WMT_PIXEL_FORMAT_INVALID, WMT_PIXEL_FORMAT_RGB9E5_FLOAT,
    WMT_PRIMITIVE_TOPOLOGY_CLASS_LINE, WMT_PRIMITIVE_TOPOLOGY_CLASS_POINT,
    WMT_PRIMITIVE_TOPOLOGY_CLASS_TRIANGLE, WMT_STENCIL_OPERATION_DECREMENT_CLAMP,
    WMT_STENCIL_OPERATION_DECREMENT_WRAP, WMT_STENCIL_OPERATION_INCREMENT_CLAMP,
    WMT_STENCIL_OPERATION_INCREMENT_WRAP, WMT_STENCIL_OPERATION_INVERT, WMT_STENCIL_OPERATION_KEEP,
    WMT_STENCIL_OPERATION_REPLACE, WMT_STENCIL_OPERATION_ZERO, WMT_TRIANGLE_FILL_MODE_FILL,
    WMT_TRIANGLE_FILL_MODE_LINES, WMT_WINDING_CLOCKWISE, WMT_WINDING_COUNTER_CLOCKWISE,
    WMTColorAttachmentBlendInfo, WMTDepthStencilInfo, WMTMeshRenderPipelineInfo, WMTStencilInfo,
    original_format,
};

use crate::{
    abi::{
        pipeline::{
            DEPTH_WRITE_MASK_ALL, E_NOTIMPL, FILL_MODE_SOLID, GraphicsPipelineStateDesc,
            InputElementDesc, InputLayoutDesc, StreamOutputDesc,
        },
        vtbl::{ID3D12PipelineStateVtbl, IUnknownVtbl},
    },
    blob::create_blob,
    capture::{
        captured_root_signature, captured_shader, dump_dxil, dxil_capture_mode, log_pipeline,
        name_pipeline,
    },
    com::{
        ComRefCounted, E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult,
        IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT, IID_ID3D12_PAGEABLE, IID_ID3D12_PIPELINE_STATE,
        IID_IUNKNOWN, S_OK, add_ref_com, init_out_ptr, release_com, set_name_private_data, vtbl,
    },
    device::Device,
    pipeline::{
        obj_description,
        record::{record_graphics, recording_on},
        root_blob, shader_blob,
    },
    root_signature::object::cast_root_signature,
    shader_cache::{CachedShader, CompileTarget, FunctionKind, Sha1Digest, compile_function},
};

#[cfg(target_pointer_width = "64")]
const NULL_PTR: MemPtr = MemPtr { ptr: 0 };
#[cfg(target_pointer_width = "32")]
const NULL_PTR: MemPtr = MemPtr { ptr: 0, high: 0 };

const COLOR_WRITE_MASK_MAP: [u8; 16] = [
    WMT_COLOR_WRITE_MASK_NONE,
    WMT_COLOR_WRITE_MASK_RED,
    WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_RED | WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_BLUE,
    WMT_COLOR_WRITE_MASK_BLUE | WMT_COLOR_WRITE_MASK_RED,
    WMT_COLOR_WRITE_MASK_BLUE | WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_BLUE | WMT_COLOR_WRITE_MASK_RED | WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_ALPHA,
    WMT_COLOR_WRITE_MASK_ALPHA | WMT_COLOR_WRITE_MASK_RED,
    WMT_COLOR_WRITE_MASK_ALPHA | WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_ALPHA | WMT_COLOR_WRITE_MASK_RED | WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_ALPHA | WMT_COLOR_WRITE_MASK_BLUE,
    WMT_COLOR_WRITE_MASK_ALPHA | WMT_COLOR_WRITE_MASK_BLUE | WMT_COLOR_WRITE_MASK_RED,
    WMT_COLOR_WRITE_MASK_ALPHA | WMT_COLOR_WRITE_MASK_BLUE | WMT_COLOR_WRITE_MASK_GREEN,
    WMT_COLOR_WRITE_MASK_ALL,
];

struct GeometryState {
    vertex_shader: Arc<CachedShader>,
    geometry_shader: Arc<CachedShader>,
    elements: Vec<Sm50IaInputElement>,
    rootsig_blob: Vec<u8>,
    _ps_func: Option<Obj>,
    geometry_info: WMTMeshRenderPipelineInfo,
    capture_line: String,
    failed: [bool; 6],
}

/// `MTLD3D12GraphicsPipelineState` (`d3d12_device.hpp:251-274`).
#[repr(C)]
pub struct Graphics {
    pub vtbl: &'static ID3D12PipelineStateVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub is_compute: bool,
    pub desc_hash: u64,
    pub pso: Option<Obj>,
    pub slot_mask: u32,
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub depth_clip_mode: u32,
    pub winding: u32,
    pub depth_bias: f32,
    pub slope_scale: f32,
    pub depth_bias_clamp: f32,
    pub forced_sample_count: u32,
    pub geometry: bool,
    pub writes_uav: bool,
    pub depth_stencil_states: [Option<Obj>; 6],
    pub geometry_variants: Mutex<[Option<Obj>; 6]>,
    geometry_state: Option<Mutex<GeometryState>>,
}

impl ComRefCounted for Graphics {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for Graphics {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` was `add_ref_com`'d when `self` was constructed.
            unsafe { release_com(self.device) };
        }
    }
}

impl Graphics {
    /// Returns (compiling on first use) the mesh-emulation PSO handle for `(strip, index_format)`
    ///
    /// (`GeometryPipeline`, `d3d12_pipeline_graphics.cpp:604`).
    #[must_use]
    pub fn geometry_pipeline(&self, strip: bool, index_format: u32) -> u64 {
        let idx = (usize::from(strip) * 3) + (index_format as usize).min(2);
        let mut variants = lock(&self.geometry_variants);
        if let Some(obj) = variants[idx].as_ref() {
            return obj.handle();
        }
        let Some(state_mu) = self.geometry_state.as_ref() else {
            return 0;
        };
        let mut state = lock(state_mu);
        if state.failed[idx] || self.device.is_null() {
            return 0;
        }
        // SAFETY: `self.device` is non-null and kept alive by `self`.
        let dev = unsafe { &*self.device };
        let Some(metal) = dev.mtl.as_ref() else {
            state.failed[idx] = true;
            return 0;
        };
        // SAFETY: `state` holds valid shader/layout buffers for the duration of the compile.
        if let Ok(pso) = unsafe {
            compile_geometry_variant(
                metal,
                &dev.shader_functions,
                self.slot_mask,
                &mut state,
                strip,
                index_format,
            )
        } {
            let handle = pso.handle();
            variants[idx] = Some(pso);
            drop(state);
            drop(variants);
            handle
        } else {
            state.failed[idx] = true;
            drop(state);
            drop(variants);
            0
        }
    }

    /// Returns the `MTLDepthStencilState` handle for `(dsv_planar, dsv_readonly_flags)`
    ///
    /// (`GetDepthStencilState`, `d3d12_pipeline_graphics.cpp:788`).
    #[must_use]
    pub fn depth_stencil_state(&self, dsv_planar: u32, dsv_readonly_flags: u32) -> u64 {
        if dsv_planar == 0 {
            if self.device.is_null() {
                return 0;
            }
            // SAFETY: `self.device` is non-null and kept alive by `self`.
            let dev = unsafe { &*self.device };
            return dev
                .default_depth_stencil_state
                .as_ref()
                .map_or(0, Obj::handle);
        }
        if dsv_planar == 1 {
            let idx = if (dsv_readonly_flags & 1) != 0 { 5 } else { 4 };
            return self.depth_stencil_states[idx]
                .as_ref()
                .map_or(0, Obj::handle);
        }
        let idx = match dsv_readonly_flags {
            3 => 3,
            2 => 2,
            1 => 1,
            _ => 0,
        };
        self.depth_stencil_states[idx]
            .as_ref()
            .map_or(0, Obj::handle)
    }
}

pub(super) static VTBL: ID3D12PipelineStateVtbl = ID3D12PipelineStateVtbl {
    query_interface: crate::slot!(
        "gfxpso.QueryInterface",
        (this: *mut c_void, riid: *const c_void, object: *mut *mut c_void) -> Hresult => {
            if object.is_null() {
                return E_POINTER;
            }
            // SAFETY: `object` is non-null.
            unsafe { init_out_ptr(object) };
            // SAFETY: `riid` is null or points to a `Guid`.
            let Some(guid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            if matches!(
                guid,
                IID_IUNKNOWN
                    | IID_ID3D12_OBJECT
                    | IID_ID3D12_DEVICE_CHILD
                    | IID_ID3D12_PAGEABLE
                    | IID_ID3D12_PIPELINE_STATE
            ) {
                // SAFETY: `this` is a live `Graphics`.
                unsafe { add_ref_com(this.cast::<Graphics>()) };
                // SAFETY: `object` is non-null and writable.
                unsafe { object.write_unaligned(this) };
                S_OK
            } else {
                crate::warn!("D3D12GraphicsPipelineState: Unknown interface query {guid}");
                E_NOINTERFACE
            }
        }
    ),
    add_ref: crate::slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Graphics`.
        unsafe { add_ref_com(this.cast::<Graphics>()) }
    }),
    release: crate::slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `Graphics`.
        unsafe { release_com(this.cast::<Graphics>()) }
    }),
    get_private_data: crate::slot!(
        (this: *mut c_void, guid: *const c_void, data_size: *mut u32, data: *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `Graphics`.
            let pso = unsafe { &*this.cast::<Graphics>() };
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: Forwarded from COM slot.
            unsafe { pso.private_data.get_data(guid, data_size, data) }
        }
    ),
    set_private_data: crate::slot!(
        (this: *mut c_void, guid: *const c_void, data_size: u32, data: *const c_void) -> Hresult => {
            // SAFETY: `this` is a live `Graphics`.
            let pso = unsafe { &*this.cast::<Graphics>() };
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: Forwarded from COM slot.
            unsafe { pso.private_data.set_data(guid, data_size, data) }
        }
    ),
    set_private_data_interface: crate::slot!(
        (this: *mut c_void, guid: *const c_void, data: *const c_void) -> Hresult => {
            // SAFETY: `this` is a live `Graphics`.
            let pso = unsafe { &*this.cast::<Graphics>() };
            // SAFETY: `guid` is null or 16 readable bytes.
            let Some(guid) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            // SAFETY: Forwarded from COM slot.
            unsafe { pso.private_data.set_interface(guid, data.cast_mut()) }
        }
    ),
    set_name: crate::slot!((this: *mut c_void, name: *const c_void) -> Hresult => {
        // SAFETY: `this` is a live `Graphics`.
        let pso = unsafe { &*this.cast::<Graphics>() };
        // SAFETY: Forwarded from COM slot.
        unsafe { set_name_private_data(&pso.private_data, name.cast::<u16>()) }
    }),
    get_device: crate::slot!(
        (this: *mut c_void, riid: *const c_void, pp_device: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `Graphics`.
            let pso = unsafe { &*this.cast::<Graphics>() };
            if pso.device.is_null() {
                // SAFETY: `pp_device` is a caller out-pointer.
                unsafe { init_out_ptr(pp_device) };
                return E_NOINTERFACE;
            }
            let dev_ptr = pso.device.cast::<c_void>();
            // SAFETY: `dev_ptr` is a live `Device` COM object.
            let unk = unsafe { vtbl::<IUnknownVtbl>(dev_ptr) };
            // SAFETY: Forwarded to `Device::QueryInterface`.
            unsafe { (unk.query_interface)(dev_ptr, riid, pp_device) }
        }
    ),
    get_cached_blob: crate::slot!(
        "gfxpso.GetCachedBlob",
        (_this: *mut c_void, pp_blob: *mut *mut c_void) -> Hresult => {
            if pp_blob.is_null() {
                return E_POINTER;
            }
            // SAFETY: `_this` is a live `Graphics` PSO.
            let pso = unsafe { &*_this.cast::<Graphics>() };
            // SAFETY: `pp_blob` is non-null.
            unsafe { create_blob(super::cached_pso_blob(pso.desc_hash), pp_blob) }
        }
    ),
};

const fn compare_func(func: u32) -> u8 {
    match func {
        2 => WMT_COMPARE_FUNCTION_LESS,
        3 => WMT_COMPARE_FUNCTION_EQUAL,
        4 => WMT_COMPARE_FUNCTION_LESS_EQUAL,
        5 => WMT_COMPARE_FUNCTION_GREATER,
        6 => WMT_COMPARE_FUNCTION_NOT_EQUAL,
        7 => WMT_COMPARE_FUNCTION_GREATER_EQUAL,
        8 => WMT_COMPARE_FUNCTION_ALWAYS,
        _ => WMT_COMPARE_FUNCTION_NEVER,
    }
}

const fn stencil_op(op: u32) -> u8 {
    match op {
        1 => WMT_STENCIL_OPERATION_KEEP,
        3 => WMT_STENCIL_OPERATION_REPLACE,
        4 => WMT_STENCIL_OPERATION_INCREMENT_CLAMP,
        5 => WMT_STENCIL_OPERATION_DECREMENT_CLAMP,
        6 => WMT_STENCIL_OPERATION_INVERT,
        7 => WMT_STENCIL_OPERATION_INCREMENT_WRAP,
        8 => WMT_STENCIL_OPERATION_DECREMENT_WRAP,
        _ => WMT_STENCIL_OPERATION_ZERO,
    }
}

const fn blend_op(op: u32) -> u8 {
    match op {
        2 => WMT_BLEND_OPERATION_SUBTRACT,
        3 => WMT_BLEND_OPERATION_REVERSE_SUBTRACT,
        4 => WMT_BLEND_OPERATION_MIN,
        5 => WMT_BLEND_OPERATION_MAX,
        _ => WMT_BLEND_OPERATION_ADD,
    }
}

const fn blend_factor(factor: u32) -> u8 {
    match factor {
        2 => WMT_BLEND_FACTOR_ONE,
        3 => WMT_BLEND_FACTOR_SOURCE_COLOR,
        4 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_COLOR,
        5 => WMT_BLEND_FACTOR_SOURCE_ALPHA,
        6 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE_ALPHA,
        7 => WMT_BLEND_FACTOR_DESTINATION_ALPHA,
        8 => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_ALPHA,
        9 => WMT_BLEND_FACTOR_DESTINATION_COLOR,
        10 => WMT_BLEND_FACTOR_ONE_MINUS_DESTINATION_COLOR,
        11 => WMT_BLEND_FACTOR_SOURCE_ALPHA_SATURATED,
        14 => WMT_BLEND_FACTOR_BLEND_COLOR,
        15 => WMT_BLEND_FACTOR_ONE_MINUS_BLEND_COLOR,
        16 => WMT_BLEND_FACTOR_SOURCE1_COLOR,
        17 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_COLOR,
        18 => WMT_BLEND_FACTOR_SOURCE1_ALPHA,
        19 => WMT_BLEND_FACTOR_ONE_MINUS_SOURCE1_ALPHA,
        20 => WMT_BLEND_FACTOR_BLEND_ALPHA,
        21 => WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA,
        _ => WMT_BLEND_FACTOR_ZERO,
    }
}

const fn blend_alpha_factor(factor: u32) -> u8 {
    match factor {
        14 => WMT_BLEND_FACTOR_BLEND_ALPHA,
        15 => WMT_BLEND_FACTOR_ONE_MINUS_BLEND_ALPHA,
        other => blend_factor(other),
    }
}

const fn blend_factor_is_dual_source(factor: u32) -> bool {
    factor >= 16 && factor <= 19
}

const IMMUTABLE_ROOT_BUFFERS: u32 =
    (1 << SM50_BINDING_INDEX_ROOT_ARGUMENTS) | (1 << SM50_BINDING_INDEX_STATIC_SAMPLERS);
const IMMUTABLE_VERTEX_BUFFERS: u32 =
    IMMUTABLE_ROOT_BUFFERS | (1 << SM50_BINDING_INDEX_VERTEX_BUFFER);
const IMMUTABLE_GS_OBJECT_BUFFERS: u32 =
    IMMUTABLE_VERTEX_BUFFERS | (1 << SM50_BINDING_INDEX_DRAW_ARGUMENTS);

const fn mesh_render_pipeline_info() -> WMTMeshRenderPipelineInfo {
    let color = WMTColorAttachmentBlendInfo {
        pixel_format: WMT_PIXEL_FORMAT_INVALID,
        rgb_blend_operation: WMT_BLEND_OPERATION_ADD,
        alpha_blend_operation: WMT_BLEND_OPERATION_ADD,
        src_rgb_blend_factor: WMT_BLEND_FACTOR_ONE,
        dst_rgb_blend_factor: WMT_BLEND_FACTOR_ZERO,
        src_alpha_blend_factor: WMT_BLEND_FACTOR_ONE,
        dst_alpha_blend_factor: WMT_BLEND_FACTOR_ZERO,
        write_mask: WMT_COLOR_WRITE_MASK_ALL,
        blending_enabled: 0,
    };
    WMTMeshRenderPipelineInfo {
        colors: [color; 8],
        alpha_to_coverage_enabled: 0,
        logic_operation_enabled: 0,
        logic_operation: WMT_LOGIC_OPERATION_CLEAR,
        rasterization_enabled: 1,
        raster_sample_count: 1,
        depth_pixel_format: WMT_PIXEL_FORMAT_INVALID,
        stencil_pixel_format: WMT_PIXEL_FORMAT_INVALID,
        object_function: 0,
        mesh_function: 0,
        fragment_function: 0,
        immutable_object_buffers: IMMUTABLE_GS_OBJECT_BUFFERS,
        immutable_mesh_buffers: IMMUTABLE_ROOT_BUFFERS,
        immutable_fragment_buffers: IMMUTABLE_ROOT_BUFFERS,
        payload_memory_length: 0,
        mesh_tgsize_is_multiple_of_sgwidth: 0,
        object_tgsize_is_multiple_of_sgwidth: 0,
        binary_archive_for_serialization: 0,
        binary_archives_for_lookup: NULL_PTR,
        num_binary_archives_for_lookup: 0,
        fail_on_binary_archive_miss: 0,
        support_indirect_command_buffers: 0,
        padding: [0; 5],
    }
}

const fn ir_err_to_hr(err: &IrError) -> Hresult {
    match err.code {
        Code::Fail => E_FAIL,
        Code::InvalidArg => E_INVALIDARG,
        Code::OutOfMemory => 0x8007_000e_u32.cast_signed(),
    }
}

const unsafe fn read_cstr_bytes<'a>(ptr: *const u8) -> &'a [u8] {
    if ptr.is_null() {
        return &[];
    }
    let mut len = 0usize;
    loop {
        // SAFETY: `ptr` is non-null and NUL-terminated.
        let byte_ptr = unsafe { ptr.add(len) };
        // SAFETY: `byte_ptr` points within the caller's NUL-terminated string.
        if unsafe { *byte_ptr } == 0 {
            break;
        }
        len += 1;
    }
    // SAFETY: `ptr` is valid for `len` bytes.
    unsafe { core::slice::from_raw_parts(ptr, len) }
}

/// Parses the DXBC input signature (`ISG1`/`ISGN`) from `vs_bytes` using `airconv_ir`.
pub(super) fn parse_input_signature(vs_bytes: &[u8]) -> Result<Signature, Hresult> {
    let container = Container::parse(vs_bytes).map_err(|e| ir_err_to_hr(&e))?;
    signature::input(&container).map_err(|e| ir_err_to_hr(&e))
}

/// Extracts `SM50_IA_INPUT_ELEMENT` records and `slot_mask` (`ExtractMTLInputLayoutElements`,
///
/// `d3d12_pipeline_graphics.cpp:160-236`).
pub(super) unsafe fn extract_input_layout(
    vs_bytes: &[u8],
    elements: &[InputElementDesc],
) -> Result<(Vec<Sm50IaInputElement>, u32), Hresult> {
    let sig = parse_input_signature(vs_bytes)?;
    let mut append_offset = [0u16; 32];
    let mut register_mask = 0u32;
    let mut out = Vec::with_capacity(elements.len());

    for desc in elements {
        let (hr, metal_format) = dxmt_core::format::query_dxgi(desc.format);
        if hr < 0 {
            crate::err!(
                "CreateInputLayout: Unsupported vertex format: {}",
                desc.format
            );
            return Err(E_FAIL);
        }
        if metal_format.attribute_format == 0 {
            crate::err!(
                "CreateInputLayout: Unsupported vertex format: {}",
                desc.format
            );
            return Err(E_INVALIDARG);
        }
        if metal_format.bytes_per_texel == 0 {
            crate::err!(
                "CreateInputLayout: not an ordinary or packed format: {}",
                desc.format
            );
            return Err(E_INVALIDARG);
        }
        let slot_idx = desc.input_slot as usize;
        if slot_idx >= 32 {
            return Err(E_INVALIDARG);
        }
        let align_to = metal_format.bytes_per_texel.min(4);
        let aligned_byte_offset = if desc.aligned_byte_offset == 0xffff_ffff {
            (u32::from(append_offset[slot_idx]) + (align_to - 1)) & !(align_to - 1)
        } else {
            desc.aligned_byte_offset
        };
        append_offset[slot_idx] =
            u16::try_from(aligned_byte_offset.wrapping_add(metal_format.bytes_per_texel) & 0xffff)
                .unwrap_or(0);

        // SAFETY: `desc.semantic_name` is null or a NUL-terminated C string.
        let sem_bytes = unsafe { read_cstr_bytes(desc.semantic_name) };
        let Some(input_sig) = sig.parameters.iter().find(|p| {
            desc.semantic_index == p.semantic_index
                && sem_bytes.eq_ignore_ascii_case(&p.semantic_name)
        }) else {
            continue;
        };
        let step_rate = if desc.input_slot_class == 1 {
            desc.instance_data_step_rate
        } else {
            1
        };
        out.push(Sm50IaInputElement::new(
            input_sig.register,
            desc.input_slot,
            aligned_byte_offset,
            metal_format.attribute_format,
            desc.input_slot_class,
            step_rate,
        ));
        register_mask |= 1u32.wrapping_shl(input_sig.register);
    }

    for input_sig in &sig.parameters {
        if input_sig.system_value != 0 {
            continue;
        }
        if (register_mask & 1u32.wrapping_shl(input_sig.register)) == 0 {
            crate::warn!(
                "CreateInputLayout: Vertex shader expects {}_{} but it's not in input layout element descriptors",
                String::from_utf8_lossy(&input_sig.semantic_name),
                input_sig.semantic_index
            );
            return Err(E_INVALIDARG);
        }
    }

    let mut slot_mask = 0u32;
    for el in &out {
        slot_mask |= 1u32.wrapping_shl(el.slot);
    }
    Ok((out, slot_mask))
}

unsafe fn hash_stream_output(so: &StreamOutputDesc, hasher: &mut FxHasher) {
    so.num_entries.hash(hasher);
    so.num_strides.hash(hasher);
    so.rasterized_stream.hash(hasher);
    if !so.so_declaration.is_null() && so.num_entries > 0 {
        // SAFETY: `so_declaration` is non-null for `num_entries` elements.
        let entries =
            unsafe { core::slice::from_raw_parts(so.so_declaration, so.num_entries as usize) };
        for e in entries {
            // SAFETY: `e.semantic_name` is null or NUL-terminated.
            let name = unsafe { read_cstr_bytes(e.semantic_name) };
            name.hash(hasher);
            e.stream.hash(hasher);
            e.semantic_index.hash(hasher);
            e.start_component.hash(hasher);
            e.component_count.hash(hasher);
            e.output_slot.hash(hasher);
        }
    }
    if !so.buffer_strides.is_null() && so.num_strides > 0 {
        // SAFETY: `buffer_strides` is non-null for `num_strides` elements.
        let strides =
            unsafe { core::slice::from_raw_parts(so.buffer_strides, so.num_strides as usize) };
        strides.hash(hasher);
    }
}

unsafe fn hash_input_layout(il: &InputLayoutDesc, hasher: &mut FxHasher) {
    il.num_elements.hash(hasher);
    if !il.input_element_descs.is_null() && il.num_elements > 0 {
        // SAFETY: `input_element_descs` is non-null for `num_elements` elements.
        let elems = unsafe {
            core::slice::from_raw_parts(il.input_element_descs, il.num_elements as usize)
        };
        for e in elems {
            // SAFETY: `e.semantic_name` is null or NUL-terminated.
            let name = unsafe { read_cstr_bytes(e.semantic_name) };
            name.hash(hasher);
            e.semantic_index.hash(hasher);
            e.format.hash(hasher);
            e.input_slot.hash(hasher);
            e.aligned_byte_offset.hash(hasher);
            e.input_slot_class.hash(hasher);
            e.instance_data_step_rate.hash(hasher);
        }
    }
}

/// Computes the build-local description hash for a `D3D12_GRAPHICS_PIPELINE_STATE_DESC`
///
/// (`HashGraphicsDesc`, `d3d12_pipeline_library.cpp:36`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_GRAPHICS_PIPELINE_STATE_DESC`.
#[must_use]
pub unsafe fn hash_graphics_desc(desc: *const c_void) -> u64 {
    // SAFETY: Caller guarantees `desc` points to a valid `GraphicsPipelineStateDesc`.
    let d = unsafe { &*desc.cast::<GraphicsPipelineStateDesc>() };
    let mut hasher = FxHasher::default();
    if !d.root_signature.is_null() {
        // SAFETY: `d.root_signature` is non-null.
        if let Some(rs) = unsafe { cast_root_signature(d.root_signature) } {
            (rs.blob.len() as u64).hash(&mut hasher);
            rs.blob.hash(&mut hasher);
        }
    }
    for bc in [&d.vs, &d.ps, &d.ds, &d.hs, &d.gs] {
        (bc.bytecode_length as u64).hash(&mut hasher);
        if !bc.shader_bytecode.is_null() && bc.bytecode_length > 0 {
            // SAFETY: `bc.shader_bytecode` is non-null and valid for `bc.bytecode_length` bytes.
            let bytes = unsafe {
                core::slice::from_raw_parts(bc.shader_bytecode.cast::<u8>(), bc.bytecode_length)
            };
            bytes.hash(&mut hasher);
        }
    }
    // SAFETY: `d.stream_output` pointers are valid per caller contract.
    unsafe { hash_stream_output(&d.stream_output, &mut hasher) };
    // SAFETY: `blend_state` is a POD struct.
    let blend_bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const d.blend_state).cast::<u8>(),
            size_of_val(&d.blend_state),
        )
    };
    blend_bytes.hash(&mut hasher);
    d.sample_mask.hash(&mut hasher);
    // SAFETY: `rasterizer_state` is a POD struct.
    let rast_bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const d.rasterizer_state).cast::<u8>(),
            size_of_val(&d.rasterizer_state),
        )
    };
    rast_bytes.hash(&mut hasher);
    // SAFETY: `depth_stencil_state` is a POD struct.
    let ds_bytes = unsafe {
        core::slice::from_raw_parts(
            (&raw const d.depth_stencil_state).cast::<u8>(),
            size_of_val(&d.depth_stencil_state),
        )
    };
    ds_bytes.hash(&mut hasher);
    // SAFETY: `d.input_layout` pointers are valid per caller contract.
    unsafe { hash_input_layout(&d.input_layout, &mut hasher) };
    d.ib_strip_cut_value.hash(&mut hasher);
    d.primitive_topology_type.hash(&mut hasher);
    d.num_render_targets.hash(&mut hasher);
    d.rtv_formats.hash(&mut hasher);
    d.dsv_format.hash(&mut hasher);
    d.sample_desc.count.hash(&mut hasher);
    d.sample_desc.quality.hash(&mut hasher);
    d.node_mask.hash(&mut hasher);
    d.flags.hash(&mut hasher);
    hasher.finish() & !1
}

struct PsoCommonTarget<'a> {
    colors: &'a mut [WMTColorAttachmentBlendInfo; 8],
    depth_pixel_format: &'a mut u32,
    stencil_pixel_format: &'a mut u32,
    input_primitive_topology: Option<&'a mut u32>,
    raster_sample_count: &'a mut u8,
    support_icb: &'a mut u8,
    alpha_to_coverage_enabled: &'a mut u8,
    dual_source_blending: &'a mut bool,
}

fn initialize_color_targets(
    dev: &Device,
    desc: &GraphicsPipelineStateDesc,
    colors: &mut [WMTColorAttachmentBlendInfo; 8],
    dual_source_blending: &mut bool,
) -> Hresult {
    let mut effective_dual_source_rtvs = 0u32;
    let num_rt = (desc.num_render_targets as usize).min(8);
    for (i, rt) in colors.iter_mut().enumerate().take(num_rt) {
        if desc.rtv_formats[i] == 0 {
            continue;
        }
        if i >= 2 && *dual_source_blending {
            break;
        }
        let (hr, format_desc) = dxmt_core::format::query_dxgi(desc.rtv_formats[i]);
        if hr < 0 {
            return hr;
        }
        rt.pixel_format = format_desc.pixel_format;

        let rt_idx = if desc.blend_state.independent_blend_enable != 0 {
            i
        } else {
            0
        };
        let render_target = &desc.blend_state.render_target[rt_idx];
        if render_target.blend_enable != 0 && render_target.logic_op_enable != 0 {
            return E_INVALIDARG;
        }
        if desc.blend_state.independent_blend_enable != 0 && render_target.logic_op_enable != 0 {
            return E_INVALIDARG;
        }
        rt.write_mask =
            COLOR_WRITE_MASK_MAP[(render_target.render_target_write_mask & 0xf) as usize];
        if rt.pixel_format == WMT_PIXEL_FORMAT_RGB9E5_FLOAT {
            rt.write_mask = if (rt.write_mask & !WMT_COLOR_WRITE_MASK_ALPHA) != 0 {
                WMT_COLOR_WRITE_MASK_ALL
            } else {
                0
            };
        }
        if render_target.blend_enable != 0 {
            let cap = dev.format_capability(original_format(rt.pixel_format));
            if (cap & dxmt_core::format::CAP_BLEND) == 0 {
                crate::warn!(
                    "CreateGraphicsPipelineState: pixel format {} is not blendable",
                    rt.pixel_format
                );
                return E_INVALIDARG;
            }
            if blend_factor_is_dual_source(render_target.src_blend_alpha)
                || blend_factor_is_dual_source(render_target.src_blend)
                || blend_factor_is_dual_source(render_target.dest_blend_alpha)
                || blend_factor_is_dual_source(render_target.dest_blend)
            {
                *dual_source_blending = true;
            }
            rt.alpha_blend_operation = blend_op(render_target.blend_op_alpha);
            rt.rgb_blend_operation = blend_op(render_target.blend_op);
            rt.blending_enabled = u8::try_from(render_target.blend_enable).unwrap_or(1);
            rt.src_alpha_blend_factor = blend_alpha_factor(render_target.src_blend_alpha);
            rt.src_rgb_blend_factor = blend_factor(render_target.src_blend);
            rt.dst_alpha_blend_factor = blend_alpha_factor(render_target.dest_blend_alpha);
            rt.dst_rgb_blend_factor = blend_factor(render_target.dest_blend);
        }
        if i < 2 {
            effective_dual_source_rtvs += 1;
        }
    }
    if *dual_source_blending && effective_dual_source_rtvs > 1 {
        return E_INVALIDARG;
    }
    S_OK
}

fn initialize_pso_fields(
    dev: &Device,
    desc: &GraphicsPipelineStateDesc,
    has_coverage_output: bool,
    target: &mut PsoCommonTarget<'_>,
) -> Hresult {
    let hr = initialize_color_targets(dev, desc, target.colors, target.dual_source_blending);
    if hr < 0 {
        return hr;
    }
    if desc.dsv_format != 0 {
        let (hr, format_desc) = dxmt_core::format::query_dxgi(desc.dsv_format);
        if hr < 0 {
            return hr;
        }
        let dsv_flags = dxmt_core::format::depth_stencil_planar_flags(format_desc.pixel_format);
        if (dsv_flags & 1) != 0 {
            *target.depth_pixel_format = format_desc.pixel_format;
        }
        if (dsv_flags & 2) != 0 {
            *target.stencil_pixel_format = format_desc.pixel_format;
        }
    }
    if let Some(topo) = target.input_primitive_topology.as_deref_mut() {
        match desc.primitive_topology_type {
            1 => *topo = WMT_PRIMITIVE_TOPOLOGY_CLASS_POINT,
            2 => *topo = WMT_PRIMITIVE_TOPOLOGY_CLASS_LINE,
            3 | 4 => *topo = WMT_PRIMITIVE_TOPOLOGY_CLASS_TRIANGLE,
            _ => {}
        }
    }
    *target.raster_sample_count = u8::try_from(desc.sample_desc.count.max(1)).unwrap_or(1);
    *target.support_icb = 1;
    *target.alpha_to_coverage_enabled =
        u8::from(desc.blend_state.alpha_to_coverage_enable != 0 && !has_coverage_output);
    S_OK
}

fn initialize_dsso(
    dev: &Device,
    metal: &Obj,
    desc: &GraphicsPipelineStateDesc,
) -> [Option<Obj>; 6] {
    let ds = &desc.depth_stencil_state;
    let empty_stencil = WMTStencilInfo {
        enabled: 0,
        depth_stencil_pass_op: 0,
        stencil_fail_op: 0,
        depth_fail_op: 0,
        stencil_compare_function: 0,
        write_mask: 0,
        read_mask: 0,
    };
    let mut info = WMTDepthStencilInfo {
        depth_compare_function: WMT_COMPARE_FUNCTION_ALWAYS,
        depth_write_enabled: 0,
        front_stencil: empty_stencil,
        back_stencil: empty_stencil,
    };
    if ds.depth_enable != 0 {
        info.depth_compare_function = compare_func(ds.depth_func);
        info.depth_write_enabled = u8::from(ds.depth_write_mask == DEPTH_WRITE_MASK_ALL);
    }
    if ds.stencil_enable != 0 {
        info.front_stencil = WMTStencilInfo {
            enabled: 1,
            depth_stencil_pass_op: stencil_op(ds.front_face.stencil_pass_op),
            stencil_fail_op: stencil_op(ds.front_face.stencil_fail_op),
            depth_fail_op: stencil_op(ds.front_face.stencil_depth_fail_op),
            stencil_compare_function: compare_func(ds.front_face.stencil_func),
            write_mask: ds.stencil_write_mask,
            read_mask: ds.stencil_read_mask,
        };
        info.back_stencil = WMTStencilInfo {
            enabled: 1,
            depth_stencil_pass_op: stencil_op(ds.back_face.stencil_pass_op),
            stencil_fail_op: stencil_op(ds.back_face.stencil_fail_op),
            depth_fail_op: stencil_op(ds.back_face.stencil_depth_fail_op),
            stencil_compare_function: compare_func(ds.back_face.stencil_func),
            write_mask: ds.stencil_write_mask,
            read_mask: ds.stencil_read_mask,
        };
    }

    let make = |d: &WMTDepthStencilInfo| -> Option<Obj> {
        let key = [
            d.depth_compare_function,
            d.depth_write_enabled,
            d.front_stencil.enabled,
            d.front_stencil.depth_stencil_pass_op,
            d.front_stencil.stencil_fail_op,
            d.front_stencil.depth_fail_op,
            d.front_stencil.stencil_compare_function,
            d.front_stencil.write_mask,
            d.front_stencil.read_mask,
            d.back_stencil.enabled,
            d.back_stencil.depth_stencil_pass_op,
            d.back_stencil.stencil_fail_op,
            d.back_stencil.depth_fail_op,
            d.back_stencil.stencil_compare_function,
            d.back_stencil.write_mask,
            d.back_stencil.read_mask,
        ];
        let cache = lock(&dev.depth_stencil_states);
        if let Some(existing) = cache.get(&key) {
            let h = existing.handle();
            drop(cache);
            // SAFETY: `h` is a live `MTLDepthStencilState` handle cached on `dev`.
            return unsafe { Obj::retain(h) };
        }
        drop(cache);
        let metal_h = metal.handle();
        let desc_ptr = &raw const *d;
        // SAFETY: `metal_h` is a live `MTLDevice` and `desc_ptr` is valid.
        let h = unsafe { MTLDevice_newDepthStencilState(metal_h, desc_ptr) };
        // SAFETY: `h` is 0 or a newly retained `MTLDepthStencilState` handle.
        let obj = unsafe { Obj::adopt(h) };
        if let Some(ref created) = obj {
            let created_h = created.handle();
            // SAFETY: `created_h` is a live `MTLDepthStencilState` handle.
            if let Some(cached) = unsafe { Obj::retain(created_h) } {
                lock(&dev.depth_stencil_states).insert(key, cached);
            }
        }
        obj
    };

    let dsso = make(&info);

    let mut info_depth_readonly = info;
    info_depth_readonly.depth_write_enabled = 0;
    let dsso_depth_readonly = make(&info_depth_readonly);

    let mut info_stencil_readonly = info;
    info_stencil_readonly.back_stencil.stencil_fail_op = WMT_STENCIL_OPERATION_KEEP;
    info_stencil_readonly.back_stencil.depth_stencil_pass_op = WMT_STENCIL_OPERATION_KEEP;
    info_stencil_readonly.back_stencil.depth_fail_op = WMT_STENCIL_OPERATION_KEEP;
    info_stencil_readonly.front_stencil.stencil_fail_op = WMT_STENCIL_OPERATION_KEEP;
    info_stencil_readonly.front_stencil.depth_stencil_pass_op = WMT_STENCIL_OPERATION_KEEP;
    info_stencil_readonly.front_stencil.depth_fail_op = WMT_STENCIL_OPERATION_KEEP;
    let dsso_stencil_readonly = make(&info_stencil_readonly);

    let mut info_readonly = info_stencil_readonly;
    info_readonly.depth_write_enabled = 0;
    let dsso_readonly = make(&info_readonly);

    let mut info_nostencil = info;
    info_nostencil.back_stencil.enabled = 0;
    info_nostencil.front_stencil.enabled = 0;
    let dsso_no_stencil = make(&info_nostencil);

    let mut info_readonly_nostencil = info_nostencil;
    info_readonly_nostencil.depth_write_enabled = 0;
    let dsso_readonly_no_stencil = make(&info_readonly_nostencil);

    [
        dsso,
        dsso_depth_readonly,
        dsso_stencil_readonly,
        dsso_readonly,
        dsso_no_stencil,
        dsso_readonly_no_stencil,
    ]
}

unsafe fn compile_pixel_shader(
    metal: &Obj,
    cache: &crate::shader_cache::ShaderFunctionCache,
    desc: &GraphicsPipelineStateDesc,
    shader_ps: &mut CachedShader,
    colors: &[WMTColorAttachmentBlendInfo; 8],
    dual_source_blending: bool,
) -> Result<Obj, Hresult> {
    let mut common = Sm50ShaderCommonData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };
    let mut pixel_formats = [0u32; 8];
    let num_rt = (desc.num_render_targets as usize).min(8);
    for i in 0..num_rt {
        pixel_formats[i] = original_format(colors[i].pixel_format);
    }
    let mut data_ps = Sm50ShaderPsoPixelShaderData {
        next: (&raw mut common).cast(),
        r#type: SM50_SHADER_PSO_PIXEL_SHADER,
        sample_mask: desc.sample_mask,
        dual_source_blending: u8::from(dual_source_blending),
        disable_depth_output: 0,
        unorm_output_reg_mask: 0,
        pixel_formats,
    };
    let (rs_ptr, rs_len) = if desc.root_signature.is_null() {
        (desc.ps.shader_bytecode, desc.ps.bytecode_length)
    } else {
        // SAFETY: `desc.root_signature` is non-null.
        unsafe { cast_root_signature(desc.root_signature) }.map_or((ptr::null(), 0), |rs| {
            (rs.blob.as_ptr().cast::<c_void>(), rs.blob.len())
        })
    };
    let mut rootsig = Sm50ShaderRootSignatureData {
        next: (&raw mut data_ps).cast(),
        r#type: SM50_SHADER_ROOT_SIGNATURE,
        bytecode: rs_ptr,
        bytecode_length: rs_len,
    };
    let d = shader_ps.digest();
    let ps_name = format!("ps_main{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3]);
    // SAFETY: `rootsig` is a valid null-terminated `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain.
    unsafe {
        compile_function(
            metal,
            cache,
            FunctionKind::Shader,
            shader_ps,
            None,
            (&raw mut rootsig).cast(),
            &CompileTarget {
                name: &ps_name,
                stage: "ps",
            },
        )
    }
}

unsafe fn compile_geometry_variant(
    metal: &Obj,
    cache: &crate::shader_cache::ShaderFunctionCache,
    slot_mask: u32,
    state: &mut GeometryState,
    strip: bool,
    index_format: u32,
) -> Result<Obj, Hresult> {
    let mut common = Sm50ShaderCommonData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };
    let mut gs_data = Sm50ShaderPsoGeometryShaderData {
        next: (&raw mut common).cast(),
        r#type: SM50_SHADER_PSO_GEOMETRY_SHADER,
        strip_topology: u8::from(strip),
    };
    let mut ia = Sm50ShaderIaInputLayoutData {
        next: (&raw mut gs_data).cast(),
        r#type: SM50_SHADER_IA_INPUT_LAYOUT,
        index_buffer_format: index_format,
        slot_mask,
        num_elements: u32::try_from(state.elements.len()).unwrap_or(0),
        elements: state.elements.as_mut_ptr(),
    };
    let mut object_args = Sm50ShaderRootSignatureData {
        next: (&raw mut ia).cast(),
        r#type: SM50_SHADER_ROOT_SIGNATURE,
        bytecode: state.rootsig_blob.as_ptr().cast::<c_void>(),
        bytecode_length: state.rootsig_blob.len(),
    };
    let mut mesh_args = Sm50ShaderRootSignatureData {
        next: (&raw mut gs_data).cast(),
        r#type: SM50_SHADER_ROOT_SIGNATURE,
        bytecode: state.rootsig_blob.as_ptr().cast::<c_void>(),
        bytecode_length: state.rootsig_blob.len(),
    };

    let Some(vs_mut) = Arc::get_mut(&mut state.vertex_shader) else {
        return Err(E_FAIL);
    };
    let Some(gs_mut) = Arc::get_mut(&mut state.geometry_shader) else {
        return Err(E_FAIL);
    };
    // SAFETY: `object_args` is a valid null-terminated `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain.
    let object_function = unsafe {
        compile_function(
            metal,
            cache,
            FunctionKind::GeometryVertex,
            vs_mut,
            Some(gs_mut),
            (&raw mut object_args).cast(),
            &CompileTarget {
                name: "vsgs_main",
                stage: "vs (geometry pipeline)",
            },
        )
    }?;
    // SAFETY: `mesh_args` is a valid null-terminated `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain.
    let mesh_function = unsafe {
        compile_function(
            metal,
            cache,
            FunctionKind::GeometryMesh,
            vs_mut,
            Some(gs_mut),
            (&raw mut mesh_args).cast(),
            &CompileTarget {
                name: "gs_main",
                stage: "gs",
            },
        )
    }?;

    let mut info = state.geometry_info;
    info.object_function = object_function.handle();
    info.mesh_function = mesh_function.handle();
    let mut err_handle = 0u64;
    // SAFETY: `metal` is a live `MTLDevice`; `info` and `err_handle` are valid for the call.
    let pso_handle = unsafe {
        MTLDevice_newMeshRenderPipelineState(metal.handle(), &raw const info, &raw mut err_handle)
    };
    // SAFETY: `err_handle` is 0 or a newly retained `NSError` handle.
    let err_obj = unsafe { Obj::adopt(err_handle) };
    // SAFETY: `pso_handle` is 0 or a newly retained `MTLRenderPipelineState` handle.
    let Some(pso) = (unsafe { Obj::adopt(pso_handle) }) else {
        let msg = err_obj.as_ref().map_or_else(String::new, obj_description);
        crate::err!("Failed to create geometry PSO: {msg}");
        return Err(E_FAIL);
    };
    name_pipeline(
        pso.handle(),
        format!(
            "{} geometry={},{}",
            state.capture_line,
            u8::from(strip),
            index_format
        ),
    );
    Ok(pso)
}

fn build_capture_line(
    desc: &GraphicsPipelineStateDesc,
    vertex_bytes: &[u8],
    pixel_bytes: &[u8],
    geometry_bytes: &[u8],
    rs_blob: Option<&[u8]>,
) -> String {
    let mut line = format!(
        "gfx{}{}{}{}",
        captured_shader("vs", vertex_bytes),
        captured_shader("ps", pixel_bytes),
        captured_shader("gs", geometry_bytes),
        captured_root_signature(rs_blob)
    );
    line.push_str(" rtv=");
    let num_rt = (desc.num_render_targets as usize).min(8);
    for i in 0..num_rt {
        if i > 0 {
            line.push(',');
        }
        let _ = write!(line, "{}", desc.rtv_formats[i]);
    }
    let _ = write!(
        line,
        " dsv={} topology={} samples={}",
        desc.dsv_format, desc.primitive_topology_type, desc.sample_desc.count
    );
    let rt0 = &desc.blend_state.render_target[0];
    if rt0.blend_enable != 0 {
        let _ = write!(
            line,
            " blend0={},{},{},{},{},{}",
            rt0.src_blend,
            rt0.dest_blend,
            rt0.blend_op,
            rt0.src_blend_alpha,
            rt0.dest_blend_alpha,
            rt0.blend_op_alpha
        );
    } else {
        line.push_str(" blend0=off");
    }
    let _ = write!(line, " mask0={}", rt0.render_target_write_mask);
    line
}

fn reflect_optional_ps(
    has_ps: bool,
    pixel_bytes: &[u8],
) -> Result<(Option<Arc<CachedShader>>, MtlShaderReflection), Hresult> {
    // SAFETY: `MtlShaderReflection` is an all-zero-valid POD C struct.
    let mut pixel_refl = unsafe { zeroed::<MtlShaderReflection>() };
    if !has_ps {
        return Ok((None, pixel_refl));
    }
    let mut pixel_shader = CachedShader::new(pixel_bytes)?;
    let Some(ps_mut) = Arc::get_mut(&mut pixel_shader) else {
        return Err(E_FAIL);
    };
    let hr = ps_mut.reflection(&mut pixel_refl);
    if hr < 0 {
        return Err(hr);
    }
    Ok((Some(pixel_shader), pixel_refl))
}

/// A Metal graphics pipeline shared by PSOs with the same descriptor.
pub struct CachedGraphicsPso {
    pub pso: Obj,
    pub slot_mask: u32,
    pub writes_uav: bool,
    pub vertex_digest: Sha1Digest,
    pub pixel_digest: Sha1Digest,
}

struct CompiledPipeline {
    slot_mask: u32,
    writes_uav: bool,
    geometry: bool,
    mtl_pso: Option<Obj>,
    geometry_variants: [Option<Obj>; 6],
    geometry_state: Option<Mutex<GeometryState>>,
    vertex_digest: Sha1Digest,
    pixel_digest: Sha1Digest,
    geometry_digest: Sha1Digest,
}

unsafe fn build_geometry_pso(
    dev: &Device,
    metal: &Obj,
    d: &GraphicsPipelineStateDesc,
    input_elems_slice: &[InputElementDesc],
    rs_blob: Option<&[u8]>,
    capture_line: String,
) -> Result<CompiledPipeline, Hresult> {
    if d.vs.shader_bytecode.is_null() {
        return Err(E_INVALIDARG);
    }
    // SAFETY: `d.vs` and `d.gs` are non-null and valid for their lengths.
    let vertex_bytes = unsafe {
        core::slice::from_raw_parts(d.vs.shader_bytecode.cast::<u8>(), d.vs.bytecode_length)
    };
    // SAFETY: `d.gs.shader_bytecode` was checked non-null by caller.
    let geometry_bytes = unsafe {
        core::slice::from_raw_parts(d.gs.shader_bytecode.cast::<u8>(), d.gs.bytecode_length)
    };
    let pixel_bytes = if d.ps.shader_bytecode.is_null() || d.ps.bytecode_length == 0 {
        &[][..]
    } else {
        // SAFETY: `d.ps` is non-null for `bytecode_length` bytes.
        unsafe {
            core::slice::from_raw_parts(d.ps.shader_bytecode.cast::<u8>(), d.ps.bytecode_length)
        }
    };
    let mut vertex_shader = CachedShader::new(vertex_bytes)?;
    let mut geometry_shader = CachedShader::new(geometry_bytes)?;
    // SAFETY: `MtlShaderReflection` is an all-zero-valid POD C struct.
    let mut vertex_refl = unsafe { zeroed::<MtlShaderReflection>() };
    // SAFETY: `MtlShaderReflection` is an all-zero-valid POD C struct.
    let mut geometry_refl = unsafe { zeroed::<MtlShaderReflection>() };
    let vertex_hr = Arc::get_mut(&mut vertex_shader)
        .ok_or(E_FAIL)?
        .reflection(&mut vertex_refl);
    if vertex_hr < 0 {
        return Err(vertex_hr);
    }
    let geometry_hr = Arc::get_mut(&mut geometry_shader)
        .ok_or(E_FAIL)?
        .reflection(&mut geometry_refl);
    if geometry_hr < 0 {
        return Err(geometry_hr);
    }
    // SAFETY: `vertex_bytes` and `input_elems_slice` are valid.
    let (elements, slot_mask) = unsafe { extract_input_layout(vertex_bytes, input_elems_slice) }?;
    let rootsig_blob = rs_blob.map_or_else(|| vertex_bytes.to_vec(), <[u8]>::to_vec);
    let (mut pixel_shader_opt, pixel_refl) =
        reflect_optional_ps(!d.ps.shader_bytecode.is_null(), pixel_bytes)?;
    let writes_uav =
        vertex_refl.writes_uav != 0 || geometry_refl.writes_uav != 0 || pixel_refl.writes_uav != 0;
    // SAFETY: Pixel shader reflection populates the `pixel_shader` union arm.
    let has_cov = unsafe { pixel_refl.u.pixel_shader.has_coverage_output } != 0;

    let mut geo_info = mesh_render_pipeline_info();
    let mut dual_source = false;
    let hr = initialize_pso_fields(
        dev,
        d,
        has_cov,
        &mut PsoCommonTarget {
            colors: &mut geo_info.colors,
            depth_pixel_format: &mut geo_info.depth_pixel_format,
            stencil_pixel_format: &mut geo_info.stencil_pixel_format,
            input_primitive_topology: None,
            raster_sample_count: &mut geo_info.raster_sample_count,
            support_icb: &mut geo_info.support_indirect_command_buffers,
            alpha_to_coverage_enabled: &mut geo_info.alpha_to_coverage_enabled,
            dual_source_blending: &mut dual_source,
        },
    );
    if hr < 0 {
        return Err(hr);
    }
    let mut pixel_digest = [0u8; 20];
    let ps_func = if let Some(pixel_shader) = pixel_shader_opt.as_mut() {
        let ps_mut = Arc::get_mut(pixel_shader).ok_or(E_FAIL)?;
        // SAFETY: `metal`, `d`, `ps_mut`, and `geo_info.colors` are valid.
        let f = unsafe {
            compile_pixel_shader(
                metal,
                &dev.shader_functions,
                d,
                ps_mut,
                &geo_info.colors,
                dual_source,
            )
        }?;
        pixel_digest = *ps_mut.digest();
        Some(f)
    } else {
        None
    };
    let vertex_digest = *vertex_shader.digest();
    let geometry_digest = *geometry_shader.digest();
    geo_info.fragment_function = ps_func.as_ref().map_or(0, Obj::handle);
    geo_info.payload_memory_length = 16256;
    geo_info.rasterization_enabled = u8::from(!d.ps.shader_bytecode.is_null() || d.dsv_format != 0);
    geo_info.support_indirect_command_buffers = 0;

    let mut state = GeometryState {
        vertex_shader,
        geometry_shader,
        elements,
        rootsig_blob,
        _ps_func: ps_func,
        geometry_info: geo_info,
        capture_line,
        failed: [false; 6],
    };
    // SAFETY: `metal` and `state` are valid.
    let default_variant = unsafe {
        compile_geometry_variant(
            metal,
            &dev.shader_functions,
            slot_mask,
            &mut state,
            false,
            SM50_INDEX_BUFFER_FORMAT_NONE,
        )
    }?;
    let mut geometry_variants: [Option<Obj>; 6] = [const { None }; 6];
    geometry_variants[0] = Some(default_variant);
    Ok(CompiledPipeline {
        slot_mask,
        writes_uav,
        geometry: true,
        mtl_pso: None,
        geometry_variants,
        geometry_state: Some(Mutex::new(state)),
        vertex_digest,
        pixel_digest,
        geometry_digest,
    })
}

unsafe fn build_vertex_pso(
    dev: &Device,
    metal: &Obj,
    d: &GraphicsPipelineStateDesc,
    input_elems_slice: &[InputElementDesc],
    rs_blob: Option<&[u8]>,
    capture_line: String,
) -> Result<CompiledPipeline, Hresult> {
    if d.vs.shader_bytecode.is_null() {
        crate::err!("no vertex shader");
        return Err(E_INVALIDARG);
    }
    // SAFETY: `d.vs` is non-null for `bytecode_length` bytes.
    let vertex_bytes = unsafe {
        core::slice::from_raw_parts(d.vs.shader_bytecode.cast::<u8>(), d.vs.bytecode_length)
    };
    let pixel_bytes = if d.ps.shader_bytecode.is_null() || d.ps.bytecode_length == 0 {
        &[][..]
    } else {
        // SAFETY: `d.ps` is non-null for `bytecode_length` bytes.
        unsafe {
            core::slice::from_raw_parts(d.ps.shader_bytecode.cast::<u8>(), d.ps.bytecode_length)
        }
    };
    let mut vertex_shader = CachedShader::new(vertex_bytes)?;
    // SAFETY: `MtlShaderReflection` is an all-zero-valid POD C struct.
    let mut vertex_refl = unsafe { zeroed::<MtlShaderReflection>() };
    let vs_mut = Arc::get_mut(&mut vertex_shader).ok_or(E_FAIL)?;
    let hr = vs_mut.reflection(&mut vertex_refl);
    if hr < 0 {
        return Err(hr);
    }
    // SAFETY: `vertex_bytes` and `input_elems_slice` are valid.
    let (mut elements, slot_mask) =
        unsafe { extract_input_layout(vertex_bytes, input_elems_slice) }?;

    let mut common = Sm50ShaderCommonData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };
    let mut data_ia = Sm50ShaderIaInputLayoutData {
        next: (&raw mut common).cast(),
        r#type: SM50_SHADER_IA_INPUT_LAYOUT,
        index_buffer_format: SM50_INDEX_BUFFER_FORMAT_NONE,
        slot_mask,
        num_elements: u32::try_from(elements.len()).unwrap_or(0),
        elements: elements.as_mut_ptr(),
    };
    let (rs_ptr, rs_len) = rs_blob.map_or((d.vs.shader_bytecode, d.vs.bytecode_length), |b| {
        (b.as_ptr().cast::<c_void>(), b.len())
    });
    let mut rootsig = Sm50ShaderRootSignatureData {
        next: (&raw mut data_ia).cast(),
        r#type: SM50_SHADER_ROOT_SIGNATURE,
        bytecode: rs_ptr,
        bytecode_length: rs_len,
    };
    // SAFETY: `rootsig` is a valid null-terminated `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain.
    let vs_func = unsafe {
        compile_function(
            metal,
            &dev.shader_functions,
            FunctionKind::Shader,
            vs_mut,
            None,
            (&raw mut rootsig).cast(),
            &CompileTarget {
                name: "vs_main",
                stage: "vs",
            },
        )
    }?;
    let vertex_digest = *vs_mut.digest();

    let (mut pixel_shader_opt, pixel_refl) =
        reflect_optional_ps(!d.ps.shader_bytecode.is_null(), pixel_bytes)?;
    let writes_uav = vertex_refl.writes_uav != 0 || pixel_refl.writes_uav != 0;
    // SAFETY: Pixel shader reflection populates the `pixel_shader` union arm.
    let has_cov = unsafe { pixel_refl.u.pixel_shader.has_coverage_output } != 0;

    let mut info = render_pipeline_info();
    let mut dual_source = false;
    let hr = initialize_pso_fields(
        dev,
        d,
        has_cov,
        &mut PsoCommonTarget {
            colors: &mut info.colors,
            depth_pixel_format: &mut info.depth_pixel_format,
            stencil_pixel_format: &mut info.stencil_pixel_format,
            input_primitive_topology: Some(&mut info.input_primitive_topology),
            raster_sample_count: &mut info.raster_sample_count,
            support_icb: &mut info.support_indirect_command_buffers,
            alpha_to_coverage_enabled: &mut info.alpha_to_coverage_enabled,
            dual_source_blending: &mut dual_source,
        },
    );
    if hr < 0 {
        return Err(hr);
    }

    let mut pixel_digest = [0u8; 20];
    let ps_func = if let Some(pixel_shader) = pixel_shader_opt.as_mut() {
        let ps_mut = Arc::get_mut(pixel_shader).ok_or(E_FAIL)?;
        // SAFETY: `metal`, `d`, `ps_mut`, and `info.colors` are valid.
        let f = unsafe {
            compile_pixel_shader(
                metal,
                &dev.shader_functions,
                d,
                ps_mut,
                &info.colors,
                dual_source,
            )
        }?;
        pixel_digest = *ps_mut.digest();
        Some(f)
    } else {
        None
    };

    info.vertex_function = vs_func.handle();
    info.fragment_function = ps_func.as_ref().map_or(0, Obj::handle);
    info.immutable_vertex_buffers = IMMUTABLE_VERTEX_BUFFERS;
    info.immutable_fragment_buffers = IMMUTABLE_ROOT_BUFFERS;
    let mut err_handle = 0u64;
    // SAFETY: `metal` is a live `MTLDevice`; `info` and `err_handle` are valid for the call.
    let pso_handle = unsafe {
        MTLDevice_newRenderPipelineState(metal.handle(), &raw const info, &raw mut err_handle)
    };
    // SAFETY: `err_handle` is 0 or a newly retained `NSError` handle.
    let err_obj = unsafe { Obj::adopt(err_handle) };
    // SAFETY: `pso_handle` is 0 or a newly retained `MTLRenderPipelineState` handle.
    let Some(p) = (unsafe { Obj::adopt(pso_handle) }) else {
        let msg = err_obj.as_ref().map_or_else(String::new, obj_description);
        crate::err!("Failed to create PSO: {msg}");
        return Err(E_FAIL);
    };
    name_pipeline(p.handle(), capture_line);
    Ok(CompiledPipeline {
        slot_mask,
        writes_uav,
        geometry: false,
        mtl_pso: Some(p),
        geometry_variants: [const { None }; 6],
        geometry_state: None,
        vertex_digest,
        pixel_digest,
        geometry_digest: [0; 20],
    })
}

/// `ID3D12Device::CreateGraphicsPipelineState` (`d3d12_device.cpp:632`,
///
/// `d3d12_pipeline_graphics.cpp:815`).
///
/// # Safety
///
/// `this`, `desc`, `riid`, and `pso` are caller-supplied COM pointers.
pub unsafe fn create_graphics_pipeline_state(
    this: *mut c_void,
    desc: *const c_void,
    riid: *const c_void,
    pso: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateGraphicsPipelineState");
    // SAFETY: `pso` is a caller-supplied out-pointer or null.
    unsafe { init_out_ptr(pso) };
    if this.is_null() || desc.is_null() {
        return E_INVALIDARG;
    }
    let device_ptr = this.cast::<Device>();
    // SAFETY: `device_ptr` is non-null and points to a live `Device`.
    let dev = unsafe { &*device_ptr };
    // SAFETY: `desc` is non-null and points to a `GraphicsPipelineStateDesc`.
    let d = unsafe { &*desc.cast::<GraphicsPipelineStateDesc>() };

    let slice_of = |ptr: *const c_void, len: usize| -> &[u8] {
        if ptr.is_null() || len == 0 {
            &[]
        } else {
            // SAFETY: Caller guarantees non-null `ptr` is valid for `len` bytes.
            unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), len) }
        }
    };
    let vertex_bytes = slice_of(d.vs.shader_bytecode, d.vs.bytecode_length);
    let pixel_bytes = slice_of(d.ps.shader_bytecode, d.ps.bytecode_length);
    let geometry_bytes = slice_of(d.gs.shader_bytecode, d.gs.bytecode_length);
    let hull_bytes = slice_of(d.hs.shader_bytecode, d.hs.bytecode_length);
    let domain_bytes = slice_of(d.ds.shader_bytecode, d.ds.bytecode_length);

    for s in [
        vertex_bytes,
        pixel_bytes,
        geometry_bytes,
        hull_bytes,
        domain_bytes,
    ] {
        if !s.is_empty() {
            dump_dxil(s);
        }
    }

    let rs_blob: Option<&[u8]> = if d.root_signature.is_null() {
        None
    } else {
        // SAFETY: `d.root_signature` is non-null.
        unsafe { cast_root_signature(d.root_signature) }.map(|rs| &*rs.blob)
    };
    let capture_line = if dxil_capture_mode() {
        let line = build_capture_line(d, vertex_bytes, pixel_bytes, geometry_bytes, rs_blob);
        log_pipeline(&line);
        line
    } else {
        String::new()
    };

    if d.stream_output.num_entries != 0 {
        crate::err!("CreatePipelineState: SO not supported");
        return E_NOTIMPL;
    }
    if !d.hs.shader_bytecode.is_null() || !d.ds.shader_bytecode.is_null() {
        crate::err!("CreatePipelineState: Tess not supported");
        return E_NOTIMPL;
    }

    let Some(metal) = dev.mtl.as_ref() else {
        return E_FAIL;
    };

    let input_elems_slice: &[InputElementDesc] =
        if d.input_layout.input_element_descs.is_null() || d.input_layout.num_elements == 0 {
            &[]
        } else {
            // SAFETY: `input_element_descs` is non-null for `num_elements` elements.
            unsafe {
                core::slice::from_raw_parts(
                    d.input_layout.input_element_descs,
                    d.input_layout.num_elements as usize,
                )
            }
        };

    let desc_hash = super::cached_desc_hash(
        d.cached_pso.cached_blob,
        d.cached_pso.cached_blob_size_in_bytes,
    )
    // SAFETY: `desc` is valid.
    .unwrap_or_else(|| unsafe { hash_graphics_desc(desc) });
    let standard = d.gs.shader_bytecode.is_null()
        && d.hs.shader_bytecode.is_null()
        && d.ds.shader_bytecode.is_null();
    let cached = if standard {
        let cache = lock(&dev.graphics_psos);
        cache.get(&desc_hash).map(|cached| {
            // SAFETY: the cached `Obj` holds a live Metal pipeline.
            let pso = unsafe { Obj::retain(cached.pso.handle()) };
            (
                pso,
                cached.slot_mask,
                cached.writes_uav,
                cached.vertex_digest,
                cached.pixel_digest,
            )
        })
    } else {
        None
    };

    let compiled = if let Some((pso, slot_mask, writes_uav, vertex_digest, pixel_digest)) = cached {
        CompiledPipeline {
            slot_mask,
            writes_uav,
            geometry: false,
            mtl_pso: pso,
            geometry_variants: [const { None }; 6],
            geometry_state: None,
            vertex_digest,
            pixel_digest,
            geometry_digest: [0; 20],
        }
    } else {
        let compiled = if standard {
            // SAFETY: `dev`, `metal`, `d`, and `input_elems_slice` are valid.
            unsafe { build_vertex_pso(dev, metal, d, input_elems_slice, rs_blob, capture_line) }
        } else {
            // SAFETY: `dev`, `metal`, `d`, and `input_elems_slice` are valid.
            unsafe { build_geometry_pso(dev, metal, d, input_elems_slice, rs_blob, capture_line) }
        };
        let compiled = match compiled {
            Ok(c) => c,
            Err(hr) => return hr,
        };
        if standard && let Some(pso) = compiled.mtl_pso.as_ref() {
            // SAFETY: `pso` is live; the cache takes a reference of its own.
            if let Some(cached_pso) = unsafe { Obj::retain(pso.handle()) } {
                lock(&dev.graphics_psos).insert(
                    desc_hash,
                    CachedGraphicsPso {
                        pso: cached_pso,
                        slot_mask: compiled.slot_mask,
                        writes_uav: compiled.writes_uav,
                        vertex_digest: compiled.vertex_digest,
                        pixel_digest: compiled.pixel_digest,
                    },
                );
            }
        }
        compiled
    };

    let depth_stencil_states = initialize_dsso(dev, metal, d);
    if recording_on() {
        // SAFETY: `d.root_signature` is null or a valid `ID3D12RootSignature`.
        let root_rec = unsafe { root_blob(d.root_signature) };
        // SAFETY: `d.vs` is valid.
        let vertex_rec = unsafe { shader_blob(compiled.vertex_digest, &d.vs) };
        // SAFETY: `d.ps` is valid.
        let pixel_rec = unsafe { shader_blob(compiled.pixel_digest, &d.ps) };
        // SAFETY: `d.gs` is valid.
        let geometry_rec = unsafe { shader_blob(compiled.geometry_digest, &d.gs) };
        // SAFETY: `desc` is valid.
        unsafe { record_graphics(desc, root_rec, vertex_rec, pixel_rec, geometry_rec) };
    }

    let fill_mode = if d.rasterizer_state.fill_mode == FILL_MODE_SOLID {
        u32::from(WMT_TRIANGLE_FILL_MODE_FILL)
    } else {
        u32::from(WMT_TRIANGLE_FILL_MODE_LINES)
    };
    let cull_mode = match d.rasterizer_state.cull_mode {
        2 => u32::from(WMT_CULL_MODE_FRONT),
        3 => u32::from(WMT_CULL_MODE_BACK),
        _ => u32::from(WMT_CULL_MODE_NONE),
    };
    let depth_clip_mode = if d.rasterizer_state.depth_clip_enable != 0 {
        u32::from(WMT_DEPTH_CLIP_MODE_CLIP)
    } else {
        u32::from(WMT_DEPTH_CLIP_MODE_CLAMP)
    };
    let winding = if d.rasterizer_state.front_counter_clockwise != 0 {
        u32::from(WMT_WINDING_COUNTER_CLOCKWISE)
    } else {
        u32::from(WMT_WINDING_CLOCKWISE)
    };

    // SAFETY: `device_ptr` is a live `Device`.
    unsafe { add_ref_com(device_ptr) };
    let raw = Box::into_raw(Box::new(Graphics {
        vtbl: &VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: device_ptr,
        is_compute: false,
        desc_hash,
        pso: compiled.mtl_pso,
        slot_mask: compiled.slot_mask,
        fill_mode,
        cull_mode,
        depth_clip_mode,
        winding,
        depth_bias: i32_to_f32(d.rasterizer_state.depth_bias),
        slope_scale: d.rasterizer_state.slope_scaled_depth_bias,
        depth_bias_clamp: d.rasterizer_state.depth_bias_clamp,
        forced_sample_count: d.rasterizer_state.forced_sample_count,
        geometry: compiled.geometry,
        writes_uav: compiled.writes_uav,
        depth_stencil_states,
        geometry_variants: Mutex::new(compiled.geometry_variants),
        geometry_state: compiled.geometry_state,
    }));
    // SAFETY: `raw` is a freshly allocated `Graphics` with 1 initial public ref.
    let hr = unsafe { (VTBL.query_interface)(raw.cast::<c_void>(), riid, pso) };
    // SAFETY: Release the initial ref from `ComObject::new()`.
    unsafe { release_com(raw) };
    hr
}

#[cfg(test)]
impl Graphics {
    /// Allocates a dummy graphics pipeline state for unit tests.
    #[must_use]
    pub fn new_dummy(vtbl: &'static ID3D12PipelineStateVtbl, pso: Option<Obj>) -> Box<Self> {
        Box::new(Self {
            vtbl,
            com: ComObject::with_initial_ref(),
            private_data: PrivateData::default(),
            device: ptr::null_mut(),
            is_compute: false,
            desc_hash: 0,
            pso,
            slot_mask: 0,
            fill_mode: 0,
            cull_mode: 0,
            depth_clip_mode: 0,
            winding: 0,
            depth_bias: 0.0,
            slope_scale: 0.0,
            depth_bias_clamp: 0.0,
            forced_sample_count: 0,
            geometry: false,
            writes_uav: false,
            depth_stencil_states: [const { None }; 6],
            geometry_variants: Mutex::new([const { None }; 6]),
            geometry_state: None,
        })
    }
}
