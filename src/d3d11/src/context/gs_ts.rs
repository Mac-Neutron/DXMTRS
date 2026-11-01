//! Geometry-shader and tessellation mesh-draw encoding (`GeometryDraw*`, `TessellationDraw*`).

use super::{Context, Mode};

impl<M: Mode> Context<M> {
    /// Emits a geometry-shader draw (`GeometryDraw`).
    pub const fn geometry_draw(&mut self, vertex_count: u32, start_vertex: u32) {
        let _ = (&self.state, vertex_count, start_vertex);
    }

    /// Emits a tessellation draw (`TessellationDraw`).
    pub const fn tessellation_draw(
        &mut self,
        num_control_points: u32,
        vertex_count_per_instance: u32,
        instance_count: u32,
        start_vertex: u32,
        start_instance: u32,
    ) {
        let _ = (
            &self.state,
            num_control_points,
            vertex_count_per_instance,
            instance_count,
            start_vertex,
            start_instance,
        );
    }
}
