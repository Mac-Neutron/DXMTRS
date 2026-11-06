/// One-shot logger init shared by all three cdylibs (`d3d9.dll`, `mtld3d.dll`, `mtld3d.so`).
///
/// Each cdylib has its own copy of the `log` / `env_logger` statics, so each
/// calls this from its own entry point (`DllMain` on PE, the `InitLogger`
/// thunk on the unix side). `try_init` is idempotent so repeat calls silently
/// no-op.
///
/// Every line goes to the process's log file (see the `OpenLog` thunk), so
/// no target ever colours: `WriteStyle::Never` on all three linkage units
/// keeps escape sequences out of the file.
///
/// The filter is `DXMT_LOG_LEVEL`'s base level refined by `RUST_LOG`
/// ([`resolved_log_filter`]).
pub fn init_logger() {
    init(None, &env_filter());
}

/// Register `env_logger` writing every formatted line into `sink`.
///
/// The PE side uses this with a sink that crosses to the unix side, which
/// owns the process's log file; the unix side uses it with the file itself.
/// Filter and style match [`init_logger`].
pub fn init_logger_to(sink: Box<dyn std::io::Write + Send + 'static>) {
    init(Some(sink), &env_filter());
}

/// Register the native logger with the filter the PE side resolved, used verbatim.
///
/// Wine child process environment changes are not reflected in the Unix environment.
/// Passing the filter explicitly keeps logging consistent across the boundary. `None` (no
/// filter sent) logs at `info`.
pub fn init_logger_to_filter(sink: Box<dyn std::io::Write + Send + 'static>, filter: Option<&str>) {
    init(Some(sink), filter.unwrap_or("info"));
}

fn init(sink: Option<Box<dyn std::io::Write + Send + 'static>>, filter: &str) {
    let mut builder = env_logger::Builder::new();
    builder
        .parse_filters(filter)
        .write_style(env_logger::WriteStyle::Never);
    if let Some(sink) = sink {
        builder.target(env_logger::Target::Pipe(sink));
    }
    let _ = builder.try_init();
}

/// This process's filter: `DXMT_LOG_LEVEL` and `RUST_LOG` from its environment.
fn env_filter() -> String {
    let level = std::env::var("DXMT_LOG_LEVEL").ok();
    let user = std::env::var("RUST_LOG").ok();
    resolved_log_filter(level.as_deref(), user.as_deref())
}

/// The `env_logger` filter of D3D9's log: `level`'s base, then `user`'s directives.
///
/// `level` is `DXMT_LOG_LEVEL`'s value, read as DXMT reads it (`info` unless it names
/// another level; `none` is `off`); `user` is `RUST_LOG`'s, appended when non-empty so its
/// directives, the later ones, refine the base per target.
#[must_use]
pub fn resolved_log_filter(level: Option<&str>, user: Option<&str>) -> String {
    let base = dxmt_util::log::level_from(level).directive();
    match user {
        Some(s) if !s.is_empty() => format!("{base},{s}"),
        _ => base.to_owned(),
    }
}

#[cfg(test)]
mod tests;
