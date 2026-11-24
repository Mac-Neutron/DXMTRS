//! The depth-stencil a core device draws against (DXMTRS C4b-3d2).
//!
//! One level of a stored depth texture, kept by reference ([`DepthRef`]) beside the colour
//! target. [`CoreDevice::depth_format`] is the one fact the front end reads: the bound one's
//! format, whatever its size (a pass draws to the smaller of the colour target and it, DXMTRS
//! C4b-3d3). A draw's pipeline key then names the format it draws with, or 0, and the key alone
//! decides the pass, the pipeline and the depth-stencil state: a draw whose key names no format
//! draws in a pass without the depth-stencil even while one is bound. A change of depth-stencil
//! ends the open render pass, as a change of colour target does. A depth-stencil copies whole
//! into another of its size, format and sample count (`StretchRect`'s, DXMTRS C4c-4b), and a
//! multisampled one resolves into a single-sample one of its size and format, sample 0 of each
//! plane, through a render pass of the device's own (DXMTRS C4c-4d).

use std::{ffi::CStr, sync::Arc};

use dxmt_core::{
    deptrack::Access,
    encoder::{NO_NEXT, direct, stage},
    format::depth_stencil_planar_flags,
    library::{render_pipeline, render_pipeline_info},
    resource::{
        Texture, TextureAllocationFlags, TextureViewDescriptor, TextureViewKey, texture_info,
    },
};
use mtld3d_core::{
    async_compile::ClearPlanes,
    depth_plane::{DepthEnd, DepthSurface, DepthTransfer, Pairing, PassTargets, pairing},
    depth_stencil_state::{
        DepthStencilSnapshot, StencilFaceDescription, description_from_snapshot, key_from_snapshot,
    },
    ids::TextureId,
    render_scale::RenderScale,
};
use winemetal_sys::{
    Obj,
    wire::{
        WMT_LOAD_ACTION_DONT_CARE, WMT_PIXEL_FORMAT_X32_STENCIL8, WMT_PRIMITIVE_TYPE_TRIANGLE,
        WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_SET_DSSO, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMT_STORE_ACTION_STORE, WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
        WMT_TEXTURE_USAGE_RENDER_TARGET, WMT_TEXTURE_USAGE_SHADER_READ, WMTDepthStencilInfo,
        WMTStencilInfo, WMTTextureInfo, WmtcmdRenderDraw, WmtcmdRenderSetdsso,
    },
};

use super::{
    CoreDevice, CoreOptions, CoreTextureDesc, CoreTextureFlags, LOG_TARGET, Stage, TargetRef,
    TextureBind, TextureKind,
    stretch::{encode_triangle_state, load_internal_functions},
    surface::blit_record,
    target::Shape,
};

/// The depth-stencil resolve's functions: the vertex one, then depth-only and depth-stencil.
const RESOLVE_FUNCTIONS: [&CStr; 3] = [
    c"resolve_depth_vs",
    c"resolve_depth_fs",
    c"resolve_depth_stencil_fs",
];

/// Single-sample nearest-texel depth transfer functions for the unscaled depth plane.
const TRANSFER_FUNCTIONS: [&CStr; 3] = [
    c"transfer_depth_vs",
    c"transfer_depth_fs",
    c"transfer_depth_stencil_fs",
];

/// A depth-stencil: one level of a stored depth texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DepthRef {
    pub id: TextureId,
    pub level: u32,
}

/// A private one-level copy of a bound depth-stencil level for draws that sample it while attached.
pub(super) struct DepthSnapshot {
    texture: Arc<Texture>,
    width: u32,
    height: u32,
    pixel_format: u32,
    samples: u8,
    dirty: bool,
}

/// The depth-stencil as a pass attaches it and a Clear clears it.
pub(super) struct Resolved {
    pub(super) texture: Arc<Texture>,
    /// The view of the depth-stencil's one level.
    pub(super) key: TextureViewKey,
    pub(super) pixel_format: u32,
}

/// The planes (`depth 1, stencil 2`) of the depth-stencil `WMTPixelFormat` `format`; 0 for none.
pub(super) const fn planes(format: u32) -> u8 {
    depth_stencil_planar_flags(format).to_le_bytes()[0]
}

impl CoreDevice {
    /// Makes `depth` the current depth-stencil, or unbinds it.
    ///
    /// False, and nothing changed, when it names no level of a stored depth texture. A change
    /// ends the open render pass, whose attachments are fixed.
    pub fn set_depth(&mut self, depth: Option<DepthRef>) -> bool {
        if let Some(depth) = depth
            && self.depth_level(depth.id, depth.level).is_none()
        {
            return false;
        }
        if depth != self.depth {
            self.close_depth_span();
            if self.render_open {
                self.close_pass();
            }
            self.depth = depth;
            self.target_gen = self.target_gen.wrapping_add(1);
            self.depth_plane.forget_targets();
        }
        true
    }

    /// The current depth-stencil.
    #[must_use]
    pub const fn depth(&self) -> Option<DepthRef> {
        self.depth
    }

    /// The bound depth-stencil's `WMTPixelFormat`; `None` when none is bound.
    ///
    /// Every draw asks this once (its key's depth format), so it takes no reference.
    #[must_use]
    pub fn depth_format(&self) -> Option<u32> {
        let depth = self.depth?;
        self.depth_level(depth.id, depth.level)
            .map(|(_, _, format, _)| format)
    }

    /// The bound depth-stencil's sample count; `None` when none is bound (DXMTRS C4b-3d5).
    ///
    /// A draw's pass attaches it only when it is colour target 0's.
    #[must_use]
    pub fn depth_samples(&self) -> Option<u8> {
        let depth = self.depth?;
        self.depth_level(depth.id, depth.level)
            .map(|(_, _, _, samples)| samples)
    }

    /// The current depth-stencil's width and height; `None` when none is bound.
    #[must_use]
    pub fn depth_size(&self) -> Option<(u32, u32)> {
        if let Some(surface) = self.depth_plane.span_surface() {
            return Some(surface.logical_size);
        }
        let depth = self.depth?;
        self.depth_level(depth.id, depth.level)
            .map(|(width, height, _, _)| (width, height))
    }

    /// The area a pass on the current colour target draws to; `None` when it does not resolve.
    ///
    /// The target's, cut to the bound depth-stencil's when the pass attaches it (`with_depth`):
    /// Metal draws to the smallest attachment.
    pub(super) fn pass_extent(&self, with_depth: bool) -> Option<(u32, u32)> {
        let (width, height) = self.target_size()?;
        Some(match self.depth_size().filter(|_| with_depth) {
            Some((depth_width, depth_height)) => (width.min(depth_width), height.min(depth_height)),
            None => (width, height),
        })
    }

    /// The current depth-stencil as a pass attaches it, whatever its size.
    ///
    /// A multisampled one's view is of the multisample type (DXMTRS C4b-3d5).
    pub(super) fn resolve_depth(&self) -> Option<Resolved> {
        if let Some(surface) = self.depth_plane.span_surface()
            && let Some(texture) = self.depth_plane.plane_for(surface)
        {
            let pixel_format = surface.format as u32;
            let key = TextureViewDescriptor {
                format: pixel_format,
                texture_type: super::textures::two_d(1),
                first_mip: 0,
                mip_count: 1,
                first_slice: 0,
                array_size: 1,
            }
            .key();
            return Some(Resolved {
                texture,
                key,
                pixel_format,
            });
        }
        let depth = self.depth?;
        let (_, _, pixel_format, samples) = self.depth_level(depth.id, depth.level)?;
        let texture = self.stored_texture(depth.id)?;
        let key = TextureViewDescriptor {
            format: pixel_format,
            texture_type: super::textures::two_d(samples),
            first_mip: depth.level,
            mip_count: 1,
            first_slice: 0,
            array_size: 1,
        }
        .key();
        Some(Resolved {
            texture,
            key,
            pixel_format,
        })
    }

    /// The current depth-stencil's view and the planes of `planes` it has, for a clear.
    ///
    /// `None` with nothing to clear: none bound, or no plane left (D3D9's stencil Clear of a
    /// depth-only format), which the core would read as a colour clear.
    fn depth_clear(&self, planes: u32) -> Option<(Arc<Texture>, TextureViewKey, u32)> {
        let resolved = self.resolve_depth()?;
        let planes = planes & u32::from(self::planes(resolved.pixel_format));
        (planes != 0).then_some((resolved.texture, resolved.key, planes))
    }

    /// Clears `planes` (`depth 1, stencil 2`) of the whole current depth-stencil.
    ///
    /// Whatever the colour target's size, and nothing for planes its format has not. `depth` is
    /// clamped to `[0, 1]`, a NaN to 0. A core Clear pass, as [`CoreDevice::clear`]'s: the
    /// optimizer folds it into the next pass that attaches the view and draws to all of it.
    pub fn clear_depth_stencil(&mut self, planes: u32, depth: f32, stencil: u8) {
        let Some((texture, key, planes)) = self.depth_clear(planes) else {
            return;
        };
        if let Some(depth_ref) = self.depth {
            let (clear_target, subresource) = super::depth_clear_key(depth_ref);
            let mut clear_planes = ClearPlanes::empty();
            if planes & 1 != 0 {
                clear_planes.insert(ClearPlanes::DEPTH);
            }
            if planes & 2 != 0 {
                clear_planes.insert(ClearPlanes::STENCIL);
            }
            if !clear_planes.is_empty() {
                self.clear_history
                    .record_key(clear_target, subresource, clear_planes);
            }
            self.mark_depth_dirty(depth_ref);
            if self.depth_plane.span_open() {
                self.depth_plane_write_epoch = self.depth_plane_write_epoch.wrapping_add(1);
            }
        }
        let depth = clear_depth(depth);
        self.close_pass();
        self.emit(move |enc| enc.clear_depth_stencil(&texture, key, 0, planes, depth, stencil));
    }

    /// Clears `planes` of `rects` (`x, y, width, height`, each inside it) of the depth-stencil.
    ///
    /// The planes and `depth` as [`CoreDevice::clear_depth_stencil`]'s; nothing for no rect. The
    /// core's quad clear on the depth-stencil alone, as [`CoreDevice::clear_rects`]'s.
    pub fn clear_depth_stencil_rects(
        &mut self,
        rects: &[[u32; 4]],
        planes: u32,
        depth: f32,
        stencil: u8,
    ) {
        if rects.is_empty() {
            return;
        }
        let Some((texture, key, planes)) = self.depth_clear(planes) else {
            return;
        };
        if let Some(depth_ref) = self.depth {
            self.mark_depth_dirty(depth_ref);
            if self.depth_plane.span_open() {
                self.depth_plane_write_epoch = self.depth_plane_write_epoch.wrapping_add(1);
            }
        }
        let depth = clear_depth(depth);
        let rects = super::recorder::scissor_rects(rects);
        self.close_pass();
        self.emit(move |enc| {
            enc.with_clear_rtv(|quad, enc| {
                quad.begin(enc, Arc::clone(&texture), key, 0, planes);
                for &rect in &rects {
                    quad.clear_depth_stencil(enc, rect, depth, stencil);
                }
                quad.end(enc);
            });
        });
    }

    /// Copies the whole of the depth-stencil `src` into `dst`, both planes.
    ///
    /// `StretchRect`'s depth-stencil copy (DXMTRS C4c-4b), recorded as [`CoreDevice::copy`]
    /// records: one blit of the level at slice 0 in the blit pass uploads share (the open render
    /// pass ends), the source's level tracked as read and the destination's as written. The
    /// blit has no plane option: one copy moves the depth and the stencil. Two multisampled
    /// levels of one count are one blit too, as mtld3d records them. False, and nothing
    /// recorded, unless both name a level of a stored depth texture, the two differ, and their
    /// width, height, pixel format and sample count are equal.
    #[must_use]
    pub fn copy_depth(&mut self, src: DepthRef, dst: DepthRef) -> bool {
        if self
            .depth_plane
            .span_surface()
            .is_some_and(|s| s.id == src.id || s.id == dst.id)
        {
            self.close_depth_span();
        }
        let (Some(from), Some(into)) = (
            self.depth_level(src.id, src.level),
            self.depth_level(dst.id, dst.level),
        ) else {
            return false;
        };
        if src == dst || from != into {
            return false;
        }
        let (Some(source), Some(destination)) =
            (self.stored_texture(src.id), self.stored_texture(dst.id))
        else {
            return false;
        };
        let (width, height, pixel_format, samples) = from;
        let shape = |level| Shape {
            width,
            height,
            pixel_format,
            srgb_format: 0,
            texel_bytes: 4,
            level,
            slice: 0,
            samples,
        };
        let (src_shape, dst_shape) = (shape(src.level), shape(dst.level));
        self.open_blit_pass();
        self.emit(move |enc| {
            let src =
                enc.access_texture::<{ stage::COMPUTE }>(&source, src_shape.level, 0, Access::READ);
            let dst = enc.access_texture::<{ stage::COMPUTE }>(
                &destination,
                dst_shape.level,
                0,
                Access::WRITE,
            );
            enc.encode_blit_command(blit_record(
                (src, &src_shape),
                [0, 0, width, height],
                (dst, &dst_shape),
                [0, 0],
            ));
        });
        self.mark_depth_dirty(dst);
        true
    }

    /// Resolves the multisampled depth-stencil `src` into `dst`: sample 0 of each plane.
    ///
    /// `StretchRect`'s depth-stencil resolve (DXMTRS C4c-4d), one render pass of its own on the
    /// destination's level: no colour target; the depth plane, and the stencil plane of a format
    /// that has one, attached for writing, loaded as don't-care and stored; the level's extent as
    /// its size, so a Clear of the destination recorded before folds into it and is dropped. One
    /// triangle covers the level (viewport, depth 0 to 1, and scissor the level's; filling and
    /// culling nothing) under a state that always passes and writes the depth, and replaces the
    /// stencil on both faces when the format has that plane; each fragment writes sample 0 of the
    /// source texel under it. A depth-only source is read itself. A source with a stencil plane
    /// is first copied, as [`CoreDevice::copy_depth`] copies, into a scratch texture of its shape
    /// made with views in another format (its `X32_Stencil8` view needs that usage, which the
    /// core's depth textures lack), whose two planes are read; the store forgets the scratch
    /// once its copy is recorded, the closures holding it. Each read is tracked, so a later write
    /// of the source waits for it.
    /// The open pass ends first and none is open after; the device's draw state is neither read
    /// nor changed.
    ///
    /// False, and nothing recorded, unless both name a level of a stored depth texture, `src` has
    /// more than one sample and `dst` one, and their width, height and pixel format are equal; or
    /// when Metal makes no function or pipeline for it (each logged once, then kept as none) or
    /// no depth-stencil state (logged). Those are made before anything is recorded.
    #[must_use]
    pub fn depth_resolve(&mut self, src: DepthRef, dst: DepthRef) -> bool {
        if self
            .depth_plane
            .span_surface()
            .is_some_and(|s| s.id == src.id || s.id == dst.id)
        {
            self.close_depth_span();
        }
        let (Some(from), Some(into)) = (
            self.depth_level(src.id, src.level),
            self.depth_level(dst.id, dst.level),
        ) else {
            return false;
        };
        let (width, height, pixel_format, samples) = from;
        if samples < 2 || into != (width, height, pixel_format, 1) {
            return false;
        }
        let (Some(source), Some(destination)) =
            (self.stored_texture(src.id), self.stored_texture(dst.id))
        else {
            return false;
        };
        let planes = planes(pixel_format);
        let has_stencil = planes & 2 != 0;
        let Some(pso) = self.resolve_pipeline(pixel_format) else {
            return false;
        };
        let state = DepthStencilSnapshot::depth_stencil_overwrite();
        let dsso = self.depth_state_handle(&state.gated_on_stencil_attachment(has_stencil));
        // Metal's default state, which handle 0 stands for, writes no depth.
        if dsso == 0 {
            return false;
        }
        let (read, level) = if has_stencil {
            let scratch = DepthRef {
                id: TextureId::new_unique(),
                level: 0,
            };
            let desc = CoreTextureDesc {
                pixel_format,
                kind: TextureKind::Tex2d,
                width,
                height,
                depth: 1,
                levels: 1,
                flags: CoreTextureFlags::RENDER_TARGET | CoreTextureFlags::VIEW_TWINS,
                srgb_format: 0,
                block: (1, 1, 4),
                samples,
            };
            self.texture_create(scratch.id, &desc);
            let copied = self.copy_depth(src, scratch);
            let texture = self.stored_texture(scratch.id);
            // The closures hold the texture; the store lets it go whatever happens next.
            self.forget_texture(scratch.id);
            // Never taken: a fresh id of the source's shape, which `copy_depth` always copies.
            let (true, Some(texture)) = (copied, texture) else {
                return false;
            };
            (texture, scratch.level)
        } else {
            (source, src.level)
        };
        let key = |format, samples, level| {
            TextureViewDescriptor {
                format,
                texture_type: super::textures::two_d(samples),
                first_mip: level,
                mip_count: 1,
                first_slice: 0,
                array_size: 1,
            }
            .key()
        };
        // The key `resolve_depth` gives the destination when it is bound, which its Clear has.
        let target = key(pixel_format, 1, dst.level);
        let depth_view = key(pixel_format, samples, level);
        let stencil_view = has_stencil.then(|| key(WMT_PIXEL_FORMAT_X32_STENCIL8, samples, level));
        self.close_pass();
        let counted = self.open_brackets.is_some();
        self.emit(move |enc| {
            enc.start_render_pass(planes, 0, 0, None);
            // One attachment a plane, both of the one view, as a draw's pass attaches them.
            let mut plane = |bit: u8| {
                if planes & bit == 0 {
                    return None;
                }
                enc.attachment::<{ stage::PIXEL }>(&destination, target, Access::WRITE)
            };
            let (depth_plane, stencil_plane) = (plane(1), plane(2));
            if let Some(targets) = enc.render_targets() {
                // Don't-care: the triangle writes every texel of each plane.
                if planes & 1 != 0 {
                    targets.depth.target.attachment = depth_plane;
                    targets.depth.target.load_action = WMT_LOAD_ACTION_DONT_CARE;
                    targets.depth.target.store_action = WMT_STORE_ACTION_STORE;
                }
                if planes & 2 != 0 {
                    targets.stencil.target.attachment = stencil_plane;
                    targets.stencil.target.load_action = WMT_LOAD_ACTION_DONT_CARE;
                    targets.stencil.target.store_action = WMT_STORE_ACTION_STORE;
                }
                (targets.render_target_width, targets.render_target_height) = (width, height);
            }
            encode_triangle_state(enc, pso, [0, 0, width, height], counted);
            enc.encode_render_command(WmtcmdRenderSetdsso {
                r#type: WMT_RENDER_COMMAND_SET_DSSO,
                reserved: [0; 3],
                next: NO_NEXT,
                dsso,
                stencil_ref: 0,
            });
            direct::bind_texture::<{ stage::PIXEL }>(enc, 0, Some((&*read, depth_view)));
            if let Some(view) = stencil_view {
                direct::bind_texture::<{ stage::PIXEL }>(enc, 1, Some((&*read, view)));
            }
            enc.encode_render_command(WmtcmdRenderDraw {
                r#type: WMT_RENDER_COMMAND_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                vertex_start: 0,
                vertex_count: 3,
                instance_count: 1,
                base_instance: 0,
            });
            enc.end_pass();
        });
        self.mark_depth_dirty(dst);
        true
    }

    /// Marks any cached sampling snapshot for `depth` dirty so the next sampling draw refreshes it.
    pub(super) fn mark_depth_dirty(&mut self, depth: DepthRef) {
        if let Some(snap) = self.depth_snapshots.get_mut(&depth) {
            snap.dirty = true;
        }
    }

    /// Drops all cached sampling snapshots belonging to texture `id`.
    pub(super) fn forget_depth_snapshots(&mut self, id: TextureId) {
        self.depth_snapshots.retain(|depth, _| depth.id != id);
    }

    /// Returns a private single-level snapshot texture of `depth` for sampling while attached.
    ///
    /// Blits the full level from `depth` when the snapshot is first created or marked dirty by a
    /// depth write, clear, upload, copy, or resolve.
    pub(super) fn depth_snapshot_for_sampling(&mut self, depth: DepthRef) -> Option<Arc<Texture>> {
        let (width, height, pixel_format, samples) = self.depth_level(depth.id, depth.level)?;
        let source = self.stored_texture(depth.id)?;
        if self.depth_snapshots.get(&depth).is_some_and(|snap| {
            (snap.width, snap.height, snap.pixel_format, snap.samples)
                != (width, height, pixel_format, samples)
        }) {
            self.depth_snapshots.remove(&depth);
        }
        let snap = self.depth_snapshots.entry(depth).or_insert_with(|| {
            let usage = WMT_TEXTURE_USAGE_SHADER_READ
                | WMT_TEXTURE_USAGE_RENDER_TARGET
                | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW;
            let info = WMTTextureInfo {
                pixel_format,
                width,
                height,
                depth: 1,
                array_length: 1,
                bits: texture_info::info_bits(
                    super::textures::two_d(samples),
                    1,
                    u32::from(samples),
                    usage,
                ),
                options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
                flags: 0,
                mach_port: 0,
                gpu_resource_id: 0,
            };
            let texture = Arc::new(Texture::new(info, self.device.device()));
            texture.rename(texture.allocate(TextureAllocationFlags::GPU_PRIVATE));
            if !self.options.contains(CoreOptions::LOGGED_DEPTH_SNAPSHOT) {
                self.options.insert(CoreOptions::LOGGED_DEPTH_SNAPSHOT);
                log::info!(
                    target: LOG_TARGET,
                    "d3d9 core: depth snapshot: a draw samples the bound depth attachment; copying it ({width}x{height} format {pixel_format}) before such draws"
                );
            }
            DepthSnapshot {
                texture,
                width,
                height,
                pixel_format,
                samples,
                dirty: true,
            }
        });
        let snap_tex = Arc::clone(&snap.texture);
        let needs_copy = snap.dirty;
        snap.dirty = false;
        if needs_copy {
            let shape = |level| Shape {
                width,
                height,
                pixel_format,
                srgb_format: 0,
                texel_bytes: 4,
                level,
                slice: 0,
                samples,
            };
            let (src_shape, dst_shape) = (shape(depth.level), shape(0));
            self.open_blit_pass();
            let dst_tex = Arc::clone(&snap_tex);
            self.emit(move |enc| {
                let src = enc.access_texture::<{ stage::COMPUTE }>(
                    &source,
                    src_shape.level,
                    0,
                    Access::READ,
                );
                let dst = enc.access_texture::<{ stage::COMPUTE }>(&dst_tex, 0, 0, Access::WRITE);
                enc.encode_blit_command(blit_record(
                    (src, &src_shape),
                    [0, 0, width, height],
                    (dst, &dst_shape),
                    [0, 0],
                ));
            });
        }
        Some(snap_tex)
    }

    /// The handle of the depth-stencil state of `snapshot`, made at its first request.
    ///
    /// The device keeps it for its life: a recorded closure names it by handle. 0 (Metal's
    /// default state) when Metal makes none, which is logged and not kept.
    pub(super) fn depth_state_handle(&mut self, snapshot: &DepthStencilSnapshot) -> u64 {
        let key = key_from_snapshot(snapshot);
        if let Some(state) = self.depth_states.get(&key.raw()) {
            return state.handle();
        }
        let desc = description_from_snapshot(snapshot, key);
        let off = WMTStencilInfo {
            enabled: 0,
            depth_stencil_pass_op: 0,
            stencil_fail_op: 0,
            depth_fail_op: 0,
            stencil_compare_function: 0,
            write_mask: 0,
            read_mask: 0,
        };
        let (front_stencil, back_stencil) = desc.stencil.as_ref().map_or((off, off), |stencil| {
            let face = |face: &StencilFaceDescription| WMTStencilInfo {
                enabled: 1,
                depth_stencil_pass_op: face.pass_op as u8,
                stencil_fail_op: face.stencil_fail_op as u8,
                depth_fail_op: face.depth_fail_op as u8,
                stencil_compare_function: face.compare_func as u8,
                write_mask: stencil.write_mask.to_le_bytes()[0],
                read_mask: stencil.read_mask.to_le_bytes()[0],
            };
            (face(&stencil.front), face(&stencil.back))
        });
        let info = WMTDepthStencilInfo {
            depth_compare_function: desc.depth_compare_func as u8,
            depth_write_enabled: u8::from(desc.depth_write_enable),
            front_stencil,
            back_stencil,
        };
        let Some(state) = dxmt_core::library::depth_stencil_state(self.device.device(), &info)
        else {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: no depth-stencil state for {:#x}; its draws take Metal's default",
                key.raw()
            );
            return 0;
        };
        let handle = state.handle();
        self.depth_states.insert(key.raw(), state);
        handle
    }

    /// The resolve's pipeline into the depth-stencil `format`, made at its first request.
    ///
    /// Through dxmt-core's `render_pipeline`: the vertex function and the fragment function of
    /// the format's planes, the depth pixel format `format` and the stencil one too for a format
    /// with a stencil plane, no colour, one sample. One Metal does not make is logged and kept as
    /// none, never made again; none when the command library or a function is missing.
    fn resolve_pipeline(&mut self, format: u32) -> Option<u64> {
        if let Some(pso) = self.resolve_pipelines.get(&format) {
            return pso.as_ref().map(Obj::handle);
        }
        let [vs, depth_fs, stencil_fs] = self
            .resolve_functions
            .get_or_init(|| {
                load_internal_functions(
                    &self.device,
                    RESOLVE_FUNCTIONS,
                    "the depth-stencil resolve",
                    "depth-stencil resolves",
                )
            })
            .as_ref()?
            .each_ref()
            .map(Obj::handle);
        let planes = planes(format);
        let mut info = render_pipeline_info();
        info.vertex_function = vs;
        info.fragment_function = if planes & 2 != 0 {
            stencil_fs
        } else {
            depth_fs
        };
        if planes & 1 != 0 {
            info.depth_pixel_format = format;
        }
        if planes & 2 != 0 {
            info.stencil_pixel_format = format;
        }
        let pso = render_pipeline(self.device.device(), &info);
        if pso.is_none() {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: the depth-stencil resolve's pipeline into format {format} does not \
                 build; those resolves are refused"
            );
        }
        let handle = pso.as_ref().map(Obj::handle);
        self.resolve_pipelines.insert(format, pso);
        handle
    }

    /// The single-sample depth transfer pipeline into `format`, built at first request.
    fn transfer_pipeline(&mut self, format: u32) -> Option<u64> {
        if let Some(pso) = self.transfer_pipelines.get(&format) {
            return pso.as_ref().map(Obj::handle);
        }
        let [vs, depth_fs, stencil_fs] = self
            .transfer_functions
            .get_or_init(|| {
                load_internal_functions(
                    &self.device,
                    TRANSFER_FUNCTIONS,
                    "the depth transfer",
                    "depth transfers",
                )
            })
            .as_ref()?
            .each_ref()
            .map(Obj::handle);
        let planes = planes(format);
        let mut info = render_pipeline_info();
        info.vertex_function = vs;
        info.fragment_function = if planes & 2 != 0 {
            stencil_fs
        } else {
            depth_fs
        };
        if planes & 1 != 0 {
            info.depth_pixel_format = format;
        }
        if planes & 2 != 0 {
            info.stencil_pixel_format = format;
        }
        let pso = render_pipeline(self.device.device(), &info);
        if pso.is_none() {
            log::error!(
                target: LOG_TARGET,
                "d3d9 core: the depth transfer pipeline into format {format} does not build; \
                 depth plane transfers are refused"
            );
        }
        let handle = pso.as_ref().map(Obj::handle);
        self.transfer_pipelines.insert(format, pso);
        handle
    }

    /// Returns `(scale, is_null, samples)` of bound colour target 0, or `None` if unresolved.
    fn target_scale_and_samples(&self) -> Option<(RenderScale, bool, u8)> {
        match self.target {
            TargetRef::BackBuffer => self
                .back_buffer
                .as_ref()
                .map(|bb| (self.back_buffer_scale, false, bb.drawn().2)),
            TargetRef::Texture { id, level, face } => {
                let shape = self.texture_shape(id, level, face)?;
                Some((self.texture_scale(id), false, shape.samples))
            }
            TargetRef::Null => {
                let depth = self.depth?;
                let (_, _, _, samples) = self.depth_level(depth.id, depth.level)?;
                Some((RenderScale::IDENTITY, true, samples))
            }
        }
    }

    /// Evaluates whether target 0 + depth require an unscaled depth plane, returning it if mixed.
    fn mixed_depth_surface(&self) -> Option<DepthSurface> {
        let (rt0_scale, rt0_is_null, rt0_samples) = self.target_scale_and_samples()?;
        let depth = self.depth?;
        let (width, height, pixel_format, depth_samples) =
            self.depth_level(depth.id, depth.level)?;
        let depth_scale = self.texture_scale(depth.id);
        match pairing(&PassTargets {
            rt0_scale,
            rt0_is_null,
            rt0_samples,
            depth_scale,
            depth_samples,
        }) {
            Pairing::Shared => None,
            Pairing::MixedMultisampled => {
                mtld3d_shared::log_once_warn!(
                    target: LOG_TARGET,
                    "a pass through an unscaled render target over a multisampled scaled depth \
                     surface draws on the surface's scaled grid: Metal offers no depth transfer \
                     into a multisampled texture"
                );
                None
            }
            Pairing::Unserved => {
                mtld3d_shared::log_once_warn!(
                    target: LOG_TARGET,
                    "a pass through a scaled render target over a depth surface at another scale \
                     (unscaled, or scaled for another back-buffer size) draws into the surface on \
                     the target's grid: its depth lands off the reported pixels"
                );
                None
            }
            Pairing::Mixed => {
                let format = mtld3d_shared::mtl::PixelFormat::from_repr(pixel_format)?;
                let logical_base = self.texture_logical_size(depth.id)?;
                let logical_size = (
                    logical_base.0.checked_shr(depth.level).unwrap_or(0).max(1),
                    logical_base.1.checked_shr(depth.level).unwrap_or(0).max(1),
                );
                Some(DepthSurface {
                    id: depth.id,
                    level: depth.level,
                    texture_size: (width, height),
                    logical_size,
                    format,
                    scale: depth_scale,
                    is_sampleable: self.is_depth_sampleable(depth.id),
                })
            }
        }
    }

    /// Classifies bound targets if unknown and returns whether a mixed depth span applies.
    fn classify_mixed_targets(&mut self) -> Option<bool> {
        if !self.depth_plane.targets_known() {
            let mixed = self.mixed_depth_surface();
            self.depth_plane.note_targets(mixed.as_ref());
        }
        self.depth_plane.mixed_has_stencil()
    }

    /// Ensures the cached unscaled depth plane matches `surface`, allocating when needed.
    fn ensure_depth_plane(&mut self, surface: &DepthSurface) -> Option<Arc<Texture>> {
        if let Some(plane) = self.depth_plane.plane_for(surface) {
            return Some(plane);
        }
        if self.depth_plane.refused_for(surface) {
            return None;
        }
        let info = WMTTextureInfo {
            pixel_format: surface.format as u32,
            width: surface.logical_size.0,
            height: surface.logical_size.1,
            depth: 1,
            array_length: 1,
            bits: texture_info::info_bits(
                super::textures::two_d(1),
                1,
                1,
                WMT_TEXTURE_USAGE_SHADER_READ
                    | WMT_TEXTURE_USAGE_RENDER_TARGET
                    | WMT_TEXTURE_USAGE_PIXEL_FORMAT_VIEW,
            ),
            options: WMT_RESOURCE_STORAGE_MODE_PRIVATE,
            flags: 0,
            mach_port: 0,
            gpu_resource_id: 0,
        };
        let texture = Arc::new(Texture::new(info, self.device.device()));
        let alloc = texture.allocate(TextureAllocationFlags::GPU_PRIVATE);
        if alloc.texture() == 0 {
            self.depth_plane.note_refused(surface);
            return None;
        }
        texture.rename(alloc);
        let _ = self
            .depth_plane
            .replace_plane(Arc::clone(&texture), surface);
        Some(texture)
    }

    /// Records a nearest-texel single-sample depth (and optional stencil) transfer.
    fn record_depth_transfer(&mut self, transfer: &DepthTransfer<Arc<Texture>>) -> bool {
        let resolve_end = |end: &DepthEnd<Arc<Texture>>| -> Option<(Arc<Texture>, u32, bool)> {
            match end {
                DepthEnd::Surface { id, level } => {
                    Some((self.stored_texture(*id)?, *level, self.view_twins(*id)))
                }
                DepthEnd::Plane(tex) => Some((Arc::clone(tex), 0, true)),
            }
        };
        let (Some((src_tex, src_level, src_twins)), Some((dst_tex, dst_level, _))) = (
            resolve_end(&transfer.source),
            resolve_end(&transfer.destination),
        ) else {
            return false;
        };
        let pixel_format = transfer.format as u32;
        let planes = planes(pixel_format);
        let has_stencil = planes & 2 != 0;
        let Some(pso) = self.transfer_pipeline(pixel_format) else {
            return false;
        };
        let state = DepthStencilSnapshot::depth_stencil_overwrite();
        let dsso = self.depth_state_handle(&state.gated_on_stencil_attachment(has_stencil));
        if dsso == 0 {
            return false;
        }
        let (src_w, src_h) = transfer.source_size;
        let (dst_w, dst_h) = transfer.destination_size;
        let (read_tex, read_level) = if has_stencil && !src_twins {
            let scratch_id = TextureId::new_unique();
            let desc = CoreTextureDesc {
                pixel_format,
                kind: TextureKind::Tex2d,
                width: src_w,
                height: src_h,
                depth: 1,
                levels: 1,
                flags: CoreTextureFlags::RENDER_TARGET | CoreTextureFlags::VIEW_TWINS,
                srgb_format: 0,
                block: (1, 1, 4),
                samples: 1,
            };
            self.texture_create(scratch_id, &desc);
            let scratch = self.stored_texture(scratch_id);
            let Some(scratch) = scratch else {
                self.forget_texture(scratch_id);
                return false;
            };
            let blit_shape = |level| Shape {
                width: src_w,
                height: src_h,
                pixel_format,
                srgb_format: 0,
                texel_bytes: 4,
                level,
                slice: 0,
                samples: 1,
            };
            let (s_shape, d_shape) = (blit_shape(src_level), blit_shape(0));
            let s_tex = Arc::clone(&src_tex);
            let d_tex = Arc::clone(&scratch);
            self.open_blit_pass();
            self.emit(move |enc| {
                let src = enc.access_texture::<{ stage::COMPUTE }>(
                    &s_tex,
                    s_shape.level,
                    0,
                    Access::READ,
                );
                let dst = enc.access_texture::<{ stage::COMPUTE }>(&d_tex, 0, 0, Access::WRITE);
                enc.encode_blit_command(blit_record(
                    (src, &s_shape),
                    [0, 0, src_w, src_h],
                    (dst, &d_shape),
                    [0, 0],
                ));
            });
            self.forget_texture(scratch_id);
            (scratch, 0)
        } else {
            (src_tex, src_level)
        };
        let view_key = |format, level| {
            TextureViewDescriptor {
                format,
                texture_type: super::textures::two_d(1),
                first_mip: level,
                mip_count: 1,
                first_slice: 0,
                array_size: 1,
            }
            .key()
        };
        let target_key = view_key(pixel_format, dst_level);
        let depth_view = view_key(pixel_format, read_level);
        let stencil_view = has_stencil.then(|| view_key(WMT_PIXEL_FORMAT_X32_STENCIL8, read_level));
        self.close_pass();
        let counted = self.open_brackets.is_some();
        self.emit(move |enc| {
            enc.start_render_pass(planes, 0, 0, None);
            let mut plane = |bit: u8| {
                if planes & bit == 0 {
                    return None;
                }
                enc.attachment::<{ stage::PIXEL }>(&dst_tex, target_key, Access::WRITE)
            };
            let (depth_plane, stencil_plane) = (plane(1), plane(2));
            if let Some(targets) = enc.render_targets() {
                if planes & 1 != 0 {
                    targets.depth.target.attachment = depth_plane;
                    targets.depth.target.load_action = WMT_LOAD_ACTION_DONT_CARE;
                    targets.depth.target.store_action = WMT_STORE_ACTION_STORE;
                }
                if planes & 2 != 0 {
                    targets.stencil.target.attachment = stencil_plane;
                    targets.stencil.target.load_action = WMT_LOAD_ACTION_DONT_CARE;
                    targets.stencil.target.store_action = WMT_STORE_ACTION_STORE;
                }
                (targets.render_target_width, targets.render_target_height) = (dst_w, dst_h);
            }
            encode_triangle_state(enc, pso, [0, 0, dst_w, dst_h], counted);
            enc.encode_render_command(WmtcmdRenderSetdsso {
                r#type: WMT_RENDER_COMMAND_SET_DSSO,
                reserved: [0; 3],
                next: NO_NEXT,
                dsso,
                stencil_ref: 0,
            });
            direct::bind_texture::<{ stage::PIXEL }>(enc, 0, Some((&*read_tex, depth_view)));
            if let Some(view) = stencil_view {
                direct::bind_texture::<{ stage::PIXEL }>(enc, 1, Some((&*read_tex, view)));
            }
            enc.encode_render_command(WmtcmdRenderDraw {
                r#type: WMT_RENDER_COMMAND_DRAW,
                reserved: [0; 3],
                next: NO_NEXT,
                primitive_type: WMT_PRIMITIVE_TYPE_TRIANGLE,
                vertex_start: 0,
                vertex_count: 3,
                instance_count: 1,
                base_instance: 0,
            });
            enc.end_pass();
        });
        true
    }

    /// Opens an unscaled depth plane span for a draw that reads or writes depth/stencil.
    pub fn open_depth_span_for_draw(&mut self, depth_stencil: &DepthStencilSnapshot) {
        if self.depth_plane.rules_out_span() {
            return;
        }
        let Some(has_stencil) = self.classify_mixed_targets() else {
            return;
        };
        if !depth_stencil.reads_or_writes(has_stencil) {
            return;
        }
        self.open_depth_span();
    }

    /// Opens an unscaled depth plane span for mixed unscaled RT0 over a scaled depth surface.
    pub fn open_depth_span(&mut self) {
        if self.depth_plane.rules_out_span() || self.classify_mixed_targets().is_none() {
            return;
        }
        let Some(surface) = self.mixed_depth_surface() else {
            return;
        };
        let depth_ref = DepthRef {
            id: surface.id,
            level: surface.level,
        };
        let Some(_plane) = self.ensure_depth_plane(&surface) else {
            return;
        };
        let Some(transfer) = self.depth_plane.open(surface) else {
            return;
        };
        if !self.record_depth_transfer(&transfer) {
            self.depth_plane.abandon_span();
            return;
        }
        self.target_gen = self.target_gen.wrapping_add(1);
        self.depth_plane_write_epoch = self.depth_plane_write_epoch.wrapping_add(1);
        self.depth_stand_in_epoch = self.depth_plane_write_epoch;
        self.mark_depth_dirty(depth_ref);
    }

    /// Closes an active depth plane span, carrying the plane back into its depth surface.
    pub fn close_depth_span(&mut self) {
        let Some((transfer, surface)) = self.depth_plane.close() else {
            return;
        };
        let depth_ref = DepthRef {
            id: surface.id,
            level: surface.level,
        };
        let _ = self.record_depth_transfer(&transfer);
        self.target_gen = self.target_gen.wrapping_add(1);
        self.mark_depth_dirty(depth_ref);
    }

    /// Copies the open plane back into its sampleable depth texture level before a sampling draw.
    pub(super) fn write_back_depth_stand_in(&mut self, textures: &[TextureBind]) {
        if self.depth_stand_in_epoch == self.depth_plane_write_epoch {
            return;
        }
        let Some(transfer) = self.depth_plane.write_back_stand_in() else {
            return;
        };
        let DepthEnd::Surface { id, level } = transfer.destination else {
            return;
        };
        if !textures
            .iter()
            .any(|t| matches!(t.stage, Stage::Pixel) && t.id == Some(id))
        {
            return;
        }
        if self.record_depth_transfer(&transfer) {
            mtld3d_shared::log_once_info!(
                target: LOG_TARGET,
                "depth plane: a draw samples the depth texture whose plane is bound; carrying the \
                 plane back into the texture before such draws"
            );
            self.depth_stand_in_epoch = self.depth_plane_write_epoch;
            self.mark_depth_dirty(DepthRef { id, level });
        }
    }

    /// Closes any open span and releases the cached unscaled depth plane allocation.
    pub fn release_depth_plane(&mut self) {
        self.close_depth_span();
        let _ = self.depth_plane.take_plane();
    }
}

/// A Clear's depth as Metal takes it: within `[0, 1]`, a NaN as 0.
const fn clear_depth(depth: f32) -> f32 {
    if depth.is_nan() {
        0.0
    } else {
        depth.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests;
