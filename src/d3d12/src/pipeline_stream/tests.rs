use core::{fmt::Write as _, ptr};

use super::*;
use crate::abi::pipeline::{
    DepthStencilOpDesc, RenderTargetBlendDesc, SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT as DS_FORMAT,
};

/// One kind of subobject for the alignment test.
///
/// Its name, how to write one, and where the next starts on a 64-bit target (the `{ type, data }`
/// struct's size rounded up to 8).
struct Case(&'static str, fn(&mut Stream), usize);

/// A stream under construction, subobject by subobject, as d3dx12 lays them out.
struct Stream(Vec<u8>);

impl Stream {
    const fn new() -> Self {
        Self(Vec::new())
    }

    /// Appends `{ ty, data }` at the next multiple of a pointer's size, with no padding behind it.
    fn push<T: Plain>(&mut self, ty: u32, data: T) -> &mut Self {
        self.pad_to(size_of::<usize>());
        let at = self.0.len();
        self.0.resize(at + size_of::<Subobject<T>>(), 0);
        // SAFETY: the buffer has `size_of::<Subobject<T>>()` bytes from `at`, just added; the write
        // does not need alignment.
        unsafe {
            self.0[at..]
                .as_mut_ptr()
                .cast::<Subobject<T>>()
                .write_unaligned(Subobject { ty, data });
        }
        self
    }

    /// Zero bytes up to the next multiple of `to`.
    fn pad_to(&mut self, to: usize) -> &mut Self {
        let len = self.0.len().next_multiple_of(to);
        self.0.resize(len, 0);
        self
    }

    fn parse(&self) -> Result<ParsedStream, StreamError> {
        parse_pipeline_stream(&self.0, &mut |_| panic!("no warning expected"))
    }
}

fn address(value: usize) -> *const c_void {
    ptr::without_provenance(value)
}

fn shader(value: usize) -> ShaderBytecode {
    ShaderBytecode {
        shader_bytecode: address(value),
        bytecode_length: value + 1,
    }
}

fn op(base: u32) -> DepthStencilOpDesc {
    DepthStencilOpDesc {
        stencil_fail_op: base,
        stencil_depth_fail_op: base + 1,
        stencil_pass_op: base + 2,
        stencil_func: base + 3,
    }
}

fn sample_desc(count: u32, quality: u32) -> SampleDesc {
    SampleDesc { count, quality }
}

fn cached(value: usize) -> CachedPipelineState {
    CachedPipelineState {
        cached_blob: address(value),
        cached_blob_size_in_bytes: value + 5,
    }
}

fn depth_stencil1(bounds: i32) -> DepthStencilDesc1 {
    DepthStencilDesc1 {
        depth_enable: 0,
        depth_write_mask: 0,
        depth_func: 4,
        stencil_enable: 1,
        stencil_read_mask: 0x12,
        stencil_write_mask: 0x34,
        front_face: op(10),
        back_face: op(20),
        depth_bounds_test_enable: bounds,
    }
}

fn view_instancing(flags: u32) -> ViewInstancingDesc {
    ViewInstancingDesc {
        view_instance_count: 2,
        view_instance_locations: ptr::null(),
        flags,
    }
}

fn null_shader() -> ShaderBytecode {
    shader_at(0)
}

fn shader_at(value: usize) -> ShaderBytecode {
    ShaderBytecode {
        shader_bytecode: address(value),
        bytecode_length: 0,
    }
}

#[test]
fn an_empty_stream_gives_the_cpps_defaults() {
    let parsed = Stream::new().parse().unwrap();
    assert!(!parsed.is_compute);
    let graphics = &parsed.graphics;
    assert!(graphics.root_signature.is_null());
    assert!(graphics.vs.shader_bytecode.is_null() && graphics.gs.shader_bytecode.is_null());
    let depth = &graphics.depth_stencil_state;
    assert_eq!(depth.depth_enable, 1);
    assert_eq!(depth.depth_write_mask, DEPTH_WRITE_MASK_ALL);
    assert_eq!(depth.depth_func, COMPARISON_FUNC_LESS);
    assert_eq!(depth.stencil_enable, 0);
    // D3D12's own defaults are 0xff; the C++ leaves both masks 0.
    assert_eq!((depth.stencil_read_mask, depth.stencil_write_mask), (0, 0));
    for face in [&depth.front_face, &depth.back_face] {
        assert_eq!(face.stencil_fail_op, STENCIL_OP_KEEP);
        assert_eq!(face.stencil_depth_fail_op, STENCIL_OP_KEEP);
        assert_eq!(face.stencil_pass_op, STENCIL_OP_KEEP);
        assert_eq!(face.stencil_func, COMPARISON_FUNC_ALWAYS);
    }
    let rasterizer = &graphics.rasterizer_state;
    assert_eq!(rasterizer.fill_mode, FILL_MODE_SOLID);
    assert_eq!(rasterizer.cull_mode, CULL_MODE_BACK);
    assert_eq!(rasterizer.depth_clip_enable, 1);
    assert_eq!(rasterizer.front_counter_clockwise, 0);
    assert_eq!(
        rasterizer.conservative_raster,
        CONSERVATIVE_RASTERIZATION_MODE_OFF
    );
    // Only render target 0 has a write mask.
    let masks = graphics
        .blend_state
        .render_target
        .each_ref()
        .map(|target: &RenderTargetBlendDesc| target.render_target_write_mask);
    assert_eq!(masks, [COLOR_WRITE_ENABLE_ALL, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(graphics.sample_desc.count, 1);
    assert_eq!(graphics.sample_desc.quality, 0);
    assert_eq!(graphics.sample_mask, DEFAULT_SAMPLE_MASK);
    assert_eq!(graphics.num_render_targets, 0);
    assert_eq!(graphics.rtv_formats, [0; 8]);
    assert_eq!(
        (graphics.dsv_format, graphics.node_mask, graphics.flags),
        (0, 0, 0)
    );
    assert!(graphics.cached_pso.cached_blob.is_null());
    assert!(graphics.input_layout.input_element_descs.is_null());
    assert!(graphics.stream_output.so_declaration.is_null());
    let compute = &parsed.compute;
    assert!(compute.root_signature.is_null() && compute.cs.shader_bytecode.is_null());
    assert_eq!((compute.node_mask, compute.flags), (0, 0));
}

#[test]
fn subobjects_land_where_the_cpp_puts_them() {
    let root = address(0x1000).cast_mut();
    let mut stream = Stream::new();
    stream
        .push(SUBOBJECT_TYPE_ROOT_SIGNATURE, root)
        .push(SUBOBJECT_TYPE_VS, shader(0x2000))
        .push(SUBOBJECT_TYPE_PS, shader(0x3000))
        .push(SUBOBJECT_TYPE_DS, shader(0x4000))
        .push(SUBOBJECT_TYPE_HS, shader(0x5000))
        .push(SUBOBJECT_TYPE_GS, shader(0x6000))
        .push(SUBOBJECT_TYPE_SAMPLE_MASK, 0x0f0f_u32)
        .push(SUBOBJECT_TYPE_IB_STRIP_CUT_VALUE, 2_u32)
        .push(SUBOBJECT_TYPE_PRIMITIVE_TOPOLOGY, 3_u32)
        .push(
            SUBOBJECT_TYPE_RENDER_TARGET_FORMATS,
            RtFormatArray {
                rt_formats: [28, 29, 30, 0, 0, 0, 0, 87],
                num_render_targets: 3,
            },
        )
        .push(DS_FORMAT, 45_u32)
        .push(SUBOBJECT_TYPE_SAMPLE_DESC, sample_desc(4, 2))
        .push(SUBOBJECT_TYPE_NODE_MASK, 5_u32)
        .push(SUBOBJECT_TYPE_CACHED_PSO, cached(0x7000))
        .push(SUBOBJECT_TYPE_FLAGS, 1_u32)
        .push(
            SUBOBJECT_TYPE_INPUT_LAYOUT,
            InputLayoutDesc {
                input_element_descs: ptr::null(),
                num_elements: 6,
            },
        )
        .push(
            SUBOBJECT_TYPE_STREAM_OUTPUT,
            StreamOutputDesc {
                so_declaration: ptr::null(),
                num_entries: 1,
                buffer_strides: ptr::null(),
                num_strides: 2,
                rasterized_stream: 3,
            },
        );
    let parsed = stream.parse().unwrap();
    assert!(!parsed.is_compute);
    let g = &parsed.graphics;
    assert_eq!(g.root_signature, root);
    for (got, want) in [
        (&g.vs, 0x2000),
        (&g.ps, 0x3000),
        (&g.ds, 0x4000),
        (&g.hs, 0x5000),
        (&g.gs, 0x6000),
    ] {
        assert_eq!(got.shader_bytecode, address(want));
        assert_eq!(got.bytecode_length, want + 1);
    }
    assert_eq!(g.sample_mask, 0x0f0f);
    assert_eq!(g.ib_strip_cut_value, 2);
    assert_eq!(g.primitive_topology_type, 3);
    assert_eq!(g.rtv_formats, [28, 29, 30, 0, 0, 0, 0, 87]);
    assert_eq!(g.num_render_targets, 3);
    assert_eq!(g.dsv_format, 45);
    assert_eq!((g.sample_desc.count, g.sample_desc.quality), (4, 2));
    assert_eq!(g.node_mask, 5);
    assert_eq!(g.cached_pso.cached_blob, address(0x7000));
    assert_eq!(g.cached_pso.cached_blob_size_in_bytes, 0x7005);
    assert_eq!(g.flags, 1);
    assert_eq!(g.input_layout.num_elements, 6);
    assert_eq!(g.stream_output.num_entries, 1);
    assert_eq!(g.stream_output.num_strides, 2);
    assert_eq!(g.stream_output.rasterized_stream, 3);
    // The root signature, node mask, cached state and flags are shared with the compute description.
    let c = &parsed.compute;
    assert_eq!(c.root_signature, root);
    assert_eq!(c.node_mask, 5);
    assert_eq!(c.cached_pso.cached_blob, address(0x7000));
    assert_eq!(c.cached_pso.cached_blob_size_in_bytes, 0x7005);
    assert_eq!(c.flags, 1);
    assert!(c.cs.shader_bytecode.is_null());
}

#[test]
fn a_compute_stream_is_compute_and_only_by_its_shader() {
    let mut stream = Stream::new();
    stream
        .push(SUBOBJECT_TYPE_CS, shader(0x9000))
        .push(SUBOBJECT_TYPE_PS, shader(0x3000));
    let parsed = stream.parse().unwrap();
    // A pixel shader beside the compute one is not refused.
    assert!(parsed.is_compute);
    assert_eq!(parsed.compute.cs.shader_bytecode, address(0x9000));
    assert_eq!(parsed.graphics.ps.shader_bytecode, address(0x3000));
    // A compute subobject without a shader is a graphics pipeline.
    let mut stream = Stream::new();
    stream.push(SUBOBJECT_TYPE_CS, null_shader());
    assert!(!stream.parse().unwrap().is_compute);
    // A vertex, amplification or mesh subobject beside a compute shader is refused, empty or not.
    for ty in [SUBOBJECT_TYPE_VS, SUBOBJECT_TYPE_AS, SUBOBJECT_TYPE_MS] {
        for before in [false, true] {
            let mut stream = Stream::new();
            if before {
                stream.push(ty, null_shader());
            }
            stream.push(SUBOBJECT_TYPE_CS, shader(0x9000));
            if !before {
                stream.push(ty, null_shader());
            }
            assert_eq!(stream.parse().err(), Some(StreamError::InvalidCompute));
        }
    }
}

#[test]
fn depth_stencil1_replaces_the_state_and_warns_about_bounds() {
    let mut stream = Stream::new();
    stream
        .push(
            SUBOBJECT_TYPE_DEPTH_STENCIL,
            DepthStencilDesc {
                depth_enable: 1,
                depth_write_mask: 1,
                depth_func: 2,
                stencil_enable: 0,
                stencil_read_mask: 0xff,
                stencil_write_mask: 0xff,
                front_face: op(1),
                back_face: op(5),
            },
        )
        .push(SUBOBJECT_TYPE_DEPTH_STENCIL1, depth_stencil1(1));
    let mut warnings = Vec::new();
    let parsed = parse_pipeline_stream(&stream.0, &mut |w| warnings.push(w)).unwrap();
    assert_eq!(warnings, [Warning::DepthBoundsIgnored]);
    let state = &parsed.graphics.depth_stencil_state;
    assert_eq!(state.depth_enable, 0);
    assert_eq!(state.depth_write_mask, 0);
    assert_eq!(state.depth_func, 4);
    assert_eq!(state.stencil_enable, 1);
    assert_eq!(
        (state.stencil_read_mask, state.stencil_write_mask),
        (0x12, 0x34)
    );
    assert_eq!(state.front_face.stencil_fail_op, 10);
    assert_eq!(state.front_face.stencil_func, 13);
    assert_eq!(state.back_face.stencil_fail_op, 20);
    assert_eq!(state.back_face.stencil_func, 23);
    // Without the bounds test there is nothing to warn about.
    let mut stream = Stream::new();
    stream.push(SUBOBJECT_TYPE_DEPTH_STENCIL1, depth_stencil1(0));
    assert!(stream.parse().is_ok());
}

#[test]
fn view_instancing_warns_only_with_flags_and_changes_nothing() {
    let mut stream = Stream::new();
    stream.push(SUBOBJECT_TYPE_VIEW_INSTANCING, view_instancing(0));
    let plain = stream.parse().unwrap();
    let mut stream = Stream::new();
    stream
        .push(SUBOBJECT_TYPE_VIEW_INSTANCING, view_instancing(1))
        .push(SUBOBJECT_TYPE_DEPTH_STENCIL1, depth_stencil1(7))
        .push(SUBOBJECT_TYPE_FLAGS, 1_u32);
    let mut warnings = Vec::new();
    let parsed = parse_pipeline_stream(&stream.0, &mut |w| warnings.push(w)).unwrap();
    // In stream order.
    assert_eq!(
        warnings,
        [Warning::ViewInstancingIgnored, Warning::DepthBoundsIgnored]
    );
    assert_eq!(parsed.graphics.flags, 1);
    assert_eq!(parsed.graphics.sample_mask, plain.graphics.sample_mask);
}

/// A warning that came before a refusal is still given.
#[test]
fn warnings_precede_the_error() {
    let mut stream = Stream::new();
    stream
        .push(SUBOBJECT_TYPE_VIEW_INSTANCING, view_instancing(1))
        .push(0x17, 0_u32);
    let mut warnings = Vec::new();
    let error = parse_pipeline_stream(&stream.0, &mut |w| warnings.push(w)).err();
    assert_eq!(warnings, [Warning::ViewInstancingIgnored]);
    assert_eq!(error, Some(StreamError::Unhandled(0x17)));
}

#[test]
fn stream_subobjects_parse_with_alignment() {
    let cases = [
        Case(
            "root signature",
            |s| {
                s.push(SUBOBJECT_TYPE_ROOT_SIGNATURE, address(8).cast_mut());
            },
            16,
        ),
        Case(
            "shader",
            |s| {
                s.push(SUBOBJECT_TYPE_VS, shader(8));
            },
            24,
        ),
        Case(
            "stream output",
            |s| {
                s.push(
                    SUBOBJECT_TYPE_STREAM_OUTPUT,
                    StreamOutputDesc {
                        so_declaration: ptr::null(),
                        num_entries: 0,
                        buffer_strides: ptr::null(),
                        num_strides: 0,
                        rasterized_stream: 0,
                    },
                );
            },
            40,
        ),
        Case(
            "blend",
            |s| {
                s.push(SUBOBJECT_TYPE_BLEND, zeroed::<BlendDesc>());
            },
            336,
        ),
        Case(
            "sample mask",
            |s| {
                s.push(SUBOBJECT_TYPE_SAMPLE_MASK, 1_u32);
            },
            8,
        ),
        Case(
            "rasterizer",
            |s| {
                s.push(SUBOBJECT_TYPE_RASTERIZER, zeroed::<RasterizerDesc>());
            },
            48,
        ),
        Case(
            "depth stencil",
            |s| {
                s.push(SUBOBJECT_TYPE_DEPTH_STENCIL, zeroed::<DepthStencilDesc>());
            },
            56,
        ),
        Case(
            "input layout",
            |s| {
                s.push(SUBOBJECT_TYPE_INPUT_LAYOUT, zeroed::<InputLayoutDesc>());
            },
            24,
        ),
        Case(
            "strip cut value",
            |s| {
                s.push(SUBOBJECT_TYPE_IB_STRIP_CUT_VALUE, 1_u32);
            },
            8,
        ),
        Case(
            "topology",
            |s| {
                s.push(SUBOBJECT_TYPE_PRIMITIVE_TOPOLOGY, 1_u32);
            },
            8,
        ),
        Case(
            "render target formats",
            |s| {
                s.push(
                    SUBOBJECT_TYPE_RENDER_TARGET_FORMATS,
                    zeroed::<RtFormatArray>(),
                );
            },
            40,
        ),
        Case(
            "depth stencil format",
            |s| {
                s.push(DS_FORMAT, 1_u32);
            },
            8,
        ),
        Case(
            "sample desc",
            |s| {
                s.push(SUBOBJECT_TYPE_SAMPLE_DESC, sample_desc(1, 0));
            },
            16,
        ),
        Case(
            "cached pso",
            |s| {
                s.push(SUBOBJECT_TYPE_CACHED_PSO, cached(8));
            },
            24,
        ),
        Case(
            "flags",
            |s| {
                s.push(SUBOBJECT_TYPE_FLAGS, 0_u32);
            },
            8,
        ),
        Case(
            "depth stencil1",
            |s| {
                s.push(SUBOBJECT_TYPE_DEPTH_STENCIL1, depth_stencil1(0));
            },
            64,
        ),
        Case(
            "view instancing",
            |s| {
                s.push(SUBOBJECT_TYPE_VIEW_INSTANCING, view_instancing(0));
            },
            32,
        ),
        Case(
            "amplification",
            |s| {
                s.push(SUBOBJECT_TYPE_AS, null_shader());
            },
            24,
        ),
        Case(
            "mesh",
            |s| {
                s.push(SUBOBJECT_TYPE_MS, null_shader());
            },
            24,
        ),
        Case(
            "compute",
            |s| {
                s.push(SUBOBJECT_TYPE_CS, null_shader());
            },
            24,
        ),
    ];
    for Case(name, push, advance) in cases {
        // The size on this target: the 64-bit advance, but for the pointer-sized part.
        let advance = if cfg!(target_pointer_width = "64") {
            advance
        } else {
            let mut probe = Stream::new();
            push(&mut probe);
            probe.0.len().next_multiple_of(size_of::<usize>())
        };
        let mut stream = Stream::new();
        push(&mut stream);
        let unpadded = stream.0.len();
        assert!(unpadded <= advance, "{name}");
        // The subobject alone, without its trailing padding, is a whole stream.
        assert!(stream.parse().is_ok(), "{name}: unpadded");
        // A node mask at the advance is found there, whether the subobject was padded or not.
        stream.pad_to(advance);
        assert_eq!(stream.0.len(), advance, "{name}");
        stream.push(SUBOBJECT_TYPE_NODE_MASK, 77_u32);
        assert_eq!(stream.parse().unwrap().graphics.node_mask, 77, "{name}");
        // The stream from an odd address parses the same.
        let mut shifted = vec![0xee_u8];
        shifted.extend_from_slice(&stream.0);
        let parsed = parse_pipeline_stream(&shifted[1..], &mut |_| {}).unwrap();
        assert_eq!(parsed.graphics.node_mask, 77, "{name}: shifted");
    }
}

#[test]
fn the_last_subobject_may_end_before_its_padding_but_not_before_its_data() {
    // On a 64-bit target a sample desc is 12 bytes and advances 16.
    let mut stream = Stream::new();
    stream.push(SUBOBJECT_TYPE_SAMPLE_DESC, sample_desc(8, 1));
    assert_eq!(stream.0.len(), 12);
    let parsed = stream.parse().unwrap();
    assert_eq!(parsed.graphics.sample_desc.count, 8);
    // One byte short of the data is an invalid stream.
    let short = &stream.0[..stream.0.len() - 1];
    let error = parse_pipeline_stream(short, &mut |_| {}).err();
    assert_eq!(error, Some(StreamError::InvalidStream));
    // Bytes behind a whole subobject make another one that is cut short.
    for extra in 1..=3 {
        let mut stream = Stream::new();
        stream
            .push(SUBOBJECT_TYPE_FLAGS, 3_u32)
            .pad_to(size_of::<usize>());
        stream.0.extend(std::iter::repeat_n(0_u8, extra));
        assert_eq!(
            stream.parse().err(),
            Some(StreamError::InvalidStream),
            "{extra}"
        );
    }
    // A type with no data behind it is cut short, however large the stream is otherwise.
    let mut stream = Stream::new();
    stream
        .push(SUBOBJECT_TYPE_FLAGS, 3_u32)
        .pad_to(size_of::<usize>());
    stream.0.extend(SUBOBJECT_TYPE_NODE_MASK.to_ne_bytes());
    assert_eq!(stream.parse().err(), Some(StreamError::InvalidStream));
}

#[test]
fn unknown_subobject_fails_as_cpp() {
    // The gap at 23, past the last known type, and beyond a 32-bit mask.
    for ty in [0x17, 0x1a, 0x1b, 0x1f, 0x20, 0x21, u32::MAX] {
        // No data behind the type: it is refused before any data is read.
        let mut stream = Stream::new();
        stream.0.extend(ty.to_ne_bytes());
        let error = stream.parse().err();
        assert_eq!(error, Some(StreamError::Unhandled(ty)), "{ty:#x}");
        assert_eq!(error.map(|e| e.hresult()), Some(E_INVALIDARG));
    }
    // A type from 32 on takes the bit of its number modulo 32, as the C++'s shift does: 0x20 after
    // a root signature is the duplicate of it, and 0x21 after a vertex shader.
    let mut stream = Stream::new();
    stream.push(SUBOBJECT_TYPE_ROOT_SIGNATURE, ptr::null_mut::<c_void>());
    stream
        .pad_to(size_of::<usize>())
        .0
        .extend(0x20_u32.to_ne_bytes());
    assert_eq!(stream.parse().err(), Some(StreamError::Duplicate(0x20)));
    let mut stream = Stream::new();
    stream.push(SUBOBJECT_TYPE_VS, null_shader());
    stream
        .pad_to(size_of::<usize>())
        .0
        .extend(0x21_u32.to_ne_bytes());
    assert_eq!(stream.parse().err(), Some(StreamError::Duplicate(0x21)));
}

#[test]
fn a_repeated_subobject_is_refused() {
    let mut stream = Stream::new();
    stream
        .push(SUBOBJECT_TYPE_FLAGS, 0_u32)
        .push(SUBOBJECT_TYPE_VS, null_shader())
        .push(SUBOBJECT_TYPE_FLAGS, 1_u32);
    let error = stream.parse().err();
    assert_eq!(error, Some(StreamError::Duplicate(SUBOBJECT_TYPE_FLAGS)));
    assert_eq!(error.map(|e| e.hresult()), Some(E_INVALIDARG));
    // Two different types that fill the same field are not repeats.
    let mut stream = Stream::new();
    stream
        .push(SUBOBJECT_TYPE_DEPTH_STENCIL, zeroed::<DepthStencilDesc>())
        .push(SUBOBJECT_TYPE_DEPTH_STENCIL1, depth_stencil1(0));
    assert!(stream.parse().is_ok());
}

#[test]
fn amplification_and_mesh_shaders_are_not_implemented_when_present() {
    for (ty, error) in [
        (SUBOBJECT_TYPE_AS, StreamError::UnsupportedAs),
        (SUBOBJECT_TYPE_MS, StreamError::UnsupportedMs),
    ] {
        let mut stream = Stream::new();
        stream.push(ty, shader_at(0x1000));
        let got = stream.parse().err();
        assert_eq!(got.as_ref().map(StreamError::hresult), Some(E_NOTIMPL));
        assert_eq!(got, Some(error));
        // A null one is accepted, and the stream goes on.
        let mut stream = Stream::new();
        stream
            .push(ty, null_shader())
            .push(SUBOBJECT_TYPE_NODE_MASK, 2_u32);
        assert_eq!(stream.parse().unwrap().graphics.node_mask, 2);
    }
}

#[test]
fn errors_read_as_the_cpps_log_lines() {
    for (error, text) in [
        (
            StreamError::InvalidStream,
            "CreatePipelineState: invalid stream",
        ),
        (
            StreamError::Duplicate(7),
            "CreatePipelineState: duplicated subobejct type 7",
        ),
        (
            StreamError::Unhandled(23),
            "CreatePipelineState: unhandled subobject type 23",
        ),
        (
            StreamError::UnsupportedAs,
            "CreatePipelineState: unsupported AS",
        ),
        (
            StreamError::UnsupportedMs,
            "CreatePipelineState: unsupported MS",
        ),
        (
            StreamError::InvalidCompute,
            "CreatePipelineState: invalid compute pipeline state stream",
        ),
    ] {
        assert_eq!(error.to_string(), text);
    }
    assert_eq!(
        Warning::DepthBoundsIgnored.to_string(),
        "CreatePipelineState: ignore DepthBoundsTestEnable"
    );
    assert_eq!(
        Warning::ViewInstancingIgnored.to_string(),
        "CreatePipelineState: ignore ViewInstancing"
    );
    assert_eq!(E_INVALIDARG, i32::from_be_bytes([0x80, 0x07, 0x00, 0x57]));
    assert_eq!(E_NOTIMPL, i32::from_be_bytes([0x80, 0x00, 0x40, 0x01]));
}

#[test]
fn a_stream_descriptions_bytes_are_its_size_from_its_pointer() {
    let empty = PipelineStateStreamDesc {
        size_in_bytes: 0,
        pipeline_state_subobject_stream: ptr::null_mut(),
    };
    // SAFETY: a size of 0 needs no readable memory.
    assert_eq!(unsafe { stream_bytes(&empty) }, &[] as &[u8]);
    let mut stream = Stream::new();
    stream.push(SUBOBJECT_TYPE_NODE_MASK, 9_u32);
    let desc = PipelineStateStreamDesc {
        size_in_bytes: stream.0.len(),
        pipeline_state_subobject_stream: stream.0.as_mut_ptr().cast(),
    };
    // SAFETY: the pointer is the vector's, readable for its length, which `desc` states.
    let bytes = unsafe { stream_bytes(&desc) };
    assert_eq!(bytes, stream.0);
    let parsed = parse_pipeline_stream(bytes, &mut |_| {}).unwrap();
    assert_eq!(parsed.graphics.node_mask, 9);
}

fn push_bytecode(words: &mut Vec<u64>, shader: &ShaderBytecode) {
    words.push(shader.shader_bytecode as usize as u64);
    words.push(shader.bytecode_length as u64);
}

fn push_face(words: &mut Vec<u64>, face: &DepthStencilOpDesc) {
    words.extend(
        [
            face.stencil_fail_op,
            face.stencil_depth_fail_op,
            face.stencil_pass_op,
            face.stencil_func,
        ]
        .map(u64::from),
    );
}

/// The graphics description's fields as 64-bit words, in the order the C++ oracle walks them.
fn graphics_words(g: &GraphicsPipelineStateDesc) -> Vec<u64> {
    let mut w = vec![g.root_signature as usize as u64];
    for shader in [&g.vs, &g.ps, &g.ds, &g.hs, &g.gs] {
        push_bytecode(&mut w, shader);
    }
    let so = &g.stream_output;
    w.extend([
        so.so_declaration as usize as u64,
        u64::from(so.num_entries),
        so.buffer_strides as usize as u64,
        u64::from(so.num_strides),
        u64::from(so.rasterized_stream),
        u64::from(g.blend_state.alpha_to_coverage_enable.cast_unsigned()),
        u64::from(g.blend_state.independent_blend_enable.cast_unsigned()),
    ]);
    for t in &g.blend_state.render_target {
        w.extend([
            u64::from(t.blend_enable.cast_unsigned()),
            u64::from(t.logic_op_enable.cast_unsigned()),
            u64::from(t.src_blend),
            u64::from(t.dest_blend),
            u64::from(t.blend_op),
            u64::from(t.src_blend_alpha),
            u64::from(t.dest_blend_alpha),
            u64::from(t.blend_op_alpha),
            u64::from(t.logic_op),
            u64::from(t.render_target_write_mask),
        ]);
    }
    let r = &g.rasterizer_state;
    w.extend([
        u64::from(g.sample_mask),
        u64::from(r.fill_mode),
        u64::from(r.cull_mode),
        u64::from(r.front_counter_clockwise.cast_unsigned()),
        u64::from(r.depth_bias.cast_unsigned()),
        u64::from(r.depth_bias_clamp.to_bits()),
        u64::from(r.slope_scaled_depth_bias.to_bits()),
        u64::from(r.depth_clip_enable.cast_unsigned()),
        u64::from(r.multisample_enable.cast_unsigned()),
        u64::from(r.antialiased_line_enable.cast_unsigned()),
        u64::from(r.forced_sample_count),
        u64::from(r.conservative_raster),
    ]);
    let d = &g.depth_stencil_state;
    w.extend([
        u64::from(d.depth_enable.cast_unsigned()),
        u64::from(d.depth_write_mask),
        u64::from(d.depth_func),
        u64::from(d.stencil_enable.cast_unsigned()),
        u64::from(d.stencil_read_mask),
        u64::from(d.stencil_write_mask),
    ]);
    push_face(&mut w, &d.front_face);
    push_face(&mut w, &d.back_face);
    w.extend([
        g.input_layout.input_element_descs as usize as u64,
        u64::from(g.input_layout.num_elements),
        u64::from(g.ib_strip_cut_value),
        u64::from(g.primitive_topology_type),
        u64::from(g.num_render_targets),
    ]);
    w.extend(g.rtv_formats.map(u64::from));
    w.extend([
        u64::from(g.dsv_format),
        u64::from(g.sample_desc.count),
        u64::from(g.sample_desc.quality),
        u64::from(g.node_mask),
        g.cached_pso.cached_blob as usize as u64,
        g.cached_pso.cached_blob_size_in_bytes as u64,
        u64::from(g.flags),
    ]);
    w
}

/// The compute description's fields as 64-bit words, in the order the C++ oracle walks them.
fn compute_words(c: &ComputePipelineStateDesc) -> Vec<u64> {
    let mut w = vec![c.root_signature as usize as u64];
    push_bytecode(&mut w, &c.cs);
    w.extend([
        u64::from(c.node_mask),
        c.cached_pso.cached_blob as usize as u64,
        c.cached_pso.cached_blob_size_in_bytes as u64,
        u64::from(c.flags),
    ]);
    w
}

/// FNV-1a over the little-endian bytes of the words, as the C++ oracle hashes them.
fn fnv(words: &[u64]) -> u64 {
    words
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        })
}

/// The bytes of a hexadecimal string.
fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

/// Every line of the C++ oracle's recording of `ParsePipelineStream` replays equal.
///
/// The recording is `d3d12_device.cpp`'s function run on 805 streams of the 64-bit layout: random
/// subobjects with missing padding, short ends, stray bytes, repeated and unknown types; compute
/// beside graphics-only ones; one stream cut at the lengths around each subobject; bytes that are
/// no stream. A line holds the `HRESULT`, the compute flag, a hash of every field of both
/// descriptions, and the messages logged in order.
#[cfg(target_pointer_width = "64")]
#[test]
fn stream_parsing_replays_the_cpp_oracle() {
    let golden = include_str!("golden/golden.txt");
    let mut lines = 0;
    let mut successes = 0;
    for line in golden.lines() {
        let (hex, _) = line.split_once(" => ").unwrap();
        let mut messages = Vec::new();
        let result = parse_pipeline_stream(&unhex(hex), &mut |w| messages.push(w.to_string()));
        let mut got = format!("{hex} => ");
        match result {
            Ok(parsed) => {
                successes += 1;
                let _ = write!(
                    got,
                    "00000000 {} {:016x} {:016x}",
                    u8::from(parsed.is_compute),
                    fnv(&graphics_words(&parsed.graphics)),
                    fnv(&compute_words(&parsed.compute)),
                );
            }
            Err(error) => {
                let _ = write!(got, "{:08x}", error.hresult().cast_unsigned());
                messages.push(error.to_string());
            }
        }
        got += " |";
        for (i, message) in messages.iter().enumerate() {
            got += if i == 0 { " " } else { " ; " };
            got += message;
        }
        assert_eq!(got, line);
        lines += 1;
    }
    assert_eq!((lines, successes), (805, 215));
}
