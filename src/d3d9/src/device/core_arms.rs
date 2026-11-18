//! The entry points the core runtime implements (DXMTRS C4a): thin arms over `d3d9_rt`.
//!
//! Each answers `None` for a device of mtld3d's runtime, whose entry point then runs as before,
//! and the call's result for a device of the core runtime. `StretchRect`, `ColorFill` and
//! `UpdateSurface` run mtld3d's gates on both runtimes, so a call they reject answers
//! `D3DERR_INVALIDCALL` on both, and a core device leaves past them for `stretch_colour` (a
//! colour pair) or `stretch_rect_resolve` (a depth-stencil pair), `color_fill` for a colour
//! target, or `update_color_target` for a region of the back buffer or a render target (DXMTRS
//! C4c-4a). `stretch_colour` resolves a multisampled target into a single-sample one, copies
//! texel for texel into a target on the GPU, and between two plain surfaces on the CPU (DXMTRS
//! C4c-4b): it tests the destination itself, since on a core device the GPU writes nothing but
//! targets. What an arm does not cover yet (a Present with rects or another window; a scaled,
//! multisampled or depth-stencil `StretchRect`, one that converts into a target) is refused with
//! one warning (C4-13). Clear and the read-back go to and come from the bound colour target
//! (DXMTRS C4b-3d1). The UP draws are `core_draw`'s.

use core::ffi::c_void;

use d3d9_rt::device::{TargetRef, UploadRegion};
use mtld3d_core::{
    clear_area::{ClearArea, clear_area, clear_regions},
    convert::d3dcolor_to_rgba_f32,
    dirty_rect::DirtyRect,
    pipeline_key::drawn_rect,
    pixel_convert::can_convert,
    readback::{ReadbackDestination, ReadbackSource, reject_readback_dst},
    render_state::scissor_region,
    stretch_rect::{SameSurfaceRoute, StretchRegion, same_surface_route},
    upload_pass::is_expanded_upload,
};
use mtld3d_shared::{InPtr, ValueIn};
use mtld3d_types::{
    D3D_OK, D3DCLEAR_STENCIL, D3DCLEAR_TARGET, D3DCLEAR_ZBUFFER, D3DERR_INVALIDCALL,
    D3DERR_NOTAVAILABLE, D3DPOOL_DEFAULT, D3DRECT, D3DRS_SCISSORTESTENABLE, D3DRS_SRGBWRITEENABLE,
    D3DSURFACE_DESC, D3DUSAGE_RENDERTARGET,
};

use super::{
    DeviceInner, Direct3DDevice9, StretchSurfaceFlags, StretchSurfaceInfo, refuse_under_core,
};
use crate::{
    bound_rt::RENDER_TARGET_SLOTS,
    surface::{Direct3DSurface9, SystemMemoryDst},
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

/// `Present` of the whole back buffer into the device window, paced by its interval.
pub fn present(
    this: *mut c_void,
    src_rect: *const c_void,
    dst_rect: *const c_void,
    dst_window_override: *mut c_void,
    dirty_region: *const c_void,
) -> Option<i32> {
    let dev = core_device(this)?;
    if !src_rect.is_null()
        || !dst_rect.is_null()
        || !dst_window_override.is_null()
        || !dirty_region.is_null()
    {
        refuse_under_core("IDirect3DDevice9::Present (rects, region or another window)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    let pp = dev.present_params();
    let window = pp.device_window as *mut c_void;
    let interval = pp.presentation_interval;
    // A hidden or minimized window presents nothing (G4): its drawable would never come back.
    // SAFETY: user32's query; any window handle, even a stale one, is a valid argument.
    let visible = unsafe { IsWindowVisible(window) } != 0 && !dxmt_util::wsi::is_minimized(window);
    let refresh = crate::direct3d9::current_adapter_display_mode().refresh_rate;
    let after = d3d9_rt::device::present_after(interval, refresh);
    dev.core.as_deref_mut()?.present(visible, after);
    Some(D3D_OK)
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
}

/// `StretchRect` of colour on a core device: a resolve, or a copy into a target texel for texel.
///
/// The caller has run mtld3d's gates (the filter, the endpoints' classes and pools, the formats,
/// the rects, a stretch inside one surface, the planar and scaling rules), so a call mtld3d
/// rejects at its gates has answered `D3DERR_INVALIDCALL` before this, and what arrives is a
/// valid shape (DXMTRS C4c-4a). In order:
///
/// - the multisample resolve `resolve_shape` records (DXMTRS C4b-3d5);
/// - refused by name: a scaled copy, a multisampled end, and a copy that converts into anything
///   but a plain surface (mtld3d's `cross_format`: a planar end, or bytes that do not copy);
/// - a destination that is a render target itself (the back buffer, an additional swap chain's,
///   a standalone target, a render-target texture's level or cube face) is written on the GPU
///   (DXMTRS C4c-4b), from a target, a level of a texture that is no target or a plain surface.
///   Two ends in one texture copy when mtld3d's `same_surface_route` says `Direct` (two levels,
///   two faces, disjoint rects); identical rects are `D3D_OK` with nothing recorded, and
///   overlapping ones are refused by name. Each end's texture has its CPU writes uploaded first,
///   the source's so that its core texture exists. A level so written is claimed for the GPU,
///   so its next lock reads the copy back, and an AUTOGENMIPMAP destination regenerates its
///   chain, as after `ColorFill`;
/// - a destination that is no target is written on the CPU. One surface or one texture at both
///   ends is `D3D_OK` for identical rects and refused by name otherwise (both routes need two
///   textures). A plain surface into another copies the bytes when the two stagings hold the
///   same ones (one D3D format, or bytes that copy with neither format widened on upload),
///   through mtld3d's CPU conversion otherwise, and is refused by name when there is none. The
///   rect written is marked dirty, so the plain surface's next use as a source uploads it and
///   its lock reads it; nothing is read back from the GPU first, since a plain surface is never
///   the GPU's on a core device (only a target is ever claimed);
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
    if call.scaling
        || src_info.sample_count > 1
        || dst_info.sample_count > 1
        || (cross_format && !plain_dst)
    {
        return refuse_stretch();
    }
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
        let source = src.core_target().or_else(|| {
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
        let one_texture = match (source, destination) {
            (TargetRef::BackBuffer, TargetRef::BackBuffer) => true,
            (TargetRef::Texture { id, .. }, TargetRef::Texture { id: to, .. }) => id == to,
            _ => false,
        };
        if one_texture {
            match route {
                SameSurfaceRoute::Skip => return D3D_OK,
                SameSurfaceRoute::Scratch => return refuse_stretch(),
                SameSurfaceRoute::Direct => {}
            }
        }
        // SAFETY: null, or the source level's parent texture, live while the level is and
        // distinct from the device.
        if let Some(texture) = unsafe { src_parent.as_mut() } {
            // Its core texture exists from here, with every CPU write so far uploaded.
            let _ = crate::texture::core_flush_dirty(texture.inner_mut(), core);
        }
        let separate = dst_parent != src_parent;
        // SAFETY: null, or the destination level's parent texture, live while the level is and
        // distinct from the device and from the source's.
        if separate && let Some(texture) = unsafe { dst_parent.as_mut() } {
            // Its owed uploads are recorded ahead of the copy, in the same blit pass.
            let _ = crate::texture::core_flush_dirty(texture.inner_mut(), core);
        }
        let rect = [src_region.x, src_region.y, src_region.w, src_region.h];
        if !core.copy(source, rect, destination, [dst_region.x, dst_region.y]) {
            log::error!(target: crate::LOG_TARGET,
                "d3d9 core: StretchRect of {source:?} {src_region:?} into {destination:?} was not recorded → INVALIDCALL");
            return D3DERR_INVALIDCALL;
        }
        if !dst_parent.is_null() {
            super::claim_dst_subresource_for_gpu(dst_surf, dst_info);
        }
        if let Some(id) = dst_info.autogen_texture_id {
            core.texture_generate_mips(id);
        }
        return D3D_OK;
    }
    // One surface or one texture at both ends: both CPU routes need two textures.
    if core::ptr::eq(src_surf, dst_surf) || (!src_parent.is_null() && src_parent == dst_parent) {
        return if route == SameSurfaceRoute::Skip {
            D3D_OK
        } else {
            refuse_stretch()
        };
    }
    let plain_src = src_info
        .flags
        .contains(StretchSurfaceFlags::IS_OFFSCREEN_PLAIN_DEFAULT);
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
        let rect = (
            src_region.x.cast_signed(),
            src_region.y.cast_signed(),
            (src_region.x + src_region.w).cast_signed(),
            (src_region.y + src_region.h).cast_signed(),
        );
        dst_tex.inner_mut().copy_sub_region_from(
            dst_info.mip_level as usize,
            src_tex.inner(),
            src_info.mip_level as usize,
            Some(rect),
            (dst_region.x.cast_signed(), dst_region.y.cast_signed()),
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

/// `StretchRect` on a core device: a multisampled render target resolved into a single-sample one.
///
/// The caller has run mtld3d's gates (the filter, the endpoints' classes and pools, the
/// depth-stencil pair, the formats, the rects, a stretch inside one surface, the planar and
/// scaling rules), so a call mtld3d rejects at its gates has answered `D3DERR_INVALIDCALL`
/// before this, and what arrives is a valid shape (DXMTRS C4c-4a). The one shape the core
/// implements (DXMTRS C4b-3d5), all of it required: both surfaces are targets the core draws to
/// and neither is an additional swap chain's back buffer (as a target it aliases the device's,
/// C4b3d1-4); both are default-pool render targets of one size and of formats whose bytes copy
/// (mtld3d's `copies_bytes`); the source has more than one sample and the destination one; each
/// rect is null or the whole surface; a destination level's texture is not AUTOGENMIPMAP, and
/// its CPU writes are uploaded first; and the core device records the resolve. Any other is the
/// logged refusal and `D3DERR_NOTAVAILABLE`. `surfaces` and `rects` are the source's and the
/// destination's.
pub fn stretch_rect_resolve(
    this: *mut c_void,
    surfaces: [*mut c_void; 2],
    rects: [*const c_void; 2],
) -> i32 {
    if resolve_shape(this, surfaces, rects).is_some() {
        D3D_OK
    } else {
        refuse_stretch()
    }
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

/// [`stretch_rect_resolve`]'s shape, recorded; `None`, and no resolve recorded, for any other.
///
/// The destination's owed uploads may be recorded before a refusal: they were due anyway.
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

/// Refuses a valid `StretchRect` the core does not do yet, with one warning a process.
///
/// The one place that names the call: the warning is keyed on the name's address and the gates
/// count one line a launch, so the name has one copy, in a body never inlined (DXMTRS C4c-4b).
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
