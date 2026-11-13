//! A core device on `dxmt-fakes` for the recorder's tests, and what its queue's commits captured.

use dxmt_fakes as _;
use winemetal_sys::Obj;

use super::{CoreDevice, CoreTextureDesc, TextureKind, queue_profile};

/// A core device on the made-up handle `handle` with a 64x32 back buffer and a window.
pub fn core_on_fakes(handle: u64) -> CoreDevice {
    // SAFETY: a made-up handle, which only the fakes see.
    let mtl = unsafe { Obj::adopt(handle) }.expect("a non-zero handle");
    let mut core = CoreDevice::new(mtl, queue_profile());
    core.attach(1, 64, 32);
    core
}

/// The index of the first captured call equal to `line`; panics with every call when none is.
pub fn at(calls: &[String], line: &str) -> usize {
    calls
        .iter()
        .position(|c| c == line)
        .unwrap_or_else(|| panic!("no {line:?} in {calls:#?}"))
}

/// Runs `f` on a thread of its own and fails the test when it has not returned within 10 s.
pub fn returns_within_10s(f: impl FnOnce() + Send + 'static) {
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        f();
        let _ = done.send(());
    });
    finished
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("returns within 10 s");
}

/// A square `size` render-target texture's description: `levels` levels, `texel_bytes` a texel.
pub const fn target_desc(
    pixel_format: u32,
    kind: TextureKind,
    size: u32,
    levels: u32,
    texel_bytes: u32,
) -> CoreTextureDesc {
    CoreTextureDesc {
        pixel_format,
        kind,
        width: size,
        height: size,
        depth: 1,
        levels,
        view_twins: false,
        block: (1, 1, texel_bytes),
        render_target: true,
    }
}

/// The handles (`#<n>`) of the textures `calls` shows made, in order.
pub fn textures_made(calls: &[String]) -> Vec<String> {
    calls
        .iter()
        .filter_map(|c| c.strip_prefix("MTLDevice_newTexture "))
        .filter_map(|c| c.split(' ').next())
        .map(str::to_owned)
        .collect()
}

/// Each render encoder's colour target in `calls`, in order: its handle (`#<n>`) and its size.
pub fn targets(calls: &[String]) -> Vec<(String, String)> {
    calls
        .iter()
        .filter_map(|c| c.strip_prefix("  target0 "))
        .filter_map(|c| c.split_once(' '))
        .map(|(handle, size)| (handle.to_owned(), size.to_owned()))
        .collect()
}
