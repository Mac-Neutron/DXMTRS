//! The version line a swap chain's HUD shows.

/// The HUD's version line: `DXMT D3D<api> FL_<feature level> <version>`.
///
/// `feature_level` is a `D3D_FEATURE_LEVEL` (`0x9100` for `9_1`, `0xb000` for `11_0`, ...); one
/// this list lacks reads `UNKNOWN`. `version` is the build's `DXMT_VERSION`, which each front end
/// holds.
#[must_use]
pub fn version_text(api_version: u32, feature_level: u32, version: &str) -> String {
    let level = match feature_level {
        0x9100 => "9_1",
        0x9200 => "9_2",
        0x9300 => "9_3",
        0xa000 => "10_0",
        0xa100 => "10_1",
        0xb000 => "11_0",
        0xb100 => "11_1",
        0xc000 => "12_0",
        0xc100 => "12_1",
        _ => "UNKNOWN",
    };
    format!("DXMT D3D{api_version} FL_{level} {version}")
}

#[cfg(test)]
mod tests;
