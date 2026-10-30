winemetal_src = [
  '../winemetal/winemetal_thunks.c',
  '../winemetal/unix/winemetal_unix.c',
  '../winemetal/unix/cache.c',
  'win32_event.c',
]
winemetal_link_depends = []

winemetal_ld_args = [
  '-weak_framework', 'CoreFoundation', '-weak_framework', 'Metal',
  '-framework', 'MetalFX', '-weak_framework', 'Foundation',
  '-weak_framework', 'CoreGraphics', '-weak_framework', 'QuartzCore',
  '-weak_framework', 'ColorSync',   '-weak_framework', 'Cocoa',
  '-lsqlite3', '-weak_framework', 'AppKit',
] + (get_option('debug') ? [] : ['-Wl,-S'])

# DXMTRS: winemetal's Rust half (mtld3d/unix/winemetal) with its native feature, linked as the Wine build's is
# (src/winemetal/unix/meson.build, AppKit's and -Wl,-S's reasons too), here by link_with: both are host targets.
# b_asneeded stays off: ld64's -dead_strip_dylibs would drop the frameworks objc2 only looks classes up in by name at
# run time.
winemetal_rust = custom_target('winemetal-native-rust',
  output             : 'libwinemetal-rust.a',
  command            : [cargo_staticlib, get_option('cargo_toolchain'),
                        native_compiler_arch == 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin',
                        'winemetal', rust_target_dir, '@OUTPUT@', 'production-unwind', 'native'],
  build_always_stale : true,
)

winemetal_dll = shared_library('winemetal', winemetal_src,
  name_prefix         : '',
  dependencies        : [ airconv_dep_darwin ],
  include_directories : [ dxmt_include_path, include_directories('.') ],
  # The Wine build's winemetal.so defines the same classes: both load in the VM host (macneutron-vm).
  c_args              : ['-ObjC', '-DDXMT_NATIVE=1', '-DCacheReader=DXMTNativeCacheReader',
                         '-DCacheWriter=DXMTNativeCacheWriter'],
  link_args           : winemetal_ld_args,
  link_with           : winemetal_rust,
  build_rpath         : '@loader_path/:@loader_path/../../',
  override_options    : { 'b_asneeded' : false },
)

winemetal_dep = declare_dependency(
  link_with           : [ winemetal_dll ],
  include_directories : [ include_directories('../winemetal') ],
)
