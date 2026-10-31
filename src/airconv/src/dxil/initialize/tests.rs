use airconv_ir::{binding::FunctionInput, container::Container, dxil_part};

use super::*;

/// `cache.cs.dxil` against the C++'s handle.
///
/// Read once from `SM50ShaderInternal` and its `DXILShader` by a scratch dump linked against the
/// baseline archives (0439fa9).
#[test]
fn cache_cs_reads_as_the_cxx() {
    let bytes = include_bytes!("../../../ir/tests/data/cache.cs.dxil");
    let container = Container::parse(bytes).unwrap();
    let part = dxil_part::find(&container).unwrap().unwrap();
    let shader = initialize(&part, 7).unwrap_or_else(|e| {
        panic!("{}", String::from_utf8_lossy(e.text()));
    });
    let e = &shader.entry;
    assert_eq!(
        (e.kind, e.sm_major, e.sm_minor, e.name.as_slice()),
        (shader_kind::COMPUTE, 6, 6, &b"csmain"[..])
    );
    assert_eq!(
        (shader.program_type, shader.decls.threadgroup_size),
        (5, [64, 1, 1])
    );
    let fs = &shader.io.func_signature;
    assert_eq!(fs.inputs(), [FunctionInput::ThreadPositionInGrid]);
    assert_eq!(fs.max_work_group_size(), 64);
    assert_eq!(shader.io.signature_handlers.len(), 1);
    let uav = shader.info.uav_map.get(&0).unwrap();
    assert_eq!(
        (
            uav.range.range_id,
            uav.range.lower_bound,
            uav.range.size,
            uav.range.space
        ),
        (0, 0, 1, 0)
    );
    assert_eq!(uav.resource_type, ResourceType::NonApplicable);
    assert_eq!(uav.scaler_type, ScalerDataType::Float);
    assert!(uav.flags.contains(UavFlags::READ) && uav.flags.contains(UavFlags::WRITTEN));
    assert!(!uav.flags.contains(UavFlags::ATOMIC_VIEW));
    assert_eq!(
        (uav.structure_stride, uav.arg_index, uav.arg_counter_index),
        (4, u32::MAX, u32::MAX)
    );
    assert_eq!(
        (shader.bitcode.len(), shader.capture_hash, shader.gs_locks),
        (1460, 7, 0)
    );
    assert_eq!(
        (shader.lane_index_arg, shader.lane_count_arg),
        (u32::MAX, u32::MAX)
    );
    assert!(!shader.quads_2d && !shader.include_helper_lanes && shader.writes_uav);
    assert_eq!(
        shader.reflection(),
        Reflection {
            constant_buffer_table_bind_index: u32::MAX,
            argument_buffer_bind_index: u32::MAX,
            num_constant_buffers: 0,
            num_arguments: 0,
            stage: StageReflection::Compute {
                threadgroup_size: [64, 1, 1]
            },
            constant_buffer_slot_mask: 0,
            sampler_slot_mask: 0,
            uav_slot_mask: 0,
            srv_slot_mask_hi: 0,
            srv_slot_mask_lo: 0,
            num_output_element: 0,
            argument_table_qwords: 0,
            writes_uav: 1,
        }
    );
}

#[test]
fn a_heap_range_reads_its_properties() {
    // A typed RWBuffer<float> with a counter, globally coherent: kind 10, UAV, bits 14 and 15.
    let uav = heap_resource([0xa | 1 << 12 | 1 << 14 | 1 << 15, 9]);
    assert_eq!(
        (
            uav.cls,
            uav.kind,
            uav.space,
            uav.lower_bound,
            uav.size,
            uav.element_type
        ),
        (
            resource_class::UAV,
            10,
            HEAP_SPACE,
            0,
            u32::MAX,
            component_type::F32
        )
    );
    for flag in [
        ResourceFlags::HEAP,
        ResourceFlags::HAS_COUNTER,
        ResourceFlags::GLOBALLY_COHERENT,
    ] {
        assert!(uav.flags.contains(flag));
    }
    assert!(!uav.flags.contains(ResourceFlags::COMPARISON_SAMPLER));
    // A comparison sampler (kind 14, bit 15), a structured SRV's stride, a constant buffer's size.
    let sampler = heap_resource([0xe | 1 << 15, 0]);
    assert_eq!(sampler.cls, resource_class::SAMPLER);
    assert!(sampler.flags.contains(ResourceFlags::COMPARISON_SAMPLER));
    assert!(!sampler.flags.contains(ResourceFlags::HAS_COUNTER));
    let srv = heap_resource([0xc, 16]);
    assert_eq!(
        (srv.cls, srv.stride, srv.element_type),
        (resource_class::SRV, 16, 0)
    );
    let cb = heap_resource([0xd, 256]);
    assert_eq!((cb.cls, cb.cbuffer_size), (resource_class::CBUFFER, 256));
}
