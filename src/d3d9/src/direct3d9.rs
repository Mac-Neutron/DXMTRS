use core::ffi::c_void;
use std::sync::{
    Arc, LazyLock, OnceLock,
    atomic::{AtomicU32, Ordering, fence},
};

use log::{error, info, trace, warn};
use mtld3d_core::{
    caps,
    config::{CursorScale, Mtld3dConfig},
    display_mode::{
        ModeRequest, drop_sizes_above_panel, drop_unscalable_sizes, effective_refresh,
        listed_modes, physical_extent, select_mode_sizes, served_modes, served_sizes,
    },
    format_probe::FormatProbeKey,
    multisample,
};
use mtld3d_shared::{InPtr, OutPtr, ValueIn, VtableThis, mtl::DeviceCapsFlags};
use mtld3d_types::{
    D3DADAPTER_IDENTIFIER9, D3DCAPS9, D3DDEVTYPE_HAL, D3DDISPLAYMODE, D3DDISPLAYMODEEX,
    D3DDISPLAYMODEEX_SIZE, D3DDISPLAYMODEFILTER, D3DDISPLAYROTATION_IDENTITY, D3DFMT_A8B8G8R8,
    D3DFMT_A8R8G8B8, D3DFMT_ATI1, D3DFMT_ATI2, D3DFMT_DF16, D3DFMT_DF24, D3DFMT_DXT1, D3DFMT_DXT2,
    D3DFMT_DXT3, D3DFMT_DXT4, D3DFMT_DXT5, D3DFMT_L8, D3DFMT_R5G6B5, D3DFMT_R8G8B8, D3DFMT_RESZ,
    D3DFMT_UYVY, D3DFMT_X8B8G8R8, D3DFMT_X8R8G8B8, D3DFMT_YUY2, D3DMULTISAMPLE_NONE,
    D3DMULTISAMPLE_NONMASKABLE, D3DOK_NOAUTOGEN, D3DPRESENT_PARAMETERS, D3DRTYPE_CUBETEXTURE,
    D3DRTYPE_INDEXBUFFER, D3DRTYPE_SURFACE, D3DRTYPE_TEXTURE, D3DRTYPE_VERTEXBUFFER,
    D3DRTYPE_VOLUME, D3DRTYPE_VOLUMETEXTURE, D3DSCANLINEORDERING_INTERLACED,
    D3DSCANLINEORDERING_PROGRESSIVE, D3DUSAGE_AUTOGENMIPMAP, D3DUSAGE_DEPTHSTENCIL,
    D3DUSAGE_DYNAMIC, D3DUSAGE_QUERY_POSTPIXELSHADER_BLENDING, D3DUSAGE_QUERY_SRGBREAD,
    D3DUSAGE_QUERY_SRGBWRITE, D3DUSAGE_QUERY_VERTEXTEXTURE, D3DUSAGE_RENDERTARGET, Guid,
    IDirect3D9ExVtbl, IDirect3D9Vtbl, LUID,
};

use super::{
    D3D_OK, D3DERR_INVALIDCALL, D3DERR_NOTAVAILABLE, LOG_TARGET, device::Direct3DDevice9,
    fullscreen::Rect, null_out, stage_bindings::STAGE_COUNT,
};

// The display-mode table behind GetAdapterModeCount / EnumAdapterModes /
// GetAdapterDisplayMode and the fullscreen mode-set. Built once, on the first
// enumeration call or the first fullscreen device, from the Win32 mode list
// (`EnumDisplaySettingsW`, see `build_adapter_modes`): a fullscreen device
// sets the mode a game picks through user32, so the list a game picks from
// has to be the list user32 validates against. Its desktop entry is the
// desktop mode at the time the table was built and backs the current
// adapter display mode, which is why the table is forced before the first
// mode-set. What an interface enumerates from it follows that interface's
// `display.maxResolutions`.
static ADAPTER_MODES: LazyLock<AdapterModes> = LazyLock::new(build_adapter_modes);

// Adapter color formats enumerated. X8R8G8B8 = "32-bit" in most game UIs
// (32-bit container, 24 useful color bits), R5G6B5 = "16-bit".
// A2R10G10B10 deliberately excluded — CAMetalLayer is hardcoded BGRA8;
// advertising 10-bit would silently downgrade an HDR opt-in.
const ADAPTER_FORMATS: &[u32] = &[D3DFMT_X8R8G8B8, D3DFMT_R5G6B5];

/// Sub-target for the display-enumeration diagnostic probes.
///
/// Confirms which `IDirect3D9` enumeration endpoints a given game actually
/// exercises at video-menu open time. Permanent probe (zero-cost when off);
/// `RUST_LOG=mtld3d::d3d9::display=trace` opts in. Useful for distinguishing
/// games whose video-menu dropdowns are D3D9-driven vs Win32-driven (Wine's
/// `EnumDisplaySettings` → macdrv → `CGDisplayCopyAllDisplayModes`).
const DISPLAY_TRACE_TARGET: &str = "mtld3d::d3d9::display";

/// The GDI name of the primary display, the one adapter 0 drives.
const PRIMARY_DISPLAY_NAME: &str = "\\\\.\\DISPLAY1";

/// The adapter display-mode format (`D3DFMT_*`) — the format `GetAdapterDisplayMode` reports.
///
/// A windowed back buffer requested as `D3DFMT_UNKNOWN` resolves to this.
pub fn adapter_display_format() -> u32 {
    ADAPTER_MODES.desktop.format
}

/// The adapter's display mode right now, as `GetAdapterDisplayMode` reports it.
///
/// Read live from Win32 rather than from the cached table: a fullscreen
/// device (ours or another process's) sets the mode, and native answers
/// with whatever is current, which is also what `GetMonitorInfo` derives its
/// rect from. The format is the table's, the one colour format the desktop
/// is advertised at; a failed query falls back to the table's desktop entry.
pub fn current_adapter_display_mode() -> D3DDISPLAYMODE {
    let desktop = ADAPTER_MODES.desktop;
    crate::fullscreen::current_display_mode().map_or(desktop, |mode| D3DDISPLAYMODE {
        width: mode.width,
        height: mode.height,
        refresh_rate: effective_refresh(mode.refresh_hz, desktop.refresh_rate),
        format: desktop.format,
    })
}

/// The display mode a device or its implicit swap chain reports.
///
/// A fullscreen device owns the mode, so the honored back-buffer size is
/// the current mode; the refresh rate is the requested one, or the host's
/// when the request left it zero. A windowed device never owns the mode and
/// reports the desktop's, exactly like native D3D9.
pub fn reported_display_mode(pp: &D3DPRESENT_PARAMETERS) -> D3DDISPLAYMODE {
    if pp.windowed == 0 {
        let refresh_rate = if pp.full_screen_refresh_rate_in_hz != 0 {
            pp.full_screen_refresh_rate_in_hz
        } else {
            ADAPTER_MODES.desktop.refresh_rate
        };
        D3DDISPLAYMODE {
            width: pp.back_buffer_width,
            height: pp.back_buffer_height,
            refresh_rate,
            format: D3DFMT_X8R8G8B8,
        }
    } else {
        current_adapter_display_mode()
    }
}

/// The mode table: what a fullscreen device may set and what games enumerate.
struct AdapterModes {
    /// Every size a fullscreen request may set, desktop first.
    ///
    /// Win32's list under [`select_mode_sizes`]' filters, less the sizes
    /// [`drop_unscalable_sizes`] and [`drop_sizes_above_panel`] leave out: a
    /// subset of what user32 accepts a mode-set for.
    settable: Vec<(u32, u32)>,
    /// Every mode Win32 lists at a settable size, as `((width, height), refresh_hz)`.
    ///
    /// The desktop's colour depth only; a rate Win32 leaves unset reads as
    /// the desktop's. [`served_modes`] picks what an interface serves from it.
    modes: Vec<((u32, u32), u32)>,
    /// The desktop mode when the table was built, the adapter display mode's fallback.
    desktop: D3DDISPLAYMODE,
}

/// The sizes games enumerate under a bound of `max` sizes, 0 for none, smallest first.
///
/// Shared by `EnumAdapterModes` and the `EnumDisplaySettings` redirect
/// (`mode_list_hook`), so both menu paths show one list.
pub fn served_sizes_for(max: u32) -> Vec<(u32, u32)> {
    served_sizes(
        &ADAPTER_MODES.settable,
        usize::try_from(max).unwrap_or(usize::MAX),
    )
}

fn build_adapter_modes() -> AdapterModes {
    // Both the host mode and the candidates come from the Win32 view
    // (`EnumDisplaySettingsW` → win32u), NOT from `NSScreen` or a table of
    // our own: win32u validates a fullscreen device's `ChangeDisplaySettingsW`
    // against this view and derives `GetMonitorInfoW` from it, so a mode
    // enumerated here is one the device can set and one that agrees with the
    // window-management side on every display. An `NSScreen` read disagreed
    // by the Retina factor on displays where the two scale differently (a CI
    // runner's virtual display), splitting `GetAdapterDisplayMode` from the
    // monitor rect.
    let host = crate::fullscreen::current_display_mode();
    let host_w = host.map_or(1920, |mode| mode.width);
    let host_h = host.map_or(1080, |mode| mode.height);
    let host_hz = effective_refresh(host.map_or(0, |mode| mode.refresh_hz), 60);
    let host_bpp = host.map(|mode| mode.bits_per_pel);

    // Win32 lists every size once per colour depth; the desktop's depth is
    // the one a game gets, so the others only repeat sizes.
    let enumerated = crate::fullscreen::enumerate_display_modes();
    let candidates = enumerated
        .iter()
        .filter(|mode| host_bpp.is_none_or(|bpp| mode.bits_per_pel == bpp))
        .map(|mode| (mode.width, mode.height));
    let physical = physical_extent((host_w, host_h), candidates.clone());
    let mut settable = select_mode_sizes((host_w, host_h), candidates);
    // After a mode-set win32u rescales the monitor by a ratio whose terms it
    // packs into 16 bits each, and some Wine builds abort the process when a
    // term does not fit. Those builds cannot be told apart from here, so the
    // sizes are left out on every Wine: a fullscreen request for one follows
    // the window rather than setting the mode.
    let dpi = crate::fullscreen::system_dpi();
    let unscalable = drop_unscalable_sizes(&mut settable, physical, dpi);
    if !unscalable.is_empty() {
        mtld3d_shared::log_once_info!(
            target: LOG_TARGET,
            "adapter modes: {} sizes left out, win32u cannot represent the scale from them onto \
             the {}x{} display at {dpi} dpi in 16 bits: {unscalable:?}",
            unscalable.len(),
            physical.0,
            physical.1
        );
    }
    // On a scaled desktop Win32 lists sizes up to the backing store; the
    // panel shows no more pixels than its own, so a driver lists none above.
    let panel = crate::rt::device::info::panel_size();
    let above_panel = drop_sizes_above_panel(&mut settable, panel);
    if !above_panel.is_empty() {
        mtld3d_shared::log_once_info!(
            target: LOG_TARGET,
            "adapter modes: {} sizes larger than the {}x{} panel left out: {above_panel:?}",
            above_panel.len(),
            panel.0,
            panel.1
        );
    }
    if enumerated.is_empty() {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "EnumDisplaySettingsW enumerated no display modes; EnumAdapterModes serves the \
             current mode only",
        );
    }
    // Every rate Win32 lists for a settable size is served: a game may filter
    // its menu by refresh rate, and the panel's own rate need not pass where
    // 60 Hz does.
    let listed = enumerated
        .iter()
        .filter(|mode| host_bpp.is_none_or(|bpp| mode.bits_per_pel == bpp))
        .map(|mode| ((mode.width, mode.height), mode.refresh_hz));
    let mut modes = listed_modes(listed, ((host_w, host_h), host_hz));
    modes.retain(|(size, _)| settable.contains(size));

    info!(
        target: LOG_TARGET,
        "adapter modes: host {host_w}x{host_h}@{host_hz}Hz on a {}x{} display ({}x{} panel); {} \
         sizes settable of {} enumerated modes",
        physical.0,
        physical.1,
        panel.0,
        panel.1,
        settable.len(),
        enumerated.len(),
    );
    AdapterModes {
        settable,
        modes,
        desktop: D3DDISPLAYMODE {
            width: host_w,
            height: host_h,
            refresh_rate: host_hz,
            format: ADAPTER_FORMATS[0],
        },
    }
}

/// The modes an interface bounded to `max` sizes serves, 0 for no bound.
///
/// Logs the list once per interface, the first time it is asked for.
fn served_table(max: u32) -> Vec<((u32, u32), u32)> {
    let sizes = served_sizes_for(max);
    let modes = served_modes(ADAPTER_MODES.modes.iter().copied(), &sizes);
    let bound = if max == 0 {
        "no bound".to_owned()
    } else {
        format!("display.maxResolutions = {max}")
    };
    info!(
        target: LOG_TARGET,
        "adapter modes served ({bound}): {} sizes, {} modes per format: {sizes:?}",
        sizes.len(),
        modes.len()
    );
    modes
}

/// The display mode a fullscreen request asks the device to set.
///
/// `Some` for a settable mode, which is one user32 accepts by construction
/// (the mode table is seeded from its list); `None` for a request that is
/// no display mode, which follows the window instead. The
/// refresh rate is the game's, 0 for "any". Reading the table here also
/// builds it before the first mode-set, so its desktop entry is the
/// desktop's.
pub fn fullscreen_mode_request(pp: &D3DPRESENT_PARAMETERS) -> Option<ModeRequest> {
    is_settable_mode(pp.back_buffer_width, pp.back_buffer_height).then_some(ModeRequest {
        width: pp.back_buffer_width,
        height: pp.back_buffer_height,
        refresh_hz: pp.full_screen_refresh_rate_in_hz,
    })
}

/// The vtable every interface carries, extended or not.
///
/// A plain interface hands out the same table: its base slots are the
/// `IDirect3D9` contract, and a caller that never queried `IID_IDirect3D9Ex`
/// has no reason to reach past them. An extended entry point called through a
/// plain interface still runs; `CreateDeviceEx` there makes a plain device,
/// since the device takes its kind from the interface that creates it.
static DIRECT3D9_VTBL: IDirect3D9ExVtbl = IDirect3D9ExVtbl {
    base: IDirect3D9Vtbl {
        query_interface: d3d9_query_interface,
        add_ref: d3d9_add_ref,
        release: d3d9_release,
        register_software_device: d3d9_register_software_device,
        get_adapter_count: d3d9_get_adapter_count,
        get_adapter_identifier: d3d9_get_adapter_identifier,
        get_adapter_mode_count: d3d9_get_adapter_mode_count,
        enum_adapter_modes: d3d9_enum_adapter_modes,
        get_adapter_display_mode: d3d9_get_adapter_display_mode,
        check_device_type: d3d9_check_device_type,
        check_device_format: d3d9_check_device_format,
        check_device_multi_sample_type: d3d9_check_device_multi_sample_type,
        check_depth_stencil_match: d3d9_check_depth_stencil_match,
        check_device_format_conversion: d3d9_check_device_format_conversion,
        get_device_caps: d3d9_get_device_caps,
        get_adapter_monitor: d3d9_get_adapter_monitor,
        create_device: d3d9_create_device,
    },
    get_adapter_mode_count_ex: d3d9_get_adapter_mode_count_ex,
    enum_adapter_modes_ex: d3d9_enum_adapter_modes_ex,
    get_adapter_display_mode_ex: d3d9_get_adapter_display_mode_ex,
    create_device_ex: d3d9_create_device_ex,
    get_adapter_luid: d3d9_get_adapter_luid,
};

// ── IDirect3D9 COM object ──

#[repr(C)]
pub struct Direct3D9 {
    vtbl: *const IDirect3D9Vtbl,
    /// The public refcount, changed from any thread that holds the interface or a device it made.
    refcount: AtomicU32,
    inner: Box<Direct3D9Inner>,
}

/// What an `IDirect3D9` owns: the configuration it resolved at `Direct3DCreate9`.
///
/// Shared with every device the interface creates, so it is reference
/// counted rather than borrowed from the interface, whose `Release` can come
/// before a device's.
pub struct Direct3D9Inner {
    config: Arc<Mtld3dConfig>,
    /// Made by `Direct3DCreate9Ex`: the interface answers `IID_IDirect3D9Ex`.
    ///
    /// Every device it creates is extended, whichever create made it.
    extended: bool,
    /// The modes `GetAdapterModeCount` / `EnumAdapterModes` serve, as `((width, height), hz)`.
    ///
    /// Built from the mode table on the first enumeration, under this
    /// interface's `display.maxResolutions`; each adapter format serves the
    /// same list.
    served: OnceLock<Vec<((u32, u32), u32)>>,
}

impl Direct3D9 {
    pub fn new(config: Arc<Mtld3dConfig>, extended: bool) -> Self {
        Self {
            vtbl: &raw const DIRECT3D9_VTBL.base,
            refcount: AtomicU32::new(1),
            inner: Box::new(Direct3D9Inner {
                config,
                extended,
                served: OnceLock::new(),
            }),
        }
    }

    /// The modes this interface's enumeration serves, smallest size first.
    fn served_modes(&self) -> &[((u32, u32), u32)] {
        self.inner
            .served
            .get_or_init(|| served_table(self.inner.config.max_resolutions))
    }

    /// The `index`-th mode this interface serves in `format`, `None` past the end.
    fn served_mode(&self, format: u32, index: u32) -> Option<D3DDISPLAYMODE> {
        let &((width, height), refresh_rate) =
            self.served_modes().get(usize::try_from(index).ok()?)?;
        Some(D3DDISPLAYMODE {
            width,
            height,
            refresh_rate,
            format,
        })
    }

    /// Whether `Direct3DCreate9Ex` made this interface.
    pub const fn is_extended(&self) -> bool {
        self.inner.extended
    }

    /// Take one more reference; the count after it.
    fn add_ref(&self) -> u32 {
        self.refcount.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// The configuration this interface resolved at `Direct3DCreate9`.
    pub const fn config(&self) -> &Arc<Mtld3dConfig> {
        &self.inner.config
    }
}

// Display formats accepted as the adapter_format / back_buffer_format pair.
const fn is_display_format(fmt: u32) -> bool {
    // Adapter/display formats only — alpha formats (A8R8G8B8) can be a
    // backbuffer but never a display mode, so they are excluded here.
    matches!(fmt, D3DFMT_X8R8G8B8 | D3DFMT_R5G6B5)
}

/// 32-bit RGB colour family whose members interconvert at present time.
///
/// (X8R8G8B8 / A8R8G8B8 — the alpha channel is ignored on present).
const fn is_32bit_rgb(fmt: u32) -> bool {
    matches!(fmt, D3DFMT_X8R8G8B8 | D3DFMT_A8R8G8B8)
}

/// Whether a fullscreen backbuffer of `src` can be presented to a `dst` display format.
///
/// The same format, or another member of the same 32-bit colour family. This
/// is the fullscreen rule only: the spec allows no present-time conversion
/// there, the display and backbuffer formats must match ignoring alpha.
/// Windowed mode goes through [`is_format_conversion_supported`] instead.
const fn is_present_compatible(src: u32, dst: u32) -> bool {
    src == dst || (is_32bit_rgb(src) && is_32bit_rgb(dst))
}

/// Formats `StretchRect` can read as the source of a format conversion.
///
/// The render-quad path samples any colour format the device can render and
/// decodes the packed 4:2:2 (`YUY2` / `UYVY`) and planar 4:2:0 (`YV12` /
/// `NV12`) YUV formats in its fragment function; the offscreen-plain CPU
/// converter covers the YUV members too.
const fn is_conversion_source(fmt: u32) -> bool {
    mtld3d_core::format::is_render_target_format(fmt)
        || mtld3d_core::stretch_rect::is_packed_yuv(fmt)
        || mtld3d_core::stretch_rect::is_planar_yuv(fmt)
}

/// `CheckDeviceFormatConversion`: whether `StretchRect` converts `src` into `dst`.
///
/// A format always converts to itself (the identity rows hold for any code,
/// mapped or not). Otherwise the source must be something the blit can read
/// ([`is_conversion_source`] — a pure predicate, since sources are sampled on
/// every device) and the destination a colour format this device renders into
/// (`is_render_target_format_on_device` — the render-quad writes into it).
/// Windowed `CheckDeviceType` shares this predicate on purpose: the runtime
/// asserts `CheckDeviceType(windowed) == CheckDeviceFormat(RT, bb) &&
/// CheckDeviceFormatConversion(bb, display)`, so the two must never drift.
fn is_format_conversion_supported(src: u32, dst: u32, expand_packed16: bool) -> bool {
    if src == dst {
        return true;
    }
    // L8 renders, but Windows drivers answer NOTAVAILABLE for a conversion into
    // it, and as a source it keeps the identity-only answer.
    if src == D3DFMT_L8 || dst == D3DFMT_L8 {
        return false;
    }
    is_conversion_source(src) && is_render_target_format_on_device(dst, expand_packed16)
}

// Formats the texture pool can sample or receive uploads in.
//
// Derived from the create path rather than listed: `CreateTexture` accepts
// exactly the formats `map_d3d_format` maps, so an independent list here
// drifts — the answer then disagrees with what a create actually does, and
// callers that probe first (every engine that picks a scene format from
// `CheckDeviceFormat`) take a fallback path for a format we support. ATI1 is
// one deliberate odd one, a creatable BC4 texture. The YUV formats are the
// exceptions in the other direction: the packed pair maps to RG8 storage that
// nothing decodes when it is sampled, and the planar pair has no mapping at
// all, so neither is a texture, and `is_plain_surface_format` admits both as
// surfaces alone.
//
// The FOURCC sampleable-depth formats (`INTZ` / `DF24` / `DF16`) belong here
// too, and are not colour mappings: D3D9-era engines (incl. WoW's CSM path)
// probe them via `CheckDeviceFormat(rtype=TEXTURE, fmt=INTZ)` without
// `USAGE_DEPTHSTENCIL` in the query, and only enable hardware shadow mapping
// when at least one comes back available.
const fn is_texture_format(fmt: u32) -> bool {
    // ATI1 is the one carve-out, for the same reason it is excluded from the
    // cube answer: it creates, but its lock reports the BC4 block pitch
    // (8 bytes per 4x4 block) where D3D9 reports ATI1N a byte per pixel, so
    // advertising it would hand callers a pitch they cannot use.
    (mtld3d_core::format::is_mapped_color_format(fmt)
        && !matches!(fmt, D3DFMT_ATI1 | D3DFMT_YUY2 | D3DFMT_UYVY))
        || mtld3d_core::format::is_raw_depth_fetch_format(fmt)
}

/// Formats `CreateOffscreenPlainSurface` accepts, the `D3DRTYPE_SURFACE` answer without usage.
///
/// Every texture format but ATI2, plus the YUV formats, which have no
/// sampling path: the packed 4:2:2 pair as an offscreen plain in any pool,
/// and the planar 4:2:0 pair as a default-pool one only. Both are lockable
/// `StretchRect` sources, so every texture-typed query for them stays
/// refused. D3D9 refuses an ATI2 plain in the default and system-memory
/// pools and creates one in the scratch pool alone, which a query, having no
/// pool, answers no for.
const fn is_plain_surface_format(fmt: u32) -> bool {
    (is_texture_format(fmt) && fmt != D3DFMT_ATI2)
        || mtld3d_core::stretch_rect::is_packed_yuv(fmt)
        || mtld3d_core::stretch_rect::is_planar_yuv(fmt)
}

/// Sampleable cube colour formats backed by `MTLTextureTypeCube`.
///
/// ATI1 requires extension-specific cube lock semantics that are not
/// implemented. Packed YUV has no shader sampling path, and depth cube maps
/// are not implemented.
const fn is_cube_texture_format(fmt: u32) -> bool {
    mtld3d_core::format::is_mapped_color_format(fmt)
        && !matches!(fmt, D3DFMT_ATI1 | D3DFMT_YUY2 | D3DFMT_UYVY)
        && !is_depth_stencil_format(fmt)
}

/// `mtld3d_core::format::is_render_target_format_device`, keyed on the config.
///
/// The form every advertisement arm and create gate that concerns actually
/// rendering into a surface uses. `expand_packed16` is the interface's
/// `intel.expandPacked16`; the pure format family is the core predicate, and
/// callers that need the device-independent answer (the conversion SOURCE
/// side) reach for that one directly.
pub fn is_render_target_format_on_device(fmt: u32, expand_packed16: bool) -> bool {
    mtld3d_core::format::is_render_target_format_device(
        fmt,
        native_packed16_supported(expand_packed16),
    )
}

/// `map_d3d_format_device` with this device's packed 16-bit answer applied.
///
/// The form every create path that freezes a Metal format into a texture
/// must use; layout-only callers (Lock pitch, staging sizing) may keep the
/// plain `map_d3d_format`, whose source-layout fields are identical.
/// `expand_packed16` is the interface's `intel.expandPacked16`.
pub fn map_for_device(
    format: u32,
    expand_packed16: bool,
) -> Option<mtld3d_core::format::FormatMapping> {
    mtld3d_core::format::map_d3d_format_device(format, native_packed16_supported(expand_packed16))
}

/// Depth-stencil formats: the ones the capability answers offer.
///
/// Read from `format::is_advertised_depth_format`, so a depth format the
/// answers offer is one every create accepts. It includes the FOURCC
/// sampleable-depth formats (`INTZ` / `DF24` / `DF16`), created with
/// `USAGE_DEPTHSTENCIL`, bound as the depth target during a caster pass and
/// sampled as a depth texture in the receiver pass, and `D24FS8`. It leaves
/// out `D15S1` and `D24X4S4`, which nothing creates, and the two lockable
/// formats, which the auto depth-stencil and a depth-stencil texture still
/// take without the `LockRect` the format promises.
pub const fn is_depth_stencil_format(fmt: u32) -> bool {
    mtld3d_core::format::is_advertised_depth_format(fmt)
}

// D3D9 colour formats whose Metal counterpart has an sRGB twin. Mirror of
// the PE-side `PixelFormat::srgb_twin()` table in `src/d3d9/shared/src/mtl.rs`
// — drives the answer `CheckDeviceFormat` returns for
// `D3DUSAGE_QUERY_SRGBREAD`. Adding a new linear/sRGB pair to `PixelFormat`
// requires extending this list too. The write side is not this question: a
// colour target without a twin encodes through the pixel shader's OETF
// variant instead, so `D3DUSAGE_QUERY_SRGBWRITE` follows render-target
// capability (`mtld3d_core::format::supports_usage_query`).
//
// `R8G8B8` belongs here even though it is widened on upload: its backing is
// `Bgra8Unorm` on every device, so the eager twin view exists and the decode
// is real. The packed 16-bit formats stay out because their backing is
// device-dependent and this predicate is pure.
//
// The list follows the Metal format, not the D3D9 name: DXT2 and DXT4 are the
// BC2 and BC3 block encodings of DXT3 and DXT5 under another content
// convention, take the same twin view at create, and answer as they do.
const fn has_srgb_twin(fmt: u32) -> bool {
    matches!(
        fmt,
        D3DFMT_A8R8G8B8
            | D3DFMT_X8R8G8B8
            | D3DFMT_A8B8G8R8
            | D3DFMT_X8B8G8R8
            | D3DFMT_R8G8B8
            | D3DFMT_DXT1
            | D3DFMT_DXT2
            | D3DFMT_DXT3
            | D3DFMT_DXT4
            | D3DFMT_DXT5
    )
}

// Formats for which `D3DUSAGE_QUERY_SRGBREAD` (the `D3DSAMP_SRGBTEXTURE`
// sampling decode) is honoured: every format with an sRGB twin. The unix
// side creates the twin view eagerly at `create_texture` time and the
// draw-time bind selects it whenever the stage's sampler sets
// `D3DSAMP_SRGBTEXTURE=1`, so the advertisement is backed by a real
// hardware decode. Keep in lock-step with `has_srgb_twin` — Source-engine
// games gate their entire gamma-correct pipeline on the A8R8G8B8
// SRGBREAD|SRGBWRITE probe and fall back to an untested shader-gamma path
// (black lightmaps in Half-Life 2) when it fails.
const fn has_srgb_read_decode(fmt: u32) -> bool {
    has_srgb_twin(fmt)
}

// ── IUnknown implementation (IDirect3D9) ──

extern "system" fn d3d9_query_interface(
    this: *mut c_void,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> i32 {
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per the IDirect3D9 ABI.
    let extended = unsafe { InPtr::<Direct3D9>::opt(this) }.is_some_and(|d3d| d3d.is_extended());
    let accepted: &[Guid] = if extended {
        &[
            mtld3d_types::IID_IUNKNOWN,
            mtld3d_types::IID_IDIRECT3D9,
            mtld3d_types::IID_IDIRECT3D9EX,
        ]
    } else {
        &[mtld3d_types::IID_IUNKNOWN, mtld3d_types::IID_IDIRECT3D9]
    };
    // SAFETY: vtable thunk; `this`, `riid` and `ppv` are the caller's per the
    // IUnknown::QueryInterface ABI.
    unsafe {
        crate::com_ref::com_query_interface(this, riid, ppv, accepted, d3d9_add_ref, "IDirect3D9")
    }
}

extern "system" fn d3d9_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: D3D9 AddRef — null `this` is UB per spec; we preserve the
    // crash semantic so refcount miscounts surface as a null-deref.
    // SAFETY: IDirect3D9 IUnknown thunk; D3D9 ABI guarantees `this` is *mut Direct3D9.
    let wrap = unsafe { VtableThis::<Direct3D9>::new(this) };
    wrap.add_ref()
}

extern "system" fn d3d9_release(this: *mut c_void) -> u32 {
    // SAFETY: D3D9 Release — same contract as AddRef above.
    // SAFETY: IDirect3D9 IUnknown thunk; D3D9 ABI guarantees `this` is *mut Direct3D9.
    let wrap = unsafe { VtableThis::<Direct3D9>::new(this) };
    // The interface is not tied to a device's lock: every device it created
    // takes and gives back references on it from its own threads, so the
    // count is atomic. `checked_sub` only keeps a `Release` past zero from
    // wrapping the count; it answers 0 and frees nothing.
    let Ok(previous) = wrap
        .refcount
        .try_update(Ordering::Release, Ordering::Relaxed, |count| {
            count.checked_sub(1)
        })
    else {
        return 0;
    };
    if previous == 1 {
        // Every other thread's last use of the interface happened before its
        // own decrement, which the `Release` ordering publishes; this fence
        // makes those uses happen before the free.
        fence(Ordering::Acquire);
        // SAFETY: the count reached zero; `this` is the original
        // `Box::into_raw(Direct3D9)` allocation from `Direct3DCreate9`,
        // and no other reference can survive a zero refcount.
        drop(unsafe { Box::from_raw(this.cast::<Direct3D9>()) });
    }
    previous - 1
}

// ── IDirect3D9 methods ──

extern "system" fn d3d9_register_software_device(_this: *mut c_void, _init_fn: *mut c_void) -> i32 {
    mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET, "stub IDirect3D9::RegisterSoftwareDevice → INVALIDCALL");
    D3DERR_INVALIDCALL
}

const extern "system" fn d3d9_get_adapter_count(_this: *mut c_void) -> u32 {
    1
}

extern "system" fn d3d9_get_adapter_identifier(
    this: *mut c_void,
    adapter: u32,
    _flags: u32,
    id_out: *mut D3DADAPTER_IDENTIFIER9,
) -> i32 {
    trace!(target: LOG_TARGET, "IDirect3D9::GetAdapterIdentifier(adapter={adapter})");
    if adapter != 0 {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if id_out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: D3DADAPTER_IDENTIFIER9 is a plain-data #[repr(C)] FFI struct
    // (fixed-size buffers + u32 fields); zeroed bytes are a valid value.
    let mut id: D3DADAPTER_IDENTIFIER9 = unsafe { core::mem::zeroed() };

    id.driver[..7].copy_from_slice(b"mtld3d\0");
    id.vendor_id = 0x106B; // Apple
    // An extended interface reports level 1 without consulting the driver, as
    // D3D9Ex does; a plain one reports 0.
    id.whql_level = u32::from(d3d.is_extended());

    // GDI-style display-device name for adapter 0. D3D9 reports the adapter's
    // GDI name here; the conformance suite (and real apps enumerating adapters)
    // require it to be non-empty.
    let device_name = PRIMARY_DISPLAY_NAME.as_bytes();
    id.device_name[..device_name.len()].copy_from_slice(device_name);

    let info = device_info();
    id.description[..info.name_len].copy_from_slice(&info.name[..info.name_len]);
    // D3DADAPTER_IDENTIFIER9.device_id is u32 by D3D9 spec; mask to 16 bits.
    id.device_id = u32::try_from(info.registry_id & 0xFFFF).expect("16-bit mask fits u32");

    // `adapter.spoof`: report a consistent well-known GPU identity. Engines of
    // this era key whole render paths (depth copies, shadow filtering) off the
    // vendor id, sniff the description string for a marketing name, and gate
    // on a minimum driver version, so all of these move together.
    let spoof = match d3d.config().adapter_spoof {
        mtld3d_core::config::AdapterSpoof::None => None,
        mtld3d_core::config::AdapterSpoof::Nvidia => Some(SpoofIdentity {
            vendor: 0x10DE,
            device: 0x0611, // GeForce 8800 GT, in every launch-era device table
            description: b"NVIDIA GeForce 8800 GT\0",
            driver: b"nvd3dum.dll\0",
            // 8.17.11.9745 (a WDDM 1.1 driver, NVIDIA 197.45) as
            // LARGE_INTEGER LowPart / HighPart. The first field matters:
            // 6.x is the XP driver model, and a title running on an NT 6
            // prefix can reject or mis-parse an XP-model version.
            version: [0x000B_2611, 0x0008_0011],
        }),
        mtld3d_core::config::AdapterSpoof::Amd => Some(SpoofIdentity {
            vendor: 0x1002,
            device: 0x9440, // Radeon HD 4870
            description: b"ATI Radeon HD 4800 Series\0",
            driver: b"atiumdag.dll\0",
            // 8.17.10.1129 (a WDDM 1.1 Catalyst driver) as LARGE_INTEGER
            // LowPart / HighPart; see the NVIDIA arm for why 8.x.
            version: [0x000A_0469, 0x0008_0011],
        }),
    };
    if let Some(s) = spoof {
        id.vendor_id = s.vendor;
        id.device_id = s.device;
        id.description = [0; 512];
        id.description[..s.description.len()].copy_from_slice(s.description);
        id.driver = [0; 512];
        id.driver[..s.driver.len()].copy_from_slice(s.driver);
        id.driver_version = s.version;
        mtld3d_shared::log_once_info!(
            target: LOG_TARGET,
            "adapter.spoof: reporting vendor {:#06x} device {:#06x}", s.vendor, s.device
        );
    }

    // SAFETY: vtable out-param; `id_out` is *mut D3DADAPTER_IDENTIFIER9 per IDirect3D9 ABI,
    // non-null per the check above; alignment is not assumed.
    unsafe { OutPtr::write_opt(id_out, id) };
    0 // S_OK
}

/// Device identity + capability bits from the one process-wide `GetDeviceInfo` call.
///
/// Every device-conditional advertised cap derives from this single cached
/// answer, so all consumers (adapter identity, `GetDeviceCaps` bits, format
/// mapping) agree on the same device without re-querying the unix side.
struct CachedDeviceInfo {
    name: [u8; 256],
    name_len: usize,
    registry_id: u64,
    caps: DeviceCapsFlags,
}

/// The process-wide Metal device info, fetched once on first use.
fn device_info() -> &'static CachedDeviceInfo {
    static INFO: std::sync::LazyLock<CachedDeviceInfo> = std::sync::LazyLock::new(|| {
        if let Some(info) = crate::rt::device::query_default_device_info() {
            return CachedDeviceInfo {
                name: info.name,
                name_len: info.name_len,
                registry_id: info.registry_id,
                caps: info.caps,
            };
        }
        mtld3d_shared::log_once_warn!(
            target: crate::LOG_TARGET,
            "d3d9 core: WMTCopyAllDevices returned no Metal device"
        );
        CachedDeviceInfo {
            name: [0; 256],
            name_len: 0,
            registry_id: 0,
            caps: DeviceCapsFlags::empty(),
        }
    });
    &INFO
}

/// `PageBox` pages from the guest VM's winemetal when built with `guest-vm`.
#[cfg(feature = "guest-vm")]
pub fn install_page_allocator() {
    mtld3d_core::page_box::set_page_allocator_source(|| {
        log::info!(target: LOG_TARGET, "page boxes: pages from the guest's winemetal");
        Some(mtld3d_core::page_box::PageAllocator {
            alloc: crate::rt::guest_pages::alloc,
            free: crate::rt::guest_pages::free,
        })
    });
}

/// `PageBox` pages stay native outside `guest-vm`.
#[cfg(not(feature = "guest-vm"))]
pub const fn install_page_allocator() {}

/// Whether the Metal device can create border-colour samplers.
///
/// `GetDeviceCaps` strips `D3DPTADDRESSCAPS_BORDER` when it cannot
/// (virtualized CI devices), and the unix sampler path clamps to edge for a
/// title that ignores the cap.
pub fn sampler_border_supported() -> bool {
    device_info().caps.contains(DeviceCapsFlags::SAMPLER_BORDER)
}

/// The Metal device's boolean capability bits.
///
/// The multisample paths resolve a `(type, quality)` request against these
/// through `mtld3d_core::multisample`, so `CheckDeviceMultiSampleType` and
/// every create path answer from the same cached `GetDeviceInfo` reply.
pub fn device_caps_flags() -> DeviceCapsFlags {
    device_info().caps
}

/// Whether the packed 16-bit Metal formats exist natively on this device.
///
/// True on Apple-family GPUs; false on Intel/AMD (Mac2), where the D3D
/// formats A4R4G4B4 / R5G6B5 / A1R5G5B5 / X1R5G5B5 are backed by
/// `Bgra8Unorm` instead and widened by the GPU upload pass. `intel.expandPacked16`
/// (`expand_packed16`, the interface's setting) forces the expansion path on
/// any device so it can be exercised on Apple Silicon; it folds in here so
/// every consumer (format mapping, `CheckDeviceFormat`, create gates) flips
/// together.
pub fn native_packed16_supported(expand_packed16: bool) -> bool {
    let native = device_info()
        .caps
        .contains(DeviceCapsFlags::NATIVE_PACKED16)
        && !expand_packed16;
    if !native {
        mtld3d_shared::log_once_info!(
            target: LOG_TARGET,
            "packed 16-bit formats unavailable natively (forced={expand_packed16}): \
             A4R4G4B4/R5G6B5/A1R5G5B5/X1R5G5B5 \
             widen to BGRA8 in the GPU upload pass, 16-bit render targets are not advertised"
        );
    }
    native
}

/// Whether the Metal device filters single-precision float textures.
///
/// `MTLDevice.supports32BitFloatFiltering` covers exactly R32F / G32R32F /
/// A32B32G32R32F; `CheckDeviceFormat` answers `D3DUSAGE_QUERY_FILTER` for
/// those three with it, so an engine that probes before picking a scene
/// format takes its own fallback instead of sampling a format the device
/// point-samples. The half-float members are filterable on every family and
/// are unaffected. `intel.denyFloat32Filtering = true` (`deny`, the
/// interface's setting) forces the negative answer on any device so the path
/// can be exercised on Apple Silicon.
pub fn float32_filtering_supported(deny: bool) -> bool {
    let supported = device_info()
        .caps
        .contains(DeviceCapsFlags::FLOAT32_FILTERING)
        && !deny;
    if !supported {
        mtld3d_shared::log_once_info!(
            target: LOG_TARGET,
            "32-bit float filtering unavailable (forced={deny}): R32F/G32R32F/A32B32G32R32F \
             answer NOTAVAILABLE for D3DUSAGE_QUERY_FILTER and are sampled point-filtered"
        );
    }
    supported
}

/// One spoofed adapter identity: everything a game sniffs, kept consistent.
struct SpoofIdentity {
    vendor: u32,
    device: u32,
    description: &'static [u8],
    driver: &'static [u8],
    /// Win32 `LARGE_INTEGER` driver version as `LowPart` / `HighPart`.
    version: [u32; 2],
}

extern "system" fn d3d9_get_adapter_mode_count(
    this: *mut c_void,
    adapter: u32,
    format: u32,
) -> u32 {
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let d3d = unsafe { InPtr::<Direct3D9>::opt(this) };
    let Some(d3d) = d3d.filter(|_| adapter == 0 && is_display_format(format)) else {
        warn!(target: LOG_TARGET, "reject GetAdapterModeCount(adapter={adapter}, format={format}) → 0");
        return 0;
    };
    let count =
        u32::try_from(d3d.served_modes().len()).expect("the mode table is a small Win32 list");
    mtld3d_shared::log_once_trace_by!(
        target: DISPLAY_TRACE_TARGET,
        key: u64::from(format),
        "GetAdapterModeCount(format={format}) → {count}"
    );
    count
}

extern "system" fn d3d9_enum_adapter_modes(
    this: *mut c_void,
    adapter: u32,
    format: u32,
    mode: u32,
    display_mode: *mut c_void,
) -> i32 {
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let d3d = unsafe { InPtr::<Direct3D9>::opt(this) };
    let Some(d3d) =
        d3d.filter(|_| adapter == 0 && !display_mode.is_null() && is_display_format(format))
    else {
        warn!(
            target: LOG_TARGET,
            "reject EnumAdapterModes(adapter={adapter}, format={format}, mode={mode}) → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    };
    let Some(entry) = d3d.served_mode(format, mode) else {
        trace!(
            target: LOG_TARGET,
            "reject EnumAdapterModes(adapter={adapter}, format={format}, mode={mode}) → INVALIDCALL (out of range)"
        );
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable out-param; `display_mode` is *mut D3DDISPLAYMODE per IDirect3D9 ABI.
    unsafe { OutPtr::write_opt(display_mode.cast::<D3DDISPLAYMODE>(), entry) };
    D3D_OK
}

extern "system" fn d3d9_get_adapter_display_mode(
    _this: *mut c_void,
    adapter: u32,
    mode: *mut c_void,
) -> i32 {
    if adapter != 0 || mode.is_null() {
        warn!(target: LOG_TARGET, "reject GetAdapterDisplayMode(adapter={adapter}) → INVALIDCALL");
        return D3DERR_INVALIDCALL;
    }
    let current = current_adapter_display_mode();
    // SAFETY: vtable out-param; `mode` is *mut D3DDISPLAYMODE per IDirect3D9 ABI.
    unsafe { OutPtr::write_opt(mode.cast::<D3DDISPLAYMODE>(), current) };
    mtld3d_shared::log_once_trace_by!(
        target: DISPLAY_TRACE_TARGET,
        key: 0u64,
        "GetAdapterDisplayMode → {}x{}@{}Hz fmt={}",
        current.width, current.height, current.refresh_rate, current.format
    );
    D3D_OK
}

/// Whether an extended enumeration under `filter` lists anything.
///
/// Every listed mode is progressive, so a filter asking for interlaced modes
/// keeps none, as does a format the adapter lists no mode in.
const fn filter_lists_modes(filter: &D3DDISPLAYMODEFILTER) -> bool {
    filter.scan_line_ordering != D3DSCANLINEORDERING_INTERLACED && is_display_format(filter.format)
}

extern "system" fn d3d9_get_adapter_mode_count_ex(
    this: *mut c_void,
    adapter: u32,
    filter: *const D3DDISPLAYMODEFILTER,
) -> u32 {
    // SAFETY: vtable in-param; `filter` is null or a readable
    // `D3DDISPLAYMODEFILTER` per the IDirect3D9Ex ABI.
    let filter = unsafe { ValueIn::<D3DDISPLAYMODEFILTER>::read_opt(filter.cast()) };
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9Ex ABI.
    let d3d = unsafe { InPtr::<Direct3D9>::opt(this) };
    let listed = filter
        .as_ref()
        .is_some_and(|filter| adapter == 0 && filter_lists_modes(filter));
    let Some(d3d) = d3d.filter(|_| listed) else {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "GetAdapterModeCountEx(adapter={adapter}): no filter, another adapter, an interlaced \
             filter or a format with no modes → 0"
        );
        return 0;
    };
    u32::try_from(d3d.served_modes().len()).expect("the mode table is a small Win32 list")
}

extern "system" fn d3d9_enum_adapter_modes_ex(
    this: *mut c_void,
    adapter: u32,
    filter: *const D3DDISPLAYMODEFILTER,
    mode_index: u32,
    mode: *mut D3DDISPLAYMODEEX,
) -> i32 {
    // SAFETY: vtable in-param; `filter` is null or a readable
    // `D3DDISPLAYMODEFILTER` per the IDirect3D9Ex ABI.
    let filter = unsafe { ValueIn::<D3DDISPLAYMODEFILTER>::read_opt(filter.cast()) };
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9Ex ABI.
    let d3d = unsafe { InPtr::<Direct3D9>::opt(this) };
    let entry = filter
        .as_ref()
        .filter(|filter| adapter == 0 && filter_lists_modes(filter))
        .zip(d3d)
        .and_then(|(filter, d3d)| d3d.served_mode(filter.format, mode_index));
    let Some(entry) = entry.filter(|_| !mode.is_null()) else {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject EnumAdapterModesEx(adapter={adapter}, mode={mode_index}): no filter or out \
             slot, another adapter, an interlaced filter, or past the list → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    };
    // SAFETY: vtable out-param; `mode` is non-null (checked) and points to a
    // writable `D3DDISPLAYMODEEX` per the IDirect3D9Ex ABI.
    unsafe { OutPtr::write_opt(mode, display_mode_ex(&entry)) };
    D3D_OK
}

extern "system" fn d3d9_get_adapter_display_mode_ex(
    _this: *mut c_void,
    adapter: u32,
    mode: *mut D3DDISPLAYMODEEX,
    rotation: *mut u32,
) -> i32 {
    if adapter != 0 {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject GetAdapterDisplayModeEx(adapter={adapter}) → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable out-params; `mode` and `rotation` are null or writable
    // per the IDirect3D9Ex ABI.
    unsafe { write_display_mode_ex(mode.cast(), rotation, &current_adapter_display_mode()) }
}

extern "system" fn d3d9_get_adapter_luid(_this: *mut c_void, adapter: u32, luid: *mut LUID) -> i32 {
    /// The adapter's LUID as `(LowPart, HighPart)`, read once for the process.
    ///
    /// A machine fact latched once and immutable after: the LUID the display
    /// driver gives the primary display's adapter, or, where the driver hands
    /// none back, one the system allocates for the process, which stays the
    /// same for every later call.
    static ADAPTER_LUID: LazyLock<(u32, i32)> = LazyLock::new(read_adapter_luid);
    if adapter != 0 || luid.is_null() {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject GetAdapterLUID(adapter={adapter}): another adapter or a null out slot → \
             INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    let (low_part, high_part) = *ADAPTER_LUID;
    // SAFETY: vtable out-param; `luid` is non-null (checked) and points to a
    // writable `LUID` per the IDirect3D9Ex ABI.
    unsafe {
        OutPtr::write_opt(
            luid,
            LUID {
                low_part,
                high_part,
            },
        );
    };
    D3D_OK
}

/// `mode` as an extended display mode: progressive, its size field filled in.
const fn display_mode_ex(mode: &D3DDISPLAYMODE) -> D3DDISPLAYMODEEX {
    D3DDISPLAYMODEEX {
        size: D3DDISPLAYMODEEX_SIZE,
        width: mode.width,
        height: mode.height,
        refresh_rate: mode.refresh_rate,
        format: mode.format,
        scan_line_ordering: D3DSCANLINEORDERING_PROGRESSIVE,
    }
}

/// Answer a `GetDisplayModeEx`-shaped call with `current` and the identity rotation.
///
/// Shared by the adapter, the device and the swap chain. The caller sets the
/// mode's `size` field; a null mode or one whose size is not this layout's is
/// `D3DERR_INVALIDCALL`. A null rotation is allowed and left alone.
///
/// # Safety
/// `mode` is null or points to a writable `D3DDISPLAYMODEEX`, and `rotation`
/// is null or points to a writable `D3DDISPLAYROTATION`.
pub unsafe fn write_display_mode_ex(
    mode: *mut c_void,
    rotation: *mut u32,
    current: &D3DDISPLAYMODE,
) -> i32 {
    if mode.is_null() {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject GetDisplayModeEx: null mode → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: the caller's contract: `mode` is a `D3DDISPLAYMODEEX` whose `size` the caller
    // set, non-null per the check above; alignment is not assumed.
    let size =
        unsafe { display_mode_ex_field(mode, core::mem::offset_of!(D3DDISPLAYMODEEX, size)) };
    if size != D3DDISPLAYMODEEX_SIZE {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject GetDisplayModeEx: D3DDISPLAYMODEEX.Size {size} is not \
             {D3DDISPLAYMODEEX_SIZE} → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: the caller's contract: `mode` is a writable `D3DDISPLAYMODEEX`, non-null per
    // the read above; alignment is not assumed.
    unsafe { OutPtr::write_opt(mode.cast::<D3DDISPLAYMODEEX>(), display_mode_ex(current)) };
    // SAFETY: the caller's contract: `rotation` is null or a writable
    // `D3DDISPLAYROTATION`, which `write_opt` skips when null.
    unsafe { OutPtr::write_opt(rotation, D3DDISPLAYROTATION_IDENTITY) };
    D3D_OK
}

/// The `(width, height)` a caller's `D3DDISPLAYMODEEX` names, `None` for a null pointer.
///
/// Read field by field: the struct is not `Copy`, and the caller's pointer
/// promises no alignment.
///
/// # Safety
/// `mode` is null or points to a readable `D3DDISPLAYMODEEX`.
pub const unsafe fn read_display_mode_ex_size(mode: *const c_void) -> Option<(u32, u32)> {
    if mode.is_null() {
        return None;
    }
    // SAFETY: the caller's contract, with `mode` non-null per the check above; alignment
    // is not assumed.
    let width =
        unsafe { display_mode_ex_field(mode, core::mem::offset_of!(D3DDISPLAYMODEEX, width)) };
    // SAFETY: as above; alignment is not assumed.
    let height =
        unsafe { display_mode_ex_field(mode, core::mem::offset_of!(D3DDISPLAYMODEEX, height)) };
    Some((width, height))
}

/// Read the `u32` field at `offset` of a caller's `D3DDISPLAYMODEEX`.
///
/// # Safety
/// `mode` is non-null and points to a readable `D3DDISPLAYMODEEX`, and `offset` is the offset
/// of one of its `u32` fields.
const unsafe fn display_mode_ex_field(mode: *const c_void, offset: usize) -> u32 {
    let field = mode.cast::<u8>().wrapping_add(offset);
    // SAFETY: the caller's contract puts a readable `u32` at `field`; alignment is not assumed.
    unsafe { field.cast::<u32>().read_unaligned() }
}

/// Read the adapter LUID the driver reports, or allocate one when it reports none.
fn read_adapter_luid() -> (u32, i32) {
    let mut open = D3dkmtOpenAdapterFromGdiDisplayName {
        device_name: [0; 32],
        adapter: 0,
        adapter_luid: LUID {
            low_part: 0,
            high_part: 0,
        },
        vid_pn_source_id: 0,
    };
    for (slot, unit) in open
        .device_name
        .iter_mut()
        .zip(PRIMARY_DISPLAY_NAME.encode_utf16())
    {
        *slot = unit;
    }
    // SAFETY: gdi32 export; `open` is a live, initialised struct of the
    // documented layout whose name is NUL-terminated (the array is zeroed past it).
    let status = unsafe { D3DKMTOpenAdapterFromGdiDisplayName(&raw mut open) };
    if status == 0 {
        let close = D3dkmtCloseAdapter {
            adapter: open.adapter,
        };
        // SAFETY: gdi32 export; `close` names the adapter handle just opened.
        unsafe { D3DKMTCloseAdapter(&raw const close) };
        if open.adapter_luid.low_part != 0 || open.adapter_luid.high_part != 0 {
            return (open.adapter_luid.low_part, open.adapter_luid.high_part);
        }
    }
    let mut allocated = LUID {
        low_part: 0,
        high_part: 0,
    };
    // SAFETY: advapi32 export; `allocated` is a writable `LUID`.
    let ok = unsafe { AllocateLocallyUniqueId(&raw mut allocated) };
    mtld3d_shared::log_once_info!(
        target: LOG_TARGET,
        "GetAdapterLUID: the display driver reports no LUID for {PRIMARY_DISPLAY_NAME} \
         (status {status:#x}); the adapter is named by a LUID allocated for the process \
         (allocated={})",
        ok != 0
    );
    (allocated.low_part, allocated.high_part)
}

extern "system" fn d3d9_check_device_type(
    this: *mut c_void,
    adapter: u32,
    dev_type: u32,
    adapter_format: u32,
    bb_format: u32,
    windowed: i32,
) -> i32 {
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if adapter != 0 || dev_type != D3DDEVTYPE_HAL || !is_display_format(adapter_format) {
        warn!(
            target: LOG_TARGET,
            "reject CheckDeviceType(adapter={adapter}, dev_type={dev_type}, adapter_fmt={adapter_format}, bb_fmt={bb_format}, windowed={windowed}) → NOTAVAILABLE"
        );
        return D3DERR_NOTAVAILABLE;
    }
    // Windowed mode accepts D3DFMT_UNKNOWN as "use the display format".
    let effective_bb = if windowed != 0 && bb_format == 0 {
        adapter_format
    } else {
        bb_format
    };
    // The backbuffer must be a colour surface THIS device renders into, and
    // presentable to the display format: in windowed mode via a supported
    // format conversion (the same predicate `CheckDeviceFormatConversion`
    // answers with, so the two agree for every pair); in fullscreen it must
    // match the display format's colour family directly. The runtime asserts
    // `CheckDeviceType(windowed) == CheckDeviceFormat(RT, bb) &&
    // CheckDeviceFormatConversion(bb, display)`, so the renderable half is
    // the device-restricted answer: where the packed 16-bit formats are
    // expansion-backed, a 16-bit back buffer is refused here as well, and an
    // engine picks X8R8G8B8 the way hardware without 16-bit render targets
    // made it. Only a D3D9 back-buffer format is offered at all, so a title
    // probing for its back buffer lands on one even where the device would
    // render into a wider format. `CreateDevice` and `Reset` stay lenient and
    // substitute the BGRA8 layer format for any format they are handed
    // (`warn_unsupported_backbuffer_format`).
    let expand_packed16 = d3d.config().expand_packed16;
    let presentable = mtld3d_core::format::is_back_buffer_format(effective_bb, windowed != 0)
        && is_render_target_format_on_device(effective_bb, expand_packed16)
        && if windowed != 0 {
            is_format_conversion_supported(effective_bb, adapter_format, expand_packed16)
        } else {
            is_present_compatible(effective_bb, adapter_format)
        };
    if !presentable {
        trace!(
            target: LOG_TARGET,
            "reject CheckDeviceType(adapter_fmt={adapter_format}, bb_fmt={bb_format}, windowed={windowed}) → NOTAVAILABLE"
        );
        return D3DERR_NOTAVAILABLE;
    }
    mtld3d_shared::log_once_trace_by!(
        target: DISPLAY_TRACE_TARGET,
        key: (u64::from(adapter_format) << 32) | u64::from(bb_format),
        "CheckDeviceType(adapter_fmt={adapter_format}, bb_fmt={bb_format}, windowed={windowed}) → OK"
    );
    D3D_OK
}

extern "system" fn d3d9_check_device_format(
    this: *mut c_void,
    adapter: u32,
    dev_type: u32,
    adapter_format: u32,
    usage: u32,
    rtype: u32,
    check_format: u32,
) -> i32 {
    // One line per distinct probe: which formats a title asks about (and in
    // what usage/rtype shape) is the map of the render path it is choosing.
    mtld3d_shared::log_once_debug_by!(
        target: LOG_TARGET,
        key: FormatProbeKey::from_probe(usage, rtype, check_format).raw(),
        "CheckDeviceFormat probe: usage={usage:#x} rtype={rtype} fmt={check_format:#x}"
    );
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let cfg = d3d.config();
    // A D3DFMT_UNKNOWN (0) adapter format is never a valid query — the runtime
    // rejects it with INVALIDCALL ahead of any availability check, for every
    // device type.
    if adapter_format == 0 {
        return D3DERR_INVALIDCALL;
    }
    // Vertex and index buffers carry no format a query could weigh, so a
    // buffer resource type is rejected as a malformed call rather than
    // answered unavailable, with or without a usage.
    if matches!(rtype, D3DRTYPE_VERTEXBUFFER | D3DRTYPE_INDEXBUFFER) {
        return D3DERR_INVALIDCALL;
    }
    if adapter != 0 || dev_type != D3DDEVTYPE_HAL || !is_display_format(adapter_format) {
        warn!(
            target: LOG_TARGET,
            "reject CheckDeviceFormat(adapter={adapter}, dev_type={dev_type}, adapter_fmt={adapter_format}, usage={usage:#x}, rtype={rtype}, check_fmt={check_format}) → NOTAVAILABLE"
        );
        return D3DERR_NOTAVAILABLE;
    }
    // D3DFMT_UNKNOWN is the "no format" sentinel — spec-correct to reject, and
    // games routinely probe it, so don't clutter the log with it.
    if check_format == 0 {
        return D3DERR_NOTAVAILABLE;
    }
    // `caps.dfFormats = false` hides the DF fourccs (INTZ stays): an engine
    // finding both DF and INTZ can pick a mixed depth path no real GPU had.
    if matches!(check_format, D3DFMT_DF24 | D3DFMT_DF16) && !cfg.df_formats {
        return D3DERR_NOTAVAILABLE;
    }
    // ATOC is a capability token, not a texture or render-target format.
    // The explicit probe and control work regardless of the adapter identity.
    if check_format == mtld3d_types::D3DFMT_ATOC {
        return if rtype == D3DRTYPE_SURFACE && usage == 0 {
            D3D_OK
        } else {
            D3DERR_NOTAVAILABLE
        };
    }
    // The RESZ pseudo-format: probing it asks "is the RESZ depth resolve
    // supported" (`SetRenderState(POINTSIZE, 0x7fa05000)`, implemented in
    // the device). No surface of this format is ever created.
    if check_format == D3DFMT_RESZ {
        return if rtype == D3DRTYPE_SURFACE && usage & D3DUSAGE_RENDERTARGET != 0 {
            D3D_OK
        } else {
            D3DERR_NOTAVAILABLE
        };
    }
    // NULL, the render-target format with no colour storage, exists as a 2D
    // render target alone: a depth-only pass binds it as render target 0.
    if mtld3d_core::format::is_null_render_target(check_format) {
        return if mtld3d_core::format::null_render_target_usage_allowed(usage, rtype) {
            D3D_OK
        } else {
            D3DERR_NOTAVAILABLE
        };
    }
    // A query may only carry the usage bits its resource type expresses.
    // The sampling-only group (FILTER, SRGBREAD, VERTEXTEXTURE, WRAPANDMIP,
    // DYNAMIC, SOFTWAREPROCESSING) presumes a shader-resource binding, so it
    // answers NOTAVAILABLE on a plain D3DRTYPE_SURFACE whatever the format
    // is: a surface is never sampled, so filtering is not a question it can
    // say yes to.
    if !mtld3d_core::format::usage_allowed_for_rtype(usage, rtype) {
        trace!(
            target: LOG_TARGET,
            "reject CheckDeviceFormat(usage={usage:#x}, rtype={rtype}, check_fmt={check_format}) → usage not expressible by the resource type"
        );
        return D3DERR_NOTAVAILABLE;
    }
    // Depth textures are GPU-only: they have no packed-depth upload or
    // automatic mip-generation path. Their shader support is fragment-only,
    // and neither sRGB conversion nor color blending applies to depth.
    if is_depth_stencil_format(check_format)
        && (usage & D3DUSAGE_DYNAMIC != 0
            && (mtld3d_core::depth_texture::PackedDepth::from_d3d(check_format).is_none()
                || usage & D3DUSAGE_DEPTHSTENCIL != 0)
            || usage
                & (D3DUSAGE_RENDERTARGET
                    | D3DUSAGE_QUERY_POSTPIXELSHADER_BLENDING
                    | D3DUSAGE_QUERY_SRGBREAD
                    | D3DUSAGE_QUERY_SRGBWRITE
                    | D3DUSAGE_QUERY_VERTEXTEXTURE)
                != 0)
    {
        trace!(target: LOG_TARGET,
            "reject CheckDeviceFormat depth format={check_format} usage={usage:#x} → NOTAVAILABLE");
        return D3DERR_NOTAVAILABLE;
    }
    // Vertex texture fetch: any sampleable texture format can be read from
    // the vertex stage (Metal binds textures to vertex functions natively),
    // matching the non-zero `VertexTextureFilterCaps`. Strip the bit and
    // let the remaining usage bits evaluate normally, so combined queries
    // (RENDERTARGET | QUERY_VERTEXTEXTURE, the render-then-fetch pattern)
    // answer on their other halves.
    let usage = if usage & D3DUSAGE_QUERY_VERTEXTEXTURE != 0 {
        if !is_texture_format(check_format) {
            return D3DERR_NOTAVAILABLE;
        }
        usage & !D3DUSAGE_QUERY_VERTEXTEXTURE
    } else {
        usage
    };
    // D3DUSAGE_QUERY_POSTPIXELSHADER_BLENDING asks whether the format blends
    // as a render target. Every colour attachment blends on Metal, so the
    // answer is the render-target question itself, whether or not the caller
    // also passed D3DUSAGE_RENDERTARGET.
    if usage & D3DUSAGE_QUERY_POSTPIXELSHADER_BLENDING != 0
        && !is_render_target_format_on_device(check_format, cfg.expand_packed16)
    {
        return D3DERR_NOTAVAILABLE;
    }
    // The per-format device answers: D3DUSAGE_QUERY_FILTER (whether the
    // format samples with linear filtering, a per-format device answer
    // rather than the device-wide `D3DPTFILTERCAPS` bits `GetDeviceCaps`
    // reports, and only the single-precision float family depends on the
    // device), D3DUSAGE_QUERY_SRGBWRITE (the render-target answer, the
    // encode being a property of the pass) and D3DUSAGE_QUERY_LEGACYBUMPMAP
    // (the signed formats a bump-environment stage reads its displacement
    // from).
    if !mtld3d_core::format::supports_usage_query(
        check_format,
        usage,
        float32_filtering_supported(cfg.deny_float32_filtering),
        native_packed16_supported(cfg.expand_packed16),
    ) {
        return D3DERR_NOTAVAILABLE;
    }
    let supported = if is_depth_stencil_format(check_format) {
        rtype == D3DRTYPE_TEXTURE
            || (rtype == D3DRTYPE_SURFACE && usage & D3DUSAGE_DEPTHSTENCIL != 0)
    } else if rtype == D3DRTYPE_CUBETEXTURE {
        if usage & D3DUSAGE_DEPTHSTENCIL != 0 {
            false
        } else if usage & D3DUSAGE_RENDERTARGET != 0 {
            is_render_target_format_on_device(check_format, cfg.expand_packed16)
        } else if usage & D3DUSAGE_QUERY_SRGBREAD != 0 && !has_srgb_read_decode(check_format) {
            false
        } else {
            is_cube_texture_format(check_format)
        }
    } else if usage & D3DUSAGE_DEPTHSTENCIL != 0 {
        is_depth_stencil_format(check_format)
    } else if usage & D3DUSAGE_RENDERTARGET != 0 {
        is_render_target_format_on_device(check_format, cfg.expand_packed16)
    } else if matches!(rtype, D3DRTYPE_VOLUME | D3DRTYPE_VOLUMETEXTURE) {
        // A volume's sRGB read is the twin view BC1, BC2 and BC3 all have,
        // the answer `has_srgb_read_decode` gives for the five DXT formats.
        if usage & D3DUSAGE_QUERY_SRGBREAD != 0 && !has_srgb_read_decode(check_format) {
            false
        } else {
            mtld3d_core::format::is_volume_texture_format(check_format)
        }
    } else if rtype == D3DRTYPE_SURFACE || rtype == D3DRTYPE_TEXTURE {
        // SRGBREAD: per-format gate for whether `D3DSAMP_SRGBTEXTURE=1`
        // delivers a real decode. Matches the eager sRGB twin view
        // created in `src/d3d9/unix/src/metal/texture.rs::create_texture`
        // — only formats with an MTLPixelFormat sRGB twin succeed.
        if usage & D3DUSAGE_QUERY_SRGBREAD != 0 && !has_srgb_read_decode(check_format) {
            false
        } else if rtype == D3DRTYPE_SURFACE {
            is_plain_surface_format(check_format)
        } else {
            is_texture_format(check_format)
        }
    } else {
        false
    };
    if !supported {
        trace!(
            target: LOG_TARGET,
            "reject CheckDeviceFormat(adapter_fmt={adapter_format}, usage={usage:#x}, rtype={rtype}, check_fmt={check_format}) → NOTAVAILABLE"
        );
        return D3DERR_NOTAVAILABLE;
    }
    // D3DUSAGE_AUTOGENMIPMAP needs render-target capability even when the
    // query does not include D3DUSAGE_RENDERTARGET, so the answer is the
    // device's render-target answer for the format: a caller derives the one
    // from the other and the two must agree. On a device that expands the
    // packed 16-bit formats `generateMipmaps` would still work on the BGRA8
    // backing, but advertising mip generation for a format the same interface
    // refuses as a render target is the contradiction, so R5G6B5/A1R5G5B5
    // answer NOAUTOGEN there and keep their existing creation policy.
    // The formats of `uses_noautogen_fallback` answer NOAUTOGEN too, and
    // their create retains usage with exactly one backing level and no internal
    // generation flag.
    if usage & D3DUSAGE_AUTOGENMIPMAP != 0
        && !is_render_target_format_on_device(check_format, cfg.expand_packed16)
    {
        return D3DOK_NOAUTOGEN;
    }
    mtld3d_shared::log_once_debug_by!(
        target: DISPLAY_TRACE_TARGET,
        key: FormatProbeKey::from_query(adapter_format, usage, rtype, check_format).raw(),
        "CheckDeviceFormat(adapter_fmt={adapter_format}, usage={usage:#x}, rtype={rtype}, check_fmt={check_format}) → OK"
    );
    D3D_OK
}

extern "system" fn d3d9_check_device_multi_sample_type(
    this: *mut c_void,
    adapter: u32,
    _dev_type: u32,
    surface_format: u32,
    _windowed: i32,
    multi_sample_type: u32,
    quality_levels: *mut u32,
) -> i32 {
    // SAFETY: vtable out-param; `quality_levels` is *mut u32 per IDirect3D9 ABI.
    unsafe { OutPtr::write_opt(quality_levels, 1) };
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if adapter != 0 {
        warn!(
            target: LOG_TARGET,
            "reject CheckDeviceMultiSampleType: adapter={adapter} → INVALIDCALL"
        );
        return D3DERR_INVALIDCALL;
    }
    // Mirror the CheckDeviceFormat gate: hidden DF fourccs stay hidden here,
    // at every count the single-sampled `D3DMULTISAMPLE_NONE` answer aside,
    // which any format gets.
    if matches!(surface_format, D3DFMT_DF24 | D3DFMT_DF16)
        && !d3d.config().df_formats
        && multi_sample_type != D3DMULTISAMPLE_NONE
    {
        return D3DERR_NOTAVAILABLE;
    }
    let caps = device_caps_flags();
    // A format the device cannot render into at all cannot be multisampled
    // either, whatever count is asked for.
    let renderable =
        is_render_target_format_on_device(surface_format, d3d.config().expand_packed16)
            || is_depth_stencil_format(surface_format);
    // `D3DMULTISAMPLE_NONMASKABLE` reports how many rungs its quality ladder
    // has; every maskable level has exactly one quality level. Games poll the
    // whole enum at start-up, so neither the yes nor the no is logged.
    match multisample::resolve_sample_count(multi_sample_type, 0, surface_format, caps) {
        Err(multisample::MultiSampleReject::Invalid) => D3DERR_INVALIDCALL,
        Err(multisample::MultiSampleReject::Unavailable) => D3DERR_NOTAVAILABLE,
        Ok(_) if !renderable && multi_sample_type != D3DMULTISAMPLE_NONE => D3DERR_NOTAVAILABLE,
        Ok(_) => {
            if multi_sample_type == D3DMULTISAMPLE_NONMASKABLE {
                let levels = multisample::nonmaskable_quality_levels(caps);
                // SAFETY: vtable out-param; `quality_levels` is *mut u32 per
                // IDirect3D9 ABI.
                unsafe { OutPtr::write_opt(quality_levels, levels) };
            }
            D3D_OK
        }
    }
}

extern "system" fn d3d9_check_depth_stencil_match(
    this: *mut c_void,
    adapter: u32,
    dev_type: u32,
    adapter_format: u32,
    rt_format: u32,
    ds_format: u32,
) -> i32 {
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let cfg = d3d.config();
    if adapter != 0
        || dev_type != D3DDEVTYPE_HAL
        || !is_display_format(adapter_format)
        || !(is_render_target_format_on_device(rt_format, cfg.expand_packed16)
            || mtld3d_core::format::is_null_render_target(rt_format))
        || !is_depth_stencil_format(ds_format)
    {
        warn!(
            target: LOG_TARGET,
            "reject CheckDepthStencilMatch(adapter={adapter}, dev_type={dev_type}, adapter_fmt={adapter_format}, rt_fmt={rt_format}, ds_fmt={ds_format}) → NOTAVAILABLE"
        );
        return D3DERR_NOTAVAILABLE;
    }
    // Mirror the CheckDeviceFormat gate: hidden DF fourccs stay hidden here.
    if matches!(ds_format, D3DFMT_DF24 | D3DFMT_DF16) && !cfg.df_formats {
        return D3DERR_NOTAVAILABLE;
    }
    D3D_OK
}

extern "system" fn d3d9_check_device_format_conversion(
    this: *mut c_void,
    adapter: u32,
    dev_type: u32,
    source_format: u32,
    target_format: u32,
) -> i32 {
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if adapter != 0
        || dev_type != D3DDEVTYPE_HAL
        || !is_format_conversion_supported(
            source_format,
            target_format,
            d3d.config().expand_packed16,
        )
    {
        trace!(
            target: LOG_TARGET,
            "reject CheckDeviceFormatConversion(adapter={adapter}, dev_type={dev_type}, src_fmt={source_format}, dst_fmt={target_format}) → NOTAVAILABLE"
        );
        return D3DERR_NOTAVAILABLE;
    }
    D3D_OK
}

extern "system" fn d3d9_get_device_caps(
    this: *mut c_void,
    adapter: u32,
    _device_type: u32,
    caps: *mut D3DCAPS9,
) -> i32 {
    trace!(target: LOG_TARGET, "IDirect3D9::GetDeviceCaps(adapter={adapter})");
    if adapter != 0 {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    if caps.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let filled = caps::build(d3d.config().caps_all, sampler_border_supported());
    // SAFETY: vtable out-param; `caps` is *mut D3DCAPS9 per IDirect3D9 ABI, non-null per the
    // check above; alignment is not assumed.
    unsafe { OutPtr::write_opt(caps, filled) };
    0 // S_OK
}

extern "system" fn d3d9_get_adapter_monitor(_this: *mut c_void, _adapter: u32) -> *mut c_void {
    // Single-adapter model: the primary display's monitor. GetMonitorInfo on
    // the result reports MONITORINFOF_PRIMARY, as the D3D9 spec requires for
    // adapter 0.
    crate::fullscreen::primary_monitor()
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetClientRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn D3DKMTOpenAdapterFromGdiDisplayName(desc: *mut D3dkmtOpenAdapterFromGdiDisplayName) -> u32;
    fn D3DKMTCloseAdapter(desc: *const D3dkmtCloseAdapter) -> u32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn AllocateLocallyUniqueId(luid: *mut LUID) -> i32;
}

/// Win32 `D3DKMT_OPENADAPTERFROMGDIDISPLAYNAME`: a GDI display name in, its adapter out.
#[repr(C)]
struct D3dkmtOpenAdapterFromGdiDisplayName {
    device_name: [u16; 32],
    adapter: u32,
    adapter_luid: LUID,
    vid_pn_source_id: u32,
}

/// Win32 `D3DKMT_CLOSEADAPTER`: the adapter handle an open handed out.
#[repr(C)]
struct D3dkmtCloseAdapter {
    adapter: u32,
}

/// Client-area pixel dimensions of `hwnd`, or `None` when the call fails or the rect is empty.
///
/// The single `GetClientRect` boundary is concentrated here so the call site
/// stays unsafe-free. A null window is the caller's own case (no window to
/// read) and passes silently; the two other ways to `None`, a window user32
/// cannot read and a window with no client area, are warned about once per
/// window, since either leaves the requested size standing where the caller
/// expected the window's.
fn client_rect_dims(hwnd: *mut c_void) -> Option<(u32, u32)> {
    if hwnd.is_null() {
        return None;
    }
    let window = hwnd as usize as u64;
    let mut rect = Rect::EMPTY;
    // SAFETY: GetClientRect accepts any HWND and writes a RECT through the
    // out pointer; `rect` is an owned local, so non-null + aligned + writable
    // holds. A bad HWND yields a zero return, handled below.
    let ok = unsafe { GetClientRect(hwnd, &raw mut rect) };
    if ok == 0 {
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET,
            key: window,
            "GetClientRect({window:#x}) failed: the window is gone or not this process's; the \
             requested back-buffer size stands",
        );
        return None;
    }
    let w = u32::try_from(rect.width()).unwrap_or(0);
    let h = u32::try_from(rect.height()).unwrap_or(0);
    if w == 0 || h == 0 {
        mtld3d_shared::log_once_warn_by!(
            target: LOG_TARGET,
            key: window,
            "window {window:#x} has an empty client area ({w}x{h}); the requested back-buffer \
             size stands",
        );
        return None;
    }
    Some((w, h))
}

/// `true` when `width`x`height` is a mode a fullscreen device may set.
///
/// The membership test behind the fullscreen honor-or-follow split in
/// [`resolve_backbuffer_dims`]. Answered against every settable size, not
/// only the bounded list games enumerate: a game's own config may name a
/// mode its menu no longer lists, and user32 accepts it all the same. The
/// sizes [`drop_unscalable_sizes`] leaves out answer `false` although user32
/// lists them: `CrossOver` 27's win32u would abort the process on a mode-set
/// to one.
pub fn is_settable_mode(width: u32, height: u32) -> bool {
    ADAPTER_MODES.settable.contains(&(width, height))
}

/// Resolve the back buffer's *logical* size.
///
/// Logical size is what D3D9 reports and the space every game-supplied
/// coordinate lives in. Three rules, keyed on who decides the resolution:
///
/// - **Fullscreen, requesting a settable mode**: the request stands. The
///   device has set that mode, so the client rect is the request too and
///   viewports, scissors and mouse coordinates all live in one space; the
///   display keeps its own size and present resolves the difference at the
///   drawable (`MetalFX` when enlarging), the same resample `render.scale`
///   rides. The request still stands in the two cases where the client rect
///   reads another size: user32 refused the mode-set and the window covers
///   the monitor, or the window is DPI-unaware on a desktop whose system DPI
///   is not 96 and user32 answers its client rect scaled. The info line below
///   reports the two sizes, and `warn_if_dpi_scaled` names the second case. A
///   request that is *not* a settable mode is one native would reject
///   outright, so no game can depend on it being honored; such games carry
///   their window size into the request and size their rendering and input
///   from the window, so the client rect wins there, the lenient answer that
///   keeps the window, back buffer and mouse in one space. The same holds
///   for a size user32 lists but the mode table leaves out because
///   `CrossOver` 27's win32u would abort on it: the device sets no mode and
///   the back buffer follows the window.
/// - **Maximized window**: the window manager sizes the window, not the game,
///   so the client area wins and the requested resolution is ignored;
///   `render.scale` is the resolution control in that mode.
/// - **Ordinary window**: the game's explicit request stands. Only the D3D9
///   "a zero dimension means the client area" rule is applied, so a zeroed
///   present-params struct (the conformance `stateblock` device, additional
///   swap chains) never forwards a 0-dimension texture descriptor to Metal.
///
/// A window whose client rect cannot be read leaves the request untouched; a
/// dimension still zero afterwards is rejected by the caller.
pub fn resolve_backbuffer_dims(hwnd: u64, pp: &mut D3DPRESENT_PARAMETERS) {
    if pp.windowed == 0 {
        crate::fullscreen::warn_if_dpi_scaled(hwnd as *mut c_void);
        // Callers reject a zero-dimension fullscreen request before the window
        // moves, so the request is always concrete here.
        let client = client_rect_dims(hwnd as *mut c_void);
        if is_settable_mode(pp.back_buffer_width, pp.back_buffer_height) {
            // With the mode set the client rect is the request. It reads
            // otherwise when user32 refused the mode and the window covers
            // the monitor, and when the window is DPI-unaware on a desktop
            // whose system DPI is not 96, where user32 answers a scaled
            // client rect. The line reports both sizes, which ties mouse
            // input in the client rect's space back to the size the game
            // asked for. A client rect that cannot be read only costs the
            // line.
            if let Some((client_w, client_h)) = client
                && (pp.back_buffer_width != client_w || pp.back_buffer_height != client_h)
            {
                mtld3d_shared::log_once_info!(
                    target: LOG_TARGET,
                    "fullscreen device: honoring the requested {}x{} back buffer although the \
                     window's client area reads {}x{}",
                    pp.back_buffer_width, pp.back_buffer_height, client_w, client_h,
                );
            }
            return;
        }
        let Some((client_w, client_h)) = client else {
            return;
        };
        if pp.back_buffer_width != client_w || pp.back_buffer_height != client_h {
            mtld3d_shared::log_once_info!(
                target: LOG_TARGET,
                "fullscreen device: requested {}x{} is no settable display mode (user32 does not \
                 list it, or CrossOver 27's win32u would abort on it), so the back buffer \
                 follows the window ({}x{}) instead",
                pp.back_buffer_width, pp.back_buffer_height, client_w, client_h,
            );
        }
        pp.back_buffer_width = client_w;
        pp.back_buffer_height = client_h;
        return;
    }
    let Some((client_w, client_h)) = client_rect_dims(hwnd as *mut c_void) else {
        return;
    };
    if crate::fullscreen::is_maximized(hwnd as *mut c_void) {
        if pp.back_buffer_width != client_w || pp.back_buffer_height != client_h {
            mtld3d_shared::log_once_info!(
                target: LOG_TARGET,
                "maximized device: back buffer follows the window ({}x{}), requested {}x{} \
                 ignored; use render.scale to pick the render resolution",
                client_w, client_h, pp.back_buffer_width, pp.back_buffer_height,
            );
        }
        pp.back_buffer_width = client_w;
        pp.back_buffer_height = client_h;
        return;
    }
    if pp.back_buffer_width == 0 {
        pp.back_buffer_width = client_w;
    }
    if pp.back_buffer_height == 0 {
        pp.back_buffer_height = client_h;
    }
}

extern "system" fn d3d9_create_device(
    this: *mut c_void,
    adapter: u32,
    dev_type: u32,
    focus_window: *mut c_void,
    behavior_flags: u32,
    present_params: *mut c_void,
    device: *mut *mut c_void,
) -> i32 {
    create_device_impl(&CreateDeviceArgs {
        this,
        adapter,
        dev_type,
        focus_window,
        behavior_flags,
        present_params,
        device,
    })
}

/// `IDirect3D9Ex::CreateDeviceEx`: `CreateDevice` with a fullscreen display mode.
///
/// The mode is not used: a fullscreen device sets the mode its back buffer
/// names, so the two only disagree for a malformed request, which is logged
/// once and created from the present parameters as `CreateDevice` would.
extern "system" fn d3d9_create_device_ex(
    this: *mut c_void,
    adapter: u32,
    dev_type: u32,
    focus_window: *mut c_void,
    behavior_flags: u32,
    present_params: *mut c_void,
    mode: *mut D3DDISPLAYMODEEX,
    device: *mut *mut c_void,
) -> i32 {
    null_out(device);
    // SAFETY: vtable in-param; `present_params` is null or a readable
    // `D3DPRESENT_PARAMETERS` per the IDirect3D9Ex ABI; alignment is not assumed.
    let windowed_and_size =
        unsafe { ValueIn::<D3DPRESENT_PARAMETERS>::read_opt(present_params.cast_const()) }.map(
            |pp| {
                (
                    pp.windowed != 0,
                    (pp.back_buffer_width, pp.back_buffer_height),
                )
            },
        );
    // SAFETY: vtable in-param; `mode` is null or a readable `D3DDISPLAYMODEEX`
    // per the IDirect3D9Ex ABI; alignment is not assumed.
    let mode_size = unsafe { read_display_mode_ex_size(mode.cast_const().cast()) };
    if let Some((windowed, size)) = windowed_and_size
        && !mtld3d_core::extended::reset_ex_mode_valid(windowed, mode_size, size)
    {
        mtld3d_shared::log_once_info!(
            target: LOG_TARGET,
            "CreateDeviceEx: display mode {mode_size:?} disagrees with the present parameters \
             (windowed={windowed}, back buffer {size:?}); the device follows the present \
             parameters"
        );
    }
    create_device_impl(&CreateDeviceArgs {
        this,
        adapter,
        dev_type,
        focus_window,
        behavior_flags,
        present_params,
        device,
    })
}

/// The arguments `CreateDevice` and `CreateDeviceEx` share.
struct CreateDeviceArgs {
    this: *mut c_void,
    adapter: u32,
    dev_type: u32,
    focus_window: *mut c_void,
    behavior_flags: u32,
    present_params: *mut c_void,
    device: *mut *mut c_void,
}

/// The body both device creates share; the device is extended when the interface is.
fn create_device_impl(args: &CreateDeviceArgs) -> i32 {
    let &CreateDeviceArgs {
        this,
        adapter,
        dev_type,
        focus_window,
        behavior_flags,
        present_params,
        device,
    } = args;
    if !crate::USED.swap(true, Ordering::Relaxed) {
        crate::pin_image();
    }

    if adapter != 0 || device.is_null() {
        return D3DERR_INVALIDCALL;
    }
    // SAFETY: vtable thunk; `this` is *mut Direct3D9 per IDirect3D9 ABI.
    let Some(d3d) = (unsafe { InPtr::<Direct3D9>::opt(this) }) else {
        return D3DERR_INVALIDCALL;
    };
    let cfg = d3d.config();
    // SAFETY: vtable in/out-param; per the D3D9 ABI `present_params` points to a
    // readable+writable `D3DPRESENT_PARAMETERS` — CreateDevice resolves and
    // reports the effective geometry back through it; alignment is not assumed.
    let Some(pp_in) =
        (unsafe { ValueIn::<D3DPRESENT_PARAMETERS>::read_opt(present_params.cast_const()) })
    else {
        return D3DERR_INVALIDCALL;
    };
    // Own a mutable copy so a windowed zero-dimension request can be resolved
    // against the device window's client area (below) and the resolved size
    // flows uniformly to the layer, backbuffer, and depth/stencil creates.
    let mut pp = pp_in;

    let extended = d3d.is_extended();
    // Reject invalid swap-effect / back-buffer-count / presentation-interval
    // combinations up front, before any Metal resource is created.
    if !mtld3d_core::present::present_params_are_valid(&pp, extended) {
        warn!(
            target: LOG_TARGET,
            "reject CreateDevice — invalid present params (swap_effect={}, bb_count={}, interval={:#x})",
            pp.swap_effect, pp.back_buffer_count, pp.presentation_interval,
        );
        return D3DERR_INVALIDCALL;
    }
    if !mtld3d_core::present::present_flags_are_valid(&pp) {
        warn!(
            target: LOG_TARGET,
            "reject CreateDevice: D3DPRESENTFLAG_LOCKABLE_BACKBUFFER with MultiSampleType {}, a \
             multisampled back buffer cannot be locked",
            pp.multi_sample_type,
        );
        return D3DERR_INVALIDCALL;
    }
    // An auto depth-stencil the depth answers refuse is refused here too,
    // before the window or the display mode moves. A format of 0 keeps
    // standing for no depth-stencil.
    if pp.enable_auto_depth_stencil != 0
        && pp.auto_depth_stencil_format != 0
        && !multisample::auto_depth_stencil_accepts(
            pp.auto_depth_stencil_format,
            pp.multi_sample_type,
            pp.multi_sample_quality,
            device_caps_flags(),
        )
    {
        warn!(
            target: LOG_TARGET,
            "reject CreateDevice: AutoDepthStencilFormat {} at MultiSampleType {} is no \
             depth-stencil the device offers",
            pp.auto_depth_stencil_format, pp.multi_sample_type,
        );
        return D3DERR_INVALIDCALL;
    }

    let hwnd = if pp.device_window != 0 {
        pp.device_window as u64
    } else {
        focus_window as u64
    };
    if pp.windowed == 0 && (pp.back_buffer_width == 0 || pp.back_buffer_height == 0) {
        warn!(
            target: LOG_TARGET,
            "reject CreateDevice({}x{}): a fullscreen request may not carry zero dimensions",
            pp.back_buffer_width, pp.back_buffer_height,
        );
        return D3DERR_INVALIDCALL;
    }
    if pp.windowed != 0 && pp.back_buffer_format == 0 {
        pp.back_buffer_format = adapter_display_format();
    }
    pp.back_buffer_count = pp.back_buffer_count.max(1);
    let Ok(count) = multisample::resolve_sample_count(
        pp.multi_sample_type,
        pp.multi_sample_quality,
        pp.back_buffer_format,
        device_caps_flags(),
    ) else {
        warn!(
            target: LOG_TARGET,
            "reject CreateDevice: MultiSampleType={} Quality={} on back-buffer format {} -> \
             INVALIDCALL",
            pp.multi_sample_type,
            pp.multi_sample_quality,
            pp.back_buffer_format
        );
        return D3DERR_INVALIDCALL;
    };
    if !matches!(pp.back_buffer_format, D3DFMT_X8R8G8B8 | D3DFMT_A8R8G8B8) {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "reject CreateDevice: requires an X8R8G8B8 or A8R8G8B8 back buffer -> D3DERR_NOTAVAILABLE"
        );
        return D3DERR_NOTAVAILABLE;
    }
    let samples = u8::try_from(count).expect("a sample count of at most 16");
    let Some(mtl) = crate::rt::device::select_device(device_info().registry_id) else {
        error!(target: LOG_TARGET, "reject CreateDevice: winemetal lists no Metal device");
        return D3DERR_NOTAVAILABLE;
    };
    let manage_window = behavior_flags & mtld3d_types::D3DCREATE_NOWINDOWCHANGES == 0;
    let fullscreen = (pp.windowed == 0).then(|| {
        let mode = fullscreen_mode_request(&pp);
        let focus = if focus_window.is_null() {
            pp.device_window as *mut c_void
        } else {
            focus_window
        };
        crate::fullscreen::activate_focus_window(focus);
        crate::fullscreen::enter(hwnd as *mut c_void, manage_window, mode)
    });
    resolve_backbuffer_dims(hwnd, &mut pp);
    crate::device::warn_present_params_fields_once(&pp);
    if pp.back_buffer_width == 0 || pp.back_buffer_height == 0 {
        warn!(target: LOG_TARGET, "reject CreateDevice: zero back-buffer dimensions (hwnd=0x{hwnd:x})");
        restore_from_fullscreen(fullscreen.as_ref());
        return D3DERR_INVALIDCALL;
    }
    let mut profile = crate::rt::device::queue_profile();
    profile.max_latency =
        mtld3d_core::frame_latency::enforced_frame_latency(None, cfg.present_max_frame_latency);
    let mut options = crate::rt::device::CoreOptions::empty();
    if cfg.shader_cache_enable {
        options.insert(crate::rt::device::CoreOptions::SHADER_CACHE);
    }
    if cfg.shader_async_compile {
        options.insert(crate::rt::device::CoreOptions::ASYNC_COMPILE);
    }
    if pp.swap_effect == mtld3d_types::D3DSWAPEFFECT_DISCARD {
        options.insert(crate::rt::device::CoreOptions::DISCARD_BACK_BUFFER);
    }
    if cfg.hdr_enable {
        options.insert(crate::rt::device::CoreOptions::HDR_ENABLE);
    }
    if cfg.color_space == mtld3d_shared::mtl::ColorSpacePolicy::Accurate {
        options.insert(crate::rt::device::CoreOptions::COLOR_SPACE_ACCURATE);
    }
    match cfg.cursor_software {
        mtld3d_shared::mtl::SoftwareCursorPolicy::On => {
            options.insert(crate::rt::device::CoreOptions::SOFTWARE_CURSOR_ON);
        }
        mtld3d_shared::mtl::SoftwareCursorPolicy::Off => {
            options.insert(crate::rt::device::CoreOptions::SOFTWARE_CURSOR_OFF);
        }
        mtld3d_shared::mtl::SoftwareCursorPolicy::Auto => {}
    }
    if cfg.present_occluded {
        options.insert(crate::rt::device::CoreOptions::PRESENT_OCCLUDED);
    }
    let render_scale = resolve_render_scale(
        crate::rt::device::metalfx_available(&mtl),
        cfg.render_scale_percent,
    );
    let (render_w, render_h) = render_scale.extent(pp.back_buffer_width, pp.back_buffer_height);
    let mut core = crate::rt::device::CoreDevice::with_options(mtl, profile, options);
    core.set_retention_cap(cfg.vbib_retention_cap_bytes);
    core.attach_scaled(
        isize::try_from(hwnd).unwrap_or_default(),
        render_w,
        render_h,
        pp.back_buffer_width,
        pp.back_buffer_height,
        samples,
    );
    let (cursor_scale, scale_origin) = resolve_cursor_scale(core.backing_scale(), cfg.cursor_scale);
    let software_cursor = core.software_cursor_active();
    info!(
        target: LOG_TARGET,
        "cursor: {} at {cursor_scale}x ({scale_origin}; cursor.software = {:?})",
        if software_cursor { "software overlay" } else { "hardware HCURSOR" },
        cfg.cursor_software,
    );
    let core_overlay = core.overlay_sink();
    let auto_depth = (pp.enable_auto_depth_stencil != 0)
        .then(|| mtld3d_core::format::map_d3d_depth_format(pp.auto_depth_stencil_format))
        .flatten()
        .map(|pixel_format| {
            let id = mtld3d_core::ids::TextureId::new_unique();
            let desc = crate::rt::device::CoreTextureDesc {
                pixel_format: pixel_format as u32,
                kind: crate::rt::device::TextureKind::Tex2d,
                width: render_w,
                height: render_h,
                depth: 1,
                levels: 1,
                flags: crate::rt::device::CoreTextureFlags::RENDER_TARGET,
                srgb_format: 0,
                block: (1, 1, 4),
                samples,
            };
            core.texture_create(id, &desc);
            if !render_scale.is_identity() {
                core.set_texture_logical_size(id, pp.back_buffer_width, pp.back_buffer_height);
            }
            let bound = core.set_depth(Some(crate::rt::device::DepthRef { id, level: 0 }));
            debug_assert!(bound, "a depth texture just made binds");
            id
        });
    addref_parent_direct3d9(this);
    let mut render_states = mtld3d_types::render_state_defaults();
    if auto_depth.is_none() {
        render_states[mtld3d_types::D3DRS_ZENABLE as usize] = 0;
    }
    let mut stored = pp;
    stored.back_buffer_count = stored.back_buffer_count.max(1);
    if stored.device_window == 0 {
        stored.device_window = focus_window as usize;
    }
    let dev = Direct3DDevice9::new(crate::device::DeviceCreateInfo {
        backbuffer_sample_count: samples,
        backbuffer_width: pp.back_buffer_width,
        backbuffer_height: pp.back_buffer_height,
        render_scale,
        core: Box::new(core),
        core_overlay,
        render_states,
        sampler_states: [mtld3d_types::sampler_state_defaults(); STAGE_COUNT],
        direct3d: this as u64,
        creation_adapter: adapter,
        creation_device_type: dev_type,
        creation_behavior_flags: behavior_flags,
        creation_focus_window: focus_window as usize,
        present_params: stored,
        hwnd: hwnd as *mut c_void,
        cursor_scale,
        software_cursor,
        display_sinks: Box::new(crate::cursor::DisplaySinks::new()),
        fullscreen,
        config: Arc::clone(cfg),
        initial_flags: {
            let mut flags = if extended {
                crate::device::DeviceFlags::EXTENDED
            } else {
                crate::device::DeviceFlags::empty()
            };
            if cfg.fail_next_submit {
                flags.insert(crate::device::DeviceFlags::FAIL_NEXT_SUBMIT);
            }
            flags
        },
    });
    let dev_ptr = Box::into_raw(Box::new(dev));
    // SAFETY: `dev_ptr` is a freshly-boxed, live `Direct3DDevice9`.
    let inner = unsafe { (*dev_ptr).inner() };
    inner.set_device_wrapper(dev_ptr.cast::<c_void>());
    if let Some(id) = auto_depth {
        inner.set_core_auto_depth(id, pp.auto_depth_stencil_format);
    }
    let inner_ptr = std::ptr::from_mut::<crate::device::DeviceInner>(inner);
    inner.cursor().install_subclass(inner_ptr);
    // SAFETY: `present_params` is the non-null pointer the request was read from, writable
    // per the ABI; alignment is not assumed.
    unsafe { crate::device::report_resolved_geometry(present_params, &pp) };
    // SAFETY: vtable out-param; `device` is *mut *mut c_void per IDirect3D9 ABI; alignment is not
    // assumed.
    unsafe { OutPtr::write_opt(device, dev_ptr.cast::<c_void>()) };
    if behavior_flags & mtld3d_types::D3DCREATE_MULTITHREADED != 0 {
        info!(
            target: LOG_TARGET,
            "CreateDevice: D3DCREATE_MULTITHREADED, device entry points serialised"
        );
    }
    apply_device_control_word(behavior_flags);
    info!(target: LOG_TARGET, "CreateDevice succeeded (extended={extended})");
    D3D_OK
}

/// Give the calling thread the device's x87 control word, unless `behavior_flags` keeps it.
#[cfg(target_arch = "x86")]
fn apply_device_control_word(behavior_flags: u32) {
    if !mtld3d_core::fpu::sets_control_word(behavior_flags) {
        return;
    }
    let mut current: u16 = 0;
    // SAFETY: `fnstcw` stores the 16-bit control word through the pointer,
    // which names `current`, a live writable `u16`; it touches no flags or stack.
    unsafe {
        core::arch::asm!(
            "fnstcw word ptr [{}]",
            in(reg) &raw mut current,
            options(nostack, preserves_flags),
        );
    }
    let word = mtld3d_core::fpu::device_control_word(current);
    // SAFETY: `fldcw` reads the 16-bit control word from `word`, a live `u16`;
    // every exception is masked in it, so loading it raises none.
    unsafe {
        core::arch::asm!(
            "fldcw word ptr [{}]",
            in(reg) &raw const word,
            options(nostack, readonly, preserves_flags),
        );
    }
}

/// A non-x86 device leaves the control word alone, so there is nothing to apply.
#[cfg(not(target_arch = "x86"))]
const fn apply_device_control_word(_behavior_flags: u32) {}

/// Retain the parent `IDirect3D9` for `IDirect3DDevice9::GetDirect3D`.
///
/// The call hands back the same interface with `AddRef` semantics rather than
/// a dangling handle after the caller Releases its outer reference.
fn addref_parent_direct3d9(this: *mut c_void) {
    if !this.is_null() {
        // SAFETY: IDirect3D9 IUnknown thunk; D3D9 ABI guarantees `this` is *mut Direct3D9.
        let parent = unsafe { VtableThis::<Direct3D9>::new(this) };
        parent.add_ref();
    }
}

/// `CAMetalLayer.pixelFormat` and the backbuffer are hardcoded `BGRA8Unorm`.
pub fn warn_unsupported_backbuffer_format(format: u32) {
    if !matches!(format, D3DFMT_A8R8G8B8 | D3DFMT_X8R8G8B8) {
        mtld3d_shared::log_once_warn!(target: crate::LOG_TARGET,
            "back_buffer_format {format:#x} requested but layer/backbuffer is hardcoded BGRA8Unorm, substituting"
        );
    }
}

/// Match the cursor bitmap to Wine's retina factor by default.
pub const fn resolve_cursor_scale(
    backing_scale: u32,
    cursor_scale: CursorScale,
) -> (u32, &'static str) {
    let scale = cursor_scale.resolve(backing_scale);
    let origin = match cursor_scale {
        CursorScale::Auto => "auto from the Wine retina mode",
        CursorScale::Fixed(_) => "cursor.scale override",
    };
    (scale, origin)
}

fn resolve_render_scale(
    metalfx_available: bool,
    render_scale_percent: u32,
) -> mtld3d_core::render_scale::RenderScale {
    use mtld3d_core::render_scale::RenderScale;

    let requested = RenderScale::from_percent(render_scale_percent);
    if requested.is_identity() {
        return RenderScale::IDENTITY;
    }
    if !metalfx_available {
        mtld3d_shared::log_once_warn!(
            target: LOG_TARGET,
            "render.scale = {} ignored: this GPU has no MetalFX spatial upscaler, so the \
             frame is rendered at full resolution",
            f64::from(requested.percent()) / 100.0,
        );
        return RenderScale::IDENTITY;
    }
    info!(
        target: LOG_TARGET,
        "render.scale = {}: rendering at {}% of the presented resolution",
        f64::from(requested.percent()) / 100.0,
        requested.percent(),
    );
    requested
}

/// Give the window back when device creation fails mid-way.
fn restore_from_fullscreen(saved: Option<&crate::fullscreen::SavedWindow>) {
    if let Some(saved) = saved {
        crate::fullscreen::leave(
            saved,
            crate::fullscreen::LeaveKind::Plain,
            &mtld3d_core::fullscreen_log::LeaveReason::FailedCreate,
        );
    }
}
