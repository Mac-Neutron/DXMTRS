//! `MTLD3D10Device` (`ID3D10Device1` wrapper around `ID3D11Device` and `ID3D11DeviceContext`).

use core::{
    ffi::{c_char, c_void},
    ptr,
};

use dxmt_util::{
    com::{E_FAIL, E_INVALIDARG, E_NOTIMPL, Guid, S_OK},
    float_cast::{f32_to_i32, f32_to_u32, i32_to_f32, u32_to_f32},
};

use super::{
    d3d11_of,
    util::{
        d10_to_d11_blend_desc, d10_to_d11_buffer_desc, d10_to_d11_dsv_desc, d10_to_d11_query_desc,
        d10_to_d11_sampler_desc, d10_to_d11_tex1d_desc, d10_to_d11_tex2d_desc,
        d10_to_d11_tex3d_desc, wrapper_context, wrapper_d3d11,
    },
};
use crate::{
    abi::{
        IID_ID3D10_BLEND_STATE, IID_ID3D10_BLEND_STATE_1, IID_ID3D10_BUFFER,
        IID_ID3D10_DEPTH_STENCIL_STATE, IID_ID3D10_DEPTH_STENCIL_VIEW, IID_ID3D10_GEOMETRY_SHADER,
        IID_ID3D10_INPUT_LAYOUT, IID_ID3D10_PIXEL_SHADER, IID_ID3D10_PREDICATE, IID_ID3D10_QUERY,
        IID_ID3D10_RASTERIZER_STATE, IID_ID3D10_RENDER_TARGET_VIEW, IID_ID3D10_SAMPLER_STATE,
        IID_ID3D10_SHADER_RESOURCE_VIEW, IID_ID3D10_SHADER_RESOURCE_VIEW_1, IID_ID3D10_TEXTURE_1D,
        IID_ID3D10_TEXTURE_2D, IID_ID3D10_TEXTURE_3D, IID_ID3D10_VERTEX_SHADER, IID_ID3D11_QUERY,
        ext::IMTLD3D11DeviceContextVtbl,
        records::{
            D3d10BlendDesc, D3d10BufferDesc, D3d10CounterInfo, D3d10DepthStencilViewDesc,
            D3d10QueryDesc, D3d10SamplerDesc, D3d10SoDeclarationEntry, D3d10Texture1dDesc,
            D3d10Texture2dDesc, D3d10Texture3dDesc, D3d10Viewport, D3d11SoDeclarationEntry,
            D3d11Viewport,
        },
        vtbl::{ID3D10Device1Vtbl, ID3D11DeviceContextVtbl, ID3D11DeviceVtbl},
    },
    com::{self, IUnknownVtbl},
};

/// `D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT`.
const MAX_CB_SLOTS: usize = 14;
/// `D3D10_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT`.
const MAX_SRV_SLOTS: usize = 128;
/// `D3D10_COMMONSHADER_SAMPLER_SLOT_COUNT`.
const MAX_SAMPLER_SLOTS: usize = 16;
/// `D3D10_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT`.
const MAX_VB_SLOTS: usize = 16;
/// `D3D10_SIMULTANEOUS_RENDER_TARGET_COUNT`.
const MAX_RTV_SLOTS: usize = 8;
/// `D3D10_SO_BUFFER_SLOT_COUNT`.
const MAX_SO_SLOTS: usize = 4;
/// `D3D10_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE`.
const MAX_VIEWPORT_SLOTS: usize = 16;

/// `MTLD3D10Device`: the `ID3D10Device1` subobject owned by the D3D11 device (`d3d10_device.hpp`).
#[repr(C)]
pub struct Device {
    pub vtbl: &'static ID3D10Device1Vtbl,
    pub d3d11: *mut c_void,
    pub context: *mut c_void,
}

impl Device {
    /// Allocates a new `ID3D10Device1` wrapper around `d3d11` and `context`.
    #[must_use]
    pub fn new(d3d11: *mut c_void, context: *mut c_void) -> Box<Self> {
        Box::new(Self {
            vtbl: &DEVICE_VTBL,
            d3d11,
            context,
        })
    }
}

/// Unwraps up to `N` `ID3D10*` pointers into a stack array of `ID3D11*` pointers.
///
/// # Safety
///
/// If `src` is non-null, it must point to at least `count.min(N)` nullable `Wrapper<V>` pointers.
unsafe fn unwrap_array<const N: usize>(count: u32, src: *const *mut c_void) -> [*mut c_void; N] {
    let mut dst = [ptr::null_mut(); N];
    if !src.is_null() {
        let n = (count as usize).min(N);
        // SAFETY: `src` is non-null and has at least `n` elements.
        let slice = unsafe { core::slice::from_raw_parts(src, n) };
        let mut i = 0;
        while i < n {
            // SAFETY: if non-null, `slice[i]` is a `Wrapper<V>` COM pointer.
            dst[i] = unsafe { d3d11_of(slice[i]) };
            i += 1;
        }
    }
    dst
}

/// Converts a nullable `ID3D11*` pointer into an `ID3D10*` out-pointer.
///
/// Queries `iid` into `out` and releases the temporary `ID3D11*` reference.
///
/// # Safety
///
/// If `d3d11_obj` is non-null, it must be a live COM object with an owned reference. If `out` is
/// non-null, it must be writable.
unsafe fn wrap_out_ptr(d3d11_obj: *mut c_void, iid: &Guid, out: *mut *mut c_void) {
    if d3d11_obj.is_null() {
        if !out.is_null() {
            // SAFETY: `out` is a non-null caller out-slot.
            unsafe { *out = ptr::null_mut() };
        }
        return;
    }
    // SAFETY: `d3d11_obj` is a live COM object with an owned reference.
    let unk = unsafe { com::vtbl::<IUnknownVtbl>(d3d11_obj) };
    if !out.is_null() {
        // SAFETY: `d3d11_obj` is live and `out` is the caller's out-slot.
        unsafe { (unk.query_interface)(d3d11_obj, iid, out) };
    }
    // SAFETY: releases the temporary reference on `d3d11_obj`.
    unsafe { (unk.release)(d3d11_obj) };
}

/// Converts up to `N` `ID3D11*` pointers returned by a context getter into `ID3D10*` pointers.
///
/// # Safety
///
/// Each non-null entry in `d3d11_arr[..count.min(N)]` must be a live COM object with an owned
/// reference. If `out_arr` is non-null, it must have at least `count.min(N)` writable slots.
unsafe fn wrap_out_array<const N: usize>(
    count: u32,
    d3d11_arr: &[*mut c_void; N],
    iid: &Guid,
    out_arr: *mut *mut c_void,
) {
    let n = (count as usize).min(N);
    let mut i = 0;
    while i < n {
        let slot = if out_arr.is_null() {
            ptr::null_mut()
        } else {
            // SAFETY: `out_arr` has at least `n` elements when non-null.
            unsafe { out_arr.add(i) }
        };
        // SAFETY: `d3d11_arr[i]` is a nullable COM object with an owned reference and `slot` is
        // either null or writable.
        unsafe { wrap_out_ptr(d3d11_arr[i], iid, slot) };
        i += 1;
    }
}

/// Initializes a COM out-pointer to null when non-null (`InitReturnPtr`).
///
/// # Safety
///
/// If non-null, `pp_out` must be writable.
unsafe fn init_return_ptr(pp_out: *mut *mut c_void) {
    if !pp_out.is_null() {
        // SAFETY: `pp_out` is a non-null caller out-slot.
        unsafe { *pp_out = ptr::null_mut() };
    }
}

/// Returns `ptr::null_mut()` when `pp_out` is null, or a pointer to `local` otherwise.
const fn out_slot(pp_out: *mut *mut c_void, local: &mut *mut c_void) -> *mut *mut c_void {
    if pp_out.is_null() {
        ptr::null_mut()
    } else {
        &raw mut *local
    }
}

/// Completes a `Create*` call by querying `iid` from `d3d11_obj` into `pp_out`.
///
/// Releases the temporary reference on `d3d11_obj` before returning `hr`.
///
/// # Safety
///
/// If `hr == S_OK` and `d3d11_obj` is non-null, `d3d11_obj` must be a live COM object with an
/// owned reference and `pp_out` (if non-null) must be writable.
unsafe fn finish_create(
    hr: i32,
    d3d11_obj: *mut c_void,
    iid: &Guid,
    pp_out: *mut *mut c_void,
) -> i32 {
    if hr != S_OK {
        return hr;
    }
    if !pp_out.is_null() && !d3d11_obj.is_null() {
        // SAFETY: `d3d11_obj` was created with a live reference by `ID3D11Device::Create*`.
        let unk = unsafe { com::vtbl::<IUnknownVtbl>(d3d11_obj) };
        // SAFETY: queries the requested `ID3D10*` wrapper into `pp_out`.
        let qi_hr = unsafe { (unk.query_interface)(d3d11_obj, iid, pp_out) };
        // SAFETY: releases the temporary `ID3D11*` reference.
        unsafe { (unk.release)(d3d11_obj) };
        if qi_hr != S_OK {
            return E_FAIL;
        }
    }
    hr
}

unsafe extern "system-unwind" fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    ppv_object: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `QueryInterface` to the underlying `ID3D11Device`.
    unsafe { (vtbl.query_interface)(d3d11, riid, ppv_object) }
}

unsafe extern "system-unwind" fn add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `AddRef` to the underlying `ID3D11Device`.
    unsafe { (vtbl.add_ref)(d3d11) }
}

unsafe extern "system-unwind" fn release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `Release` to the underlying `ID3D11Device`.
    unsafe { (vtbl.release)(d3d11) }
}

unsafe extern "system-unwind" fn vs_set_constant_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_constant_buffers: *const *mut c_void,
) {
    if (num_buffers as usize) > MAX_CB_SLOTS {
        return;
    }
    // SAFETY: `pp_constant_buffers` has up to `num_buffers` elements when non-null.
    let bufs = unsafe { unwrap_array::<MAX_CB_SLOTS>(num_buffers, pp_constant_buffers) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped constant buffers to `VSSetConstantBuffers`.
    unsafe { (vtbl.vs_set_constant_buffers)(ctx, start_slot, num_buffers, bufs.as_ptr()) };
}

unsafe extern "system-unwind" fn ps_set_shader_resources(
    this: *mut c_void,
    start_slot: u32,
    num_views: u32,
    pp_shader_resource_views: *const *mut c_void,
) {
    if (num_views as usize) > MAX_SRV_SLOTS {
        return;
    }
    // SAFETY: `pp_shader_resource_views` has up to `num_views` elements when non-null.
    let views = unsafe { unwrap_array::<MAX_SRV_SLOTS>(num_views, pp_shader_resource_views) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped SRVs to `PSSetShaderResources`.
    unsafe { (vtbl.ps_set_shader_resources)(ctx, start_slot, num_views, views.as_ptr()) };
}

unsafe extern "system-unwind" fn ps_set_shader(this: *mut c_void, p_pixel_shader: *mut c_void) {
    // SAFETY: `p_pixel_shader` is a nullable `Wrapper<ID3D10PixelShaderVtbl>`.
    let shader = unsafe { d3d11_of(p_pixel_shader) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped shader to `PSSetShader`.
    unsafe { (vtbl.ps_set_shader)(ctx, shader, ptr::null(), 0) };
}

unsafe extern "system-unwind" fn ps_set_samplers(
    this: *mut c_void,
    start_slot: u32,
    num_samplers: u32,
    pp_samplers: *const *mut c_void,
) {
    if (num_samplers as usize) > MAX_SAMPLER_SLOTS {
        return;
    }
    // SAFETY: `pp_samplers` has up to `num_samplers` elements when non-null.
    let samplers = unsafe { unwrap_array::<MAX_SAMPLER_SLOTS>(num_samplers, pp_samplers) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped samplers to `PSSetSamplers`.
    unsafe { (vtbl.ps_set_samplers)(ctx, start_slot, num_samplers, samplers.as_ptr()) };
}

unsafe extern "system-unwind" fn vs_set_shader(this: *mut c_void, p_vertex_shader: *mut c_void) {
    // SAFETY: `p_vertex_shader` is a nullable `Wrapper<ID3D10VertexShaderVtbl>`.
    let shader = unsafe { d3d11_of(p_vertex_shader) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped shader to `VSSetShader`.
    unsafe { (vtbl.vs_set_shader)(ctx, shader, ptr::null(), 0) };
}

unsafe extern "system-unwind" fn draw_indexed(
    this: *mut c_void,
    index_count: u32,
    start_index_location: u32,
    base_vertex_location: i32,
) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `DrawIndexed` to the immediate context.
    unsafe { (vtbl.draw_indexed)(ctx, index_count, start_index_location, base_vertex_location) };
}

unsafe extern "system-unwind" fn draw(
    this: *mut c_void,
    vertex_count: u32,
    start_vertex_location: u32,
) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `Draw` to the immediate context.
    unsafe { (vtbl.draw)(ctx, vertex_count, start_vertex_location) };
}

unsafe extern "system-unwind" fn ps_set_constant_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_constant_buffers: *const *mut c_void,
) {
    if (num_buffers as usize) > MAX_CB_SLOTS {
        return;
    }
    // SAFETY: `pp_constant_buffers` has up to `num_buffers` elements when non-null.
    let bufs = unsafe { unwrap_array::<MAX_CB_SLOTS>(num_buffers, pp_constant_buffers) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped constant buffers to `PSSetConstantBuffers`.
    unsafe { (vtbl.ps_set_constant_buffers)(ctx, start_slot, num_buffers, bufs.as_ptr()) };
}

unsafe extern "system-unwind" fn ia_set_input_layout(
    this: *mut c_void,
    p_input_layout: *mut c_void,
) {
    // SAFETY: `p_input_layout` is a nullable `Wrapper<ID3D10InputLayoutVtbl>`.
    let layout = unsafe { d3d11_of(p_input_layout) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped input layout to `IASetInputLayout`.
    unsafe { (vtbl.ia_set_input_layout)(ctx, layout) };
}

unsafe extern "system-unwind" fn ia_set_vertex_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_vertex_buffers: *const *mut c_void,
    p_strides: *const u32,
    p_offsets: *const u32,
) {
    if (num_buffers as usize) > MAX_VB_SLOTS {
        return;
    }
    // SAFETY: `pp_vertex_buffers` has up to `num_buffers` elements when non-null.
    let bufs = unsafe { unwrap_array::<MAX_VB_SLOTS>(num_buffers, pp_vertex_buffers) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped vertex buffers to `IASetVertexBuffers`.
    unsafe {
        (vtbl.ia_set_vertex_buffers)(
            ctx,
            start_slot,
            num_buffers,
            bufs.as_ptr(),
            p_strides,
            p_offsets,
        );
    };
}

unsafe extern "system-unwind" fn ia_set_index_buffer(
    this: *mut c_void,
    p_index_buffer: *mut c_void,
    format: u32,
    offset: u32,
) {
    // SAFETY: `p_index_buffer` is a nullable `Wrapper<ID3D10BufferVtbl>`.
    let buf = unsafe { d3d11_of(p_index_buffer) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped index buffer to `IASetIndexBuffer`.
    unsafe { (vtbl.ia_set_index_buffer)(ctx, buf, format, offset) };
}

unsafe extern "system-unwind" fn draw_indexed_instanced(
    this: *mut c_void,
    index_count_per_instance: u32,
    instance_count: u32,
    start_index_location: u32,
    base_vertex_location: i32,
    start_instance_location: u32,
) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `DrawIndexedInstanced` to the immediate context.
    unsafe {
        (vtbl.draw_indexed_instanced)(
            ctx,
            index_count_per_instance,
            instance_count,
            start_index_location,
            base_vertex_location,
            start_instance_location,
        );
    };
}

unsafe extern "system-unwind" fn draw_instanced(
    this: *mut c_void,
    vertex_count_per_instance: u32,
    instance_count: u32,
    start_vertex_location: u32,
    start_instance_location: u32,
) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `DrawInstanced` to the immediate context.
    unsafe {
        (vtbl.draw_instanced)(
            ctx,
            vertex_count_per_instance,
            instance_count,
            start_vertex_location,
            start_instance_location,
        );
    };
}

unsafe extern "system-unwind" fn gs_set_constant_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_constant_buffers: *const *mut c_void,
) {
    if (num_buffers as usize) > MAX_CB_SLOTS {
        return;
    }
    // SAFETY: `pp_constant_buffers` has up to `num_buffers` elements when non-null.
    let bufs = unsafe { unwrap_array::<MAX_CB_SLOTS>(num_buffers, pp_constant_buffers) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped constant buffers to `GSSetConstantBuffers`.
    unsafe { (vtbl.gs_set_constant_buffers)(ctx, start_slot, num_buffers, bufs.as_ptr()) };
}

unsafe extern "system-unwind" fn gs_set_shader(this: *mut c_void, p_shader: *mut c_void) {
    // SAFETY: `p_shader` is a nullable `Wrapper<ID3D10GeometryShaderVtbl>`.
    let shader = unsafe { d3d11_of(p_shader) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped geometry shader to `GSSetShader`.
    unsafe { (vtbl.gs_set_shader)(ctx, shader, ptr::null(), 0) };
}

unsafe extern "system-unwind" fn ia_set_primitive_topology(this: *mut c_void, topology: u32) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `IASetPrimitiveTopology` to the immediate context.
    unsafe { (vtbl.ia_set_primitive_topology)(ctx, topology) };
}

unsafe extern "system-unwind" fn vs_set_shader_resources(
    this: *mut c_void,
    start_slot: u32,
    num_views: u32,
    pp_shader_resource_views: *const *mut c_void,
) {
    if (num_views as usize) > MAX_SRV_SLOTS {
        return;
    }
    // SAFETY: `pp_shader_resource_views` has up to `num_views` elements when non-null.
    let views = unsafe { unwrap_array::<MAX_SRV_SLOTS>(num_views, pp_shader_resource_views) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped SRVs to `VSSetShaderResources`.
    unsafe { (vtbl.vs_set_shader_resources)(ctx, start_slot, num_views, views.as_ptr()) };
}

unsafe extern "system-unwind" fn vs_set_samplers(
    this: *mut c_void,
    start_slot: u32,
    num_samplers: u32,
    pp_samplers: *const *mut c_void,
) {
    if (num_samplers as usize) > MAX_SAMPLER_SLOTS {
        return;
    }
    // SAFETY: `pp_samplers` has up to `num_samplers` elements when non-null.
    let samplers = unsafe { unwrap_array::<MAX_SAMPLER_SLOTS>(num_samplers, pp_samplers) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped samplers to `VSSetSamplers`.
    unsafe { (vtbl.vs_set_samplers)(ctx, start_slot, num_samplers, samplers.as_ptr()) };
}

unsafe extern "system-unwind" fn set_predication(
    this: *mut c_void,
    p_predicate: *mut c_void,
    predicate_value: i32,
) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let ctx_vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    if p_predicate.is_null() {
        // SAFETY: clears predication on the immediate context.
        unsafe { (ctx_vtbl.set_predication)(ctx, ptr::null_mut(), predicate_value) };
    } else {
        let mut d3d11_pred = ptr::null_mut();
        // SAFETY: `p_predicate` is a live `ID3D10Predicate` COM object.
        let pred_vtbl = unsafe { com::vtbl::<IUnknownVtbl>(p_predicate) };
        // SAFETY: queries `ID3D11Query` (`ID3D11Predicate`) from `p_predicate`.
        unsafe { (pred_vtbl.query_interface)(p_predicate, &IID_ID3D11_QUERY, &raw mut d3d11_pred) };
        // SAFETY: forwards `d3d11_pred` to `ID3D11DeviceContext::SetPredication`.
        unsafe { (ctx_vtbl.set_predication)(ctx, d3d11_pred, predicate_value) };
        if !d3d11_pred.is_null() {
            // SAFETY: `d3d11_pred` is a live COM object returned by `QueryInterface`.
            let unk = unsafe { com::vtbl::<IUnknownVtbl>(d3d11_pred) };
            // SAFETY: releases the temporary reference on `d3d11_pred`.
            unsafe { (unk.release)(d3d11_pred) };
        }
    }
}

unsafe extern "system-unwind" fn gs_set_shader_resources(
    this: *mut c_void,
    start_slot: u32,
    num_views: u32,
    pp_shader_resource_views: *const *mut c_void,
) {
    if (num_views as usize) > MAX_SRV_SLOTS {
        return;
    }
    // SAFETY: `pp_shader_resource_views` has up to `num_views` elements when non-null.
    let views = unsafe { unwrap_array::<MAX_SRV_SLOTS>(num_views, pp_shader_resource_views) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped SRVs to `GSSetShaderResources`.
    unsafe { (vtbl.gs_set_shader_resources)(ctx, start_slot, num_views, views.as_ptr()) };
}

unsafe extern "system-unwind" fn gs_set_samplers(
    this: *mut c_void,
    start_slot: u32,
    num_samplers: u32,
    pp_samplers: *const *mut c_void,
) {
    if (num_samplers as usize) > MAX_SAMPLER_SLOTS {
        return;
    }
    // SAFETY: `pp_samplers` has up to `num_samplers` elements when non-null.
    let samplers = unsafe { unwrap_array::<MAX_SAMPLER_SLOTS>(num_samplers, pp_samplers) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped samplers to `GSSetSamplers`.
    unsafe { (vtbl.gs_set_samplers)(ctx, start_slot, num_samplers, samplers.as_ptr()) };
}

unsafe extern "system-unwind" fn om_set_render_targets(
    this: *mut c_void,
    num_views: u32,
    pp_render_target_views: *const *mut c_void,
    p_depth_stencil_view: *mut c_void,
) {
    if (num_views as usize) > MAX_RTV_SLOTS {
        return;
    }
    // SAFETY: `pp_render_target_views` has up to `num_views` elements when non-null.
    let rtvs = unsafe { unwrap_array::<MAX_RTV_SLOTS>(num_views, pp_render_target_views) };
    // SAFETY: `p_depth_stencil_view` is a nullable `Wrapper<ID3D10DepthStencilViewVtbl>`.
    let dsv = unsafe { d3d11_of(p_depth_stencil_view) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped RTVs and DSV to `OMSetRenderTargets`.
    unsafe { (vtbl.om_set_render_targets)(ctx, num_views, rtvs.as_ptr(), dsv) };
}

unsafe extern "system-unwind" fn om_set_blend_state(
    this: *mut c_void,
    p_blend_state: *mut c_void,
    blend_factor: *const f32,
    sample_mask: u32,
) {
    // SAFETY: `p_blend_state` is a nullable `Wrapper<ID3D10BlendState1Vtbl>`.
    let bs = unsafe { d3d11_of(p_blend_state) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped blend state to `OMSetBlendState`.
    unsafe { (vtbl.om_set_blend_state)(ctx, bs, blend_factor, sample_mask) };
}

unsafe extern "system-unwind" fn om_set_depth_stencil_state(
    this: *mut c_void,
    p_depth_stencil_state: *mut c_void,
    stencil_ref: u32,
) {
    // SAFETY: `p_depth_stencil_state` is a nullable `Wrapper<ID3D10DepthStencilStateVtbl>`.
    let dss = unsafe { d3d11_of(p_depth_stencil_state) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped depth-stencil state to `OMSetDepthStencilState`.
    unsafe { (vtbl.om_set_depth_stencil_state)(ctx, dss, stencil_ref) };
}

unsafe extern "system-unwind" fn so_set_targets(
    this: *mut c_void,
    num_buffers: u32,
    pp_so_targets: *const *mut c_void,
    p_offsets: *const u32,
) {
    if (num_buffers as usize) > MAX_SO_SLOTS {
        return;
    }
    // SAFETY: `pp_so_targets` has up to `num_buffers` elements when non-null.
    let bufs = unsafe { unwrap_array::<MAX_SO_SLOTS>(num_buffers, pp_so_targets) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped stream-output buffers to `SOSetTargets`.
    unsafe { (vtbl.so_set_targets)(ctx, num_buffers, bufs.as_ptr(), p_offsets) };
}

unsafe extern "system-unwind" fn draw_auto(this: *mut c_void) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `DrawAuto` to the immediate context.
    unsafe { (vtbl.draw_auto)(ctx) };
}

unsafe extern "system-unwind" fn rs_set_state(this: *mut c_void, p_rasterizer_state: *mut c_void) {
    // SAFETY: `p_rasterizer_state` is a nullable `Wrapper<ID3D10RasterizerStateVtbl>`.
    let rs = unsafe { d3d11_of(p_rasterizer_state) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped rasterizer state to `RSSetState`.
    unsafe { (vtbl.rs_set_state)(ctx, rs) };
}

unsafe extern "system-unwind" fn rs_set_viewports(
    this: *mut c_void,
    num_viewports: u32,
    p_viewports: *const c_void,
) {
    if (num_viewports as usize) > MAX_VIEWPORT_SLOTS || (num_viewports > 0 && p_viewports.is_null())
    {
        return;
    }
    let mut vp = [D3d11Viewport::default(); MAX_VIEWPORT_SLOTS];
    let n = num_viewports as usize;
    if n > 0 {
        // SAFETY: `p_viewports` is non-null and has at least `n` `D3d10Viewport` elements.
        let src = unsafe { core::slice::from_raw_parts(p_viewports.cast::<D3d10Viewport>(), n) };
        let mut i = 0;
        while i < n {
            let v10 = src[i];
            vp[i] = D3d11Viewport {
                top_left_x: i32_to_f32(v10.top_left_x),
                top_left_y: i32_to_f32(v10.top_left_y),
                width: u32_to_f32(v10.width),
                height: u32_to_f32(v10.height),
                min_depth: v10.min_depth,
                max_depth: v10.max_depth,
            };
            i += 1;
        }
    }
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards converted `D3D11_VIEWPORT` array to `RSSetViewports`.
    unsafe { (vtbl.rs_set_viewports)(ctx, num_viewports, vp.as_ptr().cast()) };
}

unsafe extern "system-unwind" fn rs_set_scissor_rects(
    this: *mut c_void,
    num_rects: u32,
    p_rects: *const c_void,
) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: `D3D10_RECT` and `D3D11_RECT` are identical (`RECT`); forwards directly.
    unsafe { (vtbl.rs_set_scissor_rects)(ctx, num_rects, p_rects) };
}

unsafe extern "system-unwind" fn copy_subresource_region(
    this: *mut c_void,
    p_dst_resource: *mut c_void,
    dst_subresource: u32,
    dst_x: u32,
    dst_y: u32,
    dst_z: u32,
    p_src_resource: *mut c_void,
    src_subresource: u32,
    p_src_box: *const c_void,
) {
    // SAFETY: `p_dst_resource` is a nullable resource `Wrapper`.
    let dst = unsafe { d3d11_of(p_dst_resource) };
    // SAFETY: `p_src_resource` is a nullable resource `Wrapper`.
    let src = unsafe { d3d11_of(p_src_resource) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped resources and `D3D10_BOX` (layout-identical to `D3D11_BOX`).
    unsafe {
        (vtbl.copy_subresource_region)(
            ctx,
            dst,
            dst_subresource,
            dst_x,
            dst_y,
            dst_z,
            src,
            src_subresource,
            p_src_box,
        );
    };
}

unsafe extern "system-unwind" fn copy_resource(
    this: *mut c_void,
    p_dst_resource: *mut c_void,
    p_src_resource: *mut c_void,
) {
    // SAFETY: `p_dst_resource` is a nullable resource `Wrapper`.
    let dst = unsafe { d3d11_of(p_dst_resource) };
    // SAFETY: `p_src_resource` is a nullable resource `Wrapper`.
    let src = unsafe { d3d11_of(p_src_resource) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped resources to `CopyResource`.
    unsafe { (vtbl.copy_resource)(ctx, dst, src) };
}

unsafe extern "system-unwind" fn update_subresource(
    this: *mut c_void,
    p_dst_resource: *mut c_void,
    dst_subresource: u32,
    p_dst_box: *const c_void,
    p_src_data: *const c_void,
    src_row_pitch: u32,
    src_depth_pitch: u32,
) {
    // SAFETY: `p_dst_resource` is a nullable resource `Wrapper`.
    let dst = unsafe { d3d11_of(p_dst_resource) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped destination resource to `UpdateSubresource`.
    unsafe {
        (vtbl.update_subresource)(
            ctx,
            dst,
            dst_subresource,
            p_dst_box,
            p_src_data,
            src_row_pitch,
            src_depth_pitch,
        );
    };
}

unsafe extern "system-unwind" fn clear_render_target_view(
    this: *mut c_void,
    p_render_target_view: *mut c_void,
    color_rgba: *const f32,
) {
    // SAFETY: `p_render_target_view` is a nullable `Wrapper<ID3D10RenderTargetViewVtbl>`.
    let rtv = unsafe { d3d11_of(p_render_target_view) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped RTV to `ClearRenderTargetView`.
    unsafe { (vtbl.clear_render_target_view)(ctx, rtv, color_rgba) };
}

unsafe extern "system-unwind" fn clear_depth_stencil_view(
    this: *mut c_void,
    p_depth_stencil_view: *mut c_void,
    clear_flags: u32,
    depth: f32,
    stencil: u8,
) {
    // SAFETY: `p_depth_stencil_view` is a nullable `Wrapper<ID3D10DepthStencilViewVtbl>`.
    let dsv = unsafe { d3d11_of(p_depth_stencil_view) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped DSV to `ClearDepthStencilView`.
    unsafe { (vtbl.clear_depth_stencil_view)(ctx, dsv, clear_flags, depth, stencil) };
}

unsafe extern "system-unwind" fn generate_mips(
    this: *mut c_void,
    p_shader_resource_view: *mut c_void,
) {
    // SAFETY: `p_shader_resource_view` is a nullable `Wrapper<ID3D10ShaderResourceView1Vtbl>`.
    let srv = unsafe { d3d11_of(p_shader_resource_view) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped SRV to `GenerateMips`.
    unsafe { (vtbl.generate_mips)(ctx, srv) };
}

unsafe extern "system-unwind" fn resolve_subresource(
    this: *mut c_void,
    p_dst_resource: *mut c_void,
    dst_subresource: u32,
    p_src_resource: *mut c_void,
    src_subresource: u32,
    format: u32,
) {
    // SAFETY: `p_dst_resource` is a nullable resource `Wrapper`.
    let dst = unsafe { d3d11_of(p_dst_resource) };
    // SAFETY: `p_src_resource` is a nullable resource `Wrapper`.
    let src = unsafe { d3d11_of(p_src_resource) };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards unwrapped resources to `ResolveSubresource`.
    unsafe { (vtbl.resolve_subresource)(ctx, dst, dst_subresource, src, src_subresource, format) };
}

unsafe extern "system-unwind" fn vs_get_constant_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_constant_buffers: *mut *mut c_void,
) {
    let mut d3d11_bufs = [ptr::null_mut(); MAX_CB_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_CB_SLOTS` constant buffers from `VSGetConstantBuffers`.
    unsafe {
        (vtbl.vs_get_constant_buffers)(ctx, start_slot, num_buffers, d3d11_bufs.as_mut_ptr());
    };
    // SAFETY: converts returned `ID3D11Buffer` pointers to `ID3D10Buffer` pointers.
    unsafe {
        wrap_out_array(
            num_buffers,
            &d3d11_bufs,
            &IID_ID3D10_BUFFER,
            pp_constant_buffers,
        );
    };
}

unsafe extern "system-unwind" fn ps_get_shader_resources(
    this: *mut c_void,
    start_slot: u32,
    num_views: u32,
    pp_shader_resource_views: *mut *mut c_void,
) {
    let mut d3d11_views = [ptr::null_mut(); MAX_SRV_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_SRV_SLOTS` SRVs from `PSGetShaderResources`.
    unsafe { (vtbl.ps_get_shader_resources)(ctx, start_slot, num_views, d3d11_views.as_mut_ptr()) };
    // SAFETY: converts returned `ID3D11ShaderResourceView` pointers to `ID3D10ShaderResourceView`.
    unsafe {
        wrap_out_array(
            num_views,
            &d3d11_views,
            &IID_ID3D10_SHADER_RESOURCE_VIEW,
            pp_shader_resource_views,
        );
    };
}

unsafe extern "system-unwind" fn ps_get_shader(
    this: *mut c_void,
    pp_pixel_shader: *mut *mut c_void,
) {
    let mut d3d11_ps = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries the bound pixel shader from `PSGetShader`.
    unsafe { (vtbl.ps_get_shader)(ctx, &raw mut d3d11_ps, ptr::null_mut(), ptr::null_mut()) };
    // SAFETY: converts `d3d11_ps` to `ID3D10PixelShader` and writes to `pp_pixel_shader`.
    unsafe { wrap_out_ptr(d3d11_ps, &IID_ID3D10_PIXEL_SHADER, pp_pixel_shader) };
}

unsafe extern "system-unwind" fn ps_get_samplers(
    this: *mut c_void,
    start_slot: u32,
    num_samplers: u32,
    pp_samplers: *mut *mut c_void,
) {
    let mut d3d11_samplers = [ptr::null_mut(); MAX_SAMPLER_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_SAMPLER_SLOTS` samplers from `PSGetSamplers`.
    unsafe { (vtbl.ps_get_samplers)(ctx, start_slot, num_samplers, d3d11_samplers.as_mut_ptr()) };
    // SAFETY: converts returned `ID3D11SamplerState` pointers to `ID3D10SamplerState`.
    unsafe {
        wrap_out_array(
            num_samplers,
            &d3d11_samplers,
            &IID_ID3D10_SAMPLER_STATE,
            pp_samplers,
        );
    };
}

unsafe extern "system-unwind" fn vs_get_shader(
    this: *mut c_void,
    pp_vertex_shader: *mut *mut c_void,
) {
    let mut d3d11_vs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries the bound vertex shader from `VSGetShader`.
    unsafe { (vtbl.vs_get_shader)(ctx, &raw mut d3d11_vs, ptr::null_mut(), ptr::null_mut()) };
    // SAFETY: converts `d3d11_vs` to `ID3D10VertexShader` and writes to `pp_vertex_shader`.
    unsafe { wrap_out_ptr(d3d11_vs, &IID_ID3D10_VERTEX_SHADER, pp_vertex_shader) };
}

unsafe extern "system-unwind" fn ps_get_constant_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_constant_buffers: *mut *mut c_void,
) {
    let mut d3d11_bufs = [ptr::null_mut(); MAX_CB_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_CB_SLOTS` constant buffers from `PSGetConstantBuffers`.
    unsafe {
        (vtbl.ps_get_constant_buffers)(ctx, start_slot, num_buffers, d3d11_bufs.as_mut_ptr());
    };
    // SAFETY: converts returned `ID3D11Buffer` pointers to `ID3D10Buffer`.
    unsafe {
        wrap_out_array(
            num_buffers,
            &d3d11_bufs,
            &IID_ID3D10_BUFFER,
            pp_constant_buffers,
        );
    };
}

unsafe extern "system-unwind" fn ia_get_input_layout(
    this: *mut c_void,
    pp_input_layout: *mut *mut c_void,
) {
    let mut d3d11_il = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries the bound input layout from `IAGetInputLayout`.
    unsafe { (vtbl.ia_get_input_layout)(ctx, &raw mut d3d11_il) };
    // SAFETY: converts `d3d11_il` to `ID3D10InputLayout` and writes to `pp_input_layout`.
    unsafe { wrap_out_ptr(d3d11_il, &IID_ID3D10_INPUT_LAYOUT, pp_input_layout) };
}

unsafe extern "system-unwind" fn ia_get_vertex_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_vertex_buffers: *mut *mut c_void,
    p_strides: *mut u32,
    p_offsets: *mut u32,
) {
    let mut d3d11_bufs = [ptr::null_mut(); MAX_VB_SLOTS];
    let buf_out = if pp_vertex_buffers.is_null() {
        ptr::null_mut()
    } else {
        d3d11_bufs.as_mut_ptr()
    };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries vertex buffers, strides, and offsets from `IAGetVertexBuffers`.
    unsafe {
        (vtbl.ia_get_vertex_buffers)(ctx, start_slot, num_buffers, buf_out, p_strides, p_offsets);
    };
    if !pp_vertex_buffers.is_null() {
        // SAFETY: converts returned `ID3D11Buffer` pointers to `ID3D10Buffer`.
        unsafe {
            wrap_out_array(
                num_buffers,
                &d3d11_bufs,
                &IID_ID3D10_BUFFER,
                pp_vertex_buffers,
            );
        };
    }
}

unsafe extern "system-unwind" fn ia_get_index_buffer(
    this: *mut c_void,
    p_index_buffer: *mut *mut c_void,
    format: *mut u32,
    offset: *mut u32,
) {
    let mut d3d11_ib = ptr::null_mut();
    let ib_out = out_slot(p_index_buffer, &mut d3d11_ib);
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries the bound index buffer, format, and offset from `IAGetIndexBuffer`.
    unsafe { (vtbl.ia_get_index_buffer)(ctx, ib_out, format, offset) };
    if !p_index_buffer.is_null() {
        // SAFETY: converts `d3d11_ib` to `ID3D10Buffer` and writes to `p_index_buffer`.
        unsafe { wrap_out_ptr(d3d11_ib, &IID_ID3D10_BUFFER, p_index_buffer) };
    }
}

unsafe extern "system-unwind" fn gs_get_constant_buffers(
    this: *mut c_void,
    start_slot: u32,
    num_buffers: u32,
    pp_constant_buffers: *mut *mut c_void,
) {
    let mut d3d11_bufs = [ptr::null_mut(); MAX_CB_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_CB_SLOTS` constant buffers from `GSGetConstantBuffers`.
    unsafe {
        (vtbl.gs_get_constant_buffers)(ctx, start_slot, num_buffers, d3d11_bufs.as_mut_ptr());
    };
    // SAFETY: converts returned `ID3D11Buffer` pointers to `ID3D10Buffer`.
    unsafe {
        wrap_out_array(
            num_buffers,
            &d3d11_bufs,
            &IID_ID3D10_BUFFER,
            pp_constant_buffers,
        );
    };
}

unsafe extern "system-unwind" fn gs_get_shader(
    this: *mut c_void,
    pp_geometry_shader: *mut *mut c_void,
) {
    let mut d3d11_gs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries the bound geometry shader from `GSGetShader`.
    unsafe { (vtbl.gs_get_shader)(ctx, &raw mut d3d11_gs, ptr::null_mut(), ptr::null_mut()) };
    // SAFETY: converts `d3d11_gs` to `ID3D10GeometryShader` and writes to `pp_geometry_shader`.
    unsafe { wrap_out_ptr(d3d11_gs, &IID_ID3D10_GEOMETRY_SHADER, pp_geometry_shader) };
}

unsafe extern "system-unwind" fn ia_get_primitive_topology(
    this: *mut c_void,
    p_topology: *mut u32,
) {
    let mut d3d11_topo = 0_u32;
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries primitive topology from `IAGetPrimitiveTopology`.
    unsafe { (vtbl.ia_get_primitive_topology)(ctx, &raw mut d3d11_topo) };
    if !p_topology.is_null() {
        let topo = if d3d11_topo < 33 { d3d11_topo } else { 0 };
        // SAFETY: `p_topology` is a non-null caller out-pointer.
        unsafe { *p_topology = topo };
    }
}

unsafe extern "system-unwind" fn vs_get_shader_resources(
    this: *mut c_void,
    start_slot: u32,
    num_views: u32,
    pp_shader_resource_views: *mut *mut c_void,
) {
    let mut d3d11_views = [ptr::null_mut(); MAX_SRV_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_SRV_SLOTS` SRVs from `VSGetShaderResources`.
    unsafe { (vtbl.vs_get_shader_resources)(ctx, start_slot, num_views, d3d11_views.as_mut_ptr()) };
    // SAFETY: converts returned `ID3D11ShaderResourceView` pointers to `ID3D10ShaderResourceView`.
    unsafe {
        wrap_out_array(
            num_views,
            &d3d11_views,
            &IID_ID3D10_SHADER_RESOURCE_VIEW,
            pp_shader_resource_views,
        );
    };
}

unsafe extern "system-unwind" fn vs_get_samplers(
    this: *mut c_void,
    start_slot: u32,
    num_samplers: u32,
    pp_samplers: *mut *mut c_void,
) {
    let mut d3d11_samplers = [ptr::null_mut(); MAX_SAMPLER_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_SAMPLER_SLOTS` samplers from `VSGetSamplers`.
    unsafe { (vtbl.vs_get_samplers)(ctx, start_slot, num_samplers, d3d11_samplers.as_mut_ptr()) };
    // SAFETY: converts returned `ID3D11SamplerState` pointers to `ID3D10SamplerState`.
    unsafe {
        wrap_out_array(
            num_samplers,
            &d3d11_samplers,
            &IID_ID3D10_SAMPLER_STATE,
            pp_samplers,
        );
    };
}

unsafe extern "system-unwind" fn get_predication(
    this: *mut c_void,
    pp_predicate: *mut *mut c_void,
    p_predicate_value: *mut i32,
) {
    let mut d3d11_pred = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries predication from `GetPredication`.
    unsafe { (vtbl.get_predication)(ctx, &raw mut d3d11_pred, p_predicate_value) };
    // SAFETY: converts `d3d11_pred` to `ID3D10Query` (`ID3D10Predicate`) and writes to `pp_predicate`.
    unsafe { wrap_out_ptr(d3d11_pred, &IID_ID3D10_QUERY, pp_predicate) };
}

unsafe extern "system-unwind" fn gs_get_shader_resources(
    this: *mut c_void,
    start_slot: u32,
    num_views: u32,
    pp_shader_resource_views: *mut *mut c_void,
) {
    let mut d3d11_views = [ptr::null_mut(); MAX_SRV_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_SRV_SLOTS` SRVs from `GSGetShaderResources`.
    unsafe { (vtbl.gs_get_shader_resources)(ctx, start_slot, num_views, d3d11_views.as_mut_ptr()) };
    // SAFETY: converts returned `ID3D11ShaderResourceView` pointers to `ID3D10ShaderResourceView`.
    unsafe {
        wrap_out_array(
            num_views,
            &d3d11_views,
            &IID_ID3D10_SHADER_RESOURCE_VIEW,
            pp_shader_resource_views,
        );
    };
}

unsafe extern "system-unwind" fn gs_get_samplers(
    this: *mut c_void,
    start_slot: u32,
    num_samplers: u32,
    pp_samplers: *mut *mut c_void,
) {
    let mut d3d11_samplers = [ptr::null_mut(); MAX_SAMPLER_SLOTS];
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries up to `MAX_SAMPLER_SLOTS` samplers from `GSGetSamplers`.
    unsafe { (vtbl.gs_get_samplers)(ctx, start_slot, num_samplers, d3d11_samplers.as_mut_ptr()) };
    // SAFETY: converts returned `ID3D11SamplerState` pointers to `ID3D10SamplerState`.
    unsafe {
        wrap_out_array(
            num_samplers,
            &d3d11_samplers,
            &IID_ID3D10_SAMPLER_STATE,
            pp_samplers,
        );
    };
}

unsafe extern "system-unwind" fn om_get_render_targets(
    this: *mut c_void,
    num_views: u32,
    pp_render_target_views: *mut *mut c_void,
    pp_depth_stencil_view: *mut *mut c_void,
) {
    let mut d3d11_rtvs = [ptr::null_mut(); MAX_RTV_SLOTS];
    let mut d3d11_dsv = ptr::null_mut();
    let rtv_out = if pp_render_target_views.is_null() {
        ptr::null_mut()
    } else {
        d3d11_rtvs.as_mut_ptr()
    };
    let dsv_out = out_slot(pp_depth_stencil_view, &mut d3d11_dsv);
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries RTVs and DSV from `OMGetRenderTargets`.
    unsafe { (vtbl.om_get_render_targets)(ctx, num_views, rtv_out, dsv_out) };
    if !pp_render_target_views.is_null() {
        // SAFETY: converts returned `ID3D11RenderTargetView` pointers to `ID3D10RenderTargetView`.
        unsafe {
            wrap_out_array(
                num_views,
                &d3d11_rtvs,
                &IID_ID3D10_RENDER_TARGET_VIEW,
                pp_render_target_views,
            );
        };
    }
    if !pp_depth_stencil_view.is_null() {
        // SAFETY: converts `d3d11_dsv` to `ID3D10DepthStencilView` and writes to `pp_depth_stencil_view`.
        unsafe {
            wrap_out_ptr(
                d3d11_dsv,
                &IID_ID3D10_DEPTH_STENCIL_VIEW,
                pp_depth_stencil_view,
            );
        };
    }
}

unsafe extern "system-unwind" fn om_get_blend_state(
    this: *mut c_void,
    pp_blend_state: *mut *mut c_void,
    blend_factor: *mut f32,
    p_sample_mask: *mut u32,
) {
    let mut d3d11_bs = ptr::null_mut();
    let bs_out = out_slot(pp_blend_state, &mut d3d11_bs);
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries blend state, blend factor, and sample mask from `OMGetBlendState`.
    unsafe { (vtbl.om_get_blend_state)(ctx, bs_out, blend_factor, p_sample_mask) };
    if !pp_blend_state.is_null() {
        // SAFETY: converts `d3d11_bs` to `ID3D10BlendState` and writes to `pp_blend_state`.
        unsafe { wrap_out_ptr(d3d11_bs, &IID_ID3D10_BLEND_STATE, pp_blend_state) };
    }
}

unsafe extern "system-unwind" fn om_get_depth_stencil_state(
    this: *mut c_void,
    pp_depth_stencil_state: *mut *mut c_void,
    p_stencil_ref: *mut u32,
) {
    let mut d3d11_dss = ptr::null_mut();
    let dss_out = out_slot(pp_depth_stencil_state, &mut d3d11_dss);
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries depth-stencil state and stencil ref from `OMGetDepthStencilState`.
    unsafe { (vtbl.om_get_depth_stencil_state)(ctx, dss_out, p_stencil_ref) };
    if !pp_depth_stencil_state.is_null() {
        // SAFETY: converts `d3d11_dss` to `ID3D10DepthStencilState` and writes to `pp_depth_stencil_state`.
        unsafe {
            wrap_out_ptr(
                d3d11_dss,
                &IID_ID3D10_DEPTH_STENCIL_STATE,
                pp_depth_stencil_state,
            );
        };
    }
}

unsafe extern "system-unwind" fn so_get_targets(
    this: *mut c_void,
    num_buffers: u32,
    pp_so_targets: *mut *mut c_void,
    p_offsets: *mut u32,
) {
    let mut d3d11_bufs = [ptr::null_mut(); MAX_SO_SLOTS];
    let buf_out = if pp_so_targets.is_null() {
        ptr::null_mut()
    } else {
        d3d11_bufs.as_mut_ptr()
    };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` implements `IMTLD3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<IMTLD3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries SO target buffers and offsets via `SOGetTargetsWithOffsets`.
    unsafe { (vtbl.so_get_targets_with_offsets)(ctx, num_buffers, buf_out, p_offsets) };
    if !pp_so_targets.is_null() {
        // SAFETY: converts returned buffer pointers to `ID3D10Buffer` pointers.
        unsafe { wrap_out_array(num_buffers, &d3d11_bufs, &IID_ID3D10_BUFFER, pp_so_targets) };
    }
}

unsafe extern "system-unwind" fn rs_get_state(
    this: *mut c_void,
    pp_rasterizer_state: *mut *mut c_void,
) {
    let mut d3d11_rs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries the bound rasterizer state from `RSGetState`.
    unsafe { (vtbl.rs_get_state)(ctx, &raw mut d3d11_rs) };
    // SAFETY: converts `d3d11_rs` to `ID3D10RasterizerState` and writes to `pp_rasterizer_state`.
    unsafe { wrap_out_ptr(d3d11_rs, &IID_ID3D10_RASTERIZER_STATE, pp_rasterizer_state) };
}

unsafe extern "system-unwind" fn rs_get_viewports(
    this: *mut c_void,
    num_viewports: *mut u32,
    p_viewports: *mut c_void,
) {
    let mut vp = [D3d11Viewport::default(); MAX_VIEWPORT_SLOTS];
    let vp_ptr = if p_viewports.is_null() {
        ptr::null_mut()
    } else {
        vp.as_mut_ptr().cast()
    };
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: queries viewports from `RSGetViewports`.
    unsafe { (vtbl.rs_get_viewports)(ctx, num_viewports, vp_ptr) };
    if !p_viewports.is_null() && !num_viewports.is_null() {
        // SAFETY: `num_viewports` is a valid non-null pointer.
        let count = unsafe { *num_viewports };
        let n = (count as usize).min(MAX_VIEWPORT_SLOTS);
        if n > 0 {
            // SAFETY: `p_viewports` is non-null and has room for at least `n` `D3d10Viewport` elements.
            let dst =
                unsafe { core::slice::from_raw_parts_mut(p_viewports.cast::<D3d10Viewport>(), n) };
            let mut i = 0;
            while i < n {
                let v11 = vp[i];
                dst[i] = D3d10Viewport {
                    top_left_x: f32_to_i32(v11.top_left_x),
                    top_left_y: f32_to_i32(v11.top_left_y),
                    width: f32_to_u32(v11.width),
                    height: f32_to_u32(v11.height),
                    min_depth: v11.min_depth,
                    max_depth: v11.max_depth,
                };
                i += 1;
            }
        }
    }
}

unsafe extern "system-unwind" fn rs_get_scissor_rects(
    this: *mut c_void,
    num_rects: *mut u32,
    p_rects: *mut c_void,
) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: `D3D10_RECT` and `D3D11_RECT` are identical (`RECT`); forwards directly.
    unsafe { (vtbl.rs_get_scissor_rects)(ctx, num_rects, p_rects) };
}

unsafe extern "system-unwind" fn get_device_removed_reason(this: *mut c_void) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `GetDeviceRemovedReason` to `ID3D11Device`.
    unsafe { (vtbl.get_device_removed_reason)(d3d11) }
}

unsafe extern "system-unwind" fn set_exception_mode(this: *mut c_void, raise_flags: u32) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `SetExceptionMode` to `ID3D11Device`.
    unsafe { (vtbl.set_exception_mode)(d3d11, raise_flags) }
}

unsafe extern "system-unwind" fn get_exception_mode(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `GetExceptionMode` to `ID3D11Device`.
    unsafe { (vtbl.get_exception_mode)(d3d11) }
}

unsafe extern "system-unwind" fn get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    p_data_size: *mut u32,
    p_data: *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `GetPrivateData` to `ID3D11Device`.
    unsafe { (vtbl.get_private_data)(d3d11, guid, p_data_size, p_data) }
}

unsafe extern "system-unwind" fn set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    data_size: u32,
    p_data: *const c_void,
) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `SetPrivateData` to `ID3D11Device`.
    unsafe { (vtbl.set_private_data)(d3d11, guid, data_size, p_data) }
}

unsafe extern "system-unwind" fn set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    p_data: *const c_void,
) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `SetPrivateDataInterface` to `ID3D11Device`.
    unsafe { (vtbl.set_private_data_interface)(d3d11, guid, p_data) }
}

unsafe extern "system-unwind" fn clear_state(this: *mut c_void) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `ClearState` to the immediate context.
    unsafe { (vtbl.clear_state)(ctx) };
}

unsafe extern "system-unwind" fn flush(this: *mut c_void) {
    // SAFETY: `this` is a live `Device`.
    let ctx = unsafe { wrapper_context(this) };
    // SAFETY: `ctx` is a live `ID3D11DeviceContext`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceContextVtbl>(ctx) };
    // SAFETY: forwards `Flush` to the immediate context.
    unsafe { (vtbl.flush)(ctx) };
}

unsafe extern "system-unwind" fn create_buffer(
    this: *mut c_void,
    p_desc: *const c_void,
    p_initial_data: *const c_void,
    pp_buffer: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_buffer` to null if non-null.
    unsafe { init_return_ptr(pp_buffer) };
    if p_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_desc` is a non-null `D3D10_BUFFER_DESC` pointer.
    let d10 = unsafe { &*p_desc.cast::<D3d10BufferDesc>() };
    let d11 = d10_to_d11_buffer_desc(d10);
    let mut d3d11_buf = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11Buffer`.
    let hr = unsafe {
        (vtbl.create_buffer)(
            d3d11,
            (&raw const d11).cast(),
            p_initial_data,
            out_slot(pp_buffer, &mut d3d11_buf),
        )
    };
    // SAFETY: queries `ID3D10Buffer` into `pp_buffer` and releases `d3d11_buf`.
    unsafe { finish_create(hr, d3d11_buf, &IID_ID3D10_BUFFER, pp_buffer) }
}

unsafe extern "system-unwind" fn create_texture1d(
    this: *mut c_void,
    p_desc: *const c_void,
    p_initial_data: *const c_void,
    pp_texture1d: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_texture1d` to null if non-null.
    unsafe { init_return_ptr(pp_texture1d) };
    if p_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_desc` is a non-null `D3D10_TEXTURE1D_DESC` pointer.
    let d10 = unsafe { &*p_desc.cast::<D3d10Texture1dDesc>() };
    let d11 = d10_to_d11_tex1d_desc(d10);
    let mut d3d11_tex = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11Texture1D`.
    let hr = unsafe {
        (vtbl.create_texture1d)(
            d3d11,
            (&raw const d11).cast(),
            p_initial_data,
            out_slot(pp_texture1d, &mut d3d11_tex),
        )
    };
    // SAFETY: queries `ID3D10Texture1D` into `pp_texture1d` and releases `d3d11_tex`.
    unsafe { finish_create(hr, d3d11_tex, &IID_ID3D10_TEXTURE_1D, pp_texture1d) }
}

unsafe extern "system-unwind" fn create_texture2d(
    this: *mut c_void,
    p_desc: *const c_void,
    p_initial_data: *const c_void,
    pp_texture2d: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_texture2d` to null if non-null.
    unsafe { init_return_ptr(pp_texture2d) };
    if p_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_desc` is a non-null `D3D10_TEXTURE2D_DESC` pointer.
    let d10 = unsafe { &*p_desc.cast::<D3d10Texture2dDesc>() };
    let d11 = d10_to_d11_tex2d_desc(d10);
    let mut d3d11_tex = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11Texture2D`.
    let hr = unsafe {
        (vtbl.create_texture2d)(
            d3d11,
            (&raw const d11).cast(),
            p_initial_data,
            out_slot(pp_texture2d, &mut d3d11_tex),
        )
    };
    // SAFETY: queries `ID3D10Texture2D` into `pp_texture2d` and releases `d3d11_tex`.
    unsafe { finish_create(hr, d3d11_tex, &IID_ID3D10_TEXTURE_2D, pp_texture2d) }
}

unsafe extern "system-unwind" fn create_texture3d(
    this: *mut c_void,
    p_desc: *const c_void,
    p_initial_data: *const c_void,
    pp_texture3d: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_texture3d` to null if non-null.
    unsafe { init_return_ptr(pp_texture3d) };
    if p_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_desc` is a non-null `D3D10_TEXTURE3D_DESC` pointer.
    let d10 = unsafe { &*p_desc.cast::<D3d10Texture3dDesc>() };
    let d11 = d10_to_d11_tex3d_desc(d10);
    let mut d3d11_tex = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11Texture3D`.
    let hr = unsafe {
        (vtbl.create_texture3d)(
            d3d11,
            (&raw const d11).cast(),
            p_initial_data,
            out_slot(pp_texture3d, &mut d3d11_tex),
        )
    };
    // SAFETY: queries `ID3D10Texture3D` into `pp_texture3d` and releases `d3d11_tex`.
    unsafe { finish_create(hr, d3d11_tex, &IID_ID3D10_TEXTURE_3D, pp_texture3d) }
}

unsafe extern "system-unwind" fn create_shader_resource_view(
    this: *mut c_void,
    p_resource: *mut c_void,
    p_desc: *const c_void,
    pp_sr_view: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_sr_view` to null if non-null.
    unsafe { init_return_ptr(pp_sr_view) };
    if p_resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_resource` is a non-null resource `Wrapper`.
    let res = unsafe { d3d11_of(p_resource) };
    let mut d3d11_srv = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_SHADER_RESOURCE_VIEW_DESC` has the same binary layout as `D3D11_SHADER_RESOURCE_VIEW_DESC`.
    let hr = unsafe {
        (vtbl.create_shader_resource_view)(d3d11, res, p_desc, out_slot(pp_sr_view, &mut d3d11_srv))
    };
    // SAFETY: queries `ID3D10ShaderResourceView` into `pp_sr_view` and releases `d3d11_srv`.
    unsafe { finish_create(hr, d3d11_srv, &IID_ID3D10_SHADER_RESOURCE_VIEW, pp_sr_view) }
}

unsafe extern "system-unwind" fn create_render_target_view(
    this: *mut c_void,
    p_resource: *mut c_void,
    p_desc: *const c_void,
    pp_rt_view: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_rt_view` to null if non-null.
    unsafe { init_return_ptr(pp_rt_view) };
    if p_resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_resource` is a non-null resource `Wrapper`.
    let res = unsafe { d3d11_of(p_resource) };
    let mut d3d11_rtv = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_RENDER_TARGET_VIEW_DESC` has the same binary layout as `D3D11_RENDER_TARGET_VIEW_DESC`.
    let hr = unsafe {
        (vtbl.create_render_target_view)(d3d11, res, p_desc, out_slot(pp_rt_view, &mut d3d11_rtv))
    };
    // SAFETY: queries `ID3D10RenderTargetView` into `pp_rt_view` and releases `d3d11_rtv`.
    unsafe { finish_create(hr, d3d11_rtv, &IID_ID3D10_RENDER_TARGET_VIEW, pp_rt_view) }
}

unsafe extern "system-unwind" fn create_depth_stencil_view(
    this: *mut c_void,
    p_resource: *mut c_void,
    p_desc: *const c_void,
    pp_depth_stencil_view: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_depth_stencil_view` to null if non-null.
    unsafe { init_return_ptr(pp_depth_stencil_view) };
    if p_resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_resource` is a non-null resource `Wrapper`.
    let res = unsafe { d3d11_of(p_resource) };
    let d11_desc;
    let d11_desc_ptr = if p_desc.is_null() {
        ptr::null()
    } else {
        // SAFETY: `p_desc` is a non-null `D3D10_DEPTH_STENCIL_VIEW_DESC` pointer.
        let d10 = unsafe { &*p_desc.cast::<D3d10DepthStencilViewDesc>() };
        d11_desc = d10_to_d11_dsv_desc(d10);
        (&raw const d11_desc).cast()
    };
    let mut d3d11_dsv = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11DepthStencilView`.
    let hr = unsafe {
        (vtbl.create_depth_stencil_view)(
            d3d11,
            res,
            d11_desc_ptr,
            out_slot(pp_depth_stencil_view, &mut d3d11_dsv),
        )
    };
    // SAFETY: queries `ID3D10DepthStencilView` into `pp_depth_stencil_view` and releases `d3d11_dsv`.
    unsafe {
        finish_create(
            hr,
            d3d11_dsv,
            &IID_ID3D10_DEPTH_STENCIL_VIEW,
            pp_depth_stencil_view,
        )
    }
}

unsafe extern "system-unwind" fn create_input_layout(
    this: *mut c_void,
    p_input_element_descs: *const c_void,
    num_elements: u32,
    p_shader_bytecode_with_input_signature: *const c_void,
    bytecode_length: usize,
    pp_input_layout: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_input_layout` to null if non-null.
    unsafe { init_return_ptr(pp_input_layout) };
    let mut d3d11_il = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_INPUT_ELEMENT_DESC` has the same binary layout as `D3D11_INPUT_ELEMENT_DESC`.
    let hr = unsafe {
        (vtbl.create_input_layout)(
            d3d11,
            p_input_element_descs,
            num_elements,
            p_shader_bytecode_with_input_signature,
            bytecode_length,
            out_slot(pp_input_layout, &mut d3d11_il),
        )
    };
    // SAFETY: queries `ID3D10InputLayout` into `pp_input_layout` and releases `d3d11_il`.
    unsafe { finish_create(hr, d3d11_il, &IID_ID3D10_INPUT_LAYOUT, pp_input_layout) }
}

unsafe extern "system-unwind" fn create_vertex_shader(
    this: *mut c_void,
    p_shader_bytecode: *const c_void,
    bytecode_length: usize,
    pp_vertex_shader: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_vertex_shader` to null if non-null.
    unsafe { init_return_ptr(pp_vertex_shader) };
    let mut d3d11_vs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11VertexShader`.
    let hr = unsafe {
        (vtbl.create_vertex_shader)(
            d3d11,
            p_shader_bytecode,
            bytecode_length,
            ptr::null_mut(),
            out_slot(pp_vertex_shader, &mut d3d11_vs),
        )
    };
    // SAFETY: queries `ID3D10VertexShader` into `pp_vertex_shader` and releases `d3d11_vs`.
    unsafe { finish_create(hr, d3d11_vs, &IID_ID3D10_VERTEX_SHADER, pp_vertex_shader) }
}

unsafe extern "system-unwind" fn create_geometry_shader(
    this: *mut c_void,
    p_shader_bytecode: *const c_void,
    bytecode_length: usize,
    pp_geometry_shader: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_geometry_shader` to null if non-null.
    unsafe { init_return_ptr(pp_geometry_shader) };
    let mut d3d11_gs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11GeometryShader`.
    let hr = unsafe {
        (vtbl.create_geometry_shader)(
            d3d11,
            p_shader_bytecode,
            bytecode_length,
            ptr::null_mut(),
            out_slot(pp_geometry_shader, &mut d3d11_gs),
        )
    };
    // SAFETY: queries `ID3D10GeometryShader` into `pp_geometry_shader` and releases `d3d11_gs`.
    unsafe {
        finish_create(
            hr,
            d3d11_gs,
            &IID_ID3D10_GEOMETRY_SHADER,
            pp_geometry_shader,
        )
    }
}

unsafe extern "system-unwind" fn create_geometry_shader_with_stream_output(
    this: *mut c_void,
    p_shader_bytecode: *const c_void,
    bytecode_length: usize,
    p_shader_stream_output: *const c_void,
    num_entries: u32,
    output_stream_stride: u32,
    pp_geometry_shader: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_geometry_shader` to null if non-null.
    unsafe { init_return_ptr(pp_geometry_shader) };
    let mut entries = Vec::with_capacity(num_entries as usize);
    if !p_shader_stream_output.is_null() && num_entries > 0 {
        // SAFETY: `p_shader_stream_output` is non-null and has `num_entries` elements.
        let src = unsafe {
            core::slice::from_raw_parts(
                p_shader_stream_output.cast::<D3d10SoDeclarationEntry>(),
                num_entries as usize,
            )
        };
        let mut i = 0;
        while i < src.len() {
            let e = src[i];
            entries.push(D3d11SoDeclarationEntry {
                stream: 0,
                semantic_name: e.semantic_name,
                semantic_index: e.semantic_index,
                start_component: e.start_component,
                component_count: e.component_count,
                output_slot: e.output_slot,
            });
            i += 1;
        }
    }
    let mut d3d11_gs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying stream-output `ID3D11GeometryShader`.
    let hr = unsafe {
        (vtbl.create_geometry_shader_with_stream_output)(
            d3d11,
            p_shader_bytecode,
            bytecode_length,
            entries.as_ptr().cast(),
            num_entries,
            &raw const output_stream_stride,
            1,
            0xffff_ffff,
            ptr::null_mut(),
            out_slot(pp_geometry_shader, &mut d3d11_gs),
        )
    };
    // SAFETY: queries `ID3D10GeometryShader` into `pp_geometry_shader` and releases `d3d11_gs`.
    unsafe {
        finish_create(
            hr,
            d3d11_gs,
            &IID_ID3D10_GEOMETRY_SHADER,
            pp_geometry_shader,
        )
    }
}

unsafe extern "system-unwind" fn create_pixel_shader(
    this: *mut c_void,
    p_shader_bytecode: *const c_void,
    bytecode_length: usize,
    pp_pixel_shader: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_pixel_shader` to null if non-null.
    unsafe { init_return_ptr(pp_pixel_shader) };
    let mut d3d11_ps = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11PixelShader`.
    let hr = unsafe {
        (vtbl.create_pixel_shader)(
            d3d11,
            p_shader_bytecode,
            bytecode_length,
            ptr::null_mut(),
            out_slot(pp_pixel_shader, &mut d3d11_ps),
        )
    };
    // SAFETY: queries `ID3D10PixelShader` into `pp_pixel_shader` and releases `d3d11_ps`.
    unsafe { finish_create(hr, d3d11_ps, &IID_ID3D10_PIXEL_SHADER, pp_pixel_shader) }
}

unsafe extern "system-unwind" fn create_blend_state(
    this: *mut c_void,
    p_blend_state_desc: *const c_void,
    pp_blend_state: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_blend_state` to null if non-null.
    unsafe { init_return_ptr(pp_blend_state) };
    let d11_desc;
    let d11_desc_ptr = if p_blend_state_desc.is_null() {
        ptr::null()
    } else {
        // SAFETY: `p_blend_state_desc` is a non-null `D3D10_BLEND_DESC` pointer.
        let d10 = unsafe { &*p_blend_state_desc.cast::<D3d10BlendDesc>() };
        d11_desc = d10_to_d11_blend_desc(d10);
        (&raw const d11_desc).cast()
    };
    let mut d3d11_bs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11BlendState`.
    let hr = unsafe {
        (vtbl.create_blend_state)(d3d11, d11_desc_ptr, out_slot(pp_blend_state, &mut d3d11_bs))
    };
    // SAFETY: queries `ID3D10BlendState` into `pp_blend_state` and releases `d3d11_bs`.
    unsafe { finish_create(hr, d3d11_bs, &IID_ID3D10_BLEND_STATE, pp_blend_state) }
}

unsafe extern "system-unwind" fn create_depth_stencil_state(
    this: *mut c_void,
    p_depth_stencil_desc: *const c_void,
    pp_depth_stencil_state: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_depth_stencil_state` to null if non-null.
    unsafe { init_return_ptr(pp_depth_stencil_state) };
    let mut d3d11_dss = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_DEPTH_STENCIL_DESC` has the same binary layout as `D3D11_DEPTH_STENCIL_DESC`.
    let hr = unsafe {
        (vtbl.create_depth_stencil_state)(
            d3d11,
            p_depth_stencil_desc,
            out_slot(pp_depth_stencil_state, &mut d3d11_dss),
        )
    };
    // SAFETY: queries `ID3D10DepthStencilState` into `pp_depth_stencil_state` and releases `d3d11_dss`.
    unsafe {
        finish_create(
            hr,
            d3d11_dss,
            &IID_ID3D10_DEPTH_STENCIL_STATE,
            pp_depth_stencil_state,
        )
    }
}

unsafe extern "system-unwind" fn create_rasterizer_state(
    this: *mut c_void,
    p_rasterizer_desc: *const c_void,
    pp_rasterizer_state: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_rasterizer_state` to null if non-null.
    unsafe { init_return_ptr(pp_rasterizer_state) };
    let mut d3d11_rs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_RASTERIZER_DESC` has the same binary layout as `D3D11_RASTERIZER_DESC`.
    let hr = unsafe {
        (vtbl.create_rasterizer_state)(
            d3d11,
            p_rasterizer_desc,
            out_slot(pp_rasterizer_state, &mut d3d11_rs),
        )
    };
    // SAFETY: queries `ID3D10RasterizerState` into `pp_rasterizer_state` and releases `d3d11_rs`.
    unsafe {
        finish_create(
            hr,
            d3d11_rs,
            &IID_ID3D10_RASTERIZER_STATE,
            pp_rasterizer_state,
        )
    }
}

unsafe extern "system-unwind" fn create_sampler_state(
    this: *mut c_void,
    p_sampler_desc: *const c_void,
    pp_sampler_state: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_sampler_state` to null if non-null.
    unsafe { init_return_ptr(pp_sampler_state) };
    if p_sampler_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_sampler_desc` is a non-null `D3D10_SAMPLER_DESC` pointer.
    let d10 = unsafe { &*p_sampler_desc.cast::<D3d10SamplerDesc>() };
    let d11 = d10_to_d11_sampler_desc(d10);
    let mut d3d11_ss = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11SamplerState`.
    let hr = unsafe {
        (vtbl.create_sampler_state)(
            d3d11,
            (&raw const d11).cast(),
            out_slot(pp_sampler_state, &mut d3d11_ss),
        )
    };
    // SAFETY: queries `ID3D10SamplerState` into `pp_sampler_state` and releases `d3d11_ss`.
    unsafe { finish_create(hr, d3d11_ss, &IID_ID3D10_SAMPLER_STATE, pp_sampler_state) }
}

unsafe extern "system-unwind" fn create_query(
    this: *mut c_void,
    p_query_desc: *const c_void,
    pp_query: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_query` to null if non-null.
    unsafe { init_return_ptr(pp_query) };
    if p_query_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_query_desc` is a non-null `D3D10_QUERY_DESC` pointer.
    let d10 = unsafe { &*p_query_desc.cast::<D3d10QueryDesc>() };
    let d11 = d10_to_d11_query_desc(d10);
    let mut d3d11_q = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11Query`.
    let hr = unsafe {
        (vtbl.create_query)(
            d3d11,
            (&raw const d11).cast(),
            out_slot(pp_query, &mut d3d11_q),
        )
    };
    // SAFETY: queries `ID3D10Query` into `pp_query` and releases `d3d11_q`.
    unsafe { finish_create(hr, d3d11_q, &IID_ID3D10_QUERY, pp_query) }
}

unsafe extern "system-unwind" fn create_predicate(
    this: *mut c_void,
    p_predicate_desc: *const c_void,
    pp_predicate: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_predicate` to null if non-null.
    unsafe { init_return_ptr(pp_predicate) };
    if p_predicate_desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_predicate_desc` is a non-null `D3D10_QUERY_DESC` pointer.
    let d10 = unsafe { &*p_predicate_desc.cast::<D3d10QueryDesc>() };
    let d11 = d10_to_d11_query_desc(d10);
    let mut d3d11_pred = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: creates the underlying `ID3D11Predicate`.
    let hr = unsafe {
        (vtbl.create_predicate)(
            d3d11,
            (&raw const d11).cast(),
            out_slot(pp_predicate, &mut d3d11_pred),
        )
    };
    // SAFETY: queries `ID3D10Predicate` into `pp_predicate` and releases `d3d11_pred`.
    unsafe { finish_create(hr, d3d11_pred, &IID_ID3D10_PREDICATE, pp_predicate) }
}

unsafe extern "system-unwind" fn create_counter(
    _this: *mut c_void,
    _p_counter_desc: *const c_void,
    pp_counter: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_counter` to null if non-null.
    unsafe { init_return_ptr(pp_counter) };
    E_NOTIMPL
}

unsafe extern "system-unwind" fn check_format_support(
    this: *mut c_void,
    format: u32,
    p_format_support: *mut u32,
) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `CheckFormatSupport` to `ID3D11Device`.
    unsafe { (vtbl.check_format_support)(d3d11, format, p_format_support) }
}

unsafe extern "system-unwind" fn check_multisample_quality_levels(
    this: *mut c_void,
    format: u32,
    sample_count: u32,
    p_num_quality_levels: *mut u32,
) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `CheckMultisampleQualityLevels` to `ID3D11Device`.
    unsafe {
        (vtbl.check_multisample_quality_levels)(d3d11, format, sample_count, p_num_quality_levels)
    }
}

unsafe extern "system-unwind" fn check_counter_info(
    _this: *mut c_void,
    p_counter_info: *mut c_void,
) {
    if !p_counter_info.is_null() {
        let info = D3d10CounterInfo {
            last_device_dependent_counter: 0x4000_0000,
            num_simultaneous_counters: 0,
            num_detectable_parallel_units: 0,
        };
        // SAFETY: `p_counter_info` is a non-null `D3D10_COUNTER_INFO` out-pointer.
        unsafe { *p_counter_info.cast::<D3d10CounterInfo>() = info };
    }
}

const unsafe extern "system-unwind" fn check_counter(
    _this: *mut c_void,
    _p_desc: *const c_void,
    _p_type: *mut u32,
    _p_active_counters: *mut u32,
    _name: *mut c_char,
    _p_name_length: *mut u32,
    _units: *mut c_char,
    _p_units_length: *mut u32,
    _description: *mut c_char,
    _p_description_length: *mut u32,
) -> i32 {
    E_NOTIMPL
}

unsafe extern "system-unwind" fn get_creation_flags(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `GetCreationFlags` to `ID3D11Device`.
    unsafe { (vtbl.get_creation_flags)(d3d11) }
}

unsafe extern "system-unwind" fn open_shared_resource(
    this: *mut c_void,
    h_resource: *mut c_void,
    returned_interface: *const c_void,
    pp_resource: *mut *mut c_void,
) -> i32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: forwards `OpenSharedResource` to `ID3D11Device`.
    unsafe { (vtbl.open_shared_resource)(d3d11, h_resource, returned_interface, pp_resource) }
}

const unsafe extern "system-unwind" fn set_text_filter_size(
    _this: *mut c_void,
    _width: u32,
    _height: u32,
) {
}

unsafe extern "system-unwind" fn get_text_filter_size(
    _this: *mut c_void,
    p_width: *mut u32,
    p_height: *mut u32,
) {
    if !p_width.is_null() {
        // SAFETY: `p_width` is a non-null out-pointer.
        unsafe { *p_width = 0 };
    }
    if !p_height.is_null() {
        // SAFETY: `p_height` is a non-null out-pointer.
        unsafe { *p_height = 0 };
    }
}

unsafe extern "system-unwind" fn create_shader_resource_view1(
    this: *mut c_void,
    p_resource: *mut c_void,
    p_desc: *const c_void,
    pp_sr_view: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_sr_view` to null if non-null.
    unsafe { init_return_ptr(pp_sr_view) };
    if p_resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `p_resource` is a non-null resource `Wrapper`.
    let res = unsafe { d3d11_of(p_resource) };
    let mut d3d11_srv = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_SHADER_RESOURCE_VIEW_DESC1` has the same binary layout as `D3D11_SHADER_RESOURCE_VIEW_DESC`.
    let hr = unsafe {
        (vtbl.create_shader_resource_view)(d3d11, res, p_desc, out_slot(pp_sr_view, &mut d3d11_srv))
    };
    // SAFETY: queries `ID3D10ShaderResourceView1` into `pp_sr_view` and releases `d3d11_srv`.
    unsafe {
        finish_create(
            hr,
            d3d11_srv,
            &IID_ID3D10_SHADER_RESOURCE_VIEW_1,
            pp_sr_view,
        )
    }
}

unsafe extern "system-unwind" fn create_blend_state1(
    this: *mut c_void,
    p_blend_state_desc: *const c_void,
    pp_blend_state: *mut *mut c_void,
) -> i32 {
    // SAFETY: initializes `pp_blend_state` to null if non-null.
    unsafe { init_return_ptr(pp_blend_state) };
    let mut d3d11_bs = ptr::null_mut();
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_BLEND_DESC1` has the same binary layout as `D3D11_BLEND_DESC`.
    let hr = unsafe {
        (vtbl.create_blend_state)(
            d3d11,
            p_blend_state_desc,
            out_slot(pp_blend_state, &mut d3d11_bs),
        )
    };
    // SAFETY: queries `ID3D10BlendState1` into `pp_blend_state` and releases `d3d11_bs`.
    unsafe { finish_create(hr, d3d11_bs, &IID_ID3D10_BLEND_STATE_1, pp_blend_state) }
}

unsafe extern "system-unwind" fn get_feature_level(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Device`.
    let d3d11 = unsafe { wrapper_d3d11(this) };
    // SAFETY: `d3d11` is a live `ID3D11Device`.
    let vtbl = unsafe { com::vtbl::<ID3D11DeviceVtbl>(d3d11) };
    // SAFETY: `D3D10_FEATURE_LEVEL1` values match `D3D_FEATURE_LEVEL`; forwards directly.
    unsafe { (vtbl.get_feature_level)(d3d11) }
}

/// `ID3D10Device1` vtable for [`Device`].
pub static DEVICE_VTBL: ID3D10Device1Vtbl = ID3D10Device1Vtbl {
    query_interface,
    add_ref,
    release,
    vs_set_constant_buffers,
    ps_set_shader_resources,
    ps_set_shader,
    ps_set_samplers,
    vs_set_shader,
    draw_indexed,
    draw,
    ps_set_constant_buffers,
    ia_set_input_layout,
    ia_set_vertex_buffers,
    ia_set_index_buffer,
    draw_indexed_instanced,
    draw_instanced,
    gs_set_constant_buffers,
    gs_set_shader,
    ia_set_primitive_topology,
    vs_set_shader_resources,
    vs_set_samplers,
    set_predication,
    gs_set_shader_resources,
    gs_set_samplers,
    om_set_render_targets,
    om_set_blend_state,
    om_set_depth_stencil_state,
    so_set_targets,
    draw_auto,
    rs_set_state,
    rs_set_viewports,
    rs_set_scissor_rects,
    copy_subresource_region,
    copy_resource,
    update_subresource,
    clear_render_target_view,
    clear_depth_stencil_view,
    generate_mips,
    resolve_subresource,
    vs_get_constant_buffers,
    ps_get_shader_resources,
    ps_get_shader,
    ps_get_samplers,
    vs_get_shader,
    ps_get_constant_buffers,
    ia_get_input_layout,
    ia_get_vertex_buffers,
    ia_get_index_buffer,
    gs_get_constant_buffers,
    gs_get_shader,
    ia_get_primitive_topology,
    vs_get_shader_resources,
    vs_get_samplers,
    get_predication,
    gs_get_shader_resources,
    gs_get_samplers,
    om_get_render_targets,
    om_get_blend_state,
    om_get_depth_stencil_state,
    so_get_targets,
    rs_get_state,
    rs_get_viewports,
    rs_get_scissor_rects,
    get_device_removed_reason,
    set_exception_mode,
    get_exception_mode,
    get_private_data,
    set_private_data,
    set_private_data_interface,
    clear_state,
    flush,
    create_buffer,
    create_texture1d,
    create_texture2d,
    create_texture3d,
    create_shader_resource_view,
    create_render_target_view,
    create_depth_stencil_view,
    create_input_layout,
    create_vertex_shader,
    create_geometry_shader,
    create_geometry_shader_with_stream_output,
    create_pixel_shader,
    create_blend_state,
    create_depth_stencil_state,
    create_rasterizer_state,
    create_sampler_state,
    create_query,
    create_predicate,
    create_counter,
    check_format_support,
    check_multisample_quality_levels,
    check_counter_info,
    check_counter,
    get_creation_flags,
    open_shared_resource,
    set_text_filter_size,
    get_text_filter_size,
    create_shader_resource_view1,
    create_blend_state1,
    get_feature_level,
};
