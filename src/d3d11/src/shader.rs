//! D3D11 shader objects and managed shader state (`d3d11_shader.cpp`).

use core::ffi::c_void;
use std::sync::Arc;

use winemetal_sys::sm50::MtlShaderReflection;

use crate::com::{E_NOTIMPL, Hresult};

pub mod args;

/// Deduplicated shader representation owned by [`crate::pipeline::PipelineCache`] (`dxmt::Shader`).
pub struct ManagedShader {
    pub sha1: [u8; 20],
    pub reflection: MtlShaderReflection,
    pub handle: u64,
}

/// COM shader object (`TShaderBase<Interface>`).
pub struct Shader {
    pub managed: Arc<ManagedShader>,
}

impl Shader {
    /// Recovers `&Shader` from any `ID3D11*Shader*` interface pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }
}

/// `ID3D11Device::CreateVertexShader`.
pub const fn create_vertex_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, bytecode, bytecode_length, class_linkage, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreatePixelShader`.
pub const fn create_pixel_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, bytecode, bytecode_length, class_linkage, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateGeometryShader`.
pub const fn create_geometry_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, bytecode, bytecode_length, class_linkage, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateHullShader`.
pub const fn create_hull_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, bytecode, bytecode_length, class_linkage, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateDomainShader`.
pub const fn create_domain_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, bytecode, bytecode_length, class_linkage, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreateComputeShader`.
pub const fn create_compute_shader(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    class_linkage: *mut c_void,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, bytecode, bytecode_length, class_linkage, out);
    E_NOTIMPL
}
