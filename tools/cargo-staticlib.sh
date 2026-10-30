#!/bin/sh
# cargo-staticlib.sh <toolchain> <rust-target> <crate> <cargo-target-dir> <out-file> [<profile> [<features>]]: builds
# <crate>'s static library for <rust-target> with <profile> (default production) and <features> (cargo's list), in the
# tree's one workspace, and copies it to <out-file>. Meson runs it for every Rust target a build links
# (meson.build). macOS targets build for macOS 27 unless MACOSX_DEPLOYMENT_TARGET says otherwise. No home or build
# folder goes into the library: rustc remaps them (cargo joins these rustflags with the workspace's; rustc tries the
# last prefix first, so the most specific comes last). Cargo runs offline: a crate missing from its cache fails the
# build instead of being downloaded.
# An <out-file> ending in .so takes the crate's cdylib instead (src/d3d9/unix's mtld3d.so), linked with -Wl,-S: the
# objects' DWARF stays out of the image's debug map, which would name the build folder.
# With MTLD3D_LLVM_MINGW set (llvm-mingw's root; src/d3d9's targets pass it), the crate's C and C++ (snmalloc-sys,
# zstd-sys) build with tools/mingw-cc.sh's compilers and flags for <rust-target>, links in <cargo-target-dir>/cc.
set -eu
[ $# -ge 5 ] && [ $# -le 7 ] || {
  echo "usage: cargo-staticlib.sh <toolchain> <rust-target> <crate> <cargo-target-dir> <out-file> [<profile> [<features>]]" >&2
  exit 2
}
root=$(cd "$(dirname "$0")/.." && pwd)
profile=${6:-production}
case $2 in *-apple-darwin) export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-27.0}" ;; esac
if [ -n "${MTLD3D_LLVM_MINGW:-}" ]; then
  e=$(sh "$root/tools/mingw-cc.sh" env "$2" "$4/cc")
  eval "$e"
fi
ch=${CARGO_HOME:-$HOME/.cargo}
lib=$(echo "$3" | tr - _)
link= f=lib$lib.a
case $2 in *-windows-msvc) f=$lib.lib ;; esac
case $5 in *.so) link=', "-C", "link-arg=-Wl,-S"' f=lib$lib.dylib ;; esac
remap="target.'cfg(all())'.rustflags=[\"--remap-path-prefix=$HOME=~\", \"--remap-path-prefix=$ch/registry/src=crates\", \
\"--remap-path-prefix=$root=dxmtrs\"$link]"
( cd "$root" && CARGO_TARGET_DIR=$4 cargo "+$1" build -q --offline --profile "$profile" --target "$2" -p "$3" \
  ${7:+--features "$7"} --config "$remap" )
cmp -s "$4/$2/$profile/$f" "$5" || cp "$4/$2/$profile/$f" "$5" # unchanged: nothing relinks
