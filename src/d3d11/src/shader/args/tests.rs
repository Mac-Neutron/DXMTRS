use std::fmt::Write as _;

use dxmt_util::config::Config;
use winemetal_sys::sm50::{
    SM50_INDEX_BUFFER_FORMAT_NONE, SM50_INDEX_BUFFER_FORMAT_UINT16,
    SM50_INDEX_BUFFER_FORMAT_UINT32, SM50_SHADER_FLAG_DEFUSE_FMA,
    SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO,
};

use crate::{
    input_layout::{InputLayoutElement, StreamOutputElement},
    shader::args::{
        CompileArgsChain, IaLayoutChainInput, ShaderVariant, SoLayoutChainInput, hex_bytes,
        input_layout_digest_with_width, input_layout_stream_with_width,
        stream_output_layout_digest_with_width, stream_output_layout_stream_with_width,
    },
};

const WMT_METAL_310: u32 = 310;
const WMT_METAL_320: u32 = 320;

const fn il_elem(
    index: u32,
    slot: u32,
    offset: u32,
    format: u32,
    step_fn: u32,
    step_rate: u32,
) -> InputLayoutElement {
    InputLayoutElement {
        index,
        slot,
        offset,
        format,
        step_function_and_rate: (step_fn & 1) | (step_rate << 1),
    }
}

const fn so_elem(reg: u32, comp: u32, slot: u32, offset: u32) -> StreamOutputElement {
    StreamOutputElement {
        register_idx: reg,
        component: comp,
        output_slot: slot,
        offset,
    }
}

fn parse_hex_sha1(s: &str) -> [u8; 20] {
    assert_eq!(s.len(), 40);
    let mut out = [0u8; 20];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("valid hex");
    }
    out
}

struct IlCase {
    name: &'static str,
    slot_mask: u32,
    elems: Vec<InputLayoutElement>,
}

struct SoCase {
    name: &'static str,
    rasterized_stream: u32,
    strides: [u32; 4],
    elems: Vec<StreamOutputElement>,
}

fn build_il_cases() -> Vec<IlCase> {
    let mut v = vec![
        IlCase {
            name: "il_0_mask0",
            slot_mask: 0x0,
            elems: vec![],
        },
        IlCase {
            name: "il_0_mask5",
            slot_mask: 0x5,
            elems: vec![],
        },
        IlCase {
            name: "il_1_vertex",
            slot_mask: 0x1,
            elems: vec![il_elem(0, 0, 0, 31, 0, 1)],
        },
        IlCase {
            name: "il_1_instance_step0",
            slot_mask: 0x2,
            elems: vec![il_elem(0, 1, 16, 29, 1, 0)],
        },
        IlCase {
            name: "il_1_instance_step1",
            slot_mask: 0x4,
            elems: vec![il_elem(3, 2, 32, 49, 1, 1)],
        },
        IlCase {
            name: "il_1_instance_maxrate",
            slot_mask: 0x8000_0000,
            elems: vec![il_elem(31, 31, 0xffff, 40, 1, 0x7fff_ffff)],
        },
        IlCase {
            name: "il_2_mixed",
            slot_mask: 0x3,
            elems: vec![il_elem(0, 0, 0, 30, 0, 1), il_elem(1, 1, 12, 31, 1, 4)],
        },
    ];
    {
        let mut elems = Vec::with_capacity(32);
        let mut mask = 0u32;
        for i in 0..32u32 {
            let slot = i % 16;
            elems.push(il_elem(
                i,
                slot,
                (i / 16) * 16 + (i % 4) * 4,
                28 + (i % 13),
                0,
                1,
            ));
            mask |= 1u32 << slot;
        }
        v.push(IlCase {
            name: "il_32_vertex",
            slot_mask: mask,
            elems,
        });
    }
    {
        let mut elems = Vec::with_capacity(32);
        let mut mask = 0u32;
        for i in 0..32u32 {
            let rate = if i == 0 {
                0
            } else if i == 31 {
                0x7fff_ffff
            } else {
                i
            };
            elems.push(il_elem(i, i, i * 16, 28 + (i % 20), 1, rate));
            mask |= 1u32 << i;
        }
        v.push(IlCase {
            name: "il_32_instance",
            slot_mask: mask,
            elems,
        });
    }
    {
        let mut elems = Vec::with_capacity(32);
        let mut mask = 0u32;
        for i in 0..32u32 {
            let slot = (i * 3) & 31;
            let step_fn = i & 1;
            let rate = if step_fn != 0 {
                (i * 7) & 0x7fff_ffff
            } else {
                1
            };
            elems.push(il_elem(31 - i, slot, i * 8, 1 + (i % 50), step_fn, rate));
            mask |= 1u32 << slot;
        }
        v.push(IlCase {
            name: "il_32_mixed",
            slot_mask: mask,
            elems,
        });
    }
    v
}

fn build_so_cases() -> Vec<SoCase> {
    let mut v = vec![
        SoCase {
            name: "so_0_stream0",
            rasterized_stream: 0,
            strides: [0, 0, 0, 0],
            elems: vec![],
        },
        SoCase {
            name: "so_0_no_raster",
            rasterized_stream: 0xffff_ffff,
            strides: [16, 32, 0, 0],
            elems: vec![],
        },
        SoCase {
            name: "so_1_basic",
            rasterized_stream: 0,
            strides: [16, 0, 0, 0],
            elems: vec![so_elem(0, 0, 0, 0)],
        },
        SoCase {
            name: "so_4_hole_and_multi",
            rasterized_stream: 1,
            strides: [16, 32, 48, 64],
            elems: vec![
                so_elem(0, 0, 0, 0),
                so_elem(0, 1, 0, 4),
                so_elem(0xffff_ffff, 2, 0, 8),
                so_elem(3, 3, 2, 12),
            ],
        },
    ];
    {
        let mut elems = Vec::with_capacity(16);
        for i in 0..16u32 {
            let reg = if i == 7 { 0xffff_ffff } else { i };
            elems.push(so_elem(reg, i % 4, i % 4, i * 4));
        }
        v.push(SoCase {
            name: "so_16_edge",
            rasterized_stream: 3,
            strides: [0xffff_ffff, 0x100, 0x200, 0x400],
            elems,
        });
    }
    v
}

#[test]
fn digests_and_names_are_the_cpps() {
    let expected = include_str!("../golden/digests.txt");
    let il_cases = build_il_cases();
    let so_cases = build_so_cases();

    let mut out = String::with_capacity(expected.len());

    for c in &il_cases {
        for (i, e) in c.elems.iter().enumerate() {
            let step_fn = e.step_function_and_rate & 1;
            let step_rate = e.step_function_and_rate >> 1;
            let _ = writeln!(
                out,
                "INPUT_LAYOUT_ELEM case={} idx={} index={} slot={} offset={} format={} step_fn={} step_rate={}",
                c.name, i, e.index, e.slot, e.offset, e.format, step_fn, step_rate
            );
        }
    }

    for c in &so_cases {
        for (i, e) in c.elems.iter().enumerate() {
            let _ = writeln!(
                out,
                "STREAM_OUTPUT_ELEM case={} idx={} reg=0x{:08x} comp={} slot={} offset={}",
                c.name, i, e.register_idx, e.component, e.output_slot, e.offset
            );
        }
    }

    let mut il_w8 = Vec::with_capacity(il_cases.len());
    let mut il_w4 = Vec::with_capacity(il_cases.len());
    for c in &il_cases {
        let stream8 = input_layout_stream_with_width(&c.elems, c.slot_mask, 8);
        let sha8 = input_layout_digest_with_width(&c.elems, c.slot_mask, 8);
        il_w8.push(sha8);
        let _ = writeln!(
            out,
            "INPUT_LAYOUT case={} size_t=8 slot_mask=0x{:08x} num_elems={} stream_len={} stream={} sha1={}",
            c.name,
            c.slot_mask,
            c.elems.len(),
            stream8.len(),
            hex_bytes(&stream8),
            hex_bytes(&sha8)
        );

        let stream4 = input_layout_stream_with_width(&c.elems, c.slot_mask, 4);
        let sha4 = input_layout_digest_with_width(&c.elems, c.slot_mask, 4);
        il_w4.push(sha4);
        let _ = writeln!(
            out,
            "INPUT_LAYOUT case={} size_t=4 slot_mask=0x{:08x} num_elems={} stream_len={} stream={} sha1={}",
            c.name,
            c.slot_mask,
            c.elems.len(),
            stream4.len(),
            hex_bytes(&stream4),
            hex_bytes(&sha4)
        );
    }

    let mut so_w8 = Vec::with_capacity(so_cases.len());
    let mut so_w4 = Vec::with_capacity(so_cases.len());
    for c in &so_cases {
        let stream8 =
            stream_output_layout_stream_with_width(&c.elems, &c.strides, c.rasterized_stream, 8);
        let sha8 =
            stream_output_layout_digest_with_width(&c.elems, &c.strides, c.rasterized_stream, 8);
        so_w8.push(sha8);
        let _ = writeln!(
            out,
            "STREAM_OUTPUT_LAYOUT case={} size_t=8 rasterized_stream=0x{:08x} strides={},{},{},{} num_elems={} stream_len={} stream={} sha1={}",
            c.name,
            c.rasterized_stream,
            c.strides[0],
            c.strides[1],
            c.strides[2],
            c.strides[3],
            c.elems.len(),
            stream8.len(),
            hex_bytes(&stream8),
            hex_bytes(&sha8)
        );

        let stream4 =
            stream_output_layout_stream_with_width(&c.elems, &c.strides, c.rasterized_stream, 4);
        let sha4 =
            stream_output_layout_digest_with_width(&c.elems, &c.strides, c.rasterized_stream, 4);
        so_w4.push(sha4);
        let _ = writeln!(
            out,
            "STREAM_OUTPUT_LAYOUT case={} size_t=4 rasterized_stream=0x{:08x} strides={},{},{},{} num_elems={} stream_len={} stream={} sha1={}",
            c.name,
            c.rasterized_stream,
            c.strides[0],
            c.strides[1],
            c.strides[2],
            c.strides[3],
            c.elems.len(),
            stream4.len(),
            hex_bytes(&stream4),
            hex_bytes(&sha4)
        );
    }

    let main_shaders = [
        parse_hex_sha1("0123456789abcdef0123456789abcdef01234567"),
        parse_hex_sha1("fedcba9876543210fedcba9876543210fedcba98"),
        parse_hex_sha1("00000000ffffffff00000000ffffffff00000000"),
    ];
    let aux_shaders = [
        parse_hex_sha1("89abcdef0123456789abcdef0123456789abcdef"),
        parse_hex_sha1("deadbeefcafebabedeadbeefcafebabedeadbeef"),
    ];

    let get_il = |il_idx: Option<usize>, width: usize| -> Option<[u8; 20]> {
        il_idx.map(|idx| if width == 8 { il_w8[idx] } else { il_w4[idx] })
    };
    let get_il_name =
        |il_idx: Option<usize>| -> &'static str { il_idx.map_or("null", |idx| il_cases[idx].name) };
    let get_so = |so_idx: usize, width: usize| -> [u8; 20] {
        if width == 8 {
            so_w8[so_idx]
        } else {
            so_w4[so_idx]
        }
    };

    // 1. Default
    {
        let mut case_id = 0;
        for flags in [0u32, 1, 2, 3] {
            for sh in &main_shaders {
                for width in [8usize, 4] {
                    let v = ShaderVariant::Default;
                    let stream = v.stream(flags);
                    let (digest, func_name) = v.digest_and_name(sh, flags);
                    let _ = writeln!(
                        out,
                        "VARIANT_DIGEST kind=Default case={} size_t={} flags={} shader_sha1={} stream_len={} stream={} digest={} func_name={}",
                        case_id,
                        width,
                        flags,
                        hex_bytes(sh),
                        stream.len(),
                        hex_bytes(&stream),
                        hex_bytes(&digest),
                        func_name
                    );
                }
                case_id += 1;
            }
        }
    }

    // 2. Vertex
    {
        let mut case_id = 0usize;
        let gs_vals = [0u32, 1, 0x1234_5678, 0xffff_ffff];
        let il_indices = [
            None,
            Some(0usize),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
            Some(9),
        ];
        for flags in [0u32, 1, 2, 3] {
            for gs_pass in gs_vals {
                for rast_dis in [false, true] {
                    for il_idx in il_indices {
                        if !((flags == 0 && gs_pass == 0 && !rast_dis)
                            || (flags == 3 && gs_pass == 0xffff_ffff && rast_dis))
                        {
                            if !matches!(il_idx, None | Some(0 | 2 | 3 | 7 | 8 | 9)) {
                                continue;
                            }
                            if gs_pass == 0x1234_5678 && (flags == 1 || flags == 2) {
                                continue;
                            }
                        }
                        let sh = &main_shaders[case_id % 3];
                        for width in [8usize, 4] {
                            let v = ShaderVariant::Vertex {
                                input_layout_sha1: get_il(il_idx, width),
                                gs_passthrough: gs_pass,
                                rasterization_disabled: rast_dis,
                            };
                            let stream = v.stream(flags);
                            let (digest, func_name) = v.digest_and_name(sh, flags);
                            let _ = writeln!(
                                out,
                                "VARIANT_DIGEST kind=Vertex case={} size_t={} flags={} shader_sha1={} il={} gs_passthrough=0x{:08x} rast_disabled={} stream_len={} stream={} digest={} func_name={}",
                                case_id,
                                width,
                                flags,
                                hex_bytes(sh),
                                get_il_name(il_idx),
                                gs_pass,
                                u32::from(rast_dis),
                                stream.len(),
                                hex_bytes(&stream),
                                hex_bytes(&digest),
                                func_name
                            );
                        }
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 3. Pixel
    {
        let mut case_id = 0usize;
        let sample_masks = [0u32, 1, 0x0f0f_0f0f, 0xffff_ffff];
        let unorm_masks = [0u32, 1, 0xff, 0xffff_ffff];
        for flags in [0u32, 1, 2, 3] {
            for sm in sample_masks {
                for um in unorm_masks {
                    for dsb in [false, true] {
                        for ddo in [false, true] {
                            if (flags == 1 || flags == 2) && (sm == 0x0f0f_0f0f || um == 0xff) {
                                continue;
                            }
                            let sh = &main_shaders[case_id % 3];
                            for width in [8usize, 4] {
                                let v = ShaderVariant::Pixel {
                                    sample_mask: sm,
                                    dual_source_blending: dsb,
                                    disable_depth_output: ddo,
                                    unorm_output_reg_mask: um,
                                };
                                let stream = v.stream(flags);
                                let (digest, func_name) = v.digest_and_name(sh, flags);
                                let _ = writeln!(
                                    out,
                                    "VARIANT_DIGEST kind=Pixel case={} size_t={} flags={} shader_sha1={} sample_mask=0x{:08x} unorm_mask=0x{:08x} dual_src={} disable_depth={} stream_len={} stream={} digest={} func_name={}",
                                    case_id,
                                    width,
                                    flags,
                                    hex_bytes(sh),
                                    sm,
                                    um,
                                    u32::from(dsb),
                                    u32::from(ddo),
                                    stream.len(),
                                    hex_bytes(&stream),
                                    hex_bytes(&digest),
                                    func_name
                                );
                            }
                            case_id += 1;
                        }
                    }
                }
            }
        }
    }

    // 4. TessellationVertexHull
    {
        let mut case_id = 0usize;
        let ib_fmts = [
            SM50_INDEX_BUFFER_FORMAT_NONE,
            SM50_INDEX_BUFFER_FORMAT_UINT16,
            SM50_INDEX_BUFFER_FORMAT_UINT32,
        ];
        let tess_factors = [0u32, 1, 16, 64, 0xffff_ffff];
        let il_indices = [
            None,
            Some(0usize),
            Some(2),
            Some(3),
            Some(5),
            Some(7),
            Some(8),
            Some(9),
        ];
        for flags in [0u32, 1, 2, 3] {
            for ib_fmt in ib_fmts {
                for tf in tess_factors {
                    for il_idx in il_indices {
                        if (flags == 1 || flags == 2)
                            && (tf == 16 || !matches!(il_idx, None | Some(2 | 7 | 8)))
                        {
                            continue;
                        }
                        let sh = &main_shaders[case_id % 3];
                        let vs = &aux_shaders[case_id % 2];
                        for width in [8usize, 4] {
                            let v = ShaderVariant::TessellationVertexHull {
                                input_layout_sha1: get_il(il_idx, width),
                                vs_sha1: *vs,
                                index_buffer_format: ib_fmt,
                                max_potential_tess_factor: tf,
                            };
                            let stream = v.stream(flags);
                            let (digest, func_name) = v.digest_and_name(sh, flags);
                            let _ = writeln!(
                                out,
                                "VARIANT_DIGEST kind=TessellationVertexHull case={} size_t={} flags={} shader_sha1={} vs_sha1={} il={} index_fmt={} max_tess={} stream_len={} stream={} digest={} func_name={}",
                                case_id,
                                width,
                                flags,
                                hex_bytes(sh),
                                hex_bytes(vs),
                                get_il_name(il_idx),
                                ib_fmt,
                                tf,
                                stream.len(),
                                hex_bytes(&stream),
                                hex_bytes(&digest),
                                func_name
                            );
                        }
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 5. TessellationDomain
    {
        let mut case_id = 0usize;
        let gs_vals = [0u32, 1, 0x8000_0001, 0xffff_ffff];
        let tess_factors = [0u32, 1, 16, 64, 0xffff_ffff];
        for flags in [0u32, 1, 2, 3] {
            for gs_pass in gs_vals {
                for tf in tess_factors {
                    for rast_dis in [false, true] {
                        let sh = &main_shaders[case_id % 3];
                        let hs = &aux_shaders[case_id % 2];
                        for width in [8usize, 4] {
                            let v = ShaderVariant::TessellationDomain {
                                hs_sha1: *hs,
                                gs_passthrough: gs_pass,
                                max_potential_tess_factor: tf,
                                rasterization_disabled: rast_dis,
                            };
                            let stream = v.stream(flags);
                            let (digest, func_name) = v.digest_and_name(sh, flags);
                            let _ = writeln!(
                                out,
                                "VARIANT_DIGEST kind=TessellationDomain case={} size_t={} flags={} shader_sha1={} hs_sha1={} gs_passthrough=0x{:08x} max_tess={} rast_disabled={} stream_len={} stream={} digest={} func_name={}",
                                case_id,
                                width,
                                flags,
                                hex_bytes(sh),
                                hex_bytes(hs),
                                gs_pass,
                                tf,
                                u32::from(rast_dis),
                                stream.len(),
                                hex_bytes(&stream),
                                hex_bytes(&digest),
                                func_name
                            );
                        }
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 6. VertexStreamOutput
    {
        let mut case_id = 0usize;
        let il_indices = [
            None,
            Some(0usize),
            Some(1),
            Some(2),
            Some(3),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
            Some(9),
        ];
        for flags in [0u32, 1, 2, 3] {
            for (so_idx, so_case) in so_cases.iter().enumerate() {
                for il_idx in il_indices {
                    let sh = &main_shaders[case_id % 3];
                    for width in [8usize, 4] {
                        let v = ShaderVariant::VertexStreamOutput {
                            input_layout_sha1: get_il(il_idx, width),
                            so_layout_sha1: get_so(so_idx, width),
                        };
                        let stream = v.stream(flags);
                        let (digest, func_name) = v.digest_and_name(sh, flags);
                        let _ = writeln!(
                            out,
                            "VARIANT_DIGEST kind=VertexStreamOutput case={} size_t={} flags={} shader_sha1={} so={} il={} stream_len={} stream={} digest={} func_name={}",
                            case_id,
                            width,
                            flags,
                            hex_bytes(sh),
                            so_case.name,
                            get_il_name(il_idx),
                            stream.len(),
                            hex_bytes(&stream),
                            hex_bytes(&digest),
                            func_name
                        );
                    }
                    case_id += 1;
                }
            }
        }
    }

    // 7. GeometryVertex
    {
        let mut case_id = 0usize;
        let ib_fmts = [
            SM50_INDEX_BUFFER_FORMAT_NONE,
            SM50_INDEX_BUFFER_FORMAT_UINT16,
            SM50_INDEX_BUFFER_FORMAT_UINT32,
        ];
        let il_indices = [
            None,
            Some(0usize),
            Some(1),
            Some(2),
            Some(3),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
            Some(9),
        ];
        for flags in [0u32, 1, 2, 3] {
            for ib_fmt in ib_fmts {
                for strip in [false, true] {
                    for il_idx in il_indices {
                        let sh = &main_shaders[case_id % 3];
                        let gs = &aux_shaders[case_id % 2];
                        for width in [8usize, 4] {
                            let v = ShaderVariant::GeometryVertex {
                                input_layout_sha1: get_il(il_idx, width),
                                gs_sha1: *gs,
                                index_buffer_format: ib_fmt,
                                strip_topology: strip,
                            };
                            let stream = v.stream(flags);
                            let (digest, func_name) = v.digest_and_name(sh, flags);
                            let _ = writeln!(
                                out,
                                "VARIANT_DIGEST kind=GeometryVertex case={} size_t={} flags={} shader_sha1={} gs_sha1={} il={} index_fmt={} strip={} stream_len={} stream={} digest={} func_name={}",
                                case_id,
                                width,
                                flags,
                                hex_bytes(sh),
                                hex_bytes(gs),
                                get_il_name(il_idx),
                                ib_fmt,
                                u32::from(strip),
                                stream.len(),
                                hex_bytes(&stream),
                                hex_bytes(&digest),
                                func_name
                            );
                        }
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 8. Geometry
    {
        let mut case_id = 0usize;
        for flags in [0u32, 1, 2, 3] {
            for sh in &main_shaders {
                for vs in &aux_shaders {
                    for strip in [false, true] {
                        for width in [8usize, 4] {
                            let v = ShaderVariant::Geometry {
                                vs_sha1: *vs,
                                strip_topology: strip,
                            };
                            let stream = v.stream(flags);
                            let (digest, func_name) = v.digest_and_name(sh, flags);
                            let _ = writeln!(
                                out,
                                "VARIANT_DIGEST kind=Geometry case={} size_t={} flags={} shader_sha1={} vs_sha1={} strip={} stream_len={} stream={} digest={} func_name={}",
                                case_id,
                                width,
                                flags,
                                hex_bytes(sh),
                                hex_bytes(vs),
                                u32::from(strip),
                                stream.len(),
                                hex_bytes(&stream),
                                hex_bytes(&digest),
                                func_name
                            );
                        }
                        case_id += 1;
                    }
                }
            }
        }
    }

    assert_eq!(out, expected);
}

struct TestIl<'a> {
    chain: IaLayoutChainInput<'a>,
    sha1: [u8; 20],
}

impl Clone for TestIl<'_> {
    fn clone(&self) -> Self {
        Self {
            chain: self.chain.clone(),
            sha1: self.sha1,
        }
    }
}

struct TestSo<'a> {
    chain: SoLayoutChainInput<'a>,
    sha1: [u8; 20],
}

impl Clone for TestSo<'_> {
    fn clone(&self) -> Self {
        Self {
            chain: self.chain.clone(),
            sha1: self.sha1,
        }
    }
}

struct TestShader {
    handle: usize,
    sha1: [u8; 20],
}

#[test]
fn chains_are_the_cpps() {
    let expected = include_str!("../golden/chains.txt");

    let il_empty = TestIl {
        chain: IaLayoutChainInput {
            slot_mask: 0x0,
            elements: &[],
        },
        sha1: input_layout_digest_with_width(&[], 0x0, 8),
    };
    let single_elems = [il_elem(0, 0, 0, 31, 0, 1)];
    let il_single = TestIl {
        chain: IaLayoutChainInput {
            slot_mask: 0x1,
            elements: &single_elems,
        },
        sha1: input_layout_digest_with_width(&single_elems, 0x1, 8),
    };
    let pair_elems = [il_elem(0, 0, 0, 30, 0, 1), il_elem(1, 2, 12, 31, 1, 4)];
    let il_pair = TestIl {
        chain: IaLayoutChainInput {
            slot_mask: 0x5,
            elements: &pair_elems,
        },
        sha1: input_layout_digest_with_width(&pair_elems, 0x5, 8),
    };
    let mut full_elems = Vec::with_capacity(32);
    let mut mask32 = 0u32;
    for i in 0..32u32 {
        let slot = i % 16;
        let step_fn = i & 1;
        let rate = if step_fn != 0 { i / 2 } else { 1 };
        full_elems.push(il_elem(i, slot, i * 4, 28 + (i % 13), step_fn, rate));
        mask32 |= 1u32 << slot;
    }
    let il_full = TestIl {
        chain: IaLayoutChainInput {
            slot_mask: mask32,
            elements: &full_elems,
        },
        sha1: input_layout_digest_with_width(&full_elems, mask32, 8),
    };
    let ils: [Option<TestIl<'_>>; 5] = [
        None,
        Some(il_empty),
        Some(il_single),
        Some(il_pair),
        Some(il_full),
    ];

    let so_0 = TestSo {
        chain: SoLayoutChainInput {
            elements: &[],
            strides: [0, 0, 0, 0],
        },
        sha1: stream_output_layout_digest_with_width(&[], &[0, 0, 0, 0], 0, 8),
    };
    let so1_elems = [so_elem(0, 0, 0, 0)];
    let so_1 = TestSo {
        chain: SoLayoutChainInput {
            elements: &so1_elems,
            strides: [16, 0, 0, 0],
        },
        sha1: stream_output_layout_digest_with_width(&so1_elems, &[16, 0, 0, 0], 0, 8),
    };
    let so4_elems = [
        so_elem(0, 0, 0, 0),
        so_elem(0, 1, 0, 4),
        so_elem(0xffff_ffff, 2, 0, 8),
        so_elem(3, 3, 2, 12),
    ];
    let so_4 = TestSo {
        chain: SoLayoutChainInput {
            elements: &so4_elems,
            strides: [16, 32, 48, 64],
        },
        sha1: stream_output_layout_digest_with_width(&so4_elems, &[16, 32, 48, 64], 0xffff_ffff, 8),
    };
    let sos = [so_0, so_1, so_4];

    let main_sh = TestShader {
        handle: 0x1001,
        sha1: parse_hex_sha1("0123456789abcdef0123456789abcdef01234567"),
    };
    let aux_sh1 = TestShader {
        handle: 0x2001,
        sha1: parse_hex_sha1("89abcdef0123456789abcdef0123456789abcdef"),
    };
    let aux_sh2 = TestShader {
        handle: 0x2002,
        sha1: parse_hex_sha1("deadbeefcafebabedeadbeefcafebabedeadbeef"),
    };

    let mut out = String::with_capacity(expected.len());

    // 1. Default
    {
        let mut case_id = 0;
        for mtl_ver in [WMT_METAL_310, WMT_METAL_320] {
            for flags in [0u32, 1, 2, 3] {
                let header = format!("kind=Default case={case_id}");
                let v = ShaderVariant::Default;
                let chain = CompileArgsChain::build_with_version(
                    &v,
                    &main_sh.sha1,
                    mtl_ver,
                    flags,
                    None,
                    None,
                );
                out.push_str(&chain.dump_chain(&header, "SM50Compile", main_sh.handle, 0));
                case_id += 1;
            }
        }
    }

    // 2. Vertex
    {
        let mut case_id = 0usize;
        let gs_vals = [0u32, 1, 0x05a3_c101, 0xffff_ffff];
        for flags in [0u32, 3] {
            for gs in gs_vals {
                for rast_dis in [false, true] {
                    for il in &ils {
                        let mtl_ver = if (case_id & 1) != 0 {
                            WMT_METAL_320
                        } else {
                            WMT_METAL_310
                        };
                        let header = format!("kind=Vertex case={case_id}");
                        let v = ShaderVariant::Vertex {
                            input_layout_sha1: il.as_ref().map(|l| l.sha1),
                            gs_passthrough: gs,
                            rasterization_disabled: rast_dis,
                        };
                        let chain = CompileArgsChain::build_with_version(
                            &v,
                            &main_sh.sha1,
                            mtl_ver,
                            flags,
                            il.as_ref().map(|l| l.chain.clone()),
                            None,
                        );
                        out.push_str(&chain.dump_chain(&header, "SM50Compile", main_sh.handle, 0));
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 3. Pixel
    {
        let mut case_id = 0usize;
        let sms = [0u32, 1, 0x0f0f_0f0f, 0xffff_ffff];
        let ums = [0u32, 1, 0xff, 0xffff_ffff];
        for flags in [0u32, 1, 2, 3] {
            for sm in sms {
                for um in ums {
                    for dsb in [false, true] {
                        for ddo in [false, true] {
                            if (flags == 1 || flags == 2) && (sm == 0x0f0f_0f0f || um == 0xff) {
                                continue;
                            }
                            let mtl_ver = if (case_id & 1) != 0 {
                                WMT_METAL_320
                            } else {
                                WMT_METAL_310
                            };
                            let header = format!("kind=Pixel case={case_id}");
                            let v = ShaderVariant::Pixel {
                                sample_mask: sm,
                                dual_source_blending: dsb,
                                disable_depth_output: ddo,
                                unorm_output_reg_mask: um,
                            };
                            let chain = CompileArgsChain::build_with_version(
                                &v,
                                &main_sh.sha1,
                                mtl_ver,
                                flags,
                                None,
                                None,
                            );
                            out.push_str(&chain.dump_chain(
                                &header,
                                "SM50Compile",
                                main_sh.handle,
                                0,
                            ));
                            case_id += 1;
                        }
                    }
                }
            }
        }
    }

    // 4. TessellationVertexHull
    {
        let mut case_id = 0usize;
        let ib_fmts = [
            SM50_INDEX_BUFFER_FORMAT_NONE,
            SM50_INDEX_BUFFER_FORMAT_UINT16,
            SM50_INDEX_BUFFER_FORMAT_UINT32,
        ];
        let tfs = [0u32, 1, 16, 64, 0xffff_ffff];
        for flags in [0u32, 3] {
            for ib_fmt in ib_fmts {
                for tf in tfs {
                    for il in &ils {
                        let mtl_ver = if (case_id & 1) != 0 {
                            WMT_METAL_320
                        } else {
                            WMT_METAL_310
                        };
                        let header = format!("kind=TessellationVertexHull case={case_id}");
                        let vs = if (case_id & 1) != 0 {
                            &aux_sh2
                        } else {
                            &aux_sh1
                        };
                        let v = ShaderVariant::TessellationVertexHull {
                            input_layout_sha1: il.as_ref().map(|l| l.sha1),
                            vs_sha1: vs.sha1,
                            index_buffer_format: ib_fmt,
                            max_potential_tess_factor: tf,
                        };
                        let chain = CompileArgsChain::build_with_version(
                            &v,
                            &main_sh.sha1,
                            mtl_ver,
                            flags,
                            il.as_ref().map(|l| l.chain.clone()),
                            None,
                        );
                        out.push_str(&chain.dump_chain(
                            &header,
                            "SM50CompileTessellationPipelineHull",
                            vs.handle,
                            main_sh.handle,
                        ));
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 5. TessellationDomain
    {
        let mut case_id = 0usize;
        let gs_vals = [0u32, 1, 0x05a3_c101, 0xffff_ffff];
        let tfs = [0u32, 1, 16, 64, 0xffff_ffff];
        for flags in [0u32, 3] {
            for gs in gs_vals {
                for tf in tfs {
                    for rast_dis in [false, true] {
                        let mtl_ver = if (case_id & 1) != 0 {
                            WMT_METAL_320
                        } else {
                            WMT_METAL_310
                        };
                        let header = format!("kind=TessellationDomain case={case_id}");
                        let hs = if (case_id & 1) != 0 {
                            &aux_sh2
                        } else {
                            &aux_sh1
                        };
                        let v = ShaderVariant::TessellationDomain {
                            hs_sha1: hs.sha1,
                            gs_passthrough: gs,
                            max_potential_tess_factor: tf,
                            rasterization_disabled: rast_dis,
                        };
                        let chain = CompileArgsChain::build_with_version(
                            &v,
                            &main_sh.sha1,
                            mtl_ver,
                            flags,
                            None,
                            None,
                        );
                        out.push_str(&chain.dump_chain(
                            &header,
                            "SM50CompileTessellationPipelineDomain",
                            hs.handle,
                            main_sh.handle,
                        ));
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 6. VertexStreamOutput
    {
        let mut case_id = 0usize;
        for flags in [0u32, 1, 2, 3] {
            for so in &sos {
                for il in &ils {
                    let mtl_ver = if (case_id & 1) != 0 {
                        WMT_METAL_320
                    } else {
                        WMT_METAL_310
                    };
                    let header = format!("kind=VertexStreamOutput case={case_id}");
                    let v = ShaderVariant::VertexStreamOutput {
                        input_layout_sha1: il.as_ref().map(|l| l.sha1),
                        so_layout_sha1: so.sha1,
                    };
                    let chain = CompileArgsChain::build_with_version(
                        &v,
                        &main_sh.sha1,
                        mtl_ver,
                        flags,
                        il.as_ref().map(|l| l.chain.clone()),
                        Some(so.chain.clone()),
                    );
                    out.push_str(&chain.dump_chain(&header, "SM50Compile", main_sh.handle, 0));
                    case_id += 1;
                }
            }
        }
    }

    // 7. GeometryVertex
    {
        let mut case_id = 0usize;
        let ib_fmts = [
            SM50_INDEX_BUFFER_FORMAT_NONE,
            SM50_INDEX_BUFFER_FORMAT_UINT16,
            SM50_INDEX_BUFFER_FORMAT_UINT32,
        ];
        for flags in [0u32, 3] {
            for ib_fmt in ib_fmts {
                for strip in [false, true] {
                    for il in &ils {
                        let mtl_ver = if (case_id & 1) != 0 {
                            WMT_METAL_320
                        } else {
                            WMT_METAL_310
                        };
                        let header = format!("kind=GeometryVertex case={case_id}");
                        let gs = if (case_id & 1) != 0 {
                            &aux_sh2
                        } else {
                            &aux_sh1
                        };
                        let v = ShaderVariant::GeometryVertex {
                            input_layout_sha1: il.as_ref().map(|l| l.sha1),
                            gs_sha1: gs.sha1,
                            index_buffer_format: ib_fmt,
                            strip_topology: strip,
                        };
                        let chain = CompileArgsChain::build_with_version(
                            &v,
                            &main_sh.sha1,
                            mtl_ver,
                            flags,
                            il.as_ref().map(|l| l.chain.clone()),
                            None,
                        );
                        out.push_str(&chain.dump_chain(
                            &header,
                            "SM50CompileGeometryPipelineVertex",
                            main_sh.handle,
                            gs.handle,
                        ));
                        case_id += 1;
                    }
                }
            }
        }
    }

    // 8. Geometry
    {
        let mut case_id = 0usize;
        for mtl_ver in [WMT_METAL_310, WMT_METAL_320] {
            for flags in [0u32, 1, 2, 3] {
                for vs in [&aux_sh1, &aux_sh2] {
                    for strip in [false, true] {
                        let header = format!("kind=Geometry case={case_id}");
                        let v = ShaderVariant::Geometry {
                            vs_sha1: vs.sha1,
                            strip_topology: strip,
                        };
                        let chain = CompileArgsChain::build_with_version(
                            &v,
                            &main_sh.sha1,
                            mtl_ver,
                            flags,
                            None,
                            None,
                        );
                        out.push_str(&chain.dump_chain(
                            &header,
                            "SM50CompileGeometryPipelineGeometry",
                            vs.handle,
                            main_sh.handle,
                        ));
                        case_id += 1;
                    }
                }
            }
        }
    }

    for (line_no, (a, e)) in out.lines().zip(expected.lines()).enumerate() {
        assert_eq!(a, e, "chains.txt mismatch at line {}", line_no + 1);
    }
    assert_eq!(out.lines().count(), expected.lines().count());
    assert_eq!(out, expected);
}

#[test]
fn digest_hashes_size_t_at_both_widths() {
    let elems = [il_elem(0, 0, 0, 31, 0, 1)];
    let s8 = input_layout_stream_with_width(&elems, 1, 8);
    let s4 = input_layout_stream_with_width(&elems, 1, 4);
    let il8 = input_layout_digest_with_width(&elems, 1, 8);
    let il4 = input_layout_digest_with_width(&elems, 1, 4);
    assert_eq!(s8.len(), s4.len() + 4);
    assert_ne!(il8, il4);

    let so_elems = [so_elem(0, 0, 0, 0)];
    let ss8 = stream_output_layout_stream_with_width(&so_elems, &[16, 0, 0, 0], 0, 8);
    let ss4 = stream_output_layout_stream_with_width(&so_elems, &[16, 0, 0, 0], 0, 4);
    let so8 = stream_output_layout_digest_with_width(&so_elems, &[16, 0, 0, 0], 0, 8);
    let so4 = stream_output_layout_digest_with_width(&so_elems, &[16, 0, 0, 0], 0, 4);
    assert_eq!(ss8.len(), ss4.len() + 4);
    assert_ne!(so8, so4);

    let v8 = ShaderVariant::Vertex {
        input_layout_sha1: Some(il8),
        gs_passthrough: 0,
        rasterization_disabled: false,
    };
    let v4 = ShaderVariant::Vertex {
        input_layout_sha1: Some(il4),
        gs_passthrough: 0,
        rasterization_disabled: false,
    };
    assert_ne!(v8.digest(0), v4.digest(0));
}

#[test]
fn flags_from_config() {
    let cfg_default = Config::from_user(Some(b""), b"", b"");
    assert_eq!(super::flags_from_config(&cfg_default), 0);

    let cfg_nan = Config::from_user(Some(b"d3d11.sampleNaNToZero = True"), b"", b"");
    assert_eq!(
        super::flags_from_config(&cfg_nan),
        SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO
    );

    let cfg_fma = Config::from_user(Some(b"d3d11.defuseFma = True"), b"", b"");
    assert_eq!(
        super::flags_from_config(&cfg_fma),
        SM50_SHADER_FLAG_DEFUSE_FMA
    );

    let cfg_both = Config::from_user(
        Some(b"d3d11.sampleNaNToZero = True\nd3d11.defuseFma = True"),
        b"",
        b"",
    );
    assert_eq!(
        super::flags_from_config(&cfg_both),
        SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO | SM50_SHADER_FLAG_DEFUSE_FMA
    );
}
