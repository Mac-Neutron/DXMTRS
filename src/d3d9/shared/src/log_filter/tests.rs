//! Unit tests for the log filter's composition.
//!
//! Logger init takes `DXMT_LOG_LEVEL`'s base (`info` unless it names another level, `off` for
//! `none`) and appends whatever `RUST_LOG` asks for, relying on `env_logger` giving the last
//! matching spec priority so a bare level or a per-module override still takes effect. These
//! cases pin the composed string for an unset or empty variable, a bare level, root and
//! sub-namespace targets, a multi-spec passthrough, and each base level.

use super::resolved_log_filter;

#[test]
fn unset_defaults_to_info() {
    assert_eq!(resolved_log_filter(None, None), "info");
}

#[test]
fn empty_string_defaults_to_info() {
    assert_eq!(resolved_log_filter(None, Some("")), "info");
}

#[test]
fn bare_level_wins_via_last_spec() {
    assert_eq!(resolved_log_filter(None, Some("warn")), "info,warn");
}

#[test]
fn root_spec_composes() {
    assert_eq!(
        resolved_log_filter(None, Some("mtld3d=warn")),
        "info,mtld3d=warn"
    );
}

#[test]
fn sub_namespace_override_restores_baseline() {
    assert_eq!(
        resolved_log_filter(None, Some("mtld3d::perf=debug")),
        "info,mtld3d::perf=debug"
    );
}

#[test]
fn multi_spec_passthrough() {
    assert_eq!(
        resolved_log_filter(None, Some("mtld3d=warn,mtld3d::dxso=trace")),
        "info,mtld3d=warn,mtld3d::dxso=trace",
    );
}

#[test]
fn dxmt_log_level_sets_the_base() {
    assert_eq!(resolved_log_filter(Some("warn"), None), "warn");
}

#[test]
fn dxmt_log_level_none_is_off() {
    assert_eq!(resolved_log_filter(Some("none"), None), "off");
}

#[test]
fn an_unknown_dxmt_log_level_is_info() {
    assert_eq!(resolved_log_filter(Some("loud"), None), "info");
}

#[test]
fn rust_log_still_overrides_the_base() {
    assert_eq!(
        resolved_log_filter(Some("none"), Some("mtld3d::perf=info")),
        "off,mtld3d::perf=info"
    );
}
