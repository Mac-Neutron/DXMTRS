//! The entry points the core runtime implements (DXMTRS C4a): thin arms over `d3d9_rt`.
//!
//! Each answers `None` for a device of mtld3d's runtime, whose entry point then runs as before,
//! and the call's result for a device of the core runtime. `StretchRect`, `ColorFill` and
//! `UpdateSurface` run mtld3d's gates on both runtimes, so a call they reject answers
//! `D3DERR_INVALIDCALL` on both, and a core device leaves past them for `stretch_colour` (a
//! colour pair) or `stretch_depth` (a depth-stencil pair), `color_fill` for a colour target, or
//! `update_color_target` for a region of the back buffer or a render target (DXMTRS C4c-4a).
//! `stretch_colour` resolves a multisampled target into a single-sample one. Into a target it
//! copies texel for texel on the GPU (DXMTRS C4c-4b), or samples the source as mtld3d's quad
//! does: scaled, converting, from packed or planar YUV, into and from a multisampled target,
//! between overlapping rects of one texture, through a scratch texture where a shape needs one
//! (DXMTRS C4c-4c). Into a plain surface it copies on the CPU, from another (DXMTRS C4c-4b) or
//! from itself (DXMTRS C4c-4c). `stretch_depth` copies a depth-stencil surface whole into another
//! (DXMTRS C4c-4b), or resolves a multisampled one into a single-sample one, sample 0 of each
//! plane (DXMTRS C4c-4d). The colour arm tests the destination itself, since on a core device the
//! GPU writes nothing but targets. Every present, whatever its arguments and entry, presents the
//! whole back buffer into the device window, as on mtld3d (DXMTRS C4c-6a). `resz` answers
//! `SetRenderState(D3DRS_POINTSIZE, RESZ)`: the bound depth-stencil copied or resolved into the
//! depth texture at stage 0 (DXMTRS C4c-7a). What an arm does not cover yet is refused with one
//! warning (C4-13): of `StretchRect`, all in the colour arm, an extended device's whole-surface
//! copy into a level that is no target (two faces of one cube included) or from a surface that is
//! no plain one, a pair of plain surfaces whose formats no CPU conversion serves, and a
//! multisampled source of a format this GPU does not resolve; of RESZ, a dynamic depth texture as
//! its destination while a depth-stencil is bound. Clear
//! and the read-back go to and come from the bound colour target (DXMTRS C4b-3d1). The UP draws
//! are `core_draw`'s.

use core::ffi::c_void;

use d3d9_rt::device::{
    CoreDevice, CoreTextureDesc, CoreTextureFlags, DepthRef, StretchQuad, TargetRef, TextureKind,
    UploadRegion, view_format,
};
use mtld3d_core::{
    buffer_rename::BufferMapMode,
    clear_area::{ClearArea, clear_area, clear_regions},
    convert::d3dcolor_to_rgba_f32,
    dirty_rect::DirtyRect,
    ids::TextureId,
    pipeline_key::drawn_rect,
    pixel_convert::{ConvertRegion, can_convert, convert_region},
    readback::{ReadbackDestination, ReadbackSource, reject_readback_dst},
    render_state::scissor_region,
    stretch_rect::{BlitDecode, SameSurfaceRoute, StretchRegion, blit_decode, same_surface_route},
    upload_pass::is_expanded_upload,
};
use mtld3d_shared::{InPtr, ValueIn};
use mtld3d_types::{
    D3D_OK, D3DCLEAR_STENCIL, D3DCLEAR_TARGET, D3DCLEAR_ZBUFFER, D3DERR_INVALIDCALL,
    D3DERR_NOTAVAILABLE, D3DFMT_X8R8G8B8, D3DPOOL_DEFAULT, D3DPRESENT_PARAMETERS, D3DRECT,
    D3DRS_SCISSORTESTENABLE, D3DRS_SRGBWRITEENABLE, D3DSURFACE_DESC, D3DTEXF_LINEAR,
    D3DUSAGE_DYNAMIC, D3DUSAGE_RENDERTARGET,
};

use super::{
    DeviceInner, Direct3DDevice9, StretchSurfaceFlags, StretchSurfaceInfo, refuse_under_core,
};
use crate::{
    bound_rt::RENDER_TARGET_SLOTS,
    surface::{Direct3DSurface9, SystemMemoryDst},
    texture::SourceImage,
    vertex_buffer::VertexBufferInner,
};

unsafe extern "system" {
    fn IsWindowVisible(window: *mut c_void) -> i32;
}

/// The device at `this` when it runs the core runtime.
fn core_device<'a>(this: *mut c_void) -> Option<&'a mut DeviceInner> {
    // SAFETY: vtable thunk; `this` is null or a *mut Direct3DDevice9 per the ABI.
    let obj = unsafe { InPtr::<Direct3DDevice9>::opt(this) }?;
    let dev = obj.inner();
    dev.is_core().then_some(dev)
}

/// `Clear`: the rects, the scissor and the viewport bound it, as under mtld3d.
///
/// Z and stencil go to the bound depth-stencil (DXMTRS C4b-3d2): `D3DERR_INVALIDCALL` with none
/// bound, as mtld3d answers. The colour goes to every bound colour target, index 0-3, in the
/// pass set or not (DXMTRS C4b-3d4), but one of target 0's size and another sample count, which
/// mtld3d's Clear leaves alone (`clear_size`, DXMTRS C4b-3d5), each with the viewport drawn on
/// its own extent; the core device picks each one's view and encodes the colour once under sRGB
/// write. Each colour target and the depth-stencil is cleared whole when a region covers it, and
/// by rects otherwise ([`clear_area`], DXMTRS C4b-3d3), judged against its own size: the
/// depth-stencil's with the viewport drawn on target 0. The stencil flag against a format with
/// no stencil plane clears nothing, as D3D9 has it (the core device masks the planes).
pub fn clear(
    this: *mut c_void,
    count: u32,
    rects: *const c_void,
    flags: u32,
    color: u32,
    z: f32,
    stencil: u32,
) -> Option<i32> {
    let dev = core_device(this)?;
    let depth_flags = flags & (D3DCLEAR_ZBUFFER | D3DCLEAR_STENCIL);
    if depth_flags != 0 && !dev.depth_stencil_bound() {
        return Some(D3DERR_INVALIDCALL);
    }
    let scissor_on = dev.render_state(D3DRS_SCISSORTESTENABLE as usize) != 0;
    let regions = clear_regions(
        (!rects.is_null()).then(|| super::clear_target_rects(count, rects)),
        scissor_on.then(|| scissor_region(dev.scissor_rect())),
    );
    // A core device always has a target (its back buffer is made with it); were there none, the
    // Clear would have nothing to clear.
    let Some(target) = dev.core.as_deref()?.target_size() else {
        return Some(D3D_OK);
    };
    let set_viewport = dev.viewport();
    let viewport = drawn_rect(&set_viewport, target);
    if flags & D3DCLEAR_TARGET != 0 {
        let srgb = dev.render_state(D3DRS_SRGBWRITEENABLE as usize) != 0;
        let linear = d3dcolor_to_rgba_f32(color);
        let core = dev.core.as_deref_mut()?;
        for slot in 0..RENDER_TARGET_SLOTS {
            let Some(extent) = core.clear_size(slot) else {
                continue;
            };
            match clear_area(
                regions.as_deref(),
                drawn_rect(&set_viewport, extent),
                extent,
            ) {
                ClearArea::Whole => core.clear(slot, linear, srgb),
                ClearArea::Regions(list) => core.clear_rects(slot, &list, linear, srgb),
            }
        }
    }
    let core = dev.core.as_deref_mut()?;
    // A depth-stencil of another sample count than target 0's is in no pass, and mtld3d's Clear
    // of its planes does nothing (DXMTRS C4b-3d5).
    if depth_flags != 0
        && core.depth_samples() == Some(core.target_samples())
        && let Some(extent) = core.depth_size()
    {
        let planes = u32::from(flags & D3DCLEAR_ZBUFFER != 0)
            | u32::from(flags & D3DCLEAR_STENCIL != 0) << 1;
        let stencil = stencil.to_le_bytes()[0];
        match clear_area(regions.as_deref(), viewport, extent) {
            ClearArea::Whole => core.clear_depth_stencil(planes, z, stencil),
            ClearArea::Regions(list) => core.clear_depth_stencil_rects(&list, planes, z, stencil),
        }
    }
    Some(D3D_OK)
}

/// A present of the whole back buffer into the device window, paced by its interval.
///
/// Every present of a core device: `DeviceInner::present` calls it first, for `Present`,
/// `PresentEx` and a swap chain's `Present` alike, after mtld3d's entry code warned once for each
/// rect, window, region and flag it ignores, as mtld3d ignores them (DXMTRS C4c-6a). `pp` is the
/// device's, so an additional swap chain presents the device's back buffer into the device window
/// at the device's interval, as on mtld3d.
pub fn present(core: &mut CoreDevice, pp: &D3DPRESENT_PARAMETERS) -> i32 {
    let window = pp.device_window as *mut c_void;
    let interval = pp.presentation_interval;
    // A hidden or minimized window presents nothing (G4): its drawable would never come back.
    // SAFETY: user32's query; any window handle, even a stale one, is a valid argument.
    let visible = unsafe { IsWindowVisible(window) } != 0 && !dxmt_util::wsi::is_minimized(window);
    let refresh = crate::direct3d9::current_adapter_display_mode().refresh_rate;
    let after = d3d9_rt::device::present_after(interval, refresh);
    core.present(visible, after);
    D3D_OK
}

/// `GetRenderTargetData` from a colour target the core draws to.
///
/// mtld3d's checks, in its order and with its answers: a source or destination that is no
/// surface, a multisampled source (DXMTRS C4b-3d5), a destination that is not system memory, a
/// source that is no render target and a destination of another size or format are
/// `D3DERR_INVALIDCALL`. An additional swap chain's back buffer reads as the device's, which it
/// aliases (C4b3d1-4), as on mtld3d (DXMTRS C4c-3b).
pub fn get_render_target_data(this: *mut c_void, rt: *mut c_void, dst: *mut c_void) -> Option<i32> {
    let dev = core_device(this)?;
    // SAFETY: `rt` is null or a caller-owned IDirect3DSurface9* per the ABI.
    let Some(src) = (unsafe { InPtr::<Direct3DSurface9>::opt(rt) }) else {
        return Some(D3DERR_INVALIDCALL);
    };
    // SAFETY: `dst` is null or a caller-owned IDirect3DSurface9* per the ABI.
    let Some(dst) = (unsafe { InPtr::<Direct3DSurface9>::opt(dst) }) else {
        return Some(D3DERR_INVALIDCALL);
    };
    if src.multi_sample().sample_count > 1 {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "GetRenderTargetData: source is multisampled → INVALIDCALL (StretchRect it into a \
             single-sampled surface first)");
        return Some(D3DERR_INVALIDCALL);
    }
    let Some(desc) = dst.system_memory_blit_dst() else {
        return Some(D3DERR_INVALIDCALL);
    };
    let Some(source) = src.core_target() else {
        return Some(D3DERR_INVALIDCALL);
    };
    let Some(from) = surface_desc(&src, rt) else {
        return Some(D3DERR_INVALIDCALL);
    };
    let reject = reject_readback_dst(
        &ReadbackSource {
            width: from.width,
            height: from.height,
            format: from.format,
        },
        &ReadbackDestination {
            width: desc.width,
            height: desc.height,
            format: desc.format,
            bytes_per_row: desc.bytes_per_row,
            len: desc.len,
        },
    );
    if reject.is_some() {
        return Some(D3DERR_INVALIDCALL);
    }
    let core = dev.core.as_deref_mut()?;
    // SAFETY: null, or the source level's parent texture, live while the level is and distinct
    // from the device.
    if let Some(parent) = unsafe { src.parent_texture().as_mut() } {
        // Its core texture exists from here, with every CPU write so far uploaded.
        let _ = crate::texture::core_flush_dirty(parent.inner_mut(), core);
    }
    let ok = core_read_into(&dst, &desc, |bytes, pitch| {
        core.read_back(source, bytes, pitch)
    });
    Some(if ok { D3D_OK } else { D3DERR_INVALIDCALL })
}

/// Runs a core read into a system-memory destination's pages, then re-arms the destination.
///
/// `desc` is what `dst.system_memory_blit_dst()` answered in the same call; `read` gets its pages
/// and its row pitch. The one place those pages become a slice, for `GetRenderTargetData` and
/// `GetFrontBufferData` (DXMTRS C4c-3a). False, and nothing re-armed, when `read` is false or
/// the pages do not fit this process's addresses; the caller logs and answers.
pub fn core_read_into(
    dst: &Direct3DSurface9,
    desc: &SystemMemoryDst,
    read: impl FnOnce(&mut [u8], usize) -> bool,
) -> bool {
    let pages = usize::try_from(desc.ptr)
        .ok()
        .zip(usize::try_from(desc.len).ok());
    let ok = pages.is_some_and(|(addr, len)| {
        let ptr = core::ptr::with_exposed_provenance_mut::<u8>(addr);
        // SAFETY: `len` bytes at `ptr` are `dst`'s own pages (a standalone surface's backing or
        // its system-memory texture level's staging), live while `dst` is, their address exposed
        // by `system_memory_blit_dst`'s cast of the page's pointer. No other reference to them is
        // alive: `system_memory_blit_dst` kept only their address and length, the API lock keeps
        // this call alone, and `read` reaches only the core device.
        let bytes = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
        read(bytes, desc.bytes_per_row as usize)
    });
    if ok {
        // A SYSTEMMEM texture's level re-arms its dirty state, as mtld3d's read-back does.
        dst.note_system_memory_written();
    }
    ok
}

/// A colour `StretchRect` past mtld3d's gates, each pair the source's then the destination's.
pub struct StretchCall<'a> {
    /// The surfaces, as the call passed them.
    pub surfaces: [*mut c_void; 2],
    /// What `resolve_stretch_surface` made of them.
    pub infos: [&'a StretchSurfaceInfo; 2],
    /// The rects as parsed: not empty, and inside their surfaces.
    pub regions: [StretchRegion; 2],
    /// The rects as passed: null, or the caller's `RECT`.
    pub rects: [*const c_void; 2],
    /// The two regions differ in size.
    pub scaling: bool,
    /// Either format is a planar YUV one.
    pub planar_endpoint: bool,
    /// The device's `expand_packed16`, as `copies_bytes` reads it.
    pub expand_packed16: bool,
    /// The call's filter: `D3DTEXF_NONE`, `D3DTEXF_POINT` or `D3DTEXF_LINEAR`, as the gates pass.
    pub filter: u32,
}

/// `StretchRect` of colour on a core device: a resolve, a copy, or a sampled copy into a target.
///
/// The caller has run mtld3d's gates (the filter, the endpoints' classes and pools, the formats,
/// the rects, a stretch inside one surface, the planar and scaling rules), so a call mtld3d
/// rejects at its gates has answered `D3DERR_INVALIDCALL` before this, and what arrives is a
/// valid shape (DXMTRS C4c-4a). In order:
///
/// - the multisample resolve `resolve_shape` records (DXMTRS C4b-3d5);
/// - a destination that is a render target itself (the back buffer, an additional swap chain's,
///   a standalone target, a render-target texture's level or cube face) is written on the GPU,
///   from a target, a level of a texture that is no target or a plain surface. Identical rects
///   of one texture are `D3D_OK` with nothing recorded, a multisampled target's included, as
///   mtld3d's encoder skips them. Each end's texture has its CPU writes uploaded first, the
///   source's so that its core texture exists (a planar YUV surface's excepted). The source is
///   then settled to a single-sample one (DXMTRS C4c-4c): a planar YUV plain surface's level is
///   decoded whole on the CPU into a scratch texture of X8R8G8B8 (ruling Y-1); a multisampled
///   target other than the back buffer (whose presented texture the core samples) is resolved
///   whole into a scratch like it, and refused by name for a format this GPU does not resolve
///   (ruling R-1); a single-sample source in the destination's texture other than mtld3d's
///   `same_surface_route` `Direct` (overlapping rects, a scaled copy between two levels or
///   faces) is copied into a scratch first, as mtld3d's scratch route. An unscaled copy whose
///   bytes copy into a single-sample target is a blit (DXMTRS C4c-4b), two levels, two faces or
///   disjoint rects of one texture included. Any other, scaled, converting or into a
///   multisampled target, samples the source onto the destination rect as mtld3d's quad does:
///   nearest for NONE and POINT, bilinear for LINEAR; a level of a texture that is no target
///   through its swizzled view, every other source and every scratch through its stored format,
///   as mtld3d binds them; packed YUV and an X format's alpha decoded as mtld3d decodes them.
///   Every scratch is forgotten once the copy is recorded or refused. A level so written is
///   claimed for the GPU, so its next lock reads the copy back, and an AUTOGENMIPMAP destination
///   regenerates its chain, as after `ColorFill`;
/// - a destination that is no target is written on the CPU. One surface or one texture at both
///   ends is `D3D_OK` for identical rects. A plain surface onto itself with other rects copies
///   inside its own staging (DXMTRS C4c-4c): the level's bytes copied out first, so overlapping
///   rects read what was there, then the source rect written back at the destination point,
///   block by block; any other pair in one texture (an extended device's two faces of one cube)
///   is refused by name, the other routes needing two textures. A plain surface into another
///   copies the bytes when the two stagings hold the same ones (one D3D format, or bytes that
///   copy with neither format widened on upload), through mtld3d's CPU conversion otherwise, and
///   is refused by name when there is none. The rect written is marked dirty, so the plain
///   surface's next use as a source uploads it and its lock reads it; nothing is read back from
///   the GPU first, since a plain surface is never the GPU's on a core device (only a target is
///   ever claimed);
/// - any other destination (an extended device's whole-surface copy into a level that is no
///   target, or from a surface that is no plain one) is refused by name.
///
/// The destination itself is tested, not the pair's class: on a core device the GPU writes
/// nothing but targets, whose locks read it back, and an extended device's whole-surface copy
/// passes mtld3d's class gate into a level that is no target. A copy the core does not record
/// is logged and `D3DERR_INVALIDCALL`. `obj` is the device, for the conversion.
pub fn stretch_colour(this: *mut c_void, obj: &Direct3DDevice9, call: &StretchCall<'_>) -> i32 {
    if resolve_shape(this, call.surfaces, call.rects).is_some() {
        return D3D_OK;
    }
    let [src_info, dst_info] = call.infos;
    let [src_region, dst_region] = call.regions;
    let cross_format = call.planar_endpoint
        || !super::copies_bytes(src_info.format, dst_info.format, call.expand_packed16);
    let plain_dst = dst_info
        .flags
        .contains(StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT);
    let src_surf = call.surfaces[0].cast::<Direct3DSurface9>();
    let dst_surf = call.surfaces[1].cast::<Direct3DSurface9>();
    // SAFETY: the caller's IDirect3DSurface9*, checked non-null by the entry point and live for
    // the call per the ABI.
    let src = unsafe { &*src_surf };
    // SAFETY: as above.
    let dst = unsafe { &*dst_surf };
    let (src_parent, dst_parent) = (src.parent_texture(), dst.parent_texture());
    let route = same_surface_route(
        src_region,
        dst_region,
        src_info.mip_level,
        dst_info.mip_level,
        src_info.slice.unwrap_or(0),
        dst_info.slice.unwrap_or(0),
    );
    if dst_info
        .flags
        .contains(StretchSurfaceFlags::IS_RENDER_TARGET)
    {
        let src_target = src.core_target();
        let source = src_target.or_else(|| {
            // SAFETY: null, or the source level's parent texture, live while the level is.
            let parent = unsafe { src_parent.as_ref() }?;
            Some(TargetRef::Texture {
                id: parent.texture_id(),
                level: src.mip_level(),
                face: src.cube_face(),
            })
        });
        let destination = dst.core_target();
        let core = core_device(this).and_then(DeviceInner::core_mut);
        let (Some(source), Some(destination), Some(core)) = (source, destination, core) else {
            log::error!(target: crate::LOG_TARGET,
                "d3d9 core: StretchRect of {source:?} {src_region:?} into {destination:?} was not recorded → INVALIDCALL");
            return D3DERR_INVALIDCALL;
        };
        // On the call's own ends, ahead of any other test: identical rects of one texture write
        // what is there, a multisampled target's included, and nothing is recorded.
        let one_texture = match (source, destination) {
            (TargetRef::BackBuffer, TargetRef::BackBuffer) => true,
            (TargetRef::Texture { id, .. }, TargetRef::Texture { id: to, .. }) => id == to,
            _ => false,
        };
        if one_texture && route == SameSurfaceRoute::Skip {
            return D3D_OK;
        }
        let planar = mtld3d_core::stretch_rect::is_planar_yuv(src_info.format);
        // SAFETY: null, or the source level's parent texture, live while the level is and
        // distinct from the device.
        if !planar && let Some(texture) = unsafe { src_parent.as_mut() } {
            // Its core texture exists from here, with every CPU write so far uploaded. A planar
            // surface's would hold its luma alone: its staging is decoded below instead.
            let _ = crate::texture::core_flush_dirty(texture.inner_mut(), core);
        }
        let separate = dst_parent != src_parent;
        // SAFETY: null, or the destination level's parent texture, live while the level is and
        // distinct from the device and from the source's.
        if separate && let Some(texture) = unsafe { dst_parent.as_mut() } {
            // Its owed uploads are recorded ahead of the copy, in the same blit pass.
            let _ = crate::texture::core_flush_dirty(texture.inner_mut(), core);
        }
        // The view the source is sampled through, as mtld3d's quad binds it (ruling SW): a level
        // of a texture that is no target through its swizzled view, every other source through
        // its stored format, never an sRGB twin. Read once each end's core texture exists.
        let stored = match source {
            TargetRef::Texture { id, .. } => core.texture_format(id),
            // The core's back buffer is stored as its format maps on this device: BGRA8.
            TargetRef::BackBuffer => {
                crate::direct3d9::map_for_device(src_info.format, call.expand_packed16)
                    .map(|mapping| mapping.metal_pixel_format() as u32)
            }
        };
        let swizzle = if src_target.is_some() {
            None
        } else {
            // SAFETY: null, or the source level's parent texture, live while the level is.
            unsafe { src_parent.as_ref() }
                .and_then(|parent| parent.inner().swizzle())
                .map(|swizzle| swizzle.map(|channel| channel as u8))
        };
        let view = stored.map(|format| view_format(format, swizzle));
        let not_recorded = || {
            log::error!(target: crate::LOG_TARGET,
                "d3d9 core: StretchRect of {source:?} {src_region:?} into {destination:?} was not recorded → INVALIDCALL");
            D3DERR_INVALIDCALL
        };
        let answer = with_scratch(core, |core, scratch| {
            let rect = [src_region.x, src_region.y, src_region.w, src_region.h];
            let decode = blit_decode(src_info.format).uniform();
            // The source settled to a single-sample one, the view it is sampled through and its
            // decode.
            let (from, view, decode) = if planar {
                // Ruling Y-1: the plain surface's level, decoded whole on the CPU into X8R8G8B8
                // (alpha one), is uploaded into a scratch texture made as `core_flush_dirty`
                // makes an X8R8G8B8 texture of usage 0 (views in another format: its swizzle,
                // its twin), sampled through its stored format with no decode.
                let x8 = crate::direct3d9::map_for_device(D3DFMT_X8R8G8B8, call.expand_packed16)
                    .map(|mapping| mapping.metal_pixel_format());
                // SAFETY: null, or the source's parent texture, live while the source is and
                // distinct from the destination's.
                let decoded = unsafe { src_parent.as_ref() }.and_then(|parent| {
                    let image = parent
                        .inner()
                        .surface_source_image(None, src_info.mip_level as usize)?;
                    let row = image.width as usize * 4;
                    let len = row * image.height as usize;
                    let mut bgra = vec![0u8; len];
                    let region = ConvertRegion {
                        src_x: 0,
                        src_y: 0,
                        dst_x: 0,
                        dst_y: 0,
                        width: image.width,
                        height: image.height,
                        src_pitch: image.pitch,
                        dst_pitch: row,
                        src_slice_pitch: image.pitch * image.height as usize,
                        dst_slice_pitch: len,
                        depth: 1,
                    };
                    convert_region(
                        &mut bgra,
                        D3DFMT_X8R8G8B8,
                        image.bytes,
                        image.format,
                        &region,
                    )
                    .then_some((bgra, image.width, image.height))
                });
                let (Some(format), Some((bgra, width, height))) = (x8, decoded) else {
                    return not_recorded();
                };
                let id = TextureId::new_unique();
                core.texture_create(
                    id,
                    &CoreTextureDesc {
                        pixel_format: format as u32,
                        kind: TextureKind::Tex2d,
                        width,
                        height,
                        depth: 1,
                        levels: 1,
                        flags: CoreTextureFlags::VIEW_TWINS,
                        srgb_format: format.srgb_twin().map_or(0, |twin| twin as u32),
                        block: (1, 1, 4),
                        samples: 1,
                    },
                );
                let made = TargetRef::Texture {
                    id,
                    level: 0,
                    face: None,
                };
                *scratch = Some(made);
                let region = UploadRegion {
                    level: 0,
                    slice: 0,
                    x: 0,
                    y: 0,
                    z: 0,
                    width,
                    height,
                    depth: 1,
                };
                let image = u32::try_from(bgra.len()).unwrap_or(u32::MAX);
                if !core.texture_upload(id, region, &bgra, width.saturating_mul(4), image) {
                    return not_recorded();
                }
                (made, Some(format as u32), BlitDecode::None.uniform())
            } else if src_info.sample_count > 1 && source != TargetRef::BackBuffer {
                // A multisampled target other than the back buffer, whose presented texture the
                // core samples, is resolved whole into a scratch like it, then the source: its
                // stored format, the call's rect, its own decode (an X8R8G8B8 one's alpha one).
                // A format this GPU does not resolve is refused by name (ruling R-1).
                let Some(made) = core.scratch_like(source, src_info.width, src_info.height) else {
                    return not_recorded();
                };
                *scratch = Some(made);
                if !core.resolve_into(source, made) {
                    return refuse_stretch();
                }
                (made, view, decode)
            } else {
                (source, view, decode)
            };
            // A single-sample source still in the destination's texture, but for an unscaled
            // `Direct`, is copied out first, as mtld3d's scratch route: its rect into a scratch
            // like it, then the source at that scratch's origin, in its stored format (the
            // source's: it is a target) with its decode. `like` is a target: the source is a
            // level or face of the destination's render-target texture, the same standalone
            // target, or the back buffer an additional swap chain's aliases.
            let (from, rect) =
                if one_texture && src_info.sample_count == 1 && route != SameSurfaceRoute::Direct {
                    let [_, _, width, height] = rect;
                    let Some(staged) = core.scratch_like(from, width, height) else {
                        return not_recorded();
                    };
                    *scratch = Some(staged);
                    if !core.copy(from, rect, staged, [0, 0]) {
                        return not_recorded();
                    }
                    (staged, [0, 0, width, height])
                } else {
                    (from, rect)
                };
            // An unscaled copy whose bytes copy into a single-sample target is a blit (DXMTRS
            // C4c-4b); any other is sampled, into a multisampled target too.
            let recorded = if !call.scaling && !cross_format && dst_info.sample_count == 1 {
                core.copy(from, rect, destination, [dst_region.x, dst_region.y])
            } else {
                view.is_some_and(|src_view| {
                    core.stretch(&StretchQuad {
                        src: from,
                        src_rect: rect,
                        src_view,
                        dst: destination,
                        dst_rect: [dst_region.x, dst_region.y, dst_region.w, dst_region.h],
                        linear: call.filter == D3DTEXF_LINEAR,
                        decode,
                    })
                })
            };
            if recorded { D3D_OK } else { not_recorded() }
        });
        if answer != D3D_OK {
            return answer;
        }
        if !dst_parent.is_null() {
            super::claim_dst_subresource_for_gpu(dst_surf, dst_info);
        }
        if let Some(id) = dst_info.autogen_texture_id {
            core.texture_generate_mips(id);
        }
        return D3D_OK;
    }
    let plain_src = src_info
        .flags
        .contains(StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT);
    // The rects as the CPU copies take them: the source's edges, the destination's origin.
    let rect = (
        src_region.x.cast_signed(),
        src_region.y.cast_signed(),
        (src_region.x + src_region.w).cast_signed(),
        (src_region.y + src_region.h).cast_signed(),
    );
    let point = (dst_region.x.cast_signed(), dst_region.y.cast_signed());
    // One surface or one texture at both ends: identical rects write what is there.
    if core::ptr::eq(src_surf, dst_surf) || (!src_parent.is_null() && src_parent == dst_parent) {
        if route == SameSurfaceRoute::Skip {
            return D3D_OK;
        }
        // Only a plain surface onto itself copies inside its own staging (DXMTRS C4c-4c). Any
        // other pair, an extended device's two faces of one cube, is refused by name: the other
        // CPU routes need two textures.
        if !core::ptr::eq(src_surf, dst_surf) || !plain_src || !plain_dst {
            return refuse_stretch();
        }
        // SAFETY: null, or the plain surface's own texture, live while the surface is and
        // reached through nothing else in this call.
        let copied = unsafe { src_parent.as_mut() }.is_some_and(|texture| {
            let inner = texture.inner_mut();
            let level = src_info.mip_level as usize;
            // The level's bytes copied out first, so overlapping rects read what was there, as
            // through a buffer; the copy back keeps its own block and bounds checks.
            let held = inner
                .surface_source_image(None, level)
                .map(|image| (image.bytes.to_vec(), image.pitch));
            held.is_some_and(|(bytes, pitch)| {
                let image = SourceImage {
                    bytes: &bytes,
                    pitch,
                    width: src_info.width,
                    height: src_info.height,
                    format: src_info.format,
                };
                inner.copy_bytes_to_staging_region(level, &image, Some(rect), point)
            })
        });
        if !copied {
            log::error!(target: crate::LOG_TARGET,
                "d3d9 core: StretchRect of a plain surface's {src_region:?} onto its own {dst_region:?} was not recorded → INVALIDCALL");
            return D3DERR_INVALIDCALL;
        }
        return D3D_OK;
    }
    if !plain_src || !plain_dst || src_parent.is_null() || dst_parent.is_null() {
        return refuse_stretch();
    }
    let expanded = |format| {
        crate::direct3d9::map_for_device(format, call.expand_packed16)
            .is_some_and(|mapping| is_expanded_upload(format, mapping.metal_pixel_format()))
    };
    let copied = if src_info.format == dst_info.format
        || (!cross_format && !expanded(src_info.format) && !expanded(dst_info.format))
    {
        // SAFETY: non-null (checked), the source's parent texture, live while the source is and
        // distinct from the destination's (checked above).
        let src_tex = unsafe { &*src_parent };
        // SAFETY: non-null (checked), the destination's parent texture, live while the
        // destination is and distinct from the source's (checked above).
        let dst_tex = unsafe { &mut *dst_parent };
        dst_tex.inner_mut().copy_sub_region_from(
            dst_info.mip_level as usize,
            src_tex.inner(),
            src_info.mip_level as usize,
            Some(rect),
            point,
        )
    } else if can_convert(src_info.format, dst_info.format) {
        let converted = super::convert_stretch_dst_staging(
            obj, src_surf, dst_surf, src_info, dst_info, src_region, dst_region,
        );
        converted == D3D_OK
    } else {
        return refuse_stretch();
    };
    if !copied {
        log::error!(target: crate::LOG_TARGET,
            "d3d9 core: StretchRect of a plain surface's {src_region:?} into another's {dst_region:?} was not recorded → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    D3D_OK
}

/// `StretchRect` of a depth-stencil pair on a core device: the whole surface copied or resolved.
///
/// The caller has run mtld3d's depth-stencil gates (two depth-stencil surfaces of one format and
/// size in the default pool, sample counts equal or a resolve, whole rects), so a pair they
/// reject has answered `D3DERR_INVALIDCALL` before this, and none past them is refused by name.
/// The resolve, more samples into one, writes sample 0 of each texel of the source's depth, and
/// of its stencil for a format that has one, into the destination, as mtld3d's resolve keeps
/// them (DXMTRS C4c-4d). Of one sample count, one surface at both ends is `D3D_OK` with nothing
/// recorded, as mtld3d's encoder skips it, and any other pair is one copy of the whole level,
/// its depth and its stencil, two multisampled surfaces of one count included, as mtld3d
/// records it (DXMTRS C4c-4b). A copy or a resolve the core does not record, one Metal makes no
/// pass for included, is logged as an error and `D3DERR_INVALIDCALL`, where mtld3d answers a
/// resolve it skips `D3D_OK` with one warning. `surfaces` and `infos` are the source's and the
/// destination's.
pub fn stretch_depth(
    this: *mut c_void,
    surfaces: [*mut c_void; 2],
    infos: [&StretchSurfaceInfo; 2],
) -> i32 {
    // The one pair of unequal counts the gates let through: more samples into one.
    let resolve = infos[0].sample_count != infos[1].sample_count;
    // SAFETY: the caller's IDirect3DSurface9*, checked non-null by the entry point and live for
    // the call per the ABI.
    let src = unsafe { &*surfaces[0].cast::<Direct3DSurface9>() };
    // SAFETY: as above.
    let dst = unsafe { &*surfaces[1].cast::<Direct3DSurface9>() };
    let (source, destination) = (src.core_depth(), dst.core_depth());
    if source.is_some() && source == destination {
        return D3D_OK;
    }
    let core = core_device(this).and_then(DeviceInner::core_mut);
    let recorded = match (source, destination, core) {
        (Some(source), Some(destination), Some(core)) => {
            if resolve {
                core.depth_resolve(source, destination)
            } else {
                core.copy_depth(source, destination)
            }
        }
        _ => false,
    };
    if !recorded && resolve {
        log::error!(target: crate::LOG_TARGET,
            "d3d9 core: StretchRect resolving depth {source:?} into {destination:?} was not recorded → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    if !recorded {
        log::error!(target: crate::LOG_TARGET,
            "d3d9 core: StretchRect of depth {source:?} into {destination:?} was not recorded → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    D3D_OK
}

/// `SetRenderState(D3DRS_POINTSIZE, RESZ)`: the bound depth-stencil into the stage-0 depth texture.
///
/// Recorded at the call, after the draws recorded so far, as mtld3d's stream order has it (DXMTRS
/// C4c-7a): a copy of the whole level from a one-sample depth-stencil, a resolve of sample 0 of
/// each plane from a multisampled one, into level 0 of the depth texture bound at stage 0. The
/// destination is named by the texture, made with `D3DUSAGE_DEPTHSTENCIL` or not, once its core
/// form is made. A RESZ the core cannot do is skipped with one warning a reason and answers `Ok`,
/// as mtld3d's skips answer `D3D_OK`: no texture at stage 0, one of no depth format, no
/// depth-stencil bound, or a pair the core does not copy or resolve (another size, pixel format
/// or sample count, or the depth-stencil's own level). The caller stores the state after every
/// `Ok`.
///
/// # Errors
///
/// `D3DERR_NOTAVAILABLE`, refused by name before anything is made, for a `D3DUSAGE_DYNAMIC`
/// depth texture at stage 0 while a depth-stencil is bound: its CPU staging would upload over
/// what the RESZ wrote at its next bind. The caller returns it before the state is recorded or
/// stored.
#[cold]
#[inline(never)]
pub fn resz(dev: &mut DeviceInner) -> Result<(), i32> {
    let texture = dev.stage_bindings().texture(0);
    // SAFETY: null, or the texture bound at stage 0, live while its stage holds it; distinct from
    // the device.
    let Some(texture) = (unsafe { texture.as_mut() }) else {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "d3d9 core: RESZ skipped: no stage-0 texture bound");
        return Ok(());
    };
    if !texture.is_depth_format() {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "d3d9 core: RESZ skipped: the stage-0 texture is of no depth format");
        return Ok(());
    }
    // Always a core device: the caller's test.
    let Some(core) = dev.core_mut() else {
        return Ok(());
    };
    let Some(source) = core.depth() else {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "d3d9 core: RESZ skipped: no depth-stencil bound");
        return Ok(());
    };
    if texture.d3d_usage() & D3DUSAGE_DYNAMIC != 0 {
        refuse_under_core("IDirect3DDevice9::SetRenderState(RESZ) (into a dynamic depth texture)");
        return Err(D3DERR_NOTAVAILABLE);
    }
    // A depth texture never bound or sampled gets its core form here.
    let _ = crate::texture::core_flush_dirty(texture.inner_mut(), core);
    let destination = DepthRef {
        id: texture.texture_id(),
        level: 0,
    };
    let recorded = if core.depth_samples().is_some_and(|samples| samples > 1) {
        core.depth_resolve(source, destination)
    } else {
        core.copy_depth(source, destination)
    };
    if !recorded {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "d3d9 core: RESZ skipped: the core does not copy or resolve the pair ({source:?} into \
             {destination:?})");
    }
    Ok(())
}

/// `ColorFill` of a colour target the core draws to: the back buffer, a render target or a level.
///
/// The caller has run mtld3d's gates (the surface's class and pool, the planar formats, the
/// region clamped to the surface and not empty, the block grid), so `region` is a part of the
/// surface's level or the whole of it. A render-target texture's level or face has its CPU
/// writes uploaded first; the core then fills the whole view or the region with `color` as
/// given, as mtld3d's fill writes it (`D3DRS_SRGBWRITEENABLE` is not read). Only a recorded fill
/// claims the level for the GPU, so its next lock reads the fill back, and regenerates an
/// AUTOGENMIPMAP texture's chain (DXMTRS C4c-4a). A fill the core refuses is logged and
/// `D3DERR_INVALIDCALL`.
pub fn color_fill(
    dev: &mut DeviceInner,
    surf: *mut Direct3DSurface9,
    info: &StretchSurfaceInfo,
    region: DirtyRect,
    color: u32,
) -> i32 {
    // SAFETY: the caller's IDirect3DSurface9*, checked non-null by the entry point and live for
    // the call per the ABI.
    let surface = unsafe { &*surf };
    let (Some(target), Some(core)) = (surface.core_target(), dev.core_mut()) else {
        log::error!(target: crate::LOG_TARGET,
            "d3d9 core: ColorFill of a surface that is no colour target of the core → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    };
    let parent = surface.parent_texture();
    // SAFETY: null, or the level's parent texture, live while the level is and distinct from the
    // device.
    if let Some(texture) = unsafe { parent.as_mut() } {
        // Its core texture exists from here, with every CPU write so far uploaded.
        let _ = crate::texture::core_flush_dirty(texture.inner_mut(), core);
    }
    let rect = (region != DirtyRect::full(info.width, info.height))
        .then_some([region.x, region.y, region.w, region.h]);
    if !core.fill(target, rect, d3dcolor_to_rgba_f32(color)) {
        log::error!(target: crate::LOG_TARGET,
            "d3d9 core: ColorFill of {target:?} (rect {rect:?}) was not recorded → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    if !parent.is_null() {
        super::claim_dst_subresource_for_gpu(surf, info);
    }
    if let Some(id) = info.autogen_texture_id {
        core.texture_generate_mips(id);
    }
    D3D_OK
}

/// `UpdateSurface` of a region into the back buffer or a standalone render target.
///
/// The caller has run mtld3d's rejects (a multisampled destination, the source's pool and
/// backing, the formats and their conversion, the source rect and the destination point), so
/// `rows` hold the region in the destination's format, `row_bytes` apart from its first texel,
/// and `origin` and `extent`, in texels of the destination, lie inside it. The back buffer (an
/// additional swap chain's too, which aliases it, C4b3d1-4) takes them through `write_presented`,
/// a standalone render target through an upload of its level 0; both are recorded in application
/// order, after the passes before them and under the passes after. A lockable target's staging
/// is stale after it, as after a draw: its next lock reads the target back (DXMTRS C4c-4a). A
/// write the core refuses, or a pitch or image size past `u32`, is logged and
/// `D3DERR_INVALIDCALL`.
pub fn update_color_target(
    dev: &mut DeviceInner,
    surface: &Direct3DSurface9,
    origin: (u32, u32),
    extent: (u32, u32),
    rows: &[u8],
    row_bytes: usize,
) -> i32 {
    let target = surface.core_target();
    let written = match (target, dev.core_mut()) {
        (Some(TargetRef::BackBuffer), Some(core)) => {
            let rect = DirtyRect {
                x: origin.0,
                y: origin.1,
                w: extent.0,
                h: extent.1,
            };
            core.write_presented(rect, rows, row_bytes)
        }
        (Some(TargetRef::Texture { id, .. }), Some(core)) => {
            let region = UploadRegion {
                level: 0,
                slice: 0,
                x: origin.0,
                y: origin.1,
                z: 0,
                width: extent.0,
                height: extent.1,
                depth: 1,
            };
            u32::try_from(row_bytes).is_ok_and(|pitch| {
                pitch
                    .checked_mul(extent.1)
                    .is_some_and(|image| core.texture_upload(id, region, rows, pitch, image))
            })
        }
        _ => false,
    };
    if !written {
        log::error!(target: crate::LOG_TARGET,
            "d3d9 core: UpdateSurface into {target:?} at {origin:?} ({extent:?}) was not recorded → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    D3D_OK
}

/// `ProcessVertices`' source bytes from the first vertex on, when the core maps the buffer.
///
/// `Ok(None)` for a device of mtld3d's runtime and for a buffer with a CPU backing, which holds
/// its bytes. A buffer the core maps directly keeps no CPU copy, so its bytes from `first()` to
/// its end are read where the CPU last wrote them (DXMTRS C4c-7d-1), before mtld3d's test of a
/// released backing, which such a buffer would fail. A first vertex past the end answers
/// `D3DERR_INVALIDCALL`, as past a backing; so do bytes the core does not hold, logged once.
pub fn process_vertices_source(
    dev: &mut DeviceInner,
    src: &VertexBufferInner,
    first: impl FnOnce() -> usize,
) -> Result<Option<Vec<u8>>, i32> {
    let Some(core) = dev.core_mut() else {
        return Ok(None);
    };
    if !matches!(src.map_mode(), BufferMapMode::Direct) {
        return Ok(None);
    }
    let first = first();
    let Some(len) = (src.length() as usize).checked_sub(first) else {
        return Err(D3DERR_INVALIDCALL);
    };
    let bytes = core.dynamic_copy(src.buffer_id(), first, len);
    if bytes.is_none() {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "d3d9 core: ProcessVertices' dynamic source holds no bytes to read → INVALIDCALL");
    }
    bytes.map(Some).ok_or(D3DERR_INVALIDCALL)
}

/// The multisample resolve `stretch_colour` takes first, recorded; `None` for any other shape.
///
/// The one shape (DXMTRS C4b-3d5), all of it required: both surfaces are targets the core draws
/// to and neither is an additional swap chain's back buffer (as a target it aliases the
/// device's, C4b3d1-4); both are default-pool render targets of one size and of formats whose
/// bytes copy (mtld3d's `copies_bytes`); the source has more than one sample and the
/// destination one; each rect is null or the whole surface; a destination level's texture is
/// not AUTOGENMIPMAP, and its CPU writes are uploaded first; and the core device records the
/// resolve. The destination's owed uploads may be recorded before a refusal: they were due
/// anyway. `surfaces` and `rects` are the source's and the destination's.
fn resolve_shape(
    this: *mut c_void,
    surfaces: [*mut c_void; 2],
    rects: [*const c_void; 2],
) -> Option<()> {
    let dev = core_device(this)?;
    // SAFETY: (each) null or a caller-owned IDirect3DSurface9* per the ABI.
    let [Some(src), Some(dst)] = surfaces.map(|s| unsafe { InPtr::<Direct3DSurface9>::opt(s) })
    else {
        return None;
    };
    let (source, destination) = (src.core_target()?, dst.core_target()?);
    let aliased = |target: TargetRef, surface: *mut c_void| {
        target == TargetRef::BackBuffer && surface as u64 != dev.implicit_render_target
    };
    if aliased(source, surfaces[0]) || aliased(destination, surfaces[1]) {
        return None;
    }
    let (from, into) = (
        surface_desc(&src, surfaces[0])?,
        surface_desc(&dst, surfaces[1])?,
    );
    let whole = |desc: &D3DSURFACE_DESC, rect: *const c_void| {
        // SAFETY: null or the caller's RECT, read with no alignment assumed.
        unsafe { ValueIn::<D3DRECT>::read_opt(rect) }.is_none_or(|r| {
            r.x1 == 0
                && r.y1 == 0
                && u32::try_from(r.x2) == Ok(desc.width)
                && u32::try_from(r.y2) == Ok(desc.height)
        })
    };
    let targets = [&from, &into]
        .iter()
        .all(|desc| desc.usage & D3DUSAGE_RENDERTARGET != 0 && desc.pool == D3DPOOL_DEFAULT);
    let shaped = targets
        && (from.width, from.height) == (into.width, into.height)
        && super::copies_bytes(from.format, into.format, dev.config().expand_packed16)
        && src.multi_sample().sample_count > 1
        && dst.multi_sample().sample_count == 1
        && whole(&from, rects[0])
        && whole(&into, rects[1]);
    if !shaped {
        return None;
    }
    let core = dev.core.as_deref_mut()?;
    // SAFETY: null, or the destination level's parent texture, live while the level is and
    // distinct from the device.
    if let Some(parent) = unsafe { dst.parent_texture().as_mut() } {
        if parent.inner().autogen_mipmap() {
            return None;
        }
        // Its core texture exists from here, with every CPU write so far uploaded.
        let _ = crate::texture::core_flush_dirty(parent.inner_mut(), core);
    }
    core.resolve_into(source, destination).then_some(())
}

/// Runs `copy` with a slot for the one scratch texture it may make, then forgets that texture.
///
/// A scratch is private to the `StretchRect` that makes it and never an application surface: it
/// is forgotten whatever `copy` answers, a failure or a refusal included, and the copies recorded
/// from or into it hold their own (DXMTRS C4c-4c). A call makes one at most: a multisampled or
/// planar source settled through one is no longer the destination's texture, so only a
/// single-sample source needs the overlap's.
fn with_scratch(
    core: &mut CoreDevice,
    copy: impl FnOnce(&mut CoreDevice, &mut Option<TargetRef>) -> i32,
) -> i32 {
    let mut scratch = None;
    let answer = copy(core, &mut scratch);
    if let Some(TargetRef::Texture { id, .. }) = scratch {
        core.forget_texture(id);
    }
    answer
}

/// Refuses a valid `StretchRect` the core does not do yet, with one warning a process.
///
/// The one place that names the call: the warning is keyed on the name's address and the gates
/// count one line a launch, so the name has one copy, in a body never inlined (DXMTRS C4c-4b).
/// Every caller is `stretch_colour`'s: a multisampled target other than the back buffer, of a
/// format this GPU does not resolve, as the source of a copy into a target; an extended
/// device's whole-surface copy into a level that is no target (two faces of one cube included)
/// or from a surface that is no plain one; and a pair of plain surfaces whose formats no CPU
/// conversion serves.
#[cold]
#[inline(never)]
fn refuse_stretch() -> i32 {
    refuse_under_core("IDirect3DDevice9::StretchRect (other than a multisample resolve)");
    D3DERR_NOTAVAILABLE
}

/// `surface`'s `GetDesc` (at `ptr`, the same surface); `None` when it fails.
fn surface_desc(surface: &Direct3DSurface9, ptr: *mut c_void) -> Option<D3DSURFACE_DESC> {
    let mut desc = D3DSURFACE_DESC {
        format: 0,
        resource_type: 0,
        usage: 0,
        pool: 0,
        multi_sample_type: 0,
        multi_sample_quality: 0,
        width: 0,
        height: 0,
    };
    // SAFETY: the surface's own `GetDesc` thunk, with the surface as `this` and a writable
    // description.
    (unsafe { (surface.vtbl().get_desc)(ptr, &raw mut desc) } == 0).then_some(desc)
}
