use core::{
    mem::{offset_of, size_of},
    ptr::null_mut,
};
use std::cell::Cell;

use super::*;

/// The C++'s answers for every DXGI format: `golden/format.cpp` over `dxmt_format.cpp`.
const DXGI: &str = include_str!("golden/dxgi.txt");

/// The C++ inspector's table for each family set.
const CAPS: &str = include_str!("golden/caps.txt");

/// The C++ helpers over every pixel format, and `SanitizeRTVClearColor` over a grid of colours.
const HELPERS: &str = include_str!("golden/helpers.txt");

thread_local! {
    /// The families (bit 0 Apple7, 1 Apple8, 2 Apple9, 3 Mac2) the fake device supports.
    static FAMILIES: Cell<u32> = const { Cell::new(0) };
    /// Whether the fake device supports BC textures.
    static BC: Cell<bool> = const { Cell::new(false) };
}

#[unsafe(export_name = "MTLDevice_supportsFamily")]
extern "C-unwind" fn fake_supports_family(_device: u64, family: u32) -> bool {
    let bit = match family {
        WMTGPU_FAMILY_APPLE7 => 1,
        WMTGPU_FAMILY_APPLE8 => 2,
        WMTGPU_FAMILY_APPLE9 => 4,
        WMTGPU_FAMILY_MAC2 => 8,
        _ => 0,
    };
    FAMILIES.get() & bit != 0
}

#[unsafe(export_name = "MTLDevice_supportsBCTextureCompression")]
extern "C-unwind" fn fake_supports_bc(_device: u64) -> bool {
    BC.get()
}

/// The number in a golden `0x%08x(Name)` or `0x%08x` token.
fn hex(token: &str) -> u32 {
    let digits = token
        .split('(')
        .next()
        .expect("a token")
        .trim_start_matches("0x");
    u32::from_str_radix(digits, 16).expect("a hex number")
}

/// The families of a golden section's name.
fn families(name: &str) -> &'static [u32] {
    match name {
        "none" => &[],
        "Apple7" => &[WMTGPU_FAMILY_APPLE7],
        "Apple7+8" => &[WMTGPU_FAMILY_APPLE7, WMTGPU_FAMILY_APPLE8],
        "Apple7+8+9" => &[
            WMTGPU_FAMILY_APPLE7,
            WMTGPU_FAMILY_APPLE8,
            WMTGPU_FAMILY_APPLE9,
        ],
        "Mac2" => &[WMTGPU_FAMILY_MAC2],
        "Apple7+Mac2" => &[WMTGPU_FAMILY_APPLE7, WMTGPU_FAMILY_MAC2],
        other => panic!("an unknown family set {other}"),
    }
}

/// The families as `FAMILIES` bits.
fn mask(set: &[u32]) -> u32 {
    set.iter().fold(0, |bits, &family| {
        bits | match family {
            WMTGPU_FAMILY_APPLE7 => 1,
            WMTGPU_FAMILY_APPLE8 => 2,
            WMTGPU_FAMILY_APPLE9 => 4,
            _ => 8,
        }
    })
}

#[test]
fn dxgi_table_is_the_golden() {
    let mut rows = 0;
    for line in DXGI.lines().filter(|line| !line.starts_with('#')) {
        let field: Vec<&str> = line.split_whitespace().collect();
        let format: u32 = field[0].parse().expect("a format");
        let (result, desc) = query_dxgi(format);
        let got = format!(
            "{format} {result} 0x{:08x} {} {} {} {}",
            desc.pixel_format,
            desc.attribute_format,
            desc.bytes_per_texel,
            desc.planar_count,
            desc.flag
        );
        let want = format!(
            "{} {} 0x{:08x} {} {} {} {}",
            field[0],
            field[1],
            hex(field[2]),
            field[3],
            field[4],
            field[5],
            field[6]
        );
        assert_eq!(got, want);
        rows += 1;
    }
    // 0..=192 and the three ids past the table.
    assert_eq!(rows, 196);
}

#[test]
fn caps_per_family_are_the_golden() {
    let mut sections = 0;
    let mut lines = CAPS.lines().peekable();
    while let Some(header) = lines.next() {
        let word: Vec<&str> = header.split_whitespace().collect();
        let set = families(word[1].strip_prefix("families=").expect("a families word"));
        let bc = word[2] == "bc=1";
        let mut want = Vec::new();
        while let Some(entry) = lines.next_if(|line| !line.starts_with("==")) {
            let field: Vec<&str> = entry.split_whitespace().collect();
            want.push((hex(field[0]), hex(field[1])));
        }
        let got = capabilities(|family| set.contains(&family), bc);
        let throws = header.contains("throws=\"Invalid MTLDevice\"");
        assert_eq!(got.is_none(), throws, "{header}");
        assert_eq!(
            word[3],
            format!("entries={}", got.as_ref().map_or(0, Vec::len)),
            "{header}"
        );
        assert_eq!(got.unwrap_or_default(), want, "{header}");
        sections += 1;
    }
    assert_eq!(sections, 12);
}

#[test]
fn helpers_are_the_golden() {
    let inputs: Vec<f64> = HELPERS
        .lines()
        .filter_map(|line| line.strip_prefix("#   ins["))
        .map(|line| {
            let bits = line.split("=0x").nth(1).expect("an input");
            f64::from_bits(u64::from_str_radix(bits, 16).expect("hex"))
        })
        .collect();
    assert_eq!(inputs.len(), 14);
    let (mut plain, mut swizzled, mut clamped) = (0, 0, 0);
    for line in HELPERS.lines().filter(|line| !line.starts_with('#')) {
        if let Some(rest) = line.strip_prefix("san ") {
            let field: Vec<&str> = rest.split_whitespace().collect();
            let format = hex(field[0]);
            let first: usize = field[1].parse().expect("an input index");
            let channel = |offset: usize| inputs[(first + offset) % inputs.len()];
            let mut color = WMTClearColor {
                r: channel(0),
                g: channel(1),
                b: channel(2),
                a: channel(3),
            };
            sanitize_clear_color(format, &mut color);
            let got = format!(
                "san 0x{format:08x} {first} 0x{:016x} 0x{:016x} 0x{:016x} 0x{:016x}",
                color.r.to_bits(),
                color.g.to_bits(),
                color.b.to_bits(),
                color.a.to_bits()
            );
            assert_eq!(got, line);
            clamped += 1;
            continue;
        }
        if line.starts_with("swz ") {
            swizzled += 1;
        } else {
            plain += 1;
        }
        let (value, columns) = line.split_once('|').expect("a bar");
        let format = hex(value.split_whitespace().rev().nth(1).expect("a value"));
        let got = format!(
            "0x{:08x} 0x{:08x} {} {} {} {} 0x{:08x} {} {}",
            forget_srgb(format),
            recall_srgb(format),
            u32::from(forget_srgb(format) != format),
            u32::from(is_bc(format)),
            depth_stencil_planar_flags(format),
            texel_size(format),
            unsigned_integer(format),
            u32::from(is_unorm8_rt(format)),
            u32::from(is_integer(format))
        );
        assert_eq!(got, columns.trim(), "{line}");
    }
    assert_eq!((plain, swizzled, clamped), (314, 138, 2100));
}

#[test]
fn desc_layout_is_the_cpps() {
    // clang's layout of `dxmt::MTL_DXGI_FORMAT_DESC` and `struct dxmt_format_capability`, and the
    // values of `MTL_DXGI_FORMAT_FLAG` and `FormatCapability`, in `dxmt_format.hpp`.
    assert_eq!(size_of::<MtlDxgiFormatDesc>(), 16);
    assert_eq!(offset_of!(MtlDxgiFormatDesc, pixel_format), 0);
    assert_eq!(offset_of!(MtlDxgiFormatDesc, attribute_format), 4);
    assert_eq!(offset_of!(MtlDxgiFormatDesc, bytes_per_texel), 8);
    assert_eq!(offset_of!(MtlDxgiFormatDesc, planar_count), 12);
    assert_eq!(offset_of!(MtlDxgiFormatDesc, flag), 14);
    assert_eq!(size_of::<DxmtFormatCapability>(), 8);
    assert_eq!(offset_of!(DxmtFormatCapability, format), 0);
    assert_eq!(offset_of!(DxmtFormatCapability, caps), 4);
    assert_eq!(
        [
            MTL_DXGI_FORMAT_TYPELESS,
            MTL_DXGI_FORMAT_BC,
            MTL_DXGI_FORMAT_BACKBUFFER,
            MTL_DXGI_FORMAT_DEPTH_PLANER,
            MTL_DXGI_FORMAT_STENCIL_PLANER,
            MTL_DXGI_FORMAT_EMULATED_D24,
            MTL_DXGI_FORMAT_EMULATED_LINEAR_DEPTH_STENCIL,
        ],
        [1, 2, 4, 16, 32, 256, 512]
    );
    assert_eq!(
        [
            CAP_ATOMIC,
            CAP_FILTER,
            CAP_WRITE,
            CAP_COLOR,
            CAP_BLEND,
            CAP_MSAA,
            CAP_SPARSE,
            CAP_RESOLVE,
            CAP_DEPTH_STENCIL,
            CAP_TEXTURE_BUFFER_READ,
            CAP_TEXTURE_BUFFER_WRITE,
            CAP_TEXTURE_BUFFER_READ_WRITE,
        ],
        [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048]
    );
}

#[test]
fn unsupported_format_leaves_the_cpps_defaults() {
    let mut desc = MtlDxgiFormatDesc {
        pixel_format: 0xA5A5_A5A5,
        attribute_format: 0xA5A5_A5A5,
        bytes_per_texel: 0xA5A5_A5A5,
        planar_count: 0xA5A5,
        flag: 0xA5A5,
    };
    // SAFETY: `desc` is writable.
    let result = unsafe { dxmt_format_query_dxgi(0xFFFF_FFFF, &raw mut desc) };
    assert_eq!(result, E_FAIL);
    assert_eq!(
        (
            desc.pixel_format,
            desc.attribute_format,
            desc.bytes_per_texel,
            desc.planar_count,
            desc.flag
        ),
        (0, 0, 0, 1, 0)
    );
}

#[test]
fn capabilities_export_counts_and_copies() {
    let blank = DxmtFormatCapability { format: 0, caps: 0 };
    let mut out = [blank, DxmtFormatCapability { format: 0, caps: 0 }];
    FAMILIES.set(mask(families("Apple7")));
    BC.set(true);
    // SAFETY: a capacity of 0 writes nothing, whatever `out` is.
    let count = unsafe { dxmt_format_capabilities(1, null_mut(), 0) };
    assert_eq!(count, 76);
    // SAFETY: `out` has room for 2 entries.
    let count = unsafe { dxmt_format_capabilities(1, out.as_mut_ptr(), 2) };
    assert_eq!(count, 76);
    let want = capabilities(|family| family == WMTGPU_FAMILY_APPLE7, true).expect("an Apple7 list");
    assert_eq!(
        out.map(|entry| (entry.format, entry.caps)),
        [want[0], want[1]]
    );
    FAMILIES.set(0);
    // SAFETY: a capacity of 0 writes nothing.
    let count = unsafe { dxmt_format_capabilities(1, null_mut(), 0) };
    assert_eq!(count, INVALID_DEVICE);
}

#[test]
fn sanitize_export_writes_the_color() {
    let mut color = WMTClearColor {
        r: 300.0,
        g: -1.0,
        b: f64::NAN,
        a: 0.5,
    };
    // SAFETY: `color` is writable.
    unsafe { dxmt_format_sanitize_clear_color(WMT_PIXEL_FORMAT_RGBA8_UINT, &raw mut color) };
    assert_eq!((color.r, color.g, color.b, color.a), (255.0, 0.0, 0.0, 0.5));
}
