//! Context internal state and dirty flags (`d3d11_context_state.hpp`).

use core::{ffi::c_void, fmt, ptr};

use super::{CommandBufferState, Context, Mode, binding_set::BindingSet};
use crate::{
    abi::records::{D3d11Viewport, Rect},
    com::{add_ref, release},
};

/// Number of programmable shader stages (`ShaderStage`).
pub const NUM_SHADER_STAGES: usize = 6;

/// `D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT`.
pub const CONSTANT_BUFFER_SLOTS: usize = 14;

/// `D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT`.
pub const SAMPLER_SLOTS: usize = 16;

/// `D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT`.
pub const SRV_SLOTS: usize = 128;

/// `D3D11_1_UAV_SLOT_COUNT`.
pub const UAV_SLOTS: usize = 64;

/// `D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT`.
pub const VERTEX_BUFFER_SLOTS: usize = 32;

/// `D3D11_SO_BUFFER_SLOT_COUNT`.
pub const STREAM_OUTPUT_SLOTS: usize = 4;

/// `D3D11_SIMULTANEOUS_RENDER_TARGET_COUNT`.
pub const RENDER_TARGET_SLOTS: usize = 8;

/// `D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE`.
pub const VIEWPORT_AND_SCISSOR_SLOTS: usize = 16;

/// `D3D11_REQ_CONSTANT_BUFFER_ELEMENT_COUNT`.
pub const REQ_CONSTANT_BUFFER_ELEMENTS: u32 = 4096;

/// `D3D11_KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL`.
pub const KEEP_RENDER_TARGETS_AND_DEPTH_STENCIL: u32 = 0xffff_ffff;

/// `D3D11_KEEP_UNORDERED_ACCESS_VIEWS`.
pub const KEEP_UNORDERED_ACCESS_VIEWS: u32 = 0xffff_ffff;

/// `D3D11_DEFAULT_SAMPLE_MASK`.
pub const DEFAULT_SAMPLE_MASK: u32 = 0xffff_ffff;

/// Default blend factor (`[1.0; 4]`).
pub const DEFAULT_BLEND_FACTOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Pipeline shader stage index (`ShaderStage` in `d3d11_context_state.hpp`).
#[repr(usize)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex = 0,
    Pixel = 1,
    Geometry = 2,
    Hull = 3,
    Domain = 4,
    Compute = 5,
}

impl ShaderStage {
    /// Returns the stage array index (`0..6`).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Debug for ShaderStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Vertex => "Vertex",
            Self::Pixel => "Pixel",
            Self::Geometry => "Geometry",
            Self::Hull => "Hull",
            Self::Domain => "Domain",
            Self::Compute => "Compute",
        };
        f.write_str(name)
    }
}

fn retain_raw(raw: *mut c_void) {
    // SAFETY: `raw` is either null or a valid COM interface pointer.
    unsafe { add_ref(raw) };
}

fn release_raw(raw: *mut c_void) {
    // SAFETY: `raw` is either null or a live COM interface pointer owned by `ComPtr`.
    unsafe { release(raw) };
}

/// Reference-counted COM interface pointer (`Com<T>` in C++).
#[repr(transparent)]
pub struct ComPtr(*mut c_void);

impl Default for ComPtr {
    fn default() -> Self {
        Self::null()
    }
}

impl Clone for ComPtr {
    fn clone(&self) -> Self {
        retain_raw(self.0);
        Self(self.0)
    }
}

impl Drop for ComPtr {
    fn drop(&mut self) {
        release_raw(self.0);
    }
}

impl PartialEq for ComPtr {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for ComPtr {}

impl ComPtr {
    /// Null COM pointer.
    #[must_use]
    pub const fn null() -> Self {
        Self(ptr::null_mut())
    }

    /// Retains `raw` (`AddRef`) and wraps it in a [`ComPtr`].
    #[must_use]
    pub fn from_raw_or_null(raw: *mut c_void) -> Self {
        retain_raw(raw);
        Self(raw)
    }

    /// Raw pointer without changing the reference count.
    #[must_use]
    pub const fn as_ptr(&self) -> *mut c_void {
        self.0
    }

    /// Returns `true` if the pointer is null.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        self.0.is_null()
    }

    /// Increments the reference count (if non-null) and returns the raw pointer.
    #[must_use]
    pub fn get_add_ref(&self) -> *mut c_void {
        retain_raw(self.0);
        self.0
    }

    /// Takes ownership of the raw pointer without releasing it, leaving `self` null.
    #[must_use]
    pub const fn take_raw(&mut self) -> *mut c_void {
        core::mem::replace(&mut self.0, ptr::null_mut())
    }

    /// Wraps `raw` without calling `AddRef` (transfers an existing reference into `Self`).
    #[must_use]
    pub const fn from_owned_raw(raw: *mut c_void) -> Self {
        Self(raw)
    }

    /// Updates the stored pointer to `raw`, returning `true` if the pointer changed.
    pub fn set(&mut self, raw: *mut c_void) -> bool {
        if self.0 == raw {
            return false;
        }
        retain_raw(raw);
        let old = core::mem::replace(&mut self.0, raw);
        release_raw(old);
        true
    }
}

/// Constant buffer binding slot entry (`ConstantBufferEntry`).
#[derive(Default)]
pub struct ConstantBufferEntry {
    pub buffer: ComPtr,
    pub offset: u32,
    pub length: u32,
}

impl Clone for ConstantBufferEntry {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            offset: self.offset,
            length: self.length,
        }
    }
}

/// Sampler state binding slot entry (`SamplerEntry`).
#[derive(Default)]
pub struct SamplerEntry {
    pub sampler: ComPtr,
}

impl Clone for SamplerEntry {
    fn clone(&self) -> Self {
        Self {
            sampler: self.sampler.clone(),
        }
    }
}

/// Shader resource view binding slot entry (`ShaderResourceViewEntry`).
#[derive(Default)]
pub struct ShaderResourceViewEntry {
    pub srv: ComPtr,
}

impl Clone for ShaderResourceViewEntry {
    fn clone(&self) -> Self {
        Self {
            srv: self.srv.clone(),
        }
    }
}

/// Unordered access view binding slot entry (`UnorderedAccessViewEntry`).
#[derive(Default)]
pub struct UnorderedAccessViewEntry {
    pub uav: ComPtr,
}

impl Clone for UnorderedAccessViewEntry {
    fn clone(&self) -> Self {
        Self {
            uav: self.uav.clone(),
        }
    }
}

/// Per-stage bindings (`D3D11ShaderStageState`).
#[derive(Default)]
pub struct ShaderStageState {
    pub shader: ComPtr,
    pub samplers: BindingSet<SamplerEntry, SAMPLER_SLOTS>,
    pub srvs: BindingSet<ShaderResourceViewEntry, SRV_SLOTS>,
    pub constant_buffers: BindingSet<ConstantBufferEntry, CONSTANT_BUFFER_SLOTS>,
}

/// Vertex buffer binding slot entry (`VertexBufferEntry`).
#[derive(Default)]
pub struct VertexBufferEntry {
    pub buffer: ComPtr,
    pub stride: u32,
    pub offset: u32,
}

impl Clone for VertexBufferEntry {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            stride: self.stride,
            offset: self.offset,
        }
    }
}

/// Input assembler stage state (`D3D11InputAssemblerState`).
#[derive(Default)]
pub struct InputAssemblerState {
    pub input_layout: ComPtr,
    pub vertex_buffers: BindingSet<VertexBufferEntry, VERTEX_BUFFER_SLOTS>,
    pub index_buffer: ComPtr,
    pub index_buffer_format: u32,
    pub index_buffer_offset: u32,
    pub topology: u32,
}

/// Output merger stage state (`D3D11OutputMergerState`).
pub struct OutputMergerState {
    pub rtvs: [ComPtr; RENDER_TARGET_SLOTS],
    pub dsv: ComPtr,
    pub uavs: BindingSet<UnorderedAccessViewEntry, UAV_SLOTS>,
    pub num_rtvs: u32,
    pub sample_count: u32,
    pub array_length: u32,
    pub render_target_width: u32,
    pub render_target_height: u32,
    pub blend_state: ComPtr,
    pub blend_factor: [f32; 4],
    pub sample_mask: u32,
    pub depth_stencil_state: ComPtr,
    pub stencil_ref: u32,
}

impl Default for OutputMergerState {
    fn default() -> Self {
        Self {
            rtvs: [const { ComPtr::null() }; RENDER_TARGET_SLOTS],
            dsv: ComPtr::null(),
            uavs: BindingSet::new(),
            num_rtvs: 0,
            sample_count: 1,
            array_length: 0,
            render_target_width: 0,
            render_target_height: 0,
            blend_state: ComPtr::null(),
            blend_factor: [0.0; 4],
            sample_mask: DEFAULT_SAMPLE_MASK,
            depth_stencil_state: ComPtr::null(),
            stencil_ref: 0,
        }
    }
}

/// Stream output target slot entry (`StreamOutputBufferEntry`).
#[derive(Default)]
pub struct StreamOutputBufferEntry {
    pub buffer: ComPtr,
    pub offset: u32,
}

impl Clone for StreamOutputBufferEntry {
    fn clone(&self) -> Self {
        Self {
            buffer: self.buffer.clone(),
            offset: self.offset,
        }
    }
}

/// Stream output stage state (`D3D11StreamOutputState`).
#[derive(Default)]
pub struct StreamOutputState {
    pub targets: BindingSet<StreamOutputBufferEntry, STREAM_OUTPUT_SLOTS>,
}

/// Rasterizer stage state (`D3D11RasterizerStageState`).
#[derive(Default)]
pub struct RasterizerStageState {
    pub viewports: [D3d11Viewport; VIEWPORT_AND_SCISSOR_SLOTS],
    pub num_viewports: u32,
    pub scissor_rects: [Rect; VIEWPORT_AND_SCISSOR_SLOTS],
    pub num_scissor_rects: u32,
    pub rasterizer_state: ComPtr,
}

/// Compute stage UAV bindings (`D3D11ComputeStageUAVState`).
#[derive(Default)]
pub struct ComputeStageUAVState {
    pub uavs: BindingSet<UnorderedAccessViewEntry, UAV_SLOTS>,
}

bitflags::bitflags! {
    /// Dirty state flags for the context (`DirtyState`).
    #[derive(Clone, Copy, Default, PartialEq, Eq)]
    pub struct DirtyFlags: u32 {
        const BLEND_FACTOR_AND_SAMPLE_MASK = 1 << 0;
        const DEPTH_STENCIL_STATE = 1 << 1;
        const RASTERIZER_STATE = 1 << 2;
        const VIEWPORTS = 1 << 3;
        const SCISSOR_RECTS = 1 << 4;
        const INDEX_BUFFER = 1 << 5;
        const VERTEX_BUFFERS = 1 << 6;
    }
}

/// Bound pipeline state for a D3D11 context (`D3D11ContextState`).
pub struct ContextState {
    pub shader_stages: [ShaderStageState; NUM_SHADER_STAGES],
    pub input_assembler: InputAssemblerState,
    pub output_merger: OutputMergerState,
    pub stream_output: StreamOutputState,
    pub rasterizer: RasterizerStageState,
    pub compute_stage_uav: ComputeStageUAVState,
    pub predicate: ComPtr,
    pub predicate_value: bool,
    pub active_state_object: ComPtr,
    pub sample_mask: u32,
    pub stencil_ref: u32,
    pub blend_factor: [f32; 4],
    pub topology: u32,
}

impl Default for ContextState {
    fn default() -> Self {
        Self {
            shader_stages: core::array::from_fn(|_| ShaderStageState::default()),
            input_assembler: InputAssemblerState::default(),
            output_merger: OutputMergerState::default(),
            stream_output: StreamOutputState::default(),
            rasterizer: RasterizerStageState::default(),
            compute_stage_uav: ComputeStageUAVState::default(),
            predicate: ComPtr::null(),
            predicate_value: false,
            active_state_object: ComPtr::null(),
            sample_mask: DEFAULT_SAMPLE_MASK,
            stencil_ref: 0,
            blend_factor: [0.0; 4],
            topology: 0,
        }
    }
}

impl<M: Mode> Context<M> {
    /// Resets encoder state and marks all bindings dirty (`ResetEncodingContextState`).
    pub fn reset_encoding_context_state(&mut self) {
        match self.cmdbuf_state {
            CommandBufferState::RenderEncoderActive | CommandBufferState::ComputeEncoderActive => {
                self.invalidate_current_pass(true);
            }
            CommandBufferState::Idle | CommandBufferState::BlitEncoderActive => {}
            CommandBufferState::RenderPipelineReady | CommandBufferState::ComputePipelineReady => {
                self.cmdbuf_state = CommandBufferState::Idle;
            }
        }
        M::emit_st(self, |enc| {
            enc.clear_state();
        });
        self.update_encode_snapshot(|snap| {
            snap.render_pipeline_valid = false;
            snap.compute_pipeline_valid = false;
        });
        self.dirty = DirtyFlags::all();
        for stage in &mut self.state.shader_stages {
            stage.constant_buffers.set_all_dirty();
            stage.samplers.set_all_dirty();
            stage.srvs.set_all_dirty();
        }
        self.state.compute_stage_uav.uavs.set_all_dirty();
        self.state.output_merger.uavs.set_all_dirty();
        self.state.input_assembler.vertex_buffers.set_all_dirty();
        self.state.stream_output.targets.set_all_dirty();
    }

    /// `ID3D11DeviceContext::ClearState`.
    pub fn clear_state(&mut self) {
        self.reset_encoding_context_state();
        let active_state_object = ComPtr::from_owned_raw(self.state.active_state_object.take_raw());
        self.state = ContextState {
            active_state_object,
            ..ContextState::default()
        };
    }
}

#[cfg(test)]
mod tests;
