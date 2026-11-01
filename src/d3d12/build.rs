//! Build script for `d3d12`: reads `DXMT_TRANSLATOR_KEY` and `DXMT_VERSION` (P8).
//!
//! Reads the generated headers `$DXMT_TRANSLATOR_KEY_H` and `$DXMT_VERSION_H` when set, falling
//! back to `""` and `"dev"` when unset.

use std::{env, fs, path::PathBuf};

fn read_define(env_var: &str, macro_name: &str, default: &str) -> String {
    println!("cargo:rerun-if-env-changed={env_var}");
    let Some(path) = env::var_os(env_var).filter(|v| !v.is_empty()) else {
        return default.to_owned();
    };
    let path = PathBuf::from(path);
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    let prefix = format!("#define {macro_name} ");
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix(&prefix) {
            let trimmed = rest.trim();
            if let Some(unquoted) = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                return unquoted.to_owned();
            }
        }
    }
    panic!(
        "missing `#define {macro_name} \"...\"` in {}",
        path.display()
    );
}

fn main() {
    let key = read_define("DXMT_TRANSLATOR_KEY_H", "DXMT_TRANSLATOR_KEY", "");
    let version = read_define("DXMT_VERSION_H", "DXMT_VERSION", "dev");
    println!("cargo:rustc-env=DXMT_TRANSLATOR_KEY={key}");
    println!("cargo:rustc-env=DXMT_VERSION={version}");
}
