use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};

use super::{CompileJob, Scheduler, wait_schedulers_idle};

#[test]
fn grows_to_twice_parallelism() {
    let max_threads = 6;
    let sched = Scheduler::with_bounds(2, max_threads);
    assert_eq!(sched.worker_count(), 2);

    let barrier = Arc::new(Barrier::new(max_threads + 1));
    let mut jobs = Vec::new();
    for _ in 0..max_threads {
        let b = Arc::clone(&barrier);
        let job = CompileJob::new(move || {
            b.wait();
            None
        });
        sched.submit(job.clone());
        jobs.push(job);
    }

    barrier.wait();
    assert_eq!(sched.worker_count(), max_threads);

    for j in jobs {
        j.wait();
    }
    assert!(sched.wait_idle(5_000));
}

#[test]
fn continuation_runs_after() {
    let sched = Scheduler::with_bounds(2, 4);
    let step = Arc::new(AtomicUsize::new(0));

    let step_dep = Arc::clone(&step);
    let dep = CompileJob::new(move || {
        thread::sleep(Duration::from_millis(15));
        step_dep.store(1, Ordering::SeqCst);
        None
    });

    let dep_for_parent = dep.clone();
    let step_parent = Arc::clone(&step);
    let parent = CompileJob::new(move || {
        if !dep_for_parent.is_done() {
            return Some(dep_for_parent.clone());
        }
        assert_eq!(step_parent.load(Ordering::SeqCst), 1);
        step_parent.store(2, Ordering::SeqCst);
        None
    });

    sched.submit(parent.clone());
    sched.submit(dep);

    parent.wait();
    assert_eq!(step.load(Ordering::SeqCst), 2);
    assert!(sched.wait_idle(5_000));
}

#[test]
fn wait_idle_times_out() {
    let sched = Scheduler::with_bounds(1, 2);
    let release = Arc::new(Barrier::new(2));
    let release_worker = Arc::clone(&release);

    let job = CompileJob::new(move || {
        release_worker.wait();
        None
    });
    sched.submit(job.clone());

    assert!(!sched.wait_idle(20));
    assert!(!wait_schedulers_idle(10));

    release.wait();
    job.wait();
    assert!(sched.wait_idle(5_000));
}

#[test]
fn wait_idle_true_when_drained() {
    let sched = Scheduler::with_bounds(2, 4);
    let popped = Arc::new(Barrier::new(2));
    let done = Arc::new(AtomicBool::new(false));

    let popped_worker = Arc::clone(&popped);
    let done_worker = Arc::clone(&done);
    let job = CompileJob::new(move || {
        popped_worker.wait();
        thread::sleep(Duration::from_millis(30));
        done_worker.store(true, Ordering::SeqCst);
        None
    });
    sched.submit(job.clone());

    // Wait until the worker has popped `job` from `tasks` and is inside `run_step`.
    popped.wait();

    // Because `running` was incremented under the queue lock when `job` was popped,
    // `wait_idle` must not return until `job` has completed and set `done = true`.
    assert!(sched.wait_idle(5_000));
    assert!(
        done.load(Ordering::SeqCst),
        "wait_idle returned true before in-flight worker finished"
    );
    assert!(job.is_done());
}

#[test]
fn worker_panic_aborts() {
    let flag = Arc::new(AtomicBool::new(false));
    let flag_cl = Arc::clone(&flag);
    {
        let _guard = super::AbortOnDrop(&flag_cl);
    }
    assert!(
        !flag.load(Ordering::SeqCst),
        "normal drop must not trigger abort"
    );

    let panicked = std::panic::catch_unwind(|| {
        let _guard = super::AbortOnDrop(&flag_cl);
        panic!("simulated worker panic");
    });
    assert!(panicked.is_err());
    assert!(
        flag.load(Ordering::SeqCst),
        "unwinding drop must trigger process abort guard"
    );
}
