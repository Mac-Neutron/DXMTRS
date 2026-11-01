//! `MTLD3D10VertexShader`, `MTLD3D10PixelShader` and `MTLD3D10GeometryShader` (`d3d10_shader.hpp`).

use super::{
    stub_add_ref, stub_get_device, stub_get_private_data, stub_qi, stub_release,
    stub_set_private_data, stub_set_private_data_interface,
};
use crate::abi::vtbl::{ID3D10GeometryShaderVtbl, ID3D10PixelShaderVtbl, ID3D10VertexShaderVtbl};

/// `ID3D10VertexShader` vtable for embedded `MTLD3D10VertexShader` wrappers.
pub static VERTEX_SHADER_VTBL: ID3D10VertexShaderVtbl = ID3D10VertexShaderVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
};

/// `ID3D10PixelShader` vtable for embedded `MTLD3D10PixelShader` wrappers.
pub static PIXEL_SHADER_VTBL: ID3D10PixelShaderVtbl = ID3D10PixelShaderVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
};

/// `ID3D10GeometryShader` vtable for embedded `MTLD3D10GeometryShader` wrappers.
pub static GEOMETRY_SHADER_VTBL: ID3D10GeometryShaderVtbl = ID3D10GeometryShaderVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
};
