//! The compiled-pipeline cache each front end keys its own way.
//!
//! An entry ([`Compiled`]) holds a pipeline's handle, the Metal object that keeps it alive and,
//! while it compiles, the job [`Compiled::ready`] waits on. [`PsoMap`] maps a front end's key to
//! entries and hands a new entry's job to the compile scheduler only once the entry is in the map,
//! starting the scheduler then.

use core::{
    borrow::Borrow,
    hash::Hash,
    sync::atomic::{AtomicU8, AtomicU64, Ordering},
};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use dxmt_util::sync::lock;
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;

use crate::scheduler::{CompileJob, CompileLane, CompileScheduler, JobTicket};

const STATE_PENDING: u8 = 0;
const STATE_READY: u8 = 1;
const STATE_FAILED: u8 = 2;

/// Non-blocking compilation status of a [`Compiled`] pipeline entry.
#[derive(PartialEq, Eq, Debug)]
pub enum CompileState {
    Pending,
    Ready(u64),
    Failed,
}

/// A compiled pipeline: its handle (0 while compiling or after a failed compile) and its spec.
pub struct Compiled<S> {
    state: AtomicU8,
    handle: AtomicU64,
    ticket: AtomicU64,
    pso: Mutex<Option<Obj>>,
    job: Mutex<Option<CompileJob>>,
    /// What the pipeline is compiled from.
    pub spec: Arc<S>,
}

impl<S> Compiled<S> {
    /// An entry compiled by a job, which [`PsoMap::insert_or_existing`] submits.
    ///
    /// The job is installed before the entry exists, so whoever finds the entry can wait on it.
    /// It runs `compile` on the spec if the entry still lives, then stores the Metal object and
    /// publishes its handle (0 when `compile` gives `None`).
    #[must_use]
    pub fn pending(
        spec: Arc<S>,
        compile: impl FnOnce(&S) -> Option<Obj> + Send + 'static,
    ) -> Arc<Self>
    where
        S: Send + Sync + 'static,
    {
        Arc::new_cyclic(|weak: &Weak<Self>| {
            let weak = weak.clone();
            let job = CompileJob::from_fn(move || {
                let Some(compiled) = weak.upgrade() else {
                    return;
                };
                let pso = {
                    let _pool = winemetal_sys::Pool::new();
                    compile(&compiled.spec)
                };
                let handle = pso.as_ref().map_or(0, Obj::handle);
                let next_state = if handle != 0 {
                    STATE_READY
                } else {
                    STATE_FAILED
                };
                *lock(&compiled.pso) = pso;
                compiled.handle.store(handle, Ordering::Release);
                compiled.state.store(next_state, Ordering::Release);
            });
            Self {
                state: AtomicU8::new(STATE_PENDING),
                handle: AtomicU64::new(0),
                ticket: AtomicU64::new(0),
                pso: Mutex::new(None),
                job: Mutex::new(Some(job)),
                spec,
            }
        })
    }

    /// An entry that is already compiled: `handle`, kept alive by `pso`, with no job.
    #[must_use]
    pub fn ready_with(spec: Arc<S>, handle: u64, pso: Option<Obj>) -> Arc<Self> {
        let initial_state = if handle != 0 {
            STATE_READY
        } else {
            STATE_FAILED
        };
        Arc::new(Self {
            state: AtomicU8::new(initial_state),
            handle: AtomicU64::new(handle),
            ticket: AtomicU64::new(0),
            pso: Mutex::new(pso),
            job: Mutex::new(None),
            spec,
        })
    }

    /// Non-blocking snapshot of the entry's compilation state.
    #[must_use]
    pub fn try_ready(&self) -> CompileState {
        match self.state.load(Ordering::Acquire) {
            STATE_READY => CompileState::Ready(self.handle.load(Ordering::Acquire)),
            STATE_FAILED => CompileState::Failed,
            _ => CompileState::Pending,
        }
    }

    /// The [`JobTicket`] assigned when the entry was submitted to a scheduler, if any.
    #[must_use]
    pub fn ticket(&self) -> Option<JobTicket> {
        JobTicket::from_raw(self.ticket.load(Ordering::Acquire))
    }

    /// The pipeline's handle, waiting for its job while it compiles; `None` if the compile failed.
    #[must_use]
    pub fn ready(&self) -> Option<u64> {
        match self.try_ready() {
            CompileState::Ready(h) => return Some(h),
            CompileState::Failed => return None,
            CompileState::Pending => {}
        }
        let job = lock(&self.job).clone();
        if let Some(job) = job {
            job.wait();
        }
        match self.try_ready() {
            CompileState::Ready(h) => Some(h),
            CompileState::Failed | CompileState::Pending => None,
        }
    }

    /// Resolves the pipeline handle, stealing and running an unstarted job on `scheduler` first.
    #[must_use]
    pub fn ready_or_steal(&self, scheduler: Option<&CompileScheduler>) -> Option<u64> {
        match self.try_ready() {
            CompileState::Ready(h) => return Some(h),
            CompileState::Failed => return None,
            CompileState::Pending => {}
        }
        let job = lock(&self.job).clone();
        if let Some(job) = job {
            if let Some(sched) = scheduler {
                sched.run_urgent_or_wait(self.ticket(), &job, false);
            } else {
                job.wait();
            }
        }
        match self.try_ready() {
            CompileState::Ready(h) => Some(h),
            CompileState::Failed | CompileState::Pending => None,
        }
    }
}

/// A front end's compiled pipelines by its key `K`, each built from a spec `S`.
pub struct PsoMap<K, S> {
    map: Mutex<FxHashMap<K, Arc<Compiled<S>>>>,
}

impl<K, S> Default for PsoMap<K, S> {
    fn default() -> Self {
        Self {
            map: Mutex::new(FxHashMap::default()),
        }
    }
}

impl<K: Hash + Eq, S> PsoMap<K, S> {
    /// The entry for `key`, if one was inserted.
    #[must_use]
    pub fn get<Q>(&self, key: &Q) -> Option<Arc<Compiled<S>>>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        lock(&self.map).get(key).cloned()
    }

    /// Inserts `entry` for `key` unless another entry got there first, and returns the one kept.
    ///
    /// A front end builds `entry` outside the map's lock, so two threads may race; the loser's
    /// entry is dropped and its job never submitted. The kept entry's job, if any, is submitted
    /// after the lock is released, to `scheduler`, started here on its first job.
    pub fn insert_or_existing(
        &self,
        key: K,
        entry: Arc<Compiled<S>>,
        scheduler: &OnceLock<CompileScheduler>,
    ) -> Arc<Compiled<S>> {
        self.insert_or_existing_lane(
            key,
            entry,
            scheduler,
            CompileScheduler::default,
            CompileLane::Normal,
        )
    }

    /// Inserts `entry` for `key`, reserving its ticket before publishing and submitting to `lane`.
    pub fn insert_or_existing_lane(
        &self,
        key: K,
        entry: Arc<Compiled<S>>,
        scheduler: &OnceLock<CompileScheduler>,
        init: impl FnOnce() -> CompileScheduler,
        lane: CompileLane,
    ) -> Arc<Compiled<S>> {
        let mut map = lock(&self.map);
        if let Some(existing) = map.get(&key) {
            return Arc::clone(existing);
        }
        let job = lock(&entry.job).clone();
        if let Some(job) = job {
            let sched = scheduler.get_or_init(init);
            let ticket = sched.reserve_ticket();
            entry.ticket.store(ticket.raw(), Ordering::Release);
            map.insert(key, Arc::clone(&entry));
            drop(map);
            sched.submit_lane_with_ticket(ticket, job, lane);
        } else {
            map.insert(key, Arc::clone(&entry));
            drop(map);
        }
        entry
    }

    /// [`Self::insert_or_existing`] for an already compiled entry, which starts no scheduler.
    ///
    /// The entry is a [`Compiled::ready_with`] one: it has no job.
    ///
    /// # Panics
    ///
    /// If `entry` is pending: its job would never run, and its `ready` would wait forever.
    pub fn insert_ready(&self, key: K, entry: Arc<Compiled<S>>) -> Arc<Compiled<S>> {
        assert!(
            lock(&entry.job).is_none(),
            "insert_ready: a pending entry has a job only a scheduler runs"
        );
        self.insert_or_existing(key, entry, &OnceLock::new())
    }

    /// Drops every entry, releasing the Metal objects no draw still holds.
    pub fn clear(&self) {
        lock(&self.map).clear();
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        lock(&self.map).len()
    }

    /// Whether the map has no entry.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        lock(&self.map).is_empty()
    }
}

#[cfg(test)]
mod tests;
