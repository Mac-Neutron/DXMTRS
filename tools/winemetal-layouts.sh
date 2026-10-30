#!/bin/sh
# winemetal-layouts.sh <llvm-mingw-bin>: prints winemetal-wire's src/layouts.rs, the compile-time asserts that pin the
# crate's hand-written mirrors to winemetal's C:
#   sh tools/winemetal-layouts.sh "$(sh dxmt/toolchain.sh)" > mtld3d/unix/winemetal-wire/src/layouts.rs
# Per target the crates build for, from clang's -fdump-record-layouts: each mirrored struct's size, each field's offset,
# each bitfield's shift and width (Apple clang for aarch64-apple-darwin; llvm-mingw's clang, -ffreestanding, for the
# four Windows targets). Once, from Apple clang: each enum value of winemetal.h and each SM50_BINDING_INDEX_*, each
# integer macro of the headers (-E -dM), the swizzle macros on every swizzled pixel format, and each slot number
# (winemetal_thunks.c's UNIX_CALLs, airconv_unixcalls, WMTForward's). The headers are read with -DDXMT_GUEST_VM, which
# only adds declarations. Names take Rust casing: a struct's snake_case becomes CamelCase (WMT names stay),
# WMTMemoryPointer and WMTConstMemoryPointer are MemPtr, fields are snake_case (originX: origin_x, type: r#type),
# constants SCREAMING_SNAKE_CASE (WMTPixelFormatBGRX8Unorm: WMT_PIXEL_FORMAT_BGRX8_UNORM). A struct's bitfields are
# one integer field, bits, with <FIELD>_SHIFT and <FIELD>_WIDTH constants. Fails when a struct of winemetal_thunks.h or
# airconv_thunks.h is missing from a dump or has no mirror, when a mirror or a constant names nothing in the C, on a
# bitfield it cannot place, and when the slots are not 0..157 once each or differ from slot.rs's. A compile_error! stops a
# build for any other target.
set -eu
[ $# -eq 1 ] || { echo "usage: winemetal-layouts.sh <llvm-mingw-bin>" >&2; exit 2; }
bin=$1
root=$(cd "$(dirname "$0")/.." && pwd)
wm=$root/src/winemetal
wire=$root/mtld3d/unix/winemetal-wire/src
t=$(mktemp -d)
trap 'rm -rf "$t"' EXIT
fail() { echo "winemetal-layouts.sh: $*" >&2; exit 1; }

names='
function screaming(s,   o, i, c, p, n) {
  o = ""
  for (i = 1; i <= length(s); i++) {
    c = substr(s, i, 1); p = substr(s, i - 1, 1); n = substr(s, i + 1, 1)
    if (i > 1 && c ~ /[A-Z]/ && (p ~ /[a-z]/ || (p ~ /[A-Z0-9]/ && n ~ /[a-z]/))) o = o "_"
    o = o c
  }
  o = toupper(o); gsub(/__+/, "_", o); return o
}
function num(v,   o) {
  if (length(v) < 6) return v
  for (o = ""; length(v) > 3; v = substr(v, 1, length(v) - 3)) o = "_" substr(v, length(v) - 2) o
  return v o
}
function field(s) {
  s = tolower(screaming(s))
  return s ~ /^(type|ref|in|fn|mod|use|move|match|loop|impl|self|struct|enum|where|let|mut|static|const|box)$/ ? \
    "r#" s : s
}
function rust(s,   o, i, k, w) {
  if (s == "WMTMemoryPointer" || s == "WMTConstMemoryPointer") return "MemPtr"
  if (s !~ /_/) return s
  k = split(s, w, "_"); o = ""
  for (i = 1; i <= k; i++) o = o toupper(substr(w[i], 1, 1)) substr(w[i], 2)
  return o
}'

flags="-DDXMT_GUEST_VM -I$wm -I$root/src/airconv"
printf '#include "winemetal_thunks.h"\n#include "airconv_thunks.h"\n' >"$t/tu.c"
apple() { xcrun clang -target aarch64-apple-darwin $flags "$@"; }

structs() { /usr/bin/grep -hoE '^struct [A-Za-z_][A-Za-z0-9_]* \{' "$@" | awk '{print $2}'; }
structs "$wm/winemetal_thunks.h" "$wm/airconv_thunks.h" >"$t/strict"
structs "$wm/winemetal_thunks.h" "$wm/airconv_thunks.h" "$wm/winemetal.h" | awk "$names"'{print $0, rust($0)}' >"$t/all"
/usr/bin/grep -oE '^pub struct [A-Za-z_][A-Za-z0-9_]*' "$wire/lib.rs" | awk '{print $3}' >"$t/mirrors"
awk 'NR == FNR { c[$2] = 1; next } !($0 in c) { print "mirror " $0 " names no C struct"; exit 1 }' \
  "$t/all" "$t/mirrors" || fail "$(tail -1 "$t/mirrors")"

cat <<'EOF'
//! winemetal's C layouts, asserted against the mirrors on each target the crates build for.
//!
//! Written by `tools/winemetal-layouts.sh` from clang's record layouts and AST: regenerate it,
//! never edit it.

use core::mem::offset_of;
EOF

# The targets and their cfgs. Records: S <c> <size>, F <c> <field> <offset>, B <c> <field> <bit> <width>, E <message>.
targets='aarch64-apple-darwin all(target_arch = "aarch64", target_os = "macos")
aarch64-w64-mingw32 all(target_arch = "aarch64", target_os = "windows")
arm64ec-w64-mingw32 all(target_arch = "arm64ec", target_os = "windows")
x86_64-w64-mingw32 all(target_arch = "x86_64", target_os = "windows")
i686-w64-mingw32 all(target_arch = "x86", target_os = "windows")'
printf '%s\n' "$targets" |
while read -r target cfg; do
  case $target in
  *-apple-*) apple -fsyntax-only -Xclang -fdump-record-layouts-complete "$t/tu.c" ;;
  *) "$bin/$target-clang" -ffreestanding $flags -fsyntax-only -Xclang -fdump-record-layouts-complete "$t/tu.c" ;;
  esac >"$t/dump"
  awk '
    /^\*\*\* Dumping AST Record Layout/ { rec = ""; next }
    rec == "" && /^ +0 \| / {
      rec = "-"
      if ($3 == "struct" && NF == 4 && !($4 in seen)) { rec = $4; seen[rec] = 1; run = 0; after = 0 }
      next
    }
    rec == "" || rec == "-" { next }
    /\| \[sizeof=/ { s = $0; sub(/.*sizeof=/, "", s); sub(/,.*/, "", s); print "S", rec, s; rec = "-"; next }
    /^ *[0-9:-]+ \|   [^ ]/ {
      if ($1 ~ /:/) {
        if (after) print "E", rec, "has two bitfield runs"
        split($1, a, /[:-]/); print "B", rec, $NF, a[1] * 8 + a[2], a[3] - a[2] + 1; run = 1
      } else {
        if (run) after = 1
        print "F", rec, $NF, $1
      }
    }' "$t/dump" >"$t/facts"
  awk "$names"'
    FILENAME == ARGV[1] { strict[$1] = 1; next }
    FILENAME == ARGV[2] { mirror[$1] = 1; next }
    $1 == "E" { print "E " $2 " " $3 " " $4 " " $5 " " $6; next }
    $1 == "S" { laid[rust($2)] = 1; found[$2] = 1 }
    !(rust($2) in mirror) { next }
    $1 == "S" { print "    assert!(size_of::<crate::" rust($2) ">() == " $3 ");" }
    $1 == "F" {
      f = rust($2) == "MemPtr" && $3 == "high_part" ? "high" : field($3)
      print "    assert!(offset_of!(crate::" rust($2) ", " f ") == " $4 ");"
    }
    $1 == "B" {
      c = "crate::" rust($2) "::" screaming($3)
      print "    assert!(offset_of!(crate::" rust($2) ", bits) * 8 + " c "_SHIFT as usize == " $4 ");"
      print "    assert!(" c "_WIDTH == " $5 ");"
    }
    END {
      for (s in strict) if (!(s in found)) print "E " s " is missing from the dump"
      for (s in strict) if (!(rust(s) in mirror)) print "E " s " has no mirror (" rust(s) ")"
      for (m in mirror) if (!(m in laid)) print "E mirror " m " has no layout"
    }' "$t/strict" "$t/mirrors" "$t/facts" >"$t/out"
  /usr/bin/grep -m1 '^E ' "$t/out" | sed "s/^E /$target: /" >"$t/err" || true
  [ ! -s "$t/err" ] || fail "$(cat "$t/err")"
  printf '\n#[cfg(%s)]\nconst _: () = {\n' "$cfg"
  awk '!seen[$0]++' "$t/out"
  printf '};\n'
done

# Any other target has no layout facts: the crate does not build there.
printf '\n#[cfg(not(any(\n'
printf '%s\n' "$targets" | sed 's/^[^ ]* \(.*\)/    \1,/'
printf ')))]\ncompile_error!("winemetal-wire: no layout facts for this target");\n'

# Constants: every enumerator of winemetal.h (WMTWsi*, the native hooks', aside) and SM50_BINDING_INDEX_*, valued by an
# enum of probes; airconv's unix_sm50_* are slots.
apple -fsyntax-only -Xclang -ast-dump -fno-color-diagnostics "$t/tu.c" |
  awk '/EnumConstantDecl/ { sub(/ '"'"'.*/, ""); print $NF }' >"$t/enumerators"
tr -cs 'A-Za-z0-9_' '\n' <"$wm/winemetal.h" | sort -u >"$t/words"
awk "$names"'
  FILENAME == ARGV[1] { w[$1] = 1; next }
  /^unix_sm50_/ { print $1, "slot::" toupper(substr($1, 6)); next }
  ($1 in w && $1 !~ /^WMTWsi/) || /^SM50_BINDING_INDEX_/ { print $1, screaming($1) }
' "$t/words" "$t/enumerators" >"$t/consts"
apple -E -dM "$t/tu.c" | awk 'NF == 3 && $3 ~ /^[0-9]+$/ { print $2, $3 }' >"$t/defines"
awk 'NR == FNR { d[$1] = $2; next } $1 == "#define" && ($2 in d) && !seen[$2]++ { print $2, d[$2] }' "$t/defines" \
  "$wm/winemetal.h" "$wm/winemetal_thunks.h" "$wm/airconv_thunks.h" "$root/src/airconv/airconv_public.h" >"$t/macros"
{
  cat "$t/tu.c"
  echo 'enum wire_probe : unsigned long long {'
  awk '{ print "  wire_" NR " = " $1 "," }' "$t/consts"
  awk '$1 ~ /^WMTPixelFormat/ { for (i = 0; i < 5; i++) print "  wire_" $1 "_" i " = " \
    (i == 4 ? "ORIGINAL_FORMAT" : "GET_SWIZZLE_" (i == 0 ? "RED" : i == 1 ? "GREEN" : i == 2 ? "BLUE" : "ALPHA")) \
    "(" $1 ")," }' "$t/consts"
  awk '$1 ~ /^WMTTextureSwizzle/ { print "  wire_encode_" $1 " = ENCODE_FORMAT_SWIZZLE(WMTPixelFormatRGBA8Unorm, " \
    $1 ", " $1 ", " $1 ", " $1 ")," }' "$t/consts"
  echo '};'
} >"$t/probe.c"
apple -fsyntax-only -Xclang -ast-dump -Xclang -ast-dump-filter -Xclang wire_probe -fno-color-diagnostics "$t/probe.c" |
  awk '/EnumConstantDecl/ { sub(/ '"'"'.*/, ""); n = $NF; next }
    /value: Int / && !(n in v) { sub(/.*value: Int /, ""); v[n] = 1; print n, $1 }' \
  >"$t/values"
awk '/^[A-Za-z_]/ && match($0, /[A-Za-z_][A-Za-z0-9_]*\(/) { f = substr($0, RSTART, RLENGTH - 1) }
  match($0, /UNIX_CALL\([0-9]+,|call != [0-9]+/) { s = substr($0, RSTART, RLENGTH); gsub(/[^0-9]/, "", s); print f, s }
' "$wm/winemetal_thunks.c" >"$t/slots"

printf '\nconst _: () = {\n'
awk "$names"'
  FILENAME == ARGV[1] { v[$1] = $2; next }
  FILENAME == ARGV[2] {
    if ($2 ~ /^slot::/) { slot[$2] = v["wire_" FNR]; next }
    print "    assert!(crate::" $2 " == " num(v["wire_" FNR]) ");"; c[$1] = $2; val[$1] = v["wire_" FNR]; next
  }
  FILENAME == ARGV[3] { print "    assert!(crate::" $1 " == " num($2) ");"; next }
  { slot["slot::" screaming($1)] = $2 }
  END {
    for (n in v) if (n ~ /^wire_WMTPixelFormat/) {
      k = split(substr(n, 6), a, "_"); f = a[1]; for (i = 2; i < k; i++) f = f "_" a[i]; i = a[k]
      if (v["wire_" f "_4"] == val[f]) continue
      fn = i == 4 ? "original_format" : "get_swizzle_" (i == 0 ? "red" : i == 1 ? "green" : i == 2 ? "blue" : "alpha")
      print "    assert!(crate::" fn "(crate::" c[f] ") == " num(v[n]) ");"
    } else if (n ~ /^wire_encode_/) {
      s = "crate::" c[substr(n, 13)]
      print "    assert!(crate::encode_format_swizzle(crate::WMT_PIXEL_FORMAT_RGBA8_UNORM, " s ", " s ", " s ", " s \
        ") == " num(v[n]) ");"
    }
    for (s in slot) print "    assert!(crate::" s " == " slot[s] ");"
  }' "$t/values" "$t/consts" "$t/macros" "$t/slots" >"$t/consts.rs"
sort "$t/consts.rs"
printf '};\n'

# Each slot once, 0..157, and slot.rs's names exactly those.
sed -n 's/.*crate::slot::\([A-Z0-9_]*\) == \([0-9]*\));/\1 \2/p' "$t/consts.rs" | sort -k2n >"$t/slotlist"
awk '$2 != NR - 1 { exit 1 } END { if (NR != 158) exit 1 }' "$t/slotlist" ||
  fail "slots are not 0..157 once each: $(awk '{print $2}' "$t/slotlist" | tr '\n' ' ')"
awk '{print $1}' "$t/slotlist" | sort >"$t/slotnames"
/usr/bin/grep -oE '^pub const [A-Z0-9_]+' "$wire/slot.rs" | awk '{print $3}' | sort | diff "$t/slotnames" - >&2 ||
  fail "slot.rs's constants differ from the slots (< the C, > slot.rs)"
sed -n 's/.*assert!(crate::\([A-Z0-9_]*\) == .*/\1/p' "$t/consts.rs" | sort >"$t/constnames"
/usr/bin/grep -oE '^pub const [A-Z0-9_]+' "$wire/lib.rs" | awk '{print $3}' | sort |
  comm -13 "$t/constnames" - >"$t/extra"
[ ! -s "$t/extra" ] || fail "lib.rs constants name nothing in the C: $(tr '\n' ' ' <"$t/extra")"
