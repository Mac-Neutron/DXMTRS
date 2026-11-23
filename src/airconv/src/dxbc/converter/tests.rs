use airconv_ir::ir::{
    NO_PHASE, OperandDataType, OperandIndex, SWIZZLE_IDENTITY, SrcOperand, SrcOperandCommon,
};

use super::*;
use crate::{
    abi::SM50_SHADER_METAL_320,
    air::ty::AirTypes,
    dxbc::resources::{RegisterFile, Resources},
    llvm::{Builder, Context},
    module,
};

struct NoBindings;

impl<'cx> BindingMap<'cx> for NoBindings {
    fn constant_buffer(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<ConstantBufferDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn sampler(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<SamplerDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn srv_texture(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn uav_texture(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<TextureDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn srv_buffer(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn uav_buffer(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<BufferDescriptor<'cx>>, Error> {
        Ok(None)
    }

    fn uav_counter(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<CounterDescriptor<'cx>>, Error> {
        Ok(None)
    }
}

fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

#[test]
fn dynamic_control_point_input_and_output_lower_regid() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"cp_index.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let fn_ty = tcx.function(tcx.void(), &[], false);
    let function = ok(m.add_function(b"main", fn_ty));
    let entry = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(entry);
    let air = AirBuilder::new(&b, &m, &types, 0);

    let cp_array_ty = tcx.array(tcx.array(types.int4, 4), 3);
    let in_alloca = b.alloca(cp_array_ty, c"in");
    let out_alloca = b.alloca(cp_array_ty, c"out");
    let tmp_alloca = b.alloca(tcx.array(types.int4, 2), c"tmp");
    let res = Resources {
        input: RegisterFile {
            ptr_int4: Some(in_alloca),
            ptr_float4: None,
        },
        output: RegisterFile {
            ptr_int4: Some(out_alloca),
            ptr_float4: None,
        },
        temp: RegisterFile {
            ptr_int4: Some(tmp_alloca),
            ptr_float4: None,
        },
        ..Resources::default()
    };

    let bindings = NoBindings;
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: &bindings,
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: program_type::HULL,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };

    let common = SrcOperandCommon {
        swizzle: SWIZZLE_IDENTITY,
        abs: false,
        neg: false,
        read_type: OperandDataType::Integer,
    };
    let dynamic_reg = OperandIndex::Temp {
        regid: 0,
        phase: NO_PHASE,
        component: 1,
        offset: 2,
    };
    let icp = ok(conv.load_operand(
        &SrcOperand::InputIcp {
            common: common.clone(),
            cpid: OperandIndex::Immediate(1),
            regid: dynamic_reg.clone(),
        },
        MASK_X,
    ));
    let ocp = ok(conv.load_operand(
        &SrcOperand::InputOcp {
            common,
            cpid: OperandIndex::Immediate(2),
            regid: dynamic_reg,
        },
        MASK_X,
    ));
    let _ = b.add(icp, ocp);
    b.ret_void();

    let text = String::from_utf8(m.print()).unwrap();
    assert!(text.contains("add i32 "), "{text}");
    assert!(
        text.contains("getelementptr [3 x [4 x <4 x i32>]], ptr %in, i32 0, i32 1, i32 "),
        "{text}"
    );
    assert!(
        text.contains("getelementptr [3 x [4 x <4 x i32>]], ptr %out, i32 0, i32 2, i32 "),
        "{text}"
    );
}
