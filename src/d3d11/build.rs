//! Embeds the shader-cache version string (`DXMT_TRANSLATOR_KEY+DXMT_GIT_VERSION`).
//!
//! Outside meson (`cargo test`, `cargo clippy`), both fall back to `""` (`"+"`), which disables
//! persistent disk caching just as an unkeyed build does.

use std::{env, fs, path::Path};

fn extract_quoted_define(path: &Path, macro_name: &str) -> String {
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(contents) = fs::read_to_string(path) else {
        return String::new();
    };
    for line in contents.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix("#define") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix(macro_name) else {
            continue;
        };
        let rest = rest.trim();
        if let Some(inner) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
            return inner.to_owned();
        }
    }
    String::new()
}

fn main() {
    println!("cargo:rerun-if-env-changed=DXMT_TRANSLATOR_KEY_H");
    println!("cargo:rerun-if-env-changed=DXMT_VERSION_H");

    let key = env::var_os("DXMT_TRANSLATOR_KEY_H")
        .map(|p| extract_quoted_define(Path::new(&p), "DXMT_TRANSLATOR_KEY"))
        .unwrap_or_default();
    let ver = env::var_os("DXMT_VERSION_H")
        .map(|p| extract_quoted_define(Path::new(&p), "DXMT_GIT_VERSION"))
        .unwrap_or_default();

    println!("cargo:rustc-env=DXMT_SHADER_CACHE_VERSION={key}+{ver}");
}
