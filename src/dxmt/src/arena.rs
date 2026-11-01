//! The chunk arena: the memory a command chunk's commands live in until the queue retires it.
//!
//! A command is its closure stored in place, behind a header that links it to the next and holds
//! two functions made for the closure's type, one that calls it and one that drops it: recording
//! a command takes arena space and nothing else (the C++'s `LambdaCommand` and its two virtual
//! functions, `dxmt_command_list.hpp`). A command runs once and is dropped when its chunk is
//! retired, after the GPU finished it, as the C++ destroys its lambdas in `reset`. The memory
//! comes in blocks, which the queue's chunks take from one [`BlockPool`] and give back when
//! retired, so blocks are held by the chunks in flight rather than by every chunk of the ring.

use core::{alloc::Layout, mem::MaybeUninit, ptr::NonNull};
use std::sync::{Arc, Mutex, PoisonError};

use crate::encoder::Encoder;

/// A block's unit: sixteen bytes aligned to sixteen.
///
/// So a block starts 16-byte aligned, the C++'s alignment for commands.
#[repr(C, align(16))]
struct Unit([u8; 16]);

/// One block of arena memory, owned as a `Box` owns its contents.
struct Block(NonNull<[MaybeUninit<Unit>]>);

// SAFETY: a `Block` is the only owner of its memory, as a `Box<[MaybeUninit<Unit>]>` would be.
unsafe impl Send for Block {}

impl Block {
    /// A block of at least `bytes` bytes, uninitialized.
    fn new(bytes: usize) -> Self {
        let units = Box::new_uninit_slice(bytes.div_ceil(size_of::<Unit>()));
        Self(NonNull::from(Box::leak(units)))
    }

    const fn len(&self) -> usize {
        self.0.len() * size_of::<Unit>()
    }

    const fn start(&self) -> *mut u8 {
        self.0.as_ptr().cast()
    }

    /// Where `layout` goes after the first `used` bytes, and the bytes used then.
    ///
    /// `None` when it does not fit.
    fn place(&self, used: usize, layout: Layout) -> Option<(NonNull<u8>, usize)> {
        let pad = self.start().addr().wrapping_add(used).wrapping_neg() & (layout.align() - 1);
        let offset = used.checked_add(pad)?;
        let end = offset.checked_add(layout.size())?;
        if end > self.len() {
            return None;
        }
        Some((NonNull::new(self.start().wrapping_add(offset))?, end))
    }
}

impl Drop for Block {
    fn drop(&mut self) {
        // SAFETY: the pointer is `new`'s `Box::leak`, given back here once.
        drop(unsafe { Box::from_raw(self.0.as_ptr()) });
    }
}

/// The blocks chunks take arena memory from, each of one size, kept for reuse once given back.
// ponytail: the pool keeps its high-water mark; free blocks by age (the C++ ring's lifetime) if a
// spike's memory matters.
pub struct BlockPool {
    size: usize,
    free: Mutex<Vec<Block>>,
}

impl BlockPool {
    /// A pool of blocks of `size` bytes (rounded up to 16).
    #[must_use]
    pub const fn new(size: usize) -> Self {
        Self {
            size: size.next_multiple_of(size_of::<Unit>()),
            free: Mutex::new(Vec::new()),
        }
    }

    fn take(&self) -> Block {
        let reused = self
            .free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        reused.unwrap_or_else(|| Block::new(self.size))
    }

    /// Keeps `block` for reuse when it is one of the pool's own size; frees a larger one.
    fn give(&self, block: Block) {
        if block.len() == self.size {
            self.free
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(block);
        }
    }
}

/// Bump allocation over blocks from a pool; a request larger than a block gets one of its own.
struct Arena {
    pool: Arc<BlockPool>,
    blocks: Vec<Block>,
    /// Bytes used in the last block.
    used: usize,
}

impl Arena {
    const fn new(pool: Arc<BlockPool>) -> Self {
        Self {
            pool,
            blocks: Vec::new(),
            used: 0,
        }
    }

    fn allocate(&mut self, layout: Layout) -> NonNull<u8> {
        loop {
            if let Some(block) = self.blocks.last()
                && let Some((address, used)) = block.place(self.used, layout)
            {
                self.used = used;
                return address;
            }
            // A `Layout`'s size is at most `isize::MAX` less its alignment: no overflow. A fresh
            // block of `size + align` bytes fits it whatever the padding.
            let need = layout.size() + layout.align();
            self.blocks.push(if need <= self.pool.size {
                self.pool.take()
            } else {
                Block::new(need)
            });
            self.used = 0;
        }
    }

    /// Gives every block back; what lived in them must be dropped already.
    fn reset(&mut self) {
        for block in self.blocks.drain(..) {
            self.pool.give(block);
        }
        self.used = 0;
    }
}

/// A command's header, in front of its closure.
struct Header {
    next: Option<NonNull<Self>>,
    call: unsafe fn(NonNull<Self>, &mut Encoder),
    drop: unsafe fn(NonNull<Self>),
}

/// A command in the arena: its header, then its closure.
#[repr(C)]
struct Node<F> {
    header: Header,
    f: F,
}

/// No-op command callback for a retained value node (`Commands::retain`).
///
/// # Safety
///
/// `_node` heads a live `Node<T>`.
const unsafe fn noop_call(_node: NonNull<Header>, _encoder: &mut Encoder) {}

/// Calls the closure of the `Node<F>` that `node` heads, through a shared reference.
///
/// # Safety
///
/// `node` heads a live `Node<F>`.
unsafe fn call_ref<F: Fn(&mut Encoder)>(node: NonNull<Header>, encoder: &mut Encoder) {
    // SAFETY: a live `Node<F>` (the caller's promise); `Fn` needs no exclusive access.
    let f = unsafe { &(*node.cast::<Node<F>>().as_ptr()).f };
    f(encoder);
}

/// Drops the `Node<F>` that `node` heads.
///
/// # Safety
///
/// `node` heads a live `Node<F>`, never used again.
unsafe fn drop_node<F>(node: NonNull<Header>) {
    // SAFETY: a live `Node<F>` (the caller's promise), dropped this once.
    unsafe { node.cast::<Node<F>>().drop_in_place() };
}

/// Commands in the order they were recorded, each in an arena's memory.
struct List {
    head: Option<NonNull<Header>>,
    tail: Option<NonNull<Header>>,
}

// SAFETY: the list owns its nodes, and `push` takes `Send` closures only. Not `Sync`: a closure
// need not be.
unsafe impl Send for List {}

impl List {
    const fn new() -> Self {
        Self {
            head: None,
            tail: None,
        }
    }

    /// Places `f` in `arena` behind a header that calls it with `call`, last in the list.
    fn push<F: Send>(
        &mut self,
        arena: &mut Arena,
        f: F,
        call: unsafe fn(NonNull<Header>, &mut Encoder),
    ) {
        let node = arena.allocate(Layout::new::<Node<F>>()).cast::<Node<F>>();
        let header = Header {
            next: None,
            call,
            drop: drop_node::<F>,
        };
        // SAFETY: `allocate` returned unused memory with `Node<F>`'s size and alignment.
        unsafe { node.write(Node { header, f }) };
        let header = node.cast::<Header>();
        match self.tail {
            // SAFETY: the tail is a live node of this list, which nothing else writes.
            Some(tail) => unsafe { (*tail.as_ptr()).next = Some(header) },
            None => self.head = Some(header),
        }
        self.tail = Some(header);
    }

    /// Calls every command, in order.
    ///
    /// # Safety
    ///
    /// Every node's `call` may run with the access the caller holds: `call_mut` nodes need the
    /// list reached by nothing else.
    unsafe fn call(&self, encoder: &mut Encoder) {
        let mut node = self.head;
        while let Some(current) = node {
            // SAFETY: a live node of this list.
            let header = unsafe { current.as_ref() };
            let (next, call) = (header.next, header.call);
            node = next;
            // SAFETY: a live node, called with the access the caller promises its kind needs.
            unsafe { call(current, encoder) };
        }
    }

    /// Drops every command, leaving the list empty.
    fn clear(&mut self) {
        let mut node = self.head.take();
        self.tail = None;
        while let Some(current) = node {
            // SAFETY: a live node, no longer reached from the list.
            let header = unsafe { current.as_ref() };
            let (next, drop) = (header.next, header.drop);
            node = next;
            // SAFETY: a live node, dropped this once: nothing reaches it after this.
            unsafe { drop(current) };
        }
    }
}

/// A chunk's commands and their arena, emptied when the queue retires the chunk.
pub struct Commands {
    arena: Arena,
    list: List,
}

impl Commands {
    /// No commands yet; blocks come from `pool`.
    #[must_use]
    pub const fn new(pool: Arc<BlockPool>) -> Self {
        Self {
            arena: Arena::new(pool),
            list: List::new(),
        }
    }

    /// Records `f`, which runs once when the chunk is encoded and is dropped when it is retired.
    pub fn emit<F: Fn(&mut Encoder) + Send + 'static>(&mut self, f: F) {
        self.list.push(&mut self.arena, f, call_ref::<F>);
    }

    /// Keeps `value` in the arena until [`Self::reset`] drops it when the chunk retires.
    pub fn retain<T: Send + 'static>(&mut self, value: T) {
        self.list.push(&mut self.arena, value, noop_call);
    }

    /// Uninitialized memory for `layout`, valid until the chunk is retired.
    pub fn allocate(&mut self, layout: Layout) -> NonNull<u8> {
        self.arena.allocate(layout)
    }

    /// Runs the commands in the order they were recorded.
    pub fn run(&mut self, encoder: &mut Encoder) {
        // SAFETY: every node is `emit`'s `call_ref` or `retain`'s `noop_call`.
        unsafe { self.list.call(encoder) };
    }

    /// Drops the commands and gives the blocks back.
    pub fn reset(&mut self) {
        self.list.clear();
        self.arena.reset();
    }
}

impl Drop for Commands {
    fn drop(&mut self) {
        self.reset();
    }
}

/// A deferred context's commands: recorded once, replayed each time the list runs.
pub struct CommandList {
    arena: Arena,
    list: List,
}

impl CommandList {
    /// No commands yet; its memory comes in blocks of `block` bytes.
    #[must_use]
    pub fn new(block: usize) -> Self {
        Self {
            arena: Arena::new(Arc::new(BlockPool::new(block))),
            list: List::new(),
        }
    }

    /// Records `f`, which runs each time the list runs and is dropped with the list.
    pub fn emit_fn<F: Fn(&mut Encoder) + Send + 'static>(&mut self, f: F) {
        self.list.push(&mut self.arena, f, call_ref::<F>);
    }

    /// Runs the commands in the order they were recorded.
    pub fn run(&self, encoder: &mut Encoder) {
        // SAFETY: every node is `emit_fn`'s `call_ref`, which a shared reference suffices for.
        unsafe { self.list.call(encoder) };
    }
}

impl Drop for CommandList {
    fn drop(&mut self) {
        self.list.clear();
    }
}

#[cfg(test)]
mod tests;
