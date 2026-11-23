# Direct3D 9 (src/d3d9): d3d9.dll from the d3d9 crate's static libraries per Rust target. Its C and C++
# (snmalloc-sys) build with llvm-mingw and today's flags: tools/cargo-staticlib.sh takes them from
# tools/mingw-cc.sh when MTLD3D_LLVM_MINGW names llvm-mingw's root, which only these targets pass.
# Meson keeps the cross file's bare compiler name; llvm-mingw's bin holds it.
d3d9_perf_build = '-DDXMT_PERF_BUILD' in get_option('cpp_args')
if get_option('d3d9_perf_tracking') and d3d9_perf_build
  error('d3d9_perf_tracking with DXMT_PERF_BUILD: perf-build compiles out the timers\' info lines, so they would cost time and report nothing')
endif
d3d9_features = ['front-end'] + (d3d9_perf_build ? ['perf-build'] : []) + (get_option('d3d9_perf_tracking') ? ['perf-tracking'] : []) + (get_option('guest_vm') ? ['guest-vm'] : [])

d3d9_llvm_mingw = fs.parent(fs.parent(find_program(cc.cmd_array()[0]).full_path()))
if not d3d9_llvm_mingw.startswith('/')
  error('d3d9: llvm-mingw\'s root must be absolute, found ' + d3d9_llvm_mingw)
endif

d3d9_env = {
  'MTLD3D_LLVM_MINGW': d3d9_llvm_mingw,
  'DXMT_COMMAND_METALLIB': dxmt_command_metallib.full_path(),
  'DXMT_TRANSLATOR_KEY_H': dxmt_translator_key_h.full_path(),
  'DXMT_VERSION_H': dxmt_version.full_path(),
}
d3d9_rust = []
foreach t : rust_pe_targets
  d3d9_rust += custom_target('d3d9-' + t,
    output             : 'd3d9-' + t + '.lib',
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'd3d9', rust_target_dir, '@OUTPUT@',
                          'production', ','.join(d3d9_features)],
    env                : d3d9_env,
    depends            : [dxmt_command_metallib, dxmt_version],
    build_always_stale : true,
  )
endforeach

# The Mac's ARM64X pair, Wine builtins beside winemetal.dll (tools/arm64x-link.sh: both halves in one image).
if cpu_family == 'aarch64'
  arm64x_link = find_program(meson.project_source_root() / 'tools/arm64x-link.sh')
  if wine_build_path != ''
    d3d9_ntdll = wine_build_path / 'dlls/ntdll' / windows_builtin_install_dir / 'libntdll.a'
  else
    d3d9_ntdll = wine_install_path / 'lib/wine' / windows_builtin_install_dir / 'libntdll.a'
  endif
  custom_target('d3d9.dll',
    input       : files('d3d9.def', 'version.rc'),
    output      : 'd3d9.dll',
    command     : [arm64x_link, '--import', winemetal_imp, d3d9_llvm_mingw / 'bin', winebuild, d3d9_ntdll,
                   '@INPUT0@', '@OUTPUT@', d3d9_rust, '@INPUT1@'],
    install     : true,
    install_dir : windows_builtin_install_dir,
  )
else
  # Every other cross build (the VM guest's x86_64 and i386 pairs): linked by meson's driver as nvngx.dll is, then
  # marked builtin. lld's MinGW driver does no ICF unless asked; llvm-mingw's CRT comes ahead of Wine's libntdll.a
  # (which meson places after these), so strlen, wcslen and mem* stay ucrt's, and kernel32 ahead of mincore's sets.
  if cpu_family == 'x86'
    # rustc exports each i386 stdcall entry under its decorated symbol (`_Name@N`), so an undecorated `.def` entry
    # would lose its ordinal and `NONAME` to rustc's `.drectve` export. Rewrite each `Name @ord ... ; N` as
    # `Name=Name@N @ord ...` for MinGW lld (which prepends `_` to match `_Name@N`).
    d3d9_awk = find_program('awk')
    d3d9_def = custom_target('d3d9.i686.def',
      input   : 'd3d9.def',
      output  : 'd3d9.i686.def',
      capture : true,
      command : [
        d3d9_awk,
        'BEGIN { in_exports = 0 } { line = $0; semi = index(line, ";"); if (semi > 0) { entry = substr(line, 1, semi - 1); comment = substr(line, semi + 1) } else { entry = line; comment = "" } gsub(/^[[:blank:]]+|[[:blank:]]+$/, "", entry); if (!in_exports || entry == "") { if (entry == "EXPORTS") in_exports = 1; print line; next } n = split(entry, f, /[[:blank:]]+/); name = f[1]; if (n < 2 || substr(f[2], 1, 1) != "@") { print "d3d9.def: entry " name " has no @ordinal" > "/dev/stderr"; exit 1 } gsub(/^[[:blank:]]+|[[:blank:]]+$/, "", comment); if (comment !~ /^[0-9]+$/) { print "d3d9.def: entry " name " has no i386 argument bytes after ;" > "/dev/stderr"; exit 1 } out = "    " name "=" name "@" comment " " f[2]; for (i = 3; i <= n; i++) out = out " " f[i]; print out }',
        '@INPUT@',
      ],
    )
  else
    d3d9_def = 'd3d9.def'
  endif
  d3d9_link_args = ['-Wl,--gc-sections', '-Wl,--icf=all', '-lmingw32', '-lmingwex', '-lucrt', '-lkernel32', '-lmincore',
                    '-luser32', '-ladvapi32', '-lgdi32', '-lws2_32', '-luserenv', '-lbcrypt', '-ldbghelp']
  d3d9_dll = shared_library('d3d9', wrc_generator.process('version.rc'),
    name_prefix    : '',
    link_with      : d3d9_rust,
    vs_module_defs : d3d9_def,
    dependencies   : [lib_ntdll, rust_runtime_dep, winemetal_dep],
    link_args      : d3d9_link_args,
    install        : true,
    install_dir    : windows_builtin_install_dir,
  )
  custom_target('postprocess_d3d9',
    input            : d3d9_dll,
    output           : 'd3d9.dll.postproc',
    command          : [winebuild, '--builtin', '@INPUT@'],
    depends          : d3d9_dll,
    build_by_default : true,
  )
endif
