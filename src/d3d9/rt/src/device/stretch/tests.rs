use dxmt_fakes::take_calls;
use mtld3d_core::{ids::TextureId, stretch_rect::BlitDecode};
use winemetal_sys::{
    Obj,
    wire::{
        WMT_LOAD_ACTION_LOAD, WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB,
        WMT_PIXEL_FORMAT_RGBA8_UNORM, WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES,
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE, WMT_RENDER_COMMAND_SET_VERTEX_BYTES,
        WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE, WMT_SAMPLER_MIN_MAG_FILTER_LINEAR,
        WMT_SAMPLER_MIN_MAG_FILTER_NEAREST, WMT_SAMPLER_MIP_FILTER_NEAREST, WMT_STORE_ACTION_STORE,
        WMT_TEXTURE_SWIZZLE_BLUE, WMT_TEXTURE_SWIZZLE_GREEN, WMT_TEXTURE_SWIZZLE_ONE,
        WMT_TEXTURE_SWIZZLE_RED, WMT_TEXTURE_TYPE2D,
    },
};

use super::{StretchQuad, quad_uniforms};
use crate::device::{
    CoreDevice, CoreTextureDesc, CoreTextureFlags, DrawData, TargetRef, TextureKind,
    fixture::{
        at, core_on_fakes, ff_binds, key, line, ms_desc, passes, target_desc, targets,
        textures_made, triangle, twinned_ms_desc,
    },
    target::Shape,
    view_format,
};

const BGRA8: u32 = WMT_PIXEL_FORMAT_BGRA8_UNORM;
const SRGB: u32 = WMT_PIXEL_FORMAT_BGRA8_UNORM_S_RGB;
/// A whole 16x16 rect from the origin.
const SIXTEEN: [u32; 4] = [0, 0, 16, 16];

/// A stored texture of `desc` under a fresh id, and its handle; the calls so far are taken.
///
/// The handle is the last texture made: a device's first take also holds its back buffer's.
fn stored(core: &mut CoreDevice, device: u64, desc: &CoreTextureDesc) -> (TextureId, String) {
    let id = TextureId::new_unique();
    core.texture_create(id, desc);
    core.drain();
    let made = textures_made(&take_calls(device));
    let handle = made.last().expect("the texture made").clone();
    (id, handle)
}

/// A square `size` BGRA8 2D render-target texture of `levels` levels, as [`stored`] makes it.
fn target_2d(core: &mut CoreDevice, device: u64, size: u32, levels: u32) -> (TextureId, String) {
    stored(
        core,
        device,
        &target_desc(BGRA8, TextureKind::Tex2d, size, levels, 4),
    )
}

/// Level `level` of the 2D texture `id`.
const fn level_of(id: TextureId, level: u32) -> TargetRef {
    TargetRef::Texture {
        id,
        level,
        face: None,
    }
}

/// A point-sampled copy of `src_rect` of `src` through `src_view` onto `dst_rect` of `dst`.
const fn quad(
    src: TargetRef,
    src_rect: [u32; 4],
    src_view: u32,
    dst: TargetRef,
    dst_rect: [u32; 4],
) -> StretchQuad {
    StretchQuad {
        src,
        src_rect,
        src_view,
        dst,
        dst_rect,
        linear: false,
        decode: BlitDecode::None.uniform(),
    }
}

/// `value` as the fakes print a float: its bits in hex.
fn hex(value: f32) -> String {
    format!("0x{:08x}", value.to_bits())
}

/// The handle of the view of `texture` whose line ends `rest` (`format= type= mip= slice=`).
fn view_of(calls: &[String], texture: &str, rest: &str) -> String {
    let made = format!(" of {texture} {rest}");
    calls
        .iter()
        .filter_map(|c| c.strip_prefix("MTLTexture_newTextureView "))
        .find_map(|c| c.strip_suffix(made.as_str()))
        .unwrap_or_else(|| panic!("no view{made}: {calls:#?}"))
        .to_owned()
}

/// The fences (`#<n>`) `lines`' records starting with `prefix` (`  render 34 fence=`) name.
fn fences<'a>(lines: &'a [String], prefix: &str) -> Vec<&'a str> {
    lines
        .iter()
        .filter_map(|c| c.strip_prefix(prefix))
        .filter_map(|c| c.split(' ').next())
        .collect()
}

/// The quad's uniforms map the rect over its level's own extent and carry the decode's number.
///
/// A sub-rect of level 1 (32x16) of a 64x32 texture: the transform divides by 32 and 16, not by
/// level 0's extent; the fragment's level is 0 (the one-level view's), its extent the level's.
#[test]
fn the_uniforms_take_the_levels_extent() {
    let level1 = Shape {
        width: 32,
        height: 16,
        pixel_format: BGRA8,
        srgb_format: 0,
        texel_bytes: 4,
        level: 1,
        slice: 0,
        samples: 1,
    };
    let decode = BlitDecode::Uyvy.uniform();
    let (xform, src_level) = quad_uniforms(&level1, [12, 2, 16, 12], decode);
    assert_eq!(
        xform.map(f32::to_bits),
        [0.5_f32, 0.75, 0.375, 0.125].map(f32::to_bits)
    );
    assert_eq!(
        src_level.map(f32::to_bits),
        [0.0_f32, 2.0, 32.0, 16.0].map(f32::to_bits)
    );
}

/// A stretch it cannot make is refused, and nothing is recorded: no pass, no pipeline.
///
/// Each case is sound but in the one it names. A source of a texture made with views in another
/// format, and the back buffer, are sampled through a swizzle.
#[test]
fn a_refused_stretch_records_nothing() {
    const DEVICE: u64 = 0x440;
    let mut core = core_on_fakes(DEVICE);
    let (mips, _) = target_2d(&mut core, DEVICE, 64, 2);
    let (level0, level1) = (level_of(mips, 0), level_of(mips, 1));
    let dst = level_of(target_2d(&mut core, DEVICE, 64, 1).0, 0);
    let flat = CoreTextureDesc {
        flags: CoreTextureFlags::empty(),
        ..target_desc(BGRA8, TextureKind::Tex2d, 64, 1, 4)
    };
    let plain = level_of(stored(&mut core, DEVICE, &flat).0, 0);
    let ms = level_of(stored(&mut core, DEVICE, &ms_desc(BGRA8, 64, 4)).0, 0);
    let twinned = CoreTextureDesc {
        flags: CoreTextureFlags::VIEW_TWINS,
        ..flat
    };
    let viewable = level_of(stored(&mut core, DEVICE, &twinned).0, 0);
    let unknown = level_of(TextureId::new_unique(), 0);
    let swizzled = view_format(
        BGRA8,
        Some([
            WMT_TEXTURE_SWIZZLE_RED,
            WMT_TEXTURE_SWIZZLE_GREEN,
            WMT_TEXTURE_SWIZZLE_BLUE,
            WMT_TEXTURE_SWIZZLE_ONE,
        ]),
    );
    let decoded = |decode: BlitDecode| StretchQuad {
        decode: decode.uniform(),
        ..quad(level0, SIXTEEN, BGRA8, dst, SIXTEEN)
    };
    for (what, refused) in [
        (
            "an unknown source",
            quad(unknown, SIXTEEN, BGRA8, dst, SIXTEEN),
        ),
        (
            "an unknown destination",
            quad(level0, SIXTEEN, BGRA8, unknown, SIXTEEN),
        ),
        (
            "a multisampled source",
            quad(ms, SIXTEEN, BGRA8, dst, SIXTEEN),
        ),
        (
            "a non-target destination",
            quad(level0, SIXTEEN, BGRA8, plain, SIXTEEN),
        ),
        (
            "no source width",
            quad(level0, [0, 0, 0, 16], BGRA8, dst, SIXTEEN),
        ),
        (
            "no destination height",
            quad(level0, SIXTEEN, BGRA8, dst, [0, 0, 16, 0]),
        ),
        (
            "a rect past level 1",
            quad(level1, [20, 20, 20, 20], BGRA8, dst, SIXTEEN),
        ),
        (
            "a rect past u32",
            quad(level0, SIXTEEN, BGRA8, dst, [u32::MAX, 0, 16, 16]),
        ),
        (
            "one level",
            quad(level0, SIXTEEN, BGRA8, level0, [32, 32, 16, 16]),
        ),
        (
            "two levels of one texture",
            quad(level0, SIXTEEN, BGRA8, level1, SIXTEEN),
        ),
        ("a YV12 decode", decoded(BlitDecode::Yv12)),
        ("an NV12 decode", decoded(BlitDecode::Nv12)),
        (
            "another base format",
            quad(
                viewable,
                SIXTEEN,
                WMT_PIXEL_FORMAT_RGBA8_UNORM,
                dst,
                SIXTEEN,
            ),
        ),
        (
            "a twin it lacks",
            quad(viewable, SIXTEEN, SRGB, dst, SIXTEEN),
        ),
        (
            "a swizzle without views",
            quad(level0, SIXTEEN, swizzled, dst, SIXTEEN),
        ),
    ] {
        assert!(!core.stretch(&refused), "{what}");
        core.drain();
        let calls = take_calls(DEVICE);
        assert!(calls.is_empty(), "{what}: {calls:#?}");
    }
    assert!(core.stretch(&quad(viewable, SIXTEEN, swizzled, dst, SIXTEEN)));
    let back_buffer = quad(TargetRef::BackBuffer, SIXTEEN, swizzled, dst, SIXTEEN);
    assert!(core.stretch(&back_buffer));
}

/// A stretch is one render pass of its own on the destination's view, of the quad's records.
///
/// From a rect of level 1 into a rect of a target, point-sampled: the pass attaches the target at
/// its size, loading and storing it; the source is bound through a 2D view of level 1 in its
/// format; the viewport and the scissor are the destination rect; the rasterizer state fills and
/// culls nothing; slot 0 of each stage takes 16 bytes; one triangle is drawn, and no pass is left
/// open. Then from face 3 of a cube, bilinear: a 2D view of that slice. Each sampler clamps.
#[test]
fn a_stretch_is_one_pass_of_the_quads_records() {
    const DEVICE: u64 = 0x441;
    let mut core = core_on_fakes(DEVICE);
    let (mips, h_mips) = target_2d(&mut core, DEVICE, 64, 2);
    let (dst, h_dst) = target_2d(&mut core, DEVICE, 64, 1);
    let cube_desc = CoreTextureDesc {
        flags: CoreTextureFlags::empty(),
        ..target_desc(BGRA8, TextureKind::Cube, 32, 1, 4)
    };
    let (cube, h_cube) = stored(&mut core, DEVICE, &cube_desc);
    let face3 = TargetRef::Texture {
        id: cube,
        level: 0,
        face: Some(3),
    };
    let clamp = WMT_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE;
    for (what, source, linear, texture, view) in [
        (
            "level 1",
            level_of(mips, 1),
            false,
            &h_mips,
            "mip=1 slice=0",
        ),
        ("face 3", face3, true, &h_cube, "mip=0 slice=3"),
    ] {
        let copy = StretchQuad {
            linear,
            ..quad(
                source,
                [2, 3, 20, 10],
                BGRA8,
                level_of(dst, 0),
                [5, 7, 40, 30],
            )
        };
        assert!(core.stretch(&copy), "{what}");
        assert!(!core.render_open && !core.blit_open, "{what}");
        core.drain();
        let calls = take_calls(DEVICE);
        let filter = if linear {
            WMT_SAMPLER_MIN_MAG_FILTER_LINEAR
        } else {
            WMT_SAMPLER_MIN_MAG_FILTER_NEAREST
        };
        let expected = format!(
            "min={filter} mag={filter} mip={WMT_SAMPLER_MIP_FILTER_NEAREST} \
             address={clamp},{clamp},{clamp} "
        );
        let sampler = line(&calls, "MTLDevice_newSamplerState ").unwrap_or_default();
        assert!(sampler.starts_with(&expected), "{what}: {calls:#?}");
        let found = passes(&calls);
        assert_eq!(found.len(), 1, "{what}: {calls:#?}");
        let pass = found[0];
        assert_eq!(
            targets(&calls),
            [(h_dst.clone(), "64x64".to_owned())],
            "{what}: {calls:#?}"
        );
        assert_eq!(
            line(pass, "  color0 "),
            Some(
                format!("load={WMT_LOAD_ACTION_LOAD} store={WMT_STORE_ACTION_STORE} clear=0,0,0,0")
                    .as_str()
            ),
            "{what}: {calls:#?}"
        );
        let bound = view_of(
            &calls,
            texture,
            &format!("format={BGRA8} type={WMT_TEXTURE_TYPE2D} {view}"),
        );
        let [left, top, across, down] = [5.0, 7.0, 40.0, 30.0].map(hex);
        let (zero, one) = (hex(0.0), hex(1.0));
        for record in [
            format!(
                "  render 13 viewports=48B viewport_count=1 viewports[0].origin_x={left} \
                 viewports[0].origin_y={top} viewports[0].width={across} \
                 viewports[0].height={down} viewports[0].znear={zero} viewports[0].zfar={one}"
            ),
            "  render 14 scissor_rects=32B rect_count=1 scissor_rects[0].x=5 scissor_rects[0].y=7 \
             scissor_rects[0].width=40 scissor_rects[0].height=30"
                .to_owned(),
            format!(
                "  render 12 fill_mode=0 cull_mode=0 depth_clip_mode=0 winding=0 \
                 depth_bias={zero} scole_scale={zero} depth_bias_clamp={zero}"
            ),
            format!("  render {WMT_RENDER_COMMAND_SET_VERTEX_BYTES} length=16 index=0"),
            format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_BYTES} length=16 index=0"),
            format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE} texture={bound} index=0"),
            "  render 19 primitive_type=3 vertex_start=0 vertex_count=3 instance_count=1 \
             base_instance=0"
                .to_owned(),
        ] {
            assert_eq!(
                pass.iter().filter(|c| **c == record).count(),
                1,
                "{what}: {record:?} once in {calls:#?}"
            );
        }
    }
}

/// The quad's own rasterizer state holds in a pass the optimizer merged with a draw's.
///
/// A draw on the target that culls (`cull_mode` 1), then a stretch into that target: one pass,
/// where the quad's rasterizer record, filling and culling nothing, comes after the draw's and
/// before the quad's draw. The quad's records are told apart by its pipeline's handle.
#[test]
fn the_quads_rasterizer_state_holds_in_a_merged_pass() {
    const DEVICE: u64 = 0x442;
    let mut core = core_on_fakes(DEVICE);
    let src = level_of(target_2d(&mut core, DEVICE, 64, 1).0, 0);
    let dst = level_of(target_2d(&mut core, DEVICE, 64, 1).0, 0);
    assert!(core.set_target(dst));
    let binds = ff_binds();
    let culled = DrawData {
        cull_mode: 1,
        ..triangle(&binds)
    };
    assert_eq!(core.draw(&key(0xf), &culled), Ok(()));
    assert!(core.stretch(&quad(src, SIXTEEN, BGRA8, dst, SIXTEEN)));
    let pso = core
        .quad_pipelines
        .get(&(BGRA8, 1))
        .and_then(Option::as_ref)
        .map(Obj::handle)
        .expect("the quad's pipeline");
    core.drain();
    let calls = take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 1, "merged: {calls:#?}");
    let pass = found[0];
    let after = |from: usize, prefix: &str| {
        from + pass[from..]
            .iter()
            .position(|c| c.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix:?} after {from}: {calls:#?}"))
    };
    let culling = after(0, "  render 12 fill_mode=0 cull_mode=1 ");
    let quad_pso = after(0, &format!("  render 15 pso=#{pso}"));
    let own = after(quad_pso, "  render 12 ");
    let drawn = after(quad_pso, "  render 19 ");
    assert!(culling < quad_pso && own < drawn, "{calls:#?}");
    assert!(
        pass[own].starts_with("  render 12 fill_mode=0 cull_mode=0 "),
        "{calls:#?}"
    );
}

/// The quad's pipelines are made once a destination format and sample count.
///
/// Two stretches into one single-sample target make one pipeline of one sample; one into a
/// 4-sample target makes a second, which names its count. No rect clear runs here: its
/// pipelines print the same line.
#[test]
fn a_pipeline_per_format_and_count() {
    const DEVICE: u64 = 0x443;
    const MADE: &str = "MTLDevice_newRenderPipelineState";
    let mut core = core_on_fakes(DEVICE);
    let src = level_of(target_2d(&mut core, DEVICE, 64, 1).0, 0);
    let dst = level_of(target_2d(&mut core, DEVICE, 64, 1).0, 0);
    let (ms, h_ms) = stored(&mut core, DEVICE, &ms_desc(BGRA8, 64, 4));
    assert!(core.stretch(&quad(src, SIXTEEN, BGRA8, dst, SIXTEEN)));
    assert!(core.stretch(&quad(src, SIXTEEN, BGRA8, dst, [8, 8, 32, 32])));
    core.drain();
    let calls = take_calls(DEVICE);
    assert_eq!(calls.iter().filter(|c| *c == MADE).count(), 1, "{calls:#?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("  pso samples=")),
        "{calls:#?}"
    );
    assert!(core.stretch(&quad(src, SIXTEEN, BGRA8, level_of(ms, 0), SIXTEEN)));
    core.drain();
    let calls = take_calls(DEVICE);
    let made = at(&calls, MADE);
    assert_eq!(calls[made + 1], "  pso samples=4 a2c=0", "{calls:#?}");
    assert_eq!(calls.iter().filter(|c| *c == MADE).count(), 1, "{calls:#?}");
    assert_eq!(targets(&calls), [(h_ms, "64x64".to_owned())], "{calls:#?}");
}

/// A stretch from a level waits for the Clear that wrote it.
///
/// A Clear of level 1 of a render-target texture, then a stretch from it: the quad's pass, on
/// another target, waits on a fence the Clear's pass updates.
#[test]
fn a_stretch_waits_on_its_sources_clear() {
    const DEVICE: u64 = 0x444;
    let mut core = core_on_fakes(DEVICE);
    let level1 = level_of(target_2d(&mut core, DEVICE, 64, 2).0, 1);
    let (dst, h_dst) = target_2d(&mut core, DEVICE, 64, 1);
    assert!(core.set_target(level1));
    core.clear(0, [0.0, 1.0, 0.0, 1.0], false);
    assert!(core.stretch(&quad(level1, SIXTEEN, BGRA8, level_of(dst, 0), SIXTEEN)));
    core.drain();
    let calls = take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 2, "{calls:#?}");
    assert_eq!(targets(&calls)[1].0, h_dst, "{calls:#?}");
    // The fences the Clear's pass updates (record 34), and those the quad's pass waits for (33).
    let updated = fences(found[0], "  render 34 fence=");
    let waits = fences(found[1], "  render 33 fence=");
    assert!(
        waits.iter().any(|w| updated.contains(w)),
        "the quad waits on {waits:?}, the Clear updates {updated:?}: {calls:#?}"
    );
}

/// A draw on a stretch's source waits for the stretch that read it.
///
/// A stretch from level 1 of a render-target texture, then a draw on that level: the draw's pass
/// waits on a fence the quad's pass updates, a write after a read.
#[test]
fn a_draw_on_a_stretchs_source_waits_on_it() {
    const DEVICE: u64 = 0x445;
    let mut core = core_on_fakes(DEVICE);
    let level1 = level_of(target_2d(&mut core, DEVICE, 64, 2).0, 1);
    let (dst, h_dst) = target_2d(&mut core, DEVICE, 64, 1);
    assert!(core.stretch(&quad(level1, SIXTEEN, BGRA8, level_of(dst, 0), SIXTEEN)));
    assert!(core.set_target(level1));
    let binds = ff_binds();
    assert_eq!(core.draw(&key(0xf), &triangle(&binds)), Ok(()));
    core.drain();
    let calls = take_calls(DEVICE);
    let found = passes(&calls);
    assert_eq!(found.len(), 2, "{calls:#?}");
    assert_eq!(targets(&calls)[0].0, h_dst, "{calls:#?}");
    let updated = fences(found[0], "  render 34 fence=");
    let waits = fences(found[1], "  render 33 fence=");
    assert!(
        waits.iter().any(|w| updated.contains(w)),
        "the draw waits on {waits:?}, the quad updates {updated:?}: {calls:#?}"
    );
}

/// A read-back of a stretch's destination waits for the stretch, at the level or face it wrote.
///
/// A stretch into level 1 of a render-target texture, and one into face 3 of a render-target
/// cube, each then read back: the quad's pass attaches a view of that one subresource, and the
/// read-back's blit waits on a fence the quad's pass updates.
#[test]
fn a_read_back_of_a_stretchs_destination_waits_on_it() {
    const DEVICE: u64 = 0x446;
    let mut core = core_on_fakes(DEVICE);
    let src = level_of(target_2d(&mut core, DEVICE, 64, 1).0, 0);
    let (mips, h_mips) = target_2d(&mut core, DEVICE, 128, 2);
    let cube_desc = target_desc(BGRA8, TextureKind::Cube, 64, 1, 4);
    let (cube, h_cube) = stored(&mut core, DEVICE, &cube_desc);
    let face3 = TargetRef::Texture {
        id: cube,
        level: 0,
        face: Some(3),
    };
    for (what, dst, texture, view) in [
        ("level 1", level_of(mips, 1), &h_mips, "mip=1 slice=0"),
        ("face 3", face3, &h_cube, "mip=0 slice=3"),
    ] {
        assert!(
            core.stretch(&quad(src, SIXTEEN, BGRA8, dst, [4, 4, 32, 32])),
            "{what}"
        );
        let mut out = vec![0_u8; 64 * 4 * 64];
        assert!(core.read_back(dst, &mut out, 64 * 4), "{what}");
        let calls = take_calls(DEVICE);
        let attached = view_of(
            &calls,
            texture,
            &format!("format={BGRA8} type={WMT_TEXTURE_TYPE2D} {view}"),
        );
        assert_eq!(targets(&calls)[0].0, attached, "{what}: {calls:#?}");
        let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
        let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
        assert!(render < blit, "{what}: {calls:#?}");
        let copy = blit
            + calls[blit..]
                .iter()
                .position(|c| c == "  blit 3")
                .unwrap_or_else(|| panic!("{what}: no texture-to-buffer copy in {calls:#?}"));
        let updated = fences(&calls[render..blit], "  render 34 fence=");
        let waits = fences(&calls[blit..copy], "  blit 6 fence=");
        assert!(
            waits.iter().any(|w| updated.contains(w)),
            "{what}: the read waits on {waits:?}, the quad updates {updated:?}: {calls:#?}"
        );
    }
}

/// A stretch writes its destination through the stored format, which a resolve then reads.
///
/// A 4-sample target with an sRGB twin is cleared through the twin, then stretched into, then
/// resolved into a twinned target: the resolve folds into the quad's pass and reads the stored
/// views (the two textures themselves). Through the twin it would make the destination's twin
/// view.
#[test]
fn a_stretch_notes_its_destination_written_in_its_stored_format() {
    const DEVICE: u64 = 0x447;
    let mut core = core_on_fakes(DEVICE);
    let src = level_of(target_2d(&mut core, DEVICE, 64, 1).0, 0);
    let (ms, h_ms) = stored(&mut core, DEVICE, &twinned_ms_desc(BGRA8, SRGB, 64, 4));
    let (one, h_one) = stored(&mut core, DEVICE, &twinned_ms_desc(BGRA8, SRGB, 64, 1));
    let ms = level_of(ms, 0);
    assert!(core.set_target(ms));
    core.clear(0, [0.5, 0.5, 0.5, 1.0], true);
    assert!(core.stretch(&quad(src, SIXTEEN, BGRA8, ms, SIXTEEN)));
    assert!(core.resolve_into(ms, level_of(one, 0)));
    core.drain();
    let calls = take_calls(DEVICE);
    let resolving = passes(&calls)
        .into_iter()
        .filter(|pass| line(pass, "  resolve0 ").is_some())
        .collect::<Vec<_>>();
    assert_eq!(resolving.len(), 1, "{calls:#?}");
    let pass = resolving[0];
    assert_eq!(
        line(pass, "  resolve0 "),
        Some(h_one.as_str()),
        "{calls:#?}"
    );
    assert_eq!(
        line(pass, "  target0 "),
        Some(format!("{h_ms} 64x64").as_str()),
        "{calls:#?}"
    );
    assert!(
        pass.iter().any(|c| c.starts_with("  render 19 ")),
        "folded into the quad's pass: {calls:#?}"
    );
    let twin_view = format!(" of {h_one} ");
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("MTLTexture_newTextureView ") && c.contains(&twin_view)),
        "{calls:#?}"
    );
}
