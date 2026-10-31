//! Takes the internal command library's metallib from `$DXMT_COMMAND_METALLIB`.
//!
//! meson builds `dxmt_command.metal` and names the result in that variable when it builds the
//! crate for a DLL. Without it (the crate's host tests) the library is an empty file.

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=DXMT_COMMAND_METALLIB");
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR for a build script"))
        .join("dxmt_command.metallib");
    match env::var_os("DXMT_COMMAND_METALLIB") {
        Some(metallib) => {
            println!("cargo:rerun-if-changed={}", metallib.to_string_lossy());
            fs::copy(&metallib, &out)
                .expect("the metallib $DXMT_COMMAND_METALLIB names is readable");
        }
        None => fs::write(&out, []).expect("OUT_DIR is writable"),
    }
}
