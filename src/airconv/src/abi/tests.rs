use core::{ffi::c_void, ptr};

use super::*;

#[test]
fn ia_input_element_bitfields_lsb_first() {
    // `step_function : 1` takes bit 0 and `step_rate : 31` bits 1-31, as clang lays them out.
    let element = |bits| Sm50IaInputElement {
        reg: 0,
        slot: 0,
        aligned_byte_offset: 0,
        format: 0,
        bits,
    };
    let per_instance = element(1 | 7 << 1);
    assert_eq!(
        (per_instance.step_function(), per_instance.step_rate()),
        (1, 7)
    );
    let all_rate = element(0xffff_fffe);
    assert_eq!(
        (all_rate.step_function(), all_rate.step_rate()),
        (0, 0x7fff_ffff)
    );
}

#[test]
fn arg_chain_first_of_each_type_wins() {
    let tessellator = Sm50ShaderPsoTessellatorData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_PSO_TESSELLATOR,
        max_potential_tess_factor: 8,
    };
    let unknown = Sm50ShaderCompilationArgumentData {
        next: ptr::from_ref(&tessellator).cast_mut().cast::<c_void>(),
        r#type: 77,
    };
    let common_320 = Sm50ShaderCommonData {
        next: ptr::from_ref(&unknown).cast_mut().cast::<c_void>(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_320,
        flags: 0,
    };
    let geometry = Sm50ShaderPsoGeometryShaderData {
        next: ptr::from_ref(&common_320).cast_mut().cast::<c_void>(),
        r#type: SM50_SHADER_PSO_GEOMETRY_SHADER,
        strip_topology: 1,
    };
    let common_310 = Sm50ShaderCommonData {
        next: ptr::from_ref(&geometry).cast_mut().cast::<c_void>(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: SM50_SHADER_FLAG_DEFUSE_FMA,
    };

    // SAFETY: every node is a live local of its type, and the chain ends in a null `next`.
    let chain = unsafe { ArgChain::walk(ptr::from_ref(&common_310).cast()) };
    let common = chain.common.map(|c| (c.metal_version, c.flags));
    assert_eq!(
        common,
        Some((SM50_SHADER_METAL_310, SM50_SHADER_FLAG_DEFUSE_FMA))
    );
    assert_eq!(chain.geometry.map(|g| g.strip_topology), Some(1));
    assert_eq!(
        chain.tessellator.map(|t| t.max_potential_tess_factor),
        Some(8)
    );
    assert!(chain.stream_output.is_none());
    assert!(chain.pixel.is_none());
    assert!(chain.ia_layout.is_none());
    assert!(chain.gs_pass_through.is_none());
    assert!(chain.root_signature.is_none());
    assert!(chain.root_signature2.is_none());

    // SAFETY: a null chain is allowed and reads nothing.
    let empty = unsafe { ArgChain::walk(ptr::null()) };
    assert!(empty.common.is_none() && empty.tessellator.is_none());
}
