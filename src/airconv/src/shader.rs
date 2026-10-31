//! The shader a handle owns: what `SM50Initialize` read, for the compiles and getters that follow.
//!
//! One handle is compiled on several threads at once (D3D11's workers compile two variants of one
//! shader), with no lock, so `Shader` is `Sync`: a compile only reads it.

use crate::abi::MtlSm50ShaderArgument;

/// An initialized shader. Its DXBC and DXIL variants come with their Initialize.
pub struct Shader;

const _: () = {
    const fn shared_between_compiles<T: Sync>() {}
    shared_between_compiles::<Shader>();
};

impl Shader {
    /// The constant-buffer table and the argument table `SM50GetArgumentsInfo` copies out.
    #[must_use]
    pub const fn arguments(&self) -> (&[MtlSm50ShaderArgument], &[MtlSm50ShaderArgument]) {
        let Self = self;
        (&[], &[])
    }
}
