//! winemetal's two Unix-call tables, indexed by slot: the 64-bit one and the `WoW64` one.
//!
//! Wine's loader finds them in `winemetal.so` by their names, `__wine_unix_call_funcs` and
//! `__wine_unix_call_wow64_funcs`; the native `winemetal.dylib`'s C thunks call through the
//! first. The slots are the C's: 158 in the 64-bit table, 83 empty, 153-155 the VM guest's own;
//! the `WoW64` table is the first 153 of them with the 32-bit translator thunks in place.

use core::ffi::c_void;

use crate::{
    ffi::{AbortOnPanic, Handler, STATUS_NOT_IMPLEMENTED},
    g1, g2, g3, g4, g5, g6, g7, g8, g9, g10,
};

/// Slots 153-155: `STATUS_NOT_IMPLEMENTED`.
///
/// The VM guest answers them itself (153-154, its memory; 155, Direct3D forwarding).
#[unsafe(export_name = "_MacNeutron_guestOnly")]
pub extern "C-unwind" fn guest_only(_args: *mut c_void) -> i32 {
    let _guard = AbortOnPanic;
    STATUS_NOT_IMPLEMENTED
}

/// The 64-bit table, as the C's.
const FUNCS: [Option<Handler>; 158] = [
    Some(g1::nsobject_retain),                                             // 0
    Some(g1::nsobject_release),                                            // 1
    Some(g1::nsarray_object),                                              // 2
    Some(g1::nsarray_count),                                               // 3
    Some(g1::mtl_copy_all_devices),                                        // 4
    Some(g1::mtldevice_recommended_max_working_set_size),                  // 5
    Some(g1::mtldevice_current_allocated_size),                            // 6
    Some(g1::mtldevice_name),                                              // 7
    Some(g1::nsstring_get_cstring),                                        // 8
    Some(g3::device_new_command_queue),                                    // 9
    Some(g1::nsautoreleasepool_alloc_init),                                // 10
    Some(g3::queue_command_buffer),                                        // 11
    Some(g3::command_buffer_commit),                                       // 12
    Some(g3::command_buffer_wait_until_completed),                         // 13
    Some(g3::command_buffer_status),                                       // 14
    Some(g7::mtl_device_new_shared_event),                                 // 15
    Some(g7::mtl_shared_event_signaled_value),                             // 16
    Some(g3::command_buffer_encode_signal_event),                          // 17
    Some(g2::new_buffer),                                                  // 18
    Some(g2::new_sampler_state),                                           // 19
    Some(g2::new_depth_stencil_state),                                     // 20
    Some(g2::new_texture),                                                 // 21
    Some(g2::buffer_new_texture),                                          // 22
    Some(g2::texture_new_texture_view),                                    // 23
    Some(g1::mtldevice_minimum_linear_texture_alignment),                  // 24
    Some(g4::mtl_device_new_library),                                      // 25
    Some(g4::mtl_library_new_function),                                    // 26
    Some(g1::nsstring_length_of_bytes),                                    // 27
    Some(g1::nsobject_description),                                        // 28
    Some(g4::mtl_device_new_compute_pipeline_state),                       // 29
    Some(g3::command_buffer_blit_command_encoder),                         // 30
    Some(g3::command_buffer_compute_command_encoder),                      // 31
    Some(g3::command_buffer_render_command_encoder),                       // 32
    Some(g3::command_encoder_end_encoding),                                // 33
    Some(g4::mtl_device_new_render_pipeline_state),                        // 34
    Some(g4::mtl_device_new_mesh_render_pipeline_state),                   // 35
    Some(g3::blit::encode_commands),                                       // 36
    Some(g3::compute::encode_commands),                                    // 37
    Some(g3::render::encode_commands),                                     // 38
    Some(g2::texture_pixel_format),                                        // 39
    Some(g2::texture_width),                                               // 40
    Some(g2::texture_height),                                              // 41
    Some(g2::texture_depth),                                               // 42
    Some(g2::texture_array_length),                                        // 43
    Some(g2::texture_mipmap_level_count),                                  // 44
    Some(g2::texture_replace_region),                                      // 45
    Some(g2::buffer_did_modify_range),                                     // 46
    Some(g3::command_buffer_present_drawable),                             // 47
    Some(g3::command_buffer_present_drawable_after_minimum_duration),      // 48
    Some(g1::mtldevice_supports_family),                                   // 49
    Some(g1::mtldevice_supports_bc_texture_compression),                   // 50
    Some(g1::mtldevice_supports_texture_sample_count),                     // 51
    Some(g1::mtldevice_has_unified_memory),                                // 52
    Some(g9::mtl_capture_manager_shared_capture_manager),                  // 53
    Some(g9::mtl_capture_manager_start_capture),                           // 54
    Some(g9::mtl_capture_manager_stop_capture),                            // 55
    Some(g5::new_temporal_scaler),                                         // 56
    Some(g5::new_spatial_scaler),                                          // 57
    Some(g5::encode_temporal_scale),                                       // 58
    Some(g5::encode_spatial_scale),                                        // 59
    Some(g1::nsstring_string),                                             // 60
    Some(g1::nsstring_alloc_init),                                         // 61
    Some(g6::hud_instance),                                                // 62
    Some(g6::hud_add_label),                                               // 63
    Some(g6::hud_update_label),                                            // 64
    Some(g6::hud_remove),                                                  // 65
    Some(g6::drawable_texture),                                            // 66
    Some(g6::layer_next_drawable),                                         // 67
    Some(g5::supports_fx_spatial_scaler),                                  // 68
    Some(g5::supports_fx_temporal_scaler),                                 // 69
    Some(g6::layer_set_props),                                             // 70
    Some(g6::layer_get_props),                                             // 71
    Some(g6::create_metal_view),                                           // 72
    Some(g6::release_metal_view),                                          // 73
    Some(g10::sm50_initialize),                                            // 74
    Some(g10::sm50_destroy),                                               // 75
    Some(g10::sm50_compile),                                               // 76
    Some(g10::sm50_get_compiled_bitcode),                                  // 77
    Some(g10::sm50_destroy_bitcode),                                       // 78
    Some(g10::sm50_get_error_message),                                     // 79
    Some(g10::sm50_free_error),                                            // 80
    Some(g10::sm50_compile_geometry_pipeline_vertex),                      // 81
    Some(g10::sm50_compile_geometry_pipeline_geometry),                    // 82
    None,                                                                  // 83
    Some(g10::sm50_compile_tessellation_pipeline_hull),                    // 84
    Some(g10::sm50_compile_tessellation_pipeline_domain),                  // 85
    Some(g1::mtlcommandencoder_set_label),                                 // 86
    Some(g1::mtldevice_set_should_maximize_concurrent_compilation),        // 87
    Some(g10::sm50_get_arguments_info),                                    // 88
    Some(g3::command_buffer_error),                                        // 89
    Some(g3::command_buffer_logs),                                         // 90
    Some(g3::log_container_enumerate),                                     // 91
    Some(g6::colorspace_supported),                                        // 92
    Some(g6::layer_set_colorspace),                                        // 93
    Some(g6::display::primary_display_id),                                 // 94
    Some(g6::display::secondary_display_id),                               // 95
    Some(g6::display::display_description),                                // 96
    Some(g6::display::layer_edr_value),                                    // 97
    Some(g4::mtl_library_new_function_with_constants),                     // 98
    Some(g6::display::query_display_setting),                              // 99
    Some(g6::display::update_display_setting),                             // 100
    Some(g6::display::query_display_setting_for_layer),                    // 101
    Some(g3::command_buffer_encode_wait_for_event),                        // 102
    Some(g7::mtl_shared_event_signal_value),                               // 103
    Some(g7::mtl_shared_event_set_win32_event_at_value),                   // 104
    Some(g7::mtl_device_new_fence),                                        // 105
    Some(g7::mtl_device_new_event),                                        // 106
    Some(g2::buffer_update_contents),                                      // 107
    Some(g7::shared_event_listener_create),                                // 108
    Some(g7::shared_event_listener_start),                                 // 109
    Some(g7::shared_event_listener_destroy),                               // 110
    Some(g1::wmt_get_os_version),                                          // 111
    Some(g4::mtl_device_new_binary_archive),                               // 112
    Some(g4::mtl_binary_archive_serialize),                                // 113
    Some(g1::dispatchdata_alloc_init),                                     // 114
    Some(g8::cache_reader_alloc_init),                                     // 115
    Some(g8::cache_reader_get),                                            // 116
    Some(g8::cache_writer_alloc_init),                                     // 117
    Some(g8::cache_writer_set),                                            // 118
    Some(g8::set_metal_shader_cache_path),                                 // 119
    Some(g7::mtl_device_new_shared_texture),                               // 120
    Some(g7::wmt_bootstrap_register),                                      // 121
    Some(g7::wmt_bootstrap_look_up),                                       // 122
    Some(g7::mtl_shared_event_create_mach_port),                           // 123
    Some(g7::mtl_device_new_shared_event_with_mach_port),                  // 124
    Some(g1::mtldevice_registry_id),                                       // 125
    Some(g7::mtl_shared_event_wait_until_signaled_value),                  // 126
    Some(g3::counter_sample_buffer_new_timestamp_buffer),                  // 127
    Some(g3::counter_sample_buffer_resolve_counter_range),                 // 128
    Some(g3::command_buffer_blit_command_encoder_with_sample_buffers),     // 129
    Some(g3::command_buffer_property),                                     // 130
    Some(g4::mtl_device_new_tile_render_pipeline_state),                   // 131
    Some(g2::new_residency_set),                                           // 132
    Some(g2::residency_set_add_allocations),                               // 133
    Some(g2::residency_set_remove_allocations),                            // 134
    Some(g2::residency_set_remove_all_allocations),                        // 135
    Some(g2::residency_set_commit),                                        // 136
    Some(g2::queue_add_residency_set),                                     // 137
    Some(g2::new_heap),                                                    // 138
    Some(g2::heap_buffer_size_and_align),                                  // 139
    Some(g2::heap_texture_size_and_align),                                 // 140
    Some(g2::heap_new_buffer),                                             // 141
    Some(g2::heap_new_texture),                                            // 142
    Some(g2::new_indirect_command_buffer),                                 // 143
    Some(g4::mtl_device_new_library_with_source),                          // 144
    Some(g2::texture_get_bytes),                                           // 145
    Some(g7::mtl_shared_event_set_win32_event_at_values),                  // 146
    Some(g3::command_buffer_compute_command_encoder_with_sample_buffers),  // 147
    Some(g1::mtldevice_sample_timestamps),                                 // 148
    Some(g1::dispatchdata_copy_bytes),                                     // 149
    Some(g5::temporal_scaler_scale_range),                                 // 150
    Some(g5::new_frame_interpolator),                                      // 151
    Some(g5::encode_frame_interpolate),                                    // 152
    Some(guest_only),                                                      // 153
    Some(guest_only),                                                      // 154
    Some(guest_only),                                                      // 155
    Some(g4::mtl_device_new_render_pipeline_state_with_vertex_descriptor), // 156
    Some(g4::mtl_device_new_library_with_source_options),                  // 157
];

/// The 64-bit Unix-call table: the handler of each slot.
#[unsafe(export_name = "__wine_unix_call_funcs")]
pub static UNIX_CALL_FUNCS: [Option<Handler>; 158] = FUNCS;

// sm50_wow64.c's converters, which widen a 32-bit caller's translator params (the Wine build only).
#[cfg(not(feature = "native"))]
unsafe extern "C-unwind" {
    fn thunk32_SM50Initialize(args: *mut c_void) -> i32;
    fn thunk32_SM50Compile(args: *mut c_void) -> i32;
    fn thunk32_SM50GetCompiledBitcode(args: *mut c_void) -> i32;
    fn thunk32_SM50GetErrorMessage(args: *mut c_void) -> i32;
    fn thunk32_SM50CompileGeometryPipelineVertex(args: *mut c_void) -> i32;
    fn thunk32_SM50CompileGeometryPipelineGeometry(args: *mut c_void) -> i32;
    fn thunk32_SM50CompileTessellationPipelineHull(args: *mut c_void) -> i32;
    fn thunk32_SM50CompileTessellationPipelineDomain(args: *mut c_void) -> i32;
    fn thunk32_SM50GetArgumentsInfo(args: *mut c_void) -> i32;
}

/// The `WoW64` Unix-call table, in the Wine build only.
///
/// Slots 0-152 of the 64-bit table, with a 32-bit caller's translator thunks (74, 76, 77, 79, 81,
/// 82, 84, 85, 88) through their converters.
#[cfg(not(feature = "native"))]
#[unsafe(export_name = "__wine_unix_call_wow64_funcs")]
pub static UNIX_CALL_WOW64_FUNCS: [Option<Handler>; 153] = {
    let [mut t @ .., _, _, _, _, _] = FUNCS;
    t[74] = Some(thunk32_SM50Initialize);
    t[76] = Some(thunk32_SM50Compile);
    t[77] = Some(thunk32_SM50GetCompiledBitcode);
    t[79] = Some(thunk32_SM50GetErrorMessage);
    t[81] = Some(thunk32_SM50CompileGeometryPipelineVertex);
    t[82] = Some(thunk32_SM50CompileGeometryPipelineGeometry);
    t[84] = Some(thunk32_SM50CompileTessellationPipelineHull);
    t[85] = Some(thunk32_SM50CompileTessellationPipelineDomain);
    t[88] = Some(thunk32_SM50GetArgumentsInfo);
    t
};
