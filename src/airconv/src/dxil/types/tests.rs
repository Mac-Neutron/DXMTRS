use super::*;

fn element(name: &[u8], semantic_indices: Vec<u32>) -> SignatureElement {
    SignatureElement {
        id: 0,
        name: name.to_vec(),
        semantic_indices,
        ty: component_type::F32,
        kind: semantic_kind::ARBITRARY,
        interpolation: 0,
        rows: 2,
        cols: 4,
        start_row: 0,
        start_col: 0,
    }
}

#[test]
fn user_name_is_the_upper_case_semantic_and_its_index() {
    let e = element(b"TexCoord", vec![3]);
    assert_eq!(user_name(&e, 0), b"TEXCOORD3");
    // A row past the indices names itself; bytes past ASCII stay as they are, as C's `toupper`.
    assert_eq!(user_name(&e, 1), b"TEXCOORD1");
    assert_eq!(user_name(&element(b"a\xe9", vec![]), 0), b"A\xe90");
}

#[test]
fn low32_keeps_the_low_bits() {
    assert_eq!(low32(-1), u32::MAX);
    assert_eq!(low32(0x1_0000_0004), 4);
    assert_eq!(low32(-128), 0xffff_ff80);
}
