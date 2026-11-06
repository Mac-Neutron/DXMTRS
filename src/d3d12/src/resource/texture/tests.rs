//! Goldens of the texture views and texture descriptions D3D12 derives from its descriptions.
//!
//! Recorded from this crate's own answers before their arithmetic moved into `dxmt-core`, so a
//! move that changes one answer fails here. Resources have no Metal device: views read the
//! resource description, and texture descriptions see no Apple8 atomics and no colour caps.

use core::{fmt::Write as _, mem::ManuallyDrop, ptr};

use dxmt_core::resource::TextureViewKey;
use dxmt_util::dxgi_consts::{
    DXGI_FORMAT_D32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_FORMAT_R16_TYPELESS,
    DXGI_FORMAT_R32_TYPELESS,
};
use winemetal_wire::WMTTextureInfo;

use super::{dsv_target, rtv_target, srv_view, uav_views};
use crate::{
    abi::{
        pipeline::SampleDesc,
        resource::{
            D3D12_RESOURCE_DIMENSION_TEXTURE1D, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            D3D12_RESOURCE_DIMENSION_TEXTURE3D, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
            D3D12_TEXTURE_LAYOUT_ROW_MAJOR, D3D12_TEXTURE_LAYOUT_UNKNOWN, ResourceDesc,
        },
        views::{
            DepthStencilViewDesc, DepthStencilViewDescBody, RenderTargetViewDesc,
            RenderTargetViewDescBody, ShaderResourceViewDesc, ShaderResourceViewDescBody,
            Tex1dArrayDsv, Tex1dArrayRtv, Tex1dArraySrv, Tex1dArrayUav, Tex1dDsv, Tex1dRtv,
            Tex1dSrv, Tex1dUav, Tex2dArrayDsv, Tex2dArrayRtv, Tex2dArraySrv, Tex2dArrayUav,
            Tex2dDsv, Tex2dRtv, Tex2dSrv, Tex2dUav, Tex2dmsArrayDsv, Tex2dmsArrayRtv,
            Tex2dmsArraySrv, Tex3dRtv, Tex3dSrv, Tex3dUav, TexCubeArraySrv, TexCubeSrv,
            UnorderedAccessViewDesc, UnorderedAccessViewDescBody,
        },
    },
    resource::populate_texture_info_opt,
};

const VIEWS: &str = include_str!("golden/views.txt");
const INFOS: &str = include_str!("golden/info.txt");

const ALL: u32 = u32::MAX;
const DXGI_FORMAT_R32_FLOAT: u32 = 41;
const DXGI_FORMAT_BC1_UNORM: u32 = 71;
const UNSUPPORTED_FORMAT: u32 = 200;

/// A view description's numbers, which each dimension reads its own way.
struct Params {
    mip: u32,
    mips: u32,
    slice: u32,
    slices: u32,
    plane: u32,
    lod: f32,
}

const PARAMS: [Params; 6] = [
    Params {
        mip: 0,
        mips: ALL,
        slice: 0,
        slices: ALL,
        plane: 0,
        lod: 0.0,
    },
    Params {
        mip: 1,
        mips: 2,
        slice: 2,
        slices: 3,
        plane: 0,
        lod: 2.5,
    },
    Params {
        mip: 5,
        mips: ALL,
        slice: 7,
        slices: ALL,
        plane: 0,
        lod: 0.0,
    },
    Params {
        mip: 9,
        mips: ALL,
        slice: 20,
        slices: ALL,
        plane: 1,
        lod: 9.75,
    },
    Params {
        mip: 3,
        mips: 0,
        slice: 1,
        slices: 0,
        plane: 0,
        lod: -1.0,
    },
    Params {
        mip: 33,
        mips: 1,
        slice: 0,
        slices: 1,
        plane: 0,
        lod: 1.0,
    },
];

const MAPPINGS: [u32; 3] = [0x1688, 0x0, (3 << 9) | (5 << 6) | (4 << 3) | 2];

fn resource(
    dimension: u32,
    [width, height, depth]: [u32; 3],
    mips: u16,
    samples: u32,
    format: u32,
    flags: u32,
) -> ResourceDesc {
    ResourceDesc {
        dimension,
        alignment: 0,
        width: u64::from(width),
        height,
        depth_or_array_size: u16::try_from(depth).unwrap_or(u16::MAX),
        mip_levels: mips,
        format,
        sample_desc: SampleDesc {
            count: samples,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags,
    }
}

fn resources() -> Vec<ResourceDesc> {
    let t1 = D3D12_RESOURCE_DIMENSION_TEXTURE1D;
    let t2 = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    let t3 = D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    let rgba = DXGI_FORMAT_R8G8B8A8_UNORM;
    let ds = D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;
    vec![
        resource(t1, [64, 1, 1], 7, 1, rgba, 0),
        resource(t1, [64, 1, 4], 3, 1, rgba, 0),
        resource(t2, [128, 64, 1], 8, 1, rgba, 0),
        resource(t2, [128, 64, 12], 4, 1, rgba, 0),
        resource(t2, [256, 256, 6], 0, 1, DXGI_FORMAT_R32_TYPELESS, ds),
        resource(t2, [64, 32, 1], 1, 4, DXGI_FORMAT_D32_FLOAT, ds),
        resource(t2, [64, 32, 3], 1, 4, DXGI_FORMAT_R16_TYPELESS, ds),
        resource(t3, [32, 16, 8], 6, 1, rgba, 0),
    ]
}

fn srv(format: u32, dim: u32, mapping: u32, p: &Params) -> ShaderResourceViewDesc {
    let view = match dim {
        2 => ShaderResourceViewDescBody {
            texture_1d: ManuallyDrop::new(Tex1dSrv {
                most_detailed_mip: p.mip,
                mip_levels: p.mips,
                resource_min_lod_clamp: p.lod,
            }),
        },
        3 => ShaderResourceViewDescBody {
            texture_1d_array: ManuallyDrop::new(Tex1dArraySrv {
                most_detailed_mip: p.mip,
                mip_levels: p.mips,
                first_array_slice: p.slice,
                array_size: p.slices,
                resource_min_lod_clamp: p.lod,
            }),
        },
        4 => ShaderResourceViewDescBody {
            texture_2d: ManuallyDrop::new(Tex2dSrv {
                most_detailed_mip: p.mip,
                mip_levels: p.mips,
                plane_slice: p.plane,
                resource_min_lod_clamp: p.lod,
            }),
        },
        7 => ShaderResourceViewDescBody {
            texture_2dms_array: ManuallyDrop::new(Tex2dmsArraySrv {
                first_array_slice: p.slice,
                array_size: p.slices,
            }),
        },
        8 => ShaderResourceViewDescBody {
            texture_3d: ManuallyDrop::new(Tex3dSrv {
                most_detailed_mip: p.mip,
                mip_levels: p.mips,
                resource_min_lod_clamp: p.lod,
            }),
        },
        9 => ShaderResourceViewDescBody {
            texture_cube: ManuallyDrop::new(TexCubeSrv {
                most_detailed_mip: p.mip,
                mip_levels: p.mips,
                resource_min_lod_clamp: p.lod,
            }),
        },
        10 => ShaderResourceViewDescBody {
            texture_cube_array: ManuallyDrop::new(TexCubeArraySrv {
                most_detailed_mip: p.mip,
                mip_levels: p.mips,
                first_2d_array_face: p.slice,
                num_cubes: p.slices,
                resource_min_lod_clamp: p.lod,
            }),
        },
        _ => ShaderResourceViewDescBody {
            texture_2d_array: ManuallyDrop::new(Tex2dArraySrv {
                most_detailed_mip: p.mip,
                mip_levels: p.mips,
                first_array_slice: p.slice,
                array_size: p.slices,
                plane_slice: p.plane,
                resource_min_lod_clamp: p.lod,
            }),
        },
    };
    ShaderResourceViewDesc {
        format,
        view_dimension: dim,
        shader_4_component_mapping: mapping,
        view,
    }
}

fn uav(format: u32, dim: u32, p: &Params) -> UnorderedAccessViewDesc {
    let view = match dim {
        2 => UnorderedAccessViewDescBody {
            texture_1d: ManuallyDrop::new(Tex1dUav { mip_slice: p.mip }),
        },
        3 => UnorderedAccessViewDescBody {
            texture_1d_array: ManuallyDrop::new(Tex1dArrayUav {
                mip_slice: p.mip,
                first_array_slice: p.slice,
                array_size: p.slices,
            }),
        },
        4 => UnorderedAccessViewDescBody {
            texture_2d: ManuallyDrop::new(Tex2dUav {
                mip_slice: p.mip,
                plane_slice: p.plane,
            }),
        },
        8 => UnorderedAccessViewDescBody {
            texture_3d: ManuallyDrop::new(Tex3dUav {
                mip_slice: p.mip,
                first_w_slice: p.slice,
                w_size: p.slices,
            }),
        },
        _ => UnorderedAccessViewDescBody {
            texture_2d_array: ManuallyDrop::new(Tex2dArrayUav {
                mip_slice: p.mip,
                first_array_slice: p.slice,
                array_size: p.slices,
                plane_slice: p.plane,
            }),
        },
    };
    UnorderedAccessViewDesc {
        format,
        view_dimension: dim,
        view,
    }
}

fn rtv(format: u32, dim: u32, p: &Params) -> RenderTargetViewDesc {
    let view = match dim {
        2 => RenderTargetViewDescBody {
            texture_1d: ManuallyDrop::new(Tex1dRtv { mip_slice: p.mip }),
        },
        3 => RenderTargetViewDescBody {
            texture_1d_array: ManuallyDrop::new(Tex1dArrayRtv {
                mip_slice: p.mip,
                first_array_slice: p.slice,
                array_size: p.slices,
            }),
        },
        4 => RenderTargetViewDescBody {
            texture_2d: ManuallyDrop::new(Tex2dRtv {
                mip_slice: p.mip,
                plane_slice: p.plane,
            }),
        },
        7 => RenderTargetViewDescBody {
            texture_2dms_array: ManuallyDrop::new(Tex2dmsArrayRtv {
                first_array_slice: p.slice,
                array_size: p.slices,
            }),
        },
        8 => RenderTargetViewDescBody {
            texture_3d: ManuallyDrop::new(Tex3dRtv {
                mip_slice: p.mip,
                first_w_slice: p.slice,
                w_size: p.slices,
            }),
        },
        _ => RenderTargetViewDescBody {
            texture_2d_array: ManuallyDrop::new(Tex2dArrayRtv {
                mip_slice: p.mip,
                first_array_slice: p.slice,
                array_size: p.slices,
                plane_slice: p.plane,
            }),
        },
    };
    RenderTargetViewDesc {
        format,
        view_dimension: dim,
        view,
    }
}

fn dsv(format: u32, dim: u32, flags: u32, p: &Params) -> DepthStencilViewDesc {
    let view = match dim {
        1 => DepthStencilViewDescBody {
            texture_1d: ManuallyDrop::new(Tex1dDsv { mip_slice: p.mip }),
        },
        2 => DepthStencilViewDescBody {
            texture_1d_array: ManuallyDrop::new(Tex1dArrayDsv {
                mip_slice: p.mip,
                first_array_slice: p.slice,
                array_size: p.slices,
            }),
        },
        3 => DepthStencilViewDescBody {
            texture_2d: ManuallyDrop::new(Tex2dDsv { mip_slice: p.mip }),
        },
        6 => DepthStencilViewDescBody {
            texture_2dms_array: ManuallyDrop::new(Tex2dmsArrayDsv {
                first_array_slice: p.slice,
                array_size: p.slices,
            }),
        },
        _ => DepthStencilViewDescBody {
            texture_2d_array: ManuallyDrop::new(Tex2dArrayDsv {
                mip_slice: p.mip,
                first_array_slice: p.slice,
                array_size: p.slices,
            }),
        },
    };
    DepthStencilViewDesc {
        format,
        view_dimension: dim,
        flags,
        view,
    }
}

fn key(out: &mut String, key: TextureViewKey) {
    let d = key.descriptor();
    let _ = write!(
        out,
        "fmt=0x{:x} type={} mip={}+{} slice={}+{}",
        d.format, d.texture_type, d.first_mip, d.mip_count, d.first_slice, d.array_size
    );
}

fn generate_views() -> String {
    let mut out = String::new();
    for (r, desc) in resources().iter().enumerate() {
        // The resource's own format through every case; another (`R32Float`: the UAV's atomic
        // twin) and an unsupported one through the first.
        for (f, format) in [desc.format, DXGI_FORMAT_R32_FLOAT, UNSUPPORTED_FORMAT]
            .into_iter()
            .enumerate()
        {
            for (c, p) in PARAMS.iter().enumerate() {
                if f > 0 && c > 0 {
                    continue;
                }
                for dim in [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 99] {
                    for (m, &mapping) in MAPPINGS.iter().enumerate() {
                        if m > 0 && (c > 1 || f > 0) {
                            continue;
                        }
                        let _ = write!(out, "SRV r={r} f={f} c={c} dim={dim} m={m} ");
                        match srv_view(desc, None, &srv(format, dim, mapping, p)) {
                            Ok((view, clamp)) => {
                                key(&mut out, view);
                                let _ = writeln!(out, " clamp={clamp:?}");
                            }
                            Err(hr) => {
                                let _ = writeln!(out, "hr=0x{:08x}", hr.cast_unsigned());
                            }
                        }
                    }
                }
                for dim in [0, 2, 3, 4, 5, 6, 8, 99] {
                    let _ = write!(out, "UAV r={r} f={f} c={c} dim={dim} ");
                    match uav_views(desc, &uav(format, dim, p)) {
                        Ok((view, atomic)) => {
                            key(&mut out, view);
                            if let Some(a) = atomic {
                                out.push_str(" atomic ");
                                key(&mut out, a);
                            }
                            out.push('\n');
                        }
                        Err(hr) => {
                            let _ = writeln!(out, "hr=0x{:08x}", hr.cast_unsigned());
                        }
                    }
                }
                for dim in [0, 2, 3, 4, 5, 6, 7, 8, 99] {
                    let _ = write!(out, "RTV r={r} f={f} c={c} dim={dim} ");
                    rt(&mut out, rtv_target(desc, None, &rtv(format, dim, p)));
                }
                for dim in [0, 1, 2, 3, 4, 5, 6, 99] {
                    let flags = u32::try_from(c % 4).unwrap_or(0);
                    let _ = write!(out, "DSV r={r} f={f} c={c} dim={dim} ");
                    rt(
                        &mut out,
                        dsv_target(desc, None, &dsv(format, dim, flags, p)),
                    );
                }
            }
        }
    }
    out
}

fn rt(out: &mut String, result: Result<crate::descriptor_heap::RenderTargetDesc, i32>) {
    match result {
        Ok(rt) => {
            key(out, rt.view);
            let _ = writeln!(
                out,
                " size={}x{} plane={} layers={} flags={} tex={}",
                rt.width,
                rt.height,
                rt.depth_plane,
                rt.render_target_array_length,
                rt.flags,
                rt.texture.is_some()
            );
        }
        Err(hr) => {
            let _ = writeln!(out, "hr=0x{:08x}", hr.cast_unsigned());
        }
    }
}

fn generate_infos() -> String {
    let mut out = String::new();
    let t1 = D3D12_RESOURCE_DIMENSION_TEXTURE1D;
    let t2 = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    let t3 = D3D12_RESOURCE_DIMENSION_TEXTURE3D;
    let rgba = DXGI_FORMAT_R8G8B8A8_UNORM;
    let rt = D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
    let ds = D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL;
    let uav = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    let deny = 0x8;
    let cross = 0x10;
    let mut descs = vec![
        resource(t1, [64, 1, 1], 0, 1, rgba, 0),
        resource(t1, [64, 1, 4], 3, 1, rgba, uav),
        resource(t1, [64, 1, 1], 1, 1, DXGI_FORMAT_BC1_UNORM, 0),
        resource(t1, [64, 1, 1], 1, 1, DXGI_FORMAT_D32_FLOAT, ds),
        resource(t2, [128, 64, 1], 0, 1, rgba, rt),
        resource(t2, [128, 64, 6], 20, 1, rgba, rt | uav),
        resource(t2, [0, 0, 1], 0, 1, rgba, 0),
        resource(t2, [64, 32, 1], 1, 4, DXGI_FORMAT_D32_FLOAT, ds | deny),
        resource(t2, [64, 32, 3], 1, 4, DXGI_FORMAT_R16_TYPELESS, ds),
        resource(t2, [64, 32, 1], 1, 0, rgba, 0),
        resource(t2, [64, 32, 1], 1, 1, DXGI_FORMAT_R32_TYPELESS, uav),
        resource(t2, [64, 32, 1], 1, 1, 42, uav),
        resource(t2, [64, 32, 1], 1, 1, 53, uav),
        resource(t2, [64, 32, 1], 1, 1, rgba, uav | cross),
        resource(t2, [64, 32, 1], 1, 1, UNSUPPORTED_FORMAT, 0),
        resource(t3, [32, 16, 8], 0, 1, rgba, uav),
        resource(t3, [32, 16, 64], 3, 1, rgba, rt),
        resource(4, [32, 16, 8], 1, 1, rgba, 0),
    ];
    let mut row_major = resource(t2, [64, 32, 1], 1, 1, rgba, 0);
    row_major.layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    descs.push(row_major);
    for alignment in [4096, 65536, 4 << 20, 1234] {
        for (samples, flags) in [(1, 0), (4, rt), (1, rt)] {
            let mut d = resource(t2, [64, 32, 1], 1, samples, rgba, flags);
            d.alignment = alignment;
            descs.push(d);
        }
    }
    let mut small_row_major = resource(t2, [64, 32, 1], 1, 1, rgba, 0);
    small_row_major.alignment = 4096;
    small_row_major.layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    descs.push(small_row_major);
    for (i, desc) in descs.iter().enumerate() {
        let mut info = WMTTextureInfo {
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
        // SAFETY: a null device is allowed: the description alone decides.
        let hr = unsafe { populate_texture_info_opt(ptr::null(), desc, &mut info) };
        let _ = writeln!(
            out,
            "INFO {i} hr=0x{:08x} pf={} {}x{}x{} arr={} bits=0x{:08x} opt=0x{:x} flags=0x{:x}",
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
    out
}

#[test]
fn views_as_the_golden() {
    assert_eq!(generate_views(), VIEWS);
}

#[test]
fn texture_infos_as_the_golden() {
    assert_eq!(generate_infos(), INFOS);
}
