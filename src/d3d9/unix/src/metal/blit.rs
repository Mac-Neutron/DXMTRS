//! "Blit-quad" pipeline used to translate a *scaling* D3D9 `StretchRect`.
//!
//! D3D9's `IDirect3DDevice9::StretchRect` can resize the source onto the
//! destination (a filtered up/down-scale). Metal's `MTLBlitCommandEncoder`
//! only does 1:1 copies, so a size-mismatch `StretchRect` can't ride the blit
//! path the 1:1 case uses. Instead the PE side opens a render pass on the
//! destination texture (`loadAction = Load` to preserve content outside the
//! destination rect), binds the source texture + a sampler, sets the Metal
//! viewport + scissor to the destination rect, pushes the source→destination
//! texcoord transform via `setVertexBytes`, and draws a single fullscreen
//! triangle that samples the source across the destination rect.
//!
//! This module is the VS/PS pair + per-destination-format pipeline for that
//! quad. It mirrors `clear_quad.rs`: one cached `MTLLibrary` + functions,
//! and a `HashMap<color_format, MTLRenderPipelineState*>` of pipelines.
//! Pipelines + library are process-lifetime (leaked via `Retained::into_raw`)
//! — same posture as `clear_quad` / `present`.

use std::{
    ffi::CStr,
    sync::{Mutex, OnceLock},
};

use mtld3d_core::stretch_rect::{BLIT_MSL, BLIT_PS, BLIT_VS};
use mtld3d_shared::{
    MetalHandle,
    mtl::PixelFormat,
    mtl_handle::{MTLFunctionKind, MTLRenderPipelineStateKind},
};
use rustc_hash::FxHashMap;

use super::texture::mtl_pixel_format;
use crate::{
    LOG_TARGET,
    metal::handle::{IntoRetained, keep_first},
    mtl::{
        objc2::{rc::Retained, runtime::ProtocolObject},
        objc2_foundation::NSString,
        objc2_metal::{
            MTLCompileOptions, MTLDevice, MTLLanguageVersion, MTLLibrary, MTLMathMode,
            MTLRenderPipelineDescriptor,
        },
    },
};

/// VS entry-point name, the shared crate's [`BLIT_VS`] as the `NSString` lookup takes it.
const VS_NAME: &str = entry_name(BLIT_VS);
/// PS entry-point name, the shared crate's [`BLIT_PS`] as the `NSString` lookup takes it.
const PS_NAME: &str = entry_name(BLIT_PS);

struct BlitCache {
    vs_fn: MetalHandle<MTLFunctionKind>,
    ps_fn: MetalHandle<MTLFunctionKind>,
    /// Keyed by `(destination colour format, pass sample count)`.
    pipelines: Mutex<FxHashMap<(PixelFormat, u32), MetalHandle<MTLRenderPipelineStateKind>>>,
}

static CACHE: OnceLock<Option<BlitCache>> = OnceLock::new();

/// Lazy create-or-fetch of the blit-quad pipeline for the requested destination colour format.
///
/// Returns `None` on any compile / pipeline-create failure; the PE side then
/// leaves the size-mismatch `StretchRect` rejected (the 1:1 path is
/// unaffected).
pub fn ensure_blit_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    color_format: PixelFormat,
    sample_count: u32,
) -> Option<MetalHandle<MTLRenderPipelineStateKind>> {
    let cache = CACHE
        .get_or_init(|| build_library_and_functions(device))
        .as_ref()?;

    let key = (color_format, sample_count.max(1));
    {
        let pipelines = cache.pipelines.lock().ok()?;
        if let Some(&handle) = pipelines.get(&key) {
            return Some(handle);
        }
    }

    let handle = build_pipeline(device, cache, key)?;
    let mut pipelines = cache.pipelines.lock().ok()?;
    // SAFETY: `build_pipeline` handed `handle` the only retain on its
    // pipeline, and nothing else copied it.
    Some(unsafe { keep_first(&mut pipelines, key, handle) })
}

fn build_library_and_functions(device: &ProtocolObject<dyn MTLDevice>) -> Option<BlitCache> {
    let source = NSString::from_str(BLIT_MSL);
    let options = MTLCompileOptions::new();
    options.setLanguageVersion(MTLLanguageVersion::Version2_4);
    // Pin `mathMode = Fast` for parity with the other inline compile sites
    // here; the blit shader does a single texture sample so the math mode is
    // immaterial to correctness.
    options.setMathMode(MTLMathMode::Fast);
    let library = match device.newLibraryWithSource_options_error(&source, Some(&options)) {
        Ok(lib) => lib,
        Err(e) => {
            log::error!(
                target: LOG_TARGET,
                "blit-quad: MSL compilation failed: {e}"
            );
            return None;
        }
    };
    library.setLabel(Some(&NSString::from_str("mtld3d-blit-quad")));

    let vs = library.newFunctionWithName(&NSString::from_str(VS_NAME))?;
    let ps = library.newFunctionWithName(&NSString::from_str(PS_NAME))?;

    // Leak the function handles for process lifetime so `build_pipeline` can
    // re-derive the `MTLFunction`s per pipeline-build (same posture as
    // `clear_quad`). The library stays alive via the function refs; the
    // function refs stay alive via the pipeline states once built.
    // SAFETY: Retained::into_raw transfers the retain into the typed handle.
    let vs_handle = unsafe { MetalHandle::<MTLFunctionKind>::new(Retained::into_raw(vs) as u64) };
    // SAFETY: Retained::into_raw transfers the retain into the typed handle.
    let ps_handle = unsafe { MetalHandle::<MTLFunctionKind>::new(Retained::into_raw(ps) as u64) };
    drop(library);

    Some(BlitCache {
        vs_fn: vs_handle,
        ps_fn: ps_handle,
        pipelines: Mutex::new(FxHashMap::default()),
    })
}

fn build_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    cache: &BlitCache,
    key: (PixelFormat, u32),
) -> Option<MetalHandle<MTLRenderPipelineStateKind>> {
    let (color_format, sample_count) = key;
    let vs = cache.vs_fn.into_retained()?;
    let ps = cache.ps_fn.into_retained()?;

    let desc = MTLRenderPipelineDescriptor::new();
    desc.setVertexFunction(Some(&vs));
    desc.setFragmentFunction(Some(&ps));
    desc.setRasterSampleCount(sample_count as usize);
    // SAFETY: `colorAttachments()` returns a non-null
    // `MTLRenderPipelineColorAttachmentDescriptorArray`; subscript 0 is always
    // valid.
    let color0 = unsafe { desc.colorAttachments().objectAtIndexedSubscript(0) };
    color0.setPixelFormat(mtl_pixel_format(color_format));
    // No depth attachment: the blit quad never writes depth and the PE side
    // opens the destination pass with no depth texture bound. Declaring a
    // depth format here would make Metal reject the pipeline against the
    // depth-less render pass.

    let label = format!("mtld3d-blit-quad c={color_format:?} s={sample_count}");
    desc.setLabel(Some(&NSString::from_str(&label)));

    let pipeline = match device.newRenderPipelineStateWithDescriptor_error(&desc) {
        Ok(p) => p,
        Err(e) => {
            log::error!(
                target: LOG_TARGET,
                "blit-quad: pipeline creation failed ({label}): {e}"
            );
            return None;
        }
    };
    drop(vs);
    drop(ps);
    // SAFETY: Retained::into_raw transfers the retain into the typed handle.
    Some(unsafe {
        MetalHandle::<MTLRenderPipelineStateKind>::new(Retained::into_raw(pipeline) as u64)
    })
}

/// An entry-point name as text: the shared names are ASCII, so this never panics.
const fn entry_name(name: &'static CStr) -> &'static str {
    let Ok(text) = name.to_str() else {
        panic!("an entry-point name is UTF-8");
    };
    text
}

#[cfg(test)]
mod tests;
