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
    // Diagnostic-only: `MTLD3D_CRUMB=1` enables the mmap breadcrumb in
    // `crate::crumb`. Routed through a build-script cfg instead of a
    // `RUSTFLAGS --cfg` flag so it composes with the rustflags set by
    // `.cargo/config.toml` (env RUSTFLAGS *replaces* those, it does not
    // append).
    println!("cargo::rustc-check-cfg=cfg(mtld3d_crumb)");
    println!("cargo:rerun-if-env-changed=MTLD3D_CRUMB");
    if env::var("MTLD3D_CRUMB").is_ok_and(|v| !v.is_empty() && v != "0") {
        println!("cargo:rustc-cfg=mtld3d_crumb");
    }

    println!("cargo::rustc-check-cfg=cfg(perf_tracking)");
    // The developer timers come with the `perf-tracking` feature; the variable that once switched
    // them only warns now.
    if env::var_os("CARGO_FEATURE_PERF_TRACKING").is_some() {
        println!("cargo:rustc-cfg=perf_tracking");
    }
    println!("cargo:rerun-if-env-changed=MTLD3D_PERF");
    if env::var_os("MTLD3D_PERF").is_some() {
        println!(
            "cargo:warning=MTLD3D_PERF is gone: build with --features d3d9/perf-tracking or meson -Dd3d9_perf_tracking=true"
        );
    }

    println!("cargo:rerun-if-env-changed=DXMT_TRANSLATOR_KEY_H");
    println!("cargo:rerun-if-env-changed=DXMT_VERSION_H");

    let key = env::var_os("DXMT_TRANSLATOR_KEY_H")
        .map_or_default(|p| extract_quoted_define(Path::new(&p), "DXMT_TRANSLATOR_KEY"));
    let ver = env::var_os("DXMT_VERSION_H")
        .map(|p| extract_quoted_define(Path::new(&p), "DXMT_VERSION"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "dev".to_owned());

    println!("cargo:rustc-env=DXMT_TRANSLATOR_KEY={key}");
    println!("cargo:rustc-env=DXMT_VERSION={ver}");
}
