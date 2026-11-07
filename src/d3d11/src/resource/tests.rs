use core::{
    cell::Cell,
    fmt::Write,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::sync::Arc;

use dxmt_core::resource::{BufferAllocationFlags, TextureViewDescriptor};
use winemetal_sys::{
    Obj,
    wire::{
        WMTBufferInfo, WMTGPU_FAMILY_APPLE7, WMTGPU_FAMILY_APPLE8, WMTGPU_FAMILY_MAC1,
        WMTGPU_FAMILY_MAC2, WMTTextureInfo,
    },
};

use super::{dynamic, helper};
use crate::{
    abi::records::{
        D3d11BufferDesc, D3d11BufferUav, D3d11BufferexSrv, D3d11DepthStencilViewDesc,
        D3d11DepthStencilViewDescUnion, D3d11RenderTargetViewDesc, D3d11RenderTargetViewDesc1,
        D3d11RenderTargetViewDesc1Union, D3d11RenderTargetViewDescUnion,
        D3d11ShaderResourceViewDesc, D3d11ShaderResourceViewDesc1,
        D3d11ShaderResourceViewDesc1Union, D3d11ShaderResourceViewDescUnion, D3d11Tex1dArraySrv,
        D3d11Tex1dSrv, D3d11Tex2dArrayDsv, D3d11Tex2dArrayRtv1, D3d11Tex2dArraySrv1,
        D3d11Tex2dArrayUav1, D3d11Tex2dSrv1, D3d11Tex2dmsArraySrv, D3d11Tex3dRtv, D3d11Tex3dSrv,
        D3d11Tex3dUav, D3d11TexcubeArraySrv, D3d11TexcubeSrv, D3d11Texture1dDesc,
        D3d11Texture2dDesc, D3d11Texture2dDesc1, D3d11Texture3dDesc, D3d11Texture3dDesc1,
        D3d11UnorderedAccessViewDesc, D3d11UnorderedAccessViewDesc1,
        D3d11UnorderedAccessViewDesc1Union, D3d11UnorderedAccessViewDescUnion, DxgiSampleDesc,
    },
    com::{Hresult, S_OK},
    view::helper::{self as view_helper, RenderPassAttachmentDesc},
};

const GOLDEN: &str = include_str!("golden/descs.txt");

const DXGI_FORMAT_UNKNOWN: u32 = 0;
const DXGI_FORMAT_R16G16B16A16_FLOAT: u32 = 10;
const DXGI_FORMAT_D32_FLOAT_S8X24_UINT: u32 = 20;
const DXGI_FORMAT_R8G8B8A8_TYPELESS: u32 = 27;
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_R8G8B8A8_UNORM_SRGB: u32 = 29;
const DXGI_FORMAT_R16G16_FLOAT: u32 = 34;
const DXGI_FORMAT_R16G16_UNORM: u32 = 35;
const DXGI_FORMAT_R32_TYPELESS: u32 = 39;
const DXGI_FORMAT_D32_FLOAT: u32 = 40;
const DXGI_FORMAT_R32_FLOAT: u32 = 41;
const DXGI_FORMAT_R32_UINT: u32 = 42;
const DXGI_FORMAT_R32_SINT: u32 = 43;
const DXGI_FORMAT_R24G8_TYPELESS: u32 = 44;
const DXGI_FORMAT_D24_UNORM_S8_UINT: u32 = 45;
const DXGI_FORMAT_R16_TYPELESS: u32 = 53;
const DXGI_FORMAT_D16_UNORM: u32 = 55;
const DXGI_FORMAT_R16_UNORM: u32 = 56;
const DXGI_FORMAT_R8_UNORM: u32 = 61;
const DXGI_FORMAT_BC1_UNORM: u32 = 71;
const DXGI_FORMAT_BC3_UNORM: u32 = 77;
const DXGI_FORMAT_B8G8R8A8_UNORM_SRGB: u32 = 91;
const DXGI_FORMAT_BC7_TYPELESS: u32 = 97;
const DXGI_FORMAT_BC7_UNORM: u32 = 98;
const DXGI_FORMAT_R32G32B32A32_UINT: u32 = 3;
const DXGI_FORMAT_R32G32_UINT: u32 = 17;
const DXGI_FORMAT_R32G32_SINT: u32 = 18;

const D3D11_USAGE_DEFAULT: u32 = 0;
const D3D11_USAGE_IMMUTABLE: u32 = 1;
const D3D11_USAGE_DYNAMIC: u32 = 2;
const D3D11_USAGE_STAGING: u32 = 3;

const D3D11_BIND_CONSTANT_BUFFER: u32 = 0x4;
const D3D11_BIND_SHADER_RESOURCE: u32 = 0x8;
const D3D11_BIND_RENDER_TARGET: u32 = 0x20;
const D3D11_BIND_DEPTH_STENCIL: u32 = 0x40;
const D3D11_BIND_UNORDERED_ACCESS: u32 = 0x80;

const D3D11_CPU_ACCESS_WRITE: u32 = 0x10000;
const D3D11_CPU_ACCESS_READ: u32 = 0x20000;

const D3D11_RESOURCE_MISC_GENERATE_MIPS: u32 = 0x1;
const D3D11_RESOURCE_MISC_SHARED: u32 = 0x2;
const D3D11_RESOURCE_MISC_TEXTURECUBE: u32 = 0x4;
const D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS: u32 = 0x20;
const D3D11_RESOURCE_MISC_BUFFER_STRUCTURED: u32 = 0x40;
const D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX: u32 = 0x100;
const D3D11_RESOURCE_MISC_SHARED_NTHANDLE: u32 = 0x800;

const D3D11_TEXTURE_LAYOUT_UNDEFINED: u32 = 0;
const D3D11_TEXTURE_LAYOUT_ROW_MAJOR: u32 = 1;
const D3D11_TEXTURE_LAYOUT_64K_STANDARD_SWIZZLE: u32 = 2;

const D3D11_SRV_DIMENSION_BUFFER: u32 = 1;
const D3D11_SRV_DIMENSION_TEXTURE1D: u32 = 2;
const D3D11_SRV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
const D3D11_SRV_DIMENSION_TEXTURE2D: u32 = 4;
const D3D11_SRV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
const D3D11_SRV_DIMENSION_TEXTURE2DMS: u32 = 6;
const D3D11_SRV_DIMENSION_TEXTURE2DMSARRAY: u32 = 7;
const D3D11_SRV_DIMENSION_TEXTURE3D: u32 = 8;
const D3D11_SRV_DIMENSION_TEXTURECUBE: u32 = 9;
const D3D11_SRV_DIMENSION_TEXTURECUBEARRAY: u32 = 10;
const D3D11_SRV_DIMENSION_BUFFEREX: u32 = 11;

const D3D11_RTV_DIMENSION_BUFFER: u32 = 1;
const D3D11_RTV_DIMENSION_TEXTURE1D: u32 = 2;
const D3D11_RTV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
const D3D11_RTV_DIMENSION_TEXTURE2D: u32 = 4;
const D3D11_RTV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
const D3D11_RTV_DIMENSION_TEXTURE2DMS: u32 = 6;
const D3D11_RTV_DIMENSION_TEXTURE2DMSARRAY: u32 = 7;
const D3D11_RTV_DIMENSION_TEXTURE3D: u32 = 8;

const D3D11_DSV_DIMENSION_TEXTURE1D: u32 = 1;
const D3D11_DSV_DIMENSION_TEXTURE1DARRAY: u32 = 2;
const D3D11_DSV_DIMENSION_TEXTURE2D: u32 = 3;
const D3D11_DSV_DIMENSION_TEXTURE2DARRAY: u32 = 4;
const D3D11_DSV_DIMENSION_TEXTURE2DMS: u32 = 5;
const D3D11_DSV_DIMENSION_TEXTURE2DMSARRAY: u32 = 6;

const D3D11_UAV_DIMENSION_BUFFER: u32 = 1;
const D3D11_UAV_DIMENSION_TEXTURE1D: u32 = 2;
const D3D11_UAV_DIMENSION_TEXTURE1DARRAY: u32 = 3;
const D3D11_UAV_DIMENSION_TEXTURE2D: u32 = 4;
const D3D11_UAV_DIMENSION_TEXTURE2DARRAY: u32 = 5;
const D3D11_UAV_DIMENSION_TEXTURE3D: u32 = 8;

const D3D11_BUFFER_UAV_FLAG_RAW: u32 = 0x1;
const D3D11_DSV_READ_ONLY_DEPTH: u32 = 0x1;

thread_local! {
    static MADE_BUFFERS: Cell<u64> = const { Cell::new(0) };
    static THREAD_TAG: Cell<usize> = const { Cell::new(0) };
}

static LAST_NEW_BUFFER_THREAD: AtomicUsize = AtomicUsize::new(0);
const FIRST_BUFFER: u64 = 0xc0_0000;

#[unsafe(export_name = "NSObject_retain")]
extern "C-unwind" fn fake_retain(_object: u64) {}

#[unsafe(export_name = "NSObject_release")]
extern "C-unwind" fn fake_release(_object: u64) {}

#[unsafe(export_name = "MTLDevice_newBuffer")]
extern "C-unwind" fn fake_new_buffer(_device: u64, info: *mut WMTBufferInfo) -> u64 {
    let tid = THREAD_TAG.with(Cell::get);
    LAST_NEW_BUFFER_THREAD.store(tid, Ordering::SeqCst);
    let n = MADE_BUFFERS.with(|m| {
        let next = m.get() + 1;
        m.set(next);
        next
    });
    // SAFETY: `info` is the buffer creation struct passed by `BufferAllocation::new`.
    let info = unsafe { &mut *info };
    info.gpu_address = n << 32;
    if info.memory.ptr == 0 {
        if tid == 0 && info.length > 0 {
            let len = usize::try_from(info.length).unwrap_or(64).max(64);
            let mem = Box::leak(vec![0u8; len].into_boxed_slice());
            info.memory.ptr = mem.as_mut_ptr() as usize as u64;
        } else {
            info.memory.ptr = n * 0x1_0000;
        }
    }
    FIRST_BUFFER + n
}

const fn supports_family(max_family: u32, family: u32) -> bool {
    if family >= WMTGPU_FAMILY_MAC1 && family <= WMTGPU_FAMILY_MAC2 {
        return true;
    }
    family <= max_family
}

const fn zero_srv1() -> D3d11ShaderResourceViewDesc1 {
    D3d11ShaderResourceViewDesc1 {
        format: 0,
        view_dimension: 0,
        u: D3d11ShaderResourceViewDesc1Union {
            texture2d: D3d11Tex2dSrv1 {
                most_detailed_mip: 0,
                mip_levels: 0,
                plane_slice: 0,
            },
        },
    }
}

const fn zero_uav1() -> D3d11UnorderedAccessViewDesc1 {
    D3d11UnorderedAccessViewDesc1 {
        format: 0,
        view_dimension: 0,
        u: D3d11UnorderedAccessViewDesc1Union {
            buffer: D3d11BufferUav {
                first_element: 0,
                num_elements: 0,
                flags: 0,
            },
        },
    }
}

const fn zero_rtv1() -> D3d11RenderTargetViewDesc1 {
    D3d11RenderTargetViewDesc1 {
        format: 0,
        view_dimension: 0,
        u: D3d11RenderTargetViewDesc1Union {
            texture3d: D3d11Tex3dRtv {
                mip_slice: 0,
                first_w_slice: 0,
                w_size: 0,
            },
        },
    }
}

const fn zero_dsv() -> D3d11DepthStencilViewDesc {
    D3d11DepthStencilViewDesc {
        format: 0,
        view_dimension: 0,
        flags: 0,
        u: D3d11DepthStencilViewDescUnion {
            texture2d_array: D3d11Tex2dArrayDsv {
                mip_slice: 0,
                first_array_slice: 0,
                array_size: 0,
            },
        },
    }
}

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

fn print_srv1(
    out: &mut String,
    prefix: &str,
    case_id: usize,
    hr: Hresult,
    d: &D3d11ShaderResourceViewDesc1,
) {
    let _ = write!(
        out,
        "{prefix} case={case_id} hr=0x{:08x} fmt={} dim={}",
        hr.cast_unsigned(),
        d.format,
        d.view_dimension
    );
    if hr < 0 {
        out.push('\n');
        return;
    }
    match d.view_dimension {
        D3D11_SRV_DIMENSION_BUFFER => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_BUFFER`.
            let b = unsafe { d.u.buffer };
            let (first, num) = (b.first_element, b.num_elements);
            let _ = writeln!(out, " buf={{first={first},num={num}}}");
        }
        D3D11_SRV_DIMENSION_BUFFEREX => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_BUFFEREX`.
            let b = unsafe { d.u.buffer_ex };
            let _ = writeln!(
                out,
                " bufex={{first={},num={},flags=0x{:x}}}",
                b.first_element, b.num_elements, b.flags
            );
        }
        D3D11_SRV_DIMENSION_TEXTURE1D => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE1D`.
            let t = unsafe { d.u.texture1d };
            let _ = writeln!(
                out,
                " tex1d={{most_mip={},mips={}}}",
                t.most_detailed_mip, t.mip_levels
            );
        }
        D3D11_SRV_DIMENSION_TEXTURE1DARRAY => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE1DARRAY`.
            let t = unsafe { d.u.texture1d_array };
            let _ = writeln!(
                out,
                " tex1darr={{most_mip={},mips={},first_slice={},arr_size={}}}",
                t.most_detailed_mip, t.mip_levels, t.first_array_slice, t.array_size
            );
        }
        D3D11_SRV_DIMENSION_TEXTURE2D => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2D`.
            let t = unsafe { d.u.texture2d };
            let _ = writeln!(
                out,
                " tex2d={{most_mip={},mips={},plane={}}}",
                t.most_detailed_mip, t.mip_levels, t.plane_slice
            );
        }
        D3D11_SRV_DIMENSION_TEXTURE2DARRAY => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2DARRAY`.
            let t = unsafe { d.u.texture2d_array };
            let _ = writeln!(
                out,
                " tex2darr={{most_mip={},mips={},first_slice={},arr_size={},plane={}}}",
                t.most_detailed_mip, t.mip_levels, t.first_array_slice, t.array_size, t.plane_slice
            );
        }
        D3D11_SRV_DIMENSION_TEXTURE2DMS => {
            out.push_str(" tex2dms={}\n");
        }
        D3D11_SRV_DIMENSION_TEXTURE2DMSARRAY => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE2DMSARRAY`.
            let t = unsafe { d.u.texture2dms_array };
            let _ = writeln!(
                out,
                " tex2dmsarr={{first_slice={},arr_size={}}}",
                t.first_array_slice, t.array_size
            );
        }
        D3D11_SRV_DIMENSION_TEXTURE3D => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURE3D`.
            let t = unsafe { d.u.texture3d };
            let _ = writeln!(
                out,
                " tex3d={{most_mip={},mips={}}}",
                t.most_detailed_mip, t.mip_levels
            );
        }
        D3D11_SRV_DIMENSION_TEXTURECUBE => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURECUBE`.
            let t = unsafe { d.u.texture_cube };
            let _ = writeln!(
                out,
                " texcube={{most_mip={},mips={}}}",
                t.most_detailed_mip, t.mip_levels
            );
        }
        D3D11_SRV_DIMENSION_TEXTURECUBEARRAY => {
            // SAFETY: union variant matches `D3D11_SRV_DIMENSION_TEXTURECUBEARRAY`.
            let t = unsafe { d.u.texture_cube_array };
            let _ = writeln!(
                out,
                " texcubearr={{most_mip={},mips={},first_face={},num_cubes={}}}",
                t.most_detailed_mip, t.mip_levels, t.first2d_array_face, t.num_cubes
            );
        }
        _ => out.push('\n'),
    }
}

fn print_uav1(
    out: &mut String,
    prefix: &str,
    case_id: usize,
    hr: Hresult,
    d: &D3d11UnorderedAccessViewDesc1,
) {
    let _ = write!(
        out,
        "{prefix} case={case_id} hr=0x{:08x} fmt={} dim={}",
        hr.cast_unsigned(),
        d.format,
        d.view_dimension
    );
    if hr < 0 {
        out.push('\n');
        return;
    }
    match d.view_dimension {
        D3D11_UAV_DIMENSION_BUFFER => {
            // SAFETY: union variant matches `D3D11_UAV_DIMENSION_BUFFER`.
            let b = unsafe { d.u.buffer };
            let _ = writeln!(
                out,
                " buf={{first={},num={},flags=0x{:x}}}",
                b.first_element, b.num_elements, b.flags
            );
        }
        D3D11_UAV_DIMENSION_TEXTURE1D => {
            // SAFETY: union variant matches `D3D11_UAV_DIMENSION_TEXTURE1D`.
            let t = unsafe { d.u.texture1d };
            let _ = writeln!(out, " tex1d={{mip={}}}", t.mip_slice);
        }
        D3D11_UAV_DIMENSION_TEXTURE1DARRAY => {
            // SAFETY: union variant matches `D3D11_UAV_DIMENSION_TEXTURE1DARRAY`.
            let t = unsafe { d.u.texture1d_array };
            let _ = writeln!(
                out,
                " tex1darr={{mip={},first_slice={},arr_size={}}}",
                t.mip_slice, t.first_array_slice, t.array_size
            );
        }
        D3D11_UAV_DIMENSION_TEXTURE2D => {
            // SAFETY: union variant matches `D3D11_UAV_DIMENSION_TEXTURE2D`.
            let t = unsafe { d.u.texture2d };
            let _ = writeln!(
                out,
                " tex2d={{mip={},plane={}}}",
                t.mip_slice, t.plane_slice
            );
        }
        D3D11_UAV_DIMENSION_TEXTURE2DARRAY => {
            // SAFETY: union variant matches `D3D11_UAV_DIMENSION_TEXTURE2DARRAY`.
            let t = unsafe { d.u.texture2d_array };
            let _ = writeln!(
                out,
                " tex2darr={{mip={},first_slice={},arr_size={},plane={}}}",
                t.mip_slice, t.first_array_slice, t.array_size, t.plane_slice
            );
        }
        D3D11_UAV_DIMENSION_TEXTURE3D => {
            // SAFETY: union variant matches `D3D11_UAV_DIMENSION_TEXTURE3D`.
            let t = unsafe { d.u.texture3d };
            let _ = writeln!(
                out,
                " tex3d={{mip={},first_w={},w_size={}}}",
                t.mip_slice, t.first_w_slice, t.w_size
            );
        }
        _ => out.push('\n'),
    }
}

fn print_rtv1(
    out: &mut String,
    prefix: &str,
    case_id: usize,
    hr: Hresult,
    d: &D3d11RenderTargetViewDesc1,
) {
    let _ = write!(
        out,
        "{prefix} case={case_id} hr=0x{:08x} fmt={} dim={}",
        hr.cast_unsigned(),
        d.format,
        d.view_dimension
    );
    if hr < 0 {
        out.push('\n');
        return;
    }
    match d.view_dimension {
        D3D11_RTV_DIMENSION_BUFFER => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_BUFFER`.
            let b = unsafe { d.u.buffer };
            let (first, num) = (b.first_element, b.num_elements);
            let _ = writeln!(out, " buf={{first={first},num={num}}}");
        }
        D3D11_RTV_DIMENSION_TEXTURE1D => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE1D`.
            let t = unsafe { d.u.texture1d };
            let _ = writeln!(out, " tex1d={{mip={}}}", t.mip_slice);
        }
        D3D11_RTV_DIMENSION_TEXTURE1DARRAY => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE1DARRAY`.
            let t = unsafe { d.u.texture1d_array };
            let _ = writeln!(
                out,
                " tex1darr={{mip={},first_slice={},arr_size={}}}",
                t.mip_slice, t.first_array_slice, t.array_size
            );
        }
        D3D11_RTV_DIMENSION_TEXTURE2D => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE2D`.
            let t = unsafe { d.u.texture2d };
            let _ = writeln!(
                out,
                " tex2d={{mip={},plane={}}}",
                t.mip_slice, t.plane_slice
            );
        }
        D3D11_RTV_DIMENSION_TEXTURE2DARRAY => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE2DARRAY`.
            let t = unsafe { d.u.texture2d_array };
            let _ = writeln!(
                out,
                " tex2darr={{mip={},first_slice={},arr_size={},plane={}}}",
                t.mip_slice, t.first_array_slice, t.array_size, t.plane_slice
            );
        }
        D3D11_RTV_DIMENSION_TEXTURE2DMS => {
            out.push_str(" tex2dms={}\n");
        }
        D3D11_RTV_DIMENSION_TEXTURE2DMSARRAY => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE2DMSARRAY`.
            let t = unsafe { d.u.texture2dms_array };
            let _ = writeln!(
                out,
                " tex2dmsarr={{first_slice={},arr_size={}}}",
                t.first_array_slice, t.array_size
            );
        }
        D3D11_RTV_DIMENSION_TEXTURE3D => {
            // SAFETY: union variant matches `D3D11_RTV_DIMENSION_TEXTURE3D`.
            let t = unsafe { d.u.texture3d };
            let _ = writeln!(
                out,
                " tex3d={{mip={},first_w={},w_size={}}}",
                t.mip_slice, t.first_w_slice, t.w_size
            );
        }
        _ => out.push('\n'),
    }
}

fn print_dsv(
    out: &mut String,
    prefix: &str,
    case_id: usize,
    hr: Hresult,
    d: &D3d11DepthStencilViewDesc,
) {
    let _ = write!(
        out,
        "{prefix} case={case_id} hr=0x{:08x} fmt={} dim={} flags=0x{:x}",
        hr.cast_unsigned(),
        d.format,
        d.view_dimension,
        d.flags
    );
    if hr < 0 {
        out.push('\n');
        return;
    }
    match d.view_dimension {
        D3D11_DSV_DIMENSION_TEXTURE1D => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE1D`.
            let t = unsafe { d.u.texture1d };
            let _ = writeln!(out, " tex1d={{mip={}}}", t.mip_slice);
        }
        D3D11_DSV_DIMENSION_TEXTURE1DARRAY => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE1DARRAY`.
            let t = unsafe { d.u.texture1d_array };
            let _ = writeln!(
                out,
                " tex1darr={{mip={},first_slice={},arr_size={}}}",
                t.mip_slice, t.first_array_slice, t.array_size
            );
        }
        D3D11_DSV_DIMENSION_TEXTURE2D => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE2D`.
            let t = unsafe { d.u.texture2d };
            let _ = writeln!(out, " tex2d={{mip={}}}", t.mip_slice);
        }
        D3D11_DSV_DIMENSION_TEXTURE2DARRAY => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE2DARRAY`.
            let t = unsafe { d.u.texture2d_array };
            let _ = writeln!(
                out,
                " tex2darr={{mip={},first_slice={},arr_size={}}}",
                t.mip_slice, t.first_array_slice, t.array_size
            );
        }
        D3D11_DSV_DIMENSION_TEXTURE2DMS => {
            out.push_str(" tex2dms={}\n");
        }
        D3D11_DSV_DIMENSION_TEXTURE2DMSARRAY => {
            // SAFETY: union variant matches `D3D11_DSV_DIMENSION_TEXTURE2DMSARRAY`.
            let t = unsafe { d.u.texture2dms_array };
            let _ = writeln!(
                out,
                " tex2dmsarr={{first_slice={},arr_size={}}}",
                t.first_array_slice, t.array_size
            );
        }
        _ => out.push('\n'),
    }
}

fn print_mtl_tex(
    out: &mut String,
    prefix: &str,
    case_id: usize,
    label: &str,
    hr: Hresult,
    out_mips: u32,
    info: &WMTTextureInfo,
) {
    let _ = write!(
        out,
        "{prefix} case={case_id} label={label} hr=0x{:08x}",
        hr.cast_unsigned()
    );
    if hr < 0 {
        out.push('\n');
        return;
    }
    let _ = writeln!(
        out,
        " out_mips={out_mips} type={} fmt={} w={} h={} d={} arr={} mips={} samples={} usage=0x{:x} options=0x{:x}",
        info.r#type(),
        info.pixel_format,
        info.width,
        info.height,
        info.depth,
        info.array_length,
        info.mipmap_level_count(),
        info.sample_count(),
        info.usage(),
        info.options
    );
}

fn print_mtl_view_desc(
    out: &mut String,
    prefix: &str,
    case_id: usize,
    hr: Hresult,
    m: &TextureViewDescriptor,
) {
    if hr < 0 {
        return;
    }
    let _ = writeln!(
        out,
        "{prefix}_MTL case={case_id} fmt={} type={} first_mip={} mips={} first_slice={} arr_size={}",
        m.format, m.texture_type, m.first_mip, m.mip_count, m.first_slice, m.array_size
    );
}

fn print_attachment_desc(
    out: &mut String,
    prefix: &str,
    case_id: usize,
    hr: Hresult,
    a: &RenderPassAttachmentDesc,
) {
    if hr < 0 {
        return;
    }
    let _ = writeln!(
        out,
        "{prefix}_ATTACH case={case_id} rt_arr_len={} samples={} depth_plane={} w={} h={}",
        a.render_target_array_length, a.sample_count, a.depth_plane, a.width, a.height
    );
}

fn generate_resource_descs_lines() -> String {
    let mut out = String::new();

    // 1. test_resource_upgrade_downgrade
    let t2d_cases = [
        D3d11Texture2dDesc {
            width: 256,
            height: 128,
            mip_levels: 4,
            array_size: 6,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            sample_desc: DxgiSampleDesc {
                count: 1,
                quality: 0,
            },
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
            cpu_access_flags: 0,
            misc_flags: D3D11_RESOURCE_MISC_TEXTURECUBE,
        },
        D3d11Texture2dDesc {
            width: 1024,
            height: 512,
            mip_levels: 1,
            array_size: 1,
            format: DXGI_FORMAT_D32_FLOAT,
            sample_desc: DxgiSampleDesc {
                count: 4,
                quality: 0,
            },
            usage: D3D11_USAGE_IMMUTABLE,
            bind_flags: D3D11_BIND_DEPTH_STENCIL,
            cpu_access_flags: D3D11_CPU_ACCESS_READ,
            misc_flags: 0,
        },
    ];
    for (i, c) in t2d_cases.iter().enumerate() {
        let mut up = helper::upgrade_texture2d_desc(c);
        up.texture_layout = if i == 0 {
            D3D11_TEXTURE_LAYOUT_UNDEFINED
        } else {
            D3D11_TEXTURE_LAYOUT_ROW_MAJOR
        };
        let down = helper::downgrade_texture2d_desc(&up);
        let eq = u32::from(
            down.width == c.width
                && down.height == c.height
                && down.mip_levels == c.mip_levels
                && down.array_size == c.array_size
                && down.format == c.format
                && down.sample_desc.count == c.sample_desc.count
                && down.sample_desc.quality == c.sample_desc.quality
                && down.usage == c.usage
                && down.bind_flags == c.bind_flags
                && down.cpu_access_flags == c.cpu_access_flags
                && down.misc_flags == c.misc_flags,
        );
        let _ = writeln!(
            out,
            "RES_UPDOWN_2D case={i} up={{w={},h={},mip={},arr={},fmt={},sc={},sq={},usage={},bind=0x{:x},cpu=0x{:x},misc=0x{:x},layout={}}} down_eq={eq}",
            up.width,
            up.height,
            up.mip_levels,
            up.array_size,
            up.format,
            up.sample_desc.count,
            up.sample_desc.quality,
            up.usage,
            up.bind_flags,
            up.cpu_access_flags,
            up.misc_flags,
            up.texture_layout
        );
    }

    let t3d_cases = [
        D3d11Texture3dDesc {
            width: 64,
            height: 32,
            depth: 16,
            mip_levels: 5,
            format: DXGI_FORMAT_R16G16B16A16_FLOAT,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: 0,
        },
        D3d11Texture3dDesc {
            width: 16,
            height: 16,
            depth: 8,
            mip_levels: 1,
            format: DXGI_FORMAT_R8_UNORM,
            usage: D3D11_USAGE_DYNAMIC,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: D3D11_CPU_ACCESS_WRITE,
            misc_flags: 0,
        },
    ];
    for (i, c) in t3d_cases.iter().enumerate() {
        let mut up = helper::upgrade_texture3d_desc(c);
        up.texture_layout = if i == 0 {
            D3D11_TEXTURE_LAYOUT_UNDEFINED
        } else {
            D3D11_TEXTURE_LAYOUT_64K_STANDARD_SWIZZLE
        };
        let down = helper::downgrade_texture3d_desc(&up);
        let eq = u32::from(
            down.width == c.width
                && down.height == c.height
                && down.depth == c.depth
                && down.mip_levels == c.mip_levels
                && down.format == c.format
                && down.usage == c.usage
                && down.bind_flags == c.bind_flags
                && down.cpu_access_flags == c.cpu_access_flags
                && down.misc_flags == c.misc_flags,
        );
        let _ = writeln!(
            out,
            "RES_UPDOWN_3D case={i} up={{w={},h={},d={},mip={},fmt={},usage={},bind=0x{:x},cpu=0x{:x},misc=0x{:x},layout={}}} down_eq={eq}",
            up.width,
            up.height,
            up.depth,
            up.mip_levels,
            up.format,
            up.usage,
            up.bind_flags,
            up.cpu_access_flags,
            up.misc_flags,
            up.texture_layout
        );
    }

    // 2. test_create_mtl_texture_descriptor
    let cases_1d: [(&str, D3d11Texture1dDesc, bool); 17] = [
        (
            "1d_basic_mips0",
            D3d11Texture1dDesc {
                width: 128,
                mip_levels: 0,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_array_rt_srv",
            D3d11Texture1dDesc {
                width: 64,
                mip_levels: 3,
                array_size: 4,
                format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_GENERATE_MIPS,
            },
            false,
        ),
        (
            "1d_dynamic",
            D3d11Texture1dDesc {
                width: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32_FLOAT,
                usage: D3D11_USAGE_DYNAMIC,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: D3D11_CPU_ACCESS_WRITE,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_staging",
            D3d11Texture1dDesc {
                width: 32,
                mip_levels: 2,
                array_size: 2,
                format: DXGI_FORMAT_R8_UNORM,
                usage: D3D11_USAGE_STAGING,
                bind_flags: 0,
                cpu_access_flags: D3D11_CPU_ACCESS_READ | D3D11_CPU_ACCESS_WRITE,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_immutable",
            D3d11Texture1dDesc {
                width: 512,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32G32B32A32_UINT,
                usage: D3D11_USAGE_IMMUTABLE,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_uav_atomic_r32",
            D3d11Texture1dDesc {
                width: 64,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32_UINT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_uav_rg32_apple8",
            D3d11Texture1dDesc {
                width: 64,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32G32_UINT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_uav_rg32_apple7_no_atomic",
            D3d11Texture1dDesc {
                width: 64,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32G32_UINT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            true,
        ),
        (
            "1d_rg32_sint_uav_no_atomic",
            D3d11Texture1dDesc {
                width: 64,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32G32_SINT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_depth_d32",
            D3d11Texture1dDesc {
                width: 128,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_D32_FLOAT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_depth_r32_typeless",
            D3d11Texture1dDesc {
                width: 128,
                mip_levels: 1,
                array_size: 2,
                format: DXGI_FORMAT_R32_TYPELESS,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_zero_width_fail",
            D3d11Texture1dDesc {
                width: 0,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_zero_array_fail",
            D3d11Texture1dDesc {
                width: 16,
                mip_levels: 1,
                array_size: 0,
                format: DXGI_FORMAT_R8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_unknown_fmt_fail",
            D3d11Texture1dDesc {
                width: 16,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_UNKNOWN,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_bad_bind_cb_fail",
            D3d11Texture1dDesc {
                width: 16,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_CONSTANT_BUFFER,
                cpu_access_flags: 0,
                misc_flags: 0,
            },
            false,
        ),
        (
            "1d_shared_conflict_fail",
            D3d11Texture1dDesc {
                width: 16,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX,
            },
            false,
        ),
        (
            "1d_nthandle_alone_fail",
            D3d11Texture1dDesc {
                width: 16,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
            },
            false,
        ),
    ];
    for (i, (label, d, use_apple7)) in cases_1d.iter().enumerate() {
        let max_fam = if *use_apple7 {
            WMTGPU_FAMILY_APPLE7
        } else {
            WMTGPU_FAMILY_APPLE8
        };
        let mut out_desc = *d;
        let mut info = zero_tex_info();
        let hr = helper::create_mtl_texture_descriptor_1d(
            d,
            &mut out_desc,
            &mut info,
            |f| supports_family(max_fam, f),
            true,
        );
        print_mtl_tex(
            &mut out,
            "CREATE_MTL_TEX_1D",
            i,
            label,
            hr,
            out_desc.mip_levels,
            &info,
        );
    }

    let s1 = DxgiSampleDesc {
        count: 1,
        quality: 0,
    };
    let cases_2d: [(&str, D3d11Texture2dDesc1, bool); 24] = [
        (
            "2d_basic_mips0",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 128,
                mip_levels: 0,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_srgb_typeless",
            D3d11Texture2dDesc1 {
                width: 128,
                height: 128,
                mip_levels: 4,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_TYPELESS,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE
                    | D3D11_BIND_RENDER_TARGET
                    | D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_array",
            D3d11Texture2dDesc1 {
                width: 64,
                height: 64,
                mip_levels: 0,
                array_size: 5,
                format: DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_cube",
            D3d11Texture2dDesc1 {
                width: 64,
                height: 64,
                mip_levels: 0,
                array_size: 6,
                format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_TEXTURECUBE,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_cube_array",
            D3d11Texture2dDesc1 {
                width: 64,
                height: 64,
                mip_levels: 3,
                array_size: 12,
                format: DXGI_FORMAT_R16G16B16A16_FLOAT,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_TEXTURECUBE,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_msaa_4x",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: DxgiSampleDesc {
                    count: 4,
                    quality: 0,
                },
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_msaa_array_8x",
            D3d11Texture2dDesc1 {
                width: 128,
                height: 128,
                mip_levels: 1,
                array_size: 4,
                format: DXGI_FORMAT_R16G16_FLOAT,
                sample_desc: DxgiSampleDesc {
                    count: 8,
                    quality: 0,
                },
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_msaa_mips0",
            D3d11Texture2dDesc1 {
                width: 128,
                height: 128,
                mip_levels: 0,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: DxgiSampleDesc {
                    count: 4,
                    quality: 0,
                },
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_msaa_mips2_fail",
            D3d11Texture2dDesc1 {
                width: 128,
                height: 128,
                mip_levels: 2,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: DxgiSampleDesc {
                    count: 4,
                    quality: 0,
                },
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_msaa_bad_count_fail",
            D3d11Texture2dDesc1 {
                width: 128,
                height: 128,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: DxgiSampleDesc {
                    count: 3,
                    quality: 0,
                },
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_msaa_cube_fail",
            D3d11Texture2dDesc1 {
                width: 64,
                height: 64,
                mip_levels: 1,
                array_size: 6,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: DxgiSampleDesc {
                    count: 4,
                    quality: 0,
                },
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_TEXTURECUBE,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_depth_d24s8",
            D3d11Texture2dDesc1 {
                width: 512,
                height: 512,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_D24_UNORM_S8_UINT,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_depth_r24g8_typeless",
            D3d11Texture2dDesc1 {
                width: 512,
                height: 512,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R24G8_TYPELESS,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_depth_d32s8",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_D32_FLOAT_S8X24_UINT,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_depth_d16",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_D16_UNORM,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_depth_r16_typeless_ds",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R16_TYPELESS,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_r16_typeless_no_ds",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R16_TYPELESS,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_bc1_srv",
            D3d11Texture2dDesc1 {
                width: 128,
                height: 128,
                mip_levels: 0,
                array_size: 1,
                format: DXGI_FORMAT_BC1_UNORM,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_bc7_srgb_typeless",
            D3d11Texture2dDesc1 {
                width: 128,
                height: 128,
                mip_levels: 4,
                array_size: 1,
                format: DXGI_FORMAT_BC7_TYPELESS,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_shared_nthandle",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_SHARED | D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_keyedmutex_nthandle",
            D3d11Texture2dDesc1 {
                width: 256,
                height: 256,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX
                    | D3D11_RESOURCE_MISC_SHARED_NTHANDLE,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_r32_sint_uav",
            D3d11Texture2dDesc1 {
                width: 64,
                height: 64,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32_SINT,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_rg32_uint_apple8",
            D3d11Texture2dDesc1 {
                width: 64,
                height: 64,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32G32_UINT,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "2d_rg32_uint_apple7",
            D3d11Texture2dDesc1 {
                width: 64,
                height: 64,
                mip_levels: 1,
                array_size: 1,
                format: DXGI_FORMAT_R32G32_UINT,
                sample_desc: s1,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            true,
        ),
    ];
    for (i, (label, d, use_apple7)) in cases_2d.iter().enumerate() {
        let max_fam = if *use_apple7 {
            WMTGPU_FAMILY_APPLE7
        } else {
            WMTGPU_FAMILY_APPLE8
        };
        let mut out_desc = *d;
        let mut info = zero_tex_info();
        let hr = helper::create_mtl_texture_descriptor_2d(
            d,
            &mut out_desc,
            &mut info,
            |f| supports_family(max_fam, f),
            true,
        );
        print_mtl_tex(
            &mut out,
            "CREATE_MTL_TEX_2D",
            i,
            label,
            hr,
            out_desc.mip_levels,
            &info,
        );
    }

    let cases_3d: [(&str, D3d11Texture3dDesc1, bool); 6] = [
        (
            "3d_basic_mips0",
            D3d11Texture3dDesc1 {
                width: 64,
                height: 32,
                depth: 16,
                mip_levels: 0,
                format: DXGI_FORMAT_R8G8B8A8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE
                    | D3D11_BIND_RENDER_TARGET
                    | D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "3d_bc1",
            D3d11Texture3dDesc1 {
                width: 32,
                height: 32,
                depth: 8,
                mip_levels: 3,
                format: DXGI_FORMAT_BC1_UNORM,
                usage: D3D11_USAGE_IMMUTABLE,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "3d_rg32_apple8",
            D3d11Texture3dDesc1 {
                width: 16,
                height: 16,
                depth: 4,
                mip_levels: 1,
                format: DXGI_FORMAT_R32G32_UINT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "3d_rg32_apple7",
            D3d11Texture3dDesc1 {
                width: 16,
                height: 16,
                depth: 4,
                mip_levels: 1,
                format: DXGI_FORMAT_R32G32_UINT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_UNORDERED_ACCESS,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            true,
        ),
        (
            "3d_depth_bind_fail",
            D3d11Texture3dDesc1 {
                width: 16,
                height: 16,
                depth: 4,
                mip_levels: 1,
                format: DXGI_FORMAT_D32_FLOAT,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_DEPTH_STENCIL,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
        (
            "3d_zero_depth_fail",
            D3d11Texture3dDesc1 {
                width: 16,
                height: 16,
                depth: 0,
                mip_levels: 1,
                format: DXGI_FORMAT_R8_UNORM,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
            },
            false,
        ),
    ];
    for (i, (label, d, use_apple7)) in cases_3d.iter().enumerate() {
        let max_fam = if *use_apple7 {
            WMTGPU_FAMILY_APPLE7
        } else {
            WMTGPU_FAMILY_APPLE8
        };
        let mut out_desc = *d;
        let mut info = zero_tex_info();
        let hr = helper::create_mtl_texture_descriptor_3d(
            d,
            &mut out_desc,
            &mut info,
            |f| supports_family(max_fam, f),
            true,
        );
        print_mtl_tex(
            &mut out,
            "CREATE_MTL_TEX_3D",
            i,
            label,
            hr,
            out_desc.mip_levels,
            &info,
        );
    }

    // 3. test_mipmap_and_linear_layout
    let mip_1d = [
        D3d11Texture1dDesc {
            width: 133,
            mip_levels: 5,
            array_size: 1,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
        },
        D3d11Texture1dDesc {
            width: 50,
            mip_levels: 4,
            array_size: 2,
            format: DXGI_FORMAT_R8_UNORM,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
        },
        D3d11Texture1dDesc {
            width: 64,
            mip_levels: 1,
            array_size: 1,
            format: DXGI_FORMAT_BC1_UNORM,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
        },
    ];
    let mut case_id = 0usize;
    for d in &mip_1d {
        for lv in 0..d.mip_levels {
            let (mw, mh, md) = helper::get_mipmap_size_1d(d, lv);
            let (hr_al, (r_al, s_al, l_al)) =
                match helper::get_linear_texture_layout_1d(d, lv, true) {
                    Ok(v) => (S_OK, v),
                    Err(e) => (e, (0, 0, 0)),
                };
            let (hr_un, (r_un, s_un, l_un)) =
                match helper::get_linear_texture_layout_1d(d, lv, false) {
                    Ok(v) => (S_OK, v),
                    Err(e) => (e, (0, 0, 0)),
                };
            let _ = writeln!(
                out,
                "MIP_LINEAR_1D case={case_id} fmt={} level={lv} mip={{w={mw},h={mh},d={md}}} aligned={{hr=0x{:08x},row={r_al},slice={s_al},len={l_al}}} unaligned={{hr=0x{:08x},row={r_un},slice={s_un},len={l_un}}}",
                d.format,
                hr_al.cast_unsigned(),
                hr_un.cast_unsigned()
            );
            case_id += 1;
        }
    }

    let mip_2d = [
        D3d11Texture2dDesc1 {
            width: 133,
            height: 75,
            mip_levels: 5,
            array_size: 1,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 133,
            height: 75,
            mip_levels: 5,
            array_size: 1,
            format: DXGI_FORMAT_BC1_UNORM,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 133,
            height: 75,
            mip_levels: 5,
            array_size: 1,
            format: DXGI_FORMAT_BC3_UNORM,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 64,
            height: 64,
            mip_levels: 2,
            array_size: 1,
            format: DXGI_FORMAT_D24_UNORM_S8_UINT,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_DEPTH_STENCIL,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 64,
            height: 64,
            mip_levels: 2,
            array_size: 1,
            format: DXGI_FORMAT_D32_FLOAT_S8X24_UINT,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_DEPTH_STENCIL,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 64,
            height: 64,
            mip_levels: 1,
            array_size: 1,
            format: DXGI_FORMAT_UNKNOWN,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
    ];
    case_id = 0;
    for d in &mip_2d {
        for lv in 0..d.mip_levels {
            let (mw, mh, md) = helper::get_mipmap_size_2d(d, lv);
            let (hr_al, (r_al, s_al, l_al)) =
                match helper::get_linear_texture_layout_2d(d, lv, true) {
                    Ok(v) => (S_OK, v),
                    Err(e) => (e, (0, 0, 0)),
                };
            let (hr_un, (r_un, s_un, l_un)) =
                match helper::get_linear_texture_layout_2d(d, lv, false) {
                    Ok(v) => (S_OK, v),
                    Err(e) => (e, (0, 0, 0)),
                };
            let _ = writeln!(
                out,
                "MIP_LINEAR_2D case={case_id} fmt={} level={lv} mip={{w={mw},h={mh},d={md}}} aligned={{hr=0x{:08x},row={r_al},slice={s_al},len={l_al}}} unaligned={{hr=0x{:08x},row={r_un},slice={s_un},len={l_un}}}",
                d.format,
                hr_al.cast_unsigned(),
                hr_un.cast_unsigned()
            );
            case_id += 1;
        }
    }

    let mip_3d = [
        D3d11Texture3dDesc1 {
            width: 35,
            height: 19,
            depth: 9,
            mip_levels: 4,
            format: DXGI_FORMAT_R16G16_UNORM,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture3dDesc1 {
            width: 35,
            height: 19,
            depth: 9,
            mip_levels: 4,
            format: DXGI_FORMAT_BC7_UNORM,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
    ];
    case_id = 0;
    for d in &mip_3d {
        for lv in 0..d.mip_levels {
            let (mw, mh, md) = helper::get_mipmap_size_3d(d, lv);
            let (hr_al, (r_al, s_al, l_al)) =
                match helper::get_linear_texture_layout_3d(d, lv, true) {
                    Ok(v) => (S_OK, v),
                    Err(e) => (e, (0, 0, 0)),
                };
            let (hr_un, (r_un, s_un, l_un)) =
                match helper::get_linear_texture_layout_3d(d, lv, false) {
                    Ok(v) => (S_OK, v),
                    Err(e) => (e, (0, 0, 0)),
                };
            let _ = writeln!(
                out,
                "MIP_LINEAR_3D case={case_id} fmt={} level={lv} mip={{w={mw},h={mh},d={md}}} aligned={{hr=0x{:08x},row={r_al},slice={s_al},len={l_al}}} unaligned={{hr=0x{:08x},row={r_un},slice={s_un},len={l_un}}}",
                d.format,
                hr_al.cast_unsigned(),
                hr_un.cast_unsigned()
            );
            case_id += 1;
        }
    }

    // 4. test_calculate_buffer_view
    let buf_cases: [(D3d11BufferDesc, u32, u32, u32); 5] = [
        (
            D3d11BufferDesc {
                byte_width: 256,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED,
                structure_byte_stride: 16,
            },
            4,
            0,
            16,
        ),
        (
            D3d11BufferDesc {
                byte_width: 256,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED,
                structure_byte_stride: 16,
            },
            4,
            4,
            8,
        ),
        (
            D3d11BufferDesc {
                byte_width: 256,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                structure_byte_stride: 0,
            },
            4,
            10,
            20,
        ),
        (
            D3d11BufferDesc {
                byte_width: 256,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED,
                structure_byte_stride: 16,
            },
            16,
            12,
            5,
        ),
        (
            D3d11BufferDesc {
                byte_width: 256,
                usage: D3D11_USAGE_DEFAULT,
                bind_flags: D3D11_BIND_SHADER_RESOURCE,
                cpu_access_flags: 0,
                misc_flags: 0,
                structure_byte_stride: 0,
            },
            4,
            0,
            0,
        ),
    ];
    for (i, (desc, stride, first_elem, num_elems)) in buf_cases.iter().enumerate() {
        let (offset, size) =
            helper::calculate_buffer_view_offset_and_size(desc, *stride, *first_elem, *num_elems);
        let _ = writeln!(out, "CALC_BUF_VIEW case={i} offset={offset} size={size}");
    }

    out
}

fn run_srv_1d(
    out: &mut String,
    srv_case: &mut usize,
    res: &D3d11Texture1dDesc,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) {
    let mut res_out = *res;
    let mut info = zero_tex_info();
    let _ = helper::create_mtl_texture_descriptor_1d(
        res,
        &mut res_out,
        &mut info,
        |f| supports_family(WMTGPU_FAMILY_APPLE8, f),
        true,
    );
    let (hr, norm, mtl) = match helper::extract_entire_srv_1d(&res_out, view_in) {
        Ok(mut norm) => match view_helper::initialize_and_normalize_srv_descriptor(
            res_out.mip_levels,
            res_out.array_size,
            &info,
            &mut norm,
        ) {
            Ok(mtl) => (S_OK, norm, mtl),
            Err(e) => (
                e,
                norm,
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        },
        Err(e) => (
            e,
            zero_srv1(),
            TextureViewDescriptor {
                format: 0,
                texture_type: 0,
                first_mip: 0,
                mip_count: 0,
                first_slice: 0,
                array_size: 0,
            },
        ),
    };
    print_srv1(out, "NORM_VIEW_SRV", *srv_case, hr, &norm);
    print_mtl_view_desc(out, "NORM_VIEW_SRV", *srv_case, hr, &mtl);
    *srv_case += 1;
}

fn run_srv_2d(
    out: &mut String,
    srv_case: &mut usize,
    res: &D3d11Texture2dDesc1,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) {
    let mut res_out = *res;
    let mut info = zero_tex_info();
    let _ = helper::create_mtl_texture_descriptor_2d(
        res,
        &mut res_out,
        &mut info,
        |f| supports_family(WMTGPU_FAMILY_APPLE8, f),
        true,
    );
    let (hr, norm, mtl) = match helper::extract_entire_srv_2d(&res_out, view_in) {
        Ok(mut norm) => match view_helper::initialize_and_normalize_srv_descriptor(
            res_out.mip_levels,
            res_out.array_size,
            &info,
            &mut norm,
        ) {
            Ok(mtl) => (S_OK, norm, mtl),
            Err(e) => (
                e,
                norm,
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        },
        Err(e) => (
            e,
            zero_srv1(),
            TextureViewDescriptor {
                format: 0,
                texture_type: 0,
                first_mip: 0,
                mip_count: 0,
                first_slice: 0,
                array_size: 0,
            },
        ),
    };
    print_srv1(out, "NORM_VIEW_SRV", *srv_case, hr, &norm);
    print_mtl_view_desc(out, "NORM_VIEW_SRV", *srv_case, hr, &mtl);
    *srv_case += 1;
}

fn run_srv_3d(
    out: &mut String,
    srv_case: &mut usize,
    res: &D3d11Texture3dDesc1,
    view_in: Option<&D3d11ShaderResourceViewDesc1>,
) {
    let mut res_out = *res;
    let mut info = zero_tex_info();
    let _ = helper::create_mtl_texture_descriptor_3d(
        res,
        &mut res_out,
        &mut info,
        |f| supports_family(WMTGPU_FAMILY_APPLE8, f),
        true,
    );
    let (hr, norm, mtl) = match helper::extract_entire_srv_3d(&res_out, view_in) {
        Ok(mut norm) => match view_helper::initialize_and_normalize_srv_descriptor(
            res_out.mip_levels,
            res_out.depth,
            &info,
            &mut norm,
        ) {
            Ok(mtl) => (S_OK, norm, mtl),
            Err(e) => (
                e,
                norm,
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        },
        Err(e) => (
            e,
            zero_srv1(),
            TextureViewDescriptor {
                format: 0,
                texture_type: 0,
                first_mip: 0,
                mip_count: 0,
                first_slice: 0,
                array_size: 0,
            },
        ),
    };
    print_srv1(out, "NORM_VIEW_SRV", *srv_case, hr, &norm);
    print_mtl_view_desc(out, "NORM_VIEW_SRV", *srv_case, hr, &mtl);
    *srv_case += 1;
}

fn generate_default_views_lines() -> String {
    let mut out = String::new();

    // 1. test_view_upgrade_downgrade
    let srv_dims = [
        D3D11_SRV_DIMENSION_BUFFER,
        D3D11_SRV_DIMENSION_TEXTURE1D,
        D3D11_SRV_DIMENSION_TEXTURE1DARRAY,
        D3D11_SRV_DIMENSION_TEXTURE2D,
        D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
        D3D11_SRV_DIMENSION_TEXTURE2DMS,
        D3D11_SRV_DIMENSION_TEXTURE2DMSARRAY,
        D3D11_SRV_DIMENSION_TEXTURE3D,
        D3D11_SRV_DIMENSION_TEXTURECUBE,
        D3D11_SRV_DIMENSION_TEXTURECUBEARRAY,
        D3D11_SRV_DIMENSION_BUFFEREX,
    ];
    for (i, &dim) in srv_dims.iter().enumerate() {
        let v0 = D3d11ShaderResourceViewDesc {
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            view_dimension: dim,
            u: D3d11ShaderResourceViewDescUnion {
                texture_cube_array: D3d11TexcubeArraySrv {
                    most_detailed_mip: 1,
                    mip_levels: 3,
                    first2d_array_face: 2,
                    num_cubes: 4,
                },
            },
        };
        let v1 = view_helper::upgrade_srv_desc(&v0);
        let v0_back = view_helper::downgrade_srv_desc(&v1);
        print_srv1(&mut out, "VIEW_UPDOWN_SRV", i, S_OK, &v1);
        let rt_ok =
            u32::from(v0_back.view_dimension == v0.view_dimension && v0_back.format == v0.format);
        let _ = writeln!(out, "VIEW_UPDOWN_SRV_RT case={i} roundtrip_ok={rt_ok}");
    }

    let rtv_dims = [
        D3D11_RTV_DIMENSION_BUFFER,
        D3D11_RTV_DIMENSION_TEXTURE1D,
        D3D11_RTV_DIMENSION_TEXTURE1DARRAY,
        D3D11_RTV_DIMENSION_TEXTURE2D,
        D3D11_RTV_DIMENSION_TEXTURE2DARRAY,
        D3D11_RTV_DIMENSION_TEXTURE2DMS,
        D3D11_RTV_DIMENSION_TEXTURE2DMSARRAY,
        D3D11_RTV_DIMENSION_TEXTURE3D,
    ];
    for (i, &dim) in rtv_dims.iter().enumerate() {
        let v0 = D3d11RenderTargetViewDesc {
            format: DXGI_FORMAT_R16G16B16A16_FLOAT,
            view_dimension: dim,
            u: D3d11RenderTargetViewDescUnion {
                texture3d: D3d11Tex3dRtv {
                    mip_slice: 2,
                    first_w_slice: 3,
                    w_size: 5,
                },
            },
        };
        let v1 = view_helper::upgrade_rtv_desc(&v0);
        let v0_back = view_helper::downgrade_rtv_desc(&v1);
        print_rtv1(&mut out, "VIEW_UPDOWN_RTV", i, S_OK, &v1);
        let rt_ok =
            u32::from(v0_back.view_dimension == v0.view_dimension && v0_back.format == v0.format);
        let _ = writeln!(out, "VIEW_UPDOWN_RTV_RT case={i} roundtrip_ok={rt_ok}");
    }

    let uav_dims = [
        D3D11_UAV_DIMENSION_BUFFER,
        D3D11_UAV_DIMENSION_TEXTURE1D,
        D3D11_UAV_DIMENSION_TEXTURE1DARRAY,
        D3D11_UAV_DIMENSION_TEXTURE2D,
        D3D11_UAV_DIMENSION_TEXTURE2DARRAY,
        D3D11_UAV_DIMENSION_TEXTURE3D,
    ];
    for (i, &dim) in uav_dims.iter().enumerate() {
        let v0 = D3d11UnorderedAccessViewDesc {
            format: DXGI_FORMAT_R32_UINT,
            view_dimension: dim,
            u: D3d11UnorderedAccessViewDescUnion {
                buffer: D3d11BufferUav {
                    first_element: 10,
                    num_elements: 20,
                    flags: D3D11_BUFFER_UAV_FLAG_RAW,
                },
            },
        };
        let v1 = view_helper::upgrade_uav_desc(&v0);
        let v0_back = view_helper::downgrade_uav_desc(&v1);
        print_uav1(&mut out, "VIEW_UPDOWN_UAV", i, S_OK, &v1);
        let rt_ok =
            u32::from(v0_back.view_dimension == v0.view_dimension && v0_back.format == v0.format);
        let _ = writeln!(out, "VIEW_UPDOWN_UAV_RT case={i} roundtrip_ok={rt_ok}");
    }

    // 2. test_extract_default_views
    let bufs = [
        D3d11BufferDesc {
            byte_width: 1024,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED,
            structure_byte_stride: 16,
        },
        D3d11BufferDesc {
            byte_width: 256,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS,
            structure_byte_stride: 0,
        },
    ];
    for (i, b) in bufs.iter().enumerate() {
        let (hr_srv, srv) = match helper::extract_entire_srv_buffer(b, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_srv1()),
        };
        print_srv1(&mut out, "DEFAULT_VIEW_BUF_SRV", i, hr_srv, &srv);

        let (hr_uav, uav) = match helper::extract_entire_uav_buffer(b, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_uav1()),
        };
        print_uav1(&mut out, "DEFAULT_VIEW_BUF_UAV", i, hr_uav, &uav);

        let srv_in = D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R32_FLOAT,
            view_dimension: D3D11_SRV_DIMENSION_BUFFER,
            u: D3d11ShaderResourceViewDesc1Union {
                buffer_ex: D3d11BufferexSrv {
                    first_element: 2,
                    num_elements: 8,
                    flags: 0,
                },
            },
        };
        let (hr_srv3, srv3) = match helper::extract_entire_srv_buffer(b, Some(&srv_in)) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_srv1()),
        };
        print_srv1(&mut out, "EXTRACT_VIEW3_BUF_SRV", i, hr_srv3, &srv3);
    }

    let tex1ds = [
        D3d11Texture1dDesc {
            width: 128,
            mip_levels: 4,
            array_size: 1,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE
                | D3D11_BIND_RENDER_TARGET
                | D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: 0,
        },
        D3d11Texture1dDesc {
            width: 64,
            mip_levels: 3,
            array_size: 5,
            format: DXGI_FORMAT_D32_FLOAT,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_DEPTH_STENCIL,
            cpu_access_flags: 0,
            misc_flags: 0,
        },
    ];
    for (i, t) in tex1ds.iter().enumerate() {
        let (hr, srv) = match helper::extract_entire_srv_1d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_srv1()),
        };
        print_srv1(&mut out, "DEFAULT_VIEW_1D_SRV", i, hr, &srv);
        let (hr, uav) = match helper::extract_entire_uav_1d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_uav1()),
        };
        print_uav1(&mut out, "DEFAULT_VIEW_1D_UAV", i, hr, &uav);
        let (hr, rtv) = match helper::extract_entire_rtv_1d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_rtv1()),
        };
        print_rtv1(&mut out, "DEFAULT_VIEW_1D_RTV", i, hr, &rtv);
        let (hr, dsv) = match helper::extract_entire_dsv_1d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_dsv()),
        };
        print_dsv(&mut out, "DEFAULT_VIEW_1D_DSV", i, hr, &dsv);
    }

    let s1 = DxgiSampleDesc {
        count: 1,
        quality: 0,
    };
    let s4 = DxgiSampleDesc {
        count: 4,
        quality: 0,
    };
    let tex2ds = [
        D3d11Texture2dDesc1 {
            width: 256,
            height: 128,
            mip_levels: 4,
            array_size: 1,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE
                | D3D11_BIND_RENDER_TARGET
                | D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 256,
            height: 128,
            mip_levels: 4,
            array_size: 6,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE
                | D3D11_BIND_RENDER_TARGET
                | D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 128,
            height: 128,
            mip_levels: 4,
            array_size: 6,
            format: DXGI_FORMAT_R16G16B16A16_FLOAT,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE
                | D3D11_BIND_RENDER_TARGET
                | D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: D3D11_RESOURCE_MISC_TEXTURECUBE,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 128,
            height: 128,
            mip_levels: 4,
            array_size: 12,
            format: DXGI_FORMAT_R16G16B16A16_FLOAT,
            sample_desc: s1,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE
                | D3D11_BIND_RENDER_TARGET
                | D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: D3D11_RESOURCE_MISC_TEXTURECUBE,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 256,
            height: 256,
            mip_levels: 1,
            array_size: 1,
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            sample_desc: s4,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_SHADER_RESOURCE
                | D3D11_BIND_RENDER_TARGET
                | D3D11_BIND_DEPTH_STENCIL,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
        D3d11Texture2dDesc1 {
            width: 256,
            height: 256,
            mip_levels: 1,
            array_size: 3,
            format: DXGI_FORMAT_D32_FLOAT,
            sample_desc: s4,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
        },
    ];
    for (i, t) in tex2ds.iter().enumerate() {
        let (hr, srv) = match helper::extract_entire_srv_2d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_srv1()),
        };
        print_srv1(&mut out, "DEFAULT_VIEW_2D_SRV", i, hr, &srv);
        let (hr, mut uav) = match helper::extract_entire_uav_2d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => {
                let mut z = zero_uav1();
                z.format = t.format;
                (e, z)
            }
        };
        let _ = &mut uav;
        print_uav1(&mut out, "DEFAULT_VIEW_2D_UAV", i, hr, &uav);
        let (hr, rtv) = match helper::extract_entire_rtv_2d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_rtv1()),
        };
        print_rtv1(&mut out, "DEFAULT_VIEW_2D_RTV", i, hr, &rtv);
        let (hr, dsv) = match helper::extract_entire_dsv_2d(t, None) {
            Ok(v) => (S_OK, v),
            Err(e) => (e, zero_dsv()),
        };
        print_dsv(&mut out, "DEFAULT_VIEW_2D_DSV", i, hr, &dsv);
    }

    let t3d = D3d11Texture3dDesc1 {
        width: 64,
        height: 32,
        depth: 16,
        mip_levels: 4,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_SHADER_RESOURCE
            | D3D11_BIND_RENDER_TARGET
            | D3D11_BIND_UNORDERED_ACCESS,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
    };
    let (hr, srv) = match helper::extract_entire_srv_3d(&t3d, None) {
        Ok(v) => (S_OK, v),
        Err(e) => (e, zero_srv1()),
    };
    print_srv1(&mut out, "DEFAULT_VIEW_3D_SRV", 0, hr, &srv);
    let (hr, uav) = match helper::extract_entire_uav_3d(&t3d, None) {
        Ok(v) => (S_OK, v),
        Err(e) => (e, zero_uav1()),
    };
    print_uav1(&mut out, "DEFAULT_VIEW_3D_UAV", 0, hr, &uav);
    let (hr, rtv) = match helper::extract_entire_rtv_3d(&t3d, None) {
        Ok(v) => (S_OK, v),
        Err(e) => (e, zero_rtv1()),
    };
    print_rtv1(&mut out, "DEFAULT_VIEW_3D_RTV", 0, hr, &rtv);
    let (hr, dsv) = match helper::extract_entire_dsv_3d(&t3d, None) {
        Ok(v) => (S_OK, v),
        Err(e) => (e, zero_dsv()),
    };
    print_dsv(&mut out, "DEFAULT_VIEW_3D_DSV", 0, hr, &dsv);

    // 3. test_initialize_and_normalize_views
    let t1d_arr = D3d11Texture1dDesc {
        width: 128,
        mip_levels: 4,
        array_size: 6,
        format: DXGI_FORMAT_R8G8B8A8_TYPELESS,
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_SHADER_RESOURCE
            | D3D11_BIND_RENDER_TARGET
            | D3D11_BIND_UNORDERED_ACCESS,
        cpu_access_flags: 0,
        misc_flags: 0,
    };
    let t2d_arr = D3d11Texture2dDesc1 {
        width: 256,
        height: 128,
        mip_levels: 4,
        array_size: 6,
        format: DXGI_FORMAT_R8G8B8A8_TYPELESS,
        sample_desc: s1,
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_SHADER_RESOURCE
            | D3D11_BIND_RENDER_TARGET
            | D3D11_BIND_UNORDERED_ACCESS,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
    };
    let t2d_cube = D3d11Texture2dDesc1 {
        width: 128,
        height: 128,
        mip_levels: 4,
        array_size: 12,
        format: DXGI_FORMAT_R16G16B16A16_FLOAT,
        sample_desc: s1,
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
        cpu_access_flags: 0,
        misc_flags: D3D11_RESOURCE_MISC_TEXTURECUBE,
        texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
    };
    let t2d_ms = D3d11Texture2dDesc1 {
        width: 128,
        height: 128,
        mip_levels: 1,
        array_size: 4,
        format: DXGI_FORMAT_R8G8B8A8_UNORM,
        sample_desc: s4,
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
    };
    let t2d_d32 = D3d11Texture2dDesc1 {
        width: 256,
        height: 256,
        mip_levels: 2,
        array_size: 2,
        format: DXGI_FORMAT_R32_TYPELESS,
        sample_desc: s1,
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
    };
    let t2d_d16 = D3d11Texture2dDesc1 {
        width: 256,
        height: 256,
        mip_levels: 2,
        array_size: 1,
        format: DXGI_FORMAT_D16_UNORM,
        sample_desc: s1,
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: D3D11_TEXTURE_LAYOUT_UNDEFINED,
    };

    let mut srv_case = 0usize;

    run_srv_1d(
        &mut out,
        &mut srv_case,
        &t1d_arr,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE1DARRAY,
            u: D3d11ShaderResourceViewDesc1Union {
                texture1d_array: D3d11Tex1dArraySrv {
                    most_detailed_mip: 1,
                    mip_levels: !0,
                    first_array_slice: 2,
                    array_size: !0,
                },
            },
        }),
    );
    run_srv_1d(
        &mut out,
        &mut srv_case,
        &t1d_arr,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE1D,
            u: D3d11ShaderResourceViewDesc1Union {
                texture1d: D3d11Tex1dSrv {
                    most_detailed_mip: 2,
                    mip_levels: 2,
                },
            },
        }),
    );
    run_srv_1d(&mut out, &mut srv_case, &t1d_arr, None);

    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_arr,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2d_array: D3d11Tex2dArraySrv1 {
                    most_detailed_mip: 1,
                    mip_levels: !0,
                    first_array_slice: 1,
                    array_size: !0,
                    plane_slice: 0,
                },
            },
        }),
    );
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_arr,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2d: D3d11Tex2dSrv1 {
                    most_detailed_mip: 0,
                    mip_levels: 1,
                    plane_slice: 0,
                },
            },
        }),
    );
    run_srv_2d(&mut out, &mut srv_case, &t2d_cube, None);
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_cube,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURECUBE,
            u: D3d11ShaderResourceViewDesc1Union {
                texture_cube: D3d11TexcubeSrv {
                    most_detailed_mip: 1,
                    mip_levels: !0,
                },
            },
        }),
    );
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_cube,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURECUBEARRAY,
            u: D3d11ShaderResourceViewDesc1Union {
                texture_cube_array: D3d11TexcubeArraySrv {
                    most_detailed_mip: 0,
                    mip_levels: 2,
                    first2d_array_face: 6,
                    num_cubes: 1,
                },
            },
        }),
    );
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_arr,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURECUBE,
            u: D3d11ShaderResourceViewDesc1Union {
                texture_cube: D3d11TexcubeSrv {
                    most_detailed_mip: 0,
                    mip_levels: 1,
                },
            },
        }),
    );
    run_srv_2d(&mut out, &mut srv_case, &t2d_ms, None);
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_ms,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE2DMS,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2d: D3d11Tex2dSrv1 {
                    most_detailed_mip: 0,
                    mip_levels: 0,
                    plane_slice: 0,
                },
            },
        }),
    );
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_ms,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2d: D3d11Tex2dSrv1 {
                    most_detailed_mip: 0,
                    mip_levels: 1,
                    plane_slice: 0,
                },
            },
        }),
    );
    {
        let mut t2d_1mip = t2d_arr;
        t2d_1mip.mip_levels = 1;
        t2d_1mip.format = DXGI_FORMAT_R8G8B8A8_UNORM;
        run_srv_2d(
            &mut out,
            &mut srv_case,
            &t2d_1mip,
            Some(&D3d11ShaderResourceViewDesc1 {
                format: DXGI_FORMAT_UNKNOWN,
                view_dimension: D3D11_SRV_DIMENSION_TEXTURE2DMSARRAY,
                u: D3d11ShaderResourceViewDesc1Union {
                    texture2dms_array: D3d11Tex2dmsArraySrv {
                        first_array_slice: 1,
                        array_size: !0,
                    },
                },
            }),
        );
    }
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_d32,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R32_FLOAT,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2d_array: D3d11Tex2dArraySrv1 {
                    most_detailed_mip: 0,
                    mip_levels: !0,
                    first_array_slice: 0,
                    array_size: !0,
                    plane_slice: 0,
                },
            },
        }),
    );
    run_srv_2d(
        &mut out,
        &mut srv_case,
        &t2d_d16,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_R16_UNORM,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2d: D3d11Tex2dSrv1 {
                    most_detailed_mip: 0,
                    mip_levels: !0,
                    plane_slice: 0,
                },
            },
        }),
    );
    run_srv_3d(&mut out, &mut srv_case, &t3d, None);
    run_srv_3d(
        &mut out,
        &mut srv_case,
        &t3d,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE3D,
            u: D3d11ShaderResourceViewDesc1Union {
                texture3d: D3d11Tex3dSrv {
                    most_detailed_mip: 1,
                    mip_levels: 2,
                },
            },
        }),
    );
    run_srv_3d(
        &mut out,
        &mut srv_case,
        &t3d,
        Some(&D3d11ShaderResourceViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            u: D3d11ShaderResourceViewDesc1Union {
                texture2d: D3d11Tex2dSrv1 {
                    most_detailed_mip: 0,
                    mip_levels: 1,
                    plane_slice: 0,
                },
            },
        }),
    );

    // UAV normalization cases
    let mut t2d_arr_out = t2d_arr;
    let mut info_2d = zero_tex_info();
    let _ = helper::create_mtl_texture_descriptor_2d(
        &t2d_arr,
        &mut t2d_arr_out,
        &mut info_2d,
        |f| supports_family(WMTGPU_FAMILY_APPLE8, f),
        true,
    );
    let mut t3d_out = t3d;
    let mut info_3d = zero_tex_info();
    let _ = helper::create_mtl_texture_descriptor_3d(
        &t3d,
        &mut t3d_out,
        &mut info_3d,
        |f| supports_family(WMTGPU_FAMILY_APPLE8, f),
        true,
    );

    {
        let uav_in = D3d11UnorderedAccessViewDesc1 {
            format: DXGI_FORMAT_R8G8B8A8_UNORM,
            view_dimension: D3D11_UAV_DIMENSION_TEXTURE2DARRAY,
            u: D3d11UnorderedAccessViewDesc1Union {
                texture2d_array: D3d11Tex2dArrayUav1 {
                    mip_slice: 1,
                    first_array_slice: 2,
                    array_size: !0,
                    plane_slice: 0,
                },
            },
        };
        let (hr, norm, mtl) = match helper::extract_entire_uav_2d(&t2d_arr_out, Some(&uav_in)) {
            Ok(mut norm) => match view_helper::initialize_and_normalize_uav_descriptor(
                t2d_arr_out.mip_levels,
                t2d_arr_out.array_size,
                &info_2d,
                &mut norm,
            ) {
                Ok(mtl) => (S_OK, norm, mtl),
                Err(e) => (
                    e,
                    norm,
                    TextureViewDescriptor {
                        format: 0,
                        texture_type: 0,
                        first_mip: 0,
                        mip_count: 0,
                        first_slice: 0,
                        array_size: 0,
                    },
                ),
            },
            Err(e) => (
                e,
                zero_uav1(),
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        };
        print_uav1(&mut out, "NORM_VIEW_UAV", 0, hr, &norm);
        print_mtl_view_desc(&mut out, "NORM_VIEW_UAV", 0, hr, &mtl);
    }
    for (uav_case, (first_w, w_size)) in [(1usize, (0, !0)), (2usize, (2, 4))] {
        let uav_in = D3d11UnorderedAccessViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_UAV_DIMENSION_TEXTURE3D,
            u: D3d11UnorderedAccessViewDesc1Union {
                texture3d: D3d11Tex3dUav {
                    mip_slice: 1,
                    first_w_slice: first_w,
                    w_size,
                },
            },
        };
        let (hr, norm, mtl) = match helper::extract_entire_uav_3d(&t3d_out, Some(&uav_in)) {
            Ok(mut norm) => match view_helper::initialize_and_normalize_uav_descriptor(
                t3d_out.mip_levels,
                t3d_out.depth,
                &info_3d,
                &mut norm,
            ) {
                Ok(mtl) => (S_OK, norm, mtl),
                Err(e) => (
                    e,
                    norm,
                    TextureViewDescriptor {
                        format: 0,
                        texture_type: 0,
                        first_mip: 0,
                        mip_count: 0,
                        first_slice: 0,
                        array_size: 0,
                    },
                ),
            },
            Err(e) => (
                e,
                zero_uav1(),
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        };
        print_uav1(&mut out, "NORM_VIEW_UAV", uav_case, hr, &norm);
        print_mtl_view_desc(&mut out, "NORM_VIEW_UAV", uav_case, hr, &mtl);
    }

    // RTV normalization cases
    {
        let rtv_in = D3d11RenderTargetViewDesc1 {
            format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            view_dimension: D3D11_RTV_DIMENSION_TEXTURE2DARRAY,
            u: D3d11RenderTargetViewDesc1Union {
                texture2d_array: D3d11Tex2dArrayRtv1 {
                    mip_slice: 2,
                    first_array_slice: 1,
                    array_size: !0,
                    plane_slice: 0,
                },
            },
        };
        let (hr, norm, attach, mtl) =
            match helper::extract_entire_rtv_2d(&t2d_arr_out, Some(&rtv_in)) {
                Ok(mut norm) => match view_helper::initialize_and_normalize_rtv_descriptor(
                    t2d_arr_out.mip_levels,
                    t2d_arr_out.array_size,
                    &info_2d,
                    &mut norm,
                ) {
                    Ok((attach, mtl)) => (S_OK, norm, attach, mtl),
                    Err(e) => (
                        e,
                        norm,
                        RenderPassAttachmentDesc::default(),
                        TextureViewDescriptor {
                            format: 0,
                            texture_type: 0,
                            first_mip: 0,
                            mip_count: 0,
                            first_slice: 0,
                            array_size: 0,
                        },
                    ),
                },
                Err(e) => (
                    e,
                    zero_rtv1(),
                    RenderPassAttachmentDesc::default(),
                    TextureViewDescriptor {
                        format: 0,
                        texture_type: 0,
                        first_mip: 0,
                        mip_count: 0,
                        first_slice: 0,
                        array_size: 0,
                    },
                ),
            };
        print_rtv1(&mut out, "NORM_VIEW_RTV", 0, hr, &norm);
        print_mtl_view_desc(&mut out, "NORM_VIEW_RTV", 0, hr, &mtl);
        print_attachment_desc(&mut out, "NORM_VIEW_RTV", 0, hr, &attach);
    }
    for (rtv_case, (first_w, w_size)) in [(1usize, (2, 1)), (2usize, (0, !0))] {
        let rtv_in = D3d11RenderTargetViewDesc1 {
            format: DXGI_FORMAT_UNKNOWN,
            view_dimension: D3D11_RTV_DIMENSION_TEXTURE3D,
            u: D3d11RenderTargetViewDesc1Union {
                texture3d: D3d11Tex3dRtv {
                    mip_slice: 2,
                    first_w_slice: first_w,
                    w_size,
                },
            },
        };
        let (hr, norm, attach, mtl) = match helper::extract_entire_rtv_3d(&t3d_out, Some(&rtv_in)) {
            Ok(mut norm) => match view_helper::initialize_and_normalize_rtv_descriptor(
                t3d_out.mip_levels,
                t3d_out.depth,
                &info_3d,
                &mut norm,
            ) {
                Ok((attach, mtl)) => (S_OK, norm, attach, mtl),
                Err(e) => (
                    e,
                    norm,
                    RenderPassAttachmentDesc::default(),
                    TextureViewDescriptor {
                        format: 0,
                        texture_type: 0,
                        first_mip: 0,
                        mip_count: 0,
                        first_slice: 0,
                        array_size: 0,
                    },
                ),
            },
            Err(e) => (
                e,
                zero_rtv1(),
                RenderPassAttachmentDesc::default(),
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        };
        print_rtv1(&mut out, "NORM_VIEW_RTV", rtv_case, hr, &norm);
        print_mtl_view_desc(&mut out, "NORM_VIEW_RTV", rtv_case, hr, &mtl);
        print_attachment_desc(&mut out, "NORM_VIEW_RTV", rtv_case, hr, &attach);
    }

    // DSV normalization cases
    let mut t2d_d32_out = t2d_d32;
    let mut info_d32 = zero_tex_info();
    let _ = helper::create_mtl_texture_descriptor_2d(
        &t2d_d32,
        &mut t2d_d32_out,
        &mut info_d32,
        |f| supports_family(WMTGPU_FAMILY_APPLE8, f),
        true,
    );
    let mut t2d_d16_out = t2d_d16;
    let mut info_d16 = zero_tex_info();
    let _ = helper::create_mtl_texture_descriptor_2d(
        &t2d_d16,
        &mut t2d_d16_out,
        &mut info_d16,
        |f| supports_family(WMTGPU_FAMILY_APPLE8, f),
        true,
    );

    {
        let dsv_in = D3d11DepthStencilViewDesc {
            format: DXGI_FORMAT_D32_FLOAT,
            view_dimension: D3D11_DSV_DIMENSION_TEXTURE2DARRAY,
            flags: D3D11_DSV_READ_ONLY_DEPTH | 0x4,
            u: D3d11DepthStencilViewDescUnion {
                texture2d_array: D3d11Tex2dArrayDsv {
                    mip_slice: 1,
                    first_array_slice: 0,
                    array_size: !0,
                },
            },
        };
        let (hr, norm, attach, mtl) =
            match helper::extract_entire_dsv_2d(&t2d_d32_out, Some(&dsv_in)) {
                Ok(mut norm) => match view_helper::initialize_and_normalize_dsv_descriptor(
                    t2d_d32_out.mip_levels,
                    t2d_d32_out.array_size,
                    &info_d32,
                    &mut norm,
                ) {
                    Ok((attach, mtl)) => (S_OK, norm, attach, mtl),
                    Err(e) => (
                        e,
                        norm,
                        RenderPassAttachmentDesc::default(),
                        TextureViewDescriptor {
                            format: 0,
                            texture_type: 0,
                            first_mip: 0,
                            mip_count: 0,
                            first_slice: 0,
                            array_size: 0,
                        },
                    ),
                },
                Err(e) => (
                    e,
                    zero_dsv(),
                    RenderPassAttachmentDesc::default(),
                    TextureViewDescriptor {
                        format: 0,
                        texture_type: 0,
                        first_mip: 0,
                        mip_count: 0,
                        first_slice: 0,
                        array_size: 0,
                    },
                ),
            };
        print_dsv(&mut out, "NORM_VIEW_DSV", 0, hr, &norm);
        print_mtl_view_desc(&mut out, "NORM_VIEW_DSV", 0, hr, &mtl);
        print_attachment_desc(&mut out, "NORM_VIEW_DSV", 0, hr, &attach);
    }
    {
        let (hr, norm, attach, mtl) = match helper::extract_entire_dsv_2d(&t2d_d16_out, None) {
            Ok(mut norm) => match view_helper::initialize_and_normalize_dsv_descriptor(
                t2d_d16_out.mip_levels,
                t2d_d16_out.array_size,
                &info_d16,
                &mut norm,
            ) {
                Ok((attach, mtl)) => (S_OK, norm, attach, mtl),
                Err(e) => (
                    e,
                    norm,
                    RenderPassAttachmentDesc::default(),
                    TextureViewDescriptor {
                        format: 0,
                        texture_type: 0,
                        first_mip: 0,
                        mip_count: 0,
                        first_slice: 0,
                        array_size: 0,
                    },
                ),
            },
            Err(e) => (
                e,
                zero_dsv(),
                RenderPassAttachmentDesc::default(),
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        };
        print_dsv(&mut out, "NORM_VIEW_DSV", 1, hr, &norm);
        print_mtl_view_desc(&mut out, "NORM_VIEW_DSV", 1, hr, &mtl);
        print_attachment_desc(&mut out, "NORM_VIEW_DSV", 1, hr, &attach);
    }
    {
        let (hr, norm, attach, mtl) = match helper::extract_entire_dsv_3d(&t3d_out, None) {
            Ok(mut norm) => match view_helper::initialize_and_normalize_dsv_descriptor(
                t3d_out.mip_levels,
                t3d_out.depth,
                &info_3d,
                &mut norm,
            ) {
                Ok((attach, mtl)) => (S_OK, norm, attach, mtl),
                Err(e) => (
                    e,
                    norm,
                    RenderPassAttachmentDesc::default(),
                    TextureViewDescriptor {
                        format: 0,
                        texture_type: 0,
                        first_mip: 0,
                        mip_count: 0,
                        first_slice: 0,
                        array_size: 0,
                    },
                ),
            },
            Err(e) => (
                e,
                zero_dsv(),
                RenderPassAttachmentDesc::default(),
                TextureViewDescriptor {
                    format: 0,
                    texture_type: 0,
                    first_mip: 0,
                    mip_count: 0,
                    first_slice: 0,
                    array_size: 0,
                },
            ),
        };
        print_dsv(&mut out, "NORM_VIEW_DSV", 2, hr, &norm);
        print_mtl_view_desc(&mut out, "NORM_VIEW_DSV", 2, hr, &mtl);
        print_attachment_desc(&mut out, "NORM_VIEW_DSV", 2, hr, &attach);
    }

    out
}

fn is_resource_desc_line(line: &str) -> bool {
    line.starts_with("RES_UPDOWN_")
        || line.starts_with("CREATE_MTL_TEX_")
        || line.starts_with("MIP_LINEAR_")
        || line.starts_with("CALC_BUF_VIEW")
}

#[test]
fn descs_validate_as_the_golden() {
    let want: String =
        GOLDEN
            .lines()
            .filter(|l| is_resource_desc_line(l))
            .fold(String::new(), |mut acc, l| {
                acc.push_str(l);
                acc.push('\n');
                acc
            });
    let got = generate_resource_descs_lines();
    assert_eq!(got, want);
}

#[test]
fn default_views_as_the_golden() {
    let want: String =
        GOLDEN
            .lines()
            .filter(|l| !is_resource_desc_line(l))
            .fold(String::new(), |mut acc, l| {
                acc.push_str(l);
                acc.push('\n');
                acc
            });
    let got = generate_default_views_lines();
    assert_eq!(got, want);
}

#[test]
fn cpu_placement_per_target() {
    assert_eq!(
        helper::cpu_placement_flag(false).bits(),
        BufferAllocationFlags::empty().bits()
    );
    assert_eq!(
        helper::cpu_placement_flag(true).bits(),
        BufferAllocationFlags::CPU_PLACED.bits()
    );
}

#[test]
fn dynamic_buffer_renames_on_the_caller() {
    MADE_BUFFERS.with(|m| m.set(0));
    let caller_tid = 0x00ca_11e4_usize;
    THREAD_TAG.with(|t| t.set(caller_tid));
    // SAFETY: fake handle; `NSObject_retain` and `NSObject_release` are faked above.
    let device = unsafe { Obj::retain(0xde) }.expect("fake device");
    let flags = BufferAllocationFlags::CPU_WRITE_COMBINED
        | BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE;

    let (buffer, dyn_buf) =
        dynamic::allocate_dynamic_buffer(&device, 2048, flags).expect("dynamic buffer");
    assert_eq!(LAST_NEW_BUFFER_THREAD.load(Ordering::SeqCst), caller_tid);
    assert_eq!(MADE_BUFFERS.with(Cell::get), 1);
    let initial = dyn_buf.immediate_name();
    assert_eq!(initial.buffer(), FIRST_BUFFER + 1);
    assert_eq!(dyn_buf.immediate_suballocation(), 0);
    assert_eq!(
        dyn_buf.immediate_mapped_memory().expose_provenance(),
        0x1_0000
    );

    // First discard map steps to suballocation 1 in the same 4096-byte page without allocating.
    let (alloc1, sub1, ptr1, renamed1) = dynamic::discard_map_dynamic_buffer(&dyn_buf, 1, 0);
    assert!(!renamed1);
    assert!(Arc::ptr_eq(&alloc1, &initial));
    assert_eq!(sub1, 1);
    assert_eq!(dyn_buf.immediate_suballocation(), 1);
    assert_eq!(ptr1.expose_provenance(), 0x1_0000 + 2048);
    assert_eq!(MADE_BUFFERS.with(Cell::get), 1);

    // Second discard map exhausts the page and allocates a new buffer on the calling thread.
    LAST_NEW_BUFFER_THREAD.store(0, Ordering::SeqCst);
    let (alloc2, sub2, ptr2, renamed2) = dynamic::discard_map_dynamic_buffer(&dyn_buf, 5, 0);
    assert!(renamed2);
    assert_eq!(LAST_NEW_BUFFER_THREAD.load(Ordering::SeqCst), caller_tid);
    assert_eq!(MADE_BUFFERS.with(Cell::get), 2);
    assert_eq!(alloc2.buffer(), FIRST_BUFFER + 2);
    assert_eq!(sub2, 0);
    assert_eq!(ptr2.expose_provenance(), 0x2_0000);
    // The underlying `Buffer` keeps the old allocation until the encode closure renames it.
    assert_eq!(
        buffer.current().expect("current").buffer(),
        FIRST_BUFFER + 1
    );
}
