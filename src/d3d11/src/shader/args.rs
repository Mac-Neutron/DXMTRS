//! Shader variant digests, function names and `SM50_SHADER_*` argument chains (`d3d11_shader.cpp`).

use std::sync::LazyLock;

/// Process-wide shader compile flags from config (`getGlobalShaderFlag`, `d3d11_shader.cpp:12-22`).
pub static FLAGS: LazyLock<u32> = LazyLock::new(|| 0);
