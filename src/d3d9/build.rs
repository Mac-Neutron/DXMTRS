fn main() {
    // Diagnostic-only: `MTLD3D_CRUMB=1` enables the mmap breadcrumb in
    // `crate::crumb`. Routed through a build-script cfg instead of a
    // `RUSTFLAGS --cfg` flag so it composes with the rustflags set by
    // `.cargo/config.toml` (env RUSTFLAGS *replaces* those, it does not
    // append).
    println!("cargo::rustc-check-cfg=cfg(mtld3d_crumb)");
    println!("cargo:rerun-if-env-changed=MTLD3D_CRUMB");
    if std::env::var("MTLD3D_CRUMB").is_ok_and(|v| !v.is_empty() && v != "0") {
        println!("cargo:rustc-cfg=mtld3d_crumb");
    }

    println!("cargo::rustc-check-cfg=cfg(perf_tracking)");
    // The developer timers come with the `perf-tracking` feature; the variable that once switched
    // them only warns now.
    if std::env::var_os("CARGO_FEATURE_PERF_TRACKING").is_some() {
        println!("cargo:rustc-cfg=perf_tracking");
    }
    println!("cargo:rerun-if-env-changed=MTLD3D_PERF");
    if std::env::var_os("MTLD3D_PERF").is_some() {
        println!(
            "cargo:warning=MTLD3D_PERF is gone: build with --features d3d9/perf-tracking or meson -Dd3d9_perf_tracking=true"
        );
    }
}
