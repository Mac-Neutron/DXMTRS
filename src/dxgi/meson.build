dxgi_res = wrc_generator.process('version.rc')

# DXMT's dxgi.dll and dxgi.dylib, in Rust (src/dxgi): linked from the crate's static library, the DLL's exports named by
# dxgi.def. DXMT_PERF_BUILD in cpp_args builds the crate's perf-build feature.
dxgi_cargo_args = [ 'production' ] + ('-DDXMT_PERF_BUILD' in get_option('cpp_args') ? [ 'perf-build' ] : [])
dxgi_rust = []
foreach t : rust_pe_targets
  dxgi_rust += custom_target('dxgi-' + t,
    output             : 'dxgi-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'dxgi', rust_target_dir, '@OUTPUT@'] + dxgi_cargo_args,
    build_always_stale : true,
  )
endforeach

dxgi_ld_args      = ['-L' + windows_native_install_dir] # a nice trick to force relink
dxgi_link_depends = []

# MacNeutron (DXMTRS step 3): natively the dylib exports its five entry points (the .def's names) and nothing else, so no
# other dylib binds to anything but them. No object names the Rust exports: -u pulls them from the archive. -Wl,-S
# (debug builds keep the map): ld64 would otherwise write a debug map naming the archive by its build path.
if not dxmt_crossbuild
  foreach n : [ 'CreateDXGIFactory', 'CreateDXGIFactory1', 'CreateDXGIFactory2', 'DXGIDeclareAdapterRemovalSupport', 'DXGIGetDebugInterface1' ]
    dxgi_ld_args += [ '-Wl,-u,_' + n, '-Wl,-exported_symbol,_' + n ]
  endforeach
  dxgi_ld_args += get_option('debug') ? [] : [ '-Wl,-S' ]
endif

dxgi_dll = shared_library('dxgi', dxgi_res,
  name_prefix         : '',
  dependencies        : [ rust_runtime_dep, winemetal_dep ],
  link_with           : dxgi_rust,
  install             : true,
  install_dir         : windows_native_install_dir,
  vs_module_defs      : 'dxgi.def',
  link_args           : dxgi_ld_args,
  link_depends        : [ dxgi_link_depends ],
)

dxgi_dep = declare_dependency(
  link_with           : [ dxgi_dll ],
  include_directories : [ dxmt_include_path, include_directories('.') ],
)

if wine_builtin_dll
custom_target('postprocess_libdxgi',
  input : dxgi_dll,
  output: 'dxgi.dll.postproc',
  command : [ winebuild, '--builtin', '@INPUT@' ],
  depends : dxgi_dll,
  build_by_default : true
)
endif
