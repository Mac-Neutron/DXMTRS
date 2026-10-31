use super::*;

/// A ring of blocks of `block` bytes whose blocks are numbers, counted from 1 as they are made.
fn ring(block: u64, lifetime: u64) -> Blocks<u32> {
    Blocks::new(RingConfig { block, lifetime })
}

/// The block and offset of `size` bytes aligned to 16; a new block takes the next number.
fn alloc(r: &mut Blocks<u32>, made: &mut u32, seq: u64, coherent: u64, size: u64) -> (u32, u64) {
    let (block, offset) = r.allocate(seq, coherent, size, 16, |_| {
        *made += 1;
        *made
    });
    (*block, offset)
}

#[test]
fn ring_reuses_after_coherent() {
    let (mut r, mut made) = (ring(64, 300), 0);
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 40), (1, 0));
    // 40 rounds up to 48: 40 more bytes don't fit, and block 1 is in use.
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 40), (2, 0));
    // Block 1's sequence must be below the coherent one, not equal to it.
    assert_eq!(alloc(&mut r, &mut made, 2, 1, 40), (3, 0));
    assert_eq!(alloc(&mut r, &mut made, 3, 2, 40), (1, 0));
    assert_eq!(made, 3);
    // Block 1 is now the latest: the next allocation bumps in it, aligned.
    assert_eq!(alloc(&mut r, &mut made, 3, 2, 8), (1, 48));
    assert_eq!(r.fifo.back().map(|b| (b.used, b.last_seq)), Some((56, 3)));
    assert_eq!(r.fifo.len(), 3);
}

#[test]
fn ring_grows_a_block_when_full() {
    let (mut r, mut made) = (ring(64, 300), 0);
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 64), (1, 0));
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 1), (2, 0));
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 15), (2, 16));
    // To the block's last byte.
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 32), (2, 32));
    // Larger than a block: a block of its own size, with nothing after it.
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 200), (3, 0));
    assert_eq!(r.fifo.back().map(|b| b.size), Some(200));
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 16), (4, 0));
    // A block of its own size is never reused: dropped once its sequence finished.
    assert_eq!(alloc(&mut r, &mut made, 2, 0, 64), (5, 0));
    assert_eq!(r.fifo.len(), 5);
    // A finished one at the front is dropped, not reused, on the way to a new block.
    let (mut r, mut made) = (ring(64, 300), 0);
    assert_eq!(alloc(&mut r, &mut made, 1, 0, 200), (1, 0));
    assert_eq!(alloc(&mut r, &mut made, 3, 2, 16), (2, 0));
    assert_eq!(r.fifo.len(), 1);
}

#[test]
fn ring_frees_idle_and_large_blocks() {
    let (mut r, mut made) = (ring(64, 2), 0);
    alloc(&mut r, &mut made, 1, 0, 200);
    alloc(&mut r, &mut made, 1, 0, 64);
    alloc(&mut r, &mut made, 2, 0, 64);
    // Sequence 1 isn't finished: nothing goes.
    r.free(0);
    assert_eq!(r.fifo.len(), 3);
    // The large block goes as soon as it is; block 2 waits its lifetime out, a free at a time.
    r.free(1);
    assert_eq!(r.fifo.iter().map(|b| b.buffer).collect::<Vec<_>>(), [2, 3]);
    assert_eq!(r.fifo.front().map(|b| b.idle_frees), Some(1));
    for idle in 2..=3 {
        r.free(1);
        assert_eq!(r.fifo.front().map(|b| b.idle_frees), Some(idle));
    }
    r.free(1);
    assert_eq!(r.fifo.iter().map(|b| b.buffer).collect::<Vec<_>>(), [3]);
    // More than `lifetime` sequences after its last use, a block goes at once.
    r.free(5);
    assert!(r.fifo.is_empty());
    // An unsigned -1 frees everything, however young.
    let (mut r, mut made) = (ring(64, u64::MAX), 0);
    alloc(&mut r, &mut made, 1, 0, 1);
    r.free(u64::MAX);
    assert!(r.fifo.is_empty());
}
