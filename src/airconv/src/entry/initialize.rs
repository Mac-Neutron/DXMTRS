//! Initialize: a shader read from its container, its reflection written.
//!
//! The container is read first and a DXIL part sent to the DXIL front end, as `SM50Initialize`
//! does; a DXBC shader is `airconv_ir::initialize::dxbc`'s, its reflection written here into
//! the caller's memory one field at a time (D3D11 hands it uninitialised: nothing is read, and
//! only the stage's part of the union is written). A DXIL shader's reflection is written whole,
//! zeroed first, as the C++'s `*refl = {}` did.

use core::{
    mem::{offset_of, size_of},
    ptr,
};
use std::ffi::CString;

use airconv_ir::{
    binding::{self, BindingType, ShaderArgument},
    container::{self, Container},
    dxil_part,
    initialize::{self, DxbcShader},
    reflection::{self, Reflection, StageReflection},
    signature::Parameter,
};

use crate::{
    abi::{
        DxilEntryElement, DxilEntryInfo, DxilEntryResource, MTL_SM50_SHADER_ARGUMENT_BUFFER,
        MTL_SM50_SHADER_ARGUMENT_ELEMENT_WIDTH, MTL_SM50_SHADER_ARGUMENT_READ_ACCESS,
        MTL_SM50_SHADER_ARGUMENT_TBUFFER_OFFSET, MTL_SM50_SHADER_ARGUMENT_TEXTURE,
        MTL_SM50_SHADER_ARGUMENT_TEXTURE_ARRAY, MTL_SM50_SHADER_ARGUMENT_TEXTURE_MINLOD_CLAMP,
        MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER, MTL_SM50_SHADER_ARGUMENT_WRITE_ACCESS,
        MtlGeometryShaderReflection, MtlPixelShaderReflection, MtlShaderReflection,
        MtlSm50ShaderArgument, MtlTessellatorReflection,
    },
    api::Error,
    dxil::{
        self,
        types::{EntryInfo, ResourceFlags},
    },
    shader::{Dxbc, Dxil, EntryView, Shader},
};

/// A signature name that holds a NUL, which a C string cannot carry.
pub const NAME_HOLDS_NUL: &[u8] = b"Invalid DXBC bytecode: a name holds a NUL";

// `airconv-ir` spells the reflection's argument flags itself; they are the header's.
const _: () = {
    assert!(binding::flag::BUFFER == MTL_SM50_SHADER_ARGUMENT_BUFFER);
    assert!(binding::flag::TEXTURE == MTL_SM50_SHADER_ARGUMENT_TEXTURE);
    assert!(binding::flag::ELEMENT_WIDTH == MTL_SM50_SHADER_ARGUMENT_ELEMENT_WIDTH);
    assert!(binding::flag::UAV_COUNTER == MTL_SM50_SHADER_ARGUMENT_UAV_COUNTER);
    assert!(binding::flag::TEXTURE_MINLOD_CLAMP == MTL_SM50_SHADER_ARGUMENT_TEXTURE_MINLOD_CLAMP);
    assert!(binding::flag::TBUFFER_OFFSET == MTL_SM50_SHADER_ARGUMENT_TBUFFER_OFFSET);
    assert!(binding::flag::TEXTURE_ARRAY == MTL_SM50_SHADER_ARGUMENT_TEXTURE_ARRAY);
    assert!(binding::flag::READ_ACCESS == MTL_SM50_SHADER_ARGUMENT_READ_ACCESS);
    assert!(binding::flag::WRITE_ACCESS == MTL_SM50_SHADER_ARGUMENT_WRITE_ACCESS);
};

/// Reads a DXBC or DXIL container into a shader and writes its reflection to `refl`.
///
/// # Errors
///
/// The container's refusal, the DXBC reader's or the DXIL front end's.
///
/// # Safety
///
/// `refl` is null or points to writable memory for one reflection, which may be uninitialised:
/// for DXBC only the stage's fields are written, nothing is read.
pub unsafe fn initialize(bytes: &[u8], refl: *mut MtlShaderReflection) -> Result<Shader, Error> {
    let container = Container::parse(bytes)?;
    if let Some(part) = dxil_part::find(&container)? {
        let ir = dxil::initialize::initialize(&part, capture_hash(bytes))?;
        let entry = dxil_entry_view(&ir.entry)?;
        if !refl.is_null() {
            // SAFETY: `refl` is non-null and writable for one reflection (the caller's contract).
            unsafe { write_whole_reflection(refl, &ir.reflection()) };
        }
        return Ok(Shader::Dxil(Dxil { ir, entry }));
    }
    let mut ir = initialize::dxbc(&container)?;
    if !refl.is_null() {
        let reflection = ir.reflect()?;
        // SAFETY: `refl` is non-null and writable for one reflection (the caller's contract).
        unsafe { write_reflection(refl, &reflection) };
    }
    let entry = entry_view(&ir)?;
    Ok(Shader::Dxbc(Dxbc {
        constant_buffers: ir.args_cbuffer.iter().map(argument).collect(),
        arguments: ir.args.iter().map(argument).collect(),
        entry,
        ir,
    }))
}

/// The shader's entry point as `DXILGetEntry` answers it.
#[must_use]
pub fn entry_info(shader: &Shader) -> DxilEntryInfo {
    shader.entry().info()
}

/// The DXBC half of `DXILGetEntry` (`BuildEntryView`).
///
/// The input signature and the rasterized output stream, each element one row at its register.
///
/// # Errors
///
/// [`NAME_HOLDS_NUL`].
pub fn entry_view(ir: &DxbcShader) -> Result<EntryView, Error> {
    let elements = || {
        ir.input
            .parameters
            .iter()
            .chain(&ir.output.rasterized().parameters)
    };
    let semantic_indices: Box<[u32]> = elements().map(|p| p.semantic_index).collect();
    let names = elements().map(c_name).collect::<Result<Vec<_>, _>>()?;
    let mut rows = elements()
        .zip(&names)
        .zip(&semantic_indices)
        .map(|((p, name), index)| DxilEntryElement {
            kind: reflection::semantic_kind(p.system_value, &p.semantic_name),
            rows: 1,
            // SV_Depth's register 0xFFFFFFFF reads -1, as DXIL marks an unpacked element.
            start_row: p.register.cast_signed(),
            name: name.as_ptr(),
            num_semantic_indices: 1,
            semantic_indices: ptr::from_ref(index),
        });
    let inputs = rows.by_ref().take(ir.input.parameters.len()).collect();
    let outputs = rows.collect();
    Ok(EntryView {
        is_dxil: false,
        stage: ir.program_type,
        inputs,
        outputs,
        resources: Vec::new(),
        names,
        semantic_indices,
    })
}

/// The DXIL half of `DXILGetEntry` (`BuildEntryView`).
///
/// The entry's elements, each with its rows and semantic indices, and its resource ranges (the
/// heap ranges last).
///
/// # Errors
///
/// [`NAME_HOLDS_NUL`].
pub fn dxil_entry_view(entry: &EntryInfo) -> Result<EntryView, Error> {
    let elements = || entry.inputs.iter().chain(&entry.outputs);
    let names = elements()
        .map(|e| CString::new(e.name.as_slice()).map_err(|_| Error::new(NAME_HOLDS_NUL)))
        .collect::<Result<Vec<_>, _>>()?;
    let semantic_indices: Box<[u32]> = elements()
        .flat_map(|e| e.semantic_indices.iter().copied())
        .collect();
    let mut offset = 0;
    let mut rows = elements().zip(&names).map(|(e, name)| {
        let indices = semantic_indices
            .get(offset..)
            .filter(|_| !e.semantic_indices.is_empty())
            .map_or(ptr::null(), <[u32]>::as_ptr);
        offset += e.semantic_indices.len();
        DxilEntryElement {
            kind: e.kind,
            rows: e.rows,
            start_row: e.start_row,
            name: name.as_ptr(),
            num_semantic_indices: binding::count_u32(e.semantic_indices.len()),
            semantic_indices: indices,
        }
    });
    let inputs = rows.by_ref().take(entry.inputs.len()).collect();
    let outputs = rows.collect();
    let resources = entry
        .resources
        .iter()
        .map(|r| DxilEntryResource {
            cls: r.cls,
            space: r.space,
            lower_bound: r.lower_bound,
            size: r.size,
            heap: u32::from(r.flags.contains(ResourceFlags::HEAP)),
        })
        .collect();
    Ok(EntryView {
        is_dxil: true,
        stage: entry.kind,
        inputs,
        outputs,
        resources,
        names,
        semantic_indices,
    })
}

/// FNV-1a 64 of the whole container.
///
/// The name D3D12's capture mode saves it under (`DXMT_PROBE` finds it by it).
fn capture_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A signature name as a C string.
///
/// # Errors
///
/// [`NAME_HOLDS_NUL`]: the signature reader stops at a name's first NUL, so it hands none.
pub fn c_name(p: &Parameter) -> Result<CString, Error> {
    CString::new(p.semantic_name.as_slice()).map_err(|_| Error::new(NAME_HOLDS_NUL))
}

/// An `airconv-ir` error as the translator answers it: its text.
impl From<container::Error> for Error {
    fn from(error: container::Error) -> Self {
        Self::new(error.what.as_bytes())
    }
}

/// A reflected argument in the header's layout.
const fn argument(a: &ShaderArgument) -> MtlSm50ShaderArgument {
    MtlSm50ShaderArgument {
        r#type: match a.ty {
            BindingType::ConstantBuffer => 0,
            BindingType::Sampler => 1,
            BindingType::Srv => 2,
            BindingType::Uav => 3,
        },
        sm50_binding_slot: a.slot,
        flags: a.flags,
        structure_ptr_offset: a.structure_ptr_offset,
    }
}

/// Writes the reflection's fields, the stage's among the union's members, and nothing else.
///
/// # Safety
///
/// `refl` is writable for one reflection; it need not be aligned or initialised.
unsafe fn write_reflection(refl: *mut MtlShaderReflection, r: &Reflection) {
    type R = MtlShaderReflection;
    // SAFETY: `refl` is writable for one reflection (the caller's contract).
    let out = unsafe { out::Out::new(refl) };
    out.put(
        offset_of!(R, constantt_buffer_table_bind_index),
        r.constant_buffer_table_bind_index,
    );
    out.put(
        offset_of!(R, argument_buffer_bind_index),
        r.argument_buffer_bind_index,
    );
    out.put(
        offset_of!(R, constant_buffer_slot_mask),
        r.constant_buffer_slot_mask,
    );
    out.put(offset_of!(R, sampler_slot_mask), r.sampler_slot_mask);
    out.put(offset_of!(R, srv_slot_mask_hi), r.srv_slot_mask_hi);
    out.put(offset_of!(R, srv_slot_mask_lo), r.srv_slot_mask_lo);
    out.put(offset_of!(R, uav_slot_mask), r.uav_slot_mask);
    out.put(offset_of!(R, writes_uav), r.writes_uav);
    out.put(offset_of!(R, num_constant_buffers), r.num_constant_buffers);
    out.put(offset_of!(R, num_arguments), r.num_arguments);
    let union = offset_of!(R, u);
    match r.stage {
        StageReflection::None => {}
        StageReflection::Compute { threadgroup_size } => out.put(union, threadgroup_size),
        StageReflection::Hull {
            threads_per_patch,
            partition,
            max_factor,
            output_primitive,
        } => {
            out.put(offset_of!(R, threads_per_patch), threads_per_patch);
            let tessellator = MtlTessellatorReflection {
                partition,
                max_factor,
                output_primitive,
            };
            out.put(union, tessellator);
        }
        StageReflection::Domain {
            max_potential_tess_factor,
        } => out.put(union, max_potential_tess_factor),
        StageReflection::Geometry {
            pass_through,
            primitive,
        } => {
            out.put(union, pass_through);
            out.put(
                union + offset_of!(MtlGeometryShaderReflection, primitive),
                primitive,
            );
        }
        StageReflection::Pixel {
            valid_render_targets,
            has_coverage_output,
        } => {
            let pixel = MtlPixelShaderReflection {
                valid_render_targets,
                has_coverage_output,
            };
            out.put(union, pixel);
        }
    }
    out.put(offset_of!(R, num_output_element), r.num_output_element);
    out.put(
        offset_of!(R, argument_table_qwords),
        r.argument_table_qwords,
    );
}

/// Zeroes the whole reflection, then writes `r`'s fields (DXIL's `*refl = {}` and fields).
///
/// # Safety
///
/// `refl` is writable for one reflection; it need not be aligned or initialised.
unsafe fn write_whole_reflection(refl: *mut MtlShaderReflection, r: &Reflection) {
    // SAFETY: `refl` is writable for one reflection (the caller's contract).
    let out = unsafe { out::Out::new(refl) };
    out.put(0, [0u8; size_of::<MtlShaderReflection>()]);
    // SAFETY: as above.
    unsafe { write_reflection(refl, r) };
}

/// The reflection being written, behind a constructor that takes the caller's contract.
mod out {
    use super::MtlShaderReflection;

    /// Writable memory for one reflection, maybe unaligned and uninitialised.
    pub struct Out(*mut MtlShaderReflection);

    impl Out {
        /// # Safety
        ///
        /// `refl` is writable for one reflection while the `Out` lives.
        pub const unsafe fn new(refl: *mut MtlShaderReflection) -> Self {
            Self(refl)
        }

        /// Writes `value` at `offset` bytes into the reflection, when it fits there.
        pub fn put<T>(&self, offset: usize, value: T) {
            if offset + size_of::<T>() > size_of::<MtlShaderReflection>() {
                return;
            }
            let place = self.0.wrapping_byte_add(offset).cast::<T>();
            // SAFETY: the memory is writable for one reflection (`new`'s contract) and `place`
            // lies inside it; the write needs no alignment.
            unsafe { place.write_unaligned(value) };
        }
    }
}

#[cfg(test)]
mod tests;
