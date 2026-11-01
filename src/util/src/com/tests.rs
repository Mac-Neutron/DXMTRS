use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use super::{ComObject, ComObjectWithInitialRef, Guid, qi_log};

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

/// A test object whose `Drop` re-enters `ComObject` by taking and releasing a private reference.
struct ReentrantDrop {
    com: ComObject,
    drops: Arc<AtomicU32>,
    nested_free: Arc<AtomicU32>,
}

impl Drop for ReentrantDrop {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        self.com.add_ref_private();
        if self.com.release_private() {
            self.nested_free.fetch_add(1, Ordering::SeqCst);
        }
    }
}

#[test]
fn counts_as_cpp() {
    let obj = ComObject::new();
    assert_eq!((obj.public_count(), obj.private_count()), (0, 0));

    // First public AddRef sets both public and private counts to 1.
    assert_eq!(obj.add_ref(), 1);
    assert_eq!((obj.public_count(), obj.private_count()), (1, 1));

    // Second public AddRef increments public to 2 while private stays at 1.
    assert_eq!(obj.add_ref(), 2);
    assert_eq!((obj.public_count(), obj.private_count()), (2, 1));

    // AddRefPrivate increments private to 2.
    obj.add_ref_private();
    assert_eq!((obj.public_count(), obj.private_count()), (2, 2));

    // Two public Releases bring public to 0 and private to 1 without freeing.
    assert_eq!(obj.release_and_free(), (1, false));
    assert_eq!((obj.public_count(), obj.private_count()), (1, 2));
    assert_eq!(obj.release_and_free(), (0, false));
    assert_eq!((obj.public_count(), obj.private_count()), (0, 1));

    // ReleasePrivate brings private to 0 and signals free once.
    assert!(obj.release_private());
    assert_eq!((obj.public_count(), obj.private_count()), (0, 0));

    // ComObjectWithInitialRef starts at (1, 1) and frees on the first public Release.
    let init = ComObjectWithInitialRef::new();
    assert_eq!((init.public_count(), init.private_count()), (1, 1));
    assert_eq!(init.release_and_free(), (0, true));
    assert_eq!((init.public_count(), init.private_count()), (0, 0));

    // A destructor that takes and releases a private ref frees once (0x80000000 guard).
    let drops = Arc::new(AtomicU32::new(0));
    let nested_free = Arc::new(AtomicU32::new(0));
    let raw = Box::into_raw(Box::new(ReentrantDrop {
        com: ComObject::new(),
        drops: Arc::clone(&drops),
        nested_free: Arc::clone(&nested_free),
    }));
    // SAFETY: `raw` is a live allocation from `Box::into_raw`.
    assert_eq!(unsafe { (*raw).com.add_ref() }, 1);
    // SAFETY: `raw` is still live; this releases the sole public reference.
    let (rem, free) = unsafe { (*raw).com.release_and_free() };
    assert_eq!((rem, free), (0, true));
    // SAFETY: `free` was true once, so we own `raw` and drop it here.
    drop(unsafe { Box::from_raw(raw) });
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(nested_free.load(Ordering::SeqCst), 0);
}

#[test]
fn private_hold_outlives_public() {
    let drops = Arc::new(AtomicU32::new(0));
    let nested_free = Arc::new(AtomicU32::new(0));
    let raw = Box::into_raw(Box::new(ReentrantDrop {
        com: ComObject::new(),
        drops: Arc::clone(&drops),
        nested_free: Arc::clone(&nested_free),
    }));
    // Internal hold taken before any public ref.
    // SAFETY: `raw` is a live allocation from `Box::into_raw`.
    unsafe { (*raw).com.add_ref_private() };
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.add_ref() }, 1);
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.private_count() }, 2);

    // Public 1 -> 0 drops the public hold's private ref, leaving the internal hold alive.
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.release() }, 0);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    // SAFETY: `raw` is still kept alive by the private hold.
    assert_eq!(unsafe { (*raw).com.private_count() }, 1);

    // Re-acquiring a public ref from 0 -> 1 adds a private ref again, and 1 -> 0 drops it.
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.add_ref() }, 1);
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.private_count() }, 2);
    // SAFETY: `raw` is live.
    assert_eq!(unsafe { (*raw).com.release_and_free() }, (0, false));

    // Releasing the private hold frees the object once.
    // SAFETY: `raw` is live.
    let free = unsafe { (*raw).com.release_private() };
    assert!(free);
    // SAFETY: the last private reference was just released.
    drop(unsafe { Box::from_raw(raw) });
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(nested_free.load(Ordering::SeqCst), 0);
}

#[test]
fn qi_log_fires_once_per_pair() {
    const OBJ: Guid = Guid::new(0x7000_0001, 0, 0, [0; 8]);
    const IID_A: Guid = Guid::new(0x7000_0002, 0, 0, [0; 8]);
    const IID_B: Guid = Guid::new(0x7000_0003, 0, 0, [0; 8]);
    assert!(qi_log(&OBJ, &IID_A));
    assert!(!qi_log(&OBJ, &IID_A));
    assert!(qi_log(&OBJ, &IID_B));
}
