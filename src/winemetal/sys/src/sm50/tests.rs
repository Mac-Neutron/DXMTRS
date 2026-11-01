use core::{ffi::c_char, mem::offset_of, slice};

use super::*;

#[unsafe(no_mangle)]
unsafe extern "C-unwind" fn SM50GetErrorMessage(
    error: u64,
    buffer: *mut c_char,
    buffer_size: usize,
) -> usize {
    let msg: &[u8] = match error {
        1 => b"shader compile failed",
        2 => &[b'x'; 300],
        _ => b"",
    };
    let n = msg.len().min(buffer_size.saturating_sub(1));
    if n > 0 {
        // SAFETY: `buffer` is writable for `buffer_size` > `n` bytes in `error_message`.
        let dst = unsafe { slice::from_raw_parts_mut(buffer.cast::<u8>(), n) };
        dst.copy_from_slice(&msg[..n]);
    }
    n
}

#[test]
fn error_message_as_cpp() {
    // SAFETY: the test stub above handles any `u64` error token without dereferencing it.
    let short = unsafe { error_message(1) };
    assert_eq!(short, "shader compile failed");

    // SAFETY: as above.
    let capped = unsafe { error_message(2) };
    assert_eq!(capped.len(), 255);
    assert!(capped.bytes().all(|b| b == b'x'));

    // SAFETY: as above.
    let empty = unsafe { error_message(0) };
    assert_eq!(empty, "");
}

#[test]
fn chain_layouts_as_cpp() {
    assert_eq!(
        (
            size_of::<MtlShaderReflection>(),
            align_of::<MtlShaderReflection>(),
            offset_of!(MtlShaderReflection, u),
            offset_of!(MtlShaderReflection, uav_slot_mask),
            offset_of!(MtlShaderReflection, writes_uav),
        ),
        (72, 8, 16, 32, 68)
    );
    assert_eq!(
        (
            size_of::<Sm50CompiledBitcode>(),
            align_of::<Sm50CompiledBitcode>(),
            offset_of!(Sm50CompiledBitcode, data),
            offset_of!(Sm50CompiledBitcode, size),
        ),
        (16, 8, 0, 8)
    );
    assert_eq!(
        (
            size_of::<Sm50IaInputElement>(),
            align_of::<Sm50IaInputElement>(),
            offset_of!(Sm50IaInputElement, reg),
            offset_of!(Sm50IaInputElement, slot),
            offset_of!(Sm50IaInputElement, aligned_byte_offset),
            offset_of!(Sm50IaInputElement, format),
            offset_of!(Sm50IaInputElement, bits),
        ),
        (20, 4, 0, 4, 8, 12, 16)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderCommonData>(),
            align_of::<Sm50ShaderCommonData>(),
            offset_of!(Sm50ShaderCommonData, r#type),
            offset_of!(Sm50ShaderCommonData, metal_version),
            offset_of!(Sm50ShaderCommonData, flags),
        ),
        (24, 8, 8, 12, 16)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderIaInputLayoutData>(),
            align_of::<Sm50ShaderIaInputLayoutData>(),
            offset_of!(Sm50ShaderIaInputLayoutData, index_buffer_format),
            offset_of!(Sm50ShaderIaInputLayoutData, slot_mask),
            offset_of!(Sm50ShaderIaInputLayoutData, num_elements),
            offset_of!(Sm50ShaderIaInputLayoutData, elements),
        ),
        (32, 8, 12, 16, 20, 24)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderPsoPixelShaderData>(),
            align_of::<Sm50ShaderPsoPixelShaderData>(),
            offset_of!(Sm50ShaderPsoPixelShaderData, sample_mask),
            offset_of!(Sm50ShaderPsoPixelShaderData, dual_source_blending),
            offset_of!(Sm50ShaderPsoPixelShaderData, disable_depth_output),
            offset_of!(Sm50ShaderPsoPixelShaderData, unorm_output_reg_mask),
            offset_of!(Sm50ShaderPsoPixelShaderData, pixel_formats),
        ),
        (56, 8, 12, 16, 17, 20, 24)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderPsoGeometryShaderData>(),
            align_of::<Sm50ShaderPsoGeometryShaderData>(),
            offset_of!(Sm50ShaderPsoGeometryShaderData, strip_topology),
        ),
        (16, 8, 12)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderPsoTessellatorData>(),
            align_of::<Sm50ShaderPsoTessellatorData>(),
            offset_of!(Sm50ShaderPsoTessellatorData, max_potential_tess_factor),
        ),
        (16, 8, 12)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderEmulateVertexStreamOutputData>(),
            align_of::<Sm50ShaderEmulateVertexStreamOutputData>(),
            offset_of!(Sm50ShaderEmulateVertexStreamOutputData, num_output_slots),
            offset_of!(Sm50ShaderEmulateVertexStreamOutputData, num_elements),
            offset_of!(Sm50ShaderEmulateVertexStreamOutputData, strides),
            offset_of!(Sm50ShaderEmulateVertexStreamOutputData, elements),
        ),
        (48, 8, 12, 16, 20, 40)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderGsPassThroughData>(),
            align_of::<Sm50ShaderGsPassThroughData>(),
            offset_of!(Sm50ShaderGsPassThroughData, u),
            offset_of!(Sm50ShaderGsPassThroughData, rasterization_disabled),
        ),
        (24, 8, 12, 16)
    );
    assert_eq!(
        (
            size_of::<Sm50ShaderRootSignatureData>(),
            align_of::<Sm50ShaderRootSignatureData>(),
            offset_of!(Sm50ShaderRootSignatureData, bytecode),
            offset_of!(Sm50ShaderRootSignatureData, bytecode_length),
        ),
        (32, 8, 16, 24)
    );
    assert_eq!(get_argument_index(SM50_BINDING_CONSTANT_BUFFER, 3), 3);
    assert_eq!(get_argument_index(SM50_BINDING_SAMPLER, 5), 5);
    assert_eq!(get_argument_index(SM50_BINDING_SRV, 2), 18);
    assert_eq!(get_argument_index(SM50_BINDING_UAV, 4), 148);
    let uav_arg = MtlSm50ShaderArgument {
        r#type: SM50_BINDING_UAV,
        sm50_binding_slot: 4,
        flags: MTL_SM50_SHADER_ARGUMENT_BUFFER | MTL_SM50_SHADER_ARGUMENT_WRITE_ACCESS,
        structure_ptr_offset: 0,
    };
    assert_eq!(uav_arg.argument_index(), 148);
}

#[test]
fn ia_input_element_packs_as_c() {
    let per_vertex = Sm50IaInputElement::new(2, 1, 16, 30, 0, 0);
    assert_eq!(per_vertex.bits, 0);
    assert_eq!((per_vertex.step_function(), per_vertex.step_rate()), (0, 0));

    let per_instance = Sm50IaInputElement::new(3, 0, 32, 12, 1, 7);
    assert_eq!(per_instance.bits, 1 | (7 << 1));
    assert_eq!(
        (per_instance.step_function(), per_instance.step_rate()),
        (1, 7)
    );

    let high_rate = Sm50IaInputElement::new(0, 0, 0, 0, 0, 0x7fff_ffff);
    assert_eq!(high_rate.bits, 0xffff_fffe);
    assert_eq!(
        (high_rate.step_function(), high_rate.step_rate()),
        (0, 0x7fff_ffff)
    );
}
