//! Goldens of texture descriptions and whole-resource views over a matrix of descriptions.
//!
//! Recorded from this module's own answers before their logic moved into `dxmt-core`, so a move
//! that changes one answer fails here.

use core::fmt::Write as _;

use dxmt_util::dxgi_consts::{
    D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_BIND_UNORDERED_ACCESS, D3D11_BIND_VERTEX_BUFFER, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_DYNAMIC, D3D11_USAGE_IMMUTABLE, D3D11_USAGE_STAGING,
};
use winemetal_sys::wire::{WMTGPU_FAMILY_APPLE8, WMTTextureInfo};

use super::{
    create_mtl_texture_descriptor_1d, create_mtl_texture_descriptor_2d,
    create_mtl_texture_descriptor_3d, extract_entire_dsv_1d, extract_entire_dsv_2d,
    extract_entire_dsv_3d, extract_entire_rtv_1d, extract_entire_rtv_2d, extract_entire_rtv_3d,
    extract_entire_srv_1d, extract_entire_srv_2d, extract_entire_srv_3d, extract_entire_uav_1d,
    extract_entire_uav_2d, extract_entire_uav_3d,
};
use crate::abi::records::{
    D3d11DepthStencilViewDesc, D3d11RenderTargetViewDesc1, D3d11ShaderResourceViewDesc1,
    D3d11Texture1dDesc, D3d11Texture2dDesc1, D3d11Texture3dDesc1, D3d11UnorderedAccessViewDesc1,
    DxgiSampleDesc,
};

const GOLDEN: &str = include_str!("golden/descs.txt");

const MISC_SHARED: u32 = 0x2;
const MISC_TEXTURECUBE: u32 = 0x4;
const MISC_SHARED_KEYEDMUTEX: u32 = 0x100;
const MISC_SHARED_NTHANDLE: u32 = 0x800;

const fn zero_info() -> WMTTextureInfo {
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

fn info_line(out: &mut String, hr: i32, info: &WMTTextureInfo, mips: u32) {
    let _ = writeln!(
        out,
        "hr=0x{:08x} pf={} {}x{}x{} arr={} bits=0x{:08x} opt=0x{:x} flags=0x{:x} mips={mips}",
        hr.cast_unsigned(),
        info.pixel_format,
        info.width,
        info.height,
        info.depth,
        info.array_length,
        info.bits,
        info.options,
        info.flags
    );
}

/// Formats: `RGBA8`, `R32Typeless`, `D32`, `R16Typeless`, `R32Uint`, `RG32Uint`, `R32Float`, `BC1`.
///
/// And one Metal lacks.
const FORMATS: [u32; 9] = [28, 39, 40, 53, 42, 17, 41, 71, 200];

fn tex2d(width: u32, height: u32, mips: u32, array: u32, samples: u32) -> D3d11Texture2dDesc1 {
    D3d11Texture2dDesc1 {
        width,
        height,
        mip_levels: mips,
        array_size: array,
        format: 28,
        sample_desc: DxgiSampleDesc {
            count: samples,
            quality: 0,
        },
        usage: D3D11_USAGE_DEFAULT,
        bind_flags: D3D11_BIND_SHADER_RESOURCE,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: 0,
    }
}

fn generate_texture_infos(out: &mut String) {
    let binds = [
        D3D11_BIND_SHADER_RESOURCE,
        D3D11_BIND_RENDER_TARGET,
        D3D11_BIND_DEPTH_STENCIL,
        D3D11_BIND_UNORDERED_ACCESS,
        D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_RENDER_TARGET,
        D3D11_BIND_VERTEX_BUFFER,
    ];
    let usages = [
        D3D11_USAGE_DEFAULT,
        D3D11_USAGE_IMMUTABLE,
        D3D11_USAGE_DYNAMIC,
        D3D11_USAGE_STAGING,
        7,
    ];
    let miscs = [
        0,
        MISC_SHARED,
        MISC_SHARED_KEYEDMUTEX,
        MISC_SHARED | MISC_SHARED_KEYEDMUTEX,
        MISC_SHARED_NTHANDLE,
    ];
    for (f, &format) in FORMATS.iter().enumerate() {
        for (b, &bind) in binds.iter().enumerate() {
            for apple8 in [false, true] {
                let mut d = tex2d(64, 32, 0, 1, 1);
                d.format = format;
                d.bind_flags = bind;
                let mut o = d;
                let mut info = zero_info();
                let hr = create_mtl_texture_descriptor_2d(
                    &d,
                    &mut o,
                    &mut info,
                    |fam| apple8 && fam <= WMTGPU_FAMILY_APPLE8,
                    true,
                );
                let _ = write!(out, "TEX2D f={f} b={b} a8={} ", u32::from(apple8));
                info_line(out, hr, &info, o.mip_levels);
            }
        }
    }
    for (u, &usage) in usages.iter().enumerate() {
        for (m, &misc) in miscs.iter().enumerate() {
            for bind in [D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_UNORDERED_ACCESS] {
                let mut d = tex2d(64, 32, 1, 1, 1);
                d.usage = usage;
                d.misc_flags = misc;
                d.bind_flags = bind;
                let mut o = d;
                let mut info = zero_info();
                let hr = create_mtl_texture_descriptor_2d(&d, &mut o, &mut info, |_| true, true);
                let _ = write!(out, "TEX2D u={u} m={m} bind=0x{bind:x} ");
                info_line(out, hr, &info, o.mip_levels);
            }
        }
    }
    let shapes = [
        (128, 64, 0, 1, 1, 0),
        (128, 64, 3, 4, 1, 0),
        (0, 64, 1, 1, 1, 0),
        (64, 64, 0, 6, 1, MISC_TEXTURECUBE),
        (64, 64, 0, 12, 1, MISC_TEXTURECUBE),
        (64, 64, 0, 7, 1, MISC_TEXTURECUBE),
        (64, 64, 1, 6, 4, MISC_TEXTURECUBE),
        (64, 32, 1, 1, 4, 0),
        (64, 32, 1, 3, 2, 0),
        (64, 32, 1, 1, 3, 0),
        (64, 32, 1, 1, 16, 0),
        (64, 32, 1, 1, 0, 0),
        (64, 32, 1, 0, 1, 0),
        (1, 1, 0, 1, 1, 0),
    ];
    for (s, &(w, h, mips, array, samples, misc)) in shapes.iter().enumerate() {
        let mut d = tex2d(w, h, mips, array, samples);
        d.misc_flags = misc;
        let mut o = d;
        let mut info = zero_info();
        let hr = create_mtl_texture_descriptor_2d(&d, &mut o, &mut info, |_| true, true);
        let _ = write!(out, "TEX2D s={s} ");
        info_line(out, hr, &info, o.mip_levels);
    }
    for (s, &(w, mips, array)) in [(64, 0, 1), (64, 3, 5), (0, 1, 1), (1000, 0, 2)]
        .iter()
        .enumerate()
    {
        let d = D3d11Texture1dDesc {
            width: w,
            mip_levels: mips,
            array_size: array,
            format: 28,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_UNORDERED_ACCESS,
            cpu_access_flags: 0,
            misc_flags: 0,
        };
        let mut o = d;
        let mut info = zero_info();
        let hr = create_mtl_texture_descriptor_1d(&d, &mut o, &mut info, |_| true, true);
        let _ = write!(out, "TEX1D s={s} ");
        info_line(out, hr, &info, o.mip_levels);
    }
    // A 3D texture's full mip count ignores its depth (d3d11's rule).
    for (s, &(w, h, depth, mips)) in [
        (32, 16, 8, 0),
        (4, 4, 64, 0),
        (32, 16, 8, 2),
        (32, 16, 0, 1),
    ]
    .iter()
    .enumerate()
    {
        let d = D3d11Texture3dDesc1 {
            width: w,
            height: h,
            depth,
            mip_levels: mips,
            format: 28,
            usage: D3D11_USAGE_DEFAULT,
            bind_flags: D3D11_BIND_RENDER_TARGET,
            cpu_access_flags: 0,
            misc_flags: 0,
            texture_layout: 0,
        };
        let mut o = d;
        let mut info = zero_info();
        let hr = create_mtl_texture_descriptor_3d(&d, &mut o, &mut info, |_| true, true);
        let _ = write!(out, "TEX3D s={s} ");
        info_line(out, hr, &info, o.mip_levels);
    }
}

fn hr_line(out: &mut String, hr: i32) {
    let _ = writeln!(out, "hr=0x{:08x}", hr.cast_unsigned());
}

/// Reads union member `$m` of `$v`, which the helper wrote for this dimension.
macro_rules! member {
    ($v:ident . $m:ident) => {{
        // SAFETY: the helper wrote the member of the dimension it set.
        unsafe { $v.u.$m }
    }};
}

/// The words of the member `view_dimension` selects.
fn srv_line(out: &mut String, r: Result<D3d11ShaderResourceViewDesc1, i32>) {
    let v = match r {
        Ok(v) => v,
        Err(hr) => return hr_line(out, hr),
    };
    let words: Vec<u32> = match v.view_dimension {
        1 => {
            let t = member!(v.buffer);
            vec![t.first_element, t.num_elements]
        }
        2 => {
            let t = member!(v.texture1d);
            vec![t.most_detailed_mip, t.mip_levels]
        }
        3 => {
            let t = member!(v.texture1d_array);
            vec![
                t.most_detailed_mip,
                t.mip_levels,
                t.first_array_slice,
                t.array_size,
            ]
        }
        4 => {
            let t = member!(v.texture2d);
            vec![t.most_detailed_mip, t.mip_levels, t.plane_slice]
        }
        5 => {
            let t = member!(v.texture2d_array);
            vec![
                t.most_detailed_mip,
                t.mip_levels,
                t.first_array_slice,
                t.array_size,
                t.plane_slice,
            ]
        }
        7 => {
            let t = member!(v.texture2dms_array);
            vec![t.first_array_slice, t.array_size]
        }
        8 => {
            let t = member!(v.texture3d);
            vec![t.most_detailed_mip, t.mip_levels]
        }
        9 => {
            let t = member!(v.texture_cube);
            vec![t.most_detailed_mip, t.mip_levels]
        }
        10 => {
            let t = member!(v.texture_cube_array);
            vec![
                t.most_detailed_mip,
                t.mip_levels,
                t.first2d_array_face,
                t.num_cubes,
            ]
        }
        _ => vec![],
    };
    let _ = writeln!(out, "fmt={} dim={} {words:?}", v.format, v.view_dimension);
}

fn uav_line(out: &mut String, r: Result<D3d11UnorderedAccessViewDesc1, i32>) {
    let v = match r {
        Ok(v) => v,
        Err(hr) => return hr_line(out, hr),
    };
    let words: Vec<u32> = match v.view_dimension {
        1 => {
            let t = member!(v.buffer);
            vec![t.first_element, t.num_elements, t.flags]
        }
        2 => vec![member!(v.texture1d).mip_slice],
        3 => {
            let t = member!(v.texture1d_array);
            vec![t.mip_slice, t.first_array_slice, t.array_size]
        }
        4 => {
            let t = member!(v.texture2d);
            vec![t.mip_slice, t.plane_slice]
        }
        5 => {
            let t = member!(v.texture2d_array);
            vec![
                t.mip_slice,
                t.first_array_slice,
                t.array_size,
                t.plane_slice,
            ]
        }
        8 => {
            let t = member!(v.texture3d);
            vec![t.mip_slice, t.first_w_slice, t.w_size]
        }
        _ => vec![],
    };
    let _ = writeln!(out, "fmt={} dim={} {words:?}", v.format, v.view_dimension);
}

fn rtv_line(out: &mut String, r: Result<D3d11RenderTargetViewDesc1, i32>) {
    let v = match r {
        Ok(v) => v,
        Err(hr) => return hr_line(out, hr),
    };
    let words: Vec<u32> = match v.view_dimension {
        2 => vec![member!(v.texture1d).mip_slice],
        3 => {
            let t = member!(v.texture1d_array);
            vec![t.mip_slice, t.first_array_slice, t.array_size]
        }
        4 => {
            let t = member!(v.texture2d);
            vec![t.mip_slice, t.plane_slice]
        }
        5 => {
            let t = member!(v.texture2d_array);
            vec![
                t.mip_slice,
                t.first_array_slice,
                t.array_size,
                t.plane_slice,
            ]
        }
        7 => {
            let t = member!(v.texture2dms_array);
            vec![t.first_array_slice, t.array_size]
        }
        8 => {
            let t = member!(v.texture3d);
            vec![t.mip_slice, t.first_w_slice, t.w_size]
        }
        _ => vec![],
    };
    let _ = writeln!(out, "fmt={} dim={} {words:?}", v.format, v.view_dimension);
}

fn dsv_line(out: &mut String, r: Result<D3d11DepthStencilViewDesc, i32>) {
    let v = match r {
        Ok(v) => v,
        Err(hr) => return hr_line(out, hr),
    };
    let words: Vec<u32> = match v.view_dimension {
        1 => vec![member!(v.texture1d).mip_slice],
        2 => {
            let t = member!(v.texture1d_array);
            vec![t.mip_slice, t.first_array_slice, t.array_size]
        }
        3 => vec![member!(v.texture2d).mip_slice],
        4 => {
            let t = member!(v.texture2d_array);
            vec![t.mip_slice, t.first_array_slice, t.array_size]
        }
        6 => {
            let t = member!(v.texture2dms_array);
            vec![t.first_array_slice, t.array_size]
        }
        _ => vec![],
    };
    let _ = writeln!(
        out,
        "fmt={} dim={} flags={} {words:?}",
        v.format, v.view_dimension, v.flags
    );
}

fn generate_default_views(out: &mut String) {
    for (s, &(mips, array)) in [(7, 1), (3, 4), (1, 0)].iter().enumerate() {
        let d = D3d11Texture1dDesc {
            width: 64,
            mip_levels: mips,
            array_size: array,
            format: 28,
            usage: 0,
            bind_flags: 0,
            cpu_access_flags: 0,
            misc_flags: 0,
        };
        let _ = write!(out, "VIEW1D s={s} SRV ");
        srv_line(out, extract_entire_srv_1d(&d, None));
        let _ = write!(out, "VIEW1D s={s} UAV ");
        uav_line(out, extract_entire_uav_1d(&d, None));
        let _ = write!(out, "VIEW1D s={s} RTV ");
        rtv_line(out, extract_entire_rtv_1d(&d, None));
        let _ = write!(out, "VIEW1D s={s} DSV ");
        dsv_line(out, extract_entire_dsv_1d(&d, None));
    }
    let shapes = [
        (8, 1, 1, 0),
        (4, 12, 1, 0),
        (7, 6, 1, MISC_TEXTURECUBE),
        (7, 12, 1, MISC_TEXTURECUBE),
        (7, 18, 1, MISC_TEXTURECUBE),
        (7, 1, 1, MISC_TEXTURECUBE),
        (1, 1, 4, 0),
        (1, 3, 4, 0),
        (1, 6, 4, MISC_TEXTURECUBE),
        (1, 0, 1, 0),
    ];
    for (s, &(mips, array, samples, misc)) in shapes.iter().enumerate() {
        let mut d = tex2d(64, 64, mips, array, samples);
        d.misc_flags = misc;
        let _ = write!(out, "VIEW2D s={s} SRV ");
        srv_line(out, extract_entire_srv_2d(&d, None));
        let _ = write!(out, "VIEW2D s={s} UAV ");
        uav_line(out, extract_entire_uav_2d(&d, None));
        let _ = write!(out, "VIEW2D s={s} RTV ");
        rtv_line(out, extract_entire_rtv_2d(&d, None));
        let _ = write!(out, "VIEW2D s={s} DSV ");
        dsv_line(out, extract_entire_dsv_2d(&d, None));
    }
    let d = D3d11Texture3dDesc1 {
        width: 32,
        height: 16,
        depth: 8,
        mip_levels: 6,
        format: 28,
        usage: 0,
        bind_flags: 0,
        cpu_access_flags: 0,
        misc_flags: 0,
        texture_layout: 0,
    };
    let _ = write!(out, "VIEW3D SRV ");
    srv_line(out, extract_entire_srv_3d(&d, None));
    let _ = write!(out, "VIEW3D UAV ");
    uav_line(out, extract_entire_uav_3d(&d, None));
    let _ = write!(out, "VIEW3D RTV ");
    rtv_line(out, extract_entire_rtv_3d(&d, None));
    let _ = write!(out, "VIEW3D DSV ");
    dsv_line(out, extract_entire_dsv_3d(&d, None));
}

fn generate() -> String {
    let mut out = String::new();
    generate_texture_infos(&mut out);
    generate_default_views(&mut out);
    out
}

#[test]
fn descs_as_the_golden() {
    assert_eq!(generate(), GOLDEN);
}
