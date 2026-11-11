use super::{RuntimeLatch, effective_mode};
use crate::config::RuntimeMode;

#[test]
fn latch_takes_the_first_mode() {
    let latch = RuntimeLatch::new();
    assert_eq!(latch.get(), None);
    assert_eq!(latch.latch(RuntimeMode::Core), Ok(RuntimeMode::Core));
    assert_eq!(latch.get(), Some(RuntimeMode::Core));
    assert_eq!(latch.latch(RuntimeMode::Core), Ok(RuntimeMode::Core));
}

#[test]
fn latch_refuses_the_other_mode() {
    let latch = RuntimeLatch::new();
    assert_eq!(latch.latch(RuntimeMode::Mtld3d), Ok(RuntimeMode::Mtld3d));
    assert_eq!(latch.latch(RuntimeMode::Core), Err(RuntimeMode::Mtld3d));
    assert_eq!(latch.get(), Some(RuntimeMode::Mtld3d));
}

#[test]
fn core_without_the_core_runtime_keeps_mtld3d() {
    assert_eq!(effective_mode(RuntimeMode::Core, true), RuntimeMode::Core);
    assert_eq!(
        effective_mode(RuntimeMode::Core, false),
        RuntimeMode::Mtld3d
    );
    assert_eq!(
        effective_mode(RuntimeMode::Mtld3d, true),
        RuntimeMode::Mtld3d
    );
    assert_eq!(
        effective_mode(RuntimeMode::Mtld3d, false),
        RuntimeMode::Mtld3d
    );
}
