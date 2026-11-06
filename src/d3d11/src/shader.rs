//! D3D11 shader objects (`d3d11_shader.{hpp,cpp}`).

use core::{
    ffi::{CStr, c_void},
    fmt::Write as _,
    ptr,
};
use std::{
    ffi::CString,
    sync::{Arc, Condvar, Mutex},
};

use dxmt_core::shader::{Entry, LoadError, Sm50Shader};
use dxmt_util::{
    com::{Qi, QiMiss, qi_table},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
    sha1::Sha1,
    sync::lock,
};
use rustc_hash::FxHashMap;
use winemetal_sys::{
    Obj,
    sm50::{AIRCONV_VERSION, MtlShaderReflection, SM50GetArgumentsInfo},
    wire::MtlSm50ShaderArgument,
};

use crate::{
    LOG,
    abi::{
        IID_ID3D10_DEVICE_CHILD, IID_ID3D10_GEOMETRY_SHADER, IID_ID3D10_PIXEL_SHADER,
        IID_ID3D10_VERTEX_SHADER, IID_ID3D11_COMPUTE_SHADER, IID_ID3D11_DEVICE_CHILD,
        IID_ID3D11_DOMAIN_SHADER, IID_ID3D11_GEOMETRY_SHADER, IID_ID3D11_HULL_SHADER,
        IID_ID3D11_PIXEL_SHADER, IID_ID3D11_VERTEX_SHADER, IID_IUNKNOWN,
        vtbl::{
            ID3D11ComputeShaderVtbl, ID3D11DomainShaderVtbl, ID3D11GeometryShaderVtbl,
            ID3D11HullShaderVtbl, ID3D11PixelShaderVtbl, ID3D11VertexShaderVtbl,
        },
    },
    com::{E_FAIL, E_INVALIDARG, Guid, Hresult, Managed, S_FALSE, S_OK, add_ref},
    d3d10,
    device::Device,
    input_layout::StreamOutputLayout,
    shader::args::{
        CompileArgsChain, FLAGS, IaLayoutChainInput, ShaderVariant, SoLayoutChainInput,
    },
};

pub mod args;

/// `IID_IMTLD3D11Shader` (`d3d11_shader.hpp:78`): `e95ba1c7-e43f-49c3-a907-4ac669c9fb42`.
pub const IID_IMTL_D3D11_SHADER: Guid = Guid::new(
    0xe95b_a1c7,
    0xe43f,
    0x49c3,
    [0xa9, 0x07, 0x4a, 0xc6, 0x69, 0xc9, 0xfb, 0x42],
);

/// Returns the 64-bit FNV-1a shader cache table version (`ShaderCacheVersion`).
#[must_use]
pub const fn shader_cache_version() -> u64 {
    let key = if env!("DXMT_TRANSLATOR_KEY").is_empty() {
        env!("DXMT_VERSION")
    } else {
        env!("DXMT_TRANSLATOR_KEY")
    };
    dxmt_core::shader_cache::version(key, AIRCONV_VERSION)
}

struct CompiledVariantEntry {
    handle: u64,
    _obj: Option<Obj>,
}

/// One in-flight compilation of a shader variant.
///
/// Other threads wait on it instead of translating and loading the same Metal library a second
/// time.
struct VariantGate {
    done: Mutex<bool>,
    finished: Condvar,
}

/// Deduplicated shader representation owned by [`crate::pipeline::PipelineCache`] (`dxmt::Shader`).
pub struct ManagedShader {
    pub sha1: [u8; 20],
    pub reflection: MtlShaderReflection,
    /// `_shader`'s handle, 0 for none.
    pub handle: u64,
    _shader: Sm50Shader,
    pub constant_buffers: Vec<MtlSm50ShaderArgument>,
    pub arguments: Vec<MtlSm50ShaderArgument>,
    pub bytecode: Vec<u8>,
    variants: Mutex<FxHashMap<ShaderVariant, CompiledVariantEntry>>,
    compiling: Mutex<FxHashMap<ShaderVariant, Arc<VariantGate>>>,
}

impl ManagedShader {
    /// Creates a `ManagedShader` from raw DXBC `bytes` via `SM50Initialize`.
    ///
    /// # Errors
    ///
    /// Returns `E_INVALIDARG` if `SM50Initialize` fails on `bytes`.
    pub fn from_bytecode(bytes: &[u8]) -> Result<Arc<Self>, Hresult> {
        let sha1 = Sha1::compute(bytes).0;
        Self::from_bytecode_with_sha1(bytes, sha1)
    }

    /// Creates a `ManagedShader` from raw DXBC `bytes` and an already-computed SHA-1.
    ///
    /// # Errors
    ///
    /// Returns `E_INVALIDARG` if `SM50Initialize` fails on `bytes`.
    pub fn from_bytecode_with_sha1(bytes: &[u8], sha1: [u8; 20]) -> Result<Arc<Self>, Hresult> {
        if bytes.is_empty() {
            return Err(E_INVALIDARG);
        }
        // SAFETY: `MtlShaderReflection` is a `#[repr(C)]` struct of integers, for which zero is valid.
        let mut reflection: MtlShaderReflection = unsafe { core::mem::zeroed() };
        let shader = match dxmt_core::shader::initialize(bytes, &mut reflection) {
            Ok(shader) => shader,
            Err(error) => {
                if error.0 != 0 {
                    let msg = error.message();
                    dxmt_util::err!(LOG, "Failed to initialize shader: {msg}");
                }
                return Err(E_FAIL);
            }
        };
        let handle = shader.0;
        let cb_len = reflection.num_constant_buffers as usize;
        let arg_len = reflection.num_arguments as usize;
        let make_zero = || MtlSm50ShaderArgument {
            r#type: 0,
            sm50_binding_slot: 0,
            flags: 0,
            structure_ptr_offset: 0,
        };
        let mut constant_buffers: Vec<MtlSm50ShaderArgument> =
            (0..cb_len).map(|_| make_zero()).collect();
        let mut arguments: Vec<MtlSm50ShaderArgument> = (0..arg_len).map(|_| make_zero()).collect();
        if handle != 0 {
            // SAFETY: `handle` is live and both slices are sized to `reflection.num_*`.
            unsafe {
                SM50GetArgumentsInfo(
                    handle,
                    if cb_len == 0 {
                        ptr::null_mut()
                    } else {
                        constant_buffers.as_mut_ptr().cast()
                    },
                    if arg_len == 0 {
                        ptr::null_mut()
                    } else {
                        arguments.as_mut_ptr().cast()
                    },
                );
            }
        }
        Ok(Arc::new(Self {
            sha1,
            reflection,
            handle,
            _shader: shader,
            constant_buffers,
            arguments,
            bytecode: bytes.to_vec(),
            variants: Mutex::new(FxHashMap::default()),
            compiling: Mutex::new(FxHashMap::default()),
        }))
    }

    /// Creates a synthetic `ManagedShader` (with `handle == 0`) for unit tests.
    #[must_use]
    pub fn new_synthetic(sha1: [u8; 20], reflection: MtlShaderReflection) -> Arc<Self> {
        Arc::new(Self {
            sha1,
            reflection,
            handle: 0,
            _shader: Sm50Shader(0),
            constant_buffers: Vec::new(),
            arguments: Vec::new(),
            bytecode: Vec::new(),
            variants: Mutex::new(FxHashMap::default()),
            compiling: Mutex::new(FxHashMap::default()),
        })
    }

    /// Returns a cached compiled Metal function handle for `variant`, or inserts `create()`.
    pub fn get_or_compile_variant<F>(&self, variant: ShaderVariant, create: F) -> u64
    where
        F: FnOnce() -> u64,
    {
        let mut map = lock(&self.variants);
        if let Some(entry) = map.get(&variant) {
            return entry.handle;
        }
        let h = create();
        map.insert(
            variant,
            CompiledVariantEntry {
                handle: h,
                _obj: None,
            },
        );
        h
    }

    /// Dumps raw DXBC bytecode when `debug_assertions` is enabled (`CachedSM50Shader::dump`).
    pub fn dump(&self) {
        if cfg!(debug_assertions) && !self.bytecode.is_empty() {
            let mut hex = String::with_capacity(40);
            for &b in &self.sha1 {
                let _ = write!(hex, "{b:02x}");
            }
            let path = format!("shader_dump_{hex}.cso");
            let _ = std::fs::write(&path, &self.bytecode);
            dxmt_util::warn!(LOG, "shader dumped to ./{path}");
        }
    }

    /// Compiles (or loads from [`dxmt_core::shader_cache`]) a Metal function for `variant`.
    #[must_use]
    pub fn compile_variant(
        &self,
        dev: &Device,
        variant: &ShaderVariant,
        ia_input: Option<IaLayoutChainInput<'_>>,
        so_input: Option<SoLayoutChainInput<'_>>,
        companion: Option<&Self>,
    ) -> u64 {
        {
            let map = lock(&self.variants);
            if let Some(entry) = map.get(variant) {
                return entry.handle;
            }
        }

        let gate = {
            let mut compiling = lock(&self.compiling);
            compiling.get(variant).map(Arc::clone).map_or_else(
                || {
                    let gate = Arc::new(VariantGate {
                        done: Mutex::new(false),
                        finished: Condvar::new(),
                    });
                    compiling.insert(variant.clone(), Arc::clone(&gate));
                    None
                },
                Some,
            )
        };
        if let Some(gate) = gate {
            let mut done = lock(&gate.done);
            while !*done {
                done = gate
                    .finished
                    .wait(done)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            drop(done);
            return lock(&self.variants)
                .get(variant)
                .map_or(0, |entry| entry.handle);
        }

        let handle = self.compile_variant_uncached(dev, variant, ia_input, so_input, companion);
        let removed = {
            let mut compiling = lock(&self.compiling);
            compiling.remove(variant)
        };
        if let Some(gate) = removed {
            *lock(&gate.done) = true;
            gate.finished.notify_all();
        }
        handle
    }

    fn compile_variant_uncached(
        &self,
        dev: &Device,
        variant: &ShaderVariant,
        ia_input: Option<IaLayoutChainInput<'_>>,
        so_input: Option<SoLayoutChainInput<'_>>,
        companion: Option<&Self>,
    ) -> u64 {
        let flags = *FLAGS;
        let metal_version = dev.core().metal_version();
        let (variant_digest, func_name) = variant.digest_and_name(&self.sha1, flags);
        let c_func_name = CString::new(func_name.as_str()).unwrap_or_default();
        let mut cache_key = [0u8; 40];
        cache_key[..20].copy_from_slice(&self.sha1);
        cache_key[20..].copy_from_slice(&variant_digest);

        let mtl_dev = dev.core().device();
        let scache = dxmt_core::shader_cache::instance(metal_version, shader_cache_version());

        if let Some(lib_data) = scache.read(&cache_key) {
            match dxmt_core::shader::load_function(mtl_dev, &lib_data, &c_func_name) {
                Ok(fn_obj) => {
                    let h = fn_obj.handle();
                    lock(&self.variants).insert(
                        variant.clone(),
                        CompiledVariantEntry {
                            handle: h,
                            _obj: Some(fn_obj),
                        },
                    );
                    return h;
                }
                Err(LoadError::Function) => {
                    dxmt_util::err!(LOG, "Failed to create MTLFunction from cache: {func_name}");
                }
                Err(LoadError::Library) => LOG.err("Failed to create MTLLibrary from cache"),
            }
        }

        if self.handle == 0 {
            return 0;
        }

        let chain = CompileArgsChain::build_with_version(
            variant,
            &self.sha1,
            metal_version,
            flags,
            ia_input,
            so_input,
        );
        let companion_handle = companion.map_or(0, |s| s.handle);
        let (entry, first, second) = match *variant {
            ShaderVariant::Default
            | ShaderVariant::Vertex { .. }
            | ShaderVariant::Pixel { .. }
            | ShaderVariant::VertexStreamOutput { .. } => (Entry::Shader, self.handle, 0),
            ShaderVariant::TessellationVertexHull { .. } => {
                (Entry::TessellationHull, companion_handle, self.handle)
            }
            ShaderVariant::TessellationDomain { .. } => {
                (Entry::TessellationDomain, companion_handle, self.handle)
            }
            ShaderVariant::GeometryVertex { .. } => {
                (Entry::GeometryVertex, self.handle, companion_handle)
            }
            ShaderVariant::Geometry { .. } => (Entry::Geometry, companion_handle, self.handle),
        };
        // SAFETY: `chain` built the NUL-terminated name and lives across the compile.
        let chain_name = unsafe { CStr::from_ptr(chain.func_name_cstr()) };
        // SAFETY: `self.handle` and `companion_handle` are live shaders (the variant has its
        // companion), and `chain`'s head is a valid argument chain.
        let bitcode = match unsafe {
            dxmt_core::shader::compile(&entry, first, second, chain.head_ptr(), chain_name)
        } {
            Ok(bitcode) => bitcode,
            Err(error) => {
                if error.0 != 0 {
                    let msg = error.message();
                    dxmt_util::err!(LOG, "Failed to compile shader: {msg}");
                }
                return 0;
            }
        };

        let lib_data = dxmt_core::shader::compiled_data(&bitcode);
        let Some(lib) = lib_data
            .as_ref()
            .and_then(|data| dxmt_core::shader::new_library(mtl_dev, data))
        else {
            LOG.err("Failed to create MTLLibrary");
            self.dump();
            return 0;
        };
        if let Some(ref ld) = lib_data {
            scache.write(&cache_key, ld);
        }
        drop(bitcode);

        let Some(fn_obj) = dxmt_core::shader::new_function(&lib, chain_name) else {
            dxmt_util::err!(LOG, "Failed to create MTLFunction: {func_name}");
            self.dump();
            return 0;
        };

        let h = fn_obj.handle();
        let mut map = lock(&self.variants);
        map.insert(
            variant.clone(),
            CompiledVariantEntry {
                handle: h,
                _obj: Some(fn_obj),
            },
        );
        h
    }
}

/// D3D11 `ID3D11{Vertex,Pixel,Geometry,Hull,Domain,Compute}Shader` payload (`TShaderBase`).
pub struct Shader {
    pub managed: Arc<ManagedShader>,
    pub so_layout: Option<StreamOutputLayout>,
}

/// Heap-allocated COM wrapper for any `ID3D11*Shader` interface.
#[repr(C)]
pub struct ShaderObject<V: 'static> {
    pub vtbl: &'static V,
    pub refs: Managed,
    pub device: *mut c_void,
    pub private_data: PrivateData,
    pub d3d10: d3d10::Wrapper<u8>,
    pub inner: Shader,
}

impl Shader {
    /// Recovers `&Shader` from any `ID3D11*Shader*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: `iface` is a non-null COM interface pointer.
        let vtbl = unsafe { *iface.cast::<*const c_void>() };
        if !ptr::eq(vtbl, (&raw const VS_VTBL).cast())
            && !ptr::eq(vtbl, (&raw const PS_VTBL).cast())
            && !ptr::eq(vtbl, (&raw const GS_VTBL).cast())
            && !ptr::eq(vtbl, (&raw const HS_VTBL).cast())
            && !ptr::eq(vtbl, (&raw const DS_VTBL).cast())
            && !ptr::eq(vtbl, (&raw const CS_VTBL).cast())
        {
            return None;
        }
        let obj = iface.cast::<ShaderObject<ID3D11VertexShaderVtbl>>();
        // SAFETY: all 6 `ShaderObject<V>` instantiations share the exact same `#[repr(C)]` prefix.
        Some(unsafe { &(*obj).inner })
    }
}

unsafe extern "system-unwind" fn shader_add_ref<V: 'static>(this: *mut c_void) -> u32 {
    let obj = this.cast::<ShaderObject<V>>();
    // SAFETY: `this` is a live `ShaderObject<V>`.
    let obj_ref = unsafe { &*obj };
    // SAFETY: `obj_ref.device` is null or a valid device pointer.
    unsafe { obj_ref.refs.add_ref(obj_ref.device) }
}

unsafe extern "system-unwind" fn shader_release<V: 'static>(this: *mut c_void) -> u32 {
    let obj = this.cast::<ShaderObject<V>>();
    // SAFETY: `this` is a live `ShaderObject<V>`.
    let obj_ref = unsafe { &*obj };
    // SAFETY: `obj_ref.device` is null or a valid device pointer.
    let (remaining, recycle) = unsafe { obj_ref.refs.release(obj_ref.device) };
    if recycle {
        // SAFETY: both public and private counts reached 0; free the heap allocation.
        drop(unsafe { Box::from_raw(obj) });
    }
    remaining
}

unsafe extern "system-unwind" fn shader_get_device<V: 'static>(
    this: *mut c_void,
    out: *mut *mut c_void,
) {
    if out.is_null() {
        return;
    }
    let obj = this.cast::<ShaderObject<V>>();
    // SAFETY: `this` is a live `ShaderObject<V>`.
    let dev = unsafe { (*obj).device };
    // SAFETY: `dev` is null or a valid device pointer.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null and writable.
    unsafe { *out = dev };
}

impl<V: 'static> HasPrivateData for ShaderObject<V> {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `ShaderObject<V>`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

macro_rules! define_shader_vtbl {
    ($vtbl_static:ident, $VtblTy:ident, $qi_fn:ident, $iid:ident, $d3d10:expr) => {
        unsafe extern "system-unwind" fn $qi_fn(
            this: *mut c_void,
            riid: *const c_void,
            out: *mut *mut c_void,
        ) -> i32 {
            // SAFETY: `this` is a valid `ShaderObject`; only the field's address is taken.
            let d3d10 = unsafe {
                (&raw mut (*this.cast::<ShaderObject<$VtblTy>>()).d3d10).cast::<c_void>()
            };
            let take_ref = || {
                // SAFETY: `this` is a valid `ShaderObject`.
                unsafe { shader_add_ref::<$VtblTy>(this) };
            };
            let qi = Qi {
                null_riid: E_INVALIDARG,
                // A stream-output layout is a geometry shader to the runtime, not to us.
                silent: &[crate::input_layout::IID_IMTL_D3D11_STREAM_OUTPUT_LAYOUT],
                log: &LOG,
                miss: QiMiss::Once {
                    object: &$iid,
                    name: "D3D11Shader",
                },
            };
            // SAFETY: the application's `riid` and `out`, null or valid.
            unsafe {
                qi_table(
                    riid,
                    out,
                    &[
                        (
                            &[
                                IID_IUNKNOWN,
                                IID_ID3D11_DEVICE_CHILD,
                                $iid,
                                IID_IMTL_D3D11_SHADER,
                            ] as &[Guid],
                            this,
                        ),
                        ($d3d10 as &[Guid], d3d10),
                    ],
                    take_ref,
                    &qi,
                )
            }
        }

        static $vtbl_static: $VtblTy = $VtblTy {
            query_interface: $qi_fn,
            add_ref: shader_add_ref::<$VtblTy>,
            release: shader_release::<$VtblTy>,
            get_device: shader_get_device::<$VtblTy>,
            get_private_data: get_private_data::<ShaderObject<$VtblTy>>,
            set_private_data: set_private_data::<ShaderObject<$VtblTy>>,
            set_private_data_interface: set_private_data_interface::<ShaderObject<$VtblTy>>,
        };
    };
}

define_shader_vtbl!(
    VS_VTBL,
    ID3D11VertexShaderVtbl,
    vs_qi,
    IID_ID3D11_VERTEX_SHADER,
    &[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_VERTEX_SHADER]
);
define_shader_vtbl!(
    PS_VTBL,
    ID3D11PixelShaderVtbl,
    ps_qi,
    IID_ID3D11_PIXEL_SHADER,
    &[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_PIXEL_SHADER]
);
define_shader_vtbl!(
    GS_VTBL,
    ID3D11GeometryShaderVtbl,
    gs_qi,
    IID_ID3D11_GEOMETRY_SHADER,
    &[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_GEOMETRY_SHADER]
);
define_shader_vtbl!(
    HS_VTBL,
    ID3D11HullShaderVtbl,
    hs_qi,
    IID_ID3D11_HULL_SHADER,
    &[]
);
define_shader_vtbl!(
    DS_VTBL,
    ID3D11DomainShaderVtbl,
    ds_qi,
    IID_ID3D11_DOMAIN_SHADER,
    &[]
);
define_shader_vtbl!(
    CS_VTBL,
    ID3D11ComputeShaderVtbl,
    cs_qi,
    IID_ID3D11_COMPUTE_SHADER,
    &[]
);

static DUMMY_D3D10_VTBL: u8 = 0;

fn allocate_shader_object<V: 'static>(
    vtbl: &'static V,
    d3d10_vtbl: &'static u8,
    device: *mut c_void,
    managed: Arc<ManagedShader>,
    so_layout: Option<StreamOutputLayout>,
) -> *mut c_void {
    let imm_ctx =
        Device::from_iface(device).map_or(ptr::null_mut(), |d| d.immediate().cast::<c_void>());
    let boxed = Box::new(ShaderObject {
        vtbl,
        refs: Managed::new(),
        device,
        private_data: PrivateData::default(),
        d3d10: d3d10::Wrapper::new(d3d10_vtbl, ptr::null_mut(), imm_ctx),
        inner: Shader { managed, so_layout },
    });
    // SAFETY: `device` is null or a live COM device pointer.
    unsafe { boxed.refs.add_ref(device) };
    let raw = Box::into_raw(boxed);
    // SAFETY: `raw` is a valid heap allocation; point `d3d10.d3d11` at its D3D11 interface.
    unsafe {
        (*raw).d3d10.d3d11 = raw.cast::<c_void>();
    }
    raw.cast()
}

fn create_shader_common<V: 'static>(
    vtbl: &'static V,
    d3d10_vtbl: &'static u8,
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if !class_linkage.is_null() {
        LOG.warn("CreateShader: ClassLinkage is not supported");
    }
    if bytecode.is_null() || length == 0 {
        return E_INVALIDARG;
    }
    // SAFETY: `bytecode` is non-null and `length > 0`.
    let bytes = unsafe { core::slice::from_raw_parts(bytecode.cast::<u8>(), length) };
    let managed = if let Some(dev) = Device::from_iface(device) {
        match dev.pipeline_cache().get_or_create_shader(bytes) {
            Ok(s) => s,
            Err(hr) => return hr,
        }
    } else {
        match ManagedShader::from_bytecode(bytes) {
            Ok(s) => s,
            Err(hr) => return hr,
        }
    };
    if out.is_null() {
        return S_FALSE;
    }
    let iface = allocate_shader_object(vtbl, d3d10_vtbl, device, managed, None);
    // SAFETY: `out` is non-null.
    unsafe { *out = iface };
    S_OK
}

/// `ID3D11Device::CreateVertexShader` (`d3d11_device.cpp`).
pub fn create_vertex_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let d3d10_vtbl =
        // SAFETY: `VERTEX_SHADER_VTBL` is a static vtable.
        unsafe { &*(&raw const d3d10::VERTEX_SHADER_VTBL).cast::<u8>() };
    create_shader_common(
        &VS_VTBL,
        d3d10_vtbl,
        device,
        bytecode,
        length,
        class_linkage,
        out,
    )
}

/// `ID3D11Device::CreatePixelShader` (`d3d11_device.cpp`).
pub fn create_pixel_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let d3d10_vtbl =
        // SAFETY: `PIXEL_SHADER_VTBL` is a static vtable.
        unsafe { &*(&raw const d3d10::PIXEL_SHADER_VTBL).cast::<u8>() };
    create_shader_common(
        &PS_VTBL,
        d3d10_vtbl,
        device,
        bytecode,
        length,
        class_linkage,
        out,
    )
}

/// `ID3D11Device::CreateGeometryShader` (`d3d11_device.cpp`).
pub fn create_geometry_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let d3d10_vtbl =
        // SAFETY: `GEOMETRY_SHADER_VTBL` is a static vtable.
        unsafe { &*(&raw const d3d10::GEOMETRY_SHADER_VTBL).cast::<u8>() };
    create_shader_common(
        &GS_VTBL,
        d3d10_vtbl,
        device,
        bytecode,
        length,
        class_linkage,
        out,
    )
}

/// `ID3D11Device::CreateHullShader` (`d3d11_device.cpp`).
pub fn create_hull_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(
        &HS_VTBL,
        &DUMMY_D3D10_VTBL,
        device,
        bytecode,
        length,
        class_linkage,
        out,
    )
}

/// `ID3D11Device::CreateDomainShader` (`d3d11_device.cpp`).
pub fn create_domain_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(
        &DS_VTBL,
        &DUMMY_D3D10_VTBL,
        device,
        bytecode,
        length,
        class_linkage,
        out,
    )
}

/// `ID3D11Device::CreateComputeShader` (`d3d11_device.cpp`).
pub fn create_compute_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(
        &CS_VTBL,
        &DUMMY_D3D10_VTBL,
        device,
        bytecode,
        length,
        class_linkage,
        out,
    )
}
