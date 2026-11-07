//! Dynamic buffers and linear textures, renamed on each discarding map.
//!
//! A discarding map makes a new allocation the resource's name (the "immediate name") and queues
//! the old one, free again once the GPU finished the sequence that last used it; a command list
//! that renamed it gives its allocations back through `recycle`. A dynamic buffer also lends
//! allocations to a VM host's guest under ids of their own (pieces), which the guest fills and
//! hands back to become the name. The C++'s `DynamicBuffer` and `DynamicLinearTexture`.
//!
//! Written ahead for the switch of `d3d11.dll`, their first caller.

use core::mem;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use dxmt_util::sync::lock;
use rustc_hash::FxHashMap;

use crate::resource::{
    Buffer, BufferAllocation, BufferAllocationFlags, Texture, TextureAllocation,
    TextureAllocationFlags,
};

/// A buffer renamed on each discarding map.
///
/// Its methods take `&self` and lock: contexts on other threads allocate and recycle too.
pub struct DynamicBuffer {
    buffer: Arc<Buffer>,
    flags: BufferAllocationFlags,
    state: Mutex<Renames<BufferAllocation>>,
}

impl DynamicBuffer {
    /// `buffer`, renamed to allocations `flags` describe.
    ///
    /// Its current allocation is the first name; `None` if it has none.
    #[must_use]
    pub fn new(buffer: Arc<Buffer>, flags: BufferAllocationFlags) -> Option<Self> {
        let name = buffer.current()?;
        Some(Self {
            buffer,
            flags,
            state: Mutex::new(Renames::new(name)),
        })
    }

    #[must_use]
    pub const fn buffer(&self) -> &Arc<Buffer> {
        &self.buffer
    }

    /// An allocation the GPU is done with as of `coherent`, or a new one.
    #[must_use]
    pub fn allocate(&self, coherent: u64) -> Arc<BufferAllocation> {
        let free = lock(&self.state).take_free(coherent);
        free.unwrap_or_else(|| self.buffer.allocate(self.flags))
    }

    /// Makes `allocation` the name, with `suballocation` current.
    ///
    /// The old name is free once the GPU finished `current_seq`, unless a command list owns it
    /// (it gives it back itself).
    pub fn update_immediate_name(
        &self,
        current_seq: u64,
        allocation: Arc<BufferAllocation>,
        suballocation: u32,
        owned_by_command_list: bool,
    ) {
        lock(&self.state).rename(
            current_seq,
            allocation,
            suballocation,
            owned_by_command_list,
        );
    }

    /// Takes back an allocation a command list used: free once the GPU finished `current_seq`.
    pub fn recycle(&self, current_seq: u64, allocation: Arc<BufferAllocation>) {
        lock(&self.state).recycle(current_seq, allocation);
    }

    /// Moves the name to its next suballocation, `None` when it has no next one.
    ///
    /// Returns the name, that suballocation and its CPU memory (or null): one lock for the step
    /// and the reads a discarding map makes after it.
    pub fn next_suballocation(&self) -> Option<(Arc<BufferAllocation>, u32, *mut u8)> {
        let mut s = lock(&self.state);
        let next = s.suballocation + 1;
        if !s.name.has_suballocation(next) {
            return None;
        }
        s.suballocation = next;
        Some((Arc::clone(&s.name), next, s.name.mapped_memory(next)))
    }

    /// Lends an allocation found as [`Self::allocate`] finds one, under a new id.
    pub fn lend(&self, coherent: u64) -> (u64, Arc<BufferAllocation>) {
        let allocation = self.allocate(coherent);
        let id = lock(&self.state).lend(Arc::clone(&allocation));
        (id, allocation)
    }

    /// Takes back the allocation lent under `id`, if it is lent.
    pub fn take_lent(&self, id: u64) -> Option<Arc<BufferAllocation>> {
        lock(&self.state).take_lent(id)
    }

    /// The name's id, given it when it has none (an allocation the front end renamed to itself).
    pub fn immediate_piece(&self) -> u64 {
        lock(&self.state).immediate_piece()
    }

    /// Gives the name `id`, the one it was lent under.
    pub fn set_immediate_piece(&self, id: u64) {
        lock(&self.state).name_piece = id;
    }

    pub fn use_immediate_suballocation(&self, suballocation: u32) {
        lock(&self.state).suballocation = suballocation;
    }

    #[must_use]
    pub fn immediate_name(&self) -> Arc<BufferAllocation> {
        Arc::clone(&lock(&self.state).name)
    }

    #[must_use]
    pub fn immediate_suballocation(&self) -> u32 {
        lock(&self.state).suballocation
    }

    /// The name's current suballocation in CPU memory, or null.
    #[must_use]
    pub fn immediate_mapped_memory(&self) -> *mut u8 {
        let s = lock(&self.state);
        s.name.mapped_memory(s.suballocation)
    }
}

/// A linear texture renamed on each discarding map.
///
/// Its methods take `&self` and lock: contexts on other threads allocate and recycle too.
pub struct DynamicLinearTexture {
    texture: Arc<Texture>,
    flags: TextureAllocationFlags,
    state: Mutex<Renames<TextureAllocation>>,
}

impl DynamicLinearTexture {
    /// `texture`, renamed to allocations `flags` describe.
    ///
    /// Its current allocation is the first name; `None` if it has none.
    #[must_use]
    pub fn new(texture: Arc<Texture>, flags: TextureAllocationFlags) -> Option<Self> {
        let name = texture.current()?;
        Some(Self {
            texture,
            flags,
            state: Mutex::new(Renames::new(name)),
        })
    }

    #[must_use]
    pub const fn texture(&self) -> &Arc<Texture> {
        &self.texture
    }

    /// An allocation the GPU is done with as of `coherent`, or a new one.
    #[must_use]
    pub fn allocate(&self, coherent: u64) -> Arc<TextureAllocation> {
        let free = lock(&self.state).take_free(coherent);
        free.unwrap_or_else(|| self.texture.allocate(self.flags))
    }

    /// Makes `allocation` the name.
    ///
    /// The old name is free once the GPU finished `current_seq`, unless a command list owns it
    /// (it gives it back itself).
    pub fn update_immediate_name(
        &self,
        current_seq: u64,
        allocation: Arc<TextureAllocation>,
        owned_by_command_list: bool,
    ) {
        lock(&self.state).rename(current_seq, allocation, 0, owned_by_command_list);
    }

    /// Takes back an allocation a command list used: free once the GPU finished `current_seq`.
    pub fn recycle(&self, current_seq: u64, allocation: Arc<TextureAllocation>) {
        lock(&self.state).recycle(current_seq, allocation);
    }

    #[must_use]
    pub fn immediate_name(&self) -> Arc<TextureAllocation> {
        Arc::clone(&lock(&self.state).name)
    }

    /// The name's CPU memory, or null.
    #[must_use]
    pub fn mapped_memory(&self) -> *mut u8 {
        lock(&self.state).name.mapped_memory()
    }
}

/// A resource's name, the allocations renamed away from it, and (a buffer's) lent pieces.
struct Renames<A> {
    /// Allocations renamed away, with the sequence after which the GPU is done with each.
    fifo: VecDeque<(Arc<A>, u64)>,
    name: Arc<A>,
    /// A command list owns the name and gives it back through `recycle`.
    owned_by_command_list: bool,
    /// The name's current suballocation (a buffer's).
    suballocation: u32,
    /// Allocations lent out, by id; they go with the resource.
    lent: FxHashMap<u64, Arc<A>>,
    next_piece: u64,
    /// The name's id, 0 for none yet.
    name_piece: u64,
}

impl<A> Renames<A> {
    fn new(name: Arc<A>) -> Self {
        Self {
            fifo: VecDeque::new(),
            name,
            owned_by_command_list: false,
            suballocation: 0,
            lent: FxHashMap::default(),
            next_piece: 1,
            name_piece: 0,
        }
    }

    fn take_free(&mut self, coherent: u64) -> Option<Arc<A>> {
        self.fifo
            .pop_front_if(|(_, at)| *at <= coherent)
            .map(|(a, _)| a)
    }

    fn rename(&mut self, current_seq: u64, allocation: Arc<A>, suballocation: u32, owned: bool) {
        let old = mem::replace(&mut self.name, allocation);
        if !self.owned_by_command_list {
            self.fifo.push_back((old, current_seq));
        }
        self.suballocation = suballocation;
        self.owned_by_command_list = owned;
        self.name_piece = 0;
    }

    fn recycle(&mut self, current_seq: u64, allocation: Arc<A>) {
        if self.owned_by_command_list && Arc::ptr_eq(&self.name, &allocation) {
            self.owned_by_command_list = false;
            return;
        }
        self.fifo.push_back((allocation, current_seq));
    }

    fn lend(&mut self, allocation: Arc<A>) -> u64 {
        let id = self.next_piece;
        self.next_piece += 1;
        self.lent.insert(id, allocation);
        id
    }

    fn take_lent(&mut self, id: u64) -> Option<Arc<A>> {
        self.lent.remove(&id)
    }

    const fn immediate_piece(&mut self) -> u64 {
        if self.name_piece == 0 {
            self.name_piece = self.next_piece;
            self.next_piece += 1;
        }
        self.name_piece
    }
}

#[cfg(test)]
mod tests;
