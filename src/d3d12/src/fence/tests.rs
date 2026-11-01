use core::{ffi::c_void, ptr};

use crate::{
    abi::objects::D3D12_FENCE_FLAG_SHARED,
    com::{E_INVALIDARG, IID_ID3D12_FENCE1, S_OK},
    fence,
};

#[test]
fn set_event_on_completion_as_cpp() {
    let fake_event: *mut c_void = ptr::with_exposed_provenance_mut(0x1234_usize);

    // Case 1: completed_value (10) >= value (5) with non-null event -> sets event immediately on
    // the caller's thread, does not wait or register with EventListener.
    let mut set_called = Vec::new();
    let mut wait_called = Vec::new();
    let mut reg_called = Vec::new();
    let hr = fence::complete_or_wait(
        10,
        5,
        fake_event,
        |ev| set_called.push(ev),
        |val| wait_called.push(val),
        |ev, val| reg_called.push((ev, val)),
    );
    assert_eq!(hr, S_OK);
    assert_eq!(set_called, vec![fake_event]);
    assert_eq!(wait_called, [] as [u64; 0]);
    assert_eq!(reg_called, [] as [(*mut c_void, u64); 0]);

    // Case 2: completed_value (10) >= value (10) with null event -> no-op, returns S_OK.
    set_called.clear();
    let hr = fence::complete_or_wait(
        10,
        10,
        ptr::null_mut(),
        |ev| set_called.push(ev),
        |val| wait_called.push(val),
        |ev, val| reg_called.push((ev, val)),
    );
    assert_eq!(hr, S_OK);
    assert_eq!(set_called, [] as [*mut c_void; 0]);
    assert_eq!(wait_called, [] as [u64; 0]);
    assert_eq!(reg_called, [] as [(*mut c_void, u64); 0]);

    // Case 3: completed_value (10) < value (15) with null event -> blocks via wait(15).
    let hr = fence::complete_or_wait(
        10,
        15,
        ptr::null_mut(),
        |ev| set_called.push(ev),
        |val| wait_called.push(val),
        |ev, val| reg_called.push((ev, val)),
    );
    assert_eq!(hr, S_OK);
    assert_eq!(set_called, [] as [*mut c_void; 0]);
    assert_eq!(wait_called, vec![15]);
    assert_eq!(reg_called, [] as [(*mut c_void, u64); 0]);

    // Case 4: completed_value (10) < value (20) with non-null event -> registers with EventListener.
    wait_called.clear();
    let hr = fence::complete_or_wait(
        10,
        20,
        fake_event,
        |ev| set_called.push(ev),
        |val| wait_called.push(val),
        |ev, val| reg_called.push((ev, val)),
    );
    assert_eq!(hr, S_OK);
    assert_eq!(set_called, [] as [*mut c_void; 0]);
    assert_eq!(wait_called, [] as [u64; 0]);
    assert_eq!(reg_called, vec![(fake_event, 20)]);

    // Null output pointer in CreateFence returns E_INVALIDARG.
    // SAFETY: Testing null out-pointer validation.
    let hr_null = unsafe {
        fence::create_fence(
            ptr::null_mut(),
            0,
            D3D12_FENCE_FLAG_SHARED,
            ptr::from_ref(&IID_ID3D12_FENCE1).cast::<c_void>(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_null, E_INVALIDARG);
}
