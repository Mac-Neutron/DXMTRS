use super::*;

const KEY: &str = "c298383ed535cde593fbce84c046f0615952bb82125d1365336db9c689095f32";

#[test]
fn parses_64_hex_digits() {
    let k = parse_key(KEY).unwrap();
    assert_eq!(k[0], 0xc2);
    assert_eq!(k[1], 0x98);
    assert_eq!(k[31], 0x32);
    assert_eq!(parse_key(&KEY.to_uppercase()), Some(k));
}

#[test]
fn empty_is_none() {
    assert_eq!(parse_key(""), None);
}

#[test]
fn short_is_none() {
    assert_eq!(parse_key(&KEY[..62]), None);
    assert_eq!(parse_key(&format!("{KEY}0")), None);
}

#[test]
fn non_hex_is_none() {
    assert_eq!(parse_key(&format!("g{}", &KEY[1..])), None);
}
