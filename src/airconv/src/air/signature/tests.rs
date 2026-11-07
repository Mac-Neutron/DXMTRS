//! Tests of functions and their AIR entries.

use airconv_ir::{
    binding::{
        AddressSpace, BufferBinding, FunctionInput, FunctionOutput, FunctionSignatureDesc,
        IndirectBufferBinding, MemoryAccess, MslScalar, MslTexture, MslType, MslValue, MslVector,
        SamplerBinding, TextureBinding, TextureKind,
    },
    handlers::UNKNOWN_COMPONENT_TYPE,
};
use rustc_hash::FxHashMap;

use super::{Handles, create_function};
use crate::{air::metadata, llvm::Context, module};

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
        named(&text, "air.vertex").starts_with("!{!{ptr @main, !{}, !{"),
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
        text.contains("declare void @main(ptr addrspace(1), ptr addrspace(1))"),
        "{text}"
    );
    // `<3 x float>` takes 96 bits and aligns to 16 bytes (the layout's `v96:128:128`).
    let float3 = "!\"air.arg_type_size\", i32 12, !\"air.arg_type_align_size\", i32 16, \
                  !\"air.arg_type_name\", !\"float3\"";
    let uint = "!\"air.arg_type_size\", i32 4, !\"air.arg_type_align_size\", i32 4, \
                !\"air.arg_type_name\", !\"uint\"";
    assert!(text.contains(float3) && text.contains(uint), "{text}");
}

/// The vertex point size, as Metal writes it:
///
/// ```metal
/// struct VO { float4 mtl_position [[position]]; float mtl_point_size [[point_size]]; };
/// vertex VO vmain() { ... }
/// ```
///
/// gives `!{!"air.point_size", !"air.arg_type_name", !"float", !"air.arg_name", !"mtl_point_size"}`
/// beside the position's (which this builder marks invariant, as it always has).
#[test]
fn vertex_point_size_metadata() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"point_size");
    let mut desc = FunctionSignatureDesc::default();
    desc.define_output(FunctionOutput::Position {
        ty: MslValue::FLOAT4,
    });
    desc.define_output(FunctionOutput::PointSize);
    let (_, entry) = ok(create_function(
        &m,
        &desc,
        b"main",
        false,
        &Handles::default(),
    ));
    metadata::add_entry(&m, c"air.vertex", entry);
    let text = String::from_utf8_lossy(&m.print()).into_owned();
    assert!(text.contains("<{ <4 x float>, float }> @main()"), "{text}");
    let md = named(&text, "air.vertex");
    let size = "!{!\"air.point_size\", !\"air.arg_type_name\", !\"float\", \
                !\"air.arg_name\", !\"mtl_point_size\"}";
    assert!(md.contains(size), "{md}");
}

/// The fragment point coordinate, as Metal writes it:
///
/// ```metal
/// fragment float4 fmain(float2 mtl_point_coord [[point_coord]]) { ... }
/// ```
///
/// gives `!{i32 0, !"air.point_coord", !"air.arg_type_name", !"float2", !"air.arg_name",
/// !"mtl_point_coord"}`.
#[test]
fn fragment_point_coord_metadata() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"point_coord");
    let mut desc = FunctionSignatureDesc::default();
    desc.define_input(FunctionInput::PointCoord);
    let (_, entry) = ok(create_function(
        &m,
        &desc,
        b"main",
        false,
        &Handles::default(),
    ));
    metadata::add_entry(&m, c"air.fragment", entry);
    let text = String::from_utf8_lossy(&m.print()).into_owned();
    assert!(text.contains("declare void @main(<2 x float>)"), "{text}");
    let md = named(&text, "air.fragment");
    let coord = "!{i32 0, !\"air.point_coord\", !\"air.arg_type_name\", !\"float2\", \
                 !\"air.arg_name\", !\"mtl_point_coord\"}";
    assert!(md.contains(coord), "{md}");
}

/// Top-level textures, a sampler and a buffer (D3D9's first production use), as Metal writes them:
///
/// ```metal
/// fragment float4 tmain(texture2d<float> t0 [[texture(0)]], depth2d<float> t1 [[texture(1)]],
///     texturecube<float> t2 [[texture(2)]], texture3d<float> t3 [[texture(3)]],
///     sampler s0 [[sampler(4)]], constant float4* b [[buffer(5)]]) { ... }
/// ```
///
/// gives, per argument (Metal spells `air.arg_type_name` `texture2d<float, sample>`, with a space
/// that this builder, as `MSLTexture::get_name` did, leaves out):
///
/// ```text
/// !{i32 0, !"air.texture", !"air.location_index", i32 0, i32 1, !"air.sample",
///   !"air.arg_type_name", !"texture2d<float, sample>", !"air.arg_name", !"t0"}
/// !{i32 4, !"air.sampler", !"air.location_index", i32 4, i32 1,
///   !"air.arg_type_name", !"sampler", !"air.arg_name", !"s0"}
/// !{i32 5, !"air.buffer", !"air.location_index", i32 5, i32 1, !"air.read",
///   !"air.address_space", i32 2, !"air.arg_type_size", i32 16, !"air.arg_type_align_size",
///   i32 16, !"air.arg_type_name", !"float4", !"air.arg_name", !"b"}
/// ```
#[test]
fn top_level_texture_sampler_buffer_metadata() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"top_level");
    let mut desc = FunctionSignatureDesc::default();
    for (slot, kind) in [
        (0, TextureKind::Texture2d),
        (1, TextureKind::Depth2d),
        (2, TextureKind::TextureCube),
        (3, TextureKind::Texture3d),
    ] {
        desc.define_input(FunctionInput::Texture(TextureBinding {
            location_index: slot,
            array_size: 1,
            memory_access: MemoryAccess::Sample,
            ty: MslTexture {
                component: MslScalar::Float,
                access: MemoryAccess::Sample,
                kind,
                kind_logical: kind,
            },
            arg_name: format!("t{slot}"),
            raster_order_group: None,
        }));
    }
    desc.define_input(FunctionInput::Sampler(SamplerBinding {
        location_index: 4,
        array_size: 1,
        arg_name: "s0".into(),
    }));
    desc.define_input(FunctionInput::Buffer(BufferBinding {
        buffer_size: None,
        location_index: 5,
        array_size: 1,
        memory_access: MemoryAccess::Read,
        address_space: AddressSpace::Constant,
        ty: MslType::Value(MslValue::FLOAT4),
        arg_name: "b".into(),
        raster_order_group: None,
    }));
    let (_, entry) = ok(create_function(
        &m,
        &desc,
        b"main",
        false,
        &Handles::default(),
    ));
    metadata::add_entry(&m, c"air.fragment", entry);
    let text = String::from_utf8_lossy(&m.print()).into_owned();
    let line = "declare void @main(ptr addrspace(1), ptr addrspace(1), ptr addrspace(1), \
                ptr addrspace(1), ptr addrspace(2), ptr addrspace(2))";
    assert!(text.contains(line), "{text}");
    let md = named(&text, "air.fragment");
    for (slot, name) in [
        (0, "texture2d<float,sample>"),
        (1, "depth2d<float,sample>"),
        (2, "texturecube<float,sample>"),
        (3, "texture3d<float,sample>"),
    ] {
        let node = format!(
            "!{{i32 {slot}, !\"air.texture\", !\"air.location_index\", i32 {slot}, i32 1, \
             !\"air.sample\", !\"air.arg_type_name\", !\"{name}\", !\"air.arg_name\", \
             !\"t{slot}\"}}"
        );
        assert!(md.contains(&node), "{node}\n{md}");
    }
    let sampler = "!{i32 4, !\"air.sampler\", !\"air.location_index\", i32 4, i32 1, \
                   !\"air.arg_type_name\", !\"sampler\", !\"air.arg_name\", !\"s0\"}";
    let buffer = "!{i32 5, !\"air.buffer\", !\"air.location_index\", i32 5, i32 1, !\"air.read\", \
                  !\"air.address_space\", i32 2, !\"air.arg_type_size\", i32 16, \
                  !\"air.arg_type_align_size\", i32 16, !\"air.arg_type_name\", !\"float4\", \
                  !\"air.arg_name\", !\"b\"}";
    assert!(md.contains(sampler), "{md}");
    assert!(md.contains(buffer), "{md}");
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
