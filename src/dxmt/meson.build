dxmt_command_air = custom_target('dxmt_command_air',
  input   : 'dxmt_command.metal',
  output  : 'dxmt_command.air',
  command : [ xcrun, '-sdk', 'macosx', 'metal', '-o', '@OUTPUT@', '-c', '@INPUT@' ],
)

dxmt_command_metallib = custom_target('dxmt_command_metallib',
  input   : dxmt_command_air,
  output  : 'dxmt_command.metallib',
  command : [ xcrun, '-sdk', 'macosx', 'metallib', '-o', '@OUTPUT@', '@INPUT@' ],
)

# MacNeutron: the shader translator's key (wine-arm64/build.sh) keys the shader cache when given. Read from a file, so
# a new key regenerates the header (fs.read makes the file a regeneration dependency) without new setup options.
translator_key_file = get_option('translator_key_file')
translator_conf = configuration_data()
translator_conf.set_quoted('DXMT_TRANSLATOR_KEY', translator_key_file == '' ? '' : fs.read(translator_key_file).strip())
dxmt_translator_key_h = configure_file(output : 'dxmt_translator_key.h', configuration : translator_conf)
