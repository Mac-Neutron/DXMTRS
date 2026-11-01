//! `.pipelines` (`DXMTPRC1`) recording writer (`d3d12_pipeline_record.{hpp,cpp}`).

use core::ffi::c_void;

use crate::shader_cache::Sha1Digest;

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
pub fn checksum(_payload: &[u8]) -> u64 {
    crate::not_implemented!("pipeline::record::checksum")
}

/// Returns whether `DXMT_PIPELINE_RECORD` is active (`RecordingOn`,
///
/// `d3d12_pipeline_record.cpp:240`).
#[must_use]
pub fn recording_on() -> bool {
    crate::not_implemented!("pipeline::record::recording_on")
}

/// Appends a graphics pipeline and any unrecorded blobs to `<DXMT_PIPELINE_RECORD>/<exe>.pipelines`
///
/// (`RecordGraphics`, `d3d12_pipeline_record.cpp:337`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_GRAPHICS_PIPELINE_STATE_DESC`.
pub unsafe fn record_graphics(
    _desc: *const c_void,
    _root: Blob<'_>,
    _vs: Blob<'_>,
    _ps: Blob<'_>,
    _gs: Blob<'_>,
) {
    let (): () = crate::not_implemented!("pipeline::record::record_graphics");
}

/// Appends a compute pipeline and any unrecorded blobs to `<DXMT_PIPELINE_RECORD>/<exe>.pipelines`
///
/// (`RecordCompute`, `d3d12_pipeline_record.cpp:361`).
///
/// # Safety
///
/// `desc` must point to a valid `D3D12_COMPUTE_PIPELINE_STATE_DESC`.
pub unsafe fn record_compute(_desc: *const c_void, _root: Blob<'_>, _cs: Blob<'_>) {
    let (): () = crate::not_implemented!("pipeline::record::record_compute");
}
