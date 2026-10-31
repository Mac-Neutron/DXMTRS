//! The shader a handle owns: what `SM50Initialize` read, for the compiles and getters that follow.
//!
//! One handle is compiled on several threads at once (D3D11's workers compile two variants of one
//! shader), with no lock, so `Shader` is `Sync`: a compile only reads it.

use std::ffi::CString;

use airconv_ir::initialize::DxbcShader;

use crate::abi::{DxilEntryElement, DxilEntryInfo, MtlSm50ShaderArgument};

/// An initialized shader. The DXIL variant comes with its Initialize.
pub enum Shader {
    /// A DXBC shader.
    Dxbc(Dxbc),
}

/// An initialized DXBC shader, with the tables and the entry its getters hand out.
pub struct Dxbc {
    pub ir: DxbcShader,
    /// The constant-buffer table and the argument table, as `SM50GetArgumentsInfo` copies them.
    pub constant_buffers: Vec<MtlSm50ShaderArgument>,
    pub arguments: Vec<MtlSm50ShaderArgument>,
    pub entry: EntryView,
}

/// `DXILGetEntry`'s arrays, which point into the view's own names and semantic indices.
///
/// The names and indices live on the heap and are never written after the view is made, so the
/// elements' pointers stay valid as long as the view, wherever it moves.
pub struct EntryView {
    pub stage: u32,
    pub inputs: Vec<DxilEntryElement>,
    pub outputs: Vec<DxilEntryElement>,
    /// What the elements' `name` pointers point into.
    pub names: Vec<CString>,
    /// What the elements' `semantic_indices` pointers point into, one per element.
    pub semantic_indices: Box<[u32]>,
}

// SAFETY: the view's raw pointers point into its own heap data, which nothing writes after the
// view is made; threads that share a `&EntryView` only copy the pointer values out, and whoever
// reads through them takes the handle's contract (alive until `SM50Destroy`).
unsafe impl Sync for EntryView {}

const _: () = {
    const fn shared_between_compiles<T: Sync>() {}
    shared_between_compiles::<Shader>();
};

impl Shader {
    /// The constant-buffer table and the argument table `SM50GetArgumentsInfo` copies out.
    #[must_use]
    pub fn arguments(&self) -> (&[MtlSm50ShaderArgument], &[MtlSm50ShaderArgument]) {
        let Self::Dxbc(dxbc) = self;
        (&dxbc.constant_buffers, &dxbc.arguments)
    }
}

impl EntryView {
    /// The entry as `DXILGetEntry` answers it.
    ///
    /// An empty array is a null pointer, as the C++'s empty vectors answered.
    #[must_use]
    pub fn info(&self) -> DxilEntryInfo {
        DxilEntryInfo {
            stage: self.stage,
            num_inputs: airconv_ir::binding::count_u32(self.inputs.len()),
            num_outputs: airconv_ir::binding::count_u32(self.outputs.len()),
            inputs: first(&self.inputs),
            outputs: first(&self.outputs),
            ..DxilEntryInfo::EMPTY
        }
    }
}

/// A pointer to the first element, null for none.
const fn first<T>(list: &[T]) -> *const T {
    if list.is_empty() {
        core::ptr::null()
    } else {
        list.as_ptr()
    }
}
