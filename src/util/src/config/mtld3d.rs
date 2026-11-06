//! mtld3d's line grammar, which `mtld3d.conf` and `MTLD3D_CONFIG` keep.
//!
//! From mtld3d (zlib, README.md's list): `key = value`, the key and the value trimmed, a value's
//! outer double quotes dropped, `#` opening a comment line. Unlike DXMT's grammar a value keeps
//! its inner spaces, so a launcher's `log.dir` with a space in a folder name survives.

use super::Item;

/// One line of `mtld3d.conf`, or one `;`-separated part of `MTLD3D_CONFIG`.
///
/// `None` for a blank or comment line; [`Item::Malformed`] for one without `=`.
#[must_use]
pub fn item(line: &str) -> Option<Item> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let Some((key, value)) = line.split_once('=') else {
        return Some(Item::Malformed);
    };
    Some(Item::Pair {
        key: key.trim().as_bytes().to_vec(),
        value: unquote(value.trim()).as_bytes().to_vec(),
    })
}

/// `value` without one pair of outer double quotes.
fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests;
