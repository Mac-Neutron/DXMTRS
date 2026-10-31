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
fn a_dxil_part_is_not_ported_yet() {
    let error = initialize_err(include_bytes!("../../../ir/tests/data/cache.cs.dxil"));
    assert_eq!(error, b"not ported");
}

fn initialize_err(bytes: &[u8]) -> Vec<u8> {
    // SAFETY: a null reflection is written nowhere.
    match unsafe { initialize(bytes, ptr::null_mut()) } {
        Ok(_) => panic!("initialized"),
        Err(e) => e.text().to_vec(),
    }
}
