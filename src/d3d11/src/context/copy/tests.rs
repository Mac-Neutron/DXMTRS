use core::{ffi::c_void, ptr};
use std::cell::RefCell;

use dxmt_core::{
    dynamic::DynamicBuffer,
    encoder::Encoder,
    resource::{BufferAllocation, BufferAllocationFlags},
};
use dxmt_util::private_data::PrivateData;

use super::{
    clear_test_state, dynamic_buffer_stats, register_test_buffer, register_test_dynamic_buffer,
    register_test_staging, register_test_texture, staging_slice_bytes, take_not_implemented_msgs,
    take_recorded_blit_cmds,
};
use crate::{
    abi::{
        records::{
            D3d11Box, D3d11BufferDesc, D3d11Texture2dDesc, D3d11Texture2dDesc1, D3d11Texture3dDesc,
            D3d11Texture3dDesc1, DxgiSampleDesc,
        },
        vtbl::{ID3D11BufferVtbl, ID3D11Texture2D1Vtbl, ID3D11Texture3D1Vtbl},
    },
    com::{E_NOINTERFACE, Hresult, Managed},
    context::{
        CommandBufferState, Context, Mode,
        state::{ContextState, DirtyFlags},
    },
    state::Defaults,
};

const GOLDEN_TRANSFER: &str = include_str!("golden/d3d11_transfer.txt");

const D3D11_RESOURCE_DIMENSION_BUFFER: u32 = 1;
const D3D11_RESOURCE_DIMENSION_TEXTURE2D: u32 = 3;
const D3D11_RESOURCE_DIMENSION_TEXTURE3D: u32 = 4;

const DXGI_FORMAT_R32G32B32A32_FLOAT: u32 = 2;
const DXGI_FORMAT_R8G8B8A8_UNORM: u32 = 28;
const DXGI_FORMAT_R8_UNORM: u32 = 61;
const DXGI_FORMAT_BC1_UNORM: u32 = 71;
const DXGI_FORMAT_BC2_UNORM: u32 = 74;

const D3D11_COPY_NO_OVERWRITE: u32 = 1;
const D3D11_COPY_DISCARD: u32 = 2;

thread_local! {
    static NEXT_STAGING: RefCell<(u64, u64)> = const { RefCell::new((85, 0)) };
}

fn set_next_staging(handle: u64, offset: u64) {
    NEXT_STAGING.with(|cell| {
        *cell.borrow_mut() = (handle, offset);
    });
}

#[derive(Default)]
struct TestMode;

impl Mode for TestMode {
    fn emit_st(_ctx: &mut Context<Self>, _f: impl Fn(&mut Encoder) + Send + 'static) {}

    fn emit_op(_ctx: &mut Context<Self>, _f: impl Fn(&mut Encoder) + Send + 'static) {}

    fn retain<T: Send + 'static>(_ctx: &mut Context<Self>, _t: T) {}

    fn alloc(_ctx: &mut Context<Self>, _size: usize, _align: usize) -> (*mut u8, u64) {
        (ptr::null_mut(), 0)
    }

    fn staging(_ctx: &mut Context<Self>, size: u64, _align: u64) -> (u64, u64) {
        NEXT_STAGING.with(|cell| {
            let mut st = cell.borrow_mut();
            let (handle, offset) = *st;
            st.1 = st.1.saturating_add(size);
            (handle, offset)
        })
    }

    fn dynamic_name(
        _ctx: &mut Context<Self>,
        dynamic: &DynamicBuffer,
    ) -> std::sync::Arc<BufferAllocation> {
        dynamic.immediate_name()
    }
}

fn new_test_context() -> Context<TestMode> {
    Context {
        vtbl: ptr::null(),
        ext_vtbl: ptr::null(),
        annotation_vtbl: ptr::null(),
        refs: Managed::default(),
        private_data: PrivateData::default(),
        device: ptr::null_mut(),
        state: ContextState {
            sample_mask: 0xffff_ffff,
            stencil_ref: 0,
            blend_factor: [0.0; 4],
            topology: 4,
        },
        cmdbuf_state: CommandBufferState::Idle,
        dirty: DirtyFlags::empty(),
        defaults: Defaults::default(),
        mode: TestMode,
    }
}

#[repr(C)]
struct FakeBufferCom {
    vtbl: *const ID3D11BufferVtbl,
    desc: D3d11BufferDesc,
}

unsafe extern "system-unwind" fn fake_qi(
    _this: *mut c_void,
    _riid: *const c_void,
    _out: *mut *mut c_void,
) -> Hresult {
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn fake_add_ref(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "system-unwind" fn fake_release(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "system-unwind" fn fake_get_device(_this: *mut c_void, _out: *mut *mut c_void) {}

unsafe extern "system-unwind" fn fake_get_priv(
    _this: *mut c_void,
    _guid: *const c_void,
    _size: *mut u32,
    _data: *mut c_void,
) -> Hresult {
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn fake_set_priv(
    _this: *mut c_void,
    _guid: *const c_void,
    _size: u32,
    _data: *const c_void,
) -> Hresult {
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn fake_set_priv_iface(
    _this: *mut c_void,
    _guid: *const c_void,
    _iface: *const c_void,
) -> Hresult {
    E_NOINTERFACE
}

unsafe extern "system-unwind" fn fake_buf_get_type(_this: *mut c_void, dim: *mut u32) {
    // SAFETY: test caller passes a valid `dim` pointer.
    unsafe { *dim = D3D11_RESOURCE_DIMENSION_BUFFER };
}

unsafe extern "system-unwind" fn fake_tex2d_get_type(_this: *mut c_void, dim: *mut u32) {
    // SAFETY: test caller passes a valid `dim` pointer.
    unsafe { *dim = D3D11_RESOURCE_DIMENSION_TEXTURE2D };
}

unsafe extern "system-unwind" fn fake_tex3d_get_type(_this: *mut c_void, dim: *mut u32) {
    // SAFETY: test caller passes a valid `dim` pointer.
    unsafe { *dim = D3D11_RESOURCE_DIMENSION_TEXTURE3D };
}

unsafe extern "system-unwind" fn fake_set_eviction(_this: *mut c_void, _prio: u32) {}

unsafe extern "system-unwind" fn fake_get_eviction(_this: *mut c_void) -> u32 {
    0
}

unsafe extern "system-unwind" fn fake_buf_get_desc(this: *mut c_void, out: *mut c_void) {
    // SAFETY: `this` is a `FakeBufferCom` in unit tests.
    let com = unsafe { &*this.cast::<FakeBufferCom>() };
    // SAFETY: `out` points to a writable `D3d11BufferDesc`.
    unsafe { *out.cast::<D3d11BufferDesc>() = com.desc };
}

static FAKE_BUFFER_VTBL: ID3D11BufferVtbl = ID3D11BufferVtbl {
    query_interface: fake_qi,
    add_ref: fake_add_ref,
    release: fake_release,
    get_device: fake_get_device,
    get_private_data: fake_get_priv,
    set_private_data: fake_set_priv,
    set_private_data_interface: fake_set_priv_iface,
    get_type: fake_buf_get_type,
    set_eviction_priority: fake_set_eviction,
    get_eviction_priority: fake_get_eviction,
    get_desc: fake_buf_get_desc,
};

#[repr(C)]
struct FakeTex2DCom {
    vtbl: *const ID3D11Texture2D1Vtbl,
    desc: D3d11Texture2dDesc1,
}

unsafe extern "system-unwind" fn fake_tex2d_get_desc(this: *mut c_void, out: *mut c_void) {
    // SAFETY: `this` is a `FakeTex2DCom` in unit tests.
    let com = unsafe { &*this.cast::<FakeTex2DCom>() };
    let desc = D3d11Texture2dDesc {
        width: com.desc.width,
        height: com.desc.height,
        mip_levels: com.desc.mip_levels,
        array_size: com.desc.array_size,
        format: com.desc.format,
        sample_desc: com.desc.sample_desc,
        usage: com.desc.usage,
        bind_flags: com.desc.bind_flags,
        cpu_access_flags: com.desc.cpu_access_flags,
        misc_flags: com.desc.misc_flags,
    };
    // SAFETY: `out` points to a writable `D3d11Texture2dDesc`.
    unsafe { *out.cast::<D3d11Texture2dDesc>() = desc };
}

unsafe extern "system-unwind" fn fake_tex2d_get_desc1(this: *mut c_void, out: *mut c_void) {
    // SAFETY: `this` is a `FakeTex2DCom` in unit tests.
    let com = unsafe { &*this.cast::<FakeTex2DCom>() };
    // SAFETY: `out` points to a writable `D3d11Texture2dDesc1`.
    unsafe { *out.cast::<D3d11Texture2dDesc1>() = com.desc };
}

static FAKE_TEX2D_VTBL: ID3D11Texture2D1Vtbl = ID3D11Texture2D1Vtbl {
    query_interface: fake_qi,
    add_ref: fake_add_ref,
    release: fake_release,
    get_device: fake_get_device,
    get_private_data: fake_get_priv,
    set_private_data: fake_set_priv,
    set_private_data_interface: fake_set_priv_iface,
    get_type: fake_tex2d_get_type,
    set_eviction_priority: fake_set_eviction,
    get_eviction_priority: fake_get_eviction,
    get_desc: fake_tex2d_get_desc,
    get_desc1: fake_tex2d_get_desc1,
};

#[repr(C)]
struct FakeTex3DCom {
    vtbl: *const ID3D11Texture3D1Vtbl,
    desc: D3d11Texture3dDesc1,
}

unsafe extern "system-unwind" fn fake_tex3d_get_desc(this: *mut c_void, out: *mut c_void) {
    // SAFETY: `this` is a `FakeTex3DCom` in unit tests.
    let com = unsafe { &*this.cast::<FakeTex3DCom>() };
    let desc = D3d11Texture3dDesc {
        width: com.desc.width,
        height: com.desc.height,
        depth: com.desc.depth,
        mip_levels: com.desc.mip_levels,
        format: com.desc.format,
        usage: com.desc.usage,
        bind_flags: com.desc.bind_flags,
        cpu_access_flags: com.desc.cpu_access_flags,
        misc_flags: com.desc.misc_flags,
    };
    // SAFETY: `out` points to a writable `D3d11Texture3dDesc`.
    unsafe { *out.cast::<D3d11Texture3dDesc>() = desc };
}

unsafe extern "system-unwind" fn fake_tex3d_get_desc1(this: *mut c_void, out: *mut c_void) {
    // SAFETY: `this` is a `FakeTex3DCom` in unit tests.
    let com = unsafe { &*this.cast::<FakeTex3DCom>() };
    // SAFETY: `out` points to a writable `D3d11Texture3dDesc1`.
    unsafe { *out.cast::<D3d11Texture3dDesc1>() = com.desc };
}

static FAKE_TEX3D_VTBL: ID3D11Texture3D1Vtbl = ID3D11Texture3D1Vtbl {
    query_interface: fake_qi,
    add_ref: fake_add_ref,
    release: fake_release,
    get_device: fake_get_device,
    get_private_data: fake_get_priv,
    set_private_data: fake_set_priv,
    set_private_data_interface: fake_set_priv_iface,
    get_type: fake_tex3d_get_type,
    set_eviction_priority: fake_set_eviction,
    get_eviction_priority: fake_get_eviction,
    get_desc: fake_tex3d_get_desc,
    get_desc1: fake_tex3d_get_desc1,
};

const fn make_buffer(byte_width: u32, usage: u32) -> FakeBufferCom {
    FakeBufferCom {
        vtbl: &raw const FAKE_BUFFER_VTBL,
        desc: D3d11BufferDesc {
            byte_width,
            usage,
            bind_flags: 0,
            cpu_access_flags: 0,
            misc_flags: 0,
            structure_byte_stride: 0,
        },
    }
}

fn make_tex2d(
    width: u32,
    height: u32,
    mip_levels: u32,
    array_size: u32,
    format: u32,
) -> FakeTex2DCom {
    let desc = D3d11Texture2dDesc1 {
        width,
        height,
        mip_levels,
        array_size,
        format,
        sample_desc: DxgiSampleDesc {
            count: 1,
            quality: 0,
        },
        ..D3d11Texture2dDesc1::default()
    };
    FakeTex2DCom {
        vtbl: &raw const FAKE_TEX2D_VTBL,
        desc,
    }
}

fn make_tex3d(width: u32, height: u32, depth: u32, mip_levels: u32, format: u32) -> FakeTex3DCom {
    let desc = D3d11Texture3dDesc1 {
        width,
        height,
        depth,
        mip_levels,
        format,
        ..D3d11Texture3dDesc1::default()
    };
    FakeTex3DCom {
        vtbl: &raw const FAKE_TEX3D_VTBL,
        desc,
    }
}

/// Replays the copy/update region cases from `d3d11_transfer.c` against the golden wmtcmd dump.
#[test]
fn copy_region_boxes_as_cpp() {
    clear_test_state();
    let mut ctx = new_test_context();

    // 1. 2D 32x32 RGBA8 CopyResource (#2 -> #4) and readback to staging (#4 -> #71)
    let mut tex_src = make_tex2d(32, 32, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM);
    let mut tex_dst = make_tex2d(32, 32, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM);
    let mut tex_stg = make_tex2d(32, 32, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM);
    let p_tex_src = (&raw mut tex_src).cast::<c_void>();
    let p_tex_dst = (&raw mut tex_dst).cast::<c_void>();
    let p_tex_stg = (&raw mut tex_stg).cast::<c_void>();
    register_test_texture(p_tex_src, 2, DXGI_FORMAT_R8G8B8A8_UNORM, 32, 32);
    register_test_texture(p_tex_dst, 4, DXGI_FORMAT_R8G8B8A8_UNORM, 32, 32);
    register_test_staging(p_tex_stg, 0, 71, 128, 4096, 4096);

    ctx.copy_resource(p_tex_dst, p_tex_src);
    ctx.copy_resource(p_tex_stg, p_tex_dst);

    // 2. Buffer CopySubresourceRegion (#71 -> #74, src_box=[100..300], dst_x=52) & CopyResource (#74 -> #77, 512B)
    let mut buf_src = make_buffer(512, 0);
    let mut buf_dst = make_buffer(512, 0);
    let mut buf_stg = make_buffer(512, 3);
    let p_buf_src = (&raw mut buf_src).cast::<c_void>();
    let p_buf_dst = (&raw mut buf_dst).cast::<c_void>();
    let p_buf_stg = (&raw mut buf_stg).cast::<c_void>();
    register_test_buffer(p_buf_src, 71, 0, 512);
    register_test_buffer(p_buf_dst, 74, 0, 512);
    register_test_staging(p_buf_stg, 0, 77, 512, 512, 512);

    let buf_box = D3d11Box {
        left: 100,
        top: 0,
        front: 0,
        right: 300,
        bottom: 1,
        back: 1,
    };
    ctx.copy_subresource_region(
        p_buf_dst,
        0,
        52,
        0,
        0,
        p_buf_src,
        0,
        (&raw const buf_box).cast::<c_void>(),
    );
    ctx.copy_resource(p_buf_stg, p_buf_dst);

    // 3. 2D texture CopySubresourceRegion (#2 -> #79, box=(8,8,0)-(24,16,1), dst=(0,4,0))
    let mut tex_region_dst = make_tex2d(32, 32, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM);
    let p_tex_region_dst = (&raw mut tex_region_dst).cast::<c_void>();
    register_test_texture(p_tex_region_dst, 79, DXGI_FORMAT_R8G8B8A8_UNORM, 32, 32);
    let tex_box = D3d11Box {
        left: 8,
        top: 8,
        front: 0,
        right: 24,
        bottom: 16,
        back: 1,
    };
    ctx.copy_subresource_region(
        p_tex_region_dst,
        0,
        0,
        4,
        0,
        p_tex_src,
        0,
        (&raw const tex_box).cast::<c_void>(),
    );

    // 4. UpdateSubresource on buffer (#85@0 -> #77@64, len=128)
    let mut buf_upd = make_buffer(256, 0);
    let p_buf_upd = (&raw mut buf_upd).cast::<c_void>();
    register_test_buffer(p_buf_upd, 77, 0, 256);
    set_next_staging(85, 0);
    let payload = vec![0xaa_u8; 4096];
    let upd_buf_box = D3d11Box {
        left: 64,
        top: 0,
        front: 0,
        right: 192,
        bottom: 1,
        back: 1,
    };
    ctx.update_subresource(
        p_buf_upd,
        0,
        (&raw const upd_buf_box).cast::<c_void>(),
        payload.as_ptr().cast::<c_void>(),
        0,
        0,
    );

    // 5. UpdateSubresource on 2D texture (#85@128 -> #90, box=(4,4,0)-(20,12,1))
    let mut tex_upd2d = make_tex2d(32, 32, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM);
    let p_tex_upd2d = (&raw mut tex_upd2d).cast::<c_void>();
    register_test_texture(p_tex_upd2d, 90, DXGI_FORMAT_R8G8B8A8_UNORM, 32, 32);
    let upd_2d_box = D3d11Box {
        left: 4,
        top: 4,
        front: 0,
        right: 20,
        bottom: 12,
        back: 1,
    };
    ctx.update_subresource(
        p_tex_upd2d,
        0,
        (&raw const upd_2d_box).cast::<c_void>(),
        payload.as_ptr().cast::<c_void>(),
        64,
        512,
    );

    // 6. UpdateSubresource on 3D texture (#85@640 -> #95, box=(2,2,1)-(10,10,5), R8_UNORM)
    let mut tex_upd3d = make_tex3d(16, 16, 8, 1, DXGI_FORMAT_R8_UNORM);
    let p_tex_upd3d = (&raw mut tex_upd3d).cast::<c_void>();
    register_test_texture(p_tex_upd3d, 95, DXGI_FORMAT_R8_UNORM, 16, 16);
    let upd_3d_box = D3d11Box {
        left: 2,
        top: 2,
        front: 1,
        right: 10,
        bottom: 10,
        back: 5,
    };
    ctx.update_subresource(
        p_tex_upd3d,
        0,
        (&raw const upd_3d_box).cast::<c_void>(),
        payload.as_ptr().cast::<c_void>(),
        8,
        64,
    );

    // 7. UpdateSubresource on 2D array mip (#85@896 -> #100, slice=1, level=2, 6 mips => subresource=8)
    let mut tex_arr = make_tex2d(32, 32, 6, 3, DXGI_FORMAT_R8G8B8A8_UNORM);
    let p_tex_arr = (&raw mut tex_arr).cast::<c_void>();
    register_test_texture(p_tex_arr, 100, DXGI_FORMAT_R8G8B8A8_UNORM, 32, 32);
    ctx.update_subresource(
        p_tex_arr,
        8,
        ptr::null(),
        payload.as_ptr().cast::<c_void>(),
        32,
        256,
    );

    let expected_first_9: Vec<&str> = GOLDEN_TRANSFER.lines().take(9).collect();
    let recorded = take_recorded_blit_cmds();
    assert_eq!(recorded, expected_first_9);

    // Verify invalid / out-of-bounds boxes are silently ignored as no-op (matching C++).
    let empty_box = D3d11Box {
        left: 16,
        top: 0,
        front: 0,
        right: 16,
        bottom: 4,
        back: 1,
    };
    ctx.copy_subresource_region(
        p_tex_dst,
        0,
        0,
        0,
        0,
        p_tex_src,
        0,
        (&raw const empty_box).cast::<c_void>(),
    );
    ctx.copy_subresource_region(
        p_tex_dst,
        0,
        24,
        0,
        0,
        p_tex_src,
        0,
        (&raw const tex_box).cast::<c_void>(),
    );
    assert_eq!(take_recorded_blit_cmds(), Vec::<String>::new());
}

/// Verifies BC 4x4 block alignment and virtual-size clamping at full and sub-block mip levels.
#[test]
fn bc_block_alignment() {
    clear_test_state();
    let mut ctx = new_test_context();
    let payload = vec![0x55_u8; 4096];

    // Golden line 10: BC1_UNORM 32x32 (#105), UpdateSubresource at (8,8)..(24,24)
    set_next_staging(85, 1152);
    let mut tex_bc1 = make_tex2d(32, 32, 1, 1, DXGI_FORMAT_BC1_UNORM);
    let p_tex_bc1 = (&raw mut tex_bc1).cast::<c_void>();
    register_test_texture(p_tex_bc1, 105, DXGI_FORMAT_BC1_UNORM, 32, 32);
    let bc1_box = D3d11Box {
        left: 8,
        top: 8,
        front: 0,
        right: 24,
        bottom: 24,
        back: 1,
    };
    ctx.update_subresource(
        p_tex_bc1,
        0,
        (&raw const bc1_box).cast::<c_void>(),
        payload.as_ptr().cast::<c_void>(),
        32,
        128,
    );

    // Golden line 11: BC2_UNORM 8x8 with 4 mips (#120), UpdateSubresource on mip 2 (2x2 sub-block!)
    let mut tex_bc2_mips = make_tex2d(8, 8, 4, 1, DXGI_FORMAT_BC2_UNORM);
    let p_tex_bc2_mips = (&raw mut tex_bc2_mips).cast::<c_void>();
    register_test_texture(p_tex_bc2_mips, 120, DXGI_FORMAT_BC2_UNORM, 8, 8);
    let subblock_box = D3d11Box {
        left: 0,
        top: 0,
        front: 0,
        right: 4,
        bottom: 4,
        back: 1,
    };
    ctx.update_subresource(
        p_tex_bc2_mips,
        2,
        (&raw const subblock_box).cast::<c_void>(),
        payload.as_ptr().cast::<c_void>(),
        16,
        16,
    );

    // Golden line 12: 2x2 RGBA32_FLOAT texture (#128) readback to staging (#129)
    let mut tex_2x2 = make_tex2d(2, 2, 1, 1, DXGI_FORMAT_R32G32B32A32_FLOAT);
    let mut stg_2x2 = make_tex2d(2, 2, 1, 1, DXGI_FORMAT_R32G32B32A32_FLOAT);
    let p_tex_2x2 = (&raw mut tex_2x2).cast::<c_void>();
    let p_stg_2x2 = (&raw mut stg_2x2).cast::<c_void>();
    register_test_texture(p_tex_2x2, 128, DXGI_FORMAT_R32G32B32A32_FLOAT, 2, 2);
    register_test_staging(p_stg_2x2, 0, 129, 16, 32, 32);
    ctx.copy_resource(p_stg_2x2, p_tex_2x2);

    // Golden line 13: buffer partial update at end of 64KiB buffer (#85@1296 -> #118@65520, len=16)
    let mut buf_64k = make_buffer(65536, 0);
    let p_buf_64k = (&raw mut buf_64k).cast::<c_void>();
    register_test_buffer(p_buf_64k, 118, 0, 65536);
    let end_box = D3d11Box {
        left: 65520,
        top: 0,
        front: 0,
        right: 65536,
        bottom: 1,
        back: 1,
    };
    ctx.update_subresource(
        p_buf_64k,
        0,
        (&raw const end_box).cast::<c_void>(),
        payload.as_ptr().cast::<c_void>(),
        0,
        0,
    );

    // Golden line 14: 3D 4x4x4 R8G8B8A8_UNORM texture (#128) UpdateSubresource (#85@1312)
    let mut tex_3d_rgba = make_tex3d(4, 4, 4, 1, DXGI_FORMAT_R8G8B8A8_UNORM);
    let p_tex_3d_rgba = (&raw mut tex_3d_rgba).cast::<c_void>();
    register_test_texture(p_tex_3d_rgba, 128, DXGI_FORMAT_R8G8B8A8_UNORM, 4, 4);
    ctx.update_subresource(
        p_tex_3d_rgba,
        0,
        ptr::null(),
        payload.as_ptr().cast::<c_void>(),
        16,
        64,
    );

    let expected_last_5: Vec<&str> = GOLDEN_TRANSFER.lines().skip(9).collect();
    let recorded = take_recorded_blit_cmds();
    assert_eq!(recorded, expected_last_5);
}

/// Verifies `UpdateSubresource1` on staging resources triggers the `not implemented` path.
#[test]
fn staging_update_is_not_implemented() {
    clear_test_state();
    let mut ctx = new_test_context();
    let payload = [1_u8, 2, 3, 4];

    let mut stg_buf = make_buffer(64, 3);
    let p_stg_buf = (&raw mut stg_buf).cast::<c_void>();
    register_test_staging(p_stg_buf, 0, 500, 64, 64, 64);
    ctx.update_subresource(
        p_stg_buf,
        0,
        ptr::null(),
        payload.as_ptr().cast::<c_void>(),
        0,
        0,
    );

    let mut stg_tex = make_tex2d(8, 8, 1, 1, DXGI_FORMAT_R8G8B8A8_UNORM);
    let p_stg_tex = (&raw mut stg_tex).cast::<c_void>();
    register_test_staging(p_stg_tex, 0, 501, 32, 256, 256);
    ctx.update_subresource(
        p_stg_tex,
        0,
        ptr::null(),
        payload.as_ptr().cast::<c_void>(),
        32,
        256,
    );

    let msgs = take_not_implemented_msgs();
    assert_eq!(
        msgs,
        vec![
            "update buffer: staging".to_owned(),
            "update texture: staging".to_owned(),
        ]
    );
}

/// Verifies BC <-> uncompressed staging copies trigger the `not implemented` paths.
#[test]
fn compressed_to_staging_is_not_implemented() {
    clear_test_state();
    let mut ctx = new_test_context();

    let mut tex_bc = make_tex2d(4, 4, 1, 1, DXGI_FORMAT_BC2_UNORM);
    let mut stg_rgba = make_tex2d(1, 1, 1, 1, DXGI_FORMAT_R32G32B32A32_FLOAT);
    let mut tex_rgba = make_tex2d(1, 1, 1, 1, DXGI_FORMAT_R32G32B32A32_FLOAT);
    let mut stg_bc = make_tex2d(4, 4, 1, 1, DXGI_FORMAT_BC2_UNORM);

    let p_tex_bc = (&raw mut tex_bc).cast::<c_void>();
    let p_stg_rgba = (&raw mut stg_rgba).cast::<c_void>();
    let p_tex_rgba = (&raw mut tex_rgba).cast::<c_void>();
    let p_stg_bc = (&raw mut stg_bc).cast::<c_void>();

    register_test_texture(p_tex_bc, 600, DXGI_FORMAT_BC2_UNORM, 4, 4);
    register_test_staging(p_stg_rgba, 0, 601, 16, 16, 16);
    register_test_texture(p_tex_rgba, 602, DXGI_FORMAT_R32G32B32A32_FLOAT, 1, 1);
    register_test_staging(p_stg_bc, 0, 603, 16, 16, 16);

    // 1. BC default -> uncompressed staging
    ctx.copy_subresource_region(p_stg_rgba, 0, 0, 0, 0, p_tex_bc, 0, ptr::null());
    // 2. BC staging -> uncompressed default
    ctx.copy_subresource_region(p_tex_rgba, 0, 0, 0, 0, p_stg_bc, 0, ptr::null());
    // 3. Uncompressed default -> BC staging
    ctx.copy_subresource_region(p_stg_bc, 0, 0, 0, 0, p_tex_rgba, 0, ptr::null());
    // 4. Uncompressed staging -> BC default
    ctx.copy_subresource_region(p_tex_bc, 0, 0, 0, 0, p_stg_rgba, 0, ptr::null());

    let msgs = take_not_implemented_msgs();
    assert_eq!(
        msgs,
        vec![
            "copy texture: from compressed to staging".to_owned(),
            "copy texture: from compressed staging to default".to_owned(),
            "copy texture: copy to compressed staging".to_owned(),
            "copy texture: from staging to compressed default".to_owned(),
        ]
    );
}

/// Implementation of `update::tests::dynamic_update_paths_as_cpp`.
fn run_dynamic_update_paths_as_cpp() {
    clear_test_state();
    let mut ctx = new_test_context();

    let mut dyn_buf = make_buffer(64, 2);
    let p_dyn = (&raw mut dyn_buf).cast::<c_void>();
    // Dynamic buffer of 64 bytes with 2 suballocations per BufferAllocation.
    register_test_dynamic_buffer(
        p_dyn,
        700,
        64,
        2,
        BufferAllocationFlags::SUBALLOCATE_FROM_ONE_PAGE,
    );
    register_test_buffer(p_dyn, 700, 0, 64);

    // (a) Full-buffer update rotates suballocation (0 -> 1) without allocating a new BufferAllocation.
    let full_a = [0x11_u8; 64];
    ctx.update_subresource(
        p_dyn,
        0,
        ptr::null(),
        full_a.as_ptr().cast::<c_void>(),
        0,
        0,
    );
    let stats_a = dynamic_buffer_stats(p_dyn).expect("dynamic buffer stats");
    assert_eq!(stats_a.current_suballocation, 1);
    assert_eq!(stats_a.suballocation_rotations, 1);
    assert_eq!(stats_a.allocation_renames, 0);
    assert_eq!(&stats_a.current_bytes[..], &full_a[..]);
    assert_eq!(take_recorded_blit_cmds(), Vec::<String>::new());

    // (b) Next full-buffer update wraps suballocations, allocating a fresh BufferAllocation and
    // calling `update_immediate_name` (resets suballocation to 0 and increments allocation_renames).
    let full_b = [0x22_u8; 64];
    ctx.update_subresource(
        p_dyn,
        0,
        ptr::null(),
        full_b.as_ptr().cast::<c_void>(),
        0,
        0,
    );
    let stats_b = dynamic_buffer_stats(p_dyn).expect("dynamic buffer stats");
    assert_eq!(stats_b.current_suballocation, 0);
    assert_eq!(stats_b.suballocation_rotations, 2);
    assert_eq!(stats_b.allocation_renames, 1);
    assert_eq!(&stats_b.current_bytes[..], &full_b[..]);
    assert_eq!(take_recorded_blit_cmds(), Vec::<String>::new());

    // (c) `D3D11_COPY_NO_OVERWRITE` writes in place without rotating suballocation or renaming.
    let patch_c = [0x33_u8; 16];
    let box_c = D3d11Box {
        left: 8,
        top: 0,
        front: 0,
        right: 24,
        bottom: 1,
        back: 1,
    };
    ctx.update_subresource1(
        p_dyn,
        0,
        (&raw const box_c).cast::<c_void>(),
        patch_c.as_ptr().cast::<c_void>(),
        0,
        0,
        D3D11_COPY_NO_OVERWRITE,
    );
    let stats_c = dynamic_buffer_stats(p_dyn).expect("dynamic buffer stats");
    assert_eq!(stats_c.current_suballocation, 0);
    assert_eq!(stats_c.suballocation_rotations, 2);
    assert_eq!(stats_c.allocation_renames, 1);
    assert_eq!(&stats_c.current_bytes[8..24], &patch_c[..]);
    assert_eq!(stats_c.current_bytes[0], 0x22);
    assert_eq!(take_recorded_blit_cmds(), Vec::<String>::new());

    // Also verify `D3D11_COPY_DISCARD` on a partial box rotates suballocation.
    let patch_discard = [0x44_u8; 16];
    ctx.update_subresource1(
        p_dyn,
        0,
        (&raw const box_c).cast::<c_void>(),
        patch_discard.as_ptr().cast::<c_void>(),
        0,
        0,
        D3D11_COPY_DISCARD,
    );
    let stats_disc = dynamic_buffer_stats(p_dyn).expect("dynamic buffer stats");
    assert_eq!(stats_disc.current_suballocation, 1);
    assert_eq!(stats_disc.suballocation_rotations, 3);
    assert_eq!(&stats_disc.current_bytes[8..24], &patch_discard[..]);
    assert_eq!(take_recorded_blit_cmds(), Vec::<String>::new());

    // (d) Partial update WITHOUT `D3D11_COPY_DISCARD` or `D3D11_COPY_NO_OVERWRITE` goes through
    // the staging-buffer blit path instead of discarding!
    set_next_staging(85, 256);
    let patch_d = [0x55_u8; 16];
    ctx.update_subresource1(
        p_dyn,
        0,
        (&raw const box_c).cast::<c_void>(),
        patch_d.as_ptr().cast::<c_void>(),
        0,
        0,
        0,
    );
    let stats_d = dynamic_buffer_stats(p_dyn).expect("dynamic buffer stats");
    assert_eq!(stats_d.current_suballocation, 1);
    assert_eq!(stats_d.suballocation_rotations, 3);
    assert_eq!(stats_d.allocation_renames, 1);
    let blit_d = take_recorded_blit_cmds();
    assert_eq!(
        blit_d,
        vec!["blit 1 src=#85 src_offset=256 dst=#700 dst_offset=8 copy_length=16".to_owned()]
    );
}

/// Implementation of `update::tests::contiguous_3d_honours_depth_pitch`.
fn run_contiguous_3d_honours_depth_pitch() {
    clear_test_state();
    let mut ctx = new_test_context();

    // 3D texture 4x2x3 R8_UNORM (EffectiveBytesPerRow = 4, EffectiveRows = 2 => bytes_per_depth_slice = 8).
    // SrcRowPitch == EffectiveBytesPerRow (4), but SrcDepthPitch = 16 > bytes_per_depth_slice (8).
    let mut tex3d = make_tex3d(4, 2, 3, 1, DXGI_FORMAT_R8_UNORM);
    let p_tex3d = (&raw mut tex3d).cast::<c_void>();
    register_test_texture(p_tex3d, 800, DXGI_FORMAT_R8_UNORM, 4, 2);
    set_next_staging(85, 0);

    let mut src_bytes = [0_u8; 48];
    // Depth slice 0 at [0..8], padding at [8..16]
    src_bytes[0..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    src_bytes[8..16].copy_from_slice(&[0xff; 8]);
    // Depth slice 1 at [16..24], padding at [24..32]
    src_bytes[16..24].copy_from_slice(&[11, 12, 13, 14, 15, 16, 17, 18]);
    src_bytes[24..32].copy_from_slice(&[0xff; 8]);
    // Depth slice 2 at [32..40], padding at [40..48]
    src_bytes[32..40].copy_from_slice(&[21, 22, 23, 24, 25, 26, 27, 28]);
    src_bytes[40..48].copy_from_slice(&[0xff; 8]);

    ctx.update_subresource(
        p_tex3d,
        0,
        ptr::null(),
        src_bytes.as_ptr().cast::<c_void>(),
        4,
        16,
    );

    let staged = staging_slice_bytes(85, 0, 24);
    assert_eq!(
        staged,
        vec![
            1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 13, 14, 15, 16, 17, 18, 21, 22, 23, 24, 25, 26, 27, 28,
        ]
    );
    let blit = take_recorded_blit_cmds();
    assert_eq!(
        blit,
        vec![
            "blit 2 src=#85 src_offset=0 bytes_per_row=4 bytes_per_image=8 size.width=4 size.height=2 size.depth=3 dst=#800 slice=0 level=0 origin.x=0 origin.y=0 origin.z=0".to_owned()
        ]
    );
}

mod update {
    pub mod tests {
        #[test]
        fn dynamic_update_paths_as_cpp() {
            super::super::run_dynamic_update_paths_as_cpp();
        }

        #[test]
        fn contiguous_3d_honours_depth_pitch() {
            super::super::run_contiguous_3d_honours_depth_pitch();
        }
    }
}
