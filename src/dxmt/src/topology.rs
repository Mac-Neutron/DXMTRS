//! `D3D_PRIMITIVE_TOPOLOGY` helpers D3D11 and D3D12 share.
//!
//! Metal's primitive type, the strip test, the geometry-shader warp table and the index type.
//!
//! The numbering is `D3D_PRIMITIVE_TOPOLOGY`'s, the same in both APIs.

use dxmt_util::dxgi_consts::DXGI_FORMAT_R32_UINT;
use winemetal_wire::{
    WMT_INDEX_TYPE_U_INT16, WMT_INDEX_TYPE_U_INT32, WMT_PRIMITIVE_TYPE_LINE,
    WMT_PRIMITIVE_TYPE_LINE_STRIP, WMT_PRIMITIVE_TYPE_POINT, WMT_PRIMITIVE_TYPE_TRIANGLE,
    WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP,
};

/// `D3D_PRIMITIVE_TOPOLOGY_POINTLIST`.
pub const POINTLIST: u32 = 1;
/// `D3D_PRIMITIVE_TOPOLOGY_LINELIST`.
pub const LINELIST: u32 = 2;
/// `D3D_PRIMITIVE_TOPOLOGY_LINESTRIP`.
pub const LINESTRIP: u32 = 3;
/// `D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST`.
pub const TRIANGLELIST: u32 = 4;
/// `D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP`.
pub const TRIANGLESTRIP: u32 = 5;
/// `D3D_PRIMITIVE_TOPOLOGY_LINELIST_ADJ`.
pub const LINELIST_ADJ: u32 = 10;
/// `D3D_PRIMITIVE_TOPOLOGY_LINESTRIP_ADJ`.
pub const LINESTRIP_ADJ: u32 = 11;
/// `D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST_ADJ`.
pub const TRIANGLELIST_ADJ: u32 = 12;
/// `D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP_ADJ`.
pub const TRIANGLESTRIP_ADJ: u32 = 13;
/// `D3D_PRIMITIVE_TOPOLOGY_1_CONTROL_POINT_PATCHLIST`: the first of the 32 patch lists.
pub const PATCHLIST_1: u32 = 33;
/// `D3D_PRIMITIVE_TOPOLOGY_32_CONTROL_POINT_PATCHLIST`: the last of the 32 patch lists.
pub const PATCHLIST_32: u32 = 64;

/// Metal's `WMTPrimitiveType` for lists and strips of points, lines and triangles.
///
/// Adjacency and patch lists answer none: [`draw_primitive`] draws them as points.
#[must_use]
pub const fn primitive_type(topology: u32) -> Option<u8> {
    match topology {
        POINTLIST => Some(WMT_PRIMITIVE_TYPE_POINT),
        LINELIST => Some(WMT_PRIMITIVE_TYPE_LINE),
        LINESTRIP => Some(WMT_PRIMITIVE_TYPE_LINE_STRIP),
        TRIANGLELIST => Some(WMT_PRIMITIVE_TYPE_TRIANGLE),
        TRIANGLESTRIP => Some(WMT_PRIMITIVE_TYPE_TRIANGLE_STRIP),
        _ => None,
    }
}

/// `(WMTPrimitiveType, control_point_count)` of any topology a draw can name.
///
/// Adjacency lists and patch lists draw as points, a patch list with `topology - 32` control
/// points; every other topology answers what [`primitive_type`] does, with 0 control points.
#[must_use]
pub const fn draw_primitive(topology: u32) -> Option<(u8, u32)> {
    match topology {
        LINELIST_ADJ | LINESTRIP_ADJ | TRIANGLELIST_ADJ | TRIANGLESTRIP_ADJ => {
            Some((WMT_PRIMITIVE_TYPE_POINT, 0))
        }
        PATCHLIST_1..=PATCHLIST_32 => Some((WMT_PRIMITIVE_TYPE_POINT, topology - 32)),
        _ => match primitive_type(topology) {
            Some(primitive) => Some((primitive, 0)),
            None => None,
        },
    }
}

/// Metal's `WMTPrimitiveTopologyClass` (1 point, 2 line, 3 triangle) of `topology`, 0 for none.
///
/// Patch lists are triangle class.
#[must_use]
pub const fn topology_class(topology: u32) -> u32 {
    match topology {
        POINTLIST => 1,
        LINELIST | LINESTRIP | LINELIST_ADJ | LINESTRIP_ADJ => 2,
        TRIANGLELIST
        | TRIANGLESTRIP
        | TRIANGLELIST_ADJ
        | TRIANGLESTRIP_ADJ
        | PATCHLIST_1..=PATCHLIST_32 => 3,
        _ => 0,
    }
}

/// Whether `topology` is a line-strip or triangle-strip topology, with or without adjacency.
#[must_use]
pub const fn is_strip(topology: u32) -> bool {
    matches!(
        topology,
        LINESTRIP | LINESTRIP_ADJ | TRIANGLESTRIP | TRIANGLESTRIP_ADJ
    )
}

/// Whether a geometry shader can run on `topology`.
///
/// The points, lines, triangles and their adjacency forms.
#[must_use]
pub const fn is_geometry_topology(topology: u32) -> bool {
    matches!(topology, POINTLIST..=TRIANGLESTRIP | LINELIST_ADJ..=TRIANGLESTRIP_ADJ)
}

/// `(vertices per warp, vertex increment per warp)` for a geometry-shader draw on `topology`.
///
/// `raw_vertices_per_warp` is the shader's own width: below 6 it is 32, the width the D3D12
/// path always uses (`geometry_warp(topology, 0)`). A topology without a geometry form
/// answers `(vpw, vpw)`; [`is_geometry_topology`] tells them apart.
#[must_use]
pub const fn geometry_warp(topology: u32, raw_vertices_per_warp: u32) -> (u32, u32) {
    let vpw = if raw_vertices_per_warp < 6 {
        32
    } else {
        raw_vertices_per_warp
    };
    match topology {
        LINELIST => {
            let step = max(vpw & !1, 2);
            (step, step)
        }
        LINESTRIP => (vpw, vpw.saturating_sub(1)),
        TRIANGLELIST => {
            let step = max(3 * (vpw / 3), 3);
            (step, step)
        }
        TRIANGLESTRIP => {
            let step = max(vpw & !1, 4);
            (step, step - 2)
        }
        LINELIST_ADJ => {
            let step = max(vpw & !3, 4);
            (step, step)
        }
        LINESTRIP_ADJ => (vpw, vpw.saturating_sub(3)),
        TRIANGLELIST_ADJ => {
            let step = max(6 * (vpw / 6), 6);
            (step, step)
        }
        TRIANGLESTRIP_ADJ => {
            let step = max(vpw & !1, 6);
            (step, step - 4)
        }
        _ => (vpw, vpw),
    }
}

/// `a` or `b`, the larger (`Ord::max` is not `const`).
const fn max(a: u32, b: u32) -> u32 {
    if a > b { a } else { b }
}

/// `(WMTIndexType, stride)` of an index buffer format: `R32_UINT` is 32-bit, anything else 16-bit.
#[must_use]
pub const fn index_type_and_stride(format: u32) -> (u8, u64) {
    if format == DXGI_FORMAT_R32_UINT {
        (WMT_INDEX_TYPE_U_INT32, 4)
    } else {
        (WMT_INDEX_TYPE_U_INT16, 2)
    }
}

#[cfg(test)]
mod tests;
