//! Golden of view normalisation over every view dimension and texture kind.
//!
//! Recorded from this module's own answers before their arithmetic moved into `dxmt-core`, so a
//! move that changes one answer, or what a call writes back into the description, fails here.

use core::fmt::Write as _;

use dxmt_core::resource::TextureViewDescriptor;
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT, WMT_PIXEL_FORMAT_RGBA8_UNORM,
    WMT_TEXTURE_TYPE_CUBE, WMT_TEXTURE_TYPE_CUBE_ARRAY, WMT_TEXTURE_TYPE2D,
    WMT_TEXTURE_TYPE2D_ARRAY, WMT_TEXTURE_TYPE2D_MULTISAMPLE, WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
    WMT_TEXTURE_TYPE3D, WMTTextureInfo,
};

use super::{
    RenderPassAttachmentDesc, initialize_and_normalize_dsv_descriptor,
    initialize_and_normalize_rtv_descriptor, initialize_and_normalize_srv_descriptor,
    initialize_and_normalize_uav_descriptor,
};
use crate::abi::records::{
    D3d11DepthStencilViewDesc, D3d11DepthStencilViewDescUnion, D3d11RenderTargetViewDesc1,
    D3d11RenderTargetViewDesc1Union, D3d11ShaderResourceViewDesc1,
    D3d11ShaderResourceViewDesc1Union, D3d11Tex2dArrayDsv, D3d11Tex2dArrayRtv1,
    D3d11Tex2dArraySrv1, D3d11Tex2dArrayUav1, D3d11UnorderedAccessViewDesc1,
    D3d11UnorderedAccessViewDesc1Union,
};

const GOLDEN: &str = include_str!("golden/normalize.txt");

const ALL: u32 = u32::MAX;

/// A texture: its Metal description and what the front end passes beside it.
///
/// That is the mip count and the array size (a 3D texture's depth).
struct Tex {
    info: WMTTextureInfo,
    mip_levels: u32,
    array_size: u32,
}

fn tex(
    texture_type: u32,
    pixel_format: u32,
    [w, h, d]: [u32; 3],
    samples: u32,
    mips: u32,
    array: u32,
) -> Tex {
    let array_length = match texture_type {
        WMT_TEXTURE_TYPE_CUBE | WMT_TEXTURE_TYPE3D => 1,
        WMT_TEXTURE_TYPE_CUBE_ARRAY => array / 6,
        _ => array,
    };
    Tex {
        info: WMTTextureInfo {
            pixel_format,
            width: w,
            height: h,
            depth: d,
            array_length,
            bits: (texture_type & 0xff)
                | ((mips & 0xff) << WMTTextureInfo::MIPMAP_LEVEL_COUNT_SHIFT)
                | ((samples & 0xff) << WMTTextureInfo::SAMPLE_COUNT_SHIFT),
            options: 0,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        },
        mip_levels: mips,
        array_size: array,
    }
}

fn textures() -> Vec<Tex> {
    let rgba = WMT_PIXEL_FORMAT_RGBA8_UNORM;
    vec![
        tex(WMT_TEXTURE_TYPE2D, rgba, [128, 64, 1], 1, 8, 1),
        tex(WMT_TEXTURE_TYPE2D_ARRAY, rgba, [128, 64, 1], 1, 4, 12),
        tex(WMT_TEXTURE_TYPE_CUBE, rgba, [64, 64, 1], 1, 7, 6),
        tex(WMT_TEXTURE_TYPE_CUBE_ARRAY, rgba, [64, 64, 1], 1, 7, 12),
        tex(WMT_TEXTURE_TYPE2D_MULTISAMPLE, rgba, [64, 32, 1], 4, 1, 1),
        tex(
            WMT_TEXTURE_TYPE2D_MULTISAMPLE_ARRAY,
            WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
            [64, 32, 1],
            4,
            1,
            3,
        ),
        tex(WMT_TEXTURE_TYPE3D, rgba, [32, 16, 8], 1, 6, 8),
        tex(
            WMT_TEXTURE_TYPE2D,
            WMT_PIXEL_FORMAT_DEPTH16_UNORM,
            [100, 50, 1],
            1,
            2,
            1,
        ),
    ]
}

/// The union's words, which each dimension reads its own way (a 2D array's fields are the widest).
const WORDS: [[u32; 5]; 9] = [
    [0, ALL, 0, ALL, 0],
    [1, 2, 2, 3, 0],
    [2, ALL, 1, ALL, 1],
    [5, ALL, 7, ALL, 0],
    [0, 1, 0, 1, 0],
    [9, 0, 20, 0, 0],
    [0, 6, 6, 1, 0],
    [1, 1, 1, 1, 0],
    [2, 3, 0, ALL, 7],
];

/// `RGBA8`, `R32Float` (a depth texture's view keeps its depth format) and one Metal lacks.
const FORMATS: [u32; 3] = [28, 41, 200];

fn view(out: &mut String, d: &TextureViewDescriptor) {
    let _ = write!(
        out,
        "fmt=0x{:x} type={} mip={}+{} slice={}+{}",
        d.format, d.texture_type, d.first_mip, d.mip_count, d.first_slice, d.array_size
    );
}

fn attach(out: &mut String, a: &RenderPassAttachmentDesc) {
    let _ = write!(
        out,
        " layers={} samples={} plane={} size={}x{} slice={} level={}",
        a.render_target_array_length,
        a.sample_count,
        a.depth_plane,
        a.width,
        a.height,
        a.slice,
        a.level
    );
}

fn generate() -> String {
    let mut out = String::new();
    for (t, tx) in textures().iter().enumerate() {
        for (f, &format) in FORMATS.iter().enumerate() {
            for (c, &[w0, w1, w2, w3, w4]) in WORDS.iter().enumerate() {
                if f > 0 && c > 1 {
                    continue;
                }
                for dim in 0..12 {
                    let mut d = D3d11ShaderResourceViewDesc1 {
                        format,
                        view_dimension: dim,
                        u: D3d11ShaderResourceViewDesc1Union {
                            texture2d_array: D3d11Tex2dArraySrv1 {
                                most_detailed_mip: w0,
                                mip_levels: w1,
                                first_array_slice: w2,
                                array_size: w3,
                                plane_slice: w4,
                            },
                        },
                    };
                    let r = initialize_and_normalize_srv_descriptor(
                        tx.mip_levels,
                        tx.array_size,
                        &tx.info,
                        &mut d,
                    );
                    let _ = write!(out, "SRV t={t} f={f} c={c} dim={dim} ");
                    match r {
                        Ok(v) => view(&mut out, &v),
                        Err(hr) => {
                            let _ = write!(out, "hr=0x{:08x}", hr.cast_unsigned());
                        }
                    }
                    // SAFETY: every member is plain words and the description was written whole.
                    let u = unsafe { d.u.texture2d_array };
                    let _ = writeln!(
                        out,
                        " desc={},{},{},{},{}",
                        u.most_detailed_mip,
                        u.mip_levels,
                        u.first_array_slice,
                        u.array_size,
                        u.plane_slice
                    );
                }
                for dim in 0..10 {
                    let mut d = D3d11UnorderedAccessViewDesc1 {
                        format,
                        view_dimension: dim,
                        u: D3d11UnorderedAccessViewDesc1Union {
                            texture2d_array: D3d11Tex2dArrayUav1 {
                                mip_slice: w0,
                                first_array_slice: w1,
                                array_size: w2,
                                plane_slice: w3,
                            },
                        },
                    };
                    let r = initialize_and_normalize_uav_descriptor(
                        tx.mip_levels,
                        tx.array_size,
                        &tx.info,
                        &mut d,
                    );
                    let _ = write!(out, "UAV t={t} f={f} c={c} dim={dim} ");
                    match r {
                        Ok(v) => view(&mut out, &v),
                        Err(hr) => {
                            let _ = write!(out, "hr=0x{:08x}", hr.cast_unsigned());
                        }
                    }
                    // SAFETY: every member is plain words and the description was written whole.
                    let u = unsafe { d.u.texture2d_array };
                    let _ = writeln!(
                        out,
                        " desc={},{},{},{}",
                        u.mip_slice, u.first_array_slice, u.array_size, u.plane_slice
                    );
                }
                for dim in 0..10 {
                    let mut d = D3d11RenderTargetViewDesc1 {
                        format,
                        view_dimension: dim,
                        u: D3d11RenderTargetViewDesc1Union {
                            texture2d_array: D3d11Tex2dArrayRtv1 {
                                mip_slice: w0,
                                first_array_slice: w1,
                                array_size: w2,
                                plane_slice: w3,
                            },
                        },
                    };
                    let r = initialize_and_normalize_rtv_descriptor(
                        tx.mip_levels,
                        tx.array_size,
                        &tx.info,
                        &mut d,
                    );
                    let _ = write!(out, "RTV t={t} f={f} c={c} dim={dim} ");
                    match r {
                        Ok((a, v)) => {
                            view(&mut out, &v);
                            attach(&mut out, &a);
                        }
                        Err(hr) => {
                            let _ = write!(out, "hr=0x{:08x}", hr.cast_unsigned());
                        }
                    }
                    // SAFETY: every member is plain words and the description was written whole.
                    let u = unsafe { d.u.texture2d_array };
                    let _ = writeln!(
                        out,
                        " desc={},{},{},{}",
                        u.mip_slice, u.first_array_slice, u.array_size, u.plane_slice
                    );
                }
                for dim in 0..8 {
                    let mut d = D3d11DepthStencilViewDesc {
                        format,
                        view_dimension: dim,
                        flags: w4 | 4,
                        u: D3d11DepthStencilViewDescUnion {
                            texture2d_array: D3d11Tex2dArrayDsv {
                                mip_slice: w0,
                                first_array_slice: w1,
                                array_size: w2,
                            },
                        },
                    };
                    let r = initialize_and_normalize_dsv_descriptor(
                        tx.mip_levels,
                        tx.array_size,
                        &tx.info,
                        &mut d,
                    );
                    let _ = write!(out, "DSV t={t} f={f} c={c} dim={dim} ");
                    match r {
                        Ok((a, v)) => {
                            view(&mut out, &v);
                            attach(&mut out, &a);
                        }
                        Err(hr) => {
                            let _ = write!(out, "hr=0x{:08x}", hr.cast_unsigned());
                        }
                    }
                    // SAFETY: every member is plain words and the description was written whole.
                    let u = unsafe { d.u.texture2d_array };
                    let _ = writeln!(
                        out,
                        " desc={},{},{} flags={}",
                        u.mip_slice, u.first_array_slice, u.array_size, d.flags
                    );
                }
            }
        }
    }
    out
}

#[test]
fn normalized_views_as_the_golden() {
    assert_eq!(generate(), GOLDEN);
}
