//! Format support and `CheckFeatureSupport` tables (`d3d11_inspection.cpp`).

use core::ffi::c_void;

use dxmt_core::format::{
    FORMAT_SUPPORT_BUFFER, FORMAT_SUPPORT_CPU_LOCKABLE, FORMAT_SUPPORT2_ALL_ATOMICS_AND_SHAREABLE,
    FORMAT_SUPPORT2_UAV_TYPED_LOAD, FORMAT_SUPPORT2_UAV_TYPED_STORE, MtlDxgiFormatDesc,
    capability_map, device_capability_map, format_support1, format_support2, query_dxgi_ref,
    supports_sample_count,
};
use dxmt_util::com::{E_INVALIDARG, Hresult, S_OK};
use rustc_hash::FxHashMap;
use winemetal_sys::MTLDevice_hasUnifiedMemory;
use winemetal_wire::{
    WMT_PIXEL_FORMAT_INVALID, WMTGPU_FAMILY_APPLE7, WMTGPU_FAMILY_APPLE8, WMTGPU_FAMILY_APPLE9,
    original_format,
};

#[cfg(test)]
mod tests;

const D3D11_FEATURE_THREADING: u32 = 0;
const D3D11_FEATURE_DOUBLES: u32 = 1;
const D3D11_FEATURE_FORMAT_SUPPORT: u32 = 2;
const D3D11_FEATURE_FORMAT_SUPPORT2: u32 = 3;
const D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS: u32 = 4;
const D3D11_FEATURE_D3D11_OPTIONS: u32 = 5;
const D3D11_FEATURE_ARCHITECTURE_INFO: u32 = 6;
const D3D11_FEATURE_D3D9_OPTIONS: u32 = 7;
const D3D11_FEATURE_SHADER_MIN_PRECISION_SUPPORT: u32 = 8;
const D3D11_FEATURE_D3D9_SHADOW_SUPPORT: u32 = 9;
const D3D11_FEATURE_D3D11_OPTIONS1: u32 = 10;
const D3D11_FEATURE_D3D11_OPTIONS2: u32 = 14;
const D3D11_FEATURE_D3D11_OPTIONS3: u32 = 15;
const D3D11_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT: u32 = 16;
const D3D11_FEATURE_SHADER_CACHE: u32 = 18;
const D3D11_FEATURE_D3D11_OPTIONS5: u32 = 19;

/// Resolved DXGI format descriptor together with its Metal format capability bits.
pub struct FormatInfo {
    /// Base DXGI format descriptor from `dxmt_core::format::query_dxgi_ref`.
    pub desc: &'static MtlDxgiFormatDesc,
    /// Combined `CAP_*` capability mask for `desc.pixel_format`.
    pub capability: u32,
}

/// Cached Metal device feature and format capability inspector (`MTLD3D11Inspection`).
pub struct Inspection {
    metal_device: u64,
    caps: FxHashMap<u32, u32>,
    unified_memory: bool,
}

impl Inspection {
    /// Creates an `Inspection` table by querying a live `WMTDevice` handle.
    ///
    /// When `metal_device` is 0 (e.g. in unit tests), defaults to the
    /// Apple8 + BC unified-memory profile.
    #[must_use]
    pub fn new(metal_device: u64) -> Self {
        if metal_device == 0 {
            return Self::default_apple8_profile();
        }
        // SAFETY: `metal_device` is a valid non-zero `WMTDevice` handle.
        let unified_memory = unsafe { MTLDevice_hasUnifiedMemory(metal_device) };
        // SAFETY: as above.
        let caps = unsafe { device_capability_map(metal_device) };
        Self {
            metal_device,
            caps,
            unified_memory,
        }
    }

    /// Creates an `Inspection` table for the Apple9 + BC unified-memory profile.
    #[must_use]
    pub fn default_apple8_profile() -> Self {
        let supports_family = |family| {
            matches!(
                family,
                WMTGPU_FAMILY_APPLE7 | WMTGPU_FAMILY_APPLE8 | WMTGPU_FAMILY_APPLE9
            )
        };
        Self {
            metal_device: 0,
            caps: capability_map(supports_family, true),
            unified_memory: true,
        }
    }

    /// Queries the DXGI format descriptor and its capability mask.
    #[must_use]
    pub fn query_dxgi_format(&self, format: u32) -> Option<FormatInfo> {
        let (hr, desc) = query_dxgi_ref(format);
        if hr < 0 {
            return None;
        }
        let orig = original_format(desc.pixel_format);
        let capability = self.caps.get(&orig).copied().unwrap_or(0);
        Some(FormatInfo { desc, capability })
    }

    /// Implements `ID3D11Device::CheckFormatSupport`.
    #[must_use]
    pub fn check_format_support(&self, format: u32, p_format_support: *mut u32) -> Hresult {
        self.check_format_support_impl(format, p_format_support)
    }

    fn check_format_support_impl(&self, format: u32, p_format_support: *mut u32) -> Hresult {
        if !p_format_support.is_null() {
            // SAFETY: `p_format_support` is non-null.
            unsafe {
                *p_format_support = 0;
            }
        }
        if format == 0 {
            if !p_format_support.is_null() {
                // SAFETY: `p_format_support` is non-null.
                unsafe {
                    *p_format_support = FORMAT_SUPPORT_BUFFER | FORMAT_SUPPORT_CPU_LOCKABLE;
                }
            }
            return S_OK;
        }
        let Some(metal_format) = self.query_dxgi_format(format) else {
            return E_INVALIDARG;
        };
        let out_support = format_support1(format, metal_format.desc, metal_format.capability);
        if !p_format_support.is_null() {
            // SAFETY: `p_format_support` is non-null.
            unsafe {
                *p_format_support = out_support;
            }
        }
        S_OK
    }

    /// Implements `CheckMultisampleQualityLevels` and `CheckMultisampleQualityLevels1`.
    #[must_use]
    pub fn check_multisample_quality_levels(
        &self,
        format: u32,
        sample_count: u32,
        flags: u32,
        p_num_quality_levels: *mut u32,
    ) -> Hresult {
        self.check_multisample_quality_levels_impl(
            format,
            sample_count,
            flags,
            p_num_quality_levels,
        )
    }

    fn check_multisample_quality_levels_impl(
        &self,
        format: u32,
        sample_count: u32,
        flags: u32,
        p_num_quality_levels: *mut u32,
    ) -> Hresult {
        if p_num_quality_levels.is_null() {
            return E_INVALIDARG;
        }
        if flags != 0 {
            dxmt_util::err!(
                crate::LOG,
                "CheckMultisampleQualityLevels1: unsupported flags {flags}"
            );
            return E_INVALIDARG;
        }
        // SAFETY: `p_num_quality_levels` was checked non-null above.
        unsafe {
            *p_num_quality_levels = 0;
        }
        let Some(metal_format) = self.query_dxgi_format(format) else {
            return E_INVALIDARG;
        };
        if metal_format.desc.pixel_format == WMT_PIXEL_FORMAT_INVALID {
            return E_INVALIDARG;
        }
        if self.supports_sample_count(sample_count) {
            // SAFETY: `p_num_quality_levels` was checked non-null above.
            unsafe {
                *p_num_quality_levels = 1;
            }
        }
        S_OK
    }

    /// Implements `ID3D11Device::CheckFeatureSupport` (`d3d11_inspection.cpp`).
    #[must_use]
    pub fn check_feature_support(
        &self,
        feature: u32,
        p_feature_support_data: *mut c_void,
        feature_support_data_size: u32,
    ) -> Hresult {
        self.check_feature_support_impl(feature, p_feature_support_data, feature_support_data_size)
    }

    fn check_feature_support_impl(
        &self,
        feature: u32,
        p_feature_support_data: *mut c_void,
        feature_support_data_size: u32,
    ) -> Hresult {
        match feature {
            D3D11_FEATURE_FORMAT_SUPPORT => {
                if p_feature_support_data.is_null() || feature_support_data_size != 8 {
                    return E_INVALIDARG;
                }
                let words = p_feature_support_data.cast::<u32>();
                // SAFETY: `words` is non-null and `feature_support_data_size == 8`.
                let in_format = unsafe { *words };
                // SAFETY: `words.add(1)` points to `OutFormatSupport` in the 8-byte struct.
                let out_ptr = unsafe { words.add(1) };
                self.check_format_support(in_format, out_ptr)
            }
            D3D11_FEATURE_FORMAT_SUPPORT2 => {
                if p_feature_support_data.is_null() || feature_support_data_size != 8 {
                    return E_INVALIDARG;
                }
                let words = p_feature_support_data.cast::<u32>();
                // SAFETY: `words` is non-null and `feature_support_data_size == 8`.
                let in_format = unsafe { *words };
                // SAFETY: `words.add(1)` points to `OutFormatSupport2` in the 8-byte struct.
                let out_ptr = unsafe { words.add(1) };
                self.check_format_support2(in_format, out_ptr)
            }
            D3D11_FEATURE_THREADING => {
                write_words(p_feature_support_data, feature_support_data_size, &[1, 1])
            }
            D3D11_FEATURE_DOUBLES | D3D11_FEATURE_D3D11_OPTIONS5 => {
                write_words(p_feature_support_data, feature_support_data_size, &[0])
            }
            D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS
            | D3D11_FEATURE_D3D9_OPTIONS
            | D3D11_FEATURE_D3D9_SHADOW_SUPPORT
            | D3D11_FEATURE_D3D11_OPTIONS3 => {
                write_words(p_feature_support_data, feature_support_data_size, &[1])
            }
            D3D11_FEATURE_ARCHITECTURE_INFO => write_words(
                p_feature_support_data,
                feature_support_data_size,
                &[u32::from(self.unified_memory)],
            ),
            D3D11_FEATURE_D3D11_OPTIONS => write_words(
                p_feature_support_data,
                feature_support_data_size,
                &[1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1],
            ),
            D3D11_FEATURE_SHADER_MIN_PRECISION_SUPPORT => {
                write_words(p_feature_support_data, feature_support_data_size, &[2, 2])
            }
            D3D11_FEATURE_D3D11_OPTIONS1 => write_words(
                p_feature_support_data,
                feature_support_data_size,
                &[0, 0, 1, 1],
            ),
            D3D11_FEATURE_D3D11_OPTIONS2 => write_words(
                p_feature_support_data,
                feature_support_data_size,
                &[1, 1, 1, 0, 0, 1, 1, 0],
            ),
            D3D11_FEATURE_GPU_VIRTUAL_ADDRESS_SUPPORT => {
                write_words(p_feature_support_data, feature_support_data_size, &[32, 40])
            }
            D3D11_FEATURE_SHADER_CACHE => {
                write_words(p_feature_support_data, feature_support_data_size, &[3])
            }
            _ => {
                dxmt_util::err!(crate::LOG, "Not supported feature: {feature}");
                E_INVALIDARG
            }
        }
    }

    fn check_format_support2(&self, in_format: u32, out_ptr: *mut u32) -> Hresult {
        // SAFETY: `out_ptr` is non-null.
        unsafe {
            *out_ptr = 0;
        }
        if in_format == 0 {
            // SAFETY: `out_ptr` is non-null.
            unsafe {
                *out_ptr = FORMAT_SUPPORT2_ALL_ATOMICS_AND_SHAREABLE
                    | FORMAT_SUPPORT2_UAV_TYPED_LOAD
                    | FORMAT_SUPPORT2_UAV_TYPED_STORE;
            }
            return S_OK;
        }
        let Some(metal_format) = self.query_dxgi_format(in_format) else {
            return E_INVALIDARG;
        };
        let out_support2 = format_support2(metal_format.capability);
        // SAFETY: `out_ptr` is non-null.
        unsafe {
            *out_ptr = out_support2;
        }
        S_OK
    }

    fn supports_sample_count(&self, sample_count: u32) -> bool {
        // SAFETY: `self.metal_device` is 0 or a live `WMTDevice` handle.
        unsafe { supports_sample_count(self.metal_device, sample_count) }
    }
}

fn write_words(dst: *mut c_void, size: u32, words: &[u32]) -> Hresult {
    let Ok(expected) = u32::try_from(core::mem::size_of_val(words)) else {
        return E_INVALIDARG;
    };
    if dst.is_null() || size != expected {
        return E_INVALIDARG;
    }
    let out = dst.cast::<u32>();
    for (idx, &word) in words.iter().enumerate() {
        // SAFETY: `dst` is non-null and `size == words.len() * 4`.
        let slot = unsafe { out.add(idx) };
        // SAFETY: `slot` points to a valid `u32` element in `dst`.
        unsafe {
            *slot = word;
        }
    }
    S_OK
}
