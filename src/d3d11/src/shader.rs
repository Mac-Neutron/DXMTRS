//! D3D11 shader objects (`d3d11_shader.{hpp,cpp}`).

use core::{ffi::c_void, mem::MaybeUninit, ptr};
use std::sync::{Arc, Mutex};

use dxmt_util::{private_data::PrivateData, sha1::Sha1};
use rustc_hash::FxHashMap;
use winemetal_sys::sm50::{
    MtlShaderReflection, MtlSm50ShaderArgument, SM50Destroy, SM50FreeError, SM50GetArgumentsInfo,
    SM50Initialize, error_message,
};

use crate::{
    LOG,
    abi::{
        IID_ID3D11_COMPUTE_SHADER, IID_ID3D11_DEVICE_CHILD, IID_ID3D11_DOMAIN_SHADER,
        IID_ID3D11_GEOMETRY_SHADER, IID_ID3D11_HULL_SHADER, IID_ID3D11_PIXEL_SHADER,
        IID_ID3D11_VERTEX_SHADER, IID_IUNKNOWN,
        vtbl::{
            ID3D11ComputeShaderVtbl, ID3D11DomainShaderVtbl, ID3D11GeometryShaderVtbl,
            ID3D11HullShaderVtbl, ID3D11PixelShaderVtbl, ID3D11VertexShaderVtbl,
        },
    },
    com::{E_INVALIDARG, E_NOINTERFACE, E_POINTER, Guid, Hresult, Managed, S_FALSE, S_OK, add_ref},
    device::Device,
    input_layout::StreamOutputLayout,
    shader::args::ShaderVariant,
};

pub mod args;

/// Deduplicated shader representation owned by [`crate::pipeline::PipelineCache`] (`dxmt::Shader`).
pub struct ManagedShader {
    pub sha1: [u8; 20],
    pub reflection: MtlShaderReflection,
    pub handle: u64,
    pub constant_buffers: Vec<MtlSm50ShaderArgument>,
    pub arguments: Vec<MtlSm50ShaderArgument>,
    pub bytecode: Vec<u8>,
    variants: Mutex<FxHashMap<ShaderVariant, u64>>,
}

impl Drop for ManagedShader {
    fn drop(&mut self) {
        if self.handle != 0 {
            // SAFETY: `self.handle` is a live `sm50_shader_t` returned by `SM50Initialize`.
            unsafe { SM50Destroy(self.handle) };
        }
    }
}

impl ManagedShader {
    /// Creates a `ManagedShader` from raw DXBC `bytes` via `SM50Initialize`.
    ///
    /// # Errors
    ///
    /// Returns `E_INVALIDARG` if `SM50Initialize` fails on `bytes`.
    pub fn from_bytecode(bytes: &[u8]) -> Result<Arc<Self>, Hresult> {
        if bytes.is_empty() {
            return Err(E_INVALIDARG);
        }
        let sha1 = Sha1::compute(bytes).0;
        let mut handle = 0u64;
        let mut reflection = MaybeUninit::<MtlShaderReflection>::zeroed();
        let mut err = 0u64;
        // SAFETY: `bytes` is a valid slice, and output pointers are valid local variables.
        let status = unsafe {
            SM50Initialize(
                bytes.as_ptr().cast(),
                bytes.len(),
                &raw mut handle,
                reflection.as_mut_ptr(),
                &raw mut err,
            )
        };
        if status != 0 {
            if err != 0 {
                // SAFETY: `err` was returned by `SM50Initialize`.
                let msg = unsafe { error_message(err) };
                LOG.err(&format!("Failed to initialize shader: {msg}"));
                // SAFETY: `err` is freed once here.
                unsafe { SM50FreeError(err) };
            }
            return Err(E_INVALIDARG);
        }
        // SAFETY: `SM50Initialize` succeeded and initialized `reflection`.
        let reflection = unsafe { reflection.assume_init() };
        let cb_len = reflection.num_constant_buffers as usize;
        let arg_len = reflection.num_arguments as usize;
        let zero_arg = MtlSm50ShaderArgument {
            r#type: 0,
            sm50_binding_slot: 0,
            flags: 0,
            structure_ptr_offset: 0,
        };
        let mut constant_buffers = vec![zero_arg; cb_len];
        let mut arguments = vec![zero_arg; arg_len];
        if handle != 0 {
            // SAFETY: `handle` is live and both slices are sized to `reflection.num_*`.
            unsafe {
                SM50GetArgumentsInfo(
                    handle,
                    if cb_len == 0 {
                        ptr::null_mut()
                    } else {
                        constant_buffers.as_mut_ptr()
                    },
                    if arg_len == 0 {
                        ptr::null_mut()
                    } else {
                        arguments.as_mut_ptr()
                    },
                );
            }
        }
        Ok(Arc::new(Self {
            sha1,
            reflection,
            handle,
            constant_buffers,
            arguments,
            bytecode: bytes.to_vec(),
            variants: Mutex::new(FxHashMap::default()),
        }))
    }

    /// Creates a synthetic `ManagedShader` (with `handle == 0`) for unit tests.
    #[must_use]
    pub fn new_synthetic(sha1: [u8; 20], reflection: MtlShaderReflection) -> Arc<Self> {
        Arc::new(Self {
            sha1,
            reflection,
            handle: 0,
            constant_buffers: Vec::new(),
            arguments: Vec::new(),
            bytecode: Vec::new(),
            variants: Mutex::new(FxHashMap::default()),
        })
    }

    /// Returns a cached compiled Metal function handle for `variant`, or inserts `create()`.
    pub fn get_or_compile_variant<F>(&self, variant: ShaderVariant, create: F) -> u64
    where
        F: FnOnce() -> u64,
    {
        let mut map = self
            .variants
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(&h) = map.get(&variant) {
            return h;
        }
        let h = create();
        map.insert(variant, h);
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
    pub inner: Shader,
}

impl Shader {
    /// Recovers `&Shader` from any `ID3D11*Shader*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
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

unsafe extern "system-unwind" fn shader_get_private_data<V: 'static>(
    this: *mut c_void,
    guid: *const c_void,
    size: *mut u32,
    data: *mut c_void,
) -> i32 {
    // SAFETY: reads `guid`.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    let obj = this.cast::<ShaderObject<V>>();
    // SAFETY: `this` is valid.
    let pd = unsafe { &(*obj).private_data };
    // SAFETY: forwards COM arguments.
    unsafe { pd.get_data(g, size, data) }
}

unsafe extern "system-unwind" fn shader_set_private_data<V: 'static>(
    this: *mut c_void,
    guid: *const c_void,
    size: u32,
    data: *const c_void,
) -> i32 {
    // SAFETY: reads `guid`.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    let obj = this.cast::<ShaderObject<V>>();
    // SAFETY: `this` is valid.
    let pd = unsafe { &(*obj).private_data };
    // SAFETY: forwards COM arguments.
    unsafe { pd.set_data(g, size, data) }
}

unsafe extern "system-unwind" fn shader_set_private_data_interface<V: 'static>(
    this: *mut c_void,
    guid: *const c_void,
    iface: *const c_void,
) -> i32 {
    // SAFETY: reads `guid`.
    let Some(g) = (unsafe { Guid::read(guid.cast::<Guid>()) }) else {
        return E_INVALIDARG;
    };
    let obj = this.cast::<ShaderObject<V>>();
    // SAFETY: `this` is valid.
    let pd = unsafe { &(*obj).private_data };
    // SAFETY: forwards COM arguments.
    unsafe { pd.set_interface(g, iface.cast_mut()) }
}

macro_rules! define_shader_vtbl {
    ($vtbl_static:ident, $VtblTy:ident, $qi_fn:ident, $iid:ident) => {
        unsafe extern "system-unwind" fn $qi_fn(
            this: *mut c_void,
            riid: *const c_void,
            out: *mut *mut c_void,
        ) -> i32 {
            if out.is_null() {
                return E_POINTER;
            }
            // SAFETY: `out` is non-null.
            unsafe { *out = ptr::null_mut() };
            // SAFETY: reads `riid`.
            let Some(iid) = (unsafe { Guid::read(riid.cast::<Guid>()) }) else {
                return E_INVALIDARG;
            };
            if iid == IID_IUNKNOWN || iid == IID_ID3D11_DEVICE_CHILD || iid == $iid {
                // SAFETY: `this` is a valid `ShaderObject`.
                unsafe { shader_add_ref::<$VtblTy>(this) };
                // SAFETY: `out` is non-null.
                unsafe { *out = this };
                return S_OK;
            }
            E_NOINTERFACE
        }

        static $vtbl_static: $VtblTy = $VtblTy {
            query_interface: $qi_fn,
            add_ref: shader_add_ref::<$VtblTy>,
            release: shader_release::<$VtblTy>,
            get_device: shader_get_device::<$VtblTy>,
            get_private_data: shader_get_private_data::<$VtblTy>,
            set_private_data: shader_set_private_data::<$VtblTy>,
            set_private_data_interface: shader_set_private_data_interface::<$VtblTy>,
        };
    };
}

define_shader_vtbl!(
    VS_VTBL,
    ID3D11VertexShaderVtbl,
    vs_qi,
    IID_ID3D11_VERTEX_SHADER
);
define_shader_vtbl!(
    PS_VTBL,
    ID3D11PixelShaderVtbl,
    ps_qi,
    IID_ID3D11_PIXEL_SHADER
);
define_shader_vtbl!(
    GS_VTBL,
    ID3D11GeometryShaderVtbl,
    gs_qi,
    IID_ID3D11_GEOMETRY_SHADER
);
define_shader_vtbl!(HS_VTBL, ID3D11HullShaderVtbl, hs_qi, IID_ID3D11_HULL_SHADER);
define_shader_vtbl!(
    DS_VTBL,
    ID3D11DomainShaderVtbl,
    ds_qi,
    IID_ID3D11_DOMAIN_SHADER
);
define_shader_vtbl!(
    CS_VTBL,
    ID3D11ComputeShaderVtbl,
    cs_qi,
    IID_ID3D11_COMPUTE_SHADER
);

fn allocate_shader_object<V: 'static>(
    vtbl: &'static V,
    device: *mut c_void,
    managed: Arc<ManagedShader>,
    so_layout: Option<StreamOutputLayout>,
) -> *mut c_void {
    let boxed = Box::new(ShaderObject {
        vtbl,
        refs: Managed::new(),
        device,
        private_data: PrivateData::default(),
        inner: Shader { managed, so_layout },
    });
    // SAFETY: `device` is null or a live COM device pointer.
    unsafe { boxed.refs.add_ref(device) };
    Box::into_raw(boxed).cast()
}

fn create_shader_common<V: 'static>(
    vtbl: &'static V,
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
    let iface = allocate_shader_object(vtbl, device, managed, None);
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
    create_shader_common(&VS_VTBL, device, bytecode, length, class_linkage, out)
}

/// `ID3D11Device::CreatePixelShader` (`d3d11_device.cpp`).
pub fn create_pixel_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(&PS_VTBL, device, bytecode, length, class_linkage, out)
}

/// `ID3D11Device::CreateGeometryShader` (`d3d11_device.cpp`).
pub fn create_geometry_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(&GS_VTBL, device, bytecode, length, class_linkage, out)
}

/// `ID3D11Device::CreateHullShader` (`d3d11_device.cpp`).
pub fn create_hull_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(&HS_VTBL, device, bytecode, length, class_linkage, out)
}

/// `ID3D11Device::CreateDomainShader` (`d3d11_device.cpp`).
pub fn create_domain_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(&DS_VTBL, device, bytecode, length, class_linkage, out)
}

/// `ID3D11Device::CreateComputeShader` (`d3d11_device.cpp`).
pub fn create_compute_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    create_shader_common(&CS_VTBL, device, bytecode, length, class_linkage, out)
}
