//! The entry points the core runtime implements (DXMTRS C4a): thin arms over `d3d9_rt`.
//!
//! Each answers `None` for a device of mtld3d's runtime, whose entry point then runs as before,
//! and the call's result for a device of the core runtime. What an arm does not cover yet (a
//! rect, scissored or partial-viewport Clear, a Present with rects or another window) is refused
//! with one warning (C4-13). The UP draws are `core_draw`'s.

use core::ffi::c_void;

use mtld3d_core::{
    convert::{d3dcolor_to_rgba_f32, linear_to_srgb_rgba},
    pipeline_key::viewport_covers,
};
use mtld3d_shared::InPtr;
use mtld3d_types::{
    D3D_OK, D3DCLEAR_STENCIL, D3DCLEAR_TARGET, D3DCLEAR_ZBUFFER, D3DERR_INVALIDCALL,
    D3DERR_NOTAVAILABLE, D3DFMT_A8R8G8B8, D3DFMT_X8R8G8B8, D3DRS_SCISSORTESTENABLE,
    D3DRS_SRGBWRITEENABLE,
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

/// `Clear` of the whole back buffer's colour; anything else is refused.
pub fn clear(this: *mut c_void, rects: *const c_void, flags: u32, color: u32) -> Option<i32> {
    let dev = core_device(this)?;
    if flags & (D3DCLEAR_ZBUFFER | D3DCLEAR_STENCIL) != 0 {
        // No depth-stencil surface under the core runtime yet: D3D9's answer for none bound.
        return Some(D3DERR_INVALIDCALL);
    }
    if !rects.is_null() || dev.render_state(D3DRS_SCISSORTESTENABLE as usize) != 0 {
        refuse_under_core("IDirect3DDevice9::Clear (rects or scissor)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    let pp = dev.present_params();
    if !viewport_covers(
        &dev.viewport(),
        (pp.back_buffer_width, pp.back_buffer_height),
    ) {
        refuse_under_core("IDirect3DDevice9::Clear (a viewport not covering the back buffer)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    if flags & D3DCLEAR_TARGET != 0 {
        // The back buffer has no sRGB view: the colour is stored encoded, as a draw's (C4b2-4).
        let linear = d3dcolor_to_rgba_f32(color);
        let rgba = if dev.render_state(D3DRS_SRGBWRITEENABLE as usize) == 0 {
            linear
        } else {
            linear_to_srgb_rgba(linear)
        };
        dev.core.as_deref_mut()?.clear(rgba);
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

/// `GetRenderTargetData` from the implicit render target.
///
/// Into a system-memory surface of the back buffer's size and format.
pub fn get_render_target_data(this: *mut c_void, rt: *mut c_void, dst: *mut c_void) -> Option<i32> {
    let dev = core_device(this)?;
    if rt.is_null() || rt as u64 != dev.implicit_render_target {
        refuse_under_core("IDirect3DDevice9::GetRenderTargetData (not the back buffer)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    // SAFETY: `dst` is null or a caller-owned IDirect3DSurface9* per the ABI.
    let Some(dst) = (unsafe { InPtr::<Direct3DSurface9>::opt(dst) }) else {
        return Some(D3DERR_INVALIDCALL);
    };
    let Some(desc) = dst.system_memory_blit_dst() else {
        return Some(D3DERR_INVALIDCALL);
    };
    let pp = dev.present_params();
    if (desc.width, desc.height) != (pp.back_buffer_width, pp.back_buffer_height)
        || !matches!(desc.format, D3DFMT_X8R8G8B8 | D3DFMT_A8R8G8B8)
    {
        return Some(D3DERR_INVALIDCALL);
    }
    let len = usize::try_from(desc.len).ok()?;
    let ptr = core::ptr::with_exposed_provenance_mut::<u8>(usize::try_from(desc.ptr).ok()?);
    // SAFETY: the surface's own pages, `len` bytes at `ptr`, which the API lock keeps to this
    // call while it copies into them.
    let bytes = unsafe { core::slice::from_raw_parts_mut(ptr, len) };
    let ok = dev
        .core
        .as_deref_mut()?
        .read_back(bytes, desc.bytes_per_row as usize);
    Some(if ok { D3D_OK } else { D3DERR_INVALIDCALL })
}
