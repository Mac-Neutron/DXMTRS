dxgi_res = wrc_generator.process('version.rc')

dxgi_src = [
  'dxgi_adapter.cpp',
  'dxgi_factory.cpp',
  'dxgi_output.cpp',
  'dxgi_options.cpp',
  'dxgi.cpp',
]

dxgi_ld_args      = ['-L' + windows_native_install_dir] # a nice trick to force relink
dxgi_link_depends = []

# MacNeutron (DXMTRS step 3): natively the dylib exports its five entry points (the .def's names) and nothing else, so no
# other dylib binds to anything but them. The C++ objects define the names; no -u.
if not dxmt_crossbuild
  foreach n : [ 'CreateDXGIFactory', 'CreateDXGIFactory1', 'CreateDXGIFactory2', 'DXGIDeclareAdapterRemovalSupport', 'DXGIGetDebugInterface1' ]
    dxgi_ld_args += [ '-Wl,-exported_symbol,_' + n ]
  endforeach
endif

dxgi_dll = shared_library('dxgi', dxgi_src, dxgi_res,
  name_prefix         : '',
  dependencies        : [ dxmt_dep, util_dep, winemetal_dep ],
  include_directories : [ dxmt_include_path ],
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
