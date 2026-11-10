//! D3D9 identity: one digest of the AIR every corpus program translates to.
//!
//! Every shader record of the committed DXSO corpus that carries its source (the DXSO and its
//! specialization) is translated through `airconv::dxso::translate`, in file order, and the
//! metallibs are concatenated (a refused program contributes `refused:` and the error text) and
//! hashed with `mtld3d_core::shader_key::ff_key_hash` (xxh3 over the bytes' `Hash`, a length
//! prefix and the bytes). The test prints `dxso-identity: <n> programs, digest <hex>`: a change
//! to the lowering that must leave every existing program alone shows the same line before and
//! after it. The digest is kept outside the test, since the corpus itself is regenerated.
//!
//! `AIRCONV_DXSO_CORPUS` names the corpus directory, as for `dxso_parity`.

use std::{env, ffi::CString, fs, path::PathBuf};

use airconv::dxso::translate;
use mtld3d_core::{shader_cache::read_records, shader_key::ff_key_hash};

const CORPUS_FILES: [&str; 3] = [
    "synthetic.bin",
    "conformance-x64.bin",
    "conformance-i686.bin",
];

#[test]
fn dxso_identity() {
    let dir = env::var_os("AIRCONV_DXSO_CORPUS").map_or_else(
        || {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/d3d9/dxso-corpus"
            ))
        },
        PathBuf::from,
    );
    let mut all = Vec::new();
    let mut programs = 0usize;
    for name in CORPUS_FILES {
        let Ok(bytes) = fs::read(dir.join(name)) else {
            continue;
        };
        for entry in &read_records(&bytes).shaders {
            let Some(source) = entry.source() else {
                continue;
            };
            let entry_name = CString::new(entry.kind.entry_name(entry.key)).expect("no NUL");
            match translate(source.tokens(), &source.stage_spec(), &entry_name) {
                Ok(metallib) => all.extend_from_slice(&metallib),
                Err(e) => {
                    all.extend_from_slice(b"refused:");
                    all.extend_from_slice(e.text());
                }
            }
            programs += 1;
        }
    }
    if programs == 0 {
        println!(
            "dxso-identity: skipped (no corpus program in {})",
            dir.display()
        );
        return;
    }
    println!(
        "dxso-identity: {programs} programs, digest {:016x}",
        ff_key_hash(&all)
    );
}
