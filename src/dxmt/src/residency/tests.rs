use super::*;

const VR: ResidencyMask = ResidencyMask::VERTEX_READ;
const FR: ResidencyMask = ResidencyMask::FRAGMENT_READ;
const FW: ResidencyMask = ResidencyMask::FRAGMENT_WRITE;

#[test]
fn residency_dedups_per_encoder() {
    let mut s = ResidencyState::default();
    assert!(s.check(1, VR) == Some(VR));
    assert!(s.check(1, VR).is_none());
    // More stages in the same encoder: everything it asked for so far.
    assert!(s.check(1, FR) == Some(VR | FR));
    assert!(s.check(1, VR | FR).is_none());
    // An older encoder is ignored; a newer one starts over.
    assert!(s.check(0, FW).is_none());
    assert!(s.check(2, FW) == Some(FW));
    assert!(s.check(2, VR) == Some(VR | FW));
    // Encoder 0 on a fresh state counts as the current one.
    assert!(ResidencyState::default().check(0, FR) == Some(FR));
}

#[test]
fn usage_and_stages_as_cpp() {
    use winemetal_sys::wire::{
        WMT_RENDER_STAGE_FRAGMENT, WMT_RENDER_STAGE_MESH, WMT_RENDER_STAGE_OBJECT,
        WMT_RENDER_STAGE_VERTEX, WMT_RESOURCE_USAGE_READ, WMT_RESOURCE_USAGE_WRITE,
    };
    assert_eq!(ResidencyMask::empty().usage(), 0);
    assert_eq!(ResidencyMask::empty().stages(), 0);
    assert_eq!((VR | FR).usage(), WMT_RESOURCE_USAGE_READ);
    assert_eq!(
        (VR | FW).usage(),
        WMT_RESOURCE_USAGE_READ | WMT_RESOURCE_USAGE_WRITE
    );
    assert_eq!(
        (VR | FW).stages(),
        WMT_RENDER_STAGE_VERTEX | WMT_RENDER_STAGE_FRAGMENT
    );
    assert_eq!(
        ResidencyMask::OBJECT_GS_WRITE.stages(),
        WMT_RENDER_STAGE_OBJECT
    );
    assert_eq!(ResidencyMask::MESH_GS_READ.stages(), WMT_RENDER_STAGE_MESH);
    assert_eq!(ResidencyMask::MESH_WRITE.usage(), WMT_RESOURCE_USAGE_WRITE);
    assert_eq!(ResidencyMask::READ.bits(), 0x555);
    assert_eq!(ResidencyMask::WRITE.bits(), 0xAAA);
}
