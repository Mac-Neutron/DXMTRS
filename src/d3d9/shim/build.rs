fn main() {
    let target = std::env::var("TARGET").unwrap();
    if !target.contains("windows") {
        return;
    }

    // The path its per-arch objects carry inside `libwinecrt0.a`. An ARM64X
    // build (`--enable-archs=arm64ec,aarch64`) installs one archive under the
    // native arch and packs BOTH arches' objects into it (`output_static_lib`
    // in Wine's makedep adds `object_files[hybrid_arch]`), so the EC build takes
    // the EC object out of the aarch64 archive, which is the one
    // `src/d3d9/meson.build` names for both halves (`MTLD3D_WINECRT0`, the
    // absolute path of this target's `libwinecrt0.a`).
    //
    // Spelled out rather than defaulted, so a target we have not thought about
    // fails here instead of silently linking another arch's `unix_lib.o`.
    let object_arch = match target.split('-').next().expect("target triple has an arch") {
        "x86_64" => "x86_64-windows",
        "i686" => "i386-windows",
        "aarch64" => "aarch64-windows",
        "arm64ec" => "arm64ec-windows",
        arch => panic!("no Wine builtin directory known for target arch `{arch}`"),
    };

    let winecrt0 =
        std::env::var("MTLD3D_WINECRT0").expect("MTLD3D_WINECRT0 must name libwinecrt0.a");
    let out_dir = std::env::var("OUT_DIR").unwrap();

    // `llvm-ar` is never on PATH under either of the names it can arrive as, so
    // resolve it explicitly: an `LLVM_AR` override first, then the toolchain's
    // own `llvm-tools` copy, then Homebrew's keg for a toolchain without that
    // component. It has to be `llvm-ar` and not the `llvm-lib` symlink beside it,
    // because the extraction below speaks ar syntax, not lib.exe syntax.
    let llvm_ar = std::env::var("LLVM_AR").unwrap_or_else(|_| resolve_llvm_ar());

    // Take just unix_lib.o out of winecrt0.a, which avoids the TLS symbol
    // conflicts with the CRT (both define __tls_index, __tls_start, etc.).
    // Absolute, because the extraction runs with its working directory set to
    // `OUT_DIR`; canonicalizing here also turns a missing archive into an error
    // naming the file rather than an `ar` failure.
    let archive =
        std::fs::canonicalize(&winecrt0).unwrap_or_else(|err| panic!("{winecrt0}: {err}"));
    let unix_lib_path =
        extract_unix_lib(&llvm_ar, &archive.to_string_lossy(), object_arch, &out_dir);

    // Every DLL links the crate as a static library (`src/d3d9/meson.build`),
    // and cargo hands link arguments to a cdylib only. So the object travels
    // inside the static library, as a native library rustc bundles into it;
    // the link that follows pulls it in through the unix-call symbols the
    // shim references.
    bundle_unix_lib(&llvm_ar, &unix_lib_path, &out_dir);

    println!("cargo:rerun-if-env-changed=MTLD3D_WINECRT0");
    println!("cargo:rerun-if-env-changed=LLVM_AR");
}

/// Extract `unix_lib.o` for `object_arch` from `archive` into `out_dir`.
///
/// Returns the path of the extracted object. An ARM64X `libwinecrt0.a` carries
/// two `unix_lib.o` members, the ARM64 one and the EC one, under different
/// directories. Without the `P` modifier `llvm-ar` compares basenames only and
/// hands back the first member of that name whatever path it was asked for,
/// and its `N` instance count is kept per stored path, so no count above one
/// ever matches. So the stored names are read first, and the wanted member is
/// extracted by its full stored name with `P`, which compares whole paths and
/// still writes the object under its basename. Reading the names also survives
/// Wine moving winecrt0 between `dlls/` and `libs/`, which it has done before.
///
/// # Panics
///
/// If the archive is unreadable or carries no `unix_lib.o` for this arch, which
/// is what a Wine built without this architecture looks like.
fn extract_unix_lib(llvm_ar: &str, archive: &str, object_arch: &str, out_dir: &str) -> String {
    let listing = std::process::Command::new(llvm_ar)
        .args(["t", archive])
        .output()
        .expect("ar failed");
    assert!(
        listing.status.success(),
        "could not read {archive}: is it a Wine build carrying this architecture?"
    );
    let listing = String::from_utf8(listing.stdout).expect("ar member names are UTF-8");

    let Some(member) = listing
        .lines()
        .find(|member| member.ends_with(&format!("{object_arch}/unix_lib.o")))
    else {
        return extract_unix_lib_by_machine(llvm_ar, archive, &listing, object_arch, out_dir);
    };

    let status = std::process::Command::new(llvm_ar)
        .current_dir(out_dir)
        .args(["xP", archive, member])
        .status()
        .expect("ar failed");
    assert!(
        status.success(),
        "extracting {member} from {archive} failed"
    );
    format!("{out_dir}/unix_lib.o")
}

/// Extract `unix_lib.o` for `object_arch` from an `archive` that stores members under bare names.
///
/// An archiver that drops the directories (llvm-mingw's `llvm-ar`, which a stock
/// Wine built with llvm-mingw packs `libwinecrt0.a` with) stores an ARM64X
/// archive's two `unix_lib.o` members under the same name. `N` counts instances
/// of one stored name, so each instance is extracted in turn and the one whose
/// COFF machine field is the target's is kept.
///
/// # Panics
///
/// If no instance carries the target's machine, or `llvm-ar` cannot extract one.
fn extract_unix_lib_by_machine(
    llvm_ar: &str,
    archive: &str,
    listing: &str,
    object_arch: &str,
    out_dir: &str,
) -> String {
    let wanted: u16 = match object_arch {
        "aarch64-windows" => 0xaa64,
        "arm64ec-windows" => 0xa641,
        "x86_64-windows" => 0x8664,
        "i386-windows" => 0x014c,
        arch => panic!("no COFF machine known for {arch}"),
    };
    let path = format!("{out_dir}/unix_lib.o");
    let instances = listing
        .lines()
        .filter(|member| *member == "unix_lib.o")
        .count();
    for instance in 1..=instances {
        let status = std::process::Command::new(llvm_ar)
            .current_dir(out_dir)
            .args(["xN", &instance.to_string(), archive, "unix_lib.o"])
            .status()
            .expect("ar failed");
        assert!(
            status.success(),
            "extracting unix_lib.o #{instance} from {archive} failed"
        );
        let header = std::fs::read(&path).unwrap_or_else(|err| panic!("{path}: {err}"));
        if header.len() >= 2 && u16::from_le_bytes([header[0], header[1]]) == wanted {
            return path;
        }
    }
    panic!("{archive} carries no {object_arch}/unix_lib.o, by path or by COFF machine")
}

/// Wrap `unix_lib` in a COFF archive in `out_dir` and link it as a native library.
///
/// rustc bundles a native static library into a static library it builds, which
/// is how the object reaches the ARM64X link. The COFF format is spelled out
/// because `llvm-ar` would otherwise write the host's format, and it is the COFF
/// writer that gives an ARM64EC member the EC symbol map the linker reads.
///
/// # Panics
///
/// If `llvm-ar` cannot write the archive.
fn bundle_unix_lib(llvm_ar: &str, unix_lib: &str, out_dir: &str) {
    let archive = format!("{out_dir}/wine_unix_lib.lib");
    // `r` adds to an archive that exists, so a rerun starts from none.
    let _ = std::fs::remove_file(&archive);
    let status = std::process::Command::new(llvm_ar)
        .args(["rcs", "--format=coff", &archive, unix_lib])
        .status()
        .expect("ar failed");
    assert!(
        status.success(),
        "archiving {unix_lib} into {archive} failed"
    );
    println!("cargo:rustc-link-search=native={out_dir}");
    println!("cargo:rustc-link-lib=static=wine_unix_lib");
}

/// Resolve the `llvm-ar` binary.
///
/// Homebrew installs llvm keg-only (not on PATH), so query `brew --prefix
/// llvm` and fall back to the default Apple-Silicon keg location if Homebrew
/// is unavailable.
fn resolve_llvm_ar() -> String {
    toolchain_llvm_ar().unwrap_or_else(homebrew_llvm_ar)
}

/// `llvm-ar` from the active toolchain's `llvm-tools` component.
///
/// The first choice, because it needs nothing installed beyond the toolchain
/// this build is already running under (the build's tools folder links
/// `lld-link` and `llvm-lib` from the same directory, so the component is
/// installed there).
/// `None` when the component is absent, which is what a toolchain installed
/// without it looks like.
fn toolchain_llvm_ar() -> Option<String> {
    let rustc = std::env::var("RUSTC").ok()?;
    let host = std::env::var("HOST").ok()?;
    let out = std::process::Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sysroot = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let path = format!("{sysroot}/lib/rustlib/{host}/bin/llvm-ar");
    std::path::Path::new(&path).is_file().then_some(path)
}

/// `llvm-ar` from the Homebrew llvm keg, which is keg-only and off PATH.
///
/// The fallback for a toolchain with no `llvm-tools`. Asking `brew` covers both
/// prefixes (Apple-Silicon `/opt/homebrew`, Intel `/usr/local`); the literal
/// path is the last resort when `brew` itself is not on PATH.
fn homebrew_llvm_ar() -> String {
    std::process::Command::new("brew")
        .args(["--prefix", "llvm"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|prefix| !prefix.is_empty())
        .map_or_else(
            || "/opt/homebrew/opt/llvm/bin/llvm-ar".to_owned(),
            |prefix| format!("{prefix}/bin/llvm-ar"),
        )
}
