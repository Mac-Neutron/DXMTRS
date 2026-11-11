subdir('airconv')

# MacNeutron: the shader translator's key (wine-arm64/build.sh) keys the shader cache when given. Read from a file, so
# a new key regenerates the header (fs.read makes the file a regeneration dependency) without new setup options.
# Configured here, before d3d9: mtld3d.so, d3d11 and d3d12 all stamp it.
translator_key_file = get_option('translator_key_file')
translator_conf = configuration_data()
translator_conf.set_quoted('DXMT_TRANSLATOR_KEY', translator_key_file == '' ? '' : fs.read(translator_key_file).strip())
dxmt_translator_key_h = configure_file(output : 'dxmt_translator_key.h', configuration : translator_conf)

if not dxmt_crossbuild
subdir('nativemetal')
else
subdir('winemetal')
endif

subdir('dxmt')
# After dxmt: d3d9's core runtime (d3d9_core_runtime) builds with the core's command library.
if dxmt_crossbuild
subdir('d3d9')
endif
subdir('dxgi')
subdir('d3d11')
subdir('d3d10')

if get_option('enable_d3d12')
subdir('d3d12')
# The replay tool: a Windows program (wmain, d3d12.dll by name), and natively a VM host's (MacNeutron: its main, and
# d3d12.dylib beside it).
subdir('replay')
endif

if get_option('enable_nvapi')
subdir('nvapi')
endif

if get_option('enable_nvngx')
subdir('nvngx')
endif