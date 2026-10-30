use super::{name_of, not_implemented, table};
use crate::nv::{HDR_CAPABILITIES_VER3, HdrCapabilitiesV3, version};

#[test]
fn a_known_id_has_its_name() {
    assert_eq!(name_of(0x0150_e828), Some("NvAPI_Initialize"));
    assert_eq!(name_of(0xe364_0a56), Some("NvAPI_GPU_GetThermalSettings"));
}

#[test]
fn an_unknown_id_has_none() {
    assert_eq!(name_of(0x1234_5678), None);
}

#[test]
fn the_table_holds_511_distinct_ids() {
    let mut ids: Vec<u32> = table().map(|(_, id)| id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 511);
}

#[test]
fn a_version_is_the_size_and_the_number() {
    // NV_HDR_CAPABILITIES_VER3 under llvm-mingw: 64 | 3 << 16.
    assert_eq!(version(size_of::<HdrCapabilitiesV3>(), 3), 196_672);
    assert_eq!(HDR_CAPABILITIES_VER3, 196_672);
}

#[test]
fn an_unprovided_function_is_logged_by_name_or_id() {
    assert_eq!(not_implemented(0xe364_0a56), "nvapi: function NvAPI_GPU_GetThermalSettings not implemented");
    assert_eq!(not_implemented(0x1234_5678), "nvapi: function id 305419896 not implemented");
}
