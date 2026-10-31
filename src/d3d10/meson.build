d3d10core_res = wrc_generator.process('version.rc')

# DXMT's d3d10core.dll and d3d10core.dylib, in Rust (src/d3d10): linked from the crate's static library, the DLL's
# exports named by d3d10core.def. DXMT_PERF_BUILD in cpp_args builds the crate's perf-build feature.
d3d10core_cargo_args = [ 'production' ] + ('-DDXMT_PERF_BUILD' in get_option('cpp_args') ? [ 'perf-build' ] : [])
d3d10core_rust = []
foreach t : rust_pe_targets
  d3d10core_rust += custom_target('d3d10core-' + t,
    output             : 'd3d10core-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'd3d10core', rust_target_dir, '@OUTPUT@'] + d3d10core_cargo_args,
    build_always_stale : true,
  )
endforeach

d3d10_ld_args      = ['-L' + windows_native_install_dir] # a nice trick to force relink
d3d10_link_depends = []

# Natively no object names the Rust exports: -u pulls them from the archive, and the dylib exports them and nothing
# else. -Wl,-S (debug builds keep the map): ld64 would otherwise write a debug map naming the archive by its build path.
if not dxmt_crossbuild
  foreach n : [ 'D3D10CoreCreateDevice', 'D3D10CoreRegisterLayers' ]
    d3d10_ld_args += [ '-Wl,-u,_' + n, '-Wl,-exported_symbol,_' + n ]
  endforeach
  d3d10_ld_args += get_option('debug') ? [] : [ '-Wl,-S' ]
endif

d3d10core_dll = shared_library('d3d10core', d3d10core_res,
  name_prefix         : '',
  dependencies        : [ rust_runtime_dep, d3d11_dep ],
  link_with           : d3d10core_rust,
  install             : true,
  install_dir         : windows_native_install_dir,
  vs_module_defs      : 'd3d10core.def',
  link_args           : d3d10_ld_args,
  link_depends        : [ d3d10_link_depends ],
)

d3d10core_dep = declare_dependency(
  link_with           : [ d3d10core_dll ],
  include_directories : [ dxmt_include_path ],
)

if wine_builtin_dll
custom_target('postprocess_libd3d10core',
  input : d3d10core_dll,
  output: 'd3d10core.dll.postproc',
  command : [ winebuild, '--builtin', '@INPUT@' ],
  depends : d3d10core_dll,
  build_by_default : true
)
endif
