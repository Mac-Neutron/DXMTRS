crate::dxmt_dll!(log "util.log", allocator, type_info);

#[test]
fn dxmt_dll_declares_the_log() {
    // A test build's log opens no file: the line goes to stderr only.
    LOG.warn("dxmt_dll! test line");
    crate::warn!(LOG, "{} test line", "dxmt_dll!");
}
