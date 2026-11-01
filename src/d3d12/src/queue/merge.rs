//! Render-pass merge planning and clear folding (`d3d12_command_queue.cpp:207-574`).

use core::{ffi::c_void, ptr};

use winemetal_sys::wire::{
    WMT_LOAD_ACTION_CLEAR, WMT_LOAD_ACTION_LOAD, WMT_STORE_ACTION_STORE, WMTRenderPassInfo,
    WMTRenderPassSampleBufferInfo,
};

use super::timestamp::DONT_SAMPLE;
use crate::{
    encoder::{
        BarrierOpacity, CLEAR_DEPTH_STENCIL, ClearEncoderData, EncoderData, EncoderType,
        RenderEncoderData, Submission, clear_slot,
    },
    list::List,
};

/// Provides read access to a command list's recorded [`Submission`].
pub trait AsSubmission {
    /// Borrows the underlying [`Submission`].
    fn as_submission(&self) -> &Submission;
}

impl AsSubmission for Submission {
    fn as_submission(&self) -> &Submission {
        self
    }
}

impl AsSubmission for List {
    fn as_submission(&self) -> &Submission {
        self.submission()
    }
}

impl<T: AsSubmission + ?Sized> AsSubmission for &T {
    fn as_submission(&self) -> &Submission {
        (**self).as_submission()
    }
}

const CROSS_OVERLAP: i32 = 7;
const CROSS_WRITE: i32 = CROSS_OVERLAP + 1;
const _: () = assert!(BarrierOpacity::KINDS == CROSS_OVERLAP as usize);

#[derive(PartialEq, Eq)]
enum MergeRefusal {
    Timestamps,
    Resolver,
    Barrier(i32),
    Attachments,
    Other,
}

/// Barrier range `(after, until]` in `lists[list_idx]` between merged render passes (`Crossing`,
///
/// `d3d12_command_queue.cpp:234-238`).
pub struct Crossing {
    pub list_idx: usize,
    pub after: u32,
    pub until: u32,
}

/// Clear-only encoder folded into a later render pass's load action (`ClearFold`,
///
/// `d3d12_command_queue.cpp:470-475`).
pub struct ClearFold {
    pub list: usize,
    pub target: *mut EncoderData,
    pub clear: *const ClearEncoderData,
}

/// Reusable scratch state for render-pass merge planning and clear folding
///
/// (`d3d12_command_queue.cpp:227-574`).
pub struct MergePlan {
    pub merge: bool,
    pub deferred_resolves: bool,
    pub merged: Vec<*mut EncoderData>,
    pub folds: Vec<*mut EncoderData>,
    pub deferred: Vec<*mut EncoderData>,
    pub crossings: Vec<Crossing>,
    pub merged_list: usize,
    pub skipped: Vec<(*mut EncoderData, u16)>,
    pub skip: usize,
    pub clear_folds: Vec<ClearFold>,
}

impl MergePlan {
    /// Creates a [`MergePlan`] with pre-allocated scratch buffers.
    #[must_use]
    pub fn new(merge: bool, deferred_resolves: bool) -> Self {
        Self {
            merge,
            deferred_resolves,
            merged: Vec::with_capacity(64),
            folds: Vec::with_capacity(64),
            deferred: Vec::with_capacity(64),
            crossings: Vec::with_capacity(64),
            merged_list: 0,
            skipped: Vec::with_capacity(64),
            skip: 0,
            clear_folds: Vec::with_capacity(64),
        }
    }

    /// Checks whether the barriers recorded in `self.crossings` can be crossed when merging `base`
    ///
    /// and `next` (`Crossable`, `d3d12_command_queue.cpp:253-299`).
    ///
    /// Returns `-1` when allowed, or a non-negative refusal reason index otherwise.
    #[must_use]
    pub fn crossable<S: AsSubmission>(
        &self,
        base: &RenderEncoderData,
        next: &RenderEncoderData,
        lists: &[S],
        serial: bool,
    ) -> i32 {
        if self.crossings.is_empty() {
            return -1;
        }
        if !serial {
            return CROSS_OVERLAP;
        }
        let uav_writes = base.uav_writes
            || next.uav_writes
            || self.merged.iter().any(|&p| {
                // SAFETY: `self.merged` holds valid encoder pointers from `lists`.
                let enc = unsafe { &*p };
                if enc.kind == EncoderType::Render {
                    // SAFETY: `enc.kind == EncoderType::Render`.
                    unsafe { (*p.cast::<RenderEncoderData>()).uav_writes }
                } else {
                    false
                }
            });
        let opaque_writes = base.base.writes_unknown
            || next.base.writes_unknown
            || self.merged.iter().any(|&p| {
                // SAFETY: `self.merged` holds valid encoder pointers from `lists`.
                unsafe { (*p).writes_unknown }
            })
            || self.folds.iter().any(|&p| {
                // SAFETY: `self.folds` holds valid encoder pointers from `lists`.
                unsafe { (*p).writes_unknown }
            });

        for c in &self.crossings {
            let sub = lists[c.list_idx].as_submission();
            for &(call, why) in &sub.barrier_opaque {
                if call > c.after
                    && call <= c.until
                    && (why >= BarrierOpacity::ALWAYS || uav_writes)
                {
                    return why as i32;
                }
            }
            for &(call, key) in &sub.barrier_names {
                if call > c.after && call <= c.until && !key.is_null() {
                    if opaque_writes || self.deferred_write(key) {
                        return CROSS_WRITE;
                    }
                    if self.writes_key(&base.base, &next.base, key) {
                        crate::count!(
                            "#render pass merges across a barrier naming their target",
                            1
                        );
                    }
                }
            }
        }
        -1
    }

    fn writes_key(&self, base: &EncoderData, next: &EncoderData, key: *const c_void) -> bool {
        encoder_writes(base, key)
            || encoder_writes(next, key)
            || self.merged.iter().any(|&p| {
                // SAFETY: `self.merged` holds valid encoder pointers.
                encoder_writes(unsafe { &*p }, key)
            })
            || self.folds.iter().any(|&p| {
                // SAFETY: `self.folds` holds valid encoder pointers.
                encoder_writes(unsafe { &*p }, key)
            })
    }

    fn deferred_write(&self, key: *const c_void) -> bool {
        let is_deferred_write = |p: *mut EncoderData| {
            // SAFETY: `p` is a valid encoder pointer in `self.merged` or `self.folds`.
            let enc = unsafe { &*p };
            enc.kind == EncoderType::Blit && enc.query_resolve_only && encoder_writes(enc, key)
        };
        self.merged.iter().copied().any(is_deferred_write)
            || self.folds.iter().copied().any(is_deferred_write)
    }

    /// Computes render-pass merges and timestamp folds into `base` (`PlanMerge`,
    ///
    /// `d3d12_command_queue.cpp:344-445`).
    ///
    /// # Safety
    ///
    /// All encoder pointers reachable from `base` and `lists` must be valid for the call.
    pub unsafe fn plan_merge<S: AsSubmission>(
        &mut self,
        base: *mut RenderEncoderData,
        list: usize,
        lists: &[S],
        info: &mut WMTRenderPassInfo,
        serial: bool,
        sampling: bool,
    ) {
        self.merged.clear();
        self.folds.clear();
        self.deferred.clear();
        // SAFETY: `base` is a valid `RenderEncoderData` pointer for the call.
        let base_ref = unsafe { &*base };
        if (!base_ref.base.join && !serial) || base_ref.visibility_buffer != 0 {
            let candidate = if !base_ref.base.next.is_null() {
                base_ref.base.next
            } else if list + 1 < lists.len() {
                lists[list + 1].as_submission().entry
            } else {
                ptr::null_mut()
            };
            // SAFETY: `candidate` is null or a valid encoder pointer in `lists`.
            unsafe { refuse_merge(candidate, &MergeRefusal::Other) };
            return;
        }
        let mut samples = usize::from(base_ref.base.num_samples);
        let mut prev_enc: *mut EncoderData = base.cast::<EncoderData>();
        let mut cursor = base_ref.base.next;
        let mut l = list;
        let mut last_list = list;
        self.crossings.clear();

        loop {
            let allowed_barriers = if l == last_list {
                // SAFETY: `prev_enc` is a valid encoder pointer in `lists[last_list]`.
                unsafe { (*prev_enc).barriers_last }
            } else {
                0
            };
            if cursor.is_null() {
                let done = lists[l].as_submission();
                if done.barrier_count != allowed_barriers {
                    self.crossings.push(Crossing {
                        list_idx: l,
                        after: allowed_barriers,
                        until: done.barrier_count,
                    });
                }
                l += 1;
                if l == lists.len() {
                    break;
                }
                let next_sub = lists[l].as_submission();
                cursor = next_sub.entry;
                if lists[list..l]
                    .iter()
                    .any(|prev| ptr::eq(prev.as_submission(), next_sub))
                {
                    // SAFETY: `cursor` is null or a valid encoder pointer in `lists[l]`.
                    unsafe { refuse_merge(cursor, &MergeRefusal::Other) };
                    break;
                }
                continue;
            }
            // SAFETY: `cursor` is non-null and points to a valid `EncoderData` in `lists[l]`.
            let cur_ref = unsafe { &*cursor };
            if cur_ref.kind == EncoderType::Null {
                cursor = cur_ref.next;
                continue;
            }
            if cur_ref.barriers_last != allowed_barriers {
                self.crossings.push(Crossing {
                    list_idx: l,
                    after: allowed_barriers,
                    until: cur_ref.barriers_last,
                });
            }
            if cur_ref.kind == EncoderType::Blit && cur_ref.timestamp_only {
                self.folds.push(cursor);
                cursor = cur_ref.next;
                continue;
            }
            if self.deferred_resolves
                && cur_ref.kind == EncoderType::Blit
                && cur_ref.query_resolve_only
                && cur_ref.num_samples == 0
                && !cur_ref.writes_unknown
            {
                self.folds.push(cursor);
                cursor = cur_ref.next;
                continue;
            }
            if cur_ref.kind != EncoderType::Render {
                break;
            }
            let next_ptr = cursor.cast::<RenderEncoderData>();
            // SAFETY: `cur_ref.kind == EncoderType::Render` guarantees `cursor` is a
            // `RenderEncoderData`.
            let next_ref = unsafe { &*next_ptr };
            let mut n = samples;
            let mut ok = next_ref.pre_tail.is_null() && same_target(base_ref, next_ref);
            for i in 0..self.folds.len() {
                // SAFETY: `self.folds[i]` is a valid `EncoderData` pointer.
                let fold_ref = unsafe { &*self.folds[i] };
                ok = ok && fits_samples(fold_ref, &mut n, info, sampling);
            }
            ok = ok && fits_samples(&next_ref.base, &mut n, info, sampling);
            let why = if ok {
                self.crossable(base_ref, next_ref, lists, serial)
            } else {
                -1
            };
            if !ok || why >= 0 {
                for j in samples..n {
                    info.sample_buffers[j] = WMTRenderPassSampleBufferInfo {
                        sample_buffer: 0,
                        end_of_fragment_sample_index: 0,
                        start_of_vertex_sample_index: 0,
                    };
                }
                if why >= 0 {
                    // SAFETY: `cursor` is a valid encoder pointer.
                    unsafe { refuse_merge(cursor, &MergeRefusal::Barrier(why)) };
                } else {
                    let refusal = if !next_ref.pre_tail.is_null() {
                        MergeRefusal::Resolver
                    } else if !same_target(base_ref, next_ref) {
                        MergeRefusal::Attachments
                    } else {
                        MergeRefusal::Timestamps
                    };
                    // SAFETY: `cursor` is a valid encoder pointer.
                    unsafe { refuse_merge(cursor, &refusal) };
                }
                break;
            }
            samples = n;
            if !self.crossings.is_empty() {
                crate::count!("#render pass merges across barriers", 1);
            }
            self.merged.extend_from_slice(&self.folds);
            for &f in &self.folds {
                // SAFETY: `f` is a valid encoder pointer in `self.folds`.
                if unsafe { (*f).query_resolve_only } {
                    self.deferred.push(f);
                }
            }
            self.merged.push(cursor);
            self.folds.clear();
            prev_enc = cursor;
            last_list = l;
            self.merged_list = l;
            cursor = next_ref.base.next;
        }
    }
}

fn encoder_writes(enc: &EncoderData, key: *const c_void) -> bool {
    let n = usize::from(enc.write_count);
    enc.writes[..n].contains(&key)
}

fn fits_samples(
    e: &EncoderData,
    n: &mut usize,
    info: &mut WMTRenderPassInfo,
    sampling: bool,
) -> bool {
    if sampling {
        return true;
    }
    let count = usize::from(e.num_samples);
    for k in 0..count {
        if *n == info.sample_buffers.len() {
            return false;
        }
        let buf = e.samples[k].buffer;
        for j in 0..*n {
            if info.sample_buffers[j].sample_buffer == buf {
                return false;
            }
        }
        info.sample_buffers[*n] = WMTRenderPassSampleBufferInfo {
            sample_buffer: buf,
            end_of_fragment_sample_index: u64::from(e.samples[k].index),
            start_of_vertex_sample_index: DONT_SAMPLE,
        };
        *n += 1;
    }
    true
}

unsafe fn refuse_merge(mut e: *mut EncoderData, why: &MergeRefusal) {
    if !crate::stats::on() {
        return;
    }
    while !e.is_null() {
        // SAFETY: `e` is a valid encoder pointer in the submission list.
        let enc = unsafe { &*e };
        if enc.kind == EncoderType::Null || (enc.kind == EncoderType::Blit && enc.timestamp_only) {
            e = enc.next;
        } else {
            break;
        }
    }
    if !e.is_null() {
        // SAFETY: `e` is non-null and valid.
        let enc = unsafe { &*e };
        if enc.kind == EncoderType::Render {
            match *why {
                MergeRefusal::Timestamps => {
                    crate::count!("#render pass merges refused (timestamps)", 1);
                }
                MergeRefusal::Resolver => {
                    crate::count!("#render pass merges refused (resolver)", 1);
                }
                MergeRefusal::Barrier(crossing) => {
                    crate::count!("#render pass merges refused (barrier)", 1);
                    match crossing {
                        0 => {
                            crate::count!(
                                "#render pass merges refused (barrier): UNORDERED_ACCESS",
                                1
                            );
                        }
                        1 => {
                            crate::count!(
                                "#render pass merges refused (barrier): a UAV barrier",
                                1
                            );
                        }
                        2 => {
                            crate::count!(
                                "#render pass merges refused (barrier): an aliasing barrier",
                                1
                            );
                        }
                        3 => {
                            crate::count!(
                                "#render pass merges refused (barrier): a split barrier's end",
                                1
                            );
                        }
                        4 => {
                            crate::count!(
                                "#render pass merges refused (barrier): an unknown resource",
                                1
                            );
                        }
                        5 => {
                            crate::count!(
                                "#render pass merges refused (barrier): simultaneous access",
                                1
                            );
                        }
                        6 => {
                            crate::count!("#render pass merges refused (barrier): COMMON", 1);
                        }
                        7 => {
                            crate::count!(
                                "#render pass merges refused (barrier): overlap order",
                                1
                            );
                        }
                        8 => {
                            crate::count!(
                                "#render pass merges refused (barrier): a resource the passes write",
                                1
                            );
                        }
                        _ => {}
                    }
                }
                MergeRefusal::Attachments => {
                    crate::count!("#render pass merges refused (attachments)", 1);
                }
                MergeRefusal::Other => {
                    crate::count!("#render pass merges refused (other)", 1);
                }
            }
        }
    }
}

/// Returns `true` when `a` and `b` can be encoded as one Metal render pass (`SameTarget`,
///
/// `d3d12_command_queue.cpp:209-225`).
#[must_use]
pub fn same_target(a: &RenderEncoderData, b: &RenderEncoderData) -> bool {
    for i in 0..a.colors.len() {
        let x = &a.colors[i];
        let y = &b.colors[i];
        let ok = x.attachment == y.attachment
            && x.level == y.level
            && x.slice == y.slice
            && x.depth_plane == y.depth_plane
            && (y.attachment.is_none() || y.load_action == WMT_LOAD_ACTION_LOAD)
            && (x.attachment.is_none() || x.store_action == WMT_STORE_ACTION_STORE);
        if !ok {
            return false;
        }
    }
    let depth_ok = a.depth.attachment == b.depth.attachment
        && a.depth.level == b.depth.level
        && a.depth.slice == b.depth.slice
        && a.depth.depth_plane == b.depth.depth_plane
        && (b.depth.attachment.is_none() || b.depth.load_action == WMT_LOAD_ACTION_LOAD)
        && (a.depth.attachment.is_none() || a.depth.store_action == WMT_STORE_ACTION_STORE);
    let stencil_ok = a.stencil.attachment == b.stencil.attachment
        && a.stencil.level == b.stencil.level
        && a.stencil.slice == b.stencil.slice
        && a.stencil.depth_plane == b.stencil.depth_plane
        && (b.stencil.attachment.is_none() || b.stencil.load_action == WMT_LOAD_ACTION_LOAD)
        && (a.stencil.attachment.is_none() || a.stencil.store_action == WMT_STORE_ACTION_STORE);

    depth_ok
        && stencil_ok
        && a.render_target_width == b.render_target_width
        && a.render_target_height == b.render_target_height
        && a.render_target_array_length == b.render_target_array_length
        && a.default_raster_sample_count == b.default_raster_sample_count
        && a.dsv_planar_flags == b.dsv_planar_flags
        && a.dsv_readonly_flags == b.dsv_readonly_flags
        && a.visibility_buffer == b.visibility_buffer
        && a.use_geometry == b.use_geometry
}

fn barrier_names(sub: &Submission, after: u32, until: u32, texture: *const c_void) -> bool {
    sub.barrier_names
        .iter()
        .any(|&(call, key)| call > after && call <= until && (key.is_null() || key == texture))
}

fn barrier_unknown(sub: &Submission, after: u32, until: u32) -> bool {
    sub.barrier_names
        .iter()
        .any(|&(call, key)| call > after && call <= until && key.is_null())
}

/// Finds the later render pass in `lists` that `clear` can fold into as a load action
///
/// (`ClearTarget`, `d3d12_command_queue.cpp:504-554`).
///
/// # Safety
///
/// All encoder pointers reachable from `clear` and `lists` must be valid for the call.
#[must_use]
pub unsafe fn clear_target<S: AsSubmission>(
    clear: *const ClearEncoderData,
    mut l: usize,
    lists: &[S],
) -> Option<(usize, *mut RenderEncoderData)> {
    // SAFETY: `clear` is a valid `ClearEncoderData` pointer for the call.
    let clear_ref = unsafe { &*clear };
    let texture = if clear_ref.base.write_count > 0 {
        clear_ref.base.writes[0]
    } else {
        ptr::null()
    };
    if texture.is_null() {
        return None;
    }
    let mut cursor = clear_ref.base.next;
    let mut after = clear_ref.base.barriers;
    let mut named = false;

    loop {
        let sub = lists[l].as_submission();
        if cursor.is_null() {
            if barrier_unknown(sub, after, u32::MAX) {
                crate::count!("#clear folds refused (barrier)", 1);
                return None;
            }
            if barrier_names(sub, after, u32::MAX, texture) {
                named = true;
            }
            l += 1;
            if l == lists.len() {
                crate::count!("#clear folds refused (no target)", 1);
                return None;
            }
            cursor = lists[l].as_submission().entry;
            after = 0;
            continue;
        }
        // SAFETY: `cursor` is non-null and points to a valid `EncoderData` in `lists[l]`.
        let cur_ref = unsafe { &*cursor };
        if cur_ref.kind == EncoderType::Null
            || (cur_ref.kind == EncoderType::Blit && cur_ref.timestamp_only)
            || (cur_ref.kind == EncoderType::Clear
                && cur_ref.write_count > 0
                && cur_ref.writes[0] != texture)
        {
            cursor = cur_ref.next;
            continue;
        }
        if barrier_unknown(sub, after, cur_ref.barriers) {
            crate::count!("#clear folds refused (barrier)", 1);
            return None;
        }
        if barrier_names(sub, after, cur_ref.barriers, texture) {
            named = true;
        }
        if cur_ref.kind != EncoderType::Render {
            crate::count!("#clear folds refused (no target)", 1);
            return None;
        }
        let r_ptr = cursor.cast::<RenderEncoderData>();
        // SAFETY: `cur_ref.kind == EncoderType::Render`.
        let r_ref = unsafe { &*r_ptr };
        if clear_slot(clear_ref, r_ref) < 0 {
            crate::count!("#clear folds refused (mismatch)", 1);
            return None;
        }
        if named {
            crate::count!("#clear folds across a barrier naming its texture", 1);
        }
        return Some((l, r_ptr));
    }
}

/// Applies a folded clear `c` into the render pass `r`'s [`WMTRenderPassInfo`] (`ApplyClear`,
///
/// `d3d12_command_queue.cpp:559-574`).
pub fn apply_clear(c: &ClearEncoderData, r: &RenderEncoderData, info: &mut WMTRenderPassInfo) {
    let slot = clear_slot(c, r);
    if slot == CLEAR_DEPTH_STENCIL {
        if (c.clear_dsv & 1) != 0 && r.depth.load_action != WMT_LOAD_ACTION_CLEAR {
            info.depth.load_action = WMT_LOAD_ACTION_CLEAR;
            info.depth.clear_depth = c.depth_stencil.0;
        }
        if (c.clear_dsv & 2) != 0 && r.stencil.load_action != WMT_LOAD_ACTION_CLEAR {
            info.stencil.load_action = WMT_LOAD_ACTION_CLEAR;
            info.stencil.clear_stencil = c.depth_stencil.1;
        }
    } else if let Ok(idx) = usize::try_from(slot)
        && idx < r.colors.len()
        && r.colors[idx].load_action != WMT_LOAD_ACTION_CLEAR
    {
        info.colors[idx].load_action = WMT_LOAD_ACTION_CLEAR;
        info.colors[idx].clear_color = c.color;
    }
}
