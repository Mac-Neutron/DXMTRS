# DXMT's nvngx.dll, in Rust (src/nvngx): the DLL is linked from the crate's static library, its exports
# named by nvngx.def.
nvngx_rust = []
foreach t : rust_pe_targets
  nvngx_rust += custom_target('nvngx-' + t,
    output             : 'nvngx-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'nvngx', rust_target_dir, '@OUTPUT@'],
    build_always_stale : true,
  )
endforeach

nvngx_dll = shared_library('nvngx',
  name_prefix         : '',
  dependencies        : [ rust_runtime_dep ],
  link_with           : nvngx_rust,
  link_args           : [ '-ldxgi' ],
  install             : true,
  install_dir         : windows_native_install_dir,
  vs_module_defs      : 'nvngx.def',
  install_tag         : 'nvext',
)

# a wine-builtin dll where DXMT's are (MacNeutron: native in system32 otherwise, as nvapi64)
if wine_builtin_dll
custom_target('postprocess_libnvngx',
  input : nvngx_dll,
  output: 'nvngx.dll.postproc',
  command : [ winebuild, '--builtin', '@INPUT@' ],
  depends : nvngx_dll,
  build_by_default : true
)
endif
