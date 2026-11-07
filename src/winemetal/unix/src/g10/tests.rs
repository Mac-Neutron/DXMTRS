use airconv::abi::{SM50_SHADER_COMMON, SM50_SHADER_D3D9};

use super::chain_has_type;

#[test]
fn d3d9_argument_is_refused_from_wow64() {
    // A 32-bit chain: COMMON (1) -> unknown 77 (2) -> D3D9 (3), with fake addresses.
    let nodes = [
        (1_u32, 2_u32, SM50_SHADER_COMMON),
        (2, 3, 77),
        (3, 0, SM50_SHADER_D3D9),
    ];
    let header = |at: u32| {
        nodes
            .iter()
            .find(|n| n.0 == at)
            .map_or((0, 0), |n| (n.1, n.2))
    };
    assert!(chain_has_type(1, SM50_SHADER_D3D9, header));
    assert!(!chain_has_type(1, 99, header));
    assert!(!chain_has_type(0, SM50_SHADER_D3D9, header));
}
