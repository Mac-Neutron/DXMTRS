#!/bin/sh
# The Rust gate of the tree; run it before every commit. No arguments. From the repository root it runs six legs and
# prints each cargo command line, `rust-check: <leg> ok|FAIL` per leg and, if every leg passed, `rust-check: ok` (exit 0;
# otherwise exit 1, every leg run):
#   fmt     the pinned nightly's `cargo fmt --all --check` (rustfmt.toml uses unstable options)
#   gen     tools/winemetal-layouts.sh regenerates the generated bindings into the target folder, each equal to the
#           committed file: winemetal-wire's layouts.rs, winemetal-pe's protos.rs, winemetal-sys's decls.rs and each
#           seam's src/dxmt/src/<seam>/abi.rs (the header its first line names, with crate::<seam>); a C declaration
#           changed without its regeneration would otherwise still compile (needs Xcode's clang and jq)
#   clippy  1.99.0, `-D warnings`, on aarch64-apple-darwin and on every -pc-windows-msvc target a crate ships on, then
#           the same host and i686 lines with MTLD3D_PERF=1 (cfg(perf_tracking) code), in a target folder of their own,
#           and the core's crates on the host and i686 with their perf-build feature (DXMT_PERF_BUILD)
#   doc     `cargo doc --no-deps` with rustdoc warnings denied, for i686 and the host
#   test    the host unit tests, also with MTLD3D_PERF=1 (not mtld3d-mtl's integration tests: see unit_tests)
#   audit   tools/audit.sh; refuses while an untracked .rs, .toml or .md file exists (the audit reads tracked files)
# The PE legs compile build scripts that need C and Wine's archives, so the inputs are variables (docs/DEVELOPMENT.md,
# "tools/rust-check.sh"); MacNeutron sets them from its pins. cargo runs offline: fetch the locked crates first.
#   MTLD3D_RUST          the rustup toolchain, with clippy (default: meson.options' cargo_toolchain); `rustc +<it>` must
#                        run, so rustup's proxy is on PATH, with lld-link and llvm-lib (MacNeutron: the folder
#                        dxmt/mtld3d-tools.sh prints)
#   MTLD3D_RUST_NIGHTLY  the pinned nightly, as rustup names it, e.g. nightly-2026-10-30 (MacNeutron: dxmt/pins)
#   RUST_NIGHTLY_BIN     optional: that nightly's bin folder (default: rustup's nightly-aarch64-apple-darwin)
#   MTLD3D_LLVM_MINGW    llvm-mingw's root
#   WINE_BUILD           the Mac Wine build tree (libs/winecrt0/aarch64-windows/libwinecrt0.a)
#   GUEST_WINE_SDK       the VM guest's Wine SDK (lib/wine/{x86_64,i386}-windows/libwinecrt0.a)
#   AIRCONV_LLVM         the LLVM 23 install airconv builds its shim against and links (include/, lib/, .complete;
#                        MacNeutron: build/wine-arm64-src/llvm23-arm64-nolto, which make llvm-nolto installs)
#   AIRCONV_XCRUN        xcrun, or a sh script taking xcrun's arguments, for airconv's Metal helpers (MacNeutron:
#                        wine-arm64/tools/xcrun-metal.sh)
#   CARGO_TARGET_DIR     optional: an absolute folder for cargo's output (default: target/ in the tree, ignored by git)
# Linux targets are not linted here (the guest build compiles them); the unit tests run under cargo test, one process
# per test binary, where the Makefile's nextest ran one per test.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

MTLD3D_RUST=${MTLD3D_RUST:-$(sed -n "s/^option('cargo_toolchain'.*value : '\([^']*\)'.*/\1/p" meson.options)}
: "${MTLD3D_RUST:?names the rustup toolchain (default: cargo_toolchain in meson.options; MacNeutron: MTLD3D_RUST in dxmt/pins)}"
: "${MTLD3D_RUST_NIGHTLY:?names the pinned nightly, such as nightly-2026-10-30 (MacNeutron: MTLD3D_RUST_NIGHTLY in dxmt/pins)}"
: "${MTLD3D_LLVM_MINGW:?names the llvm-mingw root (MacNeutron: the parent of the folder sh dxmt/toolchain.sh prints)}"
: "${WINE_BUILD:?names the Mac Wine build tree (MacNeutron: build/wine-arm64-src/wine-build)}"
: "${GUEST_WINE_SDK:?names the VM guest Wine SDK (MacNeutron: build/eac-vm-work/guest-wine-sdk)}"
: "${AIRCONV_LLVM:?names the LLVM 23 install airconv builds against (MacNeutron: build/wine-arm64-src/llvm23-arm64-nolto, from make llvm-nolto)}"
: "${AIRCONV_XCRUN:?names xcrun, or a sh script taking its arguments (MacNeutron: wine-arm64/tools/xcrun-metal.sh)}"
NIGHTLY_BIN=${RUST_NIGHTLY_BIN:-${RUSTUP_HOME:-$HOME/.rustup}/toolchains/nightly-aarch64-apple-darwin/bin}
T=${CARGO_TARGET_DIR:-$root/target}
case $T in
  /*) ;; *) echo "rust-check: CARGO_TARGET_DIR must be absolute, not $T" >&2; exit 2 ;;
esac
export MTLD3D_LLVM_MINGW AIRCONV_LLVM AIRCONV_XCRUN CARGO_TARGET_DIR="$T"
unset MTLD3D_PERF
# The toolchain's own bin folder first on PATH: a rustup proxy runs `cargo +<toolchain>`, but the subcommands (clippy, fmt)
# are found on PATH, where Homebrew's rustup keeps no proxies for them.
if sysroot=$(rustc "+$MTLD3D_RUST" --print sysroot 2>/dev/null); then PATH="$sysroot/bin:$PATH"; fi
case $(rustc --version) in
  "rustc $MTLD3D_RUST "*) ;;
  *) echo "rust-check: rustc on PATH is $(rustc --version), not $MTLD3D_RUST: install the toolchain, with the clippy component, or put its bin folder first" >&2
     exit 2 ;;
esac

PE_P='-p d3d9 -p mtld3d -p mtld3d-core -p mtld3d-types -p mtld3d-shared -p winemetal-pe -p dxgi -p d3d10core -p dxmt-util -p airconv-ir
  -p dxmt-core -p d3d11 -p d3d12 -p winemetal-sys --features d3d11/front-end'
PE64_P="$PE_P -p nvngx -p nvapi"
HOST_P='-p mtld3d-core -p mtld3d-types -p mtld3d-shared -p mtld3d-unix -p mtld3d-mtl -p mtld3d-conformance -p mtld3d-e2e -p winemetal-wire -p winemetal -p dxmt-util -p nvapi -p nvngx -p dxgi -p d3d10core -p airconv-ir -p airconv
  -p dxmt-core -p d3d11 -p d3d12 -p winemetal-sys'
# The core's crates with DXMT_PERF_BUILD's feature, which compiles the statistics and the dump folder out.
CORE_PERF='-p dxmt-core -p d3d11 -p d3d12 --features dxmt-core/perf-build,d3d11/perf-build,d3d11/front-end,d3d12/perf-build'
HOST=aarch64-apple-darwin
I686=i686-pc-windows-msvc

# cargo; the line it runs is printed.
cargo_() {
  printf 'rust-check: + %scargo %s\n' "${MTLD3D_PERF:+MTLD3D_PERF=1 }" "$*"
  cargo "$@"
}

# The libwinecrt0.a the mtld3d shim bundles into a PE target's static library.
winecrt0() {
  case $1 in
    aarch64-*|arm64ec-*) # one ARM64X archive carries both halves
      for d in libs dlls; do
        [ -f "$WINE_BUILD/$d/winecrt0/aarch64-windows/libwinecrt0.a" ] && { echo "$WINE_BUILD/$d/winecrt0/aarch64-windows/libwinecrt0.a"; return 0; }
      done
      echo "rust-check: no libwinecrt0.a under WINE_BUILD=$WINE_BUILD (libs/ or dlls/winecrt0/aarch64-windows)" >&2
      return 1 ;;
    x86_64-*) f=$GUEST_WINE_SDK/lib/wine/x86_64-windows/libwinecrt0.a ;;
    i686-*) f=$GUEST_WINE_SDK/lib/wine/i386-windows/libwinecrt0.a ;;
  esac
  [ -f "$f" ] || { echo "rust-check: no $f (GUEST_WINE_SDK=$GUEST_WINE_SDK)" >&2; return 1; }
  echo "$f"
}

# pe <target> <command...>: run <command> with <target>'s C environment (tools/mingw-cc.sh env) and its Wine archive.
pe() {
  (
    t=$1; shift
    e=$(sh tools/mingw-cc.sh env "$t" "$T/cc") || exit 1
    eval "$e"
    MTLD3D_WINECRT0=$(winecrt0 "$t") || exit 1
    export MTLD3D_WINECRT0
    "$@"
  )
}

# perf <command...>: run <command> as the MTLD3D_PERF=1 build, in a cargo folder of its own (a crate whose build script
# reads the variable rebuilds, with everything above it, whenever the value changes).
perf() { ( MTLD3D_PERF=1 CARGO_TARGET_DIR=$T/perf; export MTLD3D_PERF CARGO_TARGET_DIR; "$@" ); }

# lint <target> <cargo clippy args...>
lint() {
  t=$1; shift
  case $t in
    *-windows-msvc) pe "$t" cargo_ clippy --offline --keep-going --target "$t" "$@" -- -D warnings ;;
    *) cargo_ clippy --offline --keep-going --target "$t" "$@" -- -D warnings ;;
  esac
}

# The tests run on the host; winemetal's reference symbols only the link of winemetal.so provides.
tst() { cargo_ test --offline --no-fail-fast --target "$HOST" "$@"; }

leg_fmt() {
  want=${MTLD3D_RUST_NIGHTLY#nightly-}
  have=$(sed -n 's/^date = "\(.*\)"/\1/p' "$NIGHTLY_BIN/../lib/rustlib/multirust-channel-manifest.toml" 2>/dev/null | head -1)
  [ "$have" = "$want" ] || {
    echo "rust-check: the nightly in $NIGHTLY_BIN is dated ${have:-unknown}; MTLD3D_RUST_NIGHTLY=$MTLD3D_RUST_NIGHTLY names $want" >&2
    return 1
  }
  printf 'rust-check: + cargo fmt --all --check (%s)\n' "$MTLD3D_RUST_NIGHTLY"
  PATH="$NIGHTLY_BIN:$PATH" "$NIGHTLY_BIN/cargo" fmt --all --check
}

# gen_cmp <committed file> <generator arguments...>: one generated file, regenerated and compared.
gen_cmp() {
  f=$1; shift
  printf 'rust-check: + sh tools/winemetal-layouts.sh %s\n' "$*"
  sh tools/winemetal-layouts.sh "$@" "$MTLD3D_LLVM_MINGW/bin" > "$T/gen/out.rs" || return 1
  cmp -s "$T/gen/out.rs" "$f" && return 0
  echo "rust-check: $f is not what tools/winemetal-layouts.sh $* prints; regenerate it:" >&2
  diff "$f" "$T/gen/out.rs" | head -10 >&2
  return 1
}

leg_gen() {
  rc=0
  mkdir -p "$T/gen"
  gen_cmp src/winemetal/wire/src/layouts.rs || rc=1
  gen_cmp src/winemetal/src/protos.rs --protos || rc=1
  gen_cmp src/winemetal/sys/src/decls.rs --decls || rc=1
  gen_cmp src/util/src/dxgi_vtbl.rs --com dxgi1_6.h crate::dxgi_vtbl \
    IUnknown IDXGIObject IDXGIDeviceSubObject \
    IDXGISwapChain IDXGISwapChain1 IDXGISwapChain2 IDXGISwapChain3 IDXGISwapChain4 \
    IDXGIFactory IDXGIFactory1 IDXGIFactory2 \
    IDXGIOutput IDXGIOutput1 \
    IDXGIAdapter IDXGIAdapter1 IDXGIAdapter2 IDXGIAdapter3 IDXGIAdapter4 || rc=1
  gen_cmp src/d3d12/src/abi/vtbl.rs --com d3d12.h crate::abi::vtbl \
    IUnknown ID3D10Blob ID3D12Object ID3D12DeviceChild ID3D12Pageable \
    ID3D12RootSignature ID3D12RootSignatureDeserializer ID3D12VersionedRootSignatureDeserializer \
    ID3D12Heap ID3D12Heap1 ID3D12Resource ID3D12Resource1 ID3D12Resource2 \
    ID3D12CommandAllocator ID3D12Fence ID3D12Fence1 ID3D12PipelineState \
    ID3D12DescriptorHeap ID3D12QueryHeap ID3D12CommandSignature \
    ID3D12CommandList ID3D12GraphicsCommandList ID3D12GraphicsCommandList1 ID3D12GraphicsCommandList2 \
    ID3D12GraphicsCommandList3 ID3D12GraphicsCommandList4 ID3D12GraphicsCommandList5 \
    ID3D12GraphicsCommandList6 ID3D12GraphicsCommandList7 \
    ID3D12CommandQueue ID3D12PipelineLibrary ID3D12PipelineLibrary1 \
    ID3D12Device ID3D12Device1 ID3D12Device2 ID3D12Device3 ID3D12Device4 \
    ID3D12Device5 ID3D12Device6 ID3D12Device7 ID3D12Device8 ID3D12Device9 ID3D12Device10 || rc=1
  gen_cmp src/d3d11/src/abi/vtbl.rs --com d3d11_4.h+d3d10_1.h crate::abi::vtbl \
    IUnknown IDXGIObject IDXGIDeviceSubObject \
    IDXGIDevice IDXGIDevice1 IDXGIDevice2 IDXGIDevice3 \
    IDXGIResource IDXGIResource1 IDXGISurface IDXGISurface1 IDXGISurface2 \
    IDXGIKeyedMutex IDXGISwapChain IDXGISwapChain1 IDXGISwapChain2 IDXGISwapChain3 IDXGISwapChain4 \
    ID3D11DeviceChild ID3D11Resource ID3D11Buffer \
    ID3D11Texture1D ID3D11Texture2D ID3D11Texture2D1 ID3D11Texture3D ID3D11Texture3D1 \
    ID3D11View ID3D11ShaderResourceView ID3D11ShaderResourceView1 \
    ID3D11RenderTargetView ID3D11RenderTargetView1 ID3D11DepthStencilView \
    ID3D11UnorderedAccessView ID3D11UnorderedAccessView1 \
    ID3D11BlendState ID3D11BlendState1 ID3D11DepthStencilState \
    ID3D11RasterizerState ID3D11RasterizerState1 ID3D11RasterizerState2 ID3D11SamplerState \
    ID3D11InputLayout ID3D11VertexShader ID3D11HullShader ID3D11DomainShader \
    ID3D11GeometryShader ID3D11PixelShader ID3D11ComputeShader \
    ID3D11Asynchronous ID3D11Query ID3D11Query1 ID3D11Predicate ID3D11Counter \
    ID3D11ClassInstance ID3D11ClassLinkage ID3D11CommandList ID3DDeviceContextState \
    ID3D11DeviceContext ID3D11DeviceContext1 ID3D11DeviceContext2 ID3D11DeviceContext3 ID3D11DeviceContext4 \
    ID3D11Fence ID3D11Device ID3D11Device1 ID3D11Device2 ID3D11Device3 ID3D11Device4 ID3D11Device5 \
    ID3D11Multithread ID3DUserDefinedAnnotation \
    ID3D10DeviceChild ID3D10Resource ID3D10Buffer ID3D10Texture1D ID3D10Texture2D ID3D10Texture3D \
    ID3D10View ID3D10ShaderResourceView ID3D10ShaderResourceView1 ID3D10RenderTargetView ID3D10DepthStencilView \
    ID3D10BlendState ID3D10BlendState1 ID3D10DepthStencilState ID3D10RasterizerState ID3D10SamplerState \
    ID3D10InputLayout ID3D10VertexShader ID3D10GeometryShader ID3D10PixelShader \
    ID3D10Asynchronous ID3D10Query ID3D10Predicate ID3D10Counter \
    ID3D10Device ID3D10Device1 ID3D10Multithread || rc=1
  gen_cmp src/d3d11/src/abi/records.rs --com d3d11_4.h+d3d10_1.h crate::abi::records --records \
    LUID RECT POINT SECURITY_ATTRIBUTES \
    DXGI_RATIONAL DXGI_SAMPLE_DESC DXGI_RGB DXGI_RGBA \
    DXGI_GAMMA_CONTROL DXGI_GAMMA_CONTROL_CAPABILITIES \
    DXGI_MODE_DESC DXGI_MODE_DESC1 DXGI_SURFACE_DESC DXGI_MAPPED_RECT \
    DXGI_SHARED_RESOURCE DXGI_FRAME_STATISTICS \
    DXGI_SWAP_CHAIN_DESC DXGI_SWAP_CHAIN_DESC1 DXGI_SWAP_CHAIN_FULLSCREEN_DESC \
    DXGI_PRESENT_PARAMETERS DXGI_MATRIX_3X2_F DXGI_HDR_METADATA_HDR10 \
    D3D11_BOX D3D11_VIEWPORT D3D11_MAPPED_SUBRESOURCE D3D11_SUBRESOURCE_DATA \
    D3D11_BUFFER_DESC D3D11_TEXTURE1D_DESC D3D11_TEXTURE2D_DESC D3D11_TEXTURE2D_DESC1 \
    D3D11_TEXTURE3D_DESC D3D11_TEXTURE3D_DESC1 \
    D3D11_BUFFER_SRV D3D11_BUFFEREX_SRV \
    D3D11_TEX1D_SRV D3D11_TEX1D_ARRAY_SRV \
    D3D11_TEX2D_SRV D3D11_TEX2D_ARRAY_SRV D3D11_TEX2D_SRV1 D3D11_TEX2D_ARRAY_SRV1 \
    D3D11_TEX3D_SRV D3D11_TEXCUBE_SRV D3D11_TEXCUBE_ARRAY_SRV \
    D3D11_TEX2DMS_SRV D3D11_TEX2DMS_ARRAY_SRV \
    D3D11_SHADER_RESOURCE_VIEW_DESC D3D11_SHADER_RESOURCE_VIEW_DESC1 \
    D3D11_BUFFER_RTV D3D11_TEX1D_RTV D3D11_TEX1D_ARRAY_RTV \
    D3D11_TEX2D_RTV D3D11_TEX2D_ARRAY_RTV D3D11_TEX2D_RTV1 D3D11_TEX2D_ARRAY_RTV1 \
    D3D11_TEX2DMS_RTV D3D11_TEX2DMS_ARRAY_RTV D3D11_TEX3D_RTV \
    D3D11_RENDER_TARGET_VIEW_DESC D3D11_RENDER_TARGET_VIEW_DESC1 \
    D3D11_TEX1D_DSV D3D11_TEX1D_ARRAY_DSV \
    D3D11_TEX2D_DSV D3D11_TEX2D_ARRAY_DSV \
    D3D11_TEX2DMS_DSV D3D11_TEX2DMS_ARRAY_DSV \
    D3D11_DEPTH_STENCIL_VIEW_DESC \
    D3D11_BUFFER_UAV D3D11_TEX1D_UAV D3D11_TEX1D_ARRAY_UAV \
    D3D11_TEX2D_UAV D3D11_TEX2D_ARRAY_UAV D3D11_TEX2D_UAV1 D3D11_TEX2D_ARRAY_UAV1 \
    D3D11_TEX3D_UAV D3D11_UNORDERED_ACCESS_VIEW_DESC D3D11_UNORDERED_ACCESS_VIEW_DESC1 \
    D3D11_RENDER_TARGET_BLEND_DESC D3D11_BLEND_DESC \
    D3D11_RENDER_TARGET_BLEND_DESC1 D3D11_BLEND_DESC1 \
    D3D11_DEPTH_STENCILOP_DESC D3D11_DEPTH_STENCIL_DESC \
    D3D11_RASTERIZER_DESC D3D11_RASTERIZER_DESC1 D3D11_RASTERIZER_DESC2 \
    D3D11_SAMPLER_DESC \
    D3D11_INPUT_ELEMENT_DESC D3D11_SO_DECLARATION_ENTRY D3D11_CLASS_INSTANCE_DESC \
    D3D11_QUERY_DESC D3D11_QUERY_DESC1 \
    D3D11_QUERY_DATA_TIMESTAMP_DISJOINT D3D11_QUERY_DATA_PIPELINE_STATISTICS D3D11_QUERY_DATA_SO_STATISTICS \
    D3D11_COUNTER_DESC D3D11_COUNTER_INFO \
    D3D11_TILED_RESOURCE_COORDINATE D3D11_TILE_REGION_SIZE D3D11_TILE_SHAPE \
    D3D11_SUBRESOURCE_TILING D3D11_PACKED_MIP_DESC \
    D3D11_DRAW_INSTANCED_INDIRECT_ARGS D3D11_DRAW_INDEXED_INSTANCED_INDIRECT_ARGS \
    D3D11_FEATURE_DATA_THREADING D3D11_FEATURE_DATA_DOUBLES \
    D3D11_FEATURE_DATA_FORMAT_SUPPORT D3D11_FEATURE_DATA_FORMAT_SUPPORT2 \
    D3D11_FEATURE_DATA_D3D10_X_HARDWARE_OPTIONS \
    D3D11_FEATURE_DATA_D3D11_OPTIONS D3D11_FEATURE_DATA_ARCHITECTURE_INFO \
    D3D11_FEATURE_DATA_D3D9_OPTIONS D3D11_FEATURE_DATA_D3D9_SHADOW_SUPPORT \
    D3D11_FEATURE_DATA_SHADER_MIN_PRECISION_SUPPORT D3D11_FEATURE_DATA_D3D11_OPTIONS1 \
    D3D11_FEATURE_DATA_D3D9_SIMPLE_INSTANCING_SUPPORT D3D11_FEATURE_DATA_MARKER_SUPPORT \
    D3D11_FEATURE_DATA_D3D9_OPTIONS1 D3D11_FEATURE_DATA_D3D11_OPTIONS2 \
    D3D11_FEATURE_DATA_D3D11_OPTIONS3 D3D11_FEATURE_DATA_D3D11_OPTIONS4 \
    D3D11_FEATURE_DATA_GPU_VIRTUAL_ADDRESS_SUPPORT D3D11_FEATURE_DATA_D3D11_OPTIONS5 \
    D3D11_FEATURE_DATA_SHADER_CACHE \
    D3D10_BOX D3D10_VIEWPORT D3D10_MAPPED_TEXTURE2D D3D10_MAPPED_TEXTURE3D D3D10_SUBRESOURCE_DATA \
    D3D10_BUFFER_DESC D3D10_TEXTURE1D_DESC D3D10_TEXTURE2D_DESC D3D10_TEXTURE3D_DESC \
    D3D10_BUFFER_SRV D3D10_TEX1D_SRV D3D10_TEX1D_ARRAY_SRV \
    D3D10_TEX2D_SRV D3D10_TEX2D_ARRAY_SRV D3D10_TEX3D_SRV \
    D3D10_TEXCUBE_SRV D3D10_TEXCUBE_ARRAY_SRV1 \
    D3D10_TEX2DMS_SRV D3D10_TEX2DMS_ARRAY_SRV \
    D3D10_SHADER_RESOURCE_VIEW_DESC D3D10_SHADER_RESOURCE_VIEW_DESC1 \
    D3D10_BUFFER_RTV D3D10_TEX1D_RTV D3D10_TEX1D_ARRAY_RTV \
    D3D10_TEX2D_RTV D3D10_TEX2D_ARRAY_RTV \
    D3D10_TEX2DMS_RTV D3D10_TEX2DMS_ARRAY_RTV D3D10_TEX3D_RTV \
    D3D10_RENDER_TARGET_VIEW_DESC \
    D3D10_TEX1D_DSV D3D10_TEX1D_ARRAY_DSV \
    D3D10_TEX2D_DSV D3D10_TEX2D_ARRAY_DSV \
    D3D10_TEX2DMS_DSV D3D10_TEX2DMS_ARRAY_DSV \
    D3D10_DEPTH_STENCIL_VIEW_DESC \
    D3D10_BLEND_DESC D3D10_RENDER_TARGET_BLEND_DESC1 D3D10_BLEND_DESC1 \
    D3D10_DEPTH_STENCILOP_DESC D3D10_DEPTH_STENCIL_DESC \
    D3D10_RASTERIZER_DESC D3D10_SAMPLER_DESC \
    D3D10_INPUT_ELEMENT_DESC D3D10_SO_DECLARATION_ENTRY \
    D3D10_QUERY_DESC D3D10_QUERY_DATA_TIMESTAMP_DISJOINT \
    D3D10_QUERY_DATA_PIPELINE_STATISTICS D3D10_QUERY_DATA_SO_STATISTICS \
    D3D10_COUNTER_DESC D3D10_COUNTER_INFO || rc=1
  gen_cmp src/winemetal/sys/src/sm50/abi.rs --seam-abi src/winemetal/sys/sm50-abi.h winemetal_sys::sm50 || rc=1
  for f in src/dxmt/src/*/abi.rs; do
    h=$(sed -n '1s/^\/\/! `\([^`]*\)`.s `dxmt-abi` block.*/\1/p' "$f")
    hdr=$(git ls-files "src/*/$h")
    case $hdr in
      '' | *' '* | *'
'*) echo "rust-check: $f names no one header of the tree in its first line (${h:-none})" >&2; rc=1; continue ;;
    esac
    seam=${f#src/dxmt/src/}
    gen_cmp "$f" --seam-abi "$hdr" "crate::${seam%/abi.rs}" || rc=1
  done
  return $rc
}

leg_clippy() {
  rc=0
  lint $I686 $PE_P || rc=1
  lint $I686 -p mtld3d-tests --all-targets || rc=1
  for t in x86_64-pc-windows-msvc aarch64-pc-windows-msvc; do
    lint $t $PE64_P || rc=1
    lint $t -p mtld3d-tests --all-targets || rc=1
  done
  lint arm64ec-pc-windows-msvc $PE64_P || rc=1
  lint $HOST --all-targets $HOST_P --features mtld3d-core/disk-cache,airconv/oracle,d3d11/front-end || rc=1
  lint $HOST -p mtld3d-unix --features mtld3d-unix/winemetal --all-targets || rc=1
  lint $HOST -p winemetal --features native || rc=1
  perf lint $HOST --all-targets -p mtld3d-core -p mtld3d-unix --features mtld3d-core/disk-cache || rc=1
  perf lint $I686 $PE_P || rc=1
  lint $HOST --all-targets $CORE_PERF || rc=1
  lint $I686 $CORE_PERF || rc=1
  return $rc
}

leg_doc() {
  rc=0
  RUSTDOCFLAGS='-D warnings'; export RUSTDOCFLAGS
  pe $I686 cargo_ doc --no-deps --offline --keep-going --target $I686 $PE_P --features mtld3d-core/disk-cache || rc=1
  cargo_ doc --no-deps --offline --keep-going --target $HOST $HOST_P --features mtld3d-core/disk-cache,d3d11/front-end || rc=1
  unset RUSTDOCFLAGS
  return $rc
}

# The facade's integration tests (mtld3d-mtl's tests/) need a winemetal.so beside stand-ins for the Wine libraries it
# links, which MacNeutron's dxmt/mtld3d-facade-test.sh builds and runs; here only its unit tests run.
unit_tests() {
  r=0
  tst -p mtld3d-core -p mtld3d-types --features mtld3d-core/disk-cache || r=1
  tst -p mtld3d-unix -p mtld3d-shared -p mtld3d-conformance -p mtld3d-e2e -p winemetal-wire || r=1
  tst -p mtld3d-mtl --lib || r=1
  return $r
}

leg_test() {
  rc=0
  unit_tests || rc=1
  perf unit_tests || rc=1
  tst -p dxmt-util -p nvapi -p nvngx -p winemetal -p dxgi -p d3d10core -p airconv-ir -p airconv \
    -p dxmt-core -p d3d11 -p d3d12 -p winemetal-sys --features airconv/oracle,d3d11/front-end \
    --config 'target.aarch64-apple-darwin.rustflags=["-C","link-arg=-Wl,-undefined,dynamic_lookup"]' || rc=1
  return $rc
}

leg_audit() {
  untracked=$(git ls-files --others --exclude-standard | grep -E '\.(rs|toml|md)$' || true)
  if [ -n "$untracked" ]; then
    printf 'rust-check: the audit reads tracked files only; git add these first:\n%s\n' "$untracked" >&2
    return 1
  fi
  printf 'rust-check: + sh tools/audit.sh\n'
  sh tools/audit.sh
}

failed=
for leg in fmt gen clippy doc test audit; do
  if "leg_$leg"; then
    echo "rust-check: $leg ok"
  else
    echo "rust-check: $leg FAIL"
    failed="$failed $leg"
  fi
done
[ -z "$failed" ] || { echo "rust-check: FAIL, legs:$failed" >&2; exit 1; }
echo "rust-check: ok"
