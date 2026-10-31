use std::{
    env, fs,
    path::{Path, PathBuf},
};

use super::*;

// The SHA-256 of `BC\xc0\xde`, from an independent implementation.
const BC_HASH: &str = "f41aab480756711edcd2fb00add18f7b553cff9f09cbe93a23e421f0386139e0";
const BITCODE: &[u8] = b"BC\xc0\xde";

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn function(name: &[u8], kind: FunctionKind) -> FunctionEntry {
    FunctionEntry {
        name: name.to_vec(),
        kind,
        vertex_attributes: Vec::new(),
        patch: None,
    }
}

fn attribute(attribute: u8, name: &[u8], data_type: u8) -> VertexAttribute {
    VertexAttribute {
        attribute,
        name: name.to_vec(),
        data_type,
    }
}

fn u64_at(file: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(file[at..at + 8].try_into().unwrap())
}

#[test]
fn pack_one_fragment_function_matches_the_cxx_layout() {
    // Laid out by hand from the header the C++ writer fills: 88 header bytes, the function
    // count and the list size, the 118-byte entry (so 0x7a with its end tag), the public
    // metadata at 100 + 118, the private metadata 8 bytes on, the bitcode 8 bytes after that.
    let hash = unhex(BC_HASH);
    let expected: Vec<u8> = [
        b"MTLB".as_slice(),
        &[0x01, 0x80], // macOS
        &[2, 0],       // metallib 2.7
        &[7, 0],
        &[0x00], // executable
        &[0x81], // macOS, 14.4
        &[14, 0],
        &[4, 0],
        &0xeeu64.to_le_bytes(), // file size
        &0x58u64.to_le_bytes(), // function list
        &0x7au64.to_le_bytes(),
        &0xdau64.to_le_bytes(), // public metadata
        &8u64.to_le_bytes(),
        &0xe2u64.to_le_bytes(), // private metadata
        &8u64.to_le_bytes(),
        &0xeau64.to_le_bytes(), // bitcode
        &4u64.to_le_bytes(),
        &1u32.to_le_bytes(), // functions
        &0x7au32.to_le_bytes(),
        b"NAME",
        &[5, 0],
        b"main\0",
        b"TYPE",
        &[1, 0],
        &[1], // fragment
        b"HASH",
        &[0x20, 0],
        &hash,
        b"MDSZ",
        &[8, 0],
        &4u64.to_le_bytes(),
        b"OFFT",
        &[0x18, 0],
        &[0; 24],
        b"VERS",
        &[8, 0],
        &[2, 0, 6, 0, 3, 0, 1, 0], // air 2.6, language 3.1
        b"ENDT",
        b"ENDT", // the list
        &4u32.to_le_bytes(),
        b"ENDT", // public
        &4u32.to_le_bytes(),
        b"ENDT", // private
        BITCODE,
    ]
    .concat();
    let packed = pack(BITCODE, &[function(b"main", FunctionKind::Fragment)]);
    assert!(packed == expected, "{packed:02x?}");
}

#[test]
fn pack_vertex_attributes_and_patch_match_the_cxx_layout() {
    // As above for a vertex function with two inputs and a quad patch: the entry grows by the
    // 7-byte patch tag and the public metadata holds `VATT` and `VATY`.
    let mut vertex = function(b"vs", FunctionKind::Vertex);
    vertex.vertex_attributes = vec![
        attribute(0, b"POSITION", 0x06),
        attribute(1, b"TEXCOORD", 0x20),
    ];
    vertex.patch = Some(Patch::Quad);
    let hash = unhex(BC_HASH);
    let expected: Vec<u8> = [
        b"MTLB".as_slice(),
        &[0x01, 0x80, 2, 0, 7, 0, 0x00, 0x81, 14, 0, 4, 0],
        &0x11bu64.to_le_bytes(),
        &0x58u64.to_le_bytes(),
        &0x7fu64.to_le_bytes(),
        &0xdfu64.to_le_bytes(),
        &0x30u64.to_le_bytes(),
        &0x10fu64.to_le_bytes(),
        &8u64.to_le_bytes(),
        &0x117u64.to_le_bytes(),
        &4u64.to_le_bytes(),
        &1u32.to_le_bytes(),
        &0x7fu32.to_le_bytes(),
        b"NAME",
        &[3, 0],
        b"vs\0",
        b"TYPE",
        &[1, 0],
        &[0], // vertex
        b"HASH",
        &[0x20, 0],
        &hash,
        b"MDSZ",
        &[8, 0],
        &4u64.to_le_bytes(),
        b"OFFT",
        &[0x18, 0],
        &[0; 24],
        b"VERS",
        &[8, 0],
        &[2, 0, 6, 0, 3, 0, 1, 0],
        b"TESS",
        &[1, 0],
        &[2], // quad, no control points
        b"ENDT",
        b"ENDT",
        &0x2cu32.to_le_bytes(), // public: 44 bytes
        b"VATT",
        &[24, 0], // the count and the two entries
        &[2, 0],
        b"POSITION\0",
        &[0, 0x80], // location 0, active
        b"TEXCOORD\0",
        &[1, 0x80],
        b"VATY",
        &[4, 0],
        &[2, 0],
        &[0x06, 0x20],
        b"ENDT",
        &4u32.to_le_bytes(),
        b"ENDT",
        BITCODE,
    ]
    .concat();
    let packed = pack(BITCODE, &[vertex]);
    assert!(packed == expected, "{packed:02x?}");
}

#[test]
fn name_length_wraps_as_the_cxx_casts() {
    // `(uint16_t)(name.size() + 1)`: 65535 bytes and the terminator make 0x10000, written 0.
    let packed = pack(
        BITCODE,
        &[function(&vec![b'n'; 65535], FunctionKind::Kernel)],
    );
    assert_eq!(packed[0x60..0x66], *b"NAME\0\0");
    assert_eq!(u64_at(&packed, 0x10), packed.len() as u64);
}

fn every_kind() -> Vec<FunctionEntry> {
    let mut with_inputs = function(b"vertex_main", FunctionKind::Vertex);
    with_inputs.vertex_attributes = vec![
        attribute(0, b"POSITION", 0x06),
        attribute(3, b"COLOR\xff", 0x24),
        attribute(7, b"", 0x20),
    ];
    let mut patch = function(b"patch_main", FunctionKind::Vertex);
    patch.patch = Some(Patch::Triangle);
    vec![
        with_inputs,
        patch,
        function(b"x\xff", FunctionKind::Vertex),
        function(b"fragment_main", FunctionKind::Fragment),
        function(b"kernel_main", FunctionKind::Kernel),
        function(b"object_main", FunctionKind::Object),
        function(b"mesh_main", FunctionKind::Mesh),
    ]
}

#[test]
fn parse_inverts_pack_over_every_kind() {
    let functions = every_kind();
    let bitcode = b"BC\xc0\xde and some more bitcode".to_vec();
    let packed = pack(&bitcode, &functions);
    assert_eq!(parse(&packed), Ok((bitcode, functions)));
    // No functions: the container is a header and the bitcode.
    assert_eq!(
        parse(&pack(BITCODE, &[])),
        Ok((BITCODE.to_vec(), Vec::new()))
    );
}

#[test]
fn parse_refuses_truncation_and_survives_damage() {
    let packed = pack(b"BC\xc0\xde and some more bitcode", &every_kind());
    // The bitcode ends the file, so every shorter prefix lacks some of it.
    for len in 0..packed.len() {
        assert!(parse(&packed[..len]).is_err(), "prefix of {len} bytes");
    }
    // A byte changed anywhere is an answer, never a panic.
    for at in 0..packed.len() {
        for value in [0x00, 0x01, 0x7f, 0xff] {
            let mut damaged = packed.clone();
            damaged[at] = value;
            let _ = parse(&damaged);
        }
    }
    assert!(parse(b"MTLB").is_err());
    assert!(parse(b"").is_err());
    let mut wrong_magic = packed;
    wrong_magic[0] = b'X';
    assert_eq!(
        parse(&wrong_magic),
        Err(Error {
            what: "not a metallib"
        })
    );
}

#[test]
fn air_names_map_as_the_cxx_writer_maps_them() {
    assert_eq!(VertexAttribute::data_type_for(b"float4"), 0x06);
    assert_eq!(VertexAttribute::data_type_for(b"uint4"), 0x24);
    for other in [&b"float3"[..], b"int4", b"", b"float4 "] {
        assert_eq!(VertexAttribute::data_type_for(other), 0x20);
    }
    assert_eq!(Patch::from_air_name(b"triangle"), Patch::Triangle);
    assert_eq!(Patch::from_air_name(b"quad"), Patch::Quad);
    assert_eq!(Patch::from_air_name(b""), Patch::Quad);
}

fn sorted(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

#[test]
fn repack_every_baseline_metallib() {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: no baseline metallib is repacked");
        return;
    };
    let (mut files, mut functions, mut with_inputs, mut with_patch) = (0, 0, 0, 0);
    for out in sorted(Path::new(&base)) {
        if !out.is_dir() || out.extension().is_none_or(|ext| ext != "out") {
            continue;
        }
        for path in sorted(&out) {
            if path.extension().is_none_or(|ext| ext != "metallib") {
                continue;
            }
            let bytes = fs::read(&path).unwrap();
            let (bitcode, entries) =
                parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(pack(&bitcode, &entries) == bytes, "{}", path.display());
            files += 1;
            functions += entries.len();
            with_inputs += entries
                .iter()
                .filter(|f| !f.vertex_attributes.is_empty())
                .count();
            with_patch += entries.iter().filter(|f| f.patch.is_some()).count();
        }
    }
    assert!(
        files > 0,
        "no metallib under {}",
        Path::new(&base).display()
    );
    eprintln!(
        "repacked {files} metallibs: {functions} functions, {with_inputs} with vertex inputs, \
         {with_patch} with a patch"
    );
}
