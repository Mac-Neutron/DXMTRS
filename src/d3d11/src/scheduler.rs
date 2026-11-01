//! Background shader and pipeline compilation scheduler (`dxmt_tasks.hpp`, `dxmt_tasks.cpp`).

use core::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, LazyLock, Mutex},
    thread::{self, JoinHandle},
    time::Instant,
};

/// Process-wide list of active scheduler instances (`dxmt_tasks.cpp:11-12`).
pub static SCHEDULERS: LazyLock<Mutex<Vec<usize>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Waits up to `timeout_ms` ms for all compile schedulers to become idle (`wait_schedulers_idle`).
#[must_use]
pub fn wait_idle(timeout_ms: u32) -> bool {
    let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
    let ptrs = SCHEDULERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for addr in ptrs {
        let sched = addr as *const SchedulerInner;
        // SAFETY: `SCHEDULERS` holds addresses of live `SchedulerInner` allocations, which remove
        // their entry from `SCHEDULERS` before being dropped.
        let inner = unsafe { &*sched };
        let remaining = deadline.saturating_duration_since(Instant::now());
        let ms = u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX);
        if !inner.wait_idle(ms) {
            return false;
        }
    }
    true
}

pub use wait_idle as wait_schedulers_idle;

#[cfg(test)]
struct AbortOnDrop<'a>(&'a AtomicBool);

#[cfg(test)]
impl Drop for AbortOnDrop<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.0.store(true, Ordering::SeqCst);
        }
    }
}

struct ProcessAbortGuard;

impl Drop for ProcessAbortGuard {
    fn drop(&mut self) {
        if thread::panicking() {
            std::process::abort();
        }
    }
}

type StepFn = Box<dyn FnMut() -> Option<CompileJob> + Send + 'static>;

struct JobInner {
    done: AtomicBool,
    step: Mutex<Option<StepFn>>,
    cv: Condvar,
}

/// Asynchronous compilation work item (`ThreadpoolWork`).
pub struct CompileJob {
    inner: Arc<JobInner>,
}

impl Clone for CompileJob {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl CompileJob {
    /// Creates a multi-step compile job where returning `Some(dep)` re-queues the job after `dep`.
    #[must_use]
    pub fn new(step: impl FnMut() -> Option<Self> + Send + 'static) -> Self {
        Self {
            inner: Arc::new(JobInner {
                done: AtomicBool::new(false),
                step: Mutex::new(Some(Box::new(step))),
                cv: Condvar::new(),
            }),
        }
    }

    /// Creates a one-shot compile job from `work`.
    #[must_use]
    pub fn from_fn(work: impl FnOnce() + Send + 'static) -> Self {
        let mut opt = Some(work);
        Self::new(move || {
            if let Some(f) = opt.take() {
                f();
            }
            None
        })
    }

    /// Returns `true` when the job has finished executing (`GetIsDone`).
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.inner.done.load(Ordering::Acquire)
    }

    /// Blocks until the job completes (`WaitThreadpoolWork`).
    pub fn wait(&self) {
        if self.is_done() {
            return;
        }
        let mut guard = self
            .inner
            .step
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !self.inner.done.load(Ordering::Acquire) {
            guard = self
                .inner
                .cv
                .wait(guard)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(guard);
    }

    fn run_step(&self) -> Option<Self> {
        if self.is_done() {
            return None;
        }
        let mut step_fn = {
            let mut guard = self
                .inner
                .step
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take()?
        };
        let next = step_fn();
        let mut guard = self
            .inner
            .step
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if next.is_some() && !self.inner.done.load(Ordering::Acquire) {
            *guard = Some(step_fn);
            drop(guard);
            next
        } else {
            self.inner.done.store(true, Ordering::Release);
            self.inner.cv.notify_all();
            drop(guard);
            None
        }
    }
}

struct QueueState {
    queue: VecDeque<CompileJob>,
    in_flight: usize,
    worker_count: usize,
    stop: bool,
}

struct SchedulerInner {
    state: Mutex<QueueState>,
    work_cv: Condvar,
    idle_cv: Condvar,
    max_threads: usize,
    workers: Mutex<Vec<JoinHandle<()>>>,
}

impl SchedulerInner {
    fn wait_idle(&self, timeout_ms: u32) -> bool {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        let mut guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while !guard.queue.is_empty() || guard.in_flight > 0 {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (next_guard, res) = self
                .idle_cv
                .wait_timeout(guard, deadline - now)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard = next_guard;
            if res.timed_out() && (!guard.queue.is_empty() || guard.in_flight > 0) {
                return false;
            }
        }
        true
    }
}

fn spawn_worker(inner: &Arc<SchedulerInner>) {
    let inner_cl = Arc::clone(inner);
    let handle = thread::spawn(move || {
        let _abort = ProcessAbortGuard;
        loop {
            let job = {
                let mut st = inner_cl
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                while st.queue.is_empty() && !st.stop {
                    st = inner_cl
                        .work_cv
                        .wait(st)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                }
                if st.stop && st.queue.is_empty() {
                    return;
                }
                let Some(j) = st.queue.pop_front() else {
                    continue;
                };
                st.in_flight += 1;
                j
            };

            let continuation = job.run_step();

            let mut st = inner_cl
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            st.in_flight -= 1;
            if let Some(dep) = continuation {
                if !dep.is_done() {
                    st.queue.push_back(dep);
                }
                st.queue.push_back(job);
                inner_cl.work_cv.notify_all();
            } else if st.queue.is_empty() && st.in_flight == 0 {
                inner_cl.idle_cv.notify_all();
            }
            drop(st);
        }
    });
    inner
        .workers
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(handle);
}

/// Multi-worker thread pool for asynchronous shader/pipeline compilation (`task_scheduler`).
pub struct CompileScheduler {
    inner: Arc<SchedulerInner>,
}

/// Alias matching the C++ `task_scheduler` name in unit tests.
pub type Scheduler = CompileScheduler;

impl Default for CompileScheduler {
    fn default() -> Self {
        let par = thread::available_parallelism().map_or(2, std::num::NonZero::get);
        Self::with_bounds(2.min(par * 2), (par * 2).max(2))
    }
}

impl CompileScheduler {
    /// Creates a compile scheduler starting with `initial` workers and scaling up to `max_threads`.
    #[must_use]
    pub fn with_bounds(initial: usize, max_threads: usize) -> Self {
        let init_workers = initial.max(1);
        let max_workers = max_threads.max(init_workers);
        let inner = Arc::new(SchedulerInner {
            state: Mutex::new(QueueState {
                queue: VecDeque::new(),
                in_flight: 0,
                worker_count: init_workers,
                stop: false,
            }),
            work_cv: Condvar::new(),
            idle_cv: Condvar::new(),
            max_threads: max_workers,
            workers: Mutex::new(Vec::with_capacity(max_workers)),
        });
        let addr = Arc::as_ptr(&inner) as usize;
        SCHEDULERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(addr);

        for _ in 0..init_workers {
            spawn_worker(&inner);
        }
        Self { inner }
    }

    /// Creates a compile scheduler with `worker_count` initial threads and `2 * parallelism` max.
    #[must_use]
    pub fn new(worker_count: usize) -> Self {
        let par = thread::available_parallelism().map_or(2, std::num::NonZero::get);
        let max_threads = (par * 2).max(worker_count.max(1));
        Self::with_bounds(worker_count, max_threads)
    }

    /// Returns the current number of spawned worker threads.
    #[must_use]
    pub fn worker_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .worker_count
    }

    /// Submits `job` to the worker queue (`submit`), spawning a worker if demand exceeds supply.
    pub fn submit(&self, job: CompileJob) {
        let should_spawn = {
            let mut st = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            st.queue.push_back(job);
            if st.queue.len() + st.in_flight > st.worker_count
                && st.worker_count < self.inner.max_threads
            {
                st.worker_count += 1;
                true
            } else {
                false
            }
        };
        if should_spawn {
            spawn_worker(&self.inner);
        }
        self.inner.work_cv.notify_one();
    }

    /// Waits up to `timeout_ms` milliseconds for this scheduler's queue and workers to drain.
    #[must_use]
    pub fn wait_idle(&self, timeout_ms: u32) -> bool {
        self.inner.wait_idle(timeout_ms)
    }
}

impl Drop for CompileScheduler {
    fn drop(&mut self) {
        let addr = Arc::as_ptr(&self.inner) as usize;
        SCHEDULERS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|&a| a != addr);
        {
            let mut st = self
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            st.stop = true;
        }
        self.inner.work_cv.notify_all();
        let handles = core::mem::take(
            &mut *self
                .inner
                .workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        for h in handles {
            while !h.is_finished() {
                thread::yield_now();
            }
            drop(h);
        }
    }
}

#[cfg(test)]
mod tests;
