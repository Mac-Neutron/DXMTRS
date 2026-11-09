//! The translator key this `mtld3d.so` was built with (ruling C3b-6).
//!
//! meson hands `build.rs` the 64-hex-digit key of the shader translator
//! (`DXMT_TRANSLATOR_KEY_H`); a cargo-only build (tests, the loopback, `tools/rust-check.sh`)
//! has none and the key is empty. It is parsed at each call rather than kept in a static (the
//! audit confines those), and the literal goes through `black_box` so the optimizer keeps the 64
//! digits in the shipped binary, where the key-equality gate greps for them.

/// The build's translator key, or `None` when the build has none.
#[must_use]
pub fn translator_key() -> Option<[u8; 32]> {
    parse_key(core::hint::black_box(env!("DXMT_TRANSLATOR_KEY")))
}

/// 64 hex digits (either case) as 32 bytes; anything else is `None`.
#[must_use]
pub fn parse_key(s: &str) -> Option<[u8; 32]> {
    let (pairs, []) = s.as_bytes().as_chunks::<2>() else {
        return None;
    };
    if pairs.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    for (byte, pair) in key.iter_mut().zip(pairs) {
        let hi = char::from(pair[0]).to_digit(16)?;
        let lo = char::from(pair[1]).to_digit(16)?;
        *byte = u8::try_from(hi << 4 | lo).ok()?;
    }
    Some(key)
}

#[cfg(test)]
mod tests;
