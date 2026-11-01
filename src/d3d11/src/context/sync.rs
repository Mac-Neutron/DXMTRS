//! Context query dispatch, `Flush`, `Flush1`, `Signal`, `Wait` and context annotations.

use core::ffi::c_void;

use super::{Context, Mode};
use crate::com::{E_NOTIMPL, Hresult};

impl<M: Mode> Context<M> {
    pub const fn begin(&mut self, r#async: *mut c_void) {
        let _ = r#async;
    }

    pub const fn end(&mut self, r#async: *mut c_void) {
        let _ = r#async;
    }

    pub const fn get_data(
        &mut self,
        r#async: *mut c_void,
        data: *mut c_void,
        data_size: u32,
        get_data_flags: u32,
    ) -> Hresult {
        let _ = (r#async, data, data_size, get_data_flags);
        E_NOTIMPL
    }

    pub const fn flush(&mut self) {}

    pub const fn flush1(&mut self, context_type: u32, event: *mut c_void) {
        let _ = (context_type, event);
    }

    #[must_use]
    pub const fn get_type(&self) -> u32 {
        0
    }

    #[must_use]
    pub const fn get_context_flags(&self) -> u32 {
        0
    }

    #[must_use]
    pub const fn is_annotation_enabled(&self) -> i32 {
        0
    }

    pub const fn set_marker_int(&mut self, label: *const c_void, data: i32) {
        let _ = (label, data);
    }

    pub const fn begin_event_int(&mut self, label: *const c_void, data: i32) {
        let _ = (label, data);
    }

    pub const fn end_event(&mut self) {}

    pub const fn set_hardware_protection_state(&mut self, hw_protection_enable: i32) {
        let _ = hw_protection_enable;
    }

    pub const fn get_hardware_protection_state(&self, hw_protection_enable: *mut i32) {
        let _ = hw_protection_enable;
    }

    pub const fn signal(&mut self, fence: *mut c_void, value: u64) -> Hresult {
        let _ = (fence, value);
        E_NOTIMPL
    }

    pub const fn wait(&mut self, fence: *mut c_void, value: u64) -> Hresult {
        let _ = (fence, value);
        E_NOTIMPL
    }
}
