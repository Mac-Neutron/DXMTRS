d3d12_src = [
  'd3d12.cpp',
  'd3d12_buffer.cpp',
  'd3d12_command_allocator.cpp',
  'd3d12_command_list.cpp',
  'd3d12_command_queue.cpp',
  'd3d12_command_signature.cpp',
  'd3d12_descriptor_heap.cpp',
  'd3d12_device.cpp',
  'd3d12_dxil_dump.cpp',
  'd3d12_fence.cpp',
  'd3d12_heap.cpp',
  'd3d12_pipeline_compute.cpp',
  'd3d12_pipeline_graphics.cpp',
  'd3d12_query_heap.cpp',
  'd3d12_pipeline_library.cpp',
  'd3d12_pipeline_record.cpp',
  'd3d12_resource_helper.cpp',
  'd3d12_root_signature.cpp',
  'd3d12_sampler.cpp',
  'd3d12_shader_cache.cpp',
  'd3d12_stats.cpp',
  'd3d12_swapchain.cpp',
  'd3d12_texture.cpp',
  '../d3d10/d3d10_blob.cpp',
]

# MacNeutron (DXMTRS step 5a): the Rust linked into d3d12.dll and d3d12.dylib, src/d3d12's static library, which carries
# DXMT's core. Natively it builds with production-unwind: an Objective-C exception from winemetal passes through the
# core's Rust to the caller's @catch, as through winemetal's own. DXMT_PERF_BUILD in cpp_args and the guest_vm option
# build the crate's perf-build and guest-vm features.
d3d12_features = ('-DDXMT_PERF_BUILD' in get_option('cpp_args') ? [ 'perf-build' ] : []) + (get_option('guest_vm') ? [ 'guest-vm' ] : [])
d3d12_cargo_args = [ dxmt_crossbuild ? 'production' : 'production-unwind' ] + (d3d12_features.length() > 0 ? [ ','.join(d3d12_features) ] : [])
d3d12_rust = []
foreach t : rust_pe_targets
  d3d12_rust += custom_target('d3d12-rust-' + t,
    output             : 'd3d12-rust-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'd3d12', rust_target_dir, '@OUTPUT@'] + d3d12_cargo_args,
    build_always_stale : true,
  )
endforeach

d3d12_ld_args      = ['-L' + windows_native_install_dir] # a nice trick to force relink
d3d12_link_depends = []

# Natively the dylib exports the .def's entry points and nothing else. -Wl,-S (debug builds keep the map): ld64 would
# otherwise write a debug map naming the Rust archive by its build path.
if not dxmt_crossbuild
  foreach n : [ 'D3D12CreateDevice', 'D3D12GetDebugInterface', 'D3D12CreateRootSignatureDeserializer', 'D3D12CreateVersionedRootSignatureDeserializer', 'D3D12SerializeRootSignature', 'D3D12SerializeVersionedRootSignature', 'D3D12GetInterface' ]
    d3d12_ld_args += [ '-Wl,-exported_symbol,_' + n ]
  endforeach
  d3d12_ld_args += get_option('debug') ? [] : [ '-Wl,-S' ]
  # The Rust archive after libc++ and libSystem, in the driver's order, as a link argument (meson puts link_with
  # libraries ahead of every link argument): ld64 binds a symbol to the first library on the line that defines it, and
  # the archive's compiler_builtins would otherwise take the C++'s ___udivti3 and ___umodti3, with Rust's standard
  # library behind them. ld64 warns that both are given twice.
  d3d12_ld_args += [ '-lc++', '-lSystem', d3d12_rust[0].full_path() ]
  d3d12_link_depends += d3d12_rust
endif

d3d12_dll = shared_library('d3d12', d3d12_src,
  name_prefix         : '',
  dependencies        : [ dxgi_dep, dxmt_dep, airconv_forward_dep, util_dep, winemetal_dep, dxbc_parser_dep, rust_crt_first_dep, rust_runtime_dep ],
  link_with           : dxmt_crossbuild ? d3d12_rust : [],
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
