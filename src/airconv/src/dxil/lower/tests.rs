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

#[test]
fn eval_ops_lower_for_modes_widths_sub_rows_and_half() {
    use std::collections::BTreeSet;

    use crate::{
        air::signature::{Handles, create_function},
        dxbc::{replay::Options, resources::setup_io_registers},
        dxil::types::interpolation,
    };

    let cx = Context::new();
    let m = module::initialize(&cx, c"eval.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let i8_ty = tcx.int(8);
    let i32_ty = tcx.int(32);
    let f16_ty = tcx.half();
    let f32_ty = tcx.float();

    // Element 0: scalar width-1, perspective centroid (no extractelement).
    let el0 = SignatureElement {
        id: 0,
        name: b"TEXCOORD".to_vec(),
        semantic_indices: vec![0],
        ty: component_type::F32,
        kind: semantic_kind::ARBITRARY,
        interpolation: interpolation::LINEAR_CENTROID,
        rows: 1,
        cols: 1,
        start_row: 0,
        start_col: 0,
    };
    // Element 1: sub-row start_col=2, cols=2, noperspective (extracts lane 0, not 2; also half).
    let el1 = SignatureElement {
        id: 1,
        name: b"TEXCOORD".to_vec(),
        semantic_indices: vec![1],
        ty: component_type::F32,
        kind: semantic_kind::ARBITRARY,
        interpolation: interpolation::LINEAR_NOPERSPECTIVE,
        rows: 1,
        cols: 2,
        start_row: 1,
        start_col: 2,
    };
    // Element 2: vec3 perspective sample / snapped.
    let el2 = SignatureElement {
        id: 2,
        name: b"COLOR".to_vec(),
        semantic_indices: vec![0],
        ty: component_type::F32,
        kind: semantic_kind::ARBITRARY,
        interpolation: interpolation::LINEAR_SAMPLE,
        rows: 1,
        cols: 3,
        start_row: 2,
        start_col: 0,
    };

    let entry = EntryInfo {
        kind: shader_kind::PIXEL,
        inputs: vec![el0, el1, el2],
        ..EntryInfo::default()
    };
    let pull = BTreeSet::from([0, 1, 2]);
    let mut io = Io::default();
    ok(io.add_signature_handlers(&entry, &BTreeSet::new(), &pull, false, false));

    let mut desc = io.func_signature.clone();
    let replay = ok(io.record(
        &mut desc,
        &Options {
            ia_layout: None,
            pixel: None,
            pull_mode_reg_mask: 0,
            skip_vertex_output: false,
        },
    ));
    let (function, _) = ok(create_function(
        &m,
        &desc,
        b"ps_main",
        false,
        &Handles::default(),
    ));
    let entry_bb = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, &m, &types, 0);

    let mut res = Resources::default();
    setup_io_registers(&mut res, &types, &b, io.max_input_register, 1);
    ok(replay.prologue(&air, function, &mut res));

    let load_f32_fn = declare(
        &m,
        b"dx.op.loadInput.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty],
    );
    let centroid_f32_fn = declare(
        &m,
        b"dx.op.evalCentroid.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty],
    );
    let centroid_f16_fn = declare(
        &m,
        b"dx.op.evalCentroid.f16",
        f16_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty],
    );
    let sample_f32_fn = declare(
        &m,
        b"dx.op.evalSampleIndex.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty],
    );
    let snapped_f32_fn = declare(
        &m,
        b"dx.op.evalSnapped.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty, i32_ty],
    );

    let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
    let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);

    // Mixed loadInput + evalCentroid on element 0 (width-1 scalar).
    let load0 = b.call(
        load_f32_fn,
        &[c32(op::LOAD_INPUT), c32(0), c32(0), c8(0), cx.undef(i32_ty)],
    );
    let eval0 = b.call(
        centroid_f32_fn,
        &[c32(op::EVAL_CENTROID), c32(0), c32(0), c8(0)],
    );
    // Sub-row element 1 (start_col=2, cols=2, noperspective), col=0, half result.
    let eval1 = b.call(
        centroid_f16_fn,
        &[c32(op::EVAL_CENTROID), c32(1), c32(0), c8(0)],
    );
    // SampleIndex and Snapped on element 2 (vec3 perspective), col=2.
    let eval2_sample = b.call(
        sample_f32_fn,
        &[c32(op::EVAL_SAMPLE_INDEX), c32(2), c32(0), c8(2), c32(3)],
    );
    let off_fn = declare(&m, b"off", i32_ty, &[]);
    let off_x = b.call(off_fn, &[]);
    let off_y = b.call(off_fn, &[]);
    let eval2_snapped = b.call(
        snapped_f32_fn,
        &[c32(op::EVAL_SNAPPED), c32(2), c32(0), c8(1), off_x, off_y],
    );

    // Keep values live so replace_all_uses_with leaves visible IR.
    let sink_fn = declare(&m, b"sink", tcx.void(), &[f32_ty, f16_ty]);
    let sum = b.fadd(b.fadd(load0, eval0), b.fadd(eval2_sample, eval2_snapped));
    let _ = b.call(sink_fn, &[sum, eval1]);
    b.ret_void();

    let shader = DxilShader {
        program_type: shader_kind::PIXEL,
        info: ShaderInfo::default(),
        decls: Declarations::default(),
        io,
        bitcode: Vec::new(),
        entry,
        lane_index_arg: u32::MAX,
        lane_count_arg: u32::MAX,
        include_helper_lanes: false,
        quads_2d: false,
        gs_locks: 0,
        capture_hash: 0,
        writes_uav: false,
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
        shader_type: shader_kind::PIXEL,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };
    let lowering = Lowering {
        conv: &conv,
        dxil: &shader,
        entry: &shader.entry,
        vertex_outputs: None,
    };

    for call in [load0, eval0, eval1, eval2_sample, eval2_snapped] {
        // SAFETY: each call is live in `function` and is not reused after `lower`.
        ok(unsafe { lowering.lower(call) });
    }

    let text = String::from_utf8(m.print()).unwrap();
    // Width-1 scalar: no extractelement on float.
    assert!(
        text.contains("call float @air.interpolate_centroid_perspective.f32("),
        "{text}"
    );
    assert!(!text.contains("extractelement float"), "{text}");
    // Sub-row start_col=2: extracts lane 0 (not lane 2) from <2 x float>, then fptrunc to half.
    assert!(
        text.contains("call <2 x float> @air.interpolate_centroid_no_perspective.v2f32("),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("extractelement <2 x float>") && l.contains(", i64 0")),
        "{text}"
    );
    assert!(
        !text
            .lines()
            .any(|l| l.contains("extractelement <2 x float>") && l.contains(", i64 2")),
        "{text}"
    );
    assert!(text.contains("fptrunc float"), "{text}");
    // SampleIndex and Snapped on vec3.
    assert!(
        text.contains("call <3 x float> @air.interpolate_sample_perspective.v3f32("),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("extractelement <3 x float>") && l.contains(", i64 2")),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|l| l.contains("extractelement <3 x float>") && l.contains(", i64 1")),
        "{text}"
    );
    assert!(
        text.contains("add <2 x i32>")
            && (text.contains("<i32 8, i32 8>") || text.contains("splat (i32 8)")),
        "{text}"
    );
    assert!(
        text.contains("and <2 x i32>")
            && (text.contains("<i32 15, i32 15>") || text.contains("splat (i32 15)")),
        "{text}"
    );
    assert!(
        text.contains("call <2 x float> @air.convert.f.v2f32.s.v2i32("),
        "{text}"
    );
    assert!(
        text.contains("fmul <2 x float>") && text.contains("6.250000e-02"),
        "{text}"
    );
    assert!(
        text.contains("call <3 x float> @air.interpolate_offset_perspective.v3f32("),
        "{text}"
    );
}

#[test]
fn eval_ops_reject_invalid_inputs() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"eval_err.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let i8_ty = tcx.int(8);
    let i32_ty = tcx.int(32);
    let f32_ty = tcx.float();
    let itp_ty = tcx.ptr(tcx.named_struct(c"struct._interpolant_t"), 1);

    let function = ok(m.add_function(b"main", tcx.function(tcx.void(), &[i32_ty, itp_ty], false)));
    let bb = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(bb);
    let air = AirBuilder::new(&b, &m, &types, 0);

    let dyn_arg = function.param(tcx, 0).unwrap();
    let itp_handle = function.param(tcx, 1).unwrap();

    let centroid_float = declare(
        &m,
        b"dx.op.evalCentroid.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty],
    );
    let centroid_int = declare(
        &m,
        b"dx.op.evalCentroid.i32",
        i32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty],
    );

    let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
    let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);

    let non_const = b.call(
        centroid_float,
        &[c32(op::EVAL_CENTROID), dyn_arg, c32(0), c8(0)],
    );
    let undeclared = b.call(
        centroid_float,
        &[c32(op::EVAL_CENTROID), c32(99), c32(0), c8(0)],
    );
    let unmapped = b.call(
        centroid_float,
        &[c32(op::EVAL_CENTROID), c32(1), c32(0), c8(0)],
    );
    let bad_col = b.call(
        centroid_float,
        &[c32(op::EVAL_CENTROID), c32(0), c32(0), c8(2)],
    );
    let bad_type = b.call(
        centroid_int,
        &[c32(op::EVAL_CENTROID), c32(0), c32(0), c8(0)],
    );
    let missing_itp = b.call(
        centroid_float,
        &[c32(op::EVAL_CENTROID), c32(2), c32(0), c8(0)],
    );
    b.ret_void();

    let entry = EntryInfo {
        kind: shader_kind::PIXEL,
        inputs: vec![
            SignatureElement {
                id: 0,
                name: b"TEXCOORD".to_vec(),
                semantic_indices: vec![0],
                ty: component_type::F32,
                kind: semantic_kind::ARBITRARY,
                interpolation: 2,
                rows: 1,
                cols: 2,
                start_row: 0,
                start_col: 0,
            },
            SignatureElement {
                id: 1,
                name: b"SV_IsFrontFace".to_vec(),
                semantic_indices: vec![0],
                ty: component_type::I1,
                kind: semantic_kind::IS_FRONT_FACE,
                interpolation: 1,
                rows: 1,
                cols: 1,
                start_row: -1,
                start_col: -1,
            },
            SignatureElement {
                id: 2,
                name: b"TEXCOORD".to_vec(),
                semantic_indices: vec![1],
                ty: component_type::F32,
                kind: semantic_kind::ARBITRARY,
                interpolation: 1,
                rows: 1,
                cols: 2,
                start_row: 1,
                start_col: 0,
            },
        ],
        ..EntryInfo::default()
    };

    let mut res = Resources::default();
    res.dxil_interpolant_map.insert(
        (0, 0),
        crate::dxbc::resources::DxilInterpolant {
            handle: itp_handle,
            perspective: true,
            ty: tcx.vector(f32_ty, 2),
            cols: 2,
        },
    );

    let shader = DxilShader {
        program_type: shader_kind::PIXEL,
        info: ShaderInfo::default(),
        decls: Declarations::default(),
        io: Io::default(),
        bitcode: Vec::new(),
        entry,
        lane_index_arg: u32::MAX,
        lane_count_arg: u32::MAX,
        include_helper_lanes: false,
        quads_2d: false,
        gs_locks: 0,
        capture_hash: 0,
        writes_uav: false,
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
        shader_type: shader_kind::PIXEL,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };
    let lowering = Lowering {
        conv: &conv,
        dxil: &shader,
        entry: &shader.entry,
        vertex_outputs: None,
    };

    let cases: &[(Val<'_>, &[u8])] = &[
        (non_const, b"DXIL: non-constant eval index"),
        (undeclared, b"DXIL: eval names an undeclared input"),
        (unmapped, b"DXIL: eval signature element is not mapped"),
        (bad_col, b"DXIL: eval component out of range"),
        (bad_type, b"DXIL: unsupported eval type"),
        (missing_itp, b"DXIL: no eval interpolant"),
    ];
    for &(call, expected) in cases {
        // SAFETY: each call is live in `function` and is not reused after `lower`.
        let Err(err) = (unsafe { lowering.lower(call) }) else {
            panic!("expected error {}", String::from_utf8_lossy(expected));
        };
        assert_eq!(err.text(), expected);
    }
}

#[test]
fn hull_and_domain_opcodes_lower() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"tess_ops.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let i8_ty = tcx.int(8);
    let i32_ty = tcx.int(32);
    let f32_ty = tcx.float();
    let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
    let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);

    let domain_vec_ty = tcx.vector(f32_ty, 3);
    let function = ok(m.add_function(b"main", tcx.function(tcx.void(), &[domain_vec_ty], false)));
    let bb = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(bb);
    let air = AirBuilder::new(&b, &m, &types, 0);

    let load_in = declare(
        &m,
        b"dx.op.loadInput.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty],
    );
    let store_out = declare(
        &m,
        b"dx.op.storeOutput.f32",
        tcx.void(),
        &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty],
    );
    let load_ocp = declare(
        &m,
        b"dx.op.loadOutputControlPoint.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty],
    );
    let load_pc = declare(
        &m,
        b"dx.op.loadPatchConstant.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty],
    );
    let store_pc = declare(
        &m,
        b"dx.op.storePatchConstant.f32",
        tcx.void(),
        &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty],
    );
    let domain_loc = declare(&m, b"dx.op.domainLocation.f32", f32_ty, &[i32_ty, i8_ty]);
    let ocp_id = declare(&m, b"dx.op.outputControlPointID.i32", i32_ty, &[i32_ty]);
    let prim_id = declare(&m, b"dx.op.primitiveID.i32", i32_ty, &[i32_ty]);

    // 4D input/output files ([3 x [2 x <4 x i32>]]) and 3D patch constant file ([2 x <4 x i32>]).
    let cp_patch_ty = tcx.array(tcx.array(types.int4, 2), 3);
    let pc_ty = tcx.array(types.int4, 2);

    let mut hs_res = Resources::default();
    hs_res.input.ptr_int4 = Some(b.alloca(cp_patch_ty, c"hs_in"));
    hs_res.output.ptr_int4 = Some(b.alloca(cp_patch_ty, c"hs_out"));
    hs_res.patch_constant_output.ptr_int4 = Some(b.alloca(pc_ty, c"hs_pc"));
    hs_res.thread_id_in_patch = Some(c32(2));
    hs_res.patch_id = Some(c32(9));

    let hs_entry = EntryInfo {
        kind: shader_kind::HULL,
        inputs: vec![SignatureElement {
            id: 0,
            name: b"TEXCOORD".to_vec(),
            semantic_indices: vec![0],
            ty: component_type::F32,
            kind: semantic_kind::ARBITRARY,
            interpolation: 1,
            rows: 1,
            cols: 4,
            start_row: 0,
            start_col: 0,
        }],
        outputs: vec![SignatureElement {
            id: 0,
            name: b"COLOR".to_vec(),
            semantic_indices: vec![0],
            ty: component_type::F32,
            kind: semantic_kind::ARBITRARY,
            interpolation: 1,
            rows: 1,
            cols: 4,
            start_row: 1,
            start_col: 0,
        }],
        patch_constants: vec![SignatureElement {
            id: 0,
            name: b"SV_TessFactor".to_vec(),
            semantic_indices: vec![0],
            ty: component_type::F32,
            kind: semantic_kind::TESS_FACTOR,
            interpolation: 0,
            rows: 1,
            cols: 1,
            start_row: 0,
            start_col: 1,
        }],
        ..EntryInfo::default()
    };
    let vs_upstream = EntryInfo {
        kind: shader_kind::VERTEX,
        outputs: vec![SignatureElement {
            id: 0,
            name: b"TEXCOORD".to_vec(),
            semantic_indices: vec![0],
            ty: component_type::F32,
            kind: semantic_kind::ARBITRARY,
            interpolation: 1,
            rows: 1,
            cols: 4,
            start_row: 1,
            start_col: 2,
        }],
        ..EntryInfo::default()
    };

    let c_in = b.call(
        load_in,
        &[c32(op::LOAD_INPUT), c32(0), c32(0), c8(1), c32(1)],
    );
    let c_store = b.call(
        store_out,
        &[
            c32(op::STORE_OUTPUT),
            c32(0),
            c32(0),
            c8(2),
            cx.const_real(f32_ty, 1.0),
        ],
    );
    let c_ocp = b.call(
        load_ocp,
        &[
            c32(op::LOAD_OUTPUT_CONTROL_POINT),
            c32(0),
            c32(0),
            c8(2),
            c32(1),
        ],
    );
    let call_store_pc = b.call(
        store_pc,
        &[
            c32(op::STORE_PATCH_CONSTANT),
            c32(0),
            c32(0),
            c8(0),
            cx.const_real(f32_ty, 4.0),
        ],
    );
    let call_load_pc = b.call(
        load_pc,
        &[c32(op::LOAD_PATCH_CONSTANT), c32(0), c32(0), c8(0)],
    );
    let c_ocpid = b.call(ocp_id, &[c32(op::OUTPUT_CONTROL_POINT_ID)]);
    let c_pid = b.call(prim_id, &[c32(op::PRIMITIVE_ID)]);

    let bindings = NoBindings;
    let hull_shader = DxilShader {
        program_type: shader_kind::HULL,
        info: ShaderInfo::default(),
        decls: Declarations::default(),
        io: Io::default(),
        bitcode: Vec::new(),
        entry: hs_entry,
        lane_index_arg: u32::MAX,
        lane_count_arg: u32::MAX,
        include_helper_lanes: false,
        quads_2d: false,
        gs_locks: 0,
        capture_hash: 0,
        writes_uav: false,
    };
    let hull_conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: &bindings,
        res: &hs_res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: shader_kind::HULL,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };
    let hull_lower = Lowering {
        conv: &hull_conv,
        dxil: &hull_shader,
        entry: &hull_shader.entry,
        vertex_outputs: Some(&vs_upstream),
    };
    for call in [
        c_in,
        c_store,
        c_ocp,
        call_store_pc,
        call_load_pc,
        c_ocpid,
        c_pid,
    ] {
        // SAFETY: each call is live in `function` and is not reused after `lower`.
        ok(unsafe { hull_lower.lower(call) });
    }

    // Reposition past the erased calls before appending the domain ops.
    b.position_at_end(bb);

    // Domain shader lowering: domainLocation (0..3) and invalid col = 3 error.
    let ds_res = Resources {
        domain: Some(function.param(tcx, 0).unwrap()),
        patch_id: Some(c32(11)),
        ..Resources::default()
    };
    let domain_shader = DxilShader {
        program_type: shader_kind::DOMAIN,
        info: ShaderInfo::default(),
        decls: Declarations::default(),
        io: Io::default(),
        bitcode: Vec::new(),
        entry: EntryInfo {
            kind: shader_kind::DOMAIN,
            ..EntryInfo::default()
        },
        lane_index_arg: u32::MAX,
        lane_count_arg: u32::MAX,
        include_helper_lanes: false,
        quads_2d: false,
        gs_locks: 0,
        capture_hash: 0,
        writes_uav: false,
    };
    let domain_conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: &bindings,
        res: &ds_res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: shader_kind::DOMAIN,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };
    let domain_lower = Lowering {
        conv: &domain_conv,
        dxil: &domain_shader,
        entry: &domain_shader.entry,
        vertex_outputs: None,
    };
    let c_dom_v = b.call(domain_loc, &[c32(op::DOMAIN_LOCATION), c8(1)]);
    let c_dom_bad = b.call(domain_loc, &[c32(op::DOMAIN_LOCATION), c8(3)]);
    let c_ds_pid = b.call(prim_id, &[c32(op::PRIMITIVE_ID)]);
    b.ret_void();

    // SAFETY: live calls in `function`, each lowered once.
    ok(unsafe { domain_lower.lower(c_dom_v) });
    // SAFETY: live call in `function`, lowered once.
    ok(unsafe { domain_lower.lower(c_ds_pid) });
    // SAFETY: live call in `function`, expected to error cleanly.
    let Err(dom_err) = (unsafe { domain_lower.lower(c_dom_bad) }) else {
        panic!("expected out-of-range domainLocation error");
    };
    assert_eq!(
        dom_err.text(),
        b"DXIL: domainLocation component out of range"
    );

    let text = String::from_utf8(m.print()).unwrap();
    // Relinked 4D loadInput picks up upstream row 1, col 2 + 1 = 3.
    assert!(
        text.contains(
            "getelementptr [3 x [2 x <4 x i32>]], ptr %hs_in, i32 0, i32 1, i32 1, i32 3"
        ),
        "{text}"
    );
    // 4D storeOutput / loadOutputControlPoint address hs_out.
    assert!(
        text.contains(
            "getelementptr [3 x [2 x <4 x i32>]], ptr %hs_out, i32 0, i32 2, i32 1, i32 2"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "getelementptr [3 x [2 x <4 x i32>]], ptr %hs_out, i32 0, i32 1, i32 1, i32 2"
        ),
        "{text}"
    );
    // 3D storePatchConstant / loadPatchConstant address hs_pc at col 1.
    assert!(
        text.contains("getelementptr [2 x <4 x i32>], ptr %hs_pc, i32 0, i32 0, i32 1"),
        "{text}"
    );
    assert!(text.contains("extractelement <3 x float>"), "{text}");
}

#[test]
fn clip_distance_relinks_by_semantic_index_and_domain_loc_validates() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"clip_relink.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let i8_ty = tcx.int(8);
    let i32_ty = tcx.int(32);
    let f16_ty = tcx.half();
    let f32_ty = tcx.float();
    let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
    let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);

    let function = ok(m.add_function(b"main", tcx.function(tcx.void(), &[i8_ty], false)));
    let bb = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(bb);
    let air = AirBuilder::new(&b, &m, &types, 0);

    let load_in = declare(
        &m,
        b"dx.op.loadInput.f32",
        f32_ty,
        &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty],
    );
    let dom_f16 = declare(&m, b"dx.op.domainLocation.f16", f16_ty, &[i32_ty, i8_ty]);
    let dom_f32 = declare(&m, b"dx.op.domainLocation.f32", f32_ty, &[i32_ty, i8_ty]);

    let cp_ty = tcx.array(tcx.array(types.int4, 6), 3);
    let mut res = Resources {
        domain: Some(cx.const_vector(&[
            cx.const_real(f32_ty, 0.2),
            cx.const_real(f32_ty, 0.3),
            cx.const_real(f32_ty, 0.5),
        ])),
        ..Resources::default()
    };
    res.input.ptr_int4 = Some(b.alloca(cp_ty, c"cp_in"));

    let clip_el = |id, idx, row| SignatureElement {
        id,
        name: b"SV_ClipDistance".to_vec(),
        semantic_indices: vec![idx],
        ty: component_type::F32,
        kind: semantic_kind::CLIP_DISTANCE,
        interpolation: 1,
        rows: 1,
        cols: 1,
        start_row: row,
        start_col: 0,
    };

    let upstream = EntryInfo {
        kind: shader_kind::VERTEX,
        outputs: vec![clip_el(0, 0, 2), clip_el(1, 1, 5)],
        ..EntryInfo::default()
    };
    let ds_shader = DxilShader {
        program_type: shader_kind::DOMAIN,
        info: ShaderInfo::default(),
        decls: Declarations::default(),
        io: Io::default(),
        bitcode: Vec::new(),
        entry: EntryInfo {
            kind: shader_kind::DOMAIN,
            inputs: vec![clip_el(0, 0, 0), clip_el(1, 1, 1)],
            ..EntryInfo::default()
        },
        lane_index_arg: u32::MAX,
        lane_count_arg: u32::MAX,
        include_helper_lanes: false,
        quads_2d: false,
        gs_locks: 0,
        capture_hash: 0,
        writes_uav: false,
    };

    let load_clip0 = b.call(
        load_in,
        &[c32(op::LOAD_INPUT), c32(0), c32(0), c8(0), c32(1)],
    );
    let load_clip1 = b.call(
        load_in,
        &[c32(op::LOAD_INPUT), c32(1), c32(0), c8(0), c32(1)],
    );
    let dyn_col = function.param(tcx, 0).unwrap();
    let bad_dyn_dom = b.call(dom_f32, &[c32(op::DOMAIN_LOCATION), dyn_col]);
    let bad_ty_dom = b.call(dom_f16, &[c32(op::DOMAIN_LOCATION), c8(0)]);
    b.ret_void();

    let bindings = NoBindings;
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: &bindings,
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: shader_kind::DOMAIN,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };
    let lower = Lowering {
        conv: &conv,
        dxil: &ds_shader,
        entry: &ds_shader.entry,
        vertex_outputs: Some(&upstream),
    };
    // SAFETY: live calls in `function`, lowered once.
    ok(unsafe { lower.lower(load_clip0) });
    // SAFETY: live call in `function`, lowered once.
    ok(unsafe { lower.lower(load_clip1) });

    // SAFETY: live call in `function`, expected to fail on non-constant col.
    let Err(dyn_err) = (unsafe { lower.lower(bad_dyn_dom) }) else {
        panic!("expected dynamic domainLocation error");
    };
    assert_eq!(
        dyn_err.text(),
        b"DXIL: domainLocation component out of range"
    );

    // SAFETY: live call in `function`, expected to fail on non-float type.
    let Err(ty_err) = (unsafe { lower.lower(bad_ty_dom) }) else {
        panic!("expected domainLocation type error");
    };
    assert_eq!(ty_err.text(), b"DXIL: unsupported domainLocation type");

    let text = String::from_utf8(m.print()).unwrap();
    assert!(
        text.contains(
            "getelementptr [3 x [6 x <4 x i32>]], ptr %cp_in, i32 0, i32 1, i32 2, i32 0"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "getelementptr [3 x [6 x <4 x i32>]], ptr %cp_in, i32 0, i32 1, i32 5, i32 0"
        ),
        "{text}"
    );
}
