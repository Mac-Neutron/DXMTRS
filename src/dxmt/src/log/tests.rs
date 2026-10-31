use super::{debug, err, info, take_captured, trace, warn};

#[test]
fn levels_count_up_from_trace() {
    trace("t");
    debug("d");
    info("i");
    warn("w");
    err("e");
    let all = [
        (0, "t".to_owned()),
        (1, "d".to_owned()),
        (2, "i".to_owned()),
        (3, "w".to_owned()),
        (4, "e".to_owned()),
    ];
    let logged = take_captured();
    if cfg!(feature = "perf-build") {
        // The performance build drops everything below warnings.
        assert_eq!(logged, all[3..]);
    } else {
        assert_eq!(logged, all);
    }
}

#[test]
fn a_thread_sees_only_its_own_messages() {
    warn("here");
    std::thread::spawn(|| {
        assert_eq!(take_captured(), Vec::<(u32, String)>::new());
        err("there");
    })
    .join()
    .unwrap_or_else(|_| unreachable!("the thread's asserts hold"));
    assert_eq!(take_captured(), [(3, "here".to_owned())]);
}
