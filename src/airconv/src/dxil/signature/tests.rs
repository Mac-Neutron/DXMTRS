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
    ok(io.add_signature_handlers(&shader, &BTreeSet::new(), false, false));
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
    ok(io.add_signature_handlers(&shader, &BTreeSet::new(), false, false));
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
            .add_signature_handlers(&bad, &BTreeSet::new(), false, false)
            .err()
            .map(|e| e.text().to_vec()),
        Some(b"DXIL: pixel shader output SV_Target not supported".to_vec())
    );
}
