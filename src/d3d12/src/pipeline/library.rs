//! `ID3D12PipelineLibrary1` (`d3d12_pipeline_library.cpp`): `DXMTPLB1` serialization and loading.

use core::{ffi::c_void, ptr};
use std::{collections::BTreeMap, sync::Mutex};

use dxmt_util::{
    com::ComObject,
    private_data::{self, HasPrivateData, PrivateData},
    sync::lock,
};

use crate::{
    abi::vtbl::{ID3D12PipelineLibrary1Vtbl, IUnknownVtbl},
    com::{
        ComRefCounted, D3D12_ERROR_DRIVER_VERSION_MISMATCH, E_INVALIDARG, E_NOINTERFACE, Hresult,
        IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT, IID_ID3D12_PIPELINE_LIBRARY,
        IID_ID3D12_PIPELINE_LIBRARY1, IID_ID3D12_PIPELINE_STATE, IID_IUNKNOWN, S_FALSE, S_OK,
        add_ref_com, init_out_ptr, release_com, vtbl,
    },
    device::Device,
    pipeline::{
        PipelineKind, cast_pipeline,
        compute::{create_compute_pipeline_state, hash_compute_desc},
        graphics::{create_graphics_pipeline_state, hash_graphics_desc},
        parse_stream,
    },
};

const MAGIC: [u8; 8] = *b"DXMTPLB1";
const VERSION: u32 = 1;
const HEADER_SIZE: usize = 16;

const KIND_ANY: u8 = 0;
const KIND_GRAPHICS: u8 = 1;
const KIND_COMPUTE: u8 = 2;

#[cfg(windows)]
const WCHAR_U16S: usize = 1;
#[cfg(not(windows))]
const WCHAR_U16S: usize = 2;

/// Entry in `MTLD3D12PipelineLibrary::entries_` (`d3d12_pipeline_library.cpp:90`).
pub struct Entry {
    pub kind: u8,
    pub hash: u64,
    pub pso: usize,
}

/// `MTLD3D12PipelineLibrary` (`d3d12_pipeline_library.cpp:89`).
#[repr(C)]
pub struct PipelineLibrary {
    pub vtbl: &'static ID3D12PipelineLibrary1Vtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub entries: Mutex<BTreeMap<Vec<u16>, Entry>>,
}

impl ComRefCounted for PipelineLibrary {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for PipelineLibrary {
    fn drop(&mut self) {
        let entries = core::mem::take(&mut *lock(&self.entries));
        for (_, entry) in entries {
            if entry.pso != 0 {
                let pso_ptr = ptr::with_exposed_provenance_mut::<c_void>(entry.pso);
                // SAFETY: `pso_ptr` is a live COM object with one reference owned by `self`.
                let unk = unsafe { vtbl::<IUnknownVtbl>(pso_ptr) };
                // SAFETY: Releases the reference acquired in `store_pipeline` or `load`.
                unsafe { (unk.release)(pso_ptr) };
            }
        }
        if !self.device.is_null() {
            // SAFETY: `self.device` was `add_ref_com`'d when `self` was constructed.
            unsafe { release_com(self.device) };
        }
    }
}

unsafe fn read_wchar_name(name: *const c_void) -> Vec<u16> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        let ptr = name.cast::<u16>();
        let mut i = 0usize;
        loop {
            // SAFETY: Caller guarantees `name` is a NUL-terminated wide string.
            let ch_ptr = unsafe { ptr.add(i) };
            // SAFETY: `ch_ptr` points within the caller's NUL-terminated wide string.
            let ch = unsafe { ch_ptr.read_unaligned() };
            if ch == 0 {
                break;
            }
            out.push(ch);
            i += 1;
        }
    }
    #[cfg(not(windows))]
    {
        let ptr = name.cast::<u32>();
        let mut i = 0usize;
        loop {
            // SAFETY: Caller guarantees `name` is a NUL-terminated `wchar_t` (`u32`) string.
            let ch_ptr = unsafe { ptr.add(i) };
            // SAFETY: `ch_ptr` points within the caller's NUL-terminated `wchar_t` string.
            let ch = unsafe { ch_ptr.read_unaligned() };
            if ch == 0 {
                break;
            }
            let bytes = ch.to_ne_bytes();
            out.push(u16::from_ne_bytes([bytes[0], bytes[1]]));
            out.push(u16::from_ne_bytes([bytes[2], bytes[3]]));
            i += 1;
        }
    }
    out
}

unsafe fn initialize(
    blob: *const c_void,
    size: usize,
) -> Result<BTreeMap<Vec<u16>, Entry>, Hresult> {
    let mut entries = BTreeMap::new();
    if blob.is_null() || size == 0 {
        return Ok(entries);
    }
    if size < HEADER_SIZE {
        return Err(D3D12_ERROR_DRIVER_VERSION_MISMATCH);
    }
    // SAFETY: Caller guarantees `blob` is non-null and valid for `size` bytes.
    let bytes = unsafe { core::slice::from_raw_parts(blob.cast::<u8>(), size) };
    if bytes[..8] != MAGIC {
        return Err(D3D12_ERROR_DRIVER_VERSION_MISMATCH);
    }
    let version = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let count = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    if version != VERSION {
        return Err(D3D12_ERROR_DRIVER_VERSION_MISMATCH);
    }
    let mut off = HEADER_SIZE;
    for _ in 0..count {
        if size - off < 4 {
            return Err(E_INVALIDARG);
        }
        let chars = u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
            as usize;
        off += 4;
        let Some(u16_len) = chars.checked_mul(WCHAR_U16S) else {
            return Err(E_INVALIDARG);
        };
        let Some(byte_len) = u16_len.checked_mul(2) else {
            return Err(E_INVALIDARG);
        };
        let Some(entry_rem) = byte_len.checked_add(8) else {
            return Err(E_INVALIDARG);
        };
        if entry_rem > size - off {
            return Err(E_INVALIDARG);
        }
        let mut name = Vec::with_capacity(u16_len);
        for &[b0, b1] in bytes[off..off + byte_len].as_chunks::<2>().0 {
            name.push(u16::from_le_bytes([b0, b1]));
        }
        off += byte_len;
        let hash = u64::from_le_bytes([
            bytes[off],
            bytes[off + 1],
            bytes[off + 2],
            bytes[off + 3],
            bytes[off + 4],
            bytes[off + 5],
            bytes[off + 6],
            bytes[off + 7],
        ]);
        off += 8;
        entries.insert(
            name,
            Entry {
                kind: KIND_ANY,
                hash,
                pso: 0,
            },
        );
    }
    Ok(entries)
}

fn load_entry<F>(
    entries: &mut BTreeMap<Vec<u16>, Entry>,
    name: &[u16],
    kind: u8,
    hash: u64,
    create: F,
) -> Result<*mut c_void, Hresult>
where
    F: FnOnce(*mut *mut c_void) -> Hresult,
{
    let Some(entry) = entries.get_mut(name) else {
        return Err(E_INVALIDARG);
    };
    if (entry.kind != KIND_ANY && entry.kind != kind) || entry.hash != hash {
        return Err(E_INVALIDARG);
    }
    if entry.pso != 0 {
        Ok(ptr::with_exposed_provenance_mut::<c_void>(entry.pso))
    } else {
        let mut new_pso: *mut c_void = ptr::null_mut();
        let hr = create(&raw mut new_pso);
        if hr < 0 || new_pso.is_null() {
            return Err(hr);
        }
        entry.kind = kind;
        entry.pso = new_pso.expose_provenance();
        Ok(new_pso)
    }
}

unsafe fn load<F>(
    lib: &PipelineLibrary,
    name_ptr: *const c_void,
    kind: u8,
    hash: u64,
    riid: *const c_void,
    pp: *mut *mut c_void,
    create: F,
) -> Hresult
where
    F: FnOnce(*mut *mut c_void) -> Hresult,
{
    // SAFETY: `pp` is a caller-supplied out-pointer or null.
    unsafe { init_out_ptr(pp) };
    if name_ptr.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `name_ptr` is non-null.
    let name = unsafe { read_wchar_name(name_ptr) };
    let value = load_entry(&mut lock(&lib.entries), &name, kind, hash, create);
    let pso_ptr = match value {
        Ok(ptr) => ptr,
        Err(hr) => return hr,
    };
    // SAFETY: `pso_ptr` is kept alive by `lib.entries` for the lifetime of `lib`.
    let unk = unsafe { vtbl::<IUnknownVtbl>(pso_ptr) };
    // SAFETY: Forwards `QueryInterface` to `pso_ptr`.
    unsafe { (unk.query_interface)(pso_ptr, riid, pp) }
}

fn serialized_size(entries: &BTreeMap<Vec<u16>, Entry>) -> usize {
    let mut size = HEADER_SIZE;
    for name in entries.keys() {
        size += 4 + name.len() * 2 + 8;
    }
    size
}

fn serialize_entries(
    entries: &BTreeMap<Vec<u16>, Entry>,
    data: *mut c_void,
    data_size_in_bytes: usize,
) -> Hresult {
    let needed = serialized_size(entries);
    if data.is_null() || data_size_in_bytes < needed {
        return E_INVALIDARG;
    }
    let count = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    // SAFETY: `data` is non-null and writable for `needed` bytes.
    let out = unsafe { core::slice::from_raw_parts_mut(data.cast::<u8>(), needed) };
    out[..8].copy_from_slice(&MAGIC);
    out[8..12].copy_from_slice(&VERSION.to_le_bytes());
    out[12..16].copy_from_slice(&count.to_le_bytes());
    let mut off = HEADER_SIZE;
    for (name, entry) in entries {
        let chars = u32::try_from(name.len() / WCHAR_U16S).unwrap_or(u32::MAX);
        out[off..off + 4].copy_from_slice(&chars.to_le_bytes());
        off += 4;
        for &unit in name {
            out[off..off + 2].copy_from_slice(&unit.to_le_bytes());
            off += 2;
        }
        out[off..off + 8].copy_from_slice(&entry.hash.to_le_bytes());
        off += 8;
    }
    S_OK
}

impl HasPrivateData for PipelineLibrary {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `PipelineLibrary`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

static VTBL: ID3D12PipelineLibrary1Vtbl = ID3D12PipelineLibrary1Vtbl {
    query_interface: crate::slot!(
        "library.QueryInterface",
        (this: *mut c_void, riid: *const c_void, object: *mut *mut c_void) -> Hresult => {
            let qi = crate::com::Qi {
                null_riid: E_INVALIDARG,
                silent: &[],
                log: &crate::LOG,
                miss: crate::com::QiMiss::Quiet,
            };
            let add_ref = || {
                // SAFETY: `this` is a live `PipelineLibrary`.
                unsafe { add_ref_com(this.cast::<PipelineLibrary>()) };
            };
            // SAFETY: the application's `riid` and `object`, null or valid; `this` is a live `PipelineLibrary`.
            unsafe {
                crate::com::qi_table(
                    riid,
                    object,
                    &[(
                        &[
                            IID_IUNKNOWN,
                            IID_ID3D12_OBJECT,
                            IID_ID3D12_DEVICE_CHILD,
                            IID_ID3D12_PIPELINE_LIBRARY,
                            IID_ID3D12_PIPELINE_LIBRARY1,
                        ],
                        this,
                    )],
                    add_ref,
                    &qi,
                )
            }
        }
    ),
    add_ref: crate::slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `PipelineLibrary`.
        unsafe { add_ref_com(this.cast::<PipelineLibrary>()) }
    }),
    release: crate::slot!((this: *mut c_void) -> u32 => {
        // SAFETY: `this` is a live `PipelineLibrary`.
        unsafe { release_com(this.cast::<PipelineLibrary>()) }
    }),
    get_private_data: private_data::get_private_data::<PipelineLibrary>,
    set_private_data: private_data::set_private_data::<PipelineLibrary>,
    set_private_data_interface: private_data::set_private_data_interface::<PipelineLibrary>,
    set_name: private_data::set_name::<PipelineLibrary>,
    get_device: crate::slot!(
        (this: *mut c_void, riid: *const c_void, pp_device: *mut *mut c_void) -> Hresult => {
            // SAFETY: `this` is a live `PipelineLibrary`.
            let lib = unsafe { &*this.cast::<PipelineLibrary>() };
            if lib.device.is_null() {
                // SAFETY: `pp_device` is a caller out-pointer.
                unsafe { init_out_ptr(pp_device) };
                return E_NOINTERFACE;
            }
            let dev_ptr = lib.device.cast::<c_void>();
            // SAFETY: `dev_ptr` is a live `Device` COM object.
            let unk = unsafe { vtbl::<IUnknownVtbl>(dev_ptr) };
            // SAFETY: Forwarded to `Device::QueryInterface`.
            unsafe { (unk.query_interface)(dev_ptr, riid, pp_device) }
        }
    ),
    store_pipeline: crate::slot!(
        "library.StorePipeline",
        (this: *mut c_void, name_ptr: *const c_void, pipeline: *mut c_void) -> Hresult => {
            if name_ptr.is_null() || pipeline.is_null() {
                return E_INVALIDARG;
            }
            // SAFETY: `pipeline` is non-null.
            let Some((kind, hash)) = (unsafe {
                cast_pipeline(pipeline).map(|k| match k {
                    PipelineKind::Graphics(g) => (KIND_GRAPHICS, g.desc_hash),
                    PipelineKind::Compute(c) => (KIND_COMPUTE, c.desc_hash),
                })
            }) else {
                return E_INVALIDARG;
            };
            // SAFETY: `this` is a live `PipelineLibrary`.
            let lib = unsafe { &*this.cast::<PipelineLibrary>() };
            // SAFETY: `name_ptr` is non-null.
            let name = unsafe { read_wchar_name(name_ptr) };
            let mut entries = lock(&lib.entries);
            if entries.contains_key(&name) {
                return E_INVALIDARG;
            }
            // SAFETY: `pipeline` is a valid `ID3D12PipelineState` COM object.
            let unk = unsafe { vtbl::<IUnknownVtbl>(pipeline) };
            // SAFETY: Retain `pipeline` for `lib.entries`.
            unsafe { (unk.add_ref)(pipeline) };
            entries.insert(
                name,
                Entry {
                    kind,
                    hash,
                    pso: pipeline.expose_provenance(),
                },
            );
            S_OK
        }
    ),
    load_graphics_pipeline: crate::slot!(
        "library.LoadGraphicsPipeline",
        (
            this: *mut c_void,
            name: *const c_void,
            desc: *const c_void,
            riid: *const c_void,
            pipeline_state: *mut *mut c_void,
        ) -> Hresult => {
            if desc.is_null() {
                // SAFETY: `pipeline_state` is a caller out-pointer.
                unsafe { init_out_ptr(pipeline_state) };
                return E_INVALIDARG;
            }
            // SAFETY: `this` is a live `PipelineLibrary`.
            let lib = unsafe { &*this.cast::<PipelineLibrary>() };
            // SAFETY: `desc` is non-null.
            let hash = unsafe { hash_graphics_desc(desc) };
            let iid = IID_ID3D12_PIPELINE_STATE;
            let create = |out: *mut *mut c_void| {
                // SAFETY: `lib.device`, `desc`, `iid`, and `out` are valid pointers.
                unsafe {
                    create_graphics_pipeline_state(
                        lib.device.cast::<c_void>(),
                        desc,
                        (&raw const iid).cast::<c_void>(),
                        out,
                    )
                }
            };
            // SAFETY: Pointers forwarded from COM caller.
            unsafe { load(lib, name, KIND_GRAPHICS, hash, riid, pipeline_state, create) }
        }
    ),
    load_compute_pipeline: crate::slot!(
        "library.LoadComputePipeline",
        (
            this: *mut c_void,
            name: *const c_void,
            desc: *const c_void,
            riid: *const c_void,
            pipeline_state: *mut *mut c_void,
        ) -> Hresult => {
            if desc.is_null() {
                // SAFETY: `pipeline_state` is a caller out-pointer.
                unsafe { init_out_ptr(pipeline_state) };
                return E_INVALIDARG;
            }
            // SAFETY: `this` is a live `PipelineLibrary`.
            let lib = unsafe { &*this.cast::<PipelineLibrary>() };
            // SAFETY: `desc` is non-null.
            let hash = unsafe { hash_compute_desc(desc) };
            let iid = IID_ID3D12_PIPELINE_STATE;
            let create = |out: *mut *mut c_void| {
                // SAFETY: `lib.device`, `desc`, `iid`, and `out` are valid pointers.
                unsafe {
                    create_compute_pipeline_state(
                        lib.device.cast::<c_void>(),
                        desc,
                        (&raw const iid).cast::<c_void>(),
                        out,
                    )
                }
            };
            // SAFETY: Pointers forwarded from COM caller.
            unsafe { load(lib, name, KIND_COMPUTE, hash, riid, pipeline_state, create) }
        }
    ),
    get_serialized_size: crate::slot!(
        "library.GetSerializedSize",
        (this: *mut c_void) -> usize => {
            // SAFETY: `this` is a live `PipelineLibrary`.
            let lib = unsafe { &*this.cast::<PipelineLibrary>() };
            serialized_size(&lock(&lib.entries))
        }
    ),
    serialize: crate::slot!(
        "library.Serialize",
        (this: *mut c_void, data: *mut c_void, data_size_in_bytes: usize) -> Hresult => {
            // SAFETY: `this` is a live `PipelineLibrary`.
            let lib = unsafe { &*this.cast::<PipelineLibrary>() };
            serialize_entries(
                &lock(&lib.entries),
                data,
                data_size_in_bytes,
            )
        }
    ),
    load_pipeline: crate::slot!(
        "library.LoadPipeline",
        (
            this: *mut c_void,
            name: *const c_void,
            desc: *const c_void,
            riid: *const c_void,
            pipeline_state: *mut *mut c_void,
        ) -> Hresult => {
            // SAFETY: `desc` is validated by `parse_stream`.
            let parsed = match unsafe { parse_stream(desc) } {
                Ok(p) => p,
                Err(hr) => {
                    // SAFETY: `pipeline_state` is a caller out-pointer.
                    unsafe { init_out_ptr(pipeline_state) };
                    return hr;
                }
            };
            // SAFETY: `this` is a live `PipelineLibrary`.
            let lib = unsafe { &*this.cast::<PipelineLibrary>() };
            let iid = IID_ID3D12_PIPELINE_STATE;
            if parsed.is_compute {
                let c_ptr = (&raw const parsed.compute).cast::<c_void>();
                // SAFETY: `c_ptr` points to `parsed.compute`.
                let hash = unsafe { hash_compute_desc(c_ptr) };
                let create = |out: *mut *mut c_void| {
                    // SAFETY: `lib.device`, `c_ptr`, `iid`, and `out` are valid pointers.
                    unsafe {
                        create_compute_pipeline_state(
                            lib.device.cast::<c_void>(),
                            c_ptr,
                            (&raw const iid).cast::<c_void>(),
                            out,
                        )
                    }
                };
                // SAFETY: Pointers forwarded from COM caller.
                unsafe { load(lib, name, KIND_COMPUTE, hash, riid, pipeline_state, create) }
            } else {
                let g_ptr = (&raw const parsed.graphics).cast::<c_void>();
                // SAFETY: `g_ptr` points to `parsed.graphics`.
                let hash = unsafe { hash_graphics_desc(g_ptr) };
                let create = |out: *mut *mut c_void| {
                    // SAFETY: `lib.device`, `g_ptr`, `iid`, and `out` are valid pointers.
                    unsafe {
                        create_graphics_pipeline_state(
                            lib.device.cast::<c_void>(),
                            g_ptr,
                            (&raw const iid).cast::<c_void>(),
                            out,
                        )
                    }
                };
                // SAFETY: Pointers forwarded from COM caller.
                unsafe { load(lib, name, KIND_GRAPHICS, hash, riid, pipeline_state, create) }
            }
        }
    ),
};

/// `ID3D12Device1::CreatePipelineLibrary` (`d3d12_device.cpp:1483`,
///
/// `d3d12_pipeline_library.cpp:253`).
///
/// # Safety
///
/// `this`, `blob`, `riid`, and `library` are caller-supplied pointers.
pub unsafe fn create_pipeline_library(
    this: *mut c_void,
    blob: *const c_void,
    blob_len: usize,
    riid: *const c_void,
    library: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreatePipelineLibrary");
    // SAFETY: `library` is a caller-supplied out-pointer or null.
    unsafe { init_out_ptr(library) };
    // SAFETY: `blob` and `blob_len` are checked inside `initialize`.
    let entries = match unsafe { initialize(blob, blob_len) } {
        Ok(e) => e,
        Err(hr) => return hr,
    };
    if library.is_null() {
        return S_FALSE;
    }
    let device = this.cast::<Device>();
    if !device.is_null() {
        // SAFETY: `device` is a live `Device` when non-null.
        unsafe { add_ref_com(device) };
    }
    let raw = Box::into_raw(Box::new(PipelineLibrary {
        vtbl: &VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device,
        entries: Mutex::new(entries),
    }));
    // SAFETY: `raw` is a freshly allocated `PipelineLibrary` with 1 initial public ref.
    let hr = unsafe { (VTBL.query_interface)(raw.cast::<c_void>(), riid, library) };
    // SAFETY: Release the initial ref from `ComObject::new()`.
    unsafe { release_com(raw) };
    hr
}
