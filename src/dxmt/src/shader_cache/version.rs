//! The translation cache's 64-bit table version (`ShaderCacheVersion` in the C++).

/// Computes the 64-bit FNV-1a table version (`ShaderCacheVersion`).
///
/// Hashes the bytes of `key_or_version` (`DXMT_SHADER_CACHE_KEY`, or `DXMT_VERSION` when built
/// without one) followed by the four little-endian bytes of `airconv_version`
/// (`SM50GetApiVersion()`). A different version drops the cache table of an earlier build on open.
#[must_use]
pub const fn version(key_or_version: &str, airconv_version: u32) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;

    let bytes = key_or_version.as_bytes();
    let mut hash = OFFSET_BASIS;
    let mut i = 0;
    while i < bytes.len() {
        hash = (hash ^ (bytes[i] as u64)).wrapping_mul(PRIME);
        i += 1;
    }
    let airconv = airconv_version.to_le_bytes();
    let mut j = 0;
    while j < airconv.len() {
        hash = (hash ^ (airconv[j] as u64)).wrapping_mul(PRIME);
        j += 1;
    }
    hash
}
