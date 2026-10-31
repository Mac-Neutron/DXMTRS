use core::cell::{Cell, RefCell};

use winemetal_sys::wire::{MemPtr, WMTBufferInfo};

use super::*;

thread_local! {
    /// The buffers the fake device made on this test's thread.
    static MADE: Cell<u64> = const { Cell::new(0) };
    /// The `(buffer, offset, value)` of each initial value written.
    static WRITES: RefCell<Vec<(u64, u64, u32)>> = const { RefCell::new(Vec::new()) };
}

/// Buffer handles the fake device makes, from here up (other fakes' handles are smaller).
const FIRST_BUFFER: u64 = 0xc0_0000;

#[unsafe(export_name = "MTLDevice_newBuffer")]
extern "C-unwind" fn fake_new_buffer(_device: u64, info: *mut WMTBufferInfo) -> u64 {
    let n = MADE.with(|m| {
        m.set(m.get() + 1);
        m.get()
    });
    // SAFETY: the allocation's description, writable for the call.
    unsafe { (*info).gpu_address = n << 32 };
    FIRST_BUFFER + n
}

#[unsafe(export_name = "MTLBuffer_updateContents")]
extern "C-unwind" fn fake_update_contents(buffer: u64, offset: u64, data: MemPtr, length: u64) {
    assert_eq!(length, 4);
    let src = core::ptr::with_exposed_provenance::<u32>(usize::try_from(data.ptr).unwrap());
    // SAFETY: the counter's initial value, 4 bytes for the call.
    let value = unsafe { src.read_unaligned() };
    WRITES.with(|w| w.borrow_mut().push((buffer, offset, value)));
}

fn writes() -> Vec<(u64, u64, u32)> {
    WRITES.with(RefCell::take)
}

#[test]
fn counter_pool_reuses() {
    // SAFETY: a fake handle; retain and release are the tests' fakes.
    let device = unsafe { Obj::retain(0xde) }.unwrap();
    let mut pool = CounterPool::new(&device);
    let counters: Vec<Counter> = (0..129).map(|_| Counter::new()).collect();

    // A counter's first use allocates it with 0; the set is the first buffer's.
    let first = pool.current(&counters[0]);
    assert_eq!(
        (first.buffer().buffer(), first.offset()),
        (FIRST_BUFFER + 1, 0)
    );
    assert_eq!(writes(), [(FIRST_BUFFER + 1, 0, 0)]);
    // Using it again keeps its allocation; updating it takes a new one with the value.
    assert!(Arc::ptr_eq(&first, &pool.current(&counters[0])));
    pool.update(&counters[0], 7);
    let second = pool.current(&counters[0]);
    assert_eq!(second.offset(), COUNTER_ALIGN);
    assert_eq!(writes(), [(FIRST_BUFFER + 1, 128, 7)]);
    drop((first, second));

    // Slot 0 went back with `first`: the set fills 0, then 2 to 63, and the 64th counter
    // makes a second set.
    for (i, counter) in counters.iter().enumerate().skip(1).take(63) {
        pool.update(counter, u32::try_from(i).unwrap());
    }
    let w = writes();
    assert_eq!(w.len(), 63);
    assert_eq!((w[0].1, w[1].1, w[62].1), (0, 256, 63 * 128));
    assert!(w.iter().all(|&(b, _, _)| b == FIRST_BUFFER + 1));
    pool.update(&counters[64], 64);
    assert_eq!(writes(), [(FIRST_BUFFER + 2, 0, 64)]);

    // A slot of the first set comes free, but allocation stays on the second set until it is
    // full, then goes round to the first set's free slot.
    pool.update(&counters[5], 500);
    let w = writes();
    assert_eq!((w[0].0, w[0].1), (FIRST_BUFFER + 2, 128));
    for counter in &counters[65..127] {
        pool.update(counter, 0);
    }
    assert!(writes().iter().all(|&(b, _, _)| b == FIRST_BUFFER + 2));
    pool.update(&counters[127], 127);
    // Counter 5's old slot is offset 5 * 128 of the first set.
    assert_eq!(writes(), [(FIRST_BUFFER + 1, 5 * 128, 127)]);
    // Every slot full: a third set.
    pool.update(&counters[128], 128);
    assert_eq!(writes(), [(FIRST_BUFFER + 3, 0, 128)]);
}
