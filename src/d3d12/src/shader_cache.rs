//! D3D12 shader translation cache (`d3d12_shader_cache.{hpp,cpp}`): `CachedShader`,
//!
//! `HashCompileArgs`, `CompileFunction`, and `LogShaderCacheCounters`.

use core::{
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{
    ffi::CString,
    sync::{Arc, LazyLock, Once},
};

use airconv_ir::container::Container;
use dxmt_core::shader_cache::Store;
use winemetal_sys::{
    DispatchData_alloc_init, DispatchData_copyBytes, MTLDevice_newLibrary, MTLLibrary_newFunction,
    Obj,
    sm50::{
        AIRCONV_VERSION, MtlShaderReflection, SM50_SHADER_COMMON, SM50_SHADER_IA_INPUT_LAYOUT,
        SM50_SHADER_PSO_GEOMETRY_SHADER, SM50_SHADER_PSO_PIXEL_SHADER, SM50_SHADER_ROOT_SIGNATURE,
        SM50Compile, SM50CompileGeometryPipelineGeometry, SM50CompileGeometryPipelineVertex,
        SM50GetCompiledBitcode, SM50Initialize, Sm50CompiledBitcode, Sm50IaInputElement,
        Sm50ShaderCommonData, Sm50ShaderCompilationArgumentData, Sm50ShaderIaInputLayoutData,
        Sm50ShaderPsoGeometryShaderData, Sm50ShaderPsoPixelShaderData, Sm50ShaderRootSignatureData,
    },
};

use crate::{
    com::{E_FAIL, Hresult, S_OK},
    shader::{Sm50Bitcode, Sm50Error, Sm50Shader},
};

const D3D12_CACHE_MAGIC: u32 = 310;

static FUNCTION_HITS: AtomicU64 = AtomicU64::new(0);
static FUNCTION_MISSES: AtomicU64 = AtomicU64::new(0);
static REFLECTION_HITS: AtomicU64 = AtomicU64::new(0);
static REFLECTION_MISSES: AtomicU64 = AtomicU64::new(0);
static FUNCTION_LOOKUPS: AtomicU64 = AtomicU64::new(0);

/// 20-byte SHA-1 digest used for shader and root-signature cache keys (`dxmt::sha1_digest`).
pub type Sha1Digest = [u8; 20];

/// Stage compilation mode (`FunctionKind`, `d3d12_shader_cache.hpp:55`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionKind {
    Shader,
    GeometryVertex,
    GeometryMesh,
}

struct Sha1 {
    state: [u32; 5],
    len_bytes: u64,
    buf: [u8; 64],
    buf_len: usize,
}

impl Sha1 {
    const fn new() -> Self {
        Self {
            state: [
                0x6745_2301,
                0xefcd_ab89,
                0x98ba_dcfe,
                0x1032_5476,
                0xc3d2_e1f0,
            ],
            len_bytes: 0,
            buf: [0; 64],
            buf_len: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.len_bytes = self.len_bytes.wrapping_add(data.len() as u64);
        if self.buf_len > 0 {
            let take = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
        let (chunks, rem) = data.as_chunks::<64>();
        for chunk in chunks {
            self.compress(chunk);
        }
        if !rem.is_empty() {
            self.buf[..rem.len()].copy_from_slice(rem);
            self.buf_len = rem.len();
        }
    }

    fn finalize(mut self) -> Sha1Digest {
        let bit_len = self.len_bytes.wrapping_mul(8);
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            self.buf[self.buf_len..64].fill(0);
            let block = self.buf;
            self.compress(&block);
            self.buf_len = 0;
        }
        self.buf[self.buf_len..56].fill(0);
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        let block = self.buf;
        self.compress(&block);

        let mut out = [0_u8; 20];
        for (i, word) in self.state.into_iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut words = [0_u32; 80];
        for (idx, &chunk) in block.as_chunks::<4>().0.iter().enumerate() {
            words[idx] = u32::from_be_bytes(chunk);
        }
        for idx in 16..80 {
            words[idx] = (words[idx - 3] ^ words[idx - 8] ^ words[idx - 14] ^ words[idx - 16])
                .rotate_left(1);
        }
        let [mut s0, mut s1, mut s2, mut s3, mut s4] = self.state;
        for (idx, &word) in words.iter().enumerate() {
            let (func, konst) = match idx {
                0..=19 => ((s1 & s2) | ((!s1) & s3), 0x5a82_7999_u32),
                20..=39 => (s1 ^ s2 ^ s3, 0x6ed9_eba1_u32),
                40..=59 => ((s1 & s2) | (s1 & s3) | (s2 & s3), 0x8f1b_bcdc_u32),
                _ => (s1 ^ s2 ^ s3, 0xca62_c1d6_u32),
            };
            let temp = s0
                .rotate_left(5)
                .wrapping_add(func)
                .wrapping_add(s4)
                .wrapping_add(konst)
                .wrapping_add(word);
            s4 = s3;
            s3 = s2;
            s2 = s1.rotate_left(30);
            s1 = s0;
            s0 = temp;
        }
        self.state[0] = self.state[0].wrapping_add(s0);
        self.state[1] = self.state[1].wrapping_add(s1);
        self.state[2] = self.state[2].wrapping_add(s2);
        self.state[3] = self.state[3].wrapping_add(s3);
        self.state[4] = self.state[4].wrapping_add(s4);
    }
}

fn store() -> &'static Store {
    static STORE: LazyLock<Arc<Store>> = LazyLock::new(|| {
        let key = if env!("DXMT_TRANSLATOR_KEY").is_empty() {
            env!("DXMT_VERSION")
        } else {
            env!("DXMT_TRANSLATOR_KEY")
        };
        let ver = dxmt_core::shader_cache::version(key, AIRCONV_VERSION);
        dxmt_core::shader_cache::instance(D3D12_CACHE_MAGIC, ver)
    });
    &STORE
}

fn count_function_lookup() {
    let n = FUNCTION_LOOKUPS
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1);
    if n.is_multiple_of(1000) {
        log_counters();
    }
}

/// Computes the 20-byte SHA-1 digest of `bytes`.
#[must_use]
pub fn sha1(bytes: &[u8]) -> Sha1Digest {
    let mut h = Sha1::new();
    h.update(bytes);
    h.finalize()
}

/// Computes the 40-byte reflection cache key (`d3d12_shader_cache.cpp:97`).
#[must_use]
pub fn reflection_key(digest: &Sha1Digest) -> [u8; 40] {
    static TAG: LazyLock<Sha1Digest> = LazyLock::new(|| sha1(b"d3d12-reflection 2"));
    let mut key = [0_u8; 40];
    key[..20].copy_from_slice(digest);
    key[20..].copy_from_slice(&*TAG);
    key
}

/// Encodes [`MtlShaderReflection`] to its 72-byte cache record.
#[must_use]
pub const fn encode_reflection(refl: &MtlShaderReflection) -> [u8; 72] {
    const _: () = assert!(size_of::<MtlShaderReflection>() == 72);
    let mut out = [0_u8; 72];
    // SAFETY: `MtlShaderReflection` is a 72-byte `#[repr(C)]` struct of plain integer fields.
    let bytes = unsafe { core::slice::from_raw_parts(ptr::from_ref(refl).cast::<u8>(), 72) };
    out.copy_from_slice(bytes);
    out
}

/// Decodes a 72-byte cache record into [`MtlShaderReflection`].
#[must_use]
pub const fn decode_reflection(bytes: &[u8]) -> Option<MtlShaderReflection> {
    if bytes.len() != size_of::<MtlShaderReflection>() {
        return None;
    }
    // SAFETY: `bytes` has exact length `size_of::<MtlShaderReflection>()` and `MtlShaderReflection` is all integers.
    Some(unsafe {
        bytes
            .as_ptr()
            .cast::<MtlShaderReflection>()
            .read_unaligned()
    })
}

/// One shader stage's bytecode, digest, reflection, and lazily-parsed `SM50Shader`
///
/// (`CachedShader`, `d3d12_shader_cache.hpp:31-50`).
pub struct CachedShader {
    pub bytecode: Vec<u8>,
    pub digest: Sha1Digest,
    pub shader: Option<Sm50Shader>,
    pub reflected: bool,
    pub reflection: MtlShaderReflection,
}

impl CachedShader {
    /// Checks the container and computes its SHA-1 digest (`Initialize`,
    ///
    /// `d3d12_shader_cache.cpp:63-71`).
    ///
    /// # Errors
    ///
    /// Returns `E_FAIL` when `bytecode` is not a valid DXBC/DXIL container.
    pub fn new(bytecode: &[u8]) -> Result<Arc<Self>, Hresult> {
        let declared = Container::declared_size(bytecode)
            .filter(|&n| n <= bytecode.len())
            .ok_or(E_FAIL)?;
        if Container::parse(&bytecode[..declared]).is_err() {
            return Err(E_FAIL);
        }
        let digest = sha1(bytecode);
        // SAFETY: `MtlShaderReflection` is a `#[repr(C)]` struct of integer fields.
        let reflection: MtlShaderReflection = unsafe { zeroed() };
        Ok(Arc::new(Self {
            bytecode: bytecode.to_vec(),
            digest,
            shader: None,
            reflected: false,
            reflection,
        }))
    }

    /// The SHA-1 digest of the shader bytecode.
    #[must_use]
    pub const fn digest(&self) -> &Sha1Digest {
        &self.digest
    }

    /// Reads (or computes and caches) the shader's [`MtlShaderReflection`] (`Reflection`,
    ///
    /// `d3d12_shader_cache.cpp:95-119`).
    pub fn reflection(&mut self, out: &mut MtlShaderReflection) -> Hresult {
        if !self.reflected {
            let cache = store();
            let enabled = cache.has_reader();
            let key = reflection_key(&self.digest);
            let cached = if let Some(data) = cache.read(&key) {
                let want = size_of::<MtlShaderReflection>() as u64;
                // SAFETY: `self.reflection` is a 72-byte buffer and `data` is a live `DispatchData` handle.
                let copied = unsafe {
                    DispatchData_copyBytes(
                        data.handle(),
                        ptr::from_mut(&mut self.reflection).cast::<c_void>(),
                        want,
                    )
                };
                copied == want
            } else {
                false
            };
            if cached {
                self.reflected = true;
                REFLECTION_HITS.fetch_add(1, Ordering::Relaxed);
            } else {
                if enabled {
                    REFLECTION_MISSES.fetch_add(1, Ordering::Relaxed);
                }
                if let Err(hr) = self.parse() {
                    return hr;
                }
                if cache.has_writer() {
                    // SAFETY: `self.reflection` is a 72-byte `#[repr(C)]` struct valid for the call duration.
                    let raw_bytes = unsafe {
                        DispatchData_alloc_init(
                            ptr::from_ref(&self.reflection).addr() as u64,
                            size_of::<MtlShaderReflection>() as u64,
                        )
                    };
                    // SAFETY: `DispatchData_alloc_init` returns a +1 retained `DispatchData` handle.
                    if let Some(bytes) = unsafe { Obj::adopt(raw_bytes) } {
                        cache.write(&key, &bytes);
                    }
                }
            }
        }
        *out = self.reflection;
        S_OK
    }

    /// Lazily parses the `sm50_shader_t` handle (`Parse`, `d3d12_shader_cache.cpp:80-92`).
    ///
    /// # Errors
    ///
    /// Returns `E_FAIL` when `SM50Initialize` fails.
    pub fn parse(&mut self) -> Result<u64, Hresult> {
        if let Some(ref s) = self.shader {
            return Ok(s.0);
        }
        let mut handle = 0_u64;
        let mut err_handle = 0_u64;
        // SAFETY: `self.bytecode` is valid for its length; `handle`, `self.reflection`, and `err_handle` are writable.
        let rc = unsafe {
            SM50Initialize(
                self.bytecode.as_ptr().cast::<c_void>(),
                self.bytecode.len(),
                &raw mut handle,
                &raw mut self.reflection,
                &raw mut err_handle,
            )
        };
        let error = Sm50Error(err_handle);
        if rc != 0 {
            crate::err!("Failed to initialize shader: {}", error.message());
            return Err(E_FAIL);
        }
        self.shader = Some(Sm50Shader(handle));
        self.reflected = true;
        Ok(handle)
    }
}

/// Hashes an `SM50_SHADER_COMPILATION_ARGUMENT_DATA` chain (`HashCompileArgs`,
///
/// `d3d12_shader_cache.cpp:121-171`).
///
/// # Safety
///
/// `args` must be null or a valid null-terminated `SM50_SHADER_COMPILATION_ARGUMENT_DATA` linked
/// list whose inner array pointers are valid for their declared counts.
#[must_use]
pub unsafe fn hash_compile_args(
    args: *const Sm50ShaderCompilationArgumentData,
) -> Option<Sha1Digest> {
    let mut h = Sha1::new();
    let mut cur = args;
    while !cur.is_null() {
        // SAFETY: `cur` is a valid node in the caller-supplied linked list.
        let node = unsafe { &*cur };
        h.update(&node.r#type.to_le_bytes());
        match node.r#type {
            SM50_SHADER_COMMON => {
                // SAFETY: `node.type` identifies a `Sm50ShaderCommonData` node.
                let d = unsafe { &*cur.cast::<Sm50ShaderCommonData>() };
                h.update(&d.flags.to_le_bytes());
                h.update(&d.metal_version.to_le_bytes());
            }
            SM50_SHADER_ROOT_SIGNATURE => {
                // SAFETY: `node.type` identifies a `Sm50ShaderRootSignatureData` node.
                let d = unsafe { &*cur.cast::<Sm50ShaderRootSignatureData>() };
                let length = d.bytecode_length as u64;
                h.update(&length.to_le_bytes());
                if d.bytecode_length > 0 && !d.bytecode.is_null() {
                    // SAFETY: `d.bytecode` holds `d.bytecode_length` readable bytes.
                    let bytes = unsafe {
                        core::slice::from_raw_parts(d.bytecode.cast::<u8>(), d.bytecode_length)
                    };
                    h.update(bytes);
                }
            }
            SM50_SHADER_IA_INPUT_LAYOUT => {
                // SAFETY: `node.type` identifies a `Sm50ShaderIaInputLayoutData` node.
                let d = unsafe { &*cur.cast::<Sm50ShaderIaInputLayoutData>() };
                h.update(&d.index_buffer_format.to_le_bytes());
                h.update(&d.slot_mask.to_le_bytes());
                h.update(&d.num_elements.to_le_bytes());
                let count = usize::try_from(d.num_elements).unwrap_or(0);
                if count > 0 && !d.elements.is_null() {
                    // SAFETY: `d.elements` holds `count` `Sm50IaInputElement` entries.
                    let elems: &[Sm50IaInputElement] =
                        unsafe { core::slice::from_raw_parts(d.elements, count) };
                    for e in elems {
                        let step_function = e.step_function();
                        let step_rate = e.step_rate();
                        h.update(&e.reg.to_le_bytes());
                        h.update(&e.slot.to_le_bytes());
                        h.update(&e.aligned_byte_offset.to_le_bytes());
                        h.update(&e.format.to_le_bytes());
                        h.update(&step_function.to_le_bytes());
                        h.update(&step_rate.to_le_bytes());
                    }
                }
            }
            SM50_SHADER_PSO_PIXEL_SHADER => {
                // SAFETY: `node.type` identifies a `Sm50ShaderPsoPixelShaderData` node.
                let d = unsafe { &*cur.cast::<Sm50ShaderPsoPixelShaderData>() };
                h.update(&d.sample_mask.to_le_bytes());
                h.update(&[d.dual_source_blending]);
                h.update(&[d.disable_depth_output]);
                h.update(&d.unorm_output_reg_mask.to_le_bytes());
                for fmt in d.pixel_formats {
                    h.update(&fmt.to_le_bytes());
                }
            }
            SM50_SHADER_PSO_GEOMETRY_SHADER => {
                // SAFETY: `node.type` identifies a `Sm50ShaderPsoGeometryShaderData` node.
                let d = unsafe { &*cur.cast::<Sm50ShaderPsoGeometryShaderData>() };
                h.update(&[d.strip_topology]);
            }
            _ => return None,
        }
        cur = node.next.cast::<Sm50ShaderCompilationArgumentData>();
    }
    Some(h.finalize())
}

/// Computes the 40-byte function cache key (`d3d12_shader_cache.cpp:180-190`).
#[must_use]
pub fn function_key_from_digests(
    first: &Sha1Digest,
    second: Option<&Sha1Digest>,
    name: &str,
    variant: &Sha1Digest,
    bounds: &str,
) -> [u8; 40] {
    static TAG: LazyLock<Sha1Digest> = LazyLock::new(|| sha1(b"d3d12-function"));

    let mut shaders = Sha1::new();
    shaders.update(first);
    if let Some(sec) = second {
        shaders.update(sec);
    }
    let shaders_digest = shaders.finalize();

    let mut ver = Sha1::new();
    ver.update(&*TAG);
    ver.update(name.as_bytes());
    ver.update(variant);
    if !bounds.is_empty() {
        ver.update(bounds.as_bytes());
    }
    let ver_digest = ver.finalize();

    let mut key = [0_u8; 40];
    key[..20].copy_from_slice(&shaders_digest);
    key[20..].copy_from_slice(&ver_digest);
    key
}

fn probe_on() -> bool {
    static PROBE: LazyLock<bool> =
        LazyLock::new(|| !std::env::var("DXMT_PROBE").unwrap_or_default().is_empty());
    *PROBE
}

fn bounds_env() -> &'static str {
    static BOUNDS: LazyLock<String> =
        LazyLock::new(|| std::env::var("DXMT_DXIL_BOUNDS").unwrap_or_default());
    &BOUNDS
}

/// Environment overrides for [`compile_function_with_env`].
pub struct CompileEnv<'a> {
    pub name: &'a str,
    pub stage: &'a str,
    pub probe: bool,
    pub bounds: &'a str,
}

/// Internal entry point for [`compile_function`] taking explicit [`CompileEnv`] flags.
///
/// # Errors
///
/// Returns an `Hresult` error (`E_NOTIMPL` on `"DXIL:"`, else `E_FAIL`) if translation or Metal
/// compilation fails.
///
/// # Safety
///
/// `args` must be null or a valid `SM50_SHADER_COMPILATION_ARGUMENT_DATA` linked list.
pub unsafe fn compile_function_with_env(
    device: &Obj,
    kind: FunctionKind,
    first: &mut CachedShader,
    second: Option<&mut CachedShader>,
    args: *mut Sm50ShaderCompilationArgumentData,
    env: &CompileEnv<'_>,
) -> Result<Obj, Hresult> {
    let CompileEnv {
        name,
        stage,
        probe,
        bounds,
    } = *env;
    let c_name = CString::new(name).map_err(|_| E_FAIL)?;
    let key = if probe {
        None
    } else {
        // SAFETY: caller guarantees `args` is null or a valid argument chain.
        unsafe { hash_compile_args(args) }.map(|variant| {
            let sec_digest = second.as_deref().map(CachedShader::digest);
            function_key_from_digests(first.digest(), sec_digest, name, &variant, bounds)
        })
    };

    if let Some(ref key) = key {
        let cache = store();
        let enabled = cache.has_reader();
        if let Some(data) = cache.read(key) {
            static REJECTED_ONCE: Once = Once::new();
            // SAFETY: `device` and `data` are valid handles.
            let raw_lib =
                unsafe { MTLDevice_newLibrary(device.handle(), data.handle(), ptr::null_mut()) };
            // SAFETY: `MTLDevice_newLibrary` returns a +1 retained `MTLLibrary` handle or 0.
            let lib = unsafe { Obj::adopt(raw_lib) };
            let func = lib.and_then(|lib| {
                // SAFETY: `lib` is a live `MTLLibrary` and `c_name` is a NUL-terminated C string.
                let raw_fn = unsafe { MTLLibrary_newFunction(lib.handle(), c_name.as_ptr()) };
                // SAFETY: `MTLLibrary_newFunction` returns a +1 retained `MTLFunction` handle or 0.
                unsafe { Obj::adopt(raw_fn) }
            });
            if let Some(func) = func {
                FUNCTION_HITS.fetch_add(1, Ordering::Relaxed);
                count_function_lookup();
                return Ok(func);
            }
            REJECTED_ONCE.call_once(|| {
                crate::warn!("d3d12 shader cache: rejected a cached function, recompiling");
            });
        }
        if enabled {
            FUNCTION_MISSES.fetch_add(1, Ordering::Relaxed);
            count_function_lookup();
        }
    }

    let s0 = first.parse()?;
    let s1 = match second {
        Some(sec) => sec.parse()?,
        None => 0,
    };

    let mut raw_bitcode = 0_u64;
    let mut raw_err = 0_u64;
    let rc = match kind {
        FunctionKind::Shader => {
            // SAFETY: `s0`, `args`, `c_name`, `raw_bitcode`, and `raw_err` are valid arguments for `SM50Compile`.
            unsafe {
                SM50Compile(
                    s0,
                    args,
                    c_name.as_ptr(),
                    &raw mut raw_bitcode,
                    &raw mut raw_err,
                )
            }
        }
        FunctionKind::GeometryVertex => {
            // SAFETY: `s0`, `s1`, `args`, `c_name`, `raw_bitcode`, and `raw_err` are valid for `SM50CompileGeometryPipelineVertex`.
            unsafe {
                SM50CompileGeometryPipelineVertex(
                    s0,
                    s1,
                    args,
                    c_name.as_ptr(),
                    &raw mut raw_bitcode,
                    &raw mut raw_err,
                )
            }
        }
        FunctionKind::GeometryMesh => {
            // SAFETY: `s0`, `s1`, `args`, `c_name`, `raw_bitcode`, and `raw_err` are valid for `SM50CompileGeometryPipelineGeometry`.
            unsafe {
                SM50CompileGeometryPipelineGeometry(
                    s0,
                    s1,
                    args,
                    c_name.as_ptr(),
                    &raw mut raw_bitcode,
                    &raw mut raw_err,
                )
            }
        }
    };
    let bitcode = Sm50Bitcode(raw_bitcode);
    let error = Sm50Error(raw_err);
    if rc != 0 {
        return Err(crate::shader::shader_compile_failed(stage, &error));
    }

    let mut compiled = Sm50CompiledBitcode { data: 0, size: 0 };
    // SAFETY: `bitcode.0` was produced by `SM50Compile*` and `compiled` is writable.
    unsafe { SM50GetCompiledBitcode(bitcode.0, &raw mut compiled) };
    // SAFETY: `compiled.data` is valid for `compiled.size` bytes while `bitcode` is alive.
    let raw_data = unsafe { DispatchData_alloc_init(compiled.data, compiled.size) };
    // SAFETY: `DispatchData_alloc_init` returns a +1 retained `DispatchData` handle or 0.
    let data = unsafe { Obj::adopt(raw_data) }.ok_or(E_FAIL)?;

    // SAFETY: `device` and `data` are valid handles.
    let raw_lib = unsafe { MTLDevice_newLibrary(device.handle(), data.handle(), ptr::null_mut()) };
    // SAFETY: `MTLDevice_newLibrary` returns a +1 retained `MTLLibrary` handle or 0.
    let lib = unsafe { Obj::adopt(raw_lib) }.ok_or(E_FAIL)?;
    // SAFETY: `lib` is a live `MTLLibrary` and `c_name` is a NUL-terminated C string.
    let raw_func = unsafe { MTLLibrary_newFunction(lib.handle(), c_name.as_ptr()) };
    // SAFETY: `MTLLibrary_newFunction` returns a +1 retained `MTLFunction` handle or 0.
    let func = unsafe { Obj::adopt(raw_func) }.ok_or(E_FAIL)?;

    if let Some(ref key) = key {
        let cache = store();
        if cache.has_writer() {
            cache.write(key, &data);
        }
    }

    Ok(func)
}

/// Translates or fetches a cached Metal function (`CompileFunction`,
///
/// `d3d12_shader_cache.cpp:173-245`).
///
/// # Errors
///
/// Returns an `Hresult` error (`E_NOTIMPL` on `"DXIL:"`, else `E_FAIL`) if translation or Metal
/// compilation fails.
///
/// # Safety
///
/// `args` must be null or a valid `SM50_SHADER_COMPILATION_ARGUMENT_DATA` linked list.
pub unsafe fn compile_function(
    device: &Obj,
    kind: FunctionKind,
    first: &mut CachedShader,
    second: Option<&mut CachedShader>,
    args: *mut Sm50ShaderCompilationArgumentData,
    name: &str,
    stage: &str,
) -> Result<Obj, Hresult> {
    // SAFETY: caller guarantees `args` is null or a valid compilation argument chain.
    unsafe {
        compile_function_with_env(
            device,
            kind,
            first,
            second,
            args,
            &CompileEnv {
                name,
                stage,
                probe: probe_on(),
                bounds: bounds_env(),
            },
        )
    }
}

/// Logs `"d3d12 shader cache: functions <h> hit <m> missed, reflections <h> hit <m> missed"` if any
///
/// lookup occurred (`LogShaderCacheCounters`, `d3d12_shader_cache.cpp:248-254`).
pub fn log_counters() {
    let fh = FUNCTION_HITS.load(Ordering::Relaxed);
    let fm = FUNCTION_MISSES.load(Ordering::Relaxed);
    let rh = REFLECTION_HITS.load(Ordering::Relaxed);
    let rm = REFLECTION_MISSES.load(Ordering::Relaxed);
    if fh + fm + rh + rm == 0 {
        return;
    }
    crate::info!(
        "d3d12 shader cache: functions {fh} hit {fm} missed, reflections {rh} hit {rm} missed"
    );
}

#[cfg(test)]
mod tests;
