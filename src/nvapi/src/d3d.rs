//! NVAPI's Direct3D: the device extensions, SLI and Reflex.

use core::ffi::c_void;

use dxmt_util::{
    com::{Guid, IUnknownVtbl, query as query_interface, release, vtbl},
    ext::{D3D11ContextExtVtbl, D3D11DeviceExtVtbl, IID_D3D11_CONTEXT_EXT, IID_D3D11_DEVICE_EXT},
};

use crate::{
    LOG,
    nv::{
        EXTN_OP_FP16_ATOMIC, EXTN_OP_SHFL, EXTN_OP_UINT64_ATOMIC, FrameReport,
        GET_SLEEP_STATUS_PARAMS_VER1, GetSleepStatusParams, INCOMPATIBLE_STRUCT_VERSION,
        INVALID_ARGUMENT, LATENCY_MARKER_PARAMS_VER1, LATENCY_RESULT_PARAMS_VER1,
        LatencyResultParams, NO_IMPLEMENTATION, NOT_SUPPORTED, OK, SET_SLEEP_MODE_PARAMS_VER1,
        SLI_STATE_VER1, SLI_STATE_VER2, SliStateV1, SliStateV2, Status,
    },
};

/// `ID3D11Device`.
static IID_DEVICE: Guid = Guid::new(
    0xdb6f_6ddb,
    0xac77,
    0x4e88,
    [0x82, 0x53, 0x81, 0x9d, 0xf9, 0xbb, 0xf1, 0x40],
);

/// `ID3D11Device` up to `GetImmediateContext`, its slot 40.
#[repr(C)]
struct DeviceVtbl {
    unknown: IUnknownVtbl,
    _slots: [*const c_void; 37],
    get_immediate_context: unsafe extern "system" fn(*mut c_void, *mut *mut c_void),
}

/// A COM reference, released when it goes out of scope.
struct Held(*mut c_void);

impl Drop for Held {
    fn drop(&mut self) {
        // SAFETY: the reference `query` or `GetImmediateContext` gave.
        unsafe { release(self.0) };
    }
}

/// The `IMTLD3D11ContextExt` of a context, or of a device's immediate context.
fn context_ext(object: *mut c_void) -> Option<Held> {
    // SAFETY: the game's live COM object.
    if let Some(ext) = unsafe { query_interface(object, &IID_D3D11_CONTEXT_EXT) } {
        return Some(Held(ext));
    }
    // SAFETY: as above.
    let device = Held(unsafe { query_interface(object, &IID_DEVICE) }?);
    let mut context = core::ptr::null_mut();
    // SAFETY: `device` is an `ID3D11Device`.
    let slots = unsafe { vtbl::<DeviceVtbl>(device.0) };
    // SAFETY: the call writes the local with a new reference.
    unsafe { (slots.get_immediate_context)(device.0, &raw mut context) };
    let context = Held(context);
    // SAFETY: the device's immediate context, or null, which `query` answers none.
    unsafe { query_interface(context.0, &IID_D3D11_CONTEXT_EXT) }.map(Held)
}

const extern "C" fn get_current_sli_state(device: *mut c_void, state: *mut SliStateV1) -> Status {
    if device.is_null() || state.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: both versions start with the version word.
    let version = unsafe { state.cast::<u32>().read() };
    match version {
        SLI_STATE_VER1 => {
            // SAFETY: the game's V1 struct.
            let Some(state) = (unsafe { state.as_mut() }) else {
                return INVALID_ARGUMENT;
            };
            fill_sli(state);
        }
        SLI_STATE_VER2 => {
            // SAFETY: the game's V2 struct.
            let Some(state) = (unsafe { state.cast::<SliStateV2>().as_mut() }) else {
                return INVALID_ARGUMENT;
            };
            fill_sli(&mut state.v1);
            state.num_vr_sli_gpus = 0;
        }
        _ => return INCOMPATIBLE_STRUCT_VERSION,
    }
    OK
}

/// One AFR group, the first: SLI's answer for a single GPU.
const fn fill_sli(state: &mut SliStateV1) {
    state.max_num_afr_groups = 1;
    state.num_afr_groups = 1;
    state.current_afr_index = 0;
    state.next_frame_afr_index = 0;
    state.previous_frame_afr_index = 0;
    state.is_cur_afr_group_new = 0;
}

extern "C" fn begin_uav_overlap(object: *mut c_void) -> Status {
    let Some(ext) = context_ext(object) else {
        return INVALID_ARGUMENT;
    };
    // SAFETY: `ext` is DXMT's `IMTLD3D11ContextExt`.
    let slots = unsafe { vtbl::<D3D11ContextExtVtbl>(ext.0) };
    // SAFETY: as above.
    unsafe { (slots.begin_uav_overlap)(ext.0) };
    OK
}

extern "C" fn end_uav_overlap(object: *mut c_void) -> Status {
    let Some(ext) = context_ext(object) else {
        return INVALID_ARGUMENT;
    };
    // SAFETY: `ext` is DXMT's `IMTLD3D11ContextExt`.
    let slots = unsafe { vtbl::<D3D11ContextExtVtbl>(ext.0) };
    // SAFETY: as above.
    unsafe { (slots.end_uav_overlap)(ext.0) };
    OK
}

const extern "C" fn set_depth_bounds_test(
    _object: *mut c_void,
    _enable: u32,
    _min: f32,
    _max: f32,
) -> Status {
    NOT_SUPPORTED
}

/// Whether the C++ names the opcode: the ones games probe, which it answers without a warning.
const fn extn_op_is_known(op: u32) -> bool {
    matches!(
        op,
        EXTN_OP_UINT64_ATOMIC | EXTN_OP_SHFL | EXTN_OP_FP16_ATOMIC
    )
}

extern "C" fn is_extn_op_code_supported(
    _device: *mut c_void,
    op: u32,
    supported: *mut u8,
) -> Status {
    if supported.is_null() {
        return INVALID_ARGUMENT;
    }
    if !extn_op_is_known(op) {
        LOG.warn(&format!("nvapi: unsupported shader extension opcode {op}"));
    }
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { supported.write(0) };
    OK
}

extern "C" fn set_extn_slot(device: *mut c_void, slot: u32) -> Status {
    // SAFETY: the game's live COM object, or null.
    let Some(ext) = (unsafe { query_interface(device, &IID_D3D11_DEVICE_EXT) }).map(Held) else {
        return INVALID_ARGUMENT;
    };
    // SAFETY: `ext` is DXMT's `IMTLD3D11DeviceExt`.
    let slots = unsafe { vtbl::<D3D11DeviceExtVtbl>(ext.0) };
    // SAFETY: as above.
    unsafe { (slots.set_shader_extension_slot)(ext.0, slot) };
    OK
}

const extern "C" fn get_object_handle_for_resource(
    _device: *mut c_void,
    resource: *mut c_void,
    handle: *mut *mut c_void,
) -> Status {
    if handle.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: the game's out pointer, checked non-null.
    unsafe { handle.write(resource) };
    OK
}

const extern "C" fn set_resource_hint(
    _device: *mut c_void,
    _handle: *mut c_void,
    _category: u32,
    _name: u32,
    _value: *mut u32,
) -> Status {
    OK
}

const extern "C" fn get_sleep_status(
    device: *mut c_void,
    params: *mut GetSleepStatusParams,
) -> Status {
    if device.is_null() || params.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: every version starts with the version word.
    if unsafe { params.cast::<u32>().read() } != GET_SLEEP_STATUS_PARAMS_VER1 {
        return INCOMPATIBLE_STRUCT_VERSION;
    }
    // SAFETY: the game's V1 struct.
    let Some(params) = (unsafe { params.as_mut() }) else {
        return INVALID_ARGUMENT;
    };
    params.low_latency_mode = 0;
    params.fs_vrr = 0;
    params.cpl_vsync_on = 0;
    params.sleep_interval_us = 0;
    params.use_game_sleep = 0;
    OK
}

/// A V1 struct passes if it is a V1 and the device is not null; its contents are not read.
const fn version_only(device: *mut c_void, params: *const u32, version: u32) -> Status {
    if device.is_null() || params.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: every version starts with the version word.
    if unsafe { params.read() } == version {
        OK
    } else {
        INCOMPATIBLE_STRUCT_VERSION
    }
}

const extern "C" fn set_sleep_mode(device: *mut c_void, params: *const u32) -> Status {
    version_only(device, params, SET_SLEEP_MODE_PARAMS_VER1)
}

const extern "C" fn set_latency_marker(device: *mut c_void, params: *const u32) -> Status {
    version_only(device, params, LATENCY_MARKER_PARAMS_VER1)
}

const extern "C" fn sleep(device: *mut c_void) -> Status {
    if device.is_null() {
        INVALID_ARGUMENT
    } else {
        OK
    }
}

extern "C" fn get_latency(device: *mut c_void, params: *mut LatencyResultParams) -> Status {
    if device.is_null() || params.is_null() {
        return INVALID_ARGUMENT;
    }
    // SAFETY: every version starts with the version word.
    if unsafe { params.cast::<u32>().read() } != LATENCY_RESULT_PARAMS_VER1 {
        return INCOMPATIBLE_STRUCT_VERSION;
    }
    // SAFETY: the game's V1 struct.
    let Some(params) = (unsafe { params.as_mut() }) else {
        return INVALID_ARGUMENT;
    };
    for report in &mut params.frame_report {
        *report = FrameReport {
            times: [0; 14],
            gpu_active_render_time_us: 0,
            gpu_frame_time_us: 0,
            rsvd: [0; 120],
        };
    }
    OK
}

const extern "C" fn set_vertical_sync_mode(device: *mut c_void, _mode: u32) -> Status {
    if device.is_null() {
        INVALID_ARGUMENT
    } else {
        NO_IMPLEMENTATION
    }
}

/// The address of an `NvAPI_*` function, for `nvapi_QueryInterface`.
const fn addr(function: *const ()) -> *mut c_void {
    function.cast_mut().cast()
}

/// The function `id` names, or null if this module has none.
#[must_use]
pub fn query(id: u32) -> *mut c_void {
    match id {
        0x4b70_8b54 => addr(get_current_sli_state as *const ()),
        0x65b9_3ca8 => addr(begin_uav_overlap as *const ()),
        0x2216_a357 => addr(end_uav_overlap as *const ()),
        0x7aaf_7a04 => addr(set_depth_bounds_test as *const ()),
        0x5f68_da40 => addr(is_extn_op_code_supported as *const ()),
        0x8e90_bb9f => addr(set_extn_slot as *const ()),
        0xfcea_c864 => addr(get_object_handle_for_resource as *const ()),
        0x6c0e_d98c => addr(set_resource_hint as *const ()),
        0xaef9_6ca1 => addr(get_sleep_status as *const ()),
        0xac1c_a9e0 => addr(set_sleep_mode as *const ()),
        0xd998_4c05 => addr(set_latency_marker as *const ()),
        0x852c_d1d2 => addr(sleep as *const ()),
        0x1a58_7f9c => addr(get_latency as *const ()),
        0x5526_cfd1 => addr(set_vertical_sync_mode as *const ()),
        _ => core::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests;
