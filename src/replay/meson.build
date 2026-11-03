# DXMT's dxmt-replay.exe and dxmt-replay, in Rust (src/replay): linked from the crate's static library.
replay_rust = []
foreach t : rust_pe_targets
  replay_rust += custom_target('dxmt-replay-' + t,
    output             : 'dxmt-replay-' + t + (t.endswith('-msvc') ? '.lib' : '.a'),
    command            : [cargo_staticlib, get_option('cargo_toolchain'), t, 'dxmt-replay', rust_target_dir, '@OUTPUT@'],
    build_always_stale : true,
  )
endforeach

if dxmt_crossbuild
  replay_ld_args = [ '-municode' ]
else
  replay_ld_args = [ '-Wl,-u,_main', '-Wl,-dead_strip' ] + (get_option('debug') ? [] : [ '-Wl,-S' ])
endif

executable('dxmt-replay',
  dependencies : [ rust_runtime_dep ],
  link_with    : replay_rust,
  link_args    : replay_ld_args,
  install      : dxmt_crossbuild,
  install_dir  : windows_native_install_dir,
)
