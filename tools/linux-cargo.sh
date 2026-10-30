#!/bin/sh
# cargo for D3D9's x86_64-unknown-linux-gnu build, the VM guest's, or with MTLD3D_LINUX=i686 its i686-unknown-linux-gnu
# one (old WoW64's 32-bit Unix side): in the tree's workspace, with tools/mingw-cc.sh's linux-cc and linux-c++
# (llvm-mingw's clang and ld.lld for Fedora's triple, on the guest's sysroot of that arch) as its C, C++ and linker,
# their links in $CARGO_TARGET_DIR/cc. The arguments go to cargo, before the target. tools/cargo-staticlib.sh's Linux
# mode runs it for mtld3d.so.
#   MTLD3D_RUST=<toolchain> MTLD3D_LLVM_MINGW=<llvm-mingw root> MTLD3D_SYSROOT=<sysroot> CARGO_TARGET_DIR=<absolute> \
#     [MTLD3D_LINUX=i686] linux-cargo.sh <cargo command> [args...]
set -eu
: "${MTLD3D_RUST:?names the rustup toolchain (cargo +MTLD3D_RUST)}"
: "${MTLD3D_LLVM_MINGW:?names the llvm-mingw root}"
: "${MTLD3D_SYSROOT:?names the guest sysroot of this arch (MacNeutron: eac-vm/sysroot/fetch.sh)}"
case ${CARGO_TARGET_DIR:?names the cargo build folder, outside the tree} in
  /*) ;; *) echo "linux-cargo.sh: CARGO_TARGET_DIR must be absolute, not $CARGO_TARGET_DIR" >&2; exit 2 ;;
esac
root=$(cd "$(dirname "$0")/.." && pwd)
t=${MTLD3D_LINUX:-x86_64}-unknown-linux-gnu
e=$(sh "$root/tools/mingw-cc.sh" env "$t" "$CARGO_TARGET_DIR/cc")
eval "$e"
cd "$root"
exec cargo "+$MTLD3D_RUST" "$@" --target "$t"
