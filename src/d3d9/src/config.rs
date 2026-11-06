//! The [`Mtld3dConfig`] an `IDirect3D9` resolves at `Direct3DCreate9`.
//!
//! Its lookups: the built-in [`AppProfile`] for this application, if it has
//! one, from the executable's name plus the version resource of the image the
//! loader mapped; then the four sources every API reads
//! ([`Sources::read`]): `dxmt.conf` (`DXMT_CONFIG_FILE` or the working
//! folder), the optional `mtld3d.conf` beside the executable
//! (`std::env::current_exe()`'s folder; a missing file is fine),
//! `DXMT_CONFIG` and `MTLD3D_CONFIG`, a later one winning. See `mtld3d.conf`
//! at the repo root for the user-facing sample with documented keys and
//! defaults.
//!
//! Resolved at the top of `Direct3DCreate9`, once per interface, so option
//! resolution and the per-key info log fire before the interface answers
//! anything; the interface owns the result and hands it to each device it
//! creates.

use std::{ffi::c_void, ptr};

use dxmt_util::config::Sources;
use log::info;
use mtld3d_core::{
    app_profile::{AppIdentity, AppProfile},
    config::{Mtld3dConfig, log_options, parse_sources},
};
use mtld3d_shared::identity::version_blob;

use crate::LOG_TARGET;

unsafe extern "system" {
    fn GetModuleHandleA(module_name: *const u8) -> *mut c_void;
}

/// Resolve the configuration for a new `IDirect3D9`.
///
/// Reads the profile, the files and the environment afresh each time, so a
/// process that creates several interfaces gives each the options in force
/// when it was created.
pub fn load() -> Mtld3dConfig {
    let profile = app_profile();
    let sources = Sources::read();
    log_sources(&sources);
    let cfg = parse_sources(profile, &sources, dxmt_util::env::exe_name().as_bytes());
    log_options(&cfg);
    cfg
}

/// The built-in profile for the executable this library was loaded into.
fn app_profile() -> Option<&'static AppProfile> {
    let exe = std::env::current_exe().ok()?;
    let name = exe.file_name()?.to_string_lossy().into_owned();
    // A null module name asks for the main image, which is the executable the
    // profile matches on rather than this DLL.
    // SAFETY: the call takes a null name and returns the main image's handle.
    let main = unsafe { GetModuleHandleA(ptr::null()) };
    // SAFETY: `main` is a handle the loader owns, so the image behind it is
    // mapped for the life of the process.
    let blob = unsafe { version_blob(main) };
    mtld3d_core::app_profile::lookup(&AppIdentity::new(name, blob.as_deref()))
}

/// Name in D3D9's log what was read: `mtld3d.conf`, `dxmt.conf` and the two variables.
fn log_sources(sources: &Sources) {
    match &sources.mtld3d_file {
        None => info!(
            target: LOG_TARGET,
            "mtld3d.conf: current_exe() unavailable — using defaults"
        ),
        Some((path, Ok(_))) => {
            info!(target: LOG_TARGET, "mtld3d.conf: loaded from {}", path.display());
        }
        Some((path, Err(e))) => info!(
            target: LOG_TARGET,
            "mtld3d.conf: not loaded from {} ({e}) — using defaults",
            path.display()
        ),
    }
    if let Some((path, _)) = &sources.dxmt_file {
        info!(
            target: LOG_TARGET,
            "mtld3d.conf: dxmt.conf loaded from {}",
            String::from_utf8_lossy(path)
        );
    }
    let dxmt_env = String::from_utf8_lossy(&sources.dxmt_env);
    if !dxmt_env.trim().is_empty() {
        info!(target: LOG_TARGET, "mtld3d.conf: applying DXMT_CONFIG overrides: {dxmt_env}");
    }
    let mtld3d_env = String::from_utf8_lossy(&sources.mtld3d_env);
    if !mtld3d_env.trim().is_empty() {
        info!(target: LOG_TARGET, "mtld3d.conf: applying MTLD3D_CONFIG overrides: {mtld3d_env}");
    }
}
