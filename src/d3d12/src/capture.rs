//! DXIL shader dumping, SM6 capability reporting, and capture pipeline naming
//!
//! (`d3d12_dxil_dump.{hpp,cpp}`, `d3d12_pipeline.hpp:87-104`).

use std::sync::{LazyLock, Mutex, PoisonError};

use rustc_hash::FxHashMap;

static PIPELINE_NAMES: LazyLock<Mutex<FxHashMap<u64, String>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// Returns whether `DXMT_DXIL_DUMP` capture mode is active (`DXILCaptureMode`,
///
/// `d3d12_dxil_dump.hpp:31-34`).
#[cfg(not(feature = "perf-build"))]
#[must_use]
pub fn dxil_capture_mode() -> bool {
    dxmt_core::dump::on()
}

/// Always `false` under `perf-build`.
#[cfg(feature = "perf-build")]
#[must_use]
pub const fn dxil_capture_mode() -> bool {
    false
}

/// Returns whether SM6 capabilities should be advertised (`SM6Caps`, `d3d12_dxil_dump.cpp:42`).
#[must_use]
pub fn sm6_caps() -> bool {
    static YES: LazyLock<bool> = LazyLock::new(|| {
        dxil_capture_mode() || !std::env::var("DXMT_D3D12_SM6").is_ok_and(|v| v == "0")
    });
    *YES
}

/// FNV-1a 64-bit hash used in capture filenames and `pipelines.txt` (`CaptureHash`,
///
/// `d3d12_dxil_dump.cpp:25`).
#[must_use]
pub fn capture_hash(bytes: &[u8]) -> u64 {
    let mut h = 14_695_981_039_346_656_037_u64;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(1_099_511_628_211);
    }
    h
}

#[cfg(not(feature = "perf-build"))]
fn dxil_stage(bytecode: &[u8]) -> Option<&'static str> {
    const STAGES: [&str; 16] = [
        "ps",
        "vs",
        "gs",
        "hs",
        "ds",
        "cs",
        "lib",
        "raygen",
        "intersection",
        "anyhit",
        "closesthit",
        "miss",
        "callable",
        "ms",
        "as",
        "node",
    ];
    if bytecode.len() < 32 || &bytecode[..4] != b"DXBC" {
        return None;
    }
    let count = usize::try_from(u32::from_le_bytes(bytecode[28..32].try_into().ok()?)).ok()?;
    for i in 0..count {
        let off_at = 32usize.checked_add(i.checked_mul(4)?)?;
        let off_end = off_at.checked_add(4)?;
        if off_end > bytecode.len() {
            break;
        }
        let off = usize::try_from(u32::from_le_bytes(
            bytecode[off_at..off_end].try_into().ok()?,
        ))
        .ok()?;
        let Some(end) = off.checked_add(12) else {
            continue;
        };
        if end > bytecode.len() || &bytecode[off..off + 4] != b"DXIL" {
            continue;
        }
        let kind = usize::from(u16::from_le_bytes(
            bytecode[off + 10..off + 12].try_into().ok()?,
        ));
        return Some(STAGES.get(kind).copied().unwrap_or("unknown"));
    }
    None
}

/// Dumps a DXIL shader blob to `<DXMT_DXIL_DUMP>/<stage>-<hash>.dxil` when capture is active
///
/// (`DumpDXIL`, `d3d12_dxil_dump.cpp:64`).
#[cfg(not(feature = "perf-build"))]
pub fn dump_dxil(bytecode: &[u8]) {
    if !dxil_capture_mode() || bytecode.is_empty() {
        return;
    }
    let Some(stage) = dxil_stage(bytecode) else {
        return;
    };
    let name = format!("{stage}-{:016x}.dxil", capture_hash(bytecode));
    dxmt_core::dump::save_once(&name, bytecode);
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn dump_dxil(_bytecode: &[u8]) {}

/// Dumps a root-signature blob to `<DXMT_DXIL_DUMP>/rs-<hash>.bin` when capture is active
///
/// (`DumpRootSignature`, `d3d12_dxil_dump.cpp:78`).
#[cfg(not(feature = "perf-build"))]
pub fn dump_root_signature(blob: &[u8]) {
    if !dxil_capture_mode() || blob.is_empty() {
        return;
    }
    let name = format!("rs-{:016x}.bin", capture_hash(blob));
    dxmt_core::dump::save_once(&name, blob);
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn dump_root_signature(_blob: &[u8]) {}

/// Appends `line` to `<DXMT_DXIL_DUMP>/pipelines.txt` (`LogPipeline`, `d3d12_dxil_dump.hpp:52`).
#[cfg(not(feature = "perf-build"))]
pub fn log_pipeline(line: &str) {
    dxmt_core::dump::append_pipeline(line);
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn log_pipeline(_line: &str) {}

/// Saves `data` to `<DXMT_DXIL_DUMP>/<name>` (`SaveCapture`, `d3d12_dxil_dump.hpp:55`).
#[cfg(not(feature = "perf-build"))]
pub fn save_capture(name: &str, data: &[u8]) {
    dxmt_core::dump::save(name, data);
}

/// No-op under `perf-build`.
#[cfg(feature = "perf-build")]
pub const fn save_capture(_name: &str, _data: &[u8]) {}

/// Creates `<DXMT_DXIL_DUMP>/<name>` (`MakeCaptureFolder`, `d3d12_dxil_dump.hpp:58`).
#[cfg(not(feature = "perf-build"))]
#[must_use]
pub fn make_capture_folder(name: &str) -> bool {
    dxmt_core::dump::make_folder(name)
}

/// Always `false` under `perf-build`.
#[cfg(feature = "perf-build")]
#[must_use]
pub const fn make_capture_folder(_name: &str) -> bool {
    false
}

/// Associates a human-readable pipeline description with `pso` handle (`NamePipeline`,
///
/// `d3d12_dxil_dump.cpp:91`).
pub fn name_pipeline(pso: u64, name: String) {
    if !dxil_capture_mode() || pso == 0 {
        return;
    }
    PIPELINE_NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(pso, name);
}

/// Looks up the description recorded for `pso` handle (`PipelineName`, `d3d12_dxil_dump.cpp:98`).
#[must_use]
pub fn pipeline_name(pso: u64) -> String {
    PIPELINE_NAMES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&pso)
        .cloned()
        .unwrap_or_else(|| format!("pso-{pso:x}"))
}

/// Formats `" <name>=<16-hex-hash>"` for a non-empty shader bytecode (`CapturedShader`,
///
/// `d3d12_pipeline.hpp:88`).
#[must_use]
pub fn captured_shader(name: &str, bytecode: &[u8]) -> String {
    if bytecode.is_empty() {
        return String::new();
    }
    dump_dxil(bytecode);
    format!(" {name}={:016x}", capture_hash(bytecode))
}

/// Formats `" rs=<16-hex-hash>"` or `" rs=embedded"` (`CapturedRootSignature`,
///
/// `d3d12_pipeline.hpp:96`).
#[must_use]
pub fn captured_root_signature(blob: Option<&[u8]>) -> String {
    blob.filter(|b| !b.is_empty()).map_or_else(
        || " rs=embedded".to_owned(),
        |bytes| {
            dump_root_signature(bytes);
            format!(" rs={:016x}", capture_hash(bytes))
        },
    )
}
