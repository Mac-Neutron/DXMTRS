project('dxmt', ['c', 'cpp'], version : 'v0.1', meson_version: '>=1.3.0', default_options : [ 
  'cpp_std=c++20', 
  'warning_level=2', 
  'build.cpp_std=c++20',
  'b_ndebug=if-release',
])

fs = import('fs')

dxmt_crossbuild = meson.is_cross_build()

cpp = meson.get_compiler('cpp')
cc = meson.get_compiler('c')
clang = meson.get_compiler('c', native: true)
clangpp = meson.get_compiler('cpp', native: true)

cpu_family = host_machine.cpu_family()

dxmt_include_dirs = [
  './include',
  './libs'
]

# include/native/d3d12 first: the D3D12 headers the PE build compiles against (llvm-mingw's, dxmt/pins in MacNeutron),
# newer than the directx submodule's.
dxmt_native_include_dirs = [
  './include/native/d3d12',
  './include/native/windows',
  './include/native/directx',
]

if not dxmt_crossbuild
  dxmt_include_dirs += dxmt_native_include_dirs
  # As the PE build's mingw headers: methods that return a struct take its address (D3D12's GetDesc and siblings),
  # so native callers of these headers see the signatures DXMT implements.
  add_project_arguments('-DWIDL_EXPLICIT_AGGREGATE_RETURNS', language: ['c', 'cpp'])
endif

source_prefix_path = fs.relative_to(
    meson.project_source_root(),
    meson.project_build_root()
)

compiler_args = [
  '-Wimplicit-fallthrough',
  # gcc
  '-Wno-missing-field-initializers',
  '-Wno-unused-parameter',
  '-Wno-cast-function-type', # Needed for GetProcAddress.
  # clang
  '-Wno-unused-private-field',
  '-Wno-microsoft-exception-spec',
  '-Wno-extern-c-compat',
  '-Wno-unused-const-variable',
  '-Wno-missing-braces',
  '-fblocks',
  '-fmacro-prefix-map=' + source_prefix_path + '/=',
]

if cpu_family == 'x86'
  compiler_args += [
    '-msse',
    '-msse2',
    '-msse3',
    '-mfpmath=sse',
  ]
endif

native_compiler_args = [
  '-Wimplicit-fallthrough',
  '-Wno-unused-parameter',
  '-Wno-unused-private-field',
  '-Wno-unused-const-variable',
  '-Wno-missing-braces',
  '-fblocks',
]

link_args = []

compiler_args += [
  '-DNOMINMAX',
  '-D_WIN32_WINNT=0xa00',
]

link_args += [
  '-static',
  # -static is sufficient for clang, while c runtime will be linked to api-ms-win-crt-* (ucrt)
  # '-static-libgcc',
  # '-static-libstdc++',
  # We need to set the section alignment for debug symbols to
  # work properly as well as avoiding a memcpy from the Wine loader.
  '-Wl,--file-alignment=4096',
]

# Wine's built-in back traces only work with dwarf4 symbols
if get_option('debug')
  compiler_args += [
    '-gdwarf-4',
  ]
endif

dxmt_include_path = include_directories(dxmt_include_dirs)
dxmt_native_include_path = include_directories(dxmt_native_include_dirs)

if not fs.is_file('include/native/directx/d3d11.h')
  error('Don\'t forget to clone submodules. `git submodule update --init --recursive`')
endif

if dxmt_crossbuild
  res_ext = '.o'
  wrc = find_program('windres')
  wrc_generator = generator(wrc,
    output    : [ '@BASENAME@' + res_ext ],
    arguments : [ '-i', '@INPUT@', '-o', '@OUTPUT@' ],
  )
else
  wrc           = find_program('touch')
  wrc_generator = generator(wrc, output : [ '@BASENAME@_ignored.h' ], arguments : [ '@OUTPUT@' ] )
endif

xcrun = find_program('xcrun')

metalir_generator = generator(xcrun,
  output    : [ '@BASENAME@.air'],
  arguments : [ '-sdk', 'macosx', 'metal', '-o', '@OUTPUT@', '-c', '@INPUT@', '@EXTRA_ARGS@'],
)

metallib_generator = generator(xcrun,
  output    : [ '@BASENAME@.metallib'],
  arguments : [ '-sdk', 'macosx', 'metallib', '-o', '@OUTPUT@', '@INPUT@'],
)

xxd = find_program('xxd')

hexdump_generator = generator(xxd,
  output    : [ '@BASENAME@.h'],
  arguments : [ '-n', '@BASENAME@', '-i', '@INPUT@', '@OUTPUT@'],
)

dxmt_debug = get_option('dxmt_debug')

if dxmt_debug
add_project_arguments('-DDXMT_DEBUG=1', language: 'cpp')
endif

add_project_arguments('-DDXMT_PAGE_SIZE=4096', language: 'cpp')

# MacNeutron: memory DXMT hands to Metal comes from winemetal (WMTMemoryAlloc), which the VM's host maps too.
if get_option('guest_vm')
  add_project_arguments('-DDXMT_GUEST_VM', language: ['c', 'cpp'])
endif

dxmt_version = vcs_tag(
  command: ['git', 'describe', '--always'],
  input:  'version.h.in',
  output: 'version.h',
)

if dxmt_crossbuild
  wine_builtin_dll = get_option('wine_builtin_dll')

  if cpu_family == 'x86'
    # Enable stdcall fixup on 32-bit
    link_args += [
      '-Wl,--enable-stdcall-fixup',
      '-Wl,--kill-at',
    ]
    # Fix stack alignment issues with mingw on 32-bit
    compiler_args += [
      '-mpreferred-stack-boundary=2'
    ]
    if wine_builtin_dll
      windows_native_install_dir = 'i386-windows'
    else
      windows_native_install_dir = 'syswow64'
    endif
    windows_builtin_install_dir = 'i386-windows'
  elif cpu_family == 'x86_64'
    if wine_builtin_dll
      windows_native_install_dir = 'x86_64-windows'
    else
      windows_native_install_dir = 'system32'
    endif
    windows_builtin_install_dir = 'x86_64-windows'
  elif cpu_family == 'aarch64'
    if wine_builtin_dll
      windows_native_install_dir = 'aarch64-windows'
    else
      windows_native_install_dir = 'system32'
    endif
    windows_builtin_install_dir = 'aarch64-windows'
  else
    error('invalid cpu family')
  endif

  unix_install_dir = cpu_family + '-unix'
else
  wine_builtin_dll = false
  windows_native_install_dir = ''
endif

native_compiler_arch = cpu_family == 'aarch64' ? 'arm64' : 'x86_64'

add_project_arguments(cpp.get_supported_arguments(compiler_args), language: 'cpp')
add_project_arguments(clangpp.get_supported_arguments(native_compiler_args), language: 'cpp', native: true)
add_project_arguments(cc.get_supported_arguments(compiler_args), language: 'c')
add_project_arguments(clang.get_supported_arguments(native_compiler_args), language: 'c', native: true)
add_project_link_arguments(cpp.get_supported_link_arguments(link_args), language: 'cpp')
add_project_link_arguments(cc.get_supported_link_arguments(link_args), language: 'c')
if dxmt_crossbuild
  add_project_arguments(['-arch', native_compiler_arch], language: 'c',  native: true)
  add_project_arguments(['-arch', native_compiler_arch], language: 'cpp',  native: true)
  add_project_link_arguments(['-arch', native_compiler_arch], language: 'cpp',  native: true)
endif

# DXMTRS Phase 0 (E1): the Rust static libraries the binaries link, one per Rust target; an ARM64X DLL links both
# halves'. rust_pe_dep: the D3D DLLs (in a native build, macOS code); rust_unix_dep: winemetal's Unix side.
cargo_staticlib = find_program('tools/cargo-staticlib.sh')
rust_target_dir = meson.current_build_dir() / 'cargo'
rust_darwin = cpu_family == 'aarch64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin'
if not dxmt_crossbuild
  rust_pe_targets = [rust_darwin]
elif cpu_family == 'aarch64'
  rust_pe_targets = ['aarch64-pc-windows-msvc', 'arm64ec-pc-windows-msvc']
elif cpu_family == 'x86_64'
  rust_pe_targets = ['x86_64-pc-windows-msvc']
else
  rust_pe_targets = ['i686-pc-windows-msvc']
endif
rust_pe_libs = []
foreach t : rust_pe_targets
  rust_pe_libs += custom_target('dxmt_pe-' + t,
    output             : 'dxmt_pe-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'dxmt-pe', rust_target_dir, '@OUTPUT@'],
    build_always_stale : true,
  )
endforeach
# Rust's standard library for Windows calls ntdll directly (NtWriteFile, RtlNtStatusToDosError).
# i686 also calls MSVC's 64-bit arithmetic helpers (__aulldiv), which Wine's ntdll exports undecorated (llvm-mingw's
# import library has them only @16-decorated).
rust_pe_wine_ntdll = []
if dxmt_crossbuild and cpu_family == 'x86'
  rust_wine_dir = get_option('wine_install_path') != '' ? get_option('wine_install_path') / 'lib/wine' : get_option('wine_build_path') / 'dlls/ntdll'
  rust_pe_wine_ntdll = [cc.find_library('ntdll', static : true,
    dirs : [meson.project_source_root() / rust_wine_dir / windows_builtin_install_dir])]
endif
rust_pe_dep = declare_dependency(link_with : rust_pe_libs, link_args : dxmt_crossbuild ? ['-lntdll'] : [],
  dependencies : rust_pe_wine_ntdll,
  sources : files('src/util/rust_pe_rt.c'))
rust_unix_lib = custom_target('dxmt_unix-' + rust_darwin,
  output             : 'dxmt_unix-' + rust_darwin + '.a',
  command            : [cargo_staticlib, get_option('cargo_toolchain'), rust_darwin, 'dxmt-unix', rust_target_dir, '@OUTPUT@'],
  build_always_stale : true,
)

subdir('libs')
subdir('src')

if get_option('enable_tests')
subdir('tests')
endif
