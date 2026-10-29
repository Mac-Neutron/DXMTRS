#!/bin/sh
# cargo-staticlib.sh <toolchain> <rust-target> <crate> <cargo-target-dir> <out-file>: builds <crate>'s static library
# for <rust-target> with the production profile, in the mtld3d workspace that holds the crate, and copies it to
# <out-file>. Meson runs it for every Rust target a build links (meson.build).
set -eu
[ $# = 5 ] || { echo "usage: cargo-staticlib.sh <toolchain> <rust-target> <crate> <cargo-target-dir> <out-file>" >&2; exit 2; }
root=$(cd "$(dirname "$0")/.." && pwd)
ws=$root/mtld3d/windows; [ ! -d "$root/mtld3d/unix/$3" ] || ws=$root/mtld3d/unix
( cd "$ws" && CARGO_TARGET_DIR=$4 cargo "+$1" build -q --profile production --target "$2" -p "$3" )
lib=$(echo "$3" | tr - _)
case $2 in *-windows-msvc) f=$lib.lib ;; *) f=lib$lib.a ;; esac
cmp -s "$4/$2/production/$f" "$5" || cp "$4/$2/production/$f" "$5" # unchanged: nothing relinks
