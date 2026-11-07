use airconv::abi::{SM50_SHADER_COMMON, SM50_SHADER_D3D9};

use super::chain_is_walkable;

/// A reader over fake `(address, next, type)` nodes.
fn reader(nodes: &[(u32, u32, u32)]) -> impl Fn(u32) -> (u32, u32) + '_ {
    |at| {
        nodes
            .iter()
            .find(|n| n.0 == at)
            .map_or((0, 0), |n| (n.1, n.2))
    }
}

#[test]
fn d3d9_argument_is_refused_from_wow64() {
    // COMMON (1) -> unknown 77 (2) -> D3D9 (3), with fake addresses.
    let with_d3d9 = [
        (1, 2, SM50_SHADER_COMMON),
        (2, 3, 77),
        (3, 0, SM50_SHADER_D3D9),
    ];
    assert!(!chain_is_walkable(1, reader(&with_d3d9)));
    // The same chain without it, and the empty chain, walk.
    assert!(chain_is_walkable(
        1,
        reader(
            &with_d3d9[..2]
                .iter()
                .map(|n| (n.0, if n.0 == 2 { 0 } else { n.1 }, n.2))
                .collect::<Vec<_>>()
        )
    ));
    assert!(chain_is_walkable(0, reader(&with_d3d9)));
}

#[test]
fn a_cyclic_chain_is_refused() {
    let cycle = [(1, 2, SM50_SHADER_COMMON), (2, 1, 77)];
    assert!(!chain_is_walkable(1, reader(&cycle)));
}
