use core::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicU32, Ordering},
};

use dxmt_core::encoder::BufferSlice;

use crate::{
    abi::{
        records::{
            D3d11ShaderResourceViewDesc1, D3d11ShaderResourceViewDesc1Union, D3d11Tex2dSrv1,
        },
        vtbl::IUnknownVtbl,
    },
    com::{E_NOINTERFACE, Hresult},
    context::{
        CommandBufferState, Context, Immediate,
        state::{ContextState, DirtyFlags, ShaderStage},
    },
    state::Defaults,
    subresource::ResourceSubset,
    view::{Srv, SrvObject},
};

#[repr(C)]
struct DummyCom {
    vtbl: *const IUnknownVtbl,
    refs: AtomicU32,
}

unsafe extern "system-unwind" fn dummy_qi(
    _this: *mut c_void,
    _riid: *const c_void,
    _out: *mut *mut c_void,
) -> Hresult {
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn dummy_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` points to a `DummyCom`.
    let obj = unsafe { &*this.cast::<DummyCom>() };
    obj.refs.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system-unwind" fn dummy_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` points to a `DummyCom`.
    let obj = unsafe { &*this.cast::<DummyCom>() };
    obj.refs.fetch_sub(1, Ordering::Relaxed) - 1
}

static DUMMY_VTBL: IUnknownVtbl = IUnknownVtbl {
    query_interface: dummy_qi,
    add_ref: dummy_add_ref,
    release: dummy_release,
};

impl DummyCom {
    fn new() -> Self {
        Self {
            vtbl: &raw const DUMMY_VTBL,
            refs: AtomicU32::new(1),
        }
    }

    fn as_ptr(&mut self) -> *mut c_void {
        ptr::from_mut(self).cast()
    }
}

fn new_ctx() -> Context<Immediate> {
    Context {
        vtbl: ptr::null(),
        ext_vtbl: ptr::null(),
        annotation_vtbl: ptr::null(),
        refs: crate::com::Managed::default(),
        private_data: dxmt_util::private_data::PrivateData::default(),
        device: ptr::null_mut(),
        state: ContextState::default(),
        cmdbuf_state: CommandBufferState::Idle,
        encode: crate::context::encode::EncodeSession::default(),
        dirty: DirtyFlags::empty(),
        defaults: Defaults::default(),
        mode: Immediate::default(),
    }
}

#[test]
fn so_set_targets_rejects_duplicate_buffers() {
    let mut ctx = new_ctx();
    let mut b0 = DummyCom::new();
    let mut b1 = DummyCom::new();

    let valid = [b0.as_ptr(), b1.as_ptr()];
    let offsets = [0_u32, 64_u32];
    ctx.so_set_targets(2, valid.as_ptr(), offsets.as_ptr());
    assert!(ctx.state.stream_output.targets.test_bound(0));
    assert!(ctx.state.stream_output.targets.test_bound(1));

    // Duplicate non-null buffer in `SOSetTargets` is rejected without mutating bound targets.
    let duplicate = [b1.as_ptr(), b1.as_ptr()];
    ctx.so_set_targets(2, duplicate.as_ptr(), offsets.as_ptr());
    assert_eq!(
        ctx.state
            .stream_output
            .targets
            .get(0)
            .map(|e| e.buffer.as_ptr()),
        Some(b0.as_ptr())
    );
    ctx.clear_state();
}

#[test]
fn all_six_shader_stages_and_null_blend_factor() {
    let mut ctx = new_ctx();
    let mut sh = DummyCom::new();
    let mut cb = DummyCom::new();
    let mut srv = DummyCom::new();
    let mut samp = DummyCom::new();

    let cbs = [cb.as_ptr()];
    let srvs = [srv.as_ptr()];
    let samps = [samp.as_ptr()];

    ctx.vs_set_shader(sh.as_ptr(), ptr::null(), 0);
    ctx.ps_set_shader(sh.as_ptr(), ptr::null(), 0);
    ctx.gs_set_shader(sh.as_ptr(), ptr::null(), 0);
    ctx.hs_set_shader(sh.as_ptr(), ptr::null(), 0);
    ctx.ds_set_shader(sh.as_ptr(), ptr::null(), 0);
    ctx.cs_set_shader(sh.as_ptr(), ptr::null(), 0);

    ctx.vs_set_constant_buffers(0, 1, cbs.as_ptr());
    ctx.ps_set_constant_buffers(0, 1, cbs.as_ptr());
    ctx.gs_set_constant_buffers(0, 1, cbs.as_ptr());
    ctx.hs_set_constant_buffers(0, 1, cbs.as_ptr());
    ctx.ds_set_constant_buffers(0, 1, cbs.as_ptr());
    ctx.cs_set_constant_buffers(0, 1, cbs.as_ptr());

    ctx.vs_set_shader_resources(0, 1, srvs.as_ptr());
    ctx.ps_set_shader_resources(0, 1, srvs.as_ptr());
    ctx.gs_set_shader_resources(0, 1, srvs.as_ptr());
    ctx.hs_set_shader_resources(0, 1, srvs.as_ptr());
    ctx.ds_set_shader_resources(0, 1, srvs.as_ptr());
    ctx.cs_set_shader_resources(0, 1, srvs.as_ptr());

    ctx.vs_set_samplers(0, 1, samps.as_ptr());
    ctx.ps_set_samplers(0, 1, samps.as_ptr());
    ctx.gs_set_samplers(0, 1, samps.as_ptr());
    ctx.hs_set_samplers(0, 1, samps.as_ptr());
    ctx.ds_set_samplers(0, 1, samps.as_ptr());
    ctx.cs_set_samplers(0, 1, samps.as_ptr());

    for stage in [
        ShaderStage::Vertex,
        ShaderStage::Pixel,
        ShaderStage::Geometry,
        ShaderStage::Hull,
        ShaderStage::Domain,
        ShaderStage::Compute,
    ] {
        let st = &ctx.state.shader_stages[stage as usize];
        assert_eq!(st.shader.as_ptr(), sh.as_ptr());
        assert!(st.constant_buffers.test_bound(0));
        assert!(st.srvs.test_bound(0));
        assert!(st.samplers.test_bound(0));
    }

    // Passing null `blend_factor` resets blend factor to `[1.0; 4]`.
    let custom_bf = [0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32];
    ctx.om_set_blend_state(ptr::null_mut(), custom_bf.as_ptr(), 0xff);
    assert_eq!(ctx.state.output_merger.blend_factor, custom_bf);
    ctx.om_set_blend_state(ptr::null_mut(), ptr::null(), 0xff);
    assert_eq!(ctx.state.output_merger.blend_factor, [1.0, 1.0, 1.0, 1.0]);

    ctx.clear_state();
}

/// An SRV of `subset` of `resource`, as `CreateShaderResourceView` would build it.
fn srv_of(resource: *mut c_void, subset: ResourceSubset) -> *mut c_void {
    let inner = Srv {
        resource,
        buffer: None,
        texture: None,
        buffer_view: None,
        texture_view: None,
        slice: BufferSlice::default(),
        subset,
        format: 0,
    };
    let desc = D3d11ShaderResourceViewDesc1 {
        format: 0,
        view_dimension: 4,
        u: D3d11ShaderResourceViewDesc1Union {
            texture2d: D3d11Tex2dSrv1::default(),
        },
    };
    SrvObject::into_raw(inner, ptr::null_mut(), 0, desc)
}

/// Mip `level` of a one-slice, one-plane texture (the per-subresource bitmask encoding).
const fn mip(level: u32) -> ResourceSubset {
    ResourceSubset(0b11 | (1 << (2 + level)))
}

#[test]
fn om_bind_unbinds_srvs_of_its_mip_only() {
    let mut texture = DummyCom::new();
    let mut other = DummyCom::new();
    let views = [
        srv_of(texture.as_ptr(), mip(0)),
        srv_of(texture.as_ptr(), mip(1)),
        srv_of(other.as_ptr(), mip(1)),
    ];
    let mut ctx = new_ctx();
    ctx.set_shader_resource(ShaderStage::Pixel, 3, 3, views.as_ptr());
    ctx.set_shader_resource(ShaderStage::Compute, 0, 3, views.as_ptr());
    let srvs = |ctx: &Context<Immediate>, stage: ShaderStage| {
        ctx.state.shader_stages[stage.index()].srvs.bound_mask()
    };

    // A render target on mip 1 (mip generation's pattern: read mip 0, write mip 1).
    ctx.resolve_om_srv_hazard(texture.as_ptr(), mip(1));
    assert_eq!(srvs(&ctx, ShaderStage::Pixel), 0b101 << 3);
    // The compute stage is not the output merger's.
    assert_eq!(srvs(&ctx, ShaderStage::Compute), 0b111);

    // A compute UAV on mip 0.
    ctx.resolve_cs_srv_hazard(texture.as_ptr(), mip(0));
    assert_eq!(srvs(&ctx, ShaderStage::Compute), 0b110);

    // The whole resource, as a buffer bind does.
    ctx.resolve_om_srv_hazard(texture.as_ptr(), ResourceSubset::WHOLE);
    assert_eq!(srvs(&ctx, ShaderStage::Pixel), 0b100 << 3);

    for stage in [ShaderStage::Pixel, ShaderStage::Compute] {
        ctx.state.shader_stages[stage.index()].srvs.clear();
    }
    for view in views {
        // SAFETY: the creation reference of a live `SrvObject`.
        unsafe { crate::com::release(view) };
    }
}
