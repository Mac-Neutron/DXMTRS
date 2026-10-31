# LLVM 23 (MacNeutron's air-23 fork, with the AIR bitcode writer), by architecture, for wine-arm64/build.sh (arm64) and
# `make llvm-nolto` (sourced); dxmt/tools.sh links the host tools against it. The caller defines die, UNIX_CPU and
# UNIX_LTO. Messages go to stderr.

LLVM_AIR_REPO=https://github.com/Mac-Neutron/llvm-project.git
LLVM_AIR_COMMIT=f8ae77ad2946b65daeaac3d7690c5baf0023ed71

# LLVM 23: static, Release (-O3), no targets or tools, assertions and zstd off, for the caller's UNIX_CPU, as ThinLTO
# bitcode when UNIX_LTO asks for it (winemetal.so's link optimizes the translator and LLVM together). Built once per
# install folder and recipe: <install>/.complete names the commit, CPU and LTO it was built from
# ($LLVM_AIR_COMMIT+<cpu>+<lto>). The source folder is used as is when its HEAD is $LLVM_AIR_COMMIT, and fetched shallow
# into <llvm-project>.tmp and moved into place otherwise.
build_llvm() {  # build_llvm <arch> <install> <llvm-project>
  _bl_want="$LLVM_AIR_COMMIT+${UNIX_CPU:-generic}+${UNIX_LTO:-nolto}" _bl_lto=Off
  [ -z "${UNIX_LTO:-}" ] || _bl_lto=Thin
  if [ -f "$2/.complete" ]; then
    [ "$(cat "$2/.complete")" != "$_bl_want" ] || return 0
    echo "dxmt: $2 was built from LLVM $(cat "$2/.complete"), not $_bl_want: building it again" >&2
    rm -rf "$2" "$2-build"
  fi
  if [ ! -d "$3/llvm" ] || [ "$(git -C "$3" rev-parse HEAD 2>/dev/null)" != "$LLVM_AIR_COMMIT" ]; then
    rm -rf "$3.tmp"
    mkdir -p "$3.tmp"
    (git -C "$3.tmp" init -q &&
      git -C "$3.tmp" fetch -q --depth 1 "$LLVM_AIR_REPO" "$LLVM_AIR_COMMIT" &&
      git -C "$3.tmp" checkout -q FETCH_HEAD) \
      || die "can't fetch llvm-project $LLVM_AIR_COMMIT"
    rm -rf "$3"; mv "$3.tmp" "$3"
  fi
  echo "dxmt: building LLVM $LLVM_AIR_COMMIT (a few minutes, once); log: $2.log" >&2
  { cmake -B "$2-build" -S "$3/llvm" -G Ninja \
      -DCMAKE_INSTALL_PREFIX="$2" -DCMAKE_OSX_ARCHITECTURES="$1" -DLLVM_HOST_TRIPLE="$1-apple-darwin" \
      -DLLVM_ENABLE_ASSERTIONS=Off -DLLVM_ENABLE_ZSTD=Off -DCMAKE_BUILD_TYPE=Release \
      -DLLVM_TARGETS_TO_BUILD="" -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD="" \
      -DLLVM_BUILD_TOOLS=Off -DLLVM_INCLUDE_TESTS=Off -DLLVM_INCLUDE_BENCHMARKS=Off -DLLVM_INCLUDE_EXAMPLES=Off \
      -DLLVM_ENABLE_BACKTRACES=Off -DLLVM_ENABLE_TERMINFO=Off \
      -DLLVM_VERSION_PRINTER_SHOW_HOST_TARGET_INFO=Off -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
      -DCMAKE_C_FLAGS="$UNIX_CPU" -DCMAKE_CXX_FLAGS="$UNIX_CPU" -DLLVM_ENABLE_LTO="$_bl_lto" &&
    cmake --build "$2-build" && cmake --install "$2-build"; } > "$2.log" 2>&1 \
    || die "LLVM build failed; see $2.log"
  echo "$_bl_want" > "$2/.complete"  # written last: an interrupted install is redone
}
