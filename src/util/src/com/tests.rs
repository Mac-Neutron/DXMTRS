use super::Guid;

/// `IMTLDXGIAdapter`'s IID.
const IMTL_DXGI_ADAPTER: Guid = Guid::new(
    0xacdf_3ef1,
    0xb33a,
    0x4cb6,
    [0x97, 0xbd, 0x1c, 0x19, 0x74, 0x82, 0x7e, 0x6d],
);

#[test]
fn guid_display_is_com_guids_form() {
    assert_eq!(
        IMTL_DXGI_ADAPTER.to_string(),
        "acdf3ef1-b33a-4cb6-97bd-1c1974827e6d"
    );
    // Zero padding, and lowercase.
    assert_eq!(
        Guid::new(1, 2, 3, [4, 5, 0xab, 6, 7, 8, 9, 0xcd]).to_string(),
        "00000001-0002-0003-0405-ab06070809cd"
    );
}

/// Bytes that start on an 8-byte boundary, so one byte in is misaligned for any wider `T`.
#[repr(align(8))]
struct Bytes([u8; 24]);

impl Bytes {
    /// A pointer one byte in, where no `T` of two bytes or more is aligned.
    fn odd<T>(&mut self) -> *mut T {
        let odd = self.0[1..].as_mut_ptr().cast::<T>();
        assert!(!odd.is_aligned(), "the test needs a misaligned pointer");
        odd
    }
}

#[test]
fn guid_read_takes_an_odd_address() {
    let mut bytes = Bytes([0; 24]);
    let odd = bytes.odd::<Guid>();
    // SAFETY: `odd` addresses the 16 bytes after the first of `bytes`' 24.
    unsafe { odd.write_unaligned(IMTL_DXGI_ADAPTER) };
    // SAFETY: `odd` addresses the 16 initialised bytes written above.
    let read = unsafe { Guid::read(odd) };
    assert!(read == Some(IMTL_DXGI_ADAPTER));
}

#[test]
fn guid_read_of_null_is_none() {
    // SAFETY: a null pointer is the case `read` answers with none.
    let read = unsafe { Guid::read(core::ptr::null()) };
    assert!(read.is_none());
}

#[test]
fn guids_compare_by_value() {
    let copy = IMTL_DXGI_ADAPTER;
    assert!(copy == IMTL_DXGI_ADAPTER);
    assert!(copy != Guid::new(0, 0, 0, [0; 8]));
}
