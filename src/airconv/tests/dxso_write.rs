//! The DXSO writer over the parity corpus: `parse ∘ write` keeps every program the corpus holds.
//!
//! Each record source of `tests/d3d9/dxso-corpus/*.bin` (read through mtld3d-core, which
//! airconv-ir cannot depend on) is parsed, written and parsed again, and the two programs are
//! compared field by field (`def` floats by bits). The written stream is also a fixpoint: writing
//! its parse gives it back. `synthetic.bin` must be there and hold sources.

use std::{fs, path::PathBuf};

use airconv_ir::{
    dxso::{DxsoProgram, ir::Register, parse},
    dxso_write::write,
};
use mtld3d_core::shader_cache::{CacheHeader, read_header, read_records};

fn def_bits(p: &DxsoProgram) -> Vec<(Register, [u32; 4])> {
    p.def_constants
        .iter()
        .map(|d| (d.reg, d.value.map(f32::to_bits)))
        .collect()
}

/// Asserts that `a` and `b` agree in every public field but `bytecode`.
fn same(a: &DxsoProgram, b: &DxsoProgram, what: &str) {
    assert_eq!(a.shader_type, b.shader_type, "{what}");
    assert_eq!((a.major, a.minor), (b.major, b.minor), "{what}");
    assert_eq!(a.declarations, b.declarations, "{what}");
    assert_eq!(def_bits(a), def_bits(b), "{what}");
    assert_eq!(a.def_int_constants, b.def_int_constants, "{what}");
    assert_eq!(a.def_bool_constants, b.def_bool_constants, "{what}");
    assert_eq!(a.instructions, b.instructions, "{what}");
    assert_eq!(a.subroutines, b.subroutines, "{what}");
}

#[test]
fn round_trips_the_parity_corpus() {
    let dir = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/d3d9/dxso-corpus"
    ));
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "bin"))
        .collect();
    files.sort();
    assert!(
        files.iter().any(|p| p.ends_with("synthetic.bin")),
        "no synthetic.bin in {}",
        dir.display()
    );
    let mut checked = 0usize;
    for path in &files {
        let name = path.file_name().expect("a file name").to_string_lossy();
        let bytes = fs::read(path).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            read_header(&bytes).ok(),
            Some(CacheHeader::CURRENT),
            "{name}: not a current shader cache"
        );
        let mut sources = 0usize;
        for entry in &read_records(&bytes).shaders {
            let Some(source) = entry.source() else {
                continue;
            };
            let what = format!("{name} {:?} {:016x}", entry.kind, entry.key);
            let program = parse(source.tokens()).unwrap_or_else(|e| panic!("{what}: {e:?}"));
            let written = write(&program);
            let back = parse(&written).unwrap_or_else(|e| panic!("{what} written: {e:?}"));
            same(&program, &back, &what);
            assert_eq!(write(&back), written, "{what}: not a fixpoint");
            sources += 1;
        }
        println!("dxso-write: {name}: {sources} sources round-trip");
        assert!(sources > 0, "{name}: no record source");
        checked += sources;
    }
    println!("dxso-write: {checked} sources in {} files", files.len());
}

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
        },
        dxso_write::{ProgramParts, dst, instruction, src, write_parts},
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
