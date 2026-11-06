use super::*;

#[test]
fn known_iids_print_as_the_sdk_does() {
    assert_eq!(
        IID_ID3D11_DEVICE.to_string(),
        "db6f6ddb-ac77-4e88-8253-819df9bbf140"
    );
    assert_eq!(
        IID_IDXGI_FACTORY1.to_string(),
        "770aae78-f26f-4dba-a829-253c83d1b387"
    );
    assert_eq!(
        IID_ID3D12_DEVICE.to_string(),
        "189819f1-1db6-4b57-be54-1821339b85f7"
    );
    assert_eq!(
        IID_IDXGI_OBJECT.to_string(),
        "aec22fb8-76f3-4639-9be0-28eb43a67a2e"
    );
}

#[test]
fn every_spelling_of_an_iid_is_the_same_value() {
    assert!(IID_ID3D11_DEVICE1 == IID_ID3D11_DEVICE_1);
    assert!(IID_IDXGI_SWAP_CHAIN1 == IID_IDXGI_SWAP_CHAIN_1);
    assert!(IID_IDXGI_DEVICE1 == IID_IDXGI_DEVICE_1);
    assert!(IID_ID3D11_DEVICE != IID_ID3D11_DEVICE_1);
}

/// A copy-paste slip would alias two interfaces: every canonical IID is distinct.
#[test]
fn canonical_iids_are_distinct() {
    let mut seen = Vec::new();
    for guid in ALL {
        assert!(!seen.contains(guid), "duplicate IID {guid}");
        seen.push(*guid);
    }
}
