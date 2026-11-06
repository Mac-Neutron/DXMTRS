//! `ID3D12CommandSignature` (`d3d12_command_signature.cpp`): indirect-argument layouts and runtime
//!
//! MSL resolver kernels.

use core::{
    ffi::{c_char, c_void},
    fmt::Write,
    ptr,
};

use dxmt_util::{
    com::ComObject,
    private_data::{self, HasPrivateData, PrivateData},
};
use winemetal_sys::{
    MTLDevice_newComputePipelineState, MTLDevice_newLibraryWithSource, MTLLibrary_newFunction, Obj,
    wire::{
        MemPtr, NULL_OBJECT_HANDLE, SM50_BINDING_INDEX_ROOT_ARGUMENTS,
        SM50_BINDING_INDEX_STATIC_SAMPLERS, SM50_BINDING_INDEX_VERTEX_BUFFER,
        WMTComputePipelineInfo,
    },
};

use crate::{
    abi::{
        objects::{
            CommandSignatureDesc, D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT,
            D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW,
            D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH, D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_MESH,
            D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_RAYS, D3D12_INDIRECT_ARGUMENT_TYPE_DRAW,
            D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED,
            D3D12_INDIRECT_ARGUMENT_TYPE_INDEX_BUFFER_VIEW,
            D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW,
            D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW,
            D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW, IndirectArgumentDesc,
        },
        vtbl::{ID3D12CommandSignatureVtbl, IUnknownVtbl},
    },
    com::{
        self, ComRefCounted, E_FAIL, E_INVALIDARG, E_NOTIMPL, Hresult,
        IID_ID3D12_COMMAND_SIGNATURE, IID_ID3D12_DEVICE_CHILD, IID_ID3D12_OBJECT,
        IID_ID3D12_PAGEABLE, IID_IUNKNOWN, S_FALSE,
    },
    device::Device,
};

/// Shared MSL header (`kSharedHeader`, `d3d12_command_signature.cpp:26-92`).
const SHARED_HEADER: &str = r"
#include <metal_stdlib>

using namespace metal;

struct dxmt_compute_command_data {
  command_buffer cmd_buf;
  ulong max_count;
  device uint * max_count_buffer;
  device char * argument_buffer;
  device ulong * static_samplers;
  device ulong * rootsig_qwords;
  uint rootsig_qwords_stride;
  packed_uint3 tgsize;
};

struct d3d12_draw_arguments {
  uint vertex_count_per_instance;
  uint instance_count;
  uint start_vertex_location;
  uint start_instance_location;
};

struct d3d12_draw_indexed_arguments {
  uint index_count_per_instance;
  uint instance_count;
  uint start_index_location;
  int base_vertex_location;
  uint start_instance_location;
};

struct d3d12_vertex_buffer_view {
  device void * buffer;
  uint size_in_bytes;
  uint stride_in_bytes;
};

struct d3d12_index_buffer_view {
  device void * buffer;
  uint size_in_bytes;
  uint format;
};

struct dxmt_vertex_buffer {
  device void * buffer;
  uint stride;
  uint length;
};

struct dxmt_render_command_data {
  command_buffer cmd_buf;
  ulong max_count;
  device uint * max_count_buffer;
  device char * argument_buffer;
  device ulong * static_samplers;
  device ulong * rootsig_qwords;
  uint rootsig_qwords_stride;
  uint primitive_type;
  device char * vertex_buffer;
  device void * index_buffer;
  uint index_buffer_format;
  uint vertex_argbuf_stride;
  uint vertex_slot_mask;
  device void * null_vertex_buffer;
};

";

/// `MTLD3D12CommandSignatureImpl` (`d3d12_command_signature.cpp:94`, `d3d12_device.hpp:207-220`).
#[repr(C)]
pub struct CommandSignature {
    pub vtbl: &'static ID3D12CommandSignatureVtbl,
    pub com: ComObject,
    pub private_data: PrivateData,
    pub device: *mut Device,
    pub command_type: u32,
    pub update_root_arguments: bool,
    pub update_vertex_buffers: bool,
    pub update_index_buffer: bool,
    pub byte_stride: u32,
    pub render_resolver: Option<Obj>,
    pub compute_resolver: Option<Obj>,
}

impl ComRefCounted for CommandSignature {
    fn com(&self) -> &ComObject {
        &self.com
    }
}

impl Drop for CommandSignature {
    fn drop(&mut self) {
        if !self.device.is_null() {
            // SAFETY: `self.device` was retained by `add_ref` when `CommandSignature` was constructed.
            unsafe { com::release_com(self.device) };
        }
    }
}

impl HasPrivateData for CommandSignature {
    unsafe fn private_data<'a>(this: *mut c_void) -> &'a PrivateData {
        // SAFETY: `this` is a live `CommandSignature`.
        &unsafe { &*this.cast::<Self>() }.private_data
    }
}

static COMMAND_SIGNATURE_VTBL: ID3D12CommandSignatureVtbl = ID3D12CommandSignatureVtbl {
    query_interface: cmdsig_query_interface,
    add_ref: cmdsig_add_ref,
    release: cmdsig_release,
    get_private_data: private_data::get_private_data::<CommandSignature>,
    set_private_data: private_data::set_private_data::<CommandSignature>,
    set_private_data_interface: private_data::set_private_data_interface::<CommandSignature>,
    set_name: private_data::set_name::<CommandSignature>,
    get_device: cmdsig_get_device,
};

unsafe extern "system-unwind" fn cmdsig_query_interface(
    this: *mut c_void,
    riid: *const c_void,
    out: *mut *mut c_void,
) -> Hresult {
    crate::scope!("cmdsig.QueryInterface");
    if this.is_null() {
        // SAFETY: the out pointer is null or writable.
        return unsafe { com::qi_refuse(out, E_INVALIDARG) };
    }
    let qi = com::Qi {
        null_riid: E_INVALIDARG,
        silent: &[],
        log: &crate::LOG,
        miss: com::QiMiss::Quiet,
    };
    let add_ref = || {
        // SAFETY: `this` is a live `CommandSignature`.
        unsafe { com::add_ref_com(this.cast::<CommandSignature>()) };
    };
    // SAFETY: the application's `riid` and out pointer, null or valid; `this` is a live `CommandSignature`.
    unsafe {
        com::qi_table(
            riid,
            out,
            &[(
                &[
                    IID_IUNKNOWN,
                    IID_ID3D12_OBJECT,
                    IID_ID3D12_DEVICE_CHILD,
                    IID_ID3D12_PAGEABLE,
                    IID_ID3D12_COMMAND_SIGNATURE,
                ],
                this,
            )],
            add_ref,
            &qi,
        )
    }
}

unsafe extern "system-unwind" fn cmdsig_add_ref(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `CommandSignature` pointer.
    unsafe { com::add_ref_com(this.cast::<CommandSignature>()) }
}

unsafe extern "system-unwind" fn cmdsig_release(this: *mut c_void) -> u32 {
    // SAFETY: `this` is a valid `CommandSignature` pointer.
    unsafe { com::release_com(this.cast::<CommandSignature>()) }
}

unsafe extern "system-unwind" fn cmdsig_get_device(
    this: *mut c_void,
    riid: *const c_void,
    device: *mut *mut c_void,
) -> Hresult {
    if this.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `this` is a live `CommandSignature`.
    let dev = unsafe { (*this.cast::<CommandSignature>()).device }.cast::<c_void>();
    if dev.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `dev` is a live `Device` whose vtable starts with `IUnknownVtbl`.
    let unk = unsafe { com::vtbl::<IUnknownVtbl>(dev) };
    // SAFETY: `dev` is the owning `Device` and `riid`/`device` are caller pointers.
    unsafe { (unk.query_interface)(dev, riid, device) }
}

/// Casts a non-null `ID3D12CommandSignature` pointer if its vtable matches [`CommandSignature`].
///
/// # Safety
///
/// If non-null, `raw` must point to a readable COM object pointer.
#[must_use]
pub unsafe fn cast_command_signature<'a>(raw: *mut c_void) -> Option<&'a CommandSignature> {
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is non-null and points to a COM object whose first field is its vtable pointer.
    let vtbl = unsafe { *(raw as *const *const ID3D12CommandSignatureVtbl) };
    if !ptr::eq(vtbl, &raw const COMMAND_SIGNATURE_VTBL) {
        return None;
    }
    // SAFETY: the vtable check above confirms `raw` is a `CommandSignature`.
    Some(unsafe { &*raw.cast::<CommandSignature>() })
}

struct ParsedSignature {
    source: String,
    command_type: u32,
    byte_stride: u32,
    update_root_arguments: bool,
    update_vertex_buffers: bool,
    update_index_buffer: bool,
}

fn build_msl_source(
    byte_stride: u32,
    args: &[IndirectArgumentDesc],
    slot_qword_offsets: Option<&[u32]>,
) -> Result<ParsedSignature, Hresult> {
    let mut source = String::with_capacity(2048);
    let mut side_effect: Option<u32> = None;
    let mut update_root_arguments = false;
    let mut update_vertex_buffers = false;
    let mut ib_index: Option<usize> = None;

    source.push_str(SHARED_HEADER);
    source.push_str("struct __attribute__ ((packed)) d3d12_arguments {\n");

    for (i, arg) in args.iter().enumerate() {
        if side_effect.is_some() {
            return Err(E_INVALIDARG);
        }
        match arg.r#type {
            D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH => {
                side_effect = Some(arg.r#type);
                source.push_str("packed_uint3 dispatch;\n");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_DRAW => {
                side_effect = Some(arg.r#type);
                source.push_str("d3d12_draw_arguments draw;\n");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED => {
                side_effect = Some(arg.r#type);
                source.push_str("d3d12_draw_indexed_arguments draw_indexed;\n");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW => {
                update_vertex_buffers = true;
                let _ = writeln!(source, "d3d12_vertex_buffer_view vb_{i};");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_INDEX_BUFFER_VIEW => {
                if ib_index.is_some() {
                    return Err(E_INVALIDARG);
                }
                ib_index = Some(i);
                source.push_str("d3d12_index_buffer_view ib;\n");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT => {
                update_root_arguments = true;
                // SAFETY: `arg.r#type == D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT` selects `constant`.
                let c = unsafe { &arg.u.constant };
                for j in 0..c.num_32_bit_values_to_set {
                    let _ = writeln!(source, "uint constant_{i}_{j};");
                }
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW => {
                update_root_arguments = true;
                let _ = writeln!(source, "ulong cb_{i};");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW => {
                update_root_arguments = true;
                let _ = writeln!(source, "ulong srv_{i};");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW => {
                update_root_arguments = true;
                let _ = writeln!(source, "ulong uav_{i};");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_RAYS
            | D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH_MESH => {
                crate::err!("D3D12CommandSignatuer: unsupported rays/mesh dispatch");
                return Err(E_NOTIMPL);
            }
            _ => return Err(E_INVALIDARG),
        }
    }
    source.push_str("};\n\n");

    let Some(command_type) = side_effect else {
        return Err(E_INVALIDARG);
    };
    let is_compute = command_type == D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH;
    let update_index_buffer = ib_index.is_some();

    let cmd_data = if is_compute {
        "dxmt_compute_command_data"
    } else {
        "dxmt_render_command_data"
    };
    let _ = writeln!(
        source,
        "[[kernel]] void resolve_indirect_commands([[thread_position_in_grid]] uint x, constant {cmd_data} &command_data [[buffer(30)]]) {{"
    );
    source.push_str("if (x != 0) return;\n");
    source.push_str(
        "uint count = command_data.max_count_buffer ? command_data.max_count_buffer[0] : command_data.max_count;\n",
    );
    source.push_str("for (uint i = 0; i < command_data.max_count; i++) {\n");
    let _ = writeln!(
        source,
        "device d3d12_arguments& arg = reinterpret_cast<device d3d12_arguments *>(command_data.argument_buffer + i * {byte_stride})[0];"
    );
    if is_compute {
        source.push_str("compute_command cmd(command_data.cmd_buf, i);\n");
    } else {
        source.push_str("render_command cmd(command_data.cmd_buf, i);\n");
    }
    source.push_str("cmd.reset();\n");
    source.push_str("if (i >= count) continue;\n");
    source.push_str(
        "device ulong * rootsig_qwords = command_data.rootsig_qwords + (i * command_data.rootsig_qwords_stride);\n",
    );
    if !is_compute {
        source.push_str(
            "device dxmt_vertex_buffer * vertex_buffer = reinterpret_cast<device dxmt_vertex_buffer *>(command_data.vertex_buffer + (i * command_data.vertex_argbuf_stride));\n",
        );
    }

    if update_root_arguments || update_vertex_buffers || update_index_buffer {
        if is_compute {
            let _ = writeln!(
                source,
                "cmd.set_kernel_buffer(rootsig_qwords, {SM50_BINDING_INDEX_ROOT_ARGUMENTS});"
            );
            let _ = writeln!(
                source,
                "cmd.set_kernel_buffer(command_data.static_samplers,{SM50_BINDING_INDEX_STATIC_SAMPLERS});"
            );
        } else {
            let _ = writeln!(
                source,
                "cmd.set_vertex_buffer(vertex_buffer,{SM50_BINDING_INDEX_VERTEX_BUFFER});"
            );
            let _ = writeln!(
                source,
                "cmd.set_vertex_buffer(rootsig_qwords,{SM50_BINDING_INDEX_ROOT_ARGUMENTS});"
            );
            let _ = writeln!(
                source,
                "cmd.set_vertex_buffer(command_data.static_samplers,{SM50_BINDING_INDEX_STATIC_SAMPLERS});"
            );
            let _ = writeln!(
                source,
                "cmd.set_fragment_buffer(rootsig_qwords,{SM50_BINDING_INDEX_ROOT_ARGUMENTS});"
            );
            let _ = writeln!(
                source,
                "cmd.set_fragment_buffer(command_data.static_samplers,{SM50_BINDING_INDEX_STATIC_SAMPLERS});"
            );
        }
    }

    for (i, arg) in args.iter().enumerate() {
        match arg.r#type {
            D3D12_INDIRECT_ARGUMENT_TYPE_DRAW => {
                source.push_str(
                    "cmd.draw_primitives((primitive_type)command_data.primitive_type, arg.draw.start_vertex_location, arg.draw.vertex_count_per_instance, arg.draw.instance_count, arg.draw.start_instance_location);\n",
                );
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_DRAW_INDEXED => {
                if ib_index.is_none() {
                    source.push_str("bool ib32bit = command_data.index_buffer_format == 42;\n");
                    source.push_str("device void* ib = command_data.index_buffer;\n");
                }
                source.push_str("if (ib32bit) {\n");
                source.push_str(
                    "cmd.draw_indexed_primitives((primitive_type)command_data.primitive_type, arg.draw_indexed.index_count_per_instance, reinterpret_cast<device uint *>(ib) + arg.draw_indexed.start_index_location, arg.draw_indexed.instance_count, arg.draw_indexed.base_vertex_location, arg.draw_indexed.start_instance_location);\n",
                );
                source.push_str("} else {\n");
                source.push_str(
                    "cmd.draw_indexed_primitives((primitive_type)command_data.primitive_type, arg.draw_indexed.index_count_per_instance, reinterpret_cast<device ushort *>(ib) + arg.draw_indexed.start_index_location, arg.draw_indexed.instance_count, arg.draw_indexed.base_vertex_location, arg.draw_indexed.start_instance_location);\n",
                );
                source.push_str("}\n");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH => {
                source.push_str(
                    "cmd.concurrent_dispatch_threadgroups(arg.dispatch, command_data.tgsize);\n",
                );
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW => {
                // SAFETY: `arg.r#type == D3D12_INDIRECT_ARGUMENT_TYPE_VERTEX_BUFFER_VIEW` selects `vertex_buffer`.
                let vb = unsafe { &arg.u.vertex_buffer };
                let slot = vb.slot;
                let bit = 1_u32.checked_shl(slot).unwrap_or(0);
                let mask = bit.wrapping_sub(1);
                let _ = writeln!(
                    source,
                    "if (command_data.vertex_slot_mask & {bit}u) vertex_buffer[popcount(command_data.vertex_slot_mask & {mask}u)] = arg.vb_{i}.buffer ? dxmt_vertex_buffer{{arg.vb_{i}.buffer,arg.vb_{i}.stride_in_bytes,arg.vb_{i}.size_in_bytes}} : dxmt_vertex_buffer{{command_data.null_vertex_buffer,0,0}};"
                );
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_INDEX_BUFFER_VIEW => {
                source.push_str("bool ib32bit = arg.ib.format == 42;\n");
                source.push_str("device void* ib = arg.ib.buffer;\n");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT => {
                // SAFETY: `arg.r#type == D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT` selects `constant`.
                let c = unsafe { &arg.u.constant };
                let slots = slot_qword_offsets.ok_or(E_INVALIDARG)?;
                let offset = *slots
                    .get(c.root_parameter_index as usize)
                    .ok_or(E_INVALIDARG)?;
                for j in 0..c.num_32_bit_values_to_set {
                    let dst = j.wrapping_add(c.dest_offset_in_32_bit_values);
                    let _ = writeln!(
                        source,
                        "reinterpret_cast<device uint *>(rootsig_qwords + {offset})[{dst}] = arg.constant_{i}_{j};"
                    );
                }
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW => {
                // SAFETY: `arg.r#type == D3D12_INDIRECT_ARGUMENT_TYPE_CONSTANT_BUFFER_VIEW` selects `constant_buffer_view`.
                let rv = unsafe { &arg.u.constant_buffer_view };
                let slots = slot_qword_offsets.ok_or(E_INVALIDARG)?;
                let offset = *slots
                    .get(rv.root_parameter_index as usize)
                    .ok_or(E_INVALIDARG)?;
                let _ = writeln!(source, "rootsig_qwords[{offset}] = arg.cb_{i};");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW => {
                // SAFETY: `arg.r#type == D3D12_INDIRECT_ARGUMENT_TYPE_SHADER_RESOURCE_VIEW` selects `shader_resource_view`.
                let rv = unsafe { &arg.u.shader_resource_view };
                let slots = slot_qword_offsets.ok_or(E_INVALIDARG)?;
                let offset = *slots
                    .get(rv.root_parameter_index as usize)
                    .ok_or(E_INVALIDARG)?;
                let _ = writeln!(source, "rootsig_qwords[{offset}] = arg.srv_{i};");
            }
            D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW => {
                // SAFETY: `arg.r#type == D3D12_INDIRECT_ARGUMENT_TYPE_UNORDERED_ACCESS_VIEW` selects `unordered_access_view`.
                let rv = unsafe { &arg.u.unordered_access_view };
                let slots = slot_qword_offsets.ok_or(E_INVALIDARG)?;
                let offset = *slots
                    .get(rv.root_parameter_index as usize)
                    .ok_or(E_INVALIDARG)?;
                let _ = writeln!(source, "rootsig_qwords[{offset}] = arg.uav_{i};");
            }
            _ => return Err(E_INVALIDARG),
        }
    }

    source.push_str("}\n};\n");

    Ok(ParsedSignature {
        source,
        command_type,
        byte_stride,
        update_root_arguments,
        update_vertex_buffers,
        update_index_buffer,
    })
}

fn compile_resolver(mtl: &Obj, source: &str) -> Result<Obj, Hresult> {
    let mut err_handle = 0_u64;
    // SAFETY: `mtl` is a live `MTLDevice` and `source` is a valid UTF-8 byte slice of length `source.len()`.
    let lib_handle = unsafe {
        MTLDevice_newLibraryWithSource(
            mtl.handle(),
            source.as_ptr().cast::<c_char>(),
            source.len() as u64,
            &raw mut err_handle,
        )
    };
    // SAFETY: `err_handle` is a +1 `NSError` if non-zero.
    let _err = unsafe { Obj::adopt(err_handle) };
    // SAFETY: `lib_handle` is a +1 `MTLLibrary` if non-zero.
    let Some(lib) = (unsafe { Obj::adopt(lib_handle) }) else {
        crate::err!("Failed to compile command signature resolve shader");
        return Err(E_FAIL);
    };
    // SAFETY: `lib` is a live `MTLLibrary` and the function name is a NUL-terminated C string.
    let fn_handle =
        unsafe { MTLLibrary_newFunction(lib.handle(), c"resolve_indirect_commands".as_ptr()) };
    // SAFETY: `fn_handle` is a +1 `MTLFunction` if non-zero.
    let Some(function) = (unsafe { Obj::adopt(fn_handle) }) else {
        crate::err!("Failed to create command signature resolve shader");
        return Err(E_FAIL);
    };
    let info = WMTComputePipelineInfo {
        compute_function: function.handle(),
        binary_archives_for_lookup: MemPtr::NULL,
        binary_archive_for_serialization: NULL_OBJECT_HANDLE,
        num_binary_archives_for_lookup: 0,
        fail_on_binary_archive_miss: 0,
        support_indirect_command_buffers: 0,
        tgsize_is_multiple_of_sgwidth: 0,
        immutable_buffers: 0,
    };
    let mut pso_err = 0_u64;
    // SAFETY: `mtl` is a live `MTLDevice` and `info` references the live `function`.
    let pso_handle = unsafe {
        MTLDevice_newComputePipelineState(mtl.handle(), &raw const info, &raw mut pso_err)
    };
    // SAFETY: `pso_err` is a +1 `NSError` if non-zero.
    let pso_err_obj = unsafe { Obj::adopt(pso_err) };
    // SAFETY: `pso_handle` is a +1 `MTLComputePipelineState` if non-zero.
    let pso = unsafe { Obj::adopt(pso_handle) };
    if pso_err_obj.is_some() || pso.is_none() {
        crate::err!("Failed to compile command signature resolve pso");
        return Err(E_FAIL);
    }
    Ok(pso.expect("checked Some above"))
}

/// `ID3D12Device::CreateCommandSignature` (`d3d12_device.cpp:1458`).
///
/// # Safety
///
/// `this`, `desc`, `root_sig`, `riid`, and `cmd_sig` are caller-supplied COM pointers.
pub unsafe fn create_command_signature(
    this: *mut c_void,
    desc: *const c_void,
    root_sig: *mut c_void,
    riid: *const c_void,
    cmd_sig: *mut *mut c_void,
) -> Hresult {
    crate::scope!("device.CreateCommandSignature");
    if !cmd_sig.is_null() {
        // SAFETY: `cmd_sig` is non-null.
        unsafe { *cmd_sig = ptr::null_mut() };
    }
    if desc.is_null() {
        return E_INVALIDARG;
    }
    // SAFETY: `desc` was just checked non-null.
    let d = unsafe { &*desc.cast::<CommandSignatureDesc>() };
    if d.num_argument_descs > 0 && d.p_argument_descs.is_null() {
        return E_INVALIDARG;
    }
    let args: &[IndirectArgumentDesc] = if d.num_argument_descs == 0 {
        &[]
    } else {
        // SAFETY: `d.p_argument_descs` is non-null and holds `d.num_argument_descs` elements per the D3D12 contract.
        unsafe { core::slice::from_raw_parts(d.p_argument_descs, d.num_argument_descs as usize) }
    };
    let root = if root_sig.is_null() {
        None
    } else {
        // SAFETY: `root_sig` is non-null and caller-supplied.
        unsafe { crate::root_signature::object::cast_root_signature(root_sig) }
    };
    let slot_offsets = root.map(crate::root_signature::object::RootSignature::slot_qword_offsets);
    let parsed = match build_msl_source(d.byte_stride, args, slot_offsets) {
        Ok(p) => p,
        Err(hr) => return hr,
    };

    if cmd_sig.is_null() {
        return S_FALSE;
    }
    if this.is_null() {
        return E_INVALIDARG;
    }
    let dev = this.cast::<Device>();
    // SAFETY: `dev` is a valid `Device`.
    let mtl = unsafe { (*dev).mtl.as_ref() };
    let (render_resolver, compute_resolver) = if let Some(mtl) = mtl {
        let pso = match compile_resolver(mtl, &parsed.source) {
            Ok(p) => p,
            Err(hr) => return hr,
        };
        if parsed.command_type == D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH {
            (None, Some(pso))
        } else {
            (Some(pso), None)
        }
    } else {
        (None, None)
    };

    // SAFETY: `dev` is a live `Device` whose COM refcount is incremented for the child object.
    unsafe { (*dev).com.add_ref() };
    let obj = Box::into_raw(Box::new(CommandSignature {
        vtbl: &COMMAND_SIGNATURE_VTBL,
        com: ComObject::with_initial_ref(),
        private_data: PrivateData::default(),
        device: dev,
        command_type: parsed.command_type,
        update_root_arguments: parsed.update_root_arguments,
        update_vertex_buffers: parsed.update_vertex_buffers,
        update_index_buffer: parsed.update_index_buffer,
        byte_stride: parsed.byte_stride,
        render_resolver,
        compute_resolver,
    }));
    // SAFETY: `obj` was just allocated with refcount 1; `cmdsig_query_interface` adds a ref on success.
    let hr = unsafe { cmdsig_query_interface(obj.cast(), riid, cmd_sig) };
    // SAFETY: release the initial reference from `Box::into_raw`.
    unsafe { cmdsig_release(obj.cast()) };
    hr
}

#[cfg(test)]
mod tests;
