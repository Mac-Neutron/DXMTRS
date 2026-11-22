# DXMTRS: winemetal.dll's thunks and DllMain are Rust (winemetal-pe, src/winemetal), linked from the crate's static
# library per Rust target as nvapi's are; its exports are named by the .def (the VM guest's adds its three calls). The
# translator's SM50 thunks are in the same crate (src/winemetal/src/sm50.rs).
winemetal_src = []

winemetal_rust = []
foreach t : rust_pe_targets
  winemetal_rust += custom_target('winemetal-pe-' + t,
    output             : 'winemetal-pe-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'winemetal-pe', rust_target_dir, '@OUTPUT@'],
    build_always_stale : true,
  )
endforeach

winemetal_ld_args = []

winemetal_link_depends = []

wine_build_path = get_option('wine_build_path')
wine_install_path = get_option('wine_install_path')
if wine_build_path != ''
  if not wine_build_path.startswith('/')
    wine_build_path = join_paths(meson.project_source_root(), wine_build_path)
  endif
  winebuild = find_program(join_paths(wine_build_path, 'tools/winebuild/winebuild'), 'winebuild')
elif wine_install_path != ''
  if not wine_install_path.startswith('/')
    wine_install_path = join_paths(meson.project_source_root(), wine_install_path)
  endif
  winebuild = find_program(join_paths(wine_install_path, 'bin/winebuild'), 'winebuild')
else
  error('One of the options -Dwine_build_path or -Dwine_install_path is required')
endif

winemetal_def = files(get_option('guest_vm') ? 'winemetal-guest.def' : 'winemetal.def')

if cpu_family == 'aarch64'
  winemetal_llvm_mingw = fs.parent(fs.parent(find_program(cc.cmd_array()[0]).full_path()))
  if not winemetal_llvm_mingw.startswith('/')
    error('winemetal: llvm-mingw\'s root must be absolute, found ' + winemetal_llvm_mingw)
  endif
  if wine_build_path != ''
    winemetal_winecrt0_dirs = [wine_build_path / 'libs/winecrt0', wine_build_path / 'dlls/winecrt0']
    winemetal_ntdll_path = wine_build_path / 'dlls/ntdll' / windows_builtin_install_dir / 'libntdll.a'
  else
    winemetal_winecrt0_dirs = [wine_install_path / 'lib/wine']
    winemetal_ntdll_path = wine_install_path / 'lib/wine' / windows_builtin_install_dir / 'libntdll.a'
  endif
  winemetal_winecrt0_path = ''
  foreach d : winemetal_winecrt0_dirs
    f = d / windows_builtin_install_dir / 'libwinecrt0.a'
    if winemetal_winecrt0_path == '' and fs.is_file(f)
      winemetal_winecrt0_path = f
    endif
  endforeach
  if winemetal_winecrt0_path == ''
    error('winemetal: no ' + windows_builtin_install_dir + '/libwinecrt0.a in ' + ', '.join(winemetal_winecrt0_dirs))
  endif
  if not fs.is_file(winemetal_ntdll_path)
    error('winemetal: missing ' + winemetal_ntdll_path)
  endif

  winemetal_arm64x_link = find_program(meson.project_source_root() / 'tools/arm64x-link.sh')
  winemetal_imp = custom_target('winemetal.dll.a',
    input   : winemetal_def,
    output  : 'winemetal.dll.a',
    command : [winemetal_llvm_mingw / 'bin/llvm-dlltool', '-m', 'arm64ec', '-D', 'winemetal.dll', '-d', '@INPUT@',
               '-N', '@INPUT@', '-l', '@OUTPUT@'],
  )
  winemetal_dll = custom_target('winemetal.dll',
    input       : winemetal_def,
    output      : 'winemetal.dll',
    command     : [winemetal_arm64x_link, '--winecrt0', winemetal_winecrt0_path, winemetal_llvm_mingw / 'bin',
                   winebuild, winemetal_ntdll_path, '@INPUT@', '@OUTPUT@', winemetal_rust],
    install     : true,
    install_dir : windows_builtin_install_dir,
    build_by_default : true,
  )
  winemetal_dep = declare_dependency(
    link_with           : [ winemetal_imp ],
    include_directories : [ include_directories('.') ],
  )
else
  if wine_build_path != ''
    lib_winecrt0 = cc.find_library('winecrt0', dirs: [
        join_paths(wine_build_path, 'libs/winecrt0', windows_builtin_install_dir),
        join_paths(wine_build_path, 'dlls/winecrt0', windows_builtin_install_dir),
      ],
    )
    lib_ntdll = cc.find_library('ntdll', dirs : [ join_paths(wine_build_path, 'dlls/ntdll', windows_builtin_install_dir)], static: true)
    lib_dbghelp = cc.find_library('dbghelp', dirs : [ join_paths(wine_build_path, 'dlls/dbghelp', windows_builtin_install_dir) ])
  else
    lib_winecrt0 = cc.find_library('winecrt0', dirs : [ join_paths(wine_install_path,'lib/wine', windows_builtin_install_dir) ])
    lib_ntdll = cc.find_library('ntdll', dirs : [ join_paths(wine_install_path, 'lib/wine', windows_builtin_install_dir)], static: true)
    lib_dbghelp = cc.find_library('dbghelp', dirs : [ join_paths(wine_install_path, 'lib/wine', windows_builtin_install_dir) ])
  endif

  winemetal_dll = shared_library('winemetal', winemetal_src,
    name_prefix         : '',
    dependencies        : [ lib_winecrt0, lib_ntdll, lib_dbghelp, airconv_forward_dep, rust_runtime_dep ],
    link_with           : winemetal_rust,
    vs_module_defs      : winemetal_def,
    include_directories : dxmt_include_path,
    install             : true,
    install_dir         : windows_builtin_install_dir,
    link_args           : winemetal_ld_args,
    link_depends        : [ winemetal_link_depends ],
  )

  winemetal_dep = declare_dependency(
    link_with           : [ winemetal_dll ],
    include_directories : [ include_directories('.') ],
  )

  custom_target('postprocess_lib',
    input : winemetal_dll,
    output: 'winemetal.dll.postproc',
    command : [ winebuild, '--builtin', '@INPUT@' ],
    depends : winemetal_dll,
    build_by_default : true
  )
endif

# MacNeutron: a guest_vm build's winemetal.so is the VM's own (eac-vm/metal/wm).
if (cpu_family == 'x86_64' or cpu_family == 'aarch64') and not get_option('guest_vm')
  subdir('unix')
endif
