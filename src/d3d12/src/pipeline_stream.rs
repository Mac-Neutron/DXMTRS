//! Pipeline state streams (`CreatePipelineState`): the subobjects a stream describes.
//!
//! A stream is a run of subobjects, each a `u32` type and its data laid out as the C struct
//! `{ type; data }` (the data at its own alignment), the next one at the following multiple of a
//! pointer's size. [`parse_pipeline_stream`] reads one the way DXMT's C++ `ParsePipelineStream`
//! does, into a graphics and a compute description with the C++'s defaults for what the stream
//! leaves out, which are not all D3D12's: only render target 0 has a write mask, and both stencil
//! masks are 0.
//!
//! What the C++ logs is reported to the caller, a [`Warning`] as it happens and a [`StreamError`]
//! at the end, so the front end writes it through its own log.

use core::{ffi::c_void, fmt, slice};

use dxmt_util::com::Hresult;

use crate::abi::pipeline::{
    BlendDesc, COLOR_WRITE_ENABLE_ALL, COMPARISON_FUNC_ALWAYS, COMPARISON_FUNC_LESS,
    CONSERVATIVE_RASTERIZATION_MODE_OFF, CULL_MODE_BACK, CachedPipelineState,
    ComputePipelineStateDesc, DEFAULT_SAMPLE_MASK, DEPTH_WRITE_MASK_ALL, DepthStencilDesc,
    DepthStencilDesc1, E_INVALIDARG, E_NOTIMPL, FILL_MODE_SOLID, GraphicsPipelineStateDesc,
    InputLayoutDesc, PipelineStateStreamDesc, RasterizerDesc, RtFormatArray, STENCIL_OP_KEEP,
    SUBOBJECT_TYPE_AS, SUBOBJECT_TYPE_BLEND, SUBOBJECT_TYPE_CACHED_PSO, SUBOBJECT_TYPE_CS,
    SUBOBJECT_TYPE_DEPTH_STENCIL, SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT,
    SUBOBJECT_TYPE_DEPTH_STENCIL1, SUBOBJECT_TYPE_DS, SUBOBJECT_TYPE_FLAGS, SUBOBJECT_TYPE_GS,
    SUBOBJECT_TYPE_HS, SUBOBJECT_TYPE_IB_STRIP_CUT_VALUE, SUBOBJECT_TYPE_INPUT_LAYOUT,
    SUBOBJECT_TYPE_MS, SUBOBJECT_TYPE_NODE_MASK, SUBOBJECT_TYPE_PRIMITIVE_TOPOLOGY,
    SUBOBJECT_TYPE_PS, SUBOBJECT_TYPE_RASTERIZER, SUBOBJECT_TYPE_RENDER_TARGET_FORMATS,
    SUBOBJECT_TYPE_ROOT_SIGNATURE, SUBOBJECT_TYPE_SAMPLE_DESC, SUBOBJECT_TYPE_SAMPLE_MASK,
    SUBOBJECT_TYPE_STREAM_OUTPUT, SUBOBJECT_TYPE_VIEW_INSTANCING, SUBOBJECT_TYPE_VS, SampleDesc,
    ShaderBytecode, StreamOutputDesc, Subobject, ViewInstancingDesc,
};

/// The subobject types a compute pipeline cannot have, as a mask of `defined`'s bits.
const GRAPHICS_ONLY: u32 =
    (1 << SUBOBJECT_TYPE_VS) | (1 << SUBOBJECT_TYPE_AS) | (1 << SUBOBJECT_TYPE_MS);

/// A line the C++ logs as a warning while it reads a stream.
#[derive(Debug, PartialEq, Eq)]
pub enum Warning {
    /// A `DEPTH_STENCIL1` subobject with `DepthBoundsTestEnable` set: there is no such test.
    DepthBoundsIgnored,
    /// A `VIEW_INSTANCING` subobject with flags set: the pipeline has no view instancing.
    ViewInstancingIgnored,
}

impl fmt::Display for Warning {
    /// The C++'s message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DepthBoundsIgnored => "CreatePipelineState: ignore DepthBoundsTestEnable",
            Self::ViewInstancingIgnored => "CreatePipelineState: ignore ViewInstancing",
        })
    }
}

/// Why a stream is refused, which the C++ logs as an error and answers with an `HRESULT`.
#[derive(Debug, PartialEq, Eq)]
pub enum StreamError {
    /// Fewer bytes are left than a subobject's type or its `{ type, data }` need.
    InvalidStream,
    /// A subobject type that appeared before, here with its number.
    Duplicate(u32),
    /// A subobject type this reader does not know, here with its number.
    Unhandled(u32),
    /// An amplification shader.
    UnsupportedAs,
    /// A mesh shader.
    UnsupportedMs,
    /// A compute shader beside a vertex, amplification or mesh shader.
    InvalidCompute,
}

impl StreamError {
    /// The `HRESULT` `CreatePipelineState` answers.
    #[must_use]
    pub const fn hresult(&self) -> Hresult {
        match self {
            Self::UnsupportedAs | Self::UnsupportedMs => E_NOTIMPL,
            Self::InvalidStream
            | Self::Duplicate(_)
            | Self::Unhandled(_)
            | Self::InvalidCompute => E_INVALIDARG,
        }
    }
}

impl fmt::Display for StreamError {
    /// The C++'s message, its spelling of "subobject" in the duplicate one included.
    ///
    /// The C++ prints the type as an `int`, so one from 2^31 on reads as negative.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStream => f.write_str("CreatePipelineState: invalid stream"),
            Self::Duplicate(ty) => {
                write!(
                    f,
                    "CreatePipelineState: duplicated subobejct type {}",
                    ty.cast_signed()
                )
            }
            Self::Unhandled(ty) => {
                write!(
                    f,
                    "CreatePipelineState: unhandled subobject type {}",
                    ty.cast_signed()
                )
            }
            Self::UnsupportedAs => f.write_str("CreatePipelineState: unsupported AS"),
            Self::UnsupportedMs => f.write_str("CreatePipelineState: unsupported MS"),
            Self::InvalidCompute => {
                f.write_str("CreatePipelineState: invalid compute pipeline state stream")
            }
        }
    }
}

/// What a stream describes: both descriptions are filled, `is_compute` says which one is meant.
pub struct ParsedStream {
    pub graphics: GraphicsPipelineStateDesc,
    pub compute: ComputePipelineStateDesc,
    /// The stream has a compute shader.
    pub is_compute: bool,
}

/// A type valid for every bit pattern, which a stream's bytes can be read as.
///
/// # Safety
///
/// Implementors are integers, floats and raw pointers, and `repr(C)` structures and arrays of
/// them: no reference, `bool` or enumeration, and zero bytes are a valid value.
unsafe trait Plain {}

macro_rules! plain {
    ($($ty:ty),+ $(,)?) => {
        $(
            // SAFETY: each type listed is an integer, a raw pointer or a `repr(C)` structure of those.
            unsafe impl Plain for $ty {}
        )+
    };
}

plain!(
    u32,
    *mut c_void,
    ShaderBytecode,
    StreamOutputDesc,
    BlendDesc,
    RasterizerDesc,
    DepthStencilDesc,
    InputLayoutDesc,
    RtFormatArray,
    SampleDesc,
    CachedPipelineState,
    DepthStencilDesc1,
    ViewInstancingDesc,
    GraphicsPipelineStateDesc,
    ComputePipelineStateDesc,
);

/// A value with every byte zero, as C++'s `= {}` leaves one.
const fn zeroed<T: Plain>() -> T {
    // SAFETY: `Plain` types are valid for every bit pattern, the zero one included.
    unsafe { core::mem::zeroed() }
}

/// A reader over a stream's bytes.
struct Cursor<'a> {
    stream: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    /// The stream has no subobject left.
    const fn done(&self) -> bool {
        self.pos >= self.stream.len()
    }

    /// The type of the subobject at the cursor.
    fn subobject_type(&self) -> Result<u32, StreamError> {
        let rest = self.stream.get(self.pos..).unwrap_or_default();
        let bytes = rest.first_chunk::<4>().ok_or(StreamError::InvalidStream)?;
        Ok(u32::from_ne_bytes(*bytes))
    }

    /// The data of the subobject at the cursor, which is read as its `{ type, data }` struct.
    ///
    /// The cursor moves to the next subobject: the struct's size rounded up to a pointer's.
    fn take<T: Plain>(&mut self) -> Result<T, StreamError> {
        let size = size_of::<Subobject<T>>();
        let rest = self.stream.get(self.pos..).unwrap_or_default();
        if rest.len() < size {
            return Err(StreamError::InvalidStream);
        }
        // SAFETY: `rest` holds at least `size` bytes, checked above; `Subobject<T>` is a `u32` and a
        // `Plain`, valid for every bit pattern, and the read does not need alignment.
        let subobject = unsafe { rest.as_ptr().cast::<Subobject<T>>().read_unaligned() };
        self.pos += size.next_multiple_of(size_of::<usize>());
        Ok(subobject.data)
    }
}

/// The graphics description with D3D12's defaults as the C++ sets them, and an empty compute one.
const fn defaults() -> (GraphicsPipelineStateDesc, ComputePipelineStateDesc) {
    let mut graphics = zeroed::<GraphicsPipelineStateDesc>();
    let depth_stencil = &mut graphics.depth_stencil_state;
    depth_stencil.depth_enable = 1;
    depth_stencil.depth_write_mask = DEPTH_WRITE_MASK_ALL;
    depth_stencil.depth_func = COMPARISON_FUNC_LESS;
    depth_stencil.front_face.stencil_func = COMPARISON_FUNC_ALWAYS;
    depth_stencil.front_face.stencil_depth_fail_op = STENCIL_OP_KEEP;
    depth_stencil.front_face.stencil_pass_op = STENCIL_OP_KEEP;
    depth_stencil.front_face.stencil_fail_op = STENCIL_OP_KEEP;
    depth_stencil.back_face = depth_stencil.front_face;
    let rasterizer = &mut graphics.rasterizer_state;
    rasterizer.fill_mode = FILL_MODE_SOLID;
    rasterizer.cull_mode = CULL_MODE_BACK;
    rasterizer.depth_clip_enable = 1;
    rasterizer.conservative_raster = CONSERVATIVE_RASTERIZATION_MODE_OFF;
    graphics.blend_state.render_target[0].render_target_write_mask = COLOR_WRITE_ENABLE_ALL;
    graphics.sample_desc.count = 1;
    graphics.sample_desc.quality = 0;
    graphics.sample_mask = DEFAULT_SAMPLE_MASK;
    (graphics, zeroed())
}

/// The bytes of `desc`'s stream.
///
/// # Safety
///
/// `desc.pipeline_state_subobject_stream` is valid for reading `desc.size_in_bytes` bytes for as
/// long as the bytes are used, or `size_in_bytes` is 0.
#[must_use]
pub const unsafe fn stream_bytes(desc: &PipelineStateStreamDesc) -> &[u8] {
    if desc.size_in_bytes == 0 {
        return &[];
    }
    // SAFETY: the caller promises the stream is readable for `size_in_bytes` bytes.
    unsafe {
        slice::from_raw_parts(
            desc.pipeline_state_subobject_stream.cast::<u8>(),
            desc.size_in_bytes,
        )
    }
}

/// Reads a pipeline state stream, calling `warn` for each warning in the order the C++ logs them.
///
/// # Errors
///
/// A [`StreamError`] for a stream the C++ refuses: a subobject cut short, a type twice or unknown,
/// an amplification or mesh shader, or a compute shader with a vertex, amplification or mesh one.
pub fn parse_pipeline_stream(
    stream: &[u8],
    warn: &mut impl FnMut(Warning),
) -> Result<ParsedStream, StreamError> {
    let (mut graphics, mut compute) = defaults();
    let mut cursor = Cursor { stream, pos: 0 };
    let mut defined = 0_u32;
    while !cursor.done() {
        let ty = cursor.subobject_type()?;
        // A type of 32 or more does not alias another's bit in the C++ either, the shift is modulo 32.
        let bit = 1_u32.wrapping_shl(ty);
        if defined & bit != 0 {
            return Err(StreamError::Duplicate(ty));
        }
        defined |= bit;
        match ty {
            SUBOBJECT_TYPE_ROOT_SIGNATURE => {
                let root_signature: *mut c_void = cursor.take()?;
                compute.root_signature = root_signature;
                graphics.root_signature = root_signature;
            }
            SUBOBJECT_TYPE_VS => graphics.vs = cursor.take()?,
            SUBOBJECT_TYPE_PS => graphics.ps = cursor.take()?,
            SUBOBJECT_TYPE_DS => graphics.ds = cursor.take()?,
            SUBOBJECT_TYPE_HS => graphics.hs = cursor.take()?,
            SUBOBJECT_TYPE_GS => graphics.gs = cursor.take()?,
            SUBOBJECT_TYPE_CS => compute.cs = cursor.take()?,
            SUBOBJECT_TYPE_STREAM_OUTPUT => graphics.stream_output = cursor.take()?,
            SUBOBJECT_TYPE_BLEND => graphics.blend_state = cursor.take()?,
            SUBOBJECT_TYPE_SAMPLE_MASK => graphics.sample_mask = cursor.take()?,
            SUBOBJECT_TYPE_RASTERIZER => graphics.rasterizer_state = cursor.take()?,
            SUBOBJECT_TYPE_DEPTH_STENCIL => graphics.depth_stencil_state = cursor.take()?,
            SUBOBJECT_TYPE_INPUT_LAYOUT => graphics.input_layout = cursor.take()?,
            SUBOBJECT_TYPE_IB_STRIP_CUT_VALUE => graphics.ib_strip_cut_value = cursor.take()?,
            SUBOBJECT_TYPE_PRIMITIVE_TOPOLOGY => {
                graphics.primitive_topology_type = cursor.take()?;
            }
            SUBOBJECT_TYPE_RENDER_TARGET_FORMATS => {
                let formats: RtFormatArray = cursor.take()?;
                graphics.rtv_formats = formats.rt_formats;
                graphics.num_render_targets = formats.num_render_targets;
            }
            SUBOBJECT_TYPE_DEPTH_STENCIL_FORMAT => graphics.dsv_format = cursor.take()?,
            SUBOBJECT_TYPE_SAMPLE_DESC => graphics.sample_desc = cursor.take()?,
            SUBOBJECT_TYPE_NODE_MASK => {
                let node_mask: u32 = cursor.take()?;
                graphics.node_mask = node_mask;
                compute.node_mask = node_mask;
            }
            SUBOBJECT_TYPE_CACHED_PSO => {
                let cached: CachedPipelineState = cursor.take()?;
                graphics.cached_pso = cached;
                compute.cached_pso = cached;
            }
            SUBOBJECT_TYPE_FLAGS => {
                let flags: u32 = cursor.take()?;
                graphics.flags = flags;
                compute.flags = flags;
            }
            SUBOBJECT_TYPE_DEPTH_STENCIL1 => {
                let desc: DepthStencilDesc1 = cursor.take()?;
                let state = &mut graphics.depth_stencil_state;
                state.stencil_enable = desc.stencil_enable;
                state.depth_enable = desc.depth_enable;
                state.depth_func = desc.depth_func;
                state.depth_write_mask = desc.depth_write_mask;
                state.stencil_write_mask = desc.stencil_write_mask;
                state.stencil_read_mask = desc.stencil_read_mask;
                state.back_face = desc.back_face;
                state.front_face = desc.front_face;
                if desc.depth_bounds_test_enable != 0 {
                    warn(Warning::DepthBoundsIgnored);
                }
            }
            SUBOBJECT_TYPE_VIEW_INSTANCING => {
                let desc: ViewInstancingDesc = cursor.take()?;
                if desc.flags != 0 {
                    warn(Warning::ViewInstancingIgnored);
                }
            }
            SUBOBJECT_TYPE_AS => {
                let shader: ShaderBytecode = cursor.take()?;
                if !shader.shader_bytecode.is_null() {
                    return Err(StreamError::UnsupportedAs);
                }
            }
            SUBOBJECT_TYPE_MS => {
                let shader: ShaderBytecode = cursor.take()?;
                if !shader.shader_bytecode.is_null() {
                    return Err(StreamError::UnsupportedMs);
                }
            }
            _ => return Err(StreamError::Unhandled(ty)),
        }
    }
    let is_compute = !compute.cs.shader_bytecode.is_null();
    if is_compute && defined & GRAPHICS_ONLY != 0 {
        return Err(StreamError::InvalidCompute);
    }
    Ok(ParsedStream {
        graphics,
        compute,
        is_compute,
    })
}

#[cfg(test)]
mod tests;
