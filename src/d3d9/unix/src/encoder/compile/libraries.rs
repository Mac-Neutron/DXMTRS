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
//! Under `shader.backend=air` a stage has two libraries per key, one per
//! backend ([`PairRule`] picks the pair's), so the programmable keys, the
//! fixed-function index pairs and the memo carry the backend.

use std::sync::Arc;

use airconv_ir::dxso::{lower, spec::VsSpec};
use log::info;
use mtld3d_core::{
    build_index::BuildIndex,
    config::ShaderBackend,
    draw_data::{
        FixedPsSource, FixedVsSource, ProgrammablePsSource, ProgrammableVsSource,
        ps_source_disk_key_ff, vs_source_disk_key_ff,
    },
    dxso::{
        DxsoError, DxsoProgram, FfPsKey, FfVsKey, LOG_TARGET as DXSO_TARGET, VariantKey,
        VsSamplerKinds, parse,
    },
    ff_build::{self, FfProgram},
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
///
/// The fixed-function indexes are one per backend, indexed by
/// `backend as usize`, so a probe borrows the key instead of building an
/// owned `(key, backend)` tuple.
#[derive(Default)]
pub struct StageLibraries {
    ff_vs: [BuildIndex<FfVsKey, StageLibHandles>; 2],
    prog_vs: BuildIndex<ProgrammableVsKey, StageLibHandles>,
    ff_ps: [FxHashMap<FfPsKey, BuildIndex<VariantKey, StageLibHandles>>; 2],
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
            VsSourceView::FixedFunction(FixedVsSource { key, .. }) => {
                self.ff_vs[backend as usize].lookup_entry(key)
            }
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
            PsSourceView::FixedFunction(FixedPsSource { key, .. }) => self.ff_ps[backend as usize]
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
                self.record_ff_vs(key.clone(), backend, outcome);
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
                self.record_ff_ps(key.clone(), variant, backend, outcome);
            }
            PsSourceView::Programmable(ProgrammablePsSource { ps_id, .. }) => {
                self.record_programmable_ps(*ps_id, variant, backend, outcome);
            }
        }
    }

    /// Record a fixed-function VS build outcome under its owned key and backend.
    pub fn record_ff_vs(
        &mut self,
        key: FfVsKey,
        backend: ShaderBackend,
        outcome: Option<StageLibHandles>,
    ) {
        self.memo.forget();
        self.ff_vs[backend as usize].record(key, outcome);
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

    /// Record a fixed-function PS build outcome under its owned key, variant and backend.
    pub fn record_ff_ps(
        &mut self,
        key: FfPsKey,
        variant: VariantKey,
        backend: ShaderBackend,
        outcome: Option<StageLibHandles>,
    ) {
        self.memo.forget();
        self.ff_ps[backend as usize]
            .entry(key)
            .or_default()
            .record(variant, outcome);
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
        for index in &mut self.ff_vs {
            index.forget_failures();
        }
        self.prog_vs.forget_failures();
        for variants in self.ff_ps.iter_mut().flat_map(FxHashMap::values_mut) {
            variants.forget_failures();
        }
        self.prog_ps.forget_failures();
    }

    /// Forget every key, for a teardown that destroys the libraries they name.
    pub fn clear(&mut self) {
        self.memo.forget();
        for index in &mut self.ff_vs {
            index.clear();
        }
        self.prog_vs.clear();
        for index in &mut self.ff_ps {
            index.clear();
        }
        self.prog_ps.clear();
    }

    /// How many fixed-function libraries are built or failed, both stages.
    ///
    /// One per distinct fixed-function vertex shader key and one per distinct
    /// pixel shader key, whose variants count once, per backend. A key whose
    /// build failed counts too.
    pub fn ff_count(&self) -> usize {
        self.ff_vs.iter().map(BuildIndex::len).sum::<usize>()
            + self.ff_ps.iter().map(FxHashMap::len).sum::<usize>()
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

/// A stage that failed as AIR: a program, or a fixed-function key (ruling C3c-6).
///
/// What [`PairRule::fail`] takes and what a pipeline build carries to its
/// install, so the failure of an AIR pipeline reaches both of its stages.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AirStage {
    Program(ProgramId),
    FfVs(FfVsKey),
    FfPs(FfPsKey),
}

/// A fixed-function stage built as DXSO and parsed, once per key on the encoder thread.
///
/// The prefilter lowers `parsed`; the AIR worker translates the built
/// program's tokens and writes them in the record's source, so the key is
/// never built twice.
#[derive(Debug)]
pub struct FfBuilt {
    program: FfProgram,
    parsed: DxsoProgram,
}

impl FfBuilt {
    /// Parse a built program. Its tokens skip the application shader checks.
    ///
    /// # Errors
    ///
    /// The parser's diagnostic when the builder made a program it cannot read.
    pub fn new(program: FfProgram) -> Result<Self, DxsoError> {
        let parsed = parse(&program.tokens)?;
        Ok(Self { program, parsed })
    }

    /// The built program: tokens, input mask and warnings.
    pub const fn program(&self) -> &FfProgram {
        &self.program
    }
}

/// The backend a draw's VS/PS pair is built with (rulings C3b-2, C3b-3, C3c-6).
///
/// Under `shader.backend=msl` every pair is MSL, decided on the first compare.
/// Under `air` a pair is AIR when both stages lower through the translator's IR
/// with the pair's own specialization and neither has failed as AIR this
/// session; otherwise both stages are MSL, so an AIR stage never meets an MSL
/// one. A fixed-function stage lowers as the program `ff_build` makes of its
/// key, built and parsed once per key and kept ([`FfBuilt`]) for the AIR
/// build. The lowering is a prefilter: a library or pipeline that still fails
/// as AIR marks its stages failed ([`Self::fail`]), and their later pairs go
/// MSL. Each stage key's lowering runs once; a fixed-function key's failure
/// lives in its memo entry, so a known key's probe is a borrow.
pub struct PairRule {
    configured: ShaderBackend,
    vs_lowers: FxHashMap<(ProgramId, u16, u8, VsSamplerKinds), bool>,
    ps_lowers: FxHashMap<(ProgramId, VariantKey), bool>,
    /// `None`: refused or failed as AIR.
    ff_vs: FxHashMap<FfVsKey, Option<Arc<FfBuilt>>>,
    ff_ps: FxHashMap<FfPsKey, FfPsLowering>,
    failed: FxHashSet<ProgramId>,
    refused: FxHashSet<ProgramId>,
}

/// A fixed-function pixel key's build, and whether it lowers with each variant seen.
struct FfPsLowering {
    /// `None`: refused or failed as AIR, whatever the variant.
    built: Option<Arc<FfBuilt>>,
    variants: FxHashMap<VariantKey, bool>,
}

impl PairRule {
    pub fn new(configured: ShaderBackend) -> Self {
        Self {
            configured,
            vs_lowers: FxHashMap::default(),
            ps_lowers: FxHashMap::default(),
            ff_vs: FxHashMap::default(),
            ff_ps: FxHashMap::default(),
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

    // ponytail: a fixed-function draw hashes its keys here under `air`; an address memo in front,
    // as `LibraryMemo` has, if a profile shows the probe.
    fn air_pair_backend(
        &mut self,
        programs: &FxHashMap<ProgramId, Arc<DxsoProgram>>,
        vs: VsSourceView<'_>,
        ps: PsSourceView<'_>,
        variant: &VariantKey,
    ) -> ShaderBackend {
        if self.vs_lowers(programs, vs) && self.ps_lowers(programs, ps, variant) {
            ShaderBackend::Air
        } else {
            ShaderBackend::Msl
        }
    }

    /// Whether a VS stage lowers and has not failed as AIR.
    fn vs_lowers(
        &mut self,
        programs: &FxHashMap<ProgramId, Arc<DxsoProgram>>,
        vs: VsSourceView<'_>,
    ) -> bool {
        let vs = match vs {
            VsSourceView::FixedFunction(FixedVsSource { key, .. }) => {
                if let Some(built) = self.ff_vs.get(key) {
                    return built.is_some();
                }
                return self.lower_ff_vs(key);
            }
            VsSourceView::Programmable(vs) => vs,
        };
        if self.failed.contains(&vs.vs_id) {
            return false;
        }
        *self
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
            })
    }

    /// Whether a PS stage lowers with `variant` and has not failed as AIR.
    fn ps_lowers(
        &mut self,
        programs: &FxHashMap<ProgramId, Arc<DxsoProgram>>,
        ps: PsSourceView<'_>,
        variant: &VariantKey,
    ) -> bool {
        let ps = match ps {
            PsSourceView::FixedFunction(FixedPsSource { key, .. }) => {
                if let Some(entry) = self.ff_ps.get(key) {
                    if entry.built.is_none() {
                        return false;
                    }
                    if let Some(&lowers) = entry.variants.get(variant) {
                        return lowers;
                    }
                }
                return self.lower_ff_ps(key, variant);
            }
            PsSourceView::Programmable(ps) => ps,
        };
        if self.failed.contains(&ps.ps_id) {
            return false;
        }
        *self
            .ps_lowers
            .entry((ps.ps_id, *variant))
            .or_insert_with(|| {
                programs.get(&ps.ps_id).is_some_and(|program| {
                    lower::pixel(program, variant)
                        .map_err(|e| log_refusal(&mut self.refused, "PS", ps.ps_id, &e))
                        .is_ok()
                })
            })
    }

    /// Build, parse and lower a fixed-function VS key seen for the first time.
    #[cold]
    #[inline(never)]
    fn lower_ff_vs(&mut self, key: &FfVsKey) -> bool {
        let disk_key = vs_source_disk_key_ff(key);
        let built = build_ff(ff_build::vertex(key), "VS", disk_key).filter(|built| {
            let spec = VsSpec {
                provided_input_mask: built.program.provided_input_mask,
                clip_plane_count: key.clip_plane_count,
                sampler_kinds: VsSamplerKinds::default(),
            };
            lower::vertex(&built.parsed, &spec)
                .map_err(|e| log_ff_refusal("VS", disk_key, &e))
                .is_ok()
        });
        let lowers = built.is_some();
        self.ff_vs.insert(key.clone(), built.map(Arc::new));
        lowers
    }

    /// Build and parse a fixed-function PS key seen first, then lower it with `variant`.
    #[cold]
    #[inline(never)]
    fn lower_ff_ps(&mut self, key: &FfPsKey, variant: &VariantKey) -> bool {
        if !self.ff_ps.contains_key(key) {
            let built = build_ff(
                ff_build::pixel(key),
                "PS",
                ps_source_disk_key_ff(key, *variant),
            );
            self.ff_ps.insert(
                key.clone(),
                FfPsLowering {
                    built: built.map(Arc::new),
                    variants: FxHashMap::default(),
                },
            );
        }
        let Some(entry) = self.ff_ps.get_mut(key) else {
            return false;
        };
        let Some(built) = &entry.built else {
            return false;
        };
        let lowers = lower::pixel(&built.parsed, variant)
            .map_err(|e| log_ff_refusal("PS", ps_source_disk_key_ff(key, *variant), &e))
            .is_ok();
        entry.variants.insert(*variant, lowers);
        lowers
    }

    /// The program the prefilter built for a fixed-function VS key, for its AIR build.
    pub fn ff_vs_program(&self, key: &FfVsKey) -> Option<Arc<FfBuilt>> {
        self.ff_vs.get(key)?.clone()
    }

    /// The program the prefilter built for a fixed-function PS key, for its AIR build.
    pub fn ff_ps_program(&self, key: &FfPsKey) -> Option<Arc<FfBuilt>> {
        self.ff_ps.get(key)?.built.clone()
    }

    /// Mark a stage failed as AIR; whether it was not already.
    ///
    /// A fixed-function key's memo entry turns `None`, for every variant of a pixel key.
    pub fn fail(&mut self, stage: AirStage) -> bool {
        match stage {
            AirStage::Program(program) => self.failed.insert(program),
            AirStage::FfVs(key) => !matches!(self.ff_vs.insert(key, None), Some(None)),
            AirStage::FfPs(key) => {
                if let Some(entry) = self.ff_ps.get_mut(&key) {
                    return entry.built.take().is_some();
                }
                let failed = FfPsLowering {
                    built: None,
                    variants: FxHashMap::default(),
                };
                self.ff_ps.insert(key, failed);
                true
            }
        }
    }
}

/// Parse a fixed-function stage's built program and log its warnings, once each per process.
///
/// `None` when it does not parse, logged at info like a lowering refusal.
fn build_ff(program: FfProgram, stage: &str, disk_key: u64) -> Option<FfBuilt> {
    for &warning in &program.warnings {
        // ponytail: keyed by the text's address; a warning whose text sits at two addresses
        // logs twice.
        mtld3d_shared::log_once_warn_by!(
            target: DXSO_TARGET,
            key: warning.as_ptr().addr() as u64,
            "{warning}"
        );
    }
    FfBuilt::new(program)
        .map_err(|e| log_ff_refusal(stage, disk_key, &e))
        .ok()
}

/// Log a fixed-function key's lowering refusal; the memo entry it fills keeps it to once a key.
fn log_ff_refusal(stage: &str, disk_key: u64, error: &dyn std::fmt::Debug) {
    info!(target: crate::LOG_TARGET, "{stage} ff {disk_key:#x} stays MSL: {error:?}");
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
