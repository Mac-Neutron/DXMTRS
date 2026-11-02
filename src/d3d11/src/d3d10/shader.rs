//! `MTLD3D10VertexShader`, `MTLD3D10PixelShader` and `MTLD3D10GeometryShader` (`d3d10_shader.hpp`).

use super::util::{
    child_add_ref, child_get_device, child_get_private_data, child_query_interface, child_release,
    child_set_private_data, child_set_private_data_interface,
};
use crate::abi::vtbl::{ID3D10GeometryShaderVtbl, ID3D10PixelShaderVtbl, ID3D10VertexShaderVtbl};

/// `ID3D10VertexShader` vtable for embedded `MTLD3D10VertexShader` wrappers.
pub static VERTEX_SHADER_VTBL: ID3D10VertexShaderVtbl = ID3D10VertexShaderVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
};

/// `ID3D10PixelShader` vtable for embedded `MTLD3D10PixelShader` wrappers.
pub static PIXEL_SHADER_VTBL: ID3D10PixelShaderVtbl = ID3D10PixelShaderVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
};

/// `ID3D10GeometryShader` vtable for embedded `MTLD3D10GeometryShader` wrappers.
pub static GEOMETRY_SHADER_VTBL: ID3D10GeometryShaderVtbl = ID3D10GeometryShaderVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
};
