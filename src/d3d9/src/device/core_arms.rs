//! The entry points the core runtime implements (DXMTRS C4a): thin arms over `d3d9_rt`.
//!
//! Each answers `None` for a device of mtld3d's runtime, whose entry point then runs as before,
//! and the call's result for a device of the core runtime. What an arm does not cover yet (a
//! rect or scissored Clear, a Present with rects or another window) is refused with one warning
//! (C4-13).

use core::ffi::c_void;

use d3d9_rt::device::{Bind, DrawData, Stage};
use mtld3d_core::{
    convert::{
        d3d_to_metal_cull, d3d_to_metal_fill, d3d_to_metal_primitive, ff_vs_layout_from_elements,
        fvf_to_elements, vertex_count,
    },
    dxso::MAX_LINKED_INPUTS,
    pipeline_key::{DeclCode, PipelineKey, PsStageKey, VsStageKey, core_viewport, up_byte_count},
};
use mtld3d_shared::{InPtr, mtl};
use mtld3d_types::{
    D3D_OK, D3DCLEAR_STENCIL, D3DCLEAR_TARGET, D3DCLEAR_ZBUFFER, D3DCULL_NONE, D3DDECL_END,
    D3DERR_INVALIDCALL, D3DERR_NOTAVAILABLE, D3DFILL_SOLID, D3DFMT_A8R8G8B8, D3DFMT_X8R8G8B8,
    D3DFVF_POSITION_MASK, D3DFVF_XYZRHW, D3DMATRIX, D3DRS_ALPHABLENDENABLE, D3DRS_COLORWRITEENABLE,
    D3DRS_SCISSORTESTENABLE,
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
    if flags & D3DCLEAR_TARGET != 0 {
        let rgba = mtld3d_core::convert::d3dcolor_to_rgba_f32(color);
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

/// `DrawPrimitiveUP` of a pre-transformed (`XYZRHW`) fixed-function draw.
///
/// The keys and constants are the ones mtld3d's runtime derives from the same state (`FfState`,
/// the bound FVF declaration's layout); `d3d9_rt` looks the pipeline up by its key, builds it on
/// a miss, and records the draw. A bound shader, a transformed position, blending or a pixel
/// cascade past one constant row is refused (C4-13). Culling is not applied yet (C4b).
pub fn draw_primitive_up(
    this: *mut c_void,
    primitive: u32,
    primitive_count: u32,
    data: *const c_void,
    stride: u32,
) -> Option<i32> {
    let dev = core_device(this)?;
    let fvf = dev.fvf();
    let bindings = dev.shader_bindings();
    if fvf & D3DFVF_POSITION_MASK != D3DFVF_XYZRHW
        || !bindings.vertex_shader().is_null()
        || !bindings.pixel_shader().is_null()
        || dev.render_state(D3DRS_ALPHABLENDENABLE as usize) != 0
    {
        refuse_under_core("IDirect3DDevice9::DrawPrimitiveUP (not a pre-transformed FF draw)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    let Some(metal_primitive) = d3d_to_metal_primitive(primitive) else {
        return Some(D3DERR_INVALIDCALL);
    };
    let count = vertex_count(primitive, primitive_count);
    let Some(byte_count) = up_byte_count(stride, count) else {
        return Some(D3DERR_INVALIDCALL);
    };
    if data.is_null() || byte_count == 0 {
        return Some(D3DERR_INVALIDCALL);
    }
    // A non-zero FVF is the bound declaration's: `SetFVF` built it from `fvf_to_elements(fvf)`.
    // SAFETY: non-null (the FVF is not 0); the bound slot's refcount keeps it live.
    let layout = ff_vs_layout_from_elements(unsafe { &*dev.vertex_decl() }.inner().elements());
    let rs = *dev.render_states();
    let vs_key = dev
        .ff_state()
        .build_vs_key(&rs, layout, 0, [0; MAX_LINKED_INPUTS]);
    let ps_key = dev.ff_state().build_ps_key(&rs, 0);
    if ps_key.constant_rows() > 1 {
        refuse_under_core("IDirect3DDevice9::DrawPrimitiveUP (texture-stage constants)");
        return Some(D3DERR_NOTAVAILABLE);
    }
    let variant = dev
        .ff_state()
        .variant_key(&rs, true, dev.uses_automatic_fog());
    let viewport = dev.viewport();
    let to_f32 = |n: u32| f32::from(u16::try_from(n).unwrap_or(u16::MAX));
    let mut row = [core::mem::MaybeUninit::uninit()];
    mtld3d_core::ff_state::FfState::fill_xyzrhw_row(
        (
            to_f32(viewport.x),
            to_f32(viewport.y),
            to_f32(viewport.width),
            to_f32(viewport.height),
        ),
        &mut row,
    );
    // SAFETY: `fill_xyzrhw_row` wrote the one row.
    let vs_constants = unsafe { row[0].assume_init() }
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect::<Vec<u8>>();
    let ps_constants = dev.ff_state().build_ps_constants(&rs);
    let (alpha_ref, alpha_len) =
        mtld3d_core::draw_data::build_alpha_ref_bytes(variant, dev.ff_state().alpha_ref(&rs));
    let (fog, fog_len) = mtld3d_core::ff_state::build_fog_color_bytes(&rs, variant);
    let view = dev
        .ff_state()
        .transform(mtld3d_types::D3DTS_VIEW)
        .copied()
        .unwrap_or(D3DMATRIX::IDENTITY);
    let point_size = dev.point_size();
    let vs_draw = dev
        .vs_draw
        .build_bytes(&rs, point_size, &view, &dev.clip_planes);
    // SAFETY: per the DrawPrimitiveUP ABI, `data` holds `count` vertices `stride` bytes apart,
    // read here; alignment is not assumed.
    let vertices = unsafe { mtld3d_shared::slice_from_caller(data.cast::<u8>(), byte_count) };
    // D3D9's colour write bits (R 1, G 2, B 4, A 8) as Metal's (R 8, G 4, B 2, A 1).
    let enabled = rs[D3DRS_COLORWRITEENABLE as usize];
    let write_mask = [(1, 8), (2, 4), (4, 2), (8, 1)]
        .iter()
        .filter(|(d3d, _)| enabled & d3d != 0)
        .fold(0_u8, |mask, (_, metal)| mask | metal);
    // ponytail: the declaration's code is built per draw until the front end keeps it on the
    // declaration (C4b-2 Task 4).
    let layout = DeclCode::new(
        fvf_to_elements(fvf)
            .0
            .into_iter()
            .chain([D3DDECL_END])
            .collect(),
    );
    let key = PipelineKey {
        vs: VsStageKey::Ff(vs_key),
        ps: PsStageKey::Ff(ps_key),
        variant,
        layout,
        stride,
        write_mask,
    };
    // No depth surface: an RHW draw's depth clamps (pos_fixup.z), as mtld3d's does.
    let pos_fixup = [
        1.0 / to_f32(viewport.width.max(1)),
        -1.0 / to_f32(viewport.height.max(1)),
        1.0,
        1.0,
        0.0,
    ]
    .iter()
    .flat_map(|v: &f32| v.to_le_bytes())
    .collect::<Vec<u8>>();
    let slot = |slot: u32| u8::try_from(slot).expect("a Metal buffer slot below 31");
    let vs = |index, bytes| Bind {
        stage: Stage::Vertex,
        slot: slot(index),
        bytes,
    };
    let ps = |index, bytes| Bind {
        stage: Stage::Pixel,
        slot: slot(index),
        bytes,
    };
    let binds = [
        vs(mtl::VS_FLOAT_CONST_SLOT, &vs_constants[..]),
        vs(mtl::VS_POS_FIXUP_SLOT, &pos_fixup),
        vs(mtl::VS_DRAW_SLOT, &vs_draw),
        ps(mtl::PS_FLOAT_CONST_SLOT, &ps_constants),
        // Empty when the alpha test or fog is off: not bound.
        ps(mtl::PS_ALPHA_REF_SLOT, &alpha_ref[..alpha_len]),
        ps(mtl::PS_FOG_SLOT, &fog[..fog_len]),
    ];
    let pp = dev.present_params();
    let draw = DrawData {
        primitive: metal_primitive as u8,
        vertices: &vertices,
        vertex_count: count,
        indices: None,
        viewport: core_viewport(&viewport, (pp.back_buffer_width, pp.back_buffer_height)),
        // C4a's arm culls nothing and fills (C4b-2 Task 4 takes the render states).
        fill_mode: d3d_to_metal_fill(D3DFILL_SOLID) as u8,
        cull_mode: d3d_to_metal_cull(D3DCULL_NONE) as u8,
        binds: &binds,
    };
    // A refused draw records nothing (a pipeline that does not build is logged); D3D9 reports
    // nothing for it, as under C4a.
    let _ = dev.core.as_deref_mut()?.draw_up(&key, &draw);
    Some(D3D_OK)
}
