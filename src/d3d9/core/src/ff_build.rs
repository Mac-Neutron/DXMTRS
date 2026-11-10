//! Fixed-function stages built as DXSO tokens, for the AIR translator.
//!
//! Under `shader.backend=air` a fixed-function stage is not emitted as MSL text: its key
//! (`FfVsKey`, `FfPsKey`) is built into an SM3 program (`vs_3_0`, `ps_3_0`) that computes what
//! `dxso::emit_vs_ff`/`emit_ps_ff` compute, serialized with `airconv_ir::dxso_write`, and parsed
//! and lowered by `airconv_ir::dxso` like an application's shader. The MSL emitters stay the
//! oracle: each builder reads the constant rows, declares the inputs and writes the outputs the
//! MSL does for the same key, and the parity corpus renders the two against each other.
//!
//! Reads no `c#` row can name go through registers no valid D3D9 shader names, which the
//! lowering gives fixed-function meanings (`airconv_ir::dxso::lower`): `c2032` to `c2043` are the
//! `VsDraw` rows, `c2044` is `PosFixup` row 0 (a program reading it clamps an RHW depth),
//! `dcl_position o31` is the world-space basis the user clip planes are dotted with, and a pixel
//! source of token type 11 is a `bump_env` row. Literals are `def`s above every row the key's
//! constant block holds (`c256` and up in a vertex program, `c16` and up in a pixel one), so no
//! `def` shadows a row the stage reads. These programs fail
//! `DxsoProgram::violates_constant_register_limits` by design, and never go through the
//! application shader checks.
//!
//! The builders are deterministic: the same key builds the same tokens, so a cached AIR record
//! can be checked against a fresh build of its key. They are outside the emitter fingerprint
//! (`core/build.rs` hashes `src/dxso/`), and outside the translator key.

use std::sync::Arc;

pub mod asm;

/// One fixed-function stage built as DXSO, with what its translation needs beside the tokens.
#[derive(Debug)]
pub struct FfProgram {
    /// The `vs_3_0` or `ps_3_0` token stream, in `dxso_write`'s canonical form.
    pub tokens: Arc<[u32]>,
    /// The vertex inputs the program declares, bit `i` for `vi` (bits 0 to 15), for its `VsSpec`.
    ///
    /// Zero for a pixel program.
    pub provided_input_mask: u16,
    /// The warnings `dxso::emit_vs_ff`/`emit_ps_ff` log for the same key, once each.
    ///
    /// The caller logs each under `mtld3d::dxso`.
    pub warnings: Vec<&'static str>,
}
