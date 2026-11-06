use core::fmt::Write as _;

use super::*;
use crate::abi::{
    barrier::D3D12_HEAP_TYPE_UPLOAD,
    resource::{
        D3D12_HEAP_FLAG_NONE, D3D12_MEMORY_POOL_L0, D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
        D3D12_TEXTURE_LAYOUT_UNKNOWN, MipRegion,
    },
};

/// The C++ native `d3d12.dylib`'s own answers over the description table (Task 0's golden).
const GOLDEN: &str = include_str!("golden/golden.txt");
/// The same for descriptions and ranges the table does not reach (null, refused, wrapping).
const FOOT_EDGE: &str = include_str!("golden/foot-edge.txt");
/// The real validation functions over their input classes.
///
/// `ValidateResourceDescs`, `ValidateHeapProperties`, `ValidateResourceStates`,
/// `IsCpuVisibleHeap` and `IsPrivateHeap`.
const VALIDATE: &str = include_str!("golden/validate.txt");
/// `IsPrivateHeap` with `DXMT_D3D12_PRIVATE=0`.
const VALIDATE_PRIVATE_OFF: &str = include_str!("golden/validate0.txt");
/// The real `GetResourceExtent`.
const EXTENT: &str = include_str!("golden/extent.txt");
/// The real `DecomposeSubresource`.
const DECOMPOSE: &str = include_str!("golden/decompose.txt");
/// The real `IsD3D12BoxInBounds`.
const BOX: &str = include_str!("golden/box.txt");
/// The body of the real `GetResourceAllocationInfo1`, run on fake Metal answers.
const ALLOC: &str = include_str!("golden/alloc.txt");

/// What `DecomposeSubresource` leaves in an output it does not write, in the oracle.
const LEFT_ALONE: &str = "57005";

/// The value after `key=` in a golden line (hexadecimal with a `0x`).
fn field(line: &str, key: &str) -> u64 {
    let prefix = format!("{key}=");
    let token = line
        .split(' ')
        .find_map(|token| token.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("no {key} in {line}"));
    token.strip_prefix("0x").map_or_else(
        || token.parse().expect("a number"),
        |hex| u64::from_str_radix(hex, 16).expect("a hexadecimal number"),
    )
}

/// `field` as a 32-bit number.
fn field32(line: &str, key: &str) -> u32 {
    u32::try_from(field(line, key)).expect("a 32-bit number")
}

/// The description a `desc` or `edge` header line names.
fn desc_of(line: &str) -> ResourceDesc {
    ResourceDesc {
        dimension: field32(line, "dim"),
        alignment: 0,
        width: field(line, "w"),
        height: field32(line, "h"),
        depth_or_array_size: u16::try_from(field(line, "depth_or_array")).expect("16 bits"),
        mip_levels: u16::try_from(field(line, "mips")).expect("16 bits"),
        format: field32(line, "fmt"),
        sample_desc: SampleDesc {
            count: field32(line, "samples"),
            quality: 0,
        },
        layout: field32(line, "layout"),
        flags: field32(line, "flags"),
    }
}

/// A buffer or texture description with the fields the checks read.
fn desc(dimension: u32, width: u64, height: u32, depth_or_array: u16, mips: u16) -> ResourceDesc {
    ResourceDesc {
        dimension,
        alignment: 0,
        width,
        height,
        depth_or_array_size: depth_or_array,
        mip_levels: mips,
        format: DXGI_FORMAT_UNKNOWN,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        flags: 0,
    }
}

/// Heap properties of a type, a CPU page property and a memory pool.
const fn heap(kind: u32, cpu_page: u32, pool: u32) -> HeapProperties {
    HeapProperties {
        r#type: kind,
        cpu_page_property: cpu_page,
        memory_pool_preference: pool,
        creation_node_mask: 0,
        visible_node_mask: 0,
    }
}

/// The character the oracle prints for an `HRESULT`.
fn hr_char(hr: Hresult) -> char {
    match hr {
        S_OK => '.',
        E_INVALIDARG => 'x',
        _ => '?',
    }
}

/// The lines `GetCopyableFootprints` gives for a range, in the golden's own form.
fn footprint_lines(
    desc: Option<&ResourceDesc>,
    tag: &str,
    first: u32,
    count: u32,
    base: u64,
) -> Vec<String> {
    let mut subs = Vec::new();
    let total = copyable_footprints(desc, first, count, base, |index, footprint| {
        let layout = &footprint.layout;
        let shape = &layout.footprint;
        subs.push(format!(
            "  sub {} off={} fmt={} w={} h={} d={} pitch={} rows={} rowsize={}",
            first + index,
            layout.offset,
            shape.format,
            shape.width,
            shape.height,
            shape.depth,
            shape.row_pitch,
            footprint.num_rows,
            footprint.row_size
        ));
    });
    let mut lines = vec![format!(
        " footprints {tag} first={first} count={count} base={base} total={total}"
    )];
    lines.extend(subs);
    lines
}

/// Replays every `footprints` range of a golden and compares the lines.
fn replay_footprints(golden: &str) {
    let mut current: Option<ResourceDesc> = None;
    let mut expected = Vec::new();
    let mut actual = Vec::new();
    for line in golden.lines() {
        if line.starts_with("desc ") || line.starts_with("edge ") {
            current = (!line.ends_with(" null")).then(|| desc_of(line));
        } else if line.starts_with(" footprints ") {
            expected.push(line.to_owned());
            let tag = line.split(' ').nth(2).expect("a tag");
            actual.extend(footprint_lines(
                current.as_ref(),
                tag,
                field32(line, "first"),
                field32(line, "count"),
                field(line, "base"),
            ));
        } else if line.starts_with("  sub ") {
            expected.push(line.to_owned());
        }
    }
    assert_eq!(actual.len(), expected.len());
    for (got, want) in actual.iter().zip(&expected) {
        assert_eq!(got, want);
    }
    assert!(expected.len() > 40);
}

#[test]
fn footprints_are_the_golden() {
    replay_footprints(GOLDEN);
}

#[test]
fn footprint_edge_cases_as_cpp() {
    replay_footprints(FOOT_EDGE);
}

#[test]
fn allocation_info_is_the_golden() {
    // Metal's own sizes are not in the golden: a buffer's is its width, a texture's the golden's
    // size (already aligned) with no alignment of its own. What is checked is the alignment of the
    // result and the rounding.
    let mut checked = 0;
    let mut current: Option<ResourceDesc> = None;
    for line in GOLDEN.lines() {
        if line.starts_with("desc ") {
            current = Some(desc_of(line));
        } else if line.starts_with(" alloc ") {
            let want = (field(line, "size"), field(line, "align"));
            let desc = current.take().expect("a description");
            let is_buffer = desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER;
            let infos = allocation_info(&[desc], |desc| {
                Some(HeapSizeAndAlign {
                    size: u32::try_from(if is_buffer { desc.width } else { want.0 })
                        .expect("a 32-bit size"),
                    align: 0,
                })
            });
            assert_eq!((infos.total.size_in_bytes, infos.total.alignment), want);
            assert_eq!(infos.each.len(), 1);
            assert_eq!(infos.each[0].offset, 0);
            assert_eq!(infos.each[0].alignment, want.1);
            checked += 1;
        }
    }
    assert_eq!(checked, 45);
}

#[test]
fn allocation_info_replays_the_cpp_oracle() {
    let mut multi = 0;
    for line in ALLOC.lines() {
        let (input, want) = line.split_once(" =>  ").expect("a result");
        let (count, specs) = input
            .strip_prefix("A ")
            .and_then(|rest| rest.split_once(':'))
            .expect("a count");
        let mut descs = Vec::new();
        let mut fakes = Vec::new();
        for spec in specs.split_whitespace() {
            let parts: Vec<&str> = spec.split(',').collect();
            let is_buffer = parts[0] == "B";
            let mut desc = desc(
                if is_buffer {
                    D3D12_RESOURCE_DIMENSION_BUFFER
                } else {
                    D3D12_RESOURCE_DIMENSION_TEXTURE2D
                },
                descs.len() as u64,
                1,
                1,
                1,
            );
            desc.alignment = parts[1].parse().expect("an alignment");
            desc.sample_desc.count = parts[2].parse().expect("a sample count");
            fakes.push(HeapSizeAndAlign {
                size: parts[3].parse().expect("a size"),
                align: parts[4].parse().expect("an alignment"),
            });
            descs.push((desc, parts[5] == "1"));
        }
        assert_eq!(descs.len(), count.trim().parse::<usize>().expect("a count"));
        let plain: Vec<ResourceDesc> = descs.iter().map(|(desc, _)| desc).map(copy_desc).collect();
        let infos = allocation_info(&plain, |desc| {
            let index = usize::try_from(desc.width).expect("an index");
            let ok = desc.dimension == D3D12_RESOURCE_DIMENSION_BUFFER || descs[index].1;
            ok.then(|| HeapSizeAndAlign {
                size: fakes[index].size,
                align: fakes[index].align,
            })
        });
        let mut got = format!("{},{} |", infos.total.size_in_bytes, infos.total.alignment);
        for info in &infos.each {
            write!(
                got,
                " {},{},{}",
                info.offset, info.alignment, info.size_in_bytes
            )
            .expect("a string");
        }
        assert_eq!(got, want, "{line}");
        multi += usize::from(plain.len() > 1 && infos.each.len() == plain.len());
    }
    assert!(multi > 100);
}

/// A copy of a description, for tests that keep one beside the array they pass.
fn copy_desc(from: &ResourceDesc) -> ResourceDesc {
    ResourceDesc {
        sample_desc: SampleDesc {
            count: from.sample_desc.count,
            quality: from.sample_desc.quality,
        },
        ..*from
    }
}

#[test]
fn validate_descs_as_cpp() {
    let mut checked = 0;
    for line in VALIDATE.lines().filter(|line| !line.starts_with("C ")) {
        let (head, want) = line.split_once(':').expect("a result");
        let numbers: Vec<u32> = head[2..]
            .split(' ')
            .map(|word| word.parse().expect("a number"))
            .collect();
        let mut got = String::new();
        match head.as_bytes()[0] {
            b'V' => {
                let props = heap(numbers[0], numbers[1], 0);
                for flag_bits in 0..16_u32 {
                    for mips in [1_u16, 2] {
                        for array in [1_u16, 2] {
                            let mut desc = desc(numbers[2], 0, 0, array, mips);
                            desc.layout = numbers[3];
                            desc.flags = [
                                (1, D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET),
                                (2, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS),
                                (4, D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS),
                                (8, D3D12_RESOURCE_FLAG_ALLOW_CROSS_ADAPTER),
                            ]
                            .iter()
                            .filter(|(bit, _)| flag_bits & bit != 0)
                            .fold(0, |flags, (_, flag)| flags | flag);
                            got.push(hr_char(validate_resource_descs(&desc, &props)));
                        }
                    }
                }
            }
            b'H' => {
                let props = heap(numbers[0], numbers[1], numbers[2]);
                for flags in [
                    D3D12_HEAP_FLAG_NONE,
                    D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH,
                    D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER,
                    D3D12_HEAP_FLAG_ALLOW_WRITE_WATCH | D3D12_HEAP_FLAG_SHARED_CROSS_ADAPTER,
                ] {
                    for numa in [false, true] {
                        got.push(hr_char(validate_heap_properties(&props, flags, numa)));
                    }
                }
            }
            b'S' => {
                let props = heap(numbers[0], 0, 0);
                let mut states = vec![0_u32];
                states.extend((0..32).map(|bit| 1_u32 << bit));
                for low in 0..32 {
                    for high in low + 1..32 {
                        states.push((1_u32 << low) | (1_u32 << high));
                    }
                }
                states.extend([0x7fff_ffff, 0xffff_ffff]);
                for state in states {
                    got.push(hr_char(validate_resource_states(state, &props)));
                }
            }
            kind => panic!("a line of kind {}", char::from(kind)),
        }
        assert_eq!(got, want, "{head}");
        checked += 1;
    }
    assert_eq!(checked, 6 * 4 * 6 * 5 + 6 * 5 * 4 + 6);
}

#[test]
fn heap_visibility_as_cpp() {
    for (golden, private_on) in [(VALIDATE, true), (VALIDATE_PRIVATE_OFF, false)] {
        let mut checked = 0;
        for line in golden.lines().filter(|line| line.starts_with("C ")) {
            let (head, want) = line.split_once(':').expect("a result");
            let numbers: Vec<u32> = head[2..]
                .split(' ')
                .map(|word| word.parse().expect("a number"))
                .collect();
            let props = heap(numbers[0], numbers[1], 0);
            assert_eq!(
                format!(
                    " {} {}",
                    u8::from(is_cpu_visible_heap(&props)),
                    u8::from(is_private_heap(&props, private_on))
                ),
                want,
                "{head}"
            );
            checked += 1;
        }
        assert_eq!(checked, 30);
    }
}

#[test]
fn extents_as_cpp() {
    let mut checked = 0;
    for line in EXTENT.lines() {
        let (head, want) = line.split_once(':').expect("a result");
        let words: Vec<&str> = head[2..].split(' ').collect();
        let desc = desc(
            words[0].parse().expect("a dimension"),
            words[1].parse().expect("a width"),
            words[2].parse().expect("a height"),
            words[3].parse().expect("a depth"),
            1,
        );
        let mut got = String::new();
        for mip in [0, 1, 5, 31, 32, 33, 63, 64] {
            let b = resource_extent(&desc, mip);
            write!(
                got,
                " {},{},{},{},{},{}",
                b.left, b.top, b.front, b.right, b.bottom, b.back
            )
            .expect("a string");
        }
        assert_eq!(got, want, "{head}");
        checked += 1;
    }
    assert_eq!(checked, 6 * 4 * 3 * 4);
}

#[test]
fn subresources_decompose_as_cpp() {
    let mut checked = 0;
    for line in DECOMPOSE.lines() {
        let (head, want) = line.split_once(':').expect("a result");
        let words: Vec<&str> = head[2..].split(' ').collect();
        let desc = desc(
            words[0].parse().expect("a dimension"),
            words[1].parse().expect("a width"),
            words[2].parse().expect("a height"),
            words[3].parse().expect("a depth"),
            words[4].parse().expect("a mip count"),
        );
        let mut got = String::new();
        for subresource in [0, 1, 5, 17, 1000, 0xffff_ffff] {
            let d = decompose_subresource(&desc, subresource);
            write!(
                got,
                " {}/{}/{}/{}",
                d.mip_levels, d.mip_slice, d.array_slice, d.plane_slice
            )
            .expect("a string");
        }
        // The C++ leaves the slices of a buffer (or any dimension that is not a texture) as the
        // caller had them; they are 0 here.
        assert_eq!(got, want.replace(LEFT_ALONE, "0"), "{head}");
        checked += 1;
    }
    assert_eq!(checked, 6 * 3 * 2 * 3 * 2);
}

#[test]
fn boxes_in_bounds_as_cpp() {
    let mut checked = 0;
    for line in BOX.lines() {
        let (head, want) = line.split_once(':').expect("a result");
        let b: Vec<u32> = head[2..]
            .split(',')
            .map(|word| word.parse().expect("a number"))
            .collect();
        let bounds = D3d12Box {
            left: b[0],
            top: b[1],
            front: b[2],
            right: b[3],
            bottom: b[4],
            back: b[5],
        };
        let mut got = String::new();
        for index in 0..729_u32 {
            let c = |digit: u32| (index / 3_u32.pow(digit)) % 3;
            let inner = D3d12Box {
                left: c(0),
                top: c(1),
                front: c(2),
                right: c(3),
                bottom: c(4),
                back: c(5),
            };
            got.push(if is_box_in_bounds(&inner, &bounds) {
                '1'
            } else {
                '0'
            });
        }
        assert_eq!(got, want, "{head}");
        checked += 1;
    }
    assert_eq!(checked, 3);
}

#[test]
fn plain_desc_keeps_every_field() {
    let wide = ResourceDesc1 {
        dimension: D3D12_RESOURCE_DIMENSION_TEXTURE3D,
        alignment: 0x1_0000,
        width: 0x1_0000_0001,
        height: 7,
        depth_or_array_size: 9,
        mip_levels: 3,
        format: 28,
        sample_desc: SampleDesc {
            count: 4,
            quality: 2,
        },
        layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        flags: D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
        sampler_feedback_mip_region: MipRegion {
            width: 5,
            height: 6,
            depth: 7,
        },
    };
    let plain = plain_desc(&wide);
    assert_eq!(
        (plain.dimension, plain.alignment, plain.width, plain.height),
        (wide.dimension, wide.alignment, wide.width, wide.height)
    );
    assert_eq!(
        (plain.depth_or_array_size, plain.mip_levels, plain.format),
        (9, 3, 28)
    );
    assert_eq!((plain.sample_desc.count, plain.sample_desc.quality), (4, 2));
    assert_eq!((plain.layout, plain.flags), (wide.layout, wide.flags));
}

#[test]
fn a_zero_mip_count_or_array_size_decomposes_to_zero_slices() {
    // The C++ divides by zero on a texture with no mip levels and no extent, and on an array of 0.
    let d = decompose_subresource(&desc(D3D12_RESOURCE_DIMENSION_TEXTURE2D, 0, 0, 1, 0), 5);
    assert_eq!(
        (d.mip_levels, d.mip_slice, d.array_slice, d.plane_slice),
        (0, 0, 0, 0)
    );
    let d = decompose_subresource(&desc(D3D12_RESOURCE_DIMENSION_TEXTURE2D, 8, 8, 0, 4), 5);
    assert_eq!((d.mip_levels, d.mip_slice, d.array_slice), (4, 1, 0));
}

#[test]
fn the_format_table_has_what_the_footprints_rely_on() {
    // A format with a Metal counterpart that is not planar has a texel size (the C++ aborts or
    // throws on 0), and no format has more than the two planes the footprint arrays hold.
    for format in (0..=192).chain([0x7fff_ffff, 0xffff_fffe, 0xffff_ffff]) {
        let (hr, desc) = query_dxgi_ref(format);
        if hr >= 0 {
            assert!(desc.planar_count <= 2, "format {format}");
            assert!(
                desc.planar_count > 1 || desc.bytes_per_texel != 0,
                "format {format}"
            );
        }
    }
}

#[test]
fn an_upload_heap_is_visible_and_not_private() {
    let upload = heap(
        D3D12_HEAP_TYPE_UPLOAD,
        D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        D3D12_MEMORY_POOL_L0,
    );
    assert!(is_cpu_visible_heap(&upload));
    assert!(!is_private_heap(&upload, true));
}
