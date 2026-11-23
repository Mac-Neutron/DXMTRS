//! The DXSO writer constructed from parts outside `airconv-ir`.

/// A builder outside `airconv-ir` constructs a program from its parts, as `ff_build` does.
#[test]
fn builds_a_program_from_parts_outside_the_crate() {
    use std::collections::BTreeMap;

    use airconv_ir::{
        dxso::{
            DeclUsage, Declaration, RegKind, ShaderType,
            ir::{
                DefConstant, DstMods, DstOperand, Register, SrcModifier, SrcOperand, Swizzle,
                WriteMask,
            },
            opcode::Opcode,
            parse,
        },
        dxso_write::{ProgramParts, dst, instruction, src, write, write_parts},
    };

    let declarations = [
        Declaration::Semantic {
            usage: DeclUsage::Position,
            usage_index: 0,
            reg: Register {
                kind: RegKind::Input,
                index: 0,
            },
            mask: WriteMask::ALL,
        },
        Declaration::Semantic {
            usage: DeclUsage::Position,
            usage_index: 0,
            reg: Register {
                kind: RegKind::TexcoordOut,
                index: 0,
            },
            mask: WriteMask::ALL,
        },
    ];
    let def_constants = [DefConstant {
        reg: Register {
            kind: RegKind::Const,
            index: 256,
        },
        value: [0.5, 0.0, 1.0, 2.0],
    }];
    let mad = instruction(
        Opcode::Mad,
        Some(DstOperand {
            mods: DstMods::SATURATE,
            ..dst(RegKind::TexcoordOut, 0, WriteMask::ALL)
        }),
        [
            src(RegKind::Input, 0),
            SrcOperand {
                swizzle: Swizzle([0; 4]),
                modifier: SrcModifier::Neg,
                ..src(RegKind::Const, 256)
            },
            src(RegKind::Const, 2044),
        ],
    );
    let instructions = [mad];
    let parts = ProgramParts {
        shader_type: ShaderType::Vertex,
        major: 3,
        minor: 0,
        declarations: &declarations,
        def_constants: &def_constants,
        def_int_constants: &[],
        def_bool_constants: &[],
        instructions: &instructions,
        subroutines: &BTreeMap::new(),
    };
    let tokens = write_parts(&parts);
    let program = parse(&tokens).expect("the built program parses");
    assert_eq!(program.declarations, declarations);
    assert_eq!(program.instructions, instructions);
    assert_eq!(program.def_constants.len(), 1);
    assert_eq!(write(&program), tokens);
}
