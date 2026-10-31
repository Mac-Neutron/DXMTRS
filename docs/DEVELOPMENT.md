# Build DXMT

[Our CI](/.github/workflows/ci.yml) is the best reference.

## Prerequisites:
- MacOS Sonoma and later. <sub>It doesn't make a lot of sense to build a project targeting Apple platforms from Windows, Linux or other OSs.</sub>
- Meson 1.3+, it's the build system used by this project
- LLVM 15 (exact major version), with headers and static libs
    - CMake 3.27+, when building LLVM from the source
- Xcode 16+, with Metal toolchain
- Wine 8+, with headers and tools, _if you want to use DXMT with Wine_.
    - A cross-compiler (mingw-w64 or llvm-mingw) for Windows is also required
    - When building DXMT with Wine, it is a **Cross Build**.

The following documentation is NOT for beginners. You are assumed to be familiar with C/C++ compilers and build systems.

## Setup LLVM

First of all, you need to build LLVM (or use a pre-built LLVM package) no matter if you want to use DXMT with Wine or as a native library. Then you need to decide which architecture should you target for LLVM: `x86_64` or `arm64`.

- For a **Cross Build**, you most likely want `x86_64` at the moment.
- Otherwise you probably should use `arm64`, since Intel Macs and Rosetta 2 are being deprecated.

This is an example of building `x86_64` LLVM
```sh
# in root directory of the project
mkdir -p ./toolchains/llvm-build
echo '**' >> toolchains/.gitignore
git clone --depth 1 --branch llvmorg-15.0.7 https://github.com/llvm/llvm-project.git toolchains/llvm-project

cmake -B ./toolchains/llvm-build -S ./toolchains/llvm-project/llvm \
  -DCMAKE_INSTALL_PREFIX="$(pwd)/toolchains/llvm" \
  -DCMAKE_OSX_ARCHITECTURES=x86_64 \
  -DLLVM_HOST_TRIPLE=x86_64-apple-darwin \
  -DLLVM_ENABLE_ASSERTIONS=On \
  -DLLVM_ENABLE_ZSTD=Off \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLVM_TARGETS_TO_BUILD="" \
  -DLLVM_BUILD_TOOLS=Off \
  -DBUG_REPORT_URL="https://github.com/3Shain/dxmt" \
  -DPACKAGE_VENDOR="DXMT" \
  -DLLVM_VERSION_PRINTER_SHOW_HOST_TARGET_INFO=Off \
  -G Ninja
cmake --build ./toolchains/llvm-build
cmake --install ./toolchains/llvm-build
```

Then `./toolchains/llvm` contains what we need later.

For `arm64` target, just replace any occurance of `x86_64` to `arm64` above.

## Setup Wine

> Skip this part if you are NOT doing a **Cross Build**

See [winehq.org - MacOS Building](https://gitlab.winehq.org/wine/wine/-/wikis/MacOS-Building)

You should also check the target architecture of Wine build, `--enable-archs=i386,x86_64` is a popular option when you also want x86 (32-bit programs) support in addition to x86_64.

If you build Wine from source, you don't have to install it, just pass the build directory later.

## Build DXMT

### Cross Build

Build (64-bit) Windows PE+ dlls **and** 64-bit unixlib (a Mach-O library with `.so` extension by convention).

```sh
meson setup --cross-file build-win64.txt -Dnative_llvm_path=</path/to/llvm> -Dwine_build_path=</path/to/wine/build> build --buildtype release
meson compile -C build
```

(Optional) Build (32-bit) Windows PE dlls

```sh
meson setup --cross-file build-win32.txt -Dwine_build_path=</path/to/wine/build> build32 --buildtype release
meson compile -C build32
```

> You can't build 32-bit dlls only, they need 64-bit unixlib to be functional

There are other compile options in [meson.options](/meson.options).

#### `clangd` configuration for proper language server support 

Since this project contains code runs on macOS/Windows(Wine)/both, `clangd` may not always be able to find the correct sysroot for e.g. libc++ include files. Add clangd argument `--query-driver=**/x86_64-w64-mingw32-**` may solve this problem. 

### Native Build

Build all components as Mach-O .dylib

```sh
meson setup -Dnative_llvm_path=</path/to/llvm> --buildtype release
meson compile -C build
```

#### Side notes on building x86_64 target from arm64 device/environment

Apparently the simplist solution is to use a x86_64 shell, but you can also set following environment variables

```sh
export CFLAGS='-arch x86_64'
export CXXFLAGS='-arch x86_64'
# then perform `meson setup ...`
```


## Rust

The Rust crates (Direct3D 9 under `src/d3d9`, `src/winemetal`, `src/nvapi`, `src/nvngx`, `src/util` and the test
crates under `tests/d3d9`) are one Cargo workspace at the root: `Cargo.toml`, `Cargo.lock`, `.cargo/config.toml` (the
code-generation flags per target) and `rustfmt.toml`. A bare `cargo` command at the root builds for the host, which is
what the unit tests and clippy's host leg use. The binaries that ship are built by meson, which runs
`tools/cargo-staticlib.sh` once per crate and target and then links; do not build a shipped binary by hand.

The toolchain is the `cargo_toolchain` option of `meson.options` (MacNeutron's `MTLD3D_RUST`). Meson runs cargo
offline, so the locked crates are fetched first, from the root, with `cargo "+<toolchain>" fetch --locked`. Cargo
output stays out of the source tree: every cargo run outside meson sets an absolute `CARGO_TARGET_DIR`, and the root
`.gitignore` covers `/target` for a developer's own runs.

### Which target builds what

- `aarch64-apple-darwin`: the Unix side that runs in the Mac's Wine, `winemetal.so` and D3D9's `mtld3d.so`.
- `aarch64-pc-windows-msvc` and `arm64ec-pc-windows-msvc`: the two halves of the ARM64X images `d3d9.dll` and
  `mtld3d.dll`. Rust has no ARM64X target, so each crate is built once per half as a static library and
  `tools/arm64x-link.sh` links both into one image. It needs LLD 23 or newer (llvm-mingw's own `lld-link`, never the one
  on `PATH`) and an llvm-mingw whose `aarch64-w64-mingw32` sysroot carries the ARM64 and ARM64EC halves together.
  `nvngx.dll`, `nvapi64.dll` and `winemetal.dll` are built for both targets too.
- `x86_64-pc-windows-msvc` and `i686-pc-windows-msvc`: the PE side of the VM guest, linked by meson's own driver
  (`i686` is D3D9 and `winemetal.dll` only).
- `x86_64-unknown-linux-gnu` and `i686-unknown-linux-gnu`: the guest's `mtld3d.so`, built through
  `tools/linux-cargo.sh` against a Fedora sysroot (`MTLD3D_SYSROOT`; `MTLD3D_LINUX` picks the architecture).

The per-target C and C++ environment that cc-rs reads (compiler, flags, file-prefix remaps) comes from
`tools/mingw-cc.sh env`, which `tools/cargo-staticlib.sh` evaluates when `MTLD3D_LLVM_MINGW` is set. Only
`src/d3d9`'s meson targets set it. They also pass `MTLD3D_WINECRT0`, the path of Wine's `libwinecrt0.a`, which the
`mtld3d.dll` shim bundles. Both are build-time variables, not runtime ones.

### Instruction-set baselines

The x86 PE DLLs are guest code, which an x86 translator runs: Rosetta under an x86_64 Wine, and the translator an
arm64 Wine carries (FEX). The ARM64X images are native code and have no such baseline. `.cargo/config.toml` sets the
x86 builds' CPU baseline for the Rust, and `tools/mingw-cc.sh env` sets it for the C and C++ that build scripts compile.
Neither goes past SSE4.2 or uses BMI; the comments in `.cargo/config.toml` say why. DXMT's own C++ has no baseline of
its own.

The i686 build targets `pentium4`, plain SSE2. The 32-bit build that matters today is the one World of Warcraft 1.12
and 3.3.5a load under Rosetta, and there `pentium4` measured faster than `nehalem`: 9 to 15 % less time per call for the
state setters, the shader-constant uploads and a vertex-buffer lock, and 2.4 % off the buffer benchmark's frame, while
the World of Warcraft 1.12 frame moved by +0.8 %, inside that run's noise, and the fixed-function draws by +0.5 %. Two
per-draw gauges of the perf summary rose: the draw snapshot (`draw_snapshot_ms`) by 9.5 % and its key building
(`draw_snapshot_keys_ms`) by 7.3 %. Under the arm64 Wine's translator the same change cost about 7 % on a clean draw
and 2 to 3 % on a frame, so the baseline is worth measuring again once that translator is in production use.

The x86_64 build targets `x86-64-v2`. Lowering it to plain `x86-64` measured neutral under both translators, so the
SSE4.2 ceiling neither costs nor buys anything measurable.

### `tools/rust-check.sh`

The Rust gate. It runs, from the root, the nightly `cargo fmt --check`, clippy with `nursery` and `pedantic` and
warnings denied (1.99.0, on `aarch64-apple-darwin` and on every `-pc-windows-msvc` target a crate ships on, plus the
`MTLD3D_PERF=1` build in a target directory of its own, `clippy-perf`), `cargo doc` with rustdoc warnings denied,
the host unit tests with `cargo test` (the `test-unit-perf` leg on the perf build, the `mtld3d-core/disk-cache`
feature, and `winemetal`'s tests with `-undefined dynamic_lookup`), and `tools/audit.sh`. It prints `rust-check: ok`
and exits 0 when every leg passed. Run it from a clean checkout: its audit
leg refuses while `git ls-files --others --exclude-standard` lists a `.rs`, `.toml` or `.md` file, since the audit
reads tracked files only.

Its inputs are variables, so it runs under whatever environment holds the toolchains. MacNeutron sets them from its
pins:

| Variable | What it names |
| --- | --- |
| `MTLD3D_RUST` | The rustup toolchain for clippy, doc and the tests. Default: `cargo_toolchain` in `meson.options`. |
| `MTLD3D_RUST_NIGHTLY` | The pinned nightly rustfmt needs (`rustfmt.toml` uses unstable options), as rustup names it, such as `nightly-2026-10-30`. |
| `RUST_NIGHTLY_BIN` | Optional. The `bin` folder of that nightly; default `~/.rustup/toolchains/nightly-aarch64-apple-darwin/bin`. The script refuses unless the installed nightly's channel date is the pin's. |
| `MTLD3D_LLVM_MINGW` | The llvm-mingw root. Its `bin` folder is also on `PATH`. |
| `WINE_BUILD` | The Mac Wine build tree, for `libntdll.a` and `libwinecrt0.a`. |
| `GUEST_WINE_SDK` | The VM guest's Wine SDK. |
| `AIRCONV_LLVM` | The LLVM 15 install airconv's build script compiles its C++ shim against and links (`include`, `lib`, `.complete`). |
| `AIRCONV_XCRUN` | `xcrun`, or a `sh` script taking `xcrun`'s arguments (run through `/bin/sh` when it is not executable), for airconv's Metal helpers. |
| `CARGO_TARGET_DIR` | Optional. An absolute folder outside the tree. |

The script does no sysroot work and has no Linux leg: no leg compiles a Linux target. The guest build does, and
Linux-only code is not linted (pre-existing). Generated files (`src/winemetal/wire/src/layouts.rs`,
`src/winemetal/src/protos.rs` and `src/airconv/src/abi/layouts.rs`, from `tools/winemetal-layouts.sh`) are skipped
by rustfmt and regenerated by their generator.

# Debugging
The following environment variables can be used for **debugging** purposes.
- `MTL_SHADER_VALIDATION=1` Enable Metal shader validation layer
- `MTL_DEBUG_LAYER=1` Enable Metal API validation layer
- `MTL_CAPTURE_ENABLED=1` Enable Metal frame capture
- `DXMT_CAPTURE_EXECUTABLE="the executable name without extension"` Must be set to enable Metal frame capture. Press F10 to generate a capture. The captured result will be stored in the same directory as the executable.
- `DXMT_CAPTURE_FRAME=n` Automatically capture n-th frame. Useful for debugging a replay.
- `DXMT_LOG_LEVEL=none|error|warn|info|debug` Controls message logging.
- `DXMT_LOG_PATH=/some/directory` Changes path where log files are stored. Set to `none` to disable log file creation entirely, without disabling logging.
- `DXMT_SHADER_CACHE=0`: Disables the internal shader cache.
- `DXMT_SHADER_CACHE_PATH=/some/absolute/darwin/directory`: Path to internal shader cache files. Default to `$(getconf DARWIN_USER_CACHE_DIR)/dxmt/<executable name with extension>`.


### Logs
When used with Wine, DXMT will print log messages to `stderr`. Additionally, standalone log files can optionally be generated by setting the `DXMT_LOG_PATH` variable, where log files in the given directory will be called `app_d3d11.log`, `app_dxgi.log` etc., where `app` is the name of the game executable.
