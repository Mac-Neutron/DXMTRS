//! `.pipelines` (`DXMTPRC1`) recording writer (`d3d12_pipeline_record.{hpp,cpp}`).

use core::ffi::{CStr, c_char, c_void};
use std::{
    fs::{File, OpenOptions},
    io::{Read as _, Seek as _, SeekFrom, Write as _},
    path::PathBuf,
    sync::{LazyLock, Mutex, PoisonError},
};

use dxmt_util::env::cache_exe_name;
use rustc_hash::FxHashSet;

use crate::{
    abi::pipeline::{ComputePipelineStateDesc, GraphicsPipelineStateDesc},
    shader_cache::Sha1Digest,
};

/// File magic (`"DXMTPRC1"`, `d3d12_pipeline_record.hpp:34`).
pub const MAGIC: [u8; 8] = *b"DXMTPRC1";

/// Record header byte length (`kHeaderSize`, `d3d12_pipeline_record.hpp:35`).
pub const HEADER_SIZE: usize = 36;

/// Record kind (`dxmt::record::Kind`, `d3d12_pipeline_record.hpp:36`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Kind {
    Blob = 1,
    Graphics = 2,
    Compute = 3,
}

/// A shader or root-signature blob referenced by a recorded pipeline (`dxmt::record::Blob`,
///
/// `d3d12_pipeline_record.hpp:39-43`).
#[derive(Clone, Copy)]
pub struct Blob<'a> {
    pub id: Sha1Digest,
    pub data: Option<&'a [u8]>,
}

impl Blob<'_> {
    /// An empty blob reference (`id = 0`, `data = None`).
    pub const NONE: Self = Self {
        id: [0; 20],
        data: None,
    };
}

/// FNV-1a 64-bit checksum of a record payload (`Checksum`, `d3d12_pipeline_record.cpp:36`).
#[must_use]
pub fn checksum(payload: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for &b in payload {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Computes the 20-byte SHA-1 digest of `payload` (`sha1_hash::compute`).
#[must_use]
pub fn sha1(payload: &[u8]) -> Sha1Digest {
    let mut state = [
        0x6745_2301_u32,
        0xefcd_ab89_u32,
        0x98ba_dcfe_u32,
        0x1032_5476_u32,
        0xc3d2_e1f0_u32,
    ];

    let mut process_block = |block: &[u8; 64]| {
        let mut words = [0u32; 80];
        for (i, &chunk) in block.as_chunks::<4>().0.iter().enumerate() {
            words[i] = u32::from_be_bytes(chunk);
        }
        for i in 16..80 {
            words[i] = (words[i - 3] ^ words[i - 8] ^ words[i - 14] ^ words[i - 16]).rotate_left(1);
        }
        let mut s0 = state[0];
        let mut s1 = state[1];
        let mut s2 = state[2];
        let mut s3 = state[3];
        let mut s4 = state[4];
        for (i, &word) in words.iter().enumerate() {
            let (func, rcon) = match i {
                0..=19 => ((s1 & s2) | ((!s1) & s3), 0x5a82_7999_u32),
                20..=39 => (s1 ^ s2 ^ s3, 0x6ed9_eba1_u32),
                40..=59 => ((s1 & s2) | (s1 & s3) | (s2 & s3), 0x8f1b_bcdc_u32),
                _ => (s1 ^ s2 ^ s3, 0xca62_c1d6_u32),
            };
            let temp = s0
                .rotate_left(5)
                .wrapping_add(func)
                .wrapping_add(s4)
                .wrapping_add(rcon)
                .wrapping_add(word);
            s4 = s3;
            s3 = s2;
            s2 = s1.rotate_left(30);
            s1 = s0;
            s0 = temp;
        }
        state[0] = state[0].wrapping_add(s0);
        state[1] = state[1].wrapping_add(s1);
        state[2] = state[2].wrapping_add(s2);
        state[3] = state[3].wrapping_add(s3);
        state[4] = state[4].wrapping_add(s4);
    };

    let (blocks, rem) = payload.as_chunks::<64>();
    for block in blocks {
        process_block(block);
    }
    let mut tail = [0u8; 128];
    tail[..rem.len()].copy_from_slice(rem);
    tail[rem.len()] = 0x80;
    let total_blocks = if rem.len() < 56 { 1 } else { 2 };
    let bit_len = (payload.len() as u64).wrapping_mul(8);
    let len_offset = total_blocks * 64 - 8;
    tail[len_offset..len_offset + 8].copy_from_slice(&bit_len.to_be_bytes());
    for block in tail.as_chunks::<64>().0.iter().take(total_blocks) {
        process_block(block);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&state[0].to_be_bytes());
    out[4..8].copy_from_slice(&state[1].to_be_bytes());
    out[8..12].copy_from_slice(&state[2].to_be_bytes());
    out[12..16].copy_from_slice(&state[3].to_be_bytes());
    out[16..20].copy_from_slice(&state[4].to_be_bytes());
    out
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    put_u32(out, v.to_bits());
}

fn put_sha1(out: &mut Vec<u8>, id: &Sha1Digest) {
    out.extend_from_slice(id);
}

unsafe fn put_str(out: &mut Vec<u8>, s: *const u8) {
    let bytes = if s.is_null() {
        &[][..]
    } else {
        // SAFETY: Caller guarantees non-null `s` points to a NUL-terminated C string.
        unsafe { CStr::from_ptr(s.cast::<c_char>()) }.to_bytes()
    };
    let n = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    put_u32(out, n);
    out.extend_from_slice(bytes);
}

/// Serializes a `D3D12_GRAPHICS_PIPELINE_STATE_DESC` field-by-field (`SerializeGraphics`,
///
/// `d3d12_pipeline_record.cpp:64-169`).
///
/// # Safety
///
/// `desc.input_layout.input_element_descs` must be valid for `desc.input_layout.num_elements`
/// elements, each with a null or NUL-terminated `semantic_name`.
#[must_use]
pub unsafe fn serialize_graphics(
    desc: &GraphicsPipelineStateDesc,
    root: &Sha1Digest,
    vs: &Sha1Digest,
    ps: &Sha1Digest,
    gs: &Sha1Digest,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    put_sha1(&mut out, root);
    put_sha1(&mut out, vs);
    put_sha1(&mut out, ps);
    put_sha1(&mut out, gs);

    let b = &desc.blend_state;
    put_u32(&mut out, b.alpha_to_coverage_enable.cast_unsigned());
    put_u32(&mut out, b.independent_blend_enable.cast_unsigned());
    for rt in &b.render_target {
        put_u32(&mut out, rt.blend_enable.cast_unsigned());
        put_u32(&mut out, rt.logic_op_enable.cast_unsigned());
        put_u32(&mut out, rt.src_blend);
        put_u32(&mut out, rt.dest_blend);
        put_u32(&mut out, rt.blend_op);
        put_u32(&mut out, rt.src_blend_alpha);
        put_u32(&mut out, rt.dest_blend_alpha);
        put_u32(&mut out, rt.blend_op_alpha);
        put_u32(&mut out, rt.logic_op);
        out.push(rt.render_target_write_mask);
    }
    put_u32(&mut out, desc.sample_mask);

    let r = &desc.rasterizer_state;
    put_u32(&mut out, r.fill_mode);
    put_u32(&mut out, r.cull_mode);
    put_u32(&mut out, r.front_counter_clockwise.cast_unsigned());
    put_u32(&mut out, r.depth_bias.cast_unsigned());
    put_f32(&mut out, r.depth_bias_clamp);
    put_f32(&mut out, r.slope_scaled_depth_bias);
    put_u32(&mut out, r.depth_clip_enable.cast_unsigned());
    put_u32(&mut out, r.multisample_enable.cast_unsigned());
    put_u32(&mut out, r.antialiased_line_enable.cast_unsigned());
    put_u32(&mut out, r.forced_sample_count);
    put_u32(&mut out, r.conservative_raster);

    let ds = &desc.depth_stencil_state;
    put_u32(&mut out, ds.depth_enable.cast_unsigned());
    put_u32(&mut out, ds.depth_write_mask);
    put_u32(&mut out, ds.depth_func);
    put_u32(&mut out, ds.stencil_enable.cast_unsigned());
    out.push(ds.stencil_read_mask);
    out.push(ds.stencil_write_mask);
    for f in [&ds.front_face, &ds.back_face] {
        put_u32(&mut out, f.stencil_fail_op);
        put_u32(&mut out, f.stencil_depth_fail_op);
        put_u32(&mut out, f.stencil_pass_op);
        put_u32(&mut out, f.stencil_func);
    }

    let il_count = if desc.input_layout.input_element_descs.is_null() {
        0
    } else {
        desc.input_layout.num_elements
    };
    put_u32(&mut out, il_count);
    for i in 0..(il_count as usize) {
        // SAFETY: `input_element_descs` is non-null and valid for `il_count` elements.
        let el_ptr = unsafe { desc.input_layout.input_element_descs.add(i) };
        // SAFETY: `el_ptr` points to the `i`-th `InputElementDesc`.
        let el = unsafe { &*el_ptr };
        // SAFETY: `el.semantic_name` is null or a NUL-terminated C string per caller contract.
        unsafe { put_str(&mut out, el.semantic_name) };
        put_u32(&mut out, el.semantic_index);
        put_u32(&mut out, el.format);
        put_u32(&mut out, el.input_slot);
        put_u32(&mut out, el.aligned_byte_offset);
        put_u32(&mut out, el.input_slot_class);
        put_u32(&mut out, el.instance_data_step_rate);
    }

    put_u32(&mut out, desc.ib_strip_cut_value);
    put_u32(&mut out, desc.primitive_topology_type);
    put_u32(&mut out, desc.num_render_targets);
    for &fmt in &desc.rtv_formats {
        put_u32(&mut out, fmt);
    }
    put_u32(&mut out, desc.dsv_format);
    put_u32(&mut out, desc.sample_desc.count);
    put_u32(&mut out, desc.sample_desc.quality);
    put_u32(&mut out, desc.node_mask);
    put_u32(&mut out, desc.flags);
    out
}

/// Serializes a `D3D12_COMPUTE_PIPELINE_STATE_DESC` field-by-field (`SerializeCompute`,
///
/// `d3d12_pipeline_record.cpp:172-186`).
#[must_use]
pub fn serialize_compute(
    desc: &ComputePipelineStateDesc,
    root: &Sha1Digest,
    cs: &Sha1Digest,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(48);
    put_sha1(&mut out, root);
    put_sha1(&mut out, cs);
    put_u32(&mut out, desc.node_mask);
    put_u32(&mut out, desc.flags);
    out
}

fn append_record(out: &mut Vec<u8>, kind: Kind, id: &Sha1Digest, payload: &[u8]) {
    let size = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    put_u32(out, kind as u32);
    put_u32(out, size);
    put_u64(out, checksum(payload));
    put_sha1(out, id);
    out.extend_from_slice(payload);
}

static RECORD_DIR: LazyLock<String> = LazyLock::new(|| {
    let v = std::env::var("DXMT_PIPELINE_RECORD").unwrap_or_default();
    if v.is_empty() || v == "0" {
        return String::new();
    }
    let mut d = if v == "1" {
        let cache_path = std::env::var("DXMT_SHADER_CACHE_PATH").unwrap_or_default();
        if cache_path.is_empty() {
            dxmt_core::log::warn(
                "DXMT_PIPELINE_RECORD=1 requires DXMT_SHADER_CACHE_PATH to be set",
            );
            return String::new();
        }
        cache_path
    } else if v.starts_with('/') || v.as_bytes().get(1) == Some(&b':') {
        v
    } else {
        dxmt_core::log::warn(&format!(
            "DXMT_PIPELINE_RECORD must be 1 or an absolute path, got '{v}'"
        ));
        return String::new();
    };
    if cache_exe_name().is_empty() {
        dxmt_core::log::warn("DXMT_PIPELINE_RECORD: executable name is empty");
        return String::new();
    }
    if let Err(err) = std::fs::create_dir_all(&d) {
        dxmt_core::log::warn(&format!(
            "DXMT_PIPELINE_RECORD: failed to create '{d}': {err}"
        ));
        return String::new();
    }
    while d.len() > 1 && (d.ends_with('/') || d.ends_with('\\')) {
        d.pop();
    }
    d
});

static RECORDER: LazyLock<Mutex<Recorder>> = LazyLock::new(|| {
    let dir = RECORD_DIR.as_str();
    let path = PathBuf::from(format!("{dir}/{}.pipelines", cache_exe_name()));
    Mutex::new(Recorder::new(path))
});

/// Returns whether `DXMT_PIPELINE_RECORD` is active (`RecordingOn`,
///
/// `d3d12_pipeline_record.cpp:240`).
#[must_use]
pub fn recording_on() -> bool {
    !RECORD_DIR.is_empty()
}

/// Stateful `.pipelines` file writer (`Recorder`, `d3d12_pipeline_record.cpp:245-327`).
pub struct Recorder {
    path: PathBuf,
    file: Option<File>,
    checked: bool,
    off: bool,
    ids: FxHashSet<Sha1Digest>,
}

impl Recorder {
    /// Creates a new recorder targeting `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: None,
            checked: false,
            off: false,
            ids: FxHashSet::default(),
        }
    }

    fn turn_off(&mut self, why: &str) -> bool {
        if !self.off {
            dxmt_core::log::warn(&format!(
                "DXMT_PIPELINE_RECORD: {why} '{}'",
                self.path.display()
            ));
            self.off = true;
        }
        self.file = None;
        false
    }

    fn ready(&mut self) -> bool {
        if self.off {
            return false;
        }
        if self.checked {
            return self.file.is_some();
        }
        self.checked = true;
        let Ok(mut file) = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.path)
        else {
            return self.turn_off("can't open");
        };
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() || bytes.len() > 0x7fff_ffff {
            return self.turn_off("can't read the recording");
        }
        let mut end = 0usize;
        if bytes.starts_with(&MAGIC) {
            end = MAGIC.len();
            while bytes.len() - end >= HEADER_SIZE {
                let kind = u32::from_le_bytes([
                    bytes[end],
                    bytes[end + 1],
                    bytes[end + 2],
                    bytes[end + 3],
                ]);
                let size = u32::from_le_bytes([
                    bytes[end + 4],
                    bytes[end + 5],
                    bytes[end + 6],
                    bytes[end + 7],
                ]);
                if !(1..=3).contains(&kind) || (size as usize) > bytes.len() - end - HEADER_SIZE {
                    break;
                }
                let mut id = [0u8; 20];
                id.copy_from_slice(&bytes[end + 16..end + 36]);
                self.ids.insert(id);
                end += HEADER_SIZE + (size as usize);
            }
        }
        if file.set_len(end as u64).is_err() || file.seek(SeekFrom::Start(end as u64)).is_err() {
            return self.turn_off("can't cut the recording's torn tail");
        }
        if end == 0 && file.write_all(&MAGIC).is_err() {
            return self.turn_off("can't write the recording");
        }
        let _ = file.flush();
        self.file = Some(file);
        true
    }

    /// Appends `payload` of `kind` and any unrecorded `blobs` to the recording file (`Append`,
    ///
    /// `d3d12_pipeline_record.cpp:299-327`).
    pub fn append(&mut self, kind: Kind, payload: &[u8], blobs: &[Blob<'_>]) {
        let id = sha1(payload);
        if !self.ready() || self.ids.contains(&id) {
            return;
        }
        let mut buf = Vec::new();
        for b in blobs {
            if let Some(data) = b.data.filter(|d| !d.is_empty())
                && self.ids.insert(b.id)
            {
                append_record(&mut buf, Kind::Blob, &b.id, data);
            }
        }
        append_record(&mut buf, kind, &id, payload);
        let ok = self
            .file
            .as_mut()
            .is_some_and(|f| f.write_all(&buf).is_ok() && f.flush().is_ok());
        if !ok {
            self.turn_off("write failed on");
            return;
        }
        self.ids.insert(id);
    }
}

/// Appends a graphics pipeline and any unrecorded blobs to `<DXMT_PIPELINE_RECORD>/<exe>.pipelines`
///
/// (`RecordGraphics`, `d3d12_pipeline_record.cpp:337`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_GRAPHICS_PIPELINE_STATE_DESC`.
pub unsafe fn record_graphics(
    desc: *const c_void,
    root: Blob<'_>,
    vs: Blob<'_>,
    ps: Blob<'_>,
    gs: Blob<'_>,
) {
    if !recording_on() || desc.is_null() {
        return;
    }
    // SAFETY: Caller guarantees non-null `desc` is a valid `GraphicsPipelineStateDesc`.
    let desc_ref = unsafe { &*desc.cast::<GraphicsPipelineStateDesc>() };
    // SAFETY: `desc_ref.input_layout` is part of the valid `GraphicsPipelineStateDesc`.
    let bytes = unsafe { serialize_graphics(desc_ref, &root.id, &vs.id, &ps.id, &gs.id) };
    RECORDER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .append(Kind::Graphics, &bytes, &[root, vs, ps, gs]);
}

/// Appends a compute pipeline and any unrecorded blobs to `<DXMT_PIPELINE_RECORD>/<exe>.pipelines`
///
/// (`RecordCompute`, `d3d12_pipeline_record.cpp:361`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
pub unsafe fn record_compute(desc: *const c_void, root: Blob<'_>, cs: Blob<'_>) {
    if !recording_on() || desc.is_null() {
        return;
    }
    // SAFETY: Caller guarantees non-null `desc` is a valid `ComputePipelineStateDesc`.
    let desc_ref = unsafe { &*desc.cast::<ComputePipelineStateDesc>() };
    let bytes = serialize_compute(desc_ref, &root.id, &cs.id);
    RECORDER
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .append(Kind::Compute, &bytes, &[root, cs]);
}
