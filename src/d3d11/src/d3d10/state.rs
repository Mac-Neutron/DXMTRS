//! D3D10 state-object vtables (`d3d10_state_object.cpp`).

use super::{
    stub_add_ref, stub_get_desc, stub_get_device, stub_get_private_data, stub_qi, stub_release,
    stub_set_private_data, stub_set_private_data_interface,
};
use crate::abi::vtbl::{
    ID3D10BlendState1Vtbl, ID3D10DepthStencilStateVtbl, ID3D10RasterizerStateVtbl,
    ID3D10SamplerStateVtbl,
};

/// `ID3D10BlendState1` vtable for embedded `MTLD3D10BlendState` wrappers.
pub static BLEND_STATE_VTBL: ID3D10BlendState1Vtbl = ID3D10BlendState1Vtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_desc: stub_get_desc,
    get_desc1: stub_get_desc,
};

/// `ID3D10RasterizerState` vtable for embedded `MTLD3D10RasterizerState` wrappers.
pub static RASTERIZER_STATE_VTBL: ID3D10RasterizerStateVtbl = ID3D10RasterizerStateVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_desc: stub_get_desc,
};

/// `ID3D10SamplerState` vtable for embedded `MTLD3D10SamplerState` wrappers.
pub static SAMPLER_STATE_VTBL: ID3D10SamplerStateVtbl = ID3D10SamplerStateVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_desc: stub_get_desc,
};

/// `ID3D10DepthStencilState` vtable for embedded `MTLD3D10DepthStencilState` wrappers.
pub static DEPTH_STENCIL_STATE_VTBL: ID3D10DepthStencilStateVtbl = ID3D10DepthStencilStateVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
    get_desc: stub_get_desc,
};
