//! The signature handlers replayed into a function's prologue and epilogue.
//!
//! [`record`] runs the handlers of `A/dxbc_signature.cpp` against a compile's options (their
//! `SignatureContext` pass): each defines what the compile adds to the function signature and
//! leaves a step for the prologue or the epilogue, in the C++'s order. Once the function exists,
//! [`Replay::prologue`] stores the inputs into the input registers and [`Replay::epilogue`] packs
//! the output registers into the returned value. The helpers are
//! `A/dxbc_converter_basicblock.cpp:56-458`, vertex pulling `A/air_operations.cpp`.

use core::slice;

use airconv_ir::{
    binding::{
        AddressSpace, BufferBinding, FunctionInput, FunctionOutput, FunctionSignatureDesc,
        Interpolation, MemoryAccess, MslType, MslValue,
    },
    handlers::{SignatureHandler, msl_value},
};

use super::{
    converter::{invalid, req},
    resources::{Interpolant, Resources},
};
use crate::{
    abi::{
        SM50_BINDING_INDEX_VERTEX_BUFFER, Sm50IaInputElement, Sm50ShaderIaInputLayoutData,
        Sm50ShaderPsoPixelShaderData,
    },
    air::builder::{AirBuilder, Signedness},
    api::Error,
    llvm::{Context, Func, Ty, TyKind, Val, builder::FastMath, sys},
};

/// `RegisterComponentType`: what a pixel format or a signature element holds.
const COMPONENT_UNKNOWN: u32 = 0;
const COMPONENT_UINT: u32 = 1;
const COMPONENT_INT: u32 = 2;
const COMPONENT_FLOAT: u32 = 3;

/// The device address space, where vertex buffers live.
const DEVICE: u32 = 1;
/// The constant address space, where the vertex buffer table lives.
const CONSTANT: u32 = 2;

/// `MTLAttributeFormat`s (`A/air_operations.hpp`), the ones vertex pulling converts.
mod attribute_format {
    pub const UCHAR2: u32 = 1;
    pub const UCHAR4: u32 = 3;
    pub const CHAR2: u32 = 4;
    pub const CHAR4: u32 = 6;
    pub const UCHAR2_NORMALIZED: u32 = 7;
    pub const UCHAR4_NORMALIZED: u32 = 9;
    pub const CHAR2_NORMALIZED: u32 = 10;
    pub const CHAR4_NORMALIZED: u32 = 12;
    pub const USHORT2: u32 = 13;
    pub const USHORT4: u32 = 15;
    pub const SHORT2: u32 = 16;
    pub const SHORT4: u32 = 18;
    pub const USHORT2_NORMALIZED: u32 = 19;
    pub const USHORT4_NORMALIZED: u32 = 21;
    pub const SHORT2_NORMALIZED: u32 = 22;
    pub const SHORT4_NORMALIZED: u32 = 24;
    pub const HALF2: u32 = 25;
    pub const HALF4: u32 = 27;
    pub const FLOAT: u32 = 28;
    pub const FLOAT2: u32 = 29;
    pub const FLOAT3: u32 = 30;
    pub const FLOAT4: u32 = 31;
    pub const INT: u32 = 32;
    pub const INT2: u32 = 33;
    pub const INT3: u32 = 34;
    pub const INT4: u32 = 35;
    pub const UINT: u32 = 36;
    pub const UINT2: u32 = 37;
    pub const UINT3: u32 = 38;
    pub const UINT4: u32 = 39;
    pub const UINT1010102_NORMALIZED: u32 = 41;
    pub const UCHAR4_NORMALIZED_BGRA: u32 = 42;
    pub const UCHAR: u32 = 45;
    pub const CHAR: u32 = 46;
    pub const UCHAR_NORMALIZED: u32 = 47;
    pub const CHAR_NORMALIZED: u32 = 48;
    pub const USHORT: u32 = 49;
    pub const SHORT: u32 = 50;
    pub const USHORT_NORMALIZED: u32 = 51;
    pub const SHORT_NORMALIZED: u32 = 52;
    pub const HALF: u32 = 53;
    pub const FLOAT_RG11B10: u32 = 54;
    pub const FLOAT_RGB9E5: u32 = 55;
}

/// `MTLPixelFormat`s (`A/air_signature.hpp`) by what `component_type_from_pixel_format` reads.
mod pixel_format {
    pub const SINT: [u32; 9] = [14, 24, 34, 54, 64, 74, 104, 114, 124];
    pub const UINT: [u32; 10] = [13, 23, 33, 53, 63, 73, 91, 103, 113, 123];
    pub const FLOAT: [u32; 50] = [
        1, 10, 11, 12, 20, 22, 25, 30, 31, 32, 40, 41, 42, 43, 55, 60, 62, 65, 70, 71, 72, 80, 81,
        90, 92, 93, 94, 554, 555, 105, 110, 112, 115, 552, 553, 125, 130, 131, 132, 133, 134, 135,
        140, 141, 142, 143, 150, 151, 152, 153,
    ];
}

/// An input layout's elements and slot mask (`SM50_SHADER_IA_INPUT_LAYOUT_DATA`).
pub struct InputLayout<'a> {
    pub elements: &'a [Sm50IaInputElement],
    pub slot_mask: u32,
}

/// What a compile tells the handlers (`SignatureContext`'s options).
///
/// `pixel` is the pixel pipeline's node, read as its defaults when absent.
pub struct Options<'a> {
    pub ia_layout: Option<InputLayout<'a>>,
    pub pixel: Option<&'a Sm50ShaderPsoPixelShaderData>,
    pub pull_mode_reg_mask: u32,
    pub skip_vertex_output: bool,
}

/// The steps the handlers left, in their order.
pub struct Replay<'a> {
    prologue: Vec<Prologue<'a>>,
    epilogue: Vec<Epilogue>,
}

impl Replay<'_> {
    /// The prologue's steps at the builder's position: the input registers, the outputs' slots.
    ///
    /// # Errors
    ///
    /// An argument of a type the C++ did not store (its `UnsupportedFeature` texts), and a
    /// register file or special value the compile did not set.
    pub fn prologue<'cx>(
        &self,
        air: &AirBuilder<'_, 'cx>,
        function: Func<'cx>,
        res: &mut Resources<'cx>,
    ) -> Result<(), Error> {
        let b = air.builder();
        let types = air.types();
        for step in &self.prologue {
            match step {
                Prologue::StoreId {
                    instance,
                    reg,
                    mask,
                } => {
                    let id = if *instance {
                        req(res.instance_id, "no instance id")?
                    } else {
                        req(res.vertex_id, "no vertex id")?
                    };
                    let input = req(res.input.ptr_int4, "no input register file")?;
                    store_at_vec4_array_masked(air, input, air.get_int(*reg), id, *mask)?;
                }
                Prologue::Pull {
                    element,
                    slot_mask,
                    mask,
                    table,
                } => pull_vertex_input(air, function, res, element, *slot_mask, *mask, *table)?,
                Prologue::Init {
                    index,
                    reg,
                    mask,
                    fix_w,
                } => init_input_reg(air, function, res, *index, *reg, *mask, *fix_w)?,
                Prologue::Interpolate {
                    index,
                    reg,
                    mask,
                    interpolation,
                    sample_index,
                } => {
                    let perspective = matches!(
                        interpolation,
                        Interpolation::SamplePerspective
                            | Interpolation::CenterPerspective
                            | Interpolation::CentroidPerspective
                    );
                    let handle = arg(air, function, *index)?;
                    res.interpolant_map.insert(
                        *reg,
                        Interpolant {
                            handle,
                            perspective,
                        },
                    );
                    let sample = sample_index.map(|i| arg(air, function, i)).transpose()?;
                    let value = interpolate(air, handle, interpolation, sample)?;
                    let input = req(res.input.ptr_float4, "no input register file")?;
                    store_at_vec4_array_masked(air, input, air.get_int(*reg), value, *mask)?;
                }
                Prologue::Coverage { index } => {
                    res.coverage_mask_arg = Some(arg(air, function, *index)?);
                }
                Prologue::Alloca(special) => {
                    let (slot, ty) = match special {
                        Special::Depth => (&mut res.depth_output_reg, types.float),
                        Special::Stencil => (&mut res.stencil_ref_reg, types.int),
                        Special::Coverage => (&mut res.coverage_mask_reg, types.int),
                    };
                    *slot = Some(b.alloca(ty, c""));
                }
            }
        }
        Ok(())
    }

    /// The epilogue's steps at the builder's position, each inserting an output into `value`.
    ///
    /// `value` is the function's zero return value, `None` for a `void` function;
    /// `pso_sample_mask` masks the coverage output.
    ///
    /// # Errors
    ///
    /// An output step on a `void` function (the C++ inserted into a null), an output of a type
    /// no register converts to, and a register file the compile did not set.
    pub fn epilogue<'cx>(
        &self,
        air: &AirBuilder<'_, 'cx>,
        res: &Resources<'cx>,
        mut value: Option<Val<'cx>>,
        pso_sample_mask: u32,
    ) -> Result<Option<Val<'cx>>, Error> {
        let b = air.builder();
        let types = air.types();
        for step in &self.epilogue {
            let ret = value.ok_or_else(|| invalid("an output of a function that returns void"))?;
            value = Some(match step {
                Epilogue::Pop { reg, mask, index } => {
                    pop_output_reg(air, res, ret, *reg, *mask, *index)?
                }
                Epilogue::FixUnorm { reg, index } => {
                    let output = req(res.output.ptr_float4, "no output register file")?;
                    let fvec4 = load_from_array_at(air, output, air.get_int(*reg))?;
                    // The magic delta of `pop_output_reg_fix_unorm`.
                    let delta = air.get_float(1.0 / 127_500.0);
                    let fixed = b.fsub(fvec4, b.cx().const_vector(&[delta; 4]));
                    b.insert_value(ret, fixed, *index)
                }
                Epilogue::Position { reg, index } => {
                    let output = req(res.output.ptr_float4, "no output register file")?;
                    let fvec4 = load_from_array_at(air, output, air.get_int(*reg))?;
                    let fixed = air.sanitize_position(fvec4);
                    b.insert_value(ret, fixed, *index)
                }
                Epilogue::Special { special, index } => {
                    let (slot, ty, what) = match special {
                        Special::Depth => (res.depth_output_reg, types.float, "no depth output"),
                        Special::Stencil => (res.stencil_ref_reg, types.int, "no stencil output"),
                        Special::Coverage => {
                            (res.coverage_mask_reg, types.int, "no coverage output")
                        }
                    };
                    let ptr = b.const_inbounds_gep1_32(ty, req(slot, what)?, 0);
                    let mut output = b.load(ty, ptr);
                    if matches!(special, Special::Coverage) && pso_sample_mask != u32::MAX {
                        let mask = b.cx().const_int(types.int, pso_sample_mask.into(), false);
                        output = b.and(output, mask);
                    }
                    b.insert_value(ret, output, *index)
                }
            });
        }
        Ok(value)
    }
}

/// The handlers' pass: what each defines into `desc` and the steps it leaves.
///
/// # Errors
///
/// A signature element of an unknown component type, where the C++ fell off `to_msl_type`, and a
/// compute shader's handler (D1's `cs` replays those itself).
pub fn record<'a>(
    handlers: &[SignatureHandler],
    desc: &mut FunctionSignatureDesc,
    options: &Options<'a>,
) -> Result<Replay<'a>, Error> {
    let mut prologue = Vec::new();
    let mut epilogue = Vec::new();
    let (dual, no_depth, unorm_mask, formats) =
        options.pixel.map_or((false, false, 0, [0; 8]), |p| {
            (
                p.dual_source_blending != 0,
                p.disable_depth_output != 0,
                p.unorm_output_reg_mask,
                p.pixel_formats,
            )
        });
    let skip = options.skip_vertex_output;
    for handler in handlers {
        match handler {
            &SignatureHandler::VertexId { reg, mask } => prologue.push(Prologue::StoreId {
                instance: false,
                reg,
                mask,
            }),
            &SignatureHandler::InstanceId { reg, mask } => prologue.push(Prologue::StoreId {
                instance: true,
                reg,
                mask,
            }),
            SignatureHandler::VertexInput {
                reg,
                mask,
                component_type,
                name,
            } => {
                let Some(layout) = &options.ia_layout else {
                    let index = desc.define_input(FunctionInput::VertexStageIn {
                        attribute: *reg,
                        component_type: *component_type,
                        name: name.clone(),
                    });
                    prologue.push(Prologue::Init {
                        index,
                        reg: *reg,
                        mask: *mask,
                        fix_w: false,
                    });
                    continue;
                };
                if let Some(element) = layout.elements.iter().find(|e| e.reg == *reg) {
                    let table = desc.define_input(FunctionInput::Buffer(BufferBinding {
                        buffer_size: None,
                        location_index: SM50_BINDING_INDEX_VERTEX_BUFFER,
                        array_size: 0,
                        memory_access: MemoryAccess::Read,
                        address_space: AddressSpace::Constant,
                        ty: MslType::Value(MslValue::UINT),
                        arg_name: "vertex_buffers".into(),
                        raster_order_group: None,
                    }));
                    prologue.push(Prologue::Pull {
                        element,
                        slot_mask: layout.slot_mask,
                        mask: *mask,
                        table,
                    });
                }
            }
            &SignatureHandler::PopOutput { reg, mask, index } => {
                if !skip {
                    epilogue.push(Epilogue::Pop { reg, mask, index });
                }
            }
            &SignatureHandler::PopPosition { reg, index, .. } => {
                if !skip {
                    epilogue.push(Epilogue::Position { reg, index });
                }
            }
            &SignatureHandler::InputCoverage { index } => {
                prologue.push(Prologue::Coverage { index });
            }
            &SignatureHandler::InitInput {
                index,
                reg,
                mask,
                fix_w,
            } => prologue.push(Prologue::Init {
                index,
                reg,
                mask,
                fix_w,
            }),
            SignatureHandler::FragmentInput {
                reg,
                mask,
                interpolation,
                component_type,
                name,
            } => {
                let (reg, mask) = (*reg, *mask);
                // ARM64 shifts by the count mod 32, as the C++'s `1 << reg` ran.
                let pull_mode = options.pull_mode_reg_mask & 1u32.wrapping_shl(reg) != 0
                    && *interpolation != Interpolation::Flat;
                let index = desc.define_input(FunctionInput::FragmentStageIn {
                    user: name.clone(),
                    ty: msl_value(*component_type)?,
                    interpolation: interpolation.clone(),
                    pull_mode,
                });
                if pull_mode {
                    let sample_index = matches!(
                        interpolation,
                        Interpolation::SamplePerspective | Interpolation::SampleNoPerspective
                    )
                    .then(|| desc.define_input(FunctionInput::SampleIndex));
                    prologue.push(Prologue::Interpolate {
                        index,
                        reg,
                        mask,
                        interpolation: interpolation.clone(),
                        sample_index,
                    });
                } else {
                    prologue.push(Prologue::Init {
                        index,
                        reg,
                        mask,
                        fix_w: false,
                    });
                }
            }
            SignatureHandler::DepthRegister => prologue.push(Prologue::Alloca(Special::Depth)),
            SignatureHandler::DepthOutput { argument } => {
                if !no_depth {
                    let index = desc.define_output(FunctionOutput::Depth {
                        argument: argument.clone(),
                    });
                    epilogue.push(Epilogue::Special {
                        special: Special::Depth,
                        index,
                    });
                }
            }
            SignatureHandler::StencilRegister => prologue.push(Prologue::Alloca(Special::Stencil)),
            &SignatureHandler::StencilOutput { index } => epilogue.push(Epilogue::Special {
                special: Special::Stencil,
                index,
            }),
            SignatureHandler::CoverageRegister => {
                prologue.push(Prologue::Alloca(Special::Coverage));
            }
            &SignatureHandler::CoverageOutput { index } => epilogue.push(Epilogue::Special {
                special: Special::Coverage,
                index,
            }),
            &SignatureHandler::RenderTarget {
                reg,
                mask,
                component_type,
            } => {
                // A register past the eighth reads as no format (the C++ read past its array).
                let format = formats.get(reg as usize).copied().unwrap_or(0);
                let mut ty = component_type_from_pixel_format(format);
                if dual || ty == COMPONENT_UNKNOWN {
                    ty = component_type;
                }
                if dual && reg > 1 {
                    continue;
                }
                let index = desc.define_output(FunctionOutput::RenderTarget {
                    dual_source_blending: dual,
                    index: reg,
                    ty: msl_value(ty)?,
                });
                if ty == COMPONENT_FLOAT && unorm_mask & 1u32.wrapping_shl(reg) != 0 {
                    epilogue.push(Epilogue::FixUnorm { reg, index });
                } else {
                    epilogue.push(Epilogue::Pop { reg, mask, index });
                }
            }
            SignatureHandler::ThreadId { .. }
            | SignatureHandler::ThreadGroupId { .. }
            | SignatureHandler::ThreadIdInGroup { .. }
            | SignatureHandler::ThreadIdInGroupFlattened { .. } => {
                return Err(invalid("a compute shader's handler in another stage"));
            }
        }
    }
    Ok(Replay { prologue, epilogue })
}

/// The chain's input layout.
///
/// # Safety
///
/// `ia.elements` points to `ia.num_elements` live elements that live as long as `ia`
/// (`SM50Compile`'s contract for its argument chain), or `ia.num_elements` is 0.
#[must_use]
pub const unsafe fn input_layout(ia: &Sm50ShaderIaInputLayoutData) -> InputLayout<'_> {
    let elements = if ia.num_elements == 0 || ia.elements.is_null() {
        &[]
    } else {
        // SAFETY: the caller's contract.
        unsafe { slice::from_raw_parts(ia.elements, ia.num_elements as usize) }
    };
    InputLayout {
        elements,
        slot_mask: ia.slot_mask,
    }
}

/// The register-array element at `index` (`load_from_array_at`).
///
/// The GEP is in bounds when `index` is a constant.
///
/// # Errors
///
/// `array` does not point to an array.
pub fn load_from_array_at<'cx>(
    air: &AirBuilder<'_, 'cx>,
    array: Val<'cx>,
    index: Val<'cx>,
) -> Result<Val<'cx>, Error> {
    let b = air.builder();
    let (array_ty, element) = array_of(air, array)?;
    let indices = [air.get_int(0), index];
    let ptr = if index.const_u64().is_some() {
        b.inbounds_gep(array_ty, array, &indices)
    } else {
        b.gep(array_ty, array, &indices)
    };
    Ok(b.load(element, ptr))
}

/// The `mask` components of a `<4 x …>` array element, the others zero.
///
/// `load_from_vec4_array_masked`: the whole element for every component.
///
/// # Errors
///
/// `array` does not point to an array of vectors.
pub fn load_from_vec4_array_masked<'cx>(
    air: &AirBuilder<'_, 'cx>,
    array: Val<'cx>,
    index: Val<'cx>,
    mask: u32,
) -> Result<Val<'cx>, Error> {
    if mask == 0b1111 {
        return load_from_array_at(air, array, index);
    }
    let b = air.builder();
    let tcx = b.cx().tcx();
    let (array_ty, vec4_ty) = array_of(air, array)?;
    let element = tcx
        .element(vec4_ty)
        .ok_or_else(|| invalid("a register array of scalars"))?;
    let mut value = b.cx().null(vec4_ty);
    for i in 0..4 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let ptr = b.gep(array_ty, array, &[air.get_int(0), index, air.get_int(i)]);
        value = b.insert_element_u64(value, b.load(element, ptr), u64::from(i));
    }
    Ok(value)
}

/// `value` widened to four lanes and stored into the `mask` components of the array element.
///
/// `store_at_vec4_array_masked`.
///
/// # Errors
///
/// A vector of more than four lanes; `array` does not point to an array.
pub fn store_at_vec4_array_masked<'cx>(
    air: &AirBuilder<'_, 'cx>,
    array: Val<'cx>,
    index: Val<'cx>,
    value: Val<'cx>,
    mask: u32,
) -> Result<(), Error> {
    let b = air.builder();
    let vec4 = extend_to_vec4(air, value)?;
    let (array_ty, _) = array_of(air, array)?;
    if mask == 0b1111 {
        let ptr = b.inbounds_gep(array_ty, array, &[air.get_int(0), index]);
        b.store(vec4, ptr);
        return Ok(());
    }
    for i in 0..4 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let ptr = b.gep(array_ty, array, &[air.get_int(0), index, air.get_int(i)]);
        b.store(b.extract_element_u64(vec4, u64::from(i)), ptr);
    }
    Ok(())
}

/// The output register `reg` as the mesh vertex's position (`pop_mesh_output_position`).
///
/// # Errors
///
/// No output register file; what the AIR builder refuses.
pub fn pop_mesh_output_position<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    reg: u32,
    vertex_id: Val<'cx>,
) -> Result<(), Error> {
    let output = req(res.output.ptr_float4, "no output register file")?;
    let result = load_from_array_at(air, output, air.get_int(reg))?;
    air.create_set_mesh_position(vertex_id, result)?;
    Ok(())
}

/// The output register `reg`'s `mask` component as the primitive's render target array index.
///
/// `pop_mesh_output_render_target_array_index`.
///
/// # Errors
///
/// No output register file; what the AIR builder refuses.
pub fn pop_mesh_output_render_target_array_index<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    reg: u32,
    mask: u32,
    primitive_id: Val<'cx>,
) -> Result<(), Error> {
    let result = output_int(air, res, reg, mask)?;
    air.create_set_mesh_render_target_array_index(primitive_id, result)?;
    Ok(())
}

/// The output register `reg`'s `mask` component as the primitive's viewport array index.
///
/// `pop_mesh_output_viewport_array_index`.
///
/// # Errors
///
/// No output register file; what the AIR builder refuses.
pub fn pop_mesh_output_viewport_array_index<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    reg: u32,
    mask: u32,
    primitive_id: Val<'cx>,
) -> Result<(), Error> {
    let result = output_int(air, res, reg, mask)?;
    air.create_set_mesh_viewport_array_index(primitive_id, result)?;
    Ok(())
}

/// The output register `reg` as the mesh vertex's user data `index` of type `desired`.
///
/// `pop_mesh_output_vertex_data`; `desired` is the data's `MSLScalerOrVectorType` as an LLVM
/// type.
///
/// # Errors
///
/// No output register file, a type no register converts to; what the AIR builder refuses.
pub fn pop_mesh_output_vertex_data<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    reg: u32,
    mask: u32,
    index: u32,
    vertex_id: Val<'cx>,
    desired: Ty,
) -> Result<(), Error> {
    let output = req(res.output.ptr_int4, "no output register file")?;
    let ivec4 = load_from_vec4_array_masked(air, output, air.get_int(reg), mask)?;
    let result = to_desired_type_from_int_vec4(air, ivec4, desired, mask)?;
    air.create_set_mesh_vertex_data(vertex_id, air.get_int(index), result)?;
    Ok(())
}

/// The function's return value before any output: zero, `None` for a `void` function.
pub fn zero_return<'cx>(cx: &'cx Context, function: Func<'cx>) -> Option<Val<'cx>> {
    let tcx = cx.tcx();
    let ret = tcx.ret(function.fn_ty())?;
    (tcx.kind(ret) != TyKind::Void).then(|| cx.null(ret))
}

/// A prologue step.
enum Prologue<'a> {
    /// The vertex or instance ID into input `reg`.
    StoreId { instance: bool, reg: u32, mask: u32 },
    /// Input `element.reg` pulled from its vertex buffer, the table being argument `table`.
    Pull {
        element: &'a Sm50IaInputElement,
        slot_mask: u32,
        mask: u32,
        table: u32,
    },
    /// Argument `index` into input `reg` (`init_input_reg`).
    Init {
        index: u32,
        reg: u32,
        mask: u32,
        fix_w: bool,
    },
    /// Argument `index` interpolated into input `reg` (`init_input_reg_with_interpolation`).
    Interpolate {
        index: u32,
        reg: u32,
        mask: u32,
        interpolation: Interpolation,
        sample_index: Option<u32>,
    },
    /// Argument `index` as the input coverage.
    Coverage { index: u32 },
    /// A special output's register.
    Alloca(Special),
}

/// An epilogue step: one output inserted into the returned value at element `index`.
enum Epilogue {
    /// Output `reg` converted to the element's type (`pop_output_reg`).
    Pop { reg: u32, mask: u32, index: u32 },
    /// Output `reg` less the unorm delta (`pop_output_reg_fix_unorm`).
    FixUnorm { reg: u32, index: u32 },
    /// Output `reg` as a sanitized position (`pop_output_reg_sanitize_pos`).
    Position { reg: u32, index: u32 },
    /// A special output's register.
    Special { special: Special, index: u32 },
}

/// A pixel shader output held in a register of its own.
enum Special {
    Depth,
    Stencil,
    Coverage,
}

/// `component_type_from_pixel_format`.
fn component_type_from_pixel_format(format: u32) -> u32 {
    if pixel_format::SINT.contains(&format) {
        COMPONENT_INT
    } else if pixel_format::UINT.contains(&format) {
        COMPONENT_UINT
    } else if pixel_format::FLOAT.contains(&format) {
        COMPONENT_FLOAT
    } else {
        COMPONENT_UNKNOWN
    }
}

/// Argument `index` of the function.
fn arg<'cx>(air: &AirBuilder<'_, 'cx>, function: Func<'cx>, index: u32) -> Result<Val<'cx>, Error> {
    function
        .param(air.builder().cx().tcx(), index)
        .ok_or_else(|| invalid("a handler of no argument"))
}

/// The pointee of a register-array pointer and its element.
fn array_of(air: &AirBuilder<'_, '_>, array: Val<'_>) -> Result<(Ty, Ty), Error> {
    let tcx = air.builder().cx().tcx();
    let array_ty = tcx
        .pointee(array.ty())
        .ok_or_else(|| invalid("a register array is not a pointer"))?;
    let element = tcx
        .element(array_ty)
        .ok_or_else(|| invalid("a register array is not an array"))?;
    Ok((array_ty, element))
}

/// `value` as four lanes, the last repeated, a scalar splat (`extend_to_vec4`).
fn extend_to_vec4<'cx>(air: &AirBuilder<'_, 'cx>, value: Val<'cx>) -> Result<Val<'cx>, Error> {
    let b = air.builder();
    let mask: &[i32] = match b.cx().tcx().kind(value.ty()) {
        TyKind::Vector(_, 4) => &[0, 1, 2, 3],
        TyKind::Vector(_, 3) => &[0, 1, 2, 2],
        TyKind::Vector(_, 2) => &[0, 1, 1, 1],
        TyKind::Vector(_, 1) => &[0, 0, 0, 0],
        TyKind::Vector(..) => return Err(invalid("a vector of more than four lanes")),
        _ => return Ok(b.vector_splat(4, value, c"")),
    };
    Ok(b.shuffle_vector1(value, mask))
}

/// An `<4 x i32>` register's `mask` lanes as `desired` (`to_desired_type_from_int_vec4`).
fn to_desired_type_from_int_vec4<'cx>(
    air: &AirBuilder<'_, 'cx>,
    vec4: Val<'cx>,
    desired: Ty,
    mask: u32,
) -> Result<Val<'cx>, Error> {
    let b = air.builder();
    let t = air.types();
    let lanes = [0, 1, 2, 3].map(|i: i32| if mask & (1 << i) != 0 { i } else { -1 });
    let float4 = || b.shuffle_vector1(b.bitcast(vec4, t.float4), &lanes);
    Ok(if desired == t.int4 {
        b.shuffle_vector1(vec4, &lanes)
    } else if desired == t.float4 {
        float4()
    } else if desired == t.int3 {
        b.shuffle_vector1(vec4, &lanes[..3])
    } else if desired == t.float3 {
        b.shuffle_vector1(float4(), &lanes[..3])
    } else if desired == t.int2 {
        b.shuffle_vector1(vec4, &lanes[..2])
    } else if desired == t.float2 {
        b.shuffle_vector1(float4(), &lanes[..2])
    } else if desired == t.int {
        b.extract_element_u64(vec4, mask.trailing_zeros().into())
    } else if desired == t.float {
        b.extract_element_u64(b.bitcast(vec4, t.float4), mask.trailing_zeros().into())
    } else {
        return Err(invalid("an output of a type no register converts to"));
    })
}

/// Output `reg` converted to element `index` of `ret` and inserted there (`pop_output_reg`).
fn pop_output_reg<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    ret: Val<'cx>,
    reg: u32,
    mask: u32,
    index: u32,
) -> Result<Val<'cx>, Error> {
    let b = air.builder();
    let output = req(res.output.ptr_int4, "no output register file")?;
    let ivec4 = load_from_array_at(air, output, air.get_int(reg))?;
    let desired = b
        .cx()
        .tcx()
        .field(ret.ty(), index)
        .ok_or_else(|| invalid("an output the function does not return"))?;
    let value = to_desired_type_from_int_vec4(air, ivec4, desired, mask)?;
    Ok(b.insert_value(ret, value, index))
}

/// Output `reg`'s `mask` component as an `i32`.
fn output_int<'cx>(
    air: &AirBuilder<'_, 'cx>,
    res: &Resources<'cx>,
    reg: u32,
    mask: u32,
) -> Result<Val<'cx>, Error> {
    let output = req(res.output.ptr_int4, "no output register file")?;
    let ivec4 = load_from_array_at(air, output, air.get_int(reg))?;
    to_desired_type_from_int_vec4(air, ivec4, air.types().int, mask)
}

/// Argument `index` stored into input `reg` (`init_input_reg`).
///
/// A front-facing `bool` is sign-extended; with `fix_w` a position's w is replaced by its
/// reciprocal.
fn init_input_reg<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    res: &Resources<'cx>,
    index: u32,
    reg: u32,
    mask: u32,
    fix_w: bool,
) -> Result<(), Error> {
    let b = air.builder();
    let t = air.types();
    let tcx = b.cx().tcx();
    let mut value = arg(air, function, index)?;
    let at = air.get_int(reg);
    let scalar = tcx.element(value.ty()).unwrap_or_else(|| value.ty());
    match tcx.kind(scalar) {
        TyKind::Int(_) => {
            let input = req(res.input.ptr_int4, "no input register file")?;
            if scalar == t.bool {
                return store_at_vec4_array_masked(air, input, at, b.sext(value, t.int), mask);
            }
            if scalar != t.int {
                return Err(Error::new(b"TODO: handle non 32 bit integer"));
            }
            store_at_vec4_array_masked(air, input, at, value, mask)
        }
        TyKind::Float => {
            if fix_w && matches!(tcx.kind(value.ty()), TyKind::Vector(_, 4)) {
                let w = b.extract_element_u64(value, 3);
                // MacNeutron (B6): a reciprocal (`arcp`, `afn`), as Metal Shader Converter emits it.
                let rcp_w = {
                    let flags = b
                        .fast_math()
                        .with(FastMath::ALLOW_RECIPROCAL)
                        .with(FastMath::APPROX_FUNC);
                    let _flags = b.fast_math_guard(flags);
                    b.fdiv(air.get_float(1.0), w)
                };
                value = b.insert_element_u64(value, rcp_w, 3);
            }
            let input = req(res.input.ptr_float4, "no input register file")?;
            store_at_vec4_array_masked(air, input, at, value, mask)
        }
        _ => Err(Error::new(
            b"TODO: unhandled input type? might be interpolant...",
        )),
    }
}

/// The interpolant read in the shader at its mode's position.
///
/// `init_input_reg_with_interpolation`'s switch; `sample` is the sample index of a per-sample
/// mode.
fn interpolate<'cx>(
    air: &AirBuilder<'_, 'cx>,
    interpolant: Val<'cx>,
    interpolation: &Interpolation,
    sample: Option<Val<'cx>>,
) -> Result<Val<'cx>, Error> {
    let at_sample = |perspective| {
        let sample = sample.ok_or_else(|| invalid("a per-sample input of no sample index"))?;
        air.create_interpolate_at_sample(interpolant, sample, perspective)
    };
    match interpolation {
        Interpolation::CenterPerspective => air.create_interpolate_at_center(interpolant, true),
        Interpolation::CenterNoPerspective => air.create_interpolate_at_center(interpolant, false),
        Interpolation::CentroidPerspective => air.create_interpolate_at_centroid(interpolant, true),
        Interpolation::CentroidNoPerspective => {
            air.create_interpolate_at_centroid(interpolant, false)
        }
        Interpolation::SamplePerspective => at_sample(true),
        Interpolation::SampleNoPerspective => at_sample(false),
        Interpolation::Flat => Err(invalid("a flat input interpolated in the shader")),
    }
}

/// Input `element.reg` read from its vertex buffer (`pull_vertex_input`).
///
/// The vertex buffer table, argument `table`, is cast once per function; the element's entry is
/// its slot's place among the bound slots of `slot_mask`.
fn pull_vertex_input<'cx>(
    air: &AirBuilder<'_, 'cx>,
    function: Func<'cx>,
    res: &mut Resources<'cx>,
    element: &Sm50IaInputElement,
    slot_mask: u32,
    mask: u32,
    table: u32,
) -> Result<(), Error> {
    let b = air.builder();
    let t = air.types();
    let tcx = b.cx().tcx();
    let index = if element.step_function() == 0 {
        req(res.vertex_id_with_base, "no vertex id")?
    } else if element.step_rate() == 0 {
        // Instance data that never steps.
        req(res.base_instance_id, "no base instance")?
    } else {
        let base = req(res.base_instance_id, "no base instance")?;
        let instance = req(res.instance_id, "no instance id")?;
        b.add(base, b.udiv(instance, air.get_int(element.step_rate())))
    };
    let vertex_buffer_table = if let Some(table) = res.vertex_buffer_table {
        table
    } else {
        let ptr = tcx.ptr(t.dxmt_vertex_buffer_entry, CONSTANT);
        let cast = b.bitcast(arg(air, function, table)?, ptr);
        res.vertex_buffer_table = Some(cast);
        cast
    };
    // Shifts by 32 or more wrap mod 32, as the C++'s ran on ARM64.
    let entry_index = if element.slot == 0 {
        0
    } else {
        let shift = 32u32.wrapping_sub(element.slot);
        slot_mask
            .wrapping_shl(shift)
            .wrapping_shr(shift)
            .count_ones()
    };
    let entry_ty = t.dxmt_vertex_buffer_entry;
    let entry = b.load(
        entry_ty,
        b.const_gep1_32(entry_ty, vertex_buffer_table, entry_index),
    );
    let base_addr = b.extract_value(entry, 0);
    let stride = b.extract_value(entry, 1);
    let byte_offset = b.add(
        b.mul(stride, index),
        air.get_int(element.aligned_byte_offset),
    );
    let vec4 = pull_vec4_from_addr(air, element.format, base_addr, byte_offset)?;
    let file = if vec4.ty() == t.float4 {
        res.input.ptr_float4
    } else {
        res.input.ptr_int4
    };
    let input = req(file, "no input register file")?;
    store_at_vec4_array_masked(air, input, air.get_int(element.reg), vec4, mask)
}

/// An attribute of `format` at `byte_offset` from `base_addr`, as four lanes.
///
/// `pull_vec4_from_addr`: integers sign- or zero-extended to `<4 x i32>`, the rest converted to
/// `<4 x float>`, missing lanes read as (0, 0, 1).
fn pull_vec4_from_addr<'cx>(
    air: &AirBuilder<'_, 'cx>,
    format: u32,
    base_addr: Val<'cx>,
    byte_offset: Val<'cx>,
) -> Result<Val<'cx>, Error> {
    use attribute_format as f;
    let b = air.builder();
    let t = air.types();
    let load = |ty, align| load_from_device_buffer(air, ty, base_addr, byte_offset, 0, align);
    let int = |v| widen(air, v, t.int4, air.get_int(0), air.get_int(1));
    let float = |v| widen(air, v, t.float4, air.get_float(0.0), air.get_float(1.0));
    let half = |ty| air.create_convert_to_float(load(ty, 2), Signedness::Signed);
    Ok(match format {
        f::CHAR => int(b.sext(load(t.byte, 1), t.int)),
        f::CHAR2 => int(b.sext(load(t.char2, 1), t.int2)),
        f::CHAR4 => b.sext(load(t.char4, 1), t.int4),
        f::UCHAR => int(b.zext(load(t.byte, 1), t.int)),
        f::UCHAR2 => int(b.zext(load(t.char2, 1), t.int2)),
        f::UCHAR4 => b.zext(load(t.char4, 1), t.int4),
        f::SHORT => int(b.sext(load(t.short, 2), t.int)),
        f::SHORT2 => int(b.sext(load(t.short2, 2), t.int2)),
        f::SHORT4 => b.sext(load(t.short4, 2), t.int4),
        f::USHORT => int(b.zext(load(t.short, 2), t.int)),
        f::USHORT2 => int(b.zext(load(t.short2, 2), t.int2)),
        f::USHORT4 => b.zext(load(t.short4, 2), t.int4),
        f::HALF => float(half(t.half)?),
        f::HALF2 => float(half(t.half2)?),
        f::HALF4 => half(t.half4)?,
        f::FLOAT => float(load(t.float, 4)),
        f::FLOAT2 => float(load(t.float2, 4)),
        f::FLOAT3 => float(load(t.float3, 4)),
        f::FLOAT4 => load(t.float4, 4),
        f::UINT | f::INT => int(load(t.int, 4)),
        f::UINT2 | f::INT2 => int(load(t.int2, 4)),
        f::UINT3 | f::INT3 => int(load(t.int3, 4)),
        f::UINT4 | f::INT4 => load(t.int4, 4),
        _ => float(unpack(air, format, base_addr, byte_offset)?),
    })
}

/// A normalized or packed attribute through `air.unpack.*` (`unpack_fvec4_from_addr`).
///
/// Answers the unpacked value, BGRA swizzled to RGBA, before any widening.
fn unpack<'cx>(
    air: &AirBuilder<'_, 'cx>,
    format: u32,
    base_addr: Val<'cx>,
    offset: Val<'cx>,
) -> Result<Val<'cx>, Error> {
    use attribute_format as f;
    let b = air.builder();
    let t = air.types();
    if format == f::USHORT4_NORMALIZED || format == f::SHORT4_NORMALIZED {
        let op = if format == f::USHORT4_NORMALIZED {
            "unorm2x16.v2f32"
        } else {
            "snorm2x16.v2f32"
        };
        let xy = load_from_device_buffer(air, t.int, base_addr, offset, 0, 2);
        let xy = call_unpack(air, op, xy, t.float2)?;
        let zw = load_from_device_buffer(air, t.int, base_addr, offset, 1, 2);
        let zw = call_unpack(air, op, zw, t.float2)?;
        return Ok(b.shuffle_vector(xy, zw, &[0, 1, 2, 3]));
    }
    let (op, src, align, dst) = match format {
        f::UCHAR_NORMALIZED => ("unorm1x8.f32", t.byte, 1, t.float),
        f::UCHAR2_NORMALIZED => ("unorm2x8.v2f32", t.short, 1, t.float2),
        f::UCHAR4_NORMALIZED | f::UCHAR4_NORMALIZED_BGRA => ("unorm4x8.v4f32", t.int, 1, t.float4),
        f::CHAR_NORMALIZED => ("snorm1x8.f32", t.byte, 1, t.float),
        f::CHAR2_NORMALIZED => ("snorm2x8.v2f32", t.short, 1, t.float2),
        f::CHAR4_NORMALIZED => ("snorm4x8.v4f32", t.int, 1, t.float4),
        f::USHORT_NORMALIZED => ("unorm1x16.f32", t.short, 2, t.float),
        f::USHORT2_NORMALIZED => ("unorm2x16.v2f32", t.int, 2, t.float2),
        f::SHORT_NORMALIZED => ("snorm1x16.f32", t.short, 2, t.float),
        f::SHORT2_NORMALIZED => ("snorm2x16.v2f32", t.int, 2, t.float2),
        f::UINT1010102_NORMALIZED => ("unorm.rgb10a2.v4f32", t.int, 4, t.float4),
        f::FLOAT_RG11B10 => ("unorm.rg11b10f.v3f32", t.int, 4, t.float3),
        f::FLOAT_RGB9E5 => ("unorm.rgb9e5.v3f32", t.int, 4, t.float3),
        // Invalid, Int1010102Normalized ("unused") and numbers no format has: the C++ stored a
        // null.
        _ => return Err(invalid("a vertex attribute of an unknown format")),
    };
    let packed = load_from_device_buffer(air, src, base_addr, offset, 0, align);
    let value = call_unpack(air, op, packed, dst)?;
    Ok(if format == f::UCHAR4_NORMALIZED_BGRA {
        b.shuffle_vector1(value, &[2, 1, 0, 3])
    } else {
        value
    })
}

/// `air.unpack.<op>` of `src`, declared `nounwind willreturn mustprogress nofree nosync readnone`.
fn call_unpack<'cx>(
    air: &AirBuilder<'_, 'cx>,
    op: &str,
    src: Val<'cx>,
    dst: Ty,
) -> Result<Val<'cx>, Error> {
    let b = air.builder();
    let cx = b.cx();
    let attrs: Vec<_> = [
        &b"nounwind"[..],
        b"willreturn",
        b"mustprogress",
        b"nofree",
        b"nosync",
        b"readnone",
    ]
    .iter()
    .filter_map(|name| Some((sys::ATTRIBUTE_FUNCTION_INDEX, cx.enum_attribute(name, 0)?)))
    .collect();
    let name = [&b"air.unpack."[..], op.as_bytes()].concat();
    let fn_ty = cx.tcx().function(dst, &[src.ty()], false);
    let callee = air.module().get_or_insert_function(&name, fn_ty, &attrs)?;
    Ok(b.call(callee, &[src]))
}

/// A `ty` load, element `element` past the byte `offset` from `base_addr`, aligned to `align`.
///
/// `load_from_device_buffer`.
fn load_from_device_buffer<'cx>(
    air: &AirBuilder<'_, 'cx>,
    ty: Ty,
    base_addr: Val<'cx>,
    offset: Val<'cx>,
    element: u32,
    align: u32,
) -> Val<'cx> {
    let b = air.builder();
    let tcx = b.cx().tcx();
    let bytes = b.gep(air.types().byte, base_addr, &[offset]);
    let typed = b.bitcast(bytes, tcx.ptr(ty, DEVICE));
    b.aligned_load(ty, b.const_gep1_32(ty, typed, element), align)
}

/// `v` widened to four lanes of `vec4`: the missing ones `zero`, `zero`, `one` (x, y, z, w).
fn widen<'cx>(
    air: &AirBuilder<'_, 'cx>,
    v: Val<'cx>,
    vec4: Ty,
    zero: Val<'cx>,
    one: Val<'cx>,
) -> Val<'cx> {
    let b = air.builder();
    let (mut out, first) = match b.cx().tcx().count(v.ty()) {
        None => (b.insert_element_u64(b.cx().poison(vec4), v, 0), 1),
        Some(2) => (b.shuffle_vector1(v, &[0, 1, -1, -1]), 2),
        Some(3) => (b.shuffle_vector1(v, &[0, 1, 2, -1]), 3),
        Some(_) => return v,
    };
    for i in first..4 {
        out = b.insert_element_u64(out, if i == 3 { one } else { zero }, i);
    }
    out
}
