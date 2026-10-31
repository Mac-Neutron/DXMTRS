native_llvm_path = get_option('native_llvm_path')

if not native_llvm_path.startswith('/')
  native_llvm_path = join_paths(meson.project_source_root(), native_llvm_path)
endif

# airconv's LLVM: the crate links it unbundled (build.rs), so the image that links winemetal's static library names
# it. -lc++ for LLVM and the shim: the images' sources are C, so meson links them with the C driver.
llvm_ld_flags_darwin = [
  '-L'+join_paths(native_llvm_path, 'lib'),
  '-lm', '-lz', '-lcurses', '-lxml2', '-lc++'
]

if native_llvm_path.startswith('/usr/local/opt') or native_llvm_path.startswith('/opt/homebrew/opt')
  homebrew_prefix = native_llvm_path.startswith('/opt/homebrew/opt') ? '/opt/homebrew' : '/usr/local'
  llvm_ld_flags_darwin = [
    llvm_ld_flags_darwin,
    join_paths(homebrew_prefix, 'opt/zstd/lib/libzstd.a'), join_paths(native_llvm_path, 'lib/libunwind.a')
  ]
endif

airconv_dep_darwin = declare_dependency(
  include_directories : [ dxmt_include_path, include_directories('..') ],
  link_args           : [ llvm_ld_flags_darwin, llvm_deps ],
)

# What airconv's build.rs reads (src/airconv/build.rs), for the custom targets that build winemetal's static library:
# the LLVM it compiles the shim against, and the xcrun (a machine file's, which may be a script) for its Metal helpers.
airconv_cargo_env = { 'AIRCONV_LLVM': native_llvm_path, 'AIRCONV_XCRUN': xcrun.full_path() }
