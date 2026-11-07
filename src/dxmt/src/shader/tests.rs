use winemetal_sys::sm50::MtlShaderReflection;

use super::initialize;

#[test]
fn refuses_a_dxso_token_stream() {
    // SAFETY: the reflection is plain integers, valid all zero.
    let mut reflection: MtlShaderReflection = unsafe { core::mem::zeroed() };
    // vs_2_0 then end: the refusal comes before `SM50Initialize`, which this test binary could
    // not reach (a call would read the token stream as a container or fail to link).
    let tokens: Vec<u8> = [0xfffe_0200_u32, 0x0000_ffff]
        .iter()
        .flat_map(|t| t.to_le_bytes())
        .collect();
    assert!(matches!(
        initialize(&tokens, &mut reflection),
        Err(super::Sm50Error(0))
    ));
    assert!(matches!(
        initialize(b"DXB", &mut reflection),
        Err(super::Sm50Error(0))
    ));
    let Err(error) = initialize(b"DXB", &mut reflection) else {
        panic!("initialized");
    };
    assert_eq!(error.message(), "not a DXBC container");
}
