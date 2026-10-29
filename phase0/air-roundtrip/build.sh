#!/bin/sh
# build.sh [--dxil <llvm-source>] <llvm-prefix> <out> [compiler flags...]: builds air-roundtrip against the LLVM
# installed at <llvm-prefix> (headers and static libraries; ld64 resolves the archives in any order), with this tree's
# airconv metallib writer. --dxil: through LLVM's DXIL bitcode writer, whose headers are private to <llvm-source>'s
# DirectX target (the prefix's LLVM built with LLVM_EXPERIMENTAL_TARGETS_TO_BUILD=DirectX).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$here/../../src/airconv
dxil=
if [ "${1:-}" = --dxil ]; then
  dxil="-DAIR_ROUNDTRIP_DXIL_WRITER -DWriteBitcodeToFile=air_roundtrip_write_bitcode -I$2/lib/Target/DirectX"
  shift 2
fi
P=$1 O=$2; shift 2
# shellcheck disable=SC2086  # $dxil's words
/usr/bin/clang++ -arch arm64 -std=c++20 -O2 -fno-rtti -fno-exceptions $dxil "$@" -I"$P/include" -I"$src" \
  "$here/air-roundtrip.cpp" "$src/metallib_writer.cpp" -o "$O" "$P"/lib/libLLVM*.a -lz -lcurses -lm
