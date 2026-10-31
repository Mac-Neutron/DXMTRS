use super::*;

/// A profile table as text: each profile's names, then its keys and values.
type Table = Vec<(Vec<String>, Vec<(String, String)>)>;

/// The options as text, `key=value` in key order.
fn shown(options: &Map) -> Vec<String> {
    options
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                String::from_utf8_lossy(key),
                String::from_utf8_lossy(value)
            )
        })
        .collect()
}

#[test]
fn section_matches_exactly() {
    let file =
        b"a = 1\n[game.exe]\nb = 2\n[GAME.EXE]\nc = 3\n[game]\nd = 4\n  [x]game.exe] \ne = 5\n";
    let options = user(Some(file), b"", b"game.exe");
    assert_eq!(shown(&options), ["a=1", "b=2"]);
    let options = user(Some(file), b"", b"x]game.exe");
    assert_eq!(shown(&options), ["a=1", "e=5"]);
    // No `]` after the `[`, or one right after it: an empty name, which only an empty exe name
    // (the native build's) matches.
    let options = user(Some(b"[\nf = 6\n[]\ng = 7\n[abc\nh = 8\n"), b"", b"");
    assert_eq!(shown(&options), ["f=6", "g=7", "h=8"]);
}

#[test]
fn file_context_carries_into_env() {
    let file = b"[game.exe]\r\nvendor = 4321\r\n[other.exe]\r\n";
    let options = user(
        Some(&text_mode(file.to_vec(), true)),
        b"device=5678",
        b"game.exe",
    );
    assert_eq!(shown(&options), ["vendor=4321"]);
    let options = user(
        None,
        b"device=5678;;[other.exe];x=1;[game.exe];y=2",
        b"game.exe",
    );
    assert_eq!(shown(&options), ["device=5678", "y=2"]);
}

#[test]
fn crlf_is_lf_on_pe() {
    let file = b"k = \"a b\r\nq = \"w\r\n\r\rr = 1\r\x1as = 2\n".to_vec();
    assert_eq!(
        text_mode(file.clone(), true),
        b"k = \"a b\nq = \"w\n\r\rr = 1\r".to_vec()
    );
    let options = user(Some(&text_mode(file, true)), b"", b"");
    assert_eq!(shown(&options), ["k=a b", "q=w", "r=1"]);
}

#[test]
fn cr_is_whitespace_natively() {
    let file = b"k = v\r\nq = \"w\r\n\tr\t=\t\"x \"y\r\n".to_vec();
    assert_eq!(text_mode(file.clone(), false), file);
    let options = user(Some(&file), b"", b"");
    assert_eq!(shown(&options), ["k=v", "q=w\r", "r=x y"]);
}

#[test]
fn user_beats_profile() {
    let options = user(None, b"dxgi.customVendorId=beef", b"");
    let genshin = profile("C:\\GenshinImpact.exe").unwrap();
    let options = merged(options, genshin);
    assert_eq!(
        shown(&options),
        [
            "dxgi.customDeviceDesc=AMD Radeon Pro 5300M",
            "dxgi.customDeviceId=7340",
            "dxgi.customVendorId=beef",
        ]
    );
}

#[test]
fn profile_suffix_ignores_case() {
    let omori = profile("C:\\Games\\OMORI.exe").unwrap();
    assert_eq!(omori, [("dxgi.forceSDR", "True")]);
    assert_eq!(profile("Z:\\games\\omori\\omori.EXE"), Some(omori));
    assert_eq!(profile("C:\\Games\\xOMORI.exe"), None);
    assert_eq!(profile("OMORI.exe"), None);
    assert_eq!(profile("C:\\OMORI.exe\\game.exe"), None);
    assert_eq!(profile(""), None);
}

#[test]
fn genshin_alternation() {
    let genshin = profile("C:\\GenshinImpact.exe").unwrap();
    assert_eq!(genshin.len(), 3);
    for path in ["D:\\x\\YuanShen.exe", "D:\\x\\zenlesszonezero.exe"] {
        assert_eq!(profile(path), Some(genshin));
    }
    assert_eq!(profile("D:\\x\\GenshinImpactYuanShen.exe"), None);
}

#[test]
fn hex4_exactly_four() {
    assert_eq!(hex4(b"10de"), Some(0x10de));
    assert_eq!(hex4(b"BEEF"), Some(0xbeef));
    assert_eq!(hex4(b"0000"), Some(0));
    for bad in [
        &b""[..],
        b"10d",
        b"10dee",
        b"0x10",
        b"12g4",
        b" 123",
        b"\xe9123",
    ] {
        assert_eq!(hex4(bad), None);
    }
}

#[test]
fn bool_ignores_case() {
    assert_eq!(parse_bool(b"True"), Some(true));
    assert_eq!(parse_bool(b"tRUE"), Some(true));
    assert_eq!(parse_bool(b"FALSE"), Some(false));
    for bad in [&b""[..], b"1", b"yes", b"true "] {
        assert_eq!(parse_bool(bad), None);
    }
}

#[test]
fn profile_lines_in_key_order() {
    let genshin = profile("C:\\GenshinImpact.exe").unwrap();
    assert_eq!(
        profile_lines(genshin),
        [
            "Found built-in config:",
            "  dxgi.customDeviceDesc = AMD Radeon Pro 5300M",
            "  dxgi.customDeviceId = 7340",
            "  dxgi.customVendorId = 1002",
        ]
    );
}

/// The profile table is `config.cpp`'s: its six patterns' names, then their keys and values.
#[test]
fn table_matches_config_cpp() {
    let cpp = include_str!("../../../../src/util/config/config.cpp");
    let table = cpp
        .split_once("g_profiles = {{")
        .unwrap()
        .1
        .split_once("\n  }};")
        .unwrap()
        .0;
    let mut profiles: Table = Vec::new();
    for line in table.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("{ R\"(") {
            let pattern = rest.split_once(")\"").unwrap().0;
            let names = pattern
                .strip_prefix(r"\\")
                .and_then(|names| names.strip_suffix(r"\.exe$"))
                .unwrap();
            let names = names
                .strip_prefix('(')
                .and_then(|names| names.strip_suffix(')'))
                .unwrap_or(names);
            let names: Vec<String> = names.split('|').map(str::to_owned).collect();
            let plain = |name: &String| {
                name.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            };
            assert!(names.iter().all(plain), "{pattern}");
            profiles.push((names, Vec::new()));
        } else if let Some(rest) = line.strip_prefix("{ \"") {
            let (key, rest) = rest.split_once('"').unwrap();
            let value = rest.split_once('"').unwrap().1.split_once('"').unwrap().0;
            let options = &mut profiles.last_mut().unwrap().1;
            options.push((key.to_owned(), value.to_owned()));
        }
    }
    let ours: Table = PROFILES
        .iter()
        .map(|(names, options)| {
            (
                names.iter().map(|&name| name.to_owned()).collect(),
                options
                    .iter()
                    .map(|&(key, value)| (key.to_owned(), value.to_owned()))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(profiles.len(), 6);
    assert_eq!(profiles, ours);
}

/// The four options, from their keys; an empty description is none.
#[test]
fn options_read_the_four_keys() {
    let config = Config {
        options: user(
            None,
            b"dxgi.customVendorId=beef;dxgi.customDeviceId=12345;dxgi.forceSDR=TRUE",
            b"",
        ),
    };
    let options = Options::from_config(&config);
    assert_eq!(
        (options.vendor_id, options.device_id, options.force_sdr),
        (Some(0xbeef), None, true)
    );
    assert_eq!(options.device_desc, None);
    let config = Config {
        options: user(None, b"dxgi.customDeviceDesc=\"a b\"", b""),
    };
    let options = Options::from_config(&config);
    assert_eq!(options.device_desc.as_deref(), Some(&b"a b"[..]));
    assert!(!options.force_sdr);
}

/// PE's `getEnvVar`: a character beyond the BMP is dropped, and four NULs end the text.
///
/// The last character is kept; stray surrogates are `?`.
#[test]
fn wide_text_drops_a_char_beyond_the_bmp() {
    let wide = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
    assert_eq!(from_wide(&wide("a\u{e9}\u{1d11e}Z")), b"a\xc3\xa9Z\0\0\0\0");
    assert_eq!(from_wide(&wide("a\u{1d11e}")), "a\u{1d11e}".as_bytes());
    assert_eq!(from_wide(&wide("\u{1d11e}\u{1d11e}")), b"\0\0\0\0\0\0\0\0");
    assert_eq!(from_wide(&[0xdc00, 0x41, 0xd800]), b"?A?");
    assert_eq!(from_wide(&[0x41, 0, 0x42]), b"A");
    // A high surrogate takes whatever follows as its pair: U+10041, the last character.
    assert_eq!(from_wide(&[0xd800, 0x41]), "\u{10041}".as_bytes());
}
