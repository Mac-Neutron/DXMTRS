use core::mem::offset_of;

use super::*;

/// Each IID as its header writes it.
///
/// `include/native`'s `DEFINE_GUID`s, `src/dxgi/dxgi_interfaces.h`'s private interfaces and the
/// NVIDIA extension, Wine's adapter IID in `src/dxgi/dxgi_adapter.cpp`.
const HEADERS: [(Guid, &str); 31] = [
    (IID_IUNKNOWN, "00000000-0000-0000-c000-000000000046"),
    (IID_IDXGI_OBJECT, "aec22fb8-76f3-4639-9be0-28eb43a67a2e"),
    (IID_IDXGI_FACTORY, "7b7166ec-21c7-44ae-b21a-c9ae321ae369"),
    (IID_IDXGI_FACTORY1, "770aae78-f26f-4dba-a829-253c83d1b387"),
    (IID_IDXGI_FACTORY2, "50c83a1c-e072-4c48-87b0-3630fa36a6d0"),
    (IID_IDXGI_FACTORY3, "25483823-cd46-4c7d-86ca-47aa95b837bd"),
    (IID_IDXGI_FACTORY4, "1bc6ea02-ef36-464f-bf0c-21ca39e5168a"),
    (IID_IDXGI_FACTORY5, "7632e1f5-ee65-4dca-87fd-84cd75f8838d"),
    (IID_IDXGI_FACTORY6, "c1b6694f-ff09-44a9-b03c-77900a0a1d17"),
    (IID_IDXGI_FACTORY7, "a4966eed-76db-44da-84c1-ee9a7afb20a8"),
    (IID_IDXGI_ADAPTER, "2411e7e1-12ac-4ccf-bd14-9798e8534dc0"),
    (IID_IDXGI_ADAPTER1, "29038f61-3839-4626-91fd-086879011a05"),
    (IID_IDXGI_ADAPTER2, "0aa1ae0a-fa0e-4b84-8644-e05ff8e5acb5"),
    (IID_IDXGI_ADAPTER3, "645967a4-1392-4310-a798-8053ce3e93fd"),
    (IID_IDXGI_ADAPTER4, "3c8d99d1-4fbf-4181-a82c-af66bf7bd24e"),
    (IID_IDXGI_OUTPUT, "ae02eedb-c735-4690-8d52-5a8dc20213aa"),
    (IID_IDXGI_OUTPUT1, "00cddea8-939b-4b83-a340-a685226666cc"),
    (IID_IDXGI_OUTPUT2, "595e39d1-2724-4663-99b1-da969de28364"),
    (IID_IDXGI_OUTPUT3, "8a6bb301-7e7e-41f4-a8e0-5b32f7f99b18"),
    (IID_IDXGI_OUTPUT4, "dc7dca35-2196-414d-9f53-617884032a60"),
    (IID_IDXGI_OUTPUT5, "80a07424-ab52-42eb-833c-0c42fd282d98"),
    (IID_IDXGI_OUTPUT6, "068346e8-aaec-4b84-add7-137f513f77a1"),
    (IID_IDXGI_DEVICE, "54ec77fa-1377-44e6-8c32-88fd5f44c84c"),
    (IID_ID3D10_DEVICE, "9b7e4c0f-342c-4106-a19f-4f2704f689f0"),
    (IID_ID3D10_DEVICE1, "9b7e4c8f-342c-4106-a19f-4f2704f689f0"),
    (
        IID_IMTL_DXGI_ADAPTER,
        "acdf3ef1-b33a-4cb6-97bd-1c1974827e6d",
    ),
    (IID_IMTL_DXGI_OUTPUT, "4c1128cd-b437-4d44-9958-5d765bb85a76"),
    (IID_IMTL_DXGI_DEVICE, "6bfa1657-9cb1-471a-a4fb-7cacf8a81207"),
    (
        IID_IMTL_SWAP_CHAIN_FACTORY,
        "6c030003-460a-4b19-909b-38f97d203e45",
    ),
    (
        IID_WINE_DXGI_ADAPTER,
        "17399d75-964e-4c03-99f8-9d4fd196dd62",
    ),
    (DXMT_NVEXT_GUID, "ba0af616-4a43-4259-815c-db3b89829905"),
];

/// `n` letters, `A` to `Z` and round again (the probe's `Repeat`).
fn alphabet(n: usize) -> Vec<u8> {
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"
        .iter()
        .cycle()
        .take(n)
        .copied()
        .collect()
}

/// The probe's `configu` value: 125 letters, U+00E9, U+1D11E, `Z`.
fn configu() -> Vec<u8> {
    let mut text = alphabet(125);
    text.extend_from_slice(b"\xc3\xa9\xf0\x9d\x84\x9eZ");
    text
}

#[test]
fn iids_are_the_headers() {
    for (iid, text) in HEADERS {
        assert_eq!(iid.to_string(), text);
    }
}

#[test]
fn private_slots_follow_the_public_ones() {
    let slot = size_of::<usize>();
    assert_eq!(offset_of!(AdapterVtbl, get_mtl_device), 19 * slot);
    assert_eq!(offset_of!(AdapterVtbl, get_local_d3dkmt), 20 * slot);
    assert_eq!(size_of::<AdapterVtbl>(), 21 * slot);
    assert_eq!(offset_of!(OutputVtbl, get_gamma_ramp), 29 * slot);
    assert_eq!(size_of::<OutputVtbl>(), 30 * slot);
    assert_eq!(size_of::<FactoryVtbl>(), 30 * slot);
    assert_eq!(
        offset_of!(SwapChainFactoryVtbl, create_swap_chain),
        3 * slot
    );
}

#[test]
fn transcode_fits() {
    let mut wide = [0_u16; 8];
    assert_eq!(transcode(b"abc", &mut wide), 3);
    assert_eq!(wide, [97, 98, 99, 0, 0, 0, 0, 0]);
    let mut native: [Wchar; 4] = [7; 4];
    assert_eq!(transcode_into(b"ab", &mut native), 2);
    assert_eq!(native, [97, 98, 7, 7]);
}

#[test]
fn transcode_stops_at_the_last_unit() {
    let mut wide = [0xffff_u16; 128];
    assert_eq!(transcode(&alphabet(200), &mut wide[..127]), 127);
    assert_eq!(wide[126], u16::from(b'W'));
    assert_eq!(wide[127], 0xffff);
}

#[test]
fn transcode_stops_at_a_nul() {
    let mut wide = [0xffff_u16; 4];
    assert_eq!(transcode(b"a\0b", &mut wide), 2);
    assert_eq!(wide, [97, 0, 0xffff, 0xffff]);
}

#[test]
fn transcode_drops_a_pair_that_does_not_fit() {
    // UTF-16: the clef needs units 126 and 127 of 127, so it is dropped and `Z` fills unit 126.
    let mut wide = [0_u16; 127];
    assert_eq!(transcode(&configu(), &mut wide), 127);
    assert_eq!(
        (wide[124], wide[125], wide[126]),
        (u16::from(b'U'), 0xe9, u16::from(b'Z'))
    );
    // UTF-32: the clef fits in unit 126 and `Z` is dropped.
    let mut native = [0_u32; 127];
    assert_eq!(transcode(&configu(), &mut native), 127);
    assert_eq!((native[125], native[126]), (0xe9, 0x1_d11e));
}

#[test]
fn transcode_bad_utf8_is_a_question_mark() {
    // The probe's `confbad`: a stray continuation byte, then an invalid lead byte.
    let mut wide = [0_u16; 8];
    assert_eq!(transcode(b"ab\x80cd\xff", &mut wide), 6);
    assert_eq!(wide, [97, 98, 63, 99, 100, 63, 0, 0]);
    // A lead byte whose character runs past the end is a `?` that ends the text.
    let mut cut = [0_u16; 4];
    assert_eq!(transcode(b"a\xe9Z", &mut cut), 2);
    assert_eq!(cut, [97, 63, 0, 0]);
    // A five-byte lead is a `?` that takes five bytes.
    let mut long = [0_u16; 4];
    assert_eq!(transcode(b"\xf8\x80\x80\x80\x80b", &mut long), 2);
    assert_eq!(long, [63, 98, 0, 0]);
}
