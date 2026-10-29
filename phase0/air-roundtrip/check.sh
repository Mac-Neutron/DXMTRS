#!/bin/sh
# check.sh <phase0-build-dir>: air-roundtrip's test (E2). In <dir>: air-15 (dxil-translate --out), llvm-23 (tools),
# air-roundtrip-23 and air-roundtrip-15. The original module's pointer types are typed (bitcode TYPE_CODE_POINTER
# records); rewritten by either build, they are opaque (OPAQUE_POINTER) and none typed. llvm-bcanalyzer reads the
# records as written (llvm-dis of LLVM 23 would print any of them as opaque); it names type code 25
# (OPAQUE_POINTER) "UnknownCode25". Only the type table is read.
set -u
B=$1 M=$1/air-15/compute.cs.dxil.metallib T=$(mktemp -d) fail=0
types() {
  "$B/air-roundtrip-23" extract "$1" "$T/m.bc" && "$B/llvm-23/bin/llvm-bcanalyzer" -dump "$T/m.bc" \
    | awk '/<TYPE_BLOCK_ID/ { p = 1 } p; /<\/TYPE_BLOCK_ID>/ { p = 0 }'
}
typed() { types "$1" | grep -c '<POINTER '; }
opaque() { types "$1" | grep -cE '<(OPAQUE_POINTER|UnknownCode25) '; }
expect() { if [ "$2" = "$3" ]; then echo "ok   $1"; else echo "FAIL $1: got [$2], want [$3]"; fail=1; fi; }
o=$(typed "$M"); expect "the original has typed pointers" "$([ "$o" -gt 0 ] && echo yes)" yes
for v in 23 15; do
  "$B/air-roundtrip-$v" rewrite "$M" "$T/$v.metallib" || { echo "FAIL air-roundtrip-$v rewrite"; fail=1; continue; }
  expect "LLVM $v writes opaque pointers" "$(typed "$T/$v.metallib"):$([ "$(opaque "$T/$v.metallib")" -gt 0 ] && echo yes)" 0:yes
done
rm -rf "$T"; exit $fail
