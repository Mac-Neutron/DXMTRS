//! D3D11 event, occlusion, timestamp and dummy queries (`d3d11_query.cpp`).

use core::ffi::c_void;

use crate::{
    abi::records::{D3d11QueryDesc, D3d11QueryDesc1},
    com::{E_NOTIMPL, Hresult},
};

/// D3D11 query / predicate object (`MTLD3DQueryBase`).
#[derive(Default)]
pub struct Query {
    _private: (),
}

impl Query {
    /// Recovers `&mut Query` from an `ID3D11Asynchronous*` / `ID3D11Query*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static mut Self> {
        let _ = iface;
        None
    }

    /// Begins a query bracket (`ID3D11DeviceContext::Begin`).
    pub const fn begin(&mut self, ctx: *mut c_void) {
        let _ = ctx;
    }

    /// Ends or issues a query (`ID3D11DeviceContext::End`).
    pub const fn end(&mut self, ctx: *mut c_void) {
        let _ = ctx;
    }

    /// Reads query results (`ID3D11DeviceContext::GetData`).
    pub const fn data(
        &mut self,
        ctx: *mut c_void,
        data: *mut c_void,
        data_size: u32,
        flags: u32,
    ) -> Hresult {
        let _ = (ctx, data, data_size, flags);
        E_NOTIMPL
    }
}

/// `ID3D11Device::CreateQuery`.
pub const fn create_query(
    device: *mut c_void,
    desc: *const D3d11QueryDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device3::CreateQuery1`.
pub const fn create_query1(
    device: *mut c_void,
    desc: *const D3d11QueryDesc1,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}

/// `ID3D11Device::CreatePredicate`.
pub const fn create_predicate(
    device: *mut c_void,
    desc: *const D3d11QueryDesc,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (device, desc, out);
    E_NOTIMPL
}
