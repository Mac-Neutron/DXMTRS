//! D3D11 input layouts and stream-output layouts (`d3d11_input_layout.{hpp,cpp}`).

use core::{
    ffi::{CStr, c_void},
    ptr,
};

use airconv_ir::{
    container::{Code, Container},
    signature::{input, output_v4},
    tokens::sb_name,
};
use dxmt_core::format::query_dxgi;
use dxmt_util::{
    com::{Qi, QiMiss, qi_table},
    private_data::{
        HasPrivateData, PrivateData, get_private_data, set_private_data, set_private_data_interface,
    },
};
use winemetal_wire::WMT_ATTRIBUTE_FORMAT_INVALID;

use crate::{
    LOG,
    abi::{
        IID_ID3D10_DEVICE_CHILD, IID_ID3D10_INPUT_LAYOUT, IID_ID3D11_DEVICE_CHILD,
        IID_ID3D11_GEOMETRY_SHADER, IID_ID3D11_INPUT_LAYOUT, IID_IUNKNOWN,
        records::{D3d11InputElementDesc, D3d11SoDeclarationEntry},
        vtbl::{
            ID3D10GeometryShaderVtbl, ID3D10InputLayoutVtbl, ID3D11GeometryShaderVtbl,
            ID3D11InputLayoutVtbl,
        },
    },
    com::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY, Guid, Hresult, Managed, S_FALSE, S_OK, add_ref},
    device::Device,
    shader::{
        IID_IMTL_D3D11_SHADER,
        args::{input_layout_digest, stream_output_layout_digest},
    },
};

/// `IID_IMTLD3D11InputLayout` (`d3d11_input_layout.hpp:10`).
pub const IID_IMTL_D3D11_INPUT_LAYOUT: Guid = Guid::new(
    0xb56c_6a99,
    0x80cf,
    0x4c7f,
    [0xa7, 0x56, 0x9e, 0x9c, 0xeb, 0x38, 0x73, 0x0f],
);

/// `IID_IMTLD3D11StreamOutputLayout` (`d3d11_input_layout.hpp:18`).
pub const IID_IMTL_D3D11_STREAM_OUTPUT_LAYOUT: Guid = Guid::new(
    0xfd58_f76b,
    0x7c22,
    0x4605,
    [0xb4, 0x3c, 0x28, 0x04, 0x8c, 0x8b, 0x4a, 0x64],
);

/// `D3D11_APPEND_ALIGNED_ELEMENT` (`0xffffffff`).
pub const D3D11_APPEND_ALIGNED_ELEMENT: u32 = 0xffff_ffff;

/// `D3D11_INPUT_PER_VERTEX_DATA` (`0`).
pub const D3D11_INPUT_PER_VERTEX_DATA: u32 = 0;

/// `D3D11_INPUT_PER_INSTANCE_DATA` (`1`).
pub const D3D11_INPUT_PER_INSTANCE_DATA: u32 = 1;

/// Single translated input-layout element (`MTL_SHADER_INPUT_LAYOUT_ELEMENT_DESC`).
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct InputLayoutElement {
    pub index: u32,
    pub slot: u32,
    pub offset: u32,
    pub format: u32,
    pub step_function_and_rate: u32,
}

impl core::fmt::Debug for InputLayoutElement {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InputLayoutElement")
            .field("index", &self.index)
            .field("slot", &self.slot)
            .field("offset", &self.offset)
            .field("format", &self.format)
            .field("step_function_and_rate", &self.step_function_and_rate)
            .finish()
    }
}

impl PartialEq for InputLayoutElement {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index
            && self.slot == other.slot
            && self.offset == other.offset
            && self.format == other.format
            && self.step_function_and_rate == other.step_function_and_rate
    }
}

impl Eq for InputLayoutElement {}

impl core::hash::Hash for InputLayoutElement {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.slot.hash(state);
        self.offset.hash(state);
        self.format.hash(state);
        self.step_function_and_rate.hash(state);
    }
}

/// Single stream-output element (`MTL_SHADER_STREAM_OUTPUT_ELEMENT_DESC`).
#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct StreamOutputElement {
    pub register_idx: u32,
    pub component: u32,
    pub output_slot: u32,
    pub offset: u32,
}

impl core::fmt::Debug for StreamOutputElement {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StreamOutputElement")
            .field("register_idx", &self.register_idx)
            .field("component", &self.component)
            .field("output_slot", &self.output_slot)
            .field("offset", &self.offset)
            .finish()
    }
}

impl PartialEq for StreamOutputElement {
    fn eq(&self, other: &Self) -> bool {
        self.register_idx == other.register_idx
            && self.component == other.component
            && self.output_slot == other.output_slot
            && self.offset == other.offset
    }
}

impl Eq for StreamOutputElement {}

impl core::hash::Hash for StreamOutputElement {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.register_idx.hash(state);
        self.component.hash(state);
        self.output_slot.hash(state);
        self.offset.hash(state);
    }
}

/// Input layout object (`MTLD3D11InputLayout` / `dxmt::InputLayout`).
#[derive(Debug, PartialEq, Eq)]
pub struct InputLayout {
    elements: Vec<InputLayoutElement>,
    slot_mask: u32,
    sha1: [u8; 20],
}

impl Clone for InputLayout {
    fn clone(&self) -> Self {
        Self {
            elements: self.elements.clone(),
            slot_mask: self.slot_mask,
            sha1: self.sha1,
        }
    }
}

impl InputLayout {
    /// Creates an [`InputLayout`] and computes its SHA-1 digest.
    #[must_use]
    pub fn new(elements: Vec<InputLayoutElement>, slot_mask: u32) -> Self {
        let sha1 = input_layout_digest(&elements, slot_mask);
        Self {
            elements,
            slot_mask,
            sha1,
        }
    }

    /// Recovers `&InputLayout` from an `ID3D11InputLayout*` pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: `iface` is non-null; its first word is the COM vtable pointer.
        let vtbl = unsafe { *iface.cast::<*const ID3D11InputLayoutVtbl>() };
        if !ptr::eq(vtbl, &raw const INPUT_LAYOUT_VTBL) {
            return None;
        }
        let obj = iface.cast::<InputLayoutObject>();
        // SAFETY: `vtbl` matches `INPUT_LAYOUT_VTBL`, so `iface` was allocated by `create_input_layout`.
        Some(unsafe { &(*obj).inner })
    }

    /// Returns the translated input-layout elements (`input_layout_element`).
    #[must_use]
    pub fn elements(&self) -> &[InputLayoutElement] {
        &self.elements
    }

    /// Returns the active vertex-buffer slot bitmask (`input_slot_mask`).
    #[must_use]
    pub const fn slot_mask(&self) -> u32 {
        self.slot_mask
    }

    /// Alias for [`Self::slot_mask`] matching `CachedInputLayout::input_slot_mask()`.
    #[must_use]
    pub const fn input_slot_mask(&self) -> u32 {
        self.slot_mask
    }

    /// Returns the 20-byte SHA-1 digest of the layout (`sha1`).
    #[must_use]
    pub const fn sha1(&self) -> &[u8; 20] {
        &self.sha1
    }
}

/// Resolved stream-output declaration (`MTLD3D11StreamOutputLayout` in `d3d11_input_layout.cpp`).
#[derive(Debug, PartialEq, Eq)]
pub struct StreamOutputLayout {
    pub elements: Vec<StreamOutputElement>,
    pub strides: [u32; 4],
    pub rasterized_stream: u32,
    pub sha1: [u8; 20],
}

impl Clone for StreamOutputLayout {
    fn clone(&self) -> Self {
        Self {
            elements: self.elements.clone(),
            strides: self.strides,
            rasterized_stream: self.rasterized_stream,
            sha1: self.sha1,
        }
    }
}

impl StreamOutputLayout {
    /// Creates a [`StreamOutputLayout`] and computes its SHA-1 digest.
    #[must_use]
    pub fn new(
        elements: Vec<StreamOutputElement>,
        strides: [u32; 4],
        rasterized_stream: u32,
    ) -> Self {
        let sha1 = stream_output_layout_digest(&elements, &strides, rasterized_stream);
        Self {
            elements,
            strides,
            rasterized_stream,
            sha1,
        }
    }

    /// Recovers `&StreamOutputLayout` from an `ID3D11GeometryShader*` interface pointer.
    #[must_use]
    pub fn from_iface(iface: *mut c_void) -> Option<&'static Self> {
        if iface.is_null() {
            return None;
        }
        // SAFETY: `iface` is non-null; its first word is the COM vtable pointer.
        let vtbl = unsafe { *iface.cast::<*const ID3D11GeometryShaderVtbl>() };
        if !ptr::eq(vtbl, &raw const SO_LAYOUT_VTBL) {
            return None;
        }
        let obj = iface.cast::<StreamOutputLayoutObject>();
        // SAFETY: `vtbl` matches `SO_LAYOUT_VTBL`, so `iface` was allocated by `create_gs_with_so`.
        Some(unsafe { &(*obj).inner })
    }
}

const fn map_dxbc_err(err: &airconv_ir::container::Error) -> Hresult {
    match err.code {
        Code::Fail => E_FAIL,
        Code::InvalidArg => E_INVALIDARG,
        Code::OutOfMemory => E_OUTOFMEMORY,
    }
}

fn log_err(msg: &str) {
    if !cfg!(test) {
        LOG.err(msg);
    }
}

fn log_warn(msg: &str) {
    if !cfg!(test) {
        LOG.warn(msg);
    }
}

/// Parses `D3D11_INPUT_ELEMENT_DESC` entries against the DXBC input signature.
///
/// # Errors
///
/// Returns `E_FAIL` if the DXBC input signature is missing or a DXGI format is unknown, or
/// `E_INVALIDARG` if a required vertex shader input register is missing from `descs`.
pub fn parse_input_layout(
    descs: &[D3d11InputElementDesc],
    bytecode: &[u8],
) -> Result<InputLayout, Hresult> {
    let container = Container::parse(bytecode).map_err(|e| map_dxbc_err(&e))?;
    let sig = input(&container).map_err(|e| map_dxbc_err(&e))?;

    let mut elements = Vec::with_capacity(descs.len());
    let mut slot_mask = 0u32;
    let mut append_offset = [0u16; 32];
    let mut register_mask = 0u32;

    for d in descs {
        if d.semantic_name.is_null() {
            return Err(E_INVALIDARG);
        }
        // SAFETY: `d.semantic_name` is non-null and NUL-terminated per D3D11 contract.
        let sem_bytes = unsafe { CStr::from_ptr(d.semantic_name) }.to_bytes();

        let (hr, fmt_desc) = query_dxgi(d.format);
        if hr < 0 {
            log_err(&format!(
                "CreateInputLayout: Unsupported vertex format: {}",
                d.format
            ));
            return Err(E_FAIL);
        }
        if fmt_desc.attribute_format == WMT_ATTRIBUTE_FORMAT_INVALID {
            log_err(&format!(
                "CreateInputLayout: Unsupported vertex format: {}",
                d.format
            ));
            return Err(E_INVALIDARG);
        }
        if fmt_desc.bytes_per_texel == 0 {
            log_err(&format!(
                "CreateInputLayout: not an ordinary or packed format: {}",
                d.format
            ));
            return Err(E_INVALIDARG);
        }
        if d.input_slot >= 32 {
            return Err(E_INVALIDARG);
        }

        let slot = d.input_slot as usize;
        let align_to = fmt_desc.bytes_per_texel.min(4);
        let aligned_byte_offset = if d.aligned_byte_offset == D3D11_APPEND_ALIGNED_ELEMENT {
            let unaligned = u32::from(append_offset[slot]);
            if align_to > 1 {
                (unaligned + align_to - 1) & !(align_to - 1)
            } else {
                unaligned
            }
        } else {
            d.aligned_byte_offset
        };
        append_offset[slot] =
            u16::try_from(aligned_byte_offset + fmt_desc.bytes_per_texel).unwrap_or(u16::MAX);

        let Some(sig_param) = sig.parameters.iter().find(|p| {
            p.semantic_index == d.semantic_index && p.semantic_name.eq_ignore_ascii_case(sem_bytes)
        }) else {
            // Shader has no such input register, so skip it (`d3d11_input_layout.cpp:61-62`).
            continue;
        };

        let step_func = d.input_slot_class & 1;
        let step_rate = if d.input_slot_class == D3D11_INPUT_PER_INSTANCE_DATA {
            d.instance_data_step_rate
        } else {
            1
        };
        let step_function_and_rate = (step_rate << 1) | step_func;

        elements.push(InputLayoutElement {
            index: sig_param.register,
            slot: d.input_slot,
            offset: aligned_byte_offset,
            format: fmt_desc.attribute_format,
            step_function_and_rate,
        });
        slot_mask |= 1u32 << d.input_slot;
        register_mask |= 1u32 << sig_param.register;
    }

    for p in &sig.parameters {
        if p.system_value != sb_name::UNDEFINED {
            continue;
        }
        if (register_mask & (1u32 << p.register)) == 0 {
            let name = core::str::from_utf8(&p.semantic_name).unwrap_or("<non-utf8>");
            log_warn(&format!(
                "CreateInputLayout: Vertex shader expects {name}_{} but it's not in input layout element descriptors",
                p.semantic_index
            ));
            return Err(E_INVALIDARG);
        }
    }

    Ok(InputLayout::new(elements, slot_mask))
}

/// Parses `D3D11_SO_DECLARATION_ENTRY` entries against the DXBC v4 output signature.
///
/// # Errors
///
/// Returns `E_FAIL` if the DXBC output signature is missing, or `E_INVALIDARG` if any entry is
/// invalid or names a semantic missing from the shader's output signature.
pub fn parse_stream_output_layout(
    bytecode: &[u8],
    entries: &[D3d11SoDeclarationEntry],
    buffer_strides: &[u32],
    rasterized_stream: u32,
) -> Result<StreamOutputLayout, Hresult> {
    let container = Container::parse(bytecode).map_err(|e| map_dxbc_err(&e))?;
    let sig = output_v4(&container).map_err(|e| map_dxbc_err(&e))?;
    let mut offsets = [0u32; 4];
    let mut elements = Vec::new();

    for entry in entries {
        if entry.stream != 0 {
            log_err("CreateEmulatedVertexStreamOutputShader: stream must be 0");
            return Err(E_INVALIDARG);
        }
        if entry.output_slot != 0 {
            log_err(&format!(
                "CreateEmulatedVertexStreamOutputShader: invalid or unsupported output slot {}",
                entry.output_slot
            ));
            return Err(E_INVALIDARG);
        }
        let diff = i32::from(entry.component_count) - i32::from(entry.start_component);
        if diff < 0 || (u32::from(entry.component_count) + u32::from(entry.start_component)) > 4 {
            log_err("CreateEmulatedVertexStreamOutputShader: invalid components");
            return Err(E_INVALIDARG);
        }
        if diff == 0 {
            continue;
        }
        let slot = entry.output_slot as usize;

        if entry.semantic_name.is_null() {
            for c in 0..entry.component_count {
                let offset = offsets[slot];
                offsets[slot] += 4;
                elements.push(StreamOutputElement {
                    register_idx: 0xffff_ffff,
                    component: u32::from(entry.start_component + c),
                    output_slot: u32::from(entry.output_slot),
                    offset,
                });
            }
            continue;
        }

        // SAFETY: `entry.semantic_name` is non-null and NUL-terminated.
        let sem_bytes = unsafe { CStr::from_ptr(entry.semantic_name) }.to_bytes();
        let Some(sig_param) = sig.parameters.iter().find(|p| {
            p.semantic_index == entry.semantic_index
                && p.semantic_name.eq_ignore_ascii_case(sem_bytes)
        }) else {
            log_err("CreateEmulatedVertexStreamOutputShader: output parameter not found");
            return Err(E_INVALIDARG);
        };

        for c in 0..entry.component_count {
            let offset = offsets[slot];
            offsets[slot] += 4;
            elements.push(StreamOutputElement {
                register_idx: sig_param.register,
                component: u32::from(entry.start_component + c),
                output_slot: u32::from(entry.output_slot),
                offset,
            });
        }
    }

    let mut strides = [0u32; 4];
    for (i, &s) in buffer_strides.iter().take(4).enumerate() {
        strides[i] = s;
    }

    Ok(StreamOutputLayout::new(
        elements,
        strides,
        rasterized_stream,
    ))
}

/// Heap-allocated COM wrapper for `ID3D11InputLayout`.
#[repr(C)]
pub struct InputLayoutObject {
    vtbl: &'static ID3D11InputLayoutVtbl,
    refs: Managed,
    device: *mut c_void,
    private_data: PrivateData,
    pub inner: InputLayout,
    d3d10: crate::d3d10::Wrapper<ID3D10InputLayoutVtbl>,
}

/// Heap-allocated COM wrapper for emulated `IMTLD3D11StreamOutputLayout` (`ID3D11GeometryShader`).
#[repr(C)]
pub struct StreamOutputLayoutObject {
    vtbl: &'static ID3D11GeometryShaderVtbl,
    refs: Managed,
    device: *mut c_void,
    private_data: PrivateData,
    pub inner: StreamOutputLayout,
    d3d10: crate::d3d10::Wrapper<ID3D10GeometryShaderVtbl>,
}

unsafe extern "system-unwind" fn il_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    let obj = this.cast::<InputLayoutObject>();
    // SAFETY: `this` is a valid `InputLayoutObject`; only the field's address is taken.
    let d3d10 = unsafe { (&raw mut (*obj).d3d10).cast::<c_void>() };
    let take_ref = || {
        // SAFETY: `this` is a valid `InputLayoutObject`.
        unsafe { il_add_ref(this) };
    };
    let qi = Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &LOG,
        // The name is the C++'s, typo included.
        miss: QiMiss::Once {
            object: &IID_ID3D11_INPUT_LAYOUT,
            name: "D3D311InputLayout",
        },
    };
    // SAFETY: the application's `riid` and `out`, null or valid.
    unsafe {
        qi_table(
            riid,
            out,
            &[
                (
                    &[
                        IID_IUNKNOWN,
                        IID_ID3D11_DEVICE_CHILD,
                        IID_ID3D11_INPUT_LAYOUT,
                        IID_IMTL_D3D11_INPUT_LAYOUT,
                    ],
                    this,
                ),
                (&[IID_ID3D10_DEVICE_CHILD, IID_ID3D10_INPUT_LAYOUT], d3d10),
            ],
            take_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn il_add_ref(this: *mut c_void) -> u32 {
    let obj = this.cast::<InputLayoutObject>();
    // SAFETY: `this` is a valid `InputLayoutObject`.
    let obj_ref = unsafe { &*obj };
    // SAFETY: `obj_ref.device` is null or a valid device pointer.
    unsafe { obj_ref.refs.add_ref(obj_ref.device) }
}

unsafe extern "system-unwind" fn il_release(this: *mut c_void) -> u32 {
    let obj = this.cast::<InputLayoutObject>();
    // SAFETY: `this` is a valid `InputLayoutObject`.
    let obj_ref = unsafe { &*obj };
    // SAFETY: `obj_ref.device` is null or a valid device pointer.
    let (remaining, recycle) = unsafe { obj_ref.refs.release(obj_ref.device) };
    if recycle {
        // SAFETY: both ref counts reached zero.
        drop(unsafe { Box::from_raw(obj) });
    }
    remaining
}

unsafe extern "system-unwind" fn il_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let obj = this.cast::<InputLayoutObject>();
    // SAFETY: `this` is a valid `InputLayoutObject`.
    let dev = unsafe { (*obj).device };
    // SAFETY: `dev` is null or a valid device pointer.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null and writable.
    unsafe { *out = dev };
}

impl HasPrivateData for InputLayoutObject {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `InputLayoutObject`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

static INPUT_LAYOUT_VTBL: ID3D11InputLayoutVtbl = ID3D11InputLayoutVtbl {
    query_interface: il_query_interface,
    add_ref: il_add_ref,
    release: il_release,
    get_device: il_get_device,
    get_private_data: get_private_data::<InputLayoutObject>,
    set_private_data: set_private_data::<InputLayoutObject>,
    set_private_data_interface: set_private_data_interface::<InputLayoutObject>,
};

fn create_input_layout_impl(
    device: *mut c_void,
    elements: *const D3d11InputElementDesc,
    num_elements: u32,
    bytecode: *const c_void,
    bytecode_length: usize,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if elements.is_null() || bytecode.is_null() || bytecode_length == 0 {
        return E_INVALIDARG;
    }
    if out.is_null() {
        return S_FALSE;
    }
    // SAFETY: caller provides `num_elements` descriptors and `bytecode_length` bytes.
    let descs = unsafe { core::slice::from_raw_parts(elements, num_elements as usize) };
    // SAFETY: see above.
    let bytes = unsafe { core::slice::from_raw_parts(bytecode.cast::<u8>(), bytecode_length) };

    let layout = match parse_input_layout(descs, bytes) {
        Ok(l) => l,
        Err(hr) => return hr,
    };

    let immediate_ctx =
        Device::from_iface(device).map_or(ptr::null_mut(), |d| d.immediate().cast::<c_void>());
    let boxed = Box::new(InputLayoutObject {
        vtbl: &INPUT_LAYOUT_VTBL,
        refs: Managed::new(),
        device,
        private_data: PrivateData::default(),
        inner: layout,
        d3d10: crate::d3d10::Wrapper::new(
            &crate::d3d10::INPUT_LAYOUT_VTBL,
            ptr::null_mut(),
            immediate_ctx,
        ),
    });
    // SAFETY: `device` is null or a live COM device pointer.
    unsafe { boxed.refs.add_ref(device) };
    let raw = Box::into_raw(boxed);
    // SAFETY: `raw` was just allocated by `Box::into_raw`.
    unsafe { (*raw).d3d10.d3d11 = raw.cast::<c_void>() };
    // SAFETY: `out` is non-null.
    unsafe { *out = raw.cast::<c_void>() };
    S_OK
}

/// `ID3D11Device::CreateInputLayout` (`d3d11_device.cpp:215-235`).
pub fn create_input_layout(
    device: *mut c_void,
    elements: *const D3d11InputElementDesc,
    num_elements: u32,
    bytecode: *const c_void,
    bytecode_length: usize,
    out: *mut *mut c_void,
) -> Hresult {
    create_input_layout_impl(
        device,
        elements,
        num_elements,
        bytecode,
        bytecode_length,
        out,
    )
}

unsafe extern "system-unwind" fn so_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> i32 {
    let take_ref = || {
        // SAFETY: `this` is a valid `StreamOutputLayoutObject`.
        unsafe { so_add_ref(this) };
    };
    let qi = Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &LOG,
        miss: QiMiss::Once {
            object: &IID_IMTL_D3D11_STREAM_OUTPUT_LAYOUT,
            name: "IMTLD3D11StreamOutputLayout",
        },
    };
    // SAFETY: the application's `riid` and `out`, null or valid.
    unsafe {
        qi_table(
            riid,
            out,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_ID3D11_DEVICE_CHILD,
                    IID_ID3D11_GEOMETRY_SHADER,
                    IID_IMTL_D3D11_STREAM_OUTPUT_LAYOUT,
                    IID_IMTL_D3D11_SHADER,
                ],
                this,
            )],
            take_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn so_add_ref(this: *mut c_void) -> u32 {
    let obj = this.cast::<StreamOutputLayoutObject>();
    // SAFETY: `this` is a valid `StreamOutputLayoutObject`.
    let obj_ref = unsafe { &*obj };
    // SAFETY: `obj_ref.device` is null or a valid device pointer.
    unsafe { obj_ref.refs.add_ref(obj_ref.device) }
}

unsafe extern "system-unwind" fn so_release(this: *mut c_void) -> u32 {
    let obj = this.cast::<StreamOutputLayoutObject>();
    // SAFETY: `this` is a valid `StreamOutputLayoutObject`.
    let obj_ref = unsafe { &*obj };
    // SAFETY: `obj_ref.device` is null or a valid device pointer.
    let (remaining, recycle) = unsafe { obj_ref.refs.release(obj_ref.device) };
    if recycle {
        // SAFETY: both ref counts reached zero.
        drop(unsafe { Box::from_raw(obj) });
    }
    remaining
}

unsafe extern "system-unwind" fn so_get_device(this: *mut c_void, out: *mut *mut c_void) {
    if out.is_null() {
        return;
    }
    let obj = this.cast::<StreamOutputLayoutObject>();
    // SAFETY: `this` is a valid `StreamOutputLayoutObject`.
    let dev = unsafe { (*obj).device };
    // SAFETY: `dev` is null or a valid device pointer.
    unsafe { add_ref(dev) };
    // SAFETY: `out` is non-null and writable.
    unsafe { *out = dev };
}

impl HasPrivateData for StreamOutputLayoutObject {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `StreamOutputLayoutObject`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

static SO_LAYOUT_VTBL: ID3D11GeometryShaderVtbl = ID3D11GeometryShaderVtbl {
    query_interface: so_query_interface,
    add_ref: so_add_ref,
    release: so_release,
    get_device: so_get_device,
    get_private_data: get_private_data::<StreamOutputLayoutObject>,
    set_private_data: set_private_data::<StreamOutputLayoutObject>,
    set_private_data_interface: set_private_data_interface::<StreamOutputLayoutObject>,
};

fn create_gs_with_so_impl(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    entries: (*const D3d11SoDeclarationEntry, u32),
    strides: (*const u32, u32),
    rasterized_stream: u32,
    out: *mut *mut c_void,
) -> Hresult {
    if !out.is_null() {
        // SAFETY: `out` is non-null.
        unsafe { *out = ptr::null_mut() };
    }
    if bytecode.is_null() || bytecode_length == 0 {
        return E_INVALIDARG;
    }
    let (entries_ptr, num_entries) = entries;
    let (strides_ptr, num_strides) = strides;
    // SAFETY: `bytecode` is valid for `bytecode_length` bytes.
    let bytes = unsafe { core::slice::from_raw_parts(bytecode.cast::<u8>(), bytecode_length) };
    if num_entries > 0
        && rasterized_stream == 0xffff_ffff
        && bytes.len() >= 4
        && &bytes[..4] == b"DXBC"
    {
        log_warn("Emulate stream output");
        if out.is_null() {
            return S_FALSE;
        }
        let entry_slice = if entries_ptr.is_null() {
            &[][..]
        } else {
            // SAFETY: `entries_ptr` is valid for `num_entries` elements.
            unsafe { core::slice::from_raw_parts(entries_ptr, num_entries as usize) }
        };
        let stride_slice = if strides_ptr.is_null() || num_strides == 0 {
            &[][..]
        } else {
            // SAFETY: `strides_ptr` is valid for `num_strides` elements.
            unsafe { core::slice::from_raw_parts(strides_ptr, num_strides as usize) }
        };
        if let Ok(so_layout) =
            parse_stream_output_layout(bytes, entry_slice, stride_slice, rasterized_stream)
        {
            let immediate_ctx = Device::from_iface(device)
                .map_or(ptr::null_mut(), |d| d.immediate().cast::<c_void>());
            let boxed = Box::new(StreamOutputLayoutObject {
                vtbl: &SO_LAYOUT_VTBL,
                refs: Managed::new(),
                device,
                private_data: PrivateData::default(),
                inner: so_layout,
                d3d10: crate::d3d10::Wrapper::new(
                    &crate::d3d10::GEOMETRY_SHADER_VTBL,
                    ptr::null_mut(),
                    immediate_ctx,
                ),
            });
            // SAFETY: `device` is null or a live COM device pointer.
            unsafe { boxed.refs.add_ref(device) };
            let raw = Box::into_raw(boxed);
            // SAFETY: `raw` was just allocated by `Box::into_raw`.
            unsafe { (*raw).d3d10.d3d11 = raw.cast::<c_void>() };
            // SAFETY: `out` is non-null.
            unsafe { *out = raw.cast::<c_void>() };
            return S_OK;
        }
    }
    log_err("CreateGeometryShaderWithStreamOutput: not supported, expect problem");
    crate::shader::create_geometry_shader(device, bytecode, bytecode_length, ptr::null_mut(), out)
}

/// `ID3D11Device::CreateGeometryShaderWithStreamOutput` (`d3d11_device.cpp:264-292`).
pub fn create_gs_with_so(
    device: *mut c_void,
    bytecode: *const c_void,
    bytecode_length: usize,
    entries: (*const D3d11SoDeclarationEntry, u32),
    strides: (*const u32, u32),
    rasterized_stream: u32,
    out: *mut *mut c_void,
) -> Hresult {
    create_gs_with_so_impl(
        device,
        bytecode,
        bytecode_length,
        entries,
        strides,
        rasterized_stream,
        out,
    )
}

#[cfg(test)]
mod tests;
