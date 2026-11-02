# DXMT's d3d12.dll and d3d12.dylib, in Rust (src/d3d12): linked from the crate's static library, the DLL's exports
# named by d3d12.def. Natively it builds with production-unwind: an Objective-C exception from winemetal passes through
# the core's Rust to the caller's @catch, as through winemetal's own. DXMT_PERF_BUILD in cpp_args and the guest_vm
# option build the crate's perf-build and guest-vm features.
d3d12_features = ('-DDXMT_PERF_BUILD' in get_option('cpp_args') ? [ 'perf-build' ] : []) + (get_option('guest_vm') ? [ 'guest-vm' ] : [])
d3d12_cargo_args = [ dxmt_crossbuild ? 'production' : 'production-unwind' ] + (d3d12_features.length() > 0 ? [ ','.join(d3d12_features) ] : [])
d3d12_env = {
  'DXMT_COMMAND_METALLIB': dxmt_command_metallib.full_path(),
  'DXMT_TRANSLATOR_KEY_H': dxmt_translator_key_h.full_path(),
  'DXMT_VERSION_H': dxmt_version.full_path(),
}
d3d12_rust = []
foreach t : rust_pe_targets
  d3d12_rust += custom_target('d3d12-rust-' + t,
    output             : 'd3d12-rust-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'd3d12', rust_target_dir, '@OUTPUT@'] + d3d12_cargo_args,
    env                : d3d12_env,
    depends            : [ dxmt_version, dxmt_command_metallib ],
    build_always_stale : true,
  )
endforeach

d3d12_ld_args      = ['-L' + windows_native_install_dir] # a nice trick to force relink
d3d12_link_depends = []

# Natively no object names the Rust exports: -u pulls them from the archive, -dead_strip drops unreferenced symbols,
# and the dylib exports the .def's entry points and nothing else. -Wl,-S (debug builds keep the map): ld64 would
# otherwise write a debug map naming the archive by its build path.
if not dxmt_crossbuild
  foreach n : [ 'D3D12CreateDevice', 'D3D12GetDebugInterface', 'D3D12CreateRootSignatureDeserializer', 'D3D12CreateVersionedRootSignatureDeserializer', 'D3D12SerializeRootSignature', 'D3D12SerializeVersionedRootSignature', 'D3D12GetInterface' ]
    d3d12_ld_args += [ '-Wl,-u,_' + n, '-Wl,-exported_symbol,_' + n ]
  endforeach
  d3d12_ld_args += [ '-Wl,-dead_strip' ] + (get_option('debug') ? [] : [ '-Wl,-S' ])
endif

d3d12_dll = shared_library('d3d12',
  name_prefix         : '',
  dependencies        : [ dxgi_dep, winemetal_dep, rust_runtime_dep ],
  link_with           : d3d12_rust,
  include_directories : [ dxmt_include_path ],
  install             : true,
  install_dir         : windows_native_install_dir,
  vs_module_defs      : 'd3d12.def',
  link_args           : d3d12_ld_args,
  link_depends        : [ d3d12_link_depends ],
)

d3d12_dep = declare_dependency(
  link_with           : [ d3d12_dll ],
  include_directories : [ dxmt_include_path, include_directories('.') ],
)

if wine_builtin_dll
custom_target('postprocess_libd3d12',
  input : d3d12_dll,
  output: 'd3d12.dll.postproc',
  command : [ winebuild, '--builtin', '@INPUT@' ],
  depends : d3d12_dll,
  build_by_default : true
)
endif
