//! The D3D9 argument's validation, which runs before any lowering.

use core::ptr;

use airconv_ir::dxso::spec::{StageSpec, VariantKey, VsSpec};

use super::*;
use crate::{
    abi::{SM50_SHADER_D3D9, Sm50ShaderD3d9Data},
    entry::initialize::initialize,
};

fn dxso(tokens: &[u32]) -> Shader {
    let bytes: Vec<u8> = tokens.iter().flat_map(|t| t.to_le_bytes()).collect();
    // SAFETY: a null reflection is written nowhere.
    unsafe { initialize(&bytes, ptr::null_mut()) }.unwrap_or_else(|e| {
        panic!("{}", String::from_utf8_lossy(e.text()));
    })
}

fn node(stage: u32, vs: [u8; 6], ps: [u8; 22]) -> Sm50ShaderD3d9Data {
    Sm50ShaderD3d9Data {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_D3D9,
        stage,
        vs,
        ps,
    }
}

fn refusal(shader: &Shader, node: Option<&Sm50ShaderD3d9Data>) -> String {
    let head = node.map_or(ptr::null(), |n| ptr::from_ref(n).cast());
    // SAFETY: `head` is null or a single live node ending in a null `next`.
    let args = unsafe { ArgChain::walk(head) };
    match compile(shader, &args, c"main") {
        Ok(_) => panic!("compiled"),
        Err(e) => String::from_utf8_lossy(e.text()).into_owned(),
    }
}

const VS_2_0: [u32; 2] = [0xfffe_0200, 0x0000_ffff];

#[test]
fn dxso_without_d3d9_argument_is_refused() {
    assert_eq!(
        refusal(&dxso(&VS_2_0), None),
        "dxso: no D3D9 specialization"
    );
}

#[test]
fn d3d9_argument_with_lod_table_2_is_refused() {
    let (stage, mut vs, ps) = StageSpec::Vs(VsSpec::default()).to_wire();
    // `lod_table` is the last byte of the vertex half.
    vs[5] = 2;
    assert_eq!(
        refusal(&dxso(&VS_2_0), Some(&node(stage, vs, ps))),
        "dxso: bad D3D9 specialization"
    );
}

#[test]
fn dxso_stage_mismatch_is_refused() {
    let (stage, vs, ps) = StageSpec::Ps(VariantKey::default()).to_wire();
    assert_eq!(
        refusal(&dxso(&VS_2_0), Some(&node(stage, vs, ps))),
        "dxso: stage does not match the program"
    );
}
