//! `dxmt-replay.exe` and `dxmt-replay`: rebuilds a game's recorded D3D12 pipelines.
//!
//! Reads a `<exe>.pipelines` (`DXMTPRC1`) stream written by `d3d12`'s pipeline recorder, sets
//! `DXMT_CACHE_EXE=<exe>` and clears `DXMT_PIPELINE_RECORD`, loads `d3d12.dll` (or `d3d12.dylib`
//! natively), and replays every recorded root signature and graphics or compute pipeline state so
//! `d3d12`'s on-disk shader and pipeline caches are populated ahead of time.

use core::{
    ffi::{c_char, c_void},
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
use std::{
    ffi::OsString,
    io::{self, Write},
    path::Path,
    time::Instant,
};

use rustc_hash::FxHashMap;

/// File magic at the start of every `.pipelines` recording.
pub const MAGIC: [u8; 8] = *b"DXMTPRC1";

/// Byte size of each record header (`kind: u32`, `size: u32`, `sum: u64`, `id: [u8; 20]`).
pub const HEADER_SIZE: usize = 36;

/// 20-byte SHA-1 content digest identifying a blob or pipeline record.
pub type Sha1Digest = [u8; 20];

/// All-zero digest marking an absent optional blob (such as an omitted shader stage).
pub const NONE_DIGEST: Sha1Digest = [0u8; 20];

/// `D3D_FEATURE_LEVEL_11_0` (`0xb000`).
const D3D_FEATURE_LEVEL_11_0: u32 = 0xb000;

/// `D3D12_IA_VERTEX_INPUT_STRUCTURE_ELEMENT_COUNT`.
const MAX_INPUT_ELEMENTS: u32 = 32;

/// Maximum recorded semantic-name length accepted by the parser.
const MAX_SEMANTIC_NAME_LEN: u32 = 256;

/// COM interface identifier (`GUID`).
#[repr(C)]
pub struct Guid {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

/// `IID_ID3D12Device` (`189819f1-1db6-4b57-be54-1821339b85f7`).
pub const IID_ID3D12_DEVICE: Guid = Guid {
    data1: 0x1898_19f1,
    data2: 0x1db6,
    data3: 0x4b57,
    data4: [0xbe, 0x54, 0x18, 0x21, 0x33, 0x9b, 0x85, 0xf7],
};

/// `IID_ID3D12RootSignature` (`c54a6b66-72df-4ee8-8be5-a946a1429214`).
pub const IID_ID3D12_ROOT_SIGNATURE: Guid = Guid {
    data1: 0xc54a_6b66,
    data2: 0x72df,
    data3: 0x4ee8,
    data4: [0x8b, 0xe5, 0xa9, 0x46, 0xa1, 0x42, 0x92, 0x14],
};

/// `IID_ID3D12PipelineState` (`765a30f3-f624-4c6f-a828-ace948622445`).
pub const IID_ID3D12_PIPELINE_STATE: Guid = Guid {
    data1: 0x765a_30f3,
    data2: 0xf624,
    data3: 0x4c6f,
    data4: [0xa8, 0x28, 0xac, 0xe9, 0x48, 0x62, 0x24, 0x45],
};

/// Record kind tag in a `.pipelines` header.
#[derive(Debug, PartialEq, Eq)]
pub enum Kind {
    Blob = 1,
    Graphics = 2,
    Compute = 3,
}

/// `D3D12_SHADER_BYTECODE`.
#[repr(C)]
pub struct D3d12ShaderBytecode {
    pub shader_bytecode: *const c_void,
    pub bytecode_length: usize,
}

impl Default for D3d12ShaderBytecode {
    fn default() -> Self {
        Self {
            shader_bytecode: ptr::null(),
            bytecode_length: 0,
        }
    }
}

/// `D3D12_STREAM_OUTPUT_DESC`.
#[repr(C)]
pub struct D3d12StreamOutputDesc {
    pub so_declaration: *const c_void,
    pub num_entries: u32,
    pub buffer_strides: *const u32,
    pub num_strides: u32,
    pub rasterized_stream: u32,
}

impl Default for D3d12StreamOutputDesc {
    fn default() -> Self {
        Self {
            so_declaration: ptr::null(),
            num_entries: 0,
            buffer_strides: ptr::null(),
            num_strides: 0,
            rasterized_stream: 0,
        }
    }
}

/// `D3D12_RENDER_TARGET_BLEND_DESC`.
#[derive(Default)]
#[repr(C)]
pub struct D3d12RenderTargetBlendDesc {
    pub blend_enable: i32,
    pub logic_op_enable: i32,
    pub src_blend: u32,
    pub dest_blend: u32,
    pub blend_op: u32,
    pub src_blend_alpha: u32,
    pub dest_blend_alpha: u32,
    pub blend_op_alpha: u32,
    pub logic_op: u32,
    pub render_target_write_mask: u8,
}

/// `D3D12_BLEND_DESC`.
#[derive(Default)]
#[repr(C)]
pub struct D3d12BlendDesc {
    pub alpha_to_coverage_enable: i32,
    pub independent_blend_enable: i32,
    pub render_target: [D3d12RenderTargetBlendDesc; 8],
}

/// `D3D12_RASTERIZER_DESC`.
#[derive(Default)]
#[repr(C)]
pub struct D3d12RasterizerDesc {
    pub fill_mode: u32,
    pub cull_mode: u32,
    pub front_counter_clockwise: i32,
    pub depth_bias: i32,
    pub depth_bias_clamp: f32,
    pub slope_scaled_depth_bias: f32,
    pub depth_clip_enable: i32,
    pub multisample_enable: i32,
    pub antialiased_line_enable: i32,
    pub forced_sample_count: u32,
    pub conservative_raster: u32,
}

/// `D3D12_DEPTH_STENCILOP_DESC`.
#[derive(Default)]
#[repr(C)]
pub struct D3d12DepthStencilOpDesc {
    pub stencil_fail_op: u32,
    pub stencil_depth_fail_op: u32,
    pub stencil_pass_op: u32,
    pub stencil_func: u32,
}

/// `D3D12_DEPTH_STENCIL_DESC`.
#[derive(Default)]
#[repr(C)]
pub struct D3d12DepthStencilDesc {
    pub depth_enable: i32,
    pub depth_write_mask: u32,
    pub depth_func: u32,
    pub stencil_enable: i32,
    pub stencil_read_mask: u8,
    pub stencil_write_mask: u8,
    pub front_face: D3d12DepthStencilOpDesc,
    pub back_face: D3d12DepthStencilOpDesc,
}

/// `D3D12_INPUT_ELEMENT_DESC`.
#[repr(C)]
pub struct D3d12InputElementDesc {
    pub semantic_name: *const c_char,
    pub semantic_index: u32,
    pub format: u32,
    pub input_slot: u32,
    pub aligned_byte_offset: u32,
    pub input_slot_class: u32,
    pub instance_data_step_rate: u32,
}

/// `D3D12_INPUT_LAYOUT_DESC`.
#[repr(C)]
pub struct D3d12InputLayoutDesc {
    pub input_element_descs: *const D3d12InputElementDesc,
    pub num_elements: u32,
}

impl Default for D3d12InputLayoutDesc {
    fn default() -> Self {
        Self {
            input_element_descs: ptr::null(),
            num_elements: 0,
        }
    }
}

/// `DXGI_SAMPLE_DESC`.
#[derive(Default)]
#[repr(C)]
pub struct DxgiSampleDesc {
    pub count: u32,
    pub quality: u32,
}

/// `D3D12_CACHED_PIPELINE_STATE`.
#[repr(C)]
pub struct D3d12CachedPipelineState {
    pub cached_blob: *const c_void,
    pub cached_blob_size_in_bytes: usize,
}

impl Default for D3d12CachedPipelineState {
    fn default() -> Self {
        Self {
            cached_blob: ptr::null(),
            cached_blob_size_in_bytes: 0,
        }
    }
}

/// `D3D12_GRAPHICS_PIPELINE_STATE_DESC`.
#[derive(Default)]
#[repr(C)]
pub struct D3d12GraphicsPipelineStateDesc {
    pub root_signature: *mut c_void,
    pub vs: D3d12ShaderBytecode,
    pub ps: D3d12ShaderBytecode,
    pub ds: D3d12ShaderBytecode,
    pub hs: D3d12ShaderBytecode,
    pub gs: D3d12ShaderBytecode,
    pub stream_output: D3d12StreamOutputDesc,
    pub blend_state: D3d12BlendDesc,
    pub sample_mask: u32,
    pub rasterizer_state: D3d12RasterizerDesc,
    pub depth_stencil_state: D3d12DepthStencilDesc,
    pub input_layout: D3d12InputLayoutDesc,
    pub ib_strip_cut_value: u32,
    pub primitive_topology_type: u32,
    pub num_render_targets: u32,
    pub rtv_formats: [u32; 8],
    pub dsv_format: u32,
    pub sample_desc: DxgiSampleDesc,
    pub node_mask: u32,
    pub cached_pso: D3d12CachedPipelineState,
    pub flags: u32,
}

/// `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
#[repr(C)]
pub struct D3d12ComputePipelineStateDesc {
    pub root_signature: *mut c_void,
    pub cs: D3d12ShaderBytecode,
    pub node_mask: u32,
    pub cached_pso: D3d12CachedPipelineState,
    pub flags: u32,
}

impl Default for D3d12ComputePipelineStateDesc {
    fn default() -> Self {
        Self {
            root_signature: ptr::null_mut(),
            cs: D3d12ShaderBytecode::default(),
            node_mask: 0,
            cached_pso: D3d12CachedPipelineState::default(),
            flags: 0,
        }
    }
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<D3d12GraphicsPipelineStateDesc>() == 656);
    assert!(size_of::<D3d12ComputePipelineStateDesc>() == 56);
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(size_of::<D3d12GraphicsPipelineStateDesc>() == 572);
    assert!(size_of::<D3d12ComputePipelineStateDesc>() == 28);
};

/// Decoded `Kind::Graphics` payload.
#[derive(Default)]
pub struct GraphicsRecord {
    pub root: Sha1Digest,
    pub vs: Sha1Digest,
    pub ps: Sha1Digest,
    pub gs: Sha1Digest,
    pub names: Vec<Vec<u8>>,
    pub elements: Vec<D3d12InputElementDesc>,
    pub desc: D3d12GraphicsPipelineStateDesc,
}

/// Decoded `Kind::Compute` payload.
#[derive(Default)]
pub struct ComputeRecord {
    pub root: Sha1Digest,
    pub cs: Sha1Digest,
    pub desc: D3d12ComputePipelineStateDesc,
}

/// Parsed `.pipelines` file contents.
#[derive(Default)]
pub struct Recording {
    pub blobs: FxHashMap<Sha1Digest, Vec<u8>>,
    pub pipelines: Vec<(Kind, Vec<u8>)>,
    pub bad: u64,
}

/// FNV-1a 64-bit checksum of `data`.
#[must_use]
pub fn checksum(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let rest = self.data.get(self.at..)?;
        let slice = rest.get(..n)?;
        self.at += n;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        let b = self.take(1)?;
        b.first().copied()
    }

    fn u32(&mut self) -> Option<u32> {
        let b: [u8; 4] = self.take(4)?.try_into().ok()?;
        Some(u32::from_le_bytes(b))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_bits(self.u32()?))
    }

    fn digest(&mut self) -> Option<Sha1Digest> {
        self.take(20)?.try_into().ok()
    }
}

/// Deserializes a `Kind::Graphics` payload into a [`GraphicsRecord`].
#[must_use]
pub fn parse_graphics(data: &[u8]) -> Option<GraphicsRecord> {
    let mut r = Reader::new(data);
    let mut out = GraphicsRecord {
        root: r.digest()?,
        vs: r.digest()?,
        ps: r.digest()?,
        gs: r.digest()?,
        ..GraphicsRecord::default()
    };
    out.desc.blend_state.alpha_to_coverage_enable = r.u32()?.cast_signed();
    out.desc.blend_state.independent_blend_enable = r.u32()?.cast_signed();
    for rt in &mut out.desc.blend_state.render_target {
        rt.blend_enable = r.u32()?.cast_signed();
        rt.logic_op_enable = r.u32()?.cast_signed();
        rt.src_blend = r.u32()?;
        rt.dest_blend = r.u32()?;
        rt.blend_op = r.u32()?;
        rt.src_blend_alpha = r.u32()?;
        rt.dest_blend_alpha = r.u32()?;
        rt.blend_op_alpha = r.u32()?;
        rt.logic_op = r.u32()?;
        rt.render_target_write_mask = r.u8()?;
    }
    out.desc.sample_mask = r.u32()?;
    out.desc.rasterizer_state.fill_mode = r.u32()?;
    out.desc.rasterizer_state.cull_mode = r.u32()?;
    out.desc.rasterizer_state.front_counter_clockwise = r.u32()?.cast_signed();
    out.desc.rasterizer_state.depth_bias = r.u32()?.cast_signed();
    out.desc.rasterizer_state.depth_bias_clamp = r.f32()?;
    out.desc.rasterizer_state.slope_scaled_depth_bias = r.f32()?;
    out.desc.rasterizer_state.depth_clip_enable = r.u32()?.cast_signed();
    out.desc.rasterizer_state.multisample_enable = r.u32()?.cast_signed();
    out.desc.rasterizer_state.antialiased_line_enable = r.u32()?.cast_signed();
    out.desc.rasterizer_state.forced_sample_count = r.u32()?;
    out.desc.rasterizer_state.conservative_raster = r.u32()?;
    out.desc.depth_stencil_state.depth_enable = r.u32()?.cast_signed();
    out.desc.depth_stencil_state.depth_write_mask = r.u32()?;
    out.desc.depth_stencil_state.depth_func = r.u32()?;
    out.desc.depth_stencil_state.stencil_enable = r.u32()?.cast_signed();
    out.desc.depth_stencil_state.stencil_read_mask = r.u8()?;
    out.desc.depth_stencil_state.stencil_write_mask = r.u8()?;
    for face in [
        &mut out.desc.depth_stencil_state.front_face,
        &mut out.desc.depth_stencil_state.back_face,
    ] {
        face.stencil_fail_op = r.u32()?;
        face.stencil_depth_fail_op = r.u32()?;
        face.stencil_pass_op = r.u32()?;
        face.stencil_func = r.u32()?;
    }
    let elem_count = r.u32()?;
    if elem_count > MAX_INPUT_ELEMENTS {
        return None;
    }
    let count = elem_count as usize;
    out.names.reserve(count);
    out.elements.reserve(count);
    for _ in 0..count {
        let len = r.u32()?;
        if len > MAX_SEMANTIC_NAME_LEN {
            return None;
        }
        let raw = r.take(len as usize)?;
        let mut name = Vec::with_capacity(raw.len() + 1);
        name.extend_from_slice(raw);
        name.push(0);
        out.names.push(name);
        out.elements.push(D3d12InputElementDesc {
            semantic_name: ptr::null(),
            semantic_index: r.u32()?,
            format: r.u32()?,
            input_slot: r.u32()?,
            aligned_byte_offset: r.u32()?,
            input_slot_class: r.u32()?,
            instance_data_step_rate: r.u32()?,
        });
    }
    out.desc.ib_strip_cut_value = r.u32()?;
    out.desc.primitive_topology_type = r.u32()?;
    out.desc.num_render_targets = r.u32()?;
    for rtv in &mut out.desc.rtv_formats {
        *rtv = r.u32()?;
    }
    out.desc.dsv_format = r.u32()?;
    out.desc.sample_desc.count = r.u32()?;
    out.desc.sample_desc.quality = r.u32()?;
    out.desc.node_mask = r.u32()?;
    out.desc.flags = r.u32()?;
    (r.at == data.len()).then_some(out)
}

/// Deserializes a `Kind::Compute` payload into a [`ComputeRecord`].
#[must_use]
pub fn parse_compute(data: &[u8]) -> Option<ComputeRecord> {
    let mut r = Reader::new(data);
    let mut out = ComputeRecord {
        root: r.digest()?,
        cs: r.digest()?,
        ..ComputeRecord::default()
    };
    out.desc.node_mask = r.u32()?;
    out.desc.flags = r.u32()?;
    (r.at == data.len()).then_some(out)
}

/// Parses a `.pipelines` byte buffer into `out`, returning `false` if the file header is absent.
pub fn parse_recording(bytes: &[u8], out: &mut Recording) -> bool {
    let Some(mut rest) = bytes.strip_prefix(&MAGIC) else {
        return false;
    };
    while !rest.is_empty() {
        let Some((hdr, after_hdr)) = rest.split_at_checked(HEADER_SIZE) else {
            out.bad += 1;
            break;
        };
        let kind = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        let size = u32::from_le_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]) as usize;
        let sum = u64::from_le_bytes([
            hdr[8], hdr[9], hdr[10], hdr[11], hdr[12], hdr[13], hdr[14], hdr[15],
        ]);
        let mut id = [0u8; 20];
        id.copy_from_slice(&hdr[16..36]);
        let Some((payload, tail)) = after_hdr.split_at_checked(size) else {
            out.bad += 1;
            break;
        };
        rest = tail;
        let parsed_kind = match kind {
            1 => Kind::Blob,
            2 => Kind::Graphics,
            3 => Kind::Compute,
            _ => {
                out.bad += 1;
                continue;
            }
        };
        if checksum(payload) != sum {
            out.bad += 1;
            continue;
        }
        if parsed_kind == Kind::Blob {
            out.blobs.entry(id).or_insert_with(|| payload.to_vec());
        } else {
            out.pipelines.push((parsed_kind, payload.to_vec()));
        }
    }
    true
}

/// Reads and parses a `.pipelines` file from `path`.
pub fn read_file(path: &Path, out: &mut Recording) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    parse_recording(&bytes, out)
}

/// Extracts the `<exe>` stem from `<dir>/<exe>.pipelines` (accepting `/` and `\` separators).
#[must_use]
pub fn recording_stem(path: &str) -> Option<&str> {
    let name = path.rsplit_once(['\\', '/']).map_or(path, |(_, tail)| tail);
    name.strip_suffix(".pipelines").filter(|s| !s.is_empty())
}

#[repr(C)]
struct IUnknownVtbl {
    query_interface: *const c_void,
    add_ref: *const c_void,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
}

#[repr(C)]
struct ID3D12DeviceVtbl {
    query_interface: *const c_void,
    add_ref: *const c_void,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    get_private_data: *const c_void,
    set_private_data: *const c_void,
    set_private_data_interface: *const c_void,
    set_name: *const c_void,
    get_node_count: *const c_void,
    create_command_queue: *const c_void,
    create_command_allocator: *const c_void,
    create_graphics_pipeline_state: unsafe extern "system" fn(
        *mut c_void,
        *const D3d12GraphicsPipelineStateDesc,
        *const Guid,
        *mut *mut c_void,
    ) -> i32,
    create_compute_pipeline_state: unsafe extern "system" fn(
        *mut c_void,
        *const D3d12ComputePipelineStateDesc,
        *const Guid,
        *mut *mut c_void,
    ) -> i32,
    create_command_list: *const c_void,
    check_feature_support: *const c_void,
    create_descriptor_heap: *const c_void,
    get_descriptor_handle_increment_size: *const c_void,
    create_root_signature: unsafe extern "system" fn(
        *mut c_void,
        u32,
        *const c_void,
        usize,
        *const Guid,
        *mut *mut c_void,
    ) -> i32,
}

type PfnD3d12CreateDevice =
    unsafe extern "system" fn(*mut c_void, u32, *const Guid, *mut *mut c_void) -> i32;

/// Calls `IUnknown::Release` on a non-null COM interface pointer.
///
/// # Safety
///
/// `obj` must be a valid COM interface pointer whose first word points to an `IUnknown`-compatible
/// vtable.
unsafe fn com_release(obj: *mut c_void) {
    // SAFETY: `obj` is a valid COM object with an `IUnknown` vtable at offset 0.
    let vtbl = unsafe { *(obj.cast::<*const IUnknownVtbl>()) };
    // SAFETY: `vtbl` is the object's vtable.
    let release = unsafe { (*vtbl).release };
    // SAFETY: `obj` is the `this` pointer for `Release`.
    unsafe { release(obj) };
}

/// Calls `ID3D12Device::CreateRootSignature`.
///
/// # Safety
///
/// `device` must be a valid `ID3D12Device` pointer and `blob` must be valid for `len` bytes.
unsafe fn device_create_root_signature(
    device: *mut c_void,
    blob: *const c_void,
    len: usize,
) -> *mut c_void {
    let mut out: *mut c_void = ptr::null_mut();
    // SAFETY: `device` is a valid `ID3D12Device` COM pointer.
    let vtbl = unsafe { *(device.cast::<*const ID3D12DeviceVtbl>()) };
    // SAFETY: `vtbl` points to `ID3D12DeviceVtbl`.
    let func = unsafe { (*vtbl).create_root_signature };
    // SAFETY: `device` and `blob` are valid for the call and `out` receives the interface pointer.
    let hr = unsafe {
        func(
            device,
            0,
            blob,
            len,
            &IID_ID3D12_ROOT_SIGNATURE,
            &raw mut out,
        )
    };
    if hr < 0 { ptr::null_mut() } else { out }
}

/// Calls `ID3D12Device::CreateGraphicsPipelineState`.
///
/// # Safety
///
/// `device` must be a valid `ID3D12Device` pointer and `desc` must be a valid descriptor.
unsafe fn device_create_graphics_pso(
    device: *mut c_void,
    desc: &D3d12GraphicsPipelineStateDesc,
) -> *mut c_void {
    let mut out: *mut c_void = ptr::null_mut();
    // SAFETY: `device` is a valid `ID3D12Device` COM pointer.
    let vtbl = unsafe { *(device.cast::<*const ID3D12DeviceVtbl>()) };
    // SAFETY: `vtbl` points to `ID3D12DeviceVtbl`.
    let func = unsafe { (*vtbl).create_graphics_pipeline_state };
    // SAFETY: `device` and `desc` are valid and `out` receives the created pipeline state.
    let hr = unsafe { func(device, desc, &IID_ID3D12_PIPELINE_STATE, &raw mut out) };
    if hr < 0 { ptr::null_mut() } else { out }
}

/// Calls `ID3D12Device::CreateComputePipelineState`.
///
/// # Safety
///
/// `device` must be a valid `ID3D12Device` pointer and `desc` must be a valid descriptor.
unsafe fn device_create_compute_pso(
    device: *mut c_void,
    desc: &D3d12ComputePipelineStateDesc,
) -> *mut c_void {
    let mut out: *mut c_void = ptr::null_mut();
    // SAFETY: `device` is a valid `ID3D12Device` COM pointer.
    let vtbl = unsafe { *(device.cast::<*const ID3D12DeviceVtbl>()) };
    // SAFETY: `vtbl` points to `ID3D12DeviceVtbl`.
    let func = unsafe { (*vtbl).create_compute_pipeline_state };
    // SAFETY: `device` and `desc` are valid and `out` receives the created pipeline state.
    let hr = unsafe { func(device, desc, &IID_ID3D12_PIPELINE_STATE, &raw mut out) };
    if hr < 0 { ptr::null_mut() } else { out }
}

#[cfg(windows)]
fn load_d3d12_create_device() -> Option<PfnD3d12CreateDevice> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }
    const D3D12_DLL: [u16; 10] = [
        b'd' as u16,
        b'3' as u16,
        b'd' as u16,
        b'1' as u16,
        b'2' as u16,
        b'.' as u16,
        b'd' as u16,
        b'l' as u16,
        b'l' as u16,
        0,
    ];
    // SAFETY: `D3D12_DLL` is a NUL-terminated UTF-16 string.
    let module = unsafe { LoadLibraryW(D3D12_DLL.as_ptr()) };
    if module.is_null() {
        return None;
    }
    // SAFETY: `module` is a loaded module handle and the symbol name is NUL-terminated.
    let sym = unsafe { GetProcAddress(module, c"D3D12CreateDevice".as_ptr()) };
    if sym.is_null() {
        return None;
    }
    // SAFETY: `D3D12CreateDevice` has signature `PfnD3d12CreateDevice`.
    Some(unsafe { core::mem::transmute::<*mut c_void, PfnD3d12CreateDevice>(sym) })
}

#[cfg(not(windows))]
fn load_d3d12_create_device() -> Option<PfnD3d12CreateDevice> {
    const RTLD_LAZY: i32 = 1;
    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: i32) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    }
    // SAFETY: literal C string passed to `dlopen`.
    let mut module = unsafe { dlopen(c"@executable_path/d3d12.dylib".as_ptr(), RTLD_LAZY) };
    if module.is_null() {
        // SAFETY: literal C string passed to `dlopen`.
        module = unsafe { dlopen(c"d3d12.dylib".as_ptr(), RTLD_LAZY) };
    }
    if module.is_null() {
        return None;
    }
    // SAFETY: `module` is a valid `dlopen` handle and the symbol name is NUL-terminated.
    let sym = unsafe { dlsym(module, c"D3D12CreateDevice".as_ptr()) };
    if sym.is_null() {
        return None;
    }
    // SAFETY: `D3D12CreateDevice` has signature `PfnD3d12CreateDevice`.
    Some(unsafe { core::mem::transmute::<*mut c_void, PfnD3d12CreateDevice>(sym) })
}

enum JobKind {
    Graphics(Box<GraphicsRecord>),
    Compute(ComputeRecord),
}

struct Job {
    kind: JobKind,
    root_failed: bool,
}

// SAFETY: `Job` descriptors point only into immutable `Recording::blobs`, `GraphicsRecord::names`,
// `GraphicsRecord::elements`, and `ID3D12RootSignature` COM objects, all of which outlive the
// scoped worker threads and are only read concurrently by `ID3D12Device::Create*PipelineState`.
unsafe impl Sync for Job {}

struct DevicePtr(*mut c_void);

// SAFETY: `ID3D12Device` pipeline creation methods are free-threaded per the D3D12 specification.
unsafe impl Sync for DevicePtr {}

fn run_replay(arg_os: &OsString) -> i32 {
    let path_str = arg_os.to_string_lossy();
    let Some(stem) = recording_stem(&path_str) else {
        println!("replay: not a recording");
        let _ = io::stdout().flush();
        return 1;
    };
    // SAFETY: Process startup is single-threaded before `d3d12` is loaded or worker threads spawn.
    unsafe { std::env::set_var("DXMT_CACHE_EXE", stem) };
    // SAFETY: Process startup is single-threaded before `d3d12` is loaded or worker threads spawn.
    unsafe { std::env::remove_var("DXMT_PIPELINE_RECORD") };

    let mut recording = Recording::default();
    if !read_file(Path::new(arg_os), &mut recording) {
        println!("replay: not a recording");
        let _ = io::stdout().flush();
        return 1;
    }

    let t0 = Instant::now();
    let Some(create_device) = load_d3d12_create_device() else {
        println!("replay: no D3D12 device");
        let _ = io::stdout().flush();
        return 1;
    };
    let mut device: *mut c_void = ptr::null_mut();
    // SAFETY: `create_device` is `D3D12CreateDevice` and `device` receives the created interface.
    let hr = unsafe {
        create_device(
            ptr::null_mut(),
            D3D_FEATURE_LEVEL_11_0,
            &IID_ID3D12_DEVICE,
            &raw mut device,
        )
    };
    if hr < 0 || device.is_null() {
        println!("replay: no D3D12 device");
        let _ = io::stdout().flush();
        return 1;
    }

    let blobs = &recording.blobs;
    let has = |id: &Sha1Digest| *id == NONE_DIGEST || blobs.contains_key(id);
    let code = |id: &Sha1Digest| -> D3d12ShaderBytecode {
        if *id == NONE_DIGEST {
            D3d12ShaderBytecode::default()
        } else {
            let b = &blobs[id];
            D3d12ShaderBytecode {
                shader_bytecode: b.as_ptr().cast(),
                bytecode_length: b.len(),
            }
        }
    };

    let mut roots: FxHashMap<Sha1Digest, *mut c_void> = FxHashMap::default();
    let mut jobs: Vec<Job> = Vec::with_capacity(recording.pipelines.len());
    let mut bad = recording.bad;
    let mut graphics: u64 = 0;
    let mut compute: u64 = 0;

    for (kind, payload) in &recording.pipelines {
        match kind {
            Kind::Graphics => {
                let Some(mut g) = parse_graphics(payload) else {
                    bad += 1;
                    continue;
                };
                if !(has(&g.root) && has(&g.vs) && has(&g.ps) && has(&g.gs)) {
                    bad += 1;
                    continue;
                }
                g.desc.vs = code(&g.vs);
                g.desc.ps = code(&g.ps);
                g.desc.gs = code(&g.gs);
                let (root_ptr, root_failed) = if g.root == NONE_DIGEST {
                    (ptr::null_mut(), false)
                } else {
                    let p = *roots.entry(g.root).or_insert_with(|| {
                        let b = &blobs[&g.root];
                        // SAFETY: `device` is valid and `b` is the recorded root signature blob.
                        unsafe { device_create_root_signature(device, b.as_ptr().cast(), b.len()) }
                    });
                    (p, p.is_null())
                };
                for (elem, name) in g.elements.iter_mut().zip(g.names.iter()) {
                    elem.semantic_name = name.as_ptr().cast();
                }
                g.desc.input_layout.input_element_descs = if g.elements.is_empty() {
                    ptr::null()
                } else {
                    g.elements.as_ptr()
                };
                g.desc.input_layout.num_elements =
                    u32::try_from(g.elements.len()).unwrap_or(u32::MAX);
                g.desc.root_signature = root_ptr;
                graphics += 1;
                jobs.push(Job {
                    kind: JobKind::Graphics(Box::new(g)),
                    root_failed,
                });
            }
            Kind::Compute => {
                let Some(mut c) = parse_compute(payload) else {
                    bad += 1;
                    continue;
                };
                if !(has(&c.root) && has(&c.cs)) {
                    bad += 1;
                    continue;
                }
                c.desc.cs = code(&c.cs);
                let (root_ptr, root_failed) = if c.root == NONE_DIGEST {
                    (ptr::null_mut(), false)
                } else {
                    let p = *roots.entry(c.root).or_insert_with(|| {
                        let b = &blobs[&c.root];
                        // SAFETY: `device` is valid and `b` is the recorded root signature blob.
                        unsafe { device_create_root_signature(device, b.as_ptr().cast(), b.len()) }
                    });
                    (p, p.is_null())
                };
                c.desc.root_signature = root_ptr;
                compute += 1;
                jobs.push(Job {
                    kind: JobKind::Compute(c),
                    root_failed,
                });
            }
            Kind::Blob => {}
        }
    }

    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let created = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let nthreads = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(jobs.len());
    let dev = DevicePtr(device);
    let total = jobs.len();

    std::thread::scope(|s| {
        for _ in 0..nthreads {
            s.spawn(|| {
                let dev_ref = &dev;
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else {
                        break;
                    };
                    let pso = if job.root_failed {
                        ptr::null_mut()
                    } else {
                        match &job.kind {
                            // SAFETY: `dev_ref.0` and `g.desc` remain valid for the scoped thread.
                            JobKind::Graphics(g) => unsafe {
                                device_create_graphics_pso(dev_ref.0, &g.desc)
                            },
                            // SAFETY: `dev_ref.0` and `c.desc` remain valid for the scoped thread.
                            JobKind::Compute(c) => unsafe {
                                device_create_compute_pso(dev_ref.0, &c.desc)
                            },
                        }
                    };
                    if pso.is_null() {
                        failed.fetch_add(1, Ordering::Relaxed);
                    } else {
                        // SAFETY: `pso` is a newly created `ID3D12PipelineState` COM object.
                        unsafe { com_release(pso) };
                        created.fetch_add(1, Ordering::Relaxed);
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if (n * 10) / total != ((n - 1) * 10) / total {
                        println!("replay progress {n}/{total}");
                        let _ = io::stdout().flush();
                    }
                }
            });
        }
    });

    for root in roots.into_values() {
        if !root.is_null() {
            // SAFETY: `root` is a valid `ID3D12RootSignature` COM object.
            unsafe { com_release(root) };
        }
    }
    // SAFETY: `device` is a valid `ID3D12Device` COM object.
    unsafe { com_release(device) };

    let ms = t0.elapsed().as_millis();
    println!(
        "replay: {} pipelines ({graphics} graphics, {compute} compute), {} created, {} failed, {bad} bad records, {ms} ms",
        jobs.len(),
        created.load(Ordering::Relaxed),
        failed.load(Ordering::Relaxed),
    );
    let _ = io::stdout().flush();
    0
}

/// The `type_info` vtable under its MSVC decorated name (`std::thread::spawn` landing pad).
#[cfg(windows)]
#[unsafe(export_name = "??_7type_info@@6B@")]
pub static TYPE_INFO_VFTABLE: [usize; 1] = [0];

/// Wide-character entry point for `dxmt-replay.exe` on Windows (`-municode`).
///
/// # Safety
///
/// `args` must point to `count` valid NUL-terminated UTF-16 argument strings provided by the CRT.
#[cfg(windows)]
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn wmain(count: i32, args: *mut *mut u16) -> i32 {
    use std::os::windows::ffi::OsStringExt;
    if count != 2 || args.is_null() {
        println!("usage: dxmt-replay.exe <recording>");
        let _ = io::stdout().flush();
        return 1;
    }
    // SAFETY: `count == 2` and `args` is valid for two pointers.
    let arg_slice = unsafe { core::slice::from_raw_parts(args, 2) };
    let path_ptr = arg_slice[1];
    if path_ptr.is_null() {
        println!("usage: dxmt-replay.exe <recording>");
        let _ = io::stdout().flush();
        return 1;
    }
    let mut len = 0usize;
    loop {
        // SAFETY: `path_ptr` is a NUL-terminated UTF-16 string from the CRT.
        let unit_ptr = unsafe { path_ptr.add(len) };
        // SAFETY: `unit_ptr` points to a valid code unit up to and including the NUL terminator.
        let unit = unsafe { *unit_ptr };
        if unit == 0 {
            break;
        }
        len += 1;
    }
    // SAFETY: `path_ptr` is valid for `len` `u16` code units.
    let units = unsafe { core::slice::from_raw_parts(path_ptr, len) };
    run_replay(&OsString::from_wide(units))
}

/// Narrow-character entry point implementation for native `dxmt-replay` on macOS.
///
/// # Safety
///
/// `args` must point to `count` valid NUL-terminated C strings provided by the system C runtime.
#[cfg(not(windows))]
#[must_use]
pub unsafe extern "C" fn host_main(count: i32, args: *const *const c_char) -> i32 {
    use core::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;
    if count != 2 || args.is_null() {
        println!("usage: dxmt-replay.exe <recording>");
        let _ = io::stdout().flush();
        return 1;
    }
    // SAFETY: `count == 2` and `args` is valid for two pointers.
    let arg_slice = unsafe { core::slice::from_raw_parts(args, 2) };
    let path_ptr = arg_slice[1];
    if path_ptr.is_null() {
        println!("usage: dxmt-replay.exe <recording>");
        let _ = io::stdout().flush();
        return 1;
    }
    // SAFETY: `path_ptr` is a NUL-terminated C string from the system runtime.
    let bytes = unsafe { CStr::from_ptr(path_ptr) }.to_bytes();
    run_replay(&std::ffi::OsStr::from_bytes(bytes).to_os_string())
}

/// Narrow-character entry point for native `dxmt-replay` on macOS.
///
/// # Safety
///
/// `args` must point to `count` valid NUL-terminated C strings provided by the system C runtime.
#[cfg(all(not(windows), not(test)))]
#[unsafe(no_mangle)]
#[must_use]
pub unsafe extern "C" fn main(count: i32, args: *const *const c_char) -> i32 {
    // SAFETY: `count` and `args` come from the system C runtime.
    unsafe { host_main(count, args) }
}

#[cfg(test)]
mod tests;
