use core::{ffi::c_void, mem::zeroed, ptr};

use airconv_ir::container::{self, fourcc};

use super::{
    D3D12_PRIMITIVE_TOPOLOGY_TYPE_PATCH, E_INVALIDARG, E_NOTIMPL, compute_max_tess_factor,
    compute_patch_per_group, embedded_root_signature, has_bytecode, validate_shader_stages,
};
use crate::abi::pipeline::{GraphicsPipelineStateDesc, ShaderBytecode};

const fn fake_ptr(val: usize) -> *const c_void {
    ptr::without_provenance(val)
}

#[test]
fn has_bytecode_requires_non_null_and_non_zero_length() {
    assert!(!has_bytecode(&ShaderBytecode {
        shader_bytecode: core::ptr::null(),
        bytecode_length: 0,
    }));
    assert!(!has_bytecode(&ShaderBytecode {
        shader_bytecode: core::ptr::null(),
        bytecode_length: 16,
    }));
    assert!(!has_bytecode(&ShaderBytecode {
        shader_bytecode: fake_ptr(0x1000),
        bytecode_length: 0,
    }));
    assert!(has_bytecode(&ShaderBytecode {
        shader_bytecode: fake_ptr(0x1000),
        bytecode_length: 4,
    }));
}

#[test]
fn validate_shader_stages_rejects_partial_topology_and_gs_ts_combos() {
    // SAFETY: `GraphicsPipelineStateDesc` is an all-zero-valid POD C struct.
    let mut desc = unsafe { zeroed::<GraphicsPipelineStateDesc>() };
    assert_eq!(validate_shader_stages(&desc), Ok(false));

    // Zero-length HS pointer is treated as absent, so non-patch PSO succeeds.
    desc.hs.shader_bytecode = fake_ptr(0x1000);
    desc.hs.bytecode_length = 0;
    assert_eq!(validate_shader_stages(&desc), Ok(false));

    // Patch topology type without HS/DS is rejected.
    desc.primitive_topology_type = D3D12_PRIMITIVE_TOPOLOGY_TYPE_PATCH;
    assert_eq!(validate_shader_stages(&desc), Err(E_INVALIDARG));

    desc.primitive_topology_type = 3; // D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE
    desc.hs.bytecode_length = 4;
    assert_eq!(validate_shader_stages(&desc), Err(E_INVALIDARG));

    desc.hs.shader_bytecode = core::ptr::null();
    desc.hs.bytecode_length = 0;
    desc.ds.shader_bytecode = fake_ptr(0x2000);
    desc.ds.bytecode_length = 4;
    assert_eq!(validate_shader_stages(&desc), Err(E_INVALIDARG));

    // Valid HS+DS with non-patch topology (triangle or undefined) must fail.
    desc.hs.shader_bytecode = fake_ptr(0x1000);
    desc.hs.bytecode_length = 4;
    assert_eq!(validate_shader_stages(&desc), Err(E_INVALIDARG));
    desc.primitive_topology_type = 0;
    assert_eq!(validate_shader_stages(&desc), Err(E_INVALIDARG));

    // Valid HS+DS with D3D12_PRIMITIVE_TOPOLOGY_TYPE_PATCH passes.
    desc.primitive_topology_type = D3D12_PRIMITIVE_TOPOLOGY_TYPE_PATCH;
    assert_eq!(validate_shader_stages(&desc), Ok(true));

    desc.gs.shader_bytecode = fake_ptr(0x3000);
    desc.gs.bytecode_length = 4;
    assert_eq!(validate_shader_stages(&desc), Err(E_NOTIMPL));

    desc.gs.shader_bytecode = core::ptr::null();
    desc.gs.bytecode_length = 0;
    desc.stream_output.num_entries = 1;
    assert_eq!(validate_shader_stages(&desc), Err(E_NOTIMPL));
}

#[test]
fn tess_factor_and_patch_per_group_helpers() {
    // Domain max alone governs max_potential_tess_factor (`hull_max` does not clamp it).
    assert_eq!(compute_max_tess_factor(0, true), 64);
    assert_eq!(compute_max_tess_factor(12, true), 12);
    assert_eq!(compute_max_tess_factor(16, true), 16);
    assert_eq!(compute_max_tess_factor(100, true), 64);
    assert_eq!(compute_max_tess_factor(64, false), 8);
    assert_eq!(compute_max_tess_factor(0, false), 8);
    assert_eq!(compute_max_tess_factor(4, false), 4);

    assert_eq!(compute_patch_per_group(0), 32);
    assert_eq!(compute_patch_per_group(1), 32);
    assert_eq!(compute_patch_per_group(4), 8);
    assert_eq!(compute_patch_per_group(16), 2);
    assert_eq!(compute_patch_per_group(32), 1);
    assert_eq!(compute_patch_per_group(64), 1);
}

#[test]
fn embedded_root_signature_picks_first_rts0_container() {
    let vs = container::build(&[(fourcc::SHDR, b"vs")]).expect("vs container");
    let hs = container::build(&[(fourcc::SHEX, b"hs")]).expect("hs container");
    let ds =
        container::build(&[(fourcc::RTS0, b"rs"), (fourcc::SHEX, b"ds")]).expect("ds container");
    let ps = container::build(&[(fourcc::RTS0, b"psrs")]).expect("ps container");

    let picked = embedded_root_signature(&[&vs, &hs, &ds, &ps]);
    assert_eq!(picked, Some(ds.as_slice()));

    assert_eq!(embedded_root_signature(&[&vs, &hs]), None);
}
