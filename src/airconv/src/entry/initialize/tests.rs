//! Initialize's reflection and entry against the C++'s answers on the test shaders.
//!
//! Read once by a scratch program over the C API linked against the baseline's archives, the
//! reflection filled with 0xA5 first, as `dxil-translate` does.

use core::{ffi::CStr, mem::MaybeUninit};

use super::*;

/// Initializes `bytes` over a reflection of 0xA5 bytes and answers the shader and the bytes.
fn initialize_over_a5(bytes: &[u8]) -> (Shader, [u8; 72]) {
    let mut refl = MaybeUninit::<MtlShaderReflection>::uninit();
    // SAFETY: `refl` holds one reflection's bytes.
    unsafe { refl.as_mut_ptr().cast::<u8>().write_bytes(0xa5, 72) };
    // SAFETY: `refl` is writable for one reflection.
    let shader = unsafe { initialize(bytes, refl.as_mut_ptr()) }.unwrap_or_else(|e| {
        panic!("{}", String::from_utf8_lossy(e.text()));
    });
    // SAFETY: every byte was written above, by the fill or by Initialize.
    let written = unsafe { refl.as_ptr().cast::<[u8; 72]>().read() };
    (shader, written)
}

/// The C++'s reflection bytes, as words of four little-endian bytes.
fn words(hex: &str) -> Vec<u8> {
    hex.split(' ')
        .flat_map(|w| (0..4).map(move |i| u8::from_str_radix(&w[2 * i..2 * i + 2], 16).unwrap()))
        .collect()
}

#[test]
fn reflection_writes_only_stage_fields() {
    // A pixel shader's part is 8 of the union's 12 bytes: `ThreadgroupSize[2]` and the hull's
    // `ThreadsPerPatch` keep the fill.
    let (_, pixel) = initialize_over_a5(include_bytes!("../../../ir/tests/data/render.ps.dxbc"));
    let cxx = "ffffffff ffffffff 00000000 00000000 03000000 00000000 a5a5a5a5 00000000 00000000 \
               00000000 00000000 00000000 00000000 00000000 02000000 a5a5a5a5 00000000 00000000";
    assert_eq!(pixel.as_slice(), words(cxx));
    let (shader, compute) = initialize_over_a5(include_bytes!("../../../ir/tests/data/sync.dxbc"));
    let cxx = "ffffffff 1e000000 00000000 01000000 40000000 01000000 01000000 00000000 01000000 \
               00000000 00000000 00000000 00000000 00000000 00000000 a5a5a5a5 02000000 01000000";
    assert_eq!(compute.as_slice(), words(cxx));
    let (cbs, args) = shader.arguments();
    let arg = args.first().unwrap();
    assert_eq!(cbs.len(), 0);
    assert_eq!(
        (
            arg.r#type,
            arg.sm50_binding_slot,
            arg.flags,
            arg.structure_ptr_offset
        ),
        (3, 0, 0xc22, 0)
    );
}

#[test]
fn entry_view_is_the_signatures_rows() {
    let (shader, _) = initialize_over_a5(include_bytes!("../../../ir/tests/data/render.ps.dxbc"));
    let info = entry_info(&shader);
    assert_eq!(
        (info.is_dxil, info.stage, info.num_inputs, info.num_outputs),
        (0, 0, 2, 2)
    );
    assert!(info.resources.is_null());
    // A shader without elements answers null arrays, as the C++'s empty vectors did.
    let (compute, _) = initialize_over_a5(include_bytes!("../../../ir/tests/data/sync.dxbc"));
    let none = entry_info(&compute);
    assert_eq!((none.stage, none.num_inputs, none.num_outputs), (5, 0, 0));
    assert!(none.inputs.is_null() && none.outputs.is_null());
    // SAFETY: the array holds `num_inputs` elements, alive with `shader`.
    let inputs = unsafe { core::slice::from_raw_parts(info.inputs, 2) };
    // SAFETY: the array holds `num_outputs` elements, alive with `shader`.
    let outputs = unsafe { core::slice::from_raw_parts(info.outputs, 2) };
    let elements = inputs.iter().chain(outputs);
    let expected = [
        (3, 0, c"SV_Position", 0),
        (0, 1, c"COLOR", 0),
        (0, 0, c"SV_Target", 0),
        (0, 1, c"SV_Target", 1),
    ];
    for (e, (kind, start_row, name, index)) in elements.zip(expected) {
        // SAFETY: the name is a C string, alive with `shader`.
        let name_got = unsafe { CStr::from_ptr(e.name) };
        // SAFETY: the index array holds one index, alive with `shader`.
        let index_got = unsafe { *e.semantic_indices };
        let got = (name_got, index_got);
        assert_eq!(
            (e.kind, e.rows, e.start_row, e.num_semantic_indices),
            (kind, 1, start_row, 1)
        );
        assert_eq!(got, (name, index));
    }
}

#[test]
fn interior_nul_name_is_an_error() {
    let parameter = Parameter {
        stream: 0,
        semantic_name: b"SV_\0Target".to_vec(),
        semantic_index: 0,
        system_value: 0,
        component_type: 3,
        register: 0,
        mask: 15,
        rw_mask: 0,
        min_precision: 0,
    };
    assert_eq!(
        c_name(&parameter).err().map(|e| e.text().to_vec()),
        Some(NAME_HOLDS_NUL.to_vec())
    );
}

#[test]
fn dxil_reflection_is_written_whole() {
    // The C++ zeroes the whole reflection first (`*refl = {}`): no byte keeps the fill. Read once
    // from the C++ over the C API, as above.
    let (shader, compute) =
        initialize_over_a5(include_bytes!("../../../ir/tests/data/cache.cs.dxil"));
    let cxx = "ffffffff ffffffff 00000000 00000000 40000000 01000000 01000000 00000000 00000000 \
               00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000 01000000";
    assert_eq!(compute.as_slice(), words(cxx));
    let (cbs, args) = shader.arguments();
    assert!(cbs.is_empty() && args.is_empty());
    let info = entry_info(&shader);
    assert_eq!(
        (
            info.is_dxil,
            info.stage,
            info.num_inputs,
            info.num_outputs,
            info.num_resources
        ),
        (1, 5, 0, 0, 1)
    );
    assert!(info.inputs.is_null() && info.outputs.is_null());
    // SAFETY: the array holds `num_resources` resources, alive with `shader`.
    let r = unsafe { &*info.resources };
    assert_eq!(
        (r.cls, r.space, r.lower_bound, r.size, r.heap),
        (1, 0, 0, 1, 0)
    );
}

#[test]
fn dxil_entry_view_points_at_each_elements_indices() {
    let element = |name: &[u8], kind, indices: Vec<u32>| crate::dxil::types::SignatureElement {
        id: 0,
        name: name.to_vec(),
        rows: u32::try_from(indices.len()).unwrap(),
        semantic_indices: indices,
        ty: 9,
        kind,
        interpolation: 0,
        cols: 4,
        start_row: -1,
        start_col: -1,
    };
    let mut entry = EntryInfo {
        kind: 0,
        inputs: vec![
            element(b"TEXCOORD", 0, vec![5, 6]),
            element(b"A", 0, vec![]),
        ],
        outputs: vec![element(b"SV_Target", 16, vec![2])],
        ..EntryInfo::default()
    };
    let view = dxil_entry_view(&entry).unwrap_or_else(|_| panic!("view"));
    let info = view.info();
    assert_eq!((info.is_dxil, info.num_inputs, info.num_outputs), (1, 2, 1));
    assert!(info.resources.is_null());
    let elements = view.inputs.iter().chain(&view.outputs);
    let expected: [(&CStr, &[u32]); 3] =
        [(c"TEXCOORD", &[5, 6]), (c"A", &[]), (c"SV_Target", &[2])];
    for (e, (name, indices)) in elements.zip(expected) {
        // SAFETY: the name is a C string, alive with `view`.
        assert_eq!(unsafe { CStr::from_ptr(e.name) }, name);
        let got = if e.semantic_indices.is_null() {
            &[][..]
        } else {
            // SAFETY: the element's indices, alive with `view`.
            unsafe { core::slice::from_raw_parts(e.semantic_indices, indices.len()) }
        };
        assert_eq!(
            (got, e.num_semantic_indices, e.start_row),
            (indices, e.rows, -1)
        );
    }
    entry.outputs[0].name = b"SV_\0Target".to_vec();
    assert_eq!(
        dxil_entry_view(&entry).err().map(|e| e.text().to_vec()),
        Some(NAME_HOLDS_NUL.to_vec())
    );
}

#[test]
fn unreadable_bitcode_answers_the_readers_text() {
    // The DXIL part's bitcode magic broken (the last one: DXC's STAT part holds a program too):
    // the shim's `parseBitcodeFile`, never the C API's reader (which would exit), answers the
    // C++'s text, read once from the C++ over the C API.
    let mut bytes = include_bytes!("../../../ir/tests/data/cache.cs.dxil").to_vec();
    let magic = bytes.windows(4).rposition(|w| w == b"BC\xc0\xde").unwrap();
    bytes[magic] = b'X';
    assert_eq!(
        initialize_err(&bytes),
        b"DXIL: can't read the bitcode: file doesn't start with bitcode header"
    );
}

fn initialize_err(bytes: &[u8]) -> Vec<u8> {
    // SAFETY: a null reflection is written nowhere.
    match unsafe { initialize(bytes, ptr::null_mut()) } {
        Ok(_) => panic!("initialized"),
        Err(e) => e.text().to_vec(),
    }
}
