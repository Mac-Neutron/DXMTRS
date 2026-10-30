#!/bin/sh
# arm64x-link.sh <llvm-mingw-bin> <winebuild> <libntdll.a> <def> <out.dll> <arm64.lib> <arm64ec.lib> [<version.rc>]:
# links one ARM64X Wine builtin (src/d3d9's d3d9.dll and mtld3d.dll) from a crate's two static libraries, the
# aarch64-pc-windows-msvc half (the .def's native exports) and the arm64ec-pc-windows-msvc one, against llvm-mingw's
# ARM64X CRT and Wine's libntdll.a; an .rc is compiled by llvm-rc and linked in; the image is marked builtin. Ported
# from mtld3d's Makefile (windows-arm64x, zlib). Not meson's driver link: MacNeutron's cross file links with
# -flto=thin, which -marm64x refuses. The linker is <llvm-mingw-bin>/lld-link only, never PATH's (MacNeutron puts
# rust-lld there under that name); its version goes into the build log.
set -eu
[ $# -ge 7 ] && [ $# -le 8 ] || {
  echo "usage: arm64x-link.sh <llvm-mingw-bin> <winebuild> <libntdll.a> <def> <out.dll> <arm64.lib> <arm64ec.lib> [<version.rc>]" >&2
  exit 2
}
bin=$1 winebuild=$2 ntdll=$3 def=$4 out=$5 arm64=$6 arm64ec=$7 rc=${8:-}
crt=$bin/../aarch64-w64-mingw32/lib
# The toolchain, named before it fails in the link: LLD 23 or newer, and an llvm-mingw whose aarch64 sysroot carries
# hybrid objects (20260826 or newer).
lld=$("$bin/lld-link" --version 2>/dev/null) || true
echo "arm64x-link.sh: $bin/lld-link: $lld"
version=$(echo "$lld" | sed -n 's/.*LLD \([0-9][0-9]*\)\..*/\1/p')
[ -n "$version" ] || { echo "ARM64X: $bin/lld-link is no LLD; the ARM64X link needs lld-link 23 or newer" >&2; exit 2; }
[ "$version" -ge 23 ] || {
  echo "ARM64X: $bin/lld-link is LLD $version, and the ARM64X link needs 23 or newer" >&2; exit 2
}
[ -f "$crt/dllcrt2.o" ] || {
  echo "ARM64X: $bin/.. has no aarch64-w64-mingw32/lib/dllcrt2.o; it names an llvm-mingw install, 20260826 or newer" >&2
  exit 2
}
"$bin/llvm-readobj" --file-headers "$crt/dllcrt2.o" 2>/dev/null | grep -q IMAGE_FILE_MACHINE_ARM64EC || {
  echo "ARM64X: $bin/.. has an aarch64-w64-mingw32/lib/dllcrt2.o with no ARM64EC half; the ARM64X link needs" \
    "llvm-mingw 20260826 or newer, whose aarch64 sysroot carries hybrid objects" >&2
  exit 2
}
# d3d9's halves (the shim's link no snmalloc) are checked first. `-fno-threadsafe-statics` (tools/mingw-cc.sh) is
# sound only while every guarded local static in the C++ is one a lock of its own serializes, which for this snmalloc
# is one, the handler latch in `PALWindows::initialise_for_singleton` under its `Singleton`: so the halves carry
# exactly that guard (mingw ABI), and a thread-safe one (`__cxa_guard_*`, `_Init_thread_*`) would mean the flag
# stopped applying. snmalloc's thread-exit teardown is the one src/d3d9/src/arm64_crt.rs calls,
# `_malloc_thread_cleanup`, and nothing registers a `thread_local` destructor through llvm-mingw's `__tlregdtor`,
# which never runs one. An snmalloc upgrade that trips either has to be read before the guard moves.
if [ "$(basename "$out")" = d3d9.dll ]; then
  guard=_ZGVZN8snmalloc10PALWindows24initialise_for_singletonEPyE4tidy
  for lib in "$arm64" "$arm64ec"; do
    symbols=$("$bin/llvm-nm" "$lib" 2>/dev/null) || { echo "ARM64X: llvm-nm cannot read $lib" >&2; exit 1; }
    guards=$(echo "$symbols" | awk '{ print $NF }' \
      | grep -E '^#?(\?\?_B|\?\$TSS|_Init_thread_|_ZGV|__cxa_guard_)' | sort -u)
    [ "$guards" = "$guard" ] || {
      echo "ARM64X: $lib carries the static guards [$guards] where the C++ built with -fno-threadsafe-statics" \
        "may only carry [$guard]" >&2
      exit 1
    }
    echo "$symbols" | grep -qE ' T #?_malloc_thread_cleanup$' || {
      echo "ARM64X: $lib defines no _malloc_thread_cleanup, so no thread exit would tear its allocator down" >&2; exit 1
    }
    ! echo "$symbols" | grep -qE ' #?__tlregdtor$' || {
      echo "ARM64X: $lib registers a thread_local destructor through __tlregdtor, which llvm-mingw never runs" >&2
      exit 1
    }
  done
fi
res=
if [ -n "$rc" ]; then
  res=${out%.dll}.res
  "$bin/llvm-rc" /nologo /fo "$res" -- "$rc" # --: an absolute path would read as an option
fi
libs=
for l in mingw32 mingwex ucrt kernel32 mincore user32 advapi32 gdi32 ws2_32 userenv bcrypt dbghelp; do
  libs="$libs $crt/lib$l.a"
done
# shellcheck disable=SC2086  # $res and $libs are lists
"$bin/lld-link" -lldmingw /dll /machine:arm64x /nodefaultlib /opt:ref,icf \
  /entry:DllMainCRTStartup /def:"$def" /defarm64native:"$def" \
  /debug /pdb:"${out%.dll}.pdb" /pdbaltpath:%_PDB% /out:"$out" \
  "$crt/dllcrt2.o" "$arm64" "$arm64ec" $res $libs "$ntdll"
"$winebuild" --builtin "$out"
