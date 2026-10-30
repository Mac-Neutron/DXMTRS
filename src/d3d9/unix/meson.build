# mtld3d.so, the mtld3d-unix crate's cdylib (tools/cargo-staticlib.sh's cdylib mode: an output named .so), where
# winemetal.so is built (src/winemetal/meson.build), installed beside it unstripped, as mtld3d's own install did.
if (cpu_family == 'x86_64' or cpu_family == 'aarch64') and not get_option('guest_vm')
  custom_target('mtld3d.so',
    output             : 'mtld3d.so',
    command            : [cargo_staticlib, get_option('cargo_toolchain'),
                          native_compiler_arch == 'arm64' ? 'aarch64-apple-darwin' : 'x86_64-apple-darwin',
                          'mtld3d-unix', rust_target_dir, '@OUTPUT@'],
    env                : d3d9_env,
    build_always_stale : true,
    install            : true,
    install_dir        : unix_install_dir,
  )
endif
