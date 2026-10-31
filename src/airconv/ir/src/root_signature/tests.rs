use core::mem::offset_of;
use std::{env, fmt::Write as _, fs, path::PathBuf};

use super::*;

const SYNTHETIC: &[u8] = include_bytes!("../../tests/data/rs/v1_0_synthetic.dxbc");
const SAMPLERS: &[u8] = include_bytes!("../../tests/data/rs/v1_1_static_samplers.dxbc");

/// Each fixture and what `RootSignatureDeserializer` makes of it.
///
/// The text is what `rsdump`, a scratch program over `dxbc_root_signature.hpp`, printed.
const FIXTURES: [(&[u8], &str); 5] = [
    (
        SYNTHETIC,
        include_str!("../../tests/data/rs/v1_0_synthetic.txt"),
    ),
    (
        include_bytes!("../../tests/data/rs/v1_0_descriptors_sampler.dxbc"),
        include_str!("../../tests/data/rs/v1_0_descriptors_sampler.txt"),
    ),
    (
        include_bytes!("../../tests/data/rs/v1_1_constants_descriptors.dxbc"),
        include_str!("../../tests/data/rs/v1_1_constants_descriptors.txt"),
    ),
    (
        include_bytes!("../../tests/data/rs/v1_1_ranges.dxbc"),
        include_str!("../../tests/data/rs/v1_1_ranges.txt"),
    ),
    (
        SAMPLERS,
        include_str!("../../tests/data/rs/v1_1_static_samplers.txt"),
    ),
];

/// The text `rsdump` prints.
fn dump(rs: &RootSignature) -> String {
    let mut out = format!("raw_version {}\n", rs.version);
    for (name, desc, v1_1) in [("v1_0", &rs.v1_0, false), ("v1_1", &rs.v1_1, true)] {
        writeln!(
            out,
            "{name}\nflags {:#x}\nparameters {}",
            desc.flags,
            desc.parameters.len()
        )
        .unwrap();
        for (index, p) in desc.parameters.iter().enumerate() {
            writeln!(
                out,
                "param {index} type {} visibility {}",
                p.parameter_type(),
                p.visibility
            )
            .unwrap();
            match &p.payload {
                Payload::Table(ranges) => {
                    writeln!(out, "table ranges {}", ranges.len()).unwrap();
                    for (j, r) in ranges.iter().enumerate() {
                        write!(
                            out,
                            "range {j} type {} num {} base {} space {} offset {}",
                            r.range_type, r.num_descriptors, r.base_register, r.space, r.offset
                        )
                        .unwrap();
                        if v1_1 {
                            write!(out, " flags {:#x}", r.flags).unwrap();
                        }
                        out.push('\n');
                    }
                }
                Payload::Constants(c) => {
                    writeln!(
                        out,
                        "constants register {} space {} values {}",
                        c.register, c.space, c.values
                    )
                    .unwrap();
                }
                Payload::Cbv(d) | Payload::Srv(d) | Payload::Uav(d) => {
                    write!(out, "descriptor register {} space {}", d.register, d.space).unwrap();
                    if v1_1 {
                        write!(out, " flags {:#x}", d.flags).unwrap();
                    }
                    out.push('\n');
                }
                Payload::Unknown(_) => unreachable!("the C++ dumps garbage for these"),
            }
        }
        writeln!(out, "static_samplers {}", rs.static_samplers.len()).unwrap();
        for (k, s) in rs.static_samplers.iter().enumerate() {
            writeln!(
                out,
                "sampler {k} filter {} u {} v {} w {} lod_bias {:#010x} anisotropy {} comparison {} border {} \
                 min_lod {:#010x} max_lod {:#010x} register {} space {} visibility {}",
                s.filter,
                s.address_u,
                s.address_v,
                s.address_w,
                s.mip_lod_bias.to_bits(),
                s.max_anisotropy,
                s.comparison,
                s.border_color,
                s.min_lod.to_bits(),
                s.max_lod.to_bits(),
                s.register,
                s.space,
                s.visibility
            )
            .unwrap();
        }
    }
    out
}

fn of_container(bytes: &[u8]) -> Result<RootSignature, Error> {
    let container = Container::parse(bytes)?;
    RootSignature::deserialize(from_container(&container)?)
}

/// A blob of these words.
fn blob(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|word| word.to_le_bytes()).collect()
}

/// A 1.1 blob with one root CBV at 24 + 12 + 12 = 36 bytes: the smallest valid one with data.
fn one_cbv() -> Vec<u32> {
    vec![2, 1, 24, 0, 0, 0x1, 2, 0, 36, 5, 6, 0x2]
}

fn error_of(words: &[u32]) -> Error {
    RootSignature::deserialize(&blob(words)).expect_err(&format!("{words:?} is refused"))
}

#[test]
fn raw_layout_sizes() {
    assert_eq!(size_of::<raw::RootSignatureDesc>(), 24);
    assert_eq!(offset_of!(raw::RootSignatureDesc, flags), 20);
    assert_eq!(size_of::<raw::RootParameter>(), 12);
    assert_eq!(offset_of!(raw::RootParameter, payload_offset), 8);
    assert_eq!(size_of::<raw::RootDescriptorTable>(), 8);
    assert_eq!(
        offset_of!(raw::RootDescriptorTable, descriptor_ranges_offset),
        4
    );
}

#[test]
fn root_signature_matches_the_cxx_dump() {
    for (index, (bytes, expected)) in FIXTURES.into_iter().enumerate() {
        assert_eq!(
            dump(&of_container(bytes).unwrap()),
            expected,
            "fixture {index}"
        );
    }
}

#[test]
fn root_signature_matches_the_cxx_dump_on_every_real_blob() {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: no real root signature is compared");
        return;
    };
    // list.txt: `<blob path> <dump path>`, both relative to the folder that holds the baseline
    // (a blob's path may hold spaces, so a line splits at its last one).
    let base = PathBuf::from(base);
    let root = base.join("..");
    let list = fs::read_to_string(base.join("rsdump/list.txt")).unwrap();
    let mut count = 0;
    for line in list.lines() {
        let (blob, dump_path) = line.rsplit_once(' ').unwrap();
        let rs = of_container(&fs::read(root.join(blob)).unwrap()).unwrap();
        let expected = fs::read_to_string(root.join(dump_path)).unwrap();
        assert_eq!(dump(&rs), expected, "{dump_path}");
        count += 1;
    }
    assert!(count > 0, "list.txt is empty");
}

#[test]
fn deserialize_converts_with_setflags_defaults() {
    // Version 1.0 -> 1.1: root descriptors data-volatile, ranges descriptors- and data-volatile,
    // a sampler range descriptors-volatile. The synthetic blob holds two 1.0 SRV/UAV ranges.
    let rs = of_container(SYNTHETIC).unwrap();
    assert_eq!(rs.version, VERSION_1_0);
    let Payload::Table(ranges) = &rs.v1_1.parameters[0].payload else {
        panic!("a table");
    };
    assert_eq!(
        ranges[0].flags,
        RANGE_FLAG_DESCRIPTORS_VOLATILE | RANGE_FLAG_DATA_VOLATILE
    );
    let Payload::Table(ranges) = &rs.v1_0.parameters[0].payload else {
        panic!("a table");
    };
    assert_eq!(ranges[0].flags, 0);
    // Version 1.1 -> 1.0: the flags go.
    let rs = RootSignature::deserialize(&blob(&one_cbv())).unwrap();
    let (Payload::Cbv(v1_0), Payload::Cbv(v1_1)) = (
        &rs.v1_0.parameters[0].payload,
        &rs.v1_1.parameters[0].payload,
    ) else {
        panic!("a CBV");
    };
    assert_eq!((v1_0.register, v1_0.space, v1_0.flags), (5, 6, 0));
    assert_eq!((v1_1.register, v1_1.space, v1_1.flags), (5, 6, 2));
    // 1.0 root descriptors get data-volatile.
    let rs = RootSignature::deserialize(&blob(&[1, 1, 24, 0, 0, 0, 2, 0, 36, 5, 6])).unwrap();
    let Payload::Cbv(v1_1) = &rs.v1_1.parameters[0].payload else {
        panic!("a CBV");
    };
    assert_eq!(v1_1.flags, DESCRIPTOR_FLAG_DATA_VOLATILE);
}

#[test]
fn static_samplers_keep_the_bits_of_their_floats() {
    let rs = of_container(SAMPLERS).unwrap();
    assert_eq!(rs.static_samplers[1].mip_lod_bias.to_bits(), 0x7fc0_0001);
    assert_eq!(rs.static_samplers[1].min_lod.to_bits(), 0x8000_0000);
    assert_eq!(rs.static_samplers[0].max_lod, f32::MAX);
}

#[test]
fn deserialize_refuses_versions_but_1_0_and_1_1() {
    for version in [0, 3, 0x100, u32::MAX] {
        let error = error_of(&[version, 0, 0, 0, 0, 0]);
        assert_eq!(error.code, Code::InvalidArg, "{version:#x}");
    }
    // The C++ looked at the version first: it needs no more than that to refuse.
    assert_eq!(error_of(&[7]).code, Code::InvalidArg);
    assert_eq!(
        RootSignature::deserialize(&[1, 0, 0]).unwrap_err().code,
        Code::Fail
    );
}

#[test]
fn deserialize_refuses_a_header_that_runs_past_the_part() {
    for len in 4..24 {
        let bytes = [&blob(&[2])[..], &vec![0; len - 4]].concat();
        assert_eq!(
            RootSignature::deserialize(&bytes).unwrap_err().code,
            Code::Fail,
            "{len}"
        );
    }
    assert!(RootSignature::deserialize(&blob(&[2, 0, 0, 0, 0, 0])).is_ok());
}

#[test]
fn a_table_without_ranges_is_the_cxxs_out_of_memory() {
    // One table of no ranges: `AllocateBuffer(0)` answered null and the conversion called that
    // `E_OUTOFMEMORY`, in either direction.
    for version in [1, 2] {
        let error = error_of(&[version, 1, 24, 0, 0, 0, 0, 0, 36, 0, 0]);
        assert_eq!(error.code, Code::OutOfMemory, "version {version}");
        // Wherever the empty array points.
        let error = error_of(&[version, 1, 24, 0, 0, 0, 0, 0, 36, 0, u32::MAX]);
        assert_eq!(error.code, Code::OutOfMemory, "version {version}");
    }
}

#[test]
fn offsets_and_counts_past_the_part_are_errors() {
    let mut words = one_cbv();
    assert!(RootSignature::deserialize(&blob(&words)).is_ok());
    // Parameters: a count the part cannot hold, an array that starts or ends past it.
    for (at, value) in [
        (1, 3),
        (1, u32::MAX),
        (1, 1 << 30),
        (2, 25),
        (2, u32::MAX),
        (2, 0xffff_fff8),
    ] {
        words = one_cbv();
        words[at] = value;
        assert_eq!(error_of(&words).code, Code::Fail, "word {at} = {value:#x}");
    }
    // The payload: past the part, or ending past it.
    for payload in [41, 44, u32::MAX] {
        words = one_cbv();
        words[8] = payload;
        assert_eq!(error_of(&words).code, Code::Fail, "payload {payload}");
    }
    // Static samplers: one that does not fit, an offset past the part, a count that wraps.
    for (count, offset) in [(1, 36), (1, u32::MAX), (1 << 28, 36), (u32::MAX, 0)] {
        words = one_cbv();
        words[3] = count;
        words[4] = offset;
        assert_eq!(
            error_of(&words).code,
            Code::Fail,
            "samplers {count} at {offset}"
        );
    }
    // A table whose ranges are not all there, whose table is not there, with an absurd count.
    let table = |ranges_at: u32, count: u32, payload: u32| {
        vec![
            2, 1, 24, 0, 0, 0, 0, 0, payload, count, ranges_at, 0, 0, 0, 0, 0, 0, 0,
        ]
    };
    assert!(RootSignature::deserialize(&blob(&table(44, 1, 36))).is_ok());
    for words in [
        table(44, 2, 36),
        table(49, 1, 36),
        table(44, u32::MAX, 36),
        table(44, 1, 68),
        table(44, 1, u32::MAX),
    ] {
        assert_eq!(error_of(&words).code, Code::Fail, "{words:?}");
    }
}

#[test]
fn arrays_with_no_records_are_never_read() {
    // No parameters, no samplers: their offsets point nowhere and nothing is read.
    let rs = RootSignature::deserialize(&blob(&[2, 0, u32::MAX, 0, u32::MAX, 0xc00])).unwrap();
    assert!(rs.v1_1.parameters.is_empty() && rs.static_samplers.is_empty());
    assert_eq!((rs.v1_0.flags, rs.v1_1.flags), (0xc00, 0xc00));
}

#[test]
fn an_unknown_parameter_type_is_kept_without_a_payload() {
    // The C++ left the union it filled for the other types as malloc made it.
    let rs = RootSignature::deserialize(&blob(&[2, 1, 24, 0, 0, 0, 9, 3, 0])).unwrap();
    for desc in [&rs.v1_0, &rs.v1_1] {
        assert!(matches!(desc.parameters[0].payload, Payload::Unknown(9)));
        assert_eq!(
            (
                desc.parameters[0].parameter_type(),
                desc.parameters[0].visibility
            ),
            (9, 3)
        );
    }
}

#[test]
fn from_container_wants_a_non_empty_rts0() {
    let with = crate::container::build(&[(fourcc::SHDR, b"x"), (fourcc::RTS0, b"abcd")]).unwrap();
    assert_eq!(
        from_container(&Container::parse(&with).unwrap()).unwrap(),
        b"abcd"
    );
    for parts in [
        &[(fourcc::SHDR, &b"x"[..])][..],
        &[(fourcc::RTS0, &b""[..])][..],
    ] {
        let bytes = crate::container::build(parts).unwrap();
        assert!(from_container(&Container::parse(&bytes).unwrap()).is_err());
    }
}
