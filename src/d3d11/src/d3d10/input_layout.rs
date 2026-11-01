//! `MTLD3D10InputLayout` (`ID3D10InputLayout` wrapper, `d3d10_input_layout.hpp`).

use super::{
    stub_add_ref, stub_get_device, stub_get_private_data, stub_qi, stub_release,
    stub_set_private_data, stub_set_private_data_interface,
};
use crate::abi::vtbl::ID3D10InputLayoutVtbl;

/// `ID3D10InputLayout` vtable for embedded `MTLD3D10InputLayout` wrappers.
pub static INPUT_LAYOUT_VTBL: ID3D10InputLayoutVtbl = ID3D10InputLayoutVtbl {
    query_interface: stub_qi,
    add_ref: stub_add_ref,
    release: stub_release,
    get_device: stub_get_device,
    get_private_data: stub_get_private_data,
    set_private_data: stub_set_private_data,
    set_private_data_interface: stub_set_private_data_interface,
};
