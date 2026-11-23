//! The recorder over the test shaders.
//!
//! The counts, outputs and register counts are the C++'s
//! (its handle printed by a scratch program over the baseline's sources); the handlers' captures
//! are each declaration's register and mask, read from the shaders' declarations.

use super::*;
use crate::{
    cfg,
    container::{Container, fourcc},
    signature,
    tokens::Program,
};

fn record(bytes: &[u8]) -> SignatureState {
    let container = Container::parse(bytes).unwrap();
    let input = signature::input(&container).unwrap();
    let output = signature::output_v5(&container).unwrap();
    let code = [fourcc::SHEX, fourcc::SHDR]
        .into_iter()
        .find_map(|part| container.find(part));
    let program = Program::new(code.unwrap()).unwrap();
    let mut recorder = Recorder::new(program.program_type(), &input, output.stream(0).unwrap());
    cfg::read(&program, &mut recorder).unwrap();
    recorder.state
}

#[test]
fn vsia_vs_records_its_outputs_then_its_inputs() {
    let s = record(include_bytes!("../../tests/data/vsia.vs.dxbc"));
    let output = |reg, index| SignatureHandler::PopOutput {
        reg,
        mask: 15,
        index,
    };
    let input = |reg, component_type| SignatureHandler::VertexInput {
        reg,
        mask: 15,
        component_type,
        name: format!("reg{reg}_0"),
    };
    let expected = [
        SignatureHandler::PopPosition {
            reg: 0,
            mask: 15,
            index: 0,
        },
        output(1, 1),
        output(2, 2),
        output(3, 3),
        output(4, 4),
        input(0, 3),
        input(1, 3),
        input(2, 3),
        input(3, 1),
        SignatureHandler::VertexId { reg: 4, mask: 1 },
    ];
    assert_eq!(s.signature_handlers, expected);
    assert_eq!((s.max_input_register, s.max_output_register), (5, 5));
    let outputs: Vec<_> = (1..5)
        .map(|reg| FunctionOutput::Vertex {
            user: format!("reg{reg}_0"),
            ty: MslValue::FLOAT4,
        })
        .collect();
    assert_eq!(
        s.func_signature.outputs().first(),
        Some(&FunctionOutput::Position {
            ty: MslValue::FLOAT4
        })
    );
    assert_eq!(s.func_signature.outputs().get(1..), Some(&outputs[..]));
}

#[test]
fn render_ps_records_its_targets_and_its_varying() {
    let s = record(include_bytes!("../../tests/data/render.ps.dxbc"));
    let target = |reg| SignatureHandler::RenderTarget {
        reg,
        mask: 15,
        component_type: 3,
    };
    let varying = SignatureHandler::FragmentInput {
        reg: 1,
        mask: 15,
        interpolation: Interpolation::CenterPerspective,
        component_type: 3,
        name: "reg1_0".into(),
    };
    assert_eq!(s.signature_handlers, [target(0), target(1), varying]);
    assert_eq!(
        (
            s.max_input_register,
            s.max_output_register,
            s.pso_valid_output_reg_mask
        ),
        (2, 2, 3)
    );
}

#[test]
fn modes_and_types_the_cxx_fell_off_are_errors() {
    assert_eq!(air_interpolation(0), Err(UNKNOWN_INTERPOLATION));
    assert_eq!(air_interpolation(8), Err(UNKNOWN_INTERPOLATION));
    assert_eq!(air_interpolation(1), Ok(Interpolation::Flat));
    assert_eq!(msl_value(0), Err(UNKNOWN_COMPONENT_TYPE));
    assert_eq!(msl_value(4), Err(UNKNOWN_COMPONENT_TYPE));
    assert_eq!(msl_value(2), Ok(MslValue::INT4));
}

#[test]
fn vertex_input_raises_the_input_count() {
    let element = Parameter {
        stream: 0,
        semantic_name: b"TEXCOORD".to_vec(),
        semantic_index: 0,
        system_value: 0,
        component_type: 2,
        register: 7,
        mask: 3,
        rw_mask: 0,
        min_precision: 0,
    };
    let input = Signature {
        parameters: vec![element],
    };
    let output = Signature::default();
    let mut recorder = Recorder::new(program_type::VERTEX, &input, &output);
    let mut decl = tokens::Instruction {
        opcode: opcode::DCL_INPUT,
        ..Default::default()
    };
    decl.operands[0].ty = operand_type::INPUT;
    decl.operands[0].index[0].reg = 7;
    decl.operands[0].mask_or_swizzle = 0x30;
    recorder
        .signature(&decl, NO_PHASE, &mut ShaderInfo::default())
        .unwrap();
    let handler = SignatureHandler::VertexInput {
        reg: 7,
        mask: 3,
        component_type: 2,
        name: "reg7_0".into(),
    };
    assert_eq!(recorder.state.signature_handlers, [handler]);
    assert_eq!(recorder.state.max_input_register, 8);
}

#[test]
fn mesh_outputs_pass_through_clip_and_cull_distances() {
    let input = Signature::default();
    let output = Signature::default();
    let make_decl = |reg, mask: u32, name| {
        let mut decl = tokens::Instruction {
            opcode: opcode::DCL_OUTPUT_SIV,
            payload: tokens::Payload::SystemValue {
                interpolation: 0,
                name,
            },
            ..Default::default()
        };
        decl.operands[0].ty = operand_type::OUTPUT;
        decl.operands[0].index[0].reg = reg;
        decl.operands[0].mask_or_swizzle = mask << 4;
        decl
    };
    let clip = make_decl(1, 0b0011, sb_name::CLIP_DISTANCE);
    let cull = make_decl(2, 0b1111, sb_name::CULL_DISTANCE);
    for stage in [program_type::DOMAIN, program_type::GEOMETRY] {
        let mut recorder = Recorder::new(stage, &input, &output);
        let mut info = ShaderInfo::default();
        recorder.signature(&clip, NO_PHASE, &mut info).unwrap();
        recorder.signature(&cull, NO_PHASE, &mut info).unwrap();
        assert_eq!(
            recorder.state.clip_distance_scalars,
            [
                ScalarInfo {
                    component: 0,
                    reg: 1
                },
                ScalarInfo {
                    component: 1,
                    reg: 1
                },
            ]
        );
        assert_eq!(
            recorder.state.cull_distance_scalars,
            (0..4)
                .map(|component| ScalarInfo { component, reg: 2 })
                .collect::<Vec<_>>()
        );
        assert_eq!(
            recorder.state.func_signature.mesh_vertex_outputs(),
            [
                MeshVertexOutput::Data(MeshData {
                    user: "SV_ClipDistance1".into(),
                    ty: MslValue::FLOAT4,
                    index: 0,
                }),
                MeshVertexOutput::Data(MeshData {
                    user: "SV_CullDistance2".into(),
                    ty: MslValue::FLOAT4,
                    index: 1,
                }),
            ]
        );
        assert_eq!(
            recorder.state.mesh_output_handlers,
            [
                MeshOutputHandler::VertexData {
                    reg: 1,
                    mask: 0b0011,
                    index: 0,
                    ty: MslValue::FLOAT4,
                },
                MeshOutputHandler::VertexData {
                    reg: 2,
                    mask: 0b1111,
                    index: 1,
                    ty: MslValue::FLOAT4,
                },
            ]
        );
        assert_eq!(recorder.state.max_output_register, 3);
    }
}

#[test]
fn mesh_outputs_deduplicate_packed_clip_and_cull_distances() {
    let input = Signature::default();
    let output = Signature::default();
    let make_decl = |reg, mask: u32, name| {
        let mut decl = tokens::Instruction {
            opcode: opcode::DCL_OUTPUT_SIV,
            payload: tokens::Payload::SystemValue {
                interpolation: 0,
                name,
            },
            ..Default::default()
        };
        decl.operands[0].ty = operand_type::OUTPUT;
        decl.operands[0].index[0].reg = reg;
        decl.operands[0].mask_or_swizzle = mask << 4;
        decl
    };
    let clip_x = make_decl(1, 0b0001, sb_name::CLIP_DISTANCE);
    let clip_y = make_decl(1, 0b0010, sb_name::CLIP_DISTANCE);
    let cull_z = make_decl(1, 0b0100, sb_name::CULL_DISTANCE);

    for stage in [program_type::DOMAIN, program_type::GEOMETRY] {
        let mut recorder = Recorder::new(stage, &input, &output);
        let mut info = ShaderInfo::default();
        recorder.signature(&clip_x, NO_PHASE, &mut info).unwrap();
        recorder.signature(&clip_y, NO_PHASE, &mut info).unwrap();
        recorder.signature(&cull_z, NO_PHASE, &mut info).unwrap();
        assert_eq!(
            recorder.state.func_signature.mesh_vertex_outputs(),
            [
                MeshVertexOutput::Data(MeshData {
                    user: "SV_ClipDistance1".into(),
                    ty: MslValue::FLOAT4,
                    index: 0,
                }),
                MeshVertexOutput::Data(MeshData {
                    user: "SV_CullDistance1".into(),
                    ty: MslValue::FLOAT4,
                    index: 1,
                }),
            ]
        );
        assert_eq!(
            recorder.state.mesh_output_handlers,
            [
                MeshOutputHandler::VertexData {
                    reg: 1,
                    mask: 0b0011,
                    index: 0,
                    ty: MslValue::FLOAT4,
                },
                MeshOutputHandler::VertexData {
                    reg: 1,
                    mask: 0b0100,
                    index: 1,
                    ty: MslValue::FLOAT4,
                },
            ]
        );
        assert_eq!(recorder.state.num_mesh_vertex_data, 2);
    }
}
