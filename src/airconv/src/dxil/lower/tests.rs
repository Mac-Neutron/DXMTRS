use airconv_ir::shader_info::{Declarations, ShaderInfo};

use super::*;
use crate::{
    abi::SM50_SHADER_METAL_320,
    air::ty::AirTypes,
    dxbc::{
        converter::{
            BindingMap, BufferDescriptor, ConstantBufferDescriptor, CounterDescriptor,
            SamplerDescriptor, TextureDescriptor,
        },
        resources::Resources,
    },
    dxil::{initialize::DxilShader, signature::Io, types::EntryInfo},
    llvm::{Builder, Context, Func, Ty},
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

fn declare<'cx>(m: &crate::llvm::Module<'cx>, name: &[u8], ret: Ty, params: &[Ty]) -> Func<'cx> {
    let fn_ty = m.cx().tcx().function(ret, params, false);
    ok(m.get_or_insert_function(name, fn_ty, &[]))
}

#[test]
fn i64_wave_ops_split_or_error_cleanly() {
    for stage in [shader_kind::COMPUTE, shader_kind::PIXEL] {
        let cx = Context::new();
        let m = module::initialize(&cx, c"wave64.air");
        let tcx = cx.tcx();
        let types = AirTypes::new(tcx);
        let bool_ty = tcx.int(1);
        let byte_ty = tcx.int(8);
        let int_ty = tcx.int(32);
        let long_ty = tcx.int(64);
        let float_ty = tcx.float();

        let fn_ty = tcx.function(tcx.void(), &[long_ty, int_ty, float_ty], false);
        let function = ok(m.add_function(b"main", fn_ty));
        let entry_bb = function.append_block(&cx, c"entry").unwrap();
        let b = Builder::new(&cx);
        b.position_at_end(entry_bb);
        let air = AirBuilder::new(&b, &m, &types, 0);

        let val64 = function.param(tcx, 0).unwrap();
        let lane32 = function.param(tcx, 1).unwrap();
        let valf = function.param(tcx, 2).unwrap();

        let read_first_fn = declare(
            &m,
            b"dx.op.waveReadLaneFirst.i64",
            long_ty,
            &[int_ty, long_ty],
        );
        let read_first = b.call(
            read_first_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_READ_LANE_FIRST), false),
                val64,
            ],
        );

        let read_at_fn = declare(
            &m,
            b"dx.op.waveReadLaneAt.i64",
            long_ty,
            &[int_ty, long_ty, int_ty],
        );
        let read_at = b.call(
            read_at_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_READ_LANE_AT), false),
                val64,
                lane32,
            ],
        );

        let active_bit_fn = declare(
            &m,
            b"dx.op.waveActiveBit.i64",
            long_ty,
            &[int_ty, long_ty, byte_ty],
        );
        let active_and = b.call(
            active_bit_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_ACTIVE_BIT), false),
                val64,
                cx.const_int(byte_ty, 0, false),
            ],
        );
        let active_xor = b.call(
            active_bit_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_ACTIVE_BIT), false),
                val64,
                cx.const_int(byte_ty, 2, false),
            ],
        );

        let int_all_eq_fn = declare(
            &m,
            b"dx.op.waveActiveAllEqual.i64",
            bool_ty,
            &[int_ty, long_ty],
        );
        let int_all_eq = b.call(
            int_all_eq_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_ACTIVE_ALL_EQUAL), false),
                val64,
            ],
        );

        let fp_all_eq_fn = declare(
            &m,
            b"dx.op.waveActiveAllEqual.f32",
            bool_ty,
            &[int_ty, float_ty],
        );
        let fp_all_eq = b.call(
            fp_all_eq_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_ACTIVE_ALL_EQUAL), false),
                valf,
            ],
        );

        let active_op_fn = declare(
            &m,
            b"dx.op.waveActiveOp.i64",
            long_ty,
            &[int_ty, long_ty, byte_ty, byte_ty],
        );
        let active_op = b.call(
            active_op_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_ACTIVE_OP), false),
                val64,
                cx.const_int(byte_ty, 0, false),
                cx.const_int(byte_ty, 0, false),
            ],
        );

        let prefix_op_fn = declare(
            &m,
            b"dx.op.wavePrefixOp.i64",
            long_ty,
            &[int_ty, long_ty, byte_ty, byte_ty],
        );
        let prefix_op = b.call(
            prefix_op_fn,
            &[
                cx.const_int(int_ty, u64::from(op::WAVE_PREFIX_OP), false),
                val64,
                cx.const_int(byte_ty, 0, false),
                cx.const_int(byte_ty, 0, false),
            ],
        );
        b.ret_void();

        let entry = EntryInfo {
            kind: stage,
            ..EntryInfo::default()
        };
        let shader = DxilShader {
            program_type: stage,
            info: ShaderInfo::default(),
            decls: Declarations::default(),
            io: Io::default(),
            bitcode: Vec::new(),
            entry,
            lane_index_arg: 1,
            lane_count_arg: 2,
            include_helper_lanes: false,
            quads_2d: false,
            gs_locks: 0,
            capture_hash: 0,
            writes_uav: false,
        };
        let bindings = NoBindings;
        let res = Resources::default();
        let conv = Converter {
            air: &air,
            ir: &b,
            function,
            binding: &bindings,
            res: &res,
            types: &types,
            pso_sample_mask: u32::MAX,
            shader_type: stage,
            metal_version: SM50_SHADER_METAL_320,
            geometry: None,
        };
        let lowering = Lowering {
            conv: &conv,
            dxil: &shader,
            entry: &shader.entry,
            vertex_outputs: None,
        };

        for call in [
            read_first, read_at, active_and, active_xor, int_all_eq, fp_all_eq,
        ] {
            // SAFETY: each call is live in `function` and is not reused after `lower`.
            ok(unsafe { lowering.lower(call) });
        }
        for bad in [active_op, prefix_op] {
            // SAFETY: each call is live in `function` and is not reused after `lower`.
            let Err(err) = (unsafe { lowering.lower(bad) }) else {
                panic!("64-bit integer wave op succeeded");
            };
            assert_eq!(err.text(), b"dxil: 64-bit integer wave op not supported");
        }

        let text = String::from_utf8(m.print()).unwrap();
        assert!(!text.contains(".u.i64"), "{text}");
        assert!(!text.contains(".s.i64"), "{text}");
        assert!(!text.contains("air.simd_shuffle.u.i64"), "{text}");
        assert!(!text.contains("air.simd_broadcast_first.u.i64"), "{text}");
        assert!(!text.contains("air.simd_and.u.i64"), "{text}");
        assert!(!text.contains("air.simd_xor.u.i64"), "{text}");
        assert!(text.contains("air.simd_shuffle.u.i32"), "{text}");
        assert!(text.contains("air.simd_and.u.i32"), "{text}");
        assert!(text.contains("air.simd_xor.u.i32"), "{text}");
        assert!(text.contains("fcmp oeq float"), "{text}");
        if stage == shader_kind::PIXEL {
            assert!(text.contains("air.simd_is_helper_thread"), "{text}");
            assert!(text.contains("air.simd_ballot.i64"), "{text}");
        } else {
            assert!(text.contains("air.simd_broadcast_first.u.i32"), "{text}");
        }
    }
}
