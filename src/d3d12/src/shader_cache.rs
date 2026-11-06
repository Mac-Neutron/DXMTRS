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
    sync::{Arc, LazyLock, Mutex, Once},
};

use airconv_ir::container::Container;
use dxmt_core::shader_cache::Store;
use dxmt_util::{sha1::Sha1, sync::lock};
use rustc_hash::FxHashMap;
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

static REFLECTION_MEM_CACHE: LazyLock<Mutex<FxHashMap<Sha1Digest, MtlShaderReflection>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

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
    Sha1::compute(bytes).0
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
            let mem_hit = lock(&REFLECTION_MEM_CACHE).get(&self.digest).copied();
            if let Some(mem_refl) = mem_hit {
                self.reflection = mem_refl;
                self.reflected = true;
                REFLECTION_HITS.fetch_add(1, Ordering::Relaxed);
                *out = self.reflection;
                return S_OK;
            }
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
            lock(&REFLECTION_MEM_CACHE).insert(self.digest, self.reflection);
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
    Some(h.finalize().0)
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
    let shaders_digest = shaders.finalize().0;

    let mut ver = Sha1::new();
    ver.update(&*TAG);
    ver.update(name.as_bytes());
    ver.update(variant);
    if !bounds.is_empty() {
        ver.update(bounds.as_bytes());
    }
    let ver_digest = ver.finalize().0;

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

/// Per-device cache of compiled Metal functions.
///
/// A PSO names functions by shader digest, entry point and compile arguments, not by PSO. Games
/// create many PSOs from the same handful of shaders; the disk cache still has to build a Metal
/// library and function object for each use unless those objects are shared here.
#[derive(Default)]
pub struct ShaderFunctionCache {
    functions: Mutex<FxHashMap<[u8; 40], Obj>>,
}

impl ShaderFunctionCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached function for `key`, with a reference of its own.
    #[must_use]
    pub fn get(&self, key: &[u8; 40]) -> Option<Obj> {
        let handle = {
            let functions = lock(&self.functions);
            functions.get(key)?.handle()
        };
        // SAFETY: the handle belongs to a live cached object; the returned `Obj` owns its reference.
        unsafe { Obj::retain(handle) }
    }

    /// Drops every cached function, before the Metal device they belong to.
    pub fn clear(&self) {
        lock(&self.functions).clear();
    }

    /// Caches `function` under `key`, keeping a reference of its own.
    pub fn insert(&self, key: [u8; 40], function: &Obj) {
        // SAFETY: `function` is live; the cache owns the reference it takes.
        let Some(cached) = (unsafe { Obj::retain(function.handle()) }) else {
            return;
        };
        lock(&self.functions).insert(key, cached);
    }
}

/// The entry point a shader function is compiled under.
pub struct CompileTarget<'a> {
    /// Metal entry-point name.
    pub name: &'a str,
    /// Shader stage, for error messages.
    pub stage: &'a str,
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
    cache: Option<&ShaderFunctionCache>,
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
        if let Some(function) = cache.and_then(|cache| cache.get(key)) {
            FUNCTION_HITS.fetch_add(1, Ordering::Relaxed);
            count_function_lookup();
            return Ok(function);
        }
        let disk = store();
        let enabled = disk.has_reader();
        if let Some(data) = disk.read(key) {
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
                if let Some(cache) = cache {
                    cache.insert(*key, &func);
                }
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
        let disk = store();
        if disk.has_writer() {
            disk.write(key, &data);
        }
        if let Some(cache) = cache {
            cache.insert(*key, &func);
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
    cache: &ShaderFunctionCache,
    kind: FunctionKind,
    first: &mut CachedShader,
    second: Option<&mut CachedShader>,
    args: *mut Sm50ShaderCompilationArgumentData,
    target: &CompileTarget<'_>,
) -> Result<Obj, Hresult> {
    // SAFETY: caller guarantees `args` is null or a valid compilation argument chain.
    unsafe {
        compile_function_with_env(
            device,
            Some(cache),
            kind,
            first,
            second,
            args,
            &CompileEnv {
                name: target.name,
                stage: target.stage,
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
