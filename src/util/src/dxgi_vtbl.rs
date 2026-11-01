//! `dxgi1_6.h`'s COM vtables (`crate::dxgi_vtbl`).
//!
//! Written by `tools/winemetal-layouts.sh --com` from clang's AST on each target the crates
//! build for: regenerate it, never edit it.

use core::{ffi::c_void, mem::offset_of};

/// `IUnknownVtbl`: `IUnknown`'s 3 slots.
#[repr(C)]
pub struct IUnknownVtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
}

/// `IDXGIObjectVtbl`: `IDXGIObject`'s 7 slots.
#[repr(C)]
pub struct IDXGIObjectVtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
}

/// `IDXGIDeviceSubObjectVtbl`: `IDXGIDeviceSubObject`'s 8 slots.
#[repr(C)]
pub struct IDXGIDeviceSubObjectVtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_device: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
}

/// `IDXGISwapChainVtbl`: `IDXGISwapChain`'s 18 slots.
#[repr(C)]
pub struct IDXGISwapChainVtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_device: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_buffer: unsafe extern "system-unwind" fn(*mut c_void, u32, *const c_void, *mut *mut c_void) -> i32,
    pub set_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, i32, *mut c_void) -> i32,
    pub get_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, *mut i32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub resize_buffers: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u32, u32, u32) -> i32,
    pub resize_target: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_containing_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_frame_statistics: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_last_present_count: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
}

/// `IDXGISwapChain1Vtbl`: `IDXGISwapChain1`'s 29 slots.
#[repr(C)]
pub struct IDXGISwapChain1Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_device: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_buffer: unsafe extern "system-unwind" fn(*mut c_void, u32, *const c_void, *mut *mut c_void) -> i32,
    pub set_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, i32, *mut c_void) -> i32,
    pub get_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, *mut i32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub resize_buffers: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u32, u32, u32) -> i32,
    pub resize_target: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_containing_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_frame_statistics: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_last_present_count: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_fullscreen_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_hwnd: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_core_window: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present1: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *const c_void) -> i32,
    pub is_temporary_mono_supported: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub get_restrict_to_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub set_background_color: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_background_color: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_rotation: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub get_rotation: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
}

/// `IDXGISwapChain2Vtbl`: `IDXGISwapChain2`'s 36 slots.
#[repr(C)]
pub struct IDXGISwapChain2Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_device: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_buffer: unsafe extern "system-unwind" fn(*mut c_void, u32, *const c_void, *mut *mut c_void) -> i32,
    pub set_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, i32, *mut c_void) -> i32,
    pub get_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, *mut i32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub resize_buffers: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u32, u32, u32) -> i32,
    pub resize_target: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_containing_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_frame_statistics: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_last_present_count: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_fullscreen_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_hwnd: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_core_window: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present1: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *const c_void) -> i32,
    pub is_temporary_mono_supported: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub get_restrict_to_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub set_background_color: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_background_color: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_rotation: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub get_rotation: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub set_source_size: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_source_size: unsafe extern "system-unwind" fn(*mut c_void, *mut u32, *mut u32) -> i32,
    pub set_maximum_frame_latency: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub get_maximum_frame_latency: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub get_frame_latency_waitable_object: unsafe extern "system-unwind" fn(*mut c_void) -> *mut c_void,
    pub set_matrix_transform: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_matrix_transform: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
}

/// `IDXGISwapChain3Vtbl`: `IDXGISwapChain3`'s 40 slots.
#[repr(C)]
pub struct IDXGISwapChain3Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_device: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_buffer: unsafe extern "system-unwind" fn(*mut c_void, u32, *const c_void, *mut *mut c_void) -> i32,
    pub set_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, i32, *mut c_void) -> i32,
    pub get_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, *mut i32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub resize_buffers: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u32, u32, u32) -> i32,
    pub resize_target: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_containing_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_frame_statistics: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_last_present_count: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_fullscreen_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_hwnd: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_core_window: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present1: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *const c_void) -> i32,
    pub is_temporary_mono_supported: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub get_restrict_to_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub set_background_color: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_background_color: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_rotation: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub get_rotation: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub set_source_size: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_source_size: unsafe extern "system-unwind" fn(*mut c_void, *mut u32, *mut u32) -> i32,
    pub set_maximum_frame_latency: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub get_maximum_frame_latency: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub get_frame_latency_waitable_object: unsafe extern "system-unwind" fn(*mut c_void) -> *mut c_void,
    pub set_matrix_transform: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_matrix_transform: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_current_back_buffer_index: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub check_color_space_support: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut u32) -> i32,
    pub set_color_space1: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub resize_buffers1: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u32, u32, u32, *const u32, *const *mut c_void) -> i32,
}

/// `IDXGISwapChain4Vtbl`: `IDXGISwapChain4`'s 41 slots.
#[repr(C)]
pub struct IDXGISwapChain4Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_device: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_buffer: unsafe extern "system-unwind" fn(*mut c_void, u32, *const c_void, *mut *mut c_void) -> i32,
    pub set_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, i32, *mut c_void) -> i32,
    pub get_fullscreen_state: unsafe extern "system-unwind" fn(*mut c_void, *mut i32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub resize_buffers: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u32, u32, u32) -> i32,
    pub resize_target: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_containing_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_frame_statistics: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_last_present_count: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_fullscreen_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_hwnd: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub get_core_window: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub present1: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *const c_void) -> i32,
    pub is_temporary_mono_supported: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub get_restrict_to_output: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub set_background_color: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_background_color: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_rotation: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub get_rotation: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub set_source_size: unsafe extern "system-unwind" fn(*mut c_void, u32, u32) -> i32,
    pub get_source_size: unsafe extern "system-unwind" fn(*mut c_void, *mut u32, *mut u32) -> i32,
    pub set_maximum_frame_latency: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub get_maximum_frame_latency: unsafe extern "system-unwind" fn(*mut c_void, *mut u32) -> i32,
    pub get_frame_latency_waitable_object: unsafe extern "system-unwind" fn(*mut c_void) -> *mut c_void,
    pub set_matrix_transform: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_matrix_transform: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_current_back_buffer_index: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub check_color_space_support: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut u32) -> i32,
    pub set_color_space1: unsafe extern "system-unwind" fn(*mut c_void, u32) -> i32,
    pub resize_buffers1: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u32, u32, u32, *const u32, *const *mut c_void) -> i32,
    pub set_hdr_meta_data: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *mut c_void) -> i32,
}

/// `IDXGIFactoryVtbl`: `IDXGIFactory`'s 12 slots.
#[repr(C)]
pub struct IDXGIFactoryVtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_adapters: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub make_window_association: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u32) -> i32,
    pub get_window_association: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub create_swap_chain: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    pub create_software_adapter: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32,
}

/// `IDXGIFactory1Vtbl`: `IDXGIFactory1`'s 14 slots.
#[repr(C)]
pub struct IDXGIFactory1Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_adapters: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub make_window_association: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u32) -> i32,
    pub get_window_association: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub create_swap_chain: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    pub create_software_adapter: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    pub enum_adapters1: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub is_current: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
}

/// `IDXGIFactory2Vtbl`: `IDXGIFactory2`'s 25 slots.
#[repr(C)]
pub struct IDXGIFactory2Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_adapters: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub make_window_association: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u32) -> i32,
    pub get_window_association: unsafe extern "system-unwind" fn(*mut c_void, *mut *mut c_void) -> i32,
    pub create_swap_chain: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    pub create_software_adapter: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32,
    pub enum_adapters1: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub is_current: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub is_windowed_stereo_enabled: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub create_swap_chain_for_hwnd: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut c_void, *const c_void, *const c_void, *mut c_void, *mut *mut c_void) -> i32,
    pub create_swap_chain_for_core_window: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut c_void, *const c_void, *mut c_void, *mut *mut c_void) -> i32,
    pub get_shared_resource_adapter_luid: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut c_void) -> i32,
    pub register_stereo_status_window: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u32, *mut u32) -> i32,
    pub register_stereo_status_event: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut u32) -> i32,
    pub unregister_stereo_status: unsafe extern "system-unwind" fn(*mut c_void, u32),
    pub register_occlusion_status_window: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, u32, *mut u32) -> i32,
    pub register_occlusion_status_event: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut u32) -> i32,
    pub unregister_occlusion_status: unsafe extern "system-unwind" fn(*mut c_void, u32),
    pub create_swap_chain_for_composition: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *const c_void, *mut c_void, *mut *mut c_void) -> i32,
}

/// `IDXGIOutputVtbl`: `IDXGIOutput`'s 19 slots.
#[repr(C)]
pub struct IDXGIOutputVtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_display_mode_list: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *mut u32, *mut c_void) -> i32,
    pub find_closest_matching_mode: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void, *mut c_void) -> i32,
    pub wait_for_v_blank: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub take_ownership: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, i32) -> i32,
    pub release_ownership: unsafe extern "system-unwind" fn(*mut c_void),
    pub get_gamma_control_capabilities: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_gamma_control: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_gamma_control: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_display_surface: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_display_surface_data: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_frame_statistics: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
}

/// `IDXGIOutput1Vtbl`: `IDXGIOutput1`'s 23 slots.
#[repr(C)]
pub struct IDXGIOutput1Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_display_mode_list: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *mut u32, *mut c_void) -> i32,
    pub find_closest_matching_mode: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void, *mut c_void) -> i32,
    pub wait_for_v_blank: unsafe extern "system-unwind" fn(*mut c_void) -> i32,
    pub take_ownership: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, i32) -> i32,
    pub release_ownership: unsafe extern "system-unwind" fn(*mut c_void),
    pub get_gamma_control_capabilities: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_gamma_control: unsafe extern "system-unwind" fn(*mut c_void, *const c_void) -> i32,
    pub get_gamma_control: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub set_display_surface: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_display_surface_data: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_frame_statistics: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_display_mode_list1: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *mut u32, *mut c_void) -> i32,
    pub find_closest_matching_mode1: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void, *mut c_void) -> i32,
    pub get_display_surface_data1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub duplicate_output: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut *mut c_void) -> i32,
}

/// `IDXGIAdapterVtbl`: `IDXGIAdapter`'s 10 slots.
#[repr(C)]
pub struct IDXGIAdapterVtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_outputs: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub check_interface_support: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void) -> i32,
}

/// `IDXGIAdapter1Vtbl`: `IDXGIAdapter1`'s 11 slots.
#[repr(C)]
pub struct IDXGIAdapter1Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_outputs: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub check_interface_support: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
}

/// `IDXGIAdapter2Vtbl`: `IDXGIAdapter2`'s 12 slots.
#[repr(C)]
pub struct IDXGIAdapter2Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_outputs: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub check_interface_support: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_desc2: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
}

/// `IDXGIAdapter3Vtbl`: `IDXGIAdapter3`'s 18 slots.
#[repr(C)]
pub struct IDXGIAdapter3Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_outputs: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub check_interface_support: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_desc2: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub register_hardware_content_protection_teardown_status_event: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut u32) -> i32,
    pub unregister_hardware_content_protection_teardown_status: unsafe extern "system-unwind" fn(*mut c_void, u32),
    pub query_video_memory_info: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *mut c_void) -> i32,
    pub set_video_memory_reservation: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u64) -> i32,
    pub register_video_memory_budget_change_notification_event: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut u32) -> i32,
    pub unregister_video_memory_budget_change_notification: unsafe extern "system-unwind" fn(*mut c_void, u32),
}

/// `IDXGIAdapter4Vtbl`: `IDXGIAdapter4`'s 19 slots.
#[repr(C)]
pub struct IDXGIAdapter4Vtbl {
    pub query_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub add_ref: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub release: unsafe extern "system-unwind" fn(*mut c_void) -> u32,
    pub set_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, u32, *const c_void) -> i32,
    pub set_private_data_interface: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *const c_void) -> i32,
    pub get_private_data: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut u32, *mut c_void) -> i32,
    pub get_parent: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut *mut c_void) -> i32,
    pub enum_outputs: unsafe extern "system-unwind" fn(*mut c_void, u32, *mut *mut c_void) -> i32,
    pub get_desc: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub check_interface_support: unsafe extern "system-unwind" fn(*mut c_void, *const c_void, *mut c_void) -> i32,
    pub get_desc1: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub get_desc2: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
    pub register_hardware_content_protection_teardown_status_event: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut u32) -> i32,
    pub unregister_hardware_content_protection_teardown_status: unsafe extern "system-unwind" fn(*mut c_void, u32),
    pub query_video_memory_info: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, *mut c_void) -> i32,
    pub set_video_memory_reservation: unsafe extern "system-unwind" fn(*mut c_void, u32, u32, u64) -> i32,
    pub register_video_memory_budget_change_notification_event: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void, *mut u32) -> i32,
    pub unregister_video_memory_budget_change_notification: unsafe extern "system-unwind" fn(*mut c_void, u32),
    pub get_desc3: unsafe extern "system-unwind" fn(*mut c_void, *mut c_void) -> i32,
}

trait Slot {
    const PARAMS: usize;
}

const fn pad(n: usize) -> usize {
    (n + size_of::<usize>() - 1) & !(size_of::<usize>() - 1)
}

impl<R, A0> Slot for unsafe extern "system-unwind" fn(A0) -> R {
    const PARAMS: usize = pad(size_of::<A0>());
}

impl<R, A0, A1> Slot for unsafe extern "system-unwind" fn(A0, A1) -> R {
    const PARAMS: usize = pad(size_of::<A0>()) + pad(size_of::<A1>());
}

impl<R, A0, A1, A2> Slot for unsafe extern "system-unwind" fn(A0, A1, A2) -> R {
    const PARAMS: usize = pad(size_of::<A0>()) + pad(size_of::<A1>()) + pad(size_of::<A2>());
}

impl<R, A0, A1, A2, A3> Slot for unsafe extern "system-unwind" fn(A0, A1, A2, A3) -> R {
    const PARAMS: usize = pad(size_of::<A0>()) + pad(size_of::<A1>()) + pad(size_of::<A2>()) + pad(size_of::<A3>());
}

impl<R, A0, A1, A2, A3, A4> Slot for unsafe extern "system-unwind" fn(A0, A1, A2, A3, A4) -> R {
    const PARAMS: usize = pad(size_of::<A0>()) + pad(size_of::<A1>()) + pad(size_of::<A2>()) + pad(size_of::<A3>()) + pad(size_of::<A4>());
}

impl<R, A0, A1, A2, A3, A4, A5> Slot for unsafe extern "system-unwind" fn(A0, A1, A2, A3, A4, A5) -> R {
    const PARAMS: usize = pad(size_of::<A0>()) + pad(size_of::<A1>()) + pad(size_of::<A2>()) + pad(size_of::<A3>()) + pad(size_of::<A4>()) + pad(size_of::<A5>());
}

impl<R, A0, A1, A2, A3, A4, A5, A6> Slot for unsafe extern "system-unwind" fn(A0, A1, A2, A3, A4, A5, A6) -> R {
    const PARAMS: usize = pad(size_of::<A0>()) + pad(size_of::<A1>()) + pad(size_of::<A2>()) + pad(size_of::<A3>()) + pad(size_of::<A4>()) + pad(size_of::<A5>()) + pad(size_of::<A6>());
}

impl<R, A0, A1, A2, A3, A4, A5, A6, A7> Slot for unsafe extern "system-unwind" fn(A0, A1, A2, A3, A4, A5, A6, A7) -> R {
    const PARAMS: usize = pad(size_of::<A0>()) + pad(size_of::<A1>()) + pad(size_of::<A2>()) + pad(size_of::<A3>()) + pad(size_of::<A4>()) + pad(size_of::<A5>()) + pad(size_of::<A6>()) + pad(size_of::<A7>());
}

const fn slot_params<T, F: Slot>(_: fn(&T) -> &F) -> usize {
    F::PARAMS
}

#[cfg(all(target_arch = "aarch64", target_os = "windows"))]
const _: () = {
    assert!(size_of::<IUnknownVtbl>() == 24);
    assert!(offset_of!(IUnknownVtbl, query_interface) == 0 && slot_params::<IUnknownVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IUnknownVtbl, add_ref) == 8 && slot_params::<IUnknownVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IUnknownVtbl, release) == 16 && slot_params::<IUnknownVtbl, _>(|v| &v.release) == 8);
    assert!(size_of::<IDXGIObjectVtbl>() == 56);
    assert!(offset_of!(IDXGIObjectVtbl, query_interface) == 0 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, add_ref) == 8 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, release) == 16 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, get_parent) == 48 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(size_of::<IDXGIDeviceSubObjectVtbl>() == 64);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, query_interface) == 0 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, add_ref) == 8 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, release) == 16 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_parent) == 48 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_device) == 56 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_device) == 24);
    assert!(size_of::<IDXGISwapChainVtbl>() == 144);
    assert!(offset_of!(IDXGISwapChainVtbl, query_interface) == 0 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, add_ref) == 8 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, release) == 16 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data) == 24 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_private_data) == 40 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, get_parent) == 48 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_device) == 56 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, present) == 64 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_buffer) == 72 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_desc) == 96 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_target) == 112 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(size_of::<IDXGISwapChain1Vtbl>() == 232);
    assert!(offset_of!(IDXGISwapChain1Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, release) == 16 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present) == 64 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present1) == 176 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(size_of::<IDXGISwapChain2Vtbl>() == 288);
    assert!(offset_of!(IDXGISwapChain2Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, release) == 16 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present) == 64 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present1) == 176 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(size_of::<IDXGISwapChain3Vtbl>() == 320);
    assert!(offset_of!(IDXGISwapChain3Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, release) == 16 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present) == 64 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present1) == 176 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(size_of::<IDXGISwapChain4Vtbl>() == 328);
    assert!(offset_of!(IDXGISwapChain4Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, release) == 16 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present) == 64 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present1) == 176 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_hdr_meta_data) == 320 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_hdr_meta_data) == 32);
    assert!(size_of::<IDXGIFactoryVtbl>() == 96);
    assert!(offset_of!(IDXGIFactoryVtbl, query_interface) == 0 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, add_ref) == 8 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, release) == 16 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data) == 24 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_private_data) == 40 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, get_parent) == 48 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, enum_adapters) == 56 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, make_window_association) == 64 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_window_association) == 72 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactoryVtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(size_of::<IDXGIFactory1Vtbl>() == 112);
    assert!(offset_of!(IDXGIFactory1Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, release) == 16 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, is_current) == 104 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.is_current) == 8);
    assert!(size_of::<IDXGIFactory2Vtbl>() == 200);
    assert!(offset_of!(IDXGIFactory2Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, release) == 16 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_current) == 104 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_current) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_windowed_stereo_enabled) == 112 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_windowed_stereo_enabled) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_hwnd) == 120 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_hwnd) == 56);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_core_window) == 128 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_core_window) == 48);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_shared_resource_adapter_luid) == 136 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_shared_resource_adapter_luid) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_window) == 144 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_event) == 152 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_stereo_status) == 160 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_stereo_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_window) == 168 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_event) == 176 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_occlusion_status) == 184 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_occlusion_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_composition) == 192 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_composition) == 40);
    assert!(size_of::<IDXGIOutputVtbl>() == 152);
    assert!(offset_of!(IDXGIOutputVtbl, query_interface) == 0 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, add_ref) == 8 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, release) == 16 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data) == 24 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_private_data) == 40 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, get_parent) == 48 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_desc) == 56 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutputVtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, take_ownership) == 88 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, release_ownership) == 96 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_display_surface) == 128 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(size_of::<IDXGIOutput1Vtbl>() == 184);
    assert!(offset_of!(IDXGIOutput1Vtbl, query_interface) == 0 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, add_ref) == 8 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, release) == 16 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data) == 24 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_private_data) == 40 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_parent) == 48 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_desc) == 56 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, take_ownership) == 88 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, release_ownership) == 96 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_display_surface) == 128 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list1) == 152 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list1) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode1) == 160 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode1) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data1) == 168 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data1) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, duplicate_output) == 176 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.duplicate_output) == 24);
    assert!(size_of::<IDXGIAdapterVtbl>() == 80);
    assert!(offset_of!(IDXGIAdapterVtbl, query_interface) == 0 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, add_ref) == 8 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, release) == 16 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data) == 24 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_private_data) == 40 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, get_parent) == 48 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_desc) == 64 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapterVtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(size_of::<IDXGIAdapter1Vtbl>() == 88);
    assert!(offset_of!(IDXGIAdapter1Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, release) == 16 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter1Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(size_of::<IDXGIAdapter2Vtbl>() == 96);
    assert!(offset_of!(IDXGIAdapter2Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, release) == 16 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(size_of::<IDXGIAdapter3Vtbl>() == 144);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, release) == 16 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(size_of::<IDXGIAdapter4Vtbl>() == 152);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, release) == 16 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc3) == 144 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc3) == 16);
};

#[cfg(all(target_arch = "arm64ec", target_os = "windows"))]
const _: () = {
    assert!(size_of::<IUnknownVtbl>() == 24);
    assert!(offset_of!(IUnknownVtbl, query_interface) == 0 && slot_params::<IUnknownVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IUnknownVtbl, add_ref) == 8 && slot_params::<IUnknownVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IUnknownVtbl, release) == 16 && slot_params::<IUnknownVtbl, _>(|v| &v.release) == 8);
    assert!(size_of::<IDXGIObjectVtbl>() == 56);
    assert!(offset_of!(IDXGIObjectVtbl, query_interface) == 0 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, add_ref) == 8 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, release) == 16 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, get_parent) == 48 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(size_of::<IDXGIDeviceSubObjectVtbl>() == 64);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, query_interface) == 0 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, add_ref) == 8 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, release) == 16 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_parent) == 48 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_device) == 56 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_device) == 24);
    assert!(size_of::<IDXGISwapChainVtbl>() == 144);
    assert!(offset_of!(IDXGISwapChainVtbl, query_interface) == 0 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, add_ref) == 8 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, release) == 16 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data) == 24 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_private_data) == 40 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, get_parent) == 48 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_device) == 56 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, present) == 64 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_buffer) == 72 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_desc) == 96 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_target) == 112 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(size_of::<IDXGISwapChain1Vtbl>() == 232);
    assert!(offset_of!(IDXGISwapChain1Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, release) == 16 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present) == 64 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present1) == 176 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(size_of::<IDXGISwapChain2Vtbl>() == 288);
    assert!(offset_of!(IDXGISwapChain2Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, release) == 16 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present) == 64 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present1) == 176 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(size_of::<IDXGISwapChain3Vtbl>() == 320);
    assert!(offset_of!(IDXGISwapChain3Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, release) == 16 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present) == 64 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present1) == 176 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(size_of::<IDXGISwapChain4Vtbl>() == 328);
    assert!(offset_of!(IDXGISwapChain4Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, release) == 16 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present) == 64 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present1) == 176 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_hdr_meta_data) == 320 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_hdr_meta_data) == 32);
    assert!(size_of::<IDXGIFactoryVtbl>() == 96);
    assert!(offset_of!(IDXGIFactoryVtbl, query_interface) == 0 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, add_ref) == 8 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, release) == 16 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data) == 24 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_private_data) == 40 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, get_parent) == 48 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, enum_adapters) == 56 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, make_window_association) == 64 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_window_association) == 72 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactoryVtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(size_of::<IDXGIFactory1Vtbl>() == 112);
    assert!(offset_of!(IDXGIFactory1Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, release) == 16 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, is_current) == 104 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.is_current) == 8);
    assert!(size_of::<IDXGIFactory2Vtbl>() == 200);
    assert!(offset_of!(IDXGIFactory2Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, release) == 16 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_current) == 104 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_current) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_windowed_stereo_enabled) == 112 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_windowed_stereo_enabled) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_hwnd) == 120 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_hwnd) == 56);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_core_window) == 128 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_core_window) == 48);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_shared_resource_adapter_luid) == 136 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_shared_resource_adapter_luid) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_window) == 144 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_event) == 152 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_stereo_status) == 160 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_stereo_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_window) == 168 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_event) == 176 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_occlusion_status) == 184 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_occlusion_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_composition) == 192 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_composition) == 40);
    assert!(size_of::<IDXGIOutputVtbl>() == 152);
    assert!(offset_of!(IDXGIOutputVtbl, query_interface) == 0 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, add_ref) == 8 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, release) == 16 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data) == 24 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_private_data) == 40 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, get_parent) == 48 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_desc) == 56 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutputVtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, take_ownership) == 88 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, release_ownership) == 96 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_display_surface) == 128 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(size_of::<IDXGIOutput1Vtbl>() == 184);
    assert!(offset_of!(IDXGIOutput1Vtbl, query_interface) == 0 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, add_ref) == 8 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, release) == 16 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data) == 24 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_private_data) == 40 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_parent) == 48 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_desc) == 56 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, take_ownership) == 88 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, release_ownership) == 96 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_display_surface) == 128 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list1) == 152 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list1) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode1) == 160 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode1) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data1) == 168 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data1) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, duplicate_output) == 176 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.duplicate_output) == 24);
    assert!(size_of::<IDXGIAdapterVtbl>() == 80);
    assert!(offset_of!(IDXGIAdapterVtbl, query_interface) == 0 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, add_ref) == 8 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, release) == 16 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data) == 24 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_private_data) == 40 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, get_parent) == 48 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_desc) == 64 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapterVtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(size_of::<IDXGIAdapter1Vtbl>() == 88);
    assert!(offset_of!(IDXGIAdapter1Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, release) == 16 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter1Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(size_of::<IDXGIAdapter2Vtbl>() == 96);
    assert!(offset_of!(IDXGIAdapter2Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, release) == 16 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(size_of::<IDXGIAdapter3Vtbl>() == 144);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, release) == 16 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(size_of::<IDXGIAdapter4Vtbl>() == 152);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, release) == 16 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc3) == 144 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc3) == 16);
};

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
const _: () = {
    assert!(size_of::<IUnknownVtbl>() == 24);
    assert!(offset_of!(IUnknownVtbl, query_interface) == 0 && slot_params::<IUnknownVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IUnknownVtbl, add_ref) == 8 && slot_params::<IUnknownVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IUnknownVtbl, release) == 16 && slot_params::<IUnknownVtbl, _>(|v| &v.release) == 8);
    assert!(size_of::<IDXGIObjectVtbl>() == 56);
    assert!(offset_of!(IDXGIObjectVtbl, query_interface) == 0 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, add_ref) == 8 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, release) == 16 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, get_parent) == 48 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(size_of::<IDXGIDeviceSubObjectVtbl>() == 64);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, query_interface) == 0 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, add_ref) == 8 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, release) == 16 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_parent) == 48 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_device) == 56 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_device) == 24);
    assert!(size_of::<IDXGISwapChainVtbl>() == 144);
    assert!(offset_of!(IDXGISwapChainVtbl, query_interface) == 0 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, add_ref) == 8 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, release) == 16 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data) == 24 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_private_data) == 40 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, get_parent) == 48 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_device) == 56 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, present) == 64 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_buffer) == 72 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_desc) == 96 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_target) == 112 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(size_of::<IDXGISwapChain1Vtbl>() == 232);
    assert!(offset_of!(IDXGISwapChain1Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, release) == 16 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present) == 64 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present1) == 176 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(size_of::<IDXGISwapChain2Vtbl>() == 288);
    assert!(offset_of!(IDXGISwapChain2Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, release) == 16 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present) == 64 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present1) == 176 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(size_of::<IDXGISwapChain3Vtbl>() == 320);
    assert!(offset_of!(IDXGISwapChain3Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, release) == 16 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present) == 64 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present1) == 176 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(size_of::<IDXGISwapChain4Vtbl>() == 328);
    assert!(offset_of!(IDXGISwapChain4Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, release) == 16 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present) == 64 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present1) == 176 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_hdr_meta_data) == 320 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_hdr_meta_data) == 32);
    assert!(size_of::<IDXGIFactoryVtbl>() == 96);
    assert!(offset_of!(IDXGIFactoryVtbl, query_interface) == 0 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, add_ref) == 8 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, release) == 16 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data) == 24 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_private_data) == 40 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, get_parent) == 48 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, enum_adapters) == 56 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, make_window_association) == 64 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_window_association) == 72 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactoryVtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(size_of::<IDXGIFactory1Vtbl>() == 112);
    assert!(offset_of!(IDXGIFactory1Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, release) == 16 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, is_current) == 104 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.is_current) == 8);
    assert!(size_of::<IDXGIFactory2Vtbl>() == 200);
    assert!(offset_of!(IDXGIFactory2Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, release) == 16 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_current) == 104 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_current) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_windowed_stereo_enabled) == 112 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_windowed_stereo_enabled) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_hwnd) == 120 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_hwnd) == 56);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_core_window) == 128 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_core_window) == 48);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_shared_resource_adapter_luid) == 136 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_shared_resource_adapter_luid) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_window) == 144 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_event) == 152 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_stereo_status) == 160 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_stereo_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_window) == 168 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_event) == 176 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_occlusion_status) == 184 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_occlusion_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_composition) == 192 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_composition) == 40);
    assert!(size_of::<IDXGIOutputVtbl>() == 152);
    assert!(offset_of!(IDXGIOutputVtbl, query_interface) == 0 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, add_ref) == 8 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, release) == 16 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data) == 24 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_private_data) == 40 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, get_parent) == 48 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_desc) == 56 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutputVtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, take_ownership) == 88 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, release_ownership) == 96 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_display_surface) == 128 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(size_of::<IDXGIOutput1Vtbl>() == 184);
    assert!(offset_of!(IDXGIOutput1Vtbl, query_interface) == 0 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, add_ref) == 8 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, release) == 16 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data) == 24 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_private_data) == 40 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_parent) == 48 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_desc) == 56 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, take_ownership) == 88 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, release_ownership) == 96 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_display_surface) == 128 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list1) == 152 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list1) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode1) == 160 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode1) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data1) == 168 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data1) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, duplicate_output) == 176 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.duplicate_output) == 24);
    assert!(size_of::<IDXGIAdapterVtbl>() == 80);
    assert!(offset_of!(IDXGIAdapterVtbl, query_interface) == 0 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, add_ref) == 8 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, release) == 16 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data) == 24 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_private_data) == 40 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, get_parent) == 48 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_desc) == 64 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapterVtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(size_of::<IDXGIAdapter1Vtbl>() == 88);
    assert!(offset_of!(IDXGIAdapter1Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, release) == 16 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter1Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(size_of::<IDXGIAdapter2Vtbl>() == 96);
    assert!(offset_of!(IDXGIAdapter2Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, release) == 16 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(size_of::<IDXGIAdapter3Vtbl>() == 144);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, release) == 16 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(size_of::<IDXGIAdapter4Vtbl>() == 152);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, release) == 16 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc3) == 144 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc3) == 16);
};

#[cfg(all(target_arch = "x86", target_os = "windows"))]
const _: () = {
    assert!(size_of::<IUnknownVtbl>() == 12);
    assert!(offset_of!(IUnknownVtbl, query_interface) == 0 && slot_params::<IUnknownVtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IUnknownVtbl, add_ref) == 4 && slot_params::<IUnknownVtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IUnknownVtbl, release) == 8 && slot_params::<IUnknownVtbl, _>(|v| &v.release) == 4);
    assert!(size_of::<IDXGIObjectVtbl>() == 28);
    assert!(offset_of!(IDXGIObjectVtbl, query_interface) == 0 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIObjectVtbl, add_ref) == 4 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIObjectVtbl, release) == 8 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data) == 12 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data_interface) == 16 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIObjectVtbl, get_private_data) == 20 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIObjectVtbl, get_parent) == 24 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_parent) == 12);
    assert!(size_of::<IDXGIDeviceSubObjectVtbl>() == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, query_interface) == 0 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, add_ref) == 4 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, release) == 8 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data) == 12 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data_interface) == 16 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_private_data) == 20 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_parent) == 24 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_device) == 28 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_device) == 12);
    assert!(size_of::<IDXGISwapChainVtbl>() == 72);
    assert!(offset_of!(IDXGISwapChainVtbl, query_interface) == 0 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGISwapChainVtbl, add_ref) == 4 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGISwapChainVtbl, release) == 8 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data) == 12 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data_interface) == 16 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGISwapChainVtbl, get_private_data) == 20 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_parent) == 24 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGISwapChainVtbl, get_device) == 28 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_device) == 12);
    assert!(offset_of!(IDXGISwapChainVtbl, present) == 32 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.present) == 12);
    assert!(offset_of!(IDXGISwapChainVtbl, get_buffer) == 36 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_buffer) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, set_fullscreen_state) == 40 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChainVtbl, get_fullscreen_state) == 44 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChainVtbl, get_desc) == 48 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_buffers) == 52 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_buffers) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_target) == 56 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_target) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, get_containing_output) == 60 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_containing_output) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, get_frame_statistics) == 64 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_frame_statistics) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, get_last_present_count) == 68 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_last_present_count) == 8);
    assert!(size_of::<IDXGISwapChain1Vtbl>() == 116);
    assert!(offset_of!(IDXGISwapChain1Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, add_ref) == 4 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGISwapChain1Vtbl, release) == 8 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data) == 12 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_private_data) == 20 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_parent) == 24 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_device) == 28 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_device) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present) == 32 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_buffer) == 36 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_buffer) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_fullscreen_state) == 40 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_state) == 44 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc) == 48 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_buffers) == 52 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_buffers) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_target) == 56 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_target) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_containing_output) == 60 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_containing_output) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_frame_statistics) == 64 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_frame_statistics) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_last_present_count) == 68 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_last_present_count) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc1) == 72 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_desc) == 76 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_desc) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_hwnd) == 80 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_hwnd) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_core_window) == 84 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_core_window) == 12);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present1) == 88 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present1) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, is_temporary_mono_supported) == 92 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.is_temporary_mono_supported) == 4);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_restrict_to_output) == 96 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_restrict_to_output) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_background_color) == 100 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_background_color) == 104 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_rotation) == 108 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_rotation) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_rotation) == 112 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_rotation) == 8);
    assert!(size_of::<IDXGISwapChain2Vtbl>() == 144);
    assert!(offset_of!(IDXGISwapChain2Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, add_ref) == 4 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGISwapChain2Vtbl, release) == 8 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data) == 12 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_private_data) == 20 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_parent) == 24 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_device) == 28 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_device) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present) == 32 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_buffer) == 36 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_buffer) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_fullscreen_state) == 40 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_state) == 44 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc) == 48 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_buffers) == 52 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_buffers) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_target) == 56 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_target) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_containing_output) == 60 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_containing_output) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_statistics) == 64 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_statistics) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_last_present_count) == 68 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_last_present_count) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc1) == 72 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_desc) == 76 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_desc) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_hwnd) == 80 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_hwnd) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_core_window) == 84 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_core_window) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present1) == 88 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present1) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, is_temporary_mono_supported) == 92 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.is_temporary_mono_supported) == 4);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_restrict_to_output) == 96 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_restrict_to_output) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_background_color) == 100 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_background_color) == 104 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_rotation) == 108 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_rotation) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_rotation) == 112 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_rotation) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_source_size) == 116 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_source_size) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_source_size) == 120 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_source_size) == 12);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_maximum_frame_latency) == 124 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_maximum_frame_latency) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_maximum_frame_latency) == 128 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_maximum_frame_latency) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_latency_waitable_object) == 132 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 4);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_matrix_transform) == 136 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_matrix_transform) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_matrix_transform) == 140 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_matrix_transform) == 8);
    assert!(size_of::<IDXGISwapChain3Vtbl>() == 160);
    assert!(offset_of!(IDXGISwapChain3Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, add_ref) == 4 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGISwapChain3Vtbl, release) == 8 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data) == 12 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_private_data) == 20 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_parent) == 24 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_device) == 28 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_device) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present) == 32 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_buffer) == 36 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_buffer) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_fullscreen_state) == 40 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_state) == 44 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc) == 48 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers) == 52 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_target) == 56 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_target) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_containing_output) == 60 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_containing_output) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_statistics) == 64 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_statistics) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_last_present_count) == 68 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_last_present_count) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc1) == 72 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_desc) == 76 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_desc) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_hwnd) == 80 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_hwnd) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_core_window) == 84 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_core_window) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present1) == 88 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, is_temporary_mono_supported) == 92 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.is_temporary_mono_supported) == 4);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_restrict_to_output) == 96 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_restrict_to_output) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_background_color) == 100 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_background_color) == 104 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_rotation) == 108 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_rotation) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_rotation) == 112 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_rotation) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_source_size) == 116 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_source_size) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_source_size) == 120 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_source_size) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_maximum_frame_latency) == 124 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_maximum_frame_latency) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_maximum_frame_latency) == 128 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_maximum_frame_latency) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_latency_waitable_object) == 132 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 4);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_matrix_transform) == 136 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_matrix_transform) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_matrix_transform) == 140 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_matrix_transform) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_current_back_buffer_index) == 144 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_current_back_buffer_index) == 4);
    assert!(offset_of!(IDXGISwapChain3Vtbl, check_color_space_support) == 148 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.check_color_space_support) == 12);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_color_space1) == 152 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_color_space1) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers1) == 156 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers1) == 32);
    assert!(size_of::<IDXGISwapChain4Vtbl>() == 164);
    assert!(offset_of!(IDXGISwapChain4Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, add_ref) == 4 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGISwapChain4Vtbl, release) == 8 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data) == 12 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_private_data) == 20 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_parent) == 24 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_device) == 28 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_device) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present) == 32 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_buffer) == 36 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_buffer) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_fullscreen_state) == 40 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_state) == 44 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_state) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc) == 48 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers) == 52 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_target) == 56 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_target) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_containing_output) == 60 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_containing_output) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_statistics) == 64 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_statistics) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_last_present_count) == 68 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_last_present_count) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc1) == 72 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_desc) == 76 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_desc) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_hwnd) == 80 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_hwnd) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_core_window) == 84 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_core_window) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present1) == 88 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, is_temporary_mono_supported) == 92 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.is_temporary_mono_supported) == 4);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_restrict_to_output) == 96 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_restrict_to_output) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_background_color) == 100 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_background_color) == 104 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_background_color) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_rotation) == 108 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_rotation) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_rotation) == 112 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_rotation) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_source_size) == 116 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_source_size) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_source_size) == 120 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_source_size) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_maximum_frame_latency) == 124 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_maximum_frame_latency) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_maximum_frame_latency) == 128 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_maximum_frame_latency) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_latency_waitable_object) == 132 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 4);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_matrix_transform) == 136 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_matrix_transform) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_matrix_transform) == 140 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_matrix_transform) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_current_back_buffer_index) == 144 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_current_back_buffer_index) == 4);
    assert!(offset_of!(IDXGISwapChain4Vtbl, check_color_space_support) == 148 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.check_color_space_support) == 12);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_color_space1) == 152 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_color_space1) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers1) == 156 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers1) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_hdr_meta_data) == 160 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_hdr_meta_data) == 16);
    assert!(size_of::<IDXGIFactoryVtbl>() == 48);
    assert!(offset_of!(IDXGIFactoryVtbl, query_interface) == 0 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIFactoryVtbl, add_ref) == 4 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIFactoryVtbl, release) == 8 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data) == 12 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data_interface) == 16 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIFactoryVtbl, get_private_data) == 20 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIFactoryVtbl, get_parent) == 24 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIFactoryVtbl, enum_adapters) == 28 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.enum_adapters) == 12);
    assert!(offset_of!(IDXGIFactoryVtbl, make_window_association) == 32 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.make_window_association) == 12);
    assert!(offset_of!(IDXGIFactoryVtbl, get_window_association) == 36 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_window_association) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, create_swap_chain) == 40 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_swap_chain) == 16);
    assert!(offset_of!(IDXGIFactoryVtbl, create_software_adapter) == 44 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_software_adapter) == 12);
    assert!(size_of::<IDXGIFactory1Vtbl>() == 56);
    assert!(offset_of!(IDXGIFactory1Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIFactory1Vtbl, add_ref) == 4 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIFactory1Vtbl, release) == 8 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data) == 12 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_private_data) == 20 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_parent) == 24 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters) == 28 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters) == 12);
    assert!(offset_of!(IDXGIFactory1Vtbl, make_window_association) == 32 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.make_window_association) == 12);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_window_association) == 36 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_window_association) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_swap_chain) == 40 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_swap_chain) == 16);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_software_adapter) == 44 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_software_adapter) == 12);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters1) == 48 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters1) == 12);
    assert!(offset_of!(IDXGIFactory1Vtbl, is_current) == 52 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.is_current) == 4);
    assert!(size_of::<IDXGIFactory2Vtbl>() == 100);
    assert!(offset_of!(IDXGIFactory2Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, add_ref) == 4 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIFactory2Vtbl, release) == 8 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data) == 12 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_private_data) == 20 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_parent) == 24 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters) == 28 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, make_window_association) == 32 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.make_window_association) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_window_association) == 36 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_window_association) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain) == 40 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_software_adapter) == 44 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_software_adapter) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters1) == 48 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters1) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_current) == 52 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_current) == 4);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_windowed_stereo_enabled) == 56 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_windowed_stereo_enabled) == 4);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_hwnd) == 60 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_hwnd) == 28);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_core_window) == 64 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_core_window) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_shared_resource_adapter_luid) == 68 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_shared_resource_adapter_luid) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_window) == 72 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_window) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_event) == 76 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_event) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_stereo_status) == 80 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_stereo_status) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_window) == 84 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_window) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_event) == 88 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_event) == 12);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_occlusion_status) == 92 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_occlusion_status) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_composition) == 96 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_composition) == 20);
    assert!(size_of::<IDXGIOutputVtbl>() == 76);
    assert!(offset_of!(IDXGIOutputVtbl, query_interface) == 0 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIOutputVtbl, add_ref) == 4 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIOutputVtbl, release) == 8 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data) == 12 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data_interface) == 16 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIOutputVtbl, get_private_data) == 20 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_parent) == 24 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIOutputVtbl, get_desc) == 28 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_mode_list) == 32 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_mode_list) == 20);
    assert!(offset_of!(IDXGIOutputVtbl, find_closest_matching_mode) == 36 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.find_closest_matching_mode) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, wait_for_v_blank) == 40 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.wait_for_v_blank) == 4);
    assert!(offset_of!(IDXGIOutputVtbl, take_ownership) == 44 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.take_ownership) == 12);
    assert!(offset_of!(IDXGIOutputVtbl, release_ownership) == 48 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release_ownership) == 4);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control_capabilities) == 52 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control_capabilities) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, set_gamma_control) == 56 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_gamma_control) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control) == 60 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, set_display_surface) == 64 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_display_surface) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_surface_data) == 68 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_surface_data) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_frame_statistics) == 72 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_frame_statistics) == 8);
    assert!(size_of::<IDXGIOutput1Vtbl>() == 92);
    assert!(offset_of!(IDXGIOutput1Vtbl, query_interface) == 0 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIOutput1Vtbl, add_ref) == 4 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIOutput1Vtbl, release) == 8 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data) == 12 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_private_data) == 20 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_parent) == 24 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_desc) == 28 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list) == 32 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list) == 20);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode) == 36 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, wait_for_v_blank) == 40 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.wait_for_v_blank) == 4);
    assert!(offset_of!(IDXGIOutput1Vtbl, take_ownership) == 44 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.take_ownership) == 12);
    assert!(offset_of!(IDXGIOutput1Vtbl, release_ownership) == 48 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release_ownership) == 4);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control_capabilities) == 52 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control_capabilities) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_gamma_control) == 56 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_gamma_control) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control) == 60 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_display_surface) == 64 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_display_surface) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data) == 68 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_frame_statistics) == 72 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_frame_statistics) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list1) == 76 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list1) == 20);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode1) == 80 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode1) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data1) == 84 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data1) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, duplicate_output) == 88 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.duplicate_output) == 12);
    assert!(size_of::<IDXGIAdapterVtbl>() == 40);
    assert!(offset_of!(IDXGIAdapterVtbl, query_interface) == 0 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIAdapterVtbl, add_ref) == 4 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIAdapterVtbl, release) == 8 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data) == 12 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data_interface) == 16 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIAdapterVtbl, get_private_data) == 20 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIAdapterVtbl, get_parent) == 24 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIAdapterVtbl, enum_outputs) == 28 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.enum_outputs) == 12);
    assert!(offset_of!(IDXGIAdapterVtbl, get_desc) == 32 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, check_interface_support) == 36 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.check_interface_support) == 12);
    assert!(size_of::<IDXGIAdapter1Vtbl>() == 44);
    assert!(offset_of!(IDXGIAdapter1Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIAdapter1Vtbl, add_ref) == 4 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIAdapter1Vtbl, release) == 8 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data) == 12 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_private_data) == 20 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_parent) == 24 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIAdapter1Vtbl, enum_outputs) == 28 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.enum_outputs) == 12);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc) == 32 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, check_interface_support) == 36 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.check_interface_support) == 12);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc1) == 40 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(size_of::<IDXGIAdapter2Vtbl>() == 48);
    assert!(offset_of!(IDXGIAdapter2Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIAdapter2Vtbl, add_ref) == 4 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIAdapter2Vtbl, release) == 8 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data) == 12 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_private_data) == 20 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_parent) == 24 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIAdapter2Vtbl, enum_outputs) == 28 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.enum_outputs) == 12);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc) == 32 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, check_interface_support) == 36 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.check_interface_support) == 12);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc1) == 40 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc2) == 44 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc2) == 8);
    assert!(size_of::<IDXGIAdapter3Vtbl>() == 72);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIAdapter3Vtbl, add_ref) == 4 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIAdapter3Vtbl, release) == 8 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data) == 12 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_private_data) == 20 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_parent) == 24 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIAdapter3Vtbl, enum_outputs) == 28 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.enum_outputs) == 12);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc) == 32 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, check_interface_support) == 36 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.check_interface_support) == 12);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc1) == 40 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc2) == 44 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc2) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_hardware_content_protection_teardown_status_event) == 48 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 12);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_hardware_content_protection_teardown_status) == 52 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_video_memory_info) == 56 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_video_memory_info) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_video_memory_reservation) == 60 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_video_memory_reservation) == 20);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_video_memory_budget_change_notification_event) == 64 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 12);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_video_memory_budget_change_notification) == 68 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 8);
    assert!(size_of::<IDXGIAdapter4Vtbl>() == 76);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_interface) == 12);
    assert!(offset_of!(IDXGIAdapter4Vtbl, add_ref) == 4 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.add_ref) == 4);
    assert!(offset_of!(IDXGIAdapter4Vtbl, release) == 8 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.release) == 4);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data) == 12 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data_interface) == 16 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data_interface) == 12);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_private_data) == 20 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_private_data) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_parent) == 24 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_parent) == 12);
    assert!(offset_of!(IDXGIAdapter4Vtbl, enum_outputs) == 28 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.enum_outputs) == 12);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc) == 32 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, check_interface_support) == 36 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.check_interface_support) == 12);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc1) == 40 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc1) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc2) == 44 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc2) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_hardware_content_protection_teardown_status_event) == 48 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 12);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_hardware_content_protection_teardown_status) == 52 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_video_memory_info) == 56 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_video_memory_info) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_video_memory_reservation) == 60 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_video_memory_reservation) == 20);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_video_memory_budget_change_notification_event) == 64 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 12);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_video_memory_budget_change_notification) == 68 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc3) == 72 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc3) == 8);
};

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
const _: () = {
    assert!(size_of::<IUnknownVtbl>() == 24);
    assert!(offset_of!(IUnknownVtbl, query_interface) == 0 && slot_params::<IUnknownVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IUnknownVtbl, add_ref) == 8 && slot_params::<IUnknownVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IUnknownVtbl, release) == 16 && slot_params::<IUnknownVtbl, _>(|v| &v.release) == 8);
    assert!(size_of::<IDXGIObjectVtbl>() == 56);
    assert!(offset_of!(IDXGIObjectVtbl, query_interface) == 0 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, add_ref) == 8 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, release) == 16 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIObjectVtbl, get_parent) == 48 && slot_params::<IDXGIObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(size_of::<IDXGIDeviceSubObjectVtbl>() == 64);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, query_interface) == 0 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, add_ref) == 8 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, release) == 16 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data) == 24 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_private_data) == 40 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_parent) == 48 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIDeviceSubObjectVtbl, get_device) == 56 && slot_params::<IDXGIDeviceSubObjectVtbl, _>(|v| &v.get_device) == 24);
    assert!(size_of::<IDXGISwapChainVtbl>() == 144);
    assert!(offset_of!(IDXGISwapChainVtbl, query_interface) == 0 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, add_ref) == 8 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, release) == 16 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data) == 24 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_private_data) == 40 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, get_parent) == 48 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_device) == 56 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, present) == 64 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_buffer) == 72 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChainVtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChainVtbl, get_desc) == 96 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChainVtbl, resize_target) == 112 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChainVtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChainVtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(size_of::<IDXGISwapChain1Vtbl>() == 232);
    assert!(offset_of!(IDXGISwapChain1Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, release) == 16 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present) == 64 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain1Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain1Vtbl, present1) == 176 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain1Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain1Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain1Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(size_of::<IDXGISwapChain2Vtbl>() == 288);
    assert!(offset_of!(IDXGISwapChain2Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, release) == 16 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present) == 64 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain2Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, present1) == 176 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain2Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain2Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain2Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain2Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(size_of::<IDXGISwapChain3Vtbl>() == 320);
    assert!(offset_of!(IDXGISwapChain3Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, release) == 16 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present) == 64 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, present1) == 176 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain3Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain3Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain3Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain3Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain3Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(size_of::<IDXGISwapChain4Vtbl>() == 328);
    assert!(offset_of!(IDXGISwapChain4Vtbl, query_interface) == 0 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, add_ref) == 8 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, release) == 16 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data) == 24 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_private_data) == 40 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_parent) == 48 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_device) == 56 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_device) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present) == 64 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_buffer) == 72 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_buffer) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_fullscreen_state) == 80 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_state) == 88 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_state) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc) == 96 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers) == 104 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers) == 48);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_target) == 112 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_target) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_containing_output) == 120 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_containing_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_statistics) == 128 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_last_present_count) == 136 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_last_present_count) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_desc1) == 144 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_fullscreen_desc) == 152 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_fullscreen_desc) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_hwnd) == 160 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_hwnd) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_core_window) == 168 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_core_window) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, present1) == 176 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.present1) == 32);
    assert!(offset_of!(IDXGISwapChain4Vtbl, is_temporary_mono_supported) == 184 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.is_temporary_mono_supported) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_restrict_to_output) == 192 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_restrict_to_output) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_background_color) == 200 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_background_color) == 208 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_background_color) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_rotation) == 216 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_rotation) == 224 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_rotation) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_source_size) == 232 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_source_size) == 240 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_source_size) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_maximum_frame_latency) == 248 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_maximum_frame_latency) == 256 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_maximum_frame_latency) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_frame_latency_waitable_object) == 264 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_frame_latency_waitable_object) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_matrix_transform) == 272 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_matrix_transform) == 280 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_matrix_transform) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, get_current_back_buffer_index) == 288 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.get_current_back_buffer_index) == 8);
    assert!(offset_of!(IDXGISwapChain4Vtbl, check_color_space_support) == 296 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.check_color_space_support) == 24);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_color_space1) == 304 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_color_space1) == 16);
    assert!(offset_of!(IDXGISwapChain4Vtbl, resize_buffers1) == 312 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.resize_buffers1) == 64);
    assert!(offset_of!(IDXGISwapChain4Vtbl, set_hdr_meta_data) == 320 && slot_params::<IDXGISwapChain4Vtbl, _>(|v| &v.set_hdr_meta_data) == 32);
    assert!(size_of::<IDXGIFactoryVtbl>() == 96);
    assert!(offset_of!(IDXGIFactoryVtbl, query_interface) == 0 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, add_ref) == 8 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, release) == 16 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data) == 24 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_private_data) == 40 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, get_parent) == 48 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, enum_adapters) == 56 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, make_window_association) == 64 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactoryVtbl, get_window_association) == 72 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactoryVtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactoryVtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactoryVtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(size_of::<IDXGIFactory1Vtbl>() == 112);
    assert!(offset_of!(IDXGIFactory1Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, release) == 16 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory1Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory1Vtbl, is_current) == 104 && slot_params::<IDXGIFactory1Vtbl, _>(|v| &v.is_current) == 8);
    assert!(size_of::<IDXGIFactory2Vtbl>() == 200);
    assert!(offset_of!(IDXGIFactory2Vtbl, query_interface) == 0 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, add_ref) == 8 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, release) == 16 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data) == 24 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_private_data) == 40 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_parent) == 48 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters) == 56 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, make_window_association) == 64 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.make_window_association) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_window_association) == 72 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_window_association) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain) == 80 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_software_adapter) == 88 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_software_adapter) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, enum_adapters1) == 96 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.enum_adapters1) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_current) == 104 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_current) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, is_windowed_stereo_enabled) == 112 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.is_windowed_stereo_enabled) == 8);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_hwnd) == 120 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_hwnd) == 56);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_core_window) == 128 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_core_window) == 48);
    assert!(offset_of!(IDXGIFactory2Vtbl, get_shared_resource_adapter_luid) == 136 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.get_shared_resource_adapter_luid) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_window) == 144 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_stereo_status_event) == 152 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_stereo_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_stereo_status) == 160 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_stereo_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_window) == 168 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_window) == 32);
    assert!(offset_of!(IDXGIFactory2Vtbl, register_occlusion_status_event) == 176 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.register_occlusion_status_event) == 24);
    assert!(offset_of!(IDXGIFactory2Vtbl, unregister_occlusion_status) == 184 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.unregister_occlusion_status) == 16);
    assert!(offset_of!(IDXGIFactory2Vtbl, create_swap_chain_for_composition) == 192 && slot_params::<IDXGIFactory2Vtbl, _>(|v| &v.create_swap_chain_for_composition) == 40);
    assert!(size_of::<IDXGIOutputVtbl>() == 152);
    assert!(offset_of!(IDXGIOutputVtbl, query_interface) == 0 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, add_ref) == 8 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, release) == 16 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data) == 24 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_private_data) == 40 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, get_parent) == 48 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, get_desc) == 56 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutputVtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutputVtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, take_ownership) == 88 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutputVtbl, release_ownership) == 96 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, set_display_surface) == 128 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutputVtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutputVtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(size_of::<IDXGIOutput1Vtbl>() == 184);
    assert!(offset_of!(IDXGIOutput1Vtbl, query_interface) == 0 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, add_ref) == 8 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, release) == 16 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data) == 24 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_private_data) == 40 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_parent) == 48 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_desc) == 56 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list) == 64 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode) == 72 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, wait_for_v_blank) == 80 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.wait_for_v_blank) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, take_ownership) == 88 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.take_ownership) == 24);
    assert!(offset_of!(IDXGIOutput1Vtbl, release_ownership) == 96 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.release_ownership) == 8);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control_capabilities) == 104 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control_capabilities) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_gamma_control) == 112 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_gamma_control) == 120 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_gamma_control) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, set_display_surface) == 128 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.set_display_surface) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data) == 136 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_frame_statistics) == 144 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_frame_statistics) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_mode_list1) == 152 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_mode_list1) == 40);
    assert!(offset_of!(IDXGIOutput1Vtbl, find_closest_matching_mode1) == 160 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.find_closest_matching_mode1) == 32);
    assert!(offset_of!(IDXGIOutput1Vtbl, get_display_surface_data1) == 168 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.get_display_surface_data1) == 16);
    assert!(offset_of!(IDXGIOutput1Vtbl, duplicate_output) == 176 && slot_params::<IDXGIOutput1Vtbl, _>(|v| &v.duplicate_output) == 24);
    assert!(size_of::<IDXGIAdapterVtbl>() == 80);
    assert!(offset_of!(IDXGIAdapterVtbl, query_interface) == 0 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, add_ref) == 8 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, release) == 16 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data) == 24 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_private_data) == 40 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapterVtbl, get_parent) == 48 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapterVtbl, get_desc) == 64 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapterVtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapterVtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(size_of::<IDXGIAdapter1Vtbl>() == 88);
    assert!(offset_of!(IDXGIAdapter1Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, release) == 16 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter1Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter1Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter1Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(size_of::<IDXGIAdapter2Vtbl>() == 96);
    assert!(offset_of!(IDXGIAdapter2Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, release) == 16 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter2Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter2Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(size_of::<IDXGIAdapter3Vtbl>() == 144);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, release) == 16 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter3Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter3Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter3Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter3Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(size_of::<IDXGIAdapter4Vtbl>() == 152);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_interface) == 0 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, add_ref) == 8 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.add_ref) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, release) == 16 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.release) == 8);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data) == 24 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_private_data_interface) == 32 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_private_data_interface) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_private_data) == 40 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_private_data) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_parent) == 48 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_parent) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, enum_outputs) == 56 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.enum_outputs) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc) == 64 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, check_interface_support) == 72 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.check_interface_support) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc1) == 80 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc1) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc2) == 88 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc2) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_hardware_content_protection_teardown_status_event) == 96 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_hardware_content_protection_teardown_status_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_hardware_content_protection_teardown_status) == 104 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_hardware_content_protection_teardown_status) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, query_video_memory_info) == 112 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.query_video_memory_info) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, set_video_memory_reservation) == 120 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.set_video_memory_reservation) == 32);
    assert!(offset_of!(IDXGIAdapter4Vtbl, register_video_memory_budget_change_notification_event) == 128 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.register_video_memory_budget_change_notification_event) == 24);
    assert!(offset_of!(IDXGIAdapter4Vtbl, unregister_video_memory_budget_change_notification) == 136 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.unregister_video_memory_budget_change_notification) == 16);
    assert!(offset_of!(IDXGIAdapter4Vtbl, get_desc3) == 144 && slot_params::<IDXGIAdapter4Vtbl, _>(|v| &v.get_desc3) == 16);
};
