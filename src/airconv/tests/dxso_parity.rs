//! D3D9 parity: DXSO rendered through MSL and through AIR.
//!
//! Ruling C3-15: every pipeline recipe of the committed DXSO corpus is rendered twice
//! on the Mac's Metal device, once through D3D9's MSL emitters (the code the players run today) and
//! once through `airconv::dxso::translate`, and the pixels are compared.
//!
//! Per recipe (a vertex and a pixel shader record, the vertex descriptor and the target formats of
//! a live pipeline): the records give the DXSO and the exact specialization; the MSL pair is
//! `ShaderSource::emit` compiled as `unix/src/metal/shader.rs` does (MSL 2.4, preserve invariance,
//! fast math); the AIR pair is `translate` of the same tokens and specialization, loaded with
//! `newLibraryWithData`. A fixed-function record's MSL is `ff.rs`'s for its key, and its AIR the
//! translation of the tokens `ff_build` made from that key; a fixed-function stage reads the
//! constants the layer uploads for a fixed transform, lighting and texture state (ruling C3c-8),
//! a programmable stage the sine-shaped rows. A recipe naming a record without a source fails the
//! file, and so does a fixed-function record whose tokens this tree's `ff_build` no longer builds
//! (`ff-stale`). Both pairs get the same vertex descriptor, the same attachments, the same
//! deterministic inputs (attributes, constants, textures and samplers) and one draw of a 4x4 vertex
//! grid (triangles, or points for a point-sprite recipe) into a 64x64 target. Equal means every
//! channel within 2/255 on 8-bit formats, within 1e-3 relative (1e-5 absolute floor) on float
//! formats, NaN and Inf classes equal, and the two pipelines' used buffer, texture and sampler
//! indices (Metal's pipeline reflection) equal.
//!
//! `AIRCONV_DXSO_CORPUS` names the corpus directory (`synthetic.bin`, `conformance-x64.bin`,
//! `conformance-i686.bin` and `triaged.txt` in it). Without a Metal device or a corpus the test
//! says `dxso-parity: skipped (<reason>)` and passes. `AIRCONV_DXSO_SELF=1` renders the MSL pair
//! on both sides, which checks the harness's own determinism.

use std::{
    cell::Cell,
    collections::BTreeSet,
    env,
    ffi::{CString, c_void},
    fs,
    mem::MaybeUninit,
    path::{Path, PathBuf},
    ptr::NonNull,
};

use airconv::dxso::translate;
use airconv_ir::dxso::{
    DeclUsage, Declaration, RegKind, parse,
    spec::{StageSpec, VariantFlags, VariantKey, VsSpec},
};
use dispatch2::DispatchData;
use mtld3d_core::{
    dxso::{FfVsFlags, FfVsKey},
    ff_state::FfState,
    pipeline_state::PipelineSnapshot,
    shader_cache::{
        CacheEntry, CacheHeader, CacheReadError, ShaderSource, read_header, read_records,
    },
    shader_key::CachedKind,
};
use mtld3d_shared::{
    MetalHandle, VertexAttrDesc,
    mtl::{PixelFormat, VertexFormat, VertexStepFunction},
};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBinding, MTLBindingType, MTLBlitCommandEncoder, MTLBlitOption, MTLBuffer, MTLCommandBuffer,
    MTLCommandBufferStatus, MTLCommandEncoder, MTLCommandQueue, MTLCompareFunction,
    MTLCompileOptions, MTLCreateSystemDefaultDevice, MTLDepthStencilDescriptor,
    MTLDepthStencilState, MTLDevice, MTLFunction, MTLIndexType, MTLLanguageVersion, MTLLibrary,
    MTLLoadAction, MTLMathMode, MTLOrigin, MTLPipelineOption, MTLPixelFormat, MTLPrimitiveType,
    MTLRegion, MTLRenderCommandEncoder, MTLRenderPassDescriptor, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLResourceOptions, MTLSamplerAddressMode, MTLSamplerDescriptor,
    MTLSamplerMinMagFilter, MTLSamplerState, MTLSize, MTLStorageMode, MTLStoreAction, MTLTexture,
    MTLTextureDescriptor, MTLTextureType, MTLTextureUsage, MTLVertexDescriptor, MTLVertexFormat,
    MTLVertexStepFunction,
};
use rustc_hash::FxHashMap;

/// The render target's side, in pixels.
const SIDE: usize = 64;
/// The vertex grid's side; the draw has `GRID * GRID` vertices.
const GRID: usize = 4;
const VERTICES: usize = GRID * GRID;
/// The texture's side (and depth, for a volume).
const TEXELS: usize = 4;
/// Rows of every constant file the harness fills (the most `c#` a stage reads).
const ROWS: usize = 256;
/// What a target holds where nothing was drawn.
const CLEAR: [f64; 4] = [0.125, 0.25, 0.375, 0.5];

type Dev = Retained<ProtocolObject<dyn MTLDevice>>;
type Func = Retained<ProtocolObject<dyn MTLFunction>>;
type Pso = Retained<ProtocolObject<dyn MTLRenderPipelineState>>;
type Tex = Retained<ProtocolObject<dyn MTLTexture>>;
type Buf = Retained<ProtocolObject<dyn MTLBuffer>>;

fn us(x: u32) -> usize {
    usize::try_from(x).expect("a u32 fits usize")
}

fn small(n: usize) -> f32 {
    f32::from(u16::try_from(n).expect("a small count"))
}

fn byte_of(n: usize) -> u8 {
    u8::try_from(n % 256).expect("below 256")
}

// ---------------------------------------------------------------------------------------------
// Inputs: attributes, constants, textures.

/// Component `k` of vertex `v`'s attribute `i`, within +-4.
///
/// Zeros and negatives are in it: zero divisors for `rcp`, `rsq` and `log` (ruling C3-6).
fn attr_value(i: u32, v: usize, k: usize) -> f32 {
    const TABLE: [f32; 9] = [0.0, -1.0, 0.5, 2.0, -2.5, 4.0, 0.0, 0.25, -4.0];
    TABLE[(us(i) * 7 + v * 4 + k) % TABLE.len()]
}

/// The integer a non-float attribute format takes for `(i, v, k)`.
fn attr_hash(i: u32, v: usize, k: usize) -> usize {
    us(i) * 37 + v * 29 + k * 53
}

/// `x` as IEEE half bits (truncating, flushing denormals; the harness feeds it small values).
fn f16_bits(x: f32) -> u16 {
    let b = x.to_bits();
    let sign = (b >> 16) & 0x8000;
    let exp = (b >> 23) & 0xFF;
    let bits = if exp < 113 {
        sign
    } else if exp > 142 {
        sign | 0x7C00
    } else {
        sign | ((exp - 112) << 10) | ((b & 0x007F_FFFF) >> 13)
    };
    u16::try_from(bits).expect("half bits")
}

fn f16_value(h: u16) -> f32 {
    let sign = u32::from(h & 0x8000) << 16;
    let exp = u32::from((h >> 10) & 0x1F);
    let man = u32::from(h & 0x03FF);
    match (exp, man) {
        (0, 0) => f32::from_bits(sign),
        (0, _) => {
            let magnitude = f32::from(h & 0x03FF) * 2.0_f32.powi(-24);
            if sign == 0 { magnitude } else { -magnitude }
        }
        (31, _) => f32::from_bits(sign | 0x7F80_0000 | (man << 13)),
        _ => f32::from_bits(sign | ((exp + 112) << 23) | (man << 13)),
    }
}

/// Writes attribute `i` of vertex `v` as `format` into `out`.
fn put_attribute(out: &mut [u8], format: VertexFormat, i: u32, v: usize) {
    let words = |count: usize| (0..count).map(move |k| attr_hash(i, v, k));
    let mut at = 0;
    let mut push = |bytes: &[u8]| {
        out[at..at + bytes.len()].copy_from_slice(bytes);
        at += bytes.len();
    };
    match format {
        VertexFormat::Invalid => {}
        VertexFormat::Float
        | VertexFormat::Float2
        | VertexFormat::Float3
        | VertexFormat::Float4 => {
            for k in 0..us(format.byte_size()) / 4 {
                push(&attr_value(i, v, k).to_le_bytes());
            }
        }
        VertexFormat::Half2 | VertexFormat::Half4 => {
            for k in 0..us(format.byte_size()) / 2 {
                push(&f16_bits(attr_value(i, v, k)).to_le_bytes());
            }
        }
        VertexFormat::UChar4
        | VertexFormat::UChar4Normalized
        | VertexFormat::UChar4NormalizedBgra => {
            for h in words(4) {
                push(&[byte_of(h)]);
            }
        }
        VertexFormat::Short2 | VertexFormat::Short4 => {
            for h in words(us(format.byte_size()) / 2) {
                let value = i16::try_from(h % 11).expect("below 11") - 5;
                push(&value.to_le_bytes());
            }
        }
        VertexFormat::UShort2Normalized
        | VertexFormat::UShort4Normalized
        | VertexFormat::Short2Normalized
        | VertexFormat::Short4Normalized => {
            for h in words(us(format.byte_size()) / 2) {
                let value = u16::try_from(h * 4093 % 65536).expect("below 65536");
                push(&value.to_le_bytes());
            }
        }
    }
}

/// `rows` float4 rows, `sin(row)`-shaped, every 16th row (15, 31, ...) zero (D3D9 `c#`).
fn float_rows(rows: usize, phase: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(rows * 16);
    for r in 0..rows {
        for k in 0..4 {
            let value = if (r + 1) % 16 == 0 {
                0.0
            } else {
                0.37_f32.mul_add(small(k), small(r) + phase).sin() * 3.0
            };
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

/// Sixteen int4 rows: loops of 0-3 iterations (`i#` = count, initial value, step).
fn int_rows() -> Vec<u8> {
    let mut out = Vec::new();
    for r in 0_i32..16 {
        for value in [r % 4, (r / 4) % 3, 1, 0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

/// Every constant buffer a stage of either backend may read; all are bound on every draw.
struct Uniforms {
    vs_c: Vec<u8>,
    pos_fixup: Vec<u8>,
    vs_draw: Vec<u8>,
    vs_i: Vec<u8>,
    vs_b: Vec<u8>,
    vs_lod: Vec<u8>,
    ps_c: Vec<u8>,
    alpha_ref: Vec<u8>,
    fog: Vec<u8>,
    bump_env: Vec<u8>,
    lod_bias: Vec<u8>,
    ps_i: Vec<u8>,
    ps_b: Vec<u8>,
    ps_draw: Vec<u8>,
}

fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

impl Uniforms {
    fn new() -> Self {
        let side = 1.0 / small(SIDE);
        // PosFixup: (1/w, -1/h, depth clamp, render scale, depth bias), padded to two rows.
        let pos_fixup = floats(&[side, -side, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        // VsDraw: point (size, min, max), point scale, the inverse view, six clip planes.
        let mut vs_draw = vec![4.0, 1.0, 16.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        for row in 0..4 {
            vs_draw.extend((0..4).map(|k| if k == row { 1.0 } else { 0.0 }));
        }
        for _ in 0..6 {
            vs_draw.extend([0.0, 0.0, 0.0, 1.0]);
        }
        let lod_rows = |count: usize, width: usize| {
            floats(
                &(0..count * width)
                    .map(|n| if n % width == 0 { 0.5 } else { 1.0 })
                    .collect::<Vec<_>>(),
            )
        };
        Self {
            vs_c: float_rows(ROWS, 0.0),
            pos_fixup,
            vs_draw: floats(&vs_draw),
            vs_i: int_rows(),
            vs_b: 0x5555_5555_u32.to_le_bytes().to_vec(),
            vs_lod: lod_rows(16, 2),
            ps_c: float_rows(ROWS, 1.0),
            alpha_ref: floats(&[0.5, 0.0, 0.0, 0.0]),
            fog: floats(&[0.5, 0.25, 0.75, 1.0, 0.1, 0.9, 0.5, 0.0]),
            bump_env: floats(&[0.5, -0.25, 0.25, 0.75, 0.5, 0.25, -0.5, 1.0].repeat(4)),
            lod_bias: lod_rows(16, 4),
            ps_i: int_rows(),
            ps_b: 0x5555_5555_u32.to_le_bytes().to_vec(),
            ps_draw: floats(&[1.0, 1.0, 0.0, 0.0]),
        }
    }

    /// The inputs of a fixed-function stage (ruling C3c-8).
    ///
    /// The programmable set with the fixed-function `vs_c`, `vs_draw` and `bump_env`, and
    /// `PosFixup` from a 64x64 target at render scale 2. A pre-transformed stage also gets its
    /// depth clamp on and a depth bias, so the bias-then-clamp order shows on the depths the
    /// attributes place outside `[0, w]`.
    fn ff(rhw: bool) -> Self {
        let side = 1.0 / small(SIDE);
        let (clamp, bias) = if rhw { (1.0, 0.03) } else { (0.0, 0.0) };
        Self {
            vs_c: ff_vs_constants(rhw),
            pos_fixup: floats(&[side, -side, clamp, 2.0, bias, 0.0, 0.0, 0.0]),
            vs_draw: ff_vs_draw(),
            bump_env: ff_bump_env(),
            ..Self::new()
        }
    }
}

/// `D3DTS_VIEW`, `D3DTS_PROJECTION` and `D3DTS_WORLD` (`D3DTS_WORLDMATRIX(0)`).
const D3DTS_VIEW: u32 = 2;
const D3DTS_PROJECTION: u32 = 3;
const D3DTS_WORLD: u32 = 256;

/// Writes `$v` into the `D3DCOLORVALUE` `$c`.
macro_rules! paint {
    ($c:expr, $v:expr) => {{
        let [r, g, b, a]: [f32; 4] = $v;
        $c.r = r;
        $c.g = g;
        $c.b = b;
        $c.a = a;
    }};
}

/// The uniform set a stage of a recipe reads: the programmable one, or a fixed-function one.
// Copy: `render` indexes `Gpu::uniforms` with a recipe's field (`as usize`).
#[derive(Clone, Copy)]
enum Inputs {
    Programmable = 0,
    Ff = 1,
    /// A pre-transformed fixed-function vertex stage: `vs_c` row 0 is the viewport.
    FfRhw = 2,
}

/// The view, projection and world matrices of the fixed-function inputs, row-vector (`v * M`).
fn ff_view() -> [f32; 16] {
    let eye = [1.5_f32, 2.0, -9.0];
    let norm = |v: [f32; 3]| {
        let l = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.map(|x| x / l)
    };
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[2].mul_add(-b[1], a[1] * b[2]),
            a[0].mul_add(-b[2], a[2] * b[0]),
            a[1].mul_add(-b[0], a[0] * b[1]),
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let z = norm(eye.map(|x| -x));
    let x = norm(cross([0.0, 1.0, 0.0], z));
    let y = cross(z, x);
    [
        x[0],
        y[0],
        z[0],
        0.0, //
        x[1],
        y[1],
        z[1],
        0.0, //
        x[2],
        y[2],
        z[2],
        0.0, //
        -dot(x, eye),
        -dot(y, eye),
        -dot(z, eye),
        1.0,
    ]
}

/// A left-handed perspective: 0.9 rad vertical field, square, near 1, far 40.
fn ff_projection() -> [f32; 16] {
    let ys = 1.0 / 0.45_f32.tan();
    let (zn, zf) = (1.0_f32, 40.0_f32);
    [
        ys,
        0.0,
        0.0,
        0.0, //
        0.0,
        ys,
        0.0,
        0.0, //
        0.0,
        0.0,
        zf / (zf - zn),
        1.0, //
        0.0,
        0.0,
        -zn * zf / (zf - zn),
        0.0,
    ]
}

/// World matrix `n` of the palette: a rotation about y and x, a non-uniform scale, a translation.
fn ff_world(n: usize) -> [f32; 16] {
    let t = small(n);
    let (sy, cy) = 0.1_f32.mul_add(t, 0.5).sin_cos();
    let (sx, cx) = 0.05_f32.mul_add(t, 0.3).sin_cos();
    let scale = [0.8, 0.005_f32.mul_add(-t, 1.2), 1.0];
    // Ry * Rx, rows scaled.
    let r = [
        [cy, sy * sx, -sy * cx],
        [0.0, cx, sx],
        [sy, -cy * sx, cy * cx],
    ];
    [
        r[0][0] * scale[0],
        r[0][1] * scale[0],
        r[0][2] * scale[0],
        0.0, //
        r[1][0] * scale[1],
        r[1][1] * scale[1],
        r[1][2] * scale[1],
        0.0, //
        r[2][0] * scale[2],
        r[2][1] * scale[2],
        r[2][2] * scale[2],
        0.0, //
        0.02_f32.mul_add(t, 0.2),
        -0.1,
        0.3,
        1.0,
    ]
}

/// Texture matrix of stage `s`: a scale and an offset.
///
/// Stages 0 and 1 have a zero `w` column, so a four-component projected coordinate of theirs
/// has `w == 0` over the whole draw (the origin sample of the guarded divide); stages 2 and up
/// divide by a varying `w`.
fn ff_texture_matrix(s: usize) -> [f32; 16] {
    let t = small(s);
    let w = if s < 2 { 0.0 } else { 1.0 };
    [
        0.1_f32.mul_add(t, 0.5),
        0.0,
        0.0,
        0.1 * w, //
        0.0,
        0.75,
        0.0,
        0.2 * w, //
        0.0,
        0.0,
        1.25,
        -0.1 * w, //
        0.05 * t,
        -0.05 * t,
        0.2,
        0.5 * w,
    ]
}

/// The rows `fill` writes, read back (every row starts as zeros).
fn filled(rows: usize, fill: impl FnOnce(&mut [MaybeUninit<[f32; 4]>])) -> Vec<[f32; 4]> {
    let mut dst = vec![MaybeUninit::new([0.0; 4]); rows];
    fill(&mut dst);
    dst.into_iter()
        // SAFETY: every row was initialized by `MaybeUninit::new` above, and `fill` only writes.
        .map(|row| unsafe { row.assume_init() })
        .collect()
}

/// The fixed-function vertex constants the layer uploads for a fixed state (`ff_state.rs`).
///
/// Rows 0-3 `transpose(WORLD * VIEW)`, 4-7 `transpose(PROJ)`, 8 fog (start 2, end 18, density
/// 0.1: factors about 0.25 to 0.75 over the drawn depths), 9 ambient, 10-14 the material (power
/// 12), 15-62 eight lights (spot data in every slot, so a slot the key reads as directional or
/// point is as defined; eye space, through `fill_lights_section`), 63-94 eight texture matrices,
/// 95-254 a 40-matrix palette, `transpose(WORLD_n * VIEW)`. Pre-transformed: row 0 is the
/// logical viewport `(32, 32, 0, 0)` (width, height, x, y) the MSL divides by.
fn ff_vs_constants(rhw: bool) -> Vec<u8> {
    let mut ff = FfState::new();
    let mut matrix = *ff.transform(D3DTS_VIEW).expect("the view matrix");
    let transforms = [(D3DTS_VIEW, ff_view()), (D3DTS_PROJECTION, ff_projection())]
        .into_iter()
        .chain((0..40).map(|n| (D3DTS_WORLD + u32::try_from(n).expect("a bone"), ff_world(n))));
    for (state, m) in transforms {
        matrix.m = m;
        ff.set_transform(state, &matrix);
    }
    let mut material = *ff.material();
    paint!(material.diffuse, [0.8, 0.7, 0.6, 0.9]);
    paint!(material.ambient, [0.3, 0.35, 0.4, 1.0]);
    paint!(material.specular, [0.5, 0.45, 0.4, 1.0]);
    paint!(material.emissive, [0.05, 0.1, 0.02, 0.0]);
    material.power = 12.0;
    ff.set_material(&material);
    for i in 0..8 {
        let t = small(i);
        let mut light = FfState::enable_default_light();
        light.type_ = 2;
        paint!(
            light.diffuse,
            [
                0.05_f32.mul_add(-t, 0.9),
                0.6,
                0.05_f32.mul_add(t, 0.3),
                1.0
            ]
        );
        paint!(light.specular, [0.7, 0.05_f32.mul_add(-t, 0.7), 0.5, 0.8]);
        paint!(light.ambient, [0.1, 0.05 * t, 0.1, 1.0]);
        light.position.x = 3.0 - t;
        light.position.y = 4.0;
        light.position.z = 0.5_f32.mul_add(t, -5.0);
        light.direction.x = 0.1_f32.mul_add(t, -0.3);
        light.direction.y = -1.0;
        light.direction.z = 0.6;
        light.range = 100.0;
        light.falloff = 1.0;
        light.attenuation0 = 1.0;
        light.attenuation1 = 0.05;
        light.attenuation2 = 0.01;
        light.theta = 0.6;
        light.phi = 1.4;
        ff.set_light(i, &light);
        ff.set_light_enabled(i, true);
    }
    let key = FfVsKey {
        flags: FfVsFlags::LIGHTING_ENABLED | FfVsFlags::SPECULAR_ENABLE,
        input_tex_coord_count: 0,
        tex_coord_count: 0,
        light_active_mask: 0xFF,
        light_directional_mask: 0,
        light_spot_mask: 0xFF,
        diffuse_source: 0,
        ambient_source: 0,
        specular_source: 0,
        emissive_source: 0,
        fog_mode: 0,
        tci: [0; 8],
        passthrough: [0; 8],
        tex_coord_dims: [0; 8],
        tt_flags: [0; 8],
        vertex_blend_count: 0,
        declared_weights_count: 0,
        clip_plane_count: 0,
        reserved: 0,
    };
    let mut rows = vec![[0.0_f32; 4]; ROWS];
    rows[0..4].copy_from_slice(&filled(4, |d| ff.fill_wv_section(d)));
    rows[4..8].copy_from_slice(&filled(4, |d| ff.fill_proj_section(d)));
    rows[8] = [2.0, 18.0, 0.1, 0.0];
    rows[9] = [0.2, 0.15, 0.1, 1.0];
    rows[10..15].copy_from_slice(&filled(5, |d| ff.fill_material_section(&key, d)));
    rows[15..63].copy_from_slice(&filled(48, |d| ff.fill_lights_section(&key, d)));
    let view = *ff.transform(D3DTS_VIEW).expect("the view matrix");
    for s in 0..8 {
        matrix.m = ff_texture_matrix(s);
        rows_of(
            &FfState::transpose(&matrix).m,
            &mut rows[63 + 4 * s..67 + 4 * s],
        );
    }
    for n in 0..40 {
        matrix.m = ff_world(n);
        let bone = FfState::transpose(&FfState::mat_mul(&matrix, &view));
        rows_of(&bone.m, &mut rows[95 + 4 * n..99 + 4 * n]);
    }
    if rhw {
        rows[0] = [32.0, 32.0, 0.0, 0.0];
    }
    floats(rows.as_flattened())
}

/// The four rows of a row-major matrix.
fn rows_of(m: &[f32; 16], out: &mut [[f32; 4]]) {
    for (row, chunk) in out.iter_mut().zip(m.as_chunks::<4>().0) {
        *row = *chunk;
    }
}

/// `VsDraw` of a fixed-function draw.
///
/// Point `(4, 1, 16)`, point scale `(1, 0.1, 0.01)`, the inverse view as `vs_draw.rs` packs it
/// (`transpose(inverse(VIEW))`), six world-space planes, each cutting part of the drawn
/// geometry.
fn ff_vs_draw() -> Vec<u8> {
    let mut matrix = *FfState::new()
        .transform(D3DTS_VIEW)
        .expect("the view matrix");
    matrix.m = ff_view();
    let inverse = FfState::inverse(&matrix).expect("an invertible view");
    let mut out = vec![4.0, 1.0, 16.0, 0.0, 1.0, 0.1, 0.01, 0.0];
    out.extend(FfState::transpose(&inverse).m);
    for plane in 0..6 {
        let (s, c) = (1.05 * small(plane)).sin_cos();
        out.extend([0.4 * c, 0.4 * s, 0.1, 1.2]);
    }
    floats(&out)
}

/// `bump_env`, 16 rows, distinct per stage.
///
/// Stage `n`'s matrix at row `2n`, its luminance scale and offset at row `2n + 1`.
fn ff_bump_env() -> Vec<u8> {
    let mut out = Vec::new();
    for n in 0..8 {
        let t = small(n);
        out.extend([
            t.mul_add(1.0 / 16.0, 0.5),
            -0.25,
            0.25,
            t.mul_add(-1.0 / 16.0, 0.75),
        ]);
        out.extend([t.mul_add(0.125, 0.5), 0.125 * t, 0.0, 0.0]);
    }
    floats(&out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TexKind {
    Tex2d,
    Cube,
    Volume,
    Depth,
}

impl TexKind {
    const fn format(self, slot: u32) -> MTLPixelFormat {
        match self {
            Self::Depth => MTLPixelFormat::Depth32Float,
            _ if slot.is_multiple_of(2) => MTLPixelFormat::RGBA8Unorm,
            _ => MTLPixelFormat::RGBA16Float,
        }
    }

    const fn texel_bytes(self, slot: u32) -> usize {
        match self.format(slot) {
            MTLPixelFormat::RGBA16Float => 8,
            _ => 4,
        }
    }

    const fn slices(self) -> usize {
        match self {
            Self::Cube => 6,
            _ => 1,
        }
    }

    const fn depth(self) -> usize {
        match self {
            Self::Volume => TEXELS,
            _ => 1,
        }
    }
}

/// The texels of one slice (all depth layers of a volume), a pattern per slot, face and layer.
fn texel_bytes(kind: TexKind, slot: u32, face: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for z in 0..kind.depth() {
        for y in 0..TEXELS {
            for x in 0..TEXELS {
                match kind.format(slot) {
                    MTLPixelFormat::Depth32Float => {
                        out.extend_from_slice(&((small(x + 4 * y) + 0.5) / 16.0).to_le_bytes());
                    }
                    MTLPixelFormat::RGBA16Float => {
                        let texel = [
                            (small(x) - 0.5) * 0.75,
                            small(y).mul_add(0.5, -0.25),
                            small(face + z) * 0.4,
                            small(x + y).mul_add(-0.125, 1.0),
                        ];
                        for c in texel {
                            out.extend_from_slice(&f16_bits(c).to_le_bytes());
                        }
                    }
                    _ => {
                        let s = usize::try_from(slot).expect("a slot");
                        out.extend([
                            byte_of(x * 70 + 20 + face * 13),
                            byte_of(y * 70 + 40 + z * 50 + s * 5),
                            byte_of((x ^ y) * 80 + 30 + z * 17),
                            byte_of(255 - (x + y) * 30),
                        ]);
                    }
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The device: functions, pipelines, resources, rendering.

#[derive(Clone)]
enum Fail {
    /// The translator or emitter said no.
    Refused(String),
    /// Metal rejected what the translator made.
    Invalid(String),
}

impl Fail {
    fn text(&self) -> &str {
        match self {
            Self::Refused(t) | Self::Invalid(t) => t,
        }
    }
}

struct Gpu {
    device: Dev,
    queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    textures: FxHashMap<(TexKind, u32), Tex>,
    samplers: FxHashMap<bool, Retained<ProtocolObject<dyn MTLSamplerState>>>,
    clear_pixels: FxHashMap<usize, Vec<u8>>,
    functions: FxHashMap<(bool, CachedKind, u64), Result<Func, Fail>>,
    /// Indexed by [`Inputs`].
    uniforms: [Uniforms; 3],
    probe: Option<Func>,
    air_is_msl: bool,
}

fn buffer(device: &Dev, bytes: &[u8]) -> Result<Buf, String> {
    let mut padded = bytes.to_vec();
    padded.resize(bytes.len().max(16), 0);
    let ptr = NonNull::new(padded.as_mut_ptr().cast::<c_void>()).ok_or("a null buffer")?;
    // SAFETY: `ptr` points at `padded.len()` readable bytes, which Metal copies before returning.
    unsafe {
        device.newBufferWithBytes_length_options(
            ptr,
            padded.len(),
            MTLResourceOptions::StorageModeShared,
        )
    }
    .ok_or_else(|| "newBuffer failed".to_owned())
}

fn compile_msl(device: &Dev, source: &str, entry: &str) -> Result<Func, String> {
    let options = MTLCompileOptions::new();
    options.setLanguageVersion(MTLLanguageVersion::Version2_4);
    options.setPreserveInvariance(true);
    options.setMathMode(MTLMathMode::Fast);
    let library = device
        .newLibraryWithSource_options_error(&NSString::from_str(source), Some(&options))
        .map_err(|e| format!("{e}"))?;
    library
        .newFunctionWithName(&NSString::from_str(entry))
        .ok_or_else(|| format!("no function {entry}"))
}

fn load_air(device: &Dev, metallib: &[u8], entry: &str) -> Result<Func, String> {
    let data = DispatchData::from_bytes(metallib);
    let library = device
        .newLibraryWithData_error(&data)
        .map_err(|e| format!("{e}"))?;
    library
        .newFunctionWithName(&NSString::from_str(entry))
        .ok_or_else(|| format!("no function {entry}"))
}

/// The probe fragment function: the rasterized position, for the vertex stage's `pos-ulps`.
const PROBE_MSL: &str = "#include <metal_stdlib>\nusing namespace metal;\nstruct In { float4 position [[position]]; };\nfragment float4 probe(In in [[stage_in]]) { return in.position; }\n";

impl Gpu {
    fn new(device: Dev) -> Option<Self> {
        let queue = device.newCommandQueue()?;
        let probe = compile_msl(&device, PROBE_MSL, "probe").ok();
        Some(Self {
            device,
            queue,
            textures: FxHashMap::default(),
            samplers: FxHashMap::default(),
            clear_pixels: FxHashMap::default(),
            functions: FxHashMap::default(),
            uniforms: [Uniforms::new(), Uniforms::ff(false), Uniforms::ff(true)],
            probe,
            air_is_msl: env::var_os("AIRCONV_DXSO_SELF").is_some(),
        })
    }

    /// The function of `entry`'s record, through the MSL emitter or the AIR translator.
    fn function(&mut self, air: bool, entry: &CacheEntry) -> Result<Func, Fail> {
        let air = air && !self.air_is_msl;
        let id = (air, entry.kind, entry.key);
        if let Some(found) = self.functions.get(&id) {
            return found.clone();
        }
        let made = self.make_function(air, entry);
        let again = made.clone();
        self.functions.insert(id, made);
        again
    }

    fn make_function(&self, air: bool, entry: &CacheEntry) -> Result<Func, Fail> {
        let source = entry
            .source()
            .ok_or_else(|| Fail::Refused("a fixed-function record".to_owned()))?;
        let name = entry.kind.entry_name(entry.key);
        if air {
            let cname = CString::new(name.clone()).map_err(|e| Fail::Refused(e.to_string()))?;
            let bytes = translate(source.tokens(), &source.stage_spec(), &cname)
                .map_err(|e| Fail::Refused(String::from_utf8_lossy(e.text()).into_owned()))?;
            load_air(&self.device, &bytes, &name).map_err(Fail::Invalid)
        } else {
            let msl = source.emit(&name).map_err(Fail::Refused)?;
            compile_msl(&self.device, &msl, &name).map_err(Fail::Invalid)
        }
    }

    fn sampler(
        &mut self,
        compare: bool,
    ) -> Result<Retained<ProtocolObject<dyn MTLSamplerState>>, String> {
        if let Some(found) = self.samplers.get(&compare) {
            return Ok(found.clone());
        }
        let desc = MTLSamplerDescriptor::new();
        desc.setMinFilter(MTLSamplerMinMagFilter::Linear);
        desc.setMagFilter(MTLSamplerMinMagFilter::Linear);
        desc.setSAddressMode(MTLSamplerAddressMode::Repeat);
        desc.setTAddressMode(MTLSamplerAddressMode::Repeat);
        desc.setRAddressMode(MTLSamplerAddressMode::Repeat);
        if compare {
            desc.setCompareFunction(MTLCompareFunction::LessEqual);
        }
        let state = self
            .device
            .newSamplerStateWithDescriptor(&desc)
            .ok_or("newSamplerState failed")?;
        self.samplers.insert(compare, state.clone());
        Ok(state)
    }

    /// The 4x4 (cube, volume) texture of `kind` for sampler `slot`, made once.
    fn texture(&mut self, kind: TexKind, slot: u32) -> Result<Tex, String> {
        if let Some(found) = self.textures.get(&(kind, slot)) {
            return Ok(found.clone());
        }
        let desc = MTLTextureDescriptor::new();
        desc.setTextureType(match kind {
            TexKind::Cube => MTLTextureType::TypeCube,
            TexKind::Volume => MTLTextureType::Type3D,
            TexKind::Tex2d | TexKind::Depth => MTLTextureType::Type2D,
        });
        desc.setPixelFormat(kind.format(slot));
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setWidth(TEXELS);
        }
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setHeight(TEXELS);
        }
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe { desc.setDepth(kind.depth()) };
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setMipmapLevelCount(1);
        }
        desc.setUsage(MTLTextureUsage::ShaderRead);
        desc.setStorageMode(MTLStorageMode::Private);
        let texture = self
            .device
            .newTextureWithDescriptor(&desc)
            .ok_or("newTexture failed")?;
        let bytes_per_row = TEXELS * kind.texel_bytes(slot);
        let per_slice = bytes_per_row * TEXELS * kind.depth();
        let mut data = Vec::new();
        for face in 0..kind.slices() {
            data.extend(texel_bytes(kind, slot, face));
        }
        let staging = buffer(&self.device, &data)?;
        let cmd = self.queue.commandBuffer().ok_or("no command buffer")?;
        let blit = cmd.blitCommandEncoder().ok_or("no blit encoder")?;
        for face in 0..kind.slices() {
            // SAFETY: the staging buffer holds `per_slice` bytes at this offset, the texture was
            // made with these dimensions, and the slice index is below its slice count.
            unsafe {
                blit.copyFromBuffer_sourceOffset_sourceBytesPerRow_sourceBytesPerImage_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                    &staging,
                    face * per_slice,
                    bytes_per_row,
                    bytes_per_row * TEXELS,
                    MTLSize { width: TEXELS, height: TEXELS, depth: kind.depth() },
                    &texture,
                    face,
                    0,
                    MTLOrigin { x: 0, y: 0, z: 0 },
                );
            }
        }
        blit.endEncoding();
        cmd.commit();
        cmd.waitUntilCompleted();
        if cmd.status() != MTLCommandBufferStatus::Completed {
            return Err(format!("texture upload failed: {:?}", cmd.error()));
        }
        self.textures.insert((kind, slot), texture.clone());
        Ok(texture)
    }

    /// What a target of `format` holds after the clear alone.
    fn clear_pixel(&mut self, format: PixelFormat) -> Result<Vec<u8>, String> {
        let key = format as usize;
        if let Some(found) = self.clear_pixels.get(&key) {
            return Ok(found.clone());
        }
        let target = self.target(format)?;
        let pass = MTLRenderPassDescriptor::new();
        attach_color(&pass, 0, &target);
        let cmd = self.queue.commandBuffer().ok_or("no command buffer")?;
        let encoder = cmd
            .renderCommandEncoderWithDescriptor(&pass)
            .ok_or("no encoder")?;
        encoder.endEncoding();
        cmd.commit();
        cmd.waitUntilCompleted();
        let pixel = read_target(&target, format)?
            .get(..layout(format).ok_or("no layout")?.pixel_bytes())
            .ok_or("short target")?
            .to_vec();
        self.clear_pixels.insert(key, pixel.clone());
        Ok(pixel)
    }

    fn target(&self, format: PixelFormat) -> Result<Tex, String> {
        let desc = MTLTextureDescriptor::new();
        desc.setTextureType(MTLTextureType::Type2D);
        desc.setPixelFormat(MTLPixelFormat(format as usize));
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setWidth(SIDE);
        }
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setHeight(SIDE);
        }
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setMipmapLevelCount(1);
        }
        desc.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        desc.setStorageMode(MTLStorageMode::Shared);
        self.device
            .newTextureWithDescriptor(&desc)
            .ok_or_else(|| format!("no target of {format:?}"))
    }
}

fn attach_color(pass: &MTLRenderPassDescriptor, index: usize, texture: &Tex) {
    // SAFETY: `colorAttachments` is a non-null array of Metal's eight attachment descriptors.
    let attachment = unsafe { pass.colorAttachments().objectAtIndexedSubscript(index) };
    attachment.setTexture(Some(texture));
    attachment.setLoadAction(MTLLoadAction::Clear);
    attachment.setStoreAction(MTLStoreAction::Store);
    attachment.setClearColor(objc2_metal::MTLClearColor {
        red: CLEAR[0],
        green: CLEAR[1],
        blue: CLEAR[2],
        alpha: CLEAR[3],
    });
}

fn read_target(texture: &Tex, format: PixelFormat) -> Result<Vec<u8>, String> {
    let bytes = layout(format).ok_or("no layout")?.pixel_bytes();
    let mut out = vec![0_u8; SIDE * SIDE * bytes];
    let ptr = NonNull::new(out.as_mut_ptr().cast::<c_void>()).ok_or("a null readback")?;
    // SAFETY: `out` holds the whole `SIDE` x `SIDE` image at `bytes` per pixel, which is what the
    // region and row pitch name.
    unsafe {
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            ptr,
            SIDE * bytes,
            MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width: SIDE,
                    height: SIDE,
                    depth: 1,
                },
            },
            0,
        );
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Formats and comparison.

#[derive(Clone, Copy)]
enum Layout {
    U8(usize),
    S8(usize),
    U16(usize),
    S16(usize),
    F16(usize),
    F32(usize),
    /// A packed word of this many bytes and these field widths, low bits first.
    Packed(usize, [u32; 4]),
}

impl Layout {
    const fn pixel_bytes(self) -> usize {
        match self {
            Self::U8(n) | Self::S8(n) => n,
            Self::U16(n) | Self::S16(n) | Self::F16(n) => 2 * n,
            Self::F32(n) => 4 * n,
            Self::Packed(bytes, _) => bytes,
        }
    }

    /// The absolute tolerance of an 8-bit-class format, `None` for the float classes.
    const fn absolute(self) -> Option<f32> {
        match self {
            Self::U8(_) | Self::Packed(..) => Some(2.0 / 255.0),
            Self::S8(_) => Some(2.0 / 127.0),
            _ => None,
        }
    }

    fn decode(self, bytes: &[u8]) -> Vec<f32> {
        match self {
            Self::U8(_) => bytes.iter().map(|b| f32::from(*b) / 255.0).collect(),
            Self::S8(_) => bytes
                .iter()
                .map(|b| (f32::from(i8::from_le_bytes([*b])) / 127.0).max(-1.0))
                .collect(),
            Self::U16(_) => words(bytes).map(|w| f32::from(w) / 65535.0).collect(),
            Self::S16(_) => words(bytes)
                .map(|w| (f32::from(i16::from_le_bytes(w.to_le_bytes())) / 32767.0).max(-1.0))
                .collect(),
            Self::F16(_) => words(bytes).map(f16_value).collect(),
            Self::F32(_) => bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect(),
            Self::Packed(width, fields) => bytes
                .chunks_exact(width)
                .flat_map(|chunk| {
                    let mut raw = [0_u8; 4];
                    raw[..width].copy_from_slice(chunk);
                    let mut word = u32::from_le_bytes(raw);
                    fields
                        .into_iter()
                        .filter(|bits| *bits != 0)
                        .map(move |bits| {
                            let mask = (1_u32 << bits) - 1;
                            let value = word & mask;
                            word >>= bits;
                            f32::from(u16::try_from(value).expect("a field below 1024"))
                                / f32::from(u16::try_from(mask).expect("a mask below 1024"))
                        })
                })
                .collect::<Vec<_>>(),
        }
    }
}

fn words(bytes: &[u8]) -> impl Iterator<Item = u16> + '_ {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
}

const fn layout(format: PixelFormat) -> Option<Layout> {
    Some(match format {
        PixelFormat::A8Unorm | PixelFormat::R8Unorm => Layout::U8(1),
        PixelFormat::Rg8Unorm => Layout::U8(2),
        PixelFormat::Rg8Snorm => Layout::S8(2),
        PixelFormat::Rgba8Unorm
        | PixelFormat::Rgba8UnormSrgb
        | PixelFormat::Bgra8Unorm
        | PixelFormat::Bgra8UnormSrgb => Layout::U8(4),
        PixelFormat::Rgba8Snorm => Layout::S8(4),
        PixelFormat::R16Unorm => Layout::U16(1),
        PixelFormat::Rg16Unorm => Layout::U16(2),
        PixelFormat::Rgba16Unorm => Layout::U16(4),
        PixelFormat::Rg16Snorm => Layout::S16(2),
        PixelFormat::Rgba16Snorm => Layout::S16(4),
        PixelFormat::R16Float => Layout::F16(1),
        PixelFormat::Rg16Float => Layout::F16(2),
        PixelFormat::Rgba16Float => Layout::F16(4),
        PixelFormat::R32Float => Layout::F32(1),
        PixelFormat::Rg32Float => Layout::F32(2),
        PixelFormat::Rgba32Float => Layout::F32(4),
        PixelFormat::B5G6R5Unorm => Layout::Packed(2, [5, 6, 5, 0]),
        PixelFormat::Abgr4Unorm => Layout::Packed(2, [4, 4, 4, 4]),
        PixelFormat::Bgr5A1Unorm => Layout::Packed(2, [5, 5, 5, 1]),
        PixelFormat::Rgb10A2Unorm | PixelFormat::Bgr10A2Unorm => Layout::Packed(4, [10, 10, 10, 2]),
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Nan,
    PosInf,
    NegInf,
    Finite,
}

fn class(x: f32) -> Class {
    if x.is_nan() {
        Class::Nan
    } else if x.is_infinite() {
        if x > 0.0 {
            Class::PosInf
        } else {
            Class::NegInf
        }
    } else {
        Class::Finite
    }
}

/// Compares two images of `format`: `(max delta over finite values, equal)`.
fn compare_images(format: PixelFormat, a: &[u8], b: &[u8]) -> (f64, bool) {
    let Some(layout) = layout(format) else {
        return (f64::INFINITY, false);
    };
    let (xs, ys) = (layout.decode(a), layout.decode(b));
    let mut worst = 0.0_f64;
    let mut equal = true;
    for (x, y) in xs.iter().zip(&ys) {
        if class(*x) != class(*y) {
            equal = false;
            worst = f64::INFINITY;
            continue;
        }
        if class(*x) != Class::Finite {
            continue;
        }
        let delta = (x - y).abs();
        worst = worst.max(f64::from(delta));
        let allowed = layout
            .absolute()
            .unwrap_or_else(|| 1e-5_f32.max(1e-3 * x.abs().max(y.abs())));
        if delta > allowed {
            equal = false;
        }
    }
    (worst, equal)
}

/// The distance in representable floats between two non-NaN values.
fn ulps(a: f32, b: f32) -> u64 {
    let ordered = |f: f32| {
        let v = i32::from_ne_bytes(f.to_bits().to_ne_bytes());
        if v < 0 {
            i64::from(i32::MIN) - i64::from(v)
        } else {
            i64::from(v)
        }
    };
    (ordered(a) - ordered(b)).unsigned_abs()
}

// ---------------------------------------------------------------------------------------------
// Recipes.

struct Recipe<'a> {
    vs: &'a CacheEntry,
    ps: &'a CacheEntry,
    /// The uniforms the vertex stage reads, and the pixel stage's.
    vs_inputs: Inputs,
    ps_inputs: Inputs,
    vs_spec: VsSpec,
    ps_spec: VariantKey,
    snapshot: PipelineSnapshot,
    attrs: &'a [VertexAttrDesc],
}

impl Recipe<'_> {
    const fn points(&self) -> bool {
        self.ps_spec.flags.contains(VariantFlags::POINT_SPRITE)
    }

    const fn has_depth(&self) -> bool {
        use mtld3d_core::pipeline_state::PipelineAttachFlags as A;
        self.snapshot.attach.contains(A::HAS_DEPTH)
    }

    const fn has_stencil(&self) -> bool {
        use mtld3d_core::pipeline_state::PipelineAttachFlags as A;
        self.snapshot.attach.contains(A::HAS_STENCIL)
    }

    /// The color attachments, target 0 first: `(format, written)`.
    fn color_formats(&self) -> Vec<(PixelFormat, bool)> {
        let mut out = vec![(self.snapshot.color_format, true)];
        for (i, format) in self.snapshot.extra.formats.iter().enumerate() {
            if self.snapshot.extra.is_present(i) {
                out.push((*format, self.snapshot.ps_color_out_mask & (2 << i) != 0));
            }
        }
        out
    }
}

struct Built {
    pso: Pso,
    depth: Option<Retained<ProtocolObject<dyn MTLDepthStencilState>>>,
    bindings: BTreeSet<(&'static str, &'static str, usize)>,
}

fn used_bindings(
    stage: &'static str,
    bindings: &objc2_foundation::NSArray<ProtocolObject<dyn MTLBinding>>,
    out: &mut BTreeSet<(&'static str, &'static str, usize)>,
) {
    for binding in bindings {
        if !binding.isUsed() {
            continue;
        }
        let kind = match binding.r#type() {
            MTLBindingType::Buffer => "buffer",
            MTLBindingType::Texture => "texture",
            MTLBindingType::Sampler => "sampler",
            _ => continue,
        };
        out.insert((stage, kind, binding.index()));
    }
}

fn vertex_descriptor(r: &Recipe<'_>) -> Retained<MTLVertexDescriptor> {
    let desc = MTLVertexDescriptor::new();
    for a in r.attrs {
        // SAFETY: the attribute table has 31 entries; recipes carry indices below it.
        let entry = unsafe { desc.attributes().objectAtIndexedSubscript(us(a.attr_index)) };
        entry.setFormat(MTLVertexFormat(a.format as usize));
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            entry.setOffset(us(a.offset));
        }
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            entry.setBufferIndex(us(a.buffer_index));
        }
    }
    for (slot, layout) in r.snapshot.stream_layouts.iter().enumerate() {
        if !layout.is_used() {
            continue;
        }
        // SAFETY: the layout table has 31 entries and `slot` is below 16.
        let entry = unsafe { desc.layouts().objectAtIndexedSubscript(slot) };
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            entry.setStride(us(layout.stride));
        }
        entry.setStepFunction(match layout.step {
            VertexStepFunction::Constant => MTLVertexStepFunction::Constant,
            VertexStepFunction::PerVertex => MTLVertexStepFunction::PerVertex,
            VertexStepFunction::PerInstance => MTLVertexStepFunction::PerInstance,
        });
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            entry.setStepRate(us(layout.step_rate));
        }
    }
    desc
}

/// The pipeline of the recipe's descriptor and attachments around `vs` and `ps`.
///
/// With `probe`, one RGBA32F target and no depth, no matter the recipe's.
fn build_pipeline(
    gpu: &Gpu,
    r: &Recipe<'_>,
    vs: &Func,
    ps: &Func,
    probe: bool,
) -> Result<Built, String> {
    let desc = MTLRenderPipelineDescriptor::new();
    desc.setVertexFunction(Some(vs));
    desc.setFragmentFunction(Some(ps));
    desc.setVertexDescriptor(Some(&vertex_descriptor(r)));
    desc.setRasterSampleCount(1);
    let mut depth_state = None;
    if probe {
        // SAFETY: attachment 0 always exists.
        let color = unsafe { desc.colorAttachments().objectAtIndexedSubscript(0) };
        color.setPixelFormat(MTLPixelFormat::RGBA32Float);
    } else {
        for (i, (format, written)) in r.color_formats().into_iter().enumerate() {
            // SAFETY: at most four colour attachments are used; Metal has eight.
            let color = unsafe { desc.colorAttachments().objectAtIndexedSubscript(i) };
            color.setPixelFormat(MTLPixelFormat(format as usize));
            if !written {
                color.setWriteMask(objc2_metal::MTLColorWriteMask::empty());
            }
        }
        if r.has_depth() {
            if r.has_stencil() {
                desc.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float_Stencil8);
                desc.setStencilAttachmentPixelFormat(MTLPixelFormat::Depth32Float_Stencil8);
            } else {
                desc.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);
            }
            let ds = MTLDepthStencilDescriptor::new();
            ds.setDepthCompareFunction(MTLCompareFunction::Always);
            ds.setDepthWriteEnabled(true);
            depth_state = gpu.device.newDepthStencilStateWithDescriptor(&ds);
        }
    }
    let mut reflection = None;
    let pso = gpu
        .device
        .newRenderPipelineStateWithDescriptor_options_reflection_error(
            &desc,
            MTLPipelineOption::BindingInfo,
            Some(&mut reflection),
        )
        .map_err(|e| format!("{e}"))?;
    let mut bindings = BTreeSet::new();
    if let Some(reflection) = reflection {
        used_bindings("vs", &reflection.vertexBindings(), &mut bindings);
        used_bindings("ps", &reflection.fragmentBindings(), &mut bindings);
    }
    Ok(Built {
        pso,
        depth: depth_state,
        bindings,
    })
}

/// One vertex buffer per used slot, `VERTICES` vertices of the recipe's attributes.
fn vertex_buffers(gpu: &Gpu, r: &Recipe<'_>) -> Result<Vec<(usize, Buf)>, String> {
    let mut out = Vec::new();
    for (slot, layout) in r.snapshot.stream_layouts.iter().enumerate() {
        if !layout.is_used() {
            continue;
        }
        let stride = us(layout.stride);
        let mut data = vec![0_u8; stride * VERTICES + 64];
        for a in r.attrs.iter().filter(|a| us(a.buffer_index) == slot) {
            for v in 0..VERTICES {
                let at = v * stride + us(a.offset);
                put_attribute(&mut data[at..], a.format, a.attr_index, v);
            }
        }
        out.push((slot, buffer(&gpu.device, &data)?));
    }
    Ok(out)
}

fn index_data() -> Vec<u8> {
    let mut out = Vec::new();
    for row in 0..GRID - 1 {
        for col in 0..GRID - 1 {
            let a = row * GRID + col;
            for i in [a, a + 1, a + GRID, a + 1, a + GRID + 1, a + GRID] {
                out.extend_from_slice(&u16::try_from(i).expect("a grid index").to_le_bytes());
            }
        }
    }
    out
}

fn set_bytes_vs(encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>, bytes: &[u8], slot: usize) {
    let ptr = NonNull::new(bytes.as_ptr().cast_mut().cast::<c_void>()).expect("a slice");
    // SAFETY: `ptr` points at `bytes.len()` readable bytes (at most 4096), copied by Metal.
    unsafe { encoder.setVertexBytes_length_atIndex(ptr, bytes.len(), slot) };
}

fn set_bytes_ps(encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>, bytes: &[u8], slot: usize) {
    let ptr = NonNull::new(bytes.as_ptr().cast_mut().cast::<c_void>()).expect("a slice");
    // SAFETY: `ptr` points at `bytes.len()` readable bytes (at most 4096), copied by Metal.
    unsafe { encoder.setFragmentBytes_length_atIndex(ptr, bytes.len(), slot) };
}

/// The texture kind slot `slot` of a pixel shader is bound as, by the specialization's masks.
const fn ps_kind(v: &VariantKey, slot: u32) -> TexKind {
    let bit = 1_u16 << slot;
    if v.depth_sampler_mask & bit != 0 {
        TexKind::Depth
    } else if v.volume_sampler_mask & bit != 0 {
        TexKind::Volume
    } else if v.cube_sampler_mask & bit != 0 {
        TexKind::Cube
    } else {
        TexKind::Tex2d
    }
}

/// Renders the recipe through `built`.
///
/// The raw bytes of each color attachment (the probe: one RGBA32F).
fn render(gpu: &mut Gpu, r: &Recipe<'_>, built: &Built, probe: bool) -> Result<Rendered, String> {
    let formats: Vec<PixelFormat> = if probe {
        vec![PixelFormat::Rgba32Float]
    } else {
        r.color_formats().into_iter().map(|(f, _)| f).collect()
    };
    let targets = formats
        .iter()
        .map(|f| gpu.target(*f))
        .collect::<Result<Vec<_>, _>>()?;
    let pass = MTLRenderPassDescriptor::new();
    for (i, t) in targets.iter().enumerate() {
        attach_color(&pass, i, t);
    }
    let mut depth_texture = None;
    if !probe && r.has_depth() {
        let desc = MTLTextureDescriptor::new();
        desc.setTextureType(MTLTextureType::Type2D);
        desc.setPixelFormat(if r.has_stencil() {
            MTLPixelFormat::Depth32Float_Stencil8
        } else {
            MTLPixelFormat::Depth32Float
        });
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setWidth(SIDE);
        }
        // SAFETY: a plain descriptor property; the value is within Metal's limits.
        unsafe {
            desc.setHeight(SIDE);
        }
        desc.setUsage(MTLTextureUsage::RenderTarget);
        desc.setStorageMode(MTLStorageMode::Private);
        let depth = gpu
            .device
            .newTextureWithDescriptor(&desc)
            .ok_or("no depth target")?;
        depth_texture = Some(depth.clone());
        let attachment = pass.depthAttachment();
        attachment.setTexture(Some(&depth));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearDepth(1.0);
        if r.has_stencil() {
            let stencil = pass.stencilAttachment();
            stencil.setTexture(Some(&depth));
            stencil.setLoadAction(MTLLoadAction::Clear);
            stencil.setStoreAction(MTLStoreAction::DontCare);
        }
    }
    let streams = vertex_buffers(gpu, r)?;
    let indices = buffer(&gpu.device, &index_data())?;
    let (u, p) = (
        &gpu.uniforms[r.vs_inputs as usize],
        &gpu.uniforms[r.ps_inputs as usize],
    );
    let (vs_uniforms, ps_uniforms) = (
        [
            (&u.vs_c, 30),
            (&u.pos_fixup, 28),
            (&u.vs_draw, 27),
            (&u.vs_i, 29),
            (&u.vs_b, 26),
            (&u.vs_lod, 25),
        ],
        [
            (&p.ps_c, 15),
            (&p.alpha_ref, 14),
            (&p.fog, 13),
            (&p.bump_env, 12),
            (&p.ps_i, 11),
            (&p.ps_b, 10),
            (&p.lod_bias, 9),
            (&p.ps_draw, 8),
        ],
    );
    let cmd = gpu.queue.commandBuffer().ok_or("no command buffer")?;
    let encoder = cmd
        .renderCommandEncoderWithDescriptor(&pass)
        .ok_or("no encoder")?;
    encoder.setRenderPipelineState(&built.pso);
    if let Some(depth) = &built.depth {
        encoder.setDepthStencilState(Some(depth));
    }
    for (slot, buf) in &streams {
        // SAFETY: the buffer holds `VERTICES` vertices of the slot's stride.
        unsafe { encoder.setVertexBuffer_offset_atIndex(Some(buf), 0, *slot) };
    }
    for (bytes, slot) in vs_uniforms {
        set_bytes_vs(&encoder, bytes, slot);
    }
    for (bytes, slot) in ps_uniforms {
        set_bytes_ps(&encoder, bytes, slot);
    }
    for slot in 0..4_u32 {
        let kind = if r.vs_spec.sampler_kinds.volume_mask & (1 << slot) != 0 {
            TexKind::Volume
        } else if r.vs_spec.sampler_kinds.cube_mask & (1 << slot) != 0 {
            TexKind::Cube
        } else {
            TexKind::Tex2d
        };
        let (texture, sampler) = (gpu.texture(kind, slot)?, gpu.sampler(false)?);
        // SAFETY: slots 0-3 are within Metal's 31 vertex textures and 16 samplers.
        unsafe { encoder.setVertexTexture_atIndex(Some(&texture), us(slot)) };
        // SAFETY: as above.
        unsafe { encoder.setVertexSamplerState_atIndex(Some(&sampler), us(slot)) };
    }
    for slot in 0..16_u32 {
        let kind = ps_kind(&r.ps_spec, slot);
        let compare = r.ps_spec.depth_sampler_mask & (1 << slot) != 0
            && r.ps_spec.depth_fetch_mask & (1 << slot) == 0;
        let (texture, sampler) = (gpu.texture(kind, slot)?, gpu.sampler(compare)?);
        // SAFETY: slots 0-15 are within Metal's 128 fragment textures and 16 samplers.
        unsafe { encoder.setFragmentTexture_atIndex(Some(&texture), us(slot)) };
        // SAFETY: as above.
        unsafe { encoder.setFragmentSamplerState_atIndex(Some(&sampler), us(slot)) };
    }
    if r.points() {
        // SAFETY: the streams hold `VERTICES` vertices.
        unsafe {
            encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Point, 0, VERTICES);
        }
    } else {
        // SAFETY: the index buffer holds 54 u16 indices, all below `VERTICES`.
        unsafe {
            encoder.drawIndexedPrimitives_indexCount_indexType_indexBuffer_indexBufferOffset(
                MTLPrimitiveType::Triangle,
                (GRID - 1) * (GRID - 1) * 6,
                MTLIndexType::UInt16,
                &indices,
                0,
            );
        }
    }
    encoder.endEncoding();
    let depth_readback = match &depth_texture {
        Some(depth) => Some(blit_depth(gpu, &cmd, depth, r.has_stencil())?),
        None => None,
    };
    cmd.commit();
    cmd.waitUntilCompleted();
    if cmd.status() != MTLCommandBufferStatus::Completed {
        return Err(format!("the draw failed: {:?}", cmd.error()));
    }
    let colors = formats
        .iter()
        .zip(&targets)
        .map(|(f, t)| Ok((*f, read_target(t, *f)?)))
        .collect::<Result<Vec<_>, String>>()?;
    let depth = depth_readback.map(|buf| {
        let bytes = SIDE * SIDE * 4;
        // SAFETY: the buffer holds `bytes` bytes, shared storage, written by the completed blit.
        unsafe { std::slice::from_raw_parts(buf.contents().as_ptr().cast::<u8>(), bytes) }.to_vec()
    });
    Ok(Rendered { colors, depth })
}

/// What a render read back.
///
/// Each color attachment's bytes and, with a depth attachment, its depth plane as `SIDE * SIDE`
/// little-endian `f32`.
struct Rendered {
    colors: Vec<(PixelFormat, Vec<u8>)>,
    depth: Option<Vec<u8>>,
}

/// Encodes the depth plane's blit into a shared buffer.
///
/// The plane of `depth` (`Depth32Float`, or `Depth32Float_Stencil8` read through
/// `DepthFromDepthStencil`); the buffer is returned for reading after `cmd` ran.
fn blit_depth(
    gpu: &Gpu,
    cmd: &ProtocolObject<dyn MTLCommandBuffer>,
    depth: &Tex,
    stencil: bool,
) -> Result<Buf, String> {
    let bytes = SIDE * SIDE * 4;
    let out = buffer(&gpu.device, &vec![0_u8; bytes])?;
    let blit = cmd.blitCommandEncoder().ok_or("no blit encoder")?;
    // SAFETY: the texture is `SIDE` x `SIDE`, the buffer holds `SIDE * SIDE` 4-byte texels.
    unsafe {
        blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toBuffer_destinationOffset_destinationBytesPerRow_destinationBytesPerImage_options(
            depth,
            0,
            0,
            MTLOrigin { x: 0, y: 0, z: 0 },
            MTLSize { width: SIDE, height: SIDE, depth: 1 },
            &out,
            0,
            SIDE * 4,
            bytes,
            if stencil { MTLBlitOption::DepthFromDepthStencil } else { MTLBlitOption::empty() },
        );
    }
    blit.endEncoding();
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// The corpus.

/// A triage entry's identity: file, vertex key, pixel key, kind.
type TriageKey = (String, u64, u64, String);

/// The accepted differences: `<file> <vs key> <ps key> <kind> <reason>` per line.
///
/// `kind` is `differ`, `bind-differ`, `air-refused`, `msl-failed` or `nodraw`, so an accepted
/// kind of difference does not cover another one of the same pair.
struct Triage {
    reasons: FxHashMap<TriageKey, (String, Cell<bool>)>,
}

const KINDS: [&str; 5] = [
    "differ",
    "bind-differ",
    "air-refused",
    "msl-failed",
    "nodraw",
];

impl Triage {
    /// The reason `kind` of this pair is accepted, marking the entry used.
    fn accepted(&self, file: &str, vs: u64, ps: u64, kind: &str) -> Option<&str> {
        let (reason, used) = self
            .reasons
            .get(&(file.to_owned(), vs, ps, kind.to_owned()))?;
        used.set(true);
        Some(reason)
    }

    /// The entries of `files` no recipe used.
    fn stale(&self, files: &[&str]) -> Vec<String> {
        let mut out: Vec<String> = self
            .reasons
            .iter()
            .filter(|((file, ..), (_, used))| files.contains(&file.as_str()) && !used.get())
            .map(|((file, vs, ps, kind), _)| format!("{file} {vs:016x} {ps:016x} {kind}"))
            .collect();
        out.sort();
        out
    }
}

fn key_of(text: &str) -> Option<u64> {
    u64::from_str_radix(text.trim_start_matches("0x"), 16).ok()
}

fn read_triage(dir: &Path) -> Triage {
    let mut reasons = FxHashMap::default();
    let text = fs::read_to_string(dir.join("triaged.txt")).unwrap_or_default();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let mut parts = line.splitn(5, char::is_whitespace);
        let (Some(file), Some(vs), Some(ps), Some(kind), Some(reason)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            panic!("triaged.txt: not `<file> <vs key> <ps key> <kind> <reason>`: {line}");
        };
        let (Some(vs), Some(ps)) = (key_of(vs), key_of(ps)) else {
            panic!("triaged.txt: keys are hex: {line}");
        };
        assert!(
            KINDS.contains(&kind),
            "triaged.txt: kind {kind} is not one of {KINDS:?}: {line}"
        );
        reasons.insert(
            (file.to_owned(), vs, ps, kind.to_owned()),
            (reason.trim().to_owned(), Cell::new(false)),
        );
    }
    Triage { reasons }
}

#[derive(Default)]
struct Tally {
    pairs: usize,
    equal: usize,
    differ: usize,
    bind_differ: usize,
    air_refused: usize,
    msl_failed: usize,
    /// Recipes with a fixed-function stage.
    ff: usize,
    /// Fixed-function records whose stored tokens a fresh build of their key no longer gives.
    ff_stale: usize,
    triaged: usize,
    /// Recipes compared: not msl-failed.
    compared: usize,
    /// Recipes whose MSL render drew nothing, untriaged.
    no_draw: usize,
    /// Whether the last recipe's MSL render drew a pixel.
    last_drew: bool,
    pos_ulps: u64,
    depth_ulps: u64,
    covered: usize,
    cover_both: usize,
    cover_msl_only: usize,
    cover_air_only: usize,
}

enum Outcome {
    Equal,
    MslFailed(String),
    AirRefused(String),
    Differ(f64, String),
    BindDiffer(Vec<(&'static str, &'static str, usize)>),
}

impl Outcome {
    const fn kind(&self) -> &'static str {
        match self {
            Self::Equal => "equal",
            Self::MslFailed(_) => "msl-failed",
            Self::AirRefused(_) => "air-refused",
            Self::Differ(..) => "differ",
            Self::BindDiffer(_) => "bind-differ",
        }
    }
}

/// How many pixels of `image` hold something other than the clear value.
fn drawn(image: &[u8], clear: &[u8]) -> Vec<bool> {
    image
        .chunks_exact(clear.len())
        .map(|p| p != clear)
        .collect()
}

/// The vertex stage alone under both backends, behind the probe fragment function.
///
/// Tallies the largest distance in floats between the two rasterized positions' depth and
/// `1/w`, and where each backend drew.
fn position_probe(gpu: &mut Gpu, r: &Recipe<'_>, tally: &mut Tally) {
    let Some(probe) = gpu.probe.clone() else {
        return;
    };
    let (Ok(msl_vs), Ok(air_vs)) = (gpu.function(false, r.vs), gpu.function(true, r.vs)) else {
        return;
    };
    let run = |gpu: &mut Gpu, vs: &Func| {
        let built = build_pipeline(gpu, r, vs, &probe, true)?;
        let image = render(gpu, r, &built, true)?.colors;
        Ok::<_, String>((built, image))
    };
    let (Ok((msl_built, msl_image)), Ok((air_built, air_image))) =
        (run(gpu, &msl_vs), run(gpu, &air_vs))
    else {
        return;
    };
    let (a, b) = (&msl_image[0].1, &air_image[0].1);
    let (fa, fb) = (rgba32(a), rgba32(b));
    let is_clear = |p: &[f32]| {
        p.iter()
            .zip(CLEAR)
            .all(|(x, c)| x.to_bits() == clear_f32(c))
    };
    for (pa, pb) in fa.as_chunks::<4>().0.iter().zip(fb.as_chunks::<4>().0) {
        match (is_clear(pa), is_clear(pb)) {
            (false, false) => {
                tally.cover_both += 1;
                for k in [2, 3] {
                    if !pa[k].is_nan() && !pb[k].is_nan() {
                        tally.pos_ulps = tally.pos_ulps.max(ulps(pa[k], pb[k]));
                    }
                }
            }
            (false, true) => tally.cover_msl_only += 1,
            (true, false) => tally.cover_air_only += 1,
            (true, true) => {}
        }
    }
    for (side, mine, other) in [
        ("msl-only", &msl_built.bindings, &air_built.bindings),
        ("air-only", &air_built.bindings, &msl_built.bindings),
    ] {
        for (stage, kind, index) in mine.difference(other).filter(|(stage, ..)| *stage == "vs") {
            println!(
                "vs-probe-bind-differ {:016x} {stage} {kind} {index} {side}",
                r.vs.key
            );
        }
    }
}

fn rgba32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// `CLEAR`'s value as the float a float target holds.
fn clear_f32(c: f64) -> u32 {
    let narrowed: f32 = c.to_string().parse().expect("a float");
    narrowed.to_bits()
}

fn run_recipe(gpu: &mut Gpu, r: &Recipe<'_>, tally: &mut Tally) -> Outcome {
    tally.last_drew = false;
    let msl = (|| {
        let vs = gpu.function(false, r.vs).map_err(|f| f.text().to_owned())?;
        let ps = gpu.function(false, r.ps).map_err(|f| f.text().to_owned())?;
        let built = build_pipeline(gpu, r, &vs, &ps, false)?;
        let image = render(gpu, r, &built, false)?;
        Ok::<_, String>((built, image))
    })();
    let (msl_built, msl_image) = match msl {
        Ok(done) => done,
        Err(e) => return Outcome::MslFailed(e),
    };
    if let Some((format, image)) = msl_image.colors.first()
        && let Ok(clear) = gpu.clear_pixel(*format)
        && drawn(image, &clear).contains(&true)
    {
        tally.covered += 1;
        tally.last_drew = true;
    }
    position_probe(gpu, r, tally);
    let air_functions = (gpu.function(true, r.vs), gpu.function(true, r.ps));
    let (avert, apix) = match air_functions {
        (Ok(vs), Ok(ps)) => (vs, ps),
        (Err(f), _) | (_, Err(f)) => {
            return match f {
                Fail::Refused(t) => Outcome::AirRefused(t),
                Fail::Invalid(t) => Outcome::Differ(f64::INFINITY, format!("air-invalid: {t}")),
            };
        }
    };
    let air = build_pipeline(gpu, r, &avert, &apix, false)
        .and_then(|built| render(gpu, r, &built, false).map(|image| (built, image)));
    let (air_built, air_image) = match air {
        Ok(done) => done,
        Err(e) => return Outcome::Differ(f64::INFINITY, format!("air-invalid: {e}")),
    };
    let mut worst = 0.0_f64;
    let mut equal = true;
    let mut names = Vec::new();
    for ((format, a), (_, b)) in msl_image.colors.iter().zip(&air_image.colors) {
        let (delta, same) = compare_images(*format, a, b);
        worst = worst.max(delta);
        equal &= same;
        if !same {
            names.push(format!("{format:?}"));
        }
    }
    if let (Some(a), Some(b)) = (&msl_image.depth, &air_image.depth) {
        let (delta, same) = compare_images(PixelFormat::R32Float, a, b);
        worst = worst.max(delta);
        equal &= same;
        if !same {
            names.push("depth".to_owned());
        }
        for (x, y) in rgba32(a).into_iter().zip(rgba32(b)) {
            if !x.is_nan() && !y.is_nan() {
                tally.depth_ulps = tally.depth_ulps.max(ulps(x, y));
            }
        }
    }
    if !equal {
        return Outcome::Differ(worst, names.join(","));
    }
    let diff: Vec<_> = msl_built
        .bindings
        .symmetric_difference(&air_built.bindings)
        .copied()
        .collect();
    if diff.is_empty() {
        Outcome::Equal
    } else {
        Outcome::BindDiffer(diff)
    }
}

/// Whether a vertex program takes a pre-transformed position (`dcl_positiont`).
fn pre_transformed(tokens: &[u32]) -> bool {
    parse(tokens).is_ok_and(|program| {
        program.declarations.iter().any(|decl| {
            matches!(
                decl,
                Declaration::Semantic {
                    usage: DeclUsage::PositionT,
                    reg,
                    ..
                } if reg.kind == RegKind::Input
            )
        })
    })
}

fn run_file(gpu: &mut Gpu, dir: &Path, name: &str, triage: &Triage) -> Tally {
    let bytes = fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    match read_header(&bytes) {
        Ok(header) if header == CacheHeader::CURRENT => {}
        Ok(header) => panic!(
            "{name}: written under {header:?}, this build reads {:?}: regenerate the corpus",
            CacheHeader::CURRENT
        ),
        Err(CacheReadError::WrongMagic) => panic!("{name}: not a shader cache"),
    }
    let records = read_records(&bytes);
    let shaders: FxHashMap<_, _> = records
        .shaders
        .iter()
        .map(|s| ((s.kind, s.key), s))
        .collect();
    let mut tally = Tally {
        ff_stale: records
            .shaders
            .iter()
            .filter(|s| s.source().and_then(ShaderSource::rebuilt_ff).is_some())
            .count(),
        ..Tally::default()
    };
    for recipe in &records.pipelines {
        tally.pairs += 1;
        let (vs_ref, ps_ref) = (recipe.vs(), recipe.ps());
        let (Some(vs), Some(ps)) = (
            shaders.get(&(vs_ref.kind(), vs_ref.key())),
            shaders.get(&(ps_ref.kind(), ps_ref.key())),
        ) else {
            panic!("{name}: a recipe names a shader the file lacks");
        };
        let (vs, ps) = (*vs, *ps);
        let (Some(vs_src), Some(ps_src)) = (vs.source(), ps.source()) else {
            panic!(
                "{name}: a recipe names a record without a source ({:?} {:016x}, {:?} {:016x})",
                vs.kind, vs.key, ps.kind, ps.key
            );
        };
        if !vs.kind.is_programmable() || !ps.kind.is_programmable() {
            tally.ff += 1;
        }
        let (StageSpec::Vs(vs_spec), StageSpec::Ps(ps_spec)) =
            (vs_src.stage_spec(), ps_src.stage_spec())
        else {
            panic!("{name}: a recipe pairs the wrong stages");
        };
        let vs_inputs = if vs.kind.is_programmable() {
            Inputs::Programmable
        } else if pre_transformed(vs_src.tokens()) {
            Inputs::FfRhw
        } else {
            Inputs::Ff
        };
        let ps_inputs = if ps.kind.is_programmable() {
            Inputs::Programmable
        } else {
            Inputs::Ff
        };
        let r = Recipe {
            vs,
            ps,
            vs_inputs,
            ps_inputs,
            vs_spec,
            ps_spec,
            snapshot: recipe.resolve(MetalHandle::NULL, MetalHandle::NULL),
            attrs: recipe.vertex_attrs(),
        };
        let keys = format!("{:016x} {:016x}", vs.key, ps.key);
        let outcome = run_recipe(gpu, &r, &mut tally);
        let accepted = |kind: &str| triage.accepted(name, vs.key, ps.key, kind);
        if let Outcome::MslFailed(text) = &outcome {
            println!("msl-failed {name} {keys} {text}");
            if let Some(reason) = accepted("msl-failed") {
                println!("triaged {name} {keys} msl-failed {reason}");
                tally.triaged += 1;
            } else {
                tally.msl_failed += 1;
            }
            continue;
        }
        tally.compared += 1;
        let mut triaged_here = false;
        if !tally.last_drew {
            println!("nodraw {name} {keys}");
            if let Some(reason) = accepted("nodraw") {
                println!("triaged {name} {keys} nodraw {reason}");
                tally.triaged += 1;
                triaged_here = true;
            } else {
                tally.no_draw += 1;
            }
        }
        match &outcome {
            Outcome::Equal => {
                if !triaged_here {
                    tally.equal += 1;
                }
                continue;
            }
            Outcome::MslFailed(_) => {}
            Outcome::AirRefused(text) => println!("air-refused {name} {keys} {text}"),
            Outcome::Differ(delta, format) => println!("differ {name} {keys} {delta} {format}"),
            Outcome::BindDiffer(list) => {
                for (stage, kind, index) in list {
                    println!("bind-differ {name} {keys} {stage} {kind} {index}");
                }
            }
        }
        if let Some(reason) = accepted(outcome.kind()) {
            println!("triaged {name} {keys} {} {reason}", outcome.kind());
            if !triaged_here {
                tally.triaged += 1;
            }
            continue;
        }
        match outcome {
            Outcome::AirRefused(_) => tally.air_refused += 1,
            Outcome::Differ(..) => tally.differ += 1,
            _ => tally.bind_differ += 1,
        }
    }
    println!(
        "compared {name} {} of {} pairs, {} drew nothing",
        tally.compared, tally.pairs, tally.no_draw
    );
    println!("pos-ulps {name} {}", tally.pos_ulps);
    println!("depth-ulps {name} {}", tally.depth_ulps);
    println!(
        "pos-cover {name} {} both, {} msl only, {} air only; msl drew pixels in {}/{} pairs",
        tally.cover_both, tally.cover_msl_only, tally.cover_air_only, tally.covered, tally.pairs
    );
    println!(
        "dxso-parity: {name}: {}/{} equal, {} differ, {} bind-differ, {} air-refused, {} msl-failed, {} ff, {} ff-stale, {} triaged",
        tally.equal,
        tally.pairs,
        tally.differ,
        tally.bind_differ,
        tally.air_refused,
        tally.msl_failed,
        tally.ff,
        tally.ff_stale,
        tally.triaged
    );
    tally
}

/// The corpus files that hold fixed-function pairs, each of which must compare at least one.
///
/// `synthetic.bin` is written by `emit_corpus --write-cache` with the synthetic fixed-function
/// set. A conformance file holds fixed-function pairs once it is captured under
/// `shader.backend=air` (its fixed-function records then carry their sources); the
/// `conformance-x64.bin` of this tree was captured under `msl` and joins this list when it is
/// recaptured.
const FF_FILES: [&str; 1] = ["synthetic.bin"];

const CORPUS_FILES: [&str; 3] = [
    "synthetic.bin",
    "conformance-x64.bin",
    "conformance-i686.bin",
];

#[test]
fn dxso_parity() {
    let dir = env::var_os("AIRCONV_DXSO_CORPUS").map_or_else(
        || {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/d3d9/dxso-corpus"
            ))
        },
        PathBuf::from,
    );
    let found: Vec<&str> = CORPUS_FILES
        .into_iter()
        .filter(|f| dir.join(f).is_file())
        .collect();
    if found.is_empty() {
        println!("dxso-parity: skipped (no corpus file in {})", dir.display());
        return;
    }
    let Some(device) = MTLCreateSystemDefaultDevice() else {
        println!("dxso-parity: skipped (no Metal device)");
        return;
    };
    let Some(mut gpu) = Gpu::new(device) else {
        println!("dxso-parity: skipped (no Metal command queue)");
        return;
    };
    let absent: Vec<&str> = CORPUS_FILES
        .into_iter()
        .filter(|f| !found.contains(f))
        .collect();
    println!(
        "dxso-parity: corpus {}: found {}; absent {}",
        dir.display(),
        found.join(", "),
        absent.join(", ")
    );
    let triage = read_triage(&dir);
    let mut failures = Vec::new();
    let mut compared = 0;
    for name in &found {
        let tally = run_file(&mut gpu, &dir, name, &triage);
        compared += tally.compared;
        if tally.compared == 0 {
            failures.push(format!("{name}: no pair compared"));
        }
        if FF_FILES.contains(name) && tally.ff == 0 {
            failures.push(format!("{name}: 0 ff pairs"));
        }
        if tally.ff_stale > 0 {
            failures.push(format!(
                "{name}: {} fixed-function records stale against ff_build",
                tally.ff_stale
            ));
        }
        if tally.differ + tally.bind_differ + tally.air_refused + tally.msl_failed + tally.no_draw
            > 0
        {
            failures.push(format!(
                "{name}: {} differ, {} bind-differ, {} air-refused, {} msl-failed, {} nodraw untriaged",
                tally.differ, tally.bind_differ, tally.air_refused, tally.msl_failed, tally.no_draw
            ));
        }
    }
    // A triage line nothing used is a difference that went away or a typo: fail, so it is removed.
    // The self check renders MSL on both sides, so the entries about AIR's differences go unused.
    if !gpu.air_is_msl {
        for entry in triage.stale(&found) {
            println!("stale-triage {entry}");
            failures.push(format!("stale triage entry: {entry}"));
        }
    }
    assert!(compared > 0, "no pipeline recipe compared");
    assert!(failures.is_empty(), "dxso-parity failed: {failures:?}");
}
