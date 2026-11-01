use core::ptr;

use airconv_ir::container::{build, fourcc};
use winemetal_wire::{
    WMT_ATTRIBUTE_FORMAT_FLOAT2, WMT_ATTRIBUTE_FORMAT_FLOAT3, WMT_ATTRIBUTE_FORMAT_FLOAT4,
};

use crate::{
    abi::records::{D3d11InputElementDesc, D3d11SoDeclarationEntry},
    com::{E_FAIL, E_INVALIDARG},
    input_layout::{
        D3D11_APPEND_ALIGNED_ELEMENT, D3D11_INPUT_PER_INSTANCE_DATA, D3D11_INPUT_PER_VERTEX_DATA,
        parse_input_layout, parse_stream_output_layout,
    },
};

const DXGI_FORMAT_R32G32B32A32_TYPELESS: u32 = 1;
const DXGI_FORMAT_R32G32B32A32_FLOAT: u32 = 2;
const DXGI_FORMAT_R32G32B32_FLOAT: u32 = 6;
const DXGI_FORMAT_R32G32_FLOAT: u32 = 16;

// Builds a synthetic v4 `ISGN` or `OSGN` blob (`D3D10_INTERNALSHADER_PARAMETER`, 24-byte stride)
// wrapped in a valid DXBC container via `airconv_ir::container::build`.
fn make_dxbc_with_sig(part_fourcc: u32, params: &[(&[u8], u32, u32, u32, u8)]) -> Vec<u8> {
    let count = params.len();
    let records_end = 8 + count * 24;
    let mut strings = Vec::new();
    let mut name_offsets = Vec::with_capacity(count);
    for &(name, _, _, _, _) in params {
        name_offsets.push(u32::try_from(records_end + strings.len()).expect("fits in u32"));
        strings.extend_from_slice(name);
        strings.push(0);
    }
    while !strings.len().is_multiple_of(4) {
        strings.push(0);
    }

    let mut blob = Vec::with_capacity(records_end + strings.len());
    blob.extend_from_slice(&u32::try_from(count).expect("fits in u32").to_le_bytes());
    blob.extend_from_slice(&8u32.to_le_bytes());
    for (i, &(_, sem_idx, sys_val, reg, mask)) in params.iter().enumerate() {
        blob.extend_from_slice(&name_offsets[i].to_le_bytes()); // SemanticName offset
        blob.extend_from_slice(&sem_idx.to_le_bytes()); // SemanticIndex
        blob.extend_from_slice(&sys_val.to_le_bytes()); // SystemValue
        blob.extend_from_slice(&3u32.to_le_bytes()); // ComponentType (float32)
        blob.extend_from_slice(&reg.to_le_bytes()); // Register
        blob.push(mask); // Mask
        blob.push(mask); // ReadWriteMask
        blob.extend_from_slice(&0u16.to_le_bytes()); // Stream / padding
    }
    blob.extend_from_slice(&strings);

    build(&[(part_fourcc, &blob)]).expect("container fits in 32 bits")
}

#[test]
fn elements_from_vsia_signature() {
    let dxbc = make_dxbc_with_sig(
        fourcc::ISGN,
        &[
            (b"POSITION", 0, 0, 0, 0x7),
            (b"TEXCOORD", 0, 0, 1, 0x3),
            (b"COLOR", 1, 0, 2, 0xf),
            (b"SV_VertexID", 0, 6, 3, 0x1), // system_value = VERTEX_ID (ignored if missing from IA)
        ],
    );

    let descs = [
        D3d11InputElementDesc {
            semantic_name: c"POSITION".as_ptr(),
            semantic_index: 0,
            format: DXGI_FORMAT_R32G32B32_FLOAT,
            input_slot: 0,
            aligned_byte_offset: 0,
            input_slot_class: D3D11_INPUT_PER_VERTEX_DATA,
            instance_data_step_rate: 0,
        },
        D3d11InputElementDesc {
            semantic_name: c"texcoord".as_ptr(), // case-insensitive match
            semantic_index: 0,
            format: DXGI_FORMAT_R32G32_FLOAT,
            input_slot: 0,
            aligned_byte_offset: D3D11_APPEND_ALIGNED_ELEMENT,
            input_slot_class: D3D11_INPUT_PER_VERTEX_DATA,
            instance_data_step_rate: 0,
        },
        D3d11InputElementDesc {
            semantic_name: c"NORMAL".as_ptr(), // extra IA element not in VS signature -> skipped
            semantic_index: 0,
            format: DXGI_FORMAT_R32G32B32_FLOAT,
            input_slot: 1,
            aligned_byte_offset: 0,
            input_slot_class: D3D11_INPUT_PER_VERTEX_DATA,
            instance_data_step_rate: 0,
        },
        D3d11InputElementDesc {
            semantic_name: c"COLOR".as_ptr(),
            semantic_index: 1,
            format: DXGI_FORMAT_R32G32B32A32_FLOAT,
            input_slot: 2,
            aligned_byte_offset: 16,
            input_slot_class: D3D11_INPUT_PER_INSTANCE_DATA,
            instance_data_step_rate: 4,
        },
    ];

    let layout = parse_input_layout(&descs, &dxbc).expect("valid input layout");
    assert_eq!(layout.slot_mask(), (1 << 0) | (1 << 2));
    assert_eq!(layout.elements().len(), 3);

    let e0 = layout.elements()[0];
    assert_eq!(e0.index, 0);
    assert_eq!(e0.slot, 0);
    assert_eq!(e0.offset, 0);
    assert_eq!(e0.format, WMT_ATTRIBUTE_FORMAT_FLOAT3);
    assert_eq!(e0.step_function_and_rate, 1 << 1); // vertex (0), rate 1

    let e1 = layout.elements()[1];
    assert_eq!(e1.index, 1);
    assert_eq!(e1.slot, 0);
    assert_eq!(e1.offset, 12); // appended after 12-byte FLOAT3
    assert_eq!(e1.format, WMT_ATTRIBUTE_FORMAT_FLOAT2);
    assert_eq!(e1.step_function_and_rate, 1 << 1);

    let e2 = layout.elements()[2];
    assert_eq!(e2.index, 2);
    assert_eq!(e2.slot, 2);
    assert_eq!(e2.offset, 16);
    assert_eq!(e2.format, WMT_ATTRIBUTE_FORMAT_FLOAT4);
    assert_eq!(e2.step_function_and_rate, (4 << 1) | 1); // instance (1), rate 4
}

#[test]
fn missing_semantic_fails_as_cpp() {
    // VS expects POSITION0 (reg 0) and TEXCOORD0 (reg 1), but IA only provides POSITION0.
    // Per `d3d11_input_layout.cpp:79-88`, an unfulfilled non-system-value VS input register
    // fails with `E_INVALIDARG`.
    let dxbc = make_dxbc_with_sig(
        fourcc::ISGN,
        &[(b"POSITION", 0, 0, 0, 0xf), (b"TEXCOORD", 0, 0, 1, 0x3)],
    );
    let incomplete_descs = [D3d11InputElementDesc {
        semantic_name: c"POSITION".as_ptr(),
        semantic_index: 0,
        format: DXGI_FORMAT_R32G32B32A32_FLOAT,
        input_slot: 0,
        aligned_byte_offset: 0,
        input_slot_class: D3D11_INPUT_PER_VERTEX_DATA,
        instance_data_step_rate: 0,
    }];
    assert_eq!(
        parse_input_layout(&incomplete_descs, &dxbc),
        Err(E_INVALIDARG)
    );

    // Unknown DXGI format -> E_FAIL (`d3d11_input_layout.cpp:37-40`).
    let unknown_fmt = [D3d11InputElementDesc {
        semantic_name: c"POSITION".as_ptr(),
        semantic_index: 0,
        format: 9999,
        input_slot: 0,
        aligned_byte_offset: 0,
        input_slot_class: D3D11_INPUT_PER_VERTEX_DATA,
        instance_data_step_rate: 0,
    }];
    assert_eq!(parse_input_layout(&unknown_fmt, &dxbc), Err(E_FAIL));

    // Typeless format has `WMT_ATTRIBUTE_FORMAT_INVALID` -> E_INVALIDARG (`d3d11_input_layout.cpp:42-45`).
    let typeless_fmt = [D3d11InputElementDesc {
        semantic_name: c"POSITION".as_ptr(),
        semantic_index: 0,
        format: DXGI_FORMAT_R32G32B32A32_TYPELESS,
        input_slot: 0,
        aligned_byte_offset: 0,
        input_slot_class: D3D11_INPUT_PER_VERTEX_DATA,
        instance_data_step_rate: 0,
    }];
    assert_eq!(parse_input_layout(&typeless_fmt, &dxbc), Err(E_INVALIDARG));
}

#[test]
fn so_layout_from_output_v4() {
    let dxbc = make_dxbc_with_sig(
        fourcc::OSGN,
        &[(b"SV_POSITION", 0, 1, 0, 0xf), (b"TEXCOORD", 0, 0, 2, 0x3)],
    );

    let entries = [
        D3d11SoDeclarationEntry {
            stream: 0,
            semantic_name: c"SV_Position".as_ptr(),
            semantic_index: 0,
            start_component: 0,
            component_count: 2,
            output_slot: 0,
        },
        // Hole entry (null semantic_name)
        D3d11SoDeclarationEntry {
            stream: 0,
            semantic_name: ptr::null(),
            semantic_index: 0,
            start_component: 0,
            component_count: 1,
            output_slot: 0,
        },
        D3d11SoDeclarationEntry {
            stream: 0,
            semantic_name: c"TEXCOORD".as_ptr(),
            semantic_index: 0,
            start_component: 1,
            component_count: 2,
            output_slot: 0,
        },
    ];
    let strides = [20u32];
    let so = parse_stream_output_layout(&dxbc, &entries, &strides, 0xffff_ffff)
        .expect("valid SO layout");

    assert_eq!(so.strides, [20, 0, 0, 0]);
    assert_eq!(so.rasterized_stream, 0xffff_ffff);
    assert_eq!(so.elements.len(), 5);

    assert_eq!(so.elements[0].register_idx, 0);
    assert_eq!(so.elements[0].component, 0);
    assert_eq!(so.elements[0].offset, 0);

    assert_eq!(so.elements[1].register_idx, 0);
    assert_eq!(so.elements[1].component, 1);
    assert_eq!(so.elements[1].offset, 4);

    assert_eq!(so.elements[2].register_idx, 0xffff_ffff);
    assert_eq!(so.elements[2].component, 0);
    assert_eq!(so.elements[2].offset, 8);

    assert_eq!(so.elements[3].register_idx, 2);
    assert_eq!(so.elements[3].component, 1);
    assert_eq!(so.elements[3].offset, 12);

    assert_eq!(so.elements[4].register_idx, 2);
    assert_eq!(so.elements[4].component, 2);
    assert_eq!(so.elements[4].offset, 16);

    // Missing output semantic -> E_INVALIDARG
    let bad_entries = [D3d11SoDeclarationEntry {
        stream: 0,
        semantic_name: c"MISSING".as_ptr(),
        semantic_index: 0,
        start_component: 0,
        component_count: 1,
        output_slot: 0,
    }];
    assert_eq!(
        parse_stream_output_layout(&dxbc, &bad_entries, &strides, 0),
        Err(E_INVALIDARG)
    );
}
