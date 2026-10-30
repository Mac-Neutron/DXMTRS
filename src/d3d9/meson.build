# DXMTRS: Direct3D 9 (src/d3d9, mtld3d's crates): d3d9.dll and mtld3d.dll from the d3d9 and mtld3d crates' static
# libraries per Rust target, and mtld3d.so (unix/). Their C and C++ (snmalloc-sys, zstd-sys) build with llvm-mingw
# and today's flags: tools/cargo-staticlib.sh takes them from tools/mingw-cc.sh when MTLD3D_LLVM_MINGW names
# llvm-mingw's root, which only these targets pass. Meson keeps the cross file's bare compiler name; llvm-mingw's bin
# holds it.
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

d3d9_rust = {}
foreach crate : ['d3d9', 'mtld3d']
  libs = []
  foreach t : rust_pe_targets
    libs += custom_target(crate + '-' + t,
      output             : crate + '-' + t + '.lib',
      command            : [cargo_staticlib, get_option('cargo_toolchain'), t, crate, rust_target_dir, '@OUTPUT@'],
      env                : d3d9_env,
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
  foreach dll : [['d3d9', files('d3d9.def', 'version.rc')], ['mtld3d', files('shim/mtld3d.def')]]
    custom_target(dll[0] + '.dll',
      input       : dll[1],
      output      : dll[0] + '.dll',
      command     : [arm64x_link, d3d9_llvm_mingw / 'bin', winebuild, d3d9_ntdll, '@INPUT0@', '@OUTPUT@',
                     d3d9_rust[dll[0]]] + (dll[0] == 'd3d9' ? ['@INPUT1@'] : []),
      install     : true,
      install_dir : windows_builtin_install_dir,
    )
  endforeach
endif

subdir('unix')
