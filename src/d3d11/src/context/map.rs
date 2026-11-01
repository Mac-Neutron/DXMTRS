//! Context resource mapping (`Map`, `Unmap`).

use core::ffi::c_void;

use super::{Context, Mode};
use crate::com::{E_NOTIMPL, Hresult};

impl<M: Mode> Context<M> {
    /// `ID3D11DeviceContext::Map`.
    pub const fn map(
        &mut self,
        resource: *mut c_void,
        subresource: u32,
        map_type: u32,
        map_flags: u32,
        mapped_resource: *mut c_void,
    ) -> Hresult {
        let _ = (resource, subresource, map_type, map_flags, mapped_resource);
        E_NOTIMPL
    }

    /// `ID3D11DeviceContext::Unmap`.
    pub const fn unmap(&mut self, resource: *mut c_void, subresource: u32) {
        let _ = (resource, subresource);
    }
}
