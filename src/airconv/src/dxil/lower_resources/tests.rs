use airconv_ir::shader_info::{Declarations, ShaderInfo};

use super::*;
use crate::{
    abi::SM50_SHADER_METAL_320,
    air::{
        builder::{AirBuilder, SampleType, TextureKind},
        ty::AirTypes,
    },
    dxbc::{
        converter::{
            BindingMap, BufferDescriptor, ConstantBufferDescriptor, Converter, CounterDescriptor,
            SamplerDescriptor, TextureDescriptor,
        },
        resources::Resources,
    },
    dxil::{
        initialize::DxilShader,
        signature::Io,
        types::{EntryInfo, Resource, ResourceFlags, component_type},
    },
    llvm::{Builder, Context, Func, Ty},
    module,
};

struct TestBindings<'cx> {
    cbuf: Val<'cx>,
    tex: Val<'cx>,
    meta: Val<'cx>,
}

impl<'cx> BindingMap<'cx> for TestBindings<'cx> {
    fn constant_buffer(
        &self,
        _air: &AirBuilder<'_, 'cx>,
        _function: Func<'cx>,
        _range: u32,
        _index: Val<'cx>,
    ) -> Result<Option<ConstantBufferDescriptor<'cx>>, Error> {
        Ok(Some(ConstantBufferDescriptor { pointer: self.cbuf }))
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
        Ok(Some(TextureDescriptor {
            handle: self.tex,
            metadata: self.meta,
            global_coherent: false,
            kind: Kind::TextureBuffer,
            kind_logical: Kind::TextureBuffer,
            memory_access: MemoryAccess::Write,
            sample_type: SampleType::Float,
            buffer_address: None,
        }))
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

fn make_entry() -> EntryInfo {
    EntryInfo {
        kind: shader_kind::COMPUTE,
        resources: vec![
            Resource {
                id: 0,
                cls: resource_class::CBUFFER,
                kind: resource_kind::CBUFFER,
                space: 0,
                lower_bound: 0,
                size: 1,
                element_type: 0,
                stride: 0,
                cbuffer_size: 256,
                flags: ResourceFlags::NONE,
                heap_props: [0; 2],
            },
            Resource {
                id: 0,
                cls: resource_class::UAV,
                kind: resource_kind::TYPED_BUFFER,
                space: 0,
                lower_bound: 0,
                size: 1,
                element_type: component_type::F32,
                stride: 0,
                cbuffer_size: 0,
                flags: ResourceFlags::NONE,
                heap_props: [0; 2],
            },
        ],
        ..EntryInfo::default()
    }
}

fn make_shader(entry: EntryInfo) -> DxilShader {
    DxilShader {
        program_type: 5,
        info: ShaderInfo::default(),
        decls: Declarations::default(),
        io: Io::default(),
        bitcode: Vec::new(),
        entry,
        lane_index_arg: 0,
        lane_count_arg: 1,
        include_helper_lanes: false,
        quads_2d: false,
        gs_locks: 0,
        capture_hash: 0,
        writes_uav: false,
    }
}

fn declare_dx_op<'cx>(
    m: &crate::llvm::Module<'cx>,
    name: &[u8],
    ret: Ty,
    params: &[Ty],
) -> Func<'cx> {
    let fn_ty = m.cx().tcx().function(ret, params, false);
    ok(m.get_or_insert_function(name, fn_ty, &[]))
}

#[test]
fn typed_buffer_store_emits_oob_guard() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"tbuf_store.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let handle_ty = tcx.ptr(tcx.int(8), 0);
    let byte_ty = tcx.int(8);
    let bool_ty = tcx.int(1);
    let int_ty = tcx.int(32);
    let long_ty = tcx.int(64);
    let float_ty = tcx.float();
    let cbuf_ptr_ty = tcx.ptr(tcx.array(types.int4, 16), 2);
    let tex_ty = tcx.ptr(tcx.named_struct(c"struct._texture_buffer_1d_t"), 1);

    let fn_ty = tcx.function(
        tcx.void(),
        &[cbuf_ptr_ty, tex_ty, long_ty, int_ty, float_ty],
        false,
    );
    let function = ok(m.add_function(b"main", fn_ty));
    let entry_bb = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, &m, &types, 0);
    assert_eq!(air.texture_handle_type(TextureKind::TextureBuffer), tex_ty);

    let bindings = TestBindings {
        cbuf: function.param(tcx, 0).unwrap(),
        tex: function.param(tcx, 1).unwrap(),
        meta: function.param(tcx, 2).unwrap(),
    };
    let dyn_idx = function.param(tcx, 3).unwrap();
    let valf = function.param(tcx, 4).unwrap();

    let create_handle = declare_dx_op(
        &m,
        b"dx.op.createHandle",
        handle_ty,
        &[int_ty, byte_ty, int_ty, int_ty, bool_ty],
    );
    let handle = b.call(
        create_handle,
        &[
            cx.const_int(int_ty, u64::from(op::CREATE_HANDLE), false),
            cx.const_int(byte_ty, u64::from(resource_class::UAV), false),
            cx.const_int(int_ty, 0, false),
            cx.const_int(int_ty, 0, false),
            cx.const_int(bool_ty, 0, false),
        ],
    );
    let store_fn = declare_dx_op(
        &m,
        b"dx.op.bufferStore.f32",
        tcx.void(),
        &[
            int_ty, handle_ty, int_ty, int_ty, float_ty, float_ty, float_ty, float_ty, byte_ty,
        ],
    );
    let store_call = b.call(
        store_fn,
        &[
            cx.const_int(int_ty, u64::from(op::BUFFER_STORE), false),
            handle,
            dyn_idx,
            cx.undef(int_ty),
            valf,
            valf,
            valf,
            valf,
            cx.const_int(byte_ty, 0xf, false),
        ],
    );
    b.ret_void();

    let entry = make_entry();
    let shader = make_shader(entry);
    let res = Resources::default();
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: &bindings,
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: 5,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };
    let lowering = Lowering {
        conv: &conv,
        dxil: &shader,
        entry: &shader.entry,
        vertex_outputs: None,
    };
    // SAFETY: `store_call` is live in `function` and is not reused after `lower`.
    ok(unsafe { lowering.lower(store_call) });

    let text = String::from_utf8(m.print()).unwrap();
    assert!(text.contains("icmp uge i32 "), "{text}");
    assert!(text.contains("select i1 "), "{text}");
    assert!(text.contains(" i32 -1, i32 "), "{text}");
    assert!(text.contains("air.write_texture_buffer_1d"), "{text}");
}

#[test]
fn cbuffer_load_uses_byte_gep_and_aligned_load() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"cbufload.air");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let handle_ty = tcx.ptr(tcx.int(8), 0);
    let byte_ty = tcx.int(8);
    let bool_ty = tcx.int(1);
    let short_ty = tcx.int(16);
    let int_ty = tcx.int(32);
    let long_ty = tcx.int(64);
    let float_ty = tcx.float();
    let double_ty = tcx.double();
    let cbuf_ptr_ty = tcx.ptr(tcx.array(types.int4, 16), 2);
    let tex_ty = tcx.ptr(tcx.named_struct(c"struct._texture_buffer_1d_t"), 1);

    let fn_ty = tcx.function(tcx.void(), &[cbuf_ptr_ty, tex_ty, long_ty, int_ty], false);
    let function = ok(m.add_function(b"main", fn_ty));
    let entry_bb = function.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(entry_bb);
    let air = AirBuilder::new(&b, &m, &types, 0);

    let bindings = TestBindings {
        cbuf: function.param(tcx, 0).unwrap(),
        tex: function.param(tcx, 1).unwrap(),
        meta: function.param(tcx, 2).unwrap(),
    };
    let dyn_offset = function.param(tcx, 3).unwrap();

    let create_handle = declare_dx_op(
        &m,
        b"dx.op.createHandle",
        handle_ty,
        &[int_ty, byte_ty, int_ty, int_ty, bool_ty],
    );
    let handle = b.call(
        create_handle,
        &[
            cx.const_int(int_ty, u64::from(op::CREATE_HANDLE), false),
            cx.const_int(byte_ty, u64::from(resource_class::CBUFFER), false),
            cx.const_int(int_ty, 0, false),
            cx.const_int(int_ty, 0, false),
            cx.const_int(bool_ty, 0, false),
        ],
    );

    let make_load = |name, ty, align: u64| {
        let callee = declare_dx_op(&m, name, ty, &[int_ty, handle_ty, int_ty, int_ty]);
        b.call(
            callee,
            &[
                cx.const_int(int_ty, u64::from(op::C_BUFFER_LOAD), false),
                handle,
                dyn_offset,
                cx.const_int(int_ty, align, false),
            ],
        )
    };
    let load_short = make_load(b"dx.op.cbufferLoad.i16", short_ty, 1);
    let load_float = make_load(b"dx.op.cbufferLoad.f32", float_ty, 16);
    let load_long = make_load(b"dx.op.cbufferLoad.i64", long_ty, 4);
    let load_double = make_load(b"dx.op.cbufferLoad.f64", double_ty, 0);

    let callee_bad = declare_dx_op(
        &m,
        b"dx.op.cbufferLoad.i32",
        int_ty,
        &[int_ty, handle_ty, int_ty, int_ty],
    );
    let non_const_align = b.call(
        callee_bad,
        &[
            cx.const_int(int_ty, u64::from(op::C_BUFFER_LOAD), false),
            handle,
            dyn_offset,
            dyn_offset,
        ],
    );
    let non_pow2_align = b.call(
        callee_bad,
        &[
            cx.const_int(int_ty, u64::from(op::C_BUFFER_LOAD), false),
            handle,
            dyn_offset,
            cx.const_int(int_ty, 3, false),
        ],
    );
    b.ret_void();

    let entry = make_entry();
    let shader = make_shader(entry);
    let res = Resources::default();
    let conv = Converter {
        air: &air,
        ir: &b,
        function,
        binding: &bindings,
        res: &res,
        types: &types,
        pso_sample_mask: u32::MAX,
        shader_type: 5,
        metal_version: SM50_SHADER_METAL_320,
        geometry: None,
    };
    let lowering = Lowering {
        conv: &conv,
        dxil: &shader,
        entry: &shader.entry,
        vertex_outputs: None,
    };
    for call in [load_short, load_float, load_long, load_double] {
        // SAFETY: each call is live in `function` and is not reused after `lower`.
        ok(unsafe { lowering.lower(call) });
    }
    // SAFETY: `non_const_align` is live in `function` and is not reused after `lower`.
    let Err(err) = (unsafe { lowering.lower(non_const_align) }) else {
        panic!("non-constant cbufferLoad alignment succeeded");
    };
    assert_eq!(err.text(), b"dxil: non-constant cbufferLoad alignment");
    // SAFETY: `non_pow2_align` is live in `function` and is not reused after `lower`.
    let Err(err) = (unsafe { lowering.lower(non_pow2_align) }) else {
        panic!("non-power-of-two cbufferLoad alignment succeeded");
    };
    assert_eq!(err.text(), b"dxil: invalid cbufferLoad alignment");

    let text = String::from_utf8(m.print()).unwrap();
    assert!(
        text.contains("getelementptr i8, ptr addrspace(2)"),
        "{text}"
    );
    let has_load = |ty, align| {
        let prefix = format!("load {ty}, ptr addrspace(2) ");
        let suffix = format!(", align {align}");
        text.lines()
            .any(|line| line.contains(&prefix) && line.ends_with(&suffix))
    };
    assert!(has_load("i16", 1), "{text}");
    assert!(has_load("float", 16), "{text}");
    assert!(has_load("i64", 4), "{text}");
    assert!(has_load("double", 1), "{text}");
}
