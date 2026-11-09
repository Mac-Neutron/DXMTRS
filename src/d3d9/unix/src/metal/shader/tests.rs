use airconv_ir::dxso::spec::{StageSpec, VariantKey, VsSpec};
use mtld3d_shared::{MetalHandle, mtl_handle::MTLDeviceKind, perf::ShaderTimings};

use super::{
    compile_metallib_library, compile_shader_library, destroy_function, destroy_library,
    translate_dxso,
};
use crate::{
    metal::handle::IntoRetained as _,
    mtl::{
        objc2::{rc::Retained, runtime::ProtocolObject},
        objc2_metal::{
            MTLCreateSystemDefaultDevice, MTLDevice, MTLPixelFormat, MTLRenderPipelineDescriptor,
            MTLVertexDescriptor, MTLVertexFormat, MTLVertexStepFunction,
        },
    },
};

fn poisoned_timings() -> ShaderTimings {
    if !mtld3d_shared::perf::perf_enabled() {
        return ShaderTimings::new();
    }
    ShaderTimings {
        preparation_ns: u64::MAX,
        library_ns: u64::MAX,
        function_ns: u64::MAX,
    }
}

#[test]
fn shader_timings_reset_unreached_phases_and_preserve_results() {
    mtld3d_shared::init_logger();
    mtld3d_shared::perf::init_tracking_enabled();
    let mut timings = poisoned_timings();
    assert!(compile_shader_library(MetalHandle::NULL, "", "", &mut timings).is_none());
    assert_eq!(timings.library_ns, 0);
    assert_eq!(timings.function_ns, 0);
    assert_ne!(timings.preparation_ns, u64::MAX);

    let device = MTLCreateSystemDefaultDevice().expect("Metal device for shader timing test");
    // SAFETY: the device's retain stays alive until all three calls return.
    let handle = unsafe { MetalHandle::new(Retained::as_ptr(&device) as u64) };
    let msl = "#include <metal_stdlib>\nusing namespace metal;\nvertex float4 probe(uint id [[vertex_id]]) { return float4(float(id), 0, 0, 1); }";
    for (source, entry, success, function_reached) in [
        (msl, "probe", true, true),
        (msl, "missing", false, true),
        ("invalid MSL", "probe", false, false),
    ] {
        timings = poisoned_timings();
        let result = compile_shader_library(handle, source, entry, &mut timings);
        assert_eq!(result.is_some(), success);
        assert_ne!(timings.preparation_ns, u64::MAX);
        assert_ne!(timings.library_ns, u64::MAX);
        assert_ne!(timings.function_ns, u64::MAX);
        if !function_reached {
            assert_eq!(timings.function_ns, 0);
        }
        if mtld3d_shared::perf::perf_enabled() {
            assert!(timings.preparation_ns > 0);
            assert!(timings.library_ns > 0);
            assert_eq!(timings.function_ns > 0, function_reached);
        } else {
            assert_eq!(
                (
                    timings.preparation_ns,
                    timings.library_ns,
                    timings.function_ns
                ),
                (0, 0, 0)
            );
        }
        if let Some((library, function)) = result {
            destroy_function(function.raw());
            destroy_library(library.raw());
        }
    }
}

/// `vs_2_0`: `dcl_position v0` / `mov oPos, v0`.
const VS_2_0: [u32; 8] = [
    0xFFFE_0200,
    0x0200_001F,
    0x8000_0000,
    0x900F_0000,
    0x0200_0001,
    0xC00F_0000,
    0x90E4_0000,
    0x0000_FFFF,
];

/// `ps_2_0`: `def c0, 1, 0, 0, 1` / `mov oC0, c0`.
const PS_2_0: [u32; 11] = [
    0xFFFF_0200,
    0x0500_0051,
    0xA00F_0000,
    0x3F80_0000,
    0,
    0,
    0x3F80_0000,
    0x0200_0001,
    0x800F_0800,
    0xA0E4_0000,
    0x0000_FFFF,
];

fn vs_spec() -> StageSpec {
    StageSpec::Vs(VsSpec {
        provided_input_mask: 1,
        ..VsSpec::default()
    })
}

type Device = Retained<ProtocolObject<dyn MTLDevice>>;

fn device() -> Device {
    MTLCreateSystemDefaultDevice().expect("Metal device")
}

fn handle_of(device: &Device) -> MetalHandle<MTLDeviceKind> {
    // SAFETY: the device's retain outlives every use of the handle in these tests.
    unsafe { MetalHandle::new(Retained::as_ptr(device) as u64) }
}

#[test]
fn translates_and_loads_a_vs_2_0_and_ps_2_0() {
    let device = device();
    let handle = handle_of(&device);
    let vs = translate_dxso(&VS_2_0, &vs_spec(), c"vs_main").expect("the vertex shader translates");
    let ps = translate_dxso(&PS_2_0, &StageSpec::Ps(VariantKey::default()), c"ps_main")
        .expect("the pixel shader translates");
    let (vs_lib, vs_fn) =
        compile_metallib_library(handle, &vs, "vs_main", "vs").expect("vs library");
    let (ps_lib, ps_fn) =
        compile_metallib_library(handle, &ps, "ps_main", "ps").expect("ps library");
    let desc = MTLRenderPipelineDescriptor::new();
    desc.setVertexFunction(vs_fn.into_retained().as_deref());
    desc.setFragmentFunction(ps_fn.into_retained().as_deref());
    let vertex_desc = MTLVertexDescriptor::new();
    // SAFETY: attribute 0 is within the descriptor's table.
    let attr = unsafe { vertex_desc.attributes().objectAtIndexedSubscript(0) };
    attr.setFormat(MTLVertexFormat::Float4);
    // SAFETY: offset 0.
    unsafe { attr.setOffset(0) };
    // SAFETY: buffer slot 0.
    unsafe { attr.setBufferIndex(0) };
    // SAFETY: layout 0 is within the descriptor's table.
    let layout = unsafe { vertex_desc.layouts().objectAtIndexedSubscript(0) };
    // SAFETY: a float4 stride.
    unsafe { layout.setStride(16) };
    layout.setStepFunction(MTLVertexStepFunction::PerVertex);
    // SAFETY: step rate 1.
    unsafe { layout.setStepRate(1) };
    desc.setVertexDescriptor(Some(&vertex_desc));
    // SAFETY: index < 8.
    unsafe { desc.colorAttachments().objectAtIndexedSubscript(0) }
        .setPixelFormat(MTLPixelFormat::RGBA8Unorm);
    let pipeline = device.newRenderPipelineStateWithDescriptor_error(&desc);
    destroy_function(vs_fn.raw());
    destroy_function(ps_fn.raw());
    destroy_library(vs_lib.raw());
    destroy_library(ps_lib.raw());
    pipeline.unwrap_or_else(|e| panic!("a pipeline of the two: {e}"));
}

#[test]
fn four_threads_translate_at_once() {
    let device = device();
    let raw = handle_of(&device).raw();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(move || {
                // SAFETY: `device` outlives the scope.
                let handle = unsafe { MetalHandle::new(raw) };
                for _ in 0..8 {
                    let vs = translate_dxso(&VS_2_0, &vs_spec(), c"vs_main").expect("translates");
                    let (lib, func) =
                        compile_metallib_library(handle, &vs, "vs_main", "vs").expect("loads");
                    destroy_function(func.raw());
                    destroy_library(lib.raw());
                }
            });
        }
    });
}

#[test]
fn a_bad_metallib_is_none() {
    let device = device();
    assert!(compile_metallib_library(handle_of(&device), b"nope", "main", "bad").is_none());
}
