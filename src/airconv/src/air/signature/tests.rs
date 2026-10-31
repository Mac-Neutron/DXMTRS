//! Functions and their AIR entries, against step 4's frozen C++ outputs.
//!
//! The baseline test reads `AIRCONV_STEP4_BASE`: unset, it prints why and passes; set, a missing
//! file fails it. Each shader is initialized by `airconv_ir` (P3), the defines its compile adds
//! are replayed here in the C++ converter's order (a test-only stand-in for the D lanes' replay),
//! and its function is made in a fresh module. The `define` line (the baseline's without the
//! `local_unnamed_addr` the pipeline's `GlobalOptPass` adds), the stage's entry and the version
//! metadata are compared with the baseline's `.ll`, each `!N` written out in place.

use std::{env, fs, path::PathBuf};

use airconv_ir::{
    binding::{
        AddressSpace, BufferBinding, FunctionInput, FunctionOutput, FunctionSignatureDesc,
        IndirectBufferBinding, Interpolation, MemoryAccess, MslScalar, MslType, MslValue,
        MslVector,
    },
    container::Container,
    handlers::{SignatureHandler, UNKNOWN_COMPONENT_TYPE, msl_value},
    initialize::{self, DxbcShader},
    tokens::program_type,
};
use rustc_hash::FxHashMap;

use super::{Handles, build_argument_buffer, create_function};
use crate::{
    abi::{
        SM50_BINDING_INDEX_ARGUMENT_TABLE, SM50_BINDING_INDEX_CONSTANT_BUFFER,
        SM50_BINDING_INDEX_VERTEX_BUFFER, SM50_SHADER_METAL_310, SM50_SHADER_METAL_320,
    },
    air::{metadata, ty::AirTypes},
    llvm::{Builder, Context, Module},
    module,
};

/// A compile the baseline holds.
///
/// Its corpus, its input, its `.dis` folder and file, its stage's named metadata, its Metal
/// version, and whether its vertex inputs come through a layout.
struct Case {
    corpus: &'static str,
    file: &'static str,
    dis: &'static str,
    ll: &'static str,
    stage: &'static str,
    metal: u32,
    layout: bool,
}

const fn case(corpus: &'static str, file: &'static str, stage: &'static str) -> Case {
    Case {
        corpus,
        file,
        dis: corpus,
        ll: file,
        stage,
        metal: SM50_SHADER_METAL_310,
        layout: true,
    }
}

const CASES: [Case; 11] = [
    case("C4", "sync.dxbc", "air.kernel"),
    case("C4", "grad.dxbc", "air.kernel"),
    Case {
        dis: "C4-320",
        metal: SM50_SHADER_METAL_320,
        ..case("C4", "sync.dxbc", "air.kernel")
    },
    case("C5", "compute.cs.dxbc", "air.kernel"),
    // The base line's layout: every input a Float4, pulled from the vertex buffers.
    case("C5", "render.vs.dxbc", "air.vertex"),
    Case {
        ll: "render.vs.dxbc#nolayout",
        layout: false,
        ..case("C5", "render.vs.dxbc", "air.vertex")
    },
    case("C5", "render.ps.dxbc", "air.fragment"),
    // Both tables: the second structure is `argument_buffer_struct.0`.
    case("C5", "sample.ps.dxbc", "air.fragment"),
    case("C5", "cb.ps.dxbc", "air.fragment"),
    // A clip distance output; depth and coverage outputs.
    case("C6", "sysval.vs.dxbc", "air.vertex"),
    case("C6", "sysval.ps.dxbc", "air.fragment"),
];

#[test]
fn signature_matches_baseline() {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: no signature is compared");
        return;
    };
    let base = PathBuf::from(base);
    for case in &CASES {
        let input = base.join(format!("../corpus/{}/{}", case.corpus, case.file));
        let bytes = fs::read(&input).unwrap_or_else(|e| panic!("{}: {e}", input.display()));
        let shader = initialize::dxbc(&Container::parse(&bytes).unwrap()).unwrap();
        let ll = base.join(format!("{}.dis/{}.ll", case.dis, case.ll));
        let theirs = fs::read_to_string(&ll).unwrap_or_else(|e| panic!("{}: {e}", ll.display()));
        let cx = Context::new();
        let m = module::initialize(&cx, c"signature");
        let ours = compile(&m, &shader, case);
        let what = format!("{}/{}", case.dis, case.ll);
        assert_eq!(
            define(&ours),
            define(&theirs).replace(" local_unnamed_addr", ""),
            "{what}"
        );
        for name in [case.stage, "air.version", "air.language_version"] {
            assert_eq!(named(&ours, name), named(&theirs, name), "{what}: {name}");
        }
        let triple = |text: &str| {
            text.lines()
                .find(|l| l.starts_with("target triple"))
                .map(str::to_owned)
        };
        assert_eq!(triple(&ours), triple(&theirs), "{what}");
        for line in ours.lines().filter(|l| l.starts_with('%')) {
            assert!(theirs.lines().any(|l| l == line), "{what}: {line}");
        }
    }
}

#[test]
fn errors_name_what_is_wrong() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"errors");
    let handles = Handles::default();
    let mut desc = FunctionSignatureDesc::default();
    desc.define_input(FunctionInput::VertexStageIn {
        attribute: 0,
        component_type: 4,
        name: "reg0_0".into(),
    });
    let error = create_function(&m, &desc, b"main", false, &handles)
        .err()
        .unwrap();
    assert_eq!(error.text(), UNKNOWN_COMPONENT_TYPE.what.as_bytes());
    let mut desc = FunctionSignatureDesc::default();
    desc.define_input(table_input(7, 7, 1, "binding_table"));
    let error = create_function(&m, &desc, b"main", false, &handles)
        .err()
        .unwrap();
    assert_eq!(
        error.text(),
        b"invalid operation: a binding names an unknown type."
    );
}

#[test]
fn skipped_outputs_make_a_void_function() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"skip");
    let mut desc = FunctionSignatureDesc::default();
    desc.define_output(FunctionOutput::Position {
        ty: MslValue::FLOAT4,
    });
    desc.define_input(FunctionInput::VertexId);
    let (_, entry) = ok(create_function(
        &m,
        &desc,
        b"main",
        true,
        &Handles::default(),
    ));
    metadata::add_entry(&m, c"air.vertex", entry);
    let text = String::from_utf8_lossy(&m.print()).into_owned();
    assert!(text.contains("declare void @main(i32)"), "{text}");
    assert!(
        named(&text, "air.vertex").starts_with("!{!{void (i32)* @main, !{}, !{"),
        "{text}"
    );
}

#[test]
fn vertex_attributes_are_typed_by_their_component_type() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"attributes");
    let mut desc = FunctionSignatureDesc::default();
    for (attribute, component_type) in (0..4).zip([0, 1, 2, 3]) {
        desc.define_input(FunctionInput::VertexStageIn {
            attribute,
            component_type,
            name: format!("a{attribute}"),
        });
    }
    let (_, entry) = ok(create_function(
        &m,
        &desc,
        b"main",
        false,
        &Handles::default(),
    ));
    metadata::add_entry(&m, c"air.vertex", entry);
    let text = String::from_utf8_lossy(&m.print()).into_owned();
    let line = "declare void @main(<4 x i32>, <4 x i32>, <4 x i32>, <4 x float>)";
    assert!(text.contains(line), "{text}");
    let names: Vec<&str> = text
        .match_indices("!\"air.arg_type_name\", !\"")
        .map(|(at, _)| {
            let name = &text[at + 24..];
            &name[..name.find('"').unwrap()]
        })
        .collect();
    assert_eq!(names, ["uint4", "uint4", "int4", "float4"], "{text}");
}

#[test]
fn buffer_sizes_are_bits_and_a_vector_of_one_is_its_scalar() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"buffers");
    let mut desc = FunctionSignatureDesc::default();
    for (slot, dimension, scalar) in [(0, 3, MslScalar::Float), (1, 1, MslScalar::Uint)] {
        desc.define_input(FunctionInput::Buffer(BufferBinding {
            buffer_size: None,
            location_index: slot,
            array_size: 1,
            memory_access: MemoryAccess::Read,
            address_space: AddressSpace::Device,
            ty: MslType::Value(MslValue::Vector(MslVector { dimension, scalar })),
            arg_name: format!("b{slot}"),
            raster_order_group: None,
        }));
    }
    let (_, entry) = ok(create_function(
        &m,
        &desc,
        b"main",
        false,
        &Handles::default(),
    ));
    metadata::add_entry(&m, c"air.vertex", entry);
    let text = String::from_utf8_lossy(&m.print()).into_owned();
    assert!(
        text.contains("declare void @main(<3 x float> addrspace(1)*, i32 addrspace(1)*)"),
        "{text}"
    );
    // `<3 x float>` takes 96 bits and aligns to 16 bytes (the layout's `v96:128:128`).
    let float3 = "!\"air.arg_type_size\", i32 12, !\"air.arg_type_align_size\", i32 16, \
                  !\"air.arg_type_name\", !\"float3\"";
    let uint = "!\"air.arg_type_size\", i32 4, !\"air.arg_type_align_size\", i32 4, \
                !\"air.arg_type_name\", !\"uint\"";
    assert!(text.contains(float3) && text.contains(uint), "{text}");
}

/// The shader's function made as its C++ compile made it, and the module's text.
fn compile(m: &Module<'_>, shader: &DxbcShader, case: &Case) -> String {
    let cx = m.cx();
    // As the converters, before any binding: the context's named structures in the C++'s order.
    let _types = AirTypes::new(cx.tcx());
    let mut handles = Handles::default();
    let desc = replay(m, shader, case.layout, &mut handles);
    let (func, entry) = ok(create_function(m, &desc, b"main", false, &handles));
    metadata::set_metal_version(m, case.metal);
    let block = func.append_block(cx, c"entry").unwrap();
    let builder = Builder::new(cx);
    builder.position_at_end(block);
    builder.unreachable();
    metadata::add_entry(m, &std::ffi::CString::new(case.stage).unwrap(), entry);
    String::from_utf8_lossy(&m.print()).into_owned()
}

/// What the compile defines after Initialize, in `dxbc_converter.cpp`'s order.
///
/// The handlers' defines (vertex inputs, pixel inputs, render targets typed by `RGBA8Unorm`,
/// depth), a vertex shader's system values and clip distances, then `setup_binding_table2`'s two
/// tables.
fn replay(
    m: &Module<'_>,
    shader: &DxbcShader,
    layout: bool,
    handles: &mut Handles,
) -> FunctionSignatureDesc {
    let mut desc = shader.io.func_signature.clone();
    for handler in &shader.io.signature_handlers {
        match handler {
            SignatureHandler::VertexInput {
                reg,
                component_type,
                name,
                ..
            } => {
                desc.define_input(if layout {
                    FunctionInput::Buffer(BufferBinding {
                        buffer_size: None,
                        location_index: SM50_BINDING_INDEX_VERTEX_BUFFER,
                        array_size: 0,
                        memory_access: MemoryAccess::Read,
                        address_space: AddressSpace::Constant,
                        ty: MslType::Value(MslValue::UINT),
                        arg_name: "vertex_buffers".into(),
                        raster_order_group: None,
                    })
                } else {
                    FunctionInput::VertexStageIn {
                        attribute: *reg,
                        component_type: *component_type,
                        name: name.clone(),
                    }
                });
            }
            SignatureHandler::FragmentInput {
                reg,
                interpolation,
                component_type,
                name,
                ..
            } => {
                let pulled = shader.cfg.info.pull_mode_reg_mask & (1 << reg) != 0;
                desc.define_input(FunctionInput::FragmentStageIn {
                    user: name.clone(),
                    ty: msl_value(*component_type).unwrap(),
                    interpolation: interpolation.clone(),
                    pull_mode: pulled && *interpolation != Interpolation::Flat,
                });
            }
            SignatureHandler::RenderTarget { reg, .. } => {
                desc.define_output(FunctionOutput::RenderTarget {
                    dual_source_blending: false,
                    index: *reg,
                    ty: MslValue::FLOAT4,
                });
            }
            SignatureHandler::DepthOutput { argument } => {
                desc.define_output(FunctionOutput::Depth {
                    argument: argument.clone(),
                });
            }
            _ => {}
        }
    }
    if shader.program_type == program_type::VERTEX {
        // The user outputs the C++ defines again here were defined at Initialize.
        for input in [
            FunctionInput::VertexId,
            FunctionInput::BaseVertex,
            FunctionInput::InstanceId,
            FunctionInput::BaseInstance,
        ] {
            desc.define_input(input);
        }
        let count = shader.io.clip_distance_scalars.len();
        if count > 0 {
            desc.define_output(FunctionOutput::ClipDistance { count });
        }
    }
    let tables = [
        (
            &shader.binding_table,
            SM50_BINDING_INDEX_ARGUMENT_TABLE,
            "binding_table",
        ),
        (
            &shader.binding_table_cbuffer,
            SM50_BINDING_INDEX_CONSTANT_BUFFER,
            "cbuffer_table",
        ),
    ];
    for (table, slot, name) in tables {
        if !table.is_empty() {
            let (ty, info) = ok(build_argument_buffer(m, table, handles));
            desc.define_input(table_input(ty, info, slot, name));
        }
    }
    desc
}

/// The value, or a panic with the error's text.
fn ok<T>(result: Result<T, crate::api::Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// `setup_binding_table2`'s argument buffer input.
fn table_input(struct_type: u32, struct_type_info: u32, slot: u32, name: &str) -> FunctionInput {
    FunctionInput::IndirectBuffer(IndirectBufferBinding {
        location_index: slot,
        array_size: 1,
        memory_access: MemoryAccess::Read,
        address_space: AddressSpace::Constant,
        struct_type,
        struct_type_info,
        arg_name: name.into(),
    })
}

/// The `define` line, each attribute group `#N` written out.
fn define(text: &str) -> String {
    let groups: FxHashMap<&str, &str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("attributes ")?.split_once(" = "))
        .collect();
    let line = text.lines().find(|l| l.starts_with("define ")).unwrap();
    line.split(' ')
        .map(|word| groups.get(word).copied().unwrap_or(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The named metadata `name`'s operands, each `!N` written out.
fn named(text: &str, name: &str) -> String {
    let nodes: FxHashMap<&str, &str> = text
        .lines()
        .filter_map(|l| l.strip_prefix('!')?.split_once(" = "))
        .collect();
    let list = nodes
        .get(name)
        .unwrap_or_else(|| panic!("no !{name} in\n{text}"));
    expand(list, &nodes)
}

/// `text` with each node reference replaced by the node; strings are copied as they are.
fn expand(text: &str, nodes: &FxHashMap<&str, &str>) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('!') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(string) = rest.strip_prefix('"') {
            let end = string.find('"').unwrap() + 2;
            out.push('!');
            out.push_str(&rest[..end]);
            rest = &rest[end..];
        } else {
            let digits = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            if digits == 0 {
                out.push('!');
            } else {
                out.push_str(&expand(nodes[&rest[..digits]], nodes));
                rest = &rest[digits..];
            }
        }
    }
    out.push_str(rest);
    out
}
