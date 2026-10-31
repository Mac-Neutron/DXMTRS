use core::mem::offset_of;
use std::{env, fs, path::PathBuf};

use super::*;

const VSIA: &[u8] = include_bytes!("../../tests/data/vsia.vs.dxbc");
const SYNC: &[u8] = include_bytes!("../../tests/data/sync.dxbc");
const CACHE_CS: &[u8] = include_bytes!("../../tests/data/cache.cs.dxil");

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn parts_of(bytes: &[u8]) -> Vec<(String, usize)> {
    let container = Container::parse(bytes).unwrap();
    container
        .parts()
        .map(|part| {
            let name = part.fourcc.to_le_bytes().map(char::from).iter().collect();
            (name, part.data.len())
        })
        .collect()
}

#[test]
fn raw_layout_sizes() {
    assert_eq!(size_of::<raw::Header>(), 32);
    assert_eq!(align_of::<raw::Header>(), 4);
    assert_eq!(offset_of!(raw::Header, hash), 4);
    assert_eq!(offset_of!(raw::Header, version_major), 20);
    assert_eq!(
        offset_of!(raw::Header, container_size),
        raw::Header::SIZE_OFFSET
    );
    assert_eq!(offset_of!(raw::Header, blob_count), 28);
    assert_eq!(size_of::<raw::BlobHeader>(), 8);
    assert_eq!(offset_of!(raw::BlobHeader, size), 4);
}

#[test]
fn the_fixtures_have_the_parts_the_cxx_lists() {
    // `GetBlobFourCC` and `GetBlobSize` over each fixture, read once from libs/DXBCParser.
    let sizes = |parts: &[(&str, usize)]| {
        parts
            .iter()
            .map(|&(name, size)| (name.to_string(), size))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        parts_of(VSIA),
        sizes(&[
            ("ISGN", 156),
            ("OSGN", 188),
            ("RDEF", 92),
            ("SHDR", 572),
            ("STAT", 148)
        ])
    );
    assert_eq!(
        parts_of(SYNC),
        sizes(&[
            ("ISGN", 8),
            ("OSGN", 8),
            ("RDEF", 128),
            ("SHDR", 544),
            ("STAT", 148)
        ])
    );
    let cache = Container::parse(CACHE_CS).unwrap();
    assert!(cache.find(fourcc::DXIL).is_some());
    assert!(cache.find(fourcc::SHDR).is_none());
}

#[test]
fn find_answers_the_first_part_with_the_code() {
    let bytes = build(&[
        (fourcc::SHDR, b"one"),
        (fourcc::RTS0, b"x"),
        (fourcc::SHDR, b"two"),
    ])
    .unwrap();
    let container = Container::parse(&bytes).unwrap();
    assert_eq!(container.find(fourcc::SHDR), Some(b"one".as_slice()));
    assert_eq!(container.part(2).unwrap().data, b"two");
    assert_eq!(container.part(3), None);
}

#[test]
fn an_empty_container_and_trailing_bytes_are_accepted() {
    let empty = build(&[]).unwrap();
    assert_eq!(Container::parse(&empty).unwrap().part_count(), 0);
    // The size field counts them; the C++ never looked past the last part.
    let mut padded = build(&[(fourcc::SHDR, b"abcd")]).unwrap();
    padded.extend_from_slice(&[0; 5]);
    let size = u32::try_from(padded.len()).unwrap();
    put_u32(&mut padded, raw::Header::SIZE_OFFSET, size);
    assert_eq!(
        Container::parse(&padded).unwrap().find(fourcc::SHDR),
        Some(b"abcd".as_slice())
    );
}

#[test]
fn declared_size_reads_the_header() {
    assert_eq!(Container::declared_size(VSIA), Some(VSIA.len()));
    assert_eq!(Container::declared_size(&VSIA[..27]), None);
}

#[test]
fn container_rejects_part_past_end() {
    let good = build(&[(fourcc::SHDR, b"abcdefgh")]).unwrap();
    let part_size_at = raw::Header::SIZE + 4 + 4;
    // One byte past the end: the C++ let a last part overrun the container by exactly one.
    for size in [9, 12, 0x1000, 0xffff_fff0, 0xffff_fff8, 0xffff_ffff] {
        let mut bytes = good.clone();
        put_u32(&mut bytes, part_size_at, size);
        assert_eq!(
            Container::parse(&bytes).unwrap_err(),
            Error::INVALID,
            "size {size:#x}"
        );
    }
    // The part's header past the end.
    let mut bytes = good.clone();
    put_u32(
        &mut bytes,
        raw::Header::SIZE,
        u32::try_from(good.len() - 7).unwrap(),
    );
    assert_eq!(Container::parse(&bytes).unwrap_err(), Error::INVALID);
}

#[test]
fn container_rejects_huge_part_count() {
    let good = build(&[(fourcc::SHDR, b"abcd")]).unwrap();
    // 2^30 parts is 2^32 index bytes: the C++ cut that sum to 32 bits and went on.
    for count in [2, 1 << 24, 1 << 30, (1 << 30) + 1, u32::MAX] {
        let mut bytes = good.clone();
        put_u32(&mut bytes, 28, count);
        assert_eq!(
            Container::parse(&bytes).unwrap_err(),
            Error::INVALID,
            "count {count}"
        );
    }
}

#[test]
fn container_rejects_what_read_dxbc_refuses() {
    // The rows of the translator's malformed-container corpus (C7) for `ReadDXBC`, made on the
    // fixture: each of these answered `Invalid DXBC bytecode`.
    let good = build(&[(fourcc::DXIL, b"abcdefgh"), (fourcc::SHDR, b"ijkl")]).unwrap();
    // The index ends at 40; the parts are at 40 and 56; the container is 68 bytes.
    assert_eq!(good.len(), 68);
    let mutate = |at: usize, value: u32| {
        let mut bytes = good.clone();
        put_u32(&mut bytes, at, value);
        bytes
    };
    let rows = [
        ("shorter than the header", good[..31].to_vec()),
        ("empty", Vec::new()),
        ("size field + 4", mutate(24, 72)),
        ("size field - 1", mutate(24, 67)),
        ("magic", mutate(0, 0x4142_4344)),
        ("version major 2", mutate(20, 0x0000_0002)),
        ("version minor 1", mutate(20, 0x0001_0001)),
        ("first part not after the index", mutate(32, 44)),
        ("a gap between parts", mutate(36, 60)),
        ("parts swapped", {
            let mut bytes = mutate(32, 56);
            put_u32(&mut bytes, 36, 40);
            bytes
        }),
        ("part offset at the end", mutate(36, 68)),
        ("part offset 0", mutate(32, 0)),
        ("part offset huge", mutate(32, u32::MAX)),
    ];
    for (what, bytes) in rows {
        assert_eq!(
            Container::parse(&bytes).unwrap_err(),
            Error::INVALID,
            "{what}"
        );
    }
    assert!(Container::parse(&good).is_ok());
}

#[test]
fn reads_never_go_past_the_slice() {
    // Every proper prefix, with its size field made to match, is refused or complete: no panic.
    for end in 0..VSIA.len() {
        let mut bytes = VSIA[..end].to_vec();
        if end >= 28 {
            put_u32(&mut bytes, 24, u32::try_from(end).unwrap());
        }
        assert_eq!(
            Container::parse(&bytes).unwrap_err(),
            Error::INVALID,
            "prefix {end}"
        );
    }
}

#[test]
fn c7_container_rows_get_the_cxx_reason() {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: the C7 rows are not compared");
        return;
    };
    let base = PathBuf::from(base);
    let corpus = base.join("../corpus/C7");
    let baseline = fs::read_to_string(base.join("C7.txt")).unwrap();
    let mut seen = 0;
    for line in baseline.lines().filter(|line| line.starts_with("fail ")) {
        let (file, reason) = line["fail ".len()..].split_once(" - refl=- ").unwrap();
        let bytes = fs::read(corpus.join(file)).unwrap();
        // Only the rows this module and `dxil_part` own: the reader's are X1's.
        if reason.starts_with("Invalid DXBC bytecode") {
            assert_eq!(Container::parse(&bytes).unwrap_err().what, reason, "{file}");
            seen += 1;
        }
    }
    assert_eq!(seen, 8);
}
