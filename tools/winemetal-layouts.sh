#!/bin/sh
# winemetal-layouts.sh <llvm-mingw-bin>: prints winemetal-wire's src/layouts.rs, the compile-time asserts that pin the
# crate's hand-written mirrors to winemetal's C:
#   sh tools/winemetal-layouts.sh "$(sh dxmt/toolchain.sh)" > src/winemetal/wire/src/layouts.rs
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
#
# With --protos it prints winemetal-pe's src/protos.rs instead, the compile-time asserts that pin the PE thunks'
# signatures to winemetal.h's prototypes:
#   sh tools/winemetal-layouts.sh --protos "$(sh dxmt/toolchain.sh)" > src/winemetal/src/protos.rs
# Per Windows target, from llvm-mingw's clang -ast-dump=json (-DDXMT_GUEST_VM, read with jq): every WINEMETAL_API
# function (the dllimport ones: winemetal.h's DXMT_NATIVE and non-Windows blocks are out) as the type of an
# unsafe extern "C" fn the export of that name must coerce to. Each type is the C's on that target: a typedef resolved
# to its integer (intptr_t is isize), an enum to its underlying type (u32 for one without a fixed type, after checking
# that none of its values is negative), a struct to its winemetal-wire mirror. Fails on a type it cannot map.
#
# With --airconv (no other argument) it prints airconv's src/abi/layouts.rs instead, the compile-time asserts that pin
# the crate's mirrors in src/abi.rs to airconv_public.h and dxil/dxil_public.h:
#   sh tools/winemetal-layouts.sh --airconv > src/airconv/src/abi/layouts.rs
# For aarch64-apple-darwin, the one target airconv builds for, from Apple clang's -fdump-record-layouts: each mirrored
# struct's size, each field's offset (an anonymous union is the field u, its type <Struct>Union, with its own members),
# each bitfield's shift and width; and each enumerator of the two headers, valued by an enum of probes. Names take Rust
# casing: a struct's SCREAMING_SNAKE name becomes CamelCase (SM50_SHADER_COMMON_DATA: Sm50ShaderCommonData), fields
# are snake_case as above, enumerators keep their names. Fails when a struct of the headers (dxil_public.h's typedef'd
# ones included) is missing from the dump or has no mirror (a #[repr(C)] struct or union of abi.rs), when a mirror
# names nothing in the C, on a bitfield it cannot place, and when the enumerators and abi.rs's constants differ.
set -eu
protos= airconv=
[ "${1:-}" != --protos ] || { protos=1; shift; }
[ "${1:-}" != --airconv ] || { airconv=1; shift; }
usage() { echo "usage: winemetal-layouts.sh [--protos] <llvm-mingw-bin> | --airconv" >&2; exit 2; }
if [ -n "$airconv" ]; then [ $# -eq 0 ] || usage; else [ $# -eq 1 ] || usage; fi
bin=${1:-}
root=$(cd "$(dirname "$0")/.." && pwd)
wm=$root/src/winemetal
wire=$root/src/winemetal/wire/src
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

if [ -n "$airconv" ]; then
  ac=$root/src/airconv
  abi=$ac/src/abi.rs
  printf '#include "airconv_public.h"\n#include "dxil/dxil_public.h"\n' >"$t/tu.c"
  apple() { xcrun clang -target aarch64-apple-darwin "-I$ac" "$@"; }
  camel='
  function camel(s,   o, i, k, w) {
    k = split(s, w, "_"); o = ""
    for (i = 1; i <= k; i++) o = o toupper(substr(w[i], 1, 1)) tolower(substr(w[i], 2))
    return o
  }'
  # The headers' structs: S <c> (a struct tag), T <line> <c> (a typedef'd anonymous struct of dxil_public.h, by the
  # line its definition opens on, which is how clang names it).
  {
    /usr/bin/grep -hoE '^struct [A-Za-z_][A-Za-z0-9_]* \{' "$ac/airconv_public.h" "$ac/dxil/dxil_public.h" |
      awk '{print "S", $2}'
    awk '/^typedef struct \{/ { open = FNR } open && /^\} [A-Za-z_][A-Za-z0-9_]*;/ {
      n = $2; sub(/;/, "", n); print "T", open, n; open = 0 }' "$ac/dxil/dxil_public.h"
  } >"$t/cstructs"
  # The mirrors: abi.rs's #[repr(C)] structs and unions.
  awk '/^#\[repr\(C\)\]/ { r = 1; next }
    /^pub (struct|union) / { if (r) { n = $3; sub(/[<{].*/, "", n); print n }; r = 0; next }
    !/^(#|\/\/)/ { r = 0 }' "$abi" >"$t/mirrors"
  apple -fsyntax-only -Xclang -fdump-record-layouts-complete "$t/tu.c" >"$t/dump"
  # Records: S <rust> <size>, F <rust> <field> <offset>, B <rust> <field> <bit> <width>, E <message>.
  awk "$names$camel"'
    FILENAME == ARGV[1] { if ($1 == "S") strict[$2] = 1; else { td[$2] = $3; strict[$3] = 1 }; next }
    FILENAME == ARGV[2] { mirror[$1] = 1; next }
    /^\*\*\* Dumping AST Record Layout/ { rec = ""; next }
    rec == "" && /^ +0 \| / {
      rec = "-"; line = $0; sub(/^ +0 \| /, "", line); sub(/ +$/, "", line); c = ""
      if (line ~ /^struct [A-Za-z_][A-Za-z0-9_]*$/) { c = substr(line, 8); r = camel(c) }
      else if (line ~ /^struct \(unnamed at .*dxil_public\.h:[0-9]+:[0-9]+\)$/) {
        k = split(line, a, ":"); c = td[a[k - 1]]; r = camel(c)
      } else if (line ~ /^union [A-Za-z_][A-Za-z0-9_]*::\(unnamed at /) {
        c = substr(line, 7); sub(/::.*/, "", c); r = camel(c) "Union"
      }
      if (c == "" || !(c in strict) || (r in seen)) next
      rec = r; seen[r] = 1; run = 0; after = 0
      if (r !~ /Union$/) found[c] = 1
      next
    }
    rec == "" || rec == "-" { next }
    /\| \[sizeof=/ { s = $0; sub(/.*sizeof=/, "", s); sub(/,.*/, "", s); print "S", rec, s; rec = "-"; next }
    /^ *[0-9:-]+ \|   [^ ]/ {
      if ($1 ~ /:/) {
        if (after) print "E", rec, "has two bitfield runs"
        split($1, b, /[:-]/); print "B", rec, $NF, b[1] * 8 + b[2], b[3] - b[2] + 1; run = 1
      } else {
        if (run) after = 1
        print "F", rec, ($0 ~ /\|   union [A-Za-z_][A-Za-z0-9_]*::\(anonymous at /) ? "u" : field($NF), $1
      }
    }
    END {
      for (s in strict) if (!(s in found)) print "E " s " is missing from the dump"
      for (r in seen) if (!(r in mirror)) print "E " r " has no mirror in abi.rs"
      for (m in mirror) if (!(m in seen)) print "E mirror " m " names no C struct"
    }' "$t/cstructs" "$t/mirrors" "$t/dump" >"$t/facts"
  /usr/bin/grep -m1 '^E ' "$t/facts" | sed 's/^E //' >"$t/err" || true
  [ ! -s "$t/err" ] || fail "$(cat "$t/err")"

  # Enumerators: every EnumConstantDecl of the two headers (their words), valued by an enum of probes.
  apple -fsyntax-only -Xclang -ast-dump -fno-color-diagnostics "$t/tu.c" |
    awk '/EnumConstantDecl/ { sub(/ '"'"'.*/, ""); print $NF }' >"$t/enumerators"
  cat "$ac/airconv_public.h" "$ac/dxil/dxil_public.h" | tr -cs 'A-Za-z0-9_' '\n' | sort -u >"$t/words"
  awk 'NR == FNR { w[$1] = 1; next } ($1 in w)' "$t/words" "$t/enumerators" | sort -u >"$t/consts"
  {
    cat "$t/tu.c"
    echo 'enum airconv_probe : unsigned long long {'
    awk '{ print "  airconv_" NR " = " $1 "," }' "$t/consts"
    echo '};'
  } >"$t/probe.c"
  apple -fsyntax-only -Xclang -ast-dump -Xclang -ast-dump-filter -Xclang airconv_probe -fno-color-diagnostics \
    "$t/probe.c" |
    awk '/EnumConstantDecl/ { sub(/ '"'"'.*/, ""); n = $NF; next }
      /value: Int / && !(n in v) { sub(/.*value: Int /, ""); v[n] = 1; print n, $1 }' >"$t/values"
  /usr/bin/grep -oE '^pub const [A-Z0-9_]+' "$abi" | awk '{print $3}' | sort >"$t/rconsts"
  comm -3 "$t/consts" "$t/rconsts" >"$t/cdiff"
  [ ! -s "$t/cdiff" ] ||
    fail "the enumerators and abi.rs's constants differ (left: the C only, right: abi.rs only): $(tr '\n\t' '  ' <"$t/cdiff")"

  printf '%s\n' \
    "//! airconv's C layouts and constants, asserted against the mirrors in \`abi.rs\`." \
    '//!' \
    '//! Written by `tools/winemetal-layouts.sh --airconv` from clang'"'"'s record layouts and AST:' \
    '//! regenerate it, never edit it.' \
    '' \
    'use core::mem::offset_of;' \
    '' \
    '#[cfg(all(target_arch = "aarch64", target_os = "macos"))]' \
    'const _: () = {'
  awk '
    $1 == "S" { print "    assert!(size_of::<crate::abi::" $2 ">() == " $3 ");" }
    $1 == "F" { print "    assert!(offset_of!(crate::abi::" $2 ", " $3 ") == " $4 ");" }
    $1 == "B" {
      c = "crate::abi::" $2 "::" toupper($3)
      print "    assert!(offset_of!(crate::abi::" $2 ", bits) * 8 + " c "_SHIFT as usize == " $4 ");"
      print "    assert!(" c "_WIDTH == " $5 ");"
    }' "$t/facts"
  printf '%s\n' '};' '' '#[cfg(not(all(target_arch = "aarch64", target_os = "macos")))]' \
    'compile_error!("airconv: no layout facts for this target");' '' 'const _: () = {'
  awk "$names"'NR == FNR { name["airconv_" FNR] = $1; next }
    ($1 in name) { print "    assert!(crate::abi::" name[$1] " == " num($2) ");" }' "$t/consts" "$t/values" | sort
  printf '};\n'
  exit 0
fi

if [ -n "$protos" ]; then
  cat <<'EOF'
//! winemetal.h's prototypes, asserted against the exports on each Windows target.
//!
//! Written by `tools/winemetal-layouts.sh --protos` from clang's AST of every `WINEMETAL_API`
//! function: regenerate it, never edit it.

use core::ffi::{c_char, c_void};

use winemetal_wire as wire;
EOF
  printf '#include "winemetal.h"\n' >"$t/api.c"
  printf '%s\n' \
    'aarch64-w64-mingw32 all(target_arch = "aarch64", target_os = "windows")' \
    'arm64ec-w64-mingw32 all(target_arch = "arm64ec", target_os = "windows")' \
    'x86_64-w64-mingw32 all(target_arch = "x86_64", target_os = "windows")' \
    'i686-w64-mingw32 all(target_arch = "x86", target_os = "windows")' |
  while read -r target cfg; do
    "$bin/$target-clang" -ffreestanding $flags -fsyntax-only -Xclang -ast-dump=json "$t/api.c" >"$t/ast.json"
    # Records: T <typedef>\t<type>, E <enum>\t<underlying type, or "-" when not fixed>, N <enum with a negative value>,
    # F <function>\t<return type>\t<parameter type>...
    jq -r '.inner[] |
      if .kind == "TypedefDecl" then ["T", .name, .type.qualType] | join("\t")
      elif .kind == "EnumDecl" and .name then
        (["E", .name, (.fixedUnderlyingType.qualType // "-")] | join("\t")),
        (if [.. | .value? // empty | strings | select(startswith("-"))] | length > 0 then "N\t" + .name
         else empty end)
      elif .kind == "FunctionDecl" and ((.inner // []) | any(.kind == "DLLImportAttr")) then
        (["F", .name, (.type.qualType | sub(" ?\\(.*$"; ""))] +
          [.inner[] | select(.kind == "ParmVarDecl") | .type.qualType]) | join("\t")
      else empty end' "$t/ast.json" >"$t/decls"
    printf '\n#[cfg(%s)]\nconst _: () = {\n' "$cfg"
    awk -F '\t' "$names"'
      function base(s) {
        if (s == "intptr_t") return "isize"
        if (s == "uintptr_t") return "usize"
        while (s in td) s = td[s]
        if (s ~ /^enum /) {
          s = substr(s, 6)
          if (!(s in en)) bad = bad " enum " s
          if (s in neg) bad = bad " enum " s " has a negative value"
          s = en[s] == "-" ? "unsigned int" : en[s]
          while (s in td) s = td[s]
        }
        if (s ~ /^struct /) return "wire::" rust(substr(s, 8))
        if (s in prim) return prim[s]
        bad = bad " " s
        return s
      }
      function rs(s, ret,   c) {
        sub(/^ +/, "", s); sub(/ +$/, "", s)
        if (s ~ /\*$/) {
          sub(/ *\*$/, "", s)
          c = s ~ /^const /
          sub(/^const /, "", s)
          return (c ? "*const " : "*mut ") (s == "void" ? "c_void" : rs(s))
        }
        sub(/^const /, "", s)
        if (s == "void") return ret ? "" : "c_void"
        return base(s)
      }
      BEGIN {
        split("unsigned_long_long:u64 long_long:i64 unsigned_long:u32 long:i32 unsigned_int:u32 int:i32 " \
          "unsigned_short:u16 short:i16 unsigned_char:u8 signed_char:i8 char:c_char bool:bool float:f32 double:f64", \
          kv, " ")
        for (i in kv) { split(kv[i], p, ":"); gsub(/_/, " ", p[1]); prim[p[1]] = p[2] }
        prim["_Bool"] = "bool"
      }
      $1 == "T" { td[$2] = $3; next }
      $1 == "E" { en[$2] = $3; next }
      $1 == "N" { neg[$2] = 1; next }
      $1 == "F" {
        args = ""
        for (i = 4; i <= NF; i++) args = args (i > 4 ? ", " : "") rs($i)
        r = rs($3, 1)
        print "    let _: unsafe extern \"C\" fn(" args ")" (r == "" ? "" : " -> " r) " = crate::" $2 ";"
        n++
      }
      END {
        if (bad != "") { print "E cannot map:" bad; exit 1 }
        if (n != 145) { print "E " n " WINEMETAL_API functions, not 145"; exit 1 }
      }' "$t/decls" >"$t/out" || fail "$target: $(tail -1 "$t/out")"
    cat "$t/out"
    printf '};\n'
  done
  exit 0
fi
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
