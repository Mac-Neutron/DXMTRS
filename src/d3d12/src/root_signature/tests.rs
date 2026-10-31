use core::{mem::ManuallyDrop, ptr};

use super::*;
use crate::abi::root_signature::{
    D3D12_DESCRIPTOR_RANGE_FLAG_DATA_STATIC,
    D3D12_DESCRIPTOR_RANGE_FLAG_DATA_STATIC_WHILE_SET_AT_EXECUTE,
    D3D12_DESCRIPTOR_RANGE_FLAG_DATA_VOLATILE,
    D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_STATIC_KEEPING_BUFFER_BOUNDS_CHECKS,
    D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_VOLATILE, D3D12_DESCRIPTOR_RANGE_FLAG_NONE,
    D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND, D3D12_DESCRIPTOR_RANGE_TYPE_CBV,
    D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER, D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
    D3D12_DESCRIPTOR_RANGE_TYPE_UAV, D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC,
    D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC_WHILE_SET_AT_EXECUTE,
    D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE, D3D12_ROOT_DESCRIPTOR_FLAG_NONE,
    D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
    D3D12_ROOT_SIGNATURE_FLAG_ALLOW_STREAM_OUTPUT,
    D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS,
    D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS,
    D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS,
    D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS,
    D3D12_ROOT_SIGNATURE_FLAG_DENY_VERTEX_SHADER_ROOT_ACCESS, D3D12_ROOT_SIGNATURE_FLAG_NONE,
    D3D12_SHADER_VISIBILITY_ALL, D3D12_SHADER_VISIBILITY_AMPLIFICATION,
    D3D12_SHADER_VISIBILITY_DOMAIN, D3D12_SHADER_VISIBILITY_GEOMETRY, D3D12_SHADER_VISIBILITY_HULL,
    D3D12_SHADER_VISIBILITY_MESH, D3D12_SHADER_VISIBILITY_PIXEL, D3D12_SHADER_VISIBILITY_VERTEX,
    DescriptorRange, DescriptorRange1, RootConstants, RootDescriptor, RootDescriptor1,
    RootDescriptorTable, RootDescriptorTable1, RootParameter1Payload, RootParameterPayload,
    StaticSamplerDesc, VersionedDesc,
};

/// Task 0's blobs, recorded from the C++ `d3d12.dylib` (`golden/index.txt`, `golden/descs.txt`).
const GOLDEN: [(&str, &[u8]); 12] = [
    ("01-v10-empty", include_bytes!("golden/01-v10-empty.bin")),
    (
        "02-v10-empty-ia",
        include_bytes!("golden/02-v10-empty-ia.bin"),
    ),
    (
        "03-v10-constants",
        include_bytes!("golden/03-v10-constants.bin"),
    ),
    (
        "04-v10-root-descriptors",
        include_bytes!("golden/04-v10-root-descriptors.bin"),
    ),
    ("05-v10-tables", include_bytes!("golden/05-v10-tables.bin")),
    (
        "06-v10-static-samplers",
        include_bytes!("golden/06-v10-static-samplers.bin"),
    ),
    (
        "07-v10-everything",
        include_bytes!("golden/07-v10-everything.bin"),
    ),
    (
        "08-v11-root-descriptor-flags",
        include_bytes!("golden/08-v11-root-descriptor-flags.bin"),
    ),
    (
        "09-v11-range-flags",
        include_bytes!("golden/09-v11-range-flags.bin"),
    ),
    (
        "10-v11-visibilities",
        include_bytes!("golden/10-v11-visibilities.bin"),
    ),
    (
        "11-v11-static-samplers-deny",
        include_bytes!("golden/11-v11-static-samplers-deny.bin"),
    ),
    (
        "12-v11-everything",
        include_bytes!("golden/12-v11-everything.bin"),
    ),
];

const PS: u32 = D3D12_SHADER_VISIBILITY_PIXEL;
const VS: u32 = D3D12_SHADER_VISIBILITY_VERTEX;
const ALL: u32 = D3D12_SHADER_VISIBILITY_ALL;
const APPEND: u32 = D3D12_DESCRIPTOR_RANGE_OFFSET_APPEND;

/// A name, parameters and flags, then the C++'s offsets, total, heaps' and lock table's qwords.
type QwordCase<'a> = (&'a str, &'a [RootParameter1], u32, &'a [u32], u32, u32, u32);

/// The C++'s blob of a 1.0 description whose middle parameter has type 7.
const V10_UNKNOWN_TYPE: &str = concat!(
    "44584243678872fe300c8b07bf9ee8ce598416c201000000780000000100000024000000525453304c00",
    "0000010000000300000018000000000000004c0000000000000002000000000000003c00000007000000",
    "050000000000000003000000000000004400000001000000020000000300000004000000",
);
/// The C++'s blob of a 1.1 description whose middle parameter has type 7.
const V11_UNKNOWN_TYPE: &str = concat!(
    "44584243e5369bcd44cc0b9bc49ffff263756a5b01000000800000000100000024000000525453305400",
    "00000200000003000000180000000000000054000000000000000200000000000000",
    "3c000000070000000500000000000000030000000000000048000000010000000200000008000000",
    "030000000400000000000000",
);
/// The C++'s blob of a table without ranges and then root constants.
const V10_EMPTY_TABLE: &str = concat!(
    "4458424369f4aba51b8aeb9b9c58e456057e61700100000070000000010000002400000052545330440000",
    "000100000002000000180000000000000044000000000000000000000000000000300000000100000000",
    "000000380000000000000038000000050000000600000007000000",
);

#[test]
fn serialized_blobs_are_the_golden() {
    let samplers = samplers();
    let r5 = [
        range(D3D12_DESCRIPTOR_RANGE_TYPE_SRV, u32::MAX, 0, 0, APPEND),
        range(D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 4, 2, 1, 0),
        range(D3D12_DESCRIPTOR_RANGE_TYPE_CBV, 1, 0, 0, APPEND),
    ];
    let r5b = [range(D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER, 2, 0, 0, APPEND)];
    let p3 = [constants(0, 0, 4, ALL), constants(1, 1, 16, PS)];
    let p4 = [
        descriptor(D3D12_ROOT_PARAMETER_TYPE_CBV, 0, 0, VS),
        descriptor(D3D12_ROOT_PARAMETER_TYPE_SRV, 0, 2, PS),
        descriptor(D3D12_ROOT_PARAMETER_TYPE_UAV, 3, 0, ALL),
    ];
    let p5 = [table(&r5, ALL), table(&r5b, VS)];
    let p7 = [
        constants(2, 0, 8, ALL),
        descriptor(D3D12_ROOT_PARAMETER_TYPE_CBV, 1, 0, ALL),
        table(&r5, PS),
    ];
    let q8 = [
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_CBV,
            0,
            0,
            D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC,
            ALL,
        ),
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_SRV,
            1,
            0,
            D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC_WHILE_SET_AT_EXECUTE,
            PS,
        ),
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_UAV,
            2,
            5,
            D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE,
            ALL,
        ),
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_SRV,
            3,
            0,
            D3D12_ROOT_DESCRIPTOR_FLAG_NONE,
            VS,
        ),
    ];
    let r9 = ranges_9();
    let r9s = [range1(
        D3D12_DESCRIPTOR_RANGE_TYPE_SAMPLER,
        4,
        0,
        0,
        D3D12_DESCRIPTOR_RANGE_FLAG_NONE,
        APPEND,
    )];
    let q9 = [table1(&r9, ALL), table1(&r9s, PS)];
    let q10 = [
        constants1(0, 0, 1, D3D12_SHADER_VISIBILITY_VERTEX),
        constants1(1, 0, 2, D3D12_SHADER_VISIBILITY_HULL),
        constants1(2, 0, 3, D3D12_SHADER_VISIBILITY_DOMAIN),
        constants1(3, 0, 4, D3D12_SHADER_VISIBILITY_GEOMETRY),
        constants1(4, 0, 5, D3D12_SHADER_VISIBILITY_PIXEL),
        constants1(5, 0, 6, D3D12_SHADER_VISIBILITY_AMPLIFICATION),
        constants1(6, 0, 7, D3D12_SHADER_VISIBILITY_MESH),
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_CBV,
            7,
            0,
            D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC,
            ALL,
        ),
        table1(&r9[..2], ALL),
    ];
    let q12 = [
        constants1(0, 0, 12, ALL),
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_UAV,
            0,
            0,
            D3D12_ROOT_DESCRIPTOR_FLAG_DATA_VOLATILE,
            PS,
        ),
        table1(&r9, ALL),
        table1(&r9s, VS),
    ];
    let deny_four = D3D12_ROOT_SIGNATURE_FLAG_DENY_VERTEX_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_DOMAIN_SHADER_ROOT_ACCESS
        | D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS;
    let ia = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
    let ia_so = ia | D3D12_ROOT_SIGNATURE_FLAG_ALLOW_STREAM_OUTPUT;
    let both_heaps = D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
        | D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED;
    let none = D3D12_ROOT_SIGNATURE_FLAG_NONE;
    let cases = [
        versioned(&[], &[], none),
        versioned(&[], &[], ia),
        versioned(&p3, &[], none),
        versioned(&p4, &[], none),
        versioned(&p5, &[], none),
        versioned(&[], &samplers[..2], deny_four),
        versioned(&p7, &samplers, ia_so),
        versioned1(desc1(&q8, &[], none)),
        versioned1(desc1(&q9, &[], none)),
        versioned1(desc1(&q10, &[], none)),
        versioned1(desc1(
            &[],
            &samplers,
            deny_four | D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS,
        )),
        versioned1(desc1(&q12, &samplers, ia_so | both_heaps)),
    ];
    for ((name, golden), desc) in GOLDEN.iter().zip(&cases) {
        // SAFETY: every array of the descriptions is one of this test's locals, counted by its length.
        let blob = unsafe { serialize_versioned(desc) };
        assert_eq!(blob.as_deref(), Ok(*golden), "{name}");
    }
}

#[test]
fn qword_offsets_as_cpp() {
    // The C++'s answers, from `CalculateRootSignatureQwordOffsets` (d3d12_root_signature.cpp:351-384
    // verbatim in a host harness) over the same parameter types, value counts and flags.
    let samplers = samplers();
    let r9 = ranges_9();
    let cbv = || descriptor1(D3D12_ROOT_PARAMETER_TYPE_CBV, 0, 0, 0, ALL);
    let srv = || descriptor1(D3D12_ROOT_PARAMETER_TYPE_SRV, 0, 0, 0, ALL);
    let uav = || descriptor1(D3D12_ROOT_PARAMETER_TYPE_UAV, 0, 0, 0, ALL);
    let tab = || table1(&r9, ALL);
    let mut unknown = cbv();
    unknown.parameter_type = 9;
    let c = |n| constants1(0, 0, n, ALL);
    let cases: [QwordCase; 10] = [
        (
            "08-v11-root-descriptor-flags",
            &[cbv(), srv(), uav(), srv()],
            0,
            &[0, 1, 2, 3],
            5,
            u32::MAX,
            4,
        ),
        (
            "09-v11-range-flags",
            &[tab(), tab()],
            0,
            &[0, 1],
            3,
            u32::MAX,
            2,
        ),
        (
            "10-v11-visibilities",
            &[c(1), c(2), c(3), c(4), c(5), c(6), c(7), cbv(), tab()],
            0,
            &[0, 1, 2, 4, 6, 9, 12, 16, 17],
            19,
            u32::MAX,
            18,
        ),
        (
            "11-v11-static-samplers-deny",
            &[],
            0x3e,
            &[],
            1,
            u32::MAX,
            0,
        ),
        (
            "12-v11-everything",
            &[c(12), uav(), tab(), tab()],
            0xc41,
            &[0, 6, 7, 8],
            12,
            9,
            11,
        ),
        ("resource-heap-only", &[cbv()], 0x400, &[0], 4, 1, 3),
        ("sampler-heap-only", &[], 0x800, &[], 3, 0, 2),
        ("zero-constants", &[c(0), cbv()], 0, &[0, 0], 2, u32::MAX, 1),
        (
            "unknown-type",
            &[cbv(), unknown, srv()],
            0,
            &[0, 1],
            3,
            u32::MAX,
            2,
        ),
        (
            "wrapping-constants",
            &[c(u32::MAX), c(u32::MAX), cbv()],
            0xc00,
            &[0, 0x8000_0000, 0],
            4,
            1,
            3,
        ),
    ];
    for (name, parameters, flags, offsets, total, heaps, lock) in cases {
        // SAFETY: the parameters are this test's locals, counted by their length.
        let got = unsafe { qword_offsets(&desc1(parameters, &samplers, flags)) }.expect(name);
        assert_eq!(got.parameters, offsets, "{name}");
        assert_eq!(
            (got.total, got.heaps, got.lock),
            (total, heaps, lock),
            "{name}"
        );
    }
}

#[test]
fn bad_desc_errors_as_cpp() {
    // The C++ `d3d12.dylib`'s answers (a host harness over Task 0's native build).
    let empty = desc(&[], &[], D3D12_ROOT_SIGNATURE_FLAG_NONE);
    for version in [0, 3, u32::MAX] {
        let desc = VersionedRootSignatureDesc {
            version,
            desc: VersionedDesc {
                desc_1_0: ManuallyDrop::new(desc(&[], &[], D3D12_ROOT_SIGNATURE_FLAG_NONE)),
            },
        };
        // SAFETY: the description has no arrays.
        let blob = unsafe { serialize_versioned(&desc) };
        assert_eq!(blob, Err(E_INVALIDARG), "{version}");
    }
    for version in [0, D3D_ROOT_SIGNATURE_VERSION_1_1, 3] {
        // SAFETY: the description has no arrays.
        let blob = unsafe { serialize(&empty, version) };
        assert_eq!(blob, Err(E_INVALIDARG), "{version}");
    }
    // SAFETY: the description has no arrays.
    let blob = unsafe { serialize(&empty, D3D_ROOT_SIGNATURE_VERSION_1_0) };
    assert_eq!(blob.as_deref(), Ok(GOLDEN[0].1));

    // A parameter type the format doesn't have is kept, with payload offset 0 and no payload.
    let mut p10 = [
        descriptor(D3D12_ROOT_PARAMETER_TYPE_CBV, 1, 2, ALL),
        descriptor(D3D12_ROOT_PARAMETER_TYPE_CBV, 0, 0, PS),
        descriptor(D3D12_ROOT_PARAMETER_TYPE_SRV, 3, 4, ALL),
    ];
    p10[1].parameter_type = 7;
    let mut p11 = [
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_CBV,
            1,
            2,
            D3D12_ROOT_DESCRIPTOR_FLAG_DATA_STATIC,
            ALL,
        ),
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_CBV,
            0,
            0,
            D3D12_ROOT_DESCRIPTOR_FLAG_NONE,
            PS,
        ),
        descriptor1(
            D3D12_ROOT_PARAMETER_TYPE_SRV,
            3,
            4,
            D3D12_ROOT_DESCRIPTOR_FLAG_NONE,
            ALL,
        ),
    ];
    p11[1].parameter_type = 7;
    // A table without ranges: its ranges' offset is where they would start.
    let e10 = [table(&[], ALL), constants(5, 6, 7, ALL)];
    let cases = [
        (
            "v10-unknown-type",
            versioned(&p10, &[], D3D12_ROOT_SIGNATURE_FLAG_NONE),
            V10_UNKNOWN_TYPE,
        ),
        (
            "v11-unknown-type",
            versioned1(desc1(&p11, &[], D3D12_ROOT_SIGNATURE_FLAG_NONE)),
            V11_UNKNOWN_TYPE,
        ),
        (
            "v10-empty-table",
            versioned(&e10, &[], D3D12_ROOT_SIGNATURE_FLAG_NONE),
            V10_EMPTY_TABLE,
        ),
    ];
    for (name, desc, hex) in cases {
        // SAFETY: every array of the description is one of this test's locals, counted by its length.
        let blob = unsafe { serialize_versioned(&desc) };
        assert_eq!(blob, Ok(bytes(hex)), "{name}");
    }

    // Where the C++ dereferences a null array, the Rust refuses it.
    let mut nulls = desc(&[], &[], D3D12_ROOT_SIGNATURE_FLAG_NONE);
    nulls.num_static_samplers = 1;
    // SAFETY: the null array is the point; no other array is read.
    let blob = unsafe { serialize(&nulls, D3D_ROOT_SIGNATURE_VERSION_1_0) };
    assert_eq!(blob, Err(E_INVALIDARG));
    nulls.num_static_samplers = 0;
    nulls.num_parameters = 1;
    // SAFETY: as above.
    let blob = unsafe { serialize(&nulls, D3D_ROOT_SIGNATURE_VERSION_1_0) };
    assert_eq!(blob, Err(E_INVALIDARG));
    let null_ranges = [RootParameter {
        parameter_type: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        payload: RootParameterPayload {
            descriptor_table: ManuallyDrop::new(RootDescriptorTable {
                num_descriptor_ranges: 1,
                p_descriptor_ranges: ptr::null(),
            }),
        },
        shader_visibility: ALL,
    }];
    let tables = desc(&null_ranges, &[], D3D12_ROOT_SIGNATURE_FLAG_NONE);
    // SAFETY: as above.
    let blob = unsafe { serialize(&tables, D3D_ROOT_SIGNATURE_VERSION_1_0) };
    assert_eq!(blob, Err(E_INVALIDARG));
    let mut nulls1 = desc1(&[], &[], D3D12_ROOT_SIGNATURE_FLAG_NONE);
    nulls1.num_parameters = 1;
    // SAFETY: as above.
    let offsets = unsafe { qword_offsets(&nulls1) };
    assert!(matches!(offsets, Err(E_INVALIDARG)));
}

/// The bytes a hex string spells.
fn bytes(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).expect("hex"), 16).expect("hex"))
        .collect()
}

/// `d3d12_native.cpp`'s three static samplers; the enumerations are `descs.txt`'s numbers.
fn samplers() -> [StaticSamplerDesc; 3] {
    [
        StaticSamplerDesc {
            filter: 0x15, // MIN_MAG_MIP_LINEAR
            address_u: 1, // WRAP
            address_v: 3, // CLAMP
            address_w: 2, // MIRROR
            mip_lod_bias: 0.0,
            max_anisotropy: 16,
            comparison_func: 1, // NEVER
            border_color: 1,    // OPAQUE_BLACK
            min_lod: 0.0,
            max_lod: f32::MAX, // D3D12_FLOAT32_MAX
            shader_register: 0,
            register_space: 0,
            shader_visibility: PS,
        },
        StaticSamplerDesc {
            filter: 0x80, // COMPARISON_MIN_MAG_MIP_POINT
            address_u: 4, // BORDER
            address_v: 5, // MIRROR_ONCE
            address_w: 1, // WRAP
            mip_lod_bias: -1.5,
            max_anisotropy: 1,
            comparison_func: 4, // LESS_EQUAL
            border_color: 2,    // OPAQUE_WHITE
            min_lod: 1.0,
            max_lod: 8.0,
            shader_register: 1,
            register_space: 3,
            shader_visibility: ALL,
        },
        StaticSamplerDesc {
            filter: 0x55, // ANISOTROPIC
            address_u: 3, // CLAMP
            address_v: 3,
            address_w: 3,
            mip_lod_bias: 0.25,
            max_anisotropy: 8,
            comparison_func: 8, // ALWAYS
            border_color: 0,    // TRANSPARENT_BLACK
            min_lod: 0.0,
            max_lod: 4.0,
            shader_register: 2,
            register_space: 0,
            shader_visibility: VS,
        },
    ]
}

/// `d3d12_native.cpp`'s `r9`: every range flag.
fn ranges_9() -> [DescriptorRange1; 6] {
    [
        range1(
            D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            8,
            0,
            0,
            D3D12_DESCRIPTOR_RANGE_FLAG_NONE,
            APPEND,
        ),
        range1(
            D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            8,
            8,
            0,
            D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_VOLATILE,
            16,
        ),
        range1(
            D3D12_DESCRIPTOR_RANGE_TYPE_UAV,
            2,
            0,
            0,
            D3D12_DESCRIPTOR_RANGE_FLAG_DATA_VOLATILE,
            APPEND,
        ),
        range1(
            D3D12_DESCRIPTOR_RANGE_TYPE_CBV,
            3,
            0,
            1,
            D3D12_DESCRIPTOR_RANGE_FLAG_DATA_STATIC,
            APPEND,
        ),
        range1(
            D3D12_DESCRIPTOR_RANGE_TYPE_CBV,
            u32::MAX,
            4,
            1,
            D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_VOLATILE
                | D3D12_DESCRIPTOR_RANGE_FLAG_DATA_STATIC_WHILE_SET_AT_EXECUTE,
            APPEND,
        ),
        range1(
            D3D12_DESCRIPTOR_RANGE_TYPE_SRV,
            1,
            40,
            2,
            D3D12_DESCRIPTOR_RANGE_FLAG_DESCRIPTORS_STATIC_KEEPING_BUFFER_BOUNDS_CHECKS,
            APPEND,
        ),
    ]
}

fn count<T>(items: &[T]) -> u32 {
    u32::try_from(items.len()).expect("a test's array is short")
}

/// The array's pointer, null for an empty one as the golden's descriptions pass it.
const fn pointer<T>(items: &[T]) -> *const T {
    if items.is_empty() {
        ptr::null()
    } else {
        items.as_ptr()
    }
}

fn desc(
    parameters: &[RootParameter],
    samplers: &[StaticSamplerDesc],
    flags: u32,
) -> RootSignatureDesc {
    RootSignatureDesc {
        num_parameters: count(parameters),
        p_parameters: pointer(parameters),
        num_static_samplers: count(samplers),
        p_static_samplers: pointer(samplers),
        flags,
    }
}

fn desc1(
    parameters: &[RootParameter1],
    samplers: &[StaticSamplerDesc],
    flags: u32,
) -> RootSignatureDesc1 {
    RootSignatureDesc1 {
        num_parameters: count(parameters),
        p_parameters: pointer(parameters),
        num_static_samplers: count(samplers),
        p_static_samplers: pointer(samplers),
        flags,
    }
}

fn versioned(
    parameters: &[RootParameter],
    samplers: &[StaticSamplerDesc],
    flags: u32,
) -> VersionedRootSignatureDesc {
    VersionedRootSignatureDesc {
        version: D3D_ROOT_SIGNATURE_VERSION_1_0,
        desc: VersionedDesc {
            desc_1_0: ManuallyDrop::new(desc(parameters, samplers, flags)),
        },
    }
}

const fn versioned1(desc: RootSignatureDesc1) -> VersionedRootSignatureDesc {
    VersionedRootSignatureDesc {
        version: D3D_ROOT_SIGNATURE_VERSION_1_1,
        desc: VersionedDesc {
            desc_1_1: ManuallyDrop::new(desc),
        },
    }
}

const fn range(kind: u32, num: u32, base: u32, space: u32, offset: u32) -> DescriptorRange {
    DescriptorRange {
        range_type: kind,
        num_descriptors: num,
        base_shader_register: base,
        register_space: space,
        offset_in_descriptors_from_table_start: offset,
    }
}

const fn range1(
    kind: u32,
    num: u32,
    base: u32,
    space: u32,
    flags: u32,
    offset: u32,
) -> DescriptorRange1 {
    DescriptorRange1 {
        range_type: kind,
        num_descriptors: num,
        base_shader_register: base,
        register_space: space,
        flags,
        offset_in_descriptors_from_table_start: offset,
    }
}

const fn constants(register: u32, space: u32, values: u32, visibility: u32) -> RootParameter {
    RootParameter {
        parameter_type: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
        payload: RootParameterPayload {
            constants: ManuallyDrop::new(RootConstants {
                shader_register: register,
                register_space: space,
                num_32bit_values: values,
            }),
        },
        shader_visibility: visibility,
    }
}

const fn constants1(register: u32, space: u32, values: u32, visibility: u32) -> RootParameter1 {
    RootParameter1 {
        parameter_type: D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS,
        payload: RootParameter1Payload {
            constants: ManuallyDrop::new(RootConstants {
                shader_register: register,
                register_space: space,
                num_32bit_values: values,
            }),
        },
        shader_visibility: visibility,
    }
}

const fn descriptor(kind: u32, register: u32, space: u32, visibility: u32) -> RootParameter {
    RootParameter {
        parameter_type: kind,
        payload: RootParameterPayload {
            descriptor: ManuallyDrop::new(RootDescriptor {
                shader_register: register,
                register_space: space,
            }),
        },
        shader_visibility: visibility,
    }
}

const fn descriptor1(
    kind: u32,
    register: u32,
    space: u32,
    flags: u32,
    visibility: u32,
) -> RootParameter1 {
    RootParameter1 {
        parameter_type: kind,
        payload: RootParameter1Payload {
            descriptor: ManuallyDrop::new(RootDescriptor1 {
                shader_register: register,
                register_space: space,
                flags,
            }),
        },
        shader_visibility: visibility,
    }
}

fn table(ranges: &[DescriptorRange], visibility: u32) -> RootParameter {
    RootParameter {
        parameter_type: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        payload: RootParameterPayload {
            descriptor_table: ManuallyDrop::new(RootDescriptorTable {
                num_descriptor_ranges: count(ranges),
                p_descriptor_ranges: pointer(ranges),
            }),
        },
        shader_visibility: visibility,
    }
}

fn table1(ranges: &[DescriptorRange1], visibility: u32) -> RootParameter1 {
    RootParameter1 {
        parameter_type: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,
        payload: RootParameter1Payload {
            descriptor_table: ManuallyDrop::new(RootDescriptorTable1 {
                num_descriptor_ranges: count(ranges),
                p_descriptor_ranges: pointer(ranges),
            }),
        },
        shader_visibility: visibility,
    }
}
