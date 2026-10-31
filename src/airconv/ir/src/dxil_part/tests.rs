use std::{env, fs, path::PathBuf};

use super::*;
use crate::container::build;

const CACHE_CS: &[u8] = include_bytes!("../../tests/data/cache.cs.dxil");
const SYNC: &[u8] = include_bytes!("../../tests/data/sync.dxbc");

/// A DXIL part of the given header words and bitcode bytes.
///
/// The words are `ProgramVersion`, `SizeInUint32`, then the bitcode header.
fn part(header: [u32; 6], bitcode: &[u8]) -> Vec<u8> {
    let mut part: Vec<u8> = header.iter().flat_map(|word| word.to_le_bytes()).collect();
    part.extend_from_slice(bitcode);
    part
}

fn find_in(part: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let container = build(&[(fourcc::DXIL, part)]).unwrap();
    let found = find(&Container::parse(&container).unwrap());
    found.map(|found| found.map(|found| found.bitcode.to_vec()))
}

fn dxil(offset: u32, size: u32) -> [u32; 6] {
    [0x60, 0, u32::from_le_bytes(*b"DXIL"), 0x102, offset, size]
}

#[test]
fn dxil_part_of_the_fixture_is_its_bitcode() {
    let container = Container::parse(CACHE_CS).unwrap();
    let part = find(&container).unwrap().unwrap();
    assert_eq!(part.bitcode.get(..4), Some(b"BC\xc0\xde".as_slice()));
}

#[test]
fn dxil_part_is_none_for_dxbc() {
    assert_eq!(find(&Container::parse(SYNC).unwrap()), Ok(None));
}

#[test]
fn dxil_part_accepts_what_findxil_accepts() {
    // The bitcode starts 8 bytes into the part, plus its header's offset.
    let bytes = part(dxil(16, 5), b"BC\xc0\xde");
    let found = find_in(&bytes);
    assert_eq!(
        found,
        Err(Error::fail("DXIL: bitcode lies outside the DXIL part"))
    );
    let bytes = part(dxil(16, 4), b"BC\xc0\xde");
    assert_eq!(find_in(&bytes).unwrap().unwrap(), b"BC\xc0\xde");
    // A bitcode of 4 bytes is the least; 3 is refused even inside the part.
    let bytes = part(dxil(16, 3), b"BC\xc0\xde");
    assert_eq!(
        find_in(&bytes).unwrap_err().what,
        "DXIL: bitcode lies outside the DXIL part"
    );
    // An offset that skips some of the part.
    let bytes = part(dxil(18, 4), b"..BC\xc0\xde");
    assert_eq!(find_in(&bytes).unwrap().unwrap(), b"BC\xc0\xde");
}

#[test]
fn dxil_part_texts() {
    // The three texts, as `FindDXIL` words them.
    let short = vec![0; 23];
    assert_eq!(
        find_in(&short).unwrap_err().what,
        "DXIL: program header truncated"
    );
    let mut magic = part(dxil(16, 4), b"BC\xc0\xde");
    magic[8] = b'X';
    assert_eq!(
        find_in(&magic).unwrap_err().what,
        "DXIL: bad program header"
    );
    let past = part(dxil(16, 5), b"BC\xc0\xde");
    assert_eq!(
        find_in(&past).unwrap_err().what,
        "DXIL: bitcode lies outside the DXIL part"
    );
}

#[test]
fn dxil_part_offsets_that_wrap_are_outside() {
    // 8 + offset and offset + size overflow 32 bits: the C++ added them in 64.
    for (offset, size) in [(u32::MAX, 4), (16, u32::MAX), (u32::MAX - 7, u32::MAX)] {
        let bytes = part(dxil(offset, size), b"BC\xc0\xde");
        assert_eq!(
            find_in(&bytes).unwrap_err().what,
            "DXIL: bitcode lies outside the DXIL part",
            "{offset:#x} {size:#x}"
        );
    }
}

#[test]
fn c7_dxil_rows_get_the_cxx_reason() {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: the C7 rows are not compared");
        return;
    };
    let base = PathBuf::from(base);
    let corpus = base.join("../corpus/C7");
    let baseline = fs::read_to_string(base.join("C7.txt")).unwrap();
    let (mut refused, mut reader) = (0, 0);
    for line in baseline.lines().filter(|line| line.starts_with("fail ")) {
        let (file, reason) = line["fail ".len()..].split_once(" - refl=- ").unwrap();
        let bytes = fs::read(corpus.join(file)).unwrap();
        let Ok(container) = Container::parse(&bytes) else {
            continue;
        };
        if reason.starts_with("DXIL: can't read the bitcode") {
            // The part is well formed; the bitcode reader (X1) refuses its bitcode.
            assert!(matches!(find(&container), Ok(Some(_))), "{file}");
            reader += 1;
        } else {
            assert_eq!(find(&container).unwrap_err().what, reason, "{file}");
            refused += 1;
        }
    }
    assert_eq!((refused, reader), (3, 12));
}
