//! DXMT SQLite shader cache, single-mutex stage library table, and deduplicated recipe store.
//!
//! Shares the exact `shaders_<metal_version>.db` file and `cache_<version>` table schema with
//! D3D11 and D3D12, using domain-tagged 40-byte keys so D3D9 AIR stages and `"D3D9REC1"` pipeline
//! recipes coexist with D3D10-12 entries without collisions.

use core::{
    ffi::CStr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::sync::{Arc, Condvar, Mutex};

use airconv_ir::dxso::spec::{
    StageSpec, VARIANT_KEY_WIRE, VS_SPEC_WIRE, VariantFlags, VariantKey, VsSamplerKinds,
};
use dxmt_core::{
    shader::{
        D3D9_PS_WIRE, D3D9_VS_WIRE, Entry, compile, compiled_data, initialize_dxso, load_function,
    },
    shader_cache::Store,
};
use dxmt_util::{sha1::Sha1, sync::lock};
use mtld3d_core::{
    dxso::{FfPsKey, FfStage, FfStageFlags, FfVsFlags, FfVsKey, MAX_LINKED_INPUTS},
    ids::ProgramId,
    pipeline_key::{
        CoreExtra, DeclCode, MAX_STREAMS, PipelineKey, PsStageKey, ShaderCode, StageSource,
        StreamKey, VertexStepFunction, VsStageKey,
    },
    pipeline_state::{BlendFactor, BlendOperation, CoreBlend},
};
use mtld3d_types::D3DVERTEXELEMENT9;
use rustc_hash::{FxHashMap, FxHashSet};
use winemetal_sys::{
    DispatchData_alloc_init, DispatchData_copyBytes, Obj, Pool,
    sm50::{AIRCONV_VERSION, SM50_SHADER_COMMON, SM50_SHADER_METAL_310, Sm50ShaderCommonData},
};

use super::LOG_TARGET;

/// Maximum pipeline recipes persisted in one `"D3D9REC1"` blob.
pub const MAX_RECIPES: usize = 4096;
/// Maximum byte length accepted when reading a `"D3D9REC1"` blob from SQLite.
const MAX_RECIPE_BLOB: usize = 64 * 1024 * 1024;
/// Upper bound on shader token counts in a decoded recipe blob.
const MAX_SHADER_TOKENS: usize = 1024 * 1024;
/// Upper bound on vertex-declaration elements in a decoded recipe blob.
const MAX_DECL_ELEMENTS: usize = 4096;

const RECIPE_MAGIC: &[u8; 8] = b"D3D9REC1";

const D3D9_VS_WIRE_U8: u8 = 6;
const D3D9_PS_WIRE_U8: u8 = 22;
const _: () = assert!(D3D9_VS_WIRE_U8 as usize == D3D9_VS_WIRE);
const _: () = assert!(D3D9_PS_WIRE_U8 as usize == D3D9_PS_WIRE);
const _: () = assert!(MAX_LINKED_INPUTS == 8);

/// Tracks whether this process opened a shared DXMT SQLite shader cache store.
static CACHE_OPENED: AtomicBool = AtomicBool::new(false);

const fn cache_key_label() -> &'static str {
    if env!("DXMT_TRANSLATOR_KEY").is_empty() {
        env!("DXMT_VERSION")
    } else {
        env!("DXMT_TRANSLATOR_KEY")
    }
}

/// Computes the 64-bit FNV-1a shader cache table version matching D3D11 and D3D12.
#[must_use]
pub const fn shader_cache_version() -> u64 {
    dxmt_core::shader_cache::version(cache_key_label(), AIRCONV_VERSION)
}

/// Closes any open SQLite shader cache stores non-blockingly on `DLL_PROCESS_DETACH`.
///
/// Does zero work and avoids initializing `CACHES` when no core device opened the cache.
pub fn close_shader_cache_on_detach() {
    if CACHE_OPENED.load(Ordering::Acquire) {
        dxmt_core::shader_cache::try_close_all();
    }
}

/// Opens the shared SQLite [`Store`] for `metal_version` and logs the cache identity once.
#[must_use]
pub fn open_store(metal_version: u32) -> Arc<Store> {
    let version = shader_cache_version();
    let store = dxmt_core::shader_cache::instance(metal_version, version);
    CACHE_OPENED.store(true, Ordering::Release);
    log::log!(
        target: LOG_TARGET,
        super::gate_level(),
        "d3d9 core: shader cache opened (metal {metal_version}, version {version:016x}, key {})",
        cache_key_label()
    );
    store
}

/// Per-device shader cache and asynchronous compilation counters.
#[derive(Default)]
pub struct CompileStats {
    pub cache_hits: AtomicUsize,
    pub cache_misses: AtomicUsize,
    pub stage_compiles: AtomicUsize,
    pub async_compiled: AtomicUsize,
    pub draws_skipped: AtomicUsize,
    pub prewarmed_psos: AtomicUsize,
}

/// Computes the 40-byte domain-tagged SQLite cache key for a D3D9 shader stage.
///
/// `[0..20]` hashes the `"d3d9-air-v1"` domain tag, stage, wire specialization, and LE DXSO
/// tokens; `[20..40]` hashes the entry name, Metal version, and D3D9 wire struct sizes.
#[must_use]
pub fn stage_cache_key(
    tokens: &[u32],
    spec: &StageSpec,
    name: &CStr,
    metal_version: u32,
) -> [u8; 40] {
    let (wire_stage, vs_wire, ps_wire) = spec.to_wire();
    let stage_byte: u8 = match spec {
        StageSpec::Vs(_) => 0,
        StageSpec::Ps(_) => 1,
    };

    let mut h1 = Sha1::new();
    h1.update(b"d3d9-air-v1");
    h1.update(&[stage_byte]);
    h1.update(&wire_stage.to_le_bytes());
    h1.update(&vs_wire);
    h1.update(&ps_wire);
    for &tok in tokens {
        h1.update(&tok.to_le_bytes());
    }

    let mut h2 = Sha1::new();
    h2.update(name.to_bytes());
    h2.update(&metal_version.to_le_bytes());
    h2.update(&[D3D9_VS_WIRE_U8, D3D9_PS_WIRE_U8]);

    let mut key = [0_u8; 40];
    key[..20].copy_from_slice(h1.finalize().as_bytes());
    key[20..].copy_from_slice(h2.finalize().as_bytes());
    key
}

/// A compiled stage function's key: its program and its specialization's wire form.
#[derive(Clone, PartialEq, Eq, Hash)]
struct StageFnKey {
    code: ShaderCode,
    wire: (u32, [u8; VS_SPEC_WIRE], [u8; VARIANT_KEY_WIRE]),
}

struct StageGate {
    done: Mutex<bool>,
    cv: Condvar,
}

impl StageGate {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            done: Mutex::new(false),
            cv: Condvar::new(),
        })
    }

    fn wait(&self) {
        let mut done = lock(&self.done);
        while !*done {
            done = self
                .cv
                .wait(done)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(done);
    }

    fn signal(&self) {
        let mut done = lock(&self.done);
        *done = true;
        drop(done);
        self.cv.notify_all();
    }
}

enum StageEntry {
    Compiling(Arc<StageGate>),
    Done(Option<Obj>),
}

struct GateGuard<'a> {
    table: &'a Mutex<FxHashMap<StageFnKey, StageEntry>>,
    key: StageFnKey,
    gate: Arc<StageGate>,
    armed: bool,
}

impl<'a> GateGuard<'a> {
    const fn new(
        table: &'a Mutex<FxHashMap<StageFnKey, StageEntry>>,
        key: StageFnKey,
        gate: Arc<StageGate>,
    ) -> Self {
        Self {
            table,
            key,
            gate,
            armed: true,
        }
    }

    fn finish(mut self, result: Option<Obj>) {
        self.armed = false;
        let mut table = lock(self.table);
        if !matches!(table.get(&self.key), Some(StageEntry::Done(_))) {
            table.insert(self.key.clone(), StageEntry::Done(result));
        }
        drop(table);
        self.gate.signal();
    }
}

impl Drop for GateGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut table = lock(self.table);
            if matches!(
                table.get(&self.key),
                Some(StageEntry::Compiling(g)) if Arc::ptr_eq(g, &self.gate)
            ) {
                table.remove(&self.key);
            }
            drop(table);
            self.gate.signal();
        }
    }
}

/// Thread-safe, single-mutex, append-only cache of compiled Metal stage functions.
///
/// Entries move from `Compiling` to `Done` at most once and are never overwritten or evicted while
/// the device lives, ensuring raw `Obj` handles handed to pipelines remain valid.
#[derive(Default)]
pub struct StageFunctions {
    table: Mutex<FxHashMap<StageFnKey, StageEntry>>,
    cache: Option<Arc<Store>>,
    stats: Arc<CompileStats>,
}

impl StageFunctions {
    /// Creates a stage-function table backed by an optional SQLite `cache` and shared `stats`.
    #[must_use]
    pub fn with_cache(cache: Option<Arc<Store>>, stats: Arc<CompileStats>) -> Self {
        Self {
            table: Mutex::new(FxHashMap::default()),
            cache,
            stats,
        }
    }

    /// Returns whether `stage` has already finished compiling in this table.
    #[must_use]
    pub fn is_done(&self, stage: &StageSource) -> bool {
        let key = StageFnKey {
            code: stage.code.clone(),
            wire: stage.spec.to_wire(),
        };
        let table = lock(&self.table);
        let ready = matches!(table.get(&key), Some(StageEntry::Done(_)));
        drop(table);
        ready
    }

    /// Returns the Metal function handle for `stage`, reading SQLite or compiling on miss.
    pub fn function(
        &self,
        mtl_device: &Obj,
        metal_version: u32,
        stage: &StageSource,
        name: &CStr,
    ) -> Option<u64> {
        let key = StageFnKey {
            code: stage.code.clone(),
            wire: stage.spec.to_wire(),
        };
        loop {
            let action = {
                let mut table = lock(&self.table);
                match table.get(&key) {
                    Some(StageEntry::Done(opt)) => {
                        let handle = opt.as_ref().map(Obj::handle);
                        drop(table);
                        return handle;
                    }
                    Some(StageEntry::Compiling(gate)) => {
                        let gate = Arc::clone(gate);
                        drop(table);
                        Err(gate)
                    }
                    None => {
                        let gate = StageGate::new();
                        table.insert(key.clone(), StageEntry::Compiling(Arc::clone(&gate)));
                        drop(table);
                        Ok(gate)
                    }
                }
            };
            match action {
                Err(gate) => gate.wait(),
                Ok(gate) => {
                    let guard = GateGuard::new(&self.table, key, gate);
                    let compiled = self.lookup_or_compile(mtl_device, metal_version, stage, name);
                    let handle = compiled.as_ref().map(Obj::handle);
                    guard.finish(compiled);
                    return handle;
                }
            }
        }
    }

    fn lookup_or_compile(
        &self,
        mtl_device: &Obj,
        metal_version: u32,
        stage: &StageSource,
        name: &CStr,
    ) -> Option<Obj> {
        let _pool = Pool::new();
        let cache_key = stage_cache_key(stage.code.tokens(), &stage.spec, name, metal_version);

        if let Some(store) = &self.cache
            && store.has_reader()
        {
            if let Some(data) = store.read(&cache_key) {
                match load_function(mtl_device, &data, name) {
                    Ok(func) => {
                        self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
                        return Some(func);
                    }
                    Err(_) => {
                        log::warn!(
                            target: LOG_TARGET,
                            "d3d9 core: cached shader failed to load; recompiling"
                        );
                    }
                }
            }
            self.stats.cache_misses.fetch_add(1, Ordering::Relaxed);
        }

        let data = compile_stage_data(stage.code.tokens(), &stage.spec, name)?;
        if let Some(store) = &self.cache
            && store.has_writer()
        {
            store.write(&cache_key, &data);
        }
        let func = load_function(mtl_device, &data, name).ok()?;
        self.stats.stage_compiles.fetch_add(1, Ordering::Relaxed);
        Some(func)
    }

    /// Number of fresh stage compilations performed on this table.
    #[cfg(test)]
    #[must_use]
    pub fn stage_compiles(&self) -> usize {
        self.stats.stage_compiles.load(Ordering::Relaxed)
    }
}

/// Translates DXSO `tokens` with `spec` into a metallib `DispatchData` object.
fn compile_stage_data(tokens: &[u32], spec: &StageSpec, name: &CStr) -> Option<Obj> {
    let bytes = tokens
        .iter()
        .flat_map(|t| t.to_le_bytes())
        .collect::<Vec<u8>>();
    let (shader, mut d3d9) = initialize_dxso(&bytes, spec.to_wire())
        .map_err(|e| log::error!(target: LOG_TARGET, "d3d9 core: DXSO: {}", e.message()))
        .ok()?;
    let mut common = Sm50ShaderCommonData {
        next: (&raw mut d3d9).cast(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };
    // SAFETY: `shader` is live; the chain (`common`, then `d3d9`) lives for the call.
    let bitcode = unsafe { compile(&Entry::Shader, shader.0, 0, (&raw mut common).cast(), name) }
        .map_err(|e| log::error!(target: LOG_TARGET, "d3d9 core: compile: {}", e.message()))
        .ok()?;
    compiled_data(&bitcode)
}

/// Computes the 40-byte SQLite key for the `"D3D9REC1"` prewarm recipe blob.
///
/// `[0..20]` hashes `"d3d9-recipes-v1"`, `[20..40]` hashes `metal_version` in little-endian.
#[must_use]
pub fn recipe_cache_key(metal_version: u32) -> [u8; 40] {
    let mut key = [0_u8; 40];
    key[..20].copy_from_slice(Sha1::compute(b"d3d9-recipes-v1").as_bytes());
    key[20..].copy_from_slice(Sha1::compute(&metal_version.to_le_bytes()).as_bytes());
    key
}

/// Reads all bytes out of a `DispatchData` object using a 2-call `DispatchData_copyBytes`.
fn read_dispatch_data(data: &Obj) -> Option<Vec<u8>> {
    // SAFETY: `data` is a live `DispatchData` handle; null buffer queries full byte length.
    let total = unsafe { DispatchData_copyBytes(data.handle(), core::ptr::null_mut(), 0) };
    let Ok(len) = usize::try_from(total) else {
        return None;
    };
    if len == 0 || len > MAX_RECIPE_BLOB {
        return None;
    }
    let mut buf = vec![0_u8; len];
    // SAFETY: `data` is live and `buf` holds `len` writable bytes.
    let copied = unsafe { DispatchData_copyBytes(data.handle(), buf.as_mut_ptr().cast(), total) };
    if copied != total {
        return None;
    }
    Some(buf)
}

/// Loads persisted prewarm [`PipelineKey`] recipes from `store` for `metal_version`.
#[must_use]
pub fn load_recipes(store: &Store, metal_version: u32) -> Vec<PipelineKey> {
    if !store.has_reader() {
        return Vec::new();
    }
    let key = recipe_cache_key(metal_version);
    let Some(data) = store.read(&key) else {
        return Vec::new();
    };
    let Some(bytes) = read_dispatch_data(&data) else {
        log::warn!(
            target: LOG_TARGET,
            "d3d9 core: ignoring unreadable shader recipe blob"
        );
        return Vec::new();
    };
    decode_recipes(&bytes)
}

/// Tracks loaded and newly built [`PipelineKey`] recipes for prewarm persistence on device drop.
pub struct RecipeRecorder {
    keys: Vec<PipelineKey>,
    seen: FxHashSet<PipelineKey>,
    recipes_loaded: usize,
    dirty: bool,
}

impl Default for RecipeRecorder {
    fn default() -> Self {
        Self::seed(Vec::new())
    }
}

impl RecipeRecorder {
    /// Seeds a recorder with `loaded` recipes from disk; marks it clean (`dirty = false`).
    #[must_use]
    pub fn seed(loaded: Vec<PipelineKey>) -> Self {
        let mut keys = Vec::with_capacity(loaded.len().min(MAX_RECIPES));
        let mut seen = FxHashSet::default();
        for key in loaded {
            if keys.len() >= MAX_RECIPES {
                break;
            }
            if seen.insert(key.clone()) {
                keys.push(key);
            }
        }
        let recipes_loaded = keys.len();
        Self {
            keys,
            seen,
            recipes_loaded,
            dirty: false,
        }
    }

    /// Number of deduplicated recipes seeded from the cache at open.
    #[must_use]
    pub const fn recipes_loaded(&self) -> usize {
        self.recipes_loaded
    }

    /// Records a newly compiled [`PipelineKey`], marking the recorder dirty only on new keys.
    pub fn record(&mut self, key: &PipelineKey) {
        if self.keys.len() >= MAX_RECIPES {
            return;
        }
        if self.seen.insert(key.clone()) {
            self.keys.push(key.clone());
            self.dirty = true;
        }
    }

    /// Flushes `existing ∪ recorded` (capped at [`MAX_RECIPES`]) to `store` when dirty.
    pub fn flush(&mut self, store: &Store, metal_version: u32) -> usize {
        if !self.dirty || !store.has_writer() {
            return 0;
        }
        let existing = load_recipes(store, metal_version);
        let mut merged = Vec::with_capacity((self.keys.len() + existing.len()).min(MAX_RECIPES));
        let mut merged_seen = FxHashSet::default();
        for key in self.keys.iter().chain(existing.iter()) {
            if merged.len() >= MAX_RECIPES {
                break;
            }
            if merged_seen.insert(key.clone()) {
                merged.push(key.clone());
            }
        }
        let bytes = encode_recipes(&merged);
        if bytes.len() > MAX_RECIPE_BLOB {
            return 0;
        }
        // SAFETY: `bytes` is alive for `DispatchData_alloc_init`, which copies the slice.
        let handle =
            unsafe { DispatchData_alloc_init(bytes.as_ptr().addr() as u64, bytes.len() as u64) };
        // SAFETY: `handle` is 0 or a newly retained `DispatchData`.
        let Some(obj) = (unsafe { Obj::adopt(handle) }) else {
            return 0;
        };
        let cache_key = recipe_cache_key(metal_version);
        store.write(&cache_key, &obj);
        self.dirty = false;
        merged.len()
    }
}

/// Encodes `keys` into the deduplicated `"D3D9REC1"` binary recipe format.
#[must_use]
pub fn encode_recipes(keys: &[PipelineKey]) -> Vec<u8> {
    let mut shaders: Vec<ShaderCode> = Vec::new();
    let mut shader_map: FxHashMap<ShaderCode, u32> = FxHashMap::default();
    let mut decls: Vec<DeclCode> = Vec::new();
    let mut decl_map: FxHashMap<DeclCode, u32> = FxHashMap::default();

    let count = keys.len().min(MAX_RECIPES);
    for key in &keys[..count] {
        if let VsStageKey::Prog { code, .. } = &key.vs {
            intern_shader(&mut shaders, &mut shader_map, code);
        }
        if let PsStageKey::Prog(code) = &key.ps {
            intern_shader(&mut shaders, &mut shader_map, code);
        }
        intern_decl(&mut decls, &mut decl_map, &key.layout);
    }

    let mut out = Vec::with_capacity(64 + count * 128);
    out.extend_from_slice(RECIPE_MAGIC);
    out.extend_from_slice(
        &u32::try_from(shaders.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&u32::try_from(decls.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&u32::try_from(count).unwrap_or(u32::MAX).to_le_bytes());

    for shader in &shaders {
        let toks = shader.tokens();
        out.extend_from_slice(&u32::try_from(toks.len()).unwrap_or(u32::MAX).to_le_bytes());
        for &tok in toks {
            out.extend_from_slice(&tok.to_le_bytes());
        }
    }

    for decl in &decls {
        let elems = decl.elements_with_end();
        out.extend_from_slice(&u32::try_from(elems.len()).unwrap_or(u32::MAX).to_le_bytes());
        for elem in elems {
            out.extend_from_slice(&elem.stream.to_le_bytes());
            out.extend_from_slice(&elem.offset.to_le_bytes());
            out.push(elem.type_);
            out.push(elem.method);
            out.push(elem.usage);
            out.push(elem.usage_index);
        }
    }

    for key in &keys[..count] {
        encode_pipeline_key(&mut out, key, &shader_map, &decl_map);
    }
    out
}

fn intern_shader(
    shaders: &mut Vec<ShaderCode>,
    map: &mut FxHashMap<ShaderCode, u32>,
    code: &ShaderCode,
) {
    if !map.contains_key(code) {
        let idx = u32::try_from(shaders.len()).unwrap_or(u32::MAX);
        shaders.push(code.clone());
        map.insert(code.clone(), idx);
    }
}

fn intern_decl(decls: &mut Vec<DeclCode>, map: &mut FxHashMap<DeclCode, u32>, decl: &DeclCode) {
    if !map.contains_key(decl) {
        let idx = u32::try_from(decls.len()).unwrap_or(u32::MAX);
        decls.push(decl.clone());
        map.insert(decl.clone(), idx);
    }
}

fn encode_pipeline_key(
    out: &mut Vec<u8>,
    key: &PipelineKey,
    shader_map: &FxHashMap<ShaderCode, u32>,
    decl_map: &FxHashMap<DeclCode, u32>,
) {
    match &key.vs {
        VsStageKey::Ff(ff) => {
            out.push(0);
            out.extend_from_slice(&ff.flags.bits().to_le_bytes());
            out.push(ff.input_tex_coord_count);
            out.push(ff.tex_coord_count);
            out.push(ff.light_active_mask);
            out.push(ff.light_directional_mask);
            out.push(ff.light_spot_mask);
            out.push(ff.diffuse_source);
            out.push(ff.ambient_source);
            out.push(ff.specular_source);
            out.push(ff.emissive_source);
            out.push(ff.fog_mode);
            out.extend_from_slice(&ff.tci);
            out.extend_from_slice(&ff.passthrough);
            out.extend_from_slice(&ff.tex_coord_dims);
            out.extend_from_slice(&ff.tt_flags);
            out.push(ff.vertex_blend_count);
            out.push(ff.declared_weights_count);
            out.push(ff.clip_plane_count);
            out.push(ff.reserved);
        }
        VsStageKey::Prog {
            code,
            clip_plane_count,
            sampler_kinds,
        } => {
            out.push(1);
            out.extend_from_slice(&shader_map[code].to_le_bytes());
            out.push(*clip_plane_count);
            out.push(sampler_kinds.volume_mask);
            out.push(sampler_kinds.cube_mask);
            out.push(u8::from(sampler_kinds.lod_table));
        }
    }

    match &key.ps {
        PsStageKey::Ff(ff) => {
            out.push(0);
            for stage in &ff.stages {
                out.push(stage.color_op);
                out.push(stage.color_arg1);
                out.push(stage.color_arg2);
                out.push(stage.alpha_op);
                out.push(stage.alpha_arg1);
                out.push(stage.alpha_arg2);
                out.push(stage.flags.bits());
            }
            out.push(u8::from(ff.specular_add));
            out.push(ff.tt_projected_mask);
        }
        PsStageKey::Prog(code) => {
            out.push(1);
            out.extend_from_slice(&shader_map[code].to_le_bytes());
        }
    }

    out.push(key.variant.alpha_func);
    out.push(key.variant.fog_mode);
    out.push(key.variant.fog_table_mode);
    out.push(key.variant.linked_input_mask);
    out.extend_from_slice(&key.variant.depth_sampler_mask.to_le_bytes());
    out.extend_from_slice(&key.variant.depth_fetch_mask.to_le_bytes());
    out.extend_from_slice(&key.variant.fetch4_mask.to_le_bytes());
    out.extend_from_slice(&key.variant.fetch4_alpha_mask.to_le_bytes());
    out.extend_from_slice(&key.variant.raw_depth_red_mask.to_le_bytes());
    out.extend_from_slice(&key.variant.volume_sampler_mask.to_le_bytes());
    out.extend_from_slice(&key.variant.cube_sampler_mask.to_le_bytes());
    out.push(key.variant.tt_projected_mask);
    out.push(key.variant.color_out_mask);
    out.push(key.variant.sample_mask);
    out.push(key.variant.flags.bits());

    out.extend_from_slice(&decl_map[&key.layout].to_le_bytes());

    for stream in &key.streams {
        match *stream {
            StreamKey::Unused => out.push(0),
            StreamKey::Null => out.push(1),
            StreamKey::Inline { stride } => {
                out.push(2);
                out.extend_from_slice(&stride.to_le_bytes());
            }
            StreamKey::Bound {
                stride,
                step,
                rate,
                shift,
            } => {
                out.push(3);
                out.extend_from_slice(&stride.to_le_bytes());
                out.push((step as u32).to_le_bytes()[0]);
                out.extend_from_slice(&rate.to_le_bytes());
                out.push(shift);
            }
        }
    }

    out.push(key.write_mask);
    out.extend_from_slice(&key.color_format.to_le_bytes());
    out.extend_from_slice(&key.depth_format.to_le_bytes());
    encode_blend(out, key.blend.as_ref());
    for extra in &key.extras {
        match extra {
            None => out.push(0),
            Some(extra) => {
                out.push(1);
                out.extend_from_slice(&extra.format.to_le_bytes());
                out.push(extra.write_mask);
                encode_blend(out, extra.blend.as_ref());
            }
        }
    }
    out.push(key.samples);
    out.push(u8::from(key.alpha_to_coverage));
}

fn encode_blend(out: &mut Vec<u8>, blend: Option<&CoreBlend>) {
    match blend {
        None => out.push(0),
        Some(b) => {
            out.push(1);
            out.push((b.src as u32).to_le_bytes()[0]);
            out.push((b.dst as u32).to_le_bytes()[0]);
            out.push((b.op as u32).to_le_bytes()[0]);
            out.push((b.src_alpha as u32).to_le_bytes()[0]);
            out.push((b.dst_alpha as u32).to_le_bytes()[0]);
            out.push((b.op_alpha as u32).to_le_bytes()[0]);
        }
    }
}

/// Decodes a `"D3D9REC1"` blob into [`PipelineKey`]s, returning empty on corruption.
#[must_use]
pub fn decode_recipes(bytes: &[u8]) -> Vec<PipelineKey> {
    decode_recipes_inner(bytes).unwrap_or_else(|| {
        log::warn!(
            target: LOG_TARGET,
            "d3d9 core: ignoring corrupt shader recipe blob"
        );
        Vec::new()
    })
}

fn decode_recipes_inner(bytes: &[u8]) -> Option<Vec<PipelineKey>> {
    if bytes.len() > MAX_RECIPE_BLOB {
        return None;
    }
    let mut r = Reader { bytes, pos: 0 };
    if r.take(8)? != RECIPE_MAGIC {
        return None;
    }
    let shader_count = usize::try_from(r.u32()?).ok()?;
    let decl_count = usize::try_from(r.u32()?).ok()?;
    let recipe_count = usize::try_from(r.u32()?).ok()?;
    if recipe_count > MAX_RECIPES || shader_count > MAX_RECIPES * 2 || decl_count > MAX_RECIPES {
        return None;
    }

    let mut shaders = Vec::with_capacity(shader_count);
    for _ in 0..shader_count {
        let len = usize::try_from(r.u32()?).ok()?;
        if len > MAX_SHADER_TOKENS || r.remaining() < len.checked_mul(4)? {
            return None;
        }
        let mut toks = Vec::with_capacity(len);
        for _ in 0..len {
            toks.push(r.u32()?);
        }
        let id = ProgramId::from_tokens(&toks);
        shaders.push(ShaderCode::from_parts(id, Arc::from(toks)));
    }

    let mut decls = Vec::with_capacity(decl_count);
    for _ in 0..decl_count {
        let len = usize::try_from(r.u32()?).ok()?;
        if len > MAX_DECL_ELEMENTS || r.remaining() < len.checked_mul(8)? {
            return None;
        }
        let mut elems = Vec::with_capacity(len);
        for _ in 0..len {
            elems.push(D3DVERTEXELEMENT9 {
                stream: r.u16()?,
                offset: r.u16()?,
                type_: r.u8()?,
                method: r.u8()?,
                usage: r.u8()?,
                usage_index: r.u8()?,
            });
        }
        decls.push(DeclCode::new(Arc::from(elems)));
    }

    let mut keys = Vec::with_capacity(recipe_count);
    for _ in 0..recipe_count {
        keys.push(decode_pipeline_key(&mut r, &shaders, &decls)?);
    }
    if r.remaining() != 0 {
        return None;
    }
    Some(keys)
}

fn decode_pipeline_key(
    r: &mut Reader<'_>,
    shaders: &[ShaderCode],
    decls: &[DeclCode],
) -> Option<PipelineKey> {
    let vs = match r.u8()? {
        0 => VsStageKey::Ff(FfVsKey {
            flags: FfVsFlags::from_bits_retain(r.u16()?),
            input_tex_coord_count: r.u8()?,
            tex_coord_count: r.u8()?,
            light_active_mask: r.u8()?,
            light_directional_mask: r.u8()?,
            light_spot_mask: r.u8()?,
            diffuse_source: r.u8()?,
            ambient_source: r.u8()?,
            specular_source: r.u8()?,
            emissive_source: r.u8()?,
            fog_mode: r.u8()?,
            tci: r.array8()?,
            passthrough: r.array8()?,
            tex_coord_dims: r.array8()?,
            tt_flags: r.array8()?,
            vertex_blend_count: r.u8()?,
            declared_weights_count: r.u8()?,
            clip_plane_count: r.u8()?,
            reserved: r.u8()?,
        }),
        1 => {
            let idx = usize::try_from(r.u32()?).ok()?;
            let code = shaders.get(idx)?.clone();
            let clip_plane_count = r.u8()?;
            let volume_mask = r.u8()?;
            let cube_mask = r.u8()?;
            let lod_table = r.bool()?;
            VsStageKey::Prog {
                code,
                clip_plane_count,
                sampler_kinds: VsSamplerKinds {
                    volume_mask,
                    cube_mask,
                    lod_table,
                },
            }
        }
        _ => return None,
    };

    let ps = match r.u8()? {
        0 => {
            let mut stages = [FfStage {
                color_op: 0,
                color_arg1: 0,
                color_arg2: 0,
                alpha_op: 0,
                alpha_arg1: 0,
                alpha_arg2: 0,
                flags: FfStageFlags::empty(),
            }; 8];
            for stage in &mut stages {
                stage.color_op = r.u8()?;
                stage.color_arg1 = r.u8()?;
                stage.color_arg2 = r.u8()?;
                stage.alpha_op = r.u8()?;
                stage.alpha_arg1 = r.u8()?;
                stage.alpha_arg2 = r.u8()?;
                stage.flags = FfStageFlags::from_bits_retain(r.u8()?);
            }
            let specular_add = r.bool()?;
            let tt_projected_mask = r.u8()?;
            PsStageKey::Ff(FfPsKey {
                stages,
                specular_add,
                tt_projected_mask,
            })
        }
        1 => {
            let idx = usize::try_from(r.u32()?).ok()?;
            PsStageKey::Prog(shaders.get(idx)?.clone())
        }
        _ => return None,
    };

    let variant = VariantKey {
        alpha_func: r.u8()?,
        fog_mode: r.u8()?,
        fog_table_mode: r.u8()?,
        linked_input_mask: r.u8()?,
        depth_sampler_mask: r.u16()?,
        depth_fetch_mask: r.u16()?,
        fetch4_mask: r.u16()?,
        fetch4_alpha_mask: r.u16()?,
        raw_depth_red_mask: r.u16()?,
        volume_sampler_mask: r.u16()?,
        cube_sampler_mask: r.u16()?,
        tt_projected_mask: r.u8()?,
        color_out_mask: r.u8()?,
        sample_mask: r.u8()?,
        flags: VariantFlags::from_bits_retain(r.u8()?),
    };

    let decl_idx = usize::try_from(r.u32()?).ok()?;
    let layout = decls.get(decl_idx)?.clone();

    let mut streams = [StreamKey::Unused; MAX_STREAMS as usize];
    for stream in &mut streams {
        *stream = match r.u8()? {
            0 => StreamKey::Unused,
            1 => StreamKey::Null,
            2 => StreamKey::Inline { stride: r.u32()? },
            3 => StreamKey::Bound {
                stride: r.u32()?,
                step: VertexStepFunction::from_repr(u32::from(r.u8()?))?,
                rate: r.u32()?,
                shift: r.u8()?,
            },
            _ => return None,
        };
    }

    let write_mask = r.u8()?;
    let color_format = r.u32()?;
    let depth_format = r.u32()?;
    let blend = decode_blend(r).ok()?;
    let mut extras = [None, None, None];
    for extra in &mut extras {
        *extra = match r.u8()? {
            0 => None,
            1 => Some(CoreExtra {
                format: r.u32()?,
                write_mask: r.u8()?,
                blend: decode_blend(r).ok()?,
            }),
            _ => return None,
        };
    }
    let samples = r.u8()?;
    let alpha_to_coverage = r.bool()?;

    Some(PipelineKey {
        vs,
        ps,
        variant,
        layout,
        streams,
        write_mask,
        color_format,
        depth_format,
        blend,
        extras,
        samples,
        alpha_to_coverage,
    })
}

fn decode_blend(r: &mut Reader<'_>) -> Result<Option<CoreBlend>, ()> {
    match r.u8().ok_or(())? {
        0 => Ok(None),
        1 => Ok(Some(CoreBlend {
            src: BlendFactor::from_repr(u32::from(r.u8().ok_or(())?)).ok_or(())?,
            dst: BlendFactor::from_repr(u32::from(r.u8().ok_or(())?)).ok_or(())?,
            op: BlendOperation::from_repr(u32::from(r.u8().ok_or(())?)).ok_or(())?,
            src_alpha: BlendFactor::from_repr(u32::from(r.u8().ok_or(())?)).ok_or(())?,
            dst_alpha: BlendFactor::from_repr(u32::from(r.u8().ok_or(())?)).ok_or(())?,
            op_alpha: BlendOperation::from_repr(u32::from(r.u8().ok_or(())?)).ok_or(())?,
        })),
        _ => Err(()),
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    const fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        let slice = self.take(1)?;
        Some(slice[0])
    }

    fn bool(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn u16(&mut self) -> Option<u16> {
        let slice = self.take(2)?;
        Some(u16::from_le_bytes([slice[0], slice[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        let slice = self.take(4)?;
        Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
    }

    fn array8(&mut self) -> Option<[u8; 8]> {
        let slice = self.take(8)?;
        slice.try_into().ok()
    }
}

#[cfg(test)]
mod tests;
