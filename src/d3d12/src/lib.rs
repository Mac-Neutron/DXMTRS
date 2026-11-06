//! DXMT's `d3d12.dll` and `d3d12.dylib`: the D3D12 front end over Metal.

pub mod abi;
pub mod allocator;
pub mod barrier;
pub mod blob;
pub mod capture;
pub mod com;
pub mod command_signature;
pub mod descriptor_heap;
pub mod device;
pub mod encoder;
pub mod feature;
pub mod fence;
pub mod heap;
pub mod list;
pub mod pipeline;
pub mod pipeline_stream;
pub mod query_heap;
pub mod queue;
pub mod resource;
pub mod resource_math;
pub mod root_signature;
pub mod sampler;
pub mod shader;
pub mod shader_cache;
pub mod shared_fence;
pub mod stats;
pub mod swapchain;
pub mod unimpl;

use core::ffi::c_void;
use std::sync::LazyLock;

use crate::com::{E_INVALIDARG, E_NOINTERFACE, Guid, Hresult};

/// `D3D_FEATURE_LEVEL_11_0` (`0xb000`), the minimum feature level `D3D12CreateDevice` accepts.
pub const D3D_FEATURE_LEVEL_11_0: u32 = 0xb000;

dxmt_util::dxmt_dll!(pub log "d3d12.log", allocator, type_info);

/// One-time process initialization: latches `DXMT_STATS=1` and registers `atexit` natively.
static INIT: LazyLock<()> = LazyLock::new(|| {
    stats::init(std::env::var("DXMT_STATS").as_deref() == Ok("1"));
    #[cfg(not(windows))]
    {
        extern "C" fn on_exit() {
            on_process_detach();
        }
        // SAFETY: `on_exit` is a plain `extern "C"` function taking no arguments.
        unsafe { libc::atexit(on_exit) };
    }
});

fn ensure_init() {
    LazyLock::force(&INIT);
}

fn on_process_detach() {
    shader_cache::log_counters();
    dxmt_core::shader_cache::close_all();
    stats::exit();
}

/// Logs an error message to `d3d12.log`.
#[macro_export]
macro_rules! err {
    ($($arg:tt)*) => {
        ::dxmt_util::err!($crate::LOG, $($arg)*)
    };
}

/// Logs a warning message to `d3d12.log`.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        ::dxmt_util::warn!($crate::LOG, $($arg)*)
    };
}

/// Logs an info message to `d3d12.log` (compiled out under `perf-build`).
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        ::dxmt_util::info!($crate::LOG, $($arg)*)
    };
}

/// Logs a debug message to `d3d12.log` (compiled out under `perf-build`).
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        ::dxmt_util::debug!($crate::LOG, $($arg)*)
    };
}

/// Logs a trace message to `d3d12.log` (compiled out under `perf-build`).
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        ::dxmt_util::trace!($crate::LOG, $($arg)*)
    };
}

/// Logs an error message to `d3d12.log` once per call site.
#[macro_export]
macro_rules! err_once {
    ($($arg:tt)*) => {
        ::dxmt_util::err_once!($crate::LOG, $($arg)*)
    };
}

/// Log sink called by `dxmt_core::log` (`src/dxmt/src/log.rs:24`).
///
/// # Safety
///
/// Unless `length == 0`, `message` must point to `length` readable bytes for the duration of the
/// call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dxmt_log(level: u32, message: *const u8, length: usize) {
    let bytes = if length == 0 || message.is_null() {
        &[]
    } else {
        // SAFETY: `message` is non-null and points to `length` readable bytes.
        unsafe { core::slice::from_raw_parts(message, length) }
    };
    let text = String::from_utf8_lossy(bytes);
    match level {
        0 => LOG.trace(&text),
        1 => LOG.debug(&text),
        2 => LOG.info(&text),
        3 => LOG.warn(&text),
        _ => LOG.err(&text),
    }
}

#[cfg(windows)]
unsafe extern "system" {
    fn DisableThreadLibraryCalls(module: *mut c_void) -> i32;
}

/// `DllMain` (`d3d12.cpp:84-92`).
#[cfg(windows)]
#[unsafe(export_name = "DllMain")]
extern "system" fn dll_main(instance: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    const DLL_PROCESS_DETACH: u32 = 0;
    const DLL_PROCESS_ATTACH: u32 = 1;
    if reason == DLL_PROCESS_DETACH {
        on_process_detach();
        return 1;
    }
    if reason != DLL_PROCESS_ATTACH {
        return 1;
    }
    ensure_init();
    // SAFETY: `instance` is this DLL's module handle supplied by the Windows loader.
    unsafe { DisableThreadLibraryCalls(instance) };
    1
}

/// `D3D12CreateDevice` (`d3d12.cpp:32-68`).
///
/// # Safety
///
/// `adapter`, `riid`, and `device` are caller-supplied COM pointers (null or valid).
#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn D3D12CreateDevice(
    adapter: *mut c_void,
    minimum_feature_level: u32,
    riid: *const Guid,
    device: *mut *mut c_void,
) -> Hresult {
    let _guard = dxmt_core::ffi::AbortOnPanic;
    ensure_init();
    if minimum_feature_level < D3D_FEATURE_LEVEL_11_0 {
        return E_INVALIDARG;
    }
    // SAFETY: caller forwards export arguments.
    unsafe { device::factory::create(adapter, minimum_feature_level, riid, device) }
}

/// `D3D12GetInterface` (`d3d12.cpp:71-74`).
///
/// # Safety
///
/// `rclsid`, `riid`, and `debug` are caller-supplied pointers (null or valid).
#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn D3D12GetInterface(
    _rclsid: *const Guid,
    riid: *const Guid,
    _debug: *mut *mut c_void,
) -> Hresult {
    let _guard = dxmt_core::ffi::AbortOnPanic;
    ensure_init();
    // SAFETY: `Guid::read` checks for null and uses `read_unaligned`.
    if let Some(iid) = unsafe { Guid::read(riid) } {
        crate::warn!("D3D12GetInterface: Unknown interface query {iid}");
    }
    E_NOINTERFACE
}

/// `D3D12GetDebugInterface` (`d3d12.cpp:77-80`).
///
/// # Safety
///
/// `riid` and `debug` are caller-supplied pointers (null or valid).
#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn D3D12GetDebugInterface(
    riid: *const Guid,
    _debug: *mut *mut c_void,
) -> Hresult {
    let _guard = dxmt_core::ffi::AbortOnPanic;
    ensure_init();
    // SAFETY: `Guid::read` checks for null and uses `read_unaligned`.
    if let Some(iid) = unsafe { Guid::read(riid) } {
        crate::warn!("D3D12GetDebugInterface: Unknown interface query {iid}");
    }
    E_NOINTERFACE
}

/// `D3D12CreateRootSignatureDeserializer` (`d3d12_root_signature.cpp:331-345`).
///
/// # Safety
///
/// `src_data`, `deserializer_interface`, and `deserializer` are caller-supplied pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn D3D12CreateRootSignatureDeserializer(
    src_data: *const c_void,
    src_data_size_in_bytes: usize,
    deserializer_interface: *const Guid,
    deserializer: *mut *mut c_void,
) -> Hresult {
    let _guard = dxmt_core::ffi::AbortOnPanic;
    ensure_init();
    // SAFETY: caller forwards export arguments.
    unsafe {
        root_signature::deserializer::create_deserializer(
            src_data,
            src_data_size_in_bytes,
            deserializer_interface,
            deserializer,
        )
    }
}

/// `D3D12CreateVersionedRootSignatureDeserializer` (`d3d12_root_signature.cpp:312-328`).
///
/// # Safety
///
/// `src_data`, `deserializer_interface`, and `deserializer` are caller-supplied pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn D3D12CreateVersionedRootSignatureDeserializer(
    src_data: *const c_void,
    src_data_size_in_bytes: usize,
    deserializer_interface: *const Guid,
    deserializer: *mut *mut c_void,
) -> Hresult {
    let _guard = dxmt_core::ffi::AbortOnPanic;
    ensure_init();
    // SAFETY: caller forwards export arguments.
    unsafe {
        root_signature::deserializer::create_versioned_deserializer(
            src_data,
            src_data_size_in_bytes,
            deserializer_interface,
            deserializer,
        )
    }
}

/// `D3D12SerializeRootSignature` (`d3d12_root_signature.cpp:229-243`).
///
/// # Safety
///
/// `root_signature`, `blob`, and `error_blob` are caller-supplied pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn D3D12SerializeRootSignature(
    root_signature: *const c_void,
    version: u32,
    blob: *mut *mut c_void,
    error_blob: *mut *mut c_void,
) -> Hresult {
    let _guard = dxmt_core::ffi::AbortOnPanic;
    ensure_init();
    // SAFETY: caller forwards export arguments.
    unsafe {
        root_signature::deserializer::serialize_root_signature(
            root_signature,
            version,
            blob,
            error_blob,
        )
    }
}

/// `D3D12SerializeVersionedRootSignature` (`d3d12_root_signature.cpp:200-226`).
///
/// # Safety
///
/// `root_signature`, `blob`, and `error_blob` are caller-supplied pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system-unwind" fn D3D12SerializeVersionedRootSignature(
    root_signature: *const c_void,
    blob: *mut *mut c_void,
    error_blob: *mut *mut c_void,
) -> Hresult {
    let _guard = dxmt_core::ffi::AbortOnPanic;
    ensure_init();
    // SAFETY: caller forwards export arguments.
    unsafe {
        root_signature::deserializer::serialize_versioned_root_signature(
            root_signature,
            blob,
            error_blob,
        )
    }
}

#[cfg(test)]
mod tests;
