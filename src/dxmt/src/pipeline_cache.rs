//! The compiled-pipeline cache each front end keys its own way.
//!
//! An entry ([`Compiled`]) holds a pipeline's handle, the Metal object that keeps it alive and,
//! while it compiles, the job [`Compiled::ready`] waits on. [`PsoMap`] maps a front end's key to
//! entries and hands a new entry's job to the compile scheduler only once the entry is in the map,
//! starting the scheduler then.

use core::{
    borrow::Borrow,
    hash::Hash,
    sync::atomic::{AtomicU64, Ordering},
};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use dxmt_util::sync::lock;
use rustc_hash::FxHashMap;
use winemetal_sys::Obj;

use crate::scheduler::{CompileJob, CompileScheduler};

/// A compiled pipeline: its handle (0 while compiling or after a failed compile) and its spec.
pub struct Compiled<S> {
    handle: AtomicU64,
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
                let pso = compile(&compiled.spec);
                let handle = pso.as_ref().map_or(0, Obj::handle);
                *lock(&compiled.pso) = pso;
                compiled.handle.store(handle, Ordering::Release);
            });
            Self {
                handle: AtomicU64::new(0),
                pso: Mutex::new(None),
                job: Mutex::new(Some(job)),
                spec,
            }
        })
    }

    /// An entry that is already compiled: `handle`, kept alive by `pso`, with no job.
    #[must_use]
    pub fn ready_with(spec: Arc<S>, handle: u64, pso: Option<Obj>) -> Arc<Self> {
        Arc::new(Self {
            handle: AtomicU64::new(handle),
            pso: Mutex::new(pso),
            job: Mutex::new(None),
            spec,
        })
    }

    /// The pipeline's handle, waiting for its job while it compiles; `None` if the compile failed.
    #[must_use]
    pub fn ready(&self) -> Option<u64> {
        let h = self.handle.load(Ordering::Acquire);
        if h != 0 {
            return Some(h);
        }
        let job = lock(&self.job).clone();
        if let Some(job) = job {
            job.wait();
        }
        let h2 = self.handle.load(Ordering::Acquire);
        (h2 != 0).then_some(h2)
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
        let mut map = lock(&self.map);
        if let Some(existing) = map.get(&key) {
            return Arc::clone(existing);
        }
        let job = lock(&entry.job).clone();
        map.insert(key, Arc::clone(&entry));
        drop(map);
        if let Some(job) = job {
            scheduler.get_or_init(CompileScheduler::default).submit(job);
        }
        entry
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
