winemetal_unix_src = [
  'sm50_wow64.c',
]

if wine_build_path != ''
  winemac_so = join_paths(wine_build_path, 'dlls/winemac.drv/winemac.so')
  ntdll_so = join_paths(wine_build_path, 'dlls/ntdll/ntdll.so')
elif wine_install_path != ''
  winemac_so = join_paths(wine_install_path, 'lib/wine', unix_install_dir, 'winemac.so')
  ntdll_so = join_paths(wine_install_path, 'lib/wine', unix_install_dir, 'ntdll.so')
else
  error('One of the options -Dwine_build_path or -Dwine_install_path is required')
endif

# DXMTRS: winemetal's Rust half (src/winemetal/unix), a static library for the Mac's architecture. winemetal.so is a
# build-machine target, which meson won't link_with a custom target (a host one): the archive goes in by its path, and
# the target in link_depends. Cargo runs on every build (build_always_stale) and decides what to rebuild; the script
# copies only a changed archive, so an unchanged one relinks nothing. AppKit is named: objc2 looks its classes up by
# name, where the C's static references linked it. -Wl,-S (debug builds keep the map): the archive's members carry
# DWARF (the production profiles' debug = 1, and the prebuilt compiler_builtins), so ld64 would write a debug map naming
# the archive by its absolute build path, which no rustc remap reaches; -S drops only debug entries, no symbol.
winemetal_unix_rust = custom_target('winemetal-unix-rust',
  output             : 'libwinemetal-rust.a',
  command            : [cargo_staticlib, get_option('cargo_toolchain'),
                        native_compiler_arch == 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin',
                        'winemetal', rust_target_dir, '@OUTPUT@', 'production-unwind'],
  build_always_stale : true,
  env                : airconv_cargo_env,
)

winemetal_unix_ld_args = [
  '-Wl,-install_name,@rpath/winemetal.so',
  # The two tables are the Rust archive's (table.rs) and no object here names them: -u pulls the member that holds
  # them, and the constructors with it, but only from an archive ld64 meets before winemac.so, which exports the same
  # two names and would satisfy -u itself (a dylib's export binds; no Rust in the image). Keep the archive first.
  winemetal_unix_rust.full_path(), winemac_so, ntdll_so,
  '-weak_framework', 'CoreFoundation', '-weak_framework', 'Metal',
  '-framework', 'MetalFX', '-weak_framework', 'Foundation',
  '-weak_framework', 'CoreGraphics', '-weak_framework', 'QuartzCore',
  '-weak_framework', 'ColorSync',  '-weak_framework', 'Cocoa',
  '-lsqlite3', '-weak_framework', 'AppKit',
  '-Wl,-u,___wine_unix_call_funcs', '-Wl,-u,___wine_unix_call_wow64_funcs',
] + (get_option('debug') ? [] : ['-Wl,-S'])
winemetal_unix_link_depends = [winemetal_unix_rust]

shared_library('winemetal', winemetal_unix_src,
  name_prefix         : '',
  dependencies        : [ airconv_dep_darwin ],
  include_directories : dxmt_include_path,
  c_args              : ['-ObjC'],
  link_args           : winemetal_unix_ld_args,
  link_depends        : winemetal_unix_link_depends,
  native              : true,
  name_suffix         : 'so',
  install_rpath       : '',
  build_rpath         : '@loader_path/:@loader_path/../../',
  override_options    : { 'b_asneeded' : false },
)

meson.add_install_script(
  'install.sh', get_option('strip').to_string(), unix_install_dir, 
  install_tag: 'runtime'
)