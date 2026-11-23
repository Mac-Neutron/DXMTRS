fn main() {
    println!("cargo::rustc-check-cfg=cfg(perf_tracking)");
    // The developer timers come with the `perf-tracking` feature; the variable that once switched
    // them only warns now.
    if std::env::var_os("CARGO_FEATURE_PERF_TRACKING").is_some() {
        println!("cargo:rustc-cfg=perf_tracking");
    }
    println!("cargo:rerun-if-env-changed=MTLD3D_PERF");
    if std::env::var_os("MTLD3D_PERF").is_some() {
        println!(
            "cargo:warning=MTLD3D_PERF is gone: build with --features mtld3d-core/perf-tracking or meson -Dd3d9_perf_tracking=true"
        );
    }
}
