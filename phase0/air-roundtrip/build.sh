#!/bin/sh
# build.sh <llvm-prefix> <out> [compiler flags...]: builds air-roundtrip against the LLVM installed at <llvm-prefix>
# (headers and static libraries; ld64 resolves the archives in any order), with this tree's airconv metallib writer.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src=$here/../../src/airconv
P=$1 O=$2; shift 2
/usr/bin/clang++ -arch arm64 -std=c++20 -O2 -fno-rtti -fno-exceptions "$@" -I"$P/include" -I"$src" \
  "$here/air-roundtrip.cpp" "$src/metallib_writer.cpp" -o "$O" "$P"/lib/libLLVM*.a -lz -lcurses -lm
