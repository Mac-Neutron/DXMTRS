# DXMTRS: winemetal.dll's thunks and DllMain are Rust (winemetal-pe, src/winemetal), linked from the crate's static
# library per Rust target as nvapi's are; its exports are named by the .def (the VM guest's adds its three calls). The
# SM50 thunks stay C. winemetal_thunks.c is the native dylib's (src/nativemetal).
winemetal_src = [
  'airconv_thunks.c',
]

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
  lib_winecrt0 = cc.find_library('winecrt0', dirs: [
      join_paths(wine_build_path, 'libs/winecrt0', windows_builtin_install_dir),
      join_paths(wine_build_path, 'dlls/winecrt0', windows_builtin_install_dir),
    ],
  )
  lib_ntdll = cc.find_library('ntdll', dirs : [ join_paths(wine_build_path, 'dlls/ntdll', windows_builtin_install_dir)], static: true)
  lib_dbghelp = cc.find_library('dbghelp', dirs : [ join_paths(wine_build_path, 'dlls/dbghelp', windows_builtin_install_dir) ])
  winebuild = find_program(join_paths(wine_build_path, 'tools/winebuild/winebuild'), 'winebuild')
elif wine_install_path != ''
  if not wine_install_path.startswith('/')
    wine_install_path = join_paths(meson.project_source_root(), wine_install_path)
  endif
  lib_winecrt0 = cc.find_library('winecrt0', dirs : [ join_paths(wine_install_path,'lib/wine', windows_builtin_install_dir) ])
  lib_ntdll = cc.find_library('ntdll', dirs : [ join_paths(wine_install_path, 'lib/wine', windows_builtin_install_dir)], static: true)
  lib_dbghelp = cc.find_library('dbghelp', dirs : [ join_paths(wine_install_path, 'lib/wine', windows_builtin_install_dir) ])
  winebuild = find_program(join_paths(wine_install_path, 'bin/winebuild'), 'winebuild')
else
  error('One of the options -Dwine_build_path or -Dwine_install_path is required')
endif

winemetal_dll = shared_library('winemetal', winemetal_src,
  name_prefix         : '',
  dependencies        : [ lib_winecrt0, lib_ntdll, lib_dbghelp, airconv_forward_dep, rust_runtime_dep ],
  link_with           : winemetal_rust,
  vs_module_defs      : get_option('guest_vm') ? 'winemetal-guest.def' : 'winemetal.def',
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

# MacNeutron: a guest_vm build's winemetal.so is the VM's own (eac-vm/metal/wm).
if (cpu_family == 'x86_64' or cpu_family == 'aarch64') and not get_option('guest_vm')
  subdir('unix')
endif
