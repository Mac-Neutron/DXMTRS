#!/bin/sh
# The C and C++ compiler cc-rs runs for D3D9's Rust targets (src/d3d9), through links to this script named after the
# personality: mingw-cc and mingw-c++ for the -pc-windows-msvc targets, whose C++ (snmalloc-sys) is built for the mingw
# triple of the same arch (aarch64-, arm64ec-, x86_64- or i686-w64-mingw32), whose CRT the DLL links anyway, rather
# than against Microsoft's headers for the -msvc triple Rust builds for. llvm-mingw's <triple>-clang supplies the target
# and sysroot; the --target=<arch>-pc-windows-msvc cc-rs adds comes after it and would win, so it's dropped.
# MTLD3D_LLVM_MINGW names llvm-mingw's root. As linux-cc and linux-c++, the C, C++ and linker of the
# x86_64-unknown-linux-gnu and i686-unknown-linux-gnu builds: llvm-mingw's clang and ld.lld for Fedora's triple
# (MTLD3D_LINUX_TRIPLE, x86-64's by default), against the VM guest's sysroot (MTLD3D_SYSROOT).
#   mingw-cc.sh env <rust-target> <links-dir>
# makes the four links in <links-dir> and prints the environment cc-rs reads for <rust-target> (tools/cargo-staticlib.sh
# evals it): the compilers and today's flags per target, NDEBUG where today's build had it.
case $(basename "$0") in
  mingw-cc.sh)
    [ $# = 3 ] && [ "$1" = env ] || { echo "usage: mingw-cc.sh env <rust-target> <links-dir>" >&2; exit 2; }
    t=$2 d=$3
    self=$(cd "$(dirname "$0")" && pwd)/mingw-cc.sh
    root=$(cd "$(dirname "$0")/.." && pwd)
    mkdir -p "$d"
    for n in mingw-cc mingw-c++ linux-cc linux-c++; do
      ln -s "$self" "$d/.$n.$$" && mv -f "$d/.$n.$$" "$d/$n" || exit 1
    done
    maps="-ffile-prefix-map=$HOME=~ -ffile-prefix-map=${CARGO_HOME:-$HOME/.cargo}/registry/src=crates \
-ffile-prefix-map=$root=dxmtrs"
    # cc-rs reads every name in two spellings, <target> and <target with - and . as _>; sh's export takes only the
    # second.
    u=$(echo "$t" | tr .- __)
    case $t in
      aarch64-pc-windows-msvc|arm64ec-pc-windows-msvc)
        echo "export CC_$u='$d/mingw-cc' CXX_$u='$d/mingw-c++' CFLAGS_$u='$maps'"
        echo "export CXXFLAGS_$u='-fno-exceptions -fno-rtti -fno-threadsafe-statics -DSNMALLOC_USE_THREAD_CLEANUP \
-include stdlib.h $maps'"
        echo "export CFLAGS=-DNDEBUG CXXFLAGS=-DNDEBUG" ;;
      *-apple-darwin)
        echo "export CFLAGS_$u='$maps' CXXFLAGS_$u='$maps' CFLAGS=-DNDEBUG CXXFLAGS=-DNDEBUG" ;;
      *) echo "mingw-cc.sh: no C environment for $t" >&2; exit 2 ;;
    esac
    exit 0 ;;
esac
case $(basename "$0") in *++) tool=clang++ ;; *) tool=clang ;; esac
case $(basename "$0") in
  linux-*) exec "$MTLD3D_LLVM_MINGW/bin/$tool" --target="${MTLD3D_LINUX_TRIPLE:-x86_64-redhat-linux}" \
    --sysroot="${MTLD3D_SYSROOT:?}" -fuse-ld=lld \
    "$@" ;;
esac
# The Rust target: cc-rs's --target, or cargo's TARGET (cc-rs's compiler-family probe passes no --target).
target=${TARGET:-}
for a do
  case $a in --target=*) target=${a#--target=} ;; esac
done
# (GNU mode's ARM64EC defines __arm64ec__ and the x64 macros, not MSVC's _M_ARM64EC: vendor/snmalloc-sys tests both.)
case $target in
  aarch64-pc-windows-msvc) triple=aarch64-w64-mingw32 ;;
  arm64ec-pc-windows-msvc) triple=arm64ec-w64-mingw32 ;;
  x86_64-pc-windows-msvc) triple=x86_64-w64-mingw32 ;;
  i686-pc-windows-msvc) triple=i686-w64-mingw32 ;;
  *) echo "mingw-cc: target '$target' is no -pc-windows-msvc target of D3D9's" >&2; exit 2 ;;
esac
for a do
  shift
  case $a in --target=*) ;; *) set -- "$@" "$a" ;; esac
done
exec "$MTLD3D_LLVM_MINGW/bin/$triple-$tool" "$@"
