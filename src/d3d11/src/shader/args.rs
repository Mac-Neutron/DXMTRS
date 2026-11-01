//! SM50 compile argument chains, variant SHA-1 digests and function names (`d3d11_shader.cpp`).

use core::{
    ffi::{c_char, c_void},
    fmt::Write as _,
    marker::PhantomPinned,
    pin::Pin,
    ptr,
};
use std::sync::LazyLock;

use dxmt_util::{config::Config, sha1::Sha1};
use winemetal_sys::sm50::{
    SM50_SHADER_COMMON, SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT, SM50_SHADER_FLAG_DEFUSE_FMA,
    SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO, SM50_SHADER_GS_PASS_THROUGH, SM50_SHADER_IA_INPUT_LAYOUT,
    SM50_SHADER_METAL_310, SM50_SHADER_PSO_GEOMETRY_SHADER, SM50_SHADER_PSO_PIXEL_SHADER,
    SM50_SHADER_PSO_TESSELLATOR, Sm50IaInputElement, Sm50ShaderCommonData,
    Sm50ShaderCompilationArgumentData, Sm50ShaderEmulateVertexStreamOutputData,
    Sm50ShaderGsPassThroughData, Sm50ShaderGsPassThroughDataUnion, Sm50ShaderIaInputLayoutData,
    Sm50ShaderPsoGeometryShaderData, Sm50ShaderPsoPixelShaderData, Sm50ShaderPsoTessellatorData,
    Sm50StreamOutputElement,
};

use crate::input_layout::{InputLayoutElement, StreamOutputElement};

/// Process-wide `SM50_SHADER_FLAG_*` bits resolved once from `Config::load(&crate::LOG)`.
pub static FLAGS: LazyLock<u32> = LazyLock::new(|| flags_from_config(&Config::load(&crate::LOG)));

/// SHA-1 of the empty byte slice (`da39a3ee5e6b4b0d3255bfef95601890afd80709`).
pub const EMPTY_SHA1: [u8; 20] = [
    0xda, 0x39, 0xa3, 0xee, 0x5e, 0x6b, 0x4b, 0x0d, 0x32, 0x55, 0xbf, 0xef, 0x95, 0x60, 0x18, 0x90,
    0xaf, 0xd8, 0x07, 0x09,
];

/// Resolves global SM50 shader compile flags from `cfg` (`getGlobalShaderFlag`).
#[must_use]
pub fn flags_from_config(cfg: &Config) -> u32 {
    let mut flags = 0u32;
    if cfg.get_bool("d3d11.sampleNaNToZero", false) {
        flags |= SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO;
    }
    if cfg.get_bool("d3d11.defuseFma", false) {
        flags |= SM50_SHADER_FLAG_DEFUSE_FMA;
    }
    flags
}

/// Converts a resolved [`InputLayoutElement`] into an [`Sm50IaInputElement`].
#[must_use]
pub const fn ia_element_from_desc(desc: &InputLayoutElement) -> Sm50IaInputElement {
    Sm50IaInputElement {
        reg: desc.index,
        slot: desc.slot,
        aligned_byte_offset: desc.offset,
        format: desc.format,
        bits: desc.step_function_and_rate,
    }
}

/// Converts a resolved [`StreamOutputElement`] into an [`Sm50StreamOutputElement`].
#[must_use]
pub const fn so_element_from_desc(desc: &StreamOutputElement) -> Sm50StreamOutputElement {
    Sm50StreamOutputElement {
        reg_id: desc.register_idx,
        component: desc.component,
        output_slot: desc.output_slot,
        offset: desc.offset,
    }
}

/// Serializes an input layout into the byte stream hashed by `CachedInputLayout`.
///
/// `size_t_width` is `4` (win32 / i686) or `8` (win64 / macOS).
#[must_use]
pub fn input_layout_stream_with_width(
    elements: &[InputLayoutElement],
    slot_mask: u32,
    size_t_width: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + size_t_width + elements.len() * 20);
    out.extend_from_slice(&slot_mask.to_le_bytes());
    let len_bytes = (elements.len() as u64).to_le_bytes();
    if size_t_width == 4 {
        out.extend_from_slice(&len_bytes[..4]);
    } else {
        out.extend_from_slice(&len_bytes);
    }
    for e in elements {
        out.extend_from_slice(&e.index.to_le_bytes());
        out.extend_from_slice(&e.slot.to_le_bytes());
        out.extend_from_slice(&e.offset.to_le_bytes());
        out.extend_from_slice(&e.format.to_le_bytes());
        out.extend_from_slice(&e.step_function_and_rate.to_le_bytes());
    }
    out
}

/// Computes the SHA-1 digest of an input layout using the given `size_t` byte width (`4` or `8`).
#[must_use]
pub fn input_layout_digest_with_width(
    elements: &[InputLayoutElement],
    slot_mask: u32,
    size_t_width: usize,
) -> [u8; 20] {
    let stream = input_layout_stream_with_width(elements, slot_mask, size_t_width);
    Sha1::compute(&stream).0
}

/// Computes the SHA-1 digest of an input layout using the target's native `usize` width.
#[must_use]
pub fn input_layout_digest(elements: &[InputLayoutElement], slot_mask: u32) -> [u8; 20] {
    input_layout_digest_with_width(elements, slot_mask, core::mem::size_of::<usize>())
}

/// Serializes a stream-output layout into the byte stream hashed by `MTLD3D11StreamOutputLayout`.
///
/// `size_t_width` is `4` (win32 / i686) or `8` (win64 / macOS).
#[must_use]
pub fn stream_output_layout_stream_with_width(
    elements: &[StreamOutputElement],
    strides: &[u32; 4],
    rasterized_stream: u32,
    size_t_width: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(20 + size_t_width + elements.len() * 16);
    for &s in strides {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out.extend_from_slice(&rasterized_stream.to_le_bytes());
    let len_bytes = (elements.len() as u64).to_le_bytes();
    if size_t_width == 4 {
        out.extend_from_slice(&len_bytes[..4]);
    } else {
        out.extend_from_slice(&len_bytes);
    }
    for e in elements {
        out.extend_from_slice(&e.register_idx.to_le_bytes());
        out.extend_from_slice(&e.component.to_le_bytes());
        out.extend_from_slice(&e.output_slot.to_le_bytes());
        out.extend_from_slice(&e.offset.to_le_bytes());
    }
    out
}

/// Computes the SHA-1 digest of a stream-output layout using the given `size_t` byte width.
#[must_use]
pub fn stream_output_layout_digest_with_width(
    elements: &[StreamOutputElement],
    strides: &[u32; 4],
    rasterized_stream: u32,
    size_t_width: usize,
) -> [u8; 20] {
    let stream =
        stream_output_layout_stream_with_width(elements, strides, rasterized_stream, size_t_width);
    Sha1::compute(&stream).0
}

/// Computes the SHA-1 digest of a stream-output layout using the target's native `usize` width.
#[must_use]
pub fn stream_output_layout_digest(
    elements: &[StreamOutputElement],
    strides: &[u32; 4],
    rasterized_stream: u32,
) -> [u8; 20] {
    stream_output_layout_digest_with_width(
        elements,
        strides,
        rasterized_stream,
        core::mem::size_of::<usize>(),
    )
}

/// All 8 shader specialization variants (`ShaderVariant` in `d3d11_shader.hpp`).
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ShaderVariant {
    /// `ShaderVariantDefault` (compute shaders).
    Default,
    /// `ShaderVariantVertex`.
    Vertex {
        input_layout_sha1: Option<[u8; 20]>,
        gs_passthrough: u32,
        rasterization_disabled: bool,
    },
    /// `ShaderVariantPixel`.
    Pixel {
        sample_mask: u32,
        dual_source_blending: bool,
        disable_depth_output: bool,
        unorm_output_reg_mask: u32,
    },
    /// `ShaderVariantTessellationVertexHull`.
    TessellationVertexHull {
        input_layout_sha1: Option<[u8; 20]>,
        vs_sha1: [u8; 20],
        index_buffer_format: u32,
        max_potential_tess_factor: u32,
    },
    /// `ShaderVariantTessellationDomain`.
    TessellationDomain {
        hs_sha1: [u8; 20],
        gs_passthrough: u32,
        max_potential_tess_factor: u32,
        rasterization_disabled: bool,
    },
    /// `ShaderVariantVertexStreamOutput`.
    VertexStreamOutput {
        input_layout_sha1: Option<[u8; 20]>,
        so_layout_sha1: [u8; 20],
    },
    /// `ShaderVariantGeometryVertex`.
    GeometryVertex {
        input_layout_sha1: Option<[u8; 20]>,
        gs_sha1: [u8; 20],
        index_buffer_format: u32,
        strip_topology: bool,
    },
    /// `ShaderVariantGeometry`.
    Geometry {
        vs_sha1: [u8; 20],
        strip_topology: bool,
    },
}

impl Clone for ShaderVariant {
    fn clone(&self) -> Self {
        match *self {
            Self::Default => Self::Default,
            Self::Vertex {
                input_layout_sha1,
                gs_passthrough,
                rasterization_disabled,
            } => Self::Vertex {
                input_layout_sha1,
                gs_passthrough,
                rasterization_disabled,
            },
            Self::Pixel {
                sample_mask,
                dual_source_blending,
                disable_depth_output,
                unorm_output_reg_mask,
            } => Self::Pixel {
                sample_mask,
                dual_source_blending,
                disable_depth_output,
                unorm_output_reg_mask,
            },
            Self::TessellationVertexHull {
                input_layout_sha1,
                vs_sha1,
                index_buffer_format,
                max_potential_tess_factor,
            } => Self::TessellationVertexHull {
                input_layout_sha1,
                vs_sha1,
                index_buffer_format,
                max_potential_tess_factor,
            },
            Self::TessellationDomain {
                hs_sha1,
                gs_passthrough,
                max_potential_tess_factor,
                rasterization_disabled,
            } => Self::TessellationDomain {
                hs_sha1,
                gs_passthrough,
                max_potential_tess_factor,
                rasterization_disabled,
            },
            Self::VertexStreamOutput {
                input_layout_sha1,
                so_layout_sha1,
            } => Self::VertexStreamOutput {
                input_layout_sha1,
                so_layout_sha1,
            },
            Self::GeometryVertex {
                input_layout_sha1,
                gs_sha1,
                index_buffer_format,
                strip_topology,
            } => Self::GeometryVertex {
                input_layout_sha1,
                gs_sha1,
                index_buffer_format,
                strip_topology,
            },
            Self::Geometry {
                vs_sha1,
                strip_topology,
            } => Self::Geometry {
                vs_sha1,
                strip_topology,
            },
        }
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

impl ShaderVariant {
    /// Prefix used for the Metal entry-point function name (`cs`, `vs`, `ps`, `vshs`, `ds`, etc.).
    #[must_use]
    pub const fn prefix(&self) -> &'static str {
        match self {
            Self::Default => "cs",
            Self::Vertex { .. } => "vs",
            Self::Pixel { .. } => "ps",
            Self::TessellationVertexHull { .. } => "vshs",
            Self::TessellationDomain { .. } => "ds",
            Self::VertexStreamOutput { .. } => "vsso",
            Self::GeometryVertex { .. } => "vsgs",
            Self::Geometry { .. } => "gs",
        }
    }

    /// Serializes the variant hash payload in the field order of `d3d11_shader.cpp`.
    #[must_use]
    pub fn stream(&self, flags: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(&flags.to_le_bytes());
        match *self {
            Self::Default => {}
            Self::Vertex {
                input_layout_sha1,
                gs_passthrough,
                rasterization_disabled,
            } => {
                out.extend_from_slice(&gs_passthrough.to_le_bytes());
                out.push(u8::from(rasterization_disabled));
                if let Some(il_sha1) = input_layout_sha1 {
                    out.extend_from_slice(&il_sha1);
                }
            }
            Self::Pixel {
                sample_mask,
                dual_source_blending,
                disable_depth_output,
                unorm_output_reg_mask,
            } => {
                out.extend_from_slice(&sample_mask.to_le_bytes());
                out.extend_from_slice(&unorm_output_reg_mask.to_le_bytes());
                out.push(u8::from(dual_source_blending));
                out.push(u8::from(disable_depth_output));
            }
            Self::TessellationVertexHull {
                input_layout_sha1,
                vs_sha1,
                index_buffer_format,
                max_potential_tess_factor,
            } => {
                out.extend_from_slice(&vs_sha1);
                out.extend_from_slice(&index_buffer_format.to_le_bytes());
                out.extend_from_slice(&max_potential_tess_factor.to_le_bytes());
                if let Some(il_sha1) = input_layout_sha1 {
                    out.extend_from_slice(&il_sha1);
                }
            }
            Self::TessellationDomain {
                hs_sha1,
                gs_passthrough,
                max_potential_tess_factor,
                rasterization_disabled,
            } => {
                out.extend_from_slice(&hs_sha1);
                out.extend_from_slice(&gs_passthrough.to_le_bytes());
                out.push(u8::from(rasterization_disabled));
                out.extend_from_slice(&max_potential_tess_factor.to_le_bytes());
            }
            Self::VertexStreamOutput {
                input_layout_sha1,
                so_layout_sha1,
            } => {
                out.extend_from_slice(&so_layout_sha1);
                if let Some(il_sha1) = input_layout_sha1 {
                    out.extend_from_slice(&il_sha1);
                }
            }
            Self::GeometryVertex {
                input_layout_sha1,
                gs_sha1,
                index_buffer_format,
                strip_topology,
            } => {
                out.extend_from_slice(&gs_sha1);
                out.extend_from_slice(&index_buffer_format.to_le_bytes());
                out.push(u8::from(strip_topology));
                if let Some(il_sha1) = input_layout_sha1 {
                    out.extend_from_slice(&il_sha1);
                }
            }
            Self::Geometry {
                vs_sha1,
                strip_topology,
            } => {
                out.extend_from_slice(&vs_sha1);
                out.push(u8::from(strip_topology));
            }
        }
        out
    }

    /// Computes the 20-byte SHA-1 variant digest (`variant_digest` in `d3d11_shader.cpp`).
    #[must_use]
    pub fn digest(&self, flags: u32) -> [u8; 20] {
        let s = self.stream(flags);
        Sha1::compute(&s).0
    }

    /// Computes `(variant_digest, func_name)`.
    ///
    /// `func_name` is `<prefix>_<shader_sha1[..4] hex>_<variant_digest hex>` (`d3d11_shader.cpp`).
    #[must_use]
    pub fn digest_and_name(&self, shader_sha1: &[u8; 20], flags: u32) -> ([u8; 20], String) {
        let digest = self.digest(flags);
        let name = format!(
            "{}_{}_{}",
            self.prefix(),
            hex_bytes(&shader_sha1[..4]),
            hex_bytes(&digest)
        );
        (digest, name)
    }

    /// Formats the Metal function name `<prefix>_<shader_sha1[..4] hex>_<variant_digest hex>`.
    #[must_use]
    pub fn function_name(&self, shader_sha1: &[u8; 20], flags: u32) -> String {
        self.digest_and_name(shader_sha1, flags).1
    }
}

/// Input-assembler layout payload passed into [`CompileArgsChain::build`].
pub struct IaLayoutChainInput<'a> {
    pub slot_mask: u32,
    pub elements: &'a [InputLayoutElement],
}

impl Clone for IaLayoutChainInput<'_> {
    fn clone(&self) -> Self {
        Self {
            slot_mask: self.slot_mask,
            elements: self.elements,
        }
    }
}

/// Stream-output layout payload passed into [`CompileArgsChain::build`].
pub struct SoLayoutChainInput<'a> {
    pub elements: &'a [StreamOutputElement],
    pub strides: [u32; 4],
}

impl Clone for SoLayoutChainInput<'_> {
    fn clone(&self) -> Self {
        Self {
            elements: self.elements,
            strides: self.strides,
        }
    }
}

/// Self-referential pinned chain of `SM50_SHADER_*_DATA` structs passed to `SM50Compile*`.
///
/// The nodes form a `next`-linked list whose head pointer is returned by
/// [`CompileArgsChain::head_ptr`].
pub struct CompileArgsChain {
    func_name: String,
    func_name_cstr: Vec<u8>,
    ia_elements: Vec<Sm50IaInputElement>,
    so_elements: Vec<Sm50StreamOutputElement>,
    common: Sm50ShaderCommonData,
    ia_layout: Sm50ShaderIaInputLayoutData,
    gs_passthrough: Sm50ShaderGsPassThroughData,
    pixel: Sm50ShaderPsoPixelShaderData,
    tessellator: Sm50ShaderPsoTessellatorData,
    stream_output: Sm50ShaderEmulateVertexStreamOutputData,
    geometry: Sm50ShaderPsoGeometryShaderData,
    head_kind: HeadKind,
    _pin: PhantomPinned,
}

enum HeadKind {
    Common,
    GsPassThrough,
    Pixel,
    IaLayout,
    StreamOutput,
    Geometry,
}

const fn arg_type_name(t: u32) -> &'static str {
    match t {
        SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT => "SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT",
        SM50_SHADER_COMMON => "SM50_SHADER_COMMON",
        SM50_SHADER_PSO_PIXEL_SHADER => "SM50_SHADER_PSO_PIXEL_SHADER",
        SM50_SHADER_IA_INPUT_LAYOUT => "SM50_SHADER_IA_INPUT_LAYOUT",
        SM50_SHADER_GS_PASS_THROUGH => "SM50_SHADER_GS_PASS_THROUGH",
        SM50_SHADER_PSO_GEOMETRY_SHADER => "SM50_SHADER_PSO_GEOMETRY_SHADER",
        SM50_SHADER_PSO_TESSELLATOR => "SM50_SHADER_PSO_TESSELLATOR",
        _ => "UNKNOWN",
    }
}

impl CompileArgsChain {
    /// Builds and pins the `SM50Compile*` argument chain using default Metal version `310`.
    #[must_use]
    pub fn build(
        variant: &ShaderVariant,
        shader_sha1: &[u8; 20],
        flags: u32,
        ia_input: Option<IaLayoutChainInput<'_>>,
        so_input: Option<SoLayoutChainInput<'_>>,
    ) -> Pin<Box<Self>> {
        Self::build_with_version(
            variant,
            shader_sha1,
            SM50_SHADER_METAL_310,
            flags,
            ia_input,
            so_input,
        )
    }

    /// Builds and pins the `SM50Compile*` argument chain for `variant` (`d3d11_shader.cpp`).
    #[must_use]
    pub fn build_with_version(
        variant: &ShaderVariant,
        shader_sha1: &[u8; 20],
        metal_version: u32,
        flags: u32,
        ia_input: Option<IaLayoutChainInput<'_>>,
        so_input: Option<SoLayoutChainInput<'_>>,
    ) -> Pin<Box<Self>> {
        let func_name = variant.function_name(shader_sha1, flags);
        let mut func_name_cstr = func_name.as_bytes().to_vec();
        func_name_cstr.push(0);

        let (ia_elements, ia_slot_mask, has_ia) = ia_input.map_or_else(
            || (Vec::new(), 0, false),
            |ia| {
                (
                    ia.elements.iter().map(ia_element_from_desc).collect(),
                    ia.slot_mask,
                    true,
                )
            },
        );

        let (so_elements, so_strides) = so_input.map_or_else(
            || (Vec::new(), [0; 4]),
            |so| {
                (
                    so.elements.iter().map(so_element_from_desc).collect(),
                    so.strides,
                )
            },
        );

        let mut boxed = Box::pin(Self {
            func_name,
            func_name_cstr,
            ia_elements,
            so_elements,
            common: Sm50ShaderCommonData {
                next: ptr::null_mut(),
                r#type: SM50_SHADER_COMMON,
                metal_version,
                flags,
            },
            ia_layout: Sm50ShaderIaInputLayoutData {
                next: ptr::null_mut(),
                r#type: SM50_SHADER_IA_INPUT_LAYOUT,
                index_buffer_format: 0,
                slot_mask: ia_slot_mask,
                num_elements: 0,
                elements: ptr::null_mut(),
            },
            gs_passthrough: Sm50ShaderGsPassThroughData {
                next: ptr::null_mut(),
                r#type: SM50_SHADER_GS_PASS_THROUGH,
                u: Sm50ShaderGsPassThroughDataUnion { data_encoded: 0 },
                rasterization_disabled: 0,
            },
            pixel: Sm50ShaderPsoPixelShaderData {
                next: ptr::null_mut(),
                r#type: SM50_SHADER_PSO_PIXEL_SHADER,
                sample_mask: 0,
                dual_source_blending: 0,
                disable_depth_output: 0,
                unorm_output_reg_mask: 0,
                pixel_formats: [0; 8],
            },
            tessellator: Sm50ShaderPsoTessellatorData {
                next: ptr::null_mut(),
                r#type: SM50_SHADER_PSO_TESSELLATOR,
                max_potential_tess_factor: 0,
            },
            stream_output: Sm50ShaderEmulateVertexStreamOutputData {
                next: ptr::null_mut(),
                r#type: SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT,
                num_output_slots: 0,
                num_elements: 0,
                strides: so_strides,
                elements: ptr::null_mut(),
            },
            geometry: Sm50ShaderPsoGeometryShaderData {
                next: ptr::null_mut(),
                r#type: SM50_SHADER_PSO_GEOMETRY_SHADER,
                strip_topology: 0,
            },
            head_kind: HeadKind::Common,
            _pin: PhantomPinned,
        });

        // SAFETY: `boxed` is pinned on the heap; we only wire internal pointers, never moving `*this`.
        let this = unsafe { boxed.as_mut().get_unchecked_mut() };
        this.ia_layout.num_elements = u32::try_from(this.ia_elements.len()).unwrap_or(u32::MAX);
        this.ia_layout.elements = if this.ia_elements.is_empty() {
            ptr::null_mut()
        } else {
            this.ia_elements.as_mut_ptr()
        };
        this.stream_output.num_elements = u32::try_from(this.so_elements.len()).unwrap_or(u32::MAX);
        this.stream_output.elements = if this.so_elements.is_empty() {
            ptr::null_mut()
        } else {
            this.so_elements.as_mut_ptr()
        };

        let common_ptr: *mut c_void = (&raw mut this.common).cast();

        match *variant {
            ShaderVariant::Default => {
                this.head_kind = HeadKind::Common;
            }
            ShaderVariant::Vertex {
                gs_passthrough,
                rasterization_disabled,
                ..
            } => {
                this.gs_passthrough.u = Sm50ShaderGsPassThroughDataUnion {
                    data_encoded: gs_passthrough,
                };
                this.gs_passthrough.rasterization_disabled = u8::from(rasterization_disabled);
                if has_ia {
                    this.ia_layout.index_buffer_format = 0;
                    this.ia_layout.next = common_ptr;
                    this.gs_passthrough.next = (&raw mut this.ia_layout).cast();
                } else {
                    this.gs_passthrough.next = common_ptr;
                }
                this.head_kind = HeadKind::GsPassThrough;
            }
            ShaderVariant::Pixel {
                sample_mask,
                dual_source_blending,
                disable_depth_output,
                unorm_output_reg_mask,
            } => {
                this.pixel.sample_mask = sample_mask;
                this.pixel.dual_source_blending = u8::from(dual_source_blending);
                this.pixel.disable_depth_output = u8::from(disable_depth_output);
                this.pixel.unorm_output_reg_mask = unorm_output_reg_mask;
                this.pixel.next = common_ptr;
                this.head_kind = HeadKind::Pixel;
            }
            ShaderVariant::TessellationVertexHull {
                index_buffer_format,
                max_potential_tess_factor,
                ..
            } => {
                this.tessellator.max_potential_tess_factor = max_potential_tess_factor;
                this.tessellator.next = common_ptr;
                this.ia_layout.index_buffer_format = index_buffer_format;
                this.ia_layout.next = (&raw mut this.tessellator).cast();
                this.head_kind = HeadKind::IaLayout;
            }
            ShaderVariant::TessellationDomain {
                gs_passthrough,
                max_potential_tess_factor,
                rasterization_disabled,
                ..
            } => {
                this.tessellator.max_potential_tess_factor = max_potential_tess_factor;
                this.tessellator.next = common_ptr;
                this.gs_passthrough.u = Sm50ShaderGsPassThroughDataUnion {
                    data_encoded: gs_passthrough,
                };
                this.gs_passthrough.rasterization_disabled = u8::from(rasterization_disabled);
                this.gs_passthrough.next = (&raw mut this.tessellator).cast();
                this.head_kind = HeadKind::GsPassThrough;
            }
            ShaderVariant::VertexStreamOutput { .. } => {
                // `d3d11_shader.cpp:329`: `data_so.num_output_slots = 0`
                this.stream_output.num_output_slots = 0;
                if has_ia {
                    this.ia_layout.index_buffer_format = 0;
                    this.ia_layout.next = common_ptr;
                    this.stream_output.next = (&raw mut this.ia_layout).cast();
                } else {
                    this.stream_output.next = common_ptr;
                }
                this.head_kind = HeadKind::StreamOutput;
            }
            ShaderVariant::GeometryVertex {
                index_buffer_format,
                strip_topology,
                ..
            } => {
                this.ia_layout.index_buffer_format = index_buffer_format;
                this.ia_layout.next = common_ptr;
                this.geometry.strip_topology = u8::from(strip_topology);
                this.geometry.next = (&raw mut this.ia_layout).cast();
                this.head_kind = HeadKind::Geometry;
            }
            ShaderVariant::Geometry { strip_topology, .. } => {
                this.geometry.strip_topology = u8::from(strip_topology);
                this.geometry.next = common_ptr;
                this.head_kind = HeadKind::Geometry;
            }
        }

        boxed
    }

    /// Returns the Metal entry-point function name as a Rust string slice.
    #[must_use]
    pub fn func_name_str(&self) -> &str {
        &self.func_name
    }

    /// Returns the NUL-terminated Metal entry-point function name pointer for `SM50Compile*`.
    #[must_use]
    pub const fn func_name_cstr(&self) -> *const c_char {
        self.func_name_cstr.as_ptr().cast()
    }

    /// Returns the raw head pointer suitable for passing as `args` to `SM50Compile*`.
    #[must_use]
    pub fn head_ptr(self: &Pin<Box<Self>>) -> *mut Sm50ShaderCompilationArgumentData {
        let p: *const c_void = match self.head_kind {
            HeadKind::Common => (&raw const self.common).cast(),
            HeadKind::GsPassThrough => (&raw const self.gs_passthrough).cast(),
            HeadKind::Pixel => (&raw const self.pixel).cast(),
            HeadKind::IaLayout => (&raw const self.ia_layout).cast(),
            HeadKind::StreamOutput => (&raw const self.stream_output).cast(),
            HeadKind::Geometry => (&raw const self.geometry).cast(),
        };
        p.cast_mut().cast()
    }

    /// Formats the linked list starting at `head_ptr` in the golden `chains.txt` format.
    #[must_use]
    pub fn dump_chain(
        self: &Pin<Box<Self>>,
        header: &str,
        compile_fn: &str,
        h0: usize,
        h1: usize,
    ) -> String {
        let mut out = String::new();
        let head = self.head_ptr();
        let mut depth = 0usize;
        let mut cur = head;
        while !cur.is_null() {
            depth += 1;
            // SAFETY: every node in the chain starts with `Sm50ShaderCompilationArgumentData`.
            cur = unsafe { (*cur).next.cast() };
        }

        let _ = writeln!(
            out,
            "CHAIN_CALL {header} compile_fn={compile_fn} h0=0x{h0:x} h1=0x{h1:x} func_name={} chain_len={depth}",
            self.func_name_str()
        );

        let mut idx = 0usize;
        cur = head;
        while !cur.is_null() {
            // SAFETY: `cur` points to a valid chain node inside `self`.
            let base = unsafe { &*cur };
            let next_node: *const Sm50ShaderCompilationArgumentData = base.next.cast();
            let next_tag = if next_node.is_null() {
                "null"
            } else {
                // SAFETY: `next_node` is a valid chain node inside `self`.
                arg_type_name(unsafe { (*next_node).r#type })
            };
            let cur_tag = arg_type_name(base.r#type);

            match base.r#type {
                SM50_SHADER_COMMON => {
                    // SAFETY: `r#type` identifies `Sm50ShaderCommonData`.
                    let d = unsafe { &*cur.cast::<Sm50ShaderCommonData>() };
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}({cur_tag}) next={next_tag} metal_version={} flags=0x{:x}",
                        d.r#type, d.metal_version, d.flags
                    );
                }
                SM50_SHADER_GS_PASS_THROUGH => {
                    // SAFETY: `r#type` identifies `Sm50ShaderGsPassThroughData`.
                    let d = unsafe { &*cur.cast::<Sm50ShaderGsPassThroughData>() };
                    // SAFETY: `u` is a union of `u32` and `4 x u8`.
                    let enc = unsafe { d.u.data_encoded };
                    // SAFETY: see above.
                    let data = unsafe { d.u.data };
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}({cur_tag}) next={next_tag} DataEncoded=0x{enc:08x}(RTReg={},RTComp={},VPReg={},VPComp={}) RasterizationDisabled={}",
                        d.r#type,
                        data.render_target_array_index_reg,
                        data.render_target_array_index_component,
                        data.viewport_array_index_reg,
                        data.viewport_array_index_component,
                        u32::from(d.rasterization_disabled != 0)
                    );
                }
                SM50_SHADER_IA_INPUT_LAYOUT => {
                    // SAFETY: `r#type` identifies `Sm50ShaderIaInputLayoutData`.
                    let d = unsafe { &*cur.cast::<Sm50ShaderIaInputLayoutData>() };
                    let ptr_str = if d.elements.is_null() {
                        "null"
                    } else {
                        "nonnull"
                    };
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}({cur_tag}) next={next_tag} index_buffer_format={} slot_mask=0x{:08x} num_elements={} elements_ptr={ptr_str}",
                        d.r#type, d.index_buffer_format, d.slot_mask, d.num_elements
                    );
                    if d.num_elements > 0 && !d.elements.is_null() {
                        // SAFETY: `elements` points to `num_elements` valid elements.
                        let elems = unsafe {
                            core::slice::from_raw_parts(d.elements, d.num_elements as usize)
                        };
                        for (i, e) in elems.iter().enumerate() {
                            let _ = writeln!(
                                out,
                                "CHAIN_IA_ELEM {header} node={idx} elem={i} reg={} slot={} aligned_byte_offset={} format={} step_function={} step_rate={}",
                                e.reg,
                                e.slot,
                                e.aligned_byte_offset,
                                e.format,
                                e.step_function(),
                                e.step_rate()
                            );
                        }
                    }
                }
                SM50_SHADER_PSO_PIXEL_SHADER => {
                    // SAFETY: `r#type` identifies `Sm50ShaderPsoPixelShaderData`.
                    let d = unsafe { &*cur.cast::<Sm50ShaderPsoPixelShaderData>() };
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}({cur_tag}) next={next_tag} sample_mask=0x{:08x} dual_source_blending={} disable_depth_output={} unorm_output_reg_mask=0x{:08x} pixel_formats={},{},{},{},{},{},{},{}",
                        d.r#type,
                        d.sample_mask,
                        u32::from(d.dual_source_blending != 0),
                        u32::from(d.disable_depth_output != 0),
                        d.unorm_output_reg_mask,
                        d.pixel_formats[0],
                        d.pixel_formats[1],
                        d.pixel_formats[2],
                        d.pixel_formats[3],
                        d.pixel_formats[4],
                        d.pixel_formats[5],
                        d.pixel_formats[6],
                        d.pixel_formats[7],
                    );
                }
                SM50_SHADER_PSO_TESSELLATOR => {
                    // SAFETY: `r#type` identifies `Sm50ShaderPsoTessellatorData`.
                    let d = unsafe { &*cur.cast::<Sm50ShaderPsoTessellatorData>() };
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}({cur_tag}) next={next_tag} max_potential_tess_factor={}",
                        d.r#type, d.max_potential_tess_factor
                    );
                }
                SM50_SHADER_EMULATE_VERTEX_STREAM_OUTPUT => {
                    // SAFETY: `r#type` identifies `Sm50ShaderEmulateVertexStreamOutputData`.
                    let d = unsafe { &*cur.cast::<Sm50ShaderEmulateVertexStreamOutputData>() };
                    let ptr_str = if d.elements.is_null() {
                        "null"
                    } else {
                        "nonnull"
                    };
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}({cur_tag}) next={next_tag} num_output_slots={} num_elements={} strides={},{},{},{} elements_ptr={ptr_str}",
                        d.r#type,
                        d.num_output_slots,
                        d.num_elements,
                        d.strides[0],
                        d.strides[1],
                        d.strides[2],
                        d.strides[3],
                    );
                    if d.num_elements > 0 && !d.elements.is_null() {
                        // SAFETY: `elements` points to `num_elements` valid descriptors.
                        let elems = unsafe {
                            core::slice::from_raw_parts(d.elements, d.num_elements as usize)
                        };
                        for (i, e) in elems.iter().enumerate() {
                            let _ = writeln!(
                                out,
                                "CHAIN_SO_ELEM {header} node={idx} elem={i} reg_id=0x{:08x} component={} output_slot={} offset={}",
                                e.reg_id, e.component, e.output_slot, e.offset
                            );
                        }
                    }
                }
                SM50_SHADER_PSO_GEOMETRY_SHADER => {
                    // SAFETY: `r#type` identifies `Sm50ShaderPsoGeometryShaderData`.
                    let d = unsafe { &*cur.cast::<Sm50ShaderPsoGeometryShaderData>() };
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}({cur_tag}) next={next_tag} strip_topology={}",
                        d.r#type,
                        u32::from(d.strip_topology != 0)
                    );
                }
                _ => {
                    let _ = writeln!(
                        out,
                        "CHAIN_NODE {header} node={idx} type={}(UNKNOWN) next={next_tag}",
                        base.r#type
                    );
                }
            }

            cur = base.next.cast();
            idx += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests;
