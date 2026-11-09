//! The source-keyed shader-library indexes, and the draw-to-draw memo in front of them.
//!
//! A draw names each stage's library by its source record: the canonical
//! fixed-function key or the programmable identity, plus the pixel stage's
//! variant. Consecutive draws almost always name the same records, because a
//! snapshot re-sends a stage's record only when the application changed it,
//! so the memo answers a repeat from the record's address without hashing
//! or comparing the key again.
//!
//! A record's address names one immutable record only while the packet that
//! carries it replays: the packet retains its command storage until the
//! submission has read it, and the next packet may reuse the same bytes for
//! another record. [`StageLibraries::begin_packet`] therefore forgets the
//! memo before each packet, and every method that writes an index forgets it
//! as well, so the memo never answers with handles the index no longer
//! holds. It keeps built handles only: an unknown key or a recorded failure
//! goes to the index on every draw, as it did before the memo.
//!
//! Under `shader.backend=air` a programmable stage has two libraries per key,
//! one per backend ([`PairRule`] picks the pair's), so the programmable keys and
//! the memo carry the backend; fixed-function stages are always MSL.

use std::sync::Arc;

use airconv_ir::dxso::{lower, spec::VsSpec};
use log::info;
use mtld3d_core::{
    build_index::BuildIndex,
    config::ShaderBackend,
    draw_data::{FixedPsSource, FixedVsSource, ProgrammablePsSource, ProgrammableVsSource},
    dxso::{DxsoProgram, FfPsKey, FfVsKey, VariantKey, VsSamplerKinds},
    ids::ProgramId,
};
use mtld3d_shared::MetalHandle;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    draw::{PsSourceView, VsSourceView},
    encoder::StageLibHandles,
};

/// Per-draw shader-library lookup, keyed on the shader-identity struct.
///
/// `FxHash` + exact `Eq`, probed by borrow: no per-draw content hash, no
/// clone. One pair of maps per stage; VS keys exclude `variant` (variants
/// share one `MTLLibrary`) but carry the user clip plane count (a
/// programmable VS compiles one library per count), PS keys fold the variant
/// in. The Xxh3 `disk_key` is computed only on a miss here, to bridge the
/// encoder's `lib_cache` (warm-load) and address the on-disk cache. A key
/// whose build failed is recorded too: the same key yields the same source,
/// so its later draws are dropped on the probe instead of compiling again. A
/// library a worker is building is in the encoder's `pending_libs` until its
/// outcome lands here. The encoder's reset cleanup forgets the failures (it
/// runs when the device replaces its implicit surfaces or moves its window,
/// not on every Reset), shutdown forgets everything. The indexes do not own
/// the libraries: `lib_cache` does.
#[derive(Default)]
pub struct StageLibraries {
    ff_vs: BuildIndex<FfVsKey, StageLibHandles>,
    prog_vs: BuildIndex<ProgrammableVsKey, StageLibHandles>,
    ff_ps: FxHashMap<FfPsKey, BuildIndex<VariantKey, StageLibHandles>>,
    prog_ps: BuildIndex<(ProgramId, VariantKey, ShaderBackend), StageLibHandles>,
    memo: LibraryMemo,
}

/// A programmable VS library's index key: program, provided inputs, clip planes, samplers, backend.
pub type ProgrammableVsKey = (ProgramId, u16, u8, VsSamplerKinds, ShaderBackend);

impl StageLibraries {
    /// Both stages' built handles, or `None` when either stage is unknown or failed.
    ///
    /// A stage whose source record and variant are the ones the previous
    /// call answered for takes the memoised handles; any other goes to its
    /// index and, when built, replaces the memo's slot.
    ///
    /// `vs_snapshot` is the snapshot's own VS record, whose address
    /// identifies the source in the memo. `vs` is that record, or a copy of
    /// it the draw keyed for the vertex LOD table, which lives wherever the
    /// draw put it; see [`vs_memo_identity`]. `backend` is the pair's, and a
    /// memo slot answers only for the backend it was filled for.
    #[inline]
    pub fn lookup_ready(
        &mut self,
        vs: VsSourceView<'_>,
        vs_snapshot: VsSourceView<'_>,
        ps: PsSourceView<'_>,
        variant: VariantKey,
        backend: ShaderBackend,
    ) -> Option<(StageLibHandles, StageLibHandles)> {
        let vs_record = vs_memo_identity(vs, vs_snapshot) | backend_tag(backend);
        let vs_handles = if self.memo.vs_record == vs_record {
            self.debug_assert_memo_vs(vs, backend);
            self.memo.vs
        } else {
            self.memo_vs(vs, backend, vs_record)?
        };
        let ps_record = ps_record(ps) | backend_tag(backend);
        let ps_handles = if self.memo.holds_ps(ps_record, &variant) {
            self.debug_assert_memo_ps(ps, variant, backend);
            self.memo.ps
        } else {
            self.memo_ps(ps, variant, backend, ps_record)?
        };
        Some((vs_handles, ps_handles))
    }

    /// Probe the VS source-key index without starting or waiting for a build.
    ///
    /// The borrowed outcome is one pointer: unknown, recorded failure, or
    /// ready handles.
    #[inline]
    pub fn lookup_vs(
        &self,
        source: VsSourceView<'_>,
        backend: ShaderBackend,
    ) -> Option<&Option<StageLibHandles>> {
        match source {
            VsSourceView::FixedFunction(FixedVsSource { key, .. }) => self.ff_vs.lookup_entry(key),
            VsSourceView::Programmable(value) => self
                .prog_vs
                .lookup_entry(&programmable_vs_key(value, backend)),
        }
    }

    /// Probe the PS source-key and variant index without starting or waiting for a build.
    ///
    /// Preserves the recorded failure inside the borrowed outcome, so the
    /// full resolver distinguishes it from a key it still needs to build.
    #[inline]
    pub fn lookup_ps(
        &self,
        source: PsSourceView<'_>,
        variant: VariantKey,
        backend: ShaderBackend,
    ) -> Option<&Option<StageLibHandles>> {
        match source {
            PsSourceView::FixedFunction(FixedPsSource { key, .. }) => self
                .ff_ps
                .get(key)
                .and_then(|variants| variants.lookup_entry(&variant)),
            PsSourceView::Programmable(ProgrammablePsSource { ps_id, .. }) => {
                self.prog_ps.lookup_entry(&(*ps_id, variant, backend))
            }
        }
    }

    /// Record how building the VS library of a draw's source ended, `None` being a failure.
    pub fn record_vs(
        &mut self,
        source: VsSourceView<'_>,
        backend: ShaderBackend,
        outcome: Option<StageLibHandles>,
    ) {
        match source {
            VsSourceView::FixedFunction(FixedVsSource { key, .. }) => {
                self.record_ff_vs(key.clone(), outcome);
            }
            VsSourceView::Programmable(value) => {
                self.record_programmable_vs(programmable_vs_key(value, backend), outcome);
            }
        }
    }

    /// Record how building the PS library of a draw's source and variant ended.
    pub fn record_ps(
        &mut self,
        source: PsSourceView<'_>,
        variant: VariantKey,
        backend: ShaderBackend,
        outcome: Option<StageLibHandles>,
    ) {
        match source {
            PsSourceView::FixedFunction(FixedPsSource { key, .. }) => {
                self.record_ff_ps(key.clone(), variant, outcome);
            }
            PsSourceView::Programmable(ProgrammablePsSource { ps_id, .. }) => {
                self.record_programmable_ps(*ps_id, variant, backend, outcome);
            }
        }
    }

    /// Record a fixed-function VS build outcome under its owned key.
    pub fn record_ff_vs(&mut self, key: FfVsKey, outcome: Option<StageLibHandles>) {
        self.memo.forget();
        self.ff_vs.record(key, outcome);
    }

    /// Record a programmable VS build outcome under its [`ProgrammableVsKey`].
    pub fn record_programmable_vs(
        &mut self,
        key: ProgrammableVsKey,
        outcome: Option<StageLibHandles>,
    ) {
        self.memo.forget();
        self.prog_vs.record(key, outcome);
    }

    /// Record a fixed-function PS build outcome under its owned key and variant.
    pub fn record_ff_ps(
        &mut self,
        key: FfPsKey,
        variant: VariantKey,
        outcome: Option<StageLibHandles>,
    ) {
        self.memo.forget();
        self.ff_ps.entry(key).or_default().record(variant, outcome);
    }

    /// Record a programmable PS build outcome under its program, variant and backend.
    pub fn record_programmable_ps(
        &mut self,
        ps_id: ProgramId,
        variant: VariantKey,
        backend: ShaderBackend,
        outcome: Option<StageLibHandles>,
    ) {
        self.memo.forget();
        self.prog_ps.record((ps_id, variant, backend), outcome);
    }

    /// Forget the failed keys so each is built once more; built libraries stay.
    pub fn forget_failures(&mut self) {
        self.memo.forget();
        self.ff_vs.forget_failures();
        self.prog_vs.forget_failures();
        for variants in self.ff_ps.values_mut() {
            variants.forget_failures();
        }
        self.prog_ps.forget_failures();
    }

    /// Forget every key, for a teardown that destroys the libraries they name.
    pub fn clear(&mut self) {
        self.memo.forget();
        self.ff_vs.clear();
        self.prog_vs.clear();
        self.ff_ps.clear();
        self.prog_ps.clear();
    }

    /// How many fixed-function libraries are built or failed, both stages.
    ///
    /// One per distinct fixed-function vertex shader key and one per distinct
    /// pixel shader key, whose variants count once. A key whose build failed
    /// counts too.
    pub fn ff_count(&self) -> usize {
        self.ff_vs.len() + self.ff_ps.len()
    }

    /// Forget the memo before a packet whose records may reuse the previous packet's addresses.
    pub const fn begin_packet(&mut self) {
        self.memo.forget();
    }

    /// Look up a VS source whose record the memo does not hold, and memoise it when built.
    fn memo_vs(
        &mut self,
        vs: VsSourceView<'_>,
        backend: ShaderBackend,
        record: usize,
    ) -> Option<StageLibHandles> {
        let handles = (*self.lookup_vs(vs, backend)?)?;
        self.memo.vs_record = record;
        self.memo.vs = handles;
        Some(handles)
    }

    /// Look up a PS source and variant the memo does not hold, and memoise them when built.
    fn memo_ps(
        &mut self,
        ps: PsSourceView<'_>,
        variant: VariantKey,
        backend: ShaderBackend,
        record: usize,
    ) -> Option<StageLibHandles> {
        let handles = (*self.lookup_ps(ps, variant, backend)?)?;
        self.memo.ps_record = record;
        self.memo.ps_variant = variant;
        self.memo.ps = handles;
        Some(handles)
    }

    /// Check, in builds with debug assertions, that a memoised VS answer is the index's own.
    #[inline]
    fn debug_assert_memo_vs(&self, vs: VsSourceView<'_>, backend: ShaderBackend) {
        debug_assert!(
            matches!(self.lookup_vs(vs, backend), Some(Some(handles)) if same_handles(handles, &self.memo.vs)),
            "the VS library memo answers what the index holds for its record"
        );
    }

    /// Check, in builds with debug assertions, that a memoised PS answer is the index's own.
    #[inline]
    fn debug_assert_memo_ps(
        &self,
        ps: PsSourceView<'_>,
        variant: VariantKey,
        backend: ShaderBackend,
    ) {
        debug_assert!(
            matches!(self.lookup_ps(ps, variant, backend), Some(Some(handles)) if same_handles(handles, &self.memo.ps)),
            "the PS library memo answers what the index holds for its record and variant"
        );
    }
}

/// The built libraries of the previous draw, by the addresses of its source records.
///
/// A record address of zero is an empty slot: no source record is at
/// address zero.
struct LibraryMemo {
    vs_record: usize,
    vs: StageLibHandles,
    ps_record: usize,
    ps_variant: VariantKey,
    ps: StageLibHandles,
}

impl LibraryMemo {
    const EMPTY_HANDLES: StageLibHandles = StageLibHandles {
        library: MetalHandle::NULL,
        func: MetalHandle::NULL,
    };

    /// Whether the memo holds the pixel stage of this record and variant.
    ///
    /// The variant is compared as [`variant_words`]: the derived field-wise
    /// compare, inlined into the draw, assembled the caller's key byte by
    /// byte from registers.
    #[inline]
    fn holds_ps(&self, record: usize, variant: &VariantKey) -> bool {
        self.ps_record == record && variant_words(&self.ps_variant) == variant_words(variant)
    }

    const fn forget(&mut self) {
        self.vs_record = 0;
        self.ps_record = 0;
    }
}

impl Default for LibraryMemo {
    fn default() -> Self {
        Self {
            vs_record: 0,
            vs: Self::EMPTY_HANDLES,
            ps_record: 0,
            ps_variant: VariantKey::default(),
            ps: Self::EMPTY_HANDLES,
        }
    }
}

/// The identity of a VS source record: its address, with the low bit set for a fixed-function one.
///
/// Source records are 8-aligned, so the tag bit never collides with an
/// address bit.
fn vs_record(source: VsSourceView<'_>) -> usize {
    match source {
        VsSourceView::Programmable(value) => core::ptr::from_ref(value).addr(),
        VsSourceView::FixedFunction(value) => core::ptr::from_ref(value).addr() | 1,
    }
}

/// The memo identity of a draw's VS source: the snapshot record, tagged for the vertex LOD table.
///
/// `vs` is `snapshot` itself or its copy with `VsSamplerKinds::lod_table`
/// set. The copy is a local of the draw, so its address names no shader;
/// the snapshot record's address does, and bit 1 (free, the records being
/// 8-aligned) tells the two keys of one record apart.
fn vs_memo_identity(vs: VsSourceView<'_>, snapshot: VsSourceView<'_>) -> usize {
    let lod_table =
        matches!(vs, VsSourceView::Programmable(value) if value.sampler_kinds.lod_table);
    vs_record(snapshot) | (usize::from(lod_table) << 1)
}

/// The identity of a PS source record, tagged like [`vs_record`].
fn ps_record(source: PsSourceView<'_>) -> usize {
    match source {
        PsSourceView::Programmable(value) => core::ptr::from_ref(value).addr(),
        PsSourceView::FixedFunction(value) => core::ptr::from_ref(value).addr() | 1,
    }
}

/// Bit 2 of a memo identity, set for an AIR library: the records are 8-aligned, so it is free.
const fn backend_tag(backend: ShaderBackend) -> usize {
    (backend as usize) << 2
}

/// The 22 bytes of a variant key as three words; equal words mean equal keys.
///
/// Field order and widths follow the `repr(C)` layout, so the backend reads
/// each word with one load. The destructuring names every field, so a field
/// added to the key fails to compile here instead of escaping the compare.
fn variant_words(variant: &VariantKey) -> [u64; 3] {
    let VariantKey {
        alpha_func,
        fog_mode,
        fog_table_mode,
        linked_input_mask,
        depth_sampler_mask,
        depth_fetch_mask,
        fetch4_mask,
        fetch4_alpha_mask,
        raw_depth_red_mask,
        volume_sampler_mask,
        cube_sampler_mask,
        tt_projected_mask,
        color_out_mask,
        sample_mask,
        flags,
    } = variant;
    [
        u64::from(*alpha_func)
            | u64::from(*fog_mode) << 8
            | u64::from(*fog_table_mode) << 16
            | u64::from(*linked_input_mask) << 24
            | u64::from(*depth_sampler_mask) << 32
            | u64::from(*depth_fetch_mask) << 48,
        u64::from(*fetch4_mask)
            | u64::from(*fetch4_alpha_mask) << 16
            | u64::from(*raw_depth_red_mask) << 32
            | u64::from(*volume_sampler_mask) << 48,
        u64::from(*cube_sampler_mask)
            | u64::from(*tt_projected_mask) << 16
            | u64::from(*color_out_mask) << 24
            | u64::from(*sample_mask) << 32
            | u64::from(flags.bits()) << 40,
    ]
}

/// The programmable VS index key: program, provided inputs, clip planes, sampler kinds, backend.
const fn programmable_vs_key(
    value: &ProgrammableVsSource,
    backend: ShaderBackend,
) -> ProgrammableVsKey {
    (
        value.vs_id,
        value.provided_input_mask,
        value.clip_plane_count,
        value.sampler_kinds,
        backend,
    )
}

/// The backend a draw's VS/PS pair is built with (rulings C3b-2, C3b-3).
///
/// Under `shader.backend=msl` every pair is MSL, decided on the first compare.
/// Under `air` a pair is AIR when both stages are programmable, neither program
/// has failed as AIR this session, and both lower through the translator's IR
/// with the pair's own specialization; otherwise both stages are MSL, so an
/// AIR stage never meets an MSL one. The lowering is a prefilter: a library or
/// pipeline that still fails as AIR marks its programs failed ([`Self::fail`]),
/// and their later pairs go MSL. Each stage key's lowering runs once.
pub struct PairRule {
    configured: ShaderBackend,
    vs_lowers: FxHashMap<(ProgramId, u16, u8, VsSamplerKinds), bool>,
    ps_lowers: FxHashMap<(ProgramId, VariantKey), bool>,
    failed: FxHashSet<ProgramId>,
    refused: FxHashSet<ProgramId>,
}

impl PairRule {
    pub fn new(configured: ShaderBackend) -> Self {
        Self {
            configured,
            vs_lowers: FxHashMap::default(),
            ps_lowers: FxHashMap::default(),
            failed: FxHashSet::default(),
            refused: FxHashSet::default(),
        }
    }

    /// The pair's backend; `programs` holds every registered program.
    #[inline]
    pub fn pair_backend(
        &mut self,
        programs: &FxHashMap<ProgramId, Arc<DxsoProgram>>,
        vs: VsSourceView<'_>,
        ps: PsSourceView<'_>,
        variant: &VariantKey,
    ) -> ShaderBackend {
        if self.configured == ShaderBackend::Msl {
            return ShaderBackend::Msl;
        }
        self.air_pair_backend(programs, vs, ps, variant)
    }

    fn air_pair_backend(
        &mut self,
        programs: &FxHashMap<ProgramId, Arc<DxsoProgram>>,
        vs: VsSourceView<'_>,
        ps: PsSourceView<'_>,
        variant: &VariantKey,
    ) -> ShaderBackend {
        let (VsSourceView::Programmable(vs), PsSourceView::Programmable(ps)) = (vs, ps) else {
            return ShaderBackend::Msl;
        };
        if self.failed.contains(&vs.vs_id) || self.failed.contains(&ps.ps_id) {
            return ShaderBackend::Msl;
        }
        let vs_lowers = *self
            .vs_lowers
            .entry((
                vs.vs_id,
                vs.provided_input_mask,
                vs.clip_plane_count,
                vs.sampler_kinds,
            ))
            .or_insert_with(|| {
                let spec = VsSpec {
                    provided_input_mask: vs.provided_input_mask,
                    clip_plane_count: vs.clip_plane_count,
                    sampler_kinds: vs.sampler_kinds,
                };
                programs.get(&vs.vs_id).is_some_and(|program| {
                    lower::vertex(program, &spec)
                        .map_err(|e| log_refusal(&mut self.refused, "VS", vs.vs_id, &e))
                        .is_ok()
                })
            });
        if !vs_lowers {
            return ShaderBackend::Msl;
        }
        let ps_lowers = *self
            .ps_lowers
            .entry((ps.ps_id, *variant))
            .or_insert_with(|| {
                programs.get(&ps.ps_id).is_some_and(|program| {
                    lower::pixel(program, variant)
                        .map_err(|e| log_refusal(&mut self.refused, "PS", ps.ps_id, &e))
                        .is_ok()
                })
            });
        if ps_lowers {
            ShaderBackend::Air
        } else {
            ShaderBackend::Msl
        }
    }

    /// Mark a program failed as AIR; whether it was not already.
    pub fn fail(&mut self, program: ProgramId) -> bool {
        self.failed.insert(program)
    }
}

/// Log a program's first lowering refusal (once per program, not per stage key); whether it was.
fn log_refusal(
    refused: &mut FxHashSet<ProgramId>,
    stage: &str,
    program: ProgramId,
    error: &dyn std::fmt::Debug,
) -> bool {
    let first = refused.insert(program);
    if first {
        info!(target: crate::LOG_TARGET, "{stage} {program:#x} stays MSL: {error:?}");
    }
    first
}

const fn same_handles(left: &StageLibHandles, right: &StageLibHandles) -> bool {
    left.library.raw() == right.library.raw() && left.func.raw() == right.func.raw()
}

#[cfg(test)]
mod tests;
