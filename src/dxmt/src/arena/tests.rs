use core::alloc::Layout;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use super::{BlockPool, CommandList, Commands};
use crate::encoder::Encoder;

/// Counts its drops.
struct Dropped(Arc<AtomicUsize>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A capture aligned past a block's own 16 bytes.
#[repr(align(64))]
struct Aligned(u32);

fn count(counter: &AtomicUsize) -> usize {
    counter.load(Ordering::SeqCst)
}

/// A chunk's command is dropped once, when the chunk is retired.
///
/// After it ran (the C++ destroys its lambdas at `reset`), or unrun.
#[test]
fn arena_drops_each_command_once() {
    let (drops, calls) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    // 64-byte blocks: the commands span several, and the 200-byte one needs a block of its own.
    let pool = Arc::new(BlockPool::new(64));
    let mut commands = Commands::new(Arc::clone(&pool));
    let mut encoder = Encoder {};
    for _ in 0..5 {
        let (dropped, calls) = (Dropped(Arc::clone(&drops)), Arc::clone(&calls));
        commands.emit(move |_| {
            let _ = &dropped;
            calls.fetch_add(1, Ordering::SeqCst);
        });
    }
    let (big, big_calls) = (
        (Dropped(Arc::clone(&drops)), [7u8; 200]),
        Arc::clone(&calls),
    );
    commands.emit(move |_| {
        let (_, bytes) = &big;
        assert_eq!(bytes[199], 7);
        big_calls.fetch_add(1, Ordering::SeqCst);
    });
    commands.run(&mut encoder);
    assert_eq!(count(&calls), 6);
    assert_eq!(
        count(&drops),
        0,
        "a command lives until its chunk is retired"
    );
    commands.reset();
    assert_eq!(count(&drops), 6);
    for _ in 0..2 {
        let dropped = Dropped(Arc::clone(&drops));
        commands.emit(move |_| {
            let _ = &dropped;
        });
    }
    commands.reset();
    assert_eq!(count(&drops), 8, "an unrun command is dropped by the reset");
    let dropped = Dropped(Arc::clone(&drops));
    commands.emit(move |_| {
        let _ = &dropped;
    });
    drop(commands);
    assert_eq!(count(&drops), 9, "and by the chunk's own drop");
    assert_eq!(count(&calls), 6);
}

/// Commands run in the order they were recorded.
///
/// Across blocks, with raw allocations between them and a capture aligned past the block's.
#[test]
fn arena_runs_in_order() {
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut commands = Commands::new(Arc::new(BlockPool::new(128)));
    for i in 0..20u32 {
        let order = Arc::clone(&order);
        if i % 3 == 0 {
            let aligned = Aligned(i);
            commands.emit(move |_| {
                assert_eq!((&raw const aligned).addr() % 64, 0);
                order.lock().unwrap().push(aligned.0);
            });
        } else {
            commands.emit(move |_| order.lock().unwrap().push(i));
        }
        let raw = commands.allocate(Layout::from_size_align(24, 8).unwrap());
        assert_eq!(raw.addr().get() % 8, 0);
    }
    commands.run(&mut Encoder {});
    assert_eq!(*order.lock().unwrap(), (0..20).collect::<Vec<_>>());
}

/// A deferred list replays its commands each time it runs and drops them once, with the list.
#[test]
fn list_replays_twice() {
    let (order, drops) = (
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(AtomicUsize::new(0)),
    );
    let mut list = CommandList::new(64);
    for i in 0..3u32 {
        let (order, dropped) = (Arc::clone(&order), Dropped(Arc::clone(&drops)));
        list.emit_fn(move |_| {
            let _ = &dropped;
            order.lock().unwrap().push(i);
        });
    }
    let mut encoder = Encoder {};
    list.run(&mut encoder);
    list.run(&mut encoder);
    assert_eq!(*order.lock().unwrap(), [0, 1, 2, 0, 1, 2]);
    assert_eq!(count(&drops), 0);
    drop(list);
    assert_eq!(count(&drops), 3);
}
