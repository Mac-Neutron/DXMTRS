//! D3D9 lowering: a SM1-3 program to AIR.
//!
//! Not lowered yet: [`convert`] refuses every program.

use core::ffi::CStr;

use airconv_ir::dxso::spec::StageSpec;

use crate::{abi::ArgChain, api::Error, llvm::Module, shader::Dxso};

/// Lowers `shader`, specialized by `spec`, as the function `name` into `m`.
///
/// # Errors
///
/// `dxso: not lowered`, until the lowering lands.
pub fn convert(
    _shader: &Dxso,
    _spec: &StageSpec,
    _args: &ArgChain<'_>,
    _name: &CStr,
    _m: &Module<'_>,
) -> Result<(), Error> {
    Err(Error::new(b"dxso: not lowered"))
}
