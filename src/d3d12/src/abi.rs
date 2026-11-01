//! D3D12's structures and constants as the API defines them, asserted against the C headers.

pub mod barrier;
pub mod ext;
pub mod feature;
pub mod list_ops;
pub mod list_state;
pub mod objects;
pub mod pipeline;
pub mod queue;
pub mod resource;
pub mod root_signature;
pub mod sampler;
pub mod swapchain;
pub mod views;
pub mod vtbl;

pub use dxmt_util::com::{E_INVALIDARG, E_OUTOFMEMORY};
