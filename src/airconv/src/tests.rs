//! The link: the shim's archive, the two passes and LLVM's libraries, through the shim's probe.

unsafe extern "C" {
    fn airconv_shim_probe() -> i32;
}

#[test]
fn shim_probe_links_llvm() {
    // SAFETY: the probe takes nothing and builds its own context and module.
    assert_eq!(unsafe { airconv_shim_probe() }, 1);
}
