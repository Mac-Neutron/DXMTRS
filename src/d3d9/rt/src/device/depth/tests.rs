use mtld3d_core::{
    depth_stencil_state::DepthStencilSnapshot, ids::TextureId, pipeline_key::PipelineKey,
};
use winemetal_sys::wire::{
    WMT_PIXEL_FORMAT_BGRA8_UNORM, WMT_PIXEL_FORMAT_DEPTH32_FLOAT,
    WMT_PIXEL_FORMAT_DEPTH32_FLOAT_STENCIL8,
};

use super::{DepthRef, DepthState};
use crate::device::{
    CoreDevice, DrawData, DrawRefused, TargetRef, TextureKind,
    fixture::{
        core_on_fakes, depth_desc, ff_binds, key, line, passes, target_desc, textures_made,
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
        self.core.draw_up(key, &draw)
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
    assert_eq!(rig.core.depth_state(), DepthState::None);
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
    assert_eq!(rig.core.depth_state(), DepthState::Attached(D32F_S8));
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
    assert_eq!(rig.core.depth_state(), DepthState::None);
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
    assert_eq!(rig.core.depth_state(), DepthState::Attached(D32F));
    assert_eq!(rig.draw(&key_with(D32F), INERT), Ok(()));
    let calls = rig.take();
    let bound = passes(&calls)
        .into_iter()
        .filter_map(|pass| plane_handle(pass, "depth"))
        .map(|handle| rig.texture_of(&handle))
        .collect::<Vec<_>>();
    assert_eq!(bound, [texture.clone(), texture], "{calls:#?}");
}

/// A depth-stencil of another size than the colour target is bound but not attachable.
///
/// A key naming its format records nothing, a key without one draws in a pass without it, and a
/// Clear of it records nothing.
#[test]
fn a_depth_of_another_size_is_detached() {
    let mut rig = Rig::new(0x2e6);
    for (id, size) in [(1, (128, 128)), (2, (16, 16))] {
        let (depth, _) = rig.depth(id, D32F_S8, size, 1);
        assert!(rig.core.set_depth(Some(level0(depth))));
        assert_eq!(rig.core.depth_state(), DepthState::Detached(D32F_S8));
        assert_eq!(rig.draw(&key_with(D32F_S8), INERT), Err(DrawRefused::Depth));
        rig.core.clear_depth_stencil(3, 0.5, 1);
        let calls = rig.take();
        assert!(passes(&calls).is_empty(), "{calls:#?}");
        assert_eq!(rig.draw(&key_with(0), INERT), Ok(()));
        let calls = rig.take();
        let all = passes(&calls);
        assert_eq!(all.len(), 1, "{calls:#?}");
        assert_eq!(line(all[0], "  depth "), None, "{calls:#?}");
        assert_eq!(line(all[0], "  render 16 "), None, "{calls:#?}");
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
    rig.core.clear([0.0, 0.0, 1.0, 1.0]);
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
    assert_eq!(rig.core.depth_state(), DepthState::None);
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
    assert_eq!(rig.core.depth_state(), DepthState::Detached(D32F));
    assert!(rig.core.set_depth(Some(DepthRef {
        id: depth,
        level: 1
    })));
    assert_eq!(rig.core.depth_state(), DepthState::Attached(D32F));
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
