//! D3D9's DXSO shader bytecode: the parser, its IR, the linkage data and the specialization.
//!
//! Moved from `mtld3d-core` (zlib, README.md's list) so the AIR translator, which runs on the
//! VM host where no mtld3d crate is linked, and D3D9's MSL emitters read one parser. Pure data
//! and pure functions: no logging (a warning is returned for the caller to log), no MSL.

pub mod ir;
pub mod link;
pub mod opcode;
pub mod parser;
pub mod spec;

pub use ir::{
    CmpFunc, DeclUsage, Declaration, DxsoError, DxsoProgram, DxsoWarning, Instruction, RegKind,
    ShaderType, TextureType,
};
pub use link::{LinkInputs, MAX_LINKED_INPUTS, Semantic, SemanticSet, decl_passthrough_code};
pub use parser::{operand_token_count, parse};
