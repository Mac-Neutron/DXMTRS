use winemetal_wire::{
    WMT_INDEX_TYPE_U_INT16, WMT_INDEX_TYPE_U_INT32, WMT_PRIMITIVE_TYPE_LINE_STRIP,
    WMT_PRIMITIVE_TYPE_POINT, WMT_PRIMITIVE_TYPE_TRIANGLE,
};

use super::{
    LINESTRIP, PATCHLIST_1, POINTLIST, TRIANGLELIST, draw_primitive, geometry_warp,
    index_type_and_stride, is_geometry_topology, is_strip, primitive_type, topology_class,
};

/// d3d11's `gs_indirect_vertex_counts`, the oracle for [`geometry_warp`].
const fn d3d11_gs_indirect_vertex_counts(topology: u32, raw_vpw: u32) -> (u32, u32) {
    let vpw = if raw_vpw < 6 { 32 } else { raw_vpw };
    match topology {
        2 => {
            let s = if (vpw & !1) < 2 { 2 } else { vpw & !1 };
            (s, s)
        }
        3 => (vpw, vpw.saturating_sub(1)),
        4 => {
            let s = 3 * (vpw / 3);
            let s = if s < 3 { 3 } else { s };
            (s, s)
        }
        5 => {
            let s = if (vpw & !1) < 4 { 4 } else { vpw & !1 };
            (s, s - 2)
        }
        10 => {
            let s = if (vpw & !3) < 4 { 4 } else { vpw & !3 };
            (s, s)
        }
        11 => (vpw, vpw.saturating_sub(3)),
        12 => {
            let s = 6 * (vpw / 6);
            let s = if s < 6 { 6 } else { s };
            (s, s)
        }
        13 => {
            let s = if (vpw & !1) < 6 { 6 } else { vpw & !1 };
            (s, s - 4)
        }
        _ => (vpw, vpw),
    }
}

#[test]
fn geometry_warp_is_d3d11s_computation() {
    for topology in 0..70 {
        for raw in [0, 5, 6, 7, 31, 32, 40, 64, 1000] {
            assert_eq!(
                geometry_warp(topology, raw),
                d3d11_gs_indirect_vertex_counts(topology, raw),
                "topology {topology} width {raw}"
            );
        }
    }
}

/// d3d12's fixed table is the same computation at the default width.
#[test]
fn geometry_warp_at_the_default_width_is_d3d12s_table() {
    let table = [
        (1, (32, 32)),
        (2, (32, 32)),
        (10, (32, 32)),
        (3, (32, 31)),
        (4, (30, 30)),
        (12, (30, 30)),
        (5, (32, 30)),
        (11, (32, 29)),
        (13, (32, 28)),
    ];
    for (topology, expected) in table {
        assert!(is_geometry_topology(topology));
        assert_eq!(geometry_warp(topology, 0), expected);
    }
    for topology in [0, 6, 9, 14, 33, 64, 65] {
        assert!(!is_geometry_topology(topology));
    }
}

#[test]
fn primitive_types_as_d3d11_and_d3d12_have_them() {
    assert_eq!(primitive_type(POINTLIST), Some(WMT_PRIMITIVE_TYPE_POINT));
    assert_eq!(
        primitive_type(LINESTRIP),
        Some(WMT_PRIMITIVE_TYPE_LINE_STRIP)
    );
    assert_eq!(
        primitive_type(TRIANGLELIST),
        Some(WMT_PRIMITIVE_TYPE_TRIANGLE)
    );
    assert_eq!(primitive_type(10), None);
    assert_eq!(primitive_type(PATCHLIST_1), None);
    assert_eq!(
        draw_primitive(TRIANGLELIST),
        Some((WMT_PRIMITIVE_TYPE_TRIANGLE, 0))
    );
    assert_eq!(draw_primitive(13), Some((WMT_PRIMITIVE_TYPE_POINT, 0)));
    assert_eq!(
        draw_primitive(PATCHLIST_1),
        Some((WMT_PRIMITIVE_TYPE_POINT, 1))
    );
    assert_eq!(draw_primitive(64), Some((WMT_PRIMITIVE_TYPE_POINT, 32)));
    assert_eq!(draw_primitive(0), None);
    assert_eq!(draw_primitive(6), None);
    assert_eq!(draw_primitive(65), None);
}

/// The plain types and the adjacency and patch points are d3d12's indirect-draw table.
#[test]
fn draw_primitive_without_control_points_is_the_indirect_table() {
    for topology in 0..70 {
        let indirect = match topology {
            1 | 10..=13 | 33..=64 => Some(WMT_PRIMITIVE_TYPE_POINT),
            other => primitive_type(other),
        };
        assert_eq!(
            draw_primitive(topology).map(|p| p.0),
            indirect,
            "{topology}"
        );
    }
}

#[test]
fn classes_and_strips() {
    let classes: Vec<u32> = (0..=14).map(topology_class).collect();
    assert_eq!(classes, [0, 1, 2, 2, 3, 3, 0, 0, 0, 0, 2, 2, 3, 3, 0]);
    assert_eq!(topology_class(33), 3);
    assert_eq!(topology_class(64), 3);
    assert_eq!(topology_class(65), 0);
    let strips: Vec<u32> = (0..70).filter(|t| is_strip(*t)).collect();
    assert_eq!(strips, [3, 5, 11, 13]);
}

#[test]
fn index_types() {
    assert_eq!(index_type_and_stride(42), (WMT_INDEX_TYPE_U_INT32, 4));
    assert_eq!(index_type_and_stride(57), (WMT_INDEX_TYPE_U_INT16, 2));
    assert_eq!(index_type_and_stride(0), (WMT_INDEX_TYPE_U_INT16, 2));
}
