use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::{
    sync::{Arc, Barrier, Mutex},
    thread,
    time::Duration,
};

use super::{CompileJob, CompileLane, Scheduler, wait_schedulers_idle};

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

#[test]
fn three_lanes_pop_urgent_then_pipeline_then_normal_and_promote_moves() {
    let sched = Scheduler::with_bounds(1, 1);
    let gate = Arc::new(Barrier::new(2));
    let gate_w = Arc::clone(&gate);
    let order = Arc::new(Mutex::new(Vec::new()));

    let blocker = CompileJob::from_fn(move || {
        gate_w.wait();
    });
    sched.submit(blocker);

    let push_tag = |tag: &'static str, lane: CompileLane| {
        let order_w = Arc::clone(&order);
        let job = CompileJob::from_fn(move || {
            order_w.lock().unwrap().push(tag);
        });
        sched.submit_lane(job, lane)
    };

    let t_norm1 = push_tag("normal-1", CompileLane::Normal);
    let _t_pipe1 = push_tag("pipeline-1", CompileLane::Pipeline);
    let _t_urgent1 = push_tag("urgent-1", CompileLane::Urgent);
    let _t_pipe2 = push_tag("pipeline-2", CompileLane::Pipeline);

    assert!(sched.promote(t_norm1));

    gate.wait();
    assert!(sched.wait_idle(5_000));
    assert!(!sched.promote(t_norm1));

    let got = order.lock().unwrap().clone();
    assert_eq!(got, ["urgent-1", "normal-1", "pipeline-1", "pipeline-2"]);
}

#[test]
fn run_urgent_or_wait_steals_and_waits_for_multistep_continuation() {
    let sched = Scheduler::with_bounds(1, 1);
    let worker_entered = Arc::new(Barrier::new(2));
    let worker_release = Arc::new(Barrier::new(2));
    let worker_entered_w = Arc::clone(&worker_entered);
    let worker_release_w = Arc::clone(&worker_release);

    let blocker = CompileJob::from_fn(move || {
        worker_entered_w.wait();
        worker_release_w.wait();
    });
    sched.submit(blocker);
    worker_entered.wait();

    let dep_ran = Arc::new(AtomicBool::new(false));
    let dep_ran_w = Arc::clone(&dep_ran);
    let dep = CompileJob::from_fn(move || {
        dep_ran_w.store(true, Ordering::SeqCst);
    });
    let dep_for_parent = dep;
    let step = Arc::new(AtomicUsize::new(0));
    let step_w = Arc::clone(&step);
    let parent = CompileJob::new(move || {
        let prev = step_w.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            return Some(dep_for_parent.clone());
        }
        None
    });

    let ticket = sched.submit_lane(parent.clone(), CompileLane::Normal);

    // Release the worker thread soon so the continuation can run after caller steals step 0.
    let rel = thread::spawn(move || {
        thread::sleep(Duration::from_millis(15));
        worker_release.wait();
    });

    // Caller steals step 0 while worker is blocked, re-queues dep + parent, and waits for done.
    sched.run_urgent_or_wait(Some(ticket), &parent, false);
    assert!(parent.is_done());
    assert!(dep_ran.load(Ordering::SeqCst));
    assert_eq!(step.load(Ordering::SeqCst), 2);
    rel.join().expect("releaser");
    assert!(sched.wait_idle(5_000));
}

#[test]
fn run_urgent_or_wait_share_with_idle_leaves_job_for_idle_worker() {
    let sched = Scheduler::with_bounds(1, 1);
    // Let worker become idle first.
    assert!(sched.wait_idle(5_000));

    let gate = Arc::new(Barrier::new(2));
    let gate_w = Arc::clone(&gate);
    let ran_on = Arc::new(Mutex::new(None));
    let ran_on_w = Arc::clone(&ran_on);
    let job = CompileJob::from_fn(move || {
        *ran_on_w.lock().unwrap() = Some(thread::current().id());
        gate_w.wait();
    });
    let caller_id = thread::current().id();
    // Submit directly into queue while worker might be idle; hold lock? Even better: just verify
    // `run_urgent_or_wait(..., true)` completes without hanging regardless of who picks it up.
    let ticket = sched.submit_lane(job.clone(), CompileLane::Normal);
    gate.wait();
    sched.run_urgent_or_wait(Some(ticket), &job, true);
    assert!(job.is_done());
    let worker_id = ran_on.lock().unwrap().expect("job ran");
    assert_ne!(worker_id, caller_id);
    assert!(sched.wait_idle(5_000));
}

#[test]
fn cancel_unstarted_drops_queued_jobs_and_allows_wait_idle() {
    let sched = Scheduler::with_bounds(1, 1);
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let entered_w = Arc::clone(&entered);
    let release_w = Arc::clone(&release);
    sched.submit(CompileJob::from_fn(move || {
        entered_w.wait();
        release_w.wait();
    }));
    entered.wait();

    let ran = Arc::new(AtomicUsize::new(0));
    let mut jobs = Vec::new();
    for lane in [
        CompileLane::Urgent,
        CompileLane::Pipeline,
        CompileLane::Normal,
    ] {
        let ran_w = Arc::clone(&ran);
        let job = CompileJob::from_fn(move || {
            ran_w.fetch_add(1, Ordering::SeqCst);
        });
        let _ = sched.submit_lane(job.clone(), lane);
        jobs.push(job);
    }
    sched.cancel_unstarted();
    for job in &jobs {
        job.wait();
        assert!(job.is_done());
    }
    release.wait();
    assert!(sched.wait_idle(5_000));
    assert_eq!(ran.load(Ordering::SeqCst), 0);
}

#[test]
fn weak_schedulers_registry_survives_concurrent_drop() {
    let sched = Arc::new(Scheduler::with_bounds(1, 1));
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let entered_w = Arc::clone(&entered);
    let release_w = Arc::clone(&release);
    sched.submit(CompileJob::from_fn(move || {
        entered_w.wait();
        release_w.wait();
    }));
    entered.wait();

    let dropper = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        release.wait();
        drop(sched);
    });
    assert!(wait_schedulers_idle(5_000));
    dropper.join().expect("dropper");
}
