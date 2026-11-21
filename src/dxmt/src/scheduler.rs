//! Background shader and pipeline compilation scheduler (`dxmt_tasks.hpp`, `dxmt_tasks.cpp`).

use core::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::Duration,
};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, LazyLock, Mutex, Weak},
    thread::{self, JoinHandle},
    time::Instant,
};

use dxmt_util::sync::{lock, wait_until_finished};

/// Process-wide list of active scheduler instances (`dxmt_tasks.cpp:11-12`).
static SCHEDULERS: LazyLock<Mutex<Vec<Weak<SchedulerInner>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Waits up to `timeout_ms` ms for all compile schedulers to become idle (`wait_schedulers_idle`).
#[must_use]
pub fn wait_idle(timeout_ms: u32) -> bool {
    let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
    let live: Vec<Arc<SchedulerInner>> = {
        let mut schedulers = lock(&SCHEDULERS);
        let mut upgraded = Vec::with_capacity(schedulers.len());
        schedulers.retain(|weak| {
            weak.upgrade().is_some_and(|inner| {
                upgraded.push(inner);
                true
            })
        });
        drop(schedulers);
        upgraded
    };
    for inner in live {
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
        let mut guard = lock(&self.inner.step);
        while !self.inner.done.load(Ordering::Acquire) {
            guard = self
                .inner
                .cv
                .wait(guard)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        drop(guard);
    }

    fn cancel(&self) {
        let step = {
            let mut guard = lock(&self.inner.step);
            let step = guard.take();
            self.inner.done.store(true, Ordering::Release);
            self.inner.cv.notify_all();
            drop(guard);
            step
        };
        drop(step);
    }

    fn run_step(&self) -> Option<Self> {
        if self.is_done() {
            return None;
        }
        let mut step_fn = {
            let mut guard = lock(&self.inner.step);
            guard.take()?
        };
        let next = step_fn();
        let mut guard = lock(&self.inner.step);
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

/// Non-zero identity of a submitted or reserved [`CompileJob`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct JobTicket(u64);

impl JobTicket {
    /// Raw non-zero ticket value.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Reconstructs a ticket from a raw `u64`, returning `None` for `0`.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }
}

/// Priority lane in [`CompileScheduler`]: `Urgent` pops before `Pipeline`, then `Normal`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompileLane {
    Urgent,
    Pipeline,
    Normal,
}

struct QueueState {
    urgent: VecDeque<(JobTicket, CompileJob)>,
    pipelines: VecDeque<(JobTicket, CompileJob)>,
    normal: VecDeque<(JobTicket, CompileJob)>,
    in_flight: usize,
    idle: usize,
    worker_count: usize,
    stop: bool,
}

impl QueueState {
    fn total_queued(&self) -> usize {
        self.urgent.len() + self.pipelines.len() + self.normal.len()
    }

    fn is_empty(&self) -> bool {
        self.urgent.is_empty() && self.pipelines.is_empty() && self.normal.is_empty()
    }

    fn pop(&mut self) -> Option<(CompileLane, JobTicket, CompileJob)> {
        if let Some((ticket, job)) = self.urgent.pop_front() {
            Some((CompileLane::Urgent, ticket, job))
        } else if let Some((ticket, job)) = self.pipelines.pop_front() {
            Some((CompileLane::Pipeline, ticket, job))
        } else {
            self.normal
                .pop_front()
                .map(|(ticket, job)| (CompileLane::Normal, ticket, job))
        }
    }

    fn push(&mut self, lane: CompileLane, ticket: JobTicket, job: CompileJob) {
        match lane {
            CompileLane::Urgent => self.urgent.push_back((ticket, job)),
            CompileLane::Pipeline => self.pipelines.push_back((ticket, job)),
            CompileLane::Normal => self.normal.push_back((ticket, job)),
        }
    }

    fn promote(&mut self, ticket: JobTicket) -> bool {
        if self.urgent.iter().any(|(queued, _)| *queued == ticket) {
            return true;
        }
        for lane in [&mut self.pipelines, &mut self.normal] {
            if let Some(pos) = lane.iter().position(|(queued, _)| *queued == ticket) {
                if let Some(entry) = lane.remove(pos) {
                    self.urgent.push_back(entry);
                }
                return true;
            }
        }
        false
    }

    fn promote_and_find_urgent(
        &mut self,
        ticket: Option<JobTicket>,
        job: &CompileJob,
    ) -> Option<usize> {
        let matches = |(queued, queued_job): &(JobTicket, CompileJob)| {
            ticket.map_or_else(
                || Arc::ptr_eq(&queued_job.inner, &job.inner),
                |want| *queued == want,
            )
        };
        if let Some(pos) = self.urgent.iter().position(matches) {
            return Some(pos);
        }
        for lane in [&mut self.pipelines, &mut self.normal] {
            if let Some(pos) = lane.iter().position(matches) {
                if let Some(entry) = lane.remove(pos) {
                    self.urgent.push_back(entry);
                }
                return Some(self.urgent.len() - 1);
            }
        }
        None
    }
}

struct SchedulerInner {
    state: Mutex<QueueState>,
    work_cv: Condvar,
    idle_cv: Condvar,
    max_threads: usize,
    workers: Mutex<Vec<JoinHandle<()>>>,
    next_ticket: AtomicU64,
}

impl SchedulerInner {
    fn alloc_ticket(&self) -> JobTicket {
        loop {
            let raw = self.next_ticket.fetch_add(1, Ordering::Relaxed);
            if raw != 0 {
                return JobTicket(raw);
            }
        }
    }

    fn wait_idle(&self, timeout_ms: u32) -> bool {
        let deadline = Instant::now() + Duration::from_millis(u64::from(timeout_ms));
        let mut guard = lock(&self.state);
        while !guard.is_empty() || guard.in_flight > 0 {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (next_guard, res) = self
                .idle_cv
                .wait_timeout(guard, deadline - now)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard = next_guard;
            if res.timed_out() && (!guard.is_empty() || guard.in_flight > 0) {
                return false;
            }
        }
        drop(guard);
        true
    }
}

struct InFlightGuard<'a> {
    inner: &'a SchedulerInner,
    armed: bool,
}

impl<'a> InFlightGuard<'a> {
    const fn new(inner: &'a SchedulerInner) -> Self {
        Self { inner, armed: true }
    }

    const fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let mut st = lock(&self.inner.state);
            st.in_flight = st.in_flight.saturating_sub(1);
            if st.is_empty() && st.in_flight == 0 {
                self.inner.idle_cv.notify_all();
            }
            drop(st);
        }
    }
}

fn spawn_worker(inner: &Arc<SchedulerInner>) {
    let inner_cl = Arc::clone(inner);
    let handle = thread::spawn(move || {
        let _abort = ProcessAbortGuard;
        loop {
            let (lane, ticket, job) = {
                let mut st = lock(&inner_cl.state);
                while st.is_empty() && !st.stop {
                    st.idle += 1;
                    st = inner_cl
                        .work_cv
                        .wait(st)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    st.idle -= 1;
                }
                if st.stop && st.is_empty() {
                    return;
                }
                let Some((lane, ticket, j)) = st.pop() else {
                    continue;
                };
                st.in_flight += 1;
                drop(st);
                (lane, ticket, j)
            };

            let mut guard = InFlightGuard::new(&inner_cl);
            let continuation = job.run_step();

            let mut st = lock(&inner_cl.state);
            guard.disarm();
            st.in_flight -= 1;
            if let Some(dep) = continuation {
                if !dep.is_done() {
                    let dep_ticket = inner_cl.alloc_ticket();
                    st.push(lane, dep_ticket, dep);
                }
                st.push(lane, ticket, job);
                inner_cl.work_cv.notify_all();
            } else if st.is_empty() && st.in_flight == 0 {
                inner_cl.idle_cv.notify_all();
            }
            drop(st);
        }
    });
    lock(&inner.workers).push(handle);
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
                urgent: VecDeque::new(),
                pipelines: VecDeque::new(),
                normal: VecDeque::new(),
                in_flight: 0,
                idle: 0,
                worker_count: init_workers,
                stop: false,
            }),
            work_cv: Condvar::new(),
            idle_cv: Condvar::new(),
            max_threads: max_workers,
            workers: Mutex::new(Vec::with_capacity(max_workers)),
            next_ticket: AtomicU64::new(1),
        });
        lock(&SCHEDULERS).push(Arc::downgrade(&inner));

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
        lock(&self.inner.state).worker_count
    }

    /// Reserves a non-zero [`JobTicket`] before inserting an entry into a cache map.
    #[must_use]
    pub fn reserve_ticket(&self) -> JobTicket {
        self.inner.alloc_ticket()
    }

    /// Submits `job` with a pre-reserved `ticket` into `lane`, scaling workers on demand.
    pub fn submit_lane_with_ticket(&self, ticket: JobTicket, job: CompileJob, lane: CompileLane) {
        let should_spawn = {
            let mut st = lock(&self.inner.state);
            st.push(lane, ticket, job);
            let demand = st.total_queued() + st.in_flight > st.worker_count
                && st.worker_count < self.inner.max_threads;
            if demand {
                st.worker_count += 1;
            }
            drop(st);
            demand
        };
        if should_spawn {
            spawn_worker(&self.inner);
        }
        self.inner.work_cv.notify_one();
    }

    /// Submits `job` into `lane` and returns its [`JobTicket`].
    #[must_use]
    pub fn submit_lane(&self, job: CompileJob, lane: CompileLane) -> JobTicket {
        let ticket = self.reserve_ticket();
        self.submit_lane_with_ticket(ticket, job, lane);
        ticket
    }

    /// Submits `job` to the normal worker queue (`submit`), spawning a worker on demand.
    pub fn submit(&self, job: CompileJob) {
        let _ = self.submit_lane(job, CompileLane::Normal);
    }

    /// Moves an unstarted job identified by `ticket` into `CompileLane::Urgent`.
    ///
    /// Returns `true` if the job is in `Urgent`, or `false` if it already started or finished.
    #[must_use]
    pub fn promote(&self, ticket: JobTicket) -> bool {
        let mut st = lock(&self.inner.state);
        let promoted = st.promote(ticket);
        drop(st);
        promoted
    }

    /// Drops all unstarted queued jobs from every lane and wakes any waiters on them.
    pub fn cancel_unstarted(&self) {
        let (urgent, pipelines, normal) = {
            let mut st = lock(&self.inner.state);
            let urgent = core::mem::take(&mut st.urgent);
            let pipelines = core::mem::take(&mut st.pipelines);
            let normal = core::mem::take(&mut st.normal);
            if st.in_flight == 0 {
                self.inner.idle_cv.notify_all();
            }
            drop(st);
            (urgent, pipelines, normal)
        };
        for (_, job) in urgent.into_iter().chain(pipelines).chain(normal) {
            job.cancel();
        }
    }

    /// Promotes `job` to urgent, steals and executes it on the caller if appropriate, or waits.
    ///
    /// Never returns while `!job.is_done()`. When `share_with_idle` is `true`, leaves the `idle`
    /// oldest urgent jobs to idle workers.
    pub fn run_urgent_or_wait(
        &self,
        ticket: Option<JobTicket>,
        job: &CompileJob,
        share_with_idle: bool,
    ) {
        if job.is_done() {
            return;
        }
        let stolen = {
            let mut st = lock(&self.inner.state);
            let Some(pos) = st.promote_and_find_urgent(ticket, job) else {
                drop(st);
                job.wait();
                return;
            };
            if share_with_idle && pos < st.idle {
                drop(st);
                job.wait();
                return;
            }
            let Some(entry) = st.urgent.remove(pos) else {
                drop(st);
                job.wait();
                return;
            };
            st.in_flight += 1;
            drop(st);
            entry
        };

        let (stolen_ticket, stolen_job) = stolen;
        let mut guard = InFlightGuard::new(&self.inner);
        let continuation = stolen_job.run_step();

        let mut st = lock(&self.inner.state);
        guard.disarm();
        st.in_flight -= 1;
        if let Some(dep) = continuation {
            if !dep.is_done() {
                let dep_ticket = self.inner.alloc_ticket();
                st.push(CompileLane::Urgent, dep_ticket, dep);
            }
            st.push(CompileLane::Urgent, stolen_ticket, stolen_job);
            self.inner.work_cv.notify_all();
            drop(st);
            job.wait();
            return;
        }
        if st.is_empty() && st.in_flight == 0 {
            self.inner.idle_cv.notify_all();
        }
        drop(st);
        if !job.is_done() {
            job.wait();
        }
    }

    /// Waits up to `timeout_ms` milliseconds for this scheduler's queue and workers to drain.
    #[must_use]
    pub fn wait_idle(&self, timeout_ms: u32) -> bool {
        self.inner.wait_idle(timeout_ms)
    }
}

impl Drop for CompileScheduler {
    fn drop(&mut self) {
        let self_ptr = Arc::as_ptr(&self.inner);
        lock(&SCHEDULERS).retain(|weak| weak.as_ptr() != self_ptr && weak.strong_count() > 0);
        {
            let mut st = lock(&self.inner.state);
            st.stop = true;
            drop(st);
        }
        self.inner.work_cv.notify_all();
        let handles = core::mem::take(&mut *lock(&self.inner.workers));
        for h in handles {
            wait_until_finished(h);
        }
    }
}

#[cfg(test)]
mod tests;
