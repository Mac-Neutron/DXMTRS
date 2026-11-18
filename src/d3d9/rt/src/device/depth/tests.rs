use mtld3d_core::{
    depth_stencil_state::DepthStencilSnapshot, ids::TextureId, pipeline_key::PipelineKey,
};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH16_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
    WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
};

use super::DepthRef;
use crate::device::{
    CoreDevice, CoreTextureDesc, DepthPlanes, DrawData, DrawRefused, Stage, TargetRef, TextureKind,
    fixture::{
        at, core_on_fakes, depth_desc, ff_binds, key, line, passes, target_desc, textures_made,
        triangle,
    },
};

const D32F: u32 = WMT_PIXEL_FORMAT_DEPTH32_FLOAT;
const D32F_S8: u32 = WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8;

/// A core device on fakes (a 64x32 back buffer) and every call it has made so far.
///
/// A texture's creation is drained and taken at once, so the initializer's zeroing passes are
/// never among the calls a test then looks at.
struct Rig {
    core: CoreDevice,
    device: u64,
    log: Vec<String>,
}

impl Rig {
    fn new(device: u64) -> Self {
        let mut core = core_on_fakes(device);
        core.drain();
        let log = dxmt_fakes::take_calls(device);
        Self { core, device, log }
    }

    /// The calls since the last take, after a drain.
    fn take(&mut self) -> Vec<String> {
        self.core.drain();
        let calls = dxmt_fakes::take_calls(self.device);
        self.log.extend(calls.iter().cloned());
        calls
    }

    /// A stored depth texture and its Metal handle (`#<n>`).
    fn depth(&mut self, id: u64, format: u32, (width, height): (u32, u32), levels: u32) -> Made {
        let id = TextureId::from_raw(id);
        self.core
            .texture_create(id, &depth_desc(format, width, height, levels));
        let made = textures_made(&self.take());
        (id, made[0].clone())
    }

    /// A stored 64x64 BGRA8 render-target texture and its Metal handle.
    fn color(&mut self, id: u64) -> Made {
        let id = TextureId::from_raw(id);
        let desc = target_desc(WMT_PIXEL_FORMAT_BGRA8_UNORM, TextureKind::Tex2d, 64, 1, 4);
        self.core.texture_create(id, &desc);
        let made = textures_made(&self.take());
        (id, made[0].clone())
    }

    /// A triangle drawn with `key`'s pipeline and `data`'s depth values.
    fn draw(&mut self, key: &PipelineKey, data: Depth) -> Result<(), DrawRefused> {
        let binds = ff_binds();
        let draw = DrawData {
            depth_stencil: data.0,
            stencil_ref: data.1,
            slope_scale: data.2,
            ..triangle(&binds)
        };
        self.core.draw(key, &draw)
    }

    /// The texture `handle` is, or is a view of, by the views made so far.
    fn texture_of(&self, handle: &str) -> String {
        let view = format!("MTLTexture_newTextureView {handle} of ");
        self.log
            .iter()
            .find_map(|c| c.strip_prefix(&view))
            .and_then(|c| c.split(' ').next())
            .map_or_else(|| handle.to_owned(), str::to_owned)
    }

    /// The `mip=` of the view `handle` is.
    fn mip_of(&self, handle: &str) -> Option<String> {
        let view = format!("MTLTexture_newTextureView {handle} of ");
        let made = self.log.iter().find_map(|c| c.strip_prefix(&view))?;
        let (_, mip) = made.split_once(" mip=")?;
        mip.split(' ').next().map(str::to_owned)
    }
}

/// A stored texture's id and its Metal handle.
type Made = (TextureId, String);

/// A draw's depth-stencil snapshot, stencil reference and slope scale.
type Depth = (DepthStencilSnapshot, u8, f32);

/// The inert state, reference 0, no slope.
const INERT: Depth = (DepthStencilSnapshot::inert(), 0, 0.0);

/// The fixture's key with `depth_format`.
fn key_with(depth_format: u32) -> PipelineKey {
    PipelineKey {
        depth_format,
        ..key(0xf)
    }
}

/// `id`'s level 0 as a depth-stencil.
const fn level0(id: TextureId) -> DepthRef {
    DepthRef { id, level: 0 }
}

/// The handle (`#<n>`) a pass's `plane` line (`depth` or `stencil`) names.
fn plane_handle(pass: &[String], plane: &str) -> Option<String> {
    line(pass, &format!("  {plane} "))
        .and_then(|rest| rest.split(' ').next())
        .map(str::to_owned)
}

/// The width and height of the last scissor `pass` records, as `"<width> <height>"`.
fn scissor_of(pass: &[String]) -> Option<String> {
    let record = pass
        .iter()
        .rev()
        .find_map(|c| c.strip_prefix("  render 14 "))?;
    let field = |name: &str| {
        let (_, rest) = record.split_once(&format!("scissor_rects[0].{name}="))?;
        rest.split(' ').next().map(str::to_owned)
    };
    Some(format!("{} {}", field("width")?, field("height")?))
}

/// A snapshot testing and writing depth with `D3DCMP_LESSEQUAL`.
const fn tested() -> DepthStencilSnapshot {
    let mut s = DepthStencilSnapshot::inert();
    s.depth_enable = 1;
    s.depth_write = 1;
    s.depth_func = 4;
    s
}

/// With no depth-stencil bound a draw's pass has no depth and encodes no depth-stencil state.
#[test]
fn a_draw_with_no_depth_encodes_no_state() {
    let mut rig = Rig::new(0x2e0);
    assert_eq!(rig.core.depth_format(), None);
    assert_eq!(rig.draw(&key_with(0), INERT), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    assert_eq!(line(all[0], "  depth "), None, "{calls:#?}");
    assert_eq!(line(all[0], "  render 16 "), None, "{calls:#?}");
    assert!(
        !calls.iter().any(|c| c.starts_with("  pso depth=")),
        "{calls:#?}"
    );
}

/// A combined format attaches both planes on one view, loaded and stored.
///
/// The pipeline declares both formats and the draw sets a state with its stencil reference.
#[test]
fn a_depth_is_attached_with_the_colour_target() {
    let mut rig = Rig::new(0x2e1);
    let (id, texture) = rig.depth(1, D32F_S8, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(id))));
    assert_eq!(rig.core.depth(), Some(level0(id)));
    assert_eq!(rig.core.depth_format(), Some(D32F_S8));
    assert_eq!(rig.draw(&key_with(D32F_S8), (tested(), 5, 0.0)), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    let depth = plane_handle(all[0], "depth").unwrap_or_else(|| panic!("{calls:#?}"));
    let stencil = plane_handle(all[0], "stencil").unwrap_or_else(|| panic!("{calls:#?}"));
    assert_eq!(depth, stencil, "{calls:#?}");
    assert_eq!(rig.texture_of(&depth), texture, "{calls:#?}");
    for plane in ["  depth ", "  stencil "] {
        let rest = line(all[0], plane).unwrap_or_default();
        assert!(rest.contains(" load=1 store=1 "), "{calls:#?}");
    }
    assert!(
        calls.iter().any(|c| c == "  pso depth=260 stencil=260"),
        "{calls:#?}"
    );
    let state = line(all[0], "  render 16 dsso=#").unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(state.ends_with(" stencil_ref=5"), "{calls:#?}");
    assert!(!state.starts_with("0 "), "{calls:#?}");
    // LESSEQUAL (Metal's 3), writing, both faces off.
    let made = calls
        .iter()
        .find_map(|c| c.strip_prefix("MTLDevice_newDepthStencilState #"))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(
        made.ends_with(" cmp=3 write=1 front=0,0,0,0,0,0,0 back=0,0,0,0,0,0,0"),
        "{calls:#?}"
    );
}

/// A depth-only format attaches no stencil plane, and a stencil-enabled state is made without.
#[test]
fn a_depth_only_format_attaches_no_stencil() {
    let mut rig = Rig::new(0x2e2);
    let (id, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(id))));
    let mut snapshot = tested();
    snapshot.stencil_enable = 1;
    assert_eq!(rig.draw(&key_with(D32F), (snapshot, 1, 0.0)), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    assert!(line(all[0], "  depth ").is_some(), "{calls:#?}");
    assert_eq!(line(all[0], "  stencil "), None, "{calls:#?}");
    assert!(
        calls.iter().any(|c| c == "  pso depth=252 stencil=0"),
        "{calls:#?}"
    );
    let made = calls
        .iter()
        .find_map(|c| c.strip_prefix("MTLDevice_newDepthStencilState #"))
        .unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(made.contains(" front=0,"), "{calls:#?}");
    assert!(made.contains(" back=0,"), "{calls:#?}");
}

/// Only a level of a depth texture binds as the depth-stencil, and none as a colour target.
#[test]
fn set_depth_takes_depth_textures_only() {
    let mut rig = Rig::new(0x2e3);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 2);
    let (color, _) = rig.color(2);
    assert!(!rig.core.set_depth(Some(level0(TextureId::from_raw(9)))));
    assert!(!rig.core.set_depth(Some(level0(color))));
    assert!(!rig.core.set_depth(Some(DepthRef {
        id: depth,
        level: 2
    })));
    assert_eq!(rig.core.depth(), None);
    assert!(!rig.core.set_target(TargetRef::Texture {
        id: depth,
        level: 0,
        face: None,
    }));
    assert!(rig.core.set_depth(Some(DepthRef {
        id: depth,
        level: 1
    })));
    assert!(rig.core.set_depth(None));
    assert_eq!(rig.core.depth_format(), None);
}

/// A change of depth-stencil ends the open pass: each draw's pass has its own.
#[test]
fn a_depth_change_closes_the_open_pass() {
    let mut rig = Rig::new(0x2e4);
    let (a, texture_a) = rig.depth(1, D32F, (64, 32), 1);
    let (b, texture_b) = rig.depth(2, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(a))));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    assert!(rig.core.set_depth(Some(level0(b))));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    let calls = rig.take();
    let bound = passes(&calls)
        .into_iter()
        .filter_map(|pass| plane_handle(pass, "depth"))
        .map(|handle| rig.texture_of(&handle))
        .collect::<Vec<_>>();
    assert_eq!(bound, [texture_a, texture_b], "{calls:#?}");
}

/// The depth-stencil stays bound across a change of colour target.
#[test]
fn a_target_change_keeps_the_depth() {
    let mut rig = Rig::new(0x2e5);
    let (depth, texture) = rig.depth(1, D32F, (64, 64), 1);
    let (first, _) = rig.color(2);
    let (second, _) = rig.color(3);
    let target = |id| TargetRef::Texture {
        id,
        level: 0,
        face: None,
    };
    assert!(rig.core.set_target(target(first)));
    assert!(rig.core.set_depth(Some(level0(depth))));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    assert!(rig.core.set_target(target(second)));
    assert_eq!(rig.core.depth(), Some(level0(depth)));
    assert_eq!(rig.core.depth_format(), Some(D32F));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    let calls = rig.take();
    let bound = passes(&calls)
        .into_iter()
        .filter_map(|pass| plane_handle(pass, "depth"))
        .map(|handle| rig.texture_of(&handle))
        .collect::<Vec<_>>();
    assert_eq!(bound, [texture.clone(), texture], "{calls:#?}");
}

/// A depth-stencil of another size than the colour target is attached (DXMTRS C4b-3d3).
///
/// The pass is cut to the smaller of the two: the draw's scissor says so.
#[test]
fn a_depth_of_another_size_attaches() {
    let mut rig = Rig::new(0x2e6);
    for (id, size, cut) in [(1, (128, 128), "64 32"), (2, (16, 16), "16 16")] {
        let (depth, texture) = rig.depth(id, D32F_S8, size, 1);
        assert!(rig.core.set_depth(Some(level0(depth))));
        assert_eq!(rig.core.depth_format(), Some(D32F_S8));
        assert_eq!(rig.draw(&key_with(D32F_S8), INERT), Ok(()));
        let calls = rig.take();
        let all = passes(&calls);
        assert_eq!(all.len(), 1, "{calls:#?}");
        let handle = plane_handle(all[0], "depth").unwrap_or_else(|| panic!("{calls:#?}"));
        assert_eq!(rig.texture_of(&handle), texture, "{calls:#?}");
        assert_eq!(scissor_of(all[0]).as_deref(), Some(cut), "{calls:#?}");
        // A key that names no depth draws without it, over the whole colour target.
        assert_eq!(rig.draw(&key_with(0), INERT), Ok(()));
        let calls = rig.take();
        let all = passes(&calls);
        assert_eq!(line(all[0], "  depth "), None, "{calls:#?}");
        assert_eq!(scissor_of(all[0]).as_deref(), Some("64 32"), "{calls:#?}");
    }
}

/// A draw without the depth-stencil between two with it gets a pass of its own.
#[test]
fn a_detached_draw_between_two_attached_ones_is_three_passes() {
    let mut rig = Rig::new(0x2e7);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    for format in [D32F, 0, D32F] {
        assert_eq!(rig.draw(&key_with(format), INERT), Ok(()));
    }
    let calls = rig.take();
    let with_depth = passes(&calls)
        .into_iter()
        .map(|pass| line(pass, "  depth ").is_some())
        .collect::<Vec<_>>();
    assert_eq!(with_depth, [true, false, true], "{calls:#?}");
}

/// Clears of the colour and of both planes become the load actions of the draw's pass.
#[test]
fn a_clear_then_a_draw_is_one_pass() {
    let mut rig = Rig::new(0x2e8);
    let (depth, _) = rig.depth(1, D32F_S8, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    rig.core.clear(0, [0.0, 0.0, 1.0, 1.0], false);
    rig.core.clear_depth_stencil(3, 0.25, 7);
    assert_eq!(rig.draw(&key_with(D32F_S8), INERT), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    assert_eq!(
        line(all[0], "  color0 "),
        Some("load=2 store=1 clear=0,0,1,1"),
        "{calls:#?}"
    );
    let depth = line(all[0], "  depth ").unwrap_or_default();
    assert!(depth.ends_with(" load=2 store=1 clear=0.25"), "{calls:#?}");
    let stencil = line(all[0], "  stencil ").unwrap_or_default();
    assert!(stencil.ends_with(" load=2 store=1 clear=7"), "{calls:#?}");
}

/// A Clear of the depth plane alone, with no draw after it, is a pass that clears that plane.
#[test]
fn a_clear_with_no_draw_is_a_pass_of_its_own() {
    let mut rig = Rig::new(0x2e9);
    let (depth, texture) = rig.depth(1, D32F_S8, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    rig.core.clear_depth_stencil(1, 0.5, 9);
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    let handle = plane_handle(all[0], "depth").unwrap_or_else(|| panic!("{calls:#?}"));
    assert_eq!(rig.texture_of(&handle), texture, "{calls:#?}");
    let depth = line(all[0], "  depth ").unwrap_or_default();
    assert!(depth.ends_with(" load=2 store=1 clear=0.5"), "{calls:#?}");
    assert_eq!(line(all[0], "  stencil "), None, "{calls:#?}");
    assert_eq!(line(all[0], "  color0 "), None, "{calls:#?}");
}

/// A stencil Clear of a format with no stencil plane records nothing (not a colour clear).
#[test]
fn a_stencil_clear_of_a_depth_only_format_records_nothing() {
    let mut rig = Rig::new(0x2ea);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    rig.core.clear_depth_stencil(2, 0.5, 9);
    rig.core.clear_depth_stencil(0, 0.5, 9);
    let calls = rig.take();
    assert!(passes(&calls).is_empty(), "{calls:#?}");
    // And with no depth-stencil bound.
    assert!(rig.core.set_depth(None));
    rig.core.clear_depth_stencil(3, 0.5, 9);
    let calls = rig.take();
    assert!(passes(&calls).is_empty(), "{calls:#?}");
}

/// A Clear's depth is clamped to Metal's range, a NaN to 0.
#[test]
fn a_clear_depth_is_clamped() {
    let mut rig = Rig::new(0x2eb);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    for (value, cleared) in [(1.5, "1"), (-2.0, "0"), (f32::NAN, "0")] {
        rig.core.clear_depth_stencil(1, value, 0);
        let calls = rig.take();
        let all = passes(&calls);
        let depth = line(all[0], "  depth ").unwrap_or_default();
        assert!(
            depth.ends_with(&format!(" clear={cleared}")),
            "{value}: {calls:#?}"
        );
    }
}

/// Forgetting the bound depth-stencil's texture unbinds it and ends the pass that had it.
#[test]
fn forgetting_the_bound_depth_unbinds_it() {
    let mut rig = Rig::new(0x2ec);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    rig.core.forget_texture(depth);
    assert_eq!(rig.core.depth(), None);
    assert_eq!(rig.core.depth_format(), None);
    assert_eq!(rig.draw(&key_with(D32F), INERT), Err(DrawRefused::Depth));
    assert_eq!(rig.draw(&key_with(0), INERT), Ok(()));
    let calls = rig.take();
    let with_depth = passes(&calls)
        .into_iter()
        .map(|pass| line(pass, "  depth ").is_some())
        .collect::<Vec<_>>();
    assert_eq!(with_depth, [true, false], "{calls:#?}");
}

/// A depth-stencil state object is made once for a state, and again for another.
#[test]
fn a_depth_state_is_made_once_a_key() {
    let mut rig = Rig::new(0x2ed);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    let made = |calls: &[String]| {
        calls
            .iter()
            .filter(|c| c.starts_with("MTLDevice_newDepthStencilState "))
            .count()
    };
    assert_eq!(rig.draw(&key_with(D32F), (tested(), 0, 0.0)), Ok(()));
    assert_eq!(rig.draw(&key_with(D32F), (tested(), 3, 0.0)), Ok(()));
    assert_eq!(made(&rig.take()), 1);
    assert_eq!(rig.draw(&key_with(D32F), (tested(), 0, 0.0)), Ok(()));
    assert_eq!(made(&rig.take()), 0);
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    assert_eq!(made(&rig.take()), 1);
}

/// A level below 0 attaches as a view of that level, and is attachable at that level's size.
#[test]
fn a_level_of_a_depth_texture_attaches_that_level() {
    let mut rig = Rig::new(0x2ee);
    let (depth, texture) = rig.depth(1, D32F, (128, 64), 2);
    assert!(rig.core.set_depth(Some(level0(depth))));
    assert_eq!(rig.core.depth_size(), Some((128, 64)));
    assert!(rig.core.set_depth(Some(DepthRef {
        id: depth,
        level: 1
    })));
    assert_eq!(rig.core.depth_size(), Some((64, 32)));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    let handle = plane_handle(all[0], "depth").unwrap_or_else(|| panic!("{calls:#?}"));
    assert_eq!(rig.texture_of(&handle), texture, "{calls:#?}");
    assert_eq!(rig.mip_of(&handle).as_deref(), Some("1"), "{calls:#?}");
}

/// The slope scale is the rasterizer record's; the constant bias stays 0 (the vertex stage's).
#[test]
fn the_slope_scale_reaches_the_rasterizer() {
    let mut rig = Rig::new(0x2ef);
    assert_eq!(
        rig.draw(&key_with(0), (DepthStencilSnapshot::inert(), 0, -1.0)),
        Ok(())
    );
    let calls = rig.take();
    let all = passes(&calls);
    let rasterizer = line(all[0], "  render 12 ").unwrap_or_else(|| panic!("{calls:#?}"));
    assert!(
        rasterizer.contains(" depth_bias=0x00000000 scole_scale=0xbf800000 "),
        "{calls:#?}"
    );
}

/// A draw with `ZENABLE` off after one with it on gets a state that neither tests nor writes.
#[test]
fn a_zenable_change_in_a_pass_encodes_a_new_state() {
    let mut rig = Rig::new(0x2f0);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    assert_eq!(rig.draw(&key_with(D32F), (tested(), 0, 0.0)), Ok(()));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    let states = all[0]
        .iter()
        .filter_map(|c| c.strip_prefix("  render 16 dsso="))
        .collect::<Vec<_>>();
    assert_eq!(states.len(), 2, "{calls:#?}");
    assert_ne!(states[0], states[1], "{calls:#?}");
    let made = calls
        .iter()
        .filter_map(|c| c.strip_prefix("MTLDevice_newDepthStencilState #"))
        .collect::<Vec<_>>();
    // ALWAYS (Metal's 7), not writing.
    assert!(made[1].contains(" cmp=7 write=0 "), "{calls:#?}");
}

/// A compare sampler (a shadow sample's) is made with Metal's less-equal; a plain one with none.
#[test]
fn a_compare_sampler_is_less_equal() {
    use mtld3d_core::sampler_state::{
        description_from_snapshot, key_from_snapshot, snapshot_from_state,
    };
    let mut rig = Rig::new(0x2f1);
    for (is_compare, compare) in [(true, " compare=3 "), (false, " compare=0 ")] {
        let snapshot = snapshot_from_state(&mtld3d_types::sampler_state_defaults(), is_compare);
        let key = key_from_snapshot(&snapshot);
        assert!(
            rig.core
                .sampler(key, &description_from_snapshot(&snapshot, key))
                .is_some()
        );
        let calls = rig.take();
        let made = calls
            .iter()
            .find(|c| c.starts_with("MTLDevice_newSamplerState "))
            .unwrap_or_else(|| panic!("{calls:#?}"));
        assert!(made.contains(compare), "{made}");
    }
}

/// A draw that samples the bound depth-stencil draws without it, after the pass that wrote it.
///
/// A characterization of the passes and the core's tracking (it holds before the front end
/// samples depth): a draw to (X, T), a draw to Y sampling T in its stored format, a draw to
/// (X, T) again are three passes in that order, and the second waits on a fence the first
/// updates, its one tie to it.
#[test]
fn a_draw_sampling_the_bound_depth_detaches_it() {
    use winemetal_sys::wire::{
        WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE, WMT_RENDER_COMMAND_UPDATE_FENCE,
        WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
    };
    let mut rig = Rig::new(0x2f2);
    let (depth, _) = rig.depth(1, D32F, (64, 64), 1);
    let (x, _) = rig.color(2);
    let (y, _) = rig.color(3);
    let target = |id| TargetRef::Texture {
        id,
        level: 0,
        face: None,
    };
    assert!(rig.core.set_target(target(x)));
    assert!(rig.core.set_depth(Some(level0(depth))));
    assert_eq!(rig.draw(&key_with(D32F), (tested(), 0, 0.0)), Ok(()));
    assert!(rig.core.set_target(target(y)));
    let bind = rig
        .core
        .texture_bind(Stage::Pixel, 0, depth, D32F, None)
        .expect("the depth texture binds in its stored format");
    let binds = ff_binds();
    let textures = [bind];
    let sampling = DrawData {
        textures: &textures,
        ..triangle(&binds)
    };
    assert_eq!(rig.core.draw(&key_with(0), &sampling), Ok(()));
    assert!(rig.core.set_target(target(x)));
    assert_eq!(rig.draw(&key_with(D32F), (tested(), 0, 0.0)), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    let with_depth = all
        .iter()
        .map(|pass| line(pass, "  depth ").is_some())
        .collect::<Vec<_>>();
    assert_eq!(with_depth, [true, false, true], "{calls:#?}");
    let texture = format!("  render {WMT_RENDER_COMMAND_SET_FRAGMENT_TEXTURE} ");
    assert!(line(all[1], &texture).is_some(), "{calls:#?}");
    let fences = |pass: &[String], command: u16| {
        let prefix = format!("  render {command} fence=#");
        pass.iter()
            .filter_map(|c| c.strip_prefix(&prefix))
            .filter_map(|c| c.split(' ').next())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let written = fences(all[0], WMT_RENDER_COMMAND_UPDATE_FENCE);
    let awaited = fences(all[1], WMT_RENDER_COMMAND_WAIT_FOR_FENCE);
    assert!(
        awaited.iter().any(|fence| written.contains(fence)),
        "the sampling pass waits on the depth's writer: {calls:#?}"
    );
}

/// A Clear of rects of the depth-stencil is a pass on it alone, whatever the colour target's size.
#[test]
fn a_rect_depth_clear_attaches_the_depth_alone() {
    let mut rig = Rig::new(0x302);
    let (depth, texture) = rig.depth(1, D32F_S8, (128, 128), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    rig.core
        .clear_depth_stencil_rects(&[[0, 0, 100, 100], [100, 100, 28, 28]], 3, 0.5, 7);
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    for plane in ["depth", "stencil"] {
        let handle = plane_handle(all[0], plane).unwrap_or_else(|| panic!("{calls:#?}"));
        assert_eq!(rig.texture_of(&handle), texture, "{calls:#?}");
    }
    assert_eq!(line(all[0], "  color0 "), None, "{calls:#?}");
    let rects = all[0].iter().filter(|c| c.trim() == "render 36").count();
    assert_eq!(rects, 2, "{calls:#?}");
}

/// A stencil Clear of rects of a format with no stencil plane records nothing.
///
/// Nor does one of no rect, or with no depth-stencil bound.
#[test]
fn a_stencil_rect_clear_of_a_depth_only_format_records_nothing() {
    let mut rig = Rig::new(0x303);
    let (depth, _) = rig.depth(1, D32F, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    rig.core
        .clear_depth_stencil_rects(&[[0, 0, 8, 8]], 2, 0.5, 9);
    rig.core.clear_depth_stencil_rects(&[], 1, 0.5, 9);
    assert!(rig.core.set_depth(None));
    rig.core
        .clear_depth_stencil_rects(&[[0, 0, 8, 8]], 3, 0.5, 9);
    let calls = rig.take();
    assert!(passes(&calls).is_empty(), "{calls:#?}");
}

/// A whole Clear of a depth-stencil of another size than the colour target is recorded.
///
/// A pass that clears the plane.
#[test]
fn a_whole_depth_clear_of_another_size_is_recorded() {
    let mut rig = Rig::new(0x304);
    let (depth, texture) = rig.depth(1, D32F, (128, 128), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    assert_eq!(rig.core.depth_size(), Some((128, 128)));
    rig.core.clear_depth_stencil(1, 0.5, 0);
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    let handle = plane_handle(all[0], "depth").unwrap_or_else(|| panic!("{calls:#?}"));
    assert_eq!(rig.texture_of(&handle), texture, "{calls:#?}");
    let line = line(all[0], "  depth ").unwrap_or_default();
    assert!(line.ends_with(" load=2 store=1 clear=0.5"), "{calls:#?}");
}

/// A pass that `open_pass` begins itself (a draw of the other depth kind) records its scissor.
///
/// One rect, a depth draw, a draw without the depth-stencil and a depth draw again: three passes,
/// the rect in each.
#[test]
fn a_depth_kind_switch_records_the_scissor_again() {
    let mut rig = Rig::new(0x305);
    let (depth, _) = rig.depth(1, D32F_S8, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    for format in [D32F_S8, 0, D32F_S8] {
        assert_eq!(rig.draw(&key_with(format), INERT), Ok(()));
    }
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 3, "{calls:#?}");
    for pass in all {
        assert_eq!(scissor_of(pass).as_deref(), Some("64 32"), "{calls:#?}");
    }
}

/// The blend colour is recorded after the depth-stencil state, with the draw's stencil reference.
///
/// Only for a draw that carries one.
#[test]
fn the_blend_colour_is_recorded_with_the_stencil_ref() {
    let mut rig = Rig::new(0x331);
    let (depth, _) = rig.depth(1, D32F_S8, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(depth))));
    let binds = ff_binds();
    let blended = DrawData {
        depth_stencil: tested(),
        stencil_ref: 5,
        blend_color: Some([0.25, 0.5, 0.75, 1.0]),
        ..triangle(&binds)
    };
    assert_eq!(rig.core.draw(&key_with(D32F_S8), &blended), Ok(()));
    assert_eq!(rig.draw(&key_with(D32F_S8), (tested(), 5, 0.0)), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 1, "{calls:#?}");
    let at = |prefix: &str| all[0].iter().position(|c| c.starts_with(prefix));
    let (state, color) = (at("  render 16 "), at("  render 17 "));
    assert!(state.is_some() && state < color, "{calls:#?}");
    let colors = all[0]
        .iter()
        .filter(|c| c.starts_with("  render 17 "))
        .collect::<Vec<_>>();
    assert_eq!(colors.len(), 1, "{calls:#?}");
    assert!(colors[0].ends_with(" stencil_ref=5"), "{calls:#?}");
}

/// The lines of `calls` that are blit records (`  blit `), fences left out.
fn blit_records(calls: &[String]) -> Vec<&str> {
    calls
        .iter()
        .filter(|c| c.starts_with("  blit ") && !c.contains(" fence="))
        .map(String::as_str)
        .collect()
}

/// A `Depth32Float` level uploads in one plain copy of its `f32` rows; a stencil plane is unread.
///
/// Levels 0 and 1 of a two-level 6x4 texture: the copy names the texture, the level, slice 0,
/// origin 0, the level's size and rows of `w * 4` bytes.
#[test]
fn a_depth_level_uploads_in_one_plain_copy() {
    let mut rig = Rig::new(0x400);
    let (id, texture) = rig.depth(1, D32F, (6, 4), 2);
    let depth = [0_u8; 6 * 4 * 4];
    for (level, size, row) in [(0, "6x4x1", 24), (1, "3x2x1", 12)] {
        let planes = DepthPlanes {
            depth: &depth,
            depth_pitch: row,
            stencil: Some((&[], 0)),
        };
        assert!(rig.core.depth_upload(id, level, planes), "level {level}");
        let calls = rig.take();
        let copies = blit_records(&calls);
        assert_eq!(copies.len(), 1, "{calls:#?}");
        let head = format!(
            "  blit copy_to_texture {texture} level={level} slice=0 origin=0,0,0 size={size} "
        );
        assert!(copies[0].starts_with(&head), "{calls:#?}");
        assert!(
            copies[0].contains(&format!(" bytes_per_row={row} ")),
            "{calls:#?}"
        );
    }
}

/// Depth rows wider than the level's are copied a row at a time: the copy's rows are tight.
///
/// The fewest bytes the rows take are enough.
#[test]
fn a_depth_pitch_over_the_row_is_packed() {
    let mut rig = Rig::new(0x401);
    let (id, _) = rig.depth(1, D32F, (4, 2), 1);
    let planes = DepthPlanes {
        depth: &[0_u8; 20 + 16],
        depth_pitch: 20,
        stencil: None,
    };
    assert!(rig.core.depth_upload(id, 0, planes));
    let calls = rig.take();
    let copies = blit_records(&calls);
    assert_eq!(copies.len(), 1, "{calls:#?}");
    assert!(
        copies[0].ends_with(" bytes_per_row=16 bytes_per_image=32"),
        "{calls:#?}"
    );
}

/// A `Depth32FloatStencil8` level uploads in two plane copies in one blit pass, and no other.
#[test]
fn a_depth_stencil_level_uploads_in_two_plane_copies() {
    let mut rig = Rig::new(0x402);
    let (id, _) = rig.depth(1, D32F_S8, (3, 2), 1);
    let planes = DepthPlanes {
        depth: &[0_u8; 12 * 2],
        depth_pitch: 12,
        stencil: Some((&[0_u8; 4 * 2], 4)),
    };
    assert!(rig.core.depth_upload(id, 0, planes));
    let calls = rig.take();
    assert_eq!(
        blit_records(&calls),
        ["  blit 10", "  blit 10"],
        "{calls:#?}"
    );
    let start = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    let end = start
        + calls[start..]
            .iter()
            .position(|c| c == "MTLCommandEncoder_endEncoding")
            .unwrap_or_else(|| panic!("{calls:#?}"));
    let inside = calls[start..end]
        .iter()
        .filter(|c| *c == "  blit 10")
        .count();
    assert_eq!(inside, 2, "{calls:#?}");
}

/// An upload recorded while a draw's pass is open ends it; the next draw's pass loads the planes.
///
/// A render pass, the blit pass, a render pass, in record order.
#[test]
fn a_depth_upload_ends_the_open_pass() {
    let mut rig = Rig::new(0x403);
    let (id, _) = rig.depth(1, D32F_S8, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(id))));
    assert_eq!(rig.draw(&key_with(D32F_S8), INERT), Ok(()));
    let (depth, stencil) = (vec![0_u8; 64 * 4 * 32], vec![0_u8; 64 * 32]);
    let planes = DepthPlanes {
        depth: &depth,
        depth_pitch: 64 * 4,
        stencil: Some((&stencil, 64)),
    };
    assert!(rig.core.depth_upload(id, 0, planes));
    assert_eq!(rig.draw(&key_with(D32F_S8), INERT), Ok(()));
    let calls = rig.take();
    let all = passes(&calls);
    assert_eq!(all.len(), 2, "{calls:#?}");
    let renders = calls
        .iter()
        .enumerate()
        .filter(|(_, c)| *c == "MTLCommandBuffer_renderCommandEncoder")
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let first_end = renders[0] + all[0].len();
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    assert!(first_end < blit && blit < renders[1], "{calls:#?}");
    for plane in ["  depth ", "  stencil "] {
        let rest = line(all[1], plane).unwrap_or_default();
        assert!(rest.contains(" load=1 store=1 "), "{calls:#?}");
    }
}

/// An upload a level cannot take is refused, and nothing is recorded.
///
/// No such texture, a colour texture, a multisampled one, a level past the last, a depth format
/// other than the two, a pitch under a plane's row, a plane short of its rows, no stencil plane
/// for a stencil format.
#[test]
fn a_refused_depth_upload_records_nothing() {
    let mut rig = Rig::new(0x404);
    let (d32f, _) = rig.depth(1, D32F, (4, 2), 1);
    let (d32f_s8, _) = rig.depth(2, D32F_S8, (4, 2), 1);
    let (d16, _) = rig.depth(3, WMT_PIXEL_FORMAT_DEPTH16_UNORM, (4, 2), 1);
    let (color, _) = rig.color(4);
    let multisampled = TextureId::from_raw(5);
    let desc = CoreTextureDesc {
        samples: 4,
        ..depth_desc(D32F, 4, 2, 1)
    };
    rig.core.texture_create(multisampled, &desc);
    let _ = rig.take();
    let (depth, stencil) = ([0_u8; 32], [0_u8; 8]);
    for (what, id, level, depth_plane, stencil_plane) in [
        ("no such texture", TextureId::from_raw(9), 0, (32, 16), None),
        ("a colour texture", color, 0, (32, 16), None),
        ("a multisampled texture", multisampled, 0, (32, 16), None),
        ("a level past the last", d32f, 1, (32, 16), None),
        ("a D16 texture", d16, 0, (32, 16), None),
        ("a depth pitch under the row", d32f, 0, (32, 15), None),
        ("a depth plane short of its rows", d32f, 0, (31, 16), None),
        ("no stencil plane", d32f_s8, 0, (32, 16), None),
        ("stencil pitch < row", d32f_s8, 0, (32, 16), Some((8, 3))),
        ("stencil plane short", d32f_s8, 0, (32, 16), Some((7, 4))),
    ] {
        let planes = DepthPlanes {
            depth: &depth[..depth_plane.0],
            depth_pitch: depth_plane.1,
            stencil: stencil_plane.map(|(len, pitch)| (&stencil[..len], pitch)),
        };
        assert!(!rig.core.depth_upload(id, level, planes), "{what}");
        let calls = rig.take();
        assert!(calls.is_empty(), "{what}: {calls:#?}");
    }
}

/// A stored 64x32 `Depth32FloatStencil8` texture of `samples` a texel, of one level.
fn depth_of(rig: &mut Rig, id: u64, samples: u8) -> TextureId {
    let id = TextureId::from_raw(id);
    let desc = CoreTextureDesc {
        samples,
        ..depth_desc(D32F_S8, 64, 32, 1)
    };
    rig.core.texture_create(id, &desc);
    let _ = rig.take();
    id
}

/// The fences (`#<n>`) `lines`' records starting with `prefix` (`  blit 7 fence=`) name.
fn fence_ids<'a>(lines: &'a [String], prefix: &str) -> Vec<&'a str> {
    lines
        .iter()
        .filter_map(|c| c.strip_prefix(prefix))
        .filter_map(|c| c.split(' ').next())
        .collect()
}

/// A depth-stencil copies into another of its size, format and sample count in one blit.
///
/// One copy of the whole level, both planes, in a blit pass; a multisampled pair of one count
/// too.
#[test]
fn a_depth_copy_is_one_blit() {
    let mut rig = Rig::new(0x440);
    let (d1, d2) = (depth_of(&mut rig, 1, 1), depth_of(&mut rig, 2, 1));
    let (m1, m2) = (depth_of(&mut rig, 3, 4), depth_of(&mut rig, 4, 4));
    for (what, src, dst) in [("one sample", d1, d2), ("4 samples", m1, m2)] {
        assert!(rig.core.copy_depth(level0(src), level0(dst)), "{what}");
        let calls = rig.take();
        assert_eq!(blit_records(&calls), ["  blit 4"], "{what}: {calls:#?}");
        let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
        assert!(blit < at(&calls, "  blit 4"), "{what}: {calls:#?}");
    }
}

/// A depth copy it cannot make is refused, and nothing is recorded.
///
/// No such texture at either end, a colour texture at either end, one level at both ends,
/// another size, format or sample count, a level past the last.
#[test]
fn a_refused_depth_copy_records_nothing() {
    let mut rig = Rig::new(0x441);
    let (d1, _) = rig.depth(1, D32F_S8, (64, 32), 1);
    let d2 = level0(rig.depth(2, D32F_S8, (64, 32), 1).0);
    let small = level0(rig.depth(3, D32F_S8, (32, 32), 1).0);
    let d32f = level0(rig.depth(4, D32F, (64, 32), 1).0);
    let color = level0(rig.color(5).0);
    let multisampled = level0(depth_of(&mut rig, 6, 4));
    let unknown = level0(TextureId::from_raw(9));
    let past = DepthRef { id: d1, level: 1 };
    for (what, src, dst) in [
        ("no such source", unknown, d2),
        ("no such destination", level0(d1), unknown),
        ("a colour source", color, d2),
        ("a colour destination", level0(d1), color),
        ("one level at both ends", d2, d2),
        ("another size", small, d2),
        ("another format", d32f, d2),
        ("another sample count", multisampled, d2),
        ("a level past the last", past, d2),
    ] {
        assert!(!rig.core.copy_depth(src, dst), "{what}");
        let calls = rig.take();
        assert!(calls.is_empty(), "{what}: {calls:#?}");
    }
}

/// A depth copy waits for the Clear of its source.
///
/// The source cleared, then copied: the copy's blit pass waits on a fence the Clear's pass
/// updates.
#[test]
fn a_depth_copy_waits_on_the_clear_of_its_source() {
    let mut rig = Rig::new(0x442);
    let (d1, _) = rig.depth(1, D32F_S8, (64, 32), 1);
    let (d2, _) = rig.depth(2, D32F_S8, (64, 32), 1);
    assert!(rig.core.set_depth(Some(level0(d1))));
    rig.core.clear_depth_stencil(3, 0.25, 3);
    assert!(rig.core.copy_depth(level0(d1), level0(d2)));
    let calls = rig.take();
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    assert!(render < blit, "{calls:#?}");
    // The fences the Clear's pass updates (record 34), and those the blit waits for (6) before
    // its copy (4).
    let updated = fence_ids(&calls[render..blit], "  render 34 fence=");
    let waits = fence_ids(&calls[blit..at(&calls, "  blit 4")], "  blit 6 fence=");
    assert!(
        waits.iter().any(|w| updated.contains(w)),
        "the copy waits on {waits:?}, the Clear updates {updated:?}: {calls:#?}"
    );
}

/// A draw against a copied depth-stencil waits for the copy.
///
/// The copy, then a draw with its destination bound: the draw's pass waits on a fence the copy's
/// blit pass updates.
#[test]
fn a_draw_against_a_copied_depth_waits_on_the_copy() {
    let mut rig = Rig::new(0x443);
    let (d1, _) = rig.depth(1, D32F_S8, (64, 32), 1);
    let (d2, _) = rig.depth(2, D32F_S8, (64, 32), 1);
    assert!(rig.core.copy_depth(level0(d1), level0(d2)));
    assert!(rig.core.set_depth(Some(level0(d2))));
    assert_eq!(rig.draw(&key_with(D32F_S8), (tested(), 0, 0.0)), Ok(()));
    let calls = rig.take();
    let blit = at(&calls, "MTLCommandBuffer_blitCommandEncoder");
    let render = at(&calls, "MTLCommandBuffer_renderCommandEncoder");
    assert!(blit < render, "{calls:#?}");
    // The fences the copy's blit pass updates (record 7), and those the draw's pass waits for
    // (33).
    let updated = fence_ids(&calls[blit..render], "  blit 7 fence=");
    let waits = fence_ids(&calls[render..], "  render 33 fence=");
    assert!(
        waits.iter().any(|w| updated.contains(w)),
        "the draw waits on {waits:?}, the copy updates {updated:?}: {calls:#?}"
    );
}
