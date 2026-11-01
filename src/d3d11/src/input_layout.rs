//! D3D11 input layouts and stream-output layouts (`d3d11_input_layout.cpp`).

use core::ffi::c_void;

use crate::{
    abi::records::{D3d11InputElementDesc, D3d11SoDeclarationEntry},
    com::{E_NOTIMPL, Hresult},
};

/// Single translated input-layout element (`MTL_SHADER_INPUT_LAYOUT_ELEMENT_DESC`).
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct InputLayoutElement {
    pub index: u32,
    pub slot: u32,
    pub offset: u32,
    pub format: u32,
    pub step_function_and_rate: u32,
}

/// Single stream-output element (`MTL_SHADER_STREAM_OUTPUT_ELEMENT_DESC`).
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct StreamOutputElement {
    pub register_idx: u32,
    pub component: u32,
    pub output_slot: u32,
    pub offset: u32,
}

/// Input layout object (`MTLD3D11InputLayout` / `dxmt::InputLayout`).
pub struct InputLayout {
    elements: Vec<InputLayoutElement>,
    slot_mask: u32,
    sha1: [u8; 20],
}

impl InputLayout {
    /// Recovers `&InputLayout` from an `ID3D11InputLayout*` pointer.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }

    /// Returns the translated input-layout elements (`input_layout_element`).
    #[must_use]
    pub fn elements(&self) -> &[InputLayoutElement] {
        &self.elements
    }

    /// Returns the active vertex-buffer slot bitmask (`input_slot_mask`).
    #[must_use]
    pub const fn slot_mask(&self) -> u32 {
        self.slot_mask
    }

    /// Returns the 20-byte SHA-1 digest of the layout (`sha1`).
    #[must_use]
    pub const fn sha1(&self) -> &[u8; 20] {
        &self.sha1
    }
}

/// Recovers `&InputLayout` from an `ID3D11InputLayout*` interface pointer.
pub struct StreamOutputLayout {
    pub elements: Vec<StreamOutputElement>,
    pub strides: [u32; 4],
    pub rasterized_stream: u32,
    pub sha1: [u8; 20],
}

impl StreamOutputLayout {
    /// Returns `true` when this layout declares at least one vertex attribute.
    #[must_use]
    pub const fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        let _ = iface;
        None
    }
}

/// `ID3D11Device::CreateInputLayout`.
pub const fn create_input_layout(
    device: *mut c_void,
    elements: *const D3d11InputElementDesc,
    num_elements: u32,
    bytecode: *const c_void,
    bytecode_length: usize,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (
        device,
        elements,
        num_elements,
        bytecode,
        bytecode_length,
        out,
    );
    E_NOTIMPL
}

/// `ID3D11Device::CreateGeometryShaderWithStreamOutput`.
pub const fn create_gs_with_so(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    entries: (*const D3d11SoDeclarationEntry, u32),
    strides: (*const u32, u32),
    rasterized_stream: u32,
    out: *mut *mut c_void,
) -> Hresult {
    let _ = (
        device,
        bytecode,
        bytecode_length,
        entries,
        strides,
        rasterized_stream,
        out,
    );
    E_NOTIMPL
}
