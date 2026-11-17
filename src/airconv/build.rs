//! Builds airconv's Metal helpers and names the LLVM 23 install `AIRCONV_LLVM` to the linker.
//!
//! The Metal helpers of `shaders/` are compiled to AIR in `OUT_DIR` (`<name>.air`) by
//! `AIRCONV_XCRUN`: xcrun, or a `sh` script taking xcrun's arguments, run through `/bin/sh` when
//! it is not executable (`xcrun-metal.sh` of `MacNeutron` is not). LLVM's libraries,
//! `meson.build`'s `llvm_deps` in its order, are linked by the final image: `-bundle`, so a static
//! library of this crate holds none of them and meson links them. No C or C++ is compiled here:
//! what LLVM's C API lacks comes from the install's own `libLLVMAIRExtras.a`.

use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let llvm = PathBuf::from(var("AIRCONV_LLVM"));
    let xcrun = var("AIRCONV_XCRUN");
    let out = PathBuf::from(var("OUT_DIR"));
    // Once a build script prints any `rerun-if`, cargo reruns it only on what is named: every
    // input is named here.
    for input in ["build.rs", "meson.build", "shaders"] {
        println!("cargo:rerun-if-changed={input}");
    }
    println!(
        "cargo:rerun-if-changed={}",
        llvm.join(".complete").display()
    );

    for name in ["air_msad", "air_samplepos", "air_tessellation"] {
        let input = Path::new("shaders").join(format!("{name}.metal"));
        metal(&xcrun, &input, &out.join(format!("{name}.air")));
    }

    println!(
        "cargo:rustc-link-search=native={}",
        llvm.join("lib").display()
    );
    for lib in llvm_deps() {
        println!("cargo:rustc-link-lib=static:-bundle={lib}");
    }
    for lib in ["z", "xml2", "c++", "m"] {
        println!("cargo:rustc-link-lib={lib}");
    }
}

/// An input variable, which the build cannot go without.
fn var(name: &str) -> String {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name).unwrap_or_else(|_| panic!("airconv's build needs {name} (tools/rust-check.sh)"))
}

/// Compiles one Metal file to AIR, with the arguments meson's `metalir_generator` passes.
fn metal(xcrun: &str, input: &Path, output: &Path) {
    let input = fs::canonicalize(input).expect("the Metal helper exists");
    // A file without an execute bit is a script for /bin/sh (meson's machine files run it so);
    // anything else, a path or a name on PATH, is run itself.
    let script =
        fs::metadata(xcrun).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 == 0);
    let mut command = if script {
        let mut sh = Command::new("/bin/sh");
        sh.arg(xcrun);
        sh
    } else {
        Command::new(xcrun)
    };
    command.args(["-sdk", "macosx", "metal", "-o"]);
    command.arg(output).arg("-c").arg(&input);
    command.args(["-std=metal3.1", "--target=air64-apple-macos14.0"]);
    let status = command
        .status()
        .unwrap_or_else(|e| panic!("{xcrun} for {}: {e}", input.display()));
    assert!(
        status.success(),
        "{xcrun} for {}: {status}",
        input.display()
    );
}

/// `meson.build`'s `llvm_deps`, each `-l<lib>` as `<lib>`, in order.
fn llvm_deps() -> Vec<String> {
    let meson = fs::read_to_string("meson.build").expect("meson.build is readable");
    let list = meson
        .split_once("llvm_deps = [")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(list, _)| list)
        .expect("meson.build defines llvm_deps");
    let libs: Vec<String> = list
        .split(|c: char| c == '\'' || c == ',' || c.is_whitespace())
        .filter_map(|token| token.strip_prefix("-l"))
        .map(str::to_owned)
        .collect();
    assert!(!libs.is_empty(), "meson.build's llvm_deps names no library");
    libs
}
