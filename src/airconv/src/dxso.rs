//! D3D9 lowering: a SM1-3 program to AIR.
//!
//! `airconv_ir::dxso::lower` turns the program into DXBC blocks and a signature; this takes
//! them through the DXBC lowering (`dxbc::converter`, `dxbc::blocks`, the signature replay)
//! with D3D9's bindings, as `dxbc::vs` does for a DXBC vertex shader: Metal 3.1 (AIR 2.6),
//! common flags 0 (ruling C3-6), stage-in attributes and no vertex pulling. Pixel shaders are
//! not lowered yet.

use core::ffi::CStr;

use airconv_ir::{
    binding::{FunctionOutput, count_u32},
    dxso::{
        DxsoProgram, ShaderType,
        lower::{self, LowerError},
        parse,
        spec::{StageSpec, VsSpec},
    },
    shader_info::ShaderFlags,
    tokens::program_type,
};

use crate::{
    abi::{ArgChain, SM50_SHADER_METAL_310},
    air::{
        builder::AirBuilder,
        metadata::{add_entry, set_metal_version},
        signature::{Handles, create_function},
        ty::AirTypes,
    },
    api::Error,
    dxbc::{
        blocks,
        converter::{Converter, invalid, req},
        replay::{self, Options, zero_return},
        resources::{Resources, setup_io_registers, setup_temp_register},
    },
    entry::finish,
    llvm::{Builder, Context, Module},
    module,
};

mod bindings;

/// Lowers `program` (a handle's), specialized by `spec`, as the function `name` into `m`.
///
/// # Errors
///
/// What the lowering refuses (`dxso: unsupported …`), a stage the specialization does not
/// match, and a pixel shader (`dxso: not lowered`).
pub fn convert(
    program: &DxsoProgram,
    spec: &StageSpec,
    _args: &ArgChain<'_>,
    name: &CStr,
    m: &Module<'_>,
) -> Result<(), Error> {
    lower_into(program, spec, name, m)
}

/// The metallib of the D3D9 program `tokens`, specialized by `spec`, its function named `name`.
///
/// The path `SM50Compile` takes for a D3D9 shader, for callers holding tokens rather than a
/// handle.
///
/// # Errors
///
/// A program the parser refuses (`dxso: <error>`), and [`convert`]'s.
pub fn translate(tokens: &[u32], spec: &StageSpec, name: &CStr) -> Result<Vec<u8>, Error> {
    let program = parse(tokens).map_err(|e| Error::new(format!("dxso: {e:?}").as_bytes()))?;
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    lower_into(&program, spec, name, &m)?;
    finish(&m, ShaderFlags::default(), None)
}

fn lower_into(
    program: &DxsoProgram,
    spec: &StageSpec,
    name: &CStr,
    m: &Module<'_>,
) -> Result<(), Error> {
    match (spec, program.shader_type) {
        (StageSpec::Vs(vs), ShaderType::Vertex) => vertex(program, *vs, name, m),
        (StageSpec::Ps(_), ShaderType::Pixel) => Err(Error::new(b"dxso: not lowered")),
        _ => Err(Error::new(b"dxso: stage does not match the program")),
    }
}

/// The lowering's refusal as an error.
fn refusal(e: &LowerError) -> Error {
    match e {
        LowerError::Unsupported { opcode, what } => {
            Error::new(format!("dxso: unsupported {what} (opcode {opcode})").as_bytes())
        }
    }
}

/// The vertex function: `dxbc::vs::convert` without vertex pulling or stream output.
fn vertex(program: &DxsoProgram, spec: VsSpec, name: &CStr, m: &Module<'_>) -> Result<(), Error> {
    let lowered = lower::vertex(program, &spec).map_err(|e| refusal(&e))?;
    let cx = m.cx();
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let handles = Handles::default();
    let mut desc = lowered.io.func_signature.clone();
    let replay = replay::record(
        &lowered.io.signature_handlers,
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    )?;
    let clip = &lowered.io.clip_distance_scalars;
    let clip_out = (!clip.is_empty())
        .then(|| desc.define_output(FunctionOutput::ClipDistance { count: clip.len() }));
    let binding = bindings::vertex(&lower::vs_bindings(program, &spec), &mut desc);

    let (function, entry) = create_function(m, &desc, name.to_bytes(), false, &handles)?;
    let no_block = || invalid("a block of no function");
    let entry_bb = function.append_block(cx, c"entry").ok_or_else(no_block)?;
    let epilogue_bb = function
        .append_block(cx, c"epilogue")
        .ok_or_else(no_block)?;
    let b = Builder::new(cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, m, &types, 0);
    set_metal_version(m, SM50_SHADER_METAL_310);

    let mut res = Resources::default();
    setup_io_registers(
        &mut res,
        &types,
        &b,
        lowered.io.max_input_register,
        lowered.io.max_output_register,
    );
    setup_temp_register(&lowered.cfg.info, &mut res, &types, &b);
    replay.prologue(&air, function, &mut res)?;
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: &binding,
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: program_type::VERTEX,
        metal_version: SM50_SHADER_METAL_310,
        geometry: None,
    };
    let real_entry = blocks::convert(&conv, &lowered.cfg.blocks, epilogue_bb)?;
    b.br(real_entry);

    b.position_at_end(epilogue_bb);
    let value = replay
        .epilogue(&air, &res, zero_return(cx, function), u32::MAX)?
        .ok_or_else(|| invalid("a vertex function that returns nothing"))?;
    let value = match clip_out {
        Some(out) => {
            let output = req(res.output.ptr_float4, "no output register file")?;
            let file_ty = tcx
                .pointee(output.ty())
                .ok_or_else(|| invalid("a register array is not a pointer"))?;
            let mut distances = cx.null(tcx.array(types.float, count_u32(clip.len())));
            for (i, scalar) in (0_u32..).zip(clip) {
                let at = [
                    air.get_int(0),
                    air.get_int(scalar.reg.into()),
                    air.get_int(scalar.component.into()),
                ];
                let src = b.gep(file_ty, output, &at);
                distances = b.insert_value(distances, b.load(types.float, src), i);
            }
            b.insert_value(value, distances, out)
        }
        None => value,
    };
    b.ret(value);
    add_entry(m, c"air.vertex", entry);
    Ok(())
}

#[cfg(test)]
mod tests;
