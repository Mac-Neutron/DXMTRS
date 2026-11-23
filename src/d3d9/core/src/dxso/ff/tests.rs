//! Unit tests for fixed-function shader key layout and stage constant tracking.

use mtld3d_types::{
    D3DTA_CURRENT, D3DTA_DIFFUSE, D3DTA_TEXTURE, D3DTOP_DISABLE, D3DTOP_MODULATE, D3DTOP_SELECTARG1,
};

use super::{FfPsKey, FfStage, FfStageFlags};

/// D3D enum constant at the key's narrow width.
fn narrow(v: u32) -> u8 {
    u8::try_from(v).expect("D3D9 fixed-function enum value <= u8::MAX")
}

fn stage_disable() -> FfStage {
    FfStage {
        color_op: narrow(D3DTOP_DISABLE),
        ..FfStage::default()
    }
}

fn default_ps_key() -> FfPsKey {
    FfPsKey {
        stages: [stage_disable(); 8],
        specular_add: false,
        tt_projected_mask: 0,
    }
}

#[test]
fn packed_stage_preserves_legacy_current_hash_stream_and_key_sizes() {
    use std::hash::{Hash, Hasher};

    use super::FfStageResult;
    #[derive(Hash)]
    struct LegacyStage {
        color_op: u8,
        color_arg1: u8,
        color_arg2: u8,
        alpha_op: u8,
        alpha_arg1: u8,
        alpha_arg2: u8,
        has_texture: bool,
    }
    #[derive(Hash)]
    struct LegacyKey {
        stages: [LegacyStage; 8],
        specular_add: bool,
        tt_projected_mask: u8,
    }
    #[derive(Default)]
    struct HashWrites(Vec<u8>);
    impl Hasher for HashWrites {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.extend_from_slice(bytes);
        }
    }
    fn writes(value: &impl Hash) -> Vec<u8> {
        let mut h = HashWrites::default();
        value.hash(&mut h);
        h.0
    }
    assert_eq!(size_of::<FfStage>(), 7);
    assert_eq!(size_of::<FfPsKey>(), 58);
    assert_eq!(align_of::<FfPsKey>(), 1);
    for mask in 0u8..=u8::MAX {
        let stages = std::array::from_fn(|i| FfStage {
            color_op: narrow(D3DTOP_MODULATE),
            color_arg1: narrow(D3DTA_TEXTURE),
            color_arg2: narrow(D3DTA_DIFFUSE),
            alpha_op: narrow(D3DTOP_SELECTARG1),
            alpha_arg1: narrow(D3DTA_CURRENT),
            alpha_arg2: narrow(D3DTA_TEXTURE),
            flags: if mask & (1 << i) != 0 {
                FfStageFlags::HAS_TEXTURE
            } else {
                FfStageFlags::empty()
            },
        });
        let mut key = FfPsKey {
            stages,
            specular_add: mask & 1 != 0,
            tt_projected_mask: mask,
        };
        let legacy = LegacyKey {
            stages: key.stages.each_ref().map(|s| LegacyStage {
                color_op: s.color_op,
                color_arg1: s.color_arg1,
                color_arg2: s.color_arg2,
                alpha_op: s.alpha_op,
                alpha_arg1: s.alpha_arg1,
                alpha_arg2: s.alpha_arg2,
                has_texture: s.has_texture(),
            }),
            specular_add: key.specular_add,
            tt_projected_mask: key.tt_projected_mask,
        };
        assert_eq!(writes(&key), writes(&legacy), "CURRENT stream, mask {mask}");
        let current = key.clone();
        key.stages[0].set_result(FfStageResult::Temp);
        assert_ne!(key, current);
        assert_ne!(writes(&key), writes(&current));
        assert_eq!(key.stages[0].has_texture(), mask & 1 != 0);
    }
}

#[test]
fn modulate_alpha_add_color_constant_rows_follow_effective_binary_arguments() {
    use mtld3d_types::{D3DTA_CONSTANT, D3DTA_CURRENT, D3DTOP_MODULATEALPHA_ADDCOLOR};
    for (arg1, arg2) in [
        (D3DTA_CONSTANT, D3DTA_CURRENT),
        (D3DTA_CURRENT, D3DTA_CONSTANT),
    ] {
        let mut key = default_ps_key();
        key.stages[0] = FfStage {
            color_op: narrow(D3DTOP_SELECTARG1),
            color_arg1: narrow(D3DTA_DIFFUSE),
            alpha_op: narrow(D3DTOP_SELECTARG1),
            alpha_arg1: narrow(D3DTA_DIFFUSE),
            ..FfStage::default()
        };
        key.stages[1] = FfStage {
            color_op: narrow(D3DTOP_MODULATEALPHA_ADDCOLOR),
            color_arg1: narrow(arg1),
            color_arg2: narrow(arg2),
            alpha_op: narrow(D3DTOP_SELECTARG1),
            alpha_arg1: narrow(D3DTA_CURRENT),
            ..FfStage::default()
        };
        assert_eq!(key.constant_rows(), 3);
        assert!(key.stages[1].reads_argument(D3DTA_CONSTANT));
        // Explicit missing textures short-circuit the complete color operation.
        if arg1 == D3DTA_CONSTANT {
            key.stages[1].color_arg2 = narrow(D3DTA_TEXTURE);
        } else {
            key.stages[1].color_arg1 = narrow(D3DTA_TEXTURE);
        }
        assert_eq!(key.constant_rows(), 0);
        assert!(!key.stages[1].reads_argument(D3DTA_CONSTANT));
    }
}

#[test]
fn modulate_color_add_alpha_constant_rows_follow_effective_binary_arguments() {
    use mtld3d_types::{D3DTA_CONSTANT, D3DTA_CURRENT, D3DTOP_MODULATECOLOR_ADDALPHA};
    for (arg1, arg2) in [
        (D3DTA_CONSTANT, D3DTA_CURRENT),
        (D3DTA_CURRENT, D3DTA_CONSTANT),
    ] {
        let mut key = default_ps_key();
        key.stages[0] = FfStage {
            color_op: narrow(D3DTOP_SELECTARG1),
            color_arg1: narrow(D3DTA_DIFFUSE),
            alpha_op: narrow(D3DTOP_SELECTARG1),
            alpha_arg1: narrow(D3DTA_DIFFUSE),
            ..FfStage::default()
        };
        key.stages[1] = FfStage {
            color_op: narrow(D3DTOP_MODULATECOLOR_ADDALPHA),
            color_arg1: narrow(arg1),
            color_arg2: narrow(arg2),
            alpha_op: narrow(D3DTOP_SELECTARG1),
            alpha_arg1: narrow(D3DTA_CURRENT),
            ..FfStage::default()
        };
        assert_eq!(key.constant_rows(), 3);
        assert!(key.stages[1].reads_argument(D3DTA_CONSTANT));
        // Explicit missing textures short-circuit the complete color operation.
        if arg1 == D3DTA_CONSTANT {
            key.stages[1].color_arg2 = narrow(D3DTA_TEXTURE);
        } else {
            key.stages[1].color_arg1 = narrow(D3DTA_TEXTURE);
        }
        assert_eq!(key.constant_rows(), 0);
        assert!(!key.stages[1].reads_argument(D3DTA_CONSTANT));
    }
}
