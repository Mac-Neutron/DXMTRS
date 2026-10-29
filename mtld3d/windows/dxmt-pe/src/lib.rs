//! DXMTRS Phase 0, E1: the probe each DXMT binary calls once when it is loaded.

/// Returns `0x4458_4d54` ("DXMT"), or 0 when `DXMTRS_PROBE_FAIL` is set.
///
/// Reading the environment links the standard library, which is what the probe checks.
#[unsafe(no_mangle)]
pub extern "C" fn dxmtrs_probe() -> u32 {
    if std::env::var_os("DXMTRS_PROBE_FAIL").is_some() { 0 } else { 0x4458_4d54 }
}
