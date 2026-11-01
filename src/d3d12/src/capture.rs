//! DXIL shader dumping, SM6 capability reporting, and capture pipeline naming
//!
//! (`d3d12_dxil_dump.{hpp,cpp}`, `d3d12_pipeline.hpp:87-104`).

/// Returns whether `DXMT_DXIL_DUMP` capture mode is active (`DXILCaptureMode`,
///
/// `d3d12_dxil_dump.hpp:31-34`).
#[must_use]
pub fn dxil_capture_mode() -> bool {
    crate::not_implemented!("capture::dxil_capture_mode")
}

/// Returns whether SM6 capabilities should be advertised (`SM6Caps`, `d3d12_dxil_dump.cpp:89`).
#[must_use]
pub fn sm6_caps() -> bool {
    crate::not_implemented!("capture::sm6_caps")
}

/// FNV-1a 64-bit hash used in capture filenames and `pipelines.txt` (`CaptureHash`,
///
/// `d3d12_dxil_dump.cpp:25`).
#[must_use]
pub fn capture_hash(_bytes: &[u8]) -> u64 {
    crate::not_implemented!("capture::capture_hash")
}

/// Dumps a DXIL shader blob to `<DXMT_DXIL_DUMP>/<stage>-<hash>.dxil` when capture is active
///
/// (`DumpDXIL`, `d3d12_dxil_dump.cpp:64`).
pub fn dump_dxil(_bytecode: &[u8]) {
    let (): () = crate::not_implemented!("capture::dump_dxil");
}

/// Dumps a root-signature blob to `<DXMT_DXIL_DUMP>/rs-<hash>.bin` when capture is active
///
/// (`DumpRootSignature`, `d3d12_dxil_dump.cpp:78`).
pub fn dump_root_signature(_blob: &[u8]) {
    let (): () = crate::not_implemented!("capture::dump_root_signature");
}

/// Appends `line` to `<DXMT_DXIL_DUMP>/pipelines.txt` (`LogPipeline`, `d3d12_dxil_dump.hpp:52`).
pub fn log_pipeline(_line: &str) {
    let (): () = crate::not_implemented!("capture::log_pipeline");
}

/// Saves `data` to `<DXMT_DXIL_DUMP>/<name>` (`SaveCapture`, `d3d12_dxil_dump.hpp:55`).
pub fn save_capture(_name: &str, _data: &[u8]) {
    let (): () = crate::not_implemented!("capture::save_capture");
}

/// Creates `<DXMT_DXIL_DUMP>/<name>` (`MakeCaptureFolder`, `d3d12_dxil_dump.hpp:58`).
#[must_use]
pub fn make_capture_folder(_name: &str) -> bool {
    crate::not_implemented!("capture::make_capture_folder")
}

/// Associates a human-readable pipeline description with `pso` handle (`NamePipeline`,
///
/// `d3d12_dxil_dump.cpp:104`).
pub fn name_pipeline(_pso: u64, _name: String) {
    let (): () = crate::not_implemented!("capture::name_pipeline");
}

/// Looks up the description recorded for `pso` handle (`PipelineName`, `d3d12_dxil_dump.cpp:112`).
#[must_use]
pub fn pipeline_name(_pso: u64) -> String {
    let (): () = crate::not_implemented!("capture::pipeline_name");
    String::new()
}

/// Formats `" <name>=<16-hex-hash>"` for a non-empty shader bytecode (`CapturedShader`,
///
/// `d3d12_pipeline.hpp:88`).
#[must_use]
pub fn captured_shader(_name: &str, _bytecode: &[u8]) -> String {
    let (): () = crate::not_implemented!("capture::captured_shader");
    String::new()
}

/// Formats `" rs=<16-hex-hash>"` or `" rs=embedded"` (`CapturedRootSignature`,
///
/// `d3d12_pipeline.hpp:96`).
#[must_use]
pub fn captured_root_signature(_blob: Option<&[u8]>) -> String {
    let (): () = crate::not_implemented!("capture::captured_root_signature");
    String::new()
}
