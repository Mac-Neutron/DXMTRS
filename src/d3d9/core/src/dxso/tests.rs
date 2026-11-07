use std::{
    fs,
    path::{Path, PathBuf},
};

/// Module paths an emitter source may name, and where their source files live.
const MODULE_ROOTS: [(&str, &str); 2] = [
    ("crate::", "src/"),
    ("mtld3d_shared::", "../../unix/shared/src/"),
];

/// The emitter's front end, moved into `airconv-ir`.
///
/// `build.rs` hashes these files by name (C3-20), not the folder, so the lowering files beside
/// them stay out of the MSL fingerprint.
const FRONT_END: [&str; 6] = [
    "../../airconv/ir/src/dxso.rs",
    "../../airconv/ir/src/dxso/ir.rs",
    "../../airconv/ir/src/dxso/link.rs",
    "../../airconv/ir/src/dxso/opcode.rs",
    "../../airconv/ir/src/dxso/parser.rs",
    "../../airconv/ir/src/dxso/spec.rs",
];

/// The emitter sources `build.rs` hashes: `src/dxso.rs` and everything under `src/dxso`.
fn emitter_sources(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read emitter source directory") {
            let path = entry.expect("emitter source entry").path();
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("source name");
            if stem == "tests" || stem.ends_with("_tests") {
                continue;
            }
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = vec![root.join("src/dxso.rs")];
    walk(&root.join("src/dxso"), &mut out);
    out
}

/// The moved front end is in the emitter fingerprint, file by file, and nothing else under it is.
#[test]
fn the_moved_front_end_is_fingerprinted_by_name() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let build = fs::read_to_string(root.join("build.rs")).expect("read build.rs");
    for file in FRONT_END {
        assert!(
            root.join(file).is_file(),
            "{file} moved: update FRONT_END and build.rs"
        );
        assert!(
            build.contains(&format!("\"{file}\"")),
            "build.rs does not hash {file}"
        );
    }
    let listed = build.matches("\"../../airconv/ir/src/dxso").count();
    assert_eq!(
        listed,
        FRONT_END.len(),
        "build.rs hashes a file not in FRONT_END"
    );
}

/// Every module outside `dxso` that the emitter names is in the emitter fingerprint.
///
/// A cached MSL library is reused while the fingerprint is unchanged, so a
/// value the emitter writes into MSL from a source `build.rs` does not hash
/// leaves stale libraries in every cache when only that source changes.
/// `mtld3d_types` is not checked: it holds D3D9 ABI constants, whose values
/// are fixed.
#[test]
fn every_module_the_emitter_names_is_fingerprinted() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let build = fs::read_to_string(root.join("build.rs")).expect("read build.rs");
    let mut missing = Vec::new();
    for path in emitter_sources(root) {
        let text = fs::read_to_string(&path).expect("read emitter source");
        let code = text
            .lines()
            .map(str::trim_start)
            .filter(|line| !line.starts_with("//"));
        for line in code {
            for (prefix, dir) in MODULE_ROOTS {
                for (at, _) in line.match_indices(prefix) {
                    let rest = &line[at + prefix.len()..];
                    assert!(
                        !rest.starts_with('{'),
                        "{}: name each `{prefix}` module on its own path so this check sees it",
                        path.display()
                    );
                    let module: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    // A macro or a root item names no module source.
                    if module == "dxso" || !rest[module.len()..].starts_with("::") {
                        continue;
                    }
                    if !build.contains(&format!("\"{dir}{module}.rs\"")) {
                        missing.push(format!("{}: {prefix}{module}", path.display()));
                    }
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "emitter sources name modules the fingerprint does not hash: {missing:#?}"
    );
}

/// The two warnings the parser returns log the text it logged itself.
#[test]
fn the_returned_warnings_log_the_text_the_parser_used_to() {
    use super::{DxsoWarning, warning_text};
    assert_eq!(
        warning_text(DxsoWarning::LinkedInputsOverflow),
        "dxso: PS declares more than 8 extra input semantics → the rest read zero"
    );
    assert_eq!(
        warning_text(DxsoWarning::ColorOutBeyondTargets(5)),
        "dxso: PS writes oC5, beyond the four D3D9 render targets → write ignored"
    );
}
