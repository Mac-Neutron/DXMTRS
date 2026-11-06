//! The configuration reader every API shares, DXMT's read the way `util/config/config.cpp` does.
//!
//! Four sources, weakest first ([`Sources`]): DXMT's file (`DXMT_CONFIG_FILE`, else `dxmt.conf`
//! in the working folder), D3D9's `mtld3d.conf` beside the exe, `DXMT_CONFIG` split on `;`, then
//! `MTLD3D_CONFIG` split on `;`; for DXMT then the exe's built-in profile for the keys none set.
//! Each file keeps its grammar and each variable its file's: DXMT's below (`parse_line`: lines
//! and values are bytes, never checked for UTF-8, as the C++'s `std::string`s), mtld3d's in
//! [`mtld3d`]. The typed getters parse a value as `Config::parseOptionValue` does and take the
//! fallback when it doesn't.

use std::{collections::BTreeMap, fs::File, io::Read, path::PathBuf};

use crate::{
    env,
    float_cast::{f64_to_f32, u64_to_f64},
    log::Logger,
};

pub mod mtld3d;

/// Where a configuration entry came from.
pub enum Source {
    /// DXMT's file, `DXMT_CONFIG_FILE` or `dxmt.conf`.
    DxmtFile,
    /// D3D9's file, `mtld3d.conf` beside the exe.
    Mtld3dFile,
    /// The `DXMT_CONFIG` variable.
    DxmtEnv,
    /// The `MTLD3D_CONFIG` variable.
    Mtld3dEnv,
}

impl Source {
    /// The source's name, as D3D9's warnings name it.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::DxmtFile => "dxmt.conf",
            Self::Mtld3dFile => "mtld3d.conf",
            Self::DxmtEnv => "DXMT_CONFIG",
            Self::Mtld3dEnv => "MTLD3D_CONFIG",
        }
    }
}

/// What one line or part says.
#[derive(Debug, PartialEq, Eq)]
pub enum Item {
    /// `key = value`.
    Pair { key: Vec<u8>, value: Vec<u8> },
    /// A line of mtld3d's grammar without `=`, which D3D9 warns about.
    Malformed,
}

/// One item of the configuration, with where it came from.
pub struct Entry {
    pub source: Source,
    /// The 0-based line in a file; `None` for a variable's part.
    pub line: Option<usize>,
    pub item: Item,
}

/// The configuration's four sources, as read.
#[derive(Default)]
pub struct Sources {
    /// DXMT's file when it opened: the path as named, the text as a text-mode stream reads it.
    pub dxmt_file: Option<(Vec<u8>, Vec<u8>)>,
    /// `mtld3d.conf`'s path and the read's result.
    ///
    /// `None` when the exe has no folder to look in, and always natively: the native dylibs read
    /// no file beside the VM host's binary.
    pub mtld3d_file: Option<(PathBuf, std::io::Result<Vec<u8>>)>,
    /// `DXMT_CONFIG`, as the C++ reads it; empty when unset.
    pub dxmt_env: Vec<u8>,
    /// `MTLD3D_CONFIG` as UTF-8; empty when unset or not Unicode, as D3D9 read it.
    pub mtld3d_env: Vec<u8>,
}

impl Sources {
    /// Reads the four sources now.
    ///
    /// Never latched: D3D9 reads them at each `Direct3DCreate9`; DXMT's DLLs latch the
    /// [`Config`] they make.
    #[must_use]
    pub fn read() -> Self {
        let mut path = env_var("DXMT_CONFIG_FILE");
        if path.is_empty() {
            path = b"dxmt.conf".to_vec();
        }
        let dxmt_file = read_file(&path).map(|text| (path, text_mode(text, cfg!(windows))));
        Self {
            dxmt_file,
            mtld3d_file: mtld3d_file(),
            dxmt_env: env_var("DXMT_CONFIG"),
            mtld3d_env: std::env::var("MTLD3D_CONFIG")
                .unwrap_or_default()
                .into_bytes(),
        }
    }

    /// Every item of the four sources, weakest first.
    ///
    /// DXMT's file by line, `mtld3d.conf` by line (its text read as UTF-8, a bad byte as
    /// U+FFFD), `DXMT_CONFIG` by `;`, `MTLD3D_CONFIG` by `;`. A `[section]` of DXMT's file makes
    /// the DXMT lines after it count only when it names `exe_name` exactly, and that context
    /// carries into `DXMT_CONFIG`, not into the mtld3d sources.
    #[must_use]
    pub fn entries(&self, exe_name: &[u8]) -> Vec<Entry> {
        let mut entries = Vec::new();
        let mut active = true;
        if let Some((_, text)) = &self.dxmt_file {
            for (n, line) in text.split(|&b| b == b'\n').enumerate() {
                if let Some(item) = parse_line(&mut active, line, exe_name) {
                    entries.push(Entry {
                        source: Source::DxmtFile,
                        line: Some(n),
                        item,
                    });
                }
            }
        }
        if let Some((_, Ok(text))) = &self.mtld3d_file {
            for (n, line) in String::from_utf8_lossy(text).lines().enumerate() {
                if let Some(item) = mtld3d::item(line) {
                    entries.push(Entry {
                        source: Source::Mtld3dFile,
                        line: Some(n),
                        item,
                    });
                }
            }
        }
        for part in self.dxmt_env.split(|&b| b == b';') {
            if let Some(item) = parse_line(&mut active, part, exe_name) {
                entries.push(Entry {
                    source: Source::DxmtEnv,
                    line: None,
                    item,
                });
            }
        }
        for part in String::from_utf8_lossy(&self.mtld3d_env).split(';') {
            if let Some(item) = mtld3d::item(part) {
                entries.push(Entry {
                    source: Source::Mtld3dEnv,
                    line: None,
                    item,
                });
            }
        }
        entries
    }
}

/// `mtld3d.conf` beside the exe, as D3D9 finds it: `current_exe`'s folder (no 260-unit cut).
#[cfg(windows)]
fn mtld3d_file() -> Option<(PathBuf, std::io::Result<Vec<u8>>)> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.parent()?.join("mtld3d.conf");
    let text = std::fs::read(&path);
    Some((path, text))
}

/// Natively no `mtld3d.conf`: the native dylibs keep reading no file beside the VM host's binary.
#[cfg(not(windows))]
const fn mtld3d_file() -> Option<(PathBuf, std::io::Result<Vec<u8>>)> {
    None
}

/// The configuration: the user's options, then those of the exe's built-in profile.
pub struct Config {
    options: Map,
}

/// A three-way option: forced on, forced off, or left to the caller (`Tristate`).
#[derive(Debug, PartialEq, Eq)]
pub enum Tristate {
    Auto,
    False,
    True,
}

impl Config {
    /// Reads the process's configuration, logging what it found to `log` as the C++ does.
    ///
    /// A machine fact, latched once and immutable after: each DLL keeps the result in a `static`
    /// that reads it at the first use (`Config::getInstance`), never under the loader lock.
    ///
    /// It names DXMT's file and `DXMT_CONFIG` and nothing for the two mtld3d sources: no user
    /// path or value of D3D9's in every DLL's Wine output.
    #[must_use]
    pub fn load(log: &Logger) -> Self {
        let sources = Sources::read();
        if let Some((path, _)) = &sources.dxmt_file {
            log.info(&format!(
                "Found config file: {}",
                String::from_utf8_lossy(path)
            ));
        }
        if !sources.dxmt_env.is_empty() {
            log.info(&format!(
                "Found config env: {}",
                String::from_utf8_lossy(&sources.dxmt_env)
            ));
        }
        let mut options = pairs(sources.entries(env::exe_name().as_bytes()));
        if let Some(profile) = profile(&env::exe_path()) {
            for line in profile_lines(profile) {
                log.info(&line);
            }
            options = merged(options, profile);
        }
        Self { options }
    }

    /// The user's options alone (no built-in profile): `file`'s lines, then `env`'s parts.
    ///
    /// `exe_name` is the name a `[section]` line has to match.
    #[must_use]
    pub fn from_user(file: Option<&[u8]>, env: &[u8], exe_name: &[u8]) -> Self {
        Self {
            options: user(file, env, exe_name),
        }
    }

    /// The options `sources` set (no built-in profile), a later source winning.
    #[must_use]
    pub fn from_sources(sources: &Sources, exe_name: &[u8]) -> Self {
        Self {
            options: pairs(sources.entries(exe_name)),
        }
    }

    /// The value of `key`, empty when unset (`getOptionValue`).
    #[must_use]
    pub fn get(&self, key: &str) -> &[u8] {
        self.options.get(key.as_bytes()).map_or(&[], Vec::as_slice)
    }

    /// A boolean option: `true` or `false` in any ASCII case, else `fallback`.
    #[must_use]
    pub fn get_bool(&self, key: &str, fallback: bool) -> bool {
        parse_bool(self.get(key)).unwrap_or(fallback)
    }

    /// An integer option: an optional `-`, then decimal digits only; else `fallback`.
    #[must_use]
    pub fn get_i32(&self, key: &str, fallback: i32) -> i32 {
        parse_i32(self.get(key)).unwrap_or(fallback)
    }

    /// A decimal option, `-` and `.` allowed (no exponent); else `fallback`.
    #[must_use]
    pub fn get_f32(&self, key: &str, fallback: f32) -> f32 {
        parse_f32(self.get(key)).unwrap_or(fallback)
    }

    /// A text option: the value as the file or the variable gave it, empty when unset.
    ///
    /// The C++'s string parse always assigns, so an unset option reads empty and no fallback is
    /// ever returned; none is taken. Bytes that aren't UTF-8 read as U+FFFD.
    #[must_use]
    pub fn get_str(&self, key: &str) -> String {
        String::from_utf8_lossy(self.get(key)).into_owned()
    }

    /// A tri-state option: `true`, `false` or `auto` in any ASCII case, else `fallback`.
    #[must_use]
    pub fn get_tristate(&self, key: &str, fallback: Tristate) -> Tristate {
        let value = self.get(key);
        if value.eq_ignore_ascii_case(b"true") {
            Tristate::True
        } else if value.eq_ignore_ascii_case(b"false") {
            Tristate::False
        } else if value.eq_ignore_ascii_case(b"auto") {
            Tristate::Auto
        } else {
            fallback
        }
    }
}

/// Options by key.
type Map = BTreeMap<Vec<u8>, Vec<u8>>;

/// A built-in profile: the exe names it matches, then its options.
type Profile = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
);

/// `config.cpp`'s built-in profiles, in its order: the exe names each matches, then its options.
///
/// A profile matches an exe path ending in `\<name>.exe`, in any ASCII case (the C++'s
/// `\\<name>\.exe$`, `regex::icase`).
const PROFILES: [Profile; 6] = [
    (
        &["GenshinImpact", "YuanShen", "ZenlessZoneZero"],
        &[
            ("dxgi.customDeviceDesc", "AMD Radeon Pro 5300M"),
            ("dxgi.customVendorId", "1002"),
            ("dxgi.customDeviceId", "7340"),
        ],
    ),
    (
        &["SONIC_X_SHADOW_GENERATIONS"],
        &[("d3d11.ignoreMapFlagNoWait", "True")],
    ),
    (&["OMORI"], &[("dxgi.forceSDR", "True")]),
    (&["O2-Win64-Shipping"], &[("d3d11.sampleNaNToZero", "True")]),
    (&["SotTR"], &[("d3d11.defuseFma", "True")]),
    (&["helldivers2"], &[("d3d11.maxFeatureLevel", "12_1")]),
];

/// The user's options: `file`'s lines, then `env`'s `;`-separated parts.
///
/// A `[section]` line makes the lines after it count only when it names `exe_name` exactly, and
/// that context carries from the file into the variable.
fn user(file: Option<&[u8]>, env: &[u8], exe_name: &[u8]) -> Map {
    let sources = Sources {
        dxmt_file: file.map(|text| (Vec::new(), text.to_vec())),
        dxmt_env: env.to_vec(),
        ..Sources::default()
    };
    pairs(sources.entries(exe_name))
}

/// The pairs of `entries` by key, a later one winning; malformed lines change nothing.
fn pairs(entries: Vec<Entry>) -> Map {
    let mut options = Map::new();
    for entry in entries {
        if let Item::Pair { key, value } = entry.item {
            options.insert(key, value);
        }
    }
    options
}

/// One line of DXMT's grammar (`parseUserConfigLine`): its pair, if it sets one.
///
/// A section's name runs from its `[` to the line's last `]` (none: an empty name), and sets
/// `active`. A key is `[0-9A-Za-z._]*`, then `=`; the value runs to the first space, tab or CR
/// outside double quotes, the quotes dropped. A pair outside the active section, and anything
/// else, yields nothing.
fn parse_line(active: &mut bool, line: &[u8], exe_name: &[u8]) -> Option<Item> {
    let line = skip_whitespace(line);
    if let Some(section) = line.strip_prefix(b"[") {
        let name = section
            .iter()
            .rposition(|&b| b == b']')
            .map_or(&[][..], |end| section.split_at(end).0);
        *active = name == exe_name;
        return None;
    }
    let key_end = line
        .iter()
        .position(|&b| !is_key_char(b))
        .unwrap_or(line.len());
    let (key, rest) = line.split_at(key_end);
    let rest = skip_whitespace(rest).strip_prefix(b"=")?;
    let mut value = Vec::new();
    let mut quoted = false;
    for &b in skip_whitespace(rest) {
        if !quoted && is_whitespace(b) {
            break;
        }
        if b == b'"' {
            quoted = !quoted;
        } else {
            value.push(b);
        }
    }
    active.then(|| Item::Pair {
        key: key.to_vec(),
        value,
    })
}

/// `config.cpp`'s whitespace: space, tab and CR.
const fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r')
}

/// A byte a key may hold.
const fn is_key_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'.' || b == b'_'
}

/// `line` from its first byte that is not whitespace.
fn skip_whitespace(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|&b| !is_whitespace(b))
        .unwrap_or(line.len());
    line.split_at(start).1
}

/// The built-in profile of the exe at `exe_path`, if one matches.
fn profile(exe_path: &str) -> Option<&'static [(&'static str, &'static str)]> {
    let path = exe_path.as_bytes();
    PROFILES
        .iter()
        .find(|(names, _)| {
            names.iter().any(|name| {
                let suffix = format!("\\{name}.exe");
                path.len()
                    .checked_sub(suffix.len())
                    .and_then(|start| path.get(start..))
                    .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix.as_bytes()))
            })
        })
        .map(|&(_, options)| options)
}

/// The info lines a matched profile logs: a heading, then each option in key order.
fn profile_lines(profile: &[(&str, &str)]) -> Vec<String> {
    let sorted: BTreeMap<&str, &str> = profile.iter().copied().collect();
    core::iter::once("Found built-in config:".to_owned())
        .chain(
            sorted
                .iter()
                .map(|(key, value)| format!("  {key} = {value}")),
        )
        .collect()
}

/// `options` with each of `profile`'s it lacks: the user's file and variable win (`merge`).
fn merged(mut options: Map, profile: &[(&str, &str)]) -> Map {
    for (key, value) in profile {
        options
            .entry(key.as_bytes().to_vec())
            .or_insert_with(|| value.as_bytes().to_vec());
    }
    options
}

/// `parseOptionValue` for `bool`: `true` or `false` in any ASCII case.
const fn parse_bool(value: &[u8]) -> Option<bool> {
    if value.eq_ignore_ascii_case(b"true") {
        Some(true)
    } else if value.eq_ignore_ascii_case(b"false") {
        Some(false)
    } else {
        None
    }
}

/// `parseOptionValue` for `int32_t`: an optional `-`, then digits only (a lone `-` is 0).
///
/// The C++ overflows its `int32_t` on a long number, which is undefined; here it wraps.
fn parse_i32(value: &[u8]) -> Option<i32> {
    let (sign, digits) = match value.strip_prefix(b"-") {
        Some(digits) => (-1_i32, digits),
        None if value.is_empty() => return None,
        None => (1, value),
    };
    let mut number = 0_i32;
    for &b in digits {
        if !b.is_ascii_digit() {
            return None;
        }
        number = number.wrapping_mul(10).wrapping_add(i32::from(b - b'0'));
    }
    Some(sign.wrapping_mul(number))
}

/// `parseOptionValue` for `float`: the value the C++ assigned, if it got that far.
///
/// An optional `-`, digits, and at most one `.` followed by at least one digit; no leading `.`.
/// The C++ sums the parts in `double` (the parts wrap at 64 bits) and rounds once to `float`. It
/// reports `isfinite` of the sum, which `getOption` discards: a sum that isn't finite (a
/// fraction of 64 digits or more) was assigned, and stands.
fn parse_f32(value: &[u8]) -> Option<f32> {
    let (negate, rest) = value
        .strip_prefix(b"-")
        .map_or((false, value), |rest| (true, rest));
    if rest.is_empty() || rest.first() == Some(&b'.') {
        return None;
    }
    let (whole, fraction) = rest.iter().position(|&b| b == b'.').map_or_else(
        || (rest, &[][..]),
        |dot| {
            let (whole, fraction) = rest.split_at(dot);
            (whole, fraction.get(1..).unwrap_or_default())
        },
    );
    if rest.len() > whole.len() && fraction.is_empty() {
        return None;
    }
    let digits = |bytes: &[u8], init: (u64, u64)| {
        bytes.iter().try_fold(init, |(part, divisor), &b| {
            b.is_ascii_digit().then(|| {
                (
                    part.wrapping_mul(10).wrapping_add(u64::from(b - b'0')),
                    divisor.wrapping_mul(10),
                )
            })
        })
    };
    let (whole, _) = digits(whole, (0, 1))?;
    let (fraction, divisor) = digits(fraction, (0, 1))?;
    let sum = u64_to_f64(fraction) / u64_to_f64(divisor) + u64_to_f64(whole);
    let result = f64_to_f32(sum);
    Some(if negate { -result } else { result })
}

/// `text` as a text-mode stream reads it on PE.
///
/// Up to its first Ctrl-Z, each CR before an LF dropped. Natively the bytes stay as they are.
fn text_mode(text: Vec<u8>, pe: bool) -> Vec<u8> {
    if !pe {
        return text;
    }
    let end = text.iter().position(|&b| b == 0x1a).unwrap_or(text.len());
    let mut bytes = text.iter().take(end).peekable();
    let mut read = Vec::with_capacity(end);
    while let Some(&b) = bytes.next() {
        if b != b'\r' || bytes.peek() != Some(&&b'\n') {
            read.push(b);
        }
    }
    read
}

/// The file at `path`, or none when it can't be opened.
///
/// A read that fails part-way ends the text there, as it ends `std::getline`'s loop.
fn read_file(path: &[u8]) -> Option<Vec<u8>> {
    let mut file = File::open(path_of(path)).ok()?;
    let mut text = Vec::new();
    let _partial = file.read_to_end(&mut text);
    Some(text)
}

/// `getEnvVar`: the variable's text as the C++ reads it, empty when unset.
#[cfg(windows)]
fn env_var(name: &str) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    std::env::var_os(name).map_or_else(Vec::new, |value| {
        from_wide(&value.encode_wide().collect::<Vec<_>>())
    })
}

/// `getEnvVar`: the variable's bytes, empty when unset.
#[cfg(not(windows))]
fn env_var(name: &str) -> Vec<u8> {
    use std::os::unix::ffi::OsStringExt;
    std::env::var_os(name).map_or_else(Vec::new, std::ffi::OsString::into_vec)
}

/// The path `str::topath` makes of `bytes`, up to their first NUL: UTF-8 widened on PE.
#[cfg(windows)]
fn path_of(bytes: &[u8]) -> PathBuf {
    let text = bytes.split(|&b| b == 0).next().unwrap_or_default();
    PathBuf::from(String::from_utf8_lossy(text).into_owned())
}

/// The path `str::topath` makes of `bytes`, up to their first NUL: the bytes themselves natively.
#[cfg(not(windows))]
fn path_of(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    let text = bytes.split(|&b| b == 0).next().unwrap_or_default();
    PathBuf::from(std::ffi::OsStr::from_bytes(text))
}

/// `str::fromws`: UTF-16 text up to its first NUL, as DXMT's transcoder turns it into UTF-8.
///
/// A stray low surrogate, or a high one with nothing after it, is `?`; a high surrogate takes the
/// next unit as its pair, whatever it is. A character beyond the BMP is written only when it is
/// the text's last and none before it was dropped: `encodeTypedChar`'s room check for four bytes
/// is inverted (`util_string.cpp`), so any other is dropped and its four bytes stay NUL at the end.
#[cfg(any(windows, test))]
fn from_wide(units: &[u16]) -> Vec<u8> {
    let mut rest = units.split(|&unit| unit == 0).next().unwrap_or_default();
    let mut chars = Vec::new();
    while let Some((&first, tail)) = rest.split_first() {
        let (ch, tail) = match first {
            0xD800..0xDC00 => tail.split_first().map_or_else(
                || (u32::from('?'), tail),
                |(&second, tail)| {
                    let high = (u32::from(first) & 0x3ff) << 10;
                    (0x1_0000 + high + (u32::from(second) & 0x3ff), tail)
                },
            ),
            0xDC00..0xE000 => (u32::from('?'), tail),
            _ => (u32::from(first), tail),
        };
        chars.push(char::from_u32(ch).unwrap_or('?'));
        rest = tail;
    }
    let total = chars.iter().map(|c| c.len_utf8()).sum::<usize>();
    let mut text = vec![0; total];
    let mut at = 0;
    for c in chars {
        let len = c.len_utf8();
        if len == 4 && at + 4 < total {
            continue;
        }
        if let Some(room) = text.get_mut(at..at + len) {
            c.encode_utf8(room);
            at += len;
        }
    }
    text
}

#[cfg(test)]
mod tests;
