//! Resource description validation and upgrade helpers (`d3d11_resource_helper.cpp`).

use crate::{
    abi::records::D3d11BufferDesc,
    com::{E_NOTIMPL, Hresult},
};

/// Validates a [`D3d11BufferDesc`] (`ValidateBufferProperties`).
#[must_use]
pub const fn validate_buffer_desc(desc: *const D3d11BufferDesc) -> Hresult {
    let _ = desc;
    E_NOTIMPL
}
