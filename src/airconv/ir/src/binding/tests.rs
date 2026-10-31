use super::*;

fn stage_in(attribute: u32, name: &str) -> FunctionInput {
    FunctionInput::VertexStageIn {
        attribute,
        component_type: 3,
        name: name.into(),
    }
}

#[test]
fn define_input_answers_the_first_of_its_kind() {
    let mut s = FunctionSignatureDesc::default();
    assert_eq!(s.define_input(stage_in(0, "a")), 0);
    assert_eq!(
        s.define_input(FunctionInput::Position {
            interpolation: Interpolation::Flat
        }),
        1
    );
    // A stage-in attribute is the same by its attribute alone.
    assert_eq!(s.define_input(stage_in(0, "b")), 0);
    assert_eq!(s.define_input(stage_in(1, "a")), 2);
    // Any two positions are the same input, whatever their interpolation.
    let position = FunctionInput::Position {
        interpolation: Interpolation::CenterPerspective,
    };
    assert_eq!(s.define_input(position), 1);
    let sampler = |location_index| {
        FunctionInput::Sampler(SamplerBinding {
            location_index,
            array_size: 1,
            arg_name: String::new(),
        })
    };
    assert_eq!(s.define_input(sampler(7)), 3);
    assert_eq!(s.define_input(sampler(8)), 4);
    assert_eq!(s.define_input(sampler(7)), 3);
    assert_eq!(s.inputs().len(), 5);
}

#[test]
fn define_output_matches_by_name_and_index() {
    let mut s = FunctionSignatureDesc::default();
    let vertex = |user: &str, ty| FunctionOutput::Vertex {
        user: user.into(),
        ty,
    };
    assert_eq!(s.define_output(vertex("reg1_0", MslValue::FLOAT4)), 0);
    assert_eq!(s.define_output(vertex("reg1_0", MslValue::UINT4)), 0);
    assert_eq!(s.define_output(vertex("reg2_0", MslValue::FLOAT4)), 1);
    assert_eq!(s.define_output(FunctionOutput::StencilRef), 2);
    assert_eq!(s.define_output(FunctionOutput::StencilRef), 2);
    let data = |user: &str, index| {
        MeshVertexOutput::Data(MeshData {
            user: user.into(),
            ty: MslValue::FLOAT4,
            index,
        })
    };
    assert_eq!(s.define_mesh_vertex_output(data("reg1_0", 0)), 0);
    assert_eq!(s.define_mesh_vertex_output(data("reg1_0", 1)), 1);
    assert_eq!(s.define_mesh_vertex_output(data("reg1_0", 0)), 0);
}

#[test]
fn table_fields_take_their_location_and_bind_1d_as_2d() {
    let mut t = ArgumentTable::default();
    let texture = t.define_texture(
        "t0".into(),
        TextureKind::Texture1dArray,
        MemoryAccess::Sample,
        MslScalar::Float,
        128,
        None,
    );
    let default_location = t.define_integer64("meta_t0".into(), u32::MAX);
    assert_eq!((texture, default_location, t.len()), (0, 1, 2));
    let [TableField::Texture(tex), TableField::IndirectConstant(meta)] = t.fields() else {
        panic!("{:?}", t.fields());
    };
    assert_eq!(
        (tex.ty.kind, tex.ty.kind_logical),
        (TextureKind::Texture2dArray, TextureKind::Texture1dArray)
    );
    assert_eq!((tex.location_index, meta.location_index), (128, 1));
}

#[test]
fn argument_index_wraps_as_the_cxx() {
    assert_eq!(argument_index(BindingType::ConstantBuffer, 13), 13);
    assert_eq!(argument_index(BindingType::Sampler, 15), 47);
    assert_eq!(argument_index(BindingType::Srv, 127), 509);
    assert_eq!(argument_index(BindingType::Uav, 63), 701);
    assert_eq!(argument_index(BindingType::Uav, u32::MAX), 509);
}
