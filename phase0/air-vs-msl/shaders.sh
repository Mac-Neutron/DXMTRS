#!/bin/sh
# shaders.sh <macneutron-root> <out>: E3's MSL side (DXMTRS Phase 0). Each dxc line of MacNeutron's
# dxmt/tests/shaders/compile.sh again, with -spirv, into <out>/spv/<its -Fo name>.spv, and SPIRV-Cross's MSL 3.2 of
# each into <out>/msl/<name>.metal. DXC runs as compile.sh runs it: the pinned dxc.exe under the frozen Rosetta
# reference's Wine; SPIRV-Cross is <root>/build/phase0/spirv-cross-build/spirv-cross. Prints a line per failure and
# the counts.
set -eu
R=$(cd "$1" && pwd)
mkdir -p "$2/spv" "$2/msl"
O=$(cd "$2" && pwd)
SC=$R/build/phase0/spirv-cross-build/spirv-cross
DXC=$R/build/dxmt-src/dxc/bin/x64/dxc.exe
REF="${MACNEUTRON_REFERENCE:-$HOME/Library/Application Support/MacNeutron Reference/rosetta-tool}"
WINE="$REF/Libraries/Wine/bin/wine"
export WINEPREFIX="${TMPDIR:-/tmp}/macneutron dxc" WINEDEBUG=-all
cd "$R/dxmt/tests/shaders" # relative paths: dxc.exe would read a leading / as an option
rel=$(python3 -I -c 'import os, sys; print(os.path.relpath(sys.argv[1]))' "$O/spv")
spv=0 msl=0
grep '^dxc -T' compile.sh > "$O/lines.txt"
while read -r line; do
  eval "set -- $line"
  shift # dxc
  args="" name=""
  while [ $# -gt 0 ]; do
    if [ "$1" = -Fo ]; then name=${2%.dxil}; shift 2; continue; fi
    args="$args '$1'"; shift
  done
  if eval "\"\$WINE\" \"\$DXC\" -spirv -fspv-target-env=vulkan1.2 -Fo \"$rel/$name.spv\" $args" > "$O/spv/$name.err" 2>&1; then
    spv=$((spv + 1))
  else
    echo "dxc-fail $name: $(grep -m1 -i error "$O/spv/$name.err" | cut -c1-120)"; continue
  fi
  if "$SC" --msl --msl-version 30200 "$O/spv/$name.spv" --output "$O/msl/$name.metal" > "$O/msl/$name.err" 2>&1; then
    msl=$((msl + 1))
  else
    echo "msl-fail $name: $(head -1 "$O/msl/$name.err" | cut -c1-120)"
  fi
done < "$O/lines.txt"
echo "shaders: $(wc -l < "$O/lines.txt" | tr -d ' ') dxc lines, $spv SPIR-V, $msl MSL"
# The harness's manifest (air-vs-msl.mm): a compute shader alone, a vertex shader alone (rasterization off), a pixel
# shader with its .hlsl's first vertex shader; only names both paths have (AIR from dxil-translate --out in <out>/air).
while read -r line; do
  eval "set -- $line"
  shift; T= name= src=
  while [ $# -gt 0 ]; do
    case $1 in -T) T=$2; shift 2 ;; -Fo) name=${2%.dxil}; shift 2 ;; *.hlsl) src=$1; shift ;; *) shift ;; esac
  done
  have() { [ -f "$O/msl/$1.metal" ] && [ -f "$R/build/phase0/air-15/$1.dxil.metallib" ]; }
  have "$name" || continue
  case $T in
    cs_*) echo "compute $name" ;;
    vs_*) echo "vertex $name"; [ -n "$(grep -l . "$O/first-vs-${src%.hlsl}" 2> /dev/null)" ] || echo "$name" > "$O/first-vs-${src%.hlsl}" ;;
    ps_*) vs=$(cat "$O/first-vs-${src%.hlsl}" 2> /dev/null || true); [ -n "$vs" ] && echo "render $vs $name" ;;
  esac
done < "$O/lines.txt" > "$O/manifest.txt"
rm -f "$O"/first-vs-*
echo "manifest: $(wc -l < "$O/manifest.txt" | tr -d ' ') pipelines"
