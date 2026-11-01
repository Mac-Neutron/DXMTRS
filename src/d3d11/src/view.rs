//! D3D11 resource views (`d3d11_view.hpp`: `Srv`, `Rtv`, `Dsv`, `Uav`).

use core::ffi::c_void;
use std::sync::{Arc, Mutex};

use dxmt_core::{
    counter::Counter,
    encoder::BufferSlice,
    resource::{Buffer, BufferViewKey, RenamableTexturePool, Texture, TextureViewKey},
};
use dxmt_util::private_data::PrivateData;

use crate::{
    abi::{
        IID_ID3D10_DEPTH_STENCIL_VIEW, IID_ID3D10_DEVICE_CHILD, IID_ID3D10_RENDER_TARGET_VIEW,
        IID_ID3D10_SHADER_RESOURCE_VIEW, IID_ID3D10_SHADER_RESOURCE_VIEW_1, IID_ID3D10_VIEW,
        IID_ID3D11_DEPTH_STENCIL_VIEW, IID_ID3D11_DEVICE_CHILD, IID_ID3D11_RENDER_TARGET_VIEW,
        IID_ID3D11_RENDER_TARGET_VIEW_1, IID_ID3D11_RESOURCE, IID_ID3D11_SHADER_RESOURCE_VIEW,
        IID_ID3D11_SHADER_RESOURCE_VIEW_1, IID_ID3D11_UNORDERED_ACCESS_VIEW,
        IID_ID3D11_UNORDERED_ACCESS_VIEW_1, IID_ID3D11_VIEW, IID_IUNKNOWN,
        records::{
            D3d11DepthStencilViewDesc, D3d11RenderTargetViewDesc, D3d11RenderTargetViewDesc1,
            D3d11ShaderResourceViewDesc, D3d11ShaderResourceViewDesc1,
            D3d11UnorderedAccessViewDesc, D3d11UnorderedAccessViewDesc1,
        },
        vtbl::{
            ID3D10DepthStencilViewVtbl, ID3D10RenderTargetViewVtbl, ID3D10ShaderResourceView1Vtbl,
            ID3D11DepthStencilViewVtbl, ID3D11RenderTargetView1Vtbl, ID3D11ShaderResourceView1Vtbl,
            ID3D11UnorderedAccessView1Vtbl, IUnknownVtbl,
        },
    },
    com::{
        ComObjectWithInitialRef, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, Guid, Hresult,
        S_OK, add_ref, vtbl,
    },
    d3d10,
    device::Device,
    resource::{self, Resource, ResourceDesc, ResourceKind},
    subresource::ResourceSubset,
};

pub mod helper;

/// Shader resource view (`MTLD3D11ShaderResourceView`).
pub struct Srv {
    pub resource: *mut c_void,
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub buffer_view: Option<BufferViewKey>,
    pub texture_view: Option<TextureViewKey>,
    pub slice: BufferSlice,
    pub subset: ResourceSubset,
    pub format: u32,
}

impl Srv {
    /// Recovers `&Srv` from an `ID3D11ShaderResourceView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: non-null `ID3D11ShaderResourceView*` pointers are `SrvObject` instances.
        Some(unsafe { &(*iface.cast::<SrvObject>()).inner })
    }
}

/// Render target view (`MTLD3D11RenderTargetView`).
pub struct Rtv {
    pub resource: *mut c_void,
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub buffer_view: Option<BufferViewKey>,
    pub texture_view: Option<TextureViewKey>,
    pub slice: BufferSlice,
    pub subset: ResourceSubset,
    pub format: u32,
    pub pixel_format: u32,
    pub array_length: u32,
    pub width: u32,
    pub height: u32,
    pub sample_count: u32,
}

impl Rtv {
    /// Recovers `&Rtv` from an `ID3D11RenderTargetView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: non-null `ID3D11RenderTargetView*` pointers are `RtvObject` instances.
        Some(unsafe { &(*iface.cast::<RtvObject>()).inner })
    }
}

/// Depth-stencil view (`MTLD3D11DepthStencilView`).
pub struct Dsv {
    pub resource: *mut c_void,
    pub texture: Option<Arc<Texture>>,
    pub texture_view: Option<TextureViewKey>,
    pub subset: ResourceSubset,
    pub format: u32,
    pub planar_flags: u8,
    pub readonly_flags: u8,
    pub array_length: u32,
    pub width: u32,
    pub height: u32,
    pub sample_count: u32,
}

impl Dsv {
    /// Recovers `&Dsv` from an `ID3D11DepthStencilView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: non-null `ID3D11DepthStencilView*` pointers are `DsvObject` instances.
        Some(unsafe { &(*iface.cast::<DsvObject>()).inner })
    }
}

/// Unordered access view (`MTLD3D11UnorderedAccessView`).
pub struct Uav {
    pub resource: *mut c_void,
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub buffer_view: Option<BufferViewKey>,
    pub texture_view: Option<TextureViewKey>,
    pub slice: BufferSlice,
    pub subset: ResourceSubset,
    pub format: u32,
    pub counter: Option<Arc<Counter>>,
}

impl Uav {
    /// Recovers `&Uav` from an `ID3D11UnorderedAccessView*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: non-null `ID3D11UnorderedAccessView*` pointers are `UavObject` instances.
        Some(unsafe { &(*iface.cast::<UavObject>()).inner })
    }
}

// --- Full COM objects wrapping `Srv`, `Rtv`, `Dsv`, `Uav` ---

/// Full COM object for `ID3D11ShaderResourceView1`.
#[repr(C)]
pub struct SrvObject {
    pub vtbl: &'static ID3D11ShaderResourceView1Vtbl,
    pub inner: Srv,
    pub refs: ComObjectWithInitialRef,
    pub private_data: PrivateData,
    pub device: *mut c_void,
    pub bind_flags: u32,
    pub desc: D3d11ShaderResourceViewDesc1,
    pub d3d10: d3d10::Wrapper<ID3D10ShaderResourceView1Vtbl>,
}

impl Drop for SrvObject {
    fn drop(&mut self) {
        Resource::release_private(self.inner.resource);
    }
}

impl SrvObject {
    /// Allocates a heap `SrvObject` with 1 public ref and 1 private ref on `inner.resource`.
    #[must_use]
    pub fn into_raw(
        inner: Srv,
        device: *mut c_void,
        bind_flags: u32,
        desc: D3d11ShaderResourceViewDesc1,
    ) -> *mut c_void {
        Resource::add_ref_private(inner.resource);
        let imm_ctx = Device::from_iface(device)
            .map_or(core::ptr::null_mut(), |d| d.immediate().cast::<c_void>());
        let boxed = Box::new(Self {
            vtbl: &SRV_VTBL,
            inner,
            refs: ComObjectWithInitialRef::new(),
            private_data: PrivateData::default(),
            device,
            bind_flags,
            desc,
            d3d10: d3d10::Wrapper::new(&d3d10::SRV_VTBL, core::ptr::null_mut(), imm_ctx),
        });
        let raw = Box::into_raw(boxed);
        let iface = raw.cast::<c_void>();
        // SAFETY: `raw` was just allocated by `Box::into_raw`.
        unsafe { (*raw).d3d10.d3d11 = iface };
        iface
    }
}

/// Full COM object for `ID3D11RenderTargetView1`.
#[repr(C)]
pub struct RtvObject {
    pub vtbl: &'static ID3D11RenderTargetView1Vtbl,
    pub inner: Rtv,
    pub refs: ComObjectWithInitialRef,
    pub private_data: PrivateData,
    pub device: *mut c_void,
    pub bind_flags: u32,
    pub desc: D3d11RenderTargetViewDesc1,
    pub pass_desc: helper::RenderPassAttachmentDesc,
    pub d3d10: d3d10::Wrapper<ID3D10RenderTargetViewVtbl>,
}

impl Drop for RtvObject {
    fn drop(&mut self) {
        Resource::release_private(self.inner.resource);
    }
}

impl RtvObject {
    /// Allocates a heap `RtvObject` with 1 public ref and 1 private ref on `inner.resource`.
    #[must_use]
    pub fn into_raw(
        inner: Rtv,
        device: *mut c_void,
        bind_flags: u32,
        desc: D3d11RenderTargetViewDesc1,
        pass_desc: helper::RenderPassAttachmentDesc,
    ) -> *mut c_void {
        Resource::add_ref_private(inner.resource);
        let imm_ctx = Device::from_iface(device)
            .map_or(core::ptr::null_mut(), |d| d.immediate().cast::<c_void>());
        let boxed = Box::new(Self {
            vtbl: &RTV_VTBL,
            inner,
            refs: ComObjectWithInitialRef::new(),
            private_data: PrivateData::default(),
            device,
            bind_flags,
            desc,
            pass_desc,
            d3d10: d3d10::Wrapper::new(&d3d10::RTV_VTBL, core::ptr::null_mut(), imm_ctx),
        });
        let raw = Box::into_raw(boxed);
        let iface = raw.cast::<c_void>();
        // SAFETY: `raw` was just allocated by `Box::into_raw`.
        unsafe { (*raw).d3d10.d3d11 = iface };
        iface
    }
}

/// Full COM object for `ID3D11DepthStencilView`.
#[repr(C)]
pub struct DsvObject {
    pub vtbl: &'static ID3D11DepthStencilViewVtbl,
    pub inner: Dsv,
    pub refs: ComObjectWithInitialRef,
    pub private_data: PrivateData,
    pub device: *mut c_void,
    pub bind_flags: u32,
    pub desc: D3d11DepthStencilViewDesc,
    pub pass_desc: helper::RenderPassAttachmentDesc,
    pub renamable: Option<Arc<Mutex<RenamableTexturePool>>>,
    pub d3d10: d3d10::Wrapper<ID3D10DepthStencilViewVtbl>,
}

impl Drop for DsvObject {
    fn drop(&mut self) {
        Resource::release_private(self.inner.resource);
    }
}

impl DsvObject {
    /// Allocates a heap `DsvObject` with 1 public ref and 1 private ref on `inner.resource`.
    #[must_use]
    pub fn into_raw(
        inner: Dsv,
        device: *mut c_void,
        bind_flags: u32,
        desc: D3d11DepthStencilViewDesc,
        pass_desc: helper::RenderPassAttachmentDesc,
        renamable: Option<Arc<Mutex<RenamableTexturePool>>>,
    ) -> *mut c_void {
        Resource::add_ref_private(inner.resource);
        let imm_ctx = Device::from_iface(device)
            .map_or(core::ptr::null_mut(), |d| d.immediate().cast::<c_void>());
        let boxed = Box::new(Self {
            vtbl: &DSV_VTBL,
            inner,
            refs: ComObjectWithInitialRef::new(),
            private_data: PrivateData::default(),
            device,
            bind_flags,
            desc,
            pass_desc,
            renamable,
            d3d10: d3d10::Wrapper::new(&d3d10::DSV_VTBL, core::ptr::null_mut(), imm_ctx),
        });
        let raw = Box::into_raw(boxed);
        let iface = raw.cast::<c_void>();
        // SAFETY: `raw` was just allocated by `Box::into_raw`.
        unsafe { (*raw).d3d10.d3d11 = iface };
        iface
    }
}

/// Full COM object for `ID3D11UnorderedAccessView1`.
#[repr(C)]
pub struct UavObject {
    pub vtbl: &'static ID3D11UnorderedAccessView1Vtbl,
    pub inner: Uav,
    pub refs: ComObjectWithInitialRef,
    pub private_data: PrivateData,
    pub device: *mut c_void,
    pub bind_flags: u32,
    pub desc: D3d11UnorderedAccessViewDesc1,
}

impl Drop for UavObject {
    fn drop(&mut self) {
        Resource::release_private(self.inner.resource);
    }
}

impl UavObject {
    /// Allocates a heap `UavObject` with 1 public ref and 1 private ref on `inner.resource`.
    #[must_use]
    pub fn into_raw(
        inner: Uav,
        device: *mut c_void,
        bind_flags: u32,
        desc: D3d11UnorderedAccessViewDesc1,
    ) -> *mut c_void {
        Resource::add_ref_private(inner.resource);
        let boxed = Box::new(Self {
            vtbl: &UAV_VTBL,
            inner,
            refs: ComObjectWithInitialRef::new(),
            private_data: PrivateData::default(),
            device,
            bind_flags,
            desc,
        });
        Box::into_raw(boxed).cast::<c_void>()
    }
}

unsafe fn query_parent_resource(resource: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    if resource.is_null() {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = core::ptr::null_mut() };
        return;
    }
    // SAFETY: `resource` is a live COM object.
    let unk = unsafe { vtbl::<IUnknownVtbl>(resource) };
    let iid = IID_ID3D11_RESOURCE;
    // SAFETY: queries `resource` for `ID3D11Resource`.
    let _ = unsafe { (unk.query_interface)(resource, (&raw const iid).cast(), out) };
}

// --- `SrvObject` vtable ---

unsafe extern "system-unwind" fn srv_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    // SAFETY: `riid` addresses a GUID.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    // SAFETY: `this` is a live `SrvObject`.
    let obj = unsafe { &*this.cast::<SrvObject>() };
    if iid == IID_IUNKNOWN
        || iid == IID_ID3D11_DEVICE_CHILD
        || iid == IID_ID3D11_VIEW
        || iid == IID_ID3D11_SHADER_RESOURCE_VIEW
        || iid == IID_ID3D11_SHADER_RESOURCE_VIEW_1
    {
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe { *out = this };
        return S_OK;
    }
    if iid == IID_ID3D10_DEVICE_CHILD
        || iid == IID_ID3D10_VIEW
        || iid == IID_ID3D10_SHADER_RESOURCE_VIEW
        || iid == IID_ID3D10_SHADER_RESOURCE_VIEW_1
    {
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe { *out = core::ptr::from_ref(&obj.d3d10).cast_mut().cast() };
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn srv_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `SrvObject`.
    unsafe { &*this.cast::<SrvObject>() }.refs.add_ref()
}

unsafe extern "system-unwind" fn srv_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `SrvObject`.
    let (rem, free) = unsafe { &*this.cast::<SrvObject>() }
        .refs
        .release_and_free();
    if free {
        // SAFETY: last reference was just released.
        drop(unsafe { Box::from_raw(this.cast::<SrvObject>()) });
    }
    rem
}

unsafe extern "system-unwind" fn srv_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `SrvObject`.
    let dev = unsafe { &*this.cast::<SrvObject>() }.device;
    // SAFETY: `dev` is a live COM object or null.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null.
    unsafe { *out = dev };
}

unsafe extern "system-unwind" fn srv_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `SrvObject`.
    let obj = unsafe { &*this.cast::<SrvObject>() };
    // SAFETY: forwards caller buffers to `PrivateData::get_data`.
    unsafe { obj.private_data.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn srv_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `SrvObject`.
    let obj = unsafe { &*this.cast::<SrvObject>() };
    // SAFETY: forwards caller buffer to `PrivateData::set_data`.
    unsafe { obj.private_data.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn srv_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    iface: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `SrvObject`.
    let obj = unsafe { &*this.cast::<SrvObject>() };
    // SAFETY: forwards `iface` to `PrivateData::set_interface`.
    unsafe { obj.private_data.set_interface(g, iface.cast_mut()) }
}

unsafe extern "system-unwind" fn srv_get_resource(this: *mut c_void, out: *mut *mut c_void) {
    // SAFETY: `this` is a live `SrvObject`.
    let res = unsafe { &*this.cast::<SrvObject>() }.inner.resource;
    // SAFETY: queries `res` for `ID3D11Resource`.
    unsafe { query_parent_resource(res, out) };
}

unsafe extern "system-unwind" fn srv_get_desc(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `SrvObject`.
    let obj = unsafe { &*this.cast::<SrvObject>() };
    let down = helper::downgrade_srv_desc(&obj.desc);
    // SAFETY: `desc_out` is non-null and writable.
    unsafe { *desc_out.cast::<D3d11ShaderResourceViewDesc>() = down };
}

unsafe extern "system-unwind" fn srv_get_desc1(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `SrvObject`.
    let obj = unsafe { &*this.cast::<SrvObject>() };
    // SAFETY: `desc_out` is non-null and writable.
    unsafe { *desc_out.cast::<D3d11ShaderResourceViewDesc1>() = obj.desc };
}

static SRV_VTBL: ID3D11ShaderResourceView1Vtbl = ID3D11ShaderResourceView1Vtbl {
    query_interface: srv_query_interface,
    add_ref: srv_add_ref,
    release: srv_release,
    get_device: srv_get_device,
    get_private_data: srv_get_private_data,
    set_private_data: srv_set_private_data,
    set_private_data_interface: srv_set_private_data_interface,
    get_resource: srv_get_resource,
    get_desc: srv_get_desc,
    get_desc1: srv_get_desc1,
};

// --- `RtvObject` vtable ---

unsafe extern "system-unwind" fn rtv_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    // SAFETY: `riid` addresses a GUID.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    // SAFETY: `this` is a live `RtvObject`.
    let obj = unsafe { &*this.cast::<RtvObject>() };
    if iid == IID_IUNKNOWN
        || iid == IID_ID3D11_DEVICE_CHILD
        || iid == IID_ID3D11_VIEW
        || iid == IID_ID3D11_RENDER_TARGET_VIEW
        || iid == IID_ID3D11_RENDER_TARGET_VIEW_1
    {
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe { *out = this };
        return S_OK;
    }
    if iid == IID_ID3D10_DEVICE_CHILD
        || iid == IID_ID3D10_VIEW
        || iid == IID_ID3D10_RENDER_TARGET_VIEW
    {
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe { *out = core::ptr::from_ref(&obj.d3d10).cast_mut().cast() };
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn rtv_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `RtvObject`.
    unsafe { &*this.cast::<RtvObject>() }.refs.add_ref()
}

unsafe extern "system-unwind" fn rtv_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `RtvObject`.
    let (rem, free) = unsafe { &*this.cast::<RtvObject>() }
        .refs
        .release_and_free();
    if free {
        // SAFETY: last reference was just released.
        drop(unsafe { Box::from_raw(this.cast::<RtvObject>()) });
    }
    rem
}

unsafe extern "system-unwind" fn rtv_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `RtvObject`.
    let dev = unsafe { &*this.cast::<RtvObject>() }.device;
    // SAFETY: `dev` is a live COM object or null.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null.
    unsafe { *out = dev };
}

unsafe extern "system-unwind" fn rtv_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `RtvObject`.
    let obj = unsafe { &*this.cast::<RtvObject>() };
    // SAFETY: forwards caller buffers to `PrivateData::get_data`.
    unsafe { obj.private_data.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn rtv_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `RtvObject`.
    let obj = unsafe { &*this.cast::<RtvObject>() };
    // SAFETY: forwards caller buffer to `PrivateData::set_data`.
    unsafe { obj.private_data.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn rtv_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    iface: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `RtvObject`.
    let obj = unsafe { &*this.cast::<RtvObject>() };
    // SAFETY: forwards `iface` to `PrivateData::set_interface`.
    unsafe { obj.private_data.set_interface(g, iface.cast_mut()) }
}

unsafe extern "system-unwind" fn rtv_get_resource(this: *mut c_void, out: *mut *mut c_void) {
    // SAFETY: `this` is a live `RtvObject`.
    let res = unsafe { &*this.cast::<RtvObject>() }.inner.resource;
    // SAFETY: queries `res` for `ID3D11Resource`.
    unsafe { query_parent_resource(res, out) };
}

unsafe extern "system-unwind" fn rtv_get_desc(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `RtvObject`.
    let obj = unsafe { &*this.cast::<RtvObject>() };
    let down = helper::downgrade_rtv_desc(&obj.desc);
    // SAFETY: `desc_out` is non-null and writable.
    unsafe { *desc_out.cast::<D3d11RenderTargetViewDesc>() = down };
}

unsafe extern "system-unwind" fn rtv_get_desc1(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `RtvObject`.
    let obj = unsafe { &*this.cast::<RtvObject>() };
    // SAFETY: `desc_out` is non-null and writable.
    unsafe { *desc_out.cast::<D3d11RenderTargetViewDesc1>() = obj.desc };
}

static RTV_VTBL: ID3D11RenderTargetView1Vtbl = ID3D11RenderTargetView1Vtbl {
    query_interface: rtv_query_interface,
    add_ref: rtv_add_ref,
    release: rtv_release,
    get_device: rtv_get_device,
    get_private_data: rtv_get_private_data,
    set_private_data: rtv_set_private_data,
    set_private_data_interface: rtv_set_private_data_interface,
    get_resource: rtv_get_resource,
    get_desc: rtv_get_desc,
    get_desc1: rtv_get_desc1,
};

// --- `DsvObject` vtable ---

unsafe extern "system-unwind" fn dsv_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    // SAFETY: `riid` addresses a GUID.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    // SAFETY: `this` is a live `DsvObject`.
    let obj = unsafe { &*this.cast::<DsvObject>() };
    if iid == IID_IUNKNOWN
        || iid == IID_ID3D11_DEVICE_CHILD
        || iid == IID_ID3D11_VIEW
        || iid == IID_ID3D11_DEPTH_STENCIL_VIEW
    {
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe { *out = this };
        return S_OK;
    }
    if iid == IID_ID3D10_DEVICE_CHILD
        || iid == IID_ID3D10_VIEW
        || iid == IID_ID3D10_DEPTH_STENCIL_VIEW
    {
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe { *out = core::ptr::from_ref(&obj.d3d10).cast_mut().cast() };
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn dsv_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `DsvObject`.
    unsafe { &*this.cast::<DsvObject>() }.refs.add_ref()
}

unsafe extern "system-unwind" fn dsv_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `DsvObject`.
    let (rem, free) = unsafe { &*this.cast::<DsvObject>() }
        .refs
        .release_and_free();
    if free {
        // SAFETY: last reference was just released.
        drop(unsafe { Box::from_raw(this.cast::<DsvObject>()) });
    }
    rem
}

unsafe extern "system-unwind" fn dsv_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `DsvObject`.
    let dev = unsafe { &*this.cast::<DsvObject>() }.device;
    // SAFETY: `dev` is a live COM object or null.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null.
    unsafe { *out = dev };
}

unsafe extern "system-unwind" fn dsv_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `DsvObject`.
    let obj = unsafe { &*this.cast::<DsvObject>() };
    // SAFETY: forwards caller buffers to `PrivateData::get_data`.
    unsafe { obj.private_data.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn dsv_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `DsvObject`.
    let obj = unsafe { &*this.cast::<DsvObject>() };
    // SAFETY: forwards caller buffer to `PrivateData::set_data`.
    unsafe { obj.private_data.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn dsv_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    iface: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `DsvObject`.
    let obj = unsafe { &*this.cast::<DsvObject>() };
    // SAFETY: forwards `iface` to `PrivateData::set_interface`.
    unsafe { obj.private_data.set_interface(g, iface.cast_mut()) }
}

unsafe extern "system-unwind" fn dsv_get_resource(this: *mut c_void, out: *mut *mut c_void) {
    // SAFETY: `this` is a live `DsvObject`.
    let res = unsafe { &*this.cast::<DsvObject>() }.inner.resource;
    // SAFETY: queries `res` for `ID3D11Resource`.
    unsafe { query_parent_resource(res, out) };
}

unsafe extern "system-unwind" fn dsv_get_desc(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `DsvObject`.
    let obj = unsafe { &*this.cast::<DsvObject>() };
    // SAFETY: `desc_out` is non-null and writable.
    unsafe { *desc_out.cast::<D3d11DepthStencilViewDesc>() = obj.desc };
}

static DSV_VTBL: ID3D11DepthStencilViewVtbl = ID3D11DepthStencilViewVtbl {
    query_interface: dsv_query_interface,
    add_ref: dsv_add_ref,
    release: dsv_release,
    get_device: dsv_get_device,
    get_private_data: dsv_get_private_data,
    set_private_data: dsv_set_private_data,
    set_private_data_interface: dsv_set_private_data_interface,
    get_resource: dsv_get_resource,
    get_desc: dsv_get_desc,
};

// --- `UavObject` vtable ---

unsafe extern "system-unwind" fn uav_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    // SAFETY: `riid` addresses a GUID.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    // SAFETY: `this` is a live `UavObject`.
    let obj = unsafe { &*this.cast::<UavObject>() };
    if iid == IID_IUNKNOWN
        || iid == IID_ID3D11_DEVICE_CHILD
        || iid == IID_ID3D11_VIEW
        || iid == IID_ID3D11_UNORDERED_ACCESS_VIEW
        || iid == IID_ID3D11_UNORDERED_ACCESS_VIEW_1
    {
        obj.refs.add_ref();
        // SAFETY: `out` is non-null.
        unsafe { *out = this };
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn uav_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `UavObject`.
    unsafe { &*this.cast::<UavObject>() }.refs.add_ref()
}

unsafe extern "system-unwind" fn uav_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `UavObject`.
    let (rem, free) = unsafe { &*this.cast::<UavObject>() }
        .refs
        .release_and_free();
    if free {
        // SAFETY: last reference was just released.
        drop(unsafe { Box::from_raw(this.cast::<UavObject>()) });
    }
    rem
}

unsafe extern "system-unwind" fn uav_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `UavObject`.
    let dev = unsafe { &*this.cast::<UavObject>() }.device;
    // SAFETY: `dev` is a live COM object or null.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null.
    unsafe { *out = dev };
}

unsafe extern "system-unwind" fn uav_get_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `UavObject`.
    let obj = unsafe { &*this.cast::<UavObject>() };
    // SAFETY: forwards caller buffers to `PrivateData::get_data`.
    unsafe { obj.private_data.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn uav_set_private_data(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `UavObject`.
    let obj = unsafe { &*this.cast::<UavObject>() };
    // SAFETY: forwards caller buffer to `PrivateData::set_data`.
    unsafe { obj.private_data.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn uav_set_private_data_interface(
    this: *mut c_void,
    guid: *const c_void,
    iface: *const c_void,
) -> Hresult {
    // SAFETY: `guid` addresses a GUID.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    // SAFETY: `this` is a live `UavObject`.
    let obj = unsafe { &*this.cast::<UavObject>() };
    // SAFETY: forwards `iface` to `PrivateData::set_interface`.
    unsafe { obj.private_data.set_interface(g, iface.cast_mut()) }
}

unsafe extern "system-unwind" fn uav_get_resource(this: *mut c_void, out: *mut *mut c_void) {
    // SAFETY: `this` is a live `UavObject`.
    let res = unsafe { &*this.cast::<UavObject>() }.inner.resource;
    // SAFETY: queries `res` for `ID3D11Resource`.
    unsafe { query_parent_resource(res, out) };
}

unsafe extern "system-unwind" fn uav_get_desc(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `UavObject`.
    let obj = unsafe { &*this.cast::<UavObject>() };
    let down = helper::downgrade_uav_desc(&obj.desc);
    // SAFETY: `desc_out` is non-null and writable.
    unsafe { *desc_out.cast::<D3d11UnorderedAccessViewDesc>() = down };
}

unsafe extern "system-unwind" fn uav_get_desc1(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `UavObject`.
    let obj = unsafe { &*this.cast::<UavObject>() };
    // SAFETY: `desc_out` is non-null and writable.
    unsafe { *desc_out.cast::<D3d11UnorderedAccessViewDesc1>() = obj.desc };
}

static UAV_VTBL: ID3D11UnorderedAccessView1Vtbl = ID3D11UnorderedAccessView1Vtbl {
    query_interface: uav_query_interface,
    add_ref: uav_add_ref,
    release: uav_release,
    get_device: uav_get_device,
    get_private_data: uav_get_private_data,
    set_private_data: uav_set_private_data,
    set_private_data_interface: uav_set_private_data_interface,
    get_resource: uav_get_resource,
    get_desc: uav_get_desc,
    get_desc1: uav_get_desc1,
};

// --- Device view creation entry points ---

/// `ID3D11Device::CreateShaderResourceView`.
pub fn create_srv(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_srv_inner(device, resource, desc, out)
}

fn create_srv_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return create_srv1(device, resource, core::ptr::null(), out);
    }
    // SAFETY: `desc` was checked non-null.
    let d1 = helper::upgrade_srv_desc(unsafe { &*desc });
    create_srv1(device, resource, &raw const d1, out)
}

/// `ID3D11Device3::CreateShaderResourceView1`.
pub fn create_srv1(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_srv1_inner(device, resource, desc, out)
}

fn create_srv1_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = core::ptr::null_mut() };
    }
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(resource) };
    match (&res.kind, &res.desc) {
        (ResourceKind::DefaultBuffer, ResourceDesc::Buffer(_)) => {
            resource::buffer::create_buffer_srv(device, resource, desc, out)
        }
        (ResourceKind::DynamicLinearTexture, ResourceDesc::Texture1d(_)) => {
            resource::linear::create_linear_texture_srv_1d(device, resource, desc, out)
        }
        (ResourceKind::DynamicLinearTexture, ResourceDesc::Texture2d(_)) => {
            resource::linear::create_linear_texture_srv_2d(device, resource, desc, out)
        }
        (
            ResourceKind::DeviceTexture | ResourceKind::DynamicTexture,
            ResourceDesc::Texture1d(_),
        ) => resource::texture::create_texture_srv_1d(device, resource, desc, out),
        (
            ResourceKind::DeviceTexture | ResourceKind::DynamicTexture,
            ResourceDesc::Texture2d(_),
        ) => resource::texture::create_texture_srv_2d(device, resource, desc, out),
        (
            ResourceKind::DeviceTexture | ResourceKind::DynamicTexture,
            ResourceDesc::Texture3d(_),
        ) => resource::texture::create_texture_srv_3d(device, resource, desc, out),
        _ => E_INVALIDARG,
    }
}

/// `ID3D11Device::CreateRenderTargetView`.
pub fn create_rtv(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_rtv_inner(device, resource, desc, out)
}

fn create_rtv_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return create_rtv1(device, resource, core::ptr::null(), out);
    }
    // SAFETY: `desc` was checked non-null.
    let d1 = helper::upgrade_rtv_desc(unsafe { &*desc });
    create_rtv1(device, resource, &raw const d1, out)
}

/// `ID3D11Device3::CreateRenderTargetView1`.
pub fn create_rtv1(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_rtv1_inner(device, resource, desc, out)
}

fn create_rtv1_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = core::ptr::null_mut() };
    }
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(resource) };
    match (&res.kind, &res.desc) {
        (ResourceKind::DefaultBuffer, ResourceDesc::Buffer(_)) => E_NOTIMPL,
        (ResourceKind::DeviceTexture, ResourceDesc::Texture1d(_)) => {
            resource::texture::create_texture_rtv_1d(device, resource, desc, out)
        }
        (ResourceKind::DeviceTexture, ResourceDesc::Texture2d(_)) => {
            resource::texture::create_texture_rtv_2d(device, resource, desc, out)
        }
        (ResourceKind::DeviceTexture, ResourceDesc::Texture3d(_)) => {
            resource::texture::create_texture_rtv_3d(device, resource, desc, out)
        }
        _ => E_INVALIDARG,
    }
}

/// `ID3D11Device::CreateDepthStencilView`.
pub fn create_dsv(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_dsv_inner(device, resource, desc, out)
}

fn create_dsv_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = core::ptr::null_mut() };
    }
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(resource) };
    match (&res.kind, &res.desc) {
        (ResourceKind::DeviceTexture, ResourceDesc::Texture1d(_)) => {
            resource::texture::create_texture_dsv_1d(device, resource, desc, out)
        }
        (ResourceKind::DeviceTexture, ResourceDesc::Texture2d(_)) => {
            resource::texture::create_texture_dsv_2d(device, resource, desc, out)
        }
        (ResourceKind::DeviceTexture, ResourceDesc::Texture3d(_)) => {
            resource::texture::create_texture_dsv_3d(device, resource, desc, out)
        }
        _ => E_INVALIDARG,
    }
}

/// `ID3D11Device::CreateUnorderedAccessView`.
pub fn create_uav(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_uav_inner(device, resource, desc, out)
}

fn create_uav_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return create_uav1(device, resource, core::ptr::null(), out);
    }
    // SAFETY: `desc` was checked non-null.
    let d1 = helper::upgrade_uav_desc(unsafe { &*desc });
    create_uav1(device, resource, &raw const d1, out)
}

/// `ID3D11Device3::CreateUnorderedAccessView1`.
pub fn create_uav1(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_uav1_inner(device, resource, desc, out)
}

fn create_uav1_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = core::ptr::null_mut() };
    }
    if resource.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `resource` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(resource) };
    match (&res.kind, &res.desc) {
        (ResourceKind::DefaultBuffer, ResourceDesc::Buffer(_)) => {
            resource::buffer::create_buffer_uav(device, resource, desc, out)
        }
        (ResourceKind::DeviceTexture, ResourceDesc::Texture1d(_)) => {
            resource::texture::create_texture_uav_1d(device, resource, desc, out)
        }
        (ResourceKind::DeviceTexture, ResourceDesc::Texture2d(_)) => {
            resource::texture::create_texture_uav_2d(device, resource, desc, out)
        }
        (ResourceKind::DeviceTexture, ResourceDesc::Texture3d(_)) => {
            resource::texture::create_texture_uav_3d(device, resource, desc, out)
        }
        _ => E_INVALIDARG,
    }
}

#[cfg(test)]
mod tests;
