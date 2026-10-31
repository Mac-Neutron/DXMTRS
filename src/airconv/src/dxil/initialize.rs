//! DXIL Initialize: the module parsed, its entry, resources and signature handlers read.
//!
//! Ported from `A/dxil/dxil_initialize.cpp` (`InitializeDXIL`). The bitcode is parsed into its
//! own context with typed pointers (through the shim's `parseBitcodeFile`, so a bad module
//! answers the C++'s `DXIL: can't read the bitcode: …`), read, and dropped; the shader keeps the
//! bitcode, which every compile parses again in its own context. The refusals come in the C++'s
//! order.

use core::ffi::c_ulonglong;
use std::collections::BTreeMap;

use airconv_ir::{
    dxil_part::DxilPart,
    reflection::{Reflection, StageReflection},
    shader_info::{
        ConstantBufferInfo, Declarations, ResourceRange, ResourceType, SamplerInfo, ScalerDataType,
        ShaderFlags, ShaderInfo, ShaderResourceViewInfo, UavFlags, UnorderedAccessViewInfo,
    },
};

use crate::{
    api::Error,
    dxil::{
        metadata::{
            arg, call_users, called_ops, dx_op_functions, heap_handle, op_code, read_entry,
            resolve_handle, walk,
        },
        signature::Io,
        types::{
            EntryInfo, HEAP_RANGE_BASE, HEAP_SPACE, MALFORMED_WAVE_SIZE, Resource, ResourceFlags,
            component_type, op, resource_class, resource_kind, shader_kind,
        },
    },
    llvm::{Context, Module, sys},
    module,
};

/// An initialized DXIL shader (`SM50ShaderInternal` with its `DXILShader`).
pub struct DxilShader {
    /// `D3D10_SB_TOKENIZED_PROGRAM_TYPE`: vertex, pixel, geometry or compute.
    pub program_type: u32,
    /// The resource maps (by range ID) and the helpers and lock table the module needs.
    pub info: ShaderInfo,
    /// The thread group size and the geometry shader's scalars; the rest at their initial values.
    pub decls: Declarations,
    /// The function signature, the handlers, the register counts and the unpacked inputs.
    pub io: Io,
    /// The DXIL part's bitcode, parsed again by every compile in its own context.
    pub bitcode: Vec<u8>,
    pub entry: EntryInfo,
    /// The simdgroup lane index and size arguments, `u32::MAX` each without wave operations.
    pub lane_index_arg: u32,
    pub lane_count_arg: u32,
    /// The entry has `[WaveOpsIncludeHelperLanes]`.
    ///
    /// A pixel shader's wave operations then count its helper lanes, which D3D otherwise leaves
    /// out.
    pub include_helper_lanes: bool,
    /// A compute shader with 2D `numthreads` (X and Y even) that reads quads.
    ///
    /// Its thread IDs are remapped so that Metal's lane quads run D3D's 2x2 quads.
    pub quads_2d: bool,
    /// The 64-bit groupshared atomics' threadgroup lock array size, 0 without them.
    pub gs_locks: u32,
    /// FNV-1a of the whole container, as capture mode names the shader.
    pub capture_hash: u64,
    /// Whether the shader writes a UAV (D3D12's render passes list only attachments as writes).
    pub writes_uav: bool,
}

/// `llvm-c/Target.h`'s `LLVMTargetDataRef`.
#[repr(C)]
struct TargetData {
    _data: [u8; 0],
    _marker: core::marker::PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}

// `llvm-c/Target.h`'s two functions `getTypeAllocSize` takes, which `llvm::sys` does not declare
// yet: declared here until lane l moves them there (the ledger's request, X1).
unsafe extern "C" {
    #[link_name = "LLVMGetModuleDataLayout"]
    fn get_module_data_layout(m: sys::ModuleRef) -> *mut TargetData;
    #[link_name = "LLVMABISizeOfType"]
    fn abi_size_of_type(td: *mut TargetData, ty: sys::TypeRef) -> c_ulonglong;
}

/// `InitializeDXIL`: the shader of a container's DXIL part.
///
/// # Errors
///
/// The reader's `DXIL: can't read the bitcode: …` (or P15's `LLVM error: …`), and the C++'s
/// `DXIL: …` refusals in its order: the entry's metadata, the 64-bit groupshared atomics, the
/// wave size, the stage, the signature elements.
pub fn initialize(part: &DxilPart<'_>, capture_hash: u64) -> Result<DxilShader, Error> {
    let cx = Context::new();
    let m = module::parse(
        &cx,
        part.bitcode,
        b"dxil",
        b"DXIL: can't read the bitcode: ",
    )?;
    let mut entry = read_entry(&m)?;
    let gs_locks = group_shared_locks(&m)?;
    add_heap_ranges(&m, &mut entry);
    if entry.wave_size_min == MALFORMED_WAVE_SIZE {
        return Err(Error::new(b"DXIL: malformed WaveSize metadata"));
    }
    // Apple GPUs run 32-lane SIMD groups (d3d12_device.cpp's WaveLaneCountMin/Max); the preferred
    // size is a hint.
    if entry.wave_size_min != 0 && !(entry.wave_size_min <= 32 && 32 <= entry.wave_size_max) {
        let text = format!(
            "DXIL: [WaveSize({}, {})] excludes this device's 32-lane waves",
            entry.wave_size_min, entry.wave_size_max
        );
        return Err(Error::new(text.as_bytes()));
    }
    let mut decls = Declarations::default();
    let program_type = match entry.kind {
        shader_kind::VERTEX | shader_kind::PIXEL | shader_kind::COMPUTE => entry.kind,
        shader_kind::GEOMETRY => {
            // Streams other than 0 only feed stream output, which DXMT lacks.
            if entry.gs_stream_mask != 1 {
                return Err(Error::new(
                    b"DXIL: geometry shader streams other than 0 not supported",
                ));
            }
            decls.gs_input_primitive = entry.gs_input_primitive;
            decls.gs_output_topology = entry.gs_output_topology;
            decls.gs_max_vertex_output = entry.gs_max_vertex_count;
            decls.gs_instance_count = entry.gs_instances;
            entry.kind
        }
        _ => {
            return Err(Error::new(
                b"DXIL: only vertex, pixel, geometry and compute shaders are supported",
            ));
        }
    };
    let ops = called_ops(&m);
    let mut info = ShaderInfo::default();
    fill_resource_maps(&entry, &m, &mut info);
    // B6: D3D12 render passes list their attachments as their writes; whatever this writes, they
    // cannot.
    let writes_uav = [
        op::TEXTURE_STORE,
        op::BUFFER_STORE,
        op::RAW_BUFFER_STORE,
        op::BUFFER_UPDATE_COUNTER,
        op::ATOMIC_BIN_OP,
        op::ATOMIC_COMPARE_EXCHANGE,
    ]
    .iter()
    .any(|o| ops.contains(o));
    // The compile links airconv's msad and sample position helpers for these.
    if ops.contains(&op::MSAD) {
        info.flags.insert(ShaderFlags::USE_MSAD);
    }
    if ops.contains(&op::TEXTURE_2D_MS_GET_SAMPLE_POSITION)
        || ops.contains(&op::RENDER_TARGET_GET_SAMPLE_POSITION)
    {
        info.flags.insert(ShaderFlags::USE_SAMPLEPOS);
    }
    // Ops 78 and 79 with the i64 overload take the device's lock table from the root arguments.
    // Each stage of a geometry pipeline binds with its own ShaderInfo, so its function declares
    // the field only for its own atomics.
    if uses_atomic64_locks(&m) {
        info.flags.insert(ShaderFlags::USE_ATOMIC64_LOCKS);
    }
    // 2D compute quads (HLSL_SM_6_6_Derivatives.md): numthreads X and Y even (Y = 1 is the 1D
    // layout, Metal's own), in a shader that takes derivatives, samples with an implicit LOD,
    // calculates a LOD or reads its quad.
    let quad_ops = [
        op::SAMPLE,
        op::SAMPLE_BIAS,
        op::SAMPLE_CMP,
        op::CALCULATE_LOD,
        op::DERIV_COARSE_X,
        op::DERIV_COARSE_Y,
        op::DERIV_FINE_X,
        op::DERIV_FINE_Y,
        op::QUAD_READ_LANE_AT,
        op::QUAD_OP,
        op::QUAD_VOTE,
        op::SAMPLE_CMP_BIAS,
    ];
    let [x, y, z] = entry.numthreads;
    let quads_2d = entry.kind == shader_kind::COMPUTE
        && x % 2 == 0
        && y % 2 == 0
        && quad_ops.iter().any(|o| ops.contains(o));
    let mut io = Io::default();
    io.add_signature_handlers(&entry, &ops, quads_2d, gs_locks != 0)?;
    let (lane_index_arg, lane_count_arg) = io.add_wave_inputs(&ops, program_type);
    if entry.kind == shader_kind::COMPUTE {
        decls.threadgroup_size = entry.numthreads;
        io.func_signature
            .use_max_work_group_size(x.wrapping_mul(y).wrapping_mul(z));
    }
    let include_helper_lanes = helper_lanes_included(&m, &entry.name);
    Ok(DxilShader {
        program_type,
        info,
        decls,
        io,
        bitcode: part.bitcode.to_vec(),
        entry,
        lane_index_arg,
        lane_count_arg,
        include_helper_lanes,
        quads_2d,
        gs_locks,
        capture_hash,
        writes_uav,
    })
}

impl DxilShader {
    /// What `InitializeDXIL` writes into the reflection, which it zeroes first.
    ///
    /// No argument tables: the stage's part, the output count and the UAV writes.
    #[must_use]
    pub fn reflection(&self) -> Reflection {
        let stage = match self.program_type {
            shader_kind::COMPUTE => StageReflection::Compute {
                threadgroup_size: self.decls.threadgroup_size,
            },
            // Never a pass-through: always the mesh pipeline.
            shader_kind::GEOMETRY => StageReflection::Geometry {
                pass_through: [0xff; 4],
                primitive: self.decls.gs_input_primitive,
            },
            shader_kind::PIXEL => StageReflection::Pixel {
                valid_render_targets: self.io.pso_valid_output_reg_mask,
                has_coverage_output: self.io.ps_has_coverage_output,
            },
            _ => StageReflection::None,
        };
        Reflection {
            constant_buffer_table_bind_index: u32::MAX,
            argument_buffer_bind_index: u32::MAX,
            num_constant_buffers: 0,
            num_arguments: 0,
            stage,
            constant_buffer_slot_mask: 0,
            sampler_slot_mask: 0,
            uav_slot_mask: 0,
            srv_slot_mask_hi: 0,
            srv_slot_mask_lo: 0,
            num_output_element: self.io.max_output_register,
            argument_table_qwords: 0,
            writes_uav: u32::from(self.writes_uav),
        }
    }
}

/// `GroupSharedLocks`: the threadgroup lock array of 64-bit groupshared atomics.
///
/// They are `atomicrmw` or `cmpxchg` of i64 in address space 3, which no `dx.op` carries and
/// Metal has not. 256 u32 locks, or the largest power of two that fits beside the shader's own
/// groupshared memory in D3D's 32768 bytes; 0 for a shader without such atomics.
///
/// # Errors
///
/// An `atomicrmw` operation DXC never emits, or no room for one lock.
fn group_shared_locks(m: &Module<'_>) -> Result<u32, Error> {
    let mut any = false;
    for inst in instructions(m) {
        // SAFETY: an instruction of the module.
        if !unsafe { sys::is_a_atomic_rmw_inst(inst) }.is_null()
            && in_threadgroup(operand(inst, 0))
            && is_i64(operand(inst, 1))
        {
            // SAFETY: an `atomicrmw`.
            let operation = unsafe { sys::get_atomic_rmw_bin_op(inst) };
            if !matches!(
                operation,
                sys::ATOMIC_RMW_BIN_OP_XCHG
                    | sys::ATOMIC_RMW_BIN_OP_ADD
                    | sys::ATOMIC_RMW_BIN_OP_AND
                    | sys::ATOMIC_RMW_BIN_OP_OR
                    | sys::ATOMIC_RMW_BIN_OP_XOR
                    | sys::ATOMIC_RMW_BIN_OP_MAX
                    | sys::ATOMIC_RMW_BIN_OP_MIN
                    | sys::ATOMIC_RMW_BIN_OP_U_MAX
                    | sys::ATOMIC_RMW_BIN_OP_U_MIN
            ) {
                let text = format!(
                    "DXIL: 64-bit groupshared atomicrmw {} not supported",
                    rmw_name(operation)
                );
                return Err(Error::new(text.as_bytes()));
            }
            any = true;
        }
        // SAFETY: an instruction of the module.
        if !unsafe { sys::is_a_atomic_cmp_xchg_inst(inst) }.is_null()
            && in_threadgroup(operand(inst, 0))
            && is_i64(operand(inst, 2))
        {
            any = true;
        }
    }
    if !any {
        return Ok(0);
    }
    // SAFETY: a live module; its data layout lives as long as it.
    let layout = unsafe { get_module_data_layout(m.raw()) };
    let mut used: u64 = 0;
    // SAFETY: a live module.
    let first = unsafe { sys::get_first_global(m.raw()) };
    // SAFETY: a global of the module.
    for global in walk(first, |g| unsafe { sys::get_next_global(g) }) {
        if in_threadgroup(global) {
            // SAFETY: a global of the module.
            let ty = unsafe { sys::global_get_value_type(global) };
            // SAFETY: the module's data layout and a type of the module.
            let size = unsafe { abi_size_of_type(layout, ty) };
            used = used.wrapping_add(size);
        }
    }
    let room = 32768u64.saturating_sub(used);
    if room < 4 {
        let text = format!(
            "DXIL: 64-bit groupshared atomics need 4 bytes of threadgroup memory for a lock; the \
             shader uses {used} of 32768"
        );
        return Err(Error::new(text.as_bytes()));
    }
    let mut n: u32 = 256;
    while u64::from(n) * 4 > room {
        n /= 2;
    }
    Ok(n)
}

/// `AddHeapRanges`: dynamic resources as heap ranges.
///
/// One range for each distinct `ResourceProperties` that `annotateHandle` gives a
/// `createHandleFromHeap` handle, in the use list's order, with range IDs from
/// [`HEAP_RANGE_BASE`].
fn add_heap_ranges(m: &Module<'_>, entry: &mut EntryInfo) {
    // SAFETY: a live module; a NUL-terminated name.
    let annotate = unsafe { sys::get_named_function(m.raw(), c"dx.op.annotateHandle".as_ptr()) };
    if annotate.is_null() {
        return;
    }
    let mut next = HEAP_RANGE_BASE;
    for call in call_users(annotate) {
        let Some((_, props)) = heap_handle(call) else {
            continue;
        };
        if entry
            .resources
            .iter()
            .any(|r| r.flags.contains(ResourceFlags::HEAP) && r.heap_props == props)
        {
            continue;
        }
        let mut r = heap_resource(props);
        r.id = next;
        next = next.wrapping_add(1);
        entry.resources.push(r);
    }
}

/// `HeapResource`: the heap range `annotateHandle`'s `ResourceProperties` `{p0, p1}` describe.
///
/// `DxilResourceProperties.h`: p0 bits 0-7 the kind, 12 UAV, 13 ROV, 14 globally coherent, 15
/// a comparison sampler or a UAV counter; p1 a texture's or typed buffer's component type (bits
/// 0-7), a structured buffer's stride or a constant buffer's size. As a heap range.
const fn heap_resource([p0, p1]: [u32; 2]) -> Resource {
    let kind = p0 & 0xff;
    let (uav, bit15) = (p0 & (1 << 12) != 0, p0 & (1 << 15) != 0);
    let cls = match kind {
        resource_kind::CBUFFER => resource_class::CBUFFER,
        resource_kind::SAMPLER => resource_class::SAMPLER,
        _ if uav => resource_class::UAV,
        _ => resource_class::SRV,
    };
    let mut r = Resource {
        id: 0,
        cls,
        kind,
        space: HEAP_SPACE,
        lower_bound: 0,
        size: u32::MAX,
        element_type: 0,
        stride: 0,
        cbuffer_size: 0,
        flags: ResourceFlags::HEAP,
        heap_props: [p0, p1],
    };
    r.flags
        .set(ResourceFlags::RASTERIZER_ORDERED, p0 & (1 << 13) != 0);
    r.flags
        .set(ResourceFlags::GLOBALLY_COHERENT, p0 & (1 << 14) != 0);
    r.flags.set(
        ResourceFlags::COMPARISON_SAMPLER,
        cls == resource_class::SAMPLER && bit15,
    );
    r.flags.set(
        ResourceFlags::HAS_COUNTER,
        cls == resource_class::UAV && bit15,
    );
    match kind {
        resource_kind::STRUCTURED_BUFFER => r.stride = p1,
        resource_kind::CBUFFER => r.cbuffer_size = p1,
        resource_kind::RAW_BUFFER | resource_kind::SAMPLER => {}
        _ => r.element_type = p1 & 0xff,
    }
    r
}

/// What a resource is used for, one bit each.
///
/// The binding maps pick Metal access modes and depth textures from it.
mod usage {
    pub const READ: u8 = 1;
    pub const WRITTEN: u8 = 1 << 1;
    pub const SAMPLED: u8 = 1 << 2;
    pub const COMPARED: u8 = 1 << 3;
    pub const ATOMIC: u8 = 1 << 4;
    pub const ATOMIC64: u8 = 1 << 5;
}

/// `ScanUsage`: each resource's uses by `(class, range ID)`.
///
/// They are read from every `dx.op` call whose second argument is a handle that resolves.
fn scan_usage(entry: &EntryInfo, m: &Module<'_>) -> BTreeMap<(u32, u32), u8> {
    let mut uses = BTreeMap::new();
    for call in dx_op_functions(m).into_iter().flat_map(call_users) {
        let Ok(handle) = resolve_handle(entry, arg(call, 1)) else {
            continue;
        };
        let bits = match op_code(call) {
            op::SAMPLE
            | op::SAMPLE_BIAS
            | op::SAMPLE_LEVEL
            | op::SAMPLE_GRAD
            | op::TEXTURE_GATHER
            | op::CALCULATE_LOD => usage::SAMPLED,
            op::SAMPLE_CMP
            | op::SAMPLE_CMP_LEVEL_ZERO
            | op::TEXTURE_GATHER_CMP
            | op::SAMPLE_CMP_GRAD
            | op::SAMPLE_CMP_BIAS => usage::SAMPLED | usage::COMPARED,
            op::TEXTURE_LOAD
            | op::BUFFER_LOAD
            | op::RAW_BUFFER_LOAD
            | op::C_BUFFER_LOAD
            | op::C_BUFFER_LOAD_LEGACY => usage::READ,
            op::TEXTURE_STORE | op::BUFFER_STORE | op::RAW_BUFFER_STORE => usage::WRITTEN,
            op::ATOMIC_BIN_OP | op::ATOMIC_COMPARE_EXCHANGE => {
                let wide = if is_i64(call) { usage::ATOMIC64 } else { 0 };
                usage::READ | usage::WRITTEN | usage::ATOMIC | wide
            }
            _ => 0,
        };
        *uses.entry((handle.cls, handle.range)).or_insert(0) |= bits;
    }
    uses
}

/// `FillResourceMaps`: airconv's resource maps, keyed by range ID.
///
/// As the DXBC `dcl_*` declarations fill them; a later resource of a range ID replaces an
/// earlier one.
fn fill_resource_maps(entry: &EntryInfo, m: &Module<'_>, info: &mut ShaderInfo) {
    let scanned = scan_usage(entry, m);
    for r in &entry.resources {
        let bits = scanned.get(&(r.cls, r.id)).copied().unwrap_or(0);
        let has = |bit: u8| bits & bit != 0;
        let range = || ResourceRange {
            range_id: r.id,
            lower_bound: r.lower_bound,
            size: r.size,
            space: r.space,
        };
        match r.cls {
            resource_class::SRV => {
                let srv = ShaderResourceViewInfo {
                    range: range(),
                    scaler_type: scaler_type(r.element_type),
                    resource_type: resource_type(r.kind),
                    read: has(usage::READ),
                    sampled: has(usage::SAMPLED),
                    compared: has(usage::COMPARED),
                    structure_stride: r.stride,
                    arg_index: u32::MAX,
                    arg_metadata_index: u32::MAX,
                };
                info.srv_map.insert(r.id, srv);
            }
            resource_class::UAV => {
                let resource_type = resource_type(r.kind);
                let mut flags = UavFlags::default();
                for (on, flag) in [
                    (has(usage::READ), UavFlags::READ),
                    (has(usage::WRITTEN), UavFlags::WRITTEN),
                    (
                        r.flags.contains(ResourceFlags::GLOBALLY_COHERENT),
                        UavFlags::GLOBAL_COHERENT,
                    ),
                    (
                        r.flags.contains(ResourceFlags::RASTERIZER_ORDERED),
                        UavFlags::RASTERIZER_ORDER,
                    ),
                    (
                        r.flags.contains(ResourceFlags::HAS_COUNTER),
                        UavFlags::WITH_COUNTER,
                    ),
                ] {
                    if on {
                        flags.insert(flag);
                    }
                }
                // Float atomics on a typed R32_FLOAT UAV (SM 6.6) go through an R32Uint view of
                // it: its loads and stores bitcast, its atomics take the integer path.
                let atomic_view = has(usage::ATOMIC)
                    && r.element_type == component_type::F32
                    && resource_type != ResourceType::NonApplicable;
                // 64-bit atomics on a typed buffer go through its buffer's address, the lock a raw
                // view takes. By use: DXC 1.9 declares a RWBuffer<uint64_t> or <int64_t> U32, with
                // an Atomic64Use tag that heap handles lack.
                if has(usage::ATOMIC64) && r.kind == resource_kind::TYPED_BUFFER {
                    flags.insert(UavFlags::ADDRESS64);
                }
                let scaler_type = if atomic_view {
                    flags.insert(UavFlags::ATOMIC_VIEW);
                    ScalerDataType::Uint
                } else {
                    scaler_type(r.element_type)
                };
                let uav = UnorderedAccessViewInfo {
                    range: range(),
                    scaler_type,
                    resource_type,
                    flags,
                    structure_stride: r.stride,
                    arg_index: u32::MAX,
                    arg_metadata_index: u32::MAX,
                    arg_counter_index: u32::MAX,
                };
                info.uav_map.insert(r.id, uav);
            }
            resource_class::CBUFFER => {
                let cb = ConstantBufferInfo {
                    range: range(),
                    size_in_vec4: r.cbuffer_size.wrapping_add(15) / 16,
                    arg_index: u32::MAX,
                };
                info.cbuffer_map.insert(r.id, cb);
            }
            resource_class::SAMPLER => {
                let sampler = SamplerInfo {
                    range: range(),
                    arg_index: u32::MAX,
                    arg_cube_index: u32::MAX,
                    arg_metadata_index: u32::MAX,
                };
                info.sampler_map.insert(r.id, sampler);
            }
            _ => {}
        }
    }
}

/// Whether a `dx.op` function returning i64 is called as op 78 or 79.
///
/// Those are `AtomicBinOp` and `AtomicCompareExchange`.
fn uses_atomic64_locks(m: &Module<'_>) -> bool {
    dx_op_functions(m)
        .into_iter()
        .filter(|&f| {
            // SAFETY: a function of the module.
            let ty = unsafe { sys::global_get_value_type(f) };
            // SAFETY: a function type.
            is_i64_type(unsafe { sys::get_return_type(ty) })
        })
        .flat_map(call_users)
        .any(|call| {
            matches!(
                op_code(call),
                op::ATOMIC_BIN_OP | op::ATOMIC_COMPARE_EXCHANGE
            )
        })
}

/// Whether the entry function `name` has `"waveops-include-helper-lanes"`.
fn helper_lanes_included(m: &Module<'_>, name: &[u8]) -> bool {
    let Ok(name) = crate::llvm::name(name) else {
        return false;
    };
    // SAFETY: a live module; a NUL-terminated name.
    let function = unsafe { sys::get_named_function(m.raw(), name.as_ptr()) };
    if function.is_null() {
        return false;
    }
    let attribute = b"waveops-include-helper-lanes";
    // SAFETY: a function of the module; the key holds its length.
    let found = unsafe {
        sys::get_string_attribute_at_index(
            function,
            sys::ATTRIBUTE_FUNCTION_INDEX,
            attribute.as_ptr().cast(),
            attribute.len().try_into().unwrap_or(u32::MAX),
        )
    };
    !found.is_null()
}

/// Every instruction of every function, in the module's order.
fn instructions(m: &Module<'_>) -> Vec<sys::ValueRef> {
    let mut all = Vec::new();
    // SAFETY: a live module.
    let first = unsafe { sys::get_first_function(m.raw()) };
    // SAFETY: a function of the module.
    for function in walk(first, |f| unsafe { sys::get_next_function(f) }) {
        // SAFETY: as above.
        let mut block = unsafe { sys::get_first_basic_block(function) };
        while !block.is_null() {
            // SAFETY: a block of the function.
            let first = unsafe { sys::get_first_instruction(block) };
            // SAFETY: an instruction of the block.
            all.extend(walk(first, |i| unsafe { sys::get_next_instruction(i) }));
            // SAFETY: as above.
            block = unsafe { sys::get_next_basic_block(block) };
        }
    }
    all
}

/// Operand `i` of an instruction, null past its operands.
fn operand(inst: sys::ValueRef, i: u32) -> sys::ValueRef {
    // SAFETY: an instruction of the module.
    let count = unsafe { sys::get_num_operands(inst) };
    if i64::from(i) >= i64::from(count) {
        return core::ptr::null_mut();
    }
    // SAFETY: an instruction with more than `i` operands.
    unsafe { sys::get_operand(inst, i) }
}

/// Whether `v` is a pointer into address space 3 (threadgroup memory).
fn in_threadgroup(v: sys::ValueRef) -> bool {
    if v.is_null() {
        return false;
    }
    // SAFETY: a value of the module.
    let ty = unsafe { sys::type_of(v) };
    // SAFETY: a type of the module.
    if unsafe { sys::get_type_kind(ty) } != sys::POINTER_TYPE_KIND {
        return false;
    }
    // SAFETY: a pointer type.
    unsafe { sys::get_pointer_address_space(ty) == 3 }
}

/// Whether `v`'s type is i64.
fn is_i64(v: sys::ValueRef) -> bool {
    // SAFETY: null checked first; else a value of the module.
    !v.is_null() && is_i64_type(unsafe { sys::type_of(v) })
}

fn is_i64_type(ty: sys::TypeRef) -> bool {
    // SAFETY: a type of the module.
    let kind = unsafe { sys::get_type_kind(ty) };
    // SAFETY: an integer type.
    kind == sys::INTEGER_TYPE_KIND && unsafe { sys::get_int_type_width(ty) } == 64
}

/// `AtomicRMWInst::getOperationName`.
const fn rmw_name(operation: sys::AtomicRMWBinOp) -> &'static str {
    match operation {
        sys::ATOMIC_RMW_BIN_OP_XCHG => "xchg",
        sys::ATOMIC_RMW_BIN_OP_ADD => "add",
        sys::ATOMIC_RMW_BIN_OP_SUB => "sub",
        sys::ATOMIC_RMW_BIN_OP_AND => "and",
        sys::ATOMIC_RMW_BIN_OP_NAND => "nand",
        sys::ATOMIC_RMW_BIN_OP_OR => "or",
        sys::ATOMIC_RMW_BIN_OP_XOR => "xor",
        sys::ATOMIC_RMW_BIN_OP_MAX => "max",
        sys::ATOMIC_RMW_BIN_OP_MIN => "min",
        sys::ATOMIC_RMW_BIN_OP_U_MAX => "umax",
        sys::ATOMIC_RMW_BIN_OP_U_MIN => "umin",
        sys::ATOMIC_RMW_BIN_OP_F_ADD => "fadd",
        sys::ATOMIC_RMW_BIN_OP_F_SUB => "fsub",
        sys::ATOMIC_RMW_BIN_OP_F_MAX => "fmax",
        sys::ATOMIC_RMW_BIN_OP_F_MIN => "fmin",
        _ => "<invalid operation>",
    }
}

/// `ToResourceType`: raw and structured buffers are `NonApplicable`.
const fn resource_type(kind: u32) -> ResourceType {
    match kind {
        resource_kind::TEXTURE_1D => ResourceType::Texture1D,
        resource_kind::TEXTURE_1D_ARRAY => ResourceType::Texture1DArray,
        resource_kind::TEXTURE_2D => ResourceType::Texture2D,
        resource_kind::TEXTURE_2D_ARRAY => ResourceType::Texture2DArray,
        resource_kind::TEXTURE_2D_MS => ResourceType::Texture2DMultisampled,
        resource_kind::TEXTURE_2D_MS_ARRAY => ResourceType::Texture2DMultisampledArray,
        resource_kind::TEXTURE_3D => ResourceType::Texture3D,
        resource_kind::TEXTURE_CUBE => ResourceType::TextureCube,
        resource_kind::TEXTURE_CUBE_ARRAY => ResourceType::TextureCubeArray,
        resource_kind::TYPED_BUFFER => ResourceType::TextureBuffer,
        _ => ResourceType::NonApplicable,
    }
}

/// `ToScalerType`.
const fn scaler_type(ty: u32) -> ScalerDataType {
    match ty {
        component_type::I16 | component_type::I32 | component_type::I64 => ScalerDataType::Int,
        component_type::U16 | component_type::U32 | component_type::U64 => ScalerDataType::Uint,
        _ => ScalerDataType::Float,
    }
}

#[cfg(test)]
mod tests;
