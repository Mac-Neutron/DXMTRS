//! SM50 translation through winemetal (D3D9 C3a Task 9).

use airconv_ir::dxso::spec::{StageSpec, VariantKey, VsSpec};

use super::{new_library, settle, translate};
use crate::{
    foundation::NSString,
    metal::{
        MTLCreateSystemDefaultDevice, MTLDevice as _, MTLLibrary as _, MTLPixelFormat,
        MTLRenderPipelineDescriptor, MTLVertexDescriptor, MTLVertexFormat, MTLVertexStepFunction,
    },
};

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

/// The host's winemetal (the Mac facade test) or the guest's names itself in `MTLD3D_WINEMETAL`.
fn have_winemetal() -> bool {
    let have = std::env::var_os("MTLD3D_WINEMETAL").is_some();
    if !have {
        println!("skipped: no winemetal");
    }
    have
}

/// A freed error handle is not freed again.
///
/// A compile that fails before it writes `*error` (or succeeds without touching it) would
/// otherwise give the initialize call's handle back twice.
#[test]
fn a_freed_error_handle_is_not_freed_twice() {
    let mut error = 7_u64;
    let mut freed = Vec::new();
    settle(&mut error, |h| freed.push(h));
    settle(&mut error, |h| freed.push(h));
    assert_eq!(freed, [7]);
    assert_eq!(error, 0);
}

#[test]
fn refuses_bytes_that_are_no_shader() {
    if !have_winemetal() {
        return;
    }
    let error = translate(&[1, 2, 3], &StageSpec::Ps(VariantKey::default()), c"main")
        .expect_err("not a shader");
    assert!(error.starts_with("SM50Initialize"), "{error}");
}

/// Bytes that are no metallib come back as the error's text, and the +1 error is given back once.
#[test]
fn new_library_reports_a_bad_metallib() {
    if !have_winemetal() {
        return;
    }
    let device = MTLCreateSystemDefaultDevice().expect("a device from winemetal");
    let error = new_library(&device, b"nope").expect_err("not a metallib");
    assert!(error.starts_with("newLibraryWithData"), "{error}");
    assert!(!error.is_empty());
}

/// A `vs_2_0` and a `ps_2_0` triangle: translated, made libraries, and put in a pipeline.
///
/// Passes once the translator lowers DXSO (C3a Task 10).
#[test]
fn translates_a_dxso_vs_and_ps_on_the_host() {
    if !have_winemetal() {
        return;
    }
    let device = MTLCreateSystemDefaultDevice().expect("a device from winemetal");
    let vs_spec = StageSpec::Vs(VsSpec {
        provided_input_mask: 1,
        ..VsSpec::default()
    });
    let vs = translate(&VS_2_0, &vs_spec, c"vs_main").expect("the vertex shader translates");
    let ps = translate(&PS_2_0, &StageSpec::Ps(VariantKey::default()), c"ps_main")
        .expect("the pixel shader translates");
    let vs_library = new_library(&device, &vs).expect("a vertex library");
    let ps_library = new_library(&device, &ps).expect("a pixel library");
    let desc = MTLRenderPipelineDescriptor::new();
    desc.setVertexFunction(
        vs_library
            .newFunctionWithName(&NSString::from_str("vs_main"))
            .as_deref(),
    );
    desc.setFragmentFunction(
        ps_library
            .newFunctionWithName(&NSString::from_str("ps_main"))
            .as_deref(),
    );
    // v0 is the position: attribute 0, float4, from vertex buffer 0, as the runtime lays out a stream.
    let vertex_desc = MTLVertexDescriptor::new();
    // SAFETY: attribute 0 is within the descriptor's table.
    let attr = unsafe { vertex_desc.attributes().objectAtIndexedSubscript(0) };
    attr.setFormat(MTLVertexFormat::Float4);
    // SAFETY: plain accessor, offset 0.
    unsafe { attr.setOffset(0) };
    // SAFETY: plain accessor, buffer slot 0.
    unsafe { attr.setBufferIndex(0) };
    // SAFETY: layout 0 is within the descriptor's table.
    let layout = unsafe { vertex_desc.layouts().objectAtIndexedSubscript(0) };
    // SAFETY: plain accessor, a float4 stride.
    unsafe { layout.setStride(16) };
    layout.setStepFunction(MTLVertexStepFunction::PerVertex);
    // SAFETY: plain accessor, step rate 1.
    unsafe { layout.setStepRate(1) };
    desc.setVertexDescriptor(Some(&vertex_desc));
    // SAFETY: index < 8.
    unsafe { desc.colorAttachments().objectAtIndexedSubscript(0) }
        .setPixelFormat(MTLPixelFormat::RGBA8Unorm);
    device
        .newRenderPipelineStateWithDescriptor_error(&desc)
        .unwrap_or_else(|e| panic!("a pipeline of the two: {}", e.localizedDescription()));
}
