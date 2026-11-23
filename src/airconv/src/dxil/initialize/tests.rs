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

#[test]
fn pull_mode_elements_collects_constant_eval_ids_and_rejects_dynamic() {
    use crate::llvm::Builder;

    fn ok<T>(result: Result<T, Error>) -> T {
        result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
    }

    let cx = Context::new();
    let m = module::initialize(&cx, c"eval.dxil");
    let tcx = cx.tcx();
    let i8_ty = tcx.int(8);
    let i32_ty = tcx.int(32);
    let f32_ty = tcx.float();

    let main_fn = ok(m.add_function(b"main", tcx.function(tcx.void(), &[i32_ty], false)));
    let bb = main_fn.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(bb);

    let snapped = ok(m.get_or_insert_function(
        b"dx.op.evalSnapped.f32",
        tcx.function(
            f32_ty,
            &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty, i32_ty],
            false,
        ),
        &[],
    ));
    let sample_idx = ok(m.get_or_insert_function(
        b"dx.op.evalSampleIndex.f32",
        tcx.function(f32_ty, &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty], false),
        &[],
    ));
    let centroid = ok(m.get_or_insert_function(
        b"dx.op.evalCentroid.f32",
        tcx.function(f32_ty, &[i32_ty, i32_ty, i32_ty, i8_ty], false),
        &[],
    ));

    let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
    let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);

    let _ = b.call(
        snapped,
        &[c32(op::EVAL_SNAPPED), c32(0), c32(0), c8(0), c32(0), c32(0)],
    );
    let _ = b.call(
        sample_idx,
        &[c32(op::EVAL_SAMPLE_INDEX), c32(2), c32(0), c8(1), c32(1)],
    );
    let _ = b.call(centroid, &[c32(op::EVAL_CENTROID), c32(5), c32(0), c8(0)]);
    b.ret_void();

    let collected =
        pull_mode_elements(&m).unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())));
    assert_eq!(collected, BTreeSet::from([0, 2, 5]));

    // A non-constant signature element ID is rejected.
    let dyn_id = main_fn.param(tcx, 0).unwrap();
    let bad = b.call(centroid, &[c32(op::EVAL_CENTROID), dyn_id, c32(0), c8(0)]);
    let _ = bad;
    let Err(err) = pull_mode_elements(&m) else {
        panic!("expected non-constant eval index error");
    };
    assert_eq!(err.text(), b"DXIL: non-constant eval index");
}

#[test]
fn hull_fitting_and_hull_domain_reflection() {
    use airconv_ir::{handlers::PatchConstantScalarInfo, tokens::tessellator};

    let mut decls = Declarations {
        tessellation_domain: tessellator::DOMAIN_QUAD,
        tessellation_partition: tessellator::PARTITIONING_INTEGER,
        max_tessellation_factor: 64.0,
        output_control_point_count: 16,
        hull_maximum_threads_per_patch: 16,
        tessellator_output_primitive: tessellator::OUTPUT_TRIANGLE_CCW,
        ..Declarations::default()
    };
    let mut io = Io {
        max_output_register: 8,
        patch_constant_scalars: (0..6)
            .map(|_| PatchConstantScalarInfo {
                component: 0,
                reg: 0,
                tess_factor_index: 0,
            })
            .collect(),
        ..Io::default()
    };
    fit_hull(&mut decls, &io).unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())));
    let threads_per_patch = decls.hull_maximum_threads_per_patch;
    let partition = decls.tessellation_partition;
    let max_factor = decls.max_tessellation_factor;
    let output_primitive = decls.tessellator_output_primitive;
    assert!(threads_per_patch >= 16);
    assert!((1.0..=64.0).contains(&max_factor));

    let dummy_shader = |program_type, decls, io| DxilShader {
        program_type,
        info: ShaderInfo::default(),
        decls,
        io,
        bitcode: Vec::new(),
        entry: EntryInfo::default(),
        lane_index_arg: u32::MAX,
        lane_count_arg: u32::MAX,
        include_helper_lanes: false,
        quads_2d: false,
        gs_locks: 0,
        capture_hash: 0,
        writes_uav: false,
    };

    let hull_shader = dummy_shader(shader_kind::HULL, decls, Io::default());
    assert_eq!(
        hull_shader.reflection().stage,
        StageReflection::Hull {
            threads_per_patch,
            partition,
            max_factor,
            output_primitive,
        }
    );

    // Too many threads per patch (> 32) is rejected.
    let mut too_wide = Declarations {
        hull_maximum_threads_per_patch: 33,
        ..Declarations::default()
    };
    let Err(wide_err) = fit_hull(&mut too_wide, &io) else {
        panic!("expected threadgroup-too-large error");
    };
    assert_eq!(
        wide_err.text(),
        b"Threadgroup size of tessellation pipeline is too large."
    );

    // Domain reflection caps max_potential_tess_factor via estimators::mesh_size <= 32768.
    io.max_output_register = 16;
    let domain_shader = dummy_shader(shader_kind::DOMAIN, Declarations::default(), io);
    let StageReflection::Domain {
        max_potential_tess_factor,
    } = domain_shader.reflection().stage
    else {
        panic!("expected StageReflection::Domain");
    };
    assert!((1..=64).contains(&max_potential_tess_factor));
    assert!(estimators::mesh_size(16, max_potential_tess_factor) <= MAX_MESH);
}

#[test]
fn hull_metadata_validation_rejects_invalid_values() {
    use core::ptr;

    use airconv_ir::{dxil_part::DxilPart, metallib};

    use crate::{llvm::sys, writer};

    let cx = Context::new();
    let i32_ty = cx.tcx().int(32);
    let f32_ty = cx.tcx().float();
    let int_md = |v: u64| cx.const_int(i32_ty, v, false).as_metadata();
    let float_md = |v: f32| cx.const_real(f32_ty, f64::from(v)).as_metadata();
    let add_md = |m: &Module<'_>, name: &core::ffi::CStr, node: sys::MetadataRef| {
        // SAFETY: `m`'s context is live and `node` belongs to it.
        let val = unsafe { sys::metadata_as_value(cx.raw(), node) };
        // SAFETY: `m` is live, `name` is NUL-terminated, `val` wraps a metadata node.
        unsafe { sys::add_named_metadata_operand(m.raw(), name.as_ptr(), val) };
    };

    let cases: &[(u64, u64, u64, f32, &[u8])] = &[
        (
            0,
            3,
            1,
            16.0,
            b"DXIL: hull input control point count out of range",
        ),
        (
            33,
            3,
            1,
            16.0,
            b"DXIL: hull input control point count out of range",
        ),
        (
            3,
            33,
            1,
            16.0,
            b"DXIL: hull output control point count out of range",
        ),
        (3, 3, 0, 16.0, b"DXIL: hull partitioning mode not supported"),
        (3, 3, 5, 16.0, b"DXIL: hull partitioning mode not supported"),
        (
            3,
            3,
            1,
            f32::NAN,
            b"DXIL: hull max tessellation factor is not finite",
        ),
    ];

    for &(in_cp, out_cp, part_mode, max_tess, expected_err) in cases {
        let m = Module::new(&cx, c"hs_bad");
        let pcf = m
            .add_function(b"pcf", cx.tcx().function(cx.tcx().void(), &[], false))
            .unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())));
        let hs_state = cx.md_node(&[
            pcf.val().as_metadata(),
            int_md(in_cp),
            int_md(out_cp),
            int_md(1),
            int_md(part_mode),
            int_md(3),
            float_md(max_tess),
        ]);
        let props = cx.md_node(&[int_md(6), hs_state]);
        let point = cx.md_node(&[
            ptr::null_mut(),
            cx.md_string(b""),
            ptr::null_mut(),
            ptr::null_mut(),
            props,
        ]);
        add_md(
            &m,
            c"dx.shaderModel",
            cx.md_node(&[cx.md_string(b"hs"), int_md(6), int_md(0)]),
        );
        add_md(&m, c"dx.entryPoints", point);
        let ml = writer::write(&m, &[])
            .unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())));
        let (bc, _) = metallib::parse(&ml).unwrap();
        let Err(err) = initialize(&DxilPart { bitcode: &bc }, 0) else {
            panic!("expected error {}", String::from_utf8_lossy(expected_err));
        };
        assert_eq!(err.text(), expected_err);
    }
}
