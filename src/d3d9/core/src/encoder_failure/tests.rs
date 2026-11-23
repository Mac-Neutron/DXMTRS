use super::*;

#[test]
fn native_failure_is_deferred_until_explicit_observation() {
    let failure = AtomicI32::new(D3D_OK);
    let native = AtomicU32::new(0);
    assert_eq!(status(&failure, &native), Ok(()));
    native.store(1, Ordering::Release);
    assert_eq!(known_status(&failure), Ok(()));
    assert_eq!(failure.load(Ordering::Acquire), D3D_OK);
    assert_eq!(status(&failure, &native), Err(D3DERR_DEVICELOST));
    assert_eq!(known_status(&failure), Err(D3DERR_DEVICELOST));
}

#[test]
fn full_observation_and_later_errors_preserve_the_first_hresult() {
    for first in [E_OUTOFMEMORY, D3DERR_DEVICELOST] {
        let failure = AtomicI32::new(D3D_OK);
        let native = AtomicU32::new(1);
        assert_eq!(record_failure(&failure, first, "test"), first);
        assert_eq!(status(&failure, &native), Err(first));
        assert_eq!(record_failure(&failure, E_OUTOFMEMORY, "test"), first);
        assert_eq!(record_failure(&failure, D3DERR_DEVICELOST, "test"), first);
        assert_eq!(known_status(&failure), Err(first));
    }
}

#[test]
fn nonallocation_failures_normalize_to_device_lost() {
    for status in [D3D_OK, mtld3d_types::D3DERR_INVALIDCALL] {
        let failure = AtomicI32::new(D3D_OK);
        assert_eq!(record_failure(&failure, status, "test"), D3DERR_DEVICELOST);
        assert_eq!(known_status(&failure), Err(D3DERR_DEVICELOST));
    }
}
