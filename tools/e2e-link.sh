#!/bin/sh
# e2e-link.sh: builds mtld3d's D3D9 e2e programs (crate mtld3d-tests) for x86_64-pc-windows-msvc and links them with
# llvm-mingw's lld-link and its lib*.a (no MSVC/xwin). usage: e2e-link.sh <cargo test args>, BUILD_DIR and MTLD3D_LLVM_MINGW in the environment.
# Cargo calls this script as the linker; it adds mingw's CRT and the MSVC-runtime shims (src/util/rust_pe_rt.c).
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
if [ "${E2E_LINK_STAGE:-}" = link ]; then
  bin=$E2E_LLVM_MINGW/bin crt=$E2E_LLVM_MINGW/x86_64-w64-mingw32/lib
  # rustc's bare MSVC import libs (kernel32.lib, ...) and /defaultlib:msvcrt are replaced by the lib*.a below
  for a; do shift; case $a in /defaultlib:*) continue ;; */*) ;; *.lib) continue ;; esac; set -- "$@" "$a"; done
  exec "$bin/lld-link" -lldmingw /nodefaultlib /libpath:"$crt" "$@" "$crt/crt2.o" "$E2E_RT_OBJ" \
    "$E2E_SHIM_OBJ" "$crt/libmingw32.a" "$crt/libmingwex.a" "$crt/libucrt.a" "$crt/libvcruntime140.a" "$crt/libkernel32.a" \
    "$crt/libuser32.a" "$crt/libgdi32.a" "$crt/libadvapi32.a" "$crt/libdbghelp.a" "$crt/libmincore.a" "$crt/libntdll.a" "$crt/libbcrypt.a" "$crt/libws2_32.a" "$crt/libuserenv.a" "$crt/libsynchronization.a" "$bin/../lib/clang/"*/lib/windows/libclang_rt.builtins-x86_64.a
fi
BUILD_DIR=${BUILD_DIR:?}
# MTLD3D_LLVM_MINGW names llvm-mingw's root, as for rust-check.sh and mingw-cc.sh; nothing is fetched or built here.
[ -x "${MTLD3D_LLVM_MINGW:-}/bin/x86_64-w64-mingw32-clang" ] && [ -x "$MTLD3D_LLVM_MINGW/bin/lld-link" ] || {
  echo "e2e-link.sh: MTLD3D_LLVM_MINGW must name an llvm-mingw root with bin/x86_64-w64-mingw32-clang and bin/lld-link" >&2
  exit 1
}
llvm=$MTLD3D_LLVM_MINGW/bin
export E2E_LLVM_MINGW=$MTLD3D_LLVM_MINGW
tgt=${CARGO_TARGET_DIR:-$BUILD_DIR/d3d9-cargo/e2e}
mkdir -p "$tgt/e2e-link"
export E2E_RT_OBJ=$tgt/e2e-link/rust_pe_rt.o
"$llvm/x86_64-w64-mingw32-clang" -c -O2 "$here/src/util/rust_pe_rt.c" -o "$E2E_RT_OBJ"
# What the test binaries unwind with and rust_pe_rt.c (panic=abort DLLs) doesn't carry: the MSVC type_info vtable that
# Rust's panic type descriptor points at (vcruntime140.dll's; __CxxFrameHandler3 matches the descriptors by name, so a
# dummy serves).
export E2E_SHIM_OBJ=$tgt/e2e-link/shim.o
"$llvm/x86_64-w64-mingw32-clang" -x assembler -c - -o "$E2E_SHIM_OBJ" <<'ASM'
.data
.globl "??_7type_info@@6B@"
"??_7type_info@@6B@": .quad 0
ASM
export E2E_LINK_STAGE=link MTLD3D_LLVM_MINGW=$E2E_LLVM_MINGW
eval "$(sh "$here/tools/mingw-cc.sh" env x86_64-pc-windows-msvc "$tgt/e2e-link/cc")" # snmalloc-sys's C++
PATH=$llvm:$(dirname "$(rustup which cargo --toolchain 1.99.0)"):$PATH exec cargo test --no-run -p mtld3d-tests --target x86_64-pc-windows-msvc \
  --config "target.x86_64-pc-windows-msvc.linker=\"$here/tools/e2e-link.sh\"" "$@"
