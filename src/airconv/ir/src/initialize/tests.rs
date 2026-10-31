//! DXBC Initialize against the C++'s answers on the same shaders.
//!
//! Read once by scratch programs over the baseline's sources: its reflection bytes, argument
//! tables and handle.

use std::{env, fs, path::PathBuf};

use super::*;
use crate::binding::{TableField, TextureKind};

fn initialize(bytes: &[u8]) -> DxbcShader {
    dxbc(&Container::parse(bytes).unwrap()).unwrap()
}

#[test]
fn sync_dxbc_binds_its_uav() {
    let mut s = initialize(include_bytes!("../../tests/data/sync.dxbc"));
    let uav = ShaderArgument {
        ty: BindingType::Uav,
        slot: 0,
        flags: 0xc22,
        structure_ptr_offset: 0,
    };
    assert_eq!(
        (s.args_cbuffer.as_slice(), s.args.as_slice()),
        (&[][..], &[uav][..])
    );
    let [
        TableField::Texture(texture),
        TableField::IndirectConstant(meta),
    ] = s.binding_table.fields()
    else {
        panic!("{:?}", s.binding_table.fields());
    };
    assert_eq!(
        (texture.location_index, texture.arg_name.as_str()),
        (512, "u0")
    );
    assert_eq!(
        (texture.ty.kind, texture.memory_access),
        (TextureKind::TextureBuffer, MemoryAccess::ReadWrite)
    );
    assert_eq!(
        (meta.location_index, meta.arg_name.as_str()),
        (513, "meta_u0")
    );
    let expected = Reflection {
        constant_buffer_table_bind_index: u32::MAX,
        argument_buffer_bind_index: 30,
        num_constant_buffers: 0,
        num_arguments: 1,
        stage: StageReflection::Compute {
            threadgroup_size: [64, 1, 1],
        },
        constant_buffer_slot_mask: 0,
        sampler_slot_mask: 0,
        uav_slot_mask: 1,
        srv_slot_mask_hi: 0,
        srv_slot_mask_lo: 0,
        num_output_element: 0,
        argument_table_qwords: 2,
        writes_uav: 1,
    };
    assert_eq!(s.reflect().unwrap(), expected);
    assert_eq!(s.io.func_signature.max_work_group_size(), 64);
}

#[test]
fn patch_hs_fits_its_threadgroup_only_when_reflected() {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: patch.hs is not initialized");
        return;
    };
    let bytes = fs::read(PathBuf::from(base).join("../corpus/C5/patch.hs.dxbc")).unwrap();
    let mut s = initialize(&bytes);
    // Unreflected, the handle keeps the declared values (the C++ fitted them under `pRefl`).
    assert_eq!(s.cfg.decls.hull_maximum_threads_per_patch, 3);
    let r = s.reflect().unwrap();
    let hull = StageReflection::Hull {
        threads_per_patch: 8,
        partition: 1,
        max_factor: 64.0,
        output_primitive: 3,
    };
    assert_eq!((r.stage, r.num_output_element), (hull, 1));
    assert_eq!(s.cfg.decls.hull_maximum_threads_per_patch, 8);
}
