//! Time DXSO parsing and MSL emission over shader corpora on the host, without Wine.
//!
//! Usage: `emit_corpus [--iters N] [--metrics DIR] [--digest] [mtld3d_shaders.bin ...]`, which
//! the runner's `bench-ab` runs per leg (`--base-host`, `--cand-host`) with the production profile.
//!
//! Two more modes build the DXSO corpus (`tests/d3d9/dxso-corpus`) and time nothing.
//! `emit_corpus --write-cache <path>` writes the synthetic programmable shaders as a shader
//! cache file: one record per shader, with the DXSO and its exact specialization, and a pipeline
//! recipe per vertex and pixel shader pair of the same shader model family (a `vs_3_0` pairs with
//! `ps_3_0` only, as D3D9 requires), over a `float4` stream of the vertex shader's declared inputs.
//! The same file holds the fixed-function keys of `synthetic_ff` and `synthetic_ff_extra` as MSL
//! records with their sources (the key and `ff_build`'s tokens), in recipes over the vertex
//! declaration each vertex key stands for, and cross-family pairs of fixed-function and
//! programmable stages.
//! `emit_corpus --merge <out> <in>...` reads cache files and writes one file holding every shader
//! record and recipe of them, once each, less the records without a source and the recipes naming
//! them, which it counts (`merge: dropped <r> sourceless records, <p> recipes`).
//! `emit_corpus --kinds <in>...` prints the record count per kind, summed over the files.
//!
//! Two synthetic corpora always run: `synthetic_ff`, about sixty fixed-function
//! vertex keys and fifty pixel keys shaped like the ones a fixed-function game
//! prewarms, and `synthetic_sm`, a handful of hand-assembled SM1/SM2/SM3 token
//! streams with the variants a draw specializes them into. A synthetic shader
//! that fails to emit fails the run, since that input is this tree's own, and
//! synthetic shaders are named by position in fixed-width names, so the MSL
//! sizes change only when the emitted code does. Each cache file named
//! on the command line adds a corpus named after its directory, holding every
//! MSL record with a source; the layer writes its fixed-function MSL records
//! without one, so a game's cache contributes only its programmable half.
//!
//! A cache that is not an mtld3d shader cache, or whose container format this
//! build does not read (a stale cache next to a game install is normal), is
//! skipped with a note and writes no metrics; a torn cache is timed up to the
//! damage, with a note. A path that cannot be read fails the run.
//!
//! A corpus is timed in passes, one pass handling every shader once, and a
//! pass's per-shader time is its wall time over the shader count. The median
//! and the minimum over the passes are reported (the median of an even pass
//! count is the mean of the two middle passes). `parse` is `dxso::parse` of the
//! tokens. What the second time covers depends on the corpus, and its metric is
//! named for it:
//!
//! - `emit` in the synthetic corpora is emission alone: the fixed-function
//!   emitters for a key, and the programmable emitters over a program parsed
//!   beforehand.
//! - `parse_emit` in a cache corpus is `ShaderSource::emit`, the call the layer
//!   makes to rebuild a cached shader, which reparses the tokens before it
//!   emits, so it includes the `parse` time reported beside it.
//!
//! Without `--iters` the pass count is chosen so each measurement runs for at
//! least 200 ms.
//!
//! `--digest` ends the run with `msl-digest <xxh3 hex> <n> shaders`: the XXH3-64 of every MSL the
//! corpora emit, in corpus then shader order, each followed by a NUL, so a change of any emitted
//! byte shows.
//!
//! `--metrics DIR` writes one `bench-host_emit_<corpus>.metrics` file per corpus
//! in the bench metrics format: `meta <bench> <key> <value...>` and
//! `metric <bench> <name> <value> <unit> <direction> <class>` lines. The meta
//! lines carry `kind host`, which tells a comparison of two builds that this
//! file names a native binary of its own rather than the layer the end-to-end
//! benchmarks load, and `host_image`, this binary's Mach-O UUID (or
//! `unknown`), beside the build stamp, arch, profile and debug assertions.
//! A cache corpus's file also carries `corpus <name>`, which tells a
//! comparison that a build unable to read that cache may lack the file.

use std::{
    env,
    fmt::{self, Write as _},
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    process::ExitCode,
    time::{Duration, Instant},
};

use mtld3d_core::{
    convert::resolve_attrs_for_ff,
    dxso::{
        self, Declaration, DxsoProgram, FfPsKey, FfStage, FfStageFlags, FfStageResult, FfVsFlags,
        FfVsKey, LinkInputs, RegKind, SemanticSet, VariantFlags, VariantKey, VsSamplerKinds,
        decl_passthrough_code, tci_entry,
    },
    ff_build,
    pipeline_state::{
        ExtraColorAttachments, PipelineAttachFlags, PipelineRsBits, PipelineSnapshot, StreamLayout,
    },
    shader_cache::{
        CACHE_FORMAT_VERSION, CacheEntry, CacheHeader, PipelineRecipe, SHADER_CACHE_SCHEMA_VERSION,
        SHADER_EMITTER_VERSION, ShaderRecordRef, ShaderSource, read_header, read_records,
        write_bundle, write_header,
    },
    shader_key::CachedKind,
};
use mtld3d_shared::{
    MetalHandle, VertexAttrDesc,
    mtl::{PixelFormat, VertexFormat, VertexStepFunction},
};
use mtld3d_types::{
    D3DCMP_GREATEREQUAL, D3DDECLMETHOD_DEFAULT, D3DDECLTYPE_D3DCOLOR, D3DDECLTYPE_FLOAT1,
    D3DDECLTYPE_FLOAT2, D3DDECLTYPE_FLOAT3, D3DDECLTYPE_FLOAT4, D3DDECLTYPE_UBYTE4,
    D3DDECLUSAGE_BINORMAL, D3DDECLUSAGE_BLENDINDICES, D3DDECLUSAGE_BLENDWEIGHT, D3DDECLUSAGE_COLOR,
    D3DDECLUSAGE_DEPTH, D3DDECLUSAGE_FOG, D3DDECLUSAGE_NORMAL, D3DDECLUSAGE_POSITION,
    D3DDECLUSAGE_POSITIONT, D3DDECLUSAGE_PSIZE, D3DDECLUSAGE_SAMPLE, D3DDECLUSAGE_TANGENT,
    D3DDECLUSAGE_TEXCOORD, D3DFOG_LINEAR, D3DMCS_COLOR1, D3DMCS_COLOR2, D3DMCS_MATERIAL,
    D3DTA_ALPHAREPLICATE, D3DTA_COMPLEMENT, D3DTA_CONSTANT, D3DTA_CURRENT, D3DTA_DIFFUSE,
    D3DTA_SPECULAR, D3DTA_TEMP, D3DTA_TEXTURE, D3DTA_TFACTOR, D3DTOP_ADD, D3DTOP_ADDSIGNED,
    D3DTOP_BLENDCURRENTALPHA, D3DTOP_BLENDTEXTUREALPHA, D3DTOP_BUMPENVMAP,
    D3DTOP_BUMPENVMAPLUMINANCE, D3DTOP_DISABLE, D3DTOP_LERP, D3DTOP_MODULATE, D3DTOP_MODULATE2X,
    D3DTOP_MODULATEALPHA_ADDCOLOR, D3DTOP_SELECTARG1, D3DTSS_ALPHAARG1, D3DTSS_ALPHAARG2,
    D3DTSS_ALPHAOP, D3DTSS_COLORARG1, D3DTSS_COLORARG2, D3DTSS_COLOROP, D3DVERTEXELEMENT9,
    MAX_STREAMS, texture_stage_state_defaults,
};
use xxhash_rust::xxh3::Xxh3;

/// Wall time a measurement runs for at least when `--iters` is not given.
const TARGET: Duration = Duration::from_millis(200);

/// Fewest passes a measurement takes when `--iters` is not given.
const MIN_PASSES: u32 = 5;

/// `D3DTSS_TCI_CAMERASPACEREFLECTIONVECTOR` as the mode half of an `FfVsKey::tci` entry.
const TCI_REFLECTION: u8 = 3;

/// `D3DTTFF_COUNT2` in the `FfVsKey::tt_flags` encoding.
const TT_COUNT2: u8 = 2;

/// `D3DTTFF_COUNT3` in the `FfVsKey::tt_flags` encoding.
const TT_COUNT3: u8 = 3;

// DXSO token fields for the hand-assembled synthetic shaders. Instruction
// opcodes and register types are the bytecode's own numbering, the same the
// emitter's unit-test fixtures spell out.
const VS_1_1: u32 = 0xFFFE_0101;
const PS_1_1: u32 = 0xFFFF_0101;
const PS_1_4: u32 = 0xFFFF_0104;
const VS_2_0: u32 = 0xFFFE_0200;
const PS_2_0: u32 = 0xFFFF_0200;
const VS_3_0: u32 = 0xFFFE_0300;
const PS_3_0: u32 = 0xFFFF_0300;
const END: u32 = 0x0000_FFFF;

const OP_MOV: u16 = 1;
const OP_ADD: u16 = 2;
const OP_MAD: u16 = 4;
const OP_MUL: u16 = 5;
const OP_DP3: u16 = 8;
const OP_DP4: u16 = 9;
const OP_MAX: u16 = 11;
const OP_LRP: u16 = 18;
const OP_DCL: u16 = 31;
const OP_POW: u16 = 32;
const OP_NRM: u16 = 36;
const OP_MOVA: u16 = 46;
const OP_TEXLD: u16 = 66;
const OP_DEF: u16 = 81;

const REG_TEMP: u32 = 0;
const REG_INPUT: u32 = 1;
const REG_CONST: u32 = 2;
/// The address register in a vertex shader, a texture coordinate in a `ps_2_0`.
const REG_ADDR_OR_TEXTURE: u32 = 3;
const REG_RASTOUT: u32 = 4;
const REG_ATTROUT: u32 = 5;
/// `oT#` in SM2, and the generic `o#` output an SM3 vertex shader declares.
const REG_OUTPUT: u32 = 6;
const REG_COLOROUT: u32 = 8;
const REG_DEPTHOUT: u32 = 9;
const REG_SAMPLER: u32 = 10;

const XYZW: u8 = 0xF;
const XYZ: u8 = 0x7;
const X: u8 = 0x1;
const Y: u8 = 0x2;
const Z: u8 = 0x4;
const W: u8 = 0x8;
const SWIZ_IDENTITY: u8 = 0xE4;
const SWIZ_X: u8 = 0x00;
const SWIZ_Y: u8 = 0x55;
const SWIZ_Z: u8 = 0xAA;
const SWIZ_W: u8 = 0xFF;
/// `dcl_2d` sampler declaration token.
const DCL_2D: u32 = 0x9000_0000;
/// Plain `dcl` token of a `ps_2_0` input register.
const DCL_PLAIN: u32 = 0x8000_0000;
/// Destination-token shift that doubles an SM1 result (`_x2`).
const SHIFT_X2: u32 = 1 << 24;
/// Source-token flag selecting relative addressing, followed by the address token.
const RELATIVE: u32 = 1 << 13;

/// One shader and the entry-point name the layer would give it.
struct Shader {
    input: Input,
    entry: String,
}

/// What an emit starts from.
enum Input {
    FfVs(FfVsKey),
    FfPs(FfPsKey, VariantKey),
    /// A synthetic programmable shader, parsed once so its emit is timed alone.
    Synthetic {
        tokens: Vec<u32>,
        program: DxsoProgram,
        specialization: Specialization,
    },
    /// A cache record, emitted through `ShaderSource::emit`, which reparses its tokens.
    Record(CacheEntry),
}

/// The draw-time inputs a synthetic programmable shader is emitted with.
enum Specialization {
    Vertex { clip_planes: u8 },
    Pixel(VariantKey),
}

impl Shader {
    // Synthetic shaders are named by `fixed_names` once their corpus is built.
    const fn ff_vs(key: FfVsKey) -> Self {
        Self {
            input: Input::FfVs(key),
            entry: String::new(),
        }
    }

    const fn ff_ps(key: FfPsKey, variant: VariantKey) -> Self {
        Self {
            input: Input::FfPs(key, variant),
            entry: String::new(),
        }
    }

    fn synthetic(tokens: Vec<u32>, specialization: Specialization) -> Self {
        let program = dxso::parse(&tokens).expect("synthetic shader parses");
        Self {
            input: Input::Synthetic {
                tokens,
                program,
                specialization,
            },
            entry: String::new(),
        }
    }

    /// A cached record, or `None` for one that keeps no emission inputs.
    fn record(entry: CacheEntry) -> Option<Self> {
        entry.source()?;
        // An AIR record holds a metallib, not MSL to time.
        entry.msl()?;
        let name = entry.kind.entry_name(entry.key);
        Some(Self {
            input: Input::Record(entry),
            entry: name,
        })
    }

    fn kind(&self) -> CachedKind {
        match &self.input {
            Input::FfVs(_) => CachedKind::FfVs,
            Input::FfPs(..) => CachedKind::FfPs,
            Input::Synthetic { tokens, .. } => {
                let header = tokens[0];
                let major = u8::try_from((header >> 8) & 0xFF).expect("major version fits u8");
                CachedKind::from_programmable(major, header >> 16 == 0xFFFF)
                    .expect("synthetic shaders are SM1 to SM3")
            }
            Input::Record(entry) => entry.kind,
        }
    }

    /// The DXSO tokens of a programmable shader.
    fn tokens(&self) -> Option<&[u32]> {
        match &self.input {
            Input::Synthetic { tokens, .. } => Some(tokens),
            Input::Record(entry) => entry.source().map(ShaderSource::tokens),
            Input::FfVs(_) | Input::FfPs(..) => None,
        }
    }

    fn emit(&self) -> Result<String, String> {
        let entry = &self.entry;
        match &self.input {
            Input::FfVs(key) => Ok(dxso::emit_vs_ff_named(key, entry)),
            Input::FfPs(key, variant) => Ok(dxso::emit_ps_ff_named(key, *variant, entry)),
            Input::Synthetic {
                program,
                specialization,
                ..
            } => match specialization {
                Specialization::Vertex { clip_planes } => dxso::emit_vs_programmable_named(
                    program,
                    entry,
                    u16::MAX,
                    *clip_planes,
                    VsSamplerKinds::default(),
                ),
                Specialization::Pixel(variant) => {
                    dxso::emit_ps_programmable_named(program, *variant, entry)
                }
            }
            .map_err(|error| format!("MSL emission: {error:?}")),
            Input::Record(record) => record
                .source()
                .ok_or_else(|| "record keeps no DXSO".to_owned())?
                .emit(entry),
        }
    }
}

/// A named set of shaders, with the ones that failed to emit already set aside.
struct Corpus {
    name: String,
    shaders: Vec<Shader>,
    failures: usize,
    msl_total: usize,
    msl_max: usize,
}

impl Corpus {
    /// Emit every shader once, keeping those that emit and counting those that fail.
    fn new(name: String, candidates: Vec<Shader>) -> Self {
        let mut corpus = Self {
            name,
            shaders: Vec::with_capacity(candidates.len()),
            failures: 0,
            msl_total: 0,
            msl_max: 0,
        };
        for shader in candidates {
            match shader.emit() {
                Ok(msl) => {
                    corpus.msl_total += msl.len();
                    corpus.msl_max = corpus.msl_max.max(msl.len());
                    corpus.shaders.push(shader);
                }
                Err(error) => {
                    if corpus.failures == 0 {
                        eprintln!("{}: {}: {error}", corpus.name, shader.entry);
                    }
                    corpus.failures += 1;
                }
            }
        }
        corpus
    }

    fn is_programmable(&self) -> bool {
        self.shaders.iter().any(|shader| shader.tokens().is_some())
    }

    /// The metric name of the second timing: whether its emit reparses the tokens.
    fn emit_metric(&self) -> &'static str {
        if self
            .shaders
            .iter()
            .any(|shader| matches!(shader.input, Input::Record(_)))
        {
            "parse_emit"
        } else {
            "emit"
        }
    }
}

/// Median and minimum per-shader time of a measurement, and how many passes it took.
struct Timing {
    median_us: f64,
    min_us: f64,
    passes: u32,
}

impl fmt::Display for Timing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:9.3} {:9.3}", self.median_us, self.min_us)
    }
}

/// The measurements of one corpus.
struct Report {
    parse: Option<Timing>,
    emit: Timing,
}

/// The command line.
struct Args {
    iters: Option<u32>,
    metrics: Option<PathBuf>,
    digest: bool,
    /// `--write-cache <path>`: write the synthetic programmable set as a cache file.
    write_cache: Option<PathBuf>,
    /// `--merge <out> <in>...`: the output and the cache files to merge into it.
    merge: Option<(PathBuf, Vec<PathBuf>)>,
    /// `--kinds <file>...`: the cache files whose records are counted per kind.
    kinds: Option<Vec<PathBuf>>,
    caches: Vec<PathBuf>,
}

/// Which build the numbers came from, written as the `meta` lines of every metrics file.
struct BuildMeta {
    layer: &'static str,
    image: String,
    arch: &'static str,
    profile: String,
}

fn main() -> ExitCode {
    let args = match parse_args(env::args().skip(1)) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            eprintln!(
                "usage: emit_corpus [--iters N] [--metrics DIR] [--digest] [mtld3d_shaders.bin ...]\n       \
                 emit_corpus --write-cache <out.bin>\n       \
                 emit_corpus --merge <out.bin> <in.bin>...\n       \
                 emit_corpus --kinds <in.bin>..."
            );
            return ExitCode::from(2);
        }
    };
    if let Some(inputs) = &args.kinds {
        return report(kinds_line(inputs));
    }
    if let Some((out, inputs)) = &args.merge {
        return report(merge_caches(out, inputs));
    }
    let mut corpora = vec![
        Corpus::new("synthetic_ff".to_owned(), synthetic_ff()),
        Corpus::new("synthetic_sm".to_owned(), synthetic_sm()),
    ];
    // The synthetic inputs are this tree's own, so one that fails to emit is a bug.
    let mut failed = corpora.iter().any(|corpus| corpus.failures > 0);
    if let Some(out) = &args.write_cache {
        if failed {
            return ExitCode::FAILURE;
        }
        return report(write_synthetic_cache(out, &corpora[1]));
    }
    for path in &args.caches {
        match load_cache(path, &corpora) {
            Ok(Some(corpus)) => corpora.push(corpus),
            Ok(None) => {}
            Err(error) => {
                eprintln!("{}: {error}", path.display());
                failed = true;
            }
        }
    }
    let meta = BuildMeta {
        layer: mtld3d_shared::identity::BUILD,
        image: mtld3d_shared::identity::image_id().unwrap_or_else(|| "unknown".to_owned()),
        arch: env::consts::ARCH,
        profile: profile(),
    };
    println!(
        "emit_corpus: layer {} arch {} profile {} emitter {SHADER_EMITTER_VERSION:016x}",
        meta.layer, meta.arch, meta.profile
    );
    println!(
        "{:<16} {:>7} {:>5} {:>9} {:>9} {:>9} {:>9} {:>11} {:>8} {:>7}  timed",
        "corpus",
        "shaders",
        "fail",
        "parse med",
        "parse min",
        "emit med",
        "emit min",
        "msl bytes",
        "msl max",
        "passes"
    );
    for corpus in &corpora {
        if corpus.shaders.is_empty() {
            println!(
                "{:<16} {:>7} {:>5}  (nothing to time)",
                corpus.name, 0, corpus.failures
            );
            continue;
        }
        let report = measure(corpus, args.iters);
        let parse = report
            .parse
            .as_ref()
            .map_or_else(|| format!("{:>9} {:>9}", "-", "-"), ToString::to_string);
        println!(
            "{:<16} {:>7} {:>5} {parse} {} {:>11} {:>8} {:>7}  {}",
            corpus.name,
            corpus.shaders.len(),
            corpus.failures,
            report.emit,
            corpus.msl_total,
            corpus.msl_max,
            report.emit.passes,
            corpus.emit_metric()
        );
        if let Some(dir) = &args.metrics
            && let Err(error) = write_metrics(dir, corpus, &report, &meta)
        {
            eprintln!("{}: {error}", dir.display());
            failed = true;
        }
    }
    println!(
        "times are microseconds per shader, median and minimum over the passes; \
         parse_emit reparses the tokens, emit does not"
    );
    if args.digest {
        println!("{}", msl_digest(&corpora));
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The `msl-digest` line: XXH3-64 over every MSL the corpora emit, each followed by a NUL.
fn msl_digest(corpora: &[Corpus]) -> String {
    let mut hasher = Xxh3::new();
    let mut count = 0_usize;
    for shader in corpora.iter().flat_map(|corpus| &corpus.shaders) {
        // A corpus keeps only shaders that emitted, and emission is deterministic.
        let msl = shader.emit().expect("a kept shader emits again");
        hasher.update(msl.as_bytes());
        hasher.update(&[0]);
        count += 1;
    }
    format!("msl-digest {:016x} {count} shaders", hasher.digest())
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut parsed = Args {
        iters: None,
        metrics: None,
        digest: false,
        write_cache: None,
        merge: None,
        kinds: None,
        caches: Vec::new(),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--iters" => {
                let value = args.next().ok_or("--iters needs a count")?;
                let iters = value
                    .parse::<u32>()
                    .ok()
                    .filter(|&n| n > 0)
                    .ok_or_else(|| format!("--iters {value}: not a positive count"))?;
                parsed.iters = Some(iters);
            }
            "--metrics" => {
                parsed.metrics = Some(args.next().ok_or("--metrics needs a directory")?.into());
            }
            "--digest" => parsed.digest = true,
            "--write-cache" => {
                parsed.write_cache = Some(args.next().ok_or("--write-cache needs a path")?.into());
            }
            "--merge" => {
                let out = args.next().ok_or("--merge needs an output path")?.into();
                let inputs: Vec<PathBuf> = args.by_ref().map(PathBuf::from).collect();
                if inputs.is_empty() {
                    return Err("--merge needs at least one input cache".to_owned());
                }
                parsed.merge = Some((out, inputs));
            }
            "--kinds" => {
                let inputs: Vec<PathBuf> = args.by_ref().map(PathBuf::from).collect();
                if inputs.is_empty() {
                    return Err("--kinds needs at least one cache".to_owned());
                }
                parsed.kinds = Some(inputs);
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => parsed.caches.push(arg.into()),
        }
    }
    Ok(parsed)
}

/// Print a mode's one-line result, or its error.
fn report(result: Result<String, String>) -> ExitCode {
    match result {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Write `shaders` and `pipelines` as the one bundle of a fresh cache file at `path`.
fn write_cache_file(
    path: &Path,
    shaders: &[CacheEntry],
    pipelines: &[PipelineRecipe],
) -> Result<String, String> {
    let mut bytes = Vec::new();
    write_header(&mut bytes);
    write_bundle(&mut bytes, shaders, pipelines);
    fs::write(path, &bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut kinds = std::collections::BTreeMap::new();
    for entry in shaders {
        *kinds.entry(format!("{:?}", entry.kind)).or_insert(0_usize) += 1;
    }
    Ok(format!(
        "{}: {} shader records {kinds:?}, {} pipeline recipes, {} bytes",
        path.display(),
        shaders.len(),
        pipelines.len(),
        bytes.len()
    ))
}

/// The cache file of the synthetic programmable shaders and their pipelines.
fn write_synthetic_cache(path: &Path, corpus: &Corpus) -> Result<String, String> {
    let (shaders, pipelines) = synthetic_records(corpus)?;
    write_cache_file(path, &shaders, &pipelines)
}

/// A cache record per synthetic programmable shader, and a recipe per compatible VS and PS pair.
fn synthetic_records(corpus: &Corpus) -> Result<(Vec<CacheEntry>, Vec<PipelineRecipe>), String> {
    let mut vertex = Vec::new();
    let mut pixel = Vec::new();
    let mut shaders = Vec::new();
    let modifier_tokens = modifier_programs();
    let modifier_parsed = modifier_tokens
        .iter()
        .map(|(tokens, _)| dxso::parse(tokens).map_err(|e| format!("modifier program: {e:?}")))
        .collect::<Result<Vec<_>, _>>()?;
    for shader in &corpus.shaders {
        let Input::Synthetic {
            tokens,
            program,
            specialization,
        } = &shader.input
        else {
            continue;
        };
        let source = match specialization {
            Specialization::Vertex { clip_planes } => {
                ShaderSource::vertex(program, u16::MAX, *clip_planes, VsSamplerKinds::default())
            }
            Specialization::Pixel(variant) => ShaderSource::pixel(program, *variant),
        };
        let kind = shader.kind();
        let key = source.disk_key();
        let msl = source.emit(&kind.entry_name(key))?;
        let reference = ShaderRecordRef::new(kind, key);
        match specialization {
            Specialization::Vertex { .. } => vertex.push((reference, program, tokens[0])),
            Specialization::Pixel(_) => pixel.push((reference, tokens[0])),
        }
        shaders.push(CacheEntry::new(kind, key, msl, Some(source)));
    }
    // The texture-address families pair with the `vs_1_1` shaders only; the depth ones (and the
    // `oDepth` programs below) with the first vertex shader of their model, which is enough.
    let mut families = Vec::new();
    for (tokens, variant, depth) in texture_family_ps() {
        let program = dxso::parse(&tokens).map_err(|e| format!("family ps: {e:?}"))?;
        let source = ShaderSource::pixel(&program, variant);
        let kind = CachedKind::from_programmable(
            u8::try_from((tokens[0] >> 8) & 0xFF).map_err(|e| e.to_string())?,
            true,
        )
        .ok_or("family ps kind")?;
        let key = source.disk_key();
        let msl = source.emit(&kind.entry_name(key))?;
        families.push((ShaderRecordRef::new(kind, key), 1_u8, depth));
        shaders.push(CacheEntry::new(kind, key, msl, Some(source)));
    }
    for tokens in depth_programs() {
        let program = dxso::parse(&tokens).map_err(|e| format!("depth ps: {e:?}"))?;
        let source = ShaderSource::pixel(&program, VariantKey::default());
        let major = u8::try_from((tokens[0] >> 8) & 0xFF).map_err(|e| e.to_string())?;
        let kind = CachedKind::from_programmable(major, true).ok_or("depth ps kind")?;
        let key = source.disk_key();
        let msl = source.emit(&kind.entry_name(key))?;
        families.push((ShaderRecordRef::new(kind, key), major, true));
        shaders.push(CacheEntry::new(kind, key, msl, Some(source)));
    }
    // The modifier programs: vertex shaders join the pairing below, pixel shaders pair with the
    // vertex shaders of their model into float targets that keep what an 8-bit one clamps.
    let mut modifier_ps = Vec::new();
    for ((tokens, specialization), program) in modifier_tokens.iter().zip(&modifier_parsed) {
        let major = u8::try_from((tokens[0] >> 8) & 0xFF).map_err(|e| e.to_string())?;
        let source = match specialization {
            Specialization::Vertex { clip_planes } => {
                ShaderSource::vertex(program, u16::MAX, *clip_planes, VsSamplerKinds::default())
            }
            Specialization::Pixel(variant) => ShaderSource::pixel(program, *variant),
        };
        let is_pixel = tokens[0] >> 16 == 0xFFFF;
        let kind = CachedKind::from_programmable(major, is_pixel).ok_or("modifier kind")?;
        let key = source.disk_key();
        let msl = source.emit(&kind.entry_name(key))?;
        let reference = ShaderRecordRef::new(kind, key);
        if is_pixel {
            modifier_ps.push((reference, major));
        } else {
            vertex.push((reference, program, tokens[0]));
        }
        shaders.push(CacheEntry::new(kind, key, msl, Some(source)));
    }
    // The `ps_1_4` depth writers pair with a `vs_1_1` that also writes `oT2` (`texm3x2depth`
    // divides by a dot product with it), so their depth varies across the target.
    let depth_tokens = depth_vs_1_1();
    let depth_program = dxso::parse(&depth_tokens).map_err(|e| format!("depth vs: {e:?}"))?;
    let depth_source = ShaderSource::vertex(&depth_program, u16::MAX, 0, VsSamplerKinds::default());
    let depth_kind = CachedKind::from_programmable(1, false).ok_or("depth vs kind")?;
    let depth_key = depth_source.disk_key();
    let depth_msl = depth_source.emit(&depth_kind.entry_name(depth_key))?;
    let depth_vs = ShaderRecordRef::new(depth_kind, depth_key);
    shaders.push(CacheEntry::new(
        depth_kind,
        depth_key,
        depth_msl,
        Some(depth_source),
    ));
    let mut pipelines = Vec::new();
    let mut depth_paired = 0_u8;
    for (vs, program, vs_header) in &vertex {
        let attrs = vertex_attrs(program);
        let stride = u32::try_from(attrs.len() * 16).map_err(|e| e.to_string())?;
        for (ps, ps_header) in &pixel {
            // D3D9 pairs a shader model 3 vertex shader with a shader model 3 pixel shader only.
            if (vs_header >> 8 & 0xFF == 3) != (ps_header >> 8 & 0xFF == 3) {
                continue;
            }
            pipelines.push(synthetic_recipe(
                *vs,
                *ps,
                stride,
                &attrs,
                PixelFormat::Bgra8Unorm,
                false,
            ));
        }
        for (ps, major) in &modifier_ps {
            if u32::from(*major) == vs_header >> 8 & 0xFF {
                pipelines.push(synthetic_recipe(
                    *vs,
                    *ps,
                    stride,
                    &attrs,
                    PixelFormat::Rgba16Float,
                    false,
                ));
            }
        }
        let major = u8::try_from(vs_header >> 8 & 0xFF).map_err(|e| e.to_string())?;
        for (ps, ps_major, depth) in &families {
            let first = depth_paired & (1 << major) == 0;
            if *ps_major == major && (major == 1 && !depth || *depth && major != 1 && first) {
                pipelines.push(synthetic_recipe(
                    *vs,
                    *ps,
                    stride,
                    &attrs,
                    PixelFormat::Bgra8Unorm,
                    *depth,
                ));
            }
        }
        depth_paired |= 1 << major;
    }
    let attrs = vertex_attrs(&depth_program);
    let stride = u32::try_from(attrs.len() * 16).map_err(|e| e.to_string())?;
    for (ps, _, depth) in families
        .iter()
        .filter(|(_, major, depth)| *depth && *major == 1)
    {
        pipelines.push(synthetic_recipe(
            depth_vs,
            *ps,
            stride,
            &attrs,
            PixelFormat::Bgra8Unorm,
            *depth,
        ));
    }
    ff_records(&vertex, &pixel, &mut shaders, &mut pipelines)?;
    // Two shapes can build the same pair.
    let mut seen = rustc_hash::FxHashSet::default();
    pipelines.retain(|recipe| seen.insert(recipe.disk_key()));
    Ok((shaders, pipelines))
}

/// The recipe of a synthetic pair: one `float4` stream, one target of `color_format`, no blending.
fn synthetic_recipe(
    vs: ShaderRecordRef,
    ps: ShaderRecordRef,
    stride: u32,
    attrs: &[VertexAttrDesc],
    color_format: PixelFormat,
    depth: bool,
) -> PipelineRecipe {
    let mut stream_layouts = [StreamLayout::UNUSED; MAX_STREAMS as usize];
    stream_layouts[0] = StreamLayout {
        stride,
        step: VertexStepFunction::PerVertex,
        step_rate: 1,
    };
    let snapshot = PipelineSnapshot {
        vs_fn: MetalHandle::NULL,
        ps_fn: MetalHandle::NULL,
        vdecl_hash: vs.key(),
        stream_layouts,
        color_format,
        attach: if depth {
            PipelineAttachFlags::HAS_DEPTH
        } else {
            PipelineAttachFlags::empty()
        } | PipelineAttachFlags::HAS_COLOR_OUTPUT
            | PipelineAttachFlags::COLOR_HAS_ALPHA,
        rs: PipelineRsBits {
            color_write_mask: 0x0F,
            ..PipelineRsBits::default()
        },
        extra: ExtraColorAttachments::NONE,
        ps_color_out_mask: 1,
        sample_count: 1,
    };
    PipelineRecipe::from_snapshot(vs, ps, &snapshot, attrs)
}

/// One tightly packed `float4` per input register the vertex shader declares, in one stream.
fn vertex_attrs(program: &DxsoProgram) -> Vec<VertexAttrDesc> {
    let mut registers: Vec<u32> = program
        .declarations
        .iter()
        .filter_map(|decl| match decl {
            Declaration::Semantic { reg, .. } if reg.kind == RegKind::Input => {
                Some(u32::from(reg.index))
            }
            _ => None,
        })
        .collect();
    registers.sort_unstable();
    registers.dedup();
    registers
        .into_iter()
        .zip(0_u32..)
        .map(|(attr_index, slot)| VertexAttrDesc {
            attr_index,
            buffer_index: 0,
            offset: slot * 16,
            format: VertexFormat::Float4,
        })
        .collect()
}

/// One fixed-function vertex record of `key`: its MSL, and its source (`ff_build`'s tokens).
fn ff_vs_record(key: &FfVsKey) -> Result<(ShaderRecordRef, CacheEntry), String> {
    let source = ShaderSource::ff_vertex(key, &ff_build::vertex(key));
    let (kind, disk_key) = (CachedKind::FfVs, source.disk_key());
    let msl = source.emit(&kind.entry_name(disk_key))?;
    Ok((
        ShaderRecordRef::new(kind, disk_key),
        CacheEntry::new(kind, disk_key, msl, Some(source)),
    ))
}

/// One fixed-function pixel record of `key` under `variant`, as [`ff_vs_record`].
fn ff_ps_record(
    key: &FfPsKey,
    variant: VariantKey,
) -> Result<(ShaderRecordRef, CacheEntry), String> {
    let source = ShaderSource::ff_pixel(key, variant, &ff_build::pixel(key));
    let (kind, disk_key) = (CachedKind::FfPs, source.disk_key());
    let msl = source.emit(&kind.entry_name(disk_key))?;
    Ok((
        ShaderRecordRef::new(kind, disk_key),
        CacheEntry::new(kind, disk_key, msl, Some(source)),
    ))
}

/// The vertex declaration a game would draw `key` with: one stream, tightly packed.
///
/// Position (`POSITIONT` float4 when pre-transformed), normal float3, the two colours as
/// `D3DCOLOR`, each texture set at its dimension, the declared blend weights and `UBYTE4`
/// indices, the point size, and a float4 per passthrough entry.
fn ff_elements(key: &FfVsKey) -> Vec<D3DVERTEXELEMENT9> {
    let mut elements = Vec::new();
    let mut offset = 0_u16;
    let mut push = |type_: u8, usage: u8, usage_index: u8| {
        elements.push(D3DVERTEXELEMENT9 {
            stream: 0,
            offset,
            type_,
            method: D3DDECLMETHOD_DEFAULT,
            usage,
            usage_index,
        });
        offset += match type_ {
            D3DDECLTYPE_FLOAT1 | D3DDECLTYPE_D3DCOLOR | D3DDECLTYPE_UBYTE4 => 4,
            D3DDECLTYPE_FLOAT2 => 8,
            D3DDECLTYPE_FLOAT3 => 12,
            _ => 16,
        };
    };
    let float = |n: u8| match n {
        1 => D3DDECLTYPE_FLOAT1,
        2 | 0 => D3DDECLTYPE_FLOAT2,
        3 => D3DDECLTYPE_FLOAT3,
        _ => D3DDECLTYPE_FLOAT4,
    };
    if key.has_rhw() {
        push(D3DDECLTYPE_FLOAT4, D3DDECLUSAGE_POSITIONT, 0);
    } else {
        push(D3DDECLTYPE_FLOAT3, D3DDECLUSAGE_POSITION, 0);
    }
    if key.has_normal() {
        push(D3DDECLTYPE_FLOAT3, D3DDECLUSAGE_NORMAL, 0);
    }
    if key.has_color0() {
        push(D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR, 0);
    }
    if key.has_color1() {
        push(D3DDECLTYPE_D3DCOLOR, D3DDECLUSAGE_COLOR, 1);
    }
    for set in 0..key.input_tex_coord_count.min(8) {
        push(
            float(key.tex_coord_dims[usize::from(set)]),
            D3DDECLUSAGE_TEXCOORD,
            set,
        );
    }
    if key.vertex_blend_count > 0 && key.declared_weights_count > 0 {
        push(
            float(key.declared_weights_count),
            D3DDECLUSAGE_BLENDWEIGHT,
            0,
        );
    }
    if key.declared_indices() {
        push(D3DDECLTYPE_UBYTE4, D3DDECLUSAGE_BLENDINDICES, 0);
    }
    if key.has_psize() {
        push(D3DDECLTYPE_FLOAT1, D3DDECLUSAGE_PSIZE, 0);
    }
    for code in key.passthrough.iter().take_while(|code| **code != 0) {
        let named = (0..=D3DDECLUSAGE_SAMPLE)
            .flat_map(|usage| (0..16).map(move |index| (usage, index)))
            .find(|(usage, index)| decl_passthrough_code(*usage, *index) == Some(*code));
        if let Some((usage, index)) = named {
            push(D3DDECLTYPE_FLOAT4, usage, index);
        }
    }
    elements
}

/// The stride and attributes of `key`'s declaration, resolved as the layer resolves them.
fn ff_attrs(key: &FfVsKey, provided_input_mask: u16) -> Result<(u32, Vec<VertexAttrDesc>), String> {
    let resolved = resolve_attrs_for_ff(&ff_elements(key), &key.passthrough);
    let fed = resolved
        .attrs
        .iter()
        .fold(0_u16, |mask, attr| mask | 1 << attr.attr_index.min(15));
    if provided_input_mask & !fed != 0 {
        return Err(format!(
            "{key:?}: the declaration feeds {fed:#06x}, the program reads {provided_input_mask:#06x}"
        ));
    }
    Ok((resolved.extents[0], resolved.attrs))
}

/// The pixel key that shows every output of the vertex key `vs`.
///
/// Stage 0 modulates its texture by the diffuse colour, each further stage samples the next
/// coordinate (`ADDSIGNED` and `MODULATE2X` by turns), the specular colour is added, the vertex fog
/// applied, and a stage the vertex key projects is projected here too.
fn carrier_ps(vs: &FfVsKey) -> (FfPsKey, VariantKey) {
    let stages = vs.tex_coord_count.min(8);
    let mut cascade = vec![[
        D3DTOP_SELECTARG1,
        D3DTA_DIFFUSE,
        D3DTA_CURRENT,
        D3DTOP_SELECTARG1,
        D3DTA_DIFFUSE,
        D3DTA_CURRENT,
    ]];
    if stages > 0 {
        cascade.clear();
    }
    for stage in 0..stages {
        let (op, arg2) = match stage {
            0 => (D3DTOP_MODULATE, D3DTA_DIFFUSE),
            s if s % 2 == 1 => (D3DTOP_ADDSIGNED, D3DTA_CURRENT),
            _ => (D3DTOP_MODULATE2X, D3DTA_CURRENT),
        };
        cascade.push([
            op,
            D3DTA_TEXTURE,
            arg2,
            D3DTOP_MODULATE,
            D3DTA_TEXTURE,
            arg2,
        ]);
    }
    let mut key = ps_key(&cascade, true);
    let projected = (0..usize::from(stages))
        .filter(|stage| vs.tt_flags[*stage] & TT_PROJECTED != 0)
        .fold(0_u8, |mask, stage| mask | 1 << stage);
    key.tt_projected_mask = projected;
    let variant = VariantKey {
        fog_mode: narrow(D3DFOG_LINEAR),
        tt_projected_mask: projected,
        ..VariantKey::default()
    };
    (key, variant)
}

/// The vertex key every fixed-function pixel key is drawn with.
///
/// Both colours, vertex fog and eight four-component coordinate sets, so each stage samples a
/// coordinate of its own.
fn carrier_vs() -> FfVsKey {
    let mut key = bare_vs_key(FfVsFlags::HAS_COLOR0 | FfVsFlags::HAS_COLOR1, 8, 8);
    key.tex_coord_dims = [4; 8];
    key.fog_mode = narrow(D3DFOG_LINEAR);
    key
}

/// [`carrier_vs`] with every coordinate through a four-component projected texture matrix.
///
/// The parity inputs give stages 0 and 1 a matrix whose `w` column is zero, so a projected pixel
/// stage behind it divides by zero everywhere and reads the origin.
fn w_zero_vs() -> FfVsKey {
    let mut key = carrier_vs();
    key.tt_flags = [4 | TT_PROJECTED; 8];
    key
}

/// The lit vertex key the cross-family pairs put in front of the programmable pixel shaders.
fn cross_vs() -> FfVsKey {
    let flags = FfVsFlags::HAS_NORMAL
        | FfVsFlags::LIGHTING_ENABLED
        | FfVsFlags::COLOR_VERTEX
        | FfVsFlags::SPECULAR_ENABLE
        | FfVsFlags::LOCAL_VIEWER;
    let mut key = with_lights(bare_vs_key(flags, 4, 4), 0b11, 0b01, 0);
    key.tex_coord_dims[..4].copy_from_slice(&[2, 2, 3, 4]);
    key.fog_mode = narrow(D3DFOG_LINEAR);
    key
}

/// `ps_3_0` reading the eight passthrough extras of [`extra_vs_keys`]' pre-transformed key.
///
/// Normal, tangent, binormal, colour 2, fog, position 1, blend weight and depth, each scaled by a
/// constant row and summed, so each extra shows in the float target it renders into.
fn passthrough_ps() -> Vec<u32> {
    let mut out = vec![PS_3_0];
    let extras = [
        (D3DDECLUSAGE_NORMAL, 0),
        (D3DDECLUSAGE_TANGENT, 0),
        (D3DDECLUSAGE_BINORMAL, 0),
        (D3DDECLUSAGE_COLOR, 2),
        (D3DDECLUSAGE_FOG, 0),
        (D3DDECLUSAGE_POSITION, 1),
        (D3DDECLUSAGE_BLENDWEIGHT, 0),
        (D3DDECLUSAGE_DEPTH, 0),
    ];
    for (input, (decl_usage, index)) in (0_u16..).zip(extras) {
        dcl(&mut out, usage(decl_usage, index), REG_INPUT, input, XYZW);
    }
    alu2(
        &mut out,
        OP_MUL,
        dst(REG_TEMP, 0, XYZW),
        src(REG_INPUT, 0, SWIZ_IDENTITY),
        src(REG_CONST, 0, SWIZ_IDENTITY),
    );
    for input in 1..8 {
        alu3(
            &mut out,
            OP_MAD,
            dst(REG_TEMP, 0, XYZW),
            [
                src(REG_INPUT, input, SWIZ_IDENTITY),
                src(REG_CONST, input, SWIZ_IDENTITY),
                src(REG_TEMP, 0, SWIZ_IDENTITY),
            ],
        );
    }
    out.extend([
        op(OP_MOV, 2),
        dst(REG_COLOROUT, 0, XYZW),
        src(REG_TEMP, 0, SWIZ_IDENTITY),
        END,
    ]);
    out
}

/// The fixed-function records of the corpus file and the recipes drawing them.
///
/// Every key of `synthetic_ff()` and [`synthetic_ff_extra`] is one MSL record with its source:
/// each vertex key drawn once with its [`carrier_ps`], each pixel key once behind
/// [`carrier_vs`], and a projecting pixel key once more behind [`w_zero_vs`]. Then the
/// cross-family pairs: [`cross_vs`] in front of every programmable pixel
/// shader of `pixel`, the first vertex shader of each model in `vertex` in front of a
/// fixed-function pixel key, and the pre-transformed passthrough key in front of a `ps_3_0`
/// reading its extras. A recipe with a fixed-function vertex stage has a depth attachment, so
/// the pre-transformed depth clamp and bias show.
fn ff_records(
    vertex: &[(ShaderRecordRef, &DxsoProgram, u32)],
    pixel: &[(ShaderRecordRef, u32)],
    shaders: &mut Vec<CacheEntry>,
    pipelines: &mut Vec<PipelineRecipe>,
) -> Result<(), String> {
    let mut seen = rustc_hash::FxHashSet::default();
    let mut keep = |reference: ShaderRecordRef, entry: CacheEntry| {
        if seen.insert((reference.kind() as u8, reference.key())) {
            shaders.push(entry);
        }
        reference
    };
    let mut vs_keys = Vec::new();
    let mut ps_keys = Vec::new();
    for shader in synthetic_ff().into_iter().chain(synthetic_ff_extra()) {
        match shader.input {
            Input::FfVs(key) => vs_keys.push(key),
            Input::FfPs(key, variant) => ps_keys.push((key, variant)),
            Input::Synthetic { .. } | Input::Record(_) => {}
        }
    }
    let mut build_vs =
        |key: &FfVsKey| -> Result<(ShaderRecordRef, u32, Vec<VertexAttrDesc>), String> {
            let program = ff_build::vertex(key);
            let (stride, attrs) = ff_attrs(key, program.provided_input_mask)?;
            let (reference, entry) = ff_vs_record(key)?;
            Ok((keep(reference, entry), stride, attrs))
        };
    let mut draws = Vec::new();
    for key in &vs_keys {
        let (vs, stride, attrs) = build_vs(key)?;
        let (ps_key, variant) = carrier_ps(key);
        let (ps, entry) = ff_ps_record(&ps_key, variant)?;
        draws.push((vs, stride, attrs, ps, entry, PixelFormat::Bgra8Unorm));
    }
    let carrier = build_vs(&carrier_vs())?;
    let w_zero = build_vs(&w_zero_vs())?;
    for (key, variant) in &ps_keys {
        let (ps, entry) = ff_ps_record(key, *variant)?;
        if key.tt_projected_mask != 0 {
            let (vs, stride, attrs) = &w_zero;
            draws.push((
                *vs,
                *stride,
                attrs.clone(),
                ps,
                entry.clone(),
                PixelFormat::Bgra8Unorm,
            ));
        }
        draws.push((
            carrier.0,
            carrier.1,
            carrier.2.clone(),
            ps,
            entry,
            PixelFormat::Bgra8Unorm,
        ));
    }
    let cross = build_vs(&cross_vs())?;
    let passthrough = vs_keys
        .iter()
        .find(|key| key.has_rhw() && key.passthrough[7] != 0 && key.fog_mode == 0)
        .ok_or("no pre-transformed passthrough key")?;
    let (pass_vs, pass_stride, pass_attrs) = build_vs(passthrough)?;
    for (vs, stride, attrs, ps, entry, format) in draws {
        let ps = keep(ps, entry);
        pipelines.push(synthetic_recipe(vs, ps, stride, &attrs, format, true));
    }
    for (ps, _) in pixel {
        pipelines.push(synthetic_recipe(
            cross.0,
            *ps,
            cross.1,
            &cross.2,
            PixelFormat::Bgra8Unorm,
            true,
        ));
    }
    let (two_stage, variant) = carrier_ps(&bare_vs_key(FfVsFlags::HAS_COLOR0, 2, 2));
    let (reference, entry) = ff_ps_record(&two_stage, variant)?;
    let two_stage_ps = keep(reference, entry);
    let mut models = 0_u8;
    for (vs, program, header) in vertex {
        let major = u8::try_from(header >> 8 & 0xFF).map_err(|e| e.to_string())?;
        if models & 1 << major != 0 {
            continue;
        }
        models |= 1 << major;
        let attrs = vertex_attrs(program);
        let stride = u32::try_from(attrs.len() * 16).map_err(|e| e.to_string())?;
        pipelines.push(synthetic_recipe(
            *vs,
            two_stage_ps,
            stride,
            &attrs,
            PixelFormat::Bgra8Unorm,
            false,
        ));
    }
    let tokens = passthrough_ps();
    let program = dxso::parse(&tokens).map_err(|e| format!("passthrough ps: {e:?}"))?;
    let (inputs, _) = LinkInputs::ps_inputs(&program);
    let variant = VariantKey {
        linked_input_mask: inputs
            .mask_against(&SemanticSet::passthrough_outputs(&passthrough.passthrough)),
        ..VariantKey::default()
    };
    let source = ShaderSource::pixel(&program, variant);
    let (kind, key) = (CachedKind::Sm3Ps, source.disk_key());
    let msl = source.emit(&kind.entry_name(key))?;
    let ps = keep(
        ShaderRecordRef::new(kind, key),
        CacheEntry::new(kind, key, msl, Some(source)),
    );
    pipelines.push(synthetic_recipe(
        pass_vs,
        ps,
        pass_stride,
        &pass_attrs,
        PixelFormat::Rgba16Float,
        true,
    ));
    Ok(())
}

/// `D3DTSS_TCI_*` modes as the high nibble of an `FfVsKey::tci` entry.
const TCI_PASSTHRU: u8 = 0;
const TCI_NORMAL: u8 = 1;
const TCI_POSITION: u8 = 2;
const TCI_SPHEREMAP: u8 = 4;
/// A mode past SPHEREMAP, undefined.
const TCI_UNDEFINED: u8 = 5;
/// `FfVsKey::tt_flags`: the count in the low three bits, `D3DTTFF_PROJECTED` at bit 4.
const TT_PROJECTED: u8 = 0x10;

/// The further fixed-function shapes the corpus file carries, beyond `synthetic_ff()`.
///
/// `ff_build`'s test lists `extra_vs_keys` and `extra_ps_keys` restated (an example cannot call
/// a unit test's functions): clip planes, blending, pre-transformed passthrough, point size,
/// spot lights, every TCI mode, texture transforms, vertex fog, material sources; every colour
/// and alpha op, TEMP, bump environment stages, the specular add, depth, cube and volume slots,
/// the LOD bias, raw and projected depth. One shape more than those lists: a projected stage after
/// an untextured luminance bump. Not in `--digest` or the timed corpora.
fn synthetic_ff_extra() -> Vec<Shader> {
    let mut shaders: Vec<Shader> = extra_vs_keys().into_iter().map(Shader::ff_vs).collect();
    let mut pixel = extra_ps_keys();
    let mut after_bump = ps_key(
        &[
            [
                D3DTOP_BUMPENVMAPLUMINANCE,
                D3DTA_DIFFUSE,
                D3DTA_CURRENT,
                D3DTOP_SELECTARG1,
                D3DTA_DIFFUSE,
                D3DTA_CURRENT,
            ],
            [
                D3DTOP_MODULATE,
                D3DTA_TEXTURE,
                D3DTA_DIFFUSE,
                D3DTOP_MODULATE,
                D3DTA_TEXTURE,
                D3DTA_DIFFUSE,
            ],
        ],
        false,
    );
    after_bump.tt_projected_mask = 0b10;
    pixel.push((
        after_bump,
        VariantKey {
            tt_projected_mask: 0b10,
            ..VariantKey::default()
        },
    ));
    shaders.extend(
        pixel
            .into_iter()
            .map(|(key, variant)| Shader::ff_ps(key, variant)),
    );
    shaders
}
/// A key with `flags`, `sets` two-component coordinate sets, `stages` stages each reading its own.
fn bare_vs_key(flags: FfVsFlags, sets: u8, stages: u8) -> FfVsKey {
    let mut k = FfVsKey {
        flags,
        input_tex_coord_count: sets,
        tex_coord_count: stages,
        light_active_mask: 0,
        light_directional_mask: 0,
        light_spot_mask: 0,
        diffuse_source: 0,
        ambient_source: 0,
        specular_source: 0,
        emissive_source: 0,
        fog_mode: 0,
        tci: core::array::from_fn(|s| tci_entry(TCI_PASSTHRU, u8::try_from(s).unwrap_or(0))),
        passthrough: [0; 8],
        tex_coord_dims: [0; 8],
        tt_flags: [0; 8],
        vertex_blend_count: 0,
        declared_weights_count: 0,
        clip_plane_count: 0,
        reserved: 0,
    };
    k.tex_coord_dims[..usize::from(sets)].fill(2);
    k
}

const fn with_lights(mut k: FfVsKey, active: u8, directional: u8, spot: u8) -> FfVsKey {
    k.light_active_mask = active;
    k.light_directional_mask = directional;
    k.light_spot_mask = spot;
    k
}

/// The further vertex shapes of the AIR corpus, and each builder warning's key.
fn extra_vs_keys() -> Vec<FfVsKey> {
    let lit = FfVsFlags::HAS_NORMAL | FfVsFlags::LIGHTING_ENABLED | FfVsFlags::COLOR_VERTEX;
    let specular = lit | FfVsFlags::SPECULAR_ENABLE | FfVsFlags::LOCAL_VIEWER;
    let unlit = FfVsFlags::HAS_COLOR0;
    let rhw = FfVsFlags::HAS_RHW | FfVsFlags::HAS_COLOR0;
    let mut keys = Vec::new();

    // User clip planes, lit and unlit.
    for planes in [1, 3, 6] {
        let mut k = with_lights(bare_vs_key(lit, 1, 1), 0b1, 0b1, 0);
        k.clip_plane_count = planes;
        keys.push(k);
        let mut k = bare_vs_key(unlit, 1, 1);
        k.clip_plane_count = planes;
        keys.push(k);
    }

    // Blending: sequential and indexed, one to four matrices, with the normal through the bones.
    for count in 1..=4u8 {
        for indexed in [false, true] {
            for normal in [false, true] {
                let mut flags = if normal { lit } else { unlit };
                if indexed {
                    flags |= FfVsFlags::VERTEX_BLEND_INDEXED | FfVsFlags::DECLARED_INDICES;
                }
                let mut k = bare_vs_key(flags, 1, 1);
                if normal {
                    k = with_lights(k, 0b11, 0b01, 0);
                }
                k.vertex_blend_count = count;
                k.declared_weights_count = count - 1;
                keys.push(k);
            }
        }
    }
    // Indices declared, the render state sequential; a texgen-only normal through the bones.
    let mut k = bare_vs_key(unlit | FfVsFlags::DECLARED_INDICES, 1, 1);
    k.vertex_blend_count = 3;
    k.declared_weights_count = 2;
    keys.push(k);
    let mut k = bare_vs_key(FfVsFlags::HAS_NORMAL | FfVsFlags::NORMALIZE_NORMALS, 1, 2);
    k.tci[1] = tci_entry(TCI_REFLECTION, 0);
    k.vertex_blend_count = 2;
    k.declared_weights_count = 1;
    keys.push(k);

    // Pre-transformed: passthrough extras (FOG0 under fog mode 4 and not), PSIZE, every dim.
    let pass = |usage: u8, index: u8| decl_passthrough_code(usage, index).unwrap_or(0);
    let extras = [
        pass(D3DDECLUSAGE_NORMAL, 0),
        pass(D3DDECLUSAGE_TANGENT, 0),
        pass(D3DDECLUSAGE_BINORMAL, 0),
        pass(D3DDECLUSAGE_COLOR, 2),
        pass(D3DDECLUSAGE_FOG, 0),
        pass(D3DDECLUSAGE_POSITION, 1),
        pass(D3DDECLUSAGE_BLENDWEIGHT, 0),
        pass(D3DDECLUSAGE_DEPTH, 0),
    ];
    for fog_mode in [0, 4] {
        let mut k = bare_vs_key(rhw | FfVsFlags::HAS_COLOR1 | FfVsFlags::HAS_PSIZE, 4, 8);
        k.tex_coord_dims[..4].copy_from_slice(&[1, 2, 3, 4]);
        k.passthrough = extras;
        k.fog_mode = fog_mode;
        keys.push(k);
    }
    let mut k = bare_vs_key(rhw, 1, 1);
    k.passthrough[..2].copy_from_slice(&extras[..2]);
    keys.push(k);
    // Fog mode 4 without a specular colour, pre-transformed and not.
    let mut k = bare_vs_key(rhw, 1, 1);
    k.fog_mode = 4;
    keys.push(k);
    let mut k = bare_vs_key(unlit, 1, 1);
    k.fog_mode = 4;
    keys.push(k);

    // Point size: per vertex or the render state, scaled or not; pre-transformed with PSIZE.
    for flags in [
        unlit | FfVsFlags::POINT_SCALE,
        unlit | FfVsFlags::POINT_SCALE | FfVsFlags::HAS_PSIZE,
        unlit | FfVsFlags::HAS_PSIZE,
        rhw | FfVsFlags::HAS_PSIZE,
    ] {
        keys.push(bare_vs_key(flags, 1, 1));
    }

    // Spot lights beside directional and point ones; a spot light with no normal.
    keys.push(with_lights(
        bare_vs_key(specular, 1, 1),
        0b111,
        0b001,
        0b100,
    ));
    keys.push(with_lights(bare_vs_key(lit, 1, 1), 0b11, 0, 0b11));
    keys.push(with_lights(
        bare_vs_key(
            FfVsFlags::LIGHTING_ENABLED | unlit | FfVsFlags::HAS_COLOR1,
            1,
            1,
        ),
        0b11,
        0,
        0b10,
    ));
    // Specular with an infinite viewer, lit with a specular colour but no specular.
    keys.push(with_lights(
        bare_vs_key(lit | FfVsFlags::SPECULAR_ENABLE, 1, 1),
        0b1,
        0b1,
        0,
    ));
    keys.push(with_lights(
        bare_vs_key(lit | FfVsFlags::HAS_COLOR1, 1, 1),
        0b1,
        0b1,
        0,
    ));

    // Every TCI mode, lit and unlit, with and without a normal, and through a texture matrix.
    for mode in [
        TCI_PASSTHRU,
        TCI_NORMAL,
        TCI_POSITION,
        TCI_REFLECTION,
        TCI_SPHEREMAP,
        TCI_UNDEFINED,
    ] {
        for flags in [lit, FfVsFlags::HAS_NORMAL, unlit] {
            for count in [0, 3] {
                let mut k = bare_vs_key(flags, 1, 2);
                if flags.contains(FfVsFlags::LIGHTING_ENABLED) {
                    k = with_lights(k, 0b1, 0b1, 0);
                }
                k.tci[1] = tci_entry(mode, 0);
                k.tt_flags[1] = count;
                keys.push(k);
            }
        }
    }

    // Texture transforms: counts 1-7, projected or not, over every input dimension.
    for count in 1..=7u8 {
        for projected in [0, TT_PROJECTED] {
            let mut k = bare_vs_key(unlit, 4, 4);
            k.tex_coord_dims[..4].copy_from_slice(&[1, 2, 3, 4]);
            k.tt_flags[..4].fill(count | projected);
            keys.push(k);
        }
    }
    let mut k = bare_vs_key(unlit, 4, 4);
    k.tex_coord_dims[..4].copy_from_slice(&[1, 2, 3, 4]);
    k.tt_flags[..4].fill(TT_PROJECTED);
    keys.push(k);

    // Vertex fog: every mode, by depth and by range.
    for fog_mode in 1..=4 {
        for range in [false, true] {
            let mut flags = lit | FfVsFlags::HAS_COLOR1;
            flags.set(FfVsFlags::RANGE_FOG, range && fog_mode < 4);
            let mut k = with_lights(bare_vs_key(flags, 1, 1), 0b1, 0b1, 0);
            k.fog_mode = fog_mode;
            keys.push(k);
        }
    }

    // Material sources from the vertex colours, with and without them.
    for (diffuse, colors) in [
        (D3DMCS_COLOR1, FfVsFlags::HAS_COLOR0),
        (D3DMCS_COLOR2, FfVsFlags::HAS_COLOR1),
        (D3DMCS_COLOR1, FfVsFlags::empty()),
        (D3DMCS_COLOR2, FfVsFlags::HAS_COLOR0 | FfVsFlags::HAS_COLOR1),
    ] {
        let mut k = with_lights(bare_vs_key(specular | colors, 1, 1), 0b11, 0b01, 0);
        k.diffuse_source = narrow(diffuse);
        k.ambient_source = narrow(D3DMCS_COLOR2);
        k.specular_source = narrow(D3DMCS_COLOR1);
        k.emissive_source = narrow(D3DMCS_COLOR2);
        keys.push(k);
    }
    // Lit without a normal (ambient and emissive alone), with a light.
    keys.push(with_lights(
        bare_vs_key(FfVsFlags::LIGHTING_ENABLED | unlit, 1, 1),
        0b1,
        0b1,
        0,
    ));
    // Normalized normals.
    keys.push(with_lights(
        bare_vs_key(lit | FfVsFlags::NORMALIZE_NORMALS, 1, 1),
        0b1,
        0,
        0,
    ));

    // The warnings: an absent set, a material source past COLOR2, texgen on a pre-transformed
    // stage, an undefined mode (the last two in the TCI loop above too).
    let mut k = bare_vs_key(unlit, 1, 2);
    k.tci[1] = tci_entry(TCI_PASSTHRU, 3);
    keys.push(k);
    let mut k = with_lights(bare_vs_key(lit, 1, 1), 0b1, 0b1, 0);
    k.ambient_source = 3;
    keys.push(k);
    let mut k = bare_vs_key(rhw, 1, 2);
    k.tci[1] = tci_entry(TCI_POSITION, 0);
    keys.push(k);
    let mut k = bare_vs_key(rhw, 1, 2);
    k.tci[1] = tci_entry(TCI_PASSTHRU, 5);
    keys.push(k);

    keys.push(worst_case_vs_key());
    keys
}

/// The key holding the most at once.
///
/// Four indexed bones, eight spot lights with specular, eight texgen stages, point scale, six
/// clip planes and range fog.
fn worst_case_vs_key() -> FfVsKey {
    let flags = FfVsFlags::HAS_NORMAL
        | FfVsFlags::LIGHTING_ENABLED
        | FfVsFlags::COLOR_VERTEX
        | FfVsFlags::HAS_COLOR0
        | FfVsFlags::HAS_COLOR1
        | FfVsFlags::SPECULAR_ENABLE
        | FfVsFlags::LOCAL_VIEWER
        | FfVsFlags::NORMALIZE_NORMALS
        | FfVsFlags::VERTEX_BLEND_INDEXED
        | FfVsFlags::DECLARED_INDICES
        | FfVsFlags::POINT_SCALE
        | FfVsFlags::HAS_PSIZE
        | FfVsFlags::RANGE_FOG;
    let mut k = with_lights(bare_vs_key(flags, 8, 8), 0xFF, 0, 0xFF);
    let modes = [
        TCI_NORMAL,
        TCI_POSITION,
        TCI_REFLECTION,
        TCI_SPHEREMAP,
        TCI_PASSTHRU,
        TCI_SPHEREMAP,
        TCI_REFLECTION,
        TCI_NORMAL,
    ];
    for (stage, mode) in modes.into_iter().enumerate() {
        k.tci[stage] = tci_entry(mode, u8::try_from(stage).unwrap_or(0));
        k.tt_flags[stage] = 4 | TT_PROJECTED;
    }
    k.tex_coord_dims = [1, 2, 3, 4, 1, 2, 3, 4];
    k.diffuse_source = narrow(D3DMCS_COLOR1);
    k.specular_source = narrow(D3DMCS_COLOR2);
    k.vertex_blend_count = 4;
    k.declared_weights_count = 3;
    k.clip_plane_count = 6;
    k.fog_mode = 2;
    k
}

/// The further pixel shapes of the AIR corpus.
///
/// Every colour and alpha op over texture, diffuse and the modifiers; TEMP results; bump
/// environment stages with and without a texture, projected and chained; the specular add;
/// stages sampling depth, cube and volume slots, under a LOD bias, raw depth and projection.
fn extra_ps_keys() -> Vec<(FfPsKey, VariantKey)> {
    let plain = VariantKey::default();
    let mut keys = Vec::new();
    let tex_diffuse = |op: u32| {
        [
            op,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
            op,
            D3DTA_TEXTURE,
            D3DTA_DIFFUSE,
        ]
    };
    // Every op (DISABLE aside) as stage 0's colour and alpha op, then over CURRENT as stage 1.
    for op in D3DTOP_SELECTARG1..=D3DTOP_LERP {
        keys.push((ps_key(&[tex_diffuse(op)], false), plain));
        let second = [
            op,
            D3DTA_TEXTURE | D3DTA_COMPLEMENT,
            D3DTA_CURRENT | D3DTA_ALPHAREPLICATE,
            op,
            D3DTA_SPECULAR,
            D3DTA_TFACTOR,
        ];
        keys.push((
            ps_key(&[tex_diffuse(D3DTOP_MODULATE), second], false),
            plain,
        ));
        // Modifiers on both sides of the colour and the alpha op.
        let modified = [
            op,
            D3DTA_TEXTURE | D3DTA_ALPHAREPLICATE,
            D3DTA_DIFFUSE | D3DTA_COMPLEMENT | D3DTA_ALPHAREPLICATE,
            op,
            D3DTA_TEXTURE | D3DTA_COMPLEMENT,
            D3DTA_CURRENT | D3DTA_COMPLEMENT,
        ];
        keys.push((ps_key(&[modified], false), plain));
        // The same op without a texture: the unbound fallback or the implicit-zero-alpha arm.
        let untextured = [
            op,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
            op,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
        ];
        keys.push((
            ps_key(&[tex_diffuse(D3DTOP_MODULATE), untextured], false),
            plain,
        ));
    }
    // A kept alpha (DISABLE, a bump op in the alpha role), TEMP's alpha kept at zero.
    for alpha_op in [
        D3DTOP_DISABLE,
        D3DTOP_BUMPENVMAP,
        D3DTOP_BUMPENVMAPLUMINANCE,
    ] {
        let mut k = ps_key(
            &[
                [
                    D3DTOP_MODULATE,
                    D3DTA_TEXTURE,
                    D3DTA_DIFFUSE,
                    alpha_op,
                    D3DTA_TEXTURE,
                    D3DTA_DIFFUSE,
                ],
                [
                    D3DTOP_ADD,
                    D3DTA_TEMP,
                    D3DTA_CURRENT,
                    D3DTOP_SELECTARG1,
                    D3DTA_TEMP,
                    D3DTA_CURRENT,
                ],
            ],
            false,
        );
        keys.push((k.clone(), plain));
        k.stages[0].set_result(FfStageResult::Temp);
        keys.push((k, plain));
    }
    // Arguments: constant, temp, specular, factor; a stage writing TEMP read by the next.
    let mut k = ps_key(
        &[
            [
                D3DTOP_MODULATE,
                D3DTA_TEXTURE,
                D3DTA_CONSTANT,
                D3DTOP_SELECTARG1,
                D3DTA_TFACTOR,
                D3DTA_CURRENT,
            ],
            [
                D3DTOP_ADD,
                D3DTA_TEMP,
                D3DTA_SPECULAR,
                D3DTOP_MODULATE,
                D3DTA_TEMP | D3DTA_COMPLEMENT,
                D3DTA_CURRENT,
            ],
        ],
        true,
    );
    k.stages[0].set_result(FfStageResult::Temp);
    keys.push((k, plain));
    // Bump environment stages, with and without a texture, luminance, chained, projected.
    for op in [D3DTOP_BUMPENVMAP, D3DTOP_BUMPENVMAPLUMINANCE] {
        let bump = [
            op,
            D3DTA_TEXTURE,
            D3DTA_CURRENT,
            D3DTOP_SELECTARG1,
            D3DTA_TEXTURE,
            D3DTA_CURRENT,
        ];
        let next = tex_diffuse(D3DTOP_MODULATE);
        keys.push((ps_key(&[bump, next], false), plain));
        // Without a texture, naming one: the unbound fallback, not a bump stage.
        let mut k = ps_key(&[bump, next], false);
        k.stages[0].flags = FfStageFlags::empty();
        keys.push((k, plain));
        // Without a texture, naming none: a bump stage that displaces nothing.
        let untextured = [
            op,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
            D3DTOP_SELECTARG1,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
        ];
        keys.push((ps_key(&[untextured, next], false), plain));
        // A bump stage whose next stage samples nothing.
        let diffuse = [
            D3DTOP_MODULATE,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
            D3DTOP_SELECTARG1,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
        ];
        keys.push((ps_key(&[bump, diffuse], false), plain));
        keys.push((ps_key(&[bump, bump, next], false), plain));
        let mut k = ps_key(&[bump, next], false);
        k.tt_projected_mask = 0b11;
        let projected = VariantKey {
            tt_projected_mask: 0b11,
            ..plain
        };
        keys.push((k, projected));
    }
    // Sampling a depth, a cube and a volume slot; a LOD bias; raw depth; projected depth.
    let three = ps_key(
        &[
            tex_diffuse(D3DTOP_MODULATE),
            tex_diffuse(D3DTOP_MODULATE),
            tex_diffuse(D3DTOP_MODULATE),
        ],
        false,
    );
    let mut projected = three.clone();
    projected.tt_projected_mask = 0b001;
    for (k, variant) in [
        (
            three.clone(),
            VariantKey {
                depth_sampler_mask: 0b001,
                cube_sampler_mask: 0b010,
                volume_sampler_mask: 0b100,
                ..plain
            },
        ),
        (
            three.clone(),
            VariantKey {
                flags: VariantFlags::LOD_BIAS,
                ..plain
            },
        ),
        (
            three,
            VariantKey {
                depth_sampler_mask: 0b001,
                depth_fetch_mask: 0b001,
                ..plain
            },
        ),
        (
            projected,
            VariantKey {
                depth_sampler_mask: 0b001,
                tt_projected_mask: 0b001,
                ..plain
            },
        ),
    ] {
        keys.push((k, variant));
    }
    // A cascade cut short by DISABLE.
    let mut k = ps_key(&[tex_diffuse(D3DTOP_MODULATE)], true);
    k.stages[1].color_op = narrow(D3DTOP_DISABLE);
    keys.push((k, plain));
    keys
}

/// One cache file holding the shader records and recipes of every input, once each.
///
/// Records without a source are dropped, and so are the recipes naming a record the file does
/// not keep; the first line of the result counts both.
fn merge_caches(out: &Path, inputs: &[PathBuf]) -> Result<String, String> {
    let mut shaders: Vec<CacheEntry> = Vec::new();
    let mut pipelines: Vec<PipelineRecipe> = Vec::new();
    for path in inputs {
        let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if read_header(&bytes) != Ok(CacheHeader::CURRENT) {
            return Err(format!(
                "{}: not a shader cache this build reads (container {CACHE_FORMAT_VERSION}, \
                 schema {SHADER_CACHE_SCHEMA_VERSION})",
                path.display()
            ));
        }
        let records = read_records(&bytes);
        if records.valid_len() < bytes.len() {
            // As the layer's `load` does: keep the records before the damage.
            eprintln!(
                "{}: torn or damaged; dropping {} bytes after the valid prefix",
                path.display(),
                bytes.len() - records.valid_len()
            );
        }
        shaders.extend(records.shaders);
        pipelines.extend(records.pipelines);
    }
    let mut seen = rustc_hash::FxHashSet::default();
    shaders.retain(|entry| seen.insert((entry.kind as u8, entry.key)));
    // A record without a source cannot be rendered again (a C3b runtime's MSL fixed-function
    // records, a stage demoted to MSL under `air`), nor can a recipe naming it.
    let records = shaders.len();
    shaders.retain(|entry| entry.source().is_some());
    let sourceless = records - shaders.len();
    let kept: rustc_hash::FxHashSet<_> = shaders
        .iter()
        .map(|entry| (entry.kind as u8, entry.key))
        .collect();
    let mut seen = rustc_hash::FxHashSet::default();
    pipelines.retain(|recipe| seen.insert(recipe.disk_key()));
    let recipes = pipelines.len();
    pipelines.retain(|recipe| {
        [recipe.vs(), recipe.ps()]
            .iter()
            .all(|stage| kept.contains(&(stage.kind() as u8, stage.key())))
    });
    let written = write_cache_file(out, &shaders, &pipelines)?;
    Ok(format!(
        "merge: dropped {sourceless} sourceless records, {} recipes\n{written}",
        recipes - pipelines.len()
    ))
}

/// `--kinds`: the record count per kind, summed over `inputs`, as one line.
fn kinds_line(inputs: &[PathBuf]) -> Result<String, String> {
    const KINDS: [(CachedKind, &str); 16] = [
        (CachedKind::Sm1Vs, "sm1vs"),
        (CachedKind::Sm1Ps, "sm1ps"),
        (CachedKind::Sm2Vs, "sm2vs"),
        (CachedKind::Sm2Ps, "sm2ps"),
        (CachedKind::Sm3Vs, "sm3vs"),
        (CachedKind::Sm3Ps, "sm3ps"),
        (CachedKind::AirSm1Vs, "air_sm1vs"),
        (CachedKind::AirSm1Ps, "air_sm1ps"),
        (CachedKind::AirSm2Vs, "air_sm2vs"),
        (CachedKind::AirSm2Ps, "air_sm2ps"),
        (CachedKind::AirSm3Vs, "air_sm3vs"),
        (CachedKind::AirSm3Ps, "air_sm3ps"),
        (CachedKind::FfVs, "ffvs"),
        (CachedKind::FfPs, "ffps"),
        (CachedKind::AirFfVs, "air_ffvs"),
        (CachedKind::AirFfPs, "air_ffps"),
    ];
    let mut counts = [0_usize; KINDS.len()];
    for path in inputs {
        let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if read_header(&bytes) != Ok(CacheHeader::CURRENT) {
            return Err(format!(
                "{}: not a shader cache this build reads",
                path.display()
            ));
        }
        for entry in read_records(&bytes).shaders {
            if let Some(at) = KINDS.iter().position(|(kind, _)| *kind == entry.kind) {
                counts[at] += 1;
            }
        }
    }
    let mut line = format!("kinds {} files:", inputs.len());
    for ((_, name), count) in KINDS.iter().zip(counts) {
        let _ = write!(line, " {name}={count}");
    }
    Ok(line)
}

/// The cargo profile this binary was built with, from `target/<triple>/<profile>/examples`.
fn profile() -> String {
    env::current_exe()
        .ok()
        .and_then(|exe| {
            let examples = exe.parent()?;
            (examples.file_name()? == "examples").then_some(())?;
            Some(
                examples
                    .parent()?
                    .file_name()?
                    .to_string_lossy()
                    .into_owned(),
            )
        })
        .unwrap_or_else(|| "unknown".to_owned())
}

/// Read one shader cache into a corpus named after the directory holding it.
///
/// `Ok(None)` is a file this build cannot read as a cache, skipped with a note;
/// only a path that cannot be read at all is an error.
fn load_cache(path: &Path, taken: &[Corpus]) -> Result<Option<Corpus>, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let Ok(header) = read_header(&bytes) else {
        println!(
            "{}: skipped, not an mtld3d shader cache (no MTLD3DSH header)",
            path.display()
        );
        return Ok(None);
    };
    if header.format_version != CACHE_FORMAT_VERSION {
        println!(
            "{}: skipped, container format {} (this build reads {CACHE_FORMAT_VERSION})",
            path.display(),
            header.format_version
        );
        return Ok(None);
    }
    if header.shader_schema_version != SHADER_CACHE_SCHEMA_VERSION {
        eprintln!(
            "{}: shader schema {} (this build writes {SHADER_CACHE_SCHEMA_VERSION}); timing its \
             retained DXSO anyway",
            path.display(),
            header.shader_schema_version
        );
    }
    let records = read_records(&bytes);
    if records.valid_len() < bytes.len() {
        println!(
            "{}: torn or damaged after byte {} of {}; timing the records before it",
            path.display(),
            records.valid_len(),
            bytes.len()
        );
    }
    let records = records.shaders;
    let total = records.len();
    let shaders: Vec<Shader> = records.into_iter().filter_map(Shader::record).collect();
    println!(
        "{}: {total} shader records, {} with retained DXSO",
        path.display(),
        shaders.len()
    );
    Ok(Some(Corpus::new(corpus_name(path, taken), shaders)))
}

/// The cache's directory name as a metrics-safe identifier, unique among `taken`.
fn corpus_name(path: &Path, taken: &[Corpus]) -> String {
    let dir = fs::canonicalize(path)
        .ok()
        .and_then(|full| Some(full.parent()?.file_name()?.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "cache".to_owned());
    let base: String = dir
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let mut name = base.clone();
    let mut suffix = 2;
    while taken.iter().any(|corpus| corpus.name == name) {
        name = format!("{base}_{suffix}");
        suffix += 1;
    }
    name
}

fn measure(corpus: &Corpus, iters: Option<u32>) -> Report {
    let parse = corpus.is_programmable().then(|| {
        time(corpus.shaders.len(), iters, || {
            for tokens in corpus.shaders.iter().filter_map(Shader::tokens) {
                // Every kept shader emitted, so its tokens parse.
                let _ = black_box(dxso::parse(black_box(tokens)));
            }
        })
    });
    let emit = time(corpus.shaders.len(), iters, || {
        for shader in &corpus.shaders {
            let _ = black_box(shader.emit());
        }
    });
    Report { parse, emit }
}

/// Run `pass` repeatedly and report the per-shader median and minimum over the passes.
fn time(shaders: usize, iters: Option<u32>, mut pass: impl FnMut()) -> Timing {
    let warm = Instant::now();
    pass();
    let warm = warm.elapsed().max(Duration::from_nanos(1));
    let passes = iters.unwrap_or_else(|| {
        let needed = TARGET.as_nanos().div_ceil(warm.as_nanos());
        u32::try_from(needed).unwrap_or(u32::MAX).max(MIN_PASSES)
    });
    let mut samples: Vec<Duration> = (0..passes)
        .map(|_| {
            let start = Instant::now();
            pass();
            start.elapsed()
        })
        .collect();
    samples.sort_unstable();
    let count = f64::from(u32::try_from(shaders).expect("a corpus holds fewer than 2^32 shaders"));
    let per_shader_us = |sample: Duration| sample.as_secs_f64() * 1e6 / count;
    let middle = samples.len() / 2;
    // The mean of the two middle samples when the pass count is even.
    let median = if samples.len().is_multiple_of(2) {
        (samples[middle - 1] + samples[middle]) / 2
    } else {
        samples[middle]
    };
    Timing {
        median_us: per_shader_us(median),
        min_us: per_shader_us(samples[0]),
        passes,
    }
}

fn write_metrics(
    dir: &Path,
    corpus: &Corpus,
    report: &Report,
    meta: &BuildMeta,
) -> Result<(), String> {
    use fmt::Write as _;

    let bench = format!("host_emit_{}", corpus.name);
    let emit = corpus.emit_metric();
    let mut out = String::new();
    let mut line = |args: fmt::Arguments<'_>| {
        writeln!(out, "{args}").expect("formatting into a String cannot fail");
    };
    line(format_args!("meta {bench} kind host"));
    line(format_args!("meta {bench} layer {}", meta.layer));
    line(format_args!("meta {bench} host_image {}", meta.image));
    line(format_args!("meta {bench} arch {}", meta.arch));
    line(format_args!("meta {bench} profile {}", meta.profile));
    line(format_args!(
        "meta {bench} debug_assertions {}",
        if cfg!(debug_assertions) { "on" } else { "off" }
    ));
    line(format_args!(
        "meta {bench} emitter {SHADER_EMITTER_VERSION:016x}"
    ));
    if emit == "parse_emit" {
        // A cache corpus, not one of the synthetic ones every build writes.
        line(format_args!("meta {bench} corpus {}", corpus.name));
    }
    if let Some(parse) = &report.parse {
        line(format_args!(
            "metric {bench} parse.us_per_shader {:.3} us lower time",
            parse.median_us
        ));
        line(format_args!(
            "metric {bench} parse.us_per_shader_min {:.3} us lower time",
            parse.min_us
        ));
    }
    line(format_args!(
        "metric {bench} {emit}.us_per_shader {:.3} us lower time",
        report.emit.median_us
    ));
    line(format_args!(
        "metric {bench} {emit}.us_per_shader_min {:.3} us lower time",
        report.emit.min_us
    ));
    line(format_args!(
        "metric {bench} shaders {} count higher info",
        corpus.shaders.len()
    ));
    line(format_args!(
        "metric {bench} emit.failures {} count lower exact",
        corpus.failures
    ));
    line(format_args!(
        "metric {bench} msl.bytes_total {} bytes lower exact",
        corpus.msl_total
    ));
    line(format_args!(
        "metric {bench} msl.bytes_max {} bytes lower exact",
        corpus.msl_max
    ));
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("bench-{bench}.metrics"));
    fs::write(&path, out).map_err(|error| format!("{}: {error}", path.display()))
}

/// D3D enum constant at the key's narrow width.
fn narrow(value: u32) -> u8 {
    u8::try_from(value).expect("D3D9 fixed-function enum value fits u8")
}

/// The vertex layout and render state one synthetic fixed-function vertex key stands for.
struct VsShape {
    geometry: Geometry,
    /// `None` is lighting off; `Some(0)` is lighting on with no light, ambient only.
    lighting: Option<u8>,
    fog: bool,
    specular: bool,
    texturing: Texturing,
}

/// How the texture coordinates of a synthetic fixed-function vertex key are routed.
#[derive(PartialEq, Eq)]
enum Texturing {
    /// Each stage passes its own coordinate set through.
    Plain,
    /// One more stage fed by the camera-space reflection vector through a texture matrix.
    EnvMap,
    /// Stage 0's coordinates go through a `D3DTTFF_COUNT2` texture matrix.
    Scrolled,
    /// Both texture stages read coordinate set 0.
    SharedUv,
}

impl VsShape {
    const fn new(geometry: Geometry, lighting: Option<u8>, fog: bool) -> Self {
        Self {
            geometry,
            lighting,
            fog,
            specular: false,
            texturing: Texturing::Plain,
        }
    }
}

/// The vertex formats a fixed-function game draws with.
enum Geometry {
    /// Position, normal and `tex` texture coordinates: lit world geometry.
    Lit { tex: u8 },
    /// Position, diffuse colour and `tex` texture coordinates: unlit geometry.
    Unlit { tex: u8 },
    /// Pre-transformed position, diffuse colour and `tex` coordinates: interface quads.
    Rhw { tex: u8 },
}

/// Name every shader of a synthetic corpus by its position, in fixed-width names.
///
/// A name derived from the key hash would change width with the hash, and the
/// MSL sizes with it, without any change to the emitter. `u64::MAX - index`
/// always prints as sixteen hex digits.
fn fixed_names(mut shaders: Vec<Shader>) -> Vec<Shader> {
    for (index, shader) in shaders.iter_mut().enumerate() {
        let index = u64::try_from(index).expect("corpus index fits u64");
        shader.entry = shader.kind().entry_name(u64::MAX - index);
    }
    shaders
}

/// About sixty fixed-function vertex keys and fifty pixel keys.
///
/// Lit world geometry with one or two texture sets under zero to four lights
/// (the last of several a point light) or lighting on with ambient alone,
/// vertex fog on and off, specular on and off, an environment-mapped stage fed
/// by the camera-space reflection vector through a texture matrix, a scrolled
/// UV stage through a two-component texture matrix, and two stages sharing one
/// coordinate set; unlit and pre-transformed geometry beside it. The pixel keys
/// are the texture cascades such a game sets: one to three stages of modulate,
/// select, blend and add, each with and without alpha test and fog, and a few
/// with the specular add. Stage state a key does not set keeps its D3D9
/// default, as the layer's key builders read it.
fn synthetic_ff() -> Vec<Shader> {
    let mut shapes = Vec::new();
    for tex in [1, 2] {
        for lighting in [None, Some(1), Some(2), Some(4)] {
            for fog in [false, true] {
                let mut variants = vec![(Texturing::EnvMap, false), (Texturing::Plain, false)];
                if lighting.is_some() {
                    variants.push((Texturing::Plain, true));
                }
                for (texturing, specular) in variants {
                    let mut shape = VsShape::new(Geometry::Lit { tex }, lighting, fog);
                    shape.texturing = texturing;
                    shape.specular = specular;
                    shapes.push(shape);
                }
            }
        }
        for fog in [false, true] {
            shapes.push(VsShape::new(Geometry::Unlit { tex }, None, fog));
        }
    }
    for fog in [false, true] {
        shapes.push(VsShape::new(Geometry::Lit { tex: 1 }, Some(0), fog));
        let mut scrolled = VsShape::new(Geometry::Unlit { tex: 1 }, None, fog);
        scrolled.texturing = Texturing::Scrolled;
        shapes.push(scrolled);
        let mut detail = VsShape::new(Geometry::Lit { tex: 2 }, Some(1), fog);
        detail.texturing = Texturing::SharedUv;
        shapes.push(detail);
    }
    let mut scrolled_lit = VsShape::new(Geometry::Lit { tex: 1 }, Some(1), true);
    scrolled_lit.texturing = Texturing::Scrolled;
    shapes.push(scrolled_lit);
    let mut detail_unlit = VsShape::new(Geometry::Unlit { tex: 2 }, None, false);
    detail_unlit.texturing = Texturing::SharedUv;
    shapes.push(detail_unlit);
    for tex in [0, 1] {
        shapes.push(VsShape::new(Geometry::Rhw { tex }, None, false));
    }
    let mut shaders: Vec<Shader> = shapes
        .iter()
        .map(|shape| Shader::ff_vs(vs_key(shape)))
        .collect();

    for (index, cascade) in cascades().iter().enumerate() {
        for alpha_test in [false, true] {
            for fog in [false, true] {
                for specular_add in [false, true] {
                    // Only the plain modulate cascades are drawn lit with specular.
                    if specular_add && index > 1 {
                        continue;
                    }
                    let variant = VariantKey {
                        alpha_func: if alpha_test {
                            narrow(D3DCMP_GREATEREQUAL)
                        } else {
                            0
                        },
                        fog_mode: if fog { narrow(D3DFOG_LINEAR) } else { 0 },
                        ..VariantKey::default()
                    };
                    shaders.push(Shader::ff_ps(ps_key(cascade, specular_add), variant));
                }
            }
        }
    }
    fixed_names(shaders)
}

fn vs_key(shape: &VsShape) -> FfVsKey {
    let (tex, normal, rhw) = match shape.geometry {
        Geometry::Lit { tex } => (tex, true, false),
        Geometry::Unlit { tex } => (tex, false, false),
        Geometry::Rhw { tex } => (tex, false, true),
    };
    let lit = normal && shape.lighting.is_some();
    let lights = if lit { shape.lighting.unwrap_or(0) } else { 0 };
    let specular = lights > 0 && shape.specular;
    let mut flags = FfVsFlags::COLOR_VERTEX;
    flags.set(FfVsFlags::HAS_NORMAL, normal);
    flags.set(FfVsFlags::HAS_COLOR0, !normal);
    flags.set(FfVsFlags::HAS_RHW, rhw);
    flags.set(FfVsFlags::LIGHTING_ENABLED, lit);
    flags.set(FfVsFlags::SPECULAR_ENABLE, specular);
    // Canonicalized on only where the specular term reads the view vector.
    flags.set(FfVsFlags::LOCAL_VIEWER, specular);
    let active = if lights == 0 {
        0
    } else {
        0xFFu8 >> (8 - lights)
    };
    let point = if lights > 1 { 1u8 << (lights - 1) } else { 0 };
    let sets = if shape.texturing == Texturing::SharedUv {
        1
    } else {
        tex
    };
    let mut key = FfVsKey {
        reserved: 0,
        flags,
        input_tex_coord_count: sets,
        tex_coord_count: tex,
        light_active_mask: active,
        light_directional_mask: active & !point,
        light_spot_mask: 0,
        diffuse_source: narrow(D3DMCS_COLOR1),
        ambient_source: narrow(D3DMCS_MATERIAL),
        specular_source: narrow(D3DMCS_COLOR2),
        emissive_source: narrow(D3DMCS_MATERIAL),
        fog_mode: if shape.fog && !rhw {
            narrow(D3DFOG_LINEAR)
        } else {
            0
        },
        // Every stage's `D3DTSS_TEXCOORDINDEX` defaults to its own index.
        tci: core::array::from_fn(|stage| {
            tci_entry(0, u8::try_from(stage).expect("stage index fits u8"))
        }),
        tex_coord_dims: [0; 8],
        tt_flags: [0; 8],
        vertex_blend_count: 0,
        declared_weights_count: 0,
        clip_plane_count: 0,
        passthrough: [0; 8],
    };
    for dims in &mut key.tex_coord_dims[..usize::from(sets)] {
        *dims = 2;
    }
    match shape.texturing {
        Texturing::Plain => {}
        Texturing::EnvMap => {
            let stage = usize::from(tex);
            key.tex_coord_count = tex + 1;
            key.tci[stage] = tci_entry(TCI_REFLECTION, tex);
            key.tt_flags[stage] = TT_COUNT3;
        }
        Texturing::Scrolled => key.tt_flags[0] = TT_COUNT2,
        Texturing::SharedUv => key.tci[1] = tci_entry(0, 0),
    }
    key
}

/// The texture cascades of the synthetic pixel keys, one colour op, args, alpha op, args per stage.
///
/// An argument its operation does not read keeps the D3D9 default
/// (`D3DTA_TEXTURE` for arg 1, `D3DTA_CURRENT` for arg 2).
fn cascades() -> Vec<Vec<[u32; 6]>> {
    let modulate = [
        D3DTOP_MODULATE,
        D3DTA_TEXTURE,
        D3DTA_DIFFUSE,
        D3DTOP_MODULATE,
        D3DTA_TEXTURE,
        D3DTA_DIFFUSE,
    ];
    let select_texture = [
        D3DTOP_SELECTARG1,
        D3DTA_TEXTURE,
        D3DTA_CURRENT,
        D3DTOP_SELECTARG1,
        D3DTA_TEXTURE,
        D3DTA_CURRENT,
    ];
    let modulate2x = [
        D3DTOP_MODULATE2X,
        D3DTA_TEXTURE,
        D3DTA_DIFFUSE,
        D3DTOP_SELECTARG1,
        D3DTA_DIFFUSE,
        D3DTA_CURRENT,
    ];
    let over_current = |op| {
        [
            op,
            D3DTA_TEXTURE,
            D3DTA_CURRENT,
            D3DTOP_SELECTARG1,
            D3DTA_CURRENT,
            D3DTA_CURRENT,
        ]
    };
    vec![
        vec![modulate],
        vec![modulate, over_current(D3DTOP_MODULATE)],
        vec![select_texture],
        vec![modulate2x],
        vec![[
            D3DTOP_SELECTARG1,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
            D3DTOP_SELECTARG1,
            D3DTA_DIFFUSE,
            D3DTA_CURRENT,
        ]],
        vec![select_texture, over_current(D3DTOP_BLENDTEXTUREALPHA)],
        vec![modulate, over_current(D3DTOP_ADD)],
        vec![modulate2x, over_current(D3DTOP_MODULATE2X)],
        vec![modulate, over_current(D3DTOP_MODULATEALPHA_ADDCOLOR)],
        vec![modulate, over_current(D3DTOP_BLENDCURRENTALPHA)],
        vec![
            modulate2x,
            over_current(D3DTOP_BLENDTEXTUREALPHA),
            over_current(D3DTOP_MODULATE),
        ],
    ]
}

/// A pixel key whose stages past the cascade keep their D3D9 default state.
fn ps_key(cascade: &[[u32; 6]], specular_add: bool) -> FfPsKey {
    let stages = core::array::from_fn(|index| {
        let defaults = texture_stage_state_defaults(u8::try_from(index).expect("stage fits u8"));
        let state = |ty: u32| defaults[usize::try_from(ty).expect("state index fits usize")];
        let ops = cascade.get(index).copied().unwrap_or_else(|| {
            [
                state(D3DTSS_COLOROP),
                state(D3DTSS_COLORARG1),
                state(D3DTSS_COLORARG2),
                state(D3DTSS_ALPHAOP),
                state(D3DTSS_ALPHAARG1),
                state(D3DTSS_ALPHAARG2),
            ]
        });
        let [
            color_op,
            color_arg1,
            color_arg2,
            alpha_op,
            alpha_arg1,
            alpha_arg2,
        ] = ops;
        // A game binds a texture to the stages it samples (an argument naming the texture, with
        // or without modifiers) and to no others.
        let textured = index < cascade.len()
            && [color_arg1, color_arg2, alpha_arg1, alpha_arg2]
                .iter()
                .any(|arg| arg & 0xF == D3DTA_TEXTURE);
        FfStage {
            color_op: narrow(color_op),
            color_arg1: narrow(color_arg1),
            color_arg2: narrow(color_arg2),
            alpha_op: narrow(alpha_op),
            alpha_arg1: narrow(alpha_arg1),
            alpha_arg2: narrow(alpha_arg2),
            flags: if textured {
                FfStageFlags::HAS_TEXTURE
            } else {
                FfStageFlags::empty()
            },
        }
    });
    FfPsKey {
        stages,
        specular_add,
        tt_projected_mask: 0,
    }
}

/// Hand-assembled SM1, SM2 and SM3 shaders in the variants a draw specializes them into.
///
/// Transformed and lit world vertex shaders with vertex fog (`vs_1_1`,
/// `vs_2_0`, `vs_3_0`), with and without one user clip plane; a skinned
/// `vs_2_0` indexing its bone palette through the address register;
/// `ps_1_1`, `ps_1_4` and one- and two-texture `ps_2_0` shaders under alpha
/// test and fog; a normal-mapped `ps_3_0` shader with a specular term. The
/// SM1 streams carry no instruction-length field, as a shader compiler emits
/// them.
fn synthetic_sm() -> Vec<Shader> {
    let mut shaders = Vec::new();
    for tokens in [world_vs_1_1(), world_vs(VS_2_0), world_vs(VS_3_0)] {
        for clip_planes in [0, 1] {
            shaders.push(Shader::synthetic(
                tokens.clone(),
                Specialization::Vertex { clip_planes },
            ));
        }
    }
    shaders.push(Shader::synthetic(
        skinned_vs(),
        Specialization::Vertex { clip_planes: 0 },
    ));
    let alpha_test = narrow(D3DCMP_GREATEREQUAL);
    let fog = narrow(D3DFOG_LINEAR);
    for tokens in [
        detail_ps_1_1(),
        detail_ps_1_4(),
        single_texture_ps(),
        two_texture_ps(),
    ] {
        for (alpha_func, fog_mode) in [(0, 0), (0, fog), (alpha_test, fog)] {
            shaders.push(Shader::synthetic(
                tokens.clone(),
                Specialization::Pixel(VariantKey {
                    alpha_func,
                    fog_mode,
                    ..VariantKey::default()
                }),
            ));
        }
    }
    // SM3 pixel shaders take no automatic fog, so only alpha test varies.
    for alpha_func in [0, alpha_test] {
        shaders.push(Shader::synthetic(
            normal_mapped_ps(),
            Specialization::Pixel(VariantKey {
                alpha_func,
                ..VariantKey::default()
            }),
        ));
    }
    fixed_names(shaders)
}

/// The `ps_1_x` texture-address families the timed set lacks, for the parity corpus only.
///
/// `--write-cache` pairs each with the `vs_1_1` shaders; `--digest` and the timed corpora do not
/// see them. Programs are Task 6's hand cases: bem, texbem, texbeml, texm3x2pad/tex/depth,
/// texm3x3pad/tex/spec/vspec and texm3x3, texreg2ar/gb/rgb, texdp3, texdp3tex and texdepth.
fn texture_family_ps() -> Vec<(Vec<u32>, VariantKey, bool)> {
    const PHASE: u32 = 0xFFFD;
    const ALL: u8 = 0xF;
    let td = |n: u16| dst(REG_ADDR_OR_TEXTURE, n, ALL);
    let ts = |n: u16| src(REG_ADDR_OR_TEXTURE, n, SWIZ_IDENTITY);
    let rd = |n: u16| dst(REG_TEMP, n, ALL);
    let rs = |n: u16| src(REG_TEMP, n, SWIZ_IDENTITY);
    let c = |n: u16| src(REG_CONST, n, SWIZ_IDENTITY);
    let sel = |n: u16| ts(n) | (4 << 24);
    let ins = |code: u16, operands: &[u32]| {
        let mut v = vec![op(code, 0)];
        v.extend_from_slice(operands);
        v
    };
    let prog = |header: u32, body: &[Vec<u32>]| {
        let mut t = vec![header];
        for b in body {
            t.extend_from_slice(b);
        }
        t.push(END);
        t
    };
    let def_row = |index: u16, value: [f32; 4]| {
        let mut t = vec![op(OP_DEF, 5), dst(REG_CONST, index, ALL)];
        t.extend(value.map(f32::to_bits));
        t
    };
    // Opcodes: texcoord 64, texkill 65, tex 66, texbem 67, texbeml 68, texreg2ar 69, texreg2gb 70,
    // texm3x2pad 71, texm3x2tex 72, texm3x3pad 73, texm3x3tex 74, texm3x3spec 76, texm3x3vspec 77,
    // cnd 80, bem 89, texm3x2depth 84, texdp3 83, texm3x3 86, texdp3tex 85, texdepth 87, texreg2rgb 82.
    let bem = prog(
        PS_1_1,
        &[
            ins(66, &[td(0)]),
            ins(67, &[td(1), ts(0)]),
            ins(68, &[td(2), ts(0)]),
            ins(OP_ADD, &[rd(0), ts(1), ts(2)]),
        ],
    );
    let beml_mask = prog(
        PS_1_1,
        &[
            ins(66, &[td(0)]),
            ins(68, &[dst(REG_ADDR_OR_TEXTURE, 2, 0b0011), ts(0)]),
            ins(OP_ADD, &[rd(0), ts(0), ts(2)]),
        ],
    );
    let bem14 = prog(
        PS_1_4,
        &[
            ins(66, &[rd(0), ts(0)]),
            ins(89, &[dst(REG_TEMP, 1, 3), c(0), rs(0)]),
            vec![PHASE],
            ins(66, &[rd(2), rs(1)]),
            ins(OP_ADD, &[rd(0), rs(2), rs(1)]),
        ],
    );
    let m3x2tex = prog(
        PS_1_1,
        &[
            ins(66, &[td(0)]),
            ins(71, &[td(1), sel(0)]),
            ins(72, &[td(2), sel(0)]),
            ins(OP_ADD, &[rd(0), ts(2), ts(1)]),
        ],
    );
    let m3x2depth = |zero: bool| {
        let source = if zero { c(1) } else { ts(0) };
        prog(
            0xFFFF_0103,
            &[
                def_row(1, [0.0; 4]),
                ins(66, &[td(0)]),
                ins(71, &[td(1), ts(0)]),
                ins(84, &[td(2), source]),
                ins(OP_MOV, &[rd(0), ts(0)]),
            ],
        )
    };
    let m3x3 = |last: u16, extra: &[u32]| {
        let mut operands = vec![td(3), sel(0)];
        operands.extend_from_slice(extra);
        prog(
            0xFFFF_0102,
            &[
                ins(66, &[td(0)]),
                ins(73, &[td(1), sel(0)]),
                ins(73, &[td(2), sel(0)]),
                ins(last, &operands),
                ins(OP_MOV, &[rd(0), ts(3)]),
            ],
        )
    };
    let reg2 = prog(
        0xFFFF_0102,
        &[
            ins(66, &[td(0)]),
            ins(69, &[td(1), ts(0)]),
            ins(70, &[td(2), ts(0)]),
            ins(82, &[td(3), ts(0)]),
            ins(OP_ADD, &[rd(0), ts(1), ts(2)]),
            ins(OP_MAD, &[rd(0), ts(3), c(2), rs(0)]),
        ],
    );
    let dp3 = prog(
        0xFFFF_0102,
        &[
            ins(66, &[td(0)]),
            ins(83, &[td(1), ts(0)]),
            ins(85, &[td(2), ts(0)]),
            ins(OP_ADD, &[rd(0), ts(1), ts(2)]),
        ],
    );
    let texdepth = |value: Option<[f32; 4]>| {
        let mut body = value.map_or_else(
            || vec![ins(64, &[rd(5), ts(0)])],
            |v| vec![def_row(0, v), ins(OP_MOV, &[rd(5), c(0)])],
        );
        body.extend([vec![PHASE], ins(87, &[rd(5)]), ins(OP_MOV, &[rd(0), ts(1)])]);
        prog(PS_1_4, &body)
    };
    let key = |cube: u16| VariantKey {
        cube_sampler_mask: cube,
        ..VariantKey::default()
    };
    // A shader that writes depth needs the flag: the recipes carry no depth attachment.
    let no_depth = VariantKey {
        flags: VariantFlags::NO_DEPTH_ATTACHMENT,
        ..VariantKey::default()
    };
    let plain = vec![
        (bem, key(0)),
        (beml_mask, key(0)),
        (bem14, key(0)),
        (m3x2tex, key(0)),
        (m3x2depth(false), no_depth),
        (m3x2depth(true), no_depth),
        (m3x3(74, &[]), key(0)),
        (m3x3(86, &[]), key(0)),
        (m3x3(76, &[c(7)]), key(0b1000)),
        (m3x3(77, &[]), key(0b1000)),
        (reg2, key(0)),
        (dp3, key(0)),
        (texdepth(None), no_depth),
        (texdepth(Some([0.0; 4])), no_depth),
    ];
    // The depth writers again, now with the depth attachment (the third field): `oDepth` reaches
    // the depth output, which the parity test reads back.
    let mut out: Vec<_> = plain.into_iter().map(|(t, k)| (t, k, false)).collect();
    out.extend([
        (m3x2depth(false), key(0), true),
        (m3x2depth(true), key(0), true),
        (texdepth(None), key(0), true),
        (texdepth(Some([0.0; 4])), key(0), true),
    ]);
    out
}

/// `ps_2_0` and `ps_3_0` programs that write `oDepth` the way games do, for the parity corpus only.
///
/// `mov oDepth, r0.x` from a varying; the second `ps_3_0` one has `_sat`.
fn depth_programs() -> Vec<Vec<u32>> {
    const SAT: u32 = 1 << 20;
    let mut out = Vec::new();
    for (version, saturate) in [(PS_2_0, 0), (PS_3_0, 0), (PS_3_0, SAT)] {
        let mut ps = vec![version];
        let varying = if version == PS_3_0 {
            dcl(&mut ps, usage(D3DDECLUSAGE_TEXCOORD, 0), REG_INPUT, 0, XYZW);
            src(REG_INPUT, 0, SWIZ_IDENTITY)
        } else {
            dcl(&mut ps, DCL_PLAIN, REG_ADDR_OR_TEXTURE, 0, XYZW);
            src(REG_ADDR_OR_TEXTURE, 0, SWIZ_IDENTITY)
        };
        ps.extend([op(OP_MOV, 2), dst(REG_TEMP, 0, XYZW), varying]);
        ps.extend([
            op(OP_MOV, 2),
            dst(REG_DEPTHOUT, 0, X) | saturate,
            src(REG_TEMP, 0, SWIZ_X),
        ]);
        ps.extend([
            op(OP_MOV, 2),
            dst(REG_COLOROUT, 0, XYZW),
            src(REG_TEMP, 0, SWIZ_IDENTITY),
            END,
        ]);
        out.push(ps);
    }
    out
}

/// Programs whose result modifiers feed further arithmetic, for the parity corpus only.
///
/// `_sat` on a destination, `-r`, `|r|` (`vs_2_0` and later, `ps_3_0`), `1-r` and `_bias`
/// (`ps_1_x`) on values from `c#` and `v#`: each result goes through more arithmetic before it
/// leaves, so dropping a modifier changes the pixels even in a float target. Vertex shaders write
/// a texcoord, pass-through pixel shaders show it unclamped, the others compute with the modifiers.
fn modifier_programs() -> Vec<(Vec<u32>, Specialization)> {
    const SAT: u32 = 1 << 20;
    const NEG: u32 = 1 << 24;
    const BIAS: u32 = 2 << 24;
    const COMP: u32 = 6 << 24;
    const ABS: u32 = 11 << 24;
    let t = |n: u16| dst(REG_TEMP, n, XYZW);
    let r = |n: u16| src(REG_TEMP, n, SWIZ_IDENTITY);
    let c = |n: u16| src(REG_CONST, n, SWIZ_IDENTITY);
    let v = |n: u16| src(REG_INPUT, n, SWIZ_IDENTITY);
    let tex = |n: u16| src(REG_ADDR_OR_TEXTURE, n, SWIZ_IDENTITY);
    let mut out = Vec::new();
    // vs_1_1, vs_2_0, vs_3_0: texcoord v1 through `mad_sat`, a negated and an absolute source.
    for version in [VS_1_1, VS_2_0, VS_3_0] {
        let (sm1, sm3) = (version == VS_1_1, version == VS_3_0);
        let mut vs = vec![version];
        dcl(&mut vs, usage(D3DDECLUSAGE_POSITION, 0), REG_INPUT, 0, XYZW);
        dcl(&mut vs, usage(D3DDECLUSAGE_TEXCOORD, 0), REG_INPUT, 1, XYZW);
        let (position, texcoord) = if sm3 {
            dcl(
                &mut vs,
                usage(D3DDECLUSAGE_POSITION, 0),
                REG_OUTPUT,
                0,
                XYZW,
            );
            dcl(
                &mut vs,
                usage(D3DDECLUSAGE_TEXCOORD, 0),
                REG_OUTPUT,
                1,
                XYZW,
            );
            (dst(REG_OUTPUT, 0, XYZW), dst(REG_OUTPUT, 1, XYZW))
        } else {
            (dst(REG_RASTOUT, 0, XYZW), dst(REG_OUTPUT, 0, XYZW))
        };
        let magnitude = if sm1 { r(1) } else { r(1) | ABS };
        vs.extend([op(OP_MAD, 4), t(0) | SAT, v(1), c(0), c(1)]);
        vs.extend([op(OP_MAD, 4), t(1), r(0) | NEG, c(2), v(1)]);
        vs.extend([op(OP_ADD, 3), t(2), magnitude, c(3)]);
        vs.extend([op(OP_MUL, 3), t(2) | SAT, r(2), c(4)]);
        vs.extend([op(OP_MAD, 4), t(3), r(2), c(5), r(1)]);
        vs.extend([op(OP_MOV, 2), position, v(0)]);
        vs.extend([op(OP_MOV, 2), texcoord, r(3)]);
        vs.push(END);
        out.push((vs, Specialization::Vertex { clip_planes: 0 }));
    }
    let plain = || Specialization::Pixel(VariantKey::default());
    // ps_1_1: `_bias`, `1-` and a negated source around `mad_sat`.
    out.push((
        vec![
            PS_1_1,
            op(64, 0),
            dst(REG_ADDR_OR_TEXTURE, 0, XYZW),
            op(OP_MAD, 0),
            t(0) | SAT,
            tex(0) | BIAS,
            c(0),
            v(0),
            op(OP_MUL, 0),
            t(1),
            r(0) | COMP,
            c(1),
            op(OP_ADD, 0),
            t(0),
            r(1),
            tex(0) | NEG,
            END,
        ],
        plain(),
    ));
    // ps_2_0: texcoord t0 and colour v0 through `mad_sat`, a negated source and `add_sat`.
    let mut ps = vec![PS_2_0];
    dcl(&mut ps, DCL_PLAIN, REG_ADDR_OR_TEXTURE, 0, XYZW);
    dcl(&mut ps, DCL_PLAIN, REG_INPUT, 0, XYZW);
    ps.extend([op(OP_MAD, 4), t(0) | SAT, tex(0), c(0), v(0)]);
    ps.extend([op(OP_MAD, 4), t(1), r(0) | NEG, c(1), tex(0)]);
    ps.extend([op(OP_ADD, 3), t(2) | SAT, r(1), c(2) | NEG]);
    ps.extend([op(OP_MAD, 4), t(3), r(2), c(3), r(1)]);
    ps.extend([op(OP_MOV, 2), dst(REG_COLOROUT, 0, XYZW), r(3), END]);
    out.push((ps, plain()));
    // ps_3_0: the same with an absolute source.
    let mut ps = vec![PS_3_0];
    dcl(&mut ps, usage(D3DDECLUSAGE_TEXCOORD, 0), REG_INPUT, 0, XYZW);
    dcl(&mut ps, usage(D3DDECLUSAGE_COLOR, 0), REG_INPUT, 1, XYZW);
    ps.extend([op(OP_MAD, 4), t(0) | SAT, v(0), c(0), v(1)]);
    ps.extend([op(OP_MAD, 4), t(1), r(0) | NEG, c(1), v(0)]);
    ps.extend([op(OP_ADD, 3), t(2) | SAT, r(1) | ABS, c(2) | NEG]);
    ps.extend([op(OP_MAD, 4), t(3), r(2), c(3), r(1)]);
    ps.extend([op(OP_MOV, 2), dst(REG_COLOROUT, 0, XYZW), r(3), END]);
    out.push((ps, plain()));
    // Pass-throughs of the interpolated texcoord: ps_1_1 (clamped by `texcoord`), 2_0 and 3_0.
    out.push((
        vec![
            PS_1_1,
            op(64, 0),
            dst(REG_ADDR_OR_TEXTURE, 0, XYZW),
            op(OP_MOV, 0),
            t(0),
            tex(0),
            END,
        ],
        plain(),
    ));
    let mut ps = vec![PS_2_0];
    dcl(&mut ps, DCL_PLAIN, REG_ADDR_OR_TEXTURE, 0, XYZW);
    ps.extend([op(OP_MOV, 2), dst(REG_COLOROUT, 0, XYZW), tex(0), END]);
    out.push((ps, plain()));
    let mut ps = vec![PS_3_0];
    dcl(&mut ps, usage(D3DDECLUSAGE_TEXCOORD, 0), REG_INPUT, 0, XYZW);
    ps.extend([op(OP_MOV, 2), dst(REG_COLOROUT, 0, XYZW), v(0), END]);
    out.push((ps, plain()));
    out
}

/// `vs_1_1`: transformed, lit world geometry with a scrolled second coordinate set.
fn world_vs_1_1() -> Vec<u32> {
    let mut out = vec![VS_1_1];
    for (usage_code, index) in [
        (D3DDECLUSAGE_POSITION, 0),
        (D3DDECLUSAGE_NORMAL, 1),
        (D3DDECLUSAGE_TEXCOORD, 2),
    ] {
        out.extend([
            op(OP_DCL, 0),
            usage(usage_code, 0),
            dst(REG_INPUT, index, XYZW),
        ]);
    }
    out.extend([op(OP_DEF, 0), dst(REG_CONST, 8, XYZW)]);
    out.extend([0.0f32, 1.0, 0.0, 0.0].map(f32::to_bits));
    for (row, mask) in [X, Y, Z, W].into_iter().enumerate() {
        let row = u16::try_from(row).expect("row fits u16");
        out.extend([
            op(OP_DP4, 0),
            dst(REG_RASTOUT, 0, mask),
            src(REG_INPUT, 0, SWIZ_IDENTITY),
            src(REG_CONST, row, SWIZ_IDENTITY),
        ]);
    }
    out.extend([
        op(OP_DP3, 0),
        dst(REG_TEMP, 0, X),
        src(REG_INPUT, 1, SWIZ_IDENTITY),
        src(REG_CONST, 4, SWIZ_IDENTITY),
        op(OP_MAX, 0),
        dst(REG_TEMP, 0, X),
        src(REG_TEMP, 0, SWIZ_X),
        src(REG_CONST, 8, SWIZ_X),
        op(OP_MAD, 0),
        dst(REG_ATTROUT, 0, XYZW),
        src(REG_TEMP, 0, SWIZ_X),
        src(REG_CONST, 5, SWIZ_IDENTITY),
        src(REG_CONST, 6, SWIZ_IDENTITY),
        op(OP_MOV, 0),
        dst(REG_OUTPUT, 0, XYZW),
        src(REG_INPUT, 2, SWIZ_IDENTITY),
        op(OP_ADD, 0),
        dst(REG_OUTPUT, 1, XYZW),
        src(REG_INPUT, 2, SWIZ_IDENTITY),
        src(REG_CONST, 9, SWIZ_IDENTITY),
        END,
    ]);
    out
}

/// `vs_1_1`: `world_vs_1_1` that also writes `oT2` from the normal, scaled and offset by constants.
fn depth_vs_1_1() -> Vec<u32> {
    let mut out = world_vs_1_1();
    out.pop();
    out.extend([
        op(OP_MAD, 0),
        dst(REG_OUTPUT, 2, XYZW),
        src(REG_INPUT, 1, SWIZ_IDENTITY),
        src(REG_CONST, 5, SWIZ_IDENTITY),
        src(REG_INPUT, 2, SWIZ_IDENTITY),
        END,
    ]);
    out
}

/// `ps_1_1`: a texture modulated by the diffuse colour and doubled by a detail texture.
fn detail_ps_1_1() -> Vec<u32> {
    vec![
        PS_1_1,
        op(OP_TEXLD, 0),
        dst(REG_ADDR_OR_TEXTURE, 0, XYZW),
        op(OP_TEXLD, 0),
        dst(REG_ADDR_OR_TEXTURE, 1, XYZW),
        op(OP_MUL, 0),
        dst(REG_TEMP, 0, XYZW),
        src(REG_ADDR_OR_TEXTURE, 0, SWIZ_IDENTITY),
        src(REG_INPUT, 0, SWIZ_IDENTITY),
        op(OP_MUL, 0),
        dst(REG_TEMP, 0, XYZ) | SHIFT_X2,
        src(REG_TEMP, 0, SWIZ_IDENTITY),
        src(REG_ADDR_OR_TEXTURE, 1, SWIZ_IDENTITY),
        END,
    ]
}

/// `ps_1_4`: two textures sampled into temporaries and blended by the second's alpha.
fn detail_ps_1_4() -> Vec<u32> {
    vec![
        PS_1_4,
        op(OP_TEXLD, 0),
        dst(REG_TEMP, 0, XYZW),
        src(REG_ADDR_OR_TEXTURE, 0, SWIZ_IDENTITY),
        op(OP_TEXLD, 0),
        dst(REG_TEMP, 1, XYZW),
        src(REG_ADDR_OR_TEXTURE, 1, SWIZ_IDENTITY),
        op(OP_MUL, 0),
        dst(REG_TEMP, 0, XYZW),
        src(REG_TEMP, 0, SWIZ_IDENTITY),
        src(REG_INPUT, 0, SWIZ_IDENTITY),
        op(OP_LRP, 0),
        dst(REG_TEMP, 0, XYZ),
        src(REG_TEMP, 1, SWIZ_W),
        src(REG_TEMP, 1, SWIZ_IDENTITY),
        src(REG_TEMP, 0, SWIZ_IDENTITY),
        END,
    ]
}

fn op(opcode: u16, operands: u32) -> u32 {
    u32::from(opcode) | (operands << 24)
}

fn reg(kind: u32, index: u16) -> u32 {
    0x8000_0000 | ((kind & 0x7) << 28) | (((kind >> 3) & 0x3) << 11) | u32::from(index)
}

fn dst(kind: u32, index: u16, mask: u8) -> u32 {
    reg(kind, index) | (u32::from(mask) << 16)
}

fn src(kind: u32, index: u16, swizzle: u8) -> u32 {
    reg(kind, index) | (u32::from(swizzle) << 16)
}

fn usage(usage: u8, index: u8) -> u32 {
    0x8000_0000 | u32::from(usage) | (u32::from(index) << 16)
}

fn def(out: &mut Vec<u32>, index: u16, value: [f32; 4]) {
    out.extend([op(OP_DEF, 5), dst(REG_CONST, index, XYZW)]);
    out.extend(value.map(f32::to_bits));
}

fn dcl(out: &mut Vec<u32>, token: u32, kind: u32, index: u16, mask: u8) {
    out.extend([op(OP_DCL, 2), token, dst(kind, index, mask)]);
}

/// Three-operand instruction: `opcode dst, a, b`.
fn alu2(out: &mut Vec<u32>, opcode: u16, dst_token: u32, a: u32, b: u32) {
    out.extend([op(opcode, 3), dst_token, a, b]);
}

/// Four-operand instruction: `opcode dst, a, b, c`.
fn alu3(out: &mut Vec<u32>, opcode: u16, dst_token: u32, [a, b, c]: [u32; 3]) {
    out.extend([op(opcode, 4), dst_token, a, b, c]);
}

/// `dp4` of `input` against the four rows of the matrix at `c[base]` into `out_reg`.
fn transform(out: &mut Vec<u32>, out_reg: (u32, u16), input: u32, base: u16) {
    for (row, mask) in [X, Y, Z, W].into_iter().enumerate() {
        let row = u16::try_from(row).expect("row fits u16");
        alu2(
            out,
            OP_DP4,
            dst(out_reg.0, out_reg.1, mask),
            input,
            src(REG_CONST, base + row, SWIZ_IDENTITY),
        );
    }
}

/// Transformed, lit, textured and fogged world geometry, `vs_2_0` or `vs_3_0`.
fn world_vs(version: u32) -> Vec<u32> {
    let sm3 = version == VS_3_0;
    let mut out = vec![version];
    dcl(
        &mut out,
        usage(D3DDECLUSAGE_POSITION, 0),
        REG_INPUT,
        0,
        XYZW,
    );
    dcl(&mut out, usage(D3DDECLUSAGE_NORMAL, 0), REG_INPUT, 1, XYZW);
    dcl(
        &mut out,
        usage(D3DDECLUSAGE_TEXCOORD, 0),
        REG_INPUT,
        2,
        XYZW,
    );
    // SM3 declares its outputs; SM2 writes the fixed output registers.
    let (position, texcoord, color, fog) = if sm3 {
        dcl(
            &mut out,
            usage(D3DDECLUSAGE_POSITION, 0),
            REG_OUTPUT,
            0,
            XYZW,
        );
        dcl(
            &mut out,
            usage(D3DDECLUSAGE_TEXCOORD, 0),
            REG_OUTPUT,
            1,
            XYZW,
        );
        dcl(&mut out, usage(D3DDECLUSAGE_COLOR, 0), REG_OUTPUT, 2, XYZW);
        dcl(&mut out, usage(D3DDECLUSAGE_FOG, 0), REG_OUTPUT, 3, X);
        (
            (REG_OUTPUT, 0),
            (REG_OUTPUT, 1),
            (REG_OUTPUT, 2),
            (REG_OUTPUT, 3),
        )
    } else {
        (
            (REG_RASTOUT, 0),
            (REG_OUTPUT, 0),
            (REG_ATTROUT, 0),
            (REG_RASTOUT, 1),
        )
    };
    def(&mut out, 8, [0.0, 1.0, 0.0, 0.0]);
    transform(&mut out, position, src(REG_INPUT, 0, SWIZ_IDENTITY), 0);
    alu2(
        &mut out,
        OP_DP3,
        dst(REG_TEMP, 0, X),
        src(REG_INPUT, 1, SWIZ_IDENTITY),
        src(REG_CONST, 4, SWIZ_IDENTITY),
    );
    alu2(
        &mut out,
        OP_MAX,
        dst(REG_TEMP, 0, X),
        src(REG_TEMP, 0, SWIZ_X),
        src(REG_CONST, 8, SWIZ_X),
    );
    alu3(
        &mut out,
        OP_MAD,
        dst(color.0, color.1, XYZW),
        [
            src(REG_TEMP, 0, SWIZ_X),
            src(REG_CONST, 5, SWIZ_IDENTITY),
            src(REG_CONST, 6, SWIZ_IDENTITY),
        ],
    );
    out.extend([
        op(OP_MOV, 2),
        dst(texcoord.0, texcoord.1, XYZW),
        src(REG_INPUT, 2, SWIZ_IDENTITY),
    ]);
    alu2(
        &mut out,
        OP_DP4,
        dst(REG_TEMP, 1, Z),
        src(REG_INPUT, 0, SWIZ_IDENTITY),
        src(REG_CONST, 2, SWIZ_IDENTITY),
    );
    alu3(
        &mut out,
        OP_MAD,
        dst(fog.0, fog.1, X),
        [
            src(REG_TEMP, 1, SWIZ_Z),
            src(REG_CONST, 7, SWIZ_X),
            src(REG_CONST, 7, SWIZ_Y),
        ],
    );
    out.push(END);
    out
}

/// A `vs_2_0` that skins through a bone palette at `c[10 + 3 * index]`.
fn skinned_vs() -> Vec<u32> {
    let mut out = vec![VS_2_0];
    dcl(
        &mut out,
        usage(D3DDECLUSAGE_POSITION, 0),
        REG_INPUT,
        0,
        XYZW,
    );
    dcl(
        &mut out,
        usage(D3DDECLUSAGE_BLENDINDICES, 0),
        REG_INPUT,
        1,
        XYZW,
    );
    dcl(
        &mut out,
        usage(D3DDECLUSAGE_TEXCOORD, 0),
        REG_INPUT,
        2,
        XYZW,
    );
    def(&mut out, 90, [3.0, 1.0, 0.0, 0.0]);
    alu2(
        &mut out,
        OP_MUL,
        dst(REG_TEMP, 0, X),
        src(REG_INPUT, 1, SWIZ_X),
        src(REG_CONST, 90, SWIZ_X),
    );
    out.extend([
        op(OP_MOVA, 2),
        dst(REG_ADDR_OR_TEXTURE, 0, X),
        src(REG_TEMP, 0, SWIZ_X),
    ]);
    for (row, mask) in [(10, X), (11, Y), (12, Z)] {
        out.extend([
            op(OP_DP4, 4),
            dst(REG_TEMP, 1, mask),
            src(REG_INPUT, 0, SWIZ_IDENTITY),
            src(REG_CONST, row, SWIZ_IDENTITY) | RELATIVE,
            src(REG_ADDR_OR_TEXTURE, 0, SWIZ_X),
        ]);
    }
    out.extend([
        op(OP_MOV, 2),
        dst(REG_TEMP, 1, W),
        src(REG_CONST, 90, SWIZ_Y),
    ]);
    transform(
        &mut out,
        (REG_RASTOUT, 0),
        src(REG_TEMP, 1, SWIZ_IDENTITY),
        0,
    );
    out.extend([
        op(OP_MOV, 2),
        dst(REG_OUTPUT, 0, XYZW),
        src(REG_INPUT, 2, SWIZ_IDENTITY),
    ]);
    out.push(END);
    out
}

/// `ps_2_0`: one texture modulated by the diffuse colour.
fn single_texture_ps() -> Vec<u32> {
    let mut out = vec![PS_2_0];
    dcl(&mut out, DCL_PLAIN, REG_ADDR_OR_TEXTURE, 0, XYZW);
    dcl(&mut out, DCL_PLAIN, REG_INPUT, 0, XYZW);
    dcl(&mut out, DCL_2D, REG_SAMPLER, 0, XYZW);
    alu2(
        &mut out,
        OP_TEXLD,
        dst(REG_TEMP, 0, XYZW),
        src(REG_ADDR_OR_TEXTURE, 0, SWIZ_IDENTITY),
        src(REG_SAMPLER, 0, SWIZ_IDENTITY),
    );
    alu2(
        &mut out,
        OP_MUL,
        dst(REG_TEMP, 0, XYZW),
        src(REG_TEMP, 0, SWIZ_IDENTITY),
        src(REG_INPUT, 0, SWIZ_IDENTITY),
    );
    out.extend([
        op(OP_MOV, 2),
        dst(REG_COLOROUT, 0, XYZW),
        src(REG_TEMP, 0, SWIZ_IDENTITY),
    ]);
    out.push(END);
    out
}

/// `ps_2_0`: two textures blended by the second one's alpha, lit and tinted.
fn two_texture_ps() -> Vec<u32> {
    let mut out = vec![PS_2_0];
    dcl(&mut out, DCL_PLAIN, REG_ADDR_OR_TEXTURE, 0, XYZW);
    dcl(&mut out, DCL_PLAIN, REG_ADDR_OR_TEXTURE, 1, XYZW);
    dcl(&mut out, DCL_PLAIN, REG_INPUT, 0, XYZW);
    dcl(&mut out, DCL_2D, REG_SAMPLER, 0, XYZW);
    dcl(&mut out, DCL_2D, REG_SAMPLER, 1, XYZW);
    for index in [0, 1] {
        alu2(
            &mut out,
            OP_TEXLD,
            dst(REG_TEMP, index, XYZW),
            src(REG_ADDR_OR_TEXTURE, index, SWIZ_IDENTITY),
            src(REG_SAMPLER, index, SWIZ_IDENTITY),
        );
    }
    alu3(
        &mut out,
        OP_LRP,
        dst(REG_TEMP, 2, XYZW),
        [
            src(REG_TEMP, 1, SWIZ_W),
            src(REG_TEMP, 1, SWIZ_IDENTITY),
            src(REG_TEMP, 0, SWIZ_IDENTITY),
        ],
    );
    alu2(
        &mut out,
        OP_MUL,
        dst(REG_TEMP, 2, XYZW),
        src(REG_TEMP, 2, SWIZ_IDENTITY),
        src(REG_INPUT, 0, SWIZ_IDENTITY),
    );
    alu2(
        &mut out,
        OP_MUL,
        dst(REG_TEMP, 2, XYZ),
        src(REG_TEMP, 2, SWIZ_IDENTITY),
        src(REG_CONST, 0, SWIZ_IDENTITY),
    );
    out.extend([
        op(OP_MOV, 2),
        dst(REG_COLOROUT, 0, XYZW),
        src(REG_TEMP, 2, SWIZ_IDENTITY),
    ]);
    out.push(END);
    out
}

/// `ps_3_0`: a normal-mapped diffuse texture with a Blinn-style specular term.
fn normal_mapped_ps() -> Vec<u32> {
    let mut out = vec![PS_3_0];
    dcl(
        &mut out,
        usage(D3DDECLUSAGE_TEXCOORD, 0),
        REG_INPUT,
        0,
        XYZW,
    );
    dcl(
        &mut out,
        usage(D3DDECLUSAGE_TEXCOORD, 1),
        REG_INPUT,
        1,
        XYZW,
    );
    dcl(&mut out, usage(D3DDECLUSAGE_COLOR, 0), REG_INPUT, 2, XYZW);
    dcl(&mut out, DCL_2D, REG_SAMPLER, 0, XYZW);
    dcl(&mut out, DCL_2D, REG_SAMPLER, 1, XYZW);
    def(&mut out, 1, [2.0, -1.0, 0.0, 16.0]);
    for sampler in [0, 1] {
        alu2(
            &mut out,
            OP_TEXLD,
            dst(REG_TEMP, sampler, XYZW),
            src(REG_INPUT, 0, SWIZ_IDENTITY),
            src(REG_SAMPLER, sampler, SWIZ_IDENTITY),
        );
    }
    alu3(
        &mut out,
        OP_MAD,
        dst(REG_TEMP, 1, XYZ),
        [
            src(REG_TEMP, 1, SWIZ_IDENTITY),
            src(REG_CONST, 1, SWIZ_X),
            src(REG_CONST, 1, SWIZ_Y),
        ],
    );
    out.extend([
        op(OP_NRM, 2),
        dst(REG_TEMP, 2, XYZ),
        src(REG_TEMP, 1, SWIZ_IDENTITY),
    ]);
    alu2(
        &mut out,
        OP_DP3,
        dst(REG_TEMP, 3, X),
        src(REG_TEMP, 2, SWIZ_IDENTITY),
        src(REG_CONST, 2, SWIZ_IDENTITY),
    );
    alu2(
        &mut out,
        OP_MAX,
        dst(REG_TEMP, 3, X),
        src(REG_TEMP, 3, SWIZ_X),
        src(REG_CONST, 1, SWIZ_Z),
    );
    out.extend([
        op(OP_NRM, 2),
        dst(REG_TEMP, 4, XYZ),
        src(REG_INPUT, 1, SWIZ_IDENTITY),
    ]);
    alu2(
        &mut out,
        OP_DP3,
        dst(REG_TEMP, 3, Y),
        src(REG_TEMP, 2, SWIZ_IDENTITY),
        src(REG_TEMP, 4, SWIZ_IDENTITY),
    );
    alu2(
        &mut out,
        OP_MAX,
        dst(REG_TEMP, 3, Y),
        src(REG_TEMP, 3, SWIZ_Y),
        src(REG_CONST, 1, SWIZ_Z),
    );
    alu2(
        &mut out,
        OP_POW,
        dst(REG_TEMP, 3, Z),
        src(REG_TEMP, 3, SWIZ_Y),
        src(REG_CONST, 1, SWIZ_W),
    );
    alu2(
        &mut out,
        OP_MUL,
        dst(REG_TEMP, 0, XYZ),
        src(REG_TEMP, 0, SWIZ_IDENTITY),
        src(REG_TEMP, 3, SWIZ_X),
    );
    alu3(
        &mut out,
        OP_MAD,
        dst(REG_TEMP, 0, XYZ),
        [
            src(REG_CONST, 3, SWIZ_IDENTITY),
            src(REG_TEMP, 3, SWIZ_Z),
            src(REG_TEMP, 0, SWIZ_IDENTITY),
        ],
    );
    alu2(
        &mut out,
        OP_MUL,
        dst(REG_COLOROUT, 0, XYZW),
        src(REG_TEMP, 0, SWIZ_IDENTITY),
        src(REG_INPUT, 2, SWIZ_IDENTITY),
    );
    out.push(END);
    out
}

// An example is not a `mod.rs` file, so the test module names its file.
#[cfg(test)]
#[path = "emit_corpus/tests.rs"]
mod tests;
