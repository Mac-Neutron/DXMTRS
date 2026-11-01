use core::{
    ffi::c_void,
    fmt::Write as _,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use winemetal_sys::wire::{
    MemPtr, WMTColorAttachmentBlendInfo, WMTRenderPipelineBlendInfo, WMTSamplerInfo,
    WmtcmdRenderSetrasterizerstate,
};

use super::{
    BlendState, DepthStencilState, RasterizerState, SamplerState,
    cache::{
        CanonicalBlendKey, CanonicalDepthStencilKey, CanonicalRasterizerKey, CanonicalSamplerKey,
        DeviceChildStateObject, StateCaches,
    },
    create_blend_state1, create_depth_stencil_state, create_rasterizer_state2,
    create_sampler_state, default_blend_desc1, default_depth_stencil_desc,
    default_rasterizer_desc2, default_sampler_desc, promote_blend_desc, translate_blend_state,
    translate_depth_stencil_state, translate_rasterizer_state, translate_sampler_state,
};
use crate::{
    abi::records::{
        D3d11BlendDesc, D3d11BlendDesc1, D3d11DepthStencilDesc, D3d11DepthStencilopDesc,
        D3d11RasterizerDesc2, D3d11RenderTargetBlendDesc, D3d11RenderTargetBlendDesc1,
        D3d11SamplerDesc,
    },
    com::{Guid, Hresult, IUnknownVtbl, S_OK, add_ref, release},
};

const GOLDEN_METAL: &str = include_str!("golden/metal.txt");

#[cfg(target_pointer_width = "64")]
const NULL_PTR: MemPtr = MemPtr { ptr: 0 };
#[cfg(target_pointer_width = "32")]
const NULL_PTR: MemPtr = MemPtr { ptr: 0, high: 0 };

const DEFAULT_RT_BLEND_DESC1: D3d11RenderTargetBlendDesc1 = D3d11RenderTargetBlendDesc1 {
    blend_enable: 0,
    logic_op_enable: 0,
    src_blend: 2,
    dest_blend: 1,
    blend_op: 1,
    src_blend_alpha: 2,
    dest_blend_alpha: 1,
    blend_op_alpha: 1,
    logic_op: 4,
    render_target_write_mask: 0x0f,
};

fn fmt_hex_g(v: f32) -> String {
    let bits = v.to_bits();
    let g = match bits {
        0xcf00_0000 => "-2.14748e+09".to_owned(),
        0x4f00_0000 => "2.14748e+09".to_owned(),
        _ if v == 0.0 => "0".to_owned(),
        _ => format!("{v}"),
    };
    format!("0x{bits:08x}({g})")
}

fn run_blend_case(
    out: &mut String,
    case_id: usize,
    label: &str,
    desc: Option<&D3d11BlendDesc1>,
    num_rt: u32,
    valid_rt: u32,
    rt_formats: &[u32; 8],
) {
    let Some(d) = desc else {
        let mut iface: *mut c_void = ptr::null_mut();
        let hr_null_out = create_blend_state1(ptr::null_mut(), ptr::null(), ptr::null_mut());
        let hr = create_blend_state1(ptr::null_mut(), ptr::null(), &raw mut iface);
        let _ = writeln!(
            out,
            "BLEND_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x}",
            hr.cast_unsigned(),
            hr_null_out.cast_unsigned()
        );
        return;
    };
    let hr_null_out = create_blend_state1(ptr::null_mut(), ptr::from_ref(d), ptr::null_mut());
    match translate_blend_state(d) {
        Ok(state) => {
            let _ = writeln!(
                out,
                "BLEND_CASE case={case_id} label={label} hr=0x00000000 hr_null_out=0x{:08x}",
                hr_null_out.cast_unsigned()
            );
            let _ = writeln!(
                out,
                "BLEND_STATE case={case_id} dual_source={} alpha_to_cov={} indep_blend={}",
                u8::from(state.is_dual_source_blending()),
                state.desc.alpha_to_coverage_enable,
                state.desc.independent_blend_enable,
            );
            let zero_color = WMTColorAttachmentBlendInfo {
                pixel_format: 0,
                rgb_blend_operation: 0,
                alpha_blend_operation: 0,
                src_rgb_blend_factor: 0,
                dst_rgb_blend_factor: 0,
                src_alpha_blend_factor: 0,
                dst_alpha_blend_factor: 0,
                write_mask: 0,
                blending_enabled: 0,
            };
            let mut info = WMTRenderPipelineBlendInfo {
                colors: [zero_color; 8],
                alpha_to_coverage_enabled: 0,
                logic_operation_enabled: 0,
                logic_operation: 0,
            };
            for (color, &fmt) in info.colors.iter_mut().zip(rt_formats) {
                color.pixel_format = fmt;
            }
            state.setup_metal_pipeline_descriptor(&mut info, num_rt, valid_rt);
            let _ = writeln!(
                out,
                "BLEND_MTL_TOP case={case_id} num_rt={num_rt} valid_rt=0x{valid_rt:02x} a2c={} logic_op_en={} logic_op={}",
                u8::from(info.alpha_to_coverage_enabled != 0),
                u8::from(info.logic_operation_enabled != 0),
                info.logic_operation
            );
            for (rt, c) in info.colors.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "BLEND_MTL_RT case={case_id} rt={rt} fmt={} blend_en={} write_mask=0x{:x} rgb_op={} alpha_op={} src_rgb={} dst_rgb={} src_a={} dst_a={}",
                    c.pixel_format,
                    u8::from(c.blending_enabled != 0),
                    c.write_mask,
                    c.rgb_blend_operation,
                    c.alpha_blend_operation,
                    c.src_rgb_blend_factor,
                    c.dst_rgb_blend_factor,
                    c.src_alpha_blend_factor,
                    c.dst_alpha_blend_factor
                );
            }
        }
        Err(hr) => {
            let _ = writeln!(
                out,
                "BLEND_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x}",
                hr.cast_unsigned(),
                hr_null_out.cast_unsigned()
            );
        }
    }
}

fn run_raster_case(
    out: &mut String,
    case_id: usize,
    label: &str,
    desc: Option<D3d11RasterizerDesc2>,
) {
    let Some(d) = desc else {
        let mut iface: *mut c_void = ptr::null_mut();
        let hr_null_out = create_rasterizer_state2(ptr::null_mut(), ptr::null(), ptr::null_mut());
        let hr = create_rasterizer_state2(ptr::null_mut(), ptr::null(), &raw mut iface);
        let _ = writeln!(
            out,
            "RASTER_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x}",
            hr.cast_unsigned(),
            hr_null_out.cast_unsigned()
        );
        return;
    };
    let hr_null_out = create_rasterizer_state2(ptr::null_mut(), &raw const d, ptr::null_mut());
    match translate_rasterizer_state(&d) {
        Ok(state) => {
            let _ = writeln!(
                out,
                "RASTER_CASE case={case_id} label={label} hr=0x00000000 hr_null_out=0x{:08x}",
                hr_null_out.cast_unsigned()
            );
            let mut cmd = WmtcmdRenderSetrasterizerstate {
                r#type: 0,
                reserved: [0; 3],
                next: NULL_PTR,
                fill_mode: 0,
                cull_mode: 0,
                depth_clip_mode: 0,
                winding: 0,
                depth_bias: 0.0,
                scole_scale: 0.0,
                depth_bias_clamp: 0.0,
            };
            state.setup(&mut cmd);
            let _ = writeln!(
                out,
                "RASTER_STATE case={case_id} scissor={} uav_samples={} fill={} cull={} clip={} winding={} bias={} slope={} clamp={} d1_forced={} d2_conserv={}",
                u8::from(state.is_scissor_enabled()),
                state.uav_only_sample_count(),
                cmd.fill_mode,
                cmd.cull_mode,
                cmd.depth_clip_mode,
                cmd.winding,
                fmt_hex_g(cmd.depth_bias),
                fmt_hex_g(cmd.scole_scale),
                fmt_hex_g(cmd.depth_bias_clamp),
                state.desc.forced_sample_count,
                state.desc.conservative_raster,
            );
        }
        Err(hr) => {
            let _ = writeln!(
                out,
                "RASTER_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x}",
                hr.cast_unsigned(),
                hr_null_out.cast_unsigned()
            );
        }
    }
}

fn run_ds_case(out: &mut String, case_id: usize, label: &str, desc: Option<D3d11DepthStencilDesc>) {
    let Some(d) = desc else {
        let mut iface: *mut c_void = ptr::null_mut();
        let hr_null_out = create_depth_stencil_state(ptr::null_mut(), ptr::null(), ptr::null_mut());
        let hr = create_depth_stencil_state(ptr::null_mut(), ptr::null(), &raw mut iface);
        let _ = writeln!(
            out,
            "DS_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x} mtl_states_created=0",
            hr.cast_unsigned(),
            hr_null_out.cast_unsigned()
        );
        return;
    };
    let hr_null_out = create_depth_stencil_state(ptr::null_mut(), &raw const d, ptr::null_mut());
    match translate_depth_stencil_state(&d, None) {
        Ok(state) => {
            let _ = writeln!(
                out,
                "DS_CASE case={case_id} label={label} hr=0x00000000 hr_null_out=0x{:08x} mtl_states_created={}",
                hr_null_out.cast_unsigned(),
                state.mtl_states_created
            );
            let _ = writeln!(
                out,
                "DS_STATE case={case_id} is_enabled={} depth_en={} write_mask={} depth_func={} stencil_en={} read_mask=0x{:02x} write_mask_s=0x{:02x}",
                u8::from(state.is_enabled()),
                state.desc.depth_enable,
                state.desc.depth_write_mask,
                state.desc.depth_func,
                state.desc.stencil_enable,
                state.desc.stencil_read_mask,
                state.desc.stencil_write_mask,
            );
            for planar in 0..4 {
                let info = &state.infos[planar];
                let rel = state.rel_handles[planar];
                let f = &info.front_stencil;
                let b = &info.back_stencil;
                let _ = writeln!(
                    out,
                    "DS_MTL case={case_id} planar={planar} rel_handle={rel} depth_cmp={} depth_write={} front={{en={},pass={},sfail={},dfail={},cmp={},wmask=0x{:02x},rmask=0x{:02x}}} back={{en={},pass={},sfail={},dfail={},cmp={},wmask=0x{:02x},rmask=0x{:02x}}}",
                    info.depth_compare_function,
                    u8::from(info.depth_write_enabled != 0),
                    u8::from(f.enabled != 0),
                    f.depth_stencil_pass_op,
                    f.stencil_fail_op,
                    f.depth_fail_op,
                    f.stencil_compare_function,
                    f.write_mask,
                    f.read_mask,
                    u8::from(b.enabled != 0),
                    b.depth_stencil_pass_op,
                    b.stencil_fail_op,
                    b.depth_fail_op,
                    b.stencil_compare_function,
                    b.write_mask,
                    b.read_mask
                );
            }
        }
        Err(hr) => {
            let _ = writeln!(
                out,
                "DS_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x} mtl_states_created=0",
                hr.cast_unsigned(),
                hr_null_out.cast_unsigned()
            );
        }
    }
}

fn fmt_sampler_info(tag: &str, case_id: usize, info: &WMTSamplerInfo) -> String {
    format!(
        "SAMPLER_MTL_{tag} case={case_id} min={} mag={} mip={} addr_s={} addr_t={} addr_r={} border={} cmp={} lod_min={} lod_max={} max_aniso={} norm_coords={} lod_avg={} arg_buf={}",
        info.min_filter,
        info.mag_filter,
        info.mip_filter,
        info.s_address_mode,
        info.t_address_mode,
        info.r_address_mode,
        info.border_color,
        info.compare_function,
        fmt_hex_g(info.lod_min_clamp),
        fmt_hex_g(info.lod_max_clamp),
        info.max_anisotroy,
        u8::from(info.normalized_coords != 0),
        u8::from(info.lod_average != 0),
        u8::from(info.support_argument_buffers != 0),
    )
}

fn run_sampler_case(out: &mut String, case_id: usize, label: &str, desc: Option<D3d11SamplerDesc>) {
    let Some(d) = desc else {
        let mut iface: *mut c_void = ptr::null_mut();
        let hr_null_out = create_sampler_state(ptr::null_mut(), ptr::null(), ptr::null_mut());
        let hr = create_sampler_state(ptr::null_mut(), ptr::null(), &raw mut iface);
        let _ = writeln!(
            out,
            "SAMPLER_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x} mtl_samplers_created=0",
            hr.cast_unsigned(),
            hr_null_out.cast_unsigned()
        );
        return;
    };
    let hr_null_out = create_sampler_state(ptr::null_mut(), &raw const d, ptr::null_mut());
    match translate_sampler_state(&d, None) {
        Ok(state) => {
            let _ = writeln!(
                out,
                "SAMPLER_CASE case={case_id} label={label} hr=0x00000000 hr_null_out=0x{:08x} mtl_samplers_created=2",
                hr_null_out.cast_unsigned()
            );
            let _ = writeln!(
                out,
                "SAMPLER_STATE case={case_id} lod_bias={}",
                fmt_hex_g(state.lod_bias)
            );
            let _ = writeln!(
                out,
                "{}",
                fmt_sampler_info("PRIMARY", case_id, &state.primary_info)
            );
            let _ = writeln!(
                out,
                "{}",
                fmt_sampler_info("CUBE", case_id, &state.cube_info)
            );
        }
        Err(hr) => {
            let _ = writeln!(
                out,
                "SAMPLER_CASE case={case_id} label={label} hr=0x{:08x} hr_null_out=0x{:08x} mtl_samplers_created=0",
                hr.cast_unsigned(),
                hr_null_out.cast_unsigned()
            );
        }
    }
}

#[test]
fn converters_are_the_golden() {
    let mut actual = String::with_capacity(GOLDEN_METAL.len());

    // Blend tests (`run_blend_tests`, `metal.cpp:161-299`)
    let default_fmts: [u32; 8] = [70, 80, 53, 0, 115, 70, 53, 70];
    let mut case_id = 0usize;

    run_blend_case(
        &mut actual,
        case_id,
        "null_desc",
        None,
        8,
        0xff,
        &default_fmts,
    );
    case_id += 1;

    run_blend_case(
        &mut actual,
        case_id,
        "default_blend",
        Some(&default_blend_desc1()),
        8,
        0xff,
        &default_fmts,
    );
    case_id += 1;

    {
        let mut d = default_blend_desc1();
        d.render_target[0].blend_enable = 1;
        d.render_target[0].logic_op_enable = 1;
        run_blend_case(
            &mut actual,
            case_id,
            "invalid_blend_and_logicop_rt0",
            Some(&d),
            8,
            0xff,
            &default_fmts,
        );
        case_id += 1;
    }

    {
        let mut d = default_blend_desc1();
        d.independent_blend_enable = 0;
        d.render_target[1].blend_enable = 1;
        d.render_target[1].logic_op_enable = 1;
        run_blend_case(
            &mut actual,
            case_id,
            "valid_blend_and_logicop_rt1_when_not_indep",
            Some(&d),
            8,
            0xff,
            &default_fmts,
        );
        case_id += 1;
    }

    {
        let mut d = default_blend_desc1();
        d.independent_blend_enable = 1;
        d.render_target = [DEFAULT_RT_BLEND_DESC1; 8];
        d.render_target[3].blend_enable = 1;
        d.render_target[3].logic_op_enable = 1;
        run_blend_case(
            &mut actual,
            case_id,
            "invalid_blend_and_logicop_rt3_when_indep",
            Some(&d),
            8,
            0xff,
            &default_fmts,
        );
        case_id += 1;
    }

    {
        let mut d = default_blend_desc1();
        d.alpha_to_coverage_enable = 5;
        d.independent_blend_enable = 7;
        d.render_target = [DEFAULT_RT_BLEND_DESC1; 8];
        run_blend_case(
            &mut actual,
            case_id,
            "noncanonical_bools",
            Some(&d),
            4,
            0x0b,
            &default_fmts,
        );
        case_id += 1;
    }

    for bf in 1..=19u32 {
        for pos in 0..4u32 {
            let mut d = default_blend_desc1();
            d.render_target[0].blend_enable = 1;
            d.render_target[0].src_blend = if pos == 0 { bf } else { 2 };
            d.render_target[0].dest_blend = if pos == 1 { bf } else { 1 };
            d.render_target[0].src_blend_alpha = if pos == 2 { bf } else { 2 };
            d.render_target[0].dest_blend_alpha = if pos == 3 { bf } else { 1 };
            d.render_target[0].blend_op = 1 + ((bf + pos) % 5);
            d.render_target[0].blend_op_alpha = 1 + ((bf + pos + 2) % 5);
            d.render_target[0].render_target_write_mask = ((bf + pos) & 0xf) as u8;
            let label = format!("blend_factor_{bf}_pos_{pos}");
            run_blend_case(
                &mut actual,
                case_id,
                &label,
                Some(&d),
                4,
                0x0f,
                &default_fmts,
            );
            case_id += 1;
        }
    }

    {
        let mut d = default_blend_desc1();
        d.render_target[0].blend_enable = 0;
        d.render_target[0].src_blend = 16;
        run_blend_case(
            &mut actual,
            case_id,
            "dual_source_factor_but_blend_disabled",
            Some(&d),
            2,
            0x03,
            &default_fmts,
        );
        case_id += 1;
    }

    for lop in 0..=15u32 {
        for indep in [0i32, 1i32] {
            let mut d = default_blend_desc1();
            d.independent_blend_enable = indep;
            d.render_target = [DEFAULT_RT_BLEND_DESC1; 8];
            d.render_target[0].logic_op_enable = 1;
            d.render_target[0].logic_op = lop;
            let label = format!("logic_op_{lop}_indep_{indep}");
            run_blend_case(
                &mut actual,
                case_id,
                &label,
                Some(&d),
                2,
                0x03,
                &default_fmts,
            );
            case_id += 1;
        }
    }

    {
        let mut d0 = default_blend_desc1();
        d0.independent_blend_enable = 1;
        let mut d1 = default_blend_desc1();
        d1.independent_blend_enable = 1;
        for (i, (rt0, rt1)) in d0
            .render_target
            .iter_mut()
            .zip(&mut d1.render_target)
            .enumerate()
        {
            let iu = u32::try_from(i).unwrap_or(0);
            let ib = u8::try_from(i).unwrap_or(0);
            *rt0 = DEFAULT_RT_BLEND_DESC1;
            rt0.blend_enable = i32::from((i & 1) != 0);
            rt0.src_blend = 1 + iu;
            rt0.dest_blend = 2 + iu;
            rt0.blend_op = 1 + (iu % 5);
            rt0.src_blend_alpha = 3 + iu;
            rt0.dest_blend_alpha = 4 + iu;
            rt0.blend_op_alpha = 1 + ((iu + 1) % 5);
            rt0.render_target_write_mask = ib;

            *rt1 = *rt0;
            rt1.render_target_write_mask = 8 + ib;
        }
        run_blend_case(
            &mut actual,
            case_id,
            "indep_write_masks_0_to_7",
            Some(&d0),
            8,
            0xff,
            &default_fmts,
        );
        case_id += 1;
        run_blend_case(
            &mut actual,
            case_id,
            "indep_write_masks_8_to_15",
            Some(&d1),
            8,
            0xaa,
            &default_fmts,
        );
        case_id += 1;
        run_blend_case(
            &mut actual,
            case_id,
            "indep_partial_num_rt_3",
            Some(&d0),
            3,
            0xff,
            &default_fmts,
        );
        case_id += 1;
    }

    {
        let zero_legacy_rt = D3d11RenderTargetBlendDesc {
            blend_enable: 0,
            src_blend: 0,
            dest_blend: 0,
            blend_op: 0,
            src_blend_alpha: 0,
            dest_blend_alpha: 0,
            blend_op_alpha: 0,
            render_target_write_mask: 0,
        };
        let mut legacy = D3d11BlendDesc {
            alpha_to_coverage_enable: 1,
            independent_blend_enable: 0,
            render_target: [zero_legacy_rt; 8],
        };
        legacy.render_target[0] = D3d11RenderTargetBlendDesc {
            blend_enable: 1,
            src_blend: 5,
            dest_blend: 6,
            blend_op: 1,
            src_blend_alpha: 2,
            dest_blend_alpha: 1,
            blend_op_alpha: 5,
            render_target_write_mask: 0x7,
        };
        let upgraded = promote_blend_desc(&legacy);
        run_blend_case(
            &mut actual,
            case_id,
            "upgraded_legacy_blend_desc",
            Some(&upgraded),
            4,
            0x05,
            &default_fmts,
        );
    }

    // Rasterizer tests (`run_rasterizer_tests`, `metal.cpp:343-398`)
    let mut r_case_id = 0usize;
    run_raster_case(&mut actual, r_case_id, "null_desc", None);
    r_case_id += 1;

    run_raster_case(
        &mut actual,
        r_case_id,
        "default_rasterizer",
        Some(default_rasterizer_desc2()),
    );
    r_case_id += 1;

    for fill in [2u32, 3u32] {
        for cull in [1u32, 2u32, 3u32] {
            for fccw in [0i32, 1i32] {
                for dclip in [0i32, 1i32] {
                    for scis in [0i32, 1i32] {
                        let mut d = default_rasterizer_desc2();
                        d.fill_mode = fill;
                        d.cull_mode = cull;
                        d.front_counter_clockwise = fccw;
                        d.depth_clip_enable = dclip;
                        d.scissor_enable = scis;
                        let label = format!(
                            "fill_{fill}_cull_{cull}_fccw_{fccw}_dclip_{dclip}_scis_{scis}"
                        );
                        run_raster_case(&mut actual, r_case_id, &label, Some(d));
                        r_case_id += 1;
                    }
                }
            }
        }
    }

    let biases: [i32; 7] = [0, -1, 1, -100_000, 100_000, i32::MIN, i32::MAX];
    let slopes: [f32; 5] = [0.0, -0.5, 1.25, -16.0, 32.5];
    let clamps: [f32; 4] = [0.0, -1.0, 0.001, 100.0];
    let sample_counts: [u32; 6] = [0, 1, 2, 4, 8, 16];
    for i in 0..12usize {
        let mut d = default_rasterizer_desc2();
        d.depth_bias = biases[i % biases.len()];
        d.slope_scaled_depth_bias = slopes[i % slopes.len()];
        d.depth_bias_clamp = clamps[i % clamps.len()];
        d.forced_sample_count = sample_counts[i % sample_counts.len()];
        d.multisample_enable = i32::from((i & 1) != 0);
        d.antialiased_line_enable = i32::from((i & 2) != 0);
        d.conservative_raster = u32::from((i & 4) != 0);
        let label = format!("bias_samples_conserv_{i}");
        run_raster_case(&mut actual, r_case_id, &label, Some(d));
        r_case_id += 1;
    }

    // Depth-stencil tests (`run_depth_stencil_tests`, `metal.cpp:460-510`)
    let mut ds_case_id = 0usize;
    run_ds_case(&mut actual, ds_case_id, "null_desc", None);
    ds_case_id += 1;

    run_ds_case(
        &mut actual,
        ds_case_id,
        "default_depth_stencil",
        Some(default_depth_stencil_desc()),
    );
    ds_case_id += 1;

    for de in [0i32, 1i32] {
        for se in [0i32, 1i32] {
            for dwm in [0u32, 1u32] {
                let mut d = default_depth_stencil_desc();
                d.depth_enable = de;
                d.stencil_enable = se;
                d.depth_write_mask = dwm;
                d.depth_func = 4;
                d.stencil_read_mask = 0x5a;
                d.stencil_write_mask = 0xa5;
                d.front_face = D3d11DepthStencilopDesc {
                    stencil_fail_op: 3,
                    stencil_depth_fail_op: 4,
                    stencil_pass_op: 8,
                    stencil_func: 3,
                };
                d.back_face = D3d11DepthStencilopDesc {
                    stencil_fail_op: 2,
                    stencil_depth_fail_op: 6,
                    stencil_pass_op: 7,
                    stencil_func: 6,
                };
                let label = format!("de_{de}_se_{se}_dwm_{dwm}");
                run_ds_case(&mut actual, ds_case_id, &label, Some(d));
                ds_case_id += 1;
            }
        }
    }

    for vb in 0..=8u8 {
        let v = u32::from(vb);
        let mut d = default_depth_stencil_desc();
        d.depth_enable = 1;
        d.depth_write_mask = u32::from((v & 1) != 0);
        d.depth_func = v;
        d.stencil_enable = 1;
        d.stencil_read_mask = vb * 17;
        d.stencil_write_mask = 255 - vb * 17;
        d.front_face = D3d11DepthStencilopDesc {
            stencil_fail_op: v,
            stencil_depth_fail_op: (v + 1) % 9,
            stencil_pass_op: (v + 2) % 9,
            stencil_func: v,
        };
        d.back_face = D3d11DepthStencilopDesc {
            stencil_fail_op: (v + 3) % 9,
            stencil_depth_fail_op: (v + 4) % 9,
            stencil_pass_op: (v + 5) % 9,
            stencil_func: (v + 1) % 9,
        };
        let label = format!("cmp_and_stencil_op_{v}");
        run_ds_case(&mut actual, ds_case_id, &label, Some(d));
        ds_case_id += 1;
    }

    // Sampler tests (`run_sampler_tests`, `metal.cpp:562-668`)
    let mut s_case_id = 0usize;
    run_sampler_case(&mut actual, s_case_id, "null_desc", None);
    s_case_id += 1;

    run_sampler_case(
        &mut actual,
        s_case_id,
        "default_sampler",
        Some(default_sampler_desc()),
    );
    s_case_id += 1;

    let filters: [u32; 24] = [
        0x00, 0x01, 0x04, 0x05, 0x10, 0x11, 0x14, 0x15, 0x55, 0x80, 0x81, 0x84, 0x85, 0x90, 0x91,
        0x94, 0x95, 0xd5, 0x100, 0x115, 0x155, 0x180, 0x195, 0x1d5,
    ];
    for (idx, &flt) in (0u16..).zip(&filters) {
        let mut d = default_sampler_desc();
        d.filter = flt;
        d.max_anisotropy = 8;
        d.comparison_func = 1 + (u32::from(idx) % 8);
        d.min_lod = -100.0;
        d.max_lod = 1000.0;
        d.mip_lod_bias = f32::from(idx).mul_add(0.25, -0.5);
        let label = format!("filter_0x{flt:x}");
        run_sampler_case(&mut actual, s_case_id, &label, Some(d));
        s_case_id += 1;
    }

    let anisos: [u32; 9] = [0, 1, 2, 4, 8, 16, 17, 32, 0xffff_ffff];
    for aniso in anisos {
        for flt in [0x55u32, 0xd5u32, 0x15u32] {
            let mut d = default_sampler_desc();
            d.filter = flt;
            d.max_anisotropy = aniso;
            d.comparison_func = 2;
            let label = format!("aniso_{aniso}_flt_0x{flt:x}");
            run_sampler_case(&mut actual, s_case_id, &label, Some(d));
            s_case_id += 1;
        }
    }

    for u in 0..=6u32 {
        let mut d = default_sampler_desc();
        d.address_u = u;
        d.address_v = (u + 1) % 7;
        d.address_w = (u + 2) % 7;
        let label = format!("addr_u{u}_v{}_w{}", (u + 1) % 7, (u + 2) % 7);
        run_sampler_case(&mut actual, s_case_id, &label, Some(d));
        s_case_id += 1;
    }

    let border_colors: [[f32; 4]; 5] = [
        [0.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
        [1.0, 1.0, 1.0, 1.0],
        [0.5, 0.5, 0.5, 1.0],
        [1.0, 0.0, 0.0, 1.0],
    ];
    for (bc, &color) in border_colors.iter().enumerate() {
        for which_axis in 0..4u32 {
            let mut d = default_sampler_desc();
            d.address_u = if which_axis == 0 { 4 } else { 1 };
            d.address_v = if which_axis == 1 { 4 } else { 2 };
            d.address_w = if which_axis == 2 { 4 } else { 3 };
            d.border_color = color;
            let label = format!("border_bc{bc}_axis{which_axis}");
            run_sampler_case(&mut actual, s_case_id, &label, Some(d));
            s_case_id += 1;
        }
    }

    for cf in 0..=9u32 {
        for flt in [0x95u32, 0x15u32] {
            let mut d = default_sampler_desc();
            d.filter = flt;
            d.comparison_func = cf;
            let label = format!("cmpfunc_{cf}_flt_0x{flt:x}");
            run_sampler_case(&mut actual, s_case_id, &label, Some(d));
            s_case_id += 1;
        }
    }

    for (line_no, (a, e)) in actual.lines().zip(GOLDEN_METAL.lines()).enumerate() {
        assert_eq!(a, e, "metal.txt mismatch at line {}", line_no + 1);
    }
    assert_eq!(actual.lines().count(), GOLDEN_METAL.lines().count());
    assert_eq!(actual, GOLDEN_METAL);
}

#[test]
fn equal_descs_share_an_object() {
    let caches = StateCaches::default();

    let mut b1 = default_blend_desc1();
    b1.alpha_to_coverage_enable = 1;
    b1.independent_blend_enable = 0;
    b1.render_target[0].blend_enable = 1;
    b1.render_target[3].src_blend = 7;

    let mut b2 = default_blend_desc1();
    b2.alpha_to_coverage_enable = 42;
    b2.independent_blend_enable = 0;
    b2.render_target[0].blend_enable = -1;
    b2.render_target[3].src_blend = 12;

    let blend_a = caches
        .blend
        .get_or_create(CanonicalBlendKey::from_desc(&b1), || {
            Ok(DeviceChildStateObject::new(
                &super::BLEND_STATE_VTBL,
                ptr::null_mut(),
                translate_blend_state(&b1)?,
            ))
        })
        .expect("b1 creation succeeds");
    let blend_b = caches
        .blend
        .get_or_create(CanonicalBlendKey::from_desc(&b2), || {
            Ok(DeviceChildStateObject::new(
                &super::BLEND_STATE_VTBL,
                ptr::null_mut(),
                translate_blend_state(&b2)?,
            ))
        })
        .expect("b2 creation succeeds");
    assert_eq!(blend_a, blend_b);
    assert_eq!(caches.blend.len(), 1);
    assert!(BlendState::from_iface(blend_a.cast()).is_some());

    let mut r1 = default_rasterizer_desc2();
    r1.scissor_enable = 1;
    r1.depth_bias_clamp = 0.0;
    let mut r2 = default_rasterizer_desc2();
    r2.scissor_enable = 99;
    r2.depth_bias_clamp = -0.0;
    let rast_a = caches
        .rasterizer
        .get_or_create(CanonicalRasterizerKey::from_desc(&r1), || {
            Ok(DeviceChildStateObject::new(
                &super::RASTERIZER_STATE_VTBL,
                ptr::null_mut(),
                translate_rasterizer_state(&r1)?,
            ))
        })
        .expect("r1 creation succeeds");
    let rast_b = caches
        .rasterizer
        .get_or_create(CanonicalRasterizerKey::from_desc(&r2), || {
            Ok(DeviceChildStateObject::new(
                &super::RASTERIZER_STATE_VTBL,
                ptr::null_mut(),
                translate_rasterizer_state(&r2)?,
            ))
        })
        .expect("r2 creation succeeds");
    assert_eq!(rast_a, rast_b);
    assert_eq!(caches.rasterizer.len(), 1);
    assert!(RasterizerState::from_iface(rast_a.cast()).is_some());

    let mut ds1 = default_depth_stencil_desc();
    ds1.depth_enable = 1;
    ds1.stencil_enable = 0;
    ds1.stencil_read_mask = 0x11;
    let mut ds2 = default_depth_stencil_desc();
    ds2.depth_enable = 7;
    ds2.stencil_enable = 0;
    ds2.stencil_read_mask = 0xee;
    let ds_a = caches
        .depth_stencil
        .get_or_create(CanonicalDepthStencilKey::from_desc(&ds1), || {
            Ok(DeviceChildStateObject::new(
                &super::DEPTH_STENCIL_STATE_VTBL,
                ptr::null_mut(),
                translate_depth_stencil_state(&ds1, None)?,
            ))
        })
        .expect("ds1 creation succeeds");
    let ds_b = caches
        .depth_stencil
        .get_or_create(CanonicalDepthStencilKey::from_desc(&ds2), || {
            Ok(DeviceChildStateObject::new(
                &super::DEPTH_STENCIL_STATE_VTBL,
                ptr::null_mut(),
                translate_depth_stencil_state(&ds2, None)?,
            ))
        })
        .expect("ds2 creation succeeds");
    assert_eq!(ds_a, ds_b);
    assert_eq!(caches.depth_stencil.len(), 1);
    assert!(DepthStencilState::from_iface(ds_a.cast()).is_some());

    let mut s1 = default_sampler_desc();
    s1.mip_lod_bias = 0.0;
    let mut s2 = default_sampler_desc();
    s2.mip_lod_bias = -0.0;
    let samp_a = caches
        .sampler
        .get_or_create(CanonicalSamplerKey::from_desc(&s1), || {
            Ok(DeviceChildStateObject::new(
                &super::SAMPLER_STATE_VTBL,
                ptr::null_mut(),
                translate_sampler_state(&s1, None)?,
            ))
        })
        .expect("s1 creation succeeds");
    let samp_b = caches
        .sampler
        .get_or_create(CanonicalSamplerKey::from_desc(&s2), || {
            Ok(DeviceChildStateObject::new(
                &super::SAMPLER_STATE_VTBL,
                ptr::null_mut(),
                translate_sampler_state(&s2, None)?,
            ))
        })
        .expect("s2 creation succeeds");
    assert_eq!(samp_a, samp_b);
    assert_eq!(caches.sampler.len(), 1);
    assert!(SamplerState::from_iface(samp_a.cast()).is_some());
}

#[repr(C)]
struct MockDevice {
    vtbl: *const IUnknownVtbl,
    refs: AtomicU32,
}

unsafe extern "system" fn mock_qi(
    _this: *mut c_void,
    _riid: *const Guid,
    _out: *mut *mut c_void,
) -> Hresult {
    S_OK
}

unsafe extern "system" fn mock_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` points to a `MockDevice`.
    unsafe {
        (*this.cast::<MockDevice>())
            .refs
            .fetch_add(1, Ordering::SeqCst)
            + 1
    }
}

unsafe extern "system" fn mock_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` points to a `MockDevice`.
    unsafe {
        (*this.cast::<MockDevice>())
            .refs
            .fetch_sub(1, Ordering::SeqCst)
            - 1
    }
}

static MOCK_DEVICE_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: mock_qi,
    add_ref: mock_add_ref,
    release: mock_release,
};

#[test]
fn released_object_comes_back() {
    let dev = MockDevice {
        vtbl: &raw const MOCK_DEVICE_VTBL,
        refs: AtomicU32::new(1),
    };
    let dev_ptr: *mut c_void = (&raw const dev).cast_mut().cast();

    let desc = default_blend_desc1();
    let mut state1: *mut c_void = ptr::null_mut();
    let hr = create_blend_state1(dev_ptr, &raw const desc, &raw mut state1);
    assert_eq!(hr, S_OK);
    assert!(!state1.is_null());
    assert_eq!(dev.refs.load(Ordering::SeqCst), 2);

    let mut state2: *mut c_void = ptr::null_mut();
    let hr2 = create_blend_state1(dev_ptr, &raw const desc, &raw mut state2);
    assert_eq!(hr2, S_OK);
    assert_eq!(state1, state2);
    assert_eq!(dev.refs.load(Ordering::SeqCst), 2);

    // SAFETY: `state1` is a live COM state object.
    unsafe { add_ref(state1) };
    assert_eq!(dev.refs.load(Ordering::SeqCst), 2);

    // SAFETY: `state1` is a live COM state object.
    unsafe { release(state1) };
    // SAFETY: `state2` is a live COM state object.
    unsafe { release(state2) };
    assert_eq!(dev.refs.load(Ordering::SeqCst), 2);

    // Final external release (1 -> 0) drops the state object's hold on the device (2 -> 1),
    // while keeping the state object in the cache.
    // SAFETY: `state1` is a live COM state object.
    unsafe { release(state1) };
    assert_eq!(dev.refs.load(Ordering::SeqCst), 1);

    let mut state3: *mut c_void = ptr::null_mut();
    let hr3 = create_blend_state1(dev_ptr, &raw const desc, &raw mut state3);
    assert_eq!(hr3, S_OK);
    assert_eq!(state1, state3);
    assert_eq!(dev.refs.load(Ordering::SeqCst), 2);

    // SAFETY: `state3` is a live COM state object.
    unsafe { release(state3) };
    assert_eq!(dev.refs.load(Ordering::SeqCst), 1);
}

#[test]
fn nan_desc_never_equals() {
    let caches = StateCaches::default();

    let mut r_nan = default_rasterizer_desc2();
    r_nan.slope_scaled_depth_bias = f32::NAN;
    let kr1 = CanonicalRasterizerKey::from_desc(&r_nan);
    let kr2 = CanonicalRasterizerKey::from_desc(&r_nan);
    assert_ne!(kr1, kr2);

    let pr1 = caches
        .rasterizer
        .get_or_create(kr1, || {
            Ok(DeviceChildStateObject::new(
                &super::RASTERIZER_STATE_VTBL,
                ptr::null_mut(),
                translate_rasterizer_state(&r_nan)?,
            ))
        })
        .expect("first NaN rasterizer state succeeds");
    let pr2 = caches
        .rasterizer
        .get_or_create(kr2, || {
            Ok(DeviceChildStateObject::new(
                &super::RASTERIZER_STATE_VTBL,
                ptr::null_mut(),
                translate_rasterizer_state(&r_nan)?,
            ))
        })
        .expect("second NaN rasterizer state succeeds");
    assert_ne!(pr1, pr2);
    assert_eq!(caches.rasterizer.len(), 2);

    let mut s_nan = default_sampler_desc();
    s_nan.mip_lod_bias = f32::NAN;
    let ks1 = CanonicalSamplerKey::from_desc(&s_nan);
    let ks2 = CanonicalSamplerKey::from_desc(&s_nan);
    assert_ne!(ks1, ks2);

    let ps1 = caches
        .sampler
        .get_or_create(ks1, || {
            Ok(DeviceChildStateObject::new(
                &super::SAMPLER_STATE_VTBL,
                ptr::null_mut(),
                translate_sampler_state(&s_nan, None)?,
            ))
        })
        .expect("first NaN sampler state succeeds");
    let ps2 = caches
        .sampler
        .get_or_create(ks2, || {
            Ok(DeviceChildStateObject::new(
                &super::SAMPLER_STATE_VTBL,
                ptr::null_mut(),
                translate_sampler_state(&s_nan, None)?,
            ))
        })
        .expect("second NaN sampler state succeeds");
    assert_ne!(ps1, ps2);
    assert_eq!(caches.sampler.len(), 2);
}
