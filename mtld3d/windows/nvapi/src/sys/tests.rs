use super::*;

#[test]
fn a_short_string_is_nul_terminated_and_leaves_the_rest() {
    let mut buf = [0xaa_u8; SHORT_STRING_MAX];
    short_string(&mut buf, "r57218_000");
    assert_eq!(&buf[..11], b"r57218_000\0");
    assert!(buf[11..].iter().all(|&b| b == 0xaa));
}

#[test]
fn the_build_branch_names_the_sdk() {
    assert_eq!(build_branch(), "r57218_000");
}

#[test]
fn unload_without_initialize_is_refused() {
    assert_eq!(unload(), API_NOT_INITIALIZED);
}

#[test]
fn query_knows_only_its_own_ids() {
    assert!(!query(0x0150_e828).is_null());
    assert!(query(0xe5ac_921f).is_null());
}
