dxmt_command_air = custom_target('dxmt_command_air',
  input   : 'dxmt_command.metal',
  output  : 'dxmt_command.air',
  depend_files : 'dxmt_command_constants.hpp',
  command : [ xcrun, '-sdk', 'macosx', 'metal', '-o', '@OUTPUT@', '-c', '@INPUT@' ],
)

dxmt_command_metallib = custom_target('dxmt_command_metallib',
  input   : dxmt_command_air,
  output  : 'dxmt_command.metallib',
  command : [ xcrun, '-sdk', 'macosx', 'metallib', '-o', '@OUTPUT@', '@INPUT@' ],
)

