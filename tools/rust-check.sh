#!/bin/sh
# The Rust gate of the tree; run it before every commit. No arguments. From the repository root it runs five legs and
# prints each cargo command line, `rust-check: <leg> ok|FAIL` per leg and, if every leg passed, `rust-check: ok` (exit 0;
# otherwise exit 1, every leg run):
#   fmt     the pinned nightly's `cargo fmt --all --check` (rustfmt.toml uses unstable options)
#   clippy  1.99.0, `-D warnings`, on aarch64-apple-darwin and on every -pc-windows-msvc target a crate ships on, then
#           the same host and i686 lines with MTLD3D_PERF=1 (cfg(perf_tracking) code), in a target folder of their own
#   doc     `cargo doc --no-deps` with rustdoc warnings denied, for i686 and the host
#   test    the host unit tests, also with MTLD3D_PERF=1 (not mtld3d-mtl's integration tests: see unit_tests)
#   audit   tools/audit.sh; refuses while an untracked .rs, .toml or .md file exists (the audit reads tracked files)
# The PE legs compile build scripts that need C and Wine's archives, so the inputs are variables (docs/DEVELOPMENT.md,
# "tools/rust-check.sh"); MacNeutron sets them from its pins. cargo runs offline: fetch the locked crates first.
#   MTLD3D_RUST          the rustup toolchain, with clippy (default: meson.options' cargo_toolchain); `rustc +<it>` must
#                        run, so rustup's proxy is on PATH, with lld-link and llvm-lib (MacNeutron: the folder
#                        dxmt/mtld3d-tools.sh prints)
#   MTLD3D_RUST_NIGHTLY  the pinned nightly, as rustup names it, e.g. nightly-2026-10-30 (MacNeutron: dxmt/pins)
#   RUST_NIGHTLY_BIN     optional: that nightly's bin folder (default: rustup's nightly-aarch64-apple-darwin)
#   MTLD3D_LLVM_MINGW    llvm-mingw's root
#   WINE_BUILD           the Mac Wine build tree (libs/winecrt0/aarch64-windows/libwinecrt0.a)
#   GUEST_WINE_SDK       the VM guest's Wine SDK (lib/wine/{x86_64,i386}-windows/libwinecrt0.a)
#   AIRCONV_LLVM         the LLVM 15 install airconv builds its shim against and links (include/, lib/, .complete)
#   AIRCONV_XCRUN        xcrun, or a sh script taking xcrun's arguments, for airconv's Metal helpers (MacNeutron:
#                        wine-arm64/tools/xcrun-metal.sh)
#   CARGO_TARGET_DIR     optional: an absolute folder for cargo's output (default: target/ in the tree, ignored by git)
# Linux targets are not linted here (the guest build compiles them); the unit tests run under cargo test, one process
# per test binary, where the Makefile's nextest ran one per test.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

MTLD3D_RUST=${MTLD3D_RUST:-$(sed -n "s/^option('cargo_toolchain'.*value : '\([^']*\)'.*/\1/p" meson.options)}
: "${MTLD3D_RUST:?names the rustup toolchain (default: cargo_toolchain in meson.options; MacNeutron: MTLD3D_RUST in dxmt/pins)}"
: "${MTLD3D_RUST_NIGHTLY:?names the pinned nightly, such as nightly-2026-10-30 (MacNeutron: MTLD3D_RUST_NIGHTLY in dxmt/pins)}"
: "${MTLD3D_LLVM_MINGW:?names the llvm-mingw root (MacNeutron: the parent of the folder sh dxmt/toolchain.sh prints)}"
: "${WINE_BUILD:?names the Mac Wine build tree (MacNeutron: build/wine-arm64-src/wine-build)}"
: "${GUEST_WINE_SDK:?names the VM guest Wine SDK (MacNeutron: build/eac-vm-work/guest-wine-sdk)}"
: "${AIRCONV_LLVM:?names the LLVM 15 install airconv builds against (include/, lib/, .complete)}"
: "${AIRCONV_XCRUN:?names xcrun, or a sh script taking its arguments (MacNeutron: wine-arm64/tools/xcrun-metal.sh)}"
NIGHTLY_BIN=${RUST_NIGHTLY_BIN:-${RUSTUP_HOME:-$HOME/.rustup}/toolchains/nightly-aarch64-apple-darwin/bin}
T=${CARGO_TARGET_DIR:-$root/target}
case $T in
  /*) ;; *) echo "rust-check: CARGO_TARGET_DIR must be absolute, not $T" >&2; exit 2 ;;
esac
export MTLD3D_LLVM_MINGW AIRCONV_LLVM AIRCONV_XCRUN CARGO_TARGET_DIR="$T"
unset MTLD3D_PERF
# The toolchain's own bin folder first on PATH: a rustup proxy runs `cargo +<toolchain>`, but the subcommands (clippy, fmt)
# are found on PATH, where Homebrew's rustup keeps no proxies for them.
if sysroot=$(rustc "+$MTLD3D_RUST" --print sysroot 2>/dev/null); then PATH="$sysroot/bin:$PATH"; fi
case $(rustc --version) in
  "rustc $MTLD3D_RUST "*) ;;
  *) echo "rust-check: rustc on PATH is $(rustc --version), not $MTLD3D_RUST: install the toolchain, with the clippy component, or put its bin folder first" >&2
     exit 2 ;;
esac

PE_P='-p d3d9 -p mtld3d -p mtld3d-core -p mtld3d-types -p mtld3d-shared -p winemetal-pe -p dxgi -p d3d10core -p dxmt-util -p airconv-ir'
PE64_P="$PE_P -p nvngx -p nvapi"
HOST_P='-p mtld3d-core -p mtld3d-types -p mtld3d-shared -p mtld3d-unix -p mtld3d-mtl -p mtld3d-conformance -p mtld3d-e2e -p winemetal-wire -p winemetal -p dxmt-util -p nvapi -p nvngx -p dxgi -p d3d10core -p airconv-ir -p airconv'
HOST=aarch64-apple-darwin
I686=i686-pc-windows-msvc

# cargo; the line it runs is printed.
cargo_() {
  printf 'rust-check: + %scargo %s\n' "${MTLD3D_PERF:+MTLD3D_PERF=1 }" "$*"
  cargo "$@"
}

# The libwinecrt0.a the mtld3d shim bundles into a PE target's static library.
winecrt0() {
  case $1 in
    aarch64-*|arm64ec-*) # one ARM64X archive carries both halves
      for d in libs dlls; do
        [ -f "$WINE_BUILD/$d/winecrt0/aarch64-windows/libwinecrt0.a" ] && { echo "$WINE_BUILD/$d/winecrt0/aarch64-windows/libwinecrt0.a"; return 0; }
      done
      echo "rust-check: no libwinecrt0.a under WINE_BUILD=$WINE_BUILD (libs/ or dlls/winecrt0/aarch64-windows)" >&2
      return 1 ;;
    x86_64-*) f=$GUEST_WINE_SDK/lib/wine/x86_64-windows/libwinecrt0.a ;;
    i686-*) f=$GUEST_WINE_SDK/lib/wine/i386-windows/libwinecrt0.a ;;
  esac
  [ -f "$f" ] || { echo "rust-check: no $f (GUEST_WINE_SDK=$GUEST_WINE_SDK)" >&2; return 1; }
  echo "$f"
}

# pe <target> <command...>: run <command> with <target>'s C environment (tools/mingw-cc.sh env) and its Wine archive.
pe() {
  (
    t=$1; shift
    e=$(sh tools/mingw-cc.sh env "$t" "$T/cc") || exit 1
    eval "$e"
    MTLD3D_WINECRT0=$(winecrt0 "$t") || exit 1
    export MTLD3D_WINECRT0
    "$@"
  )
}

# perf <command...>: run <command> as the MTLD3D_PERF=1 build, in a cargo folder of its own (a crate whose build script
# reads the variable rebuilds, with everything above it, whenever the value changes).
perf() { ( MTLD3D_PERF=1 CARGO_TARGET_DIR=$T/perf; export MTLD3D_PERF CARGO_TARGET_DIR; "$@" ); }

# lint <target> <cargo clippy args...>
lint() {
  t=$1; shift
  case $t in
    *-windows-msvc) pe "$t" cargo_ clippy --offline --keep-going --target "$t" "$@" -- -D warnings ;;
    *) cargo_ clippy --offline --keep-going --target "$t" "$@" -- -D warnings ;;
  esac
}

# The tests run on the host; winemetal's reference symbols only the link of winemetal.so provides.
tst() { cargo_ test --offline --no-fail-fast --target "$HOST" "$@"; }

leg_fmt() {
  want=${MTLD3D_RUST_NIGHTLY#nightly-}
  have=$(sed -n 's/^date = "\(.*\)"/\1/p' "$NIGHTLY_BIN/../lib/rustlib/multirust-channel-manifest.toml" 2>/dev/null | head -1)
  [ "$have" = "$want" ] || {
    echo "rust-check: the nightly in $NIGHTLY_BIN is dated ${have:-unknown}; MTLD3D_RUST_NIGHTLY=$MTLD3D_RUST_NIGHTLY names $want" >&2
    return 1
  }
  printf 'rust-check: + cargo fmt --all --check (%s)\n' "$MTLD3D_RUST_NIGHTLY"
  PATH="$NIGHTLY_BIN:$PATH" "$NIGHTLY_BIN/cargo" fmt --all --check
}

leg_clippy() {
  rc=0
  lint $I686 $PE_P || rc=1
  lint $I686 -p mtld3d-tests --all-targets || rc=1
  for t in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
    lint $t $PE64_P || rc=1
    lint $t -p mtld3d-tests --all-targets || rc=1
  done
  lint arm64ec-pc-windows-msvc $PE64_P || rc=1
  lint $HOST --all-targets $HOST_P --features mtld3d-core/disk-cache,airconv/oracle || rc=1
  lint $HOST -p mtld3d-unix --features mtld3d-unix/winemetal --all-targets || rc=1
  lint $HOST -p winemetal --features native || rc=1
  perf lint $HOST --all-targets -p mtld3d-core -p mtld3d-unix --features mtld3d-core/disk-cache || rc=1
  perf lint $I686 $PE_P || rc=1
  return $rc
}

leg_doc() {
  rc=0
  RUSTDOCFLAGS='-D warnings'; export RUSTDOCFLAGS
  pe $I686 cargo_ doc --no-deps --offline --keep-going --target $I686 $PE_P --features mtld3d-core/disk-cache || rc=1
  cargo_ doc --no-deps --offline --keep-going --target $HOST $HOST_P --features mtld3d-core/disk-cache || rc=1
  unset RUSTDOCFLAGS
  return $rc
}

# The facade's integration tests (mtld3d-mtl's tests/) need a winemetal.so beside stand-ins for the Wine libraries it
# links, which MacNeutron's dxmt/mtld3d-facade-test.sh builds and runs; here only its unit tests run.
unit_tests() {
  r=0
  tst -p mtld3d-core -p mtld3d-types --features mtld3d-core/disk-cache || r=1
  tst -p mtld3d-unix -p mtld3d-shared -p mtld3d-conformance -p mtld3d-e2e -p winemetal-wire || r=1
  tst -p mtld3d-mtl --lib || r=1
  return $r
}

leg_test() {
  rc=0
  unit_tests || rc=1
  perf unit_tests || rc=1
  tst -p dxmt-util -p nvapi -p nvngx -p winemetal -p dxgi -p d3d10core -p airconv-ir -p airconv --features airconv/oracle \
    --config 'target.aarch64-apple-darwin.rustflags=["-C","link-arg=-Wl,-undefined,dynamic_lookup"]' || rc=1
  return $rc
}

leg_audit() {
  untracked=$(git ls-files --others --exclude-standard | grep -E '\.(rs|toml|md)$' || true)
  if [ -n "$untracked" ]; then
    printf 'rust-check: the audit reads tracked files only; git add these first:\n%s\n' "$untracked" >&2
    return 1
  fi
  printf 'rust-check: + sh tools/audit.sh\n'
  sh tools/audit.sh
}

failed=
for leg in fmt clippy doc test audit; do
  if "leg_$leg"; then
    echo "rust-check: $leg ok"
  else
    echo "rust-check: $leg FAIL"
    failed="$failed $leg"
  fi
done
[ -z "$failed" ] || { echo "rust-check: FAIL, legs:$failed" >&2; exit 1; }
echo "rust-check: ok"
