//! The link: the shim's archive, the two passes and LLVM's libraries, through the shim's probe.

use crate::llvm::sys;

#[test]
fn shim_probe_links_llvm() {
    // SAFETY: the probe takes nothing and builds its own context and module.
    assert_eq!(unsafe { sys::airconv_shim_probe() }, 1);
}
