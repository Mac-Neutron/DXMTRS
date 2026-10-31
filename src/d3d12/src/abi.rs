//! D3D12's structures and constants as the API defines them, asserted against the C headers.

use dxmt_util::com::Hresult;

pub mod barrier;
pub mod feature;
pub mod pipeline;
pub mod resource;
pub mod root_signature;
pub mod sampler;

pub const E_OUTOFMEMORY: Hresult = 0x8007_000e_u32.cast_signed();
pub const E_INVALIDARG: Hresult = 0x8007_0057_u32.cast_signed();
