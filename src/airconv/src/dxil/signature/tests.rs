use super::*;

fn element(name: &[u8], kind: u32, rows: u32, start: (i32, i32), cols: u32) -> SignatureElement {
    SignatureElement {
        id: 0,
        name: name.to_vec(),
        semantic_indices: (5..5 + rows).collect(),
        ty: component_type::F32,
        kind,
        interpolation: interpolation::LINEAR_SAMPLE,
        rows,
        cols,
        start_row: start.0,
        start_col: start.1,
    }
}

fn entry(kind: u32, inputs: Vec<SignatureElement>, outputs: Vec<SignatureElement>) -> EntryInfo {
    EntryInfo {
        kind,
        inputs,
        outputs,
        ..EntryInfo::default()
    }
}

fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

#[test]
fn a_varying_is_a_vector_at_the_signatures_width() {
    let mut e = element(b"TexCoord", semantic_kind::ARBITRARY, 1, (0, 0), 1);
    e.ty = component_type::U16;
    let (user, ty) = ok(varying(&e, 0));
    assert_eq!(user, "TEXCOORD5");
    // A vector of one, not the scalar (the C++'s MSLVector{1, uint}).
    assert_eq!(
        ty,
        MslValue::Vector(MslVector {
            dimension: 1,
            scalar: MslScalar::Uint
        })
    );
    e.name = b"T\xff".to_vec();
    assert_eq!(
        varying(&e, 0).err().map(|e| e.text().to_vec()),
        Some(NAME_NOT_UTF8.to_vec())
    );
}

#[test]
fn vertex_elements_record_their_handlers() {
    let mut vertex_id = element(b"SV_VertexID", semantic_kind::VERTEX_ID, 1, (-1, -1), 1);
    vertex_id.id = 3;
    let shader = entry(
        shader_kind::VERTEX,
        vec![
            vertex_id,
            element(b"TEXCOORD", semantic_kind::ARBITRARY, 2, (1, 0), 4),
        ],
        vec![
            element(b"SV_Position", semantic_kind::POSITION, 1, (0, 0), 4),
            element(b"COLOR", semantic_kind::ARBITRARY, 1, (2, 2), 2),
        ],
    );
    let mut io = Io::default();
    ok(io.add_signature_handlers(&shader, &BTreeSet::new(), &BTreeSet::new(), false, false));
    assert_eq!(
        io.unpacked_inputs.into_iter().collect::<Vec<_>>(),
        [(3, UNPACKED_VERTEX_ID)]
    );
    let [
        SignatureHandler::VertexInput {
            reg: 1,
            mask: 0xf,
            component_type: FLOAT,
            name: first,
        },
        SignatureHandler::VertexInput {
            reg: 2,
            name: second,
            ..
        },
        SignatureHandler::PopPosition {
            reg: 0,
            mask: 0xf,
            index: 0,
        },
        SignatureHandler::PopShifted {
            reg: 2,
            col: 2,
            cols: 2,
            index: 1,
        },
    ] = &io.signature_handlers[..]
    else {
        panic!("handlers");
    };
    assert_eq!(
        (first.as_str(), second.as_str()),
        ("TEXCOORD5", "TEXCOORD6")
    );
    assert_eq!((io.max_input_register, io.max_output_register), (3, 3));
}

#[test]
fn pixel_outputs_record_their_handlers() {
    let mut target = element(b"SV_Target", semantic_kind::TARGET, 1, (0, 0), 4);
    target.semantic_indices = vec![2];
    target.ty = component_type::U32;
    let shader = entry(
        shader_kind::PIXEL,
        vec![element(
            b"SV_IsFrontFace",
            semantic_kind::IS_FRONT_FACE,
            1,
            (-1, -1),
            1,
        )],
        vec![
            target,
            element(
                b"SV_DepthLessEqual",
                semantic_kind::DEPTH_LESS_EQUAL,
                1,
                (-1, -1),
                1,
            ),
            element(b"SV_Coverage", semantic_kind::COVERAGE, 1, (-1, -1), 1),
            element(b"SV_StencilRef", semantic_kind::STENCIL_REF, 1, (-1, -1), 1),
        ],
    );
    let mut io = Io::default();
    ok(io.add_signature_handlers(&shader, &BTreeSet::new(), &BTreeSet::new(), false, false));
    // The front face is defined and unpacked; the target is defined at compile time.
    assert_eq!(io.func_signature.inputs(), [FunctionInput::FrontFacing]);
    assert_eq!(
        io.func_signature.outputs(),
        [FunctionOutput::CoverageMask, FunctionOutput::StencilRef]
    );
    assert_eq!(io.unpacked_inputs.get(&0), Some(&0));
    assert!(matches!(
        io.signature_handlers[..],
        [
            SignatureHandler::RenderTarget {
                reg: 0,
                mask: 0xf,
                target: 2,
                component_type: UINT,
            },
            SignatureHandler::Depth {
                argument: DepthArgument::Less
            },
            SignatureHandler::Coverage { index: 0 },
            SignatureHandler::StencilRef { index: 1 },
        ]
    ));
    assert_eq!(
        (
            io.pso_valid_output_reg_mask,
            io.ps_has_coverage_output,
            io.max_output_register
        ),
        (4, 1, 1)
    );
    let mut bad = shader;
    bad.outputs[0].kind = semantic_kind::POSITION;
    assert_eq!(
        Io::default()
            .add_signature_handlers(&bad, &BTreeSet::new(), &BTreeSet::new(), false, false)
            .err()
            .map(|e| e.text().to_vec()),
        Some(b"DXIL: pixel shader output SV_Target not supported".to_vec())
    );
}

#[test]
fn pixel_position_always_uses_center_no_perspective() {
    for mode in [0, 2, interpolation::LINEAR_SAMPLE] {
        let mut pos = element(b"SV_Position", semantic_kind::POSITION, 1, (0, 0), 4);
        pos.interpolation = mode;
        let shader = entry(shader_kind::PIXEL, vec![pos], vec![]);
        let mut io = Io::default();
        ok(io.add_signature_handlers(&shader, &BTreeSet::new(), &BTreeSet::new(), false, false));
        assert_eq!(
            io.func_signature.inputs(),
            [FunctionInput::Position {
                interpolation: Interpolation::CenterNoPerspective,
            }]
        );
    }
}

#[test]
fn pixel_dual_source_blending_uses_first_format_with_fallback() {
    let mut rt0 = element(b"SV_Target", semantic_kind::TARGET, 1, (0, 0), 4);
    rt0.semantic_indices = vec![0];
    rt0.ty = component_type::U32;
    let mut rt1 = element(b"SV_Target", semantic_kind::TARGET, 1, (1, 0), 4);
    rt1.semantic_indices = vec![1];
    rt1.ty = component_type::U32;
    let shader = entry(shader_kind::PIXEL, vec![], vec![rt0, rt1]);
    let mut io = Io::default();
    ok(io.add_signature_handlers(&shader, &BTreeSet::new(), &BTreeSet::new(), false, false));

    let mut pixel = crate::abi::Sm50ShaderPsoPixelShaderData {
        next: core::ptr::null_mut(),
        r#type: crate::abi::SM50_SHADER_PSO_PIXEL_SHADER,
        sample_mask: u32::MAX,
        dual_source_blending: 1,
        disable_depth_output: 0,
        unorm_output_reg_mask: 0,
        pixel_formats: [70, 0, 0, 0, 0, 0, 0, 0],
    };
    let mut desc = io.func_signature.clone();
    ok(io.record(
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: Some(&pixel),
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    ));
    assert_eq!(
        desc.outputs(),
        [
            FunctionOutput::RenderTarget {
                dual_source_blending: true,
                index: 0,
                ty: MslValue::FLOAT4,
            },
            FunctionOutput::RenderTarget {
                dual_source_blending: true,
                index: 1,
                ty: MslValue::FLOAT4,
            },
        ]
    );

    pixel.pixel_formats = [0; 8];
    let mut fallback = io.func_signature.clone();
    ok(io.record(
        &mut fallback,
        &Options {
            ia_layout: None,
            pixel: Some(&pixel),
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    ));
    assert_eq!(
        fallback.outputs(),
        [
            FunctionOutput::RenderTarget {
                dual_source_blending: true,
                index: 0,
                ty: MslValue::UINT4,
            },
            FunctionOutput::RenderTarget {
                dual_source_blending: true,
                index: 1,
                ty: MslValue::UINT4,
            },
        ]
    );
}

#[test]
fn pixel_pull_mode_handles_flat_and_sub_row_elements() {
    let mut e0 = element(b"TEXCOORD", semantic_kind::ARBITRARY, 1, (0, 0), 4);
    e0.id = 0;
    e0.interpolation = interpolation::LINEAR_SAMPLE;

    let mut e1 = element(b"TEXCOORD", semantic_kind::ARBITRARY, 1, (1, 0), 4);
    e1.id = 1;
    e1.semantic_indices = vec![6];
    e1.interpolation = interpolation::CONSTANT;

    let mut e2 = element(b"COLOR", semantic_kind::ARBITRARY, 2, (2, 2), 2);
    e2.id = 2;
    e2.interpolation = interpolation::LINEAR_NOPERSPECTIVE;

    let shader = entry(shader_kind::PIXEL, vec![e0, e1, e2], vec![]);
    let pull = BTreeSet::from([0, 1, 2]);
    let mut io = Io::default();
    ok(io.add_signature_handlers(&shader, &BTreeSet::new(), &pull, false, false));

    assert_eq!(
        io.func_signature.inputs(),
        [
            FunctionInput::FragmentStageIn {
                user: "TEXCOORD5".into(),
                ty: MslValue::FLOAT4,
                interpolation: Interpolation::SamplePerspective,
                pull_mode: true,
            },
            FunctionInput::SampleIndex,
            FunctionInput::FragmentStageIn {
                user: "TEXCOORD6".into(),
                ty: MslValue::FLOAT4,
                interpolation: Interpolation::Flat,
                pull_mode: false,
            },
            FunctionInput::FragmentStageIn {
                user: "COLOR5".into(),
                ty: MslValue::Vector(MslVector {
                    dimension: 2,
                    scalar: MslScalar::Float,
                }),
                interpolation: Interpolation::CenterNoPerspective,
                pull_mode: true,
            },
            FunctionInput::FragmentStageIn {
                user: "COLOR6".into(),
                ty: MslValue::Vector(MslVector {
                    dimension: 2,
                    scalar: MslScalar::Float,
                }),
                interpolation: Interpolation::CenterNoPerspective,
                pull_mode: true,
            },
        ]
    );
    assert!(matches!(
        io.signature_handlers[..],
        [
            SignatureHandler::InitInterpolatedShifted {
                element_id: 0,
                row: 0,
                index: 0,
                reg: 0,
                col: 0,
                cols: 4,
                sample_index: Some(1),
                ..
            },
            SignatureHandler::InitShifted {
                index: 2,
                reg: 1,
                col: 0,
                cols: 4,
            },
            SignatureHandler::InitInterpolatedShifted {
                element_id: 2,
                row: 0,
                index: 3,
                reg: 2,
                col: 2,
                cols: 2,
                sample_index: None,
                ..
            },
            SignatureHandler::InitInterpolatedShifted {
                element_id: 2,
                row: 1,
                index: 4,
                reg: 3,
                col: 2,
                cols: 2,
                sample_index: None,
                ..
            },
        ]
    ));
}

#[test]
fn pixel_pull_mode_prologue_populates_interpolant_map_and_registers() {
    use crate::{
        air::{
            signature::{Handles, create_function},
            ty::AirTypes,
        },
        dxbc::resources::setup_io_registers,
        llvm::{Builder, Context},
        module,
    };

    let mut scalar_el = element(b"TEXCOORD", semantic_kind::ARBITRARY, 1, (0, 0), 1);
    scalar_el.id = 0;
    scalar_el.semantic_indices = vec![0];
    scalar_el.interpolation = interpolation::LINEAR_CENTROID;

    let mut vec2_el = element(b"COLOR", semantic_kind::ARBITRARY, 1, (1, 2), 2);
    vec2_el.id = 1;
    vec2_el.semantic_indices = vec![0];
    vec2_el.interpolation = interpolation::LINEAR_SAMPLE;

    let shader = entry(shader_kind::PIXEL, vec![scalar_el, vec2_el], vec![]);
    let pull = BTreeSet::from([0, 1]);
    let mut io = Io::default();
    ok(io.add_signature_handlers(&shader, &BTreeSet::new(), &pull, false, false));

    let mut desc = io.func_signature.clone();
    let replay = ok(io.record(
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    ));

    let cx = Context::new();
    let m = module::initialize(&cx, c"pull.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let (function, _) = ok(create_function(
        &m,
        &desc,
        b"ps_main",
        false,
        &Handles::default(),
    ));
    let entry_bb = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, &m, &types, 0);

    let mut res = Resources::default();
    setup_io_registers(&mut res, &types, &b, io.max_input_register, 1);
    ok(replay.prologue(&air, function, &mut res));
    b.ret_void();

    let itp0 = res.dxil_interpolant_map.get(&(0, 0)).unwrap();
    assert_eq!(
        (itp0.perspective, itp0.ty, itp0.cols),
        (true, tcx.float(), 1)
    );
    let itp1 = res.dxil_interpolant_map.get(&(1, 0)).unwrap();
    assert_eq!(
        (itp1.perspective, itp1.ty, itp1.cols),
        (true, tcx.vector(tcx.float(), 2), 2)
    );

    let text = String::from_utf8(m.print()).unwrap();
    assert!(
        text.contains("call float @air.interpolate_centroid_perspective.f32("),
        "{text}"
    );
    assert!(
        text.contains("call <2 x float> @air.interpolate_sample_perspective.v2f32("),
        "{text}"
    );
    assert!(!text.contains("extractelement float"), "{text}");
    assert!(text.contains("extractelement <2 x float>"), "{text}");
}

#[test]
fn hull_signature_tracks_patch_constant_scalars() {
    use airconv_ir::handlers::PatchConstantScalarInfo;

    let mut outer = element(b"SV_TessFactor", semantic_kind::TESS_FACTOR, 4, (0, 0), 1);
    outer.semantic_indices = vec![0, 1, 2, 3];
    let mut inner = element(
        b"SV_InsideTessFactor",
        semantic_kind::INSIDE_TESS_FACTOR,
        2,
        (4, 0),
        1,
    );
    inner.semantic_indices = vec![0, 1];
    let arb = element(b"COLOR", semantic_kind::ARBITRARY, 1, (5, 1), 2);

    let mut quad_entry = entry(
        shader_kind::HULL,
        vec![element(b"POS", semantic_kind::ARBITRARY, 1, (0, 0), 4)],
        vec![element(b"POS", semantic_kind::ARBITRARY, 2, (0, 0), 4)],
    );
    quad_entry.tess_domain = tessellator::DOMAIN_QUAD;
    quad_entry.patch_constants = vec![outer, inner, arb];

    let mut io = Io::default();
    ok(io.add_signature_handlers(
        &quad_entry,
        &BTreeSet::new(),
        &BTreeSet::new(),
        false,
        false,
    ));
    assert_eq!(io.max_input_register, 1);
    assert_eq!(io.max_output_register, 2);
    assert_eq!(io.max_patch_constant_output_register, 6);
    assert_eq!(
        io.patch_constant_scalars,
        vec![
            PatchConstantScalarInfo {
                component: 0,
                reg: 0,
                tess_factor_index: 0,
            },
            PatchConstantScalarInfo {
                component: 0,
                reg: 1,
                tess_factor_index: 1,
            },
            PatchConstantScalarInfo {
                component: 0,
                reg: 2,
                tess_factor_index: 2,
            },
            PatchConstantScalarInfo {
                component: 0,
                reg: 3,
                tess_factor_index: 3,
            },
            PatchConstantScalarInfo {
                component: 0,
                reg: 4,
                tess_factor_index: 4,
            },
            PatchConstantScalarInfo {
                component: 0,
                reg: 5,
                tess_factor_index: 5,
            },
            PatchConstantScalarInfo {
                component: 1,
                reg: 5,
                tess_factor_index: -1,
            },
            PatchConstantScalarInfo {
                component: 2,
                reg: 5,
                tess_factor_index: -1,
            },
        ]
    );

    // Triangle domain offsets SV_InsideTessFactor by 3.
    let mut tri_inner = element(
        b"SV_InsideTessFactor",
        semantic_kind::INSIDE_TESS_FACTOR,
        1,
        (0, 0),
        1,
    );
    tri_inner.semantic_indices = vec![0];
    let mut tri_entry = entry(shader_kind::HULL, vec![], vec![]);
    tri_entry.tess_domain = tessellator::DOMAIN_TRI;
    tri_entry.patch_constants = vec![tri_inner];
    let mut tri_io = Io::default();
    ok(tri_io.add_signature_handlers(&tri_entry, &BTreeSet::new(), &BTreeSet::new(), false, false));
    assert_eq!(
        tri_io.patch_constant_scalars,
        vec![PatchConstantScalarInfo {
            component: 0,
            reg: 0,
            tess_factor_index: 3,
        }]
    );
}

#[test]
fn domain_signature_tracks_patch_constants_and_clip_cull_dedup() {
    use airconv_ir::handlers::ScalarInfo;

    let pc = element(b"PATCHDATA", semantic_kind::ARBITRARY, 2, (1, 0), 4);
    let pos = element(b"SV_Position", semantic_kind::POSITION, 1, (0, 0), 4);
    // Two packed clip-distance elements sharing row 1 must emit one SV_ClipDistance1 mesh data.
    let clip_lo = element(
        b"SV_ClipDistance",
        semantic_kind::CLIP_DISTANCE,
        1,
        (1, 0),
        2,
    );
    let clip_hi = element(
        b"SV_ClipDistance",
        semantic_kind::CLIP_DISTANCE,
        1,
        (1, 2),
        2,
    );
    let cull = element(
        b"SV_CullDistance",
        semantic_kind::CULL_DISTANCE,
        1,
        (2, 0),
        1,
    );

    let mut ds_entry = entry(
        shader_kind::DOMAIN,
        vec![element(b"POS", semantic_kind::ARBITRARY, 1, (0, 0), 4)],
        vec![pos, clip_lo, clip_hi, cull],
    );
    ds_entry.patch_constants = vec![pc];

    let mut io = Io::default();
    ok(io.add_signature_handlers(&ds_entry, &BTreeSet::new(), &BTreeSet::new(), false, false));
    assert_eq!(io.max_input_register, 3);
    assert_eq!(io.max_output_register, 3);
    assert_eq!(io.num_mesh_vertex_data, 2);
    assert_eq!(
        io.clip_distance_scalars,
        vec![
            ScalarInfo {
                component: 0,
                reg: 1,
            },
            ScalarInfo {
                component: 1,
                reg: 1,
            },
            ScalarInfo {
                component: 2,
                reg: 1,
            },
            ScalarInfo {
                component: 3,
                reg: 1,
            },
        ]
    );
    assert_eq!(
        io.cull_distance_scalars,
        vec![ScalarInfo {
            component: 0,
            reg: 2,
        }]
    );
    assert_eq!(io.mesh_output_handlers.len(), 3);
    assert!(matches!(
        io.mesh_output_handlers.first(),
        Some(MeshOutputHandler::Position { reg: 0 })
    ));
    assert!(matches!(
        io.mesh_output_handlers.get(1),
        Some(MeshOutputHandler::Shifted {
            reg: 1,
            col: 0,
            cols: 4,
            index: 0,
            ..
        })
    ));
    assert!(matches!(
        io.mesh_output_handlers.get(2),
        Some(MeshOutputHandler::Shifted {
            reg: 2,
            col: 0,
            cols: 4,
            index: 1,
            ..
        })
    ));
}
