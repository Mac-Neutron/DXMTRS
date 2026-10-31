use core::{fmt::Write as _, ptr};

use super::*;
use crate::abi::barrier::{
    BarrierGroupBarriers, BarrierSubresourceRange, D3D12_BARRIER_ACCESS_COMMON,
    D3D12_BARRIER_ACCESS_PREDICATION, D3D12_BARRIER_ACCESS_RAYTRACING_ACCELERATION_STRUCTURE_READ,
    D3D12_BARRIER_ACCESS_RAYTRACING_ACCELERATION_STRUCTURE_WRITE,
    D3D12_BARRIER_ACCESS_VIDEO_DECODE_READ, D3D12_BARRIER_ACCESS_VIDEO_ENCODE_WRITE,
    D3D12_BARRIER_LAYOUT_COMMON, D3D12_BARRIER_LAYOUT_UNDEFINED, D3D12_BARRIER_SYNC_ALL,
    D3D12_BARRIER_SYNC_COPY, D3D12_BARRIER_SYNC_PIXEL_SHADING, D3D12_BARRIER_SYNC_RENDER_TARGET,
    D3D12_BARRIER_SYNC_SPLIT,
};

/// The C++ header's own output over the truth table (`golden/golden.txt`).
const GOLDEN: &str = include_str!("golden/golden.txt");

/// The two resources the golden names; the functions only pass the pointers on.
const R: *mut c_void = ptr::without_provenance_mut(0x1000);
const R2: *mut c_void = ptr::without_provenance_mut(0x2000);

/// The access values of the golden's truth table, with the names it prints.
const ACCESSES: [(&str, u32); 29] = [
    ("NO_ACCESS", D3D12_BARRIER_ACCESS_NO_ACCESS),
    ("COMMON", D3D12_BARRIER_ACCESS_COMMON),
    ("VERTEX_BUFFER", D3D12_BARRIER_ACCESS_VERTEX_BUFFER),
    ("CONSTANT_BUFFER", D3D12_BARRIER_ACCESS_CONSTANT_BUFFER),
    ("VERTEX_BUFFER|CONSTANT_BUFFER", 3),
    ("INDEX_BUFFER", D3D12_BARRIER_ACCESS_INDEX_BUFFER),
    ("RENDER_TARGET", D3D12_BARRIER_ACCESS_RENDER_TARGET),
    ("UNORDERED_ACCESS", D3D12_BARRIER_ACCESS_UNORDERED_ACCESS),
    (
        "DEPTH_STENCIL_WRITE",
        D3D12_BARRIER_ACCESS_DEPTH_STENCIL_WRITE,
    ),
    (
        "DEPTH_STENCIL_READ",
        D3D12_BARRIER_ACCESS_DEPTH_STENCIL_READ,
    ),
    ("SHADER_RESOURCE", D3D12_BARRIER_ACCESS_SHADER_RESOURCE),
    ("STREAM_OUTPUT", D3D12_BARRIER_ACCESS_STREAM_OUTPUT),
    ("PREDICATION", D3D12_BARRIER_ACCESS_PREDICATION),
    ("COPY_DEST", D3D12_BARRIER_ACCESS_COPY_DEST),
    ("COPY_SOURCE", D3D12_BARRIER_ACCESS_COPY_SOURCE),
    ("RESOLVE_DEST", D3D12_BARRIER_ACCESS_RESOLVE_DEST),
    ("RESOLVE_SOURCE", D3D12_BARRIER_ACCESS_RESOLVE_SOURCE),
    (
        "RAYTRACING_ACCELERATION_STRUCTURE_READ",
        D3D12_BARRIER_ACCESS_RAYTRACING_ACCELERATION_STRUCTURE_READ,
    ),
    (
        "RAYTRACING_ACCELERATION_STRUCTURE_WRITE",
        D3D12_BARRIER_ACCESS_RAYTRACING_ACCELERATION_STRUCTURE_WRITE,
    ),
    (
        "SHADING_RATE_SOURCE",
        D3D12_BARRIER_ACCESS_SHADING_RATE_SOURCE,
    ),
    ("VIDEO_DECODE_READ", D3D12_BARRIER_ACCESS_VIDEO_DECODE_READ),
    (
        "VIDEO_ENCODE_WRITE",
        D3D12_BARRIER_ACCESS_VIDEO_ENCODE_WRITE,
    ),
    (
        "SHADER_RESOURCE|COPY_SOURCE",
        D3D12_BARRIER_ACCESS_SHADER_RESOURCE | D3D12_BARRIER_ACCESS_COPY_SOURCE,
    ),
    (
        "RENDER_TARGET|UNORDERED_ACCESS",
        D3D12_BARRIER_ACCESS_RENDER_TARGET | D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
    ),
    (
        "SHADER_RESOURCE|UNORDERED_ACCESS",
        D3D12_BARRIER_ACCESS_SHADER_RESOURCE | D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
    ),
    (
        "UNORDERED_ACCESS|COPY_DEST",
        D3D12_BARRIER_ACCESS_UNORDERED_ACCESS | D3D12_BARRIER_ACCESS_COPY_DEST,
    ),
    (
        "SHADER_RESOURCE|RT_AS_READ",
        D3D12_BARRIER_ACCESS_SHADER_RESOURCE
            | D3D12_BARRIER_ACCESS_RAYTRACING_ACCELERATION_STRUCTURE_READ,
    ),
    ("unknown-bit-0x800000", 0x80_0000),
    ("0x7fffffff", 0x7fff_ffff),
];

/// The sync values of the truth table.
const SYNCS: [(&str, u32); 4] = [
    ("NONE", D3D12_BARRIER_SYNC_NONE),
    ("ALL", D3D12_BARRIER_SYNC_ALL),
    ("COPY", D3D12_BARRIER_SYNC_COPY),
    ("SPLIT", D3D12_BARRIER_SYNC_SPLIT),
];

/// A resource as the golden prints it.
fn res(resource: *mut c_void) -> &'static str {
    if resource.is_null() {
        "null"
    } else if resource == R {
        "R"
    } else if resource == R2 {
        "R2"
    } else {
        "?"
    }
}

/// A barrier list as the golden prints it: the count, then each barrier.
fn show(barriers: &[ResourceBarrier]) -> String {
    let mut text = format!("n={}", barriers.len());
    for b in barriers {
        match b.r#type {
            D3D12_RESOURCE_BARRIER_TYPE_TRANSITION => {
                // SAFETY: a transition barrier's union holds a transition.
                let t = unsafe { &b.barrier.transition };
                let (resource, sub) = (res(t.resource), t.subresource);
                let (before, after) = (t.state_before, t.state_after);
                write!(
                    text,
                    " T({resource},sub={sub},{before:#x}->{after:#x},flags={})",
                    b.flags
                )
            }
            D3D12_RESOURCE_BARRIER_TYPE_UAV => {
                // SAFETY: a UAV barrier's union holds a UAV barrier.
                let u = unsafe { &b.barrier.uav };
                write!(text, " U({},flags={})", res(u.resource), b.flags)
            }
            D3D12_RESOURCE_BARRIER_TYPE_ALIASING => {
                // SAFETY: an aliasing barrier's union holds an aliasing barrier.
                let a = unsafe { &b.barrier.aliasing };
                let (before, after) = (res(a.resource_before), res(a.resource_after));
                write!(text, " A({before},{after},flags={})", b.flags)
            }
            other => write!(text, " ?(type={other})"),
        }
        .unwrap();
    }
    text
}

/// A group of `count` barriers at `barriers`.
const fn group(kind: u32, count: u32, barriers: BarrierGroupBarriers) -> BarrierGroup {
    BarrierGroup {
        r#type: kind,
        num_barriers: count,
        barriers,
    }
}

/// A group's pointer to no barriers at all.
const fn no_barriers() -> BarrierGroupBarriers {
    BarrierGroupBarriers {
        global_barriers: ptr::null(),
    }
}

/// `legacy_barriers` as the golden prints it.
///
/// Over `count` groups at `groups`, into an `out` that starts with `prefilled` barriers.
fn legacy(count: u32, groups: *const BarrierGroup, prefilled: usize) -> String {
    let mut out: Vec<ResourceBarrier> = (0..prefilled).map(|_| any_resource_barrier()).collect();
    // SAFETY: every caller passes `count` groups (or null) whose arrays hold the barriers they count.
    unsafe { legacy_barriers(count, groups, &mut out) };
    show(&out)
}

/// A subresource range of the six values.
const fn range(v: [u32; 6]) -> BarrierSubresourceRange {
    BarrierSubresourceRange {
        index_or_first_mip_level: v[0],
        num_mip_levels: v[1],
        first_array_slice: v[2],
        num_array_slices: v[3],
        first_plane: v[4],
        num_planes: v[5],
    }
}

/// One barrier of `group_kind` (a `D3D12_BARRIER_TYPE_*`) in a group of its own.
fn one(
    group_kind: u32,
    global: &GlobalBarrier,
    texture: &TextureBarrier,
    buffer: &BufferBarrier,
) -> String {
    let barriers = match group_kind {
        D3D12_BARRIER_TYPE_GLOBAL => BarrierGroupBarriers {
            global_barriers: ptr::from_ref(global),
        },
        D3D12_BARRIER_TYPE_TEXTURE => BarrierGroupBarriers {
            texture_barriers: ptr::from_ref(texture),
        },
        _ => BarrierGroupBarriers {
            buffer_barriers: ptr::from_ref(buffer),
        },
    };
    legacy(1, &group(group_kind, 1, barriers), 0)
}

/// The truth table: every type, sync before, access before and access after.
fn truth_table() -> Vec<String> {
    let kinds = [
        ("global", D3D12_BARRIER_TYPE_GLOBAL),
        ("texture", D3D12_BARRIER_TYPE_TEXTURE),
        ("buffer", D3D12_BARRIER_TYPE_BUFFER),
    ];
    let mut rows = Vec::new();
    for (kind, group_kind) in kinds {
        for (sync_name, sync) in SYNCS {
            for (before_name, before) in ACCESSES {
                for (after_name, after) in ACCESSES {
                    let global = GlobalBarrier {
                        sync_before: sync,
                        sync_after: D3D12_BARRIER_SYNC_ALL,
                        access_before: before,
                        access_after: after,
                    };
                    let texture = TextureBarrier {
                        sync_before: sync,
                        sync_after: D3D12_BARRIER_SYNC_ALL,
                        access_before: before,
                        access_after: after,
                        layout_before: D3D12_BARRIER_LAYOUT_COMMON,
                        layout_after: D3D12_BARRIER_LAYOUT_COMMON,
                        resource: R,
                        subresources: range([0; 6]),
                        flags: 0,
                    };
                    let buffer = BufferBarrier {
                        sync_before: sync,
                        sync_after: D3D12_BARRIER_SYNC_ALL,
                        access_before: before,
                        access_after: after,
                        resource: R,
                        offset: 0,
                        size: 256,
                    };
                    let out = one(group_kind, &global, &texture, &buffer);
                    rows.push(format!(
                        "{kind} sync={sync_name} before={before_name} after={after_name} -> {out}"
                    ));
                }
            }
        }
    }
    rows
}

/// Layouts, the `DISCARD` flag and subresource ranges change nothing.
fn texture_layouts() -> Vec<String> {
    let mut rows = Vec::new();
    for before in [D3D12_BARRIER_LAYOUT_UNDEFINED, 0, 2, 6, 0x15] {
        for after in [D3D12_BARRIER_LAYOUT_UNDEFINED, 0, 3, 8, 0x1b] {
            for flags in [0, 1] {
                let texture = TextureBarrier {
                    sync_before: D3D12_BARRIER_SYNC_RENDER_TARGET,
                    sync_after: D3D12_BARRIER_SYNC_PIXEL_SHADING,
                    access_before: D3D12_BARRIER_ACCESS_RENDER_TARGET,
                    access_after: D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
                    layout_before: before,
                    layout_after: after,
                    resource: R,
                    subresources: range([before & 3, 2, 1, 3, 0, 2]),
                    flags,
                };
                let barriers = BarrierGroupBarriers {
                    texture_barriers: &raw const texture,
                };
                let out = legacy(1, &group(D3D12_BARRIER_TYPE_TEXTURE, 1, barriers), 0);
                rows.push(format!(
                    "texture-layouts before={before:#x} after={after:#x} flags={flags} -> {out}"
                ));
            }
        }
    }
    rows
}

/// Several barriers in a group, several groups in a call, null arrays, an unknown group type.
fn group_rows() -> Vec<String> {
    let global = |sync_before, access_before, access_after| GlobalBarrier {
        sync_before,
        sync_after: D3D12_BARRIER_SYNC_ALL,
        access_before,
        access_after,
    };
    let globals = [
        global(
            D3D12_BARRIER_SYNC_ALL,
            D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
            D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
        ),
        global(
            D3D12_BARRIER_SYNC_NONE,
            D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
            D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
        ),
        global(
            D3D12_BARRIER_SYNC_ALL,
            D3D12_BARRIER_ACCESS_RENDER_TARGET,
            D3D12_BARRIER_ACCESS_COPY_SOURCE,
        ),
    ];
    let texture = TextureBarrier {
        sync_before: D3D12_BARRIER_SYNC_RENDER_TARGET,
        sync_after: D3D12_BARRIER_SYNC_PIXEL_SHADING,
        access_before: D3D12_BARRIER_ACCESS_RENDER_TARGET,
        access_after: D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
        layout_before: D3D12_BARRIER_LAYOUT_RENDER_TARGET,
        layout_after: D3D12_BARRIER_LAYOUT_SHADER_RESOURCE,
        resource: R2,
        subresources: range([0; 6]),
        flags: 0,
    };
    let buffer = |access_before, access_after, resource| BufferBarrier {
        sync_before: D3D12_BARRIER_SYNC_ALL,
        sync_after: D3D12_BARRIER_SYNC_COPY,
        access_before,
        access_after,
        resource,
        offset: 0,
        size: 64,
    };
    let buffers = [
        buffer(
            D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
            D3D12_BARRIER_ACCESS_UNORDERED_ACCESS,
            R,
        ),
        buffer(
            D3D12_BARRIER_ACCESS_NO_ACCESS,
            D3D12_BARRIER_ACCESS_COPY_DEST,
            R2,
        ),
    ];
    let groups = [
        group(
            D3D12_BARRIER_TYPE_BUFFER,
            2,
            BarrierGroupBarriers {
                buffer_barriers: buffers.as_ptr(),
            },
        ),
        group(
            D3D12_BARRIER_TYPE_GLOBAL,
            3,
            BarrierGroupBarriers {
                global_barriers: globals.as_ptr(),
            },
        ),
        group(
            D3D12_BARRIER_TYPE_TEXTURE,
            1,
            BarrierGroupBarriers {
                texture_barriers: &raw const texture,
            },
        ),
        group(99, 2, no_barriers()),
    ];
    let nulls = [
        group(D3D12_BARRIER_TYPE_BUFFER, 2, no_barriers()),
        group(D3D12_BARRIER_TYPE_GLOBAL, 2, no_barriers()),
        group(D3D12_BARRIER_TYPE_TEXTURE, 2, no_barriers()),
        group(D3D12_BARRIER_TYPE_BUFFER, 0, no_barriers()),
    ];
    let unknown = group(7, 3, no_barriers());
    vec![
        format!("groups four -> {}", legacy(4, groups.as_ptr(), 5)),
        format!(
            "groups first three, out pre-filled -> {}",
            legacy(3, groups.as_ptr(), 3)
        ),
        format!(
            "groups null array, count 3 -> {}",
            legacy(3, ptr::null(), 2)
        ),
        format!("groups count 0 -> {}", legacy(0, groups.as_ptr(), 2)),
        format!(
            "groups with null barrier arrays and an empty group -> {}",
            legacy(4, nulls.as_ptr(), 1)
        ),
        format!(
            "unknown group type 7, three barriers -> {}",
            legacy(1, &raw const unknown, 0)
        ),
    ]
}

/// `legacy_state` and `read_only_access` over the table's values and every single bit.
fn state_rows() -> Vec<String> {
    let row = |name: &str, access: u32| {
        let (state, known) = legacy_state(access);
        let readonly = read_only_access(access);
        format!(
            "legacy-state {name} 0x{access:08x} -> state={state:#x} known={} readonly={}",
            u8::from(known),
            u8::from(readonly)
        )
    };
    let mut rows: Vec<String> = ACCESSES
        .iter()
        .map(|&(name, access)| row(name, access))
        .collect();
    rows.extend((0..32).map(|bit| row(&format!("bit{bit}"), 1 << bit)));
    rows
}

/// `initial_state` over the layouts the header names, `0..=0x20` and a few outside, per heap type.
fn initial_rows() -> Vec<String> {
    let layouts = [0xffff_ffff, 0x7fff_ffff, 0x1000]
        .into_iter()
        .chain(0..=0x20);
    let mut rows = Vec::new();
    for layout in layouts {
        for heap in 0..=4 {
            rows.push(format!(
                "initial-state layout={layout:#x} heap={heap} -> {:#x}",
                initial_state(layout, heap)
            ));
        }
    }
    rows
}

/// Every row of the golden, in the order the C++ prints them.
fn rows() -> Vec<String> {
    [
        truth_table(),
        texture_layouts(),
        group_rows(),
        state_rows(),
        initial_rows(),
    ]
    .concat()
}

#[test]
fn barrier_table_is_the_golden() {
    let want: Vec<&str> = GOLDEN
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    let got = rows();
    for (i, (got, want)) in got.iter().zip(&want).enumerate() {
        assert_eq!(got, want, "row {i} differs from the C++ header's");
    }
    assert_eq!(
        got.len(),
        want.len(),
        "the golden has {} rows, the port {}",
        want.len(),
        got.len()
    );
}
