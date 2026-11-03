//! `ID3D11Device5` method dispatch and COM vtable for [`super::Device`] (`d3d11_device.cpp`).

use core::{
    ffi::{c_char, c_void},
    ptr,
};

use dxmt_util::com::{
    E_FAIL, E_INVALIDARG, E_NOTIMPL, Guid, Hresult, IUnknownVtbl, S_OK, add_ref, write_out,
};

use crate::{
    LOG,
    abi::{
        IID_ID3D11_DEVICE_CONTEXT_1, IID_ID3D11_DEVICE_CONTEXT_2, IID_ID3D11_DEVICE_CONTEXT_3,
        records::{
            D3d11BlendDesc, D3d11BlendDesc1, D3d11BufferDesc, D3d11CounterInfo,
            D3d11DepthStencilDesc, D3d11DepthStencilViewDesc, D3d11InputElementDesc,
            D3d11QueryDesc, D3d11QueryDesc1, D3d11RasterizerDesc, D3d11RasterizerDesc1,
            D3d11RasterizerDesc2, D3d11RenderTargetViewDesc, D3d11RenderTargetViewDesc1,
            D3d11SamplerDesc, D3d11ShaderResourceViewDesc, D3d11ShaderResourceViewDesc1,
            D3d11SoDeclarationEntry, D3d11SubresourceData, D3d11Texture1dDesc, D3d11Texture2dDesc,
            D3d11Texture2dDesc1, D3d11Texture3dDesc, D3d11Texture3dDesc1,
            D3d11UnorderedAccessViewDesc, D3d11UnorderedAccessViewDesc1,
        },
        vtbl::ID3D11Device5Vtbl,
    },
    device::{Device, container_add_ref, container_query_interface, container_release},
};

const D3D11_RESOURCE_MISC_TILE_POOL: u32 = 0x0002_0000;
const D3D11_USAGE_STAGING: u32 = 3;
const D3D11_CREATE_DEVICE_VIDEO_SUPPORT: u32 = 0x0000_0800;

/// `ID3D11Device5` vtable for [`Device`].
pub static VTBL: ID3D11Device5Vtbl = ID3D11Device5Vtbl {
    query_interface,
    add_ref: device_add_ref,
    release: device_release,
    create_buffer,
    create_texture1d,
    create_texture2d,
    create_texture3d,
    create_shader_resource_view,
    create_unordered_access_view,
    create_render_target_view,
    create_depth_stencil_view,
    create_input_layout,
    create_vertex_shader,
    create_geometry_shader,
    create_geometry_shader_with_stream_output,
    create_pixel_shader,
    create_hull_shader,
    create_domain_shader,
    create_compute_shader,
    create_class_linkage,
    create_blend_state,
    create_depth_stencil_state,
    create_rasterizer_state,
    create_sampler_state,
    create_query,
    create_predicate,
    create_counter,
    create_deferred_context,
    open_shared_resource,
    check_format_support,
    check_multisample_quality_levels,
    check_counter_info,
    check_counter,
    check_feature_support,
    get_private_data,
    set_private_data,
    set_private_data_interface,
    get_feature_level,
    get_creation_flags,
    get_device_removed_reason,
    get_immediate_context,
    set_exception_mode,
    get_exception_mode,
    get_immediate_context1,
    create_deferred_context1,
    create_blend_state1,
    create_rasterizer_state1,
    create_device_context_state,
    open_shared_resource1,
    open_shared_resource_by_name,
    get_immediate_context2,
    create_deferred_context2,
    get_resource_tiling,
    check_multisample_quality_levels1,
    create_texture2d1,
    create_texture3d1,
    create_rasterizer_state2,
    create_shader_resource_view1,
    create_unordered_access_view1,
    create_render_target_view1,
    create_query1,
    get_immediate_context3,
    create_deferred_context3,
    write_to_subresource,
    read_from_subresource,
    register_device_removed_event,
    unregister_device_removed,
    open_shared_fence,
    create_fence,
};

unsafe extern "system-unwind" fn query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a valid `Device` pointer.
    let container = unsafe { (*this.cast::<Device>()).container };
    // SAFETY: `container` is the enclosing `DxgiDevice`.
    unsafe { container_query_interface(container, riid, out) }
}

unsafe extern "system-unwind" fn device_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Device` pointer.
    let container = unsafe { (*this.cast::<Device>()).container };
    // SAFETY: `container` is the enclosing `DxgiDevice`.
    unsafe { container_add_ref(container) }
}

unsafe extern "system-unwind" fn device_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Device` pointer.
    let container = unsafe { (*this.cast::<Device>()).container };
    // SAFETY: `container` is the enclosing `DxgiDevice`.
    unsafe { container_release(container) }
}

unsafe extern "system-unwind" fn create_buffer(
    this: *mut c_void,
    desc: *const c_void,
    init: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let desc_typed = desc.cast::<D3d11BufferDesc>();
    // SAFETY: `desc_typed` was checked non-null above.
    let d = unsafe { &*desc_typed };
    if d.byte_width == 0 && (d.misc_flags & D3D11_RESOURCE_MISC_TILE_POOL) == 0 {
        return E_INVALIDARG;
    }
    // Validate `Usage` before dispatching so invalid usage values never abort (P10).
    if d.usage > D3D11_USAGE_STAGING {
        return E_INVALIDARG;
    }
    crate::resource::create_buffer(this, desc_typed, init.cast::<D3d11SubresourceData>(), out)
}

unsafe extern "system-unwind" fn create_texture1d(
    this: *mut c_void,
    desc: *const c_void,
    init: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let desc_typed = desc.cast::<D3d11Texture1dDesc>();
    // SAFETY: `desc_typed` was checked non-null above.
    let d = unsafe { &*desc_typed };
    if d.usage > D3D11_USAGE_STAGING {
        return E_INVALIDARG;
    }
    crate::resource::create_texture1d(this, desc_typed, init.cast::<D3d11SubresourceData>(), out)
}

unsafe extern "system-unwind" fn create_texture2d(
    this: *mut c_void,
    desc: *const c_void,
    init: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let desc_typed = desc.cast::<D3d11Texture2dDesc>();
    // SAFETY: `desc_typed` was checked non-null above.
    let d = unsafe { &*desc_typed };
    if d.usage > D3D11_USAGE_STAGING {
        return E_INVALIDARG;
    }
    crate::resource::create_texture2d(this, desc_typed, init.cast::<D3d11SubresourceData>(), out)
}

unsafe extern "system-unwind" fn create_texture3d(
    this: *mut c_void,
    desc: *const c_void,
    init: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let desc_typed = desc.cast::<D3d11Texture3dDesc>();
    // SAFETY: `desc_typed` was checked non-null above.
    let d = unsafe { &*desc_typed };
    if d.usage > D3D11_USAGE_STAGING {
        return E_INVALIDARG;
    }
    crate::resource::create_texture3d(this, desc_typed, init.cast::<D3d11SubresourceData>(), out)
}

unsafe extern "system-unwind" fn create_shader_resource_view(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::view::create_srv(
        this,
        resource,
        desc.cast::<D3d11ShaderResourceViewDesc>(),
        out,
    )
}

unsafe extern "system-unwind" fn create_unordered_access_view(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::view::create_uav(
        this,
        resource,
        desc.cast::<D3d11UnorderedAccessViewDesc>(),
        out,
    )
}

unsafe extern "system-unwind" fn create_render_target_view(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::view::create_rtv(
        this,
        resource,
        desc.cast::<D3d11RenderTargetViewDesc>(),
        out,
    )
}

unsafe extern "system-unwind" fn create_depth_stencil_view(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::view::create_dsv(
        this,
        resource,
        desc.cast::<D3d11DepthStencilViewDesc>(),
        out,
    )
}

unsafe extern "system-unwind" fn create_input_layout(
    this: *mut c_void,
    descs: *const c_void,
    count: u32,
    bytecode: *const c_void,
    bytecode_len: usize,
    out: *mut *mut c_void,
) -> Hresult {
    crate::input_layout::create_input_layout(
        this,
        descs.cast::<D3d11InputElementDesc>(),
        count,
        bytecode,
        bytecode_len,
        out,
    )
}

unsafe extern "system-unwind" fn create_vertex_shader(
    this: *mut c_void,
    bytecode: *const c_void,
    len: usize,
    linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::shader::create_vertex_shader(this, bytecode, len, linkage, out)
}

unsafe extern "system-unwind" fn create_geometry_shader(
    this: *mut c_void,
    bytecode: *const c_void,
    len: usize,
    linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::shader::create_geometry_shader(this, bytecode, len, linkage, out)
}

unsafe extern "system-unwind" fn create_geometry_shader_with_stream_output(
    this: *mut c_void,
    bytecode: *const c_void,
    len: usize,
    so_decl: *const c_void,
    num_entries: u32,
    strides: *const u32,
    num_strides: u32,
    rasterized_stream: u32,
    _linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::input_layout::create_gs_with_so(
        this,
        bytecode,
        len,
        (so_decl.cast::<D3d11SoDeclarationEntry>(), num_entries),
        (strides, num_strides),
        rasterized_stream,
        out,
    )
}

unsafe extern "system-unwind" fn create_pixel_shader(
    this: *mut c_void,
    bytecode: *const c_void,
    len: usize,
    linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::shader::create_pixel_shader(this, bytecode, len, linkage, out)
}

unsafe extern "system-unwind" fn create_hull_shader(
    this: *mut c_void,
    bytecode: *const c_void,
    len: usize,
    linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::shader::create_hull_shader(this, bytecode, len, linkage, out)
}

unsafe extern "system-unwind" fn create_domain_shader(
    this: *mut c_void,
    bytecode: *const c_void,
    len: usize,
    linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::shader::create_domain_shader(this, bytecode, len, linkage, out)
}

unsafe extern "system-unwind" fn create_compute_shader(
    this: *mut c_void,
    bytecode: *const c_void,
    len: usize,
    linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::shader::create_compute_shader(this, bytecode, len, linkage, out)
}

unsafe extern "system-unwind" fn create_class_linkage(
    this: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: Forwarding COM caller pointers to `create_class_linkage`.
    unsafe { crate::class_linkage::create_class_linkage(this, out) }
}

unsafe extern "system-unwind" fn create_blend_state(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::state::create_blend_state(this, desc.cast::<D3d11BlendDesc>(), out)
}

unsafe extern "system-unwind" fn create_depth_stencil_state(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::state::create_depth_stencil_state(this, desc.cast::<D3d11DepthStencilDesc>(), out)
}

unsafe extern "system-unwind" fn create_rasterizer_state(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::state::create_rasterizer_state(this, desc.cast::<D3d11RasterizerDesc>(), out)
}

unsafe extern "system-unwind" fn create_sampler_state(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::state::create_sampler_state(this, desc.cast::<D3d11SamplerDesc>(), out)
}

unsafe extern "system-unwind" fn create_query(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::query::create_query(this, desc.cast::<D3d11QueryDesc>(), out)
}

unsafe extern "system-unwind" fn create_predicate(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::query::create_predicate(this, desc.cast::<D3d11QueryDesc>(), out)
}

unsafe extern "system-unwind" fn create_counter(
    _this: *mut c_void,
    _desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    LOG.warn("CreateCounter: not supported");
    E_NOTIMPL
}

unsafe extern "system-unwind" fn create_deferred_context(
    this: *mut c_void,
    flags: u32,
    out: *mut *mut c_void,
) -> Hresult {
    crate::context::deferred::create_deferred(this, flags, out)
}

unsafe extern "system-unwind" fn open_shared_resource(
    this: *mut c_void,
    handle: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::resource::open_shared(this, handle, riid.cast::<Guid>(), out)
}

unsafe extern "system-unwind" fn check_format_support(
    this: *mut c_void,
    format: u32,
    out: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    dev.inspection().check_format_support(format, out)
}

unsafe extern "system-unwind" fn check_multisample_quality_levels(
    this: *mut c_void,
    format: u32,
    sample_count: u32,
    out: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    dev.inspection()
        .check_multisample_quality_levels(format, sample_count, 0, out)
}

unsafe extern "system-unwind" fn check_counter_info(_this: *mut c_void, info: *mut c_void) {
    if info.is_null() {
        return;
    }
    // SAFETY: `info` was checked non-null above.
    unsafe {
        *info.cast::<D3d11CounterInfo>() = D3d11CounterInfo {
            last_device_dependent_counter: 0x4000_0000,
            num_simultaneous_counters: 0,
            num_detectable_parallel_units: 0,
        };
    }
}

unsafe extern "system-unwind" fn check_counter(
    _this: *mut c_void,
    _desc: *const c_void,
    _type_: *mut u32,
    _active: *mut u32,
    _name: *mut c_char,
    _name_len: *mut u32,
    _units: *mut c_char,
    _units_len: *mut u32,
    _desc_str: *mut c_char,
    _desc_str_len: *mut u32,
) -> Hresult {
    LOG.warn("CheckCounter: not supported");
    E_NOTIMPL
}

unsafe extern "system-unwind" fn check_feature_support(
    this: *mut c_void,
    feature: u32,
    data: *mut c_void,
    size: u32,
) -> Hresult {
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    dev.inspection().check_feature_support(feature, data, size)
}

unsafe extern "system-unwind" fn get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `Device` pointer.
    let container = unsafe { (*this.cast::<Device>()).container };
    // SAFETY: `container` is the enclosing `DxgiDevice`.
    let dxgi = unsafe { &*container };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::get_data`.
    unsafe { dxgi.private_data.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `Device` pointer.
    let container = unsafe { (*this.cast::<Device>()).container };
    // SAFETY: `container` is the enclosing `DxgiDevice`.
    let dxgi = unsafe { &*container };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::set_data`.
    unsafe { dxgi.private_data.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    unknown: *const c_void,
) -> Hresult {
    // SAFETY: `guid` is a `GUID` pointer passed by the COM caller.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a valid `Device` pointer.
    let container = unsafe { (*this.cast::<Device>()).container };
    // SAFETY: `container` is the enclosing `DxgiDevice`.
    let dxgi = unsafe { &*container };
    // SAFETY: Forwarding COM caller pointers to `PrivateData::set_interface`.
    unsafe { dxgi.private_data.set_interface(g, unknown.cast_mut()) }
}

const unsafe extern "system-unwind" fn get_feature_level(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    dev.feature_level()
}

const unsafe extern "system-unwind" fn get_creation_flags(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    dev.raw_flags() & 0x7fff_ffff
}

const unsafe extern "system-unwind" fn get_device_removed_reason(_this: *mut c_void) -> Hresult {
    S_OK
}

unsafe extern "system-unwind" fn get_immediate_context(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    let imm = dev.immediate().cast::<c_void>();
    if !imm.is_null() {
        // SAFETY: `imm` is a live `ID3D11DeviceContext*` COM pointer.
        unsafe {
            add_ref(imm);
        }
    }
    // SAFETY: `out` was checked non-null above.
    unsafe {
        write_out(out, imm);
    }
}

unsafe extern "system-unwind" fn set_exception_mode(_this: *mut c_void, _flags: u32) -> Hresult {
    LOG.err("Not implemented");
    E_NOTIMPL
}

unsafe extern "system-unwind" fn get_exception_mode(_this: *mut c_void) -> u32 {
    LOG.err("Not implemented");
    0
}

unsafe extern "system-unwind" fn get_immediate_context1(this: *mut c_void, out: *mut *mut c_void) {
    query_immediate_context(this, &IID_ID3D11_DEVICE_CONTEXT_1, out);
}

unsafe extern "system-unwind" fn create_deferred_context1(
    this: *mut c_void,
    flags: u32,
    out: *mut *mut c_void,
) -> Hresult {
    crate::context::deferred::create_deferred(this, flags, out)
}

unsafe extern "system-unwind" fn create_blend_state1(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::state::create_blend_state1(this, desc.cast::<D3d11BlendDesc1>(), out)
}

unsafe extern "system-unwind" fn create_rasterizer_state1(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::state::create_rasterizer_state1(this, desc.cast::<D3d11RasterizerDesc1>(), out)
}

unsafe extern "system-unwind" fn create_device_context_state(
    this: *mut c_void,
    flags: u32,
    levels: *const u32,
    num_levels: u32,
    sdk_version: u32,
    emulated_iface: *const c_void,
    chosen_level: *mut u32,
    out: *mut *mut c_void,
) -> Hresult {
    crate::context::context_state::create_device_context_state(
        this,
        flags,
        (levels, num_levels),
        sdk_version,
        emulated_iface,
        chosen_level,
        out,
    )
}

unsafe extern "system-unwind" fn open_shared_resource1(
    this: *mut c_void,
    handle: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::resource::open_shared1(this, handle, riid.cast::<Guid>(), out)
}

unsafe extern "system-unwind" fn open_shared_resource_by_name(
    this: *mut c_void,
    name: *const c_void,
    access: u32,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::resource::open_shared_by_name(this, name.cast::<u16>(), access, riid.cast::<Guid>(), out)
}

unsafe extern "system-unwind" fn get_immediate_context2(this: *mut c_void, out: *mut *mut c_void) {
    query_immediate_context(this, &IID_ID3D11_DEVICE_CONTEXT_2, out);
}

unsafe extern "system-unwind" fn create_deferred_context2(
    this: *mut c_void,
    flags: u32,
    out: *mut *mut c_void,
) -> Hresult {
    crate::context::deferred::create_deferred(this, flags, out)
}

unsafe extern "system-unwind" fn get_resource_tiling(
    _this: *mut c_void,
    _resource: *mut c_void,
    _num_tiles: *mut u32,
    _packed_mip: *mut c_void,
    _tile_shape: *mut c_void,
    _num_subresource_tilings: *mut u32,
    _first_subresource: u32,
    _tilings: *mut c_void,
) {
    LOG.err("GetResourceTiling: Not implemented");
    #[cfg(all(not(windows), not(test)))]
    {
        // SAFETY: Recording not-implemented call for native probe diagnostic.
        unsafe {
            winemetal_sys::wmt_native_not_implemented();
        }
    }
}

unsafe extern "system-unwind" fn check_multisample_quality_levels1(
    this: *mut c_void,
    format: u32,
    sample_count: u32,
    flags: u32,
    out: *mut u32,
) -> Hresult {
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    dev.inspection()
        .check_multisample_quality_levels(format, sample_count, flags, out)
}

unsafe extern "system-unwind" fn create_texture2d1(
    this: *mut c_void,
    desc: *const c_void,
    init: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let desc_typed = desc.cast::<D3d11Texture2dDesc1>();
    // SAFETY: `desc_typed` was checked non-null above.
    let d = unsafe { &*desc_typed };
    if d.usage > D3D11_USAGE_STAGING {
        return E_INVALIDARG;
    }
    crate::resource::create_texture2d1(this, desc_typed, init.cast::<D3d11SubresourceData>(), out)
}

unsafe extern "system-unwind" fn create_texture3d1(
    this: *mut c_void,
    desc: *const c_void,
    init: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    let desc_typed = desc.cast::<D3d11Texture3dDesc1>();
    // SAFETY: `desc_typed` was checked non-null above.
    let d = unsafe { &*desc_typed };
    if d.usage > D3D11_USAGE_STAGING {
        return E_INVALIDARG;
    }
    crate::resource::create_texture3d1(this, desc_typed, init.cast::<D3d11SubresourceData>(), out)
}

unsafe extern "system-unwind" fn create_rasterizer_state2(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::state::create_rasterizer_state2(this, desc.cast::<D3d11RasterizerDesc2>(), out)
}

unsafe extern "system-unwind" fn create_shader_resource_view1(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::view::create_srv1(
        this,
        resource,
        desc.cast::<D3d11ShaderResourceViewDesc1>(),
        out,
    )
}

unsafe extern "system-unwind" fn create_unordered_access_view1(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::view::create_uav1(
        this,
        resource,
        desc.cast::<D3d11UnorderedAccessViewDesc1>(),
        out,
    )
}

unsafe extern "system-unwind" fn create_render_target_view1(
    this: *mut c_void,
    resource: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::view::create_rtv1(
        this,
        resource,
        desc.cast::<D3d11RenderTargetViewDesc1>(),
        out,
    )
}

unsafe extern "system-unwind" fn create_query1(
    this: *mut c_void,
    desc: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::query::create_query1(this, desc.cast::<D3d11QueryDesc1>(), out)
}

unsafe extern "system-unwind" fn get_immediate_context3(this: *mut c_void, out: *mut *mut c_void) {
    query_immediate_context(this, &IID_ID3D11_DEVICE_CONTEXT_3, out);
}

unsafe extern "system-unwind" fn create_deferred_context3(
    this: *mut c_void,
    flags: u32,
    out: *mut *mut c_void,
) -> Hresult {
    crate::context::deferred::create_deferred(this, flags, out)
}

unsafe extern "system-unwind" fn write_to_subresource(
    _this: *mut c_void,
    _dst_resource: *mut c_void,
    _dst_subresource: u32,
    _dst_box: *const c_void,
    _src_data: *const c_void,
    _src_row_pitch: u32,
    _src_depth_pitch: u32,
) {
    LOG.err("WriteToSubresource: Not implemented");
    #[cfg(all(not(windows), not(test)))]
    {
        // SAFETY: Recording not-implemented call for native probe diagnostic.
        unsafe {
            winemetal_sys::wmt_native_not_implemented();
        }
    }
}

unsafe extern "system-unwind" fn read_from_subresource(
    _this: *mut c_void,
    _dst_data: *mut c_void,
    _dst_row_pitch: u32,
    _dst_depth_pitch: u32,
    _src_resource: *mut c_void,
    _src_subresource: u32,
    _src_box: *const c_void,
) {
    LOG.err("ReadFromSubresource: Not implemented");
    #[cfg(all(not(windows), not(test)))]
    {
        // SAFETY: Recording not-implemented call for native probe diagnostic.
        unsafe {
            winemetal_sys::wmt_native_not_implemented();
        }
    }
}

unsafe extern "system-unwind" fn open_shared_fence(
    this: *mut c_void,
    handle: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::fence::open_shared_fence(this, handle, riid.cast::<Guid>(), out)
}

unsafe extern "system-unwind" fn create_fence(
    this: *mut c_void,
    initial_value: u64,
    flags: u32,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe {
            write_out(out, ptr::null_mut());
        }
    }
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    if (dev.raw_flags() & D3D11_CREATE_DEVICE_VIDEO_SUPPORT) != 0 {
        return E_FAIL;
    }
    crate::fence::create_fence(this, initial_value, flags, riid.cast::<Guid>(), out)
}

const unsafe extern "system-unwind" fn register_device_removed_event(
    _this: *mut c_void,
    _event: *mut c_void,
    _cookie: *mut u32,
) -> Hresult {
    S_OK
}

const unsafe extern "system-unwind" fn unregister_device_removed(_this: *mut c_void, _cookie: u32) {
}

fn query_immediate_context(this: *mut c_void, iid: &Guid, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a valid `Device` pointer.
    let dev = unsafe { &*this.cast::<Device>() };
    let imm = dev.immediate().cast::<c_void>();
    if imm.is_null() {
        // SAFETY: `out` was checked non-null above.
        unsafe {
            write_out(out, ptr::null_mut());
        }
        return;
    }
    // SAFETY: `imm` is a valid COM object with an `IUnknownVtbl` header.
    let vtbl = unsafe { *imm.cast::<*const IUnknownVtbl>() };
    // SAFETY: `vtbl` is a valid `IUnknownVtbl`.
    let qi = unsafe { (*vtbl).query_interface };
    // SAFETY: Querying `imm` for the requested `ID3D11DeviceContext*` version.
    let _ = unsafe { qi(imm, ptr::from_ref(iid).cast(), out) };
}
