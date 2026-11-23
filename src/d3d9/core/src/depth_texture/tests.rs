use super::{PackedDepth, PlaneLayout};

#[test]
fn every_d16_code_survives_float_storage() {
    for code in 0..=u16::MAX {
        let bytes = code.to_le_bytes();
        let (depth, stencil) = PackedDepth::D16.decode(&bytes);
        let mut packed = [0; 2];
        PackedDepth::D16.encode(depth, stencil, &mut packed);
        assert_eq!(packed, bytes);
    }
}

#[test]
fn every_d24_code_and_stencil_value_survives_float_storage() {
    for code in 0..=0xff_ffffu32 {
        let bytes = ((code << 8) | (code & 255)).to_le_bytes();
        let (depth, stencil) = PackedDepth::D24S8.decode(&bytes);
        let mut packed = [0; 4];
        PackedDepth::D24S8.encode(depth, stencil, &mut packed);
        assert_eq!(packed, bytes, "code {code}");
    }
}

#[test]
fn padded_plane_rectangles_do_not_touch_borders() {
    let layout = PlaneLayout::new(3, 2, 256).unwrap();
    let packed: Vec<u8> = (0..32).collect();
    let mut depth = vec![0xcc; 512];
    let mut stencil = vec![0xcc; 512];
    assert!(layout.unpack(
        &PackedDepth::D24S8,
        &packed[4..],
        16,
        &mut depth,
        &mut stencil
    ));
    assert_eq!(&stencil[..3], &[4, 8, 12]);
    assert_eq!(&stencil[256..259], &[20, 24, 28]);
    assert!(stencil[3..256].iter().all(|&v| v == 0xcc));
    let mut output = [0xcc; 32];
    assert!(layout.pack(&PackedDepth::D24S8, &depth, &stencil, &mut output[4..], 16));
    assert_eq!(&output[4..16], &packed[4..16]);
    assert_eq!(&output[20..32], &packed[20..32]);
    assert_eq!(&output[..4], &[0xcc; 4]);
    assert_eq!(&output[16..20], &[0xcc; 4]);
}

#[test]
fn short_stencil_plane_rejects_before_writing_depth() {
    let layout = PlaneLayout::new(1, 1, 256).unwrap();
    let mut depth = vec![0xcc; 256];
    assert!(!layout.unpack(&PackedDepth::D24S8, &[0; 4], 4, &mut depth, &mut []));
    assert!(depth.iter().all(|&v| v == 0xcc));
}
