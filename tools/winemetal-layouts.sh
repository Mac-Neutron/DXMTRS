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
# With --decls it prints winemetal-sys's src/decls.rs, winemetal.h's functions declared for Rust callers:
#   sh tools/winemetal-layouts.sh --decls "$(sh dxmt/toolchain.sh)" > src/winemetal/sys/src/decls.rs
# --protos's function set, typed as --protos types it, printed once as an unsafe extern "C" block under cfg(windows)
# and again as unsafe extern "C-unwind" under cfg(not(windows)); the functions -DDXMT_GUEST_VM adds (the guest's)
# under cfg(feature = "guest-vm"). The block must come out the same from each Windows target's AST and from Apple
# clang's (long is 64-bit there), else it fails. Then, compiled on macOS only, the prototype checks of the native
# build's functions lib.rs declares by hand, printed as --seam-abi prints them: winemetal.h's WMTMemoryHooked and
# WMTGetWsiHooks (-DDXMT_NATIVE; a pointer to a winemetal struct without a mirror, WMTWsiHooks, is a c_void pointer) and
# the functions of src/nativemetal/win32_native.cpp's dxmt-abi block.
#
# With --seam-abi <header> <module path> it prints a seam's abi.rs, its C ABI asserted against its Rust exports:
#   sh tools/winemetal-layouts.sh --seam-abi src/dxmt/dxmt_format.hpp crate::format "$(sh dxmt/toolchain.sh)" \
#     > src/dxmt/src/format/abi.rs
# The header's lines between /* dxmt-abi begin */ and /* dxmt-abi end */ are C (a C++ header puts its extern "C" {
# before the begin line and its } after the end line), read after <stddef.h> and winemetal.h. Per target (the four
# Windows ones, then aarch64-apple-darwin, whose exports are "C-unwind") each of the block's functions is printed as
# const _: unsafe extern "C" fn(..) = <module path>::<name>; under that target's cfg. Types as --protos types them, a
# winemetal struct as its winemetal-wire mirror, size_t and uintptr_t as usize, ptrdiff_t and intptr_t as isize, long
# as the target's; a pointer to a struct winemetal.h doesn't define points to <module path>::<Name>, the seam's mirror
# (a snake_case name in CamelCase, an all-capitals word capitalised: MTL_DXGI_FORMAT_DESC is MtlDxgiFormatDesc). Fails
# on a type it cannot map and on a header with no block or no function in it.
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
#
# With --com <header> <module path> <Interface>... <llvm-mingw-bin> it prints COM vtable mirrors and per-target asserts:
#   sh tools/winemetal-layouts.sh --com dxgi1_6.h crate::dxgi_vtbl IUnknown IDXGIObject "$LM/bin"
# Reads <header> (a file path or a +/,-separated list of system headers) with -DCINTERFACE on the four Windows targets
# and on aarch64-apple-darwin (-DWIDL_EXPLICIT_AGGREGATE_RETURNS). Emits #[repr(C)] pub struct <I>Vtbl per interface
# with unsafe extern "system-unwind" fn slots, referenced by-value mirrors, and per-target size, offset, and
# stack-rounded parameter-byte asserts from clang's sizeof probe. Fails if any target disagrees on the vtable layout.
set -eu
usage() {
  echo "usage: winemetal-layouts.sh [--protos | --decls | --seam-abi <header> <module path>] <llvm-mingw-bin>" >&2
  echo "       winemetal-layouts.sh --com <header> <module path> <Interface>... <llvm-mingw-bin>" >&2
  echo "       winemetal-layouts.sh --airconv" >&2
  exit 2
}
case ${1:-} in
--protos | --decls) mode=${1#--}; shift ;;
--seam-abi) [ $# -eq 4 ] || usage; mode=seam header=$2 module=$3; shift 3 ;;
--com)
  [ $# -ge 5 ] || usage
  mode=com header=$2 module=$3
  shift 3
  ifaces=
  while [ $# -gt 1 ]; do
    ifaces="${ifaces:+$ifaces }$1"
    shift
  done
  ;;
--airconv) [ $# -eq 1 ] || usage; mode=airconv; shift ;;
*) mode=layouts ;;
esac
[ "$mode" = airconv ] || [ $# -eq 1 ] || usage
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
structs() { /usr/bin/grep -hoE '^struct [A-Za-z_][A-Za-z0-9_]* \{' "$@" | awk '{print $2}'; }

if [ "$mode" = airconv ]; then
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

if [ "$mode" != layouts ]; then
  pe='aarch64-w64-mingw32 all(target_arch = "aarch64", target_os = "windows")
arm64ec-w64-mingw32 all(target_arch = "arm64ec", target_os = "windows")
x86_64-w64-mingw32 all(target_arch = "x86_64", target_os = "windows")
i686-w64-mingw32 all(target_arch = "x86", target_os = "windows")'
  mac='all(target_arch = "aarch64", target_os = "macos")'
  # ast <target> <source> <clang flag>...: $t/recs, the records of clang's AST of <source> (read with jq):
  # T <typedef>\t<type>, E <enum>\t<underlying type, or "-" when not fixed>, N <enum with a negative value>,
  # F <function>\t<1 if dllimport, else 0>\t<return type>\t<parameter type>\t<parameter name>..., in source order.
  ast() {
    tgt=$1 src=$2
    shift 2
    case $tgt in
    *-apple-*) xcrun clang -target "$tgt" "$@" -fsyntax-only -Xclang -ast-dump=json "$src" ;;
    *) "$bin/$tgt-clang" -ffreestanding "$@" -fsyntax-only -Xclang -ast-dump=json "$src" ;;
    esac >"$t/ast.json"
    jq -r '.inner[] |
      if .kind == "TypedefDecl" then ["T", .name, .type.qualType] | join("\t")
      elif .kind == "EnumDecl" and .name then
        (["E", .name, (.fixedUnderlyingType.qualType // "-")] | join("\t")),
        (if [.. | .value? // empty | strings | select(startswith("-"))] | length > 0 then "N\t" + .name
         else empty end)
      elif .kind == "FunctionDecl" then
        (["F", .name, (if ((.inner // []) | any(.kind == "DLLImportAttr")) then "1" else "0" end),
          (.type.qualType | sub(" ?\\(.*$"; ""))] +
          [.inner[]? | select(.kind == "ParmVarDecl") | (.type.qualType, (.name // "_"))]) | join("\t")
      else empty end' "$t/ast.json" >"$t/recs"
  }
  # printer -v <var>=<value>...: the selected functions of $t/recs, one line each (out=protos: a let of protos.rs;
  # decl: a declaration, with its parameters' names; check: a cfg-gated const of <module>::<name>). Selected: dllimport
  # ones (imp=1), the ones between dxmt_abi_begin_ and dxmt_abi_end_ (block=1), and those named in pick (each of which
  # must be there). guest: names gated by the guest-vm feature; want: how many there must be; lp64: long is 64-bit;
  # wp, cp: the paths of winemetal-wire's mirrors and of c_void and c_char; local: where a struct winemetal.h doesn't
  # define is mirrored (behind a pointer only); opaque=1: a pointer to a winemetal struct without a mirror is c_void.
  structs "$wm/winemetal.h" >"$t/pstructs"
  /usr/bin/grep -oE '^pub struct [A-Za-z_][A-Za-z0-9_]*' "$wire/lib.rs" | awk '{print $3}' >"$t/pmirrors"
  printer() {
    awk -F '\t' "$@" "$names"'
      function local_name(s,   o, i, k, w) {
        if (s !~ /_/) return s
        k = split(s, w, "_"); o = ""
        for (i = 1; i <= k; i++)
          o = o toupper(substr(w[i], 1, 1)) (w[i] ~ /^[A-Z0-9]+$/ ? tolower(substr(w[i], 2)) : substr(w[i], 2))
        return o
      }
      function base(s, ptr,   x) {
        if (s == "intptr_t" || s == "ptrdiff_t") return "isize"
        if (s == "uintptr_t" || s == "size_t") return "usize"
        while (s in td) s = td[s]
        if (s ~ /^enum /) {
          s = substr(s, 6)
          if (!(s in en)) bad = bad " enum " s
          if (s in neg) bad = bad " enum " s " has a negative value"
          s = en[s] == "-" ? "unsigned int" : en[s]
          while (s in td) s = td[s]
        }
        if (s ~ /^struct /) {
          x = substr(s, 8)
          if (x in wms) {
            if (rust(x) in mir) return wp rust(x)
            if (ptr && opaque) return cp "c_void"
            bad = bad " struct " x " (no mirror)"
          } else if (ptr && local != "") return local "::" local_name(x)
          else bad = bad " struct " x
          return x
        }
        if (s in prim) return prim[s]
        bad = bad " " s
        return s
      }
      function rs(s, ret, ptr,   c) {
        sub(/^ +/, "", s); sub(/ +$/, "", s)
        if (s ~ /\*$/) {
          sub(/ *\*$/, "", s)
          c = s ~ /^const /
          sub(/^const /, "", s)
          return (c ? "*const " : "*mut ") (s == "void" ? cp "c_void" : rs(s, 0, 1))
        }
        sub(/^const /, "", s)
        if (s == "void") return ret ? "" : cp "c_void"
        return base(s, ptr)
      }
      BEGIN {
        split("unsigned_long_long:u64 long_long:i64 unsigned_int:u32 int:i32 unsigned_short:u16 short:i16 " \
          "unsigned_char:u8 signed_char:i8 bool:bool float:f32 double:f64", kv, " ")
        for (i in kv) { split(kv[i], p, ":"); gsub(/_/, " ", p[1]); prim[p[1]] = p[2] }
        prim["_Bool"] = "bool"
        prim["char"] = cp "c_char"
        prim["long"] = lp64 ? "i64" : "i32"
        prim["unsigned long"] = lp64 ? "u64" : "u32"
        k = split(pick, a, " "); for (i = 1; i <= k; i++) picked[a[i]] = 1
        k = split(guest, a, " "); for (i = 1; i <= k; i++) guests[a[i]] = 1
      }
      FILENAME == ARGV[ARGC - 3] { wms[$1] = 1; next }
      FILENAME == ARGV[ARGC - 2] { mir[$1] = 1; next }
      $1 == "T" { td[$2] = $3; next }
      $1 == "E" { en[$2] = $3; next }
      $1 == "N" { neg[$2] = 1; next }
      $1 == "F" && $2 == "dxmt_abi_begin_" { inblock = 1; next }
      $1 == "F" && $2 == "dxmt_abi_end_" { inblock = 0; next }
      $1 == "F" && ((imp && $3 == 1) || (block && inblock) || ($2 in picked)) && !seen[$2]++ {
        args = ""
        for (i = 5; i <= NF; i += 2)
          args = args (i > 5 ? ", " : "") (out == "decl" ? ($(i + 1) == "_" ? "_" : field($(i + 1))) ": " : "") rs($i)
        r = rs($4, 1)
        sig = "(" args ")" (r == "" ? "" : " -> " r)
        if (out == "protos") print "    let _: unsafe extern \"C\" fn" sig " = crate::" $2 ";"
        else if (out == "decl") {
          if ($2 in guests) print "    #[cfg(feature = \"guest-vm\")]"
          print "    pub fn " $2 sig ";"
        } else print "#[cfg(" cfg ")]\nconst _: unsafe extern \"" abi "\" fn" sig " = " module "::" $2 ";"
        n++
      }
      END {
        if (bad != "") { print "E cannot map:" bad; exit 1 }
        for (q in picked) if (!(q in seen)) { print "E no function " q; exit 1 }
        if (n == 0) { print "E no function"; exit 1 }
        if (want != "" && n != want) { print "E " n " functions, not " want; exit 1 }
      }' "$t/pstructs" "$t/pmirrors" "$t/recs"
  }
  # scratch <header>: a C file of <stddef.h>, winemetal.h and the header's dxmt-abi block between two marker functions.
  scratch() {
    printf '#include <stddef.h>\n#include "winemetal.h"\nvoid dxmt_abi_begin_(void);\n'
    awk '/\/\* dxmt-abi end \*\// { f = 0 } f { print; n++ } /\/\* dxmt-abi begin \*\// { f = 1 } END { exit n == 0 }' \
      "$1" || fail "$1 has no dxmt-abi block"
    printf 'void dxmt_abi_end_(void);\n'
  }
  printf '#include "winemetal.h"\n' >"$t/api.c"
fi

if [ "$mode" = protos ]; then
  cat <<'EOF'
//! winemetal.h's prototypes, asserted against the exports on each Windows target.
//!
//! Written by `tools/winemetal-layouts.sh --protos` from clang's AST of every `WINEMETAL_API`
//! function: regenerate it, never edit it.

use core::ffi::{c_char, c_void};

use winemetal_wire as wire;
EOF
  printf '%s\n' "$pe" |
  while read -r target cfg; do
    ast "$target" "$t/api.c" $flags
    printf '\n#[cfg(%s)]\nconst _: () = {\n' "$cfg"
    printer -v out=protos -v imp=1 -v want=145 -v wp=wire:: >"$t/out" || fail "$target: $(tail -1 "$t/out")"
    cat "$t/out"
    printf '};\n'
  done
  exit 0
fi

if [ "$mode" = decls ]; then
  # The set: the dllimport functions; the guest's: the ones -DDXMT_GUEST_VM adds.
  ast x86_64-w64-mingw32 "$t/api.c" $flags
  awk -F '\t' '$1 == "F" && $3 == 1 { print $2 }' "$t/recs" | sort -u >"$t/set"
  ast x86_64-w64-mingw32 "$t/api.c" -I"$wm" -I"$root/src/airconv"
  guest=$(awk -F '\t' '$1 == "F" && $3 == 1 { print $2 }' "$t/recs" | sort -u | comm -13 - "$t/set" | tr '\n' ' ')
  [ -n "$guest" ] || fail "-DDXMT_GUEST_VM adds no function"
  printf '%s\naarch64-apple-darwin -\n' "$pe" |
  while read -r target cfg; do
    case $target in
    *-apple-*) ast "$target" "$t/api.c" $flags; set -- -v pick="$(tr '\n' ' ' <"$t/set")" -v lp64=1 ;;
    *) ast "$target" "$t/api.c" $flags; set -- -v imp=1 ;;
    esac
    printer "$@" -v out=decl -v want=145 -v guest="$guest" -v wp=wire:: >"$t/body-$target" ||
      fail "$target: $(tail -1 "$t/body-$target")"
    [ -f "$t/body" ] || cp "$t/body-$target" "$t/body"
    cmp -s "$t/body" "$t/body-$target" || {
      diff "$t/body" "$t/body-$target" >&2
      fail "the declarations differ on $target (< aarch64-w64-mingw32, > $target)"
    }
  done
  scratch "$root/src/nativemetal/win32_native.cpp" >"$t/native.c"
  ast aarch64-apple-darwin "$t/native.c" $flags -DDXMT_NATIVE
  printer -v out=check -v block=1 -v pick="WMTMemoryHooked WMTGetWsiHooks" -v lp64=1 -v abi=C-unwind -v module=crate \
    -v cfg="$mac" -v opaque=1 -v wp=wire:: >"$t/native" || fail "native: $(tail -1 "$t/native")"
  cat <<'EOF'
//! winemetal.h's functions, declared for Rust callers.
//!
//! Written by `tools/winemetal-layouts.sh --decls` from clang's AST: `protos.rs`'s functions,
//! typed the same on every target, then the prototype checks of the native build's functions
//! `lib.rs` declares by hand. Regenerate it, never edit it.

use core::ffi::{c_char, c_void};

use winemetal_wire as wire;

#[cfg(windows)]
unsafe extern "C" {
EOF
  cat "$t/body"
  printf '}\n\n#[cfg(not(windows))]\nunsafe extern "C-unwind" {\n'
  cat "$t/body"
  printf '}\n\n'
  cat "$t/native"
  exit 0
fi

if [ "$mode" = seam ]; then
  [ -f "$header" ] || fail "no $header"
  scratch "$header" >"$t/abi.c"
  cat <<EOF
//! \`$(basename "$header")\`'s \`dxmt-abi\` block, asserted against the Rust exports.
//!
//! Written by \`tools/winemetal-layouts.sh --seam-abi\` from clang's AST on each target the
//! crates build for: regenerate it, never edit it.
EOF
  printf '%s\naarch64-apple-darwin %s\n' "$pe" "$mac" |
  while read -r target cfg; do
    case $target in
    *-apple-*) set -- -v abi=C-unwind -v lp64=1 ;;
    *) set -- -v abi=C ;;
    esac
    ast "$target" "$t/abi.c" $flags
    printer "$@" -v out=check -v block=1 -v cfg="$cfg" -v module="$module" -v local="$module" -v cp=core::ffi:: \
      -v wp=winemetal_wire:: >"$t/out" || fail "$target: $(tail -1 "$t/out")"
    printf '\n'
    cat "$t/out"
  done
  exit 0
fi

if [ "$mode" = com ]; then
  hdir=
  if [ -f "$header" ]; then
    hdir="-I$(dirname "$header")"
    hlabel=$(basename "$header")
    printf '#include "%s"\n' "$hlabel" >"$t/com.c"
  else
    hlabel=$header
    printf '%s\n' "$header" | tr '+,' '\n\n' | awk 'NF { printf "#include <%s>\n", $0 }' >"$t/com.c"
  fi
  com_cc() {
    tgt=$1; shift
    case $tgt in
    *-apple-*)
      xcrun clang -target "$tgt" -DCINTERFACE -DWIDL_EXPLICIT_AGGREGATE_RETURNS ${hdir:+"$hdir"} \
        -I"$root/include/native/d3d12" -I"$root/include/native/windows" -I"$root/include/native/directx" \
        -I"$root/include" "$@"
      ;;
    *)
      "$bin/$tgt-clang" -DCINTERFACE -D_WIN32_WINNT=0xa00 ${hdir:+"$hdir"} \
        -I"$root/include/native/d3d12" -I"$root/include" "$@"
      ;;
    esac
  }
  printf '%s\naarch64-apple-darwin %s\n' "$pe" "$mac" |
  while read -r target cfg; do
    com_cc "$target" -fsyntax-only -Xclang -ast-dump=json "$t/com.c" >"$t/ast.json"
    jq -r '.inner[] |
      if .kind == "TypedefDecl" and .name then ["T", .name, .type.qualType] | join("\t")
      elif .kind == "EnumDecl" and .name then
        (["E", .name, (.fixedUnderlyingType.qualType // "-")] | join("\t")),
        (if [.. | .value? // empty | strings | select(startswith("-"))] | length > 0 then "N\t" + .name
         else empty end)
      elif .kind == "RecordDecl" and .completeDefinition == true and ((.name // "") | endswith("Vtbl")) then
        (["V", (.name | sub("Vtbl$"; "")),
          ([.inner[]? | select(.kind == "FieldDecl") | .name, .type.qualType] | join("\t"))] | join("\t"))
      else empty end' "$t/ast.json" >"$t/recs"
    awk -F '\t' -v pick="$ifaces" -v probe="$t/probe-$target.c" -v meta="$t/meta-$target" "$names"'
      function is_usize(s) {
        return s == "SIZE_T" || s == "ULONG_PTR" || s == "UINT_PTR" || s == "DWORD_PTR" || \
          s == "size_t" || s == "uintptr_t"
      }
      function is_isize(s) {
        return s == "SSIZE_T" || s == "LONG_PTR" || s == "INT_PTR" || s == "ptrdiff_t" || s == "intptr_t"
      }
      function is_wchar(s) {
        return s == "WCHAR" || s == "wchar_t"
      }
      function map_type(s, ret, ptr,   is_c, inner, tag, en) {
        sub(/^ +/, "", s); sub(/ +$/, "", s)
        sub(/ *\[[0-9]+\]$/, " *", s)
        if (!ptr && s ~ /\*const$/) { sub(/ *\*const$/, " *", s) }
        while ((s in td) && !is_usize(s) && !is_isize(s) && !(ptr && is_wchar(s))) {
          s = td[s]
          sub(/^ +/, "", s); sub(/ +$/, "", s)
          if (!ptr && s ~ /\*const$/) { sub(/ *\*const$/, " *", s) }
        }
        if (s ~ /\(\*\) *\(/) {
          if (!ptr && !ret) return "*mut c_void"
        }
        if (s ~ /\*$/) {
          sub(/ *\*$/, "", s)
          is_c = 0
          if (s ~ /\*const$/) { is_c = 1; sub(/ *\*const$/, " *", s) }
          else if (s ~ /^const /) { is_c = 1; sub(/^const +/, "", s) }
          inner = (s == "void") ? "c_void" : map_type(s, 0, 1)
          return (is_c ? "*const " : "*mut ") inner
        }
        sub(/^const +/, "", s)
        if (s == "void") return ret ? "" : "c_void"
        if (ptr && is_wchar(s)) return "c_void"
        if (is_usize(s)) return "usize"
        if (is_isize(s)) return "isize"
        while (s in td) {
          if (is_usize(s)) return "usize"
          if (is_isize(s)) return "isize"
          s = td[s]
          sub(/^ +/, "", s); sub(/ +$/, "", s)
        }
        if (s ~ /^enum /) {
          en = substr(s, 6)
          if (!(en in enums)) bad = bad " enum " en
          if (en in neg) bad = bad " enum " en " has a negative value"
          return "u32"
        }
        if (s ~ /^(struct|union) /) {
          tag = s; sub(/^(struct|union) +/, "", tag)
          if (ptr) return "c_void"
          if (!(tag in byval)) { bad = bad " by-value " s; return tag }
          used_byval[byval[tag]] = 1
          return byval[tag]
        }
        if (s in prim) return prim[s]
        bad = bad " " s
        return s
      }
      BEGIN {
        split("unsigned_long_long:u64 long_long:i64 unsigned_int:u32 int:i32 unsigned_long:u32 long:i32 " \
          "unsigned_short:u16 short:i16 unsigned_char:u8 signed_char:i8 char:c_char bool:bool _Bool:bool " \
          "float:f32 double:f64", kv, " ")
        for (i in kv) { split(kv[i], p, ":"); gsub(/_/, " ", p[1]); prim[p[1]] = p[2] }
        byval["_LUID"] = "Luid"; byval["LUID"] = "Luid"
        byval["D3D12_CPU_DESCRIPTOR_HANDLE"] = "D3d12CpuDescriptorHandle"
        byval["D3D12_GPU_DESCRIPTOR_HANDLE"] = "D3d12GpuDescriptorHandle"
        byval["DXGI_SAMPLE_DESC"] = "DxgiSampleDesc"
        byval["SIZE"] = "Size"; byval["tagSIZE"] = "Size"
        byval_c["Luid"] = "LUID"
        byval_c["D3d12CpuDescriptorHandle"] = "D3D12_CPU_DESCRIPTOR_HANDLE"
        byval_c["D3d12GpuDescriptorHandle"] = "D3D12_GPU_DESCRIPTOR_HANDLE"
        byval_c["DxgiSampleDesc"] = "DXGI_SAMPLE_DESC"
        byval_c["Size"] = "SIZE"
        byval_def["Luid"] = "/// `LUID`, passed or returned by value.\n#[derive(Clone, Copy, Default, PartialEq, Eq)]\n#[repr(C)]\npub struct Luid {\n    pub low_part: u32,\n    pub high_part: i32,\n}\n"
        byval_def["D3d12CpuDescriptorHandle"] = "/// `D3D12_CPU_DESCRIPTOR_HANDLE`, passed by value.\n#[derive(Clone, Copy, Default, PartialEq, Eq)]\n#[repr(C)]\npub struct D3d12CpuDescriptorHandle {\n    pub ptr: usize,\n}\n"
        byval_def["D3d12GpuDescriptorHandle"] = "/// `D3D12_GPU_DESCRIPTOR_HANDLE`, passed by value.\n#[derive(Clone, Copy, Default, PartialEq, Eq)]\n#[repr(C)]\npub struct D3d12GpuDescriptorHandle {\n    pub ptr: u64,\n}\n"
        byval_def["DxgiSampleDesc"] = "/// `DXGI_SAMPLE_DESC`, passed by value.\n#[derive(Clone, Copy, Default, PartialEq, Eq)]\n#[repr(C)]\npub struct DxgiSampleDesc {\n    pub count: u32,\n    pub quality: u32,\n}\n"
        byval_def["Size"] = "/// `SIZE`, passed by value.\n#[derive(Clone, Copy, Default, PartialEq, Eq)]\n#[repr(C)]\npub struct Size {\n    pub cx: i32,\n    pub cy: i32,\n}\n"
        n_pick = split(pick, order, " ")
      }
      $1 == "T" { td[$2] = $3; next }
      $1 == "E" { enums[$2] = $3; next }
      $1 == "N" { neg[$2] = 1; next }
      $1 == "V" {
        iface = $2
        has_v[iface] = 1
        v_nf[iface] = (NF - 2) / 2
        for (i = 3; i <= NF; i += 2) {
          idx = (i - 1) / 2
          v_fn[iface, idx] = $i
          v_ft[iface, idx] = $(i + 1)
        }
      }
      END {
        for (k = 1; k <= n_pick; k++) {
          iface = order[k]
          if (!(iface in has_v)) { print "E no interface " iface "Vtbl"; exit 1 }
          nf = v_nf[iface]
          doc = "/// `" iface "Vtbl`: `" iface "`'\''s " nf " slots."
          if (length(doc) > 100) doc = "/// `" iface "`'\''s " nf " slots."
          body = body doc "\n#[repr(C)]\npub struct " iface "Vtbl {\n"
          p_idx++
          probe_body = probe_body "  com_p_" p_idx " = sizeof(struct " iface "Vtbl),\n"
          printf "I\t%s\tcom_p_%d\n", iface, p_idx > meta
          for (j = 1; j <= nf; j++) {
            c_fn = v_fn[iface, j]
            fn_raw = c_fn
            if (fn_raw == "GetMessageA") fn_raw = "GetMessage"
            fn_rs = field(fn_raw)
            ft = v_ft[iface, j]
            sub(/ *__attribute__\(\(stdcall\)\)$/, "", ft)
            pos = index(ft, "(*)(")
            if (pos == 0) { print "E cannot parse slot " iface "::" fn_raw ": " ft; exit 1 }
            ret_c = substr(ft, 1, pos - 1)
            args_c = substr(ft, pos + 4)
            sub(/\)$/, "", args_c)
            n_args = 0
            if (args_c != "" && args_c != "void") n_args = split(args_c, a_c, ", ")
            arity[n_args] = 1
            rs_args = ""
            c_expr = ""
            for (a = 1; a <= n_args; a++) {
              rs_args = rs_args (a > 1 ? ", " : "") map_type(a_c[a], 0, 0)
              ca = a_c[a]
              sub(/ *\[[0-9]+\]$/, " *", ca)
              c_expr = c_expr (a > 1 ? " + " : "") "((sizeof(" ca ") + sizeof(void *) - 1) & ~(sizeof(void *) - 1))"
            }
            if (c_expr == "") c_expr = "0"
            ret_rs = map_type(ret_c, 1, 0)
            sig = "(" rs_args ")" (ret_rs == "" ? "" : " -> " ret_rs)
            body = body "    pub " fn_rs ": unsafe extern \"system-unwind\" fn" sig ",\n"
            p_idx++
            off_id = "com_p_" p_idx
            probe_body = probe_body "  " off_id " = __builtin_offsetof(struct " iface "Vtbl, " c_fn "),\n"
            p_idx++
            par_id = "com_p_" p_idx
            probe_body = probe_body "  " par_id " = " c_expr ",\n"
            printf "S\t%s\t%s\t%s\t%s\n", iface, fn_rs, off_id, par_id > meta
          }
          body = body "}\n\n"
        }
        if (bad != "") { print "E cannot map:" bad; exit 1 }
        split("Luid D3d12CpuDescriptorHandle D3d12GpuDescriptorHandle DxgiSampleDesc Size", bv_order, " ")
        for (b = 1; b <= 5; b++) {
          bn = bv_order[b]
          if (bn in used_byval) {
            printf "%s\n", byval_def[bn]
            p_idx++
            probe_body = probe_body "  com_p_" p_idx " = sizeof(" byval_c[bn] "),\n"
            printf "M\t%s\tcom_p_%d\n", bn, p_idx > meta
          }
        }
        printf "%s", body
        printf "trait Slot {\n    const PARAMS: usize;\n}\n\n"
        printf "const fn pad(n: usize) -> usize {\n    (n + size_of::<usize>() - 1) & !(size_of::<usize>() - 1)\n}\n\n"
        for (ar = 0; ar <= 20; ar++) {
          if (!(ar in arity)) continue
          tparams = "R"; fparams = ""; sum_expr = ""
          for (a = 0; a < ar; a++) {
            tparams = tparams ", A" a
            fparams = fparams (a > 0 ? ", " : "") "A" a
            sum_expr = sum_expr (a > 0 ? " + " : "") "pad(size_of::<A" a ">())"
          }
          if (sum_expr == "") sum_expr = "0"
          printf "impl<%s> Slot for unsafe extern \"system-unwind\" fn(%s) -> R {\n    const PARAMS: usize = %s;\n}\n\n", \
            tparams, fparams, sum_expr
        }
        printf "const fn slot_params<T, F: Slot>(_: fn(&T) -> &F) -> usize {\n    F::PARAMS\n}\n"
        printf "enum com_probe : unsigned long long {\n%s};\n", probe_body > probe
      }' "$t/recs" >"$t/structs-$target" || fail "$target: $(tail -1 "$t/structs-$target")"
    [ -f "$t/structs" ] || cp "$t/structs-$target" "$t/structs"
    cmp -s "$t/structs" "$t/structs-$target" || {
      diff "$t/structs" "$t/structs-$target" >&2
      fail "the COM vtables differ on $target (< aarch64-w64-mingw32, > $target)"
    }
    cat "$t/com.c" "$t/probe-$target.c" >"$t/full-probe-$target.c"
    com_cc "$target" -fsyntax-only -Xclang -ast-dump -Xclang -ast-dump-filter -Xclang com_probe \
      -fno-color-diagnostics "$t/full-probe-$target.c" |
      awk '/EnumConstantDecl/ { sub(/ '"'"'.*/, ""); n = $NF; next }
        /value: Int / && !(n in v) { sub(/.*value: Int /, ""); v[n] = 1; print n, $1 }' >"$t/vals-$target"
    awk -v cfg="$cfg" '
      NR == FNR { val[$1] = $2; next }
      BEGIN { printf "\n#[cfg(%s)]\nconst _: () = {\n", cfg }
      $1 == "M" { printf "    assert!(size_of::<%s>() == %s);\n", $2, val[$3] }
      $1 == "I" { printf "    assert!(size_of::<%sVtbl>() == %s);\n", $2, val[$3] }
      $1 == "S" {
        printf "    assert!(offset_of!(%sVtbl, %s) == %s && slot_params::<%sVtbl, _>(|v| &v.%s) == %s);\n", \
          $2, $3, val[$4], $2, $3, val[$5]
      }
      END { printf "};\n" }' "$t/vals-$target" "$t/meta-$target" >>"$t/asserts"
  done
  use_ffi="c_void"
  if /usr/bin/grep -q 'c_char' "$t/structs"; then use_ffi="{c_char, c_void}"; fi
  cat <<EOF
//! \`$hlabel\`'s COM vtables (\`$module\`).
//!
//! Written by \`tools/winemetal-layouts.sh --com\` from clang's AST on each target the crates
//! build for: regenerate it, never edit it.

use core::{ffi::$use_ffi, mem::offset_of};

EOF
  cat "$t/structs"
  cat "$t/asserts"
  exit 0
fi

printf '#include "winemetal_thunks.h"\n#include "airconv_thunks.h"\n' >"$t/tu.c"
apple() { xcrun clang -target aarch64-apple-darwin $flags "$@"; }

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
