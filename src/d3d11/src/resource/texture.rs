//! 1D, 2D and 3D device/dynamic texture resources (`d3d11_texture*.cpp`).

use core::{ffi::c_void, num::NonZeroUsize, ptr};
use std::sync::{Arc, Mutex};

use dxmt_core::{
    dynamic::DynamicBuffer,
    encoder::BufferSlice,
    format::{
        MTL_DXGI_FORMAT_BC, MTL_DXGI_FORMAT_EMULATED_D24, depth_stencil_planar_flags, query_dxgi,
    },
    initializer::SubresourceData,
    resource::{
        Buffer, BufferAllocationFlags, RenamableTexturePool, Texture, TextureAllocation,
        TextureAllocationFlags,
    },
};
use dxmt_util::dxgi_consts::{
    D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_RENDER_TARGET, D3D11_BIND_UNORDERED_ACCESS,
    D3D11_USAGE_IMMUTABLE,
};
use winemetal_sys::{
    MTLTexture_replaceRegion, WMTBootstrapLookUp, WMTBootstrapRegister,
    wire::{MemPtr, WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8, WMTOrigin, WMTSize, WMTTextureInfo},
};

use super::{
    Common, DynamicTexSubresource, Resource, ResourceDesc, ResourceKind, device_supports_family,
    helper::{
        create_mtl_texture_descriptor_1d, create_mtl_texture_descriptor_2d,
        create_mtl_texture_descriptor_3d, extract_entire_dsv_1d, extract_entire_dsv_2d,
        extract_entire_dsv_3d, extract_entire_rtv_1d, extract_entire_rtv_2d, extract_entire_rtv_3d,
        extract_entire_srv_1d, extract_entire_srv_2d, extract_entire_srv_3d, extract_entire_uav_1d,
        extract_entire_uav_2d, extract_entire_uav_3d, get_linear_texture_layout_1d,
        get_linear_texture_layout_2d, get_linear_texture_layout_3d, get_mipmap_size_1d,
        get_mipmap_size_2d, get_mipmap_size_3d,
    },
};
use crate::{
    abi::records::{
        D3d11DepthStencilViewDesc, D3d11RenderTargetViewDesc1, D3d11ShaderResourceViewDesc1,
        D3d11SubresourceData, D3d11Texture1dDesc, D3d11Texture2dDesc1, D3d11Texture3dDesc1,
        D3d11UnorderedAccessViewDesc1,
    },
    com::{E_FAIL, E_INVALIDARG, Guid, Hresult, S_FALSE, S_OK, release},
    device::Device,
    keyed_mutex::{
        D3D11_RESOURCE_MISC_SHARED, D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX,
        D3D11_RESOURCE_MISC_SHARED_NTHANDLE, KeyedMutex,
    },
    kmt::{self, SharedResourceData, SharedResourceDesc},
    subresource::{
        ResourceSubset, calc_subresource, enumerate_subresources_1d, enumerate_subresources_2d,
        enumerate_subresources_3d,
    },
    view::{
        Dsv, DsvObject, Rtv, RtvObject, Srv, SrvObject, Uav, UavObject,
        helper::{
            initialize_and_normalize_dsv_descriptor, initialize_and_normalize_rtv_descriptor,
            initialize_and_normalize_srv_descriptor, initialize_and_normalize_uav_descriptor,
        },
    },
};

const D3D11_RESOURCE_DIMENSION_TEXTURE1D: u32 = 2;
const D3D11_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;
const D3D11_RESOURCE_DIMENSION_TEXTURE3D: u32 = 4;

const D3D11_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;

const SHARED_FLAGS: u32 = D3D11_RESOURCE_MISC_SHARED
    | D3D11_RESOURCE_MISC_SHARED_NTHANDLE
    | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX;

const RENAMABLE_POOL_CAPACITY: NonZeroUsize = NonZeroUsize::new(32).unwrap();

const fn zero_tex_info() -> WMTTextureInfo {
    WMTTextureInfo {
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
    }
}

/// Texture resource wrapper (`MTLD3D11Texture1D`, `MTLD3D11Texture2D`, `MTLD3D11Texture3D`).
pub struct TextureResource {
    pub common: Common,
}

/// Uploads initial data into a 1D managed texture (`InitializeTextureData` in `d3d11_texture.cpp`).
pub fn initialize_texture_data_1d(
    target_handle: u64,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
) {
    initialize_texture_data_1d_inner(target_handle, desc, init);
}

fn initialize_texture_data_1d_inner(
    target_handle: u64,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
) {
    if init.is_null() || target_handle == 0 {
        return;
    }
    let mut width = desc.width;
    for level in 0..desc.mip_levels {
        let bpr = get_linear_texture_layout_1d(desc, level, false).map_or(0, |(r, _, _)| r);
        for slice in 0..desc.array_size {
            let idx = usize::try_from(calc_subresource(level, slice, desc.mip_levels)).unwrap_or(0);
            // SAFETY: `init` points to an array of `D3d11SubresourceData` covering all subresources.
            let sub_ptr = unsafe { init.add(idx) };
            // SAFETY: `sub_ptr` is a valid element of the initial data array.
            let sub = unsafe { &*sub_ptr };
            if !sub.p_sys_mem.is_null() {
                let row_pitch = u64::from(sub.sys_mem_pitch.max(bpr));
                let data = MemPtr::from_addr(sub.p_sys_mem.expose_provenance());
                // SAFETY: `target_handle` is a live `MTLTexture` and `data` points to subresource bytes.
                unsafe {
                    MTLTexture_replaceRegion(
                        target_handle,
                        WMTOrigin { x: 0, y: 0, z: 0 },
                        WMTSize {
                            width: u64::from(width),
                            height: 1,
                            depth: 1,
                        },
                        u64::from(level),
                        u64::from(slice),
                        data,
                        row_pitch,
                        0,
                    );
                }
            }
        }
        width = (width >> 1).max(1);
    }
}

/// Uploads initial data into a 2D managed texture (`InitializeTextureData` in `d3d11_texture.cpp`).
pub fn initialize_texture_data_2d(
    target_handle: u64,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
) {
    initialize_texture_data_2d_inner(target_handle, desc, init);
}

fn initialize_texture_data_2d_inner(
    target_handle: u64,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
) {
    if init.is_null() || target_handle == 0 {
        return;
    }
    let mut width = desc.width;
    let mut height = desc.height;
    for level in 0..desc.mip_levels {
        let bpr = get_linear_texture_layout_2d(desc, level, false).map_or(0, |(r, _, _)| r);
        for slice in 0..desc.array_size {
            let idx = usize::try_from(calc_subresource(level, slice, desc.mip_levels)).unwrap_or(0);
            // SAFETY: `init` points to an array of `D3d11SubresourceData` covering all subresources.
            let sub_ptr = unsafe { init.add(idx) };
            // SAFETY: `sub_ptr` is a valid element of the initial data array.
            let sub = unsafe { &*sub_ptr };
            if !sub.p_sys_mem.is_null() {
                let row_pitch = u64::from(sub.sys_mem_pitch.max(bpr));
                let data = MemPtr::from_addr(sub.p_sys_mem.expose_provenance());
                // SAFETY: `target_handle` is a live `MTLTexture` and `data` points to subresource bytes.
                unsafe {
                    MTLTexture_replaceRegion(
                        target_handle,
                        WMTOrigin { x: 0, y: 0, z: 0 },
                        WMTSize {
                            width: u64::from(width),
                            height: u64::from(height),
                            depth: 1,
                        },
                        u64::from(level),
                        u64::from(slice),
                        data,
                        row_pitch,
                        0,
                    );
                }
            }
        }
        width = (width >> 1).max(1);
        height = (height >> 1).max(1);
    }
}

/// Uploads initial data into a 3D managed texture (`InitializeTextureData` in `d3d11_texture.cpp`).
pub fn initialize_texture_data_3d(
    target_handle: u64,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
) {
    initialize_texture_data_3d_inner(target_handle, desc, init);
}

fn initialize_texture_data_3d_inner(
    target_handle: u64,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
) {
    if init.is_null() || target_handle == 0 {
        return;
    }
    let mut width = desc.width;
    let mut height = desc.height;
    let mut depth = desc.depth;
    for level in 0..desc.mip_levels {
        let idx = usize::try_from(level).unwrap_or(0);
        // SAFETY: `init` points to an array of `D3d11SubresourceData` covering all mip levels.
        let sub_ptr = unsafe { init.add(idx) };
        // SAFETY: `sub_ptr` is a valid element of the initial data array.
        let sub = unsafe { &*sub_ptr };
        if !sub.p_sys_mem.is_null() {
            let data = MemPtr::from_addr(sub.p_sys_mem.expose_provenance());
            // SAFETY: `target_handle` is a live `MTLTexture` and `data` points to subresource bytes.
            unsafe {
                MTLTexture_replaceRegion(
                    target_handle,
                    WMTOrigin { x: 0, y: 0, z: 0 },
                    WMTSize {
                        width: u64::from(width),
                        height: u64::from(height),
                        depth: u64::from(depth),
                    },
                    u64::from(level),
                    0,
                    data,
                    u64::from(sub.sys_mem_pitch),
                    u64::from(sub.sys_mem_slice_pitch),
                );
            }
        }
        width = (width >> 1).max(1);
        height = (height >> 1).max(1);
        depth = (depth >> 1).max(1);
    }
}

/// Computes the byte span of a subresource in `D3D11_SUBRESOURCE_DATA::pSysMem`.
fn subresource_data_byte_len(
    info: &WMTTextureInfo,
    fmt_flag: u16,
    bytes_per_texel: u32,
    size: (u32, u32, u32),
    row_pitch: usize,
    depth_pitch: usize,
    dim: u8,
) -> usize {
    let (w, h, d) = size;
    if info.pixel_format == WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8 {
        let texel = if (fmt_flag & MTL_DXGI_FORMAT_EMULATED_D24) != 0 {
            4usize
        } else {
            8usize
        };
        let row_bytes = usize::try_from(w).unwrap_or(0).saturating_mul(texel);
        return usize::try_from(h)
            .unwrap_or(0)
            .saturating_sub(1)
            .saturating_mul(row_pitch)
            .saturating_add(row_bytes);
    }
    let is_bc = (fmt_flag & MTL_DXGI_FORMAT_BC) != 0;
    let rows = usize::try_from(if is_bc { h.div_ceil(4) } else { h }).unwrap_or(0);
    let cols = usize::try_from(if is_bc { w.div_ceil(4) } else { w }).unwrap_or(0);
    let row_bytes = usize::try_from(bytes_per_texel)
        .unwrap_or(0)
        .saturating_mul(cols);
    let image_bytes = if dim == 1 {
        row_bytes
    } else {
        rows.saturating_sub(1)
            .saturating_mul(row_pitch)
            .saturating_add(row_pitch.min(row_bytes))
    };
    if dim == 3 {
        usize::try_from(d)
            .unwrap_or(0)
            .saturating_sub(1)
            .saturating_mul(depth_pitch)
            .saturating_add(
                depth_pitch
                    .min(row_bytes.saturating_mul(rows))
                    .max(image_bytes),
            )
    } else {
        image_bytes
    }
}

struct SubresourceInit<'a> {
    dev: &'a Device,
    texture: &'a Texture,
    allocation: &'a Arc<TextureAllocation>,
    init: *const D3d11SubresourceData,
    fmt_flag: u16,
    bytes_per_texel: u32,
    dim: u8,
}

/// Initializes a device texture allocation via [`dxmt_core::initializer::ResourceInitializer`].
fn init_device_subresource(
    ctx: &SubresourceInit<'_>,
    slice: u32,
    level: u32,
    size: (u32, u32, u32),
    subresource_id: u32,
) {
    let initializer = ctx.dev.queue().initializer();
    if ctx.init.is_null() {
        let _ = initializer.init_texture_zero(ctx.texture, ctx.allocation, slice, level);
        return;
    }
    let idx = usize::try_from(subresource_id).unwrap_or(0);
    // SAFETY: `init` points to an array of `D3d11SubresourceData` covering all subresources.
    let sub_ptr = unsafe { ctx.init.add(idx) };
    // SAFETY: `sub_ptr` is a valid element of the initial data array.
    let sub = unsafe { &*sub_ptr };
    if sub.p_sys_mem.is_null() {
        let _ = initializer.init_texture_zero(ctx.texture, ctx.allocation, slice, level);
        return;
    }
    let row_pitch = usize::try_from(sub.sys_mem_pitch).unwrap_or(0);
    let depth_pitch = usize::try_from(sub.sys_mem_slice_pitch).unwrap_or(0);
    let byte_len = subresource_data_byte_len(
        ctx.texture.info(),
        ctx.fmt_flag,
        ctx.bytes_per_texel,
        size,
        row_pitch,
        depth_pitch,
        ctx.dim,
    );
    let ptr = sub.p_sys_mem.cast::<u8>();
    // SAFETY: `p_sys_mem` points to at least `byte_len` bytes of subresource initial data.
    let bytes = unsafe { core::slice::from_raw_parts(ptr, byte_len) };
    let data = SubresourceData {
        bytes,
        row_pitch,
        depth_pitch,
    };
    let _ = initializer.init_texture_data(
        ctx.texture,
        ctx.allocation,
        slice,
        level,
        &data,
        u32::from(ctx.fmt_flag),
    );
}

struct SharedTextureSetup {
    allocation: Arc<TextureAllocation>,
    local_kmt: u32,
    global_kmt: u32,
    keyed_mutex: Option<Arc<KeyedMutex>>,
}

/// Creates a shared D3DKMT allocation + optional [`KeyedMutex`] for a shared `DeviceTexture`.
fn setup_shared_device_texture(
    device: *mut c_void,
    texture: &Texture,
    usage: u32,
    bind_flags: u32,
    misc_flags: u32,
    dimension: u32,
    shared_desc: SharedResourceDesc,
) -> Result<SharedTextureSetup, Hresult> {
    let Some(dev) = Device::from_iface(device) else {
        return Err(E_FAIL);
    };
    let kmt_device = kmt::device_local_kmt(device);
    if (kmt_device & 0xc000_0000) == 0 {
        return Err(E_FAIL);
    }
    let mut flags = TextureAllocationFlags::GPU_PRIVATE | TextureAllocationFlags::SHARED;
    if usage == D3D11_USAGE_IMMUTABLE {
        flags.insert(TextureAllocationFlags::GPU_READONLY);
    }
    if (bind_flags
        & (D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_RENDER_TARGET | D3D11_BIND_DEPTH_STENCIL))
        == 0
    {
        flags.insert(TextureAllocationFlags::SHADER_READONLY);
    }
    let allocation = texture.allocate(flags);
    let mach_port = allocation.mach_port();
    if mach_port == 0 {
        return Err(E_FAIL);
    }
    let mach_port_name = kmt::make_unique_shared_name();
    // SAFETY: `mach_port_name` is a 54-byte NUL-terminated ASCII string.
    let registered = unsafe { WMTBootstrapRegister(mach_port_name.as_ptr().cast(), mach_port) };
    if !registered {
        return Err(E_FAIL);
    }
    let keyed_mutex = if (misc_flags & D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX) != 0 {
        KeyedMutex::create(dev.core().device())
    } else {
        None
    };
    let runtime_data = SharedResourceData {
        mach_port_name,
        dimension,
        desc: shared_desc,
        mutex_handle: keyed_mutex.as_ref().map_or(0, |km| km.global_handle()),
    };
    let nt_sharing = (misc_flags & D3D11_RESOURCE_MISC_SHARED_NTHANDLE) != 0;
    let Some((local_kmt, global_kmt)) =
        kmt::create_shared_allocation(kmt_device, &runtime_data, nt_sharing)
    else {
        return Err(E_FAIL);
    };
    Ok(SharedTextureSetup {
        allocation,
        local_kmt,
        global_kmt,
        keyed_mutex,
    })
}

/// Computes `TextureAllocationFlags` for a non-shared `DeviceTexture`.
fn device_texture_flags(
    cpu_access_flags: u32,
    bind_flags: u32,
    usage: u32,
) -> TextureAllocationFlags {
    let mut flags = if cpu_access_flags != 0 {
        TextureAllocationFlags::GPU_MANAGED
    } else {
        TextureAllocationFlags::GPU_PRIVATE
    };
    if (bind_flags
        & (D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_RENDER_TARGET | D3D11_BIND_DEPTH_STENCIL))
        == 0
    {
        flags.insert(TextureAllocationFlags::SHADER_READONLY);
    }
    if usage == D3D11_USAGE_IMMUTABLE {
        flags.insert(TextureAllocationFlags::GPU_READONLY);
    }
    flags
}

/// Creates a 1D device texture (`CreateDeviceTexture1D` in `d3d11_texture_device.cpp`).
pub fn create_device_texture_1d(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_device_texture_1d_inner(device, desc, init, out)
}

fn create_device_texture_1d_inner(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_1d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let single_subresource = info.mipmap_level_count() == 1
        && info.array_length == 1
        && (final_desc.misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) == 0;
    let texture = Arc::new(Texture::new(info, mtl));
    let (_, fmt_desc) = query_dxgi(final_desc.format);
    let subs = enumerate_subresources_1d(&final_desc);
    let initialize = |alloc: Arc<TextureAllocation>| {
        let _ = texture.rename(Arc::clone(&alloc));
        let ctx = SubresourceInit {
            dev,
            texture: &texture,
            allocation: &alloc,
            init,
            fmt_flag: fmt_desc.flag,
            bytes_per_texel: fmt_desc.bytes_per_texel,
            dim: 1,
        };
        for sub in &subs {
            let size = get_mipmap_size_1d(&final_desc, sub.mip_level);
            init_device_subresource(
                &ctx,
                sub.array_slice,
                sub.mip_level,
                size,
                sub.subresource_id,
            );
        }
    };
    let mut local_kmt = 0;
    let mut global_kmt = 0;
    let mut keyed_mutex = None;
    let mut renamable = None;
    if (final_desc.misc_flags & SHARED_FLAGS) != 0 {
        let Ok(setup) = setup_shared_device_texture(
            device,
            &texture,
            final_desc.usage,
            final_desc.bind_flags,
            final_desc.misc_flags,
            D3D11_RESOURCE_DIMENSION_TEXTURE1D,
            SharedResourceDesc { desc1d: *desc },
        ) else {
            return E_FAIL;
        };
        initialize(setup.allocation);
        local_kmt = setup.local_kmt;
        global_kmt = setup.global_kmt;
        keyed_mutex = setup.keyed_mutex;
    } else {
        let flags = device_texture_flags(
            final_desc.cpu_access_flags,
            final_desc.bind_flags,
            final_desc.usage,
        );
        if single_subresource && (final_desc.bind_flags & D3D11_BIND_DEPTH_STENCIL) != 0 {
            let mut pool =
                RenamableTexturePool::new(Arc::clone(&texture), RENAMABLE_POOL_CAPACITY, flags);
            initialize(pool.next_allocation(0));
            renamable = Some(Arc::new(Mutex::new(pool)));
        } else {
            initialize(texture.allocate(flags));
        }
    }
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DeviceTexture,
        ResourceDesc::Texture1d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    let res = unsafe { &mut *raw };
    res.renamable = renamable;
    res.local_kmt = local_kmt;
    res.global_kmt = global_kmt;
    // SAFETY: `out` was checked non-null by `create_texture1d`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 2D device texture (`CreateDeviceTexture2D` in `d3d11_texture_device.cpp`).
pub fn create_device_texture_2d(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_device_texture_2d_inner(device, desc, init, out)
}

fn create_device_texture_2d_inner(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_2d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let single_subresource = info.mipmap_level_count() == 1
        && info.array_length == 1
        && (final_desc.misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) == 0;
    let texture = Arc::new(Texture::new(info, mtl));
    let (_, fmt_desc) = query_dxgi(final_desc.format);
    let subs = enumerate_subresources_2d(&final_desc);
    let initialize = |alloc: Arc<TextureAllocation>| {
        let _ = texture.rename(Arc::clone(&alloc));
        let ctx = SubresourceInit {
            dev,
            texture: &texture,
            allocation: &alloc,
            init,
            fmt_flag: fmt_desc.flag,
            bytes_per_texel: fmt_desc.bytes_per_texel,
            dim: 2,
        };
        for sub in &subs {
            let size = get_mipmap_size_2d(&final_desc, sub.mip_level);
            init_device_subresource(
                &ctx,
                sub.array_slice,
                sub.mip_level,
                size,
                sub.subresource_id,
            );
        }
    };
    let mut local_kmt = 0;
    let mut global_kmt = 0;
    let mut keyed_mutex = None;
    let mut renamable = None;
    if (final_desc.misc_flags & SHARED_FLAGS) != 0 {
        let Ok(setup) = setup_shared_device_texture(
            device,
            &texture,
            final_desc.usage,
            final_desc.bind_flags,
            final_desc.misc_flags,
            D3D11_RESOURCE_DIMENSION_TEXTURE2D,
            SharedResourceDesc { desc2d: *desc },
        ) else {
            return E_FAIL;
        };
        initialize(setup.allocation);
        local_kmt = setup.local_kmt;
        global_kmt = setup.global_kmt;
        keyed_mutex = setup.keyed_mutex;
    } else {
        let flags = device_texture_flags(
            final_desc.cpu_access_flags,
            final_desc.bind_flags,
            final_desc.usage,
        );
        if single_subresource && (final_desc.bind_flags & D3D11_BIND_DEPTH_STENCIL) != 0 {
            let mut pool =
                RenamableTexturePool::new(Arc::clone(&texture), RENAMABLE_POOL_CAPACITY, flags);
            initialize(pool.next_allocation(0));
            renamable = Some(Arc::new(Mutex::new(pool)));
        } else {
            initialize(texture.allocate(flags));
        }
    }
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DeviceTexture,
        ResourceDesc::Texture2d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    let res = unsafe { &mut *raw };
    res.renamable = renamable;
    res.local_kmt = local_kmt;
    res.global_kmt = global_kmt;
    // SAFETY: `out` was checked non-null by `create_texture2d1`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 3D device texture (`CreateDeviceTexture3D` in `d3d11_texture_device.cpp`).
pub fn create_device_texture_3d(
    device: *mut c_void,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_device_texture_3d_inner(device, desc, init, out)
}

fn create_device_texture_3d_inner(
    device: *mut c_void,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_3d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let single_subresource = info.mipmap_level_count() == 1
        && info.array_length == 1
        && (final_desc.misc_flags & D3D11_RESOURCE_MISC_TEXTURECUBE) == 0;
    let texture = Arc::new(Texture::new(info, mtl));
    let (_, fmt_desc) = query_dxgi(final_desc.format);
    let subs = enumerate_subresources_3d(&final_desc);
    let initialize = |alloc: Arc<TextureAllocation>| {
        let _ = texture.rename(Arc::clone(&alloc));
        let ctx = SubresourceInit {
            dev,
            texture: &texture,
            allocation: &alloc,
            init,
            fmt_flag: fmt_desc.flag,
            bytes_per_texel: fmt_desc.bytes_per_texel,
            dim: 3,
        };
        for sub in &subs {
            let size = get_mipmap_size_3d(&final_desc, sub.mip_level);
            init_device_subresource(
                &ctx,
                sub.array_slice,
                sub.mip_level,
                size,
                sub.subresource_id,
            );
        }
    };
    let mut local_kmt = 0;
    let mut global_kmt = 0;
    let mut keyed_mutex = None;
    let mut renamable = None;
    if (final_desc.misc_flags & SHARED_FLAGS) != 0 {
        let Ok(setup) = setup_shared_device_texture(
            device,
            &texture,
            final_desc.usage,
            final_desc.bind_flags,
            final_desc.misc_flags,
            D3D11_RESOURCE_DIMENSION_TEXTURE3D,
            SharedResourceDesc { desc3d: *desc },
        ) else {
            return E_FAIL;
        };
        initialize(setup.allocation);
        local_kmt = setup.local_kmt;
        global_kmt = setup.global_kmt;
        keyed_mutex = setup.keyed_mutex;
    } else {
        let flags = device_texture_flags(
            final_desc.cpu_access_flags,
            final_desc.bind_flags,
            final_desc.usage,
        );
        if single_subresource && (final_desc.bind_flags & D3D11_BIND_DEPTH_STENCIL) != 0 {
            let mut pool =
                RenamableTexturePool::new(Arc::clone(&texture), RENAMABLE_POOL_CAPACITY, flags);
            initialize(pool.next_allocation(0));
            renamable = Some(Arc::new(Mutex::new(pool)));
        } else {
            initialize(texture.allocate(flags));
        }
    }
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DeviceTexture,
        ResourceDesc::Texture3d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    let res = unsafe { &mut *raw };
    res.renamable = renamable;
    res.local_kmt = local_kmt;
    res.global_kmt = global_kmt;
    // SAFETY: `out` was checked non-null by `create_texture3d1`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Allocates a [`DynamicTexSubresource`] for one subresource of a dynamic non-linear texture.
fn alloc_dynamic_subresource(
    mtl: &winemetal_sys::Obj,
    bpr: u32,
    bpi: u32,
    buf_len: u32,
) -> Option<DynamicTexSubresource> {
    let mut buffer_flags = BufferAllocationFlags::CPU_WRITE_COMBINED
        | BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE;
    if cfg!(target_arch = "x86") {
        buffer_flags.insert(BufferAllocationFlags::CPU_PLACED);
    }
    let buffer = Arc::new(Buffer::new(u64::from(buf_len), mtl));
    let _ = buffer.rename(buffer.allocate(buffer_flags));
    let dynamic = Arc::new(DynamicBuffer::new(Arc::clone(&buffer), buffer_flags)?);
    Some(DynamicTexSubresource {
        dynamic,
        bytes_per_row: bpr,
        bytes_per_image: bpi,
    })
}

/// Creates a 1D dynamic non-linear texture (`CreateDynamicTexture1D`).
pub fn create_dynamic_texture_1d(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_dynamic_texture_1d_inner(device, desc, init, out)
}

fn create_dynamic_texture_1d_inner(
    device: *mut c_void,
    desc: &D3d11Texture1dDesc,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_1d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let subs = enumerate_subresources_1d(&final_desc);
    let mut subresources = Vec::with_capacity(subs.len());
    for sub in subs {
        let Ok((bpr, bpi, buf_len)) =
            get_linear_texture_layout_1d(&final_desc, sub.mip_level, true)
        else {
            return E_FAIL;
        };
        let Some(dyn_sub) = alloc_dynamic_subresource(mtl, bpr, bpi, buf_len) else {
            return E_FAIL;
        };
        subresources.push(dyn_sub);
    }
    let texture = Arc::new(Texture::new(info, mtl));
    let flags = TextureAllocationFlags::GPU_MANAGED | TextureAllocationFlags::SHADER_READONLY;
    let allocation = texture.allocate(flags);
    if !init.is_null() {
        initialize_texture_data_1d(allocation.texture(), &final_desc, init);
    }
    let _ = texture.rename(allocation);
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DynamicTexture,
        ResourceDesc::Texture1d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    unsafe { (*raw).dynamic_subresources = subresources };
    // SAFETY: `out` was checked non-null by `create_texture1d`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 2D dynamic non-linear texture (`CreateDynamicTexture2D`).
pub fn create_dynamic_texture_2d(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_dynamic_texture_2d_inner(device, desc, init, out)
}

fn create_dynamic_texture_2d_inner(
    device: *mut c_void,
    desc: &D3d11Texture2dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_2d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let subs = enumerate_subresources_2d(&final_desc);
    let mut subresources = Vec::with_capacity(subs.len());
    for sub in subs {
        let Ok((bpr, bpi, buf_len)) =
            get_linear_texture_layout_2d(&final_desc, sub.mip_level, true)
        else {
            return E_FAIL;
        };
        let Some(dyn_sub) = alloc_dynamic_subresource(mtl, bpr, bpi, buf_len) else {
            return E_FAIL;
        };
        subresources.push(dyn_sub);
    }
    let texture = Arc::new(Texture::new(info, mtl));
    let flags = TextureAllocationFlags::GPU_MANAGED | TextureAllocationFlags::SHADER_READONLY;
    let allocation = texture.allocate(flags);
    if !init.is_null() {
        initialize_texture_data_2d(allocation.texture(), &final_desc, init);
    }
    let _ = texture.rename(allocation);
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DynamicTexture,
        ResourceDesc::Texture2d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    unsafe { (*raw).dynamic_subresources = subresources };
    // SAFETY: `out` was checked non-null by `create_texture2d1`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// Creates a 3D dynamic non-linear texture (`CreateDynamicTexture3D`).
pub fn create_dynamic_texture_3d(
    device: *mut c_void,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    create_dynamic_texture_3d_inner(device, desc, init, out)
}

fn create_dynamic_texture_3d_inner(
    device: *mut c_void,
    desc: &D3d11Texture3dDesc1,
    init: *const D3d11SubresourceData,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let mtl = dev.core().device();
    let mut final_desc = *desc;
    let mut info = zero_tex_info();
    if create_mtl_texture_descriptor_3d(
        desc,
        &mut final_desc,
        &mut info,
        |f| device_supports_family(device, f),
        true,
    ) < 0
    {
        return E_INVALIDARG;
    }
    let subs = enumerate_subresources_3d(&final_desc);
    let mut subresources = Vec::with_capacity(subs.len());
    for sub in subs {
        let Ok((bpr, bpi, buf_len)) =
            get_linear_texture_layout_3d(&final_desc, sub.mip_level, true)
        else {
            return E_FAIL;
        };
        let Some(dyn_sub) = alloc_dynamic_subresource(mtl, bpr, bpi, buf_len) else {
            return E_FAIL;
        };
        subresources.push(dyn_sub);
    }
    let texture = Arc::new(Texture::new(info, mtl));
    let flags = TextureAllocationFlags::GPU_MANAGED | TextureAllocationFlags::SHADER_READONLY;
    let allocation = texture.allocate(flags);
    if !init.is_null() {
        initialize_texture_data_3d(allocation.texture(), &final_desc, init);
    }
    let _ = texture.rename(allocation);
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: None,
        bind_flags: final_desc.bind_flags,
        usage: final_desc.usage,
        misc_flags: final_desc.misc_flags,
        keyed_mutex: None,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(
        common,
        ResourceKind::DynamicTexture,
        ResourceDesc::Texture3d(final_desc),
    );
    // SAFETY: `raw` was just allocated by `Resource::into_raw`.
    unsafe { (*raw).dynamic_subresources = subresources };
    // SAFETY: `out` was checked non-null by `create_texture3d1`.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

// --- Texture SRV / UAV / RTV / DSV creation ---

fn finish_texture_srv(
    device: *mut c_void,
    resource: *mut c_void,
    res: &Resource,
    mip_levels: u32,
    array_size: u32,
    mut final_desc: D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(texture) = res.common.texture.as_ref() else {
        return E_FAIL;
    };
    let Ok(descriptor) = initialize_and_normalize_srv_descriptor(
        mip_levels,
        array_size,
        texture.info(),
        &mut final_desc,
    ) else {
        return E_FAIL;
    };
    if out.is_null() {
        return S_FALSE;
    }
    let view_key = descriptor.key();
    let subset = ResourceSubset::from_texture_view(
        &descriptor,
        texture.info().mipmap_level_count(),
        texture.array_length(),
        0,
    );
    let srv = Srv {
        resource,
        buffer: None,
        texture: Some(Arc::clone(texture)),
        buffer_view: None,
        texture_view: Some(view_key),
        slice: BufferSlice {
            byte_offset: 0,
            byte_length: 0,
            first_element: 0,
            element_count: 0,
        },
        subset,
        format: descriptor.format,
    };
    let raw = SrvObject::into_raw(srv, device, res.common.bind_flags, final_desc);
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates a 1D texture SRV (`DeviceTexture::CreateShaderResourceView`).
pub fn create_texture_srv_1d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_srv_1d_inner(device, resource, desc, out)
}

fn create_texture_srv_1d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_srv1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture1d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_srv_1d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_srv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 2D texture SRV (`DeviceTexture::CreateShaderResourceView`).
pub fn create_texture_srv_2d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_srv_2d_inner(device, resource, desc, out)
}

fn create_texture_srv_2d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_srv1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture2d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_srv_2d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_srv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 3D texture SRV (`DeviceTexture::CreateShaderResourceView`).
pub fn create_texture_srv_3d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_srv_3d_inner(device, resource, desc, out)
}

fn create_texture_srv_3d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11ShaderResourceViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_srv1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture3d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_srv_3d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_srv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.depth,
        final_desc,
        out,
    )
}

fn finish_texture_uav(
    device: *mut c_void,
    resource: *mut c_void,
    res: &Resource,
    mip_levels: u32,
    array_size: u32,
    mut final_desc: D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(texture) = res.common.texture.as_ref() else {
        return E_FAIL;
    };
    let Ok(descriptor) = initialize_and_normalize_uav_descriptor(
        mip_levels,
        array_size,
        texture.info(),
        &mut final_desc,
    ) else {
        return E_FAIL;
    };
    if out.is_null() {
        return S_FALSE;
    }
    let view_key = descriptor.key();
    let subset = ResourceSubset::from_texture_view(
        &descriptor,
        texture.info().mipmap_level_count(),
        texture.array_length(),
        0,
    );
    let uav = Uav {
        resource,
        buffer: None,
        texture: Some(Arc::clone(texture)),
        buffer_view: None,
        texture_view: Some(view_key),
        slice: BufferSlice {
            byte_offset: 0,
            byte_length: 0,
            first_element: 0,
            element_count: 0,
        },
        subset,
        format: descriptor.format,
        counter: None,
    };
    let raw = UavObject::into_raw(uav, device, res.common.bind_flags, final_desc);
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates a 1D texture UAV (`DeviceTexture::CreateUnorderedAccessView`).
pub fn create_texture_uav_1d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_uav_1d_inner(device, resource, desc, out)
}

fn create_texture_uav_1d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_uav1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture1d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_uav_1d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_uav(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 2D texture UAV (`DeviceTexture::CreateUnorderedAccessView`).
pub fn create_texture_uav_2d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_uav_2d_inner(device, resource, desc, out)
}

fn create_texture_uav_2d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_uav1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture2d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_uav_2d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_uav(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 3D texture UAV (`DeviceTexture::CreateUnorderedAccessView`).
pub fn create_texture_uav_3d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_uav_3d_inner(device, resource, desc, out)
}

fn create_texture_uav_3d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11UnorderedAccessViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_uav1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture3d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_uav_3d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_uav(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.depth,
        final_desc,
        out,
    )
}

fn finish_texture_rtv(
    device: *mut c_void,
    resource: *mut c_void,
    res: &Resource,
    mip_levels: u32,
    array_size: u32,
    mut final_desc: D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(texture) = res.common.texture.as_ref() else {
        return E_FAIL;
    };
    let Ok((pass_desc, descriptor)) = initialize_and_normalize_rtv_descriptor(
        mip_levels,
        array_size,
        texture.info(),
        &mut final_desc,
    ) else {
        return E_FAIL;
    };
    if out.is_null() {
        return S_FALSE;
    }
    let view_key = descriptor.key();
    let subset = ResourceSubset::from_texture_view(
        &descriptor,
        texture.info().mipmap_level_count(),
        texture.array_length(),
        0,
    );
    let rtv = Rtv {
        resource,
        buffer: None,
        texture: Some(Arc::clone(texture)),
        buffer_view: None,
        texture_view: Some(view_key),
        slice: BufferSlice {
            byte_offset: 0,
            byte_length: 0,
            first_element: 0,
            element_count: 0,
        },
        subset,
        format: descriptor.format,
        pixel_format: descriptor.format,
        array_length: pass_desc.render_target_array_length,
        width: pass_desc.width,
        height: pass_desc.height,
        sample_count: pass_desc.sample_count,
    };
    let raw = RtvObject::into_raw(rtv, device, res.common.bind_flags, final_desc, pass_desc);
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates a 1D texture RTV (`DeviceTexture::CreateRenderTargetView`).
pub fn create_texture_rtv_1d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_rtv_1d_inner(device, resource, desc, out)
}

fn create_texture_rtv_1d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_rtv1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture1d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_rtv_1d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_rtv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 2D texture RTV (`DeviceTexture::CreateRenderTargetView`).
pub fn create_texture_rtv_2d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_rtv_2d_inner(device, resource, desc, out)
}

fn create_texture_rtv_2d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_rtv1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture2d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_rtv_2d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_rtv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 3D texture RTV (`DeviceTexture::CreateRenderTargetView`).
pub fn create_texture_rtv_3d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_rtv_3d_inner(device, resource, desc, out)
}

fn create_texture_rtv_3d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11RenderTargetViewDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_rtv1`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture3d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_rtv_3d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_rtv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.depth,
        final_desc,
        out,
    )
}

fn finish_texture_dsv(
    device: *mut c_void,
    resource: *mut c_void,
    res: &Resource,
    mip_levels: u32,
    array_size: u32,
    mut final_desc: D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(texture) = res.common.texture.as_ref() else {
        return E_FAIL;
    };
    let Ok((pass_desc, descriptor)) = initialize_and_normalize_dsv_descriptor(
        mip_levels,
        array_size,
        texture.info(),
        &mut final_desc,
    ) else {
        return E_FAIL;
    };
    if out.is_null() {
        return S_FALSE;
    }
    let view_key = descriptor.key();
    let readonly_flags = u8::try_from(final_desc.flags & 0b11).unwrap_or(0);
    let planar_flags = u8::try_from(depth_stencil_planar_flags(descriptor.format)).unwrap_or(0);
    let subset = ResourceSubset::from_texture_view(
        &descriptor,
        texture.info().mipmap_level_count(),
        texture.array_length(),
        final_desc.flags & 0b11,
    );
    let dsv = Dsv {
        resource,
        texture: Some(Arc::clone(texture)),
        texture_view: Some(view_key),
        subset,
        format: descriptor.format,
        planar_flags,
        readonly_flags,
        array_length: pass_desc.render_target_array_length,
        width: pass_desc.width,
        height: pass_desc.height,
        sample_count: pass_desc.sample_count,
    };
    let raw = DsvObject::into_raw(
        dsv,
        device,
        res.common.bind_flags,
        final_desc,
        pass_desc,
        res.renamable.clone(),
    );
    // SAFETY: `out` was checked non-null above.
    unsafe { *out = raw };
    S_OK
}

/// Creates a 1D texture DSV (`DeviceTexture::CreateDepthStencilView`).
pub fn create_texture_dsv_1d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_dsv_1d_inner(device, resource, desc, out)
}

fn create_texture_dsv_1d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_dsv`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture1d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_dsv_1d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_dsv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 2D texture DSV (`DeviceTexture::CreateDepthStencilView`).
pub fn create_texture_dsv_2d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_dsv_2d_inner(device, resource, desc, out)
}

fn create_texture_dsv_2d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_dsv`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture2d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_dsv_2d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_dsv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.array_size,
        final_desc,
        out,
    )
}

/// Creates a 3D texture DSV (`DeviceTexture::CreateDepthStencilView`).
pub fn create_texture_dsv_3d(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    create_texture_dsv_3d_inner(device, resource, desc, out)
}

fn create_texture_dsv_3d_inner(
    device: *mut c_void,
    resource: *mut c_void,
    desc: *const D3d11DepthStencilViewDesc,
    out: *mut *mut c_void,
) -> Hresult {
    // SAFETY: `resource` was checked non-null by `create_dsv`.
    let res = unsafe { Resource::from_ptr(resource) };
    let ResourceDesc::Texture3d(tex_desc) = res.desc else {
        return E_INVALIDARG;
    };
    let desc_opt = if desc.is_null() {
        None
    } else {
        // SAFETY: `desc` was checked non-null above.
        Some(unsafe { &*desc })
    };
    let Ok(final_desc) = extract_entire_dsv_3d(&tex_desc, desc_opt) else {
        return E_INVALIDARG;
    };
    finish_texture_dsv(
        device,
        resource,
        res,
        tex_desc.mip_levels,
        tex_desc.depth,
        final_desc,
        out,
    )
}

// --- Shared texture import (`ImportSharedTexture*` in `d3d11_texture_device.cpp`) ---

fn finish_imported_shared_texture(
    device: *mut c_void,
    info: WMTTextureInfo,
    desc: ResourceDesc,
    common_flags: (u32, u32, u32),
    handles: (u32, u32),
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let Some(dev) = Device::from_iface(device) else {
        return E_INVALIDARG;
    };
    let (bind_flags, usage, misc_flags) = common_flags;
    let (mach_port, mutex_handle) = handles;
    let mtl = dev.core().device();
    let texture = Arc::new(Texture::new(info, mtl));
    let Some(allocation) = texture.import(mach_port) else {
        return E_FAIL;
    };
    let _ = texture.rename(allocation);
    let keyed_mutex = if (mutex_handle & 0xc000_0000) != 0 {
        KeyedMutex::import(mtl, mutex_handle)
    } else {
        None
    };
    let common = Common {
        device,
        buffer: None,
        texture: Some(texture),
        dynamic_buffer: None,
        dynamic_texture: None,
        staging: None,
        bind_flags,
        usage,
        misc_flags,
        keyed_mutex,
        min_lod: 0.0,
    };
    let raw = Resource::into_raw(common, ResourceKind::DeviceTexture, desc).cast::<c_void>();
    // SAFETY: `raw` is a live COM object starting with `IUnknownVtbl`.
    let unk = unsafe { crate::com::vtbl::<crate::abi::vtbl::IUnknownVtbl>(raw) };
    // SAFETY: `QueryInterface` is slot 0 on `IUnknownVtbl`.
    let hr = unsafe { (unk.query_interface)(raw, riid.cast(), out) };
    // SAFETY: releases the initial +1 reference from `Resource::into_raw`.
    unsafe { release(raw) };
    hr
}

fn import_from_runtime_data(
    device: *mut c_void,
    runtime_data: &SharedResourceData,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let mut mach_port = 0u32;
    // SAFETY: `mach_port_name` is a 54-byte NUL-terminated buffer; `mach_port` is a valid out ptr.
    let looked_up = unsafe {
        WMTBootstrapLookUp(
            runtime_data.mach_port_name.as_ptr().cast(),
            &raw mut mach_port,
        )
    };
    if !looked_up {
        return E_INVALIDARG;
    }
    let handles = (mach_port, runtime_data.mutex_handle);
    match runtime_data.dimension {
        D3D11_RESOURCE_DIMENSION_TEXTURE1D => {
            // SAFETY: `dimension == D3D11_RESOURCE_DIMENSION_TEXTURE1D` selects `desc.desc1d`.
            let d = unsafe { runtime_data.desc.desc1d };
            let mut final_desc = d;
            let mut info = zero_tex_info();
            if create_mtl_texture_descriptor_1d(
                &d,
                &mut final_desc,
                &mut info,
                |f| device_supports_family(device, f),
                true,
            ) < 0
            {
                return E_INVALIDARG;
            }
            finish_imported_shared_texture(
                device,
                info,
                ResourceDesc::Texture1d(final_desc),
                (
                    final_desc.bind_flags,
                    final_desc.usage,
                    final_desc.misc_flags,
                ),
                handles,
                riid,
                out,
            )
        }
        D3D11_RESOURCE_DIMENSION_TEXTURE2D => {
            // SAFETY: `dimension == D3D11_RESOURCE_DIMENSION_TEXTURE2D` selects `desc.desc2d`.
            let d = unsafe { runtime_data.desc.desc2d };
            let mut final_desc = d;
            let mut info = zero_tex_info();
            if create_mtl_texture_descriptor_2d(
                &d,
                &mut final_desc,
                &mut info,
                |f| device_supports_family(device, f),
                true,
            ) < 0
            {
                return E_INVALIDARG;
            }
            finish_imported_shared_texture(
                device,
                info,
                ResourceDesc::Texture2d(final_desc),
                (
                    final_desc.bind_flags,
                    final_desc.usage,
                    final_desc.misc_flags,
                ),
                handles,
                riid,
                out,
            )
        }
        D3D11_RESOURCE_DIMENSION_TEXTURE3D => {
            // SAFETY: `dimension == D3D11_RESOURCE_DIMENSION_TEXTURE3D` selects `desc.desc3d`.
            let d = unsafe { runtime_data.desc.desc3d };
            let mut final_desc = d;
            let mut info = zero_tex_info();
            if create_mtl_texture_descriptor_3d(
                &d,
                &mut final_desc,
                &mut info,
                |f| device_supports_family(device, f),
                true,
            ) < 0
            {
                return E_INVALIDARG;
            }
            finish_imported_shared_texture(
                device,
                info,
                ResourceDesc::Texture3d(final_desc),
                (
                    final_desc.bind_flags,
                    final_desc.usage,
                    final_desc.misc_flags,
                ),
                handles,
                riid,
                out,
            )
        }
        _ => E_INVALIDARG,
    }
}

/// Imports a shared texture from a legacy global KMT handle (`ImportSharedTexture`).
pub fn import_shared_texture(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    import_shared_texture_inner(device, handle, riid, out)
}

fn import_shared_texture_inner(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = ptr::null_mut() };
    }
    let handle_u32 = u32::try_from((handle.expose_provenance() as u64) & 0xffff_ffff).unwrap_or(0);
    if (handle_u32 & 0xc000_0000) == 0 {
        crate::LOG.warn("ImportSharedTexture: Invalid shared handle type");
        return E_INVALIDARG;
    }
    if out.is_null() {
        return S_FALSE;
    }
    let kmt_device = kmt::device_local_kmt(device);
    let Some(runtime_data) = kmt::open_shared_from_global_handle(kmt_device, handle_u32) else {
        return E_INVALIDARG;
    };
    import_from_runtime_data(device, &runtime_data, riid, out)
}

/// Imports a shared texture from an NT handle (`ImportSharedTextureFromNtHandle`).
pub fn import_shared_texture_from_nt_handle(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    import_shared_texture_from_nt_handle_inner(device, handle, riid, out)
}

fn import_shared_texture_from_nt_handle_inner(
    device: *mut c_void,
    handle: *mut c_void,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` was checked non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if (handle.expose_provenance() & 0xc000_0000) != 0 {
        crate::LOG.warn("ImportSharedTextureFromNtHandle: Invalid shared handle type");
        return E_INVALIDARG;
    }
    if out.is_null() {
        return S_FALSE;
    }
    let kmt_device = kmt::device_local_kmt(device);
    let Some(runtime_data) = kmt::open_shared_from_nt_handle(kmt_device, handle) else {
        crate::LOG.warn(&format!(
            "ImportSharedTextureFromNtHandle: Failed to query resource: {:x}",
            handle.expose_provenance()
        ));
        return E_INVALIDARG;
    };
    import_from_runtime_data(device, &runtime_data, riid, out)
}

/// Imports a shared texture by object name (`ImportSharedTextureByName`).
pub fn import_shared_texture_by_name(
    device: *mut c_void,
    name: *const u16,
    access: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    import_shared_texture_by_name_inner(device, name, access, riid, out)
}

fn import_shared_texture_by_name_inner(
    device: *mut c_void,
    name: *const u16,
    access: u32,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    let nt_handle = kmt::open_nt_handle_from_name(name, access);
    if nt_handle.is_null() {
        if !name.is_null() {
            let mut len = 0usize;
            loop {
                // SAFETY: `name` is a NUL-terminated UTF-16 `LPCWSTR`.
                let p = unsafe { name.add(len) };
                // SAFETY: `p` points within the NUL-terminated UTF-16 string.
                let ch = unsafe { *p };
                if ch == 0 {
                    break;
                }
                len += 1;
            }
            // SAFETY: `name` has `len` valid `u16` units before NUL.
            let slice = unsafe { core::slice::from_raw_parts(name, len) };
            let name_str = String::from_utf16_lossy(slice);
            crate::LOG.warn(&format!(
                "ImportSharedTextureByName: Failed to open NT handle from name: {name_str}"
            ));
        }
        if !out.is_null() {
            // SAFETY: `out` was checked non-null.
            unsafe { *out = ptr::null_mut() };
        }
        return E_INVALIDARG;
    }
    let hr = import_shared_texture_from_nt_handle(device, nt_handle, riid, out);
    kmt::close_nt_handle(nt_handle);
    hr
}
