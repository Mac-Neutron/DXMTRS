use core::{ffi::c_void, ptr};

use airconv_ir::container::{build, fourcc};
use winemetal_sys::sm50::Sm50IaInputElement;

use crate::{
    abi::pipeline::{
        BlendDesc, CachedPipelineState, ComputePipelineStateDesc, DepthStencilDesc,
        DepthStencilOpDesc, GraphicsPipelineStateDesc, InputElementDesc, InputLayoutDesc,
        RasterizerDesc, RenderTargetBlendDesc, SampleDesc, ShaderBytecode, StreamOutputDesc,
    },
    com::{E_INVALIDARG, IID_ID3D12_PIPELINE_LIBRARY1, S_OK},
    pipeline::{
        graphics::{extract_input_layout, parse_input_signature},
        library::{PipelineLibrary, create_pipeline_library},
        record::{
            Blob, HEADER_SIZE, Kind, MAGIC, Recorder, checksum, serialize_compute,
            serialize_graphics, sha1,
        },
    },
    shader_cache::Sha1Digest,
};

const fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => panic!("invalid hex digit"),
    }
}

const fn hex_sha1(s: &[u8; 40]) -> Sha1Digest {
    let mut out = [0u8; 20];
    let mut i = 0;
    while i < 20 {
        out[i] = (hex_nibble(s[i * 2]) << 4) | hex_nibble(s[i * 2 + 1]);
        i += 1;
    }
    out
}

const EMPTY_BYTECODE: ShaderBytecode = ShaderBytecode {
    shader_bytecode: ptr::null(),
    bytecode_length: 0,
};

const EMPTY_CACHED_PSO: CachedPipelineState = CachedPipelineState {
    cached_blob: ptr::null(),
    cached_blob_size_in_bytes: 0,
};

const ZERO_RT_BLEND: RenderTargetBlendDesc = RenderTargetBlendDesc {
    blend_enable: 0,
    logic_op_enable: 0,
    src_blend: 0,
    dest_blend: 0,
    blend_op: 0,
    src_blend_alpha: 0,
    dest_blend_alpha: 0,
    blend_op_alpha: 0,
    logic_op: 0,
    render_target_write_mask: 0,
};

const ZERO_DS_OP: DepthStencilOpDesc = DepthStencilOpDesc {
    stencil_fail_op: 0,
    stencil_depth_fail_op: 0,
    stencil_pass_op: 0,
    stencil_func: 0,
};

#[test]
fn record_bytes_as_cpp() {
    let cs_id = hex_sha1(b"54dc0b56c1c38d06eab30e199745a8eeb07e0a4c");
    let compute_desc = ComputePipelineStateDesc {
        root_signature: ptr::null_mut(),
        cs: EMPTY_BYTECODE,
        node_mask: 0,
        cached_pso: EMPTY_CACHED_PSO,
        flags: 0,
    };
    let compute_bytes = serialize_compute(&compute_desc, &[0; 20], &cs_id);
    assert_eq!(compute_bytes.len(), 48);
    assert_eq!(checksum(&compute_bytes), 0xa6a5_d997_e601_e5b7);
    assert_eq!(
        sha1(&compute_bytes),
        hex_sha1(b"c9a25b0c51878422a2b7753cafbe83879f1b0804")
    );

    let root_id = hex_sha1(b"8192391c1cf752f1fca8d213c7da20885913f26b");
    let vs_id = hex_sha1(b"b1906139a5229a69a19679318b89ce3d038d2d30");
    let ps_id = hex_sha1(b"f4f0210b14eba58eef9636aa137554564e8430d3");
    let gs_id = [0u8; 20];
    let elem = InputElementDesc {
        semantic_name: c"POSITION".as_ptr().cast::<u8>(),
        semantic_index: 0,
        format: 16,
        input_slot: 0,
        aligned_byte_offset: 0,
        input_slot_class: 0,
        instance_data_step_rate: 0,
    };
    let mut rts = [ZERO_RT_BLEND; 8];
    rts[0].render_target_write_mask = 0x0f;
    let mut rtv_formats = [0u32; 8];
    rtv_formats[0] = 2;
    let gfx_desc = GraphicsPipelineStateDesc {
        root_signature: ptr::null_mut(),
        vs: EMPTY_BYTECODE,
        ps: EMPTY_BYTECODE,
        ds: EMPTY_BYTECODE,
        hs: EMPTY_BYTECODE,
        gs: EMPTY_BYTECODE,
        stream_output: StreamOutputDesc {
            so_declaration: ptr::null(),
            num_entries: 0,
            buffer_strides: ptr::null(),
            num_strides: 0,
            rasterized_stream: 0,
        },
        blend_state: BlendDesc {
            alpha_to_coverage_enable: 0,
            independent_blend_enable: 0,
            render_target: rts,
        },
        sample_mask: u32::MAX,
        rasterizer_state: RasterizerDesc {
            fill_mode: 3,
            cull_mode: 1,
            front_counter_clockwise: 0,
            depth_bias: 0,
            depth_bias_clamp: 0.0,
            slope_scaled_depth_bias: 0.0,
            depth_clip_enable: 1,
            multisample_enable: 0,
            antialiased_line_enable: 0,
            forced_sample_count: 0,
            conservative_raster: 0,
        },
        depth_stencil_state: DepthStencilDesc {
            depth_enable: 0,
            depth_write_mask: 0,
            depth_func: 0,
            stencil_enable: 0,
            stencil_read_mask: 0,
            stencil_write_mask: 0,
            front_face: ZERO_DS_OP,
            back_face: ZERO_DS_OP,
        },
        input_layout: InputLayoutDesc {
            input_element_descs: &raw const elem,
            num_elements: 1,
        },
        ib_strip_cut_value: 0,
        primitive_topology_type: 3,
        num_render_targets: 1,
        rtv_formats,
        dsv_format: 0,
        sample_desc: SampleDesc {
            count: 1,
            quality: 0,
        },
        node_mask: 0,
        cached_pso: EMPTY_CACHED_PSO,
        flags: 0,
    };
    // SAFETY: `gfx_desc.input_layout` points to `elem` with a valid C string `semantic_name`.
    let gfx_bytes = unsafe { serialize_graphics(&gfx_desc, &root_id, &vs_id, &ps_id, &gs_id) };
    assert_eq!(gfx_bytes.len(), 586);
    assert_eq!(checksum(&gfx_bytes), 0x1769_b899_ffca_40ff);
    assert_eq!(
        sha1(&gfx_bytes),
        hex_sha1(b"db95c12485630ac591b3260c03f9735f93d8b9d3")
    );
}

#[test]
fn torn_tail_cut() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("dxmtrs_test_torn_{}.pipelines", std::process::id()));
    let _ = std::fs::remove_file(&path);

    let payload1 = b"first_compute_payload";
    let payload2 = b"second_compute_payload";
    {
        let mut rec = Recorder::new(path.clone());
        rec.append(Kind::Compute, payload1, &[]);
    }
    let valid_len = 8 + HEADER_SIZE + payload1.len();
    let mut raw = std::fs::read(&path).expect("read after first append");
    assert_eq!(raw.len(), valid_len);
    raw.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef, 1, 2, 3, 4, 5, 6]);
    std::fs::write(&path, &raw).expect("write torn tail");

    {
        let mut rec = Recorder::new(path.clone());
        rec.append(Kind::Compute, payload2, &[Blob::NONE]);
    }
    let final_bytes = std::fs::read(&path).expect("read after second append");
    let expected_len = valid_len + HEADER_SIZE + payload2.len();
    assert_eq!(final_bytes.len(), expected_len);
    assert_eq!(&final_bytes[..8], &MAGIC);
    assert_eq!(&final_bytes[valid_len + HEADER_SIZE..], payload2.as_slice());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn foreign_file_restarted() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!(
        "dxmtrs_test_foreign_{}.pipelines",
        std::process::id()
    ));
    std::fs::write(&path, b"not a pipelines file").expect("write foreign file");

    let payload = b"fresh_compute_record";
    {
        let mut rec = Recorder::new(path.clone());
        rec.append(Kind::Compute, payload, &[]);
    }
    let bytes = std::fs::read(&path).expect("read restarted file");
    assert_eq!(&bytes[..8], &MAGIC);
    assert_eq!(bytes.len(), 8 + HEADER_SIZE + payload.len());
    assert_eq!(&bytes[8 + HEADER_SIZE..], payload.as_slice());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn library_blob_round_trip() {
    let iid = IID_ID3D12_PIPELINE_LIBRARY1;
    let mut first_ptr: *mut c_void = ptr::null_mut();
    // SAFETY: Null device and null blob with zero length are valid for creating an empty library in tests.
    let hr = unsafe {
        create_pipeline_library(
            ptr::null_mut(),
            ptr::null(),
            0,
            (&raw const iid).cast::<c_void>(),
            &raw mut first_ptr,
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!first_ptr.is_null());

    // SAFETY: `first_ptr` was returned by `create_pipeline_library` as an `ID3D12PipelineLibrary1`.
    let first_lib = unsafe { &*first_ptr.cast::<PipelineLibrary>() };
    let key: Vec<u16> = if cfg!(windows) {
        b"pso_a".iter().map(|&c| u16::from(c)).collect()
    } else {
        b"pso_a".iter().flat_map(|&c| [u16::from(c), 0]).collect()
    };
    first_lib
        .entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            key.clone(),
            super::library::Entry {
                kind: 2,
                hash: 0x1122_3344_5566_7789,
                pso: 0,
            },
        );

    // SAFETY: `first_lib.vtbl.get_serialized_size` is valid on `first_ptr`.
    let size = unsafe { (first_lib.vtbl.get_serialized_size)(first_ptr) };
    assert!(size > 16);
    let mut buf = vec![0u8; size];
    // SAFETY: `buf` has `size` writable bytes for `serialize`.
    let hr_ser =
        unsafe { (first_lib.vtbl.serialize)(first_ptr, buf.as_mut_ptr().cast::<c_void>(), size) };
    assert_eq!(hr_ser, S_OK);

    let mut reloaded_ptr: *mut c_void = ptr::null_mut();
    // SAFETY: `buf` holds `size` serialized library bytes.
    let hr2 = unsafe {
        create_pipeline_library(
            ptr::null_mut(),
            buf.as_ptr().cast::<c_void>(),
            buf.len(),
            (&raw const iid).cast::<c_void>(),
            &raw mut reloaded_ptr,
        )
    };
    assert_eq!(hr2, S_OK);
    assert!(!reloaded_ptr.is_null());

    // SAFETY: `reloaded_ptr` was returned by `create_pipeline_library`.
    let reloaded_lib = unsafe { &*reloaded_ptr.cast::<PipelineLibrary>() };
    {
        let entries = reloaded_lib
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries.get(&key).map(|e| e.hash),
            Some(0x1122_3344_5566_7789)
        );
    }

    // SAFETY: Release `reloaded_ptr`.
    unsafe {
        (reloaded_lib.vtbl.release)(reloaded_ptr);
    }
    // SAFETY: Release `first_ptr`.
    unsafe {
        (first_lib.vtbl.release)(first_ptr);
    }
}

#[test]
fn library_cpp_blob_loads_then_refuses_by_hash() {
    let mut blob = Vec::new();
    blob.extend_from_slice(b"DXMTPLB1");
    blob.extend_from_slice(&1u32.to_le_bytes());
    blob.extend_from_slice(&1u32.to_le_bytes());
    let name_ascii = b"comp0";
    let chars = u32::try_from(name_ascii.len()).expect("len fits u32");
    blob.extend_from_slice(&chars.to_le_bytes());
    let mut wide_name = Vec::new();
    for &c in name_ascii {
        if cfg!(windows) {
            blob.extend_from_slice(&u16::from(c).to_le_bytes());
            wide_name.push(u16::from(c));
        } else {
            blob.extend_from_slice(&u32::from(c).to_le_bytes());
            wide_name.push(u16::from(c));
            wide_name.push(0);
        }
    }
    wide_name.push(0);
    if !cfg!(windows) {
        wide_name.push(0);
    }
    blob.extend_from_slice(&0xdead_beef_cafe_babf_u64.to_le_bytes());

    let iid = IID_ID3D12_PIPELINE_LIBRARY1;
    let mut lib_ptr: *mut c_void = ptr::null_mut();
    // SAFETY: `blob` is a valid buffer of `blob.len()` bytes.
    let hr = unsafe {
        create_pipeline_library(
            ptr::null_mut(),
            blob.as_ptr().cast::<c_void>(),
            blob.len(),
            (&raw const iid).cast::<c_void>(),
            &raw mut lib_ptr,
        )
    };
    assert_eq!(hr, S_OK);
    assert!(!lib_ptr.is_null());

    // SAFETY: `lib_ptr` is a valid `PipelineLibrary`.
    let lib = unsafe { &*lib_ptr.cast::<PipelineLibrary>() };
    let cs_bytes = [1u8, 2, 3, 4];
    let desc = ComputePipelineStateDesc {
        root_signature: ptr::null_mut(),
        cs: ShaderBytecode {
            shader_bytecode: cs_bytes.as_ptr().cast::<c_void>(),
            bytecode_length: cs_bytes.len(),
        },
        node_mask: 0,
        cached_pso: EMPTY_CACHED_PSO,
        flags: 0,
    };
    let mut out_pso: *mut c_void = ptr::null_mut();
    // SAFETY: `wide_name`, `desc`, and `out_pso` are valid pointers for `load_compute_pipeline`.
    let load_hr = unsafe {
        (lib.vtbl.load_compute_pipeline)(
            lib_ptr,
            wide_name.as_ptr().cast::<c_void>(),
            (&raw const desc).cast::<c_void>(),
            (&raw const iid).cast::<c_void>(),
            &raw mut out_pso,
        )
    };
    assert_eq!(load_hr, E_INVALIDARG);
    assert!(out_pso.is_null());

    // SAFETY: Release `lib_ptr`.
    unsafe {
        (lib.vtbl.release)(lib_ptr);
    }
}

fn isgn_blob(elements: &[(&[u8], u32, u32)]) -> Vec<u8> {
    let count = u32::try_from(elements.len()).expect("count fits u32");
    let mut out = Vec::new();
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    let mut names = Vec::new();
    for &(name, semantic_index, register) in elements {
        let name_at =
            u32::try_from(8 + 24 * elements.len() + names.len()).expect("offset fits u32");
        out.extend_from_slice(&name_at.to_le_bytes());
        out.extend_from_slice(&semantic_index.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&register.to_le_bytes());
        out.extend_from_slice(&[15, 15, 0, 0]);
        names.extend_from_slice(name);
        names.push(0);
    }
    out.extend_from_slice(&names);
    out
}

const fn elem_tuple(e: &Sm50IaInputElement) -> (u32, u32, u32, u32, u32) {
    (e.reg, e.slot, e.aligned_byte_offset, e.format, e.bits)
}

#[test]
fn input_signature_from_airconv_ir() {
    let isgn = isgn_blob(&[(b"POSITION", 0, 0), (b"TEXCOORD", 0, 1)]);
    let dxbc = build(&[(fourcc::ISGN, &isgn)]).expect("build DXBC container");
    let sig = parse_input_signature(&dxbc).expect("parse input sig");
    assert_eq!(sig.parameters.len(), 2);

    let elements = [
        InputElementDesc {
            semantic_name: c"POSITION".as_ptr().cast::<u8>(),
            semantic_index: 0,
            format: 6, // DXGI_FORMAT_R32G32B32_FLOAT (12 bytes, attribute_format = 30 / Float3)
            input_slot: 0,
            aligned_byte_offset: 0,
            input_slot_class: 0,
            instance_data_step_rate: 0,
        },
        InputElementDesc {
            semantic_name: c"texcoord".as_ptr().cast::<u8>(),
            semantic_index: 0,
            format: 16, // DXGI_FORMAT_R32G32_FLOAT (8 bytes, attribute_format = 29 / Float2)
            input_slot: 0,
            aligned_byte_offset: 0xffff_ffff,
            input_slot_class: 0,
            instance_data_step_rate: 0,
        },
    ];

    // SAFETY: Every `semantic_name` in `elements` is a valid C string literal.
    let (extracted, slot_mask) =
        unsafe { extract_input_layout(&dxbc, &elements) }.expect("valid input layout");
    assert_eq!(slot_mask, 1);
    assert_eq!(extracted.len(), 2);
    assert_eq!(
        elem_tuple(&extracted[0]),
        elem_tuple(&Sm50IaInputElement::new(0, 0, 0, 30, 0, 1))
    );
    assert_eq!(
        elem_tuple(&extracted[1]),
        elem_tuple(&Sm50IaInputElement::new(1, 0, 12, 29, 0, 1))
    );

    // SAFETY: `elements[0].semantic_name` is a valid C string literal.
    let missing_res = unsafe { extract_input_layout(&dxbc, &elements[..1]) };
    assert_eq!(missing_res.err(), Some(E_INVALIDARG));
}
