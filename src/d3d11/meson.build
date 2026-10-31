d3d11_res = wrc_generator.process('version.rc')

d3d11_src = [
  'd3d11_class_linkage.cpp',
  'd3d11_device.cpp',
  'd3d11_input_layout.cpp',
  'd3d11_inspection.cpp',
  'd3d11_query.cpp',
  'd3d11_shader.cpp',
  'd3d11_state_object.cpp',
  'd3d11_swapchain.cpp',
  'd3d11_texture.cpp',
  'd3d11_buffer.cpp',
  'd3d11.cpp',
  'd3d11_resource_helper.cpp',
  'd3d11_resource_view_helper.cpp',
  'd3d11_texture_device.cpp',
  'd3d11_texture_linear.cpp',
  'd3d11_texture_dynamic.cpp',
  'd3d11_resource_staging.cpp',
  'd3d11_pipeline.cpp',
  'd3d11_pipeline_gs.cpp',
  'd3d11_pipeline_ts.cpp',
  'd3d11_enumerable.cpp',
  'd3d11_pipeline_cache.cpp',
  'd3d11_context_imm.cpp',
  'd3d11_context_def.cpp',
  'd3d11_multithread.cpp',
  'd3d11_fence.cpp',
  'd3d11_surface.cpp',
]

d3d10_src = [
  '../d3d10/d3d10_buffer.cpp',
  '../d3d10/d3d10_device.cpp',
  '../d3d10/d3d10_state_object.cpp',
  '../d3d10/d3d10_texture.cpp',
  '../d3d10/d3d10_util.cpp',
  '../d3d10/d3d10_view.cpp',
]

# MacNeutron (DXMTRS step 5a): the Rust linked into d3d11.dll and d3d11.dylib, src/d3d11's static library, which carries
# DXMT's core. Natively it builds with production-unwind: an Objective-C exception from winemetal passes through the
# core's Rust to the caller's @catch, as through winemetal's own. DXMT_PERF_BUILD in cpp_args and the guest_vm option
# build the crate's perf-build and guest-vm features.
d3d11_features = ('-DDXMT_PERF_BUILD' in get_option('cpp_args') ? [ 'perf-build' ] : []) + (get_option('guest_vm') ? [ 'guest-vm' ] : [])
d3d11_cargo_args = [ dxmt_crossbuild ? 'production' : 'production-unwind' ] + (d3d11_features.length() > 0 ? [ ','.join(d3d11_features) ] : [])

# The import libraries the C++ bound to before Rust was linked in, in the order it bound to them, ahead of
# rust_runtime_dep's ntdll: lld binds a symbol two archives define to the first on the line, and dependencies keep
# their order. Without them Wine's i386 libntdll.a, which exports C-runtime functions, takes the C++'s memcpy, _errno,
# swprintf (ntdll's has no count argument) and the rest, and on x86-64 and ARM64EC the unwinder's Rtl* imports move
# from kernel32 to ntdll. d3d12 links the same way.
rust_crt_first_dep = declare_dependency(link_args : dxmt_crossbuild ? [ '-lkernel32', '-luser32', '-lgdi32', '-lmingwex', '-lmsvcrt' ] : [])

d3d11_rust = []
foreach t : rust_pe_targets
  d3d11_rust += custom_target('d3d11-rust-' + t,
    output             : 'd3d11-rust-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'd3d11', rust_target_dir, '@OUTPUT@'] + d3d11_cargo_args,
    build_always_stale : true,
  )
endforeach

d3d11_ld_args      = ['-L' + windows_native_install_dir] # a nice trick to force relink
d3d11_link_depends = []

# Natively the dylib exports the .def's entry points and MTLD3D11WaitIdle (the VM host's dispatch looks it up) and
# nothing else. -Wl,-S (debug builds keep the map): ld64 would otherwise write a debug map naming the Rust archive by
# its build path.
if not dxmt_crossbuild
  foreach n : [ 'D3D11CoreCreateDevice', 'D3D11CreateDevice', 'D3D11CreateDeviceAndSwapChain', 'D3D11On12CreateDevice', 'MTLD3D11WaitIdle' ]
    d3d11_ld_args += [ '-Wl,-exported_symbol,_' + n ]
  endforeach
  d3d11_ld_args += get_option('debug') ? [] : [ '-Wl,-S' ]
  # The Rust archive after libc++ and libSystem, in the driver's order, as a link argument (meson puts link_with
  # libraries ahead of every link argument): ld64 binds a symbol to the first library on the line that defines it, and
  # the archive's compiler_builtins would otherwise take the C++'s ___udivti3 and ___umodti3, with Rust's standard
  # library behind them. ld64 warns that both are given twice.
  d3d11_ld_args += [ '-lc++', '-lSystem', d3d11_rust[0].full_path() ]
  d3d11_link_depends += d3d11_rust
endif

d3d11_dll = shared_library('d3d11', d3d11_src, d3d10_src, d3d11_res,
  name_prefix         : '',
  dependencies        : [ 
    dxgi_dep, 
    dxbc_parser_dep,
    airconv_forward_dep,
    dxmt_dep,
    winemetal_dep,
    util_dep,
    rust_crt_first_dep,
    rust_runtime_dep ],
  link_with           : dxmt_crossbuild ? d3d11_rust : [],
  include_directories : [ dxmt_include_path ],
  install             : true,
  install_dir         : windows_native_install_dir,
  vs_module_defs      : 'd3d11.def',
  link_args           : d3d11_ld_args,
  link_depends        : [ d3d11_link_depends ],
)

d3d11_dep = declare_dependency(
  link_with           : [ d3d11_dll ],
  include_directories : [ dxmt_include_path ],
)

if wine_builtin_dll
custom_target('postprocess_libd3d11',
  input : d3d11_dll,
  output: 'd3d11.dll.postproc',
  command : [ winebuild, '--builtin', '@INPUT@' ],
  depends : d3d11_dll,
  build_by_default : true
)
endif
