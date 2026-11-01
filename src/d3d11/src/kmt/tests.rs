use core::mem::{align_of, offset_of, size_of};

use super::{
    SHARED_NAME_LEN, SharedName, SharedResourceData, SharedResourceDesc, make_unique_shared_name,
};

const _: () = {
    assert!(SHARED_NAME_LEN == 54);
    assert!(size_of::<SharedName>() == 54);
    assert!(align_of::<SharedName>() == 1);

    assert!(size_of::<SharedResourceDesc>() == 48);
    assert!(align_of::<SharedResourceDesc>() == 4);

    assert!(size_of::<SharedResourceData>() == 112);
    assert!(align_of::<SharedResourceData>() == 4);
    assert!(offset_of!(SharedResourceData, mach_port_name) == 0);
    assert!(offset_of!(SharedResourceData, dimension) == 56);
    assert!(offset_of!(SharedResourceData, desc) == 60);
    assert!(offset_of!(SharedResourceData, mutex_handle) == 108);
};

#[test]
fn shared_name_formats_53_ascii_chars_and_nul() {
    let name1 = make_unique_shared_name();
    let name2 = make_unique_shared_name();
    assert_eq!(name1[53], 0);
    assert_eq!(name2[53], 0);
    assert!(name1.starts_with(b"DXMT_shared_resource_"));
    assert!(name2.starts_with(b"DXMT_shared_resource_"));
    assert_ne!(name1, name2);
    for &b in &name1[21..53] {
        assert!(b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
    }
}
