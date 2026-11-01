//! Deduplicating caches for blend, depth-stencil, rasterizer and sampler states.

/// Per-device caches of blend, rasterizer, depth-stencil and sampler state objects.
#[derive(Default)]
pub struct StateCaches;
