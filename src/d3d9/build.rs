use std::{env, fs, path::Path};

fn read_define(env_var: &str, macro_name: &str, default: &str) -> String {
    println!("cargo:rerun-if-env-changed={env_var}");
    let Some(path) = env::var_os(env_var).filter(|v| !v.is_empty()) else {
        return default.to_owned();
    };
    let path = Path::new(&path);
    println!("cargo:rerun-if-changed={}", path.display());
    let header = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let prefix = format!("#define {macro_name} ");
    header
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix(&prefix)
                .map(|rest| rest.trim().trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| panic!("no {macro_name} define in {}", path.display()))
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

    let key = read_define("DXMT_TRANSLATOR_KEY_H", "DXMT_TRANSLATOR_KEY", "");
    let ver = read_define("DXMT_VERSION_H", "DXMT_VERSION", "dev");

    println!("cargo:rustc-env=DXMT_TRANSLATOR_KEY={key}");
    println!("cargo:rustc-env=DXMT_VERSION={ver}");
}
