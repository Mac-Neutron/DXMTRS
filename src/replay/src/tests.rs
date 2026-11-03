use super::{
    HEADER_SIZE, Kind, MAGIC, NONE_DIGEST, Recording, Sha1Digest, checksum, parse_compute,
    parse_graphics, parse_recording, recording_stem,
};

fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn push_record(stream: &mut Vec<u8>, kind: u32, id: Sha1Digest, payload: &[u8]) {
    push_u32(stream, kind);
    push_u32(stream, u32::try_from(payload.len()).unwrap());
    stream.extend_from_slice(&checksum(payload).to_le_bytes());
    stream.extend_from_slice(&id);
    stream.extend_from_slice(payload);
}

#[test]
fn stem_extracts_exe_name_and_rejects_non_recordings() {
    assert_eq!(recording_stem("game.exe.pipelines"), Some("game.exe"));
    assert_eq!(
        recording_stem("Z:\\tmp\\work\\MonsterHunterWilds.exe.pipelines"),
        Some("MonsterHunterWilds.exe")
    );
    assert_eq!(
        recording_stem("/tmp/work/EldenRing.exe.pipelines"),
        Some("EldenRing.exe")
    );
    assert_eq!(recording_stem(".pipelines"), None);
    assert_eq!(recording_stem("dir/.pipelines"), None);
    assert_eq!(recording_stem("game.exe.dxmt-cache"), None);
}

#[test]
fn parse_compute_roundtrip_and_length_check() {
    let root = [0x11u8; 20];
    let cs = [0x22u8; 20];
    let mut payload = Vec::new();
    payload.extend_from_slice(&root);
    payload.extend_from_slice(&cs);
    push_u32(&mut payload, 7);
    push_u32(&mut payload, 3);

    let parsed = parse_compute(&payload).expect("valid compute payload");
    assert_eq!(parsed.root, root);
    assert_eq!(parsed.cs, cs);
    assert_eq!(parsed.desc.node_mask, 7);
    assert_eq!(parsed.desc.flags, 3);

    payload.push(0);
    assert!(parse_compute(&payload).is_none());
}

#[test]
fn parse_graphics_roundtrip_with_input_elements() {
    let root = [0x33u8; 20];
    let vs = [0x44u8; 20];
    let ps = [0x55u8; 20];
    let gs = NONE_DIGEST;
    let mut payload = Vec::new();
    payload.extend_from_slice(&root);
    payload.extend_from_slice(&vs);
    payload.extend_from_slice(&ps);
    payload.extend_from_slice(&gs);
    // Blend state: alpha_to_coverage, independent_blend, 8 render targets.
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 0);
    for i in 0..8u32 {
        for _ in 0..9 {
            push_u32(&mut payload, i);
        }
        payload.push(0x0f);
    }
    // Sample mask.
    push_u32(&mut payload, 0xffff_ffff);
    // Rasterizer: 4 u32, 2 f32, 5 u32.
    push_u32(&mut payload, 3);
    push_u32(&mut payload, 2);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, (-4i32).cast_unsigned());
    push_u32(&mut payload, 0.5f32.to_bits());
    push_u32(&mut payload, 1.5f32.to_bits());
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 0);
    // Depth-stencil: 4 u32, 2 u8, 2 * 4 u32.
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 4);
    push_u32(&mut payload, 0);
    payload.push(0xff);
    payload.push(0xff);
    for _ in 0..8 {
        push_u32(&mut payload, 1);
    }
    // 1 input element ("POSITION").
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 8);
    payload.extend_from_slice(b"POSITION");
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 6);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 12);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 0);
    // Strip cut, topology, num_rt, 8 rtv, dsv, sample_desc (2), node_mask, flags.
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 3);
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 28);
    for _ in 1..8 {
        push_u32(&mut payload, 0);
    }
    push_u32(&mut payload, 45);
    push_u32(&mut payload, 1);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 0);

    let parsed = parse_graphics(&payload).expect("valid graphics payload");
    assert_eq!(parsed.root, root);
    assert_eq!(parsed.vs, vs);
    assert_eq!(parsed.ps, ps);
    assert_eq!(parsed.gs, gs);
    assert_eq!(parsed.desc.rasterizer_state.depth_bias, -4);
    assert_eq!(parsed.elements.len(), 1);
    assert_eq!(parsed.names[0], b"POSITION\0");
    assert_eq!(parsed.elements[0].format, 6);
    assert_eq!(parsed.elements[0].aligned_byte_offset, 12);
    assert_eq!(parsed.desc.rtv_formats[0], 28);
    assert_eq!(parsed.desc.dsv_format, 45);
}

#[test]
fn parse_recording_handles_valid_and_corrupt_records() {
    assert_eq!(HEADER_SIZE, 36);
    let mut rec = Recording::default();
    assert!(!parse_recording(b"NOTMAGIC", &mut rec));

    let mut stream = Vec::from(MAGIC);
    let blob_id = [0xaa; 20];
    push_record(&mut stream, 1, blob_id, b"dxbc-blob");

    let mut compute_payload = Vec::new();
    compute_payload.extend_from_slice(&blob_id);
    compute_payload.extend_from_slice(&blob_id);
    push_u32(&mut compute_payload, 0);
    push_u32(&mut compute_payload, 0);
    push_record(&mut stream, 3, [0xbb; 20], &compute_payload);

    // Record with bad checksum (counted in `bad`, stream continues).
    let bad_at = stream.len();
    push_record(&mut stream, 1, [0xcc; 20], b"corrupt");
    stream[bad_at + 8] ^= 0xff;

    // Truncated trailing header (counted in `bad`, stops loop).
    stream.extend_from_slice(&[1, 2, 3]);

    assert!(parse_recording(&stream, &mut rec));
    assert_eq!(rec.blobs.len(), 1);
    assert_eq!(rec.blobs[&blob_id], b"dxbc-blob");
    assert_eq!(rec.pipelines.len(), 1);
    assert_eq!(rec.pipelines[0].0, Kind::Compute);
    assert_eq!(rec.bad, 2);
}
