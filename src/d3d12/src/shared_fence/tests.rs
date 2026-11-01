use core::{ffi::c_void, ptr};
use std::collections::BTreeMap;

use crate::{
    com::{E_INVALIDARG, E_NOTIMPL, IID_ID3D12_FENCE},
    shared_fence::{self, SharedFenceTable},
};

/// Fake kernel handle table obeying `wmt-exports.sh`'s pinned `wmt_native_handle_count` semantics.
///
/// Returns `0` for null or closed handles, `1` on creation, `+1` on duplicate, `-1` on close.
struct FakeKernel {
    next_handle: usize,
    next_obj: usize,
    handle_to_obj: BTreeMap<usize, usize>,
    obj_refcount: BTreeMap<usize, u32>,
}

impl FakeKernel {
    fn new() -> Self {
        Self {
            next_handle: 0x100,
            next_obj: 1,
            handle_to_obj: BTreeMap::new(),
            obj_refcount: BTreeMap::new(),
        }
    }

    fn create_event(&mut self) -> usize {
        let h = self.next_handle;
        self.next_handle += 1;
        let obj = self.next_obj;
        self.next_obj += 1;
        self.handle_to_obj.insert(h, obj);
        self.obj_refcount.insert(obj, 1);
        h
    }

    fn duplicate_handle(&mut self, src: usize) -> usize {
        let obj = *self.handle_to_obj.get(&src).expect("valid source handle");
        let h = self.next_handle;
        self.next_handle += 1;
        self.handle_to_obj.insert(h, obj);
        *self.obj_refcount.get_mut(&obj).unwrap() += 1;
        h
    }

    fn close_handle(&mut self, h: usize) {
        if let Some(obj) = self.handle_to_obj.remove(&h)
            && let Some(rc) = self.obj_refcount.get_mut(&obj)
        {
            *rc -= 1;
            if *rc == 0 {
                self.obj_refcount.remove(&obj);
            }
        }
    }

    fn handle_count(&self, h: usize) -> u32 {
        if h == 0 {
            return 0;
        }
        let Some(&obj) = self.handle_to_obj.get(&h) else {
            return 0;
        };
        self.obj_refcount.get(&obj).copied().unwrap_or(0)
    }
}

#[test]
fn table_sweeps_closed_handles() {
    let mut kernel = FakeKernel::new();
    assert_eq!(kernel.handle_count(0), 0);
    assert_eq!(kernel.handle_count(0xdead), 0);

    let mut table = SharedFenceTable::new();
    let mut released: Vec<usize> = Vec::new();

    // Register two shared fences: each gets `ours` (count 1) and `client` duplicate (count 2).
    let ours_a = kernel.create_event();
    let client_a = kernel.duplicate_handle(ours_a);
    assert_eq!(kernel.handle_count(ours_a), 2);
    table.insert(client_a, ours_a, 0xA000, &mut |h, op| match op {
        shared_fence::TableOp::Count => kernel.handle_count(h),
        shared_fence::TableOp::Close => {
            kernel.close_handle(h);
            0
        }
        shared_fence::TableOp::ReleaseFence => {
            released.push(h);
            0
        }
    });

    let ours_b = kernel.create_event();
    let client_b = kernel.duplicate_handle(ours_b);
    assert_eq!(kernel.handle_count(ours_b), 2);
    table.insert(client_b, ours_b, 0xB000, &mut |h, op| match op {
        shared_fence::TableOp::Count => kernel.handle_count(h),
        shared_fence::TableOp::Close => {
            kernel.close_handle(h);
            0
        }
        shared_fence::TableOp::ReleaseFence => {
            released.push(h);
            0
        }
    });
    assert_eq!(released, [] as [usize; 0]);

    // Both handles resolve while client duplicates remain open.
    let found_a = table.get(client_a, &mut |h, op| match op {
        shared_fence::TableOp::Count => kernel.handle_count(h),
        shared_fence::TableOp::Close => {
            kernel.close_handle(h);
            0
        }
        shared_fence::TableOp::ReleaseFence => {
            released.push(h);
            0
        }
    });
    assert_eq!(found_a, Some(0xA000));

    // Client closes `client_a`: `ours_a` drops to handle count 1 and must be swept on the next op.
    kernel.close_handle(client_a);
    assert_eq!(kernel.handle_count(ours_a), 1);

    let found_b = table.get(client_b, &mut |h, op| match op {
        shared_fence::TableOp::Count => kernel.handle_count(h),
        shared_fence::TableOp::Close => {
            kernel.close_handle(h);
            0
        }
        shared_fence::TableOp::ReleaseFence => {
            released.push(h);
            0
        }
    });
    assert_eq!(found_b, Some(0xB000));
    assert_eq!(released, vec![0xA000]);
    assert_eq!(kernel.handle_count(ours_a), 0);

    // Closed `client_a` is no longer in the table.
    let found_a_after = table.get(client_a, &mut |h, op| match op {
        shared_fence::TableOp::Count => kernel.handle_count(h),
        shared_fence::TableOp::Close => {
            kernel.close_handle(h);
            0
        }
        shared_fence::TableOp::ReleaseFence => {
            released.push(h);
            0
        }
    });
    assert_eq!(found_a_after, None);

    // Entry-point validation: null handle returns E_INVALIDARG; OpenSharedHandleByName returns E_NOTIMPL.
    // SAFETY: Testing null-pointer validation on `CreateSharedHandle` and `OpenSharedHandleByName`.
    let hr_null = unsafe {
        shared_fence::create_shared_handle(
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_null, E_INVALIDARG);

    // SAFETY: Testing `OpenSharedHandleByName` returning `E_NOTIMPL`.
    let hr_by_name = unsafe {
        shared_fence::open_shared_handle_by_name(ptr::null_mut(), ptr::null(), 0, ptr::null_mut())
    };
    assert_eq!(hr_by_name, E_NOTIMPL);

    // SAFETY: Testing `OpenSharedHandle` with unknown handle returning `E_INVALIDARG`.
    let hr_open_unknown = unsafe {
        shared_fence::open_shared_handle(
            ptr::null_mut(),
            ptr::with_exposed_provenance_mut(0xdead_usize),
            ptr::from_ref(&IID_ID3D12_FENCE).cast::<c_void>(),
            ptr::null_mut(),
        )
    };
    assert_eq!(hr_open_unknown, E_INVALIDARG);
}
