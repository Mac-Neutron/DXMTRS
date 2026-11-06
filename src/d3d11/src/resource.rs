//! D3D11 resources (`d3d11_resource.hpp`, `d3d11_buffer.cpp`, `d3d11_texture*.cpp`).

use core::ffi::c_void;
use std::sync::{Arc, Mutex};

use dxmt_core::{
    dynamic::{DynamicBuffer, DynamicLinearTexture},
    resource::{Buffer, RenamableTexturePool, Texture},
    staging::Staging,
};
use dxmt_util::{
    com::{Qi, QiMiss, hresult::DXGI_ERROR_INVALID_CALL, qi_table},
    dxgi_consts::{
        D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_RENDER_TARGET,
        D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_STREAM_OUTPUT, D3D11_BIND_UNORDERED_ACCESS,
        D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, D3D11_USAGE_IMMUTABLE, D3D11_USAGE_STAGING,
        DXGI_RESOURCE_PRIORITY_NORMAL, DXGI_USAGE_RENDER_TARGET_OUTPUT, DXGI_USAGE_SHADER_INPUT,
        DXGI_USAGE_UNORDERED_ACCESS,
    },
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
};

use crate::{
    abi::{
        IID_ID3D10_BUFFER, IID_ID3D10_DEVICE_CHILD, IID_ID3D10_RESOURCE, IID_ID3D10_TEXTURE_1D,
        IID_ID3D10_TEXTURE_2D, IID_ID3D10_TEXTURE_3D, IID_ID3D11_BUFFER, IID_ID3D11_DEVICE_CHILD,
        IID_ID3D11_RESOURCE, IID_ID3D11_TEXTURE_1D, IID_ID3D11_TEXTURE_2D, IID_ID3D11_TEXTURE_2D1,
        IID_ID3D11_TEXTURE_3D, IID_ID3D11_TEXTURE_3D1, IID_IDXGI_DEVICE_SUB_OBJECT,
        IID_IDXGI_KEYED_MUTEX, IID_IDXGI_OBJECT, IID_IDXGI_RESOURCE, IID_IDXGI_RESOURCE_1,
        IID_IDXGI_SURFACE, IID_IDXGI_SURFACE_1, IID_IDXGI_SURFACE_2, IID_IUNKNOWN,
        ext::{IID_IMTL_MIN_LOD_CLAMPABLE, IMTLMinLODClampableVtbl},
        records::{
            D3d11BufferDesc, D3d11SubresourceData, D3d11Texture1dDesc, D3d11Texture2dDesc,
            D3d11Texture2dDesc1, D3d11Texture3dDesc, D3d11Texture3dDesc1,
        },
        vtbl::{
            ID3D10BufferVtbl, ID3D10Texture1DVtbl, ID3D10Texture2DVtbl, ID3D10Texture3DVtbl,
            ID3D11BufferVtbl, ID3D11Texture1DVtbl, ID3D11Texture2D1Vtbl, ID3D11Texture3D1Vtbl,
            IDXGIKeyedMutexVtbl, IDXGIResource1Vtbl, IDXGISurface2Vtbl, IUnknownVtbl,
        },
    },
    com::{
        ComObjectWithInitialRef, E_INVALIDARG, E_NOINTERFACE, Guid, Hresult, S_FALSE, S_OK,
        TearOff, add_ref, container_of, iface_of, vtbl,
    },
    d3d10,
    device::Device,
    keyed_mutex::{
        D3D11_RESOURCE_MISC_SHARED, D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX,
        D3D11_RESOURCE_MISC_SHARED_NTHANDLE, KeyedMutexRef,
    },
    surface::SURFACE_VTBL,
};

pub mod buffer;
pub mod dynamic;
pub mod helper;
pub mod linear;
pub mod staging;
pub mod texture;

const D3D11_RESOURCE_DIMENSION_BUFFER: u32 = 1;
const D3D11_RESOURCE_DIMENSION_TEXTURE1D: u32 = 2;
const D3D11_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;
const D3D11_RESOURCE_DIMENSION_TEXTURE3D: u32 = 4;

const D3D11_RESOURCE_MISC_GENERATE_MIPS: u32 = 0x1;

/// Shared resource fields accessed across lanes (`MTLResourceCommon`).
pub struct Common {
    pub device: *mut c_void,
    pub buffer: Option<Arc<Buffer>>,
    pub texture: Option<Arc<Texture>>,
    pub dynamic_buffer: Option<Arc<DynamicBuffer>>,
    pub dynamic_texture: Option<Arc<DynamicLinearTexture>>,
    pub staging: Option<Arc<Staging>>,
    pub bind_flags: u32,
    pub usage: u32,
    pub misc_flags: u32,
    pub keyed_mutex: Option<KeyedMutexRef>,
    pub min_lod: f32,
}

impl Default for Common {
    fn default() -> Self {
        Self {
            device: core::ptr::null_mut(),
            buffer: None,
            texture: None,
            dynamic_buffer: None,
            dynamic_texture: None,
            staging: None,
            bind_flags: 0,
            usage: 0,
            misc_flags: 0,
            keyed_mutex: None,
            min_lod: 0.0,
        }
    }
}

impl Common {
    /// Returns the owning `ID3D11Device*` pointer.
    #[must_use]
    pub const fn device(&self) -> *mut c_void {
        self.device
    }

    /// Returns the attached [`KeyedMutexRef`], if any.
    #[must_use]
    pub const fn keyed_mutex(&self) -> Option<&KeyedMutexRef> {
        self.keyed_mutex.as_ref()
    }

    /// Whether the resource has no output bind flags (`D3D11ResourceCommon::hazardsFree`).
    #[must_use]
    pub const fn hazards_free(&self) -> bool {
        (self.bind_flags
            & (D3D11_BIND_STREAM_OUTPUT
                | D3D11_BIND_UNORDERED_ACCESS
                | D3D11_BIND_RENDER_TARGET
                | D3D11_BIND_DEPTH_STENCIL))
            == 0
    }
}

/// Storage class of a [`Resource`].
#[derive(PartialEq, Eq)]
pub enum ResourceKind {
    DefaultBuffer,
    StagingBuffer,
    StagingTexture,
    DynamicLinearTexture,
    DynamicTexture,
    DeviceTexture,
}

/// Dimension-tagged D3D11 resource description stored on [`Resource`].
pub enum ResourceDesc {
    Buffer(D3d11BufferDesc),
    Texture1d(D3d11Texture1dDesc),
    Texture2d(D3d11Texture2dDesc1),
    Texture3d(D3d11Texture3dDesc1),
}

impl ResourceDesc {
    /// Returns the `D3D11_RESOURCE_DIMENSION` discriminant for this resource.
    #[must_use]
    pub const fn dimension(&self) -> u32 {
        match self {
            Self::Buffer(_) => D3D11_RESOURCE_DIMENSION_BUFFER,
            Self::Texture1d(_) => D3D11_RESOURCE_DIMENSION_TEXTURE1D,
            Self::Texture2d(_) => D3D11_RESOURCE_DIMENSION_TEXTURE2D,
            Self::Texture3d(_) => D3D11_RESOURCE_DIMENSION_TEXTURE3D,
        }
    }
}

/// One subresource of a dynamic non-linear texture (`d3d11_texture_dynamic.cpp`).
pub struct DynamicTexSubresource {
    pub dynamic: Arc<DynamicBuffer>,
    pub bytes_per_row: u32,
    pub bytes_per_image: u32,
}

/// Unified D3D11 resource COM object (`TResourceBase` in `d3d11_resource.hpp`).
#[repr(C)]
pub struct Resource {
    pub vtbl: *const c_void,
    pub common: Common,
    pub refs: ComObjectWithInitialRef,
    pub private_data: PrivateData,
    pub kind: ResourceKind,
    pub desc: ResourceDesc,
    pub staging_subresources: Vec<Arc<Staging>>,
    pub dynamic_subresources: Vec<DynamicTexSubresource>,
    pub linear_bytes_per_row: u32,
    pub linear_bytes_per_image: u32,
    pub renamable: Option<Arc<Mutex<RenamableTexturePool>>>,
    pub local_kmt: u32,
    pub global_kmt: u32,
    pub dxgi_resource: TearOff<IDXGIResource1Vtbl>,
    pub keyed_mutex_tearoff: TearOff<IDXGIKeyedMutexVtbl>,
    pub dxgi_surface: TearOff<IDXGISurface2Vtbl>,
    pub min_lod_vtbl: *const IMTLMinLODClampableVtbl,
    pub d3d10_buffer: d3d10::Wrapper<ID3D10BufferVtbl>,
    pub d3d10_tex1d: d3d10::Wrapper<ID3D10Texture1DVtbl>,
    pub d3d10_tex2d: d3d10::Wrapper<ID3D10Texture2DVtbl>,
    pub d3d10_tex3d: d3d10::Wrapper<ID3D10Texture3DVtbl>,
}

impl Drop for Resource {
    fn drop(&mut self) {
        if self.local_kmt != 0 {
            let h_device = crate::kmt::device_local_kmt(self.common.device);
            crate::kmt::destroy_allocation(h_device, self.local_kmt);
        }
    }
}

impl Resource {
    /// Allocates a heap [`Resource`] with 1 initial reference and wired tear-offs.
    #[must_use]
    pub fn into_raw(common: Common, kind: ResourceKind, desc: ResourceDesc) -> *mut Self {
        let primary_vtbl: *const c_void = match desc {
            ResourceDesc::Buffer(_) => core::ptr::from_ref(&BUFFER_VTBL).cast(),
            ResourceDesc::Texture1d(_) => core::ptr::from_ref(&TEXTURE1D_VTBL).cast(),
            ResourceDesc::Texture2d(_) => core::ptr::from_ref(&TEXTURE2D_VTBL).cast(),
            ResourceDesc::Texture3d(_) => core::ptr::from_ref(&TEXTURE3D_VTBL).cast(),
        };
        let imm_ctx = Device::from_iface(common.device)
            .map_or(core::ptr::null_mut(), |d| d.immediate().cast::<c_void>());
        let null = core::ptr::null_mut();
        let boxed = Box::new(Self {
            vtbl: primary_vtbl,
            common,
            refs: ComObjectWithInitialRef::new(),
            private_data: PrivateData::default(),
            kind,
            desc,
            staging_subresources: Vec::new(),
            dynamic_subresources: Vec::new(),
            linear_bytes_per_row: 0,
            linear_bytes_per_image: 0,
            renamable: None,
            local_kmt: 0,
            global_kmt: 0,
            dxgi_resource: TearOff::new(&DXGI_RESOURCE_VTBL, null),
            keyed_mutex_tearoff: TearOff::new(&KEYED_MUTEX_VTBL, null),
            dxgi_surface: TearOff::new(&SURFACE_VTBL, null),
            min_lod_vtbl: &raw const MIN_LOD_VTBL,
            d3d10_buffer: d3d10::Wrapper::new(&d3d10::BUFFER_VTBL, null, imm_ctx),
            d3d10_tex1d: d3d10::Wrapper::new(&d3d10::TEXTURE1D_VTBL, null, imm_ctx),
            d3d10_tex2d: d3d10::Wrapper::new(&d3d10::TEXTURE2D_VTBL, null, imm_ctx),
            d3d10_tex3d: d3d10::Wrapper::new(&d3d10::TEXTURE3D_VTBL, null, imm_ctx),
        });
        let raw = Box::into_raw(boxed);
        let iface = raw.cast::<c_void>();
        // SAFETY: `raw` was just allocated by `Box::into_raw` and is uniquely owned here.
        let res = unsafe { &mut *raw };
        res.dxgi_resource.outer = iface;
        res.keyed_mutex_tearoff.outer = iface;
        res.dxgi_surface.outer = iface;
        res.d3d10_buffer.d3d11 = iface;
        res.d3d10_tex1d.d3d11 = iface;
        res.d3d10_tex2d.d3d11 = iface;
        res.d3d10_tex3d.d3d11 = iface;
        raw
    }

    /// Recovers `&Resource` from a primary `ID3D11Resource*` pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be a live primary `Resource` COM pointer.
    #[must_use]
    pub const unsafe fn from_ptr(ptr: *mut c_void) -> &'static Self {
        // SAFETY: the caller guarantees `ptr` points to a live `Resource`.
        unsafe { &*ptr.cast::<Self>() }
    }

    /// Recovers `&mut Resource` from a primary `ID3D11Resource*` pointer.
    ///
    /// # Safety
    ///
    /// `ptr` must be a live primary `Resource` COM pointer with exclusive access.
    #[must_use]
    pub const unsafe fn from_ptr_mut(ptr: *mut c_void) -> &'static mut Self {
        // SAFETY: the caller guarantees `ptr` points to a live `Resource`.
        unsafe { &mut *ptr.cast::<Self>() }
    }

    /// Increments the private reference count on `resource` (used by resource views).
    pub fn add_ref_private(resource: *mut c_void) {
        add_ref_private_inner(resource);
    }

    /// Decrements the private reference count on `resource`, freeing it at 0.
    pub fn release_private(resource: *mut c_void) {
        release_private_inner(resource);
    }
}

fn is_resource_vtbl(vtbl: *const c_void) -> bool {
    core::ptr::eq(vtbl, (&raw const BUFFER_VTBL).cast())
        || core::ptr::eq(vtbl, (&raw const TEXTURE1D_VTBL).cast())
        || core::ptr::eq(vtbl, (&raw const TEXTURE2D_VTBL).cast())
        || core::ptr::eq(vtbl, (&raw const TEXTURE3D_VTBL).cast())
}

fn add_ref_private_inner(resource: *mut c_void) {
    if let Some(res) = resource_from_iface(resource) {
        res.refs.add_ref_private();
    }
}

fn release_private_inner(resource: *mut c_void) {
    if let Some(res) = resource_from_iface(resource) {
        let free = res.refs.release_private();
        if free {
            // SAFETY: the last private reference was just released.
            drop(unsafe { Box::from_raw(resource.cast::<Resource>()) });
        }
    }
}

fn resource_from_iface(resource: *mut c_void) -> Option<&'static Resource> {
    if resource.is_null() || !resource.cast::<*const c_void>().is_aligned() {
        return None;
    }
    // SAFETY: `resource` is a non-null, pointer-aligned COM interface pointer.
    let vtbl = unsafe { *resource.cast::<*const c_void>() };
    if !is_resource_vtbl(vtbl) {
        return None;
    }
    // SAFETY: `vtbl` matches one of `Resource`'s static primary vtables.
    Some(unsafe { Resource::from_ptr(resource) })
}

/// Recovers `&Common` from an `ID3D11Resource*` pointer (`GetCommonResource`).
#[must_use]
pub fn common(resource: *mut c_void) -> Option<&'static Common> {
    Some(&resource_from_iface(resource)?.common)
}

/// Returns the [`DynamicBuffer`] for `resource` (`GetDynamicBuffer`).
#[must_use]
pub fn dynamic_buffer(resource: *mut c_void) -> Option<&'static Arc<DynamicBuffer>> {
    common(resource)?.dynamic_buffer.as_ref()
}

/// Returns the [`DynamicLinearTexture`] for `resource` (`GetDynamicLinearTexture`).
#[must_use]
pub fn dynamic_texture(resource: *mut c_void) -> Option<&'static Arc<DynamicLinearTexture>> {
    common(resource)?.dynamic_texture.as_ref()
}

/// Returns `(bytes_per_row, bytes_per_image)` for a dynamic linear texture.
#[must_use]
pub fn dynamic_linear_texture_layout(resource: *mut c_void) -> Option<(u32, u32)> {
    let res = resource_from_iface(resource)?;
    if res.common.dynamic_texture.is_some() {
        Some((res.linear_bytes_per_row, res.linear_bytes_per_image))
    } else {
        None
    }
}

/// Returns `(dynamic_buffer, bytes_per_row, bytes_per_image)` for a dynamic texture subresource.
#[must_use]
pub fn dynamic_texture_subresource(
    resource: *mut c_void,
    subresource: u32,
) -> Option<(&'static Arc<DynamicBuffer>, u32, u32)> {
    let res = resource_from_iface(resource)?;
    let sub = res.dynamic_subresources.get(subresource as usize)?;
    Some((&sub.dynamic, sub.bytes_per_row, sub.bytes_per_image))
}

/// Returns the [`RenamableTexturePool`] for a renameable DSV resource, if any.
#[must_use]
pub fn renamable(resource: *mut c_void) -> Option<&'static Arc<Mutex<RenamableTexturePool>>> {
    resource_from_iface(resource)?.renamable.as_ref()
}

/// Returns the [`Staging`] buffer for `subresource` of `resource` (`GetStagingResource`).
#[must_use]
pub fn staging(resource: *mut c_void, subresource: u32) -> Option<&'static Arc<Staging>> {
    let res = resource_from_iface(resource)?;
    if !res.staging_subresources.is_empty() {
        return res.staging_subresources.get(subresource as usize);
    }
    if subresource == 0 {
        res.common.staging.as_ref()
    } else {
        None
    }
}

// --- Primary `ID3D11Buffer` / `ID3D11Texture{1,2,3}D1` vtable thunks ---

unsafe extern "system-unwind" fn res_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(this) };
    // The D3D11 interfaces, the D3D10 ones with their wrapper, the base IID and the miss line's name.
    let (d3d11, d3d10, d3d10_iface, base_iid, name): (&[Guid], &[Guid], *mut c_void, &Guid, &str) =
        match res.desc {
            ResourceDesc::Buffer(_) => (
                &[
                    IID_IUNKNOWN,
                    IID_ID3D11_DEVICE_CHILD,
                    IID_ID3D11_RESOURCE,
                    IID_ID3D11_BUFFER,
                ],
                &[
                    IID_ID3D10_DEVICE_CHILD,
                    IID_ID3D10_RESOURCE,
                    IID_ID3D10_BUFFER,
                ],
                core::ptr::from_ref(&res.d3d10_buffer).cast_mut().cast(),
                &IID_ID3D11_BUFFER,
                "D3D11Resource(buffer)",
            ),
            ResourceDesc::Texture1d(_) => (
                &[
                    IID_IUNKNOWN,
                    IID_ID3D11_DEVICE_CHILD,
                    IID_ID3D11_RESOURCE,
                    IID_ID3D11_TEXTURE_1D,
                ],
                &[
                    IID_ID3D10_DEVICE_CHILD,
                    IID_ID3D10_RESOURCE,
                    IID_ID3D10_TEXTURE_1D,
                ],
                core::ptr::from_ref(&res.d3d10_tex1d).cast_mut().cast(),
                &IID_ID3D11_TEXTURE_1D,
                "D3D11Resource(tex1d)",
            ),
            ResourceDesc::Texture2d(_) => (
                &[
                    IID_IUNKNOWN,
                    IID_ID3D11_DEVICE_CHILD,
                    IID_ID3D11_RESOURCE,
                    IID_ID3D11_TEXTURE_2D,
                    IID_ID3D11_TEXTURE_2D1,
                ],
                &[
                    IID_ID3D10_DEVICE_CHILD,
                    IID_ID3D10_RESOURCE,
                    IID_ID3D10_TEXTURE_2D,
                ],
                core::ptr::from_ref(&res.d3d10_tex2d).cast_mut().cast(),
                &IID_ID3D11_TEXTURE_2D,
                "D3D11Resource(tex2d)",
            ),
            ResourceDesc::Texture3d(_) => (
                &[
                    IID_IUNKNOWN,
                    IID_ID3D11_DEVICE_CHILD,
                    IID_ID3D11_RESOURCE,
                    IID_ID3D11_TEXTURE_3D,
                    IID_ID3D11_TEXTURE_3D1,
                ],
                &[
                    IID_ID3D10_DEVICE_CHILD,
                    IID_ID3D10_RESOURCE,
                    IID_ID3D10_TEXTURE_3D,
                ],
                core::ptr::from_ref(&res.d3d10_tex3d).cast_mut().cast(),
                &IID_ID3D11_TEXTURE_3D,
                "D3D11Resource(tex3d)",
            ),
        };
    let min_lod: &[Guid] = if res.kind == ResourceKind::DeviceTexture {
        &[IID_IMTL_MIN_LOD_CLAMPABLE]
    } else {
        &[]
    };
    // A surface answers for one subresource only.
    let single_subresource = match res.desc {
        ResourceDesc::Texture1d(d) => d.array_size.saturating_mul(d.mip_levels) == 1,
        ResourceDesc::Texture2d(d) => d.array_size.saturating_mul(d.mip_levels) == 1,
        ResourceDesc::Buffer(_) | ResourceDesc::Texture3d(_) => false,
    };
    let surface: &[Guid] = if single_subresource {
        &[IID_IDXGI_SURFACE, IID_IDXGI_SURFACE_1, IID_IDXGI_SURFACE_2]
    } else {
        &[]
    };
    let keyed = (res.common.misc_flags & D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX) != 0;
    let (keyed_mutex, silent): (&[Guid], &[Guid]) = if keyed {
        (&[IID_IDXGI_KEYED_MUTEX], &[])
    } else {
        (&[], &[IID_IDXGI_KEYED_MUTEX])
    };
    let qi = Qi {
        null_riid: E_NOINTERFACE,
        silent,
        log: &crate::LOG,
        miss: QiMiss::Once {
            object: base_iid,
            name,
        },
    };
    // SAFETY: the application's `riid` and `out`, null or valid.
    unsafe {
        qi_table(
            riid,
            out,
            &[
                (min_lod, iface_of(&res.min_lod_vtbl)),
                (d3d11, this),
                (d3d10, d3d10_iface),
                (
                    &[
                        IID_IDXGI_OBJECT,
                        IID_IDXGI_DEVICE_SUB_OBJECT,
                        IID_IDXGI_RESOURCE,
                        IID_IDXGI_RESOURCE_1,
                    ],
                    res.dxgi_resource.as_iface(),
                ),
                (surface, res.dxgi_surface.as_iface()),
                (keyed_mutex, res.keyed_mutex_tearoff.as_iface()),
            ],
            || {
                res.refs.add_ref();
            },
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn res_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Resource`.
    unsafe { Resource::from_ptr(this) }.refs.add_ref()
}

unsafe extern "system-unwind" fn res_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a live `Resource`.
    let (rem, free) = unsafe { Resource::from_ptr(this) }.refs.release_and_free();
    if free {
        // SAFETY: the last reference was just released.
        drop(unsafe { Box::from_raw(this.cast::<Resource>()) });
    }
    rem
}

unsafe extern "system-unwind" fn res_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Resource`.
    let dev = unsafe { Resource::from_ptr(this) }.common.device;
    // SAFETY: `dev` is a live `ID3D11Device` or null.
    unsafe { add_ref(dev) };
    // SAFETY: `out` was checked non-null.
    unsafe { *out = dev };
}

impl HasPrivateData for Resource {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `Resource`.
        &unsafe { Self::from_ptr(this) }.private_data
    }
}

unsafe extern "system-unwind" fn res_get_type(this: *mut c_void, out: *mut u32) {
    if out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Resource`.
    let dim = unsafe { Resource::from_ptr(this) }.desc.dimension();
    // SAFETY: `out` was checked non-null.
    unsafe { *out = dim };
}

const unsafe extern "system-unwind" fn res_set_eviction_priority(_this: *mut c_void, _prio: u32) {}

const unsafe extern "system-unwind" fn res_get_eviction_priority(_this: *mut c_void) -> u32 {
    DXGI_RESOURCE_PRIORITY_NORMAL
}

unsafe extern "system-unwind" fn res_get_desc(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(this) };
    match res.desc {
        ResourceDesc::Buffer(d) => {
            // SAFETY: caller passed a `D3D11_BUFFER_DESC*`.
            unsafe { *desc_out.cast::<D3d11BufferDesc>() = d };
        }
        ResourceDesc::Texture1d(d) => {
            // SAFETY: caller passed a `D3D11_TEXTURE1D_DESC*`.
            unsafe { *desc_out.cast::<D3d11Texture1dDesc>() = d };
        }
        ResourceDesc::Texture2d(d) => {
            let down = helper::downgrade_texture2d_desc(&d);
            // SAFETY: caller passed a `D3D11_TEXTURE2D_DESC*`.
            unsafe { *desc_out.cast::<D3d11Texture2dDesc>() = down };
        }
        ResourceDesc::Texture3d(d) => {
            let down = helper::downgrade_texture3d_desc(&d);
            // SAFETY: caller passed a `D3D11_TEXTURE3D_DESC*`.
            unsafe { *desc_out.cast::<D3d11Texture3dDesc>() = down };
        }
    }
}

unsafe extern "system-unwind" fn res_get_desc1(this: *mut c_void, desc_out: *mut c_void) {
    if desc_out.is_null() {
        return;
    }
    // SAFETY: `this` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(this) };
    match res.desc {
        ResourceDesc::Texture2d(d) => {
            // SAFETY: caller passed a `D3D11_TEXTURE2D_DESC1*`.
            unsafe { *desc_out.cast::<D3d11Texture2dDesc1>() = d };
        }
        ResourceDesc::Texture3d(d) => {
            // SAFETY: caller passed a `D3D11_TEXTURE3D_DESC1*`.
            unsafe { *desc_out.cast::<D3d11Texture3dDesc1>() = d };
        }
        ResourceDesc::Buffer(d) => {
            // SAFETY: caller passed a `D3D11_BUFFER_DESC*`.
            unsafe { *desc_out.cast::<D3d11BufferDesc>() = d };
        }
        ResourceDesc::Texture1d(d) => {
            // SAFETY: caller passed a `D3D11_TEXTURE1D_DESC*`.
            unsafe { *desc_out.cast::<D3d11Texture1dDesc>() = d };
        }
    }
}

static BUFFER_VTBL: ID3D11BufferVtbl = ID3D11BufferVtbl {
    query_interface: res_query_interface,
    add_ref: res_add_ref,
    release: res_release,
    get_device: res_get_device,
    get_private_data: get_private_data::<Resource>,
    set_private_data: set_private_data::<Resource>,
    set_private_data_interface: set_private_data_interface::<Resource>,
    get_type: res_get_type,
    set_eviction_priority: res_set_eviction_priority,
    get_eviction_priority: res_get_eviction_priority,
    get_desc: res_get_desc,
};

static TEXTURE1D_VTBL: ID3D11Texture1DVtbl = ID3D11Texture1DVtbl {
    query_interface: res_query_interface,
    add_ref: res_add_ref,
    release: res_release,
    get_device: res_get_device,
    get_private_data: get_private_data::<Resource>,
    set_private_data: set_private_data::<Resource>,
    set_private_data_interface: set_private_data_interface::<Resource>,
    get_type: res_get_type,
    set_eviction_priority: res_set_eviction_priority,
    get_eviction_priority: res_get_eviction_priority,
    get_desc: res_get_desc,
};

static TEXTURE2D_VTBL: ID3D11Texture2D1Vtbl = ID3D11Texture2D1Vtbl {
    query_interface: res_query_interface,
    add_ref: res_add_ref,
    release: res_release,
    get_device: res_get_device,
    get_private_data: get_private_data::<Resource>,
    set_private_data: set_private_data::<Resource>,
    set_private_data_interface: set_private_data_interface::<Resource>,
    get_type: res_get_type,
    set_eviction_priority: res_set_eviction_priority,
    get_eviction_priority: res_get_eviction_priority,
    get_desc: res_get_desc,
    get_desc1: res_get_desc1,
};

static TEXTURE3D_VTBL: ID3D11Texture3D1Vtbl = ID3D11Texture3D1Vtbl {
    query_interface: res_query_interface,
    add_ref: res_add_ref,
    release: res_release,
    get_device: res_get_device,
    get_private_data: get_private_data::<Resource>,
    set_private_data: set_private_data::<Resource>,
    set_private_data_interface: set_private_data_interface::<Resource>,
    get_type: res_get_type,
    set_eviction_priority: res_set_eviction_priority,
    get_eviction_priority: res_get_eviction_priority,
    get_desc: res_get_desc,
    get_desc1: res_get_desc1,
};

// --- `IMTLMinLODClampable` tear-off ---

unsafe extern "system-unwind" fn min_lod_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let res_ptr = container_of!(this, Resource, min_lod_vtbl);
    // SAFETY: `this` is `&Resource::min_lod_vtbl`.
    unsafe { res_query_interface(res_ptr.cast(), riid, out) }
}

unsafe extern "system-unwind" fn min_lod_add_ref(this: *mut c_void) -> u32 {
    let res_ptr = container_of!(this, Resource, min_lod_vtbl);
    // SAFETY: `this` is `&Resource::min_lod_vtbl`.
    unsafe { res_add_ref(res_ptr.cast()) }
}

unsafe extern "system-unwind" fn min_lod_release(this: *mut c_void) -> u32 {
    let res_ptr = container_of!(this, Resource, min_lod_vtbl);
    // SAFETY: `this` is `&Resource::min_lod_vtbl`.
    unsafe { res_release(res_ptr.cast()) }
}

unsafe extern "system-unwind" fn min_lod_set(this: *mut c_void, min_lod: f32) {
    let res_ptr = container_of!(this, Resource, min_lod_vtbl);
    // SAFETY: `this` is `&Resource::min_lod_vtbl`.
    unsafe { (*res_ptr).common.min_lod = min_lod };
}

unsafe extern "system-unwind" fn min_lod_get(this: *mut c_void) -> f32 {
    let res_ptr = container_of!(this, Resource, min_lod_vtbl);
    // SAFETY: `this` is `&Resource::min_lod_vtbl`.
    unsafe { (*res_ptr).common.min_lod }
}

static MIN_LOD_VTBL: IMTLMinLODClampableVtbl = IMTLMinLODClampableVtbl {
    unknown: IUnknownVtbl {
        query_interface: min_lod_query_interface,
        add_ref: min_lod_add_ref,
        release: min_lod_release,
    },
    set_min_lod: min_lod_set,
    get_min_lod: min_lod_get,
};

// --- `IDXGIResource1` tear-off ---

/// A [`Resource`]'s tear-off (DXGI resource, surface, keyed mutex), for its private data.
pub struct ResourceTearOff;

impl HasPrivateData for ResourceTearOff {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a `TearOff` whose `outer` is a live `Resource`.
        let outer = unsafe { (*this.cast::<TearOff<()>>()).outer };
        // SAFETY: `outer` is a live `Resource`.
        &unsafe { Resource::from_ptr(outer) }.private_data
    }
}

unsafe extern "system-unwind" fn dxgi_res_get_device(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is `Resource::dxgi_resource`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGIResource1Vtbl>>()).outer };
    // SAFETY: `outer` is a live `Resource`.
    let dev = unsafe { Resource::from_ptr(outer) }.common.device;
    if dev.is_null() {
        return E_NOINTERFACE;
    }
    // SAFETY: `dev` is a live COM object.
    let unk = unsafe { vtbl::<IUnknownVtbl>(dev) };
    // SAFETY: queries `dev` for `riid`.
    unsafe { (unk.query_interface)(dev, riid, out) }
}

unsafe extern "system-unwind" fn dxgi_res_get_shared_handle(
    this: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is `Resource::dxgi_resource`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGIResource1Vtbl>>()).outer };
    // SAFETY: `outer` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(outer) };
    if res.kind != ResourceKind::DeviceTexture {
        return E_INVALIDARG;
    }
    if out.is_null() || (res.common.misc_flags & D3D11_RESOURCE_MISC_SHARED_NTHANDLE) != 0 {
        return E_INVALIDARG;
    }
    if (res.common.misc_flags
        & (D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX))
        == 0
    {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = core::ptr::null_mut() };
        return S_OK;
    }
    if res.global_kmt == 0 {
        return E_INVALIDARG;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = (res.global_kmt as usize) as *mut c_void };
    S_OK
}

unsafe extern "system-unwind" fn dxgi_res_get_usage(this: *mut c_void, out: *mut u32) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is `Resource::dxgi_resource`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGIResource1Vtbl>>()).outer };
    // SAFETY: `outer` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(outer) };
    let mut usage = 0u32;
    if res.kind == ResourceKind::DefaultBuffer {
        if (res.common.bind_flags & D3D11_BIND_RENDER_TARGET) != 0 {
            usage |= DXGI_USAGE_RENDER_TARGET_OUTPUT;
        }
        if (res.common.bind_flags & D3D11_BIND_UNORDERED_ACCESS) != 0 {
            usage |= DXGI_USAGE_UNORDERED_ACCESS;
        }
        if (res.common.bind_flags & (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_CONSTANT_BUFFER)) != 0
        {
            usage |= DXGI_USAGE_SHADER_INPUT;
        }
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = usage };
    S_OK
}

const unsafe extern "system-unwind" fn dxgi_res_set_eviction_priority(
    _this: *mut c_void,
    _prio: u32,
) -> Hresult {
    S_OK
}

unsafe extern "system-unwind" fn dxgi_res_get_eviction_priority(
    _this: *mut c_void,
    out: *mut u32,
) -> Hresult {
    if out.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = DXGI_RESOURCE_PRIORITY_NORMAL };
    S_OK
}

unsafe extern "system-unwind" fn dxgi_res_create_subresource_surface(
    this: *mut c_void,
    index: u32,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `this` is `Resource::dxgi_resource`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGIResource1Vtbl>>()).outer };
    // SAFETY: `outer` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(outer) };
    match res.kind {
        ResourceKind::StagingBuffer | ResourceKind::DynamicLinearTexture => {
            if index != 0 {
                return E_INVALIDARG;
            }
            crate::surface::create_subresource_surface(outer, index, out)
        }
        ResourceKind::StagingTexture => {
            if let ResourceDesc::Texture3d(d) = res.desc
                && d.depth > 1
            {
                return E_INVALIDARG;
            }
            if (index as usize) >= res.staging_subresources.len() {
                return E_INVALIDARG;
            }
            crate::surface::create_subresource_surface(outer, index, out)
        }
        ResourceKind::DynamicTexture => {
            if let ResourceDesc::Texture3d(d) = res.desc
                && d.depth > 1
            {
                return E_INVALIDARG;
            }
            if (index as usize) >= res.dynamic_subresources.len() {
                return E_INVALIDARG;
            }
            crate::surface::create_subresource_surface(outer, index, out)
        }
        ResourceKind::DefaultBuffer | ResourceKind::DeviceTexture => DXGI_ERROR_INVALID_CALL,
    }
}

unsafe extern "system-unwind" fn dxgi_res_create_shared_handle(
    this: *mut c_void,
    attributes: *const c_void,
    access: u32,
    name: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = core::ptr::null_mut() };
    }
    // SAFETY: `this` is `Resource::dxgi_resource`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGIResource1Vtbl>>()).outer };
    // SAFETY: `outer` is a live `Resource`.
    let res = unsafe { Resource::from_ptr(outer) };
    if res.kind != ResourceKind::DeviceTexture {
        return E_INVALIDARG;
    }
    crate::kmt::share_object(
        res.local_kmt,
        attributes.cast::<crate::abi::records::SecurityAttributes>(),
        access,
        name.cast::<u16>(),
        out,
    )
}

static DXGI_RESOURCE_VTBL: IDXGIResource1Vtbl = IDXGIResource1Vtbl {
    query_interface: TearOff::query_interface,
    add_ref: TearOff::add_ref,
    release: TearOff::release,
    set_private_data: set_private_data::<ResourceTearOff>,
    set_private_data_interface: set_private_data_interface::<ResourceTearOff>,
    get_private_data: get_private_data::<ResourceTearOff>,
    get_parent: dxgi_res_get_device,
    get_device: dxgi_res_get_device,
    get_shared_handle: dxgi_res_get_shared_handle,
    get_usage: dxgi_res_get_usage,
    set_eviction_priority: dxgi_res_set_eviction_priority,
    get_eviction_priority: dxgi_res_get_eviction_priority,
    create_subresource_surface: dxgi_res_create_subresource_surface,
    create_shared_handle: dxgi_res_create_shared_handle,
};

// --- `IDXGIKeyedMutex` tear-off ---

unsafe extern "system-unwind" fn km_acquire_sync(this: *mut c_void, key: u64, ms: u32) -> Hresult {
    // SAFETY: `this` is `Resource::keyed_mutex_tearoff`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGIKeyedMutexVtbl>>()).outer };
    // SAFETY: `outer` is `Resource::primary_iface`.
    let res = unsafe { Resource::from_ptr(outer) };
    crate::keyed_mutex::acquire_sync_for_resource(&res.common, key, ms)
}

unsafe extern "system-unwind" fn km_release_sync(this: *mut c_void, key: u64) -> Hresult {
    // SAFETY: `this` is `Resource::keyed_mutex_tearoff`.
    let outer = unsafe { (*this.cast::<TearOff<IDXGIKeyedMutexVtbl>>()).outer };
    // SAFETY: `outer` is `Resource::primary_iface`.
    let res = unsafe { Resource::from_ptr(outer) };
    crate::keyed_mutex::release_sync_for_resource(&res.common, key)
}

static KEYED_MUTEX_VTBL: IDXGIKeyedMutexVtbl = IDXGIKeyedMutexVtbl {
    query_interface: TearOff::query_interface,
    add_ref: TearOff::add_ref,
    release: TearOff::release,
    set_private_data: set_private_data::<ResourceTearOff>,
    set_private_data_interface: set_private_data_interface::<ResourceTearOff>,
    get_private_data: get_private_data::<ResourceTearOff>,
    get_parent: dxgi_res_get_device,
    get_device: dxgi_res_get_device,
    acquire_sync: km_acquire_sync,
    release_sync: km_release_sync,
};

// --- Device resource creation entry points ---

pub fn device_supports_family(device: *mut c_void, family: u32) -> bool {
    let Some(dev) = Device::from_iface(device) else {
        return true;
    };
    let h = dev.core().device().handle();
    if h == 0 {
        return true;
    }
    // SAFETY: `h` is a live `MTLDevice` handle.
    unsafe { winemetal_sys::MTLDevice_supportsFamily(h, family) }
}

fn create_buffer_inner(
    device: *mut c_void,
    desc: *const D3d11BufferDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let hr = helper::validate_buffer_desc(desc);
    if hr < 0 {
        return hr;
    }
    // SAFETY: `validate_buffer_desc` succeeded, so `desc` is non-null.
    let d = unsafe { *desc };
    if out.is_null() {
        return S_FALSE;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    if d.usage == D3D11_USAGE_STAGING {
        staging::create_staging_buffer(device, &d, init, out)
    } else {
        buffer::create_default_buffer(device, &d, init, out)
    }
}

/// `ID3D11Device::CreateBuffer`.
pub fn create_buffer(
    device: *mut c_void,
    desc: *const D3d11BufferDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_buffer_inner(device, desc, init, out)
}

fn create_texture1d_inner(
    device: *mut c_void,
    desc: *const D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null.
    let d = unsafe { *desc };
    let mut final_desc = d;
    let mut mtl_desc = winemetal_sys::wire::WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: 0,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let hr = helper::create_mtl_texture_descriptor_1d(
        &d,
        &mut final_desc,
        &mut mtl_desc,
        |f| device_supports_family(device, f),
        true,
    );
    if hr < 0 {
        return hr;
    }
    if out.is_null() {
        return S_FALSE;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    match final_desc.usage {
        D3D11_USAGE_DEFAULT | D3D11_USAGE_IMMUTABLE => {
            texture::create_device_texture_1d(device, &d, init, out)
        }
        D3D11_USAGE_DYNAMIC => {
            if (final_desc.bind_flags
                & (D3D11_BIND_RENDER_TARGET
                    | D3D11_BIND_DEPTH_STENCIL
                    | D3D11_BIND_UNORDERED_ACCESS))
                != 0
                || (final_desc.misc_flags & D3D11_RESOURCE_MISC_GENERATE_MIPS) != 0
            {
                return E_INVALIDARG;
            }
            if final_desc.mip_levels == 1
                && final_desc.array_size == 1
                && linear::create_dynamic_linear_texture_1d(device, &d, init, out) >= 0
            {
                return S_OK;
            }
            texture::create_dynamic_texture_1d(device, &d, init, out)
        }
        D3D11_USAGE_STAGING => staging::create_staging_texture_1d(device, &d, init, out),
        _ => E_INVALIDARG,
    }
}

/// `ID3D11Device::CreateTexture1D`.
pub fn create_texture1d(
    device: *mut c_void,
    desc: *const D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture1d_inner(device, desc, init, out)
}

fn create_texture2d_inner(
    device: *mut c_void,
    desc: *const D3d11Texture2dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null.
    let d1 = helper::upgrade_texture2d_desc(unsafe { &*desc });
    create_texture2d1_inner(device, &raw const d1, init, out)
}

/// `ID3D11Device::CreateTexture2D`.
pub fn create_texture2d(
    device: *mut c_void,
    desc: *const D3d11Texture2dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture2d_inner(device, desc, init, out)
}

fn create_texture2d1_inner(
    device: *mut c_void,
    desc: *const D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null.
    let d = unsafe { *desc };
    let mut final_desc = d;
    let mut mtl_desc = winemetal_sys::wire::WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: 0,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let hr = helper::create_mtl_texture_descriptor_2d(
        &d,
        &mut final_desc,
        &mut mtl_desc,
        |f| device_supports_family(device, f),
        true,
    );
    if hr < 0 {
        return hr;
    }
    if out.is_null() {
        return S_FALSE;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    match final_desc.usage {
        D3D11_USAGE_DEFAULT | D3D11_USAGE_IMMUTABLE => {
            texture::create_device_texture_2d(device, &d, init, out)
        }
        D3D11_USAGE_DYNAMIC => {
            if (final_desc.bind_flags
                & (D3D11_BIND_RENDER_TARGET
                    | D3D11_BIND_DEPTH_STENCIL
                    | D3D11_BIND_UNORDERED_ACCESS))
                != 0
                || (final_desc.misc_flags & D3D11_RESOURCE_MISC_GENERATE_MIPS) != 0
            {
                return E_INVALIDARG;
            }
            if final_desc.mip_levels == 1
                && final_desc.array_size == 1
                && linear::create_dynamic_linear_texture_2d(device, &d, init, out) >= 0
            {
                return S_OK;
            }
            texture::create_dynamic_texture_2d(device, &d, init, out)
        }
        D3D11_USAGE_STAGING => staging::create_staging_texture_2d(device, &d, init, out),
        _ => E_INVALIDARG,
    }
}

/// `ID3D11Device3::CreateTexture2D1`.
pub fn create_texture2d1(
    device: *mut c_void,
    desc: *const D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture2d1_inner(device, desc, init, out)
}

fn create_texture3d_inner(
    device: *mut c_void,
    desc: *const D3d11Texture3dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null.
    let d1 = helper::upgrade_texture3d_desc(unsafe { &*desc });
    create_texture3d1_inner(device, &raw const d1, init, out)
}

/// `ID3D11Device::CreateTexture3D`.
pub fn create_texture3d(
    device: *mut c_void,
    desc: *const D3d11Texture3dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture3d_inner(device, desc, init, out)
}

fn create_texture3d1_inner(
    device: *mut c_void,
    desc: *const D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was checked non-null.
    let d = unsafe { *desc };
    let mut final_desc = d;
    let mut mtl_desc = winemetal_sys::wire::WMTTextureInfo {
        pixel_format: 0,
        width: 0,
        height: 0,
        depth: 0,
        array_length: 0,
        bits: 0,
        options: 0,
        flags: 0,
        mach_port: 0,
        gpu_resource_id: 0,
    };
    let hr = helper::create_mtl_texture_descriptor_3d(
        &d,
        &mut final_desc,
        &mut mtl_desc,
        |f| device_supports_family(device, f),
        true,
    );
    if hr < 0 {
        return hr;
    }
    if out.is_null() {
        return S_FALSE;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = core::ptr::null_mut() };
    match final_desc.usage {
        D3D11_USAGE_DEFAULT | D3D11_USAGE_IMMUTABLE => {
            texture::create_device_texture_3d(device, &d, init, out)
        }
        D3D11_USAGE_DYNAMIC => {
            if (final_desc.bind_flags
                & (D3D11_BIND_RENDER_TARGET
                    | D3D11_BIND_DEPTH_STENCIL
                    | D3D11_BIND_UNORDERED_ACCESS))
                != 0
                || (final_desc.misc_flags & D3D11_RESOURCE_MISC_GENERATE_MIPS) != 0
            {
                return E_INVALIDARG;
            }
            texture::create_dynamic_texture_3d(device, &d, init, out)
        }
        D3D11_USAGE_STAGING => staging::create_staging_texture_3d(device, &d, init, out),
        _ => E_INVALIDARG,
    }
}

/// `ID3D11Device3::CreateTexture3D1`.
pub fn create_texture3d1(
    device: *mut c_void,
    desc: *const D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture3d1_inner(device, desc, init, out)
}

/// `ID3D11Device::OpenSharedResource`.
pub fn open_shared(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    texture::import_shared_texture(device, handle, riid, out)
}

/// `ID3D11Device1::OpenSharedResource1`.
pub fn open_shared1(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    texture::import_shared_texture_from_nt_handle(device, handle, riid, out)
}

/// `ID3D11Device1::OpenSharedResourceByName`.
pub fn open_shared_by_name(
    device: *mut c_void,
    name: *const u16,
    access: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    texture::import_shared_texture_by_name(device, name, access, riid, out)
}

#[cfg(test)]
mod tests;
