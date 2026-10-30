use super::*;

#[test]
fn a_luid_is_the_registry_id_byte_swapped() {
    assert_eq!(
        luid(0x0102_0304_0506_0708),
        0x0807_0605_0403_0201_u64.to_ne_bytes()
    );
}

#[test]
fn a_handle_is_its_registry_id() {
    assert_eq!(
        handle_id(id_handle(0x0102_0304_0506_0708)),
        0x0102_0304_0506_0708
    );
}

#[test]
fn a_stub_needs_its_arguments() {
    let mut out = 0_u32;
    let out = (&raw mut out).cast::<c_void>();
    let gpu = id_handle(1);
    assert_eq!(
        unsupported(core::ptr::null_mut(), out, NOT_SUPPORTED),
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsupported(gpu, core::ptr::null_mut(), NOT_SUPPORTED),
        INVALID_ARGUMENT
    );
    assert_eq!(unsupported(gpu, out, NO_IMPLEMENTATION), NO_IMPLEMENTATION);
    assert_eq!(
        get_all_clock_frequencies(core::ptr::null_mut(), out),
        NOT_SUPPORTED
    );
    assert_eq!(
        get_all_clock_frequencies(gpu, core::ptr::null_mut()),
        INVALID_ARGUMENT
    );
}
