# DXMT's nvapi64.dll, in Rust (mtld3d/windows/nvapi): the DLL is linked from the crate's static library, its export
# named by nvapi64.def.
nvapi_rust = []
foreach t : rust_pe_targets
  nvapi_rust += custom_target('nvapi-' + t,
    output             : 'nvapi-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'nvapi', rust_target_dir, '@OUTPUT@'],
    build_always_stale : true,
  )
endforeach

nvapi_dll = shared_library('nvapi64',
  name_prefix         : '',
  dependencies        : [ rust_runtime_dep, winemetal_dep ],
  link_with           : nvapi_rust,
  link_args           : [ '-ldxgi', '-luser32' ],
  install             : true,
  install_dir         : windows_native_install_dir,
  vs_module_defs      : 'nvapi64.def',
  install_tag         : 'nvext',
)

if wine_builtin_dll
custom_target('postprocess_libnvapi64',
  input : nvapi_dll,
  output: 'nvapi64.dll.postproc',
  command : [ winebuild, '--builtin', '@INPUT@' ],
  depends : nvapi_dll,
  build_by_default : true
)
endif
