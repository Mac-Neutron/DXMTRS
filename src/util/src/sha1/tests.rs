use super::*;

#[test]
fn fips_vectors() {
    assert_eq!(
        Sha1::compute(b"").hex(),
        "da39a3ee5e6b4b0d3255bfef95601890afd80709"
    );

    let abc = Sha1::compute(b"abc");
    let copied = abc;
    assert_eq!(copied, abc);
    assert_eq!(abc.hex(), "a9993e364706816aba3e25717850c26c9cd0d89d");
    assert_eq!(
        *abc.as_bytes(),
        [
            0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50,
            0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d,
        ]
    );
    assert_eq!(abc.to_string(), abc.hex());

    let two_blocks = Sha1::compute(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq");
    assert_eq!(two_blocks.hex(), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
}

#[test]
fn million_a() {
    let expected = "34aa973cd4c4daa4f61eeb2bdbad27316534016f";

    let mut thousand_chunks = Sha1::new();
    let chunk = [b'a'; 1000];
    for _ in 0..1000 {
        thousand_chunks.update(&chunk);
    }
    let digest = thousand_chunks.finalize();
    assert_eq!(digest.hex(), expected);

    let mut odd_chunks = Sha1::new();
    let mut remaining = 1_000_000_usize;
    let odd = [b'a'; 997];
    while remaining > 0 {
        let take = remaining.min(odd.len());
        odd_chunks.update(&odd[..take]);
        remaining -= take;
    }
    assert_eq!(odd_chunks.finalize(), digest);
}

#[test]
fn streams_like_the_c() {
    let message: Vec<u8> = (0..512_u32)
        .map(|i| u8::try_from(i.wrapping_mul(37) & 0xff).unwrap_or(0))
        .collect();

    for len in [
        0, 1, 3, 54, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 257, 512,
    ] {
        let slice = &message[..len];
        let whole = Sha1::compute(slice);
        assert_ne!(whole, Sha1Digest::default());

        for step in [1, 2, 3, 7, 16, 31, 63, 64, 65, 129] {
            let mut streamed = Sha1::new();
            streamed.update(&[]);
            for chunk in slice.chunks(step) {
                streamed.update(chunk);
            }
            streamed.update(&[]);
            assert_eq!(
                streamed.finalize(),
                whole,
                "streaming len={len} in step={step} chunks must match single-buffer compute"
            );
        }
    }
}
