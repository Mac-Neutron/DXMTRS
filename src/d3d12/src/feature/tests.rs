use core::mem::size_of;

use super::*;
use crate::abi::feature::D3D12_FEATURE_D3D12_OPTIONS;

/// The C++ native `d3d12.dylib`'s answers over the sections Task 0 recorded.
const GOLDEN: &str = include_str!("golden/golden.txt");
/// Every feature at every size, including the 60 bytes of `D3D12_OPTIONS` Task 0's grid lacks.
const WIDE: &str = include_str!("golden/wide.txt");
/// The shader model and feature level answers for inputs with the top bit set, on both settings.
const SIGNED: &str = include_str!("golden/signed.txt");
/// The sections Shader Model 6 changes, and every size again, with `DXMT_D3D12_SM6=0`.
const SM5: &str = include_str!("golden/sm5.txt");

/// The device the goldens were recorded on: Metal supports 1, 2, 4 and 8 samples.
struct Recorded {
    sm6: bool,
}

impl Device for Recorded {
    fn sm6_caps(&self) -> bool {
        self.sm6
    }

    fn advertise_numa(&self) -> bool {
        false
    }

    fn supports_texture_sample_count(&self, count: u32) -> bool {
        matches!(count, 1 | 2 | 4 | 8)
    }
}

/// The sizes of `Raw`'s grid in the driver that recorded Task 0's golden.
const SIZES: [usize; 19] = [
    4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 64, 72, 80, 96, 128,
];

/// Calls `check_feature_support` on a buffer.
fn call(device: &Recorded, id: u32, data: &mut [u8]) -> Hresult {
    // SAFETY: the buffers here are either zero (or 0xff) filled structures of the wrong size or a
    // feature other than FEATURE_LEVELS, or FEATURE_LEVELS with a count of 0, or the levels
    // section's, whose pointer is to a live array of its count; none reads past what it holds.
    unsafe { check_feature_support(device, id, data) }
}

/// The driver's `Raw`: the call on two fills of a buffer.
///
/// The fills are 0x00 and 0xff; the line it prints is none when both answer `E_INVALIDARG`.
fn raw(
    device: &Recorded,
    id: u32,
    size: usize,
    tag: &str,
    init: fn(&mut [u8], i32, i32),
    inputs: (i32, i32),
) -> Option<String> {
    let mut buffers = [[0_u8; 512], [0xff_u8; 512]];
    let mut results = [0; 2];
    for (buffer, result) in buffers.iter_mut().zip(&mut results) {
        init(buffer, inputs.0, inputs.1);
        *result = call(device, id, &mut buffer[..size]);
    }
    if results == [E_INVALIDARG; 2] {
        return None;
    }
    let data: String = buffers[0][..size]
        .iter()
        .zip(&buffers[1][..size])
        .map(|(zero, ones)| {
            if zero == ones {
                format!("{zero:02x}")
            } else {
                "??".to_owned()
            }
        })
        .collect();
    Some(format!(
        "feat {tag} id={id} size={size} in={},{} rc={:08x}/{:08x} data={data}",
        inputs.0,
        inputs.1,
        results[0].cast_unsigned(),
        results[1].cast_unsigned()
    ))
}

fn set_first(data: &mut [u8], value: i32, _: i32) {
    put(data, 0, value.cast_unsigned());
}

fn set_msaa(data: &mut [u8], format: i32, count: i32) {
    put(data, 0, format.cast_unsigned());
    put(data, 4, count.cast_unsigned());
    put(data, 8, 0);
}

fn set_nothing(_: &mut [u8], _: i32, _: i32) {}

/// The driver's `levels` section.
fn levels(device: &Recorded) -> Vec<String> {
    let all: [u32; 9] = [
        0x9100, 0x9200, 0x9300, 0xa000, 0xa100, 0xb000, 0xb100, 0xc000, 0xc100,
    ];
    let mut lines = Vec::new();
    let run = |count: u32, first: usize| {
        let mut data = [0_u8; size_of::<FeatureLevels>()];
        put(
            &mut data,
            offset_of!(FeatureLevels, num_feature_levels),
            count,
        );
        let pointer = all[first..].as_ptr().expose_provenance().to_le_bytes();
        data[offset_of!(FeatureLevels, feature_levels_requested)..][..pointer.len()]
            .copy_from_slice(&pointer);
        let hr = call(device, D3D12_FEATURE_FEATURE_LEVELS, &mut data);
        let max = get(
            &data,
            offset_of!(FeatureLevels, max_supported_feature_level),
        );
        (hr.cast_unsigned(), max)
    };
    for count in 0..=9 {
        let (hr, max) = run(count, 0);
        lines.push(format!("feat levels n={count} rc={hr:08x} max=0x{max:x}"));
    }
    let (hr, max) = run(1, 7);
    lines.push(format!("feat levels only-12_0 rc={hr:08x} max=0x{max:x}"));
    lines
}

/// The driver's `shader_model` section.
fn shader_model(device: &Recorded) -> Vec<String> {
    let models = [
        0, 0x4f, 0x50, 0x51, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a,
        0x70, 0xff,
    ];
    let mut lines: Vec<String> = models
        .iter()
        .filter_map(|&model| {
            raw(
                device,
                D3D12_FEATURE_SHADER_MODEL,
                4,
                "shader_model",
                set_first,
                (model, 0),
            )
        })
        .collect();
    lines.extend(raw(
        device,
        D3D12_FEATURE_SHADER_MODEL,
        8,
        "shader_model",
        set_first,
        (0x65, 0),
    ));
    lines
}

/// The sections of Task 0's golden, in its order.
fn sections(device: &Recorded) -> Vec<String> {
    let mut lines = Vec::new();
    for id in (0..=80).filter(|id| ![2, 3, 4, 5, 7, 12, 1, 16, 10].contains(id)) {
        for size in SIZES {
            lines.extend(raw(device, id, size, "generic", set_nothing, (0, 0)));
        }
    }
    for format in 0..=200 {
        lines.extend(raw(
            device,
            D3D12_FEATURE_FORMAT_SUPPORT,
            12,
            "format_support",
            set_first,
            (format, 0),
        ));
    }
    for format in [0x7fff_ffff_u32, 0xffff_fffe] {
        lines.extend(raw(
            device,
            D3D12_FEATURE_FORMAT_SUPPORT,
            12,
            "format_support",
            set_first,
            (format.cast_signed(), 0),
        ));
    }
    for format in 0..=200 {
        lines.extend(raw(
            device,
            D3D12_FEATURE_FORMAT_INFO,
            8,
            "format_info",
            set_first,
            (format, 0),
        ));
    }
    lines.extend(raw(
        device,
        D3D12_FEATURE_FORMAT_INFO,
        4,
        "format_info",
        set_first,
        (28, 0),
    ));
    lines.extend(raw(
        device,
        D3D12_FEATURE_FORMAT_INFO,
        16,
        "format_info",
        set_first,
        (28, 0),
    ));
    for format in [0, 2, 28, 29, 40, 45, 55, 71, 77, 83, 87, 61] {
        for count in [0, 1, 2, 3, 4, 8, 16, 32] {
            lines.extend(raw(
                device,
                D3D12_FEATURE_MULTISAMPLE_QUALITY_LEVELS,
                16,
                "msaa",
                set_msaa,
                (format, count),
            ));
        }
    }
    lines.extend(levels(device));
    lines.extend(shader_model(device));
    for version in [0, 1, 2, 3, 0x10001] {
        lines.extend(raw(
            device,
            D3D12_FEATURE_ROOT_SIGNATURE,
            4,
            "root_signature",
            set_first,
            (version, 0),
        ));
    }
    for id in [D3D12_FEATURE_ARCHITECTURE, D3D12_FEATURE_ARCHITECTURE1] {
        for size in [4, 8, 12, 16, 20, 24] {
            for node in [0, 1, 2] {
                lines.extend(raw(device, id, size, "arch", set_first, (node, 0)));
            }
        }
    }
    for node in [0, 1] {
        lines.extend(raw(
            device,
            D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_SUPPORT,
            8,
            "protected",
            set_first,
            (node, 0),
        ));
        lines.extend(raw(
            device,
            D3D12_FEATURE_PROTECTED_RESOURCE_SESSION_SUPPORT,
            4,
            "protected",
            set_first,
            (node, 0),
        ));
    }
    lines
}

/// The driver's `feat-wide` mode.
fn wide(device: &Recorded) -> Vec<String> {
    let sizes = [
        1, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 64, 68, 72, 80, 96, 128,
    ];
    let mut lines = Vec::new();
    for id in 0..=80_u32 {
        let mut rows = Vec::new();
        let mut answered = false;
        for size in sizes {
            if id == D3D12_FEATURE_FEATURE_LEVELS && size == size_of::<FeatureLevels>() {
                continue;
            }
            let Some(line) = raw(device, id, size, "wide", set_nothing, (0, 0)) else {
                answered = true;
                continue;
            };
            if line.contains("rc=80004001/80004001") {
                continue;
            }
            // The driver prints this form: no inputs.
            rows.push(line.replacen(" in=0,0", "", 1));
        }
        if rows.is_empty() {
            lines.push(format!(
                "feat wide id={id} {}",
                if answered {
                    "invalid-at-every-size"
                } else {
                    "notimpl"
                }
            ));
        } else {
            lines.extend(rows);
        }
    }
    for id in [0, 1, 2, 3, 4, 5, 7, 12, 16] {
        lines.push(format!(
            "feat wide-null id={id} rc={:08x}",
            call(device, id, &mut []).cast_unsigned()
        ));
    }
    lines
}

/// Compares a replay with a golden, line by line.
fn assert_lines(actual: &[String], golden: &str) {
    let expected: Vec<&str> = golden.lines().collect();
    for (index, (got, want)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(got, want, "line {}", index + 1);
    }
    assert_eq!(actual.len(), expected.len());
}

#[test]
fn features_are_the_golden() {
    assert_lines(&sections(&Recorded { sm6: true }), GOLDEN);
}

#[test]
fn features_at_every_size_as_cpp() {
    assert_lines(&wide(&Recorded { sm6: true }), WIDE);
}

#[test]
fn features_without_sm6_as_cpp() {
    let device = Recorded { sm6: false };
    let mut lines = levels(&device);
    lines.extend(shader_model(&device));
    lines.extend(wide(&device));
    assert_lines(&lines, SM5);
}

#[test]
fn options_is_sixty_bytes() {
    // The size Task 0's grid lacks: `features_at_every_size_as_cpp` is the only test that has it.
    assert_eq!(size_of::<Options>(), 60);
    assert!(WIDE.contains(" id=0 size=60 "));
    let mut data = [0xff_u8; 60];
    assert_eq!(
        call(
            &Recorded { sm6: true },
            D3D12_FEATURE_D3D12_OPTIONS,
            &mut data
        ),
        S_OK
    );
}

#[test]
fn feature_levels_without_an_array_is_refused() {
    // The C++ reads the application's array at a null pointer; here it is an error.
    let mut data = [0_u8; size_of::<FeatureLevels>()];
    put(&mut data, offset_of!(FeatureLevels, num_feature_levels), 3);
    assert_eq!(
        call(
            &Recorded { sm6: true },
            D3D12_FEATURE_FEATURE_LEVELS,
            &mut data
        ),
        E_INVALIDARG
    );
}

#[test]
fn the_enumerations_compare_as_signed_numbers_as_cpp() {
    let mut checked = 0;
    for line in SIGNED.lines() {
        let words: Vec<&str> = line.split(' ').collect();
        let device = Recorded {
            sm6: words[0] == "sm6",
        };
        let hex = |word: &str| {
            u32::from_str_radix(word.split('=').nth(1).expect("a value"), 16).expect("hex")
        };
        let (hr, out) = if words[1] == "shader_model" {
            let mut data = hex(words[2]).to_le_bytes();
            let hr = call(&device, D3D12_FEATURE_SHADER_MODEL, &mut data);
            (hr, u32::from_le_bytes(data))
        } else {
            let count = words[2]
                .split('=')
                .nth(1)
                .expect("a count")
                .parse()
                .expect("a count");
            let levels = [0xb000_u32, hex(words[3])];
            let first = usize::from(count == 1);
            let mut data = [0_u8; size_of::<FeatureLevels>()];
            put(
                &mut data,
                offset_of!(FeatureLevels, num_feature_levels),
                count,
            );
            let pointer = levels[first..].as_ptr().expose_provenance().to_le_bytes();
            data[offset_of!(FeatureLevels, feature_levels_requested)..][..pointer.len()]
                .copy_from_slice(&pointer);
            let hr = call(&device, D3D12_FEATURE_FEATURE_LEVELS, &mut data);
            (
                hr,
                get(
                    &data,
                    offset_of!(FeatureLevels, max_supported_feature_level),
                ),
            )
        };
        assert_eq!(hr.cast_unsigned(), hex(words[words.len() - 2]), "{line}");
        assert_eq!(out, hex(words[words.len() - 1]), "{line}");
        checked += 1;
    }
    assert_eq!(checked, 54);
}
