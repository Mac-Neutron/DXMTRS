//! Semantic linkage re-exports from `airconv_ir::dxso::link`.

pub use airconv_ir::dxso::link::{
    LinkInputs, MAX_LINKED_INPUTS, SemanticSet, decl_passthrough_code,
};

#[cfg(test)]
mod tests;
