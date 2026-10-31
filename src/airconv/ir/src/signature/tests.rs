use std::fmt::Write as _;

use super::*;
use crate::container::{Container, build};

const VSIA: &[u8] = include_bytes!("../../tests/data/vsia.vs.dxbc");
const V11_1: &[u8] = include_bytes!("../../tests/data/sig/v11_1.dxbc");
const OSG5: &[u8] = include_bytes!("../../tests/data/sig/osg5.dxbc");
const STREAM_ORDER: &[u8] = include_bytes!("../../tests/data/sig/stream_order.dxbc");

/// Each fixture and what libs/DXBCParser reads from it.
///
/// The text is what `sigdump`, a scratch program over `CDXBCParser`, `DXBCGet*Signature` and
/// both signature parsers, printed.
const FIXTURES: [(&[u8], &str); 9] = [
    (VSIA, include_str!("../../tests/data/sig/vsia.vs.dxbc.txt")),
    (
        include_bytes!("../../tests/data/render.ps.dxbc"),
        include_str!("../../tests/data/sig/render.ps.dxbc.txt"),
    ),
    (
        include_bytes!("../../tests/data/sync.dxbc"),
        include_str!("../../tests/data/sig/sync.dxbc.txt"),
    ),
    (
        include_bytes!("../../tests/data/cache.cs.dxil"),
        include_str!("../../tests/data/sig/cache.cs.dxil.txt"),
    ),
    (V11_1, include_str!("../../tests/data/sig/v11_1.txt")),
    (
        include_bytes!("../../tests/data/sig/v4.dxbc"),
        include_str!("../../tests/data/sig/v4.txt"),
    ),
    (OSG5, include_str!("../../tests/data/sig/osg5.txt")),
    (
        STREAM_ORDER,
        include_str!("../../tests/data/sig/stream_order.txt"),
    ),
    (
        include_bytes!("../../tests/data/sig/none.dxbc"),
        include_str!("../../tests/data/sig/none.txt"),
    ),
];

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn params(out: &mut String, signature: &Signature) {
    writeln!(out, "count {}", signature.parameters.len()).unwrap();
    for (index, p) in signature.parameters.iter().enumerate() {
        writeln!(
            out,
            "p {index} stream {} name \"{}\" index {} sv {} ct {} reg {} mask {} rw {} minprec {}",
            p.stream,
            String::from_utf8_lossy(&p.semantic_name),
            p.semantic_index,
            p.system_value,
            p.component_type,
            p.register,
            p.mask,
            p.rw_mask,
            p.min_precision
        )
        .unwrap();
    }
}

/// The text `sigdump` prints for the same bytes.
fn dump(bytes: &[u8]) -> String {
    let Ok(container) = Container::parse(bytes) else {
        return "container fail\n".to_string();
    };
    let mut out = format!("container ok\nparts {}\n", container.part_count());
    for (index, part) in container.parts().enumerate() {
        let name = String::from_utf8_lossy(&part.fourcc.to_le_bytes()).into_owned();
        writeln!(out, "part {index} {name} size {}", part.data.len()).unwrap();
    }
    for (name, read) in [
        ("input", input(&container)),
        ("output_v4", output_v4(&container)),
    ] {
        match read {
            Ok(signature) => {
                writeln!(out, "{name} ok").unwrap();
                params(&mut out, &signature);
            }
            Err(_) => writeln!(out, "{name} fail").unwrap(),
        }
    }
    match output_v5(&container) {
        Ok(output) => {
            writeln!(out, "output_v5 ok").unwrap();
            writeln!(
                out,
                "streams {} rasterized {}",
                output.num_streams(),
                output.rasterized_stream()
            )
            .unwrap();
            for stream in 0..4 {
                write!(out, "stream {stream} ").unwrap();
                params(&mut out, output.stream(stream).unwrap());
            }
        }
        Err(_) => writeln!(out, "output_v5 fail").unwrap(),
    }
    match patch_constant(&container) {
        Ok(signature) => {
            writeln!(out, "patch_constant ok").unwrap();
            params(&mut out, &signature);
        }
        Err(_) => writeln!(out, "patch_constant fail").unwrap(),
    }
    out
}

/// An ISGN-style blob with these (name, register) elements.
fn blob(elements: &[(&[u8], u32)]) -> Vec<u8> {
    let count = u32::try_from(elements.len()).unwrap();
    let mut out = [count, 8]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    let mut names = Vec::new();
    for &(name, register) in elements {
        let name_at = u32::try_from(8 + 24 * elements.len() + names.len()).unwrap();
        out.extend(
            [name_at, 0, 0, 3, register]
                .iter()
                .flat_map(|word| word.to_le_bytes()),
        );
        out.extend([15, 0, 0, 0]);
        names.extend_from_slice(name);
        names.push(0);
    }
    out.extend(names);
    out
}

/// An ISG1-style blob with these (name, register, stream) elements.
fn blob_11_1(elements: &[(&[u8], u32, u32)]) -> Vec<u8> {
    let count = u32::try_from(elements.len()).unwrap();
    let mut out = [count, 8]
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    let mut names = Vec::new();
    for &(name, register, stream) in elements {
        let name_at = u32::try_from(8 + 32 * elements.len() + names.len()).unwrap();
        out.extend(
            [stream, name_at, 0, 0, 3, register]
                .iter()
                .flat_map(|word| word.to_le_bytes()),
        );
        out.extend([15, 0, 0, 0, 0, 0, 0, 0]);
        names.extend_from_slice(name);
        names.push(0);
    }
    out.extend(names);
    out
}

fn container_of(part: u32, blob: &[u8]) -> Vec<u8> {
    build(&[(part, blob)]).unwrap()
}

fn input_of(bytes: &[u8]) -> Result<Signature, Error> {
    input(&Container::parse(bytes).unwrap())
}

#[test]
fn signatures_match_the_cxx_dump() {
    for (index, (bytes, expected)) in FIXTURES.into_iter().enumerate() {
        assert_eq!(dump(bytes), expected, "fixture {index}");
    }
}

#[test]
fn signature_of_vsia_vs() {
    let container = Container::parse(VSIA).unwrap();
    let signature = input(&container).unwrap();
    let names: Vec<_> = signature
        .parameters
        .iter()
        .map(|p| p.semantic_name.as_slice())
        .collect();
    assert_eq!(names, [b"A".as_slice(), b"B", b"C", b"D", b"SV_VertexID"]);
    let vertex_id = signature.parameters.last().unwrap();
    assert_eq!(
        (vertex_id.system_value, vertex_id.register, vertex_id.mask),
        (6, 4, 1)
    );
    let position = &output_v4(&container).unwrap().parameters[0];
    assert_eq!(position.semantic_name, b"SV_Position");
    assert_eq!(position.system_value, sb_name::UNDEFINED + 1);
}

#[test]
fn input_signature_prefers_isg1_over_isgn() {
    let signature = input_of(V11_1).unwrap();
    assert_eq!(signature.parameters.len(), 3);
    assert_eq!(signature.parameters[1].semantic_name, b"Texcoord");
    // Case is the blob's own; the minimum precision comes from the 11.1 record.
    assert_eq!(signature.parameters[1].min_precision, 1);
    assert_eq!(signature.parameters[1].semantic_index, 3);
}

#[test]
fn output_v4_puts_every_element_on_stream_0_and_ignores_osg5() {
    let osg1 = container_of(
        fourcc::OSG1,
        &blob_11_1(&[(b"A", 0, 0), (b"B", 1, 1), (b"C", 2, 7)]),
    );
    let signature = output_v4(&Container::parse(&osg1).unwrap()).unwrap();
    let streams: Vec<_> = signature.parameters.iter().map(|p| p.stream).collect();
    assert_eq!(streams, [0, 0, 0]);
    // Its registers must not decrease across streams, which the stream split does not ask.
    let decreasing = container_of(fourcc::OSG1, &blob_11_1(&[(b"A", 3, 0), (b"B", 1, 1)]));
    assert!(output_v4(&Container::parse(&decreasing).unwrap()).is_err());
    assert_eq!(
        output_v5(&Container::parse(&decreasing).unwrap())
            .unwrap()
            .num_streams(),
        2
    );
    // An OSG5 alone is not an output signature to it.
    assert!(output_v4(&Container::parse(OSG5).unwrap()).is_err());
}

#[test]
fn output_v5_splits_streams() {
    let output = output_v5(&Container::parse(OSG5).unwrap()).unwrap();
    assert_eq!(output.num_streams(), 4);
    let counts: Vec<_> = (0..4)
        .map(|s| output.stream(s).unwrap().parameters.len())
        .collect();
    assert_eq!(counts, [2, 1, 0, 1]);
    assert_eq!(output.stream(4), None);
    assert_eq!(output.rasterized(), output.stream(0).unwrap());
    // OSG1 comes before OSG5 and OSGN: this container has all three.
    let output = output_v5(&Container::parse(V11_1).unwrap()).unwrap();
    assert_eq!(output.num_streams(), 2);
    assert_eq!(output.stream(0).unwrap().parameters[1].min_precision, 3);
}

#[test]
fn output_v5_registers_must_not_decrease_within_a_stream() {
    let within = container_of(fourcc::OSG1, &blob_11_1(&[(b"A", 3, 0), (b"B", 1, 0)]));
    assert!(output_v5(&Container::parse(&within).unwrap()).is_err());
    let after_a_stream = container_of(
        fourcc::OSG1,
        &blob_11_1(&[(b"A", 3, 0), (b"B", 3, 0), (b"C", 1, 1), (b"D", 2, 1)]),
    );
    let output = output_v5(&Container::parse(&after_a_stream).unwrap()).unwrap();
    assert_eq!(output.num_streams(), 2);
}

#[test]
fn output_v5_prefers_osgn_over_osg5() {
    let bytes = build(&[
        (fourcc::OSG5, &blob(&[(b"NEW", 0)])),
        (fourcc::OSGN, &blob(&[(b"OLD", 0)])),
    ])
    .unwrap();
    let output = output_v5(&Container::parse(&bytes).unwrap()).unwrap();
    assert_eq!(
        output.stream(0).unwrap().parameters[0].semantic_name,
        b"OLD"
    );
}

#[test]
fn output_v5_osgn_is_one_stream() {
    let output = output_v5(&Container::parse(VSIA).unwrap()).unwrap();
    assert_eq!(output.num_streams(), 1);
    assert_eq!(output.stream(0).unwrap().parameters.len(), 5);
    // Even an empty one names a stream.
    let empty = container_of(fourcc::OSGN, &blob(&[]));
    assert_eq!(
        output_v5(&Container::parse(&empty).unwrap())
            .unwrap()
            .num_streams(),
        1
    );
    // And an empty OSG1 names none.
    let empty = container_of(fourcc::OSG1, &blob(&[]));
    assert_eq!(
        output_v5(&Container::parse(&empty).unwrap())
            .unwrap()
            .num_streams(),
        0
    );
}

#[test]
fn output_v5_elements_out_of_stream_order_belong_to_no_stream() {
    // The first element names stream 1, so stream 0 takes nothing and the two that name 0
    // after it are taken by no stream; they are not looked at, in their registers either.
    let output = output_v5(&Container::parse(STREAM_ORDER).unwrap()).unwrap();
    assert_eq!(output.num_streams(), 2);
    let counts: Vec<_> = (0..4)
        .map(|s| output.stream(s).unwrap().parameters.len())
        .collect();
    assert_eq!(counts, [0, 1, 0, 0]);
}

#[test]
fn patch_constant_reads_psg1_before_pcsg() {
    let signature = patch_constant(&Container::parse(V11_1).unwrap()).unwrap();
    assert_eq!(signature.parameters.len(), 36);
    // PSG1 and PCSG hold the same elements: the 11.1 record is told by its precision field.
    assert_eq!(signature.parameters[0].min_precision, 0);
}

#[test]
fn system_values_convert_as_the_cxx() {
    let signature = patch_constant(&Container::parse(V11_1).unwrap()).unwrap();
    let converted: Vec<_> = signature
        .parameters
        .iter()
        .map(|p| (p.semantic_index, p.system_value))
        .collect();
    // (11, 0..=3) are the quad edge factors, a fifth index passes through; render target, depth
    // and coverage have no system value; depth-greater-equal (67) and -less-equal (68) have no
    // case and pass through, so do values the C++ never heard of.
    assert_eq!(converted[..4], [(0, 11), (1, 12), (2, 13), (3, 14)]);
    assert_eq!(converted[4], (4, 11));
    assert_eq!(converted[18], (0, 0));
    assert_eq!(
        converted[29..34],
        [(0, 0), (0, 0), (0, 0), (0, 67), (0, 68)]
    );
    assert_eq!(converted[35], (0, 99));
}

#[test]
fn signature_part_that_is_missing_or_empty_is_refused() {
    let bytes = build(&[(fourcc::SHDR, b"x")]).unwrap();
    let container = Container::parse(&bytes).unwrap();
    for read in [
        input(&container),
        output_v4(&container),
        patch_constant(&container),
    ] {
        assert!(read.unwrap_err().what.ends_with("signature not found"));
    }
    assert!(output_v5(&container).is_err());
    // A part with no bytes is refused, a part of fewer than 8 as well.
    for len in 0..8 {
        let bytes = container_of(fourcc::ISGN, &vec![0; len]);
        assert!(input_of(&bytes).is_err(), "{len} bytes");
    }
    // 8 bytes with no element is an empty signature.
    assert_eq!(
        input_of(&container_of(fourcc::ISGN, &blob(&[])))
            .unwrap()
            .parameters
            .len(),
        0
    );
}

#[test]
fn signature_reads_that_run_past_the_blob_are_errors() {
    let good = blob(&[(b"A", 0), (b"B", 1)]);
    assert_eq!(
        input_of(&container_of(fourcc::ISGN, &good))
            .unwrap()
            .parameters
            .len(),
        2
    );
    let run = |mutate: &dyn Fn(&mut Vec<u8>)| {
        let mut blob = good.clone();
        mutate(&mut blob);
        input_of(&container_of(fourcc::ISGN, &blob)).unwrap_err()
    };
    // More elements than the blob holds, wrapping the C++'s 32-bit size or not.
    for count in [3, 1 << 27, (1 << 27) + 1, u32::MAX] {
        run(&|blob| put_u32(blob, 0, count));
    }
    // The records start past the blob, or end past it.
    for at in [u32::MAX, 13, 56, 60] {
        run(&|blob| put_u32(blob, 4, at));
    }
    // A name that starts past the blob or at its end, or that has no NUL.
    run(&|blob| put_u32(blob, 8, u32::MAX));
    run(&|blob| {
        let len = u32::try_from(blob.len()).unwrap();
        put_u32(blob, 8, len);
    });
    run(&|blob| {
        let len = blob.len();
        blob[len - 1] = b'x';
    });
}

#[test]
fn signature_registers_that_decrease_are_refused() {
    let ordered = blob(&[(b"A", 1), (b"B", 1), (b"C", 5)]);
    assert!(input_of(&container_of(fourcc::ISGN, &ordered)).is_ok());
    let decreasing = blob(&[(b"A", 2), (b"B", 1)]);
    let error = input_of(&container_of(fourcc::ISGN, &decreasing)).unwrap_err();
    assert_eq!(
        error.what,
        "Invalid DXBC bytecode: signature registers decrease"
    );
}

#[test]
fn signature_names_keep_their_bytes() {
    let signature = input_of(&container_of(
        fourcc::ISGN,
        &blob(&[(b"\xff\xfe", 0), (b"", 1)]),
    ))
    .unwrap();
    assert_eq!(signature.parameters[0].semantic_name, b"\xff\xfe");
    assert_eq!(signature.parameters[1].semantic_name, b"");
}
