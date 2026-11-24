//! Pure-Rust d3d9 helpers shared by `d3d9.dll`.
//!
//! Host-testable: no COM, no `raw-dylib`, no `winecrt0`. Consumed by
//! `src/d3d9` as an rlib.

/// `log` target for every mtld3d-core call site *except* `dxso::*` and `perf`.
///
/// Shares the COM layer's `"mtld3d::d3d9"` target — these modules were
/// carved out of `d3d9.dll` and from a user's perspective they're still
/// the d3d9 layer. The `dxso` submodule logs to `"mtld3d::dxso"`, `perf`
/// logs to `"mtld3d::perf"`.
const LOG_TARGET: &str = "mtld3d::d3d9";

pub mod address_space;
pub mod api_lock;
pub mod app_profile;
pub mod async_compile;
pub mod buffer_backing;
pub mod buffer_rename;
pub mod caps;
pub mod clear_area;
pub mod config;
pub mod convert;
pub mod crash_report;
pub mod cursor;
pub mod depth_plane;
pub mod depth_stencil_state;
pub mod depth_texture;
pub mod dirty_range;
pub mod dirty_rect;
pub mod display_mode;
pub mod draw_data;
pub mod dxso;
pub mod encoder_data;
pub mod encoder_failure;
pub mod extended;
pub mod fetch4;
pub mod ff_build;
pub mod ff_state;
pub mod format;
pub mod format_probe;
pub mod fpu;
pub mod frame_latency;
pub mod fullscreen_log;
pub mod fullscreen_resize;
pub mod gamma;
pub mod gpu_caps;
#[cfg(any(test, all(target_arch = "x86_64", target_os = "windows")))]
pub mod guest_mem;
pub mod held_pages;
pub mod ids;
pub mod level_authority;
pub mod multisample;
pub mod page_box;
pub mod pe_log;
pub mod perf;
pub mod pipeline_key;
pub mod pipeline_state;
pub mod pixel_convert;
pub mod planar_yuv;
pub mod pool;
pub mod present;
pub mod process_vertices;
pub mod ps_draw;
pub mod query_fence;
pub mod readback;
pub mod render_scale;
pub mod render_state;
pub mod reset_summary;
pub mod sampler_state;
pub mod shader_constants;
pub mod shader_create;
pub mod shader_prewarm;
pub mod staging_coverage;
pub mod state_trace;
pub mod streams;
pub mod stretch_rect;
pub mod texture_flags;
pub mod texture_staging;
pub mod thread_wait;
pub mod upload_pass;
pub mod validate_device;
pub mod visibility;
pub mod vs_draw;
pub mod watch_handoff;
pub mod window_pos;
pub mod window_subclass;
