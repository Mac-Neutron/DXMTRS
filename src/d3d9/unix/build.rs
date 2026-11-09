use std::{env, fs, path::Path};

const DELEGATE_FORWARD: &str = "src/metal/macdrv/delegate_forward.m";

/// The quoted value of `#define <name> "…"` in meson's header, `""` when there is none.
fn extract_quoted_define(path: &Path, name: &str) -> String {
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(contents) = fs::read_to_string(path) else {
        return String::new();
    };
    contents
        .lines()
        .filter_map(|l| {
            l.trim()
                .strip_prefix("#define")?
                .trim_start()
                .strip_prefix(name)
        })
        .find_map(|rest| rest.trim().strip_prefix('"')?.strip_suffix('"'))
        .unwrap_or_default()
        .to_owned()
}

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
            "cargo:warning=MTLD3D_PERF is gone: build with --features mtld3d-unix/perf-tracking or meson -Dd3d9_perf_tracking=true"
        );
    }
    println!("cargo::rustc-check-cfg=cfg(mtld3d_crumb)");
    println!("cargo:rerun-if-env-changed=MTLD3D_CRUMB");
    if std::env::var("MTLD3D_CRUMB").is_ok_and(|v| !v.is_empty() && v != "0") {
        println!("cargo:rustc-cfg=mtld3d_crumb");
    }

    // The translator's key (ruling C3b-6): meson's header, `""` outside meson. Read before the non-Apple return: the
    // guest's Linux build stamps the shader cache with it too.
    println!("cargo:rerun-if-env-changed=DXMT_TRANSLATOR_KEY_H");
    let key = env::var_os("DXMT_TRANSLATOR_KEY_H").map_or_else(String::new, |p| {
        extract_quoted_define(Path::new(&p), "DXMT_TRANSLATOR_KEY")
    });
    println!("cargo:rustc-env=DXMT_TRANSLATOR_KEY={key}");

    let target = std::env::var("TARGET").unwrap();
    // MacNeutron: `mtl_facade` when the Metal layer goes through winemetal (src/mtl.rs): the `winemetal` feature, and
    // every build off Apple targets.
    println!("cargo::rustc-check-cfg=cfg(mtl_facade)");
    let facade = std::env::var_os("CARGO_FEATURE_WINEMETAL").is_some() || !target.contains("apple");
    if facade {
        println!("cargo:rustc-cfg=mtl_facade");
    }
    if !target.contains("apple") {
        return;
    }

    println!("cargo:rustc-link-arg-cdylib=-Wl,-install_name,@rpath/mtld3d.so");

    // Every Rust frame of this crate aborts on unwind (`panic = "abort"`),
    // so a message send whose Objective-C exception has to be caught keeps
    // its `@try` and the send together in Objective-C.
    if facade {
        return;
    }
    println!("cargo:rerun-if-changed={DELEGATE_FORWARD}");
    cc::Build::new()
        .file(DELEGATE_FORWARD)
        .flag("-fobjc-exceptions")
        .flag("-fno-objc-arc")
        .warnings_into_errors(true)
        .compile("mtld3d_delegate_forward");
}
