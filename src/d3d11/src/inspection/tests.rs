//! Golden probe tests for `Inspection` (`d11dev.txt`).

use core::ptr;

use super::Inspection;

const GOLDEN: &str = include_str!("golden/d11dev.txt");

#[test]
fn features_are_the_probe_golden() {
    let insp = Inspection::default_apple8_profile();
    let mut count = 0usize;

    for line in GOLDEN.lines() {
        let Some(rest) = line.strip_prefix("d11dev feat=") else {
            continue;
        };
        count += 1;
        let parts: Vec<&str> = rest.split_whitespace().collect();
        let feat_str = parts[0].split('(').next().unwrap();
        let feat: u32 = feat_str.parse().unwrap();

        let expected_null = parse_hex_field(parts[1], "null=");
        let expected_badsz = parse_hex_field(parts[2], "badsz=");
        let expected_ok = parse_hex_field(parts[3], "ok=");
        let word_tokens = &parts[4..];
        let byte_size = u32::try_from(word_tokens.len() * 4).unwrap();

        // 1. Null pointer with valid size -> E_INVALIDARG.
        let hr_null = insp
            .check_feature_support(feat, ptr::null_mut(), byte_size)
            .cast_unsigned();
        assert_eq!(hr_null, expected_null, "feat {feat} null mismatch");

        // 2. Wrong size -> E_INVALIDARG.
        let mut scratch = vec![0u32; word_tokens.len().max(1)];
        let hr_badsz = insp
            .check_feature_support(feat, scratch.as_mut_ptr().cast(), byte_size + 1)
            .cast_unsigned();
        assert_eq!(hr_badsz, expected_badsz, "feat {feat} badsz mismatch");

        // 3. Valid buffer -> matches `ok=` and `w0..wN`.
        scratch.fill(0);
        if feat == 2 || feat == 3 {
            // Probe passes DXGI_FORMAT_R8G8B8A8_UNORM (28) in word 0 for FORMAT_SUPPORT(2).
            scratch[0] = 28;
        }
        let hr_ok = insp
            .check_feature_support(feat, scratch.as_mut_ptr().cast(), byte_size)
            .cast_unsigned();
        assert_eq!(hr_ok, expected_ok, "feat {feat} ok mismatch");

        for (idx, token) in word_tokens.iter().enumerate() {
            let prefix = format!("w{idx}=");
            let expected_w = parse_hex_field(token, &prefix);
            assert_eq!(scratch[idx], expected_w, "feat {feat} word w{idx} mismatch");
        }
    }

    assert_eq!(count, 22, "expected 22 feature lines in d11dev.txt");
}

#[test]
fn format_support_is_the_probe_golden() {
    let insp = Inspection::default_apple8_profile();
    let mut fmt_count = 0usize;
    let mut saw_edge = false;

    for line in GOLDEN.lines() {
        if let Some(rest) = line.strip_prefix("d11dev fmt=") {
            fmt_count += 1;
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let fmt: u32 = parts[0].parse().unwrap();

            let (exp_sup1_hr, exp_sup1_val) = parse_pair_hex(parts[1], "sup1=");
            let (exp_sup2_hr, exp_sup2_val) = parse_pair_hex(parts[2], "sup2=");

            let mut sup1 = 0u32;
            let hr1 = insp
                .check_format_support(fmt, &raw mut sup1)
                .cast_unsigned();
            assert_eq!(hr1, exp_sup1_hr, "fmt {fmt} sup1 hr mismatch");
            assert_eq!(sup1, exp_sup1_val, "fmt {fmt} sup1 val mismatch");

            let mut sup2_data = [fmt, 0u32];
            let hr2 = insp
                .check_feature_support(3, sup2_data.as_mut_ptr().cast(), 8)
                .cast_unsigned();
            assert_eq!(hr2, exp_sup2_hr, "fmt {fmt} sup2 hr mismatch");
            assert_eq!(sup2_data[1], exp_sup2_val, "fmt {fmt} sup2 val mismatch");

            // msq has 4 comma-separated entries for sample counts 1, 2, 4, 8.
            let legacy_msq = parts[3].strip_prefix("msq=").unwrap();
            for (sample_count, entry) in [1u32, 2, 4, 8].into_iter().zip(legacy_msq.split(',')) {
                let (exp_hr, exp_q) = parse_slash_hex_dec(entry);
                let mut q_n = 0u32;
                let hr_n = insp
                    .check_multisample_quality_levels(fmt, sample_count, 0, &raw mut q_n)
                    .cast_unsigned();
                assert_eq!(hr_n, exp_hr, "fmt {fmt} msq({sample_count}) hr mismatch");
                assert_eq!(q_n, exp_q, "fmt {fmt} msq({sample_count}) q mismatch");
            }

            // msq1 has two comma-separated entries: flags=0 (pre-fill 0) and flags=1 (pre-fill 99).
            let v1_msq = parts[4].strip_prefix("msq1=").unwrap();
            let mut v1_parts = v1_msq.split(',');
            let (exp_msq1_hr, exp_msq1_q) = parse_slash_hex_dec(v1_parts.next().unwrap());
            let (exp_msq1_f1_hr, exp_msq1_f1_q) = parse_slash_hex_dec(v1_parts.next().unwrap());

            let mut q = 0u32;
            let hr_msq1 = insp
                .check_multisample_quality_levels(fmt, 1, 0, &raw mut q)
                .cast_unsigned();
            assert_eq!(hr_msq1, exp_msq1_hr, "fmt {fmt} msq1 hr mismatch");
            assert_eq!(q, exp_msq1_q, "fmt {fmt} msq1 q mismatch");

            let mut q_f1 = 99u32;
            let hr_msq1_f1 = insp
                .check_multisample_quality_levels(fmt, 1, 1, &raw mut q_f1)
                .cast_unsigned();
            assert_eq!(
                hr_msq1_f1, exp_msq1_f1_hr,
                "fmt {fmt} msq1 flags=1 hr mismatch"
            );
            assert_eq!(q_f1, exp_msq1_f1_q, "fmt {fmt} msq1 flags=1 q mismatch");
        } else if let Some(rest) = line.strip_prefix("d11dev fmt_edge ") {
            saw_edge = true;
            let parts: Vec<&str> = rest.split_whitespace().collect();
            let exp_fmt_null = parse_hex_field(parts[0], "fmt_null=");
            let exp_msq_null = parse_hex_field(parts[1], "msq_null=");
            let (exp_s3_hr, exp_s3_q) = parse_slash_hex_dec(parts[2].strip_prefix("s3=").unwrap());
            let (exp_s16_hr, exp_s16_q) =
                parse_slash_hex_dec(parts[3].strip_prefix("s16=").unwrap());

            let hr_fmt_null = insp
                .check_format_support(28, ptr::null_mut())
                .cast_unsigned();
            assert_eq!(hr_fmt_null, exp_fmt_null);

            let hr_msq_null = insp
                .check_multisample_quality_levels(28, 1, 0, ptr::null_mut())
                .cast_unsigned();
            assert_eq!(hr_msq_null, exp_msq_null);

            let mut q3 = 77u32;
            let hr_s3 = insp
                .check_multisample_quality_levels(28, 3, 0, &raw mut q3)
                .cast_unsigned();
            assert_eq!(hr_s3, exp_s3_hr);
            assert_eq!(q3, exp_s3_q);

            let mut q16 = 77u32;
            let hr_s16 = insp
                .check_multisample_quality_levels(28, 16, 0, &raw mut q16)
                .cast_unsigned();
            assert_eq!(hr_s16, exp_s16_hr);
            assert_eq!(q16, exp_s16_q);
        }
    }

    assert_eq!(fmt_count, 192, "expected 192 format lines in d11dev.txt");
    assert!(saw_edge, "expected fmt_edge line in d11dev.txt");
}

fn parse_hex_field(token: &str, prefix: &str) -> u32 {
    u32::from_str_radix(token.strip_prefix(prefix).unwrap(), 16).unwrap()
}

fn parse_pair_hex(token: &str, prefix: &str) -> (u32, u32) {
    let body = token.strip_prefix(prefix).unwrap();
    let (a, b) = body.split_once(':').unwrap();
    (
        u32::from_str_radix(a, 16).unwrap(),
        u32::from_str_radix(b, 16).unwrap(),
    )
}

fn parse_slash_hex_dec(body: &str) -> (u32, u32) {
    let (a, b) = body.split_once('/').unwrap();
    (u32::from_str_radix(a, 16).unwrap(), b.parse().unwrap())
}
