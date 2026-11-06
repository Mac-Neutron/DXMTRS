//! Fence ring and encoder ordering (`d3d12_command_queue.cpp:115-205, 448-466, 622-729`).

use core::ptr;

use dxmt_core::presenter::framegen;
use winemetal_sys::{
    MTLBlitCommandEncoder_encodeCommands, MTLComputeCommandEncoder_encodeCommands,
    MTLDevice_newBuffer, MTLRenderCommandEncoder_encodeCommands, Obj,
    wire::{
        MemPtr, WMT_BLIT_COMMAND_FILL_BUFFER, WMT_BLIT_COMMAND_UPDATE_FENCE,
        WMT_BLIT_COMMAND_WAIT_FOR_FENCE, WMT_COMPUTE_COMMAND_UPDATE_FENCE,
        WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE, WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE,
        WMT_RENDER_COMMAND_DRAW, WMT_RENDER_COMMAND_DRAW_INDEXED,
        WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT, WMT_RENDER_COMMAND_DRAW_INDIRECT,
        WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS,
        WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT, WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW,
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED,
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT,
        WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT,
        WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT,
        WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER, WMT_RENDER_COMMAND_UPDATE_FENCE,
        WMT_RENDER_COMMAND_WAIT_FOR_FENCE, WMT_RENDER_STAGE_FRAGMENT,
        WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED, WMT_RESOURCE_STORAGE_MODE_PRIVATE,
        WMTBufferInfo, WmtcmdBase, WmtcmdBlitFenceOp, WmtcmdBlitFillbuffer, WmtcmdComputeFenceOp,
        WmtcmdRenderFenceOp,
    },
};

use crate::encoder::{EncoderData, EncoderType, RenderEncoderData};

/// Number of `MTLFence` objects in the queue's fence ring (`kFences = 256`).
pub const FENCE_COUNT: usize = 256;
const FENCE_COUNT_U16: u16 = 256;
const _: () = assert!(FENCE_COUNT == FENCE_COUNT_U16 as usize);

/// Sentinel for an absent fence index (`kNoFence = 0xffff`).
pub const NO_FENCE: u16 = 0xffff;

/// Reads the strict-serial flag from `DXMT_D3D12_OVERLAP` and `DXMT_D3D12_SERIAL`
///
/// (`d3d12_command_queue.cpp:132`).
#[must_use]
pub fn serial_from_env(overlap: Option<&str>, serial: Option<&str>) -> bool {
    overlap != Some("1") || serial == Some("1")
}

/// Returns `true` when `data`'s command chain contains a draw, mesh dispatch, tile dispatch, or ICB
///
/// (`Draws`, `d3d12_command_queue.cpp:181-205`).
#[must_use]
pub const fn draws(data: &RenderEncoderData) -> bool {
    let mut cmd = data.cmd_head.next.ptr as *const WmtcmdBase;
    while !cmd.is_null() {
        // SAFETY: `cmd` points to a valid `WmtcmdBase` node in the render encoder's command chain.
        let base = unsafe { *cmd };
        match base.r#type {
            WMT_RENDER_COMMAND_DRAW
            | WMT_RENDER_COMMAND_DRAW_INDEXED
            | WMT_RENDER_COMMAND_DRAW_INDIRECT
            | WMT_RENDER_COMMAND_DRAW_INDEXED_INDIRECT
            | WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS
            | WMT_RENDER_COMMAND_DRAW_MESH_THREADGROUPS_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_GEOMETRY_DRAW_INDEXED_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDIRECT
            | WMT_RENDER_COMMAND_DXMT_TESSELLATION_MESH_DRAW_INDEXED_INDIRECT
            | WMT_RENDER_COMMAND_DISPATCH_THREADS_PER_TILE
            | WMT_RENDER_COMMAND_EXECUTE_COMMANDS_IN_BUFFER => return true,
            _ => {}
        }
        cmd = base.next.ptr as *const WmtcmdBase;
    }
    false
}

/// Per-queue fence ring and encoder ordering state (`d3d12_command_queue.cpp:119-135`).
pub struct OrderState {
    pub fences: [Option<Obj>; FENCE_COUNT],
    pub fence_handles: [u64; FENCE_COUNT],
    pub fence_group: [u64; FENCE_COUNT],
    pub group: u64,
    pub next_fence: u16,
    pub frontier: Vec<u16>,
    pub waits: Vec<u16>,
    pub group_wait: u16,
    pub early: u16,
    pub list_fences: Vec<u16>,
    pub list_join: u32,
    pub serial: bool,
    pub sample_scratch: Option<Obj>,
    pub render_ops: Vec<WmtcmdRenderFenceOp>,
    pub blit_ops: Vec<WmtcmdBlitFenceOp>,
    pub compute_ops: Vec<WmtcmdComputeFenceOp>,
}

impl OrderState {
    /// Creates a new [`OrderState`] with pre-allocated scratch buffers.
    #[must_use]
    pub fn new(serial: bool) -> Self {
        Self {
            fences: [const { None }; FENCE_COUNT],
            fence_handles: [0; FENCE_COUNT],
            fence_group: [0; FENCE_COUNT],
            group: 2,
            next_fence: 0,
            frontier: Vec::with_capacity(FENCE_COUNT),
            waits: Vec::with_capacity(FENCE_COUNT),
            group_wait: NO_FENCE,
            early: NO_FENCE,
            list_fences: Vec::with_capacity(64),
            list_join: 0,
            serial,
            sample_scratch: None,
            render_ops: Vec::with_capacity(FENCE_COUNT + 2),
            blit_ops: Vec::with_capacity(FENCE_COUNT + 2),
            compute_ops: Vec::with_capacity(FENCE_COUNT + 2),
        }
    }

    /// Assigns the fence `e` updates and populates `self.waits` (`Order`,
    ///
    /// `d3d12_command_queue.cpp:141-177`).
    pub fn order(&mut self, e: Option<&EncoderData>, serial: bool) -> u16 {
        let fence = self.next_fence;
        self.next_fence = (self.next_fence + 1) % FENCE_COUNT_U16;
        let join = e.is_none_or(|enc| enc.join)
            || serial
            || self.fence_group[usize::from(fence)] + 1 >= self.group;
        self.waits.clear();
        self.early = NO_FENCE;
        if join {
            self.waits.extend_from_slice(&self.frontier);
            self.frontier.clear();
            self.group += 1;
            self.list_join = e.map_or(u32::MAX, |enc| enc.position);
            self.group_wait = fence;
            if !serial
                && let Some(enc) = e
                && enc.kind == EncoderType::Render
            {
                let render_ptr = ptr::from_ref(enc).cast::<RenderEncoderData>();
                // SAFETY: `enc.kind == EncoderType::Render` guarantees `enc` is the base header of
                // a `RenderEncoderData`.
                let render = unsafe { &*render_ptr };
                if draws(render) {
                    crate::count!("#encoder early fences", 1);
                    self.early = self.next_fence;
                    self.group_wait = self.early;
                    self.next_fence = (self.next_fence + 1) % FENCE_COUNT_U16;
                    self.fence_group[usize::from(self.early)] = self.group;
                }
            }
        } else if let Some(enc) = e {
            if self.list_join == u32::MAX {
                self.list_join = enc.position;
            }
            self.waits.push(self.group_wait);
            if !enc.deps.is_null() {
                for i in 0..(enc.dep_count as usize) {
                    // SAFETY: `i < enc.dep_count` is within `enc.deps`.
                    let dep_ptr = unsafe { enc.deps.add(i) };
                    // SAFETY: `dep_ptr` points to a valid `u32` dependency index.
                    let dep = unsafe { *dep_ptr };
                    if dep >= self.list_join {
                        self.waits.push(self.list_fences[dep as usize]);
                    }
                }
            }
        }
        if let Some(enc) = e
            && crate::stats::on()
        {
            self.count_order(enc, join);
        }
        self.fence_group[usize::from(fence)] = self.group;
        self.frontier.push(fence);
        if let Some(enc) = e {
            let pos = enc.position as usize;
            if pos >= self.list_fences.len() {
                self.list_fences.resize(pos + 1, 0);
            }
            self.list_fences[pos] = fence;
        }
        fence
    }

    fn count_order(&self, e: &EncoderData, join: bool) {
        #[cfg(not(feature = "perf-build"))]
        {
            static IDS: std::sync::LazyLock<[crate::stats::StatId; 5]> =
                std::sync::LazyLock::new(|| {
                    [
                        crate::stats::id("#encoder full joins"),
                        crate::stats::id("#encoders with a dependency list"),
                        crate::stats::id("#encoder dependency waits"),
                        crate::stats::id("#encoder fence waits"),
                        crate::stats::id("#encoder boundaries free to overlap"),
                    ]
                });
            let [joins, listed, dep_waits, fence_waits, free] = *IDS;
            crate::stats::count_id(fence_waits, self.waits.len() as u64);
            if join {
                crate::stats::count_id(joins, 1);
                return;
            }
            let deps = self.waits.len().saturating_sub(1);
            if deps > 0 {
                crate::stats::count_id(listed, 1);
                crate::stats::count_id(dep_waits, deps as u64);
            }
            let pos = e.position as usize;
            if pos > 0 && pos - 1 < self.list_fences.len() {
                let prev_fence = self.list_fences[pos - 1];
                if !self.waits.contains(&prev_fence) {
                    crate::stats::count_id(free, 1);
                }
            }
        }
        #[cfg(feature = "perf-build")]
        let _ = (self, e, join, Option::from(()));
    }

    /// Returns a 4-byte fill command for the queue's scratch buffer (`ScratchFill`,
    ///
    /// `d3d12_command_queue.cpp:622-637`).
    pub fn scratch_fill(&mut self, mtl_device: u64) -> WmtcmdBlitFillbuffer {
        if self.sample_scratch.is_none() && mtl_device != 0 {
            let mut info = WMTBufferInfo {
                length: 16,
                options: WMT_RESOURCE_STORAGE_MODE_PRIVATE
                    | WMT_RESOURCE_HAZARD_TRACKING_MODE_UNTRACKED,
                memory: MemPtr::NULL,
                gpu_address: 0,
            };
            // SAFETY: `mtl_device` is a live `MTLDevice` handle (or faked in tests) and `info` is
            // valid on the stack.
            let raw = unsafe { MTLDevice_newBuffer(mtl_device, &raw mut info) };
            // SAFETY: `MTLDevice_newBuffer` returns a +1 object handle or 0.
            self.sample_scratch = unsafe { Obj::adopt(raw) };
        }
        let buffer = self.sample_scratch.as_ref().map_or(0, Obj::handle);
        WmtcmdBlitFillbuffer {
            r#type: WMT_BLIT_COMMAND_FILL_BUFFER,
            reserved: [0; 3],
            next: MemPtr::NULL,
            buffer,
            offset: 0,
            length: 4,
            value: 0,
        }
    }

    /// Joins the queue's frontier on a blit command encoder (`Join`,
    ///
    /// `d3d12_command_queue.cpp:676-682`).
    ///
    /// # Safety
    ///
    /// `encoder` must be a valid `MTLBlitCommandEncoder` handle.
    pub unsafe fn join_blit(&mut self, encoder: u64) -> u16 {
        let fence = self.order(None, true);
        for i in 0..self.waits.len() {
            let wait = self.waits[i];
            let op = WmtcmdBlitFenceOp {
                r#type: WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
                reserved: [0; 3],
                next: MemPtr::NULL,
                fence: self.fence_handles[usize::from(wait)],
            };
            // SAFETY: `encoder` is a valid `MTLBlitCommandEncoder` and `op` is live on the stack.
            unsafe {
                MTLBlitCommandEncoder_encodeCommands(
                    encoder,
                    ptr::from_ref(&op).cast::<WmtcmdBase>(),
                );
            }
        }
        fence
    }

    /// Joins the queue's frontier on a render command encoder before `before` (`Join`,
    ///
    /// `d3d12_command_queue.cpp:676-682`).
    ///
    /// # Safety
    ///
    /// `encoder` must be a valid `MTLRenderCommandEncoder` handle.
    pub unsafe fn join_render(&mut self, encoder: u64, before: u8) -> u16 {
        let fence = self.order(None, true);
        for i in 0..self.waits.len() {
            let wait = self.waits[i];
            let op = WmtcmdRenderFenceOp {
                r#type: WMT_RENDER_COMMAND_WAIT_FOR_FENCE,
                reserved: [0; 3],
                next: MemPtr::NULL,
                fence: self.fence_handles[usize::from(wait)],
                stages: before,
            };
            // SAFETY: `encoder` is a valid `MTLRenderCommandEncoder` and `op` is live on the stack.
            unsafe {
                MTLRenderCommandEncoder_encodeCommands(
                    encoder,
                    ptr::from_ref(&op).cast::<WmtcmdBase>(),
                );
            }
        }
        fence
    }

    /// Encodes `self.waits`, the render pass commands `[head..tail]`, and the fence update(s)
    ///
    /// (`EncodeOrdered`, `d3d12_command_queue.cpp:690-717`).
    ///
    /// # Safety
    ///
    /// `encoder` must be a valid `MTLRenderCommandEncoder` handle; `head` and `tail` must be null
    /// or valid command chain nodes.
    pub unsafe fn encode_render(
        &mut self,
        encoder: u64,
        fence: u16,
        before: u8,
        head: *mut WmtcmdBase,
        tail: *mut WmtcmdBase,
        early: u16,
    ) {
        let n = self.waits.len();
        let all = n + if early == NO_FENCE { 1 } else { 2 };
        self.render_ops.clear();
        self.render_ops.resize(
            all,
            WmtcmdRenderFenceOp {
                r#type: 0,
                reserved: [0; 3],
                next: MemPtr::NULL,
                fence: 0,
                stages: 0,
            },
        );
        for i in 0..all {
            let fence_idx = match i.cmp(&n) {
                core::cmp::Ordering::Less => self.waits[i],
                core::cmp::Ordering::Equal => fence,
                core::cmp::Ordering::Greater => early,
            };
            let next_ptr = if i + 1 < all && i + 1 != n {
                ptr::from_ref(&self.render_ops[i + 1]).cast::<u8>()
            } else {
                ptr::null()
            };
            self.render_ops[i] = WmtcmdRenderFenceOp {
                r#type: if i < n {
                    WMT_RENDER_COMMAND_WAIT_FOR_FENCE
                } else {
                    WMT_RENDER_COMMAND_UPDATE_FENCE
                },
                reserved: [0; 3],
                next: MemPtr::from_ptr(next_ptr),
                fence: self.fence_handles[usize::from(fence_idx)],
                stages: if i == n {
                    WMT_RENDER_STAGE_FRAGMENT
                } else {
                    before
                },
            };
        }
        let update_ptr = ptr::from_mut(&mut self.render_ops[n]).cast::<WmtcmdBase>();
        let body = if head.is_null() { update_ptr } else { head };
        if n > 0 {
            self.render_ops[n - 1].next = MemPtr::from_ptr(body.cast::<u8>());
        }
        if !tail.is_null() {
            // SAFETY: `tail` is a valid command node in the caller's render command list.
            unsafe { (*tail).next = MemPtr::from_ptr(update_ptr.cast::<u8>()) };
        }
        let start = if n > 0 {
            self.render_ops.as_ptr().cast::<WmtcmdBase>()
        } else {
            body.cast_const()
        };
        // SAFETY: `encoder` is a valid `MTLRenderCommandEncoder` and `start` is the head of a
        // valid command chain.
        unsafe { MTLRenderCommandEncoder_encodeCommands(encoder, start) };
        if !tail.is_null() {
            // SAFETY: `tail` is restored to a null `next` pointer after encoding.
            unsafe { (*tail).next = MemPtr::NULL };
        }
    }

    /// Encodes `self.waits`, the blit pass commands `[head..tail]`, and `fence` update
    ///
    /// (`EncodeOrdered`, `d3d12_command_queue.cpp:690-723`).
    ///
    /// # Safety
    ///
    /// `encoder` must be a valid `MTLBlitCommandEncoder` handle; `head` and `tail` must be null or
    /// valid command chain nodes.
    pub unsafe fn encode_blit(
        &mut self,
        encoder: u64,
        fence: u16,
        head: *mut WmtcmdBase,
        tail: *mut WmtcmdBase,
    ) {
        let n = self.waits.len();
        let all = n + 1;
        self.blit_ops.clear();
        self.blit_ops.resize(
            all,
            WmtcmdBlitFenceOp {
                r#type: 0,
                reserved: [0; 3],
                next: MemPtr::NULL,
                fence: 0,
            },
        );
        for i in 0..all {
            let fence_idx = if i < n { self.waits[i] } else { fence };
            let next_ptr = if i + 1 < all && i + 1 != n {
                ptr::from_ref(&self.blit_ops[i + 1]).cast::<u8>()
            } else {
                ptr::null()
            };
            self.blit_ops[i] = WmtcmdBlitFenceOp {
                r#type: if i < n {
                    WMT_BLIT_COMMAND_WAIT_FOR_FENCE
                } else {
                    WMT_BLIT_COMMAND_UPDATE_FENCE
                },
                reserved: [0; 3],
                next: MemPtr::from_ptr(next_ptr),
                fence: self.fence_handles[usize::from(fence_idx)],
            };
        }
        let update_ptr = ptr::from_mut(&mut self.blit_ops[n]).cast::<WmtcmdBase>();
        let body = if head.is_null() { update_ptr } else { head };
        if n > 0 {
            self.blit_ops[n - 1].next = MemPtr::from_ptr(body.cast::<u8>());
        }
        if !tail.is_null() {
            // SAFETY: `tail` is a valid command node in the caller's blit command list.
            unsafe { (*tail).next = MemPtr::from_ptr(update_ptr.cast::<u8>()) };
        }
        let start = if n > 0 {
            self.blit_ops.as_ptr().cast::<WmtcmdBase>()
        } else {
            body.cast_const()
        };
        // SAFETY: `encoder` is a valid `MTLBlitCommandEncoder` and `start` is the head of a valid
        // command chain.
        unsafe { MTLBlitCommandEncoder_encodeCommands(encoder, start) };
        if !tail.is_null() {
            // SAFETY: `tail` is restored to a null `next` pointer after encoding.
            unsafe { (*tail).next = MemPtr::NULL };
        }
    }

    /// Encodes `self.waits`, the compute pass commands `[head..tail]`, and `fence` update
    ///
    /// (`EncodeOrdered`, `d3d12_command_queue.cpp:690-729`).
    ///
    /// # Safety
    ///
    /// `encoder` must be a valid `MTLComputeCommandEncoder` handle; `head` and `tail` must be null
    /// or valid command chain nodes.
    pub unsafe fn encode_compute(
        &mut self,
        encoder: u64,
        fence: u16,
        head: *mut WmtcmdBase,
        tail: *mut WmtcmdBase,
    ) {
        let n = self.waits.len();
        let all = n + 1;
        self.compute_ops.clear();
        self.compute_ops.resize(
            all,
            WmtcmdComputeFenceOp {
                r#type: 0,
                reserved: [0; 3],
                next: MemPtr::NULL,
                fence: 0,
            },
        );
        for i in 0..all {
            let fence_idx = if i < n { self.waits[i] } else { fence };
            let next_ptr = if i + 1 < all && i + 1 != n {
                ptr::from_ref(&self.compute_ops[i + 1]).cast::<u8>()
            } else {
                ptr::null()
            };
            self.compute_ops[i] = WmtcmdComputeFenceOp {
                r#type: if i < n {
                    WMT_COMPUTE_COMMAND_WAIT_FOR_FENCE
                } else {
                    WMT_COMPUTE_COMMAND_UPDATE_FENCE
                },
                reserved: [0; 3],
                next: MemPtr::from_ptr(next_ptr),
                fence: self.fence_handles[usize::from(fence_idx)],
            };
        }
        let update_ptr = ptr::from_mut(&mut self.compute_ops[n]).cast::<WmtcmdBase>();
        let body = if head.is_null() { update_ptr } else { head };
        if n > 0 {
            self.compute_ops[n - 1].next = MemPtr::from_ptr(body.cast::<u8>());
        }
        if !tail.is_null() {
            // SAFETY: `tail` is a valid command node in the caller's compute command list.
            unsafe { (*tail).next = MemPtr::from_ptr(update_ptr.cast::<u8>()) };
        }
        let start = if n > 0 {
            self.compute_ops.as_ptr().cast::<WmtcmdBase>()
        } else {
            body.cast_const()
        };
        // SAFETY: `encoder` is a valid `MTLComputeCommandEncoder` and `start` is the head of a
        // valid command chain.
        unsafe { MTLComputeCommandEncoder_encodeCommands(encoder, start) };
        if !tail.is_null() {
            // SAFETY: `tail` is restored to a null `next` pointer after encoding.
            unsafe { (*tail).next = MemPtr::NULL };
        }
    }
}

impl super::dump::PixelHistoryFences for OrderState {
    fn join_blit(&mut self, blit_encoder: u64) -> usize {
        // SAFETY: `blit_encoder` is a valid `MTLBlitCommandEncoder` supplied by `dump`.
        usize::from(unsafe { Self::join_blit(self, blit_encoder) })
    }

    fn update_blit(&mut self, blit_encoder: u64, fence_slot: usize) {
        let op = WmtcmdBlitFenceOp {
            r#type: WMT_BLIT_COMMAND_UPDATE_FENCE,
            reserved: [0; 3],
            next: MemPtr::NULL,
            fence: self.fence_handles.get(fence_slot).copied().unwrap_or(0),
        };
        // SAFETY: `blit_encoder` is a valid `MTLBlitCommandEncoder` and `op` is live on the stack.
        unsafe {
            MTLBlitCommandEncoder_encodeCommands(
                blit_encoder,
                ptr::from_ref(&op).cast::<WmtcmdBase>(),
            );
        }
    }

    fn join_render(&mut self, render_encoder: u64, stage: u8) -> usize {
        // SAFETY: `render_encoder` is a valid `MTLRenderCommandEncoder` supplied by `dump`.
        usize::from(unsafe { Self::join_render(self, render_encoder, stage) })
    }

    fn update_render(&mut self, render_encoder: u64, fence_slot: usize, stage: u8) {
        let op = WmtcmdRenderFenceOp {
            r#type: WMT_RENDER_COMMAND_UPDATE_FENCE,
            reserved: [0; 3],
            next: MemPtr::NULL,
            fence: self.fence_handles.get(fence_slot).copied().unwrap_or(0),
            stages: stage,
        };
        // SAFETY: `render_encoder` is a valid `MTLRenderCommandEncoder` and `op` is live on the stack.
        unsafe {
            MTLRenderCommandEncoder_encodeCommands(
                render_encoder,
                ptr::from_ref(&op).cast::<WmtcmdBase>(),
            );
        }
    }
}

/// Adapter implementing [`framegen::Order`] over [`OrderState`]
///
/// (`d3d12_command_queue.cpp:2430-2466`).
pub struct FrameGenOrder<'a> {
    pub state: &'a mut OrderState,
    pub mtl_device: u64,
    pub done: u16,
}

impl<'a> FrameGenOrder<'a> {
    /// Creates a new [`FrameGenOrder`] over `state`.
    #[must_use]
    pub const fn new(state: &'a mut OrderState, mtl_device: u64) -> Self {
        Self {
            state,
            mtl_device,
            done: NO_FENCE,
        }
    }
}

impl framegen::Order for FrameGenOrder<'_> {
    fn wait(&mut self, begin: &Obj) {
        self.done = self.state.order(None, true);
        let fill = self.state.scratch_fill(self.mtl_device);
        for i in 0..self.state.waits.len() {
            let wait = self.state.waits[i];
            let op = WmtcmdBlitFenceOp {
                r#type: WMT_BLIT_COMMAND_WAIT_FOR_FENCE,
                reserved: [0; 3],
                next: MemPtr::NULL,
                fence: self.state.fence_handles[usize::from(wait)],
            };
            // SAFETY: `begin` is a valid `MTLBlitCommandEncoder` and `op` is live on the stack.
            unsafe {
                MTLBlitCommandEncoder_encodeCommands(
                    begin.handle(),
                    ptr::from_ref(&op).cast::<WmtcmdBase>(),
                );
            }
        }
        // SAFETY: `begin` is a valid `MTLBlitCommandEncoder` and `fill` is live on the stack.
        unsafe {
            MTLBlitCommandEncoder_encodeCommands(
                begin.handle(),
                ptr::from_ref(&fill).cast::<WmtcmdBase>(),
            );
        }
    }

    fn update(&mut self, end: &Obj) {
        let op = WmtcmdBlitFenceOp {
            r#type: WMT_BLIT_COMMAND_UPDATE_FENCE,
            reserved: [0; 3],
            next: MemPtr::NULL,
            fence: self.state.fence_handles[usize::from(self.done)],
        };
        // SAFETY: `end` is a valid `MTLBlitCommandEncoder` and `op` is live on the stack.
        unsafe {
            MTLBlitCommandEncoder_encodeCommands(
                end.handle(),
                ptr::from_ref(&op).cast::<WmtcmdBase>(),
            );
        }
    }
}
