# DXMTRS: Direct3D 9 (src/d3d9, mtld3d's crates): d3d9.dll and mtld3d.dll from the d3d9 and mtld3d crates' static
# libraries per Rust target, and mtld3d.so (unix/). Their C and C++ (snmalloc-sys, zstd-sys) build with llvm-mingw
# and today's flags: tools/cargo-staticlib.sh takes them from tools/mingw-cc.sh when MTLD3D_LLVM_MINGW names
# llvm-mingw's root, which only these targets pass. Meson keeps the cross file's bare compiler name; llvm-mingw's bin
# holds it.
# Features, one rule for both halves: perf-build (DXMT_PERF_BUILD in cpp_args) on d3d9, mtld3d and mtld3d-unix;
# perf-tracking (-Dd3d9_perf_tracking=true) on d3d9 and mtld3d-unix, never on the shim.
d3d9_perf_build = '-DDXMT_PERF_BUILD' in get_option('cpp_args')
if get_option('d3d9_perf_tracking') and d3d9_perf_build
  error('d3d9_perf_tracking with DXMT_PERF_BUILD: perf-build compiles out the timers\' info lines, so they would cost time and report nothing')
endif
d3d9_features = {
  'd3d9'   : (d3d9_perf_build ? ['perf-build'] : []) + (get_option('d3d9_perf_tracking') ? ['perf-tracking'] : []),
  'mtld3d' : d3d9_perf_build ? ['perf-build'] : [],
}

d3d9_llvm_mingw = fs.parent(fs.parent(find_program(cc.cmd_array()[0]).full_path()))
if not d3d9_llvm_mingw.startswith('/')
  error('d3d9: llvm-mingw\'s root must be absolute, found ' + d3d9_llvm_mingw)
endif

# The shim bundles Wine's unix_lib.o out of this arch's libwinecrt0.a (shim/build.rs); cc.find_library returns no path.
if wine_build_path != ''
  d3d9_winecrt0_dirs = [wine_build_path / 'libs/winecrt0', wine_build_path / 'dlls/winecrt0']
else
  d3d9_winecrt0_dirs = [wine_install_path / 'lib/wine']
endif
d3d9_winecrt0 = ''
foreach d : d3d9_winecrt0_dirs
  f = d / windows_builtin_install_dir / 'libwinecrt0.a'
  if d3d9_winecrt0 == '' and fs.is_file(f)
    d3d9_winecrt0 = f
  endif
endforeach
if d3d9_winecrt0 == ''
  error('d3d9: no ' + windows_builtin_install_dir + '/libwinecrt0.a in ' + ', '.join(d3d9_winecrt0_dirs))
endif
d3d9_env = { 'MTLD3D_LLVM_MINGW': d3d9_llvm_mingw, 'MTLD3D_WINECRT0': d3d9_winecrt0 }

# DXMTRS C4: d3d9_core_runtime builds the d3d9 crate's core-runtime feature (d3d9-rt on dxmt-core, runtime.mode=core)
# into every half that can import winemetal.dll: arm64ec, x86_64 and i686. winemetal.dll has no native arm64 half, so
# the ARM64X image's aarch64 half is built without it and a native arm64 process keeps mtld3d. That feature needs the
# core's command library and the translator key, as d3d11's crate does; guest_vm builds d3d9's guest-vm feature.
d3d9_core = get_option('d3d9_core_runtime')
d3d9_core_env = {
  'DXMT_COMMAND_METALLIB': dxmt_command_metallib.full_path(),
  'DXMT_TRANSLATOR_KEY_H': dxmt_translator_key_h.full_path(),
  'DXMT_VERSION_H': dxmt_version.full_path(),
}
d3d9_rust = {}
foreach crate : ['d3d9', 'mtld3d']
  libs = []
  foreach t : rust_pe_targets
    features = d3d9_features[crate]
    env = d3d9_env
    depends = []
    if crate == 'd3d9'
      features += get_option('guest_vm') ? ['guest-vm'] : []
      if d3d9_core and t != 'aarch64-pc-windows-msvc'
        features += ['core-runtime']
        env += d3d9_core_env
        depends = [dxmt_command_metallib, dxmt_version]
      endif
    endif
    libs += custom_target(crate + '-' + t,
      output             : crate + '-' + t + '.lib',
      command            : [cargo_staticlib, get_option('cargo_toolchain'), t, crate, rust_target_dir, '@OUTPUT@',
                            'production', ','.join(features)],
      env                : env,
      depends            : depends,
      build_always_stale : true,
    )
  endforeach
  d3d9_rust += { crate : libs }
endforeach

# The Mac's ARM64X pair, Wine builtins beside winemetal.dll (tools/arm64x-link.sh: both halves in one image).
if cpu_family == 'aarch64'
  arm64x_link = find_program(meson.project_source_root() / 'tools/arm64x-link.sh')
  if wine_build_path != ''
    d3d9_ntdll = wine_build_path / 'dlls/ntdll' / windows_builtin_install_dir / 'libntdll.a'
  else
    d3d9_ntdll = wine_install_path / 'lib/wine' / windows_builtin_install_dir / 'libntdll.a'
  endif
  # The core runtime's ARM64EC half imports winemetal.dll, whose own image is ARM64EC alone: an import library of the
  # .def winemetal.dll exports, for that half only (arm64x-link.sh's --ec-import).
  d3d9_ec_import = []
  if d3d9_core
    d3d9_ec_import = custom_target('winemetal-ec.lib',
      input   : files('../winemetal/winemetal.def'),
      output  : 'winemetal-ec.lib',
      command : [d3d9_llvm_mingw / 'bin/llvm-dlltool', '-m', 'arm64ec', '-D', 'winemetal.dll', '-d', '@INPUT@',
                 '-l', '@OUTPUT@'],
    )
  endif
  foreach dll : [['d3d9', files('d3d9.def', 'version.rc')], ['mtld3d', files('shim/mtld3d.def')]]
    ec_import = dll[0] == 'd3d9' and d3d9_core ? ['--ec-import', d3d9_ec_import] : []
    custom_target(dll[0] + '.dll',
      input       : dll[1],
      output      : dll[0] + '.dll',
      command     : [arm64x_link] + ec_import + [d3d9_llvm_mingw / 'bin', winebuild, d3d9_ntdll, '@INPUT0@',
                     '@OUTPUT@', d3d9_rust[dll[0]]] + (dll[0] == 'd3d9' ? ['@INPUT1@'] : []),
      install     : true,
      install_dir : windows_builtin_install_dir,
    )
  endforeach
else
  # Every other cross build (the VM guest's x86_64 and i386 pairs): linked by meson's driver as nvngx.dll is, then
  # marked builtin. lld's MinGW driver does no ICF unless asked; llvm-mingw's CRT comes ahead of Wine's libntdll.a
  # (which meson places after these), so strlen, wcslen and mem* stay ucrt's, and kernel32 ahead of mincore's sets.
  d3d9_link_args = ['-Wl,--gc-sections', '-Wl,--icf=all', '-lmingw32', '-lmingwex', '-lucrt', '-lkernel32', '-lmincore',
                    '-luser32', '-ladvapi32', '-lgdi32', '-lws2_32', '-luserenv', '-lbcrypt', '-ldbghelp']
  foreach dll : [['d3d9', 'd3d9.def', wrc_generator.process('version.rc')], ['mtld3d', 'shim/mtld3d.def', []]]
    d3d9_dll = shared_library(dll[0], dll[2],
      name_prefix    : '',
      link_with      : d3d9_rust[dll[0]],
      vs_module_defs : dll[1],
      dependencies   : [lib_ntdll, rust_runtime_dep] + (dll[0] == 'd3d9' and d3d9_core ? [winemetal_dep] : []),
      link_args      : d3d9_link_args,
      install        : true,
      install_dir    : windows_builtin_install_dir,
    )
    custom_target('postprocess_' + dll[0],
      input            : d3d9_dll,
      output           : dll[0] + '.dll.postproc',
      command          : [winebuild, '--builtin', '@INPUT@'],
      depends          : d3d9_dll,
      build_by_default : true,
    )
  endforeach
endif

subdir('unix')
