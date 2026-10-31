//! Function signatures and argument buffers, with their AIR metadata.
//!
//! Ported from `air_signature.cpp`: [`create_function`] is `FunctionSignatureBuilder::
//! CreateFunction` (`:522-1050`) and [`build_argument_buffer`] `ArgumentBufferBuilder::Build`
//! (`:361-441`), lowering `airconv_ir::binding`'s descriptors. Each element's metadata tuple is
//! made operand by operand in the C++'s order (`StreamMDHelper`, here `Md`); sizes, alignments
//! and offsets are the module's data layout's.
//!
//! A descriptor names what it cannot hold (an argument buffer's structure and its metadata, a
//! draw-arguments structure) by a `u32` handle into [`Handles`].

use core::marker::PhantomData;

use airconv_ir::{
    binding::{
        ArgumentTable, BufferBinding, DepthArgument, FunctionInput, FunctionOutput,
        FunctionSignatureDesc, IndirectBufferBinding, Interpolation, MemoryAccess,
        MeshOutputTopology, MeshPrimitiveOutput, MeshVertexOutput, MslArgType, MslScalar,
        MslTexture, MslType, MslValue, MslVector, SamplerBinding, TableField, TextureBinding,
        TextureKind, count_u32,
    },
    handlers::UNKNOWN_COMPONENT_TYPE,
};

use crate::{
    api::Error,
    llvm::{Context, Func, Module, Ty, TyCx, sys},
};

/// A function's entry, `!{fn, outputs, inputs, …}`, for its stage's named metadata.
///
/// Made by [`create_function`], of the module's context; [`super::metadata::add_entry`] adds it.
pub struct Entry<'cx> {
    node: sys::MetadataRef,
    cx: PhantomData<&'cx Context>,
}

impl Entry<'_> {
    /// The metadata node.
    #[must_use]
    pub const fn node(self) -> sys::MetadataRef {
        self.node
    }
}

/// The LLVM types and metadata nodes a compile's descriptors name by handle.
///
/// `MslType::Struct::ty` and `IndirectBufferBinding::struct_type` are type handles,
/// `IndirectBufferBinding::struct_type_info` a metadata handle; [`build_argument_buffer`] makes
/// both of an argument buffer, [`Handles::add_ty`] a type's (`AirTypes`' draw-arguments structs).
#[derive(Default)]
pub struct Handles {
    tys: Vec<Ty>,
    mds: Vec<sys::MetadataRef>,
}

impl Handles {
    /// The handle of `ty`.
    pub fn add_ty(&mut self, ty: Ty) -> u32 {
        let handle = count_u32(self.tys.len());
        self.tys.push(ty);
        handle
    }

    fn add_md(&mut self, node: sys::MetadataRef) -> u32 {
        let handle = count_u32(self.mds.len());
        self.mds.push(node);
        handle
    }

    fn ty(&self, handle: u32) -> Result<Ty, Error> {
        self.tys
            .get(handle as usize)
            .copied()
            .ok_or_else(|| Error::new(b"invalid operation: a binding names an unknown type."))
    }

    fn md(&self, handle: u32) -> Result<sys::MetadataRef, Error> {
        self.mds
            .get(handle as usize)
            .copied()
            .ok_or_else(|| Error::new(b"invalid operation: a binding names unknown metadata."))
    }
}

/// `ArgumentBufferBuilder::Build`: the argument buffer's structure and its metadata.
///
/// The structure is a new `argument_buffer_struct` (LLVM suffixes a second one's name); both are
/// added to `handles`, whose handles of them are answered, `(struct_type, struct_type_info)`.
///
/// # Errors
///
/// A field of an unknown type handle.
pub fn build_argument_buffer(
    m: &Module<'_>,
    table: &ArgumentTable,
    handles: &mut Handles,
) -> Result<(u32, u32), Error> {
    let cx = m.cx();
    let tcx = cx.tcx();
    let mut fields = Vec::with_capacity(table.len());
    let mut entries = Vec::with_capacity(table.len());
    for (offset, field) in (0_u32..).zip(table.fields()) {
        let mut md = Md::new(cx);
        md.integer(offset.into());
        let (ty, type_name, arg_name) = match field {
            TableField::Buffer(b) => {
                let (ty, name) = buffer(&mut md, m, b, handles)?;
                (ty, name, &b.arg_name)
            }
            TableField::IndirectBuffer(b) => {
                let (ty, name) = indirect_buffer(&mut md, m, b, handles)?;
                (ty, name, &b.arg_name)
            }
            TableField::Texture(t) => {
                let (ty, name) = texture(&mut md, tcx, t);
                (ty, name, &t.arg_name)
            }
            TableField::Sampler(s) => {
                let (ty, name) = sampler(&mut md, tcx, s);
                (ty, name, &s.arg_name)
            }
            TableField::IndirectConstant(c) => {
                let name = arg_type_name(&c.ty).into_bytes();
                md.string(b"air.indirect_constant")
                    .string(b"air.location_index")
                    .integer(c.location_index.into())
                    .integer(c.array_size.into())
                    .names(&name, c.arg_name.as_bytes());
                (arg_type_ty(tcx, &c.ty, handles)?, name, &c.arg_name)
            }
        };
        fields.push(ty);
        entries.push((type_name, arg_name, md.tuple()));
    }
    let st = tcx.create_struct(c"argument_buffer_struct", Some((&fields, false)));
    let mut info = Md::new(cx);
    for ((i, &ty), (type_name, arg_name, tuple)) in (0_u32..).zip(&fields).zip(&entries) {
        // The field's size is its type's bits: a `bool` would be 0 (the C++'s FIXME).
        info.integer(layout::offset_of(m, st, i))
            .integer(layout::size_in_bits(m, ty) / 8)
            .integer(0)
            .string(type_name)
            .string(arg_name.as_bytes())
            .string(b"air.indirect_argument")
            .node(*tuple);
    }
    let info = info.tuple();
    Ok((handles.add_ty(st), handles.add_md(info)))
}

/// `FunctionSignatureBuilder::CreateFunction`: the function `name` and its entry's metadata.
///
/// The [`Entry`] goes under the stage's named metadata ([`super::metadata::add_entry`]). With
/// `skip_output` the function returns `void` and lists no output. The C++'s `sign_mask` is left
/// out: every caller passed 0.
///
/// # Errors
///
/// A vertex attribute of an unknown component type (P15, where the C++ fell off its switch), an
/// unknown handle, a name holding a NUL.
pub fn create_function<'cx>(
    m: &Module<'cx>,
    desc: &FunctionSignatureDesc,
    name: &[u8],
    skip_output: bool,
    handles: &Handles,
) -> Result<(Func<'cx>, Entry<'cx>), Error> {
    let cx = m.cx();
    let tcx = cx.tcx();
    let mesh = MeshOutputs {
        vertex: desc
            .mesh_vertex_outputs()
            .iter()
            .map(|o| mesh_vertex_output(cx, o))
            .collect(),
        primitive: desc
            .mesh_primitive_outputs()
            .iter()
            .map(|o| mesh_primitive_output(cx, o))
            .collect(),
    };
    let mut input_tys = Vec::with_capacity(desc.inputs().len());
    let mut input_mds = Vec::with_capacity(desc.inputs().len());
    for (i, input) in (0_u32..).zip(desc.inputs()) {
        let mut md = Md::new(cx);
        md.integer(i.into());
        input_tys.push(input_field(&mut md, m, input, handles, &mesh)?);
        input_mds.push(md.tuple());
    }
    let mut output_tys = Vec::new();
    let mut output_mds = Vec::new();
    if !skip_output {
        for output in desc.outputs() {
            let mut md = Md::new(cx);
            output_tys.push(output_field(&mut md, tcx, output));
            output_mds.push(md.tuple());
        }
    }
    let ret = if output_tys.is_empty() {
        tcx.void()
    } else {
        tcx.literal_struct(&output_tys, true)
    };
    let func = m.add_function(name, tcx.function(ret, &input_tys, false))?;
    let mut entry = vec![
        func.val().as_metadata(),
        cx.md_node(&output_mds),
        cx.md_node(&input_mds),
    ];
    if desc.early_fragment_tests() {
        entry.push(cx.md_string(b"early_fragment_tests"));
    }
    let size = desc.max_work_group_size();
    if size > 0 {
        entry.push(
            Md::new(cx)
                .string(b"air.max_work_group_size")
                .integer(size.into())
                .tuple(),
        );
        let key = b"max-work-group-size";
        let value = size.to_string();
        // SAFETY: the context is live; both strings hold their lengths.
        let attr = unsafe {
            sys::create_string_attribute(
                cx.raw(),
                key.as_ptr().cast(),
                count_u32(key.len()),
                value.as_ptr().cast(),
                count_u32(value.len()),
            )
        };
        let raw = func.val().raw();
        // SAFETY: a function just made; an attribute of its context.
        unsafe { sys::add_attribute_at_index(raw, sys::ATTRIBUTE_FUNCTION_INDEX, attr) };
    }
    let mesh_size = desc.max_mesh_work_group_size();
    if mesh_size > 0 {
        entry.push(
            Md::new(cx)
                .string(b"air.max_mesh_work_groups")
                .integer(mesh_size.into())
                .tuple(),
        );
    }
    let entry = Entry {
        node: cx.md_node(&entry),
        cx: PhantomData,
    };
    Ok((func, entry))
}

/// A metadata tuple made operand by operand (`StreamMDHelper`).
struct Md<'a> {
    cx: &'a Context,
    ops: Vec<sys::MetadataRef>,
}

impl<'a> Md<'a> {
    const fn new(cx: &'a Context) -> Self {
        Self {
            cx,
            ops: Vec::new(),
        }
    }

    fn string(&mut self, bytes: &[u8]) -> &mut Self {
        self.ops.push(self.cx.md_string(bytes));
        self
    }

    /// An `i32` operand: `APInt{32, value}` keeps the low 32 bits.
    fn integer(&mut self, value: u64) -> &mut Self {
        let i32_ty = self.cx.tcx().int(32);
        self.ops
            .push(self.cx.const_int(i32_ty, value, false).as_metadata());
        self
    }

    fn node(&mut self, node: sys::MetadataRef) -> &mut Self {
        self.ops.push(node);
        self
    }

    /// The four operands every element ends with: its type's name and its own.
    fn names(&mut self, type_name: &[u8], arg_name: &[u8]) -> &mut Self {
        self.string(b"air.arg_type_name")
            .string(type_name)
            .string(b"air.arg_name")
            .string(arg_name)
    }

    fn tuple(&self) -> sys::MetadataRef {
        self.cx.md_node(&self.ops)
    }
}

/// A mesh function's output tuples, made before its inputs as the C++ made them.
struct MeshOutputs {
    vertex: Vec<sys::MetadataRef>,
    primitive: Vec<sys::MetadataRef>,
}

/// One input's metadata after its index, and its parameter type.
fn input_field(
    md: &mut Md<'_>,
    m: &Module<'_>,
    input: &FunctionInput,
    handles: &Handles,
    mesh: &MeshOutputs,
) -> Result<Ty, Error> {
    const UINT: MslValue = MslValue::UINT;
    const UINT3: MslValue = MslValue::Vector(MslVector {
        dimension: 3,
        scalar: MslScalar::Uint,
    });
    let cx = m.cx();
    let tcx = cx.tcx();
    // A system value: its kind, its argument's name and its type; the others answer here.
    let (kind, arg_name, ty): (&[u8], &[u8], MslValue) = match input {
        FunctionInput::VertexStageIn {
            attribute,
            component_type,
            name,
        } => {
            let ty = attribute_type(*component_type)?;
            md.string(b"air.vertex_input")
                .string(b"air.location_index")
                .integer((*attribute).into())
                .integer(1)
                .names(value_name(ty).as_bytes(), name.as_bytes());
            return Ok(value_ty(tcx, ty));
        }
        FunctionInput::FragmentStageIn {
            user,
            ty,
            interpolation,
            pull_mode,
        } => {
            md.string(b"air.fragment_input")
                .string(format!("user({user})").as_bytes());
            interpolate(md, interpolation, *pull_mode);
            md.names(value_name(*ty).as_bytes(), user.as_bytes());
            return Ok(if *pull_mode {
                tcx.ptr(tcx.named_struct(c"struct._interpolant_t"), 1)
            } else {
                value_ty(tcx, *ty)
            });
        }
        FunctionInput::Buffer(b) => return Ok(buffer(md, m, b, handles)?.0),
        FunctionInput::IndirectBuffer(b) => return Ok(indirect_buffer(md, m, b, handles)?.0),
        FunctionInput::Texture(t) => return Ok(texture(md, tcx, t).0),
        FunctionInput::Sampler(s) => return Ok(sampler(md, tcx, s).0),
        FunctionInput::Position { interpolation } => {
            md.string(b"air.position");
            interpolate(md, interpolation, false);
            md.names(b"float4", b"mtl_position");
            return Ok(value_ty(tcx, MslValue::FLOAT4));
        }
        FunctionInput::Payload { size } => {
            md.string(b"air.payload")
                .string(b"air.arg_type_size")
                .integer((*size).into())
                .string(b"air.arg_type_align_size")
                .integer(4)
                .names(b"uint", b"mtl_payload");
            return Ok(tcx.ptr(tcx.int(32), 6));
        }
        FunctionInput::MeshGridProperties => {
            md.string(b"air.mesh_grid_properties")
                .names(b"mesh_grid_properties", b"mtl_mesh_grid_properties");
            return Ok(tcx.ptr(tcx.named_struct(c"struct._mesh_grid_properties_t"), 3));
        }
        FunctionInput::Mesh {
            vertex_count,
            primitive_count,
            topology,
        } => {
            let topology: &[u8] = match topology {
                MeshOutputTopology::Line => b"air.line",
                MeshOutputTopology::Triangle => b"air.triangle",
                MeshOutputTopology::Point => b"air.point",
            };
            let info = Md::new(cx)
                .string(b"air.mesh_type_info")
                .node(cx.md_node(&mesh.vertex))
                .node(cx.md_node(&mesh.primitive))
                .integer((*vertex_count).into())
                .integer((*primitive_count).into())
                .string(topology)
                .tuple();
            md.string(b"air.mesh")
                .node(info)
                .names(b"mesh<>", b"mtl_mesh");
            return Ok(tcx.ptr(tcx.named_struct(c"struct._mesh_t"), 7));
        }
        FunctionInput::FrontFacing => (
            b"air.front_facing",
            b"mtl_frontface",
            MslValue::Scalar(MslScalar::Bool),
        ),
        FunctionInput::SampleIndex => (b"air.sample_id", b"mtl_sample_index", UINT),
        FunctionInput::RenderTargetArrayIndex => {
            (b"air.render_target_array_index", b"mtl_rt_index", UINT)
        }
        FunctionInput::ViewportArrayIndex => (b"air.viewport_array_index", b"mtl_vp_index", UINT),
        FunctionInput::ThreadgroupPositionInGrid => (
            b"air.threadgroup_position_in_grid",
            b"mtl_threadgroup_position_in_grid",
            UINT3,
        ),
        FunctionInput::ThreadPositionInGrid => (
            b"air.thread_position_in_grid",
            b"mtl_thread_position_in_grid",
            UINT3,
        ),
        FunctionInput::ThreadPositionInThreadgroup => (
            b"air.thread_position_in_threadgroup",
            b"mtl_thread_position_in_threadgroup",
            UINT3,
        ),
        FunctionInput::ThreadIndexInThreadgroup => (
            b"air.thread_index_in_threadgroup",
            b"mtl_thread_index_in_threadgroup",
            UINT,
        ),
        FunctionInput::ThreadgroupsPerGrid => (
            b"air.threadgroups_per_grid",
            b"mtl_threadgroups_per_grid",
            UINT3,
        ),
        FunctionInput::ThreadIndexInSimdgroup => (
            b"air.thread_index_in_simdgroup",
            b"mtl_thread_index_in_simdgroup",
            UINT,
        ),
        FunctionInput::ThreadsPerSimdgroup => (
            b"air.threads_per_simdgroup",
            b"mtl_threads_per_simdgroup",
            UINT,
        ),
        FunctionInput::VertexId => (b"air.vertex_id", b"mtl_vertex_id", UINT),
        FunctionInput::BaseVertex => (b"air.base_vertex", b"mtl_base_vertex", UINT),
        FunctionInput::InstanceId => (b"air.instance_id", b"mtl_instance_id", UINT),
        FunctionInput::BaseInstance => (b"air.base_instance", b"mtl_base_instance", UINT),
        FunctionInput::PrimitiveId => (b"air.primitive_id", b"mtl_primitive_id", UINT),
        FunctionInput::InputCoverage => (b"air.sample_mask_in", b"mtl_sample_mask", UINT),
    };
    md.string(kind).names(value_name(ty).as_bytes(), arg_name);
    Ok(value_ty(tcx, ty))
}

/// One output's metadata and its field of the returned structure.
fn output_field(md: &mut Md<'_>, tcx: &TyCx, output: &FunctionOutput) -> Ty {
    let int = tcx.int(32);
    match output {
        FunctionOutput::Vertex { user, ty } => {
            md.string(b"air.vertex_output")
                .string(format!("user({user})").as_bytes())
                .names(value_name(*ty).as_bytes(), user.as_bytes());
            value_ty(tcx, *ty)
        }
        FunctionOutput::RenderTarget {
            dual_source_blending,
            index,
            ty,
        } => {
            md.string(b"air.render_target");
            if *dual_source_blending {
                md.integer(0).integer((*index).into());
            } else {
                md.integer((*index).into()).integer(0);
            }
            md.names(
                value_name(*ty).as_bytes(),
                format!("render_target_{index}").as_bytes(),
            );
            value_ty(tcx, *ty)
        }
        FunctionOutput::Position { ty } => {
            md.string(b"air.position")
                .string(b"air.invariant")
                .names(value_name(*ty).as_bytes(), b"mtl_position");
            value_ty(tcx, *ty)
        }
        FunctionOutput::Depth { argument } => {
            let qualifier: &[u8] = match argument {
                DepthArgument::Any => b"air.any",
                DepthArgument::Greater => b"air.greater",
                DepthArgument::Less => b"air.less",
            };
            md.string(b"air.depth")
                .string(b"air.depth_qualifier")
                .string(qualifier)
                .names(b"float", b"mtl_depth");
            tcx.float()
        }
        FunctionOutput::CoverageMask => {
            md.string(b"air.sample_mask")
                .names(b"uint", b"mtl_coverage_mask");
            int
        }
        FunctionOutput::StencilRef => {
            md.string(b"air.stencil").names(b"uint", b"mtl_stencil_ref");
            int
        }
        FunctionOutput::ClipDistance { count } => {
            md.string(b"air.clip_distance")
                .string(b"air.clip_distance_array_size")
                .integer(*count as u64)
                .names(b"float", b"mtl_clip_distance");
            tcx.array(tcx.float(), count_u32(*count))
        }
        FunctionOutput::RenderTargetArrayIndex => {
            md.string(b"air.render_target_array_index")
                .names(b"uint", b"mtl_render_target_array_index");
            int
        }
        FunctionOutput::ViewportArrayIndex => {
            md.string(b"air.viewport_array_index")
                .names(b"uint", b"mtl_viewport_array_index");
            int
        }
    }
}

/// A mesh function's per-vertex output tuple; position and clip distance as an output's.
fn mesh_vertex_output(cx: &Context, output: &MeshVertexOutput) -> sys::MetadataRef {
    let mut md = Md::new(cx);
    match output {
        MeshVertexOutput::Data(d) => {
            md.string(b"air.mesh_vertex_data")
                .integer(d.index.into())
                .string(format!("user({})", d.user).as_bytes())
                .names(value_name(d.ty).as_bytes(), d.user.as_bytes());
        }
        MeshVertexOutput::Position { ty } => {
            output_field(&mut md, cx.tcx(), &FunctionOutput::Position { ty: *ty });
        }
        MeshVertexOutput::ClipDistance { count } => {
            let clip = FunctionOutput::ClipDistance { count: *count };
            output_field(&mut md, cx.tcx(), &clip);
        }
        MeshVertexOutput::PointSize => {
            md.string(b"air.point_size")
                .names(b"float", b"mtl_point_size");
        }
    }
    md.tuple()
}

/// A mesh function's per-primitive output tuple; the indices as an output's.
fn mesh_primitive_output(cx: &Context, output: &MeshPrimitiveOutput) -> sys::MetadataRef {
    let mut md = Md::new(cx);
    match output {
        MeshPrimitiveOutput::Data(d) => {
            md.string(b"air.mesh_primitive_data")
                .integer(d.index.into())
                .string(format!("user({})", d.user).as_bytes())
                .names(value_name(d.ty).as_bytes(), d.user.as_bytes());
        }
        MeshPrimitiveOutput::RenderTargetArrayIndex => {
            output_field(&mut md, cx.tcx(), &FunctionOutput::RenderTargetArrayIndex);
        }
        MeshPrimitiveOutput::ViewportArrayIndex => {
            output_field(&mut md, cx.tcx(), &FunctionOutput::ViewportArrayIndex);
        }
    }
    md.tuple()
}

/// `build_argument_binding_buffer`: the metadata and the pointer type, with the type's name.
fn buffer(
    md: &mut Md<'_>,
    m: &Module<'_>,
    b: &BufferBinding,
    handles: &Handles,
) -> Result<(Ty, Vec<u8>), Error> {
    md.string(b"air.buffer");
    if let Some(size) = b.buffer_size {
        md.string(b"air.buffer_size").integer(size.into());
    }
    md.string(b"air.location_index")
        .integer(b.location_index.into())
        .integer(b.array_size.into());
    buffer_access(md, b.memory_access);
    let address_space = address_space(&b.address_space);
    md.string(b"air.address_space")
        .integer(address_space.into());
    if let Some(group) = b.raster_order_group {
        md.string(b"air.raster_order_group").integer(group.into());
    }
    let ty = type_ty(m.cx().tcx(), &b.ty, handles)?;
    let name = type_name(&b.ty).into_bytes();
    md.string(b"air.arg_type_size")
        .integer(layout::size_in_bits(m, ty) / 8)
        .string(b"air.arg_type_align_size")
        .integer(layout::abi_align(m, ty).into())
        .names(&name, b.arg_name.as_bytes());
    Ok((m.cx().tcx().ptr(ty, address_space), name))
}

/// `build_argument_binding_indirect_buffer`: sizes from the structure's layout.
fn indirect_buffer(
    md: &mut Md<'_>,
    m: &Module<'_>,
    b: &IndirectBufferBinding,
    handles: &Handles,
) -> Result<(Ty, Vec<u8>), Error> {
    let st = handles.ty(b.struct_type)?;
    let size = layout::size_in_bits(m, st) / 8;
    md.string(b"air.indirect_buffer")
        .string(b"air.buffer_size")
        .integer(size.wrapping_mul(b.array_size.into()))
        .string(b"air.location_index")
        .integer(b.location_index.into())
        .integer(b.array_size.into());
    buffer_access(md, b.memory_access);
    let address_space = address_space(&b.address_space);
    let name = layout::struct_name(m, st);
    md.string(b"air.address_space")
        .integer(address_space.into())
        .string(b"air.struct_type_info")
        .node(handles.md(b.struct_type_info)?)
        .string(b"air.arg_type_size")
        .integer(size)
        .string(b"air.arg_type_align_size")
        .integer(layout::abi_align(m, st).into())
        .names(&name, b.arg_name.as_bytes());
    Ok((m.cx().tcx().ptr(st, address_space), name))
}

/// `build_argument_binding_texture`.
fn texture(md: &mut Md<'_>, tcx: &TyCx, t: &TextureBinding) -> (Ty, Vec<u8>) {
    let name = texture_name(&t.ty).into_bytes();
    md.string(b"air.texture")
        .string(b"air.location_index")
        .integer(t.location_index.into())
        .integer(t.array_size.into());
    md.string(match t.memory_access {
        MemoryAccess::Read => b"air.read",
        MemoryAccess::Write => b"air.write",
        MemoryAccess::ReadWrite => b"air.read_write",
        MemoryAccess::Sample => b"air.sample",
    });
    if let Some(group) = t.raster_order_group {
        md.string(b"air.raster_order_group").integer(group.into());
    }
    md.names(&name, t.arg_name.as_bytes());
    (texture_ty(tcx, t.ty.kind), name)
}

/// `build_argument_binding_sampler`.
fn sampler(md: &mut Md<'_>, tcx: &TyCx, s: &SamplerBinding) -> (Ty, Vec<u8>) {
    md.string(b"air.sampler")
        .string(b"air.location_index")
        .integer(s.location_index.into())
        .integer(s.array_size.into())
        .names(b"sampler", s.arg_name.as_bytes());
    (sampler_ty(tcx), b"sampler".to_vec())
}

/// A buffer's access; `sample` adds nothing (the C++'s release build passed its `assert(0)`).
fn buffer_access(md: &mut Md<'_>, access: MemoryAccess) {
    match access {
        MemoryAccess::Read => md.string(b"air.read"),
        MemoryAccess::Write => md.string(b"air.write"),
        MemoryAccess::ReadWrite => md.string(b"air.read_write"),
        MemoryAccess::Sample => md,
    };
}

/// `insert_inteterpolation`; a pulled flat input adds nothing (the C++'s `assert(0)`).
fn interpolate(md: &mut Md<'_>, interpolation: &Interpolation, pull_mode: bool) {
    let (center, perspective): (&[u8], &[u8]) = match interpolation {
        Interpolation::CenterPerspective => (b"air.center", b"air.perspective"),
        Interpolation::CenterNoPerspective => (b"air.center", b"air.no_perspective"),
        Interpolation::CentroidPerspective => (b"air.centroid", b"air.perspective"),
        Interpolation::CentroidNoPerspective => (b"air.centroid", b"air.no_perspective"),
        Interpolation::SamplePerspective => (b"air.sample", b"air.perspective"),
        Interpolation::SampleNoPerspective => (b"air.sample", b"air.no_perspective"),
        Interpolation::Flat => {
            if !pull_mode {
                md.string(b"air.flat");
            }
            return;
        }
    };
    if pull_mode {
        md.string(b"air.interpolation_function").string(perspective);
    } else {
        md.string(center).string(perspective);
    }
}

/// `get_input_attribute_type` under the sign mask every caller passed, 0.
fn attribute_type(component_type: u32) -> Result<MslValue, Error> {
    match component_type {
        0 | 1 => Ok(MslValue::UINT4),
        2 => Ok(MslValue::INT4),
        3 => Ok(MslValue::FLOAT4),
        _ => Err(UNKNOWN_COMPONENT_TYPE.into()),
    }
}

const fn address_space(a: &airconv_ir::binding::AddressSpace) -> u32 {
    use airconv_ir::binding::AddressSpace as A;
    match a {
        A::Unknown => 0,
        A::Device => 1,
        A::Constant => 2,
        A::Threadgroup => 3,
        A::ObjectData => 6,
    }
}

const fn scalar_name(s: MslScalar) -> &'static str {
    match s {
        MslScalar::Float => "float",
        MslScalar::Int => "int",
        MslScalar::Uint => "uint",
        MslScalar::Bool => "bool",
        MslScalar::Half => "half",
        MslScalar::Ushort => "ushort",
        MslScalar::Ulong => "ulong",
    }
}

fn scalar_ty(tcx: &TyCx, s: MslScalar) -> Ty {
    match s {
        MslScalar::Float => tcx.float(),
        MslScalar::Int | MslScalar::Uint => tcx.int(32),
        MslScalar::Bool => tcx.int(1),
        MslScalar::Half => tcx.half(),
        MslScalar::Ushort => tcx.int(16),
        MslScalar::Ulong => tcx.int(64),
    }
}

/// `MSLVector::get_name`: a dimension of 1 is the scalar's name.
fn value_name(v: MslValue) -> String {
    match v {
        MslValue::Vector(MslVector { dimension, scalar }) if dimension != 1 => {
            format!("{}{dimension}", scalar_name(scalar))
        }
        MslValue::Scalar(scalar) | MslValue::Vector(MslVector { scalar, .. }) => {
            scalar_name(scalar).to_owned()
        }
    }
}

fn value_ty(tcx: &TyCx, v: MslValue) -> Ty {
    match v {
        MslValue::Vector(MslVector { dimension, scalar }) if dimension != 1 => {
            tcx.vector(scalar_ty(tcx, scalar), dimension)
        }
        MslValue::Scalar(scalar) | MslValue::Vector(MslVector { scalar, .. }) => {
            scalar_ty(tcx, scalar)
        }
    }
}

/// A texture kind's MSL name and its structure's name.
const fn texture_names(kind: TextureKind) -> (&'static str, &'static core::ffi::CStr) {
    match kind {
        TextureKind::Texture1d => ("texture1d", c"struct._texture_1d_t"),
        TextureKind::Texture1dArray => ("texture1d_array", c"struct._texture_1d_array_t"),
        TextureKind::Texture2d => ("texture2d", c"struct._texture_2d_t"),
        TextureKind::Texture2dArray => ("texture2d_array", c"struct._texture_2d_array_t"),
        TextureKind::Texture2dMs => ("texture2d_ms", c"struct._texture_2d_ms_t"),
        TextureKind::Texture2dMsArray => ("texture2d_ms_array", c"struct._texture_2d_ms_array_t"),
        TextureKind::Texture3d => ("texture3d", c"struct._texture_3d_t"),
        TextureKind::TextureBuffer => ("texture_buffer", c"struct._texture_buffer_1d_t"),
        TextureKind::TextureCube => ("texturecube", c"struct._texture_cube_t"),
        TextureKind::TextureCubeArray => ("texturecube_array", c"struct._texture_cube_array_t"),
        TextureKind::Depth2d => ("depth2d", c"struct._depth_2d_t"),
        TextureKind::Depth2dArray => ("depth2d_array", c"struct._depth_2d_array_t"),
        TextureKind::Depth2dMs => ("depth2d_ms", c"struct._depth_2d_ms_t"),
        TextureKind::Depth2dMsArray => ("depth2d_ms_array", c"struct._depth_2d_ms_array_t"),
        TextureKind::DepthCube => ("depthcube", c"struct._depth_cube_t"),
        TextureKind::DepthCubeArray => ("depthcube_array", c"struct._depth_cube_array_t"),
    }
}

/// `MSLTexture::get_name`, of the kind Metal binds.
fn texture_name(t: &MslTexture) -> String {
    let access = match t.access {
        MemoryAccess::Read => "read",
        MemoryAccess::ReadWrite => "read_write",
        MemoryAccess::Write => "write",
        MemoryAccess::Sample => "sample",
    };
    let (kind, _) = texture_names(t.kind);
    format!("{kind}<{},{access}>", scalar_name(t.component))
}

fn texture_ty(tcx: &TyCx, kind: TextureKind) -> Ty {
    tcx.ptr(tcx.named_struct(texture_names(kind).1), 1)
}

/// Samplers are in the constant address space.
fn sampler_ty(tcx: &TyCx) -> Ty {
    tcx.ptr(tcx.named_struct(c"struct._sampler_t"), 2)
}

fn type_name(t: &MslType) -> String {
    match t {
        MslType::Value(v) => value_name(*v),
        MslType::Sampler => "sampler".to_owned(),
        MslType::Texture(t) => texture_name(t),
        MslType::Struct { name, .. } => name.clone(),
    }
}

fn type_ty(tcx: &TyCx, t: &MslType, handles: &Handles) -> Result<Ty, Error> {
    Ok(match t {
        MslType::Value(v) => value_ty(tcx, *v),
        MslType::Sampler => sampler_ty(tcx),
        MslType::Texture(t) => texture_ty(tcx, t.kind),
        MslType::Struct { ty, .. } => handles.ty(*ty)?,
    })
}

/// `MSLStaticArray::get_name` and `MSLPointer::get_name` (the pointee's).
fn arg_type_name(t: &MslArgType) -> String {
    match t {
        MslArgType::Type(t) | MslArgType::Pointer { pointee: t, .. } => type_name(t),
        MslArgType::Array { size, element } => format!("array<{},{size}>", type_name(element)),
    }
}

fn arg_type_ty(tcx: &TyCx, t: &MslArgType, handles: &Handles) -> Result<Ty, Error> {
    Ok(match t {
        MslArgType::Type(t) => type_ty(tcx, t, handles)?,
        MslArgType::Array { size, element } => tcx.array(type_ty(tcx, element, handles)?, *size),
        MslArgType::Pointer {
            pointee,
            address_space: a,
        } => tcx.ptr(type_ty(tcx, pointee, handles)?, address_space(a)),
    })
}

/// The module's data layout, through `llvm-c/Target.h`.
///
/// Declared here, not in [`crate::llvm::sys`] (lane l's file): a request to move them there is
/// ledgered.
mod layout {
    use core::{
        ffi::{CStr, c_uint, c_ulonglong},
        marker::{PhantomData, PhantomPinned},
    };

    use crate::llvm::{Module, Ty, sys};

    /// `LLVMOpaqueTargetData`.
    #[repr(C)]
    struct TargetData {
        _data: [u8; 0],
        _marker: PhantomData<(*mut u8, PhantomPinned)>,
    }

    unsafe extern "C" {
        #[link_name = "LLVMGetModuleDataLayout"]
        fn get_module_data_layout(m: sys::ModuleRef) -> *mut TargetData;
        #[link_name = "LLVMSizeOfTypeInBits"]
        fn size_of_type_in_bits(td: *mut TargetData, ty: sys::TypeRef) -> c_ulonglong;
        #[link_name = "LLVMABIAlignmentOfType"]
        fn abi_alignment_of_type(td: *mut TargetData, ty: sys::TypeRef) -> c_uint;
        #[link_name = "LLVMOffsetOfElement"]
        fn offset_of_element(
            td: *mut TargetData,
            struct_ty: sys::TypeRef,
            element: c_uint,
        ) -> c_ulonglong;
    }

    /// The module's own layout, valid while it lives and keeps it.
    fn data_layout(m: &Module<'_>) -> *mut TargetData {
        // SAFETY: a live module; the answer is its layout, never disposed of.
        unsafe { get_module_data_layout(m.raw()) }
    }

    /// `DataLayout::getTypeSizeInBits` of the sized `ty`.
    pub fn size_in_bits(m: &Module<'_>, ty: Ty) -> u64 {
        let raw = m.cx().tcx().lower(ty);
        // SAFETY: the module's layout; a sized type of its context.
        unsafe { size_of_type_in_bits(data_layout(m), raw) }
    }

    /// `DataLayout::getABITypeAlign` of the sized `ty`, in bytes.
    pub fn abi_align(m: &Module<'_>, ty: Ty) -> u32 {
        let raw = m.cx().tcx().lower(ty);
        // SAFETY: the module's layout; a sized type of its context.
        unsafe { abi_alignment_of_type(data_layout(m), raw) }
    }

    /// `StructLayout::getElementOffset` of field `i` of the struct `st`, which has it.
    pub fn offset_of(m: &Module<'_>, st: Ty, i: u32) -> u64 {
        let raw = m.cx().tcx().lower(st);
        // SAFETY: the module's layout; a struct with a body of more than `i` fields.
        unsafe { offset_of_element(data_layout(m), raw, i) }
    }

    /// `StructType::getName`: empty for a literal struct.
    pub fn struct_name(m: &Module<'_>, st: Ty) -> Vec<u8> {
        let raw = m.cx().tcx().lower(st);
        // SAFETY: a struct type of this context.
        let name = unsafe { sys::get_struct_name(raw) };
        if name.is_null() {
            return Vec::new();
        }
        // SAFETY: LLVM's NUL-terminated name, alive with the type.
        unsafe { CStr::from_ptr(name) }.to_bytes().to_vec()
    }
}

#[cfg(test)]
mod tests;
