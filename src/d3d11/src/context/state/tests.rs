use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use crate::{
    abi::{
        IID_ID3D11_DEVICE_CONTEXT, IID_IUNKNOWN,
        ext::{IID_IMTL_MIN_LOD_CLAMPABLE, IMTLMinLODClampableVtbl},
        records::{D3d11Viewport, Rect},
        vtbl::IUnknownVtbl,
    },
    com::{E_NOINTERFACE, E_POINTER, Guid, Hresult, S_OK, release},
    context::{
        Context, Immediate,
        context_state::create_device_context_state,
        state::{DirtyFlags, KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL},
    },
    state::Defaults,
};

const GOLDEN: &str = include_str!("golden/d11ctx.txt");

fn golden_line(prefix: &str) -> &'static str {
    GOLDEN
        .lines()
        .find(|line| line.starts_with(prefix))
        .expect("golden line present in d11ctx.txt")
}

fn new_test_context() -> Context<Immediate> {
    Context {
        vtbl: ptr::null(),
        ext_vtbl: ptr::null(),
        annotation_vtbl: ptr::null(),
        refs: crate::com::Managed::default(),
        private_data: dxmt_util::private_data::PrivateData::default(),
        device: ptr::null_mut(),
        live: false,
        state: super::ContextState::default(),
        cmdbuf_state: crate::context::CommandBufferState::Idle,
        encode: crate::context::encode::EncodeSession::default(),
        dirty: DirtyFlags::empty(),
        defaults: Defaults::default(),
        mode: Immediate::default(),
    }
}

#[repr(C)]
struct MockCom {
    vtbl: *const IUnknownVtbl,
    clamp_vtbl: *const IMTLMinLODClampableVtbl,
    refs: AtomicU32,
    clampable: bool,
    min_lod: f32,
}

unsafe extern "system-unwind" fn mock_qi(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if out.is_null() {
        return E_POINTER;
    }
    // SAFETY: `out` was checked non-null.
    unsafe { *out = ptr::null_mut() };
    // SAFETY: `riid` points to a valid `Guid`.
    let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
        return E_NOINTERFACE;
    };
    // SAFETY: `this` points to a `MockCom`.
    let obj = unsafe { &mut *this.cast::<MockCom>() };
    if iid == IID_IUNKNOWN {
        obj.refs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `out` is non-null.
        unsafe { *out = this };
        return S_OK;
    }
    if iid == IID_IMTL_MIN_LOD_CLAMPABLE && obj.clampable {
        obj.refs.fetch_add(1, Ordering::Relaxed);
        let clamp_ptr = (&raw mut obj.clamp_vtbl).cast::<c_void>();
        // SAFETY: `out` is non-null.
        unsafe { *out = clamp_ptr };
        return S_OK;
    }
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn mock_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` points to a `MockCom`.
    let obj = unsafe { &*this.cast::<MockCom>() };
    obj.refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system-unwind" fn mock_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` points to a `MockCom`.
    let obj = unsafe { &*this.cast::<MockCom>() };
    obj.refs.fetch_sub(1, Ordering::Relaxed) - 1
}

unsafe fn clamp_owner(this: *mut c_void) -> *mut MockCom {
    let field_offset = core::mem::offset_of!(MockCom, clamp_vtbl);
    // SAFETY: `this` points to the `clamp_vtbl` field inside a `MockCom`.
    unsafe { this.byte_sub(field_offset).cast::<MockCom>() }
}

unsafe extern "system-unwind" fn clamp_set_min_lod(this: *mut c_void, min_lod: f32) {
    // SAFETY: `this` is a `clamp_vtbl` tear-off inside `MockCom`.
    let ptr = unsafe { clamp_owner(this) };
    // SAFETY: `ptr` points to the owning `MockCom`.
    let owner = unsafe { &mut *ptr };
    owner.min_lod = min_lod;
}

unsafe extern "system-unwind" fn clamp_get_min_lod(this: *mut c_void) -> f32 {
    // SAFETY: `this` is a `clamp_vtbl` tear-off inside `MockCom`.
    let ptr = unsafe { clamp_owner(this) };
    // SAFETY: `ptr` points to the owning `MockCom`.
    let owner = unsafe { &*ptr };
    owner.min_lod
}

static MOCK_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: mock_qi,
    add_ref: mock_add_ref,
    release: mock_release,
};

static CLAMP_VTBL: IMTLMinLODClampableVtbl = IMTLMinLODClampableVtbl {
    unknown: IUnknownVtbl {
        query_interface: mock_qi,
        add_ref: mock_add_ref,
        release: mock_release,
    },
    set_min_lod: clamp_set_min_lod,
    get_min_lod: clamp_get_min_lod,
};

impl MockCom {
    fn new(clampable: bool) -> Self {
        Self {
            vtbl: &raw const MOCK_VTBL,
            clamp_vtbl: &raw const CLAMP_VTBL,
            refs: AtomicU32::new(1),
            clampable,
            min_lod: 0.0,
        }
    }

    fn as_ptr(&mut self) -> *mut c_void {
        ptr::from_mut(self).cast::<c_void>()
    }

    fn ref_count(&self) -> u32 {
        self.refs.load(Ordering::Relaxed)
    }
}

#[test]
fn clear_state_defaults_as_cpp() {
    let mut ctx = new_test_context();
    let mut bs = MockCom::new(false);
    let mut ds = MockCom::new(false);
    let bf_set = [0.25_f32, 0.5_f32, 0.75_f32, 1.0_f32];
    let vp = D3d11Viewport {
        top_left_x: 0.0,
        top_left_y: 0.0,
        width: 16.0,
        height: 16.0,
        min_depth: 0.0,
        max_depth: 1.0,
    };

    ctx.ia_set_primitive_topology(4);
    ctx.om_set_blend_state(bs.as_ptr(), bf_set.as_ptr(), 0x1234_5678);
    ctx.om_set_depth_stencil_state(ds.as_ptr(), 99);
    ctx.rs_set_viewports(1, (&raw const vp).cast());

    ctx.clear_state();

    let mut clr_topo = 4_u32;
    ctx.ia_get_primitive_topology(&raw mut clr_topo);
    let mut out_blend_state: *mut c_void = ptr::dangling_mut();
    let mut out_blend_factor = [9.0_f32; 4];
    let mut out_sample_mask = 0_u32;
    ctx.om_get_blend_state(
        &raw mut out_blend_state,
        out_blend_factor.as_mut_ptr(),
        &raw mut out_sample_mask,
    );
    let mut out_depth_stencil: *mut c_void = ptr::dangling_mut();
    let mut out_stencil_ref = 99_u32;
    ctx.om_get_depth_stencil_state(&raw mut out_depth_stencil, &raw mut out_stencil_ref);
    let mut out_viewport_count = 99_u32;
    ctx.rs_get_viewports(&raw mut out_viewport_count, ptr::null_mut());

    let line = format!(
        "d11ctx clear_state topo={} bs={} bf={:.0},{:.0},{:.0},{:.0} sm={:08x} ds={} sref={} nvp={}",
        clr_topo,
        usize::from(!out_blend_state.is_null()),
        out_blend_factor[0],
        out_blend_factor[1],
        out_blend_factor[2],
        out_blend_factor[3],
        out_sample_mask,
        usize::from(!out_depth_stencil.is_null()),
        out_stencil_ref,
        out_viewport_count,
    );
    assert_eq!(line, golden_line("d11ctx clear_state "));
}

#[test]
fn getters_after_setters_as_cpp() {
    let mut ctx = new_test_context();
    let mut vb = MockCom::new(false);
    let mut ib = MockCom::new(false);
    let mut cb = MockCom::new(false);
    let mut tex = MockCom::new(true);
    let mut srv = MockCom::new(false);
    let mut rtv = MockCom::new(false);
    let mut samp = MockCom::new(false);

    let stride = 16_u32;
    let offset = 4_u32;
    let vbs = [vb.as_ptr()];
    ctx.ia_set_vertex_buffers(0, 1, vbs.as_ptr(), &raw const stride, &raw const offset);
    let mut out_vertex_buf: *mut c_void = ptr::null_mut();
    let mut got_stride = 0_u32;
    let mut got_offset = 0_u32;
    ctx.ia_get_vertex_buffers(
        0,
        1,
        &raw mut out_vertex_buf,
        &raw mut got_stride,
        &raw mut got_offset,
    );

    ctx.ia_set_index_buffer(ib.as_ptr(), 57, 8);
    let mut out_index_buf: *mut c_void = ptr::null_mut();
    let mut got_ib_fmt = 0_u32;
    let mut got_ib_off = 0_u32;
    ctx.ia_get_index_buffer(
        &raw mut out_index_buf,
        &raw mut got_ib_fmt,
        &raw mut got_ib_off,
    );

    ctx.ia_set_primitive_topology(4);
    let mut got_topo = 0_u32;
    ctx.ia_get_primitive_topology(&raw mut got_topo);

    let cbs = [cb.as_ptr()];
    let first_constant = 4_u32;
    let num_constants = 8_u32;
    ctx.vs_set_constant_buffers1(
        0,
        1,
        cbs.as_ptr(),
        &raw const first_constant,
        &raw const num_constants,
    );
    let mut out_const_buf: *mut c_void = ptr::null_mut();
    let mut out_first_const = 0_u32;
    let mut out_num_const = 0_u32;
    ctx.vs_get_constant_buffers1(
        0,
        1,
        &raw mut out_const_buf,
        &raw mut out_first_const,
        &raw mut out_num_const,
    );

    let srvs = [srv.as_ptr()];
    ctx.ps_set_shader_resources(0, 1, srvs.as_ptr());
    let mut got_srv: *mut c_void = ptr::null_mut();
    ctx.ps_get_shader_resources(0, 1, &raw mut got_srv);

    let samps = [samp.as_ptr()];
    ctx.ps_set_samplers(0, 1, samps.as_ptr());
    let mut got_samp: *mut c_void = ptr::null_mut();
    ctx.ps_get_samplers(0, 1, &raw mut got_samp);

    let rtvs = [rtv.as_ptr()];
    ctx.om_set_render_targets(1, rtvs.as_ptr(), ptr::null_mut());
    let mut got_rtv: *mut c_void = ptr::null_mut();
    ctx.om_get_render_targets(1, &raw mut got_rtv, ptr::null_mut());

    let vp = D3d11Viewport {
        top_left_x: 1.0,
        top_left_y: 2.0,
        width: 16.0,
        height: 16.0,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    ctx.rs_set_viewports(1, (&raw const vp).cast());
    let mut viewport_count = 1_u32;
    let mut out_viewport = D3d11Viewport::default();
    ctx.rs_get_viewports(&raw mut viewport_count, (&raw mut out_viewport).cast());

    let sc = Rect {
        left: 1,
        top: 2,
        right: 15,
        bottom: 16,
    };
    ctx.rs_set_scissor_rects(1, (&raw const sc).cast());
    let mut scissor_count = 1_u32;
    let mut out_scissor = Rect::default();
    ctx.rs_get_scissor_rects(&raw mut scissor_count, (&raw mut out_scissor).cast());

    let so_off = 0_u32;
    ctx.so_set_targets(1, vbs.as_ptr(), &raw const so_off);
    let mut got_so: *mut c_void = ptr::null_mut();
    ctx.so_get_targets(1, &raw mut got_so);

    ctx.set_resource_min_lod(tex.as_ptr(), 1.0);
    let lod_tex = ctx.get_resource_min_lod(tex.as_ptr());
    ctx.set_resource_min_lod(vb.as_ptr(), 1.0);
    let lod_buf = ctx.get_resource_min_lod(vb.as_ptr());

    let line = format!(
        "d11ctx bind_roundtrip vb={}/{}/{} ib={}/{}/{} topo={} cb1={}/{}/{} srv={} samp={} rtv={} vp={:.0},{:.0} sc={},{} so={} lod={:.1},{:.1}",
        usize::from(out_vertex_buf == vb.as_ptr()),
        got_stride,
        got_offset,
        usize::from(out_index_buf == ib.as_ptr()),
        got_ib_fmt,
        got_ib_off,
        got_topo,
        usize::from(out_const_buf == cb.as_ptr()),
        out_first_const,
        out_num_const,
        usize::from(got_srv == srv.as_ptr()),
        usize::from(got_samp == samp.as_ptr()),
        usize::from(got_rtv == rtv.as_ptr()),
        out_viewport.width,
        out_viewport.height,
        out_scissor.right,
        out_scissor.bottom,
        usize::from(got_so == vb.as_ptr()),
        lod_tex,
        lod_buf,
    );
    assert_eq!(line, golden_line("d11ctx bind_roundtrip "));

    for ptr in [
        out_vertex_buf,
        out_index_buf,
        out_const_buf,
        got_srv,
        got_samp,
        got_rtv,
        got_so,
    ] {
        // SAFETY: `ptr` is a `MockCom` pointer returned with +1 refcount.
        unsafe { release(ptr) };
    }
    ctx.clear_state();
}

#[test]
fn redundant_bind_sets_no_dirty() {
    let mut ctx = new_test_context();
    let mut cb = MockCom::new(false);
    let mut srv = MockCom::new(false);
    let mut samp = MockCom::new(false);
    let mut vb = MockCom::new(false);
    let mut rs = MockCom::new(false);

    let cbs = [cb.as_ptr()];
    let fc = 4_u32;
    let nc = 8_u32;
    ctx.vs_set_constant_buffers1(0, 1, cbs.as_ptr(), &raw const fc, &raw const nc);

    let srvs = [srv.as_ptr()];
    ctx.ps_set_shader_resources(0, 1, srvs.as_ptr());

    let samps = [samp.as_ptr()];
    ctx.ps_set_samplers(0, 1, samps.as_ptr());

    let vbs = [vb.as_ptr()];
    let stride = 16_u32;
    let offset = 0_u32;
    ctx.ia_set_vertex_buffers(0, 1, vbs.as_ptr(), &raw const stride, &raw const offset);

    ctx.rs_set_state(rs.as_ptr());
    let vp = D3d11Viewport {
        top_left_x: 0.0,
        top_left_y: 0.0,
        width: 100.0,
        height: 100.0,
        min_depth: 0.0,
        max_depth: 1.0,
    };
    ctx.rs_set_viewports(1, (&raw const vp).cast());
    let sc = Rect {
        left: 0,
        top: 0,
        right: 100,
        bottom: 100,
    };
    ctx.rs_set_scissor_rects(1, (&raw const sc).cast());

    ctx.state.shader_stages[0].constant_buffers.clear_dirty();
    ctx.state.shader_stages[1].srvs.clear_dirty();
    ctx.state.shader_stages[1].samplers.clear_dirty();
    ctx.state.input_assembler.vertex_buffers.clear_dirty();
    ctx.dirty = DirtyFlags::empty();

    // Re-bind the exact same objects and parameters.
    ctx.vs_set_constant_buffers1(0, 1, cbs.as_ptr(), &raw const fc, &raw const nc);
    ctx.ps_set_shader_resources(0, 1, srvs.as_ptr());
    ctx.ps_set_samplers(0, 1, samps.as_ptr());
    ctx.ia_set_vertex_buffers(0, 1, vbs.as_ptr(), &raw const stride, &raw const offset);
    ctx.rs_set_state(rs.as_ptr());
    ctx.rs_set_viewports(1, (&raw const vp).cast());
    ctx.rs_set_scissor_rects(1, (&raw const sc).cast());

    assert!(!ctx.state.shader_stages[0].constant_buffers.any_dirty());
    assert!(!ctx.state.shader_stages[1].srvs.any_dirty());
    assert!(!ctx.state.shader_stages[1].samplers.any_dirty());
    assert!(!ctx.state.input_assembler.vertex_buffers.any_dirty());
    assert!(ctx.dirty.is_empty());

    ctx.clear_state();
}

#[test]
fn getters_return_set_objects_addrefed() {
    let mut ctx = new_test_context();
    let mut vb = MockCom::new(false);
    let mut srv = MockCom::new(false);
    let mut vs = MockCom::new(false);
    let mut pred = MockCom::new(false);

    assert_eq!(vb.ref_count(), 1);
    let vbs = [vb.as_ptr()];
    let stride = 12_u32;
    let offset = 0_u32;
    ctx.ia_set_vertex_buffers(0, 1, vbs.as_ptr(), &raw const stride, &raw const offset);
    assert_eq!(vb.ref_count(), 2);

    let mut got_vb: *mut c_void = ptr::null_mut();
    ctx.ia_get_vertex_buffers(0, 1, &raw mut got_vb, ptr::null_mut(), ptr::null_mut());
    assert_eq!(got_vb, vb.as_ptr());
    assert_eq!(vb.ref_count(), 3);
    // SAFETY: `got_vb` is a valid `MockCom` pointer.
    unsafe { release(got_vb) };
    assert_eq!(vb.ref_count(), 2);

    let srvs = [srv.as_ptr()];
    ctx.vs_set_shader_resources(2, 1, srvs.as_ptr());
    assert_eq!(srv.ref_count(), 2);
    let mut got_srv: *mut c_void = ptr::null_mut();
    ctx.vs_get_shader_resources(2, 1, &raw mut got_srv);
    assert_eq!(got_srv, srv.as_ptr());
    assert_eq!(srv.ref_count(), 3);
    // SAFETY: `got_srv` is a valid `MockCom` pointer.
    unsafe { release(got_srv) };
    assert_eq!(srv.ref_count(), 2);

    ctx.vs_set_shader(vs.as_ptr(), ptr::null(), 0);
    assert_eq!(vs.ref_count(), 2);
    let mut out_vertex_shader: *mut c_void = ptr::null_mut();
    let mut num_ci = 5_u32;
    ctx.vs_get_shader(&raw mut out_vertex_shader, ptr::null_mut(), &raw mut num_ci);
    assert_eq!(out_vertex_shader, vs.as_ptr());
    assert_eq!(num_ci, 0);
    assert_eq!(vs.ref_count(), 3);
    // SAFETY: `out_vertex_shader` is a valid `MockCom` pointer.
    unsafe { release(out_vertex_shader) };
    assert_eq!(vs.ref_count(), 2);

    ctx.set_predication(pred.as_ptr(), 1);
    assert_eq!(pred.ref_count(), 2);
    let mut got_pred: *mut c_void = ptr::null_mut();
    let mut got_pval = 0_i32;
    ctx.get_predication(&raw mut got_pred, &raw mut got_pval);
    assert_eq!(got_pred, pred.as_ptr());
    assert_eq!(got_pval, 1);
    assert_eq!(pred.ref_count(), 3);
    // SAFETY: `got_pred` is a valid `MockCom` pointer.
    unsafe { release(got_pred) };

    ctx.clear_state();
    assert_eq!(vb.ref_count(), 1);
    assert_eq!(srv.ref_count(), 1);
    assert_eq!(vs.ref_count(), 1);
    assert_eq!(pred.ref_count(), 1);
}

#[test]
fn swap_state_round_trip() {
    let mut ctx = new_test_context();
    let fl = 0xb000_u32;
    let iid = IID_ID3D11_DEVICE_CONTEXT;
    let mut dcs: *mut c_void = ptr::null_mut();
    let mut prev: *mut c_void = ptr::null_mut();

    let hr = create_device_context_state(
        ptr::null_mut(),
        0,
        (&raw const fl, 1),
        7,
        (&raw const iid).cast(),
        ptr::null_mut(),
        &raw mut dcs,
    );
    assert_eq!(hr, S_OK);
    assert!(!dcs.is_null());

    ctx.ia_set_primitive_topology(2);
    ctx.swap_device_context_state(dcs, &raw mut prev);
    let mut topo_after_swap = 2_u32;
    ctx.ia_get_primitive_topology(&raw mut topo_after_swap);
    ctx.swap_device_context_state(prev, ptr::null_mut());
    let mut topo_restored = 0_u32;
    ctx.ia_get_primitive_topology(&raw mut topo_restored);

    let line = format!(
        "d11ctx swap_dcs prev={} topo_swapped={} topo_restored={}",
        usize::from(!prev.is_null()),
        topo_after_swap,
        topo_restored,
    );
    assert_eq!(line, golden_line("d11ctx swap_dcs "));

    // SAFETY: `prev` is a live `DeviceContextState` COM pointer.
    unsafe { release(prev) };
    // SAFETY: `dcs` is a live `DeviceContextState` COM pointer.
    unsafe { release(dcs) };
    drop(ctx);
}

#[test]
fn uav_slots_64_srv_128_cb_14() {
    let mut ctx = new_test_context();
    let mut uav63 = MockCom::new(false);
    let mut om_uav63 = MockCom::new(false);
    let mut srv127 = MockCom::new(false);
    let mut cb13 = MockCom::new(false);

    let cs_uavs = [uav63.as_ptr()];
    ctx.cs_set_unordered_access_views(63, 1, cs_uavs.as_ptr(), ptr::null());
    let mut got_cs_uav: *mut c_void = ptr::null_mut();
    ctx.cs_get_unordered_access_views(63, 1, &raw mut got_cs_uav);
    assert_eq!(got_cs_uav, uav63.as_ptr());

    let om_uavs = [om_uav63.as_ptr()];
    ctx.om_set_render_targets_and_unordered_access_views(
        KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL,
        ptr::null(),
        ptr::null_mut(),
        63,
        1,
        om_uavs.as_ptr(),
        ptr::null(),
    );
    let mut got_om_uav: *mut c_void = ptr::null_mut();
    ctx.om_get_render_targets_and_unordered_access_views(
        0,
        ptr::null_mut(),
        ptr::null_mut(),
        63,
        1,
        &raw mut got_om_uav,
    );
    assert_eq!(got_om_uav, om_uav63.as_ptr());

    let srvs = [srv127.as_ptr()];
    ctx.ps_set_shader_resources(127, 1, srvs.as_ptr());
    let mut got_srv: *mut c_void = ptr::null_mut();
    ctx.ps_get_shader_resources(127, 1, &raw mut got_srv);
    assert_eq!(got_srv, srv127.as_ptr());

    let cbs = [cb13.as_ptr()];
    let fc = 16_u32;
    let nc = 32_u32;
    ctx.vs_set_constant_buffers1(13, 1, cbs.as_ptr(), &raw const fc, &raw const nc);
    let mut got_cb: *mut c_void = ptr::null_mut();
    let mut out_first_const = 0_u32;
    let mut out_num_const = 0_u32;
    ctx.vs_get_constant_buffers1(
        13,
        1,
        &raw mut got_cb,
        &raw mut out_first_const,
        &raw mut out_num_const,
    );
    assert_eq!(got_cb, cb13.as_ptr());
    assert_eq!(out_first_const, 16);
    assert_eq!(out_num_const, 32);

    for ptr in [got_cs_uav, got_om_uav, got_srv, got_cb] {
        // SAFETY: `ptr` is a `MockCom` pointer returned with +1 refcount.
        unsafe { release(ptr) };
    }
    ctx.clear_state();
}
