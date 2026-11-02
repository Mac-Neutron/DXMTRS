//! `MTLD3D10InputLayout` (`ID3D10InputLayout` wrapper, `d3d10_input_layout.hpp`).

use super::util::{
    child_add_ref, child_get_device, child_get_private_data, child_query_interface, child_release,
    child_set_private_data, child_set_private_data_interface,
};
use crate::abi::vtbl::ID3D10InputLayoutVtbl;

/// `ID3D10InputLayout` vtable for embedded `MTLD3D10InputLayout` wrappers.
pub static INPUT_LAYOUT_VTBL: ID3D10InputLayoutVtbl = ID3D10InputLayoutVtbl {
    query_interface: child_query_interface,
    add_ref: child_add_ref,
    release: child_release,
    get_device: child_get_device,
    get_private_data: child_get_private_data,
    set_private_data: child_set_private_data,
    set_private_data_interface: child_set_private_data_interface,
};
