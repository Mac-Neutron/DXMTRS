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

