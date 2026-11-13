//! The entry points the core runtime implements (DXMTRS C4a): thin arms over `d3d9_rt`.
//!
//! Each answers `None` for a device of mtld3d's runtime, whose entry point then runs as before,
//! and the call's result for a device of the core runtime. What an arm does not cover yet (a
//! rect, scissored or partial-viewport Clear, a Present with rects or another window) is refused
//! with one warning (C4-13). Clear and the read-back go to and come from the bound colour target
//! (DXMTRS C4b-3d1). The UP draws are `core_draw`'s.

use core::ffi::c_void;

use d3d9_rt::device::{DepthState, TargetRef};
use mtld3d_core::{
    convert::{d3dcolor_to_rgba_f32, linear_to_srgb_rgba},
    pipeline_key::viewport_covers,
    readback::{ReadbackDestination, ReadbackSource, reject_readback_dst},
};
use mtld3d_shared::InPtr;
use mtld3d_types::{
    D3D_OK, D3DCLEAR_STENCIL, D3DCLEAR_TARGET, D3DCLEAR_ZBUFFER, D3DERR_INVALIDCALL,
    D3DERR_NOTAVAILABLE, D3DRS_SCISSORTESTENABLE, D3DRS_SRGBWRITEENABLE, D3DSURFACE_DESC,
};

use super::{DeviceInner, Direct3DDevice9, refuse_under_core};
use crate::surface::Direct3DSurface9;

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

/// `Clear` of the whole colour target and of the whole depth-stencil; anything else is refused.
///
/// Z and stencil go to the bound depth-stencil (DXMTRS C4b-3d2): `D3DERR_INVALIDCALL` with none
/// bound, as mtld3d answers; refused for one of another size than the colour target, whose
/// whole-target Clear is a partial one of it (C4b-3d3). The stencil flag against a format with no
/// stencil plane clears nothing, as D3D9 has it (the core device masks the planes).
pub fn clear(
    this: *mut c_void,
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
    if !rects.is_null() || dev.render_state(D3DRS_SCISSORTESTENABLE as usize) != 0 {
        refuse_under_core("IDirect3DDevice9::Clear (rects or scissor)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    // A core device always has a target (its back buffer is made with it); were there none, the
    // Clear would have nothing to clear.
    let Some(target) = dev.core.as_deref()?.target_size() else {
        return Some(D3D_OK);
    };
    if !viewport_covers(&dev.viewport(), target) {
        refuse_under_core("IDirect3DDevice9::Clear (a viewport not covering the target)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    if depth_flags != 0 && matches!(dev.core.as_deref()?.depth_state(), DepthState::Detached(_)) {
        refuse_under_core(
            "IDirect3DDevice9::Clear (a depth-stencil of another size than the target)",
        );
        return Some(D3DERR_NOTAVAILABLE);
    }
    if flags & D3DCLEAR_TARGET != 0 {
        // No target has an sRGB view yet (C4b3d1-1): the colour is stored encoded, as a draw's
        // (C4b2-4).
        let linear = d3dcolor_to_rgba_f32(color);
        let rgba = if dev.render_state(D3DRS_SRGBWRITEENABLE as usize) == 0 {
            linear
        } else {
            linear_to_srgb_rgba(linear)
        };
        dev.core.as_deref_mut()?.clear(rgba);
    }
    if depth_flags != 0 {
        let planes = u32::from(flags & D3DCLEAR_ZBUFFER != 0)
            | u32::from(flags & D3DCLEAR_STENCIL != 0) << 1;
        dev.core
            .as_deref_mut()?
            .clear_depth_stencil(planes, z, stencil.to_le_bytes()[0]);
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
/// surface, a destination that is not system memory, a source that is no render target and a
/// destination of another size or format are `D3DERR_INVALIDCALL`. An additional swap chain's
/// back buffer is refused: as a target it aliases the device's (C4b3d1-4), which is not what it
/// would read.
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
    let Some(desc) = dst.system_memory_blit_dst() else {
        return Some(D3DERR_INVALIDCALL);
    };
    let Some(source) = src.core_target() else {
        return Some(D3DERR_INVALIDCALL);
    };
    if source == TargetRef::BackBuffer && rt as u64 != dev.implicit_render_target {
        refuse_under_core(
            "IDirect3DDevice9::GetRenderTargetData (an additional swap chain's back buffer)",
        );
        return Some(D3DERR_NOTAVAILABLE);
    }
    let mut from = D3DSURFACE_DESC {
        format: 0,
        resource_type: 0,
        usage: 0,
        pool: 0,
        multi_sample_type: 0,
        multi_sample_quality: 0,
        width: 0,
        height: 0,
    };
    // SAFETY: the source surface's own `GetDesc` thunk, with the surface as `this` and a
    // writable description.
    if unsafe { (src.vtbl().get_desc)(rt, &raw mut from) } != 0 {
        return Some(D3DERR_INVALIDCALL);
    }
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
    let len = usize::try_from(desc.len).ok()?;
    let ptr = core::ptr::with_exposed_provenance_mut::<u8>(usize::try_from(desc.ptr).ok()?);
    // SAFETY: the surface's own pages, `len` bytes at `ptr`, which the API lock keeps to this
    // call while it copies into them.
    let bytes = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    let ok = core.read_back(source, bytes, desc.bytes_per_row as usize);
    if ok {
        // A SYSTEMMEM texture's level re-arms its dirty state, as mtld3d's read-back does.
        dst.note_system_memory_written();
    }
    Some(if ok { D3D_OK } else { D3DERR_INVALIDCALL })
}
