use super::{IID_ID3D10_DEVICE, IID_ID3D10_MULTITHREAD};

#[test]
fn iids_are_the_sdks() {
    assert_eq!(
        IID_ID3D10_DEVICE.to_string(),
        "9b7e4c0f-342c-4106-a19f-4f2704f689f0"
    );
    assert_eq!(
        IID_ID3D10_MULTITHREAD.to_string(),
        "9b7e4e00-342c-4106-a19f-4f2704f689f0"
    );
}
