//! The DXIL converter: concurrent compiles, a static global's name and the 64-bit atomics.
//!
//! The corpus itself is compared with the C++'s by the lane runs of `dxil-translate` (C1, C2,
//! C3, C8, C11, with the root signatures the tool gives them); these pin what a lane run cannot
//! see.

use airconv_ir::{
    container::{self, Container, fourcc},
    dxil_part, metallib,
};

use super::{convert, move_constants_to_constant_space, rebuilt};
use crate::{
    abi::ArgChain,
    api::Error,
    entry::{compile, initialize},
    llvm::{Builder, Context, sys},
    module,
    shader::Shader,
    writer,
};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// `cache.cs.dxil` initialized once and compiled on eight threads at once (Review Focus 2).
///
/// D3D12's workers may compile one shader handle concurrently: every metallib equals the one a
/// single compile makes.
#[test]
fn compile_on_eight_threads_matches_one() {
    let bytes = include_bytes!("../../../ir/tests/data/cache.cs.dxil");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    let compile_once = || {
        // SAFETY: a null chain is the empty one.
        let args = unsafe { ArgChain::walk(core::ptr::null()) };
        ok(compile(&shader, &args, c"main"))
    };
    let single = compile_once();
    let shared = std::thread::scope(|s| {
        let mut threads = Vec::new();
        for _ in 0..8 {
            threads.push(s.spawn(compile_once));
        }
        threads
            .into_iter()
            .map(|t| t.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(shared.iter().all(|metallib| *metallib == single));
    // A compute function, its bindings (none without a root signature) in a metallib.
    let (bitcode, entries) = metallib::parse(&single).unwrap();
    assert_eq!(entries.len(), 1);
    let cx = Context::new();
    let text =
        String::from_utf8(ok(module::parse(&cx, &bitcode, b"", b"", module::Layout::Keep)).print())
            .unwrap();
    assert!(text.contains("!air.kernel"));
}

/// A mutable static's name reaches the error as its bytes, not as UTF-8 (P15).
#[test]
fn static_global_name_keeps_its_bytes() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    let i32_ty = cx.tcx().int(32);
    // SAFETY: a live module, a type of its context and a NUL-terminated name.
    let global = unsafe {
        sys::add_global_in_address_space(m.raw(), cx.tcx().lower(i32_ty), c"".as_ptr(), 0)
    };
    let name = b"g\xff";
    // SAFETY: the global just made; the name holds its length.
    unsafe { sys::set_value_name2(global, name.as_ptr().cast(), name.len()) };
    // SAFETY: as above; a constant of its value type.
    unsafe { sys::set_initializer(global, cx.const_int(i32_ty, 7, false).raw()) };
    let Err(error) = move_constants_to_constant_space(&m) else {
        panic!("a mutable static moved");
    };
    assert_eq!(
        error.text(),
        b"DXIL: static (non-const) global g\xff not supported"
    );
}

/// 64-bit groupshared atomics (`gsatomic64.dxil`) lower on LLVM 23's opaque pointers (P4).
#[test]
fn groupshared64_atomics_lower_on_opaque_pointers() {
    let bytes = include_bytes!("../../../ir/tests/data/gsatomic64.dxil");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };
    let metallib = ok(compile(&shader, &args, c"main"));
    let (bitcode, entries) = metallib::parse(&metallib).unwrap();
    assert_eq!(entries.len(), 1);
    let cx = Context::new();
    let text =
        String::from_utf8(ok(module::parse(&cx, &bitcode, b"", b"", module::Layout::Keep)).print())
            .unwrap();
    assert!(text.contains("!air.kernel"), "{text}");
    assert!(!text.contains("atomicrmw"), "{text}");
    assert!(
        !text
            .lines()
            .any(|l| l.contains("cmpxchg") && l.contains("i64")),
        "{text}"
    );
}

/// Every 64-bit texel atomic's write (`atomic64typed.dxil`) is built on its section's read.
///
/// So the write can't overtake the read: an Exchange's value doesn't depend on the read, Metal's
/// texture fence orders only a write before a later read, and a read before a later write to the
/// same texel was measured unordered (its old came back as its own new value about once in 50
/// dispatches). The read is of the write's texel, and no lock acquire (a `cmpxchg`) lies between.
#[test]
fn texel64_atomic_writes_depend_on_their_read() {
    let bytes = include_bytes!("../../../ir/tests/data/atomic64typed.dxil");
    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    // The lock table comes with a root signature: RTS0 1.0, one table of t0-t6, u0-u14 and b0
    // (range type, count, base, space, offset).
    let mut rts0: Vec<u32> = vec![1, 1, 24, 0, 0, 0, 0, 0, 36, 3, 44];
    rts0.extend([0, 7, 0, 0, 0, 1, 15, 0, 0, 7, 2, 1, 0, 0, 22]);
    let part: Vec<u8> = rts0.iter().flat_map(|w| w.to_le_bytes()).collect();
    let container = container::build(&[(fourcc::RTS0, &part)]).unwrap();
    let root = crate::abi::Sm50ShaderRootSignatureData {
        next: core::ptr::null_mut(),
        r#type: crate::abi::SM50_SHADER_ROOT_SIGNATURE,
        bytecode: container.as_ptr().cast(),
        bytecode_length: container.len(),
    };
    // SAFETY: a chain of one node, which outlives it, as is its bytecode.
    let args = unsafe { ArgChain::walk((&raw const root).cast()) };
    let metallib = ok(compile(&shader, &args, c"main"));
    let (bitcode, _) = metallib::parse(&metallib).unwrap();
    let cx = Context::new();
    let text =
        String::from_utf8(ok(module::parse(&cx, &bitcode, b"", b"", module::Layout::Keep)).print())
            .unwrap();
    let lines: Vec<&str> = text.lines().collect();
    // `%name` -> (its line, its definition).
    let defs: rustc_hash::FxHashMap<&str, (usize, &str)> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.trim_start().split_once(" = ").map(|(n, d)| (n, (i, d))))
        .filter(|(name, _)| name.starts_with('%'))
        .collect();
    let writes: Vec<usize> = (0..lines.len())
        .filter(|&i| lines[i].contains("call void @air.write_texture_2d"))
        .collect();
    assert!(!writes.is_empty(), "{text}");
    for at in writes {
        let write = lines[at];
        // (handle, coordinate, texel, ...)
        let wargs = call_args(write);
        // The texel's lane .z (which an R32G32 write ignores), traced back through
        // insertelement, shufflevector and extractvalue: it must be a texture read's.
        let (mut value, mut lane) = (last(wargs[2]), 2);
        let (read_at, read) = loop {
            let &(_, def) = defs
                .get(value)
                .unwrap_or_else(|| panic!("{write}: lane {lane} is the constant {value}"));
            let ops = operands(def);
            if def.starts_with("insertelement") {
                let index: usize = last(ops[2])
                    .parse()
                    .unwrap_or_else(|_| panic!("{write}: {def} inserts at a variable index"));
                assert_ne!(index, lane, "{write}: lane {lane} inserted by {def}");
                value = last(ops[0]);
            } else if def.starts_with("shufflevector") {
                let mask: Vec<u32> = ops[2]
                    .trim_start_matches("<4 x i32> <")
                    .trim_end_matches('>')
                    .split(", ")
                    .map(|m| last(m).parse().unwrap())
                    .collect();
                let m = mask[lane];
                (value, lane) = (last(ops[usize::from(m >= 4)]), (m % 4) as usize);
            } else if def.starts_with("extractvalue") {
                break defs.get(last(ops[0])).copied().unwrap_or((0, ""));
            } else {
                panic!("{write}: lane {lane} comes from {value} = {def}");
            }
        };
        assert!(read.contains("@air.read_texture_2d"), "{write}: {read}");
        assert_eq!(
            call_args(read)[1],
            wargs[1],
            "{write} reads another texel: {read}"
        );
        assert!(
            read_at < at && !lines[read_at..at].iter().any(|l| l.contains("cmpxchg")),
            "{write}: its read {read} is of another lock section"
        );
    }
}

/// An instruction's operands, split at the commas outside `<>`, `{}` and `()`.
fn operands(def: &str) -> Vec<&str> {
    split_top(def.split_once(' ').map_or("", |(_, b)| b))
}

/// A call's arguments: inside the parentheses after its callee.
fn call_args(call: &str) -> Vec<&str> {
    let after = &call[call.find('@').unwrap_or(0)..];
    let open = after.find('(').unwrap_or_else(|| panic!("{call}"));
    let mut depth = 0i32;
    let close = after[open..]
        .char_indices()
        .find(|&(_, c)| {
            depth += match c {
                '(' => 1,
                ')' => -1,
                _ => 0,
            };
            depth == 0
        })
        .map_or(after.len(), |(i, _)| open + i);
    split_top(&after[open + 1..close])
}

/// `s` split at the commas outside `<>`, `{}` and `()`, each part trimmed.
fn split_top(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut from) = (Vec::new(), 0i32, 0);
    for (i, c) in s.char_indices() {
        match c {
            '<' | '{' | '(' => depth += 1,
            '>' | '}' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[from..i].trim());
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[from..].trim());
    out
}

/// An operand's last token: its value without its type.
fn last(operand: &str) -> &str {
    operand.rsplit(' ').next().unwrap_or(operand)
}

/// DXC's `.hca` pattern (`buffers.dxil`) retargets and erases a no-op bitcast.
#[test]
fn hca_noop_bitcast_is_retargeted_not_rebuilt() {
    let bytes = include_bytes!("../../../ir/tests/data/buffers.dxil");
    let container = Container::parse(bytes).unwrap();
    let part = dxil_part::find(&container).unwrap().unwrap();
    let raw_cx = Context::new();
    let raw_m = ok(module::parse(
        &raw_cx,
        part.bitcode,
        b"dxil",
        b"DXIL: can't read the bitcode: ",
        module::Layout::Dxil,
    ));
    // SAFETY: a live module and NUL-terminated names.
    let hca0 = unsafe { sys::get_named_global(raw_m.raw(), c"kNeighbor.i.0.hca".as_ptr()) };
    assert!(!hca0.is_null());
    // SAFETY: as above.
    let main_fn = unsafe { sys::get_named_function(raw_m.raw(), c"main".as_ptr()) };
    // Slot 125 of `@main` is the reader's dead `bitcast ptr %10 to ptr` instruction from
    // `llvm.lifetime.start.p0i8`; point it at `@kNeighbor.i.0.hca` so `retarget` sees
    // `bitcast ptr @kNeighbor.i.0.hca to ptr`.
    let dead_cast = crate::dxil::probe::local_slot(main_fn, 125);
    assert!(!dead_cast.is_null());
    // SAFETY: `dead_cast` is a `BitCastInst` of the module.
    assert!(!unsafe { sys::is_a_bit_cast_inst(dead_cast) }.is_null());
    // SAFETY: `dead_cast` has operand 0 of type `ptr`, matching `hca0`.
    unsafe { sys::set_operand(dead_cast, 0, hca0) };
    let before_text = String::from_utf8(raw_m.print()).unwrap();
    assert!(
        before_text.contains("bitcast ptr @kNeighbor.i.0.hca to ptr"),
        "{before_text}"
    );
    ok(move_constants_to_constant_space(&raw_m));
    let moved_text = String::from_utf8(raw_m.print()).unwrap();
    assert!(
        moved_text.contains("@kNeighbor.i.0.hca = internal addrspace(2) constant [4 x i32]"),
        "{moved_text}"
    );
    assert!(
        moved_text.contains("@kNeighbor.i.1.hca = internal addrspace(2) constant [4 x i32]"),
        "{moved_text}"
    );
    assert!(
        !moved_text.contains("bitcast ptr @kNeighbor"),
        "{moved_text}"
    );
    assert_eq!(
        moved_text
            .lines()
            .filter(|l| l.contains(" = load ") && l.contains(", ptr addrspace(2) "))
            .count(),
        4,
        "{moved_text}"
    );

    // SAFETY: no reflection is asked for.
    let shader = ok(unsafe { initialize(bytes, core::ptr::null_mut()) });
    let Shader::Dxil(dxil) = &shader else {
        panic!("expected a DXIL shader");
    };
    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };
    let conv_cx = Context::new();
    let conv_m = ok(convert(&dxil.ir, b"main", &conv_cx, &args));
    let metallib = ok(writer::write(&conv_m, &[]));
    let (bitcode, entries) = metallib::parse(&metallib).unwrap();
    assert_eq!(entries.len(), 1);
    let out_cx = Context::new();
    let out_text = String::from_utf8(
        ok(module::parse(
            &out_cx,
            &bitcode,
            b"",
            b"",
            module::Layout::Keep,
        ))
        .print(),
    )
    .unwrap();
    assert!(
        out_text.contains("@kNeighbor.i.0.hca = internal addrspace(2) constant [4 x i32]"),
        "{out_text}"
    );
    assert!(
        out_text.contains("@kNeighbor.i.1.hca = internal addrspace(2) constant [4 x i32]"),
        "{out_text}"
    );
    assert_eq!(
        out_text
            .lines()
            .filter(|l| l.contains(" = load ") && l.contains(", ptr addrspace(2) "))
            .count(),
        4,
        "{out_text}"
    );
}

/// `rebuilt` refuses a builder closure that folds to its `freeze` placeholder.
#[test]
fn rebuilt_refuses_a_folded_cast() {
    let cx = Context::new();
    let m = module::initialize(&cx, c"shader.air");
    let tcx = cx.tcx();
    let i32_ty = tcx.int(32);
    let fn_ty = tcx.function(tcx.void(), &[], false);
    let f = ok(m.add_function(b"f", fn_ty));
    let bb = f.append_block(&cx, c"entry").unwrap();
    let b = Builder::new(&cx);
    b.position_at_end(bb);
    let old = b.ret_void();
    let to = cx.const_int(i32_ty, 7, false);
    let Err(err) = rebuilt(&cx, old.raw(), to.raw(), |_, placeholder| placeholder) else {
        panic!("a folded cast was accepted");
    };
    assert!(
        String::from_utf8_lossy(err.text()).contains("placeholder"),
        "{}",
        String::from_utf8_lossy(err.text())
    );
    let text = String::from_utf8(m.print()).unwrap();
    assert!(text.contains("freeze i32 7"), "{text}");
    assert!(text.contains("ret void"), "{text}");
}

#[test]
fn standalone_convert_rejects_hull_and_domain() {
    use airconv_ir::shader_info::{Declarations, ShaderInfo};

    use crate::dxil::{
        initialize::DxilShader,
        signature::Io,
        types::{EntryInfo, shader_kind},
    };

    let cx = Context::new();
    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };
    for kind in [shader_kind::HULL, shader_kind::DOMAIN] {
        let shader = DxilShader {
            program_type: kind,
            info: ShaderInfo::default(),
            decls: Declarations::default(),
            io: Io::default(),
            bitcode: Vec::new(),
            entry: EntryInfo {
                kind,
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
        let Err(err) = convert(&shader, b"main", &cx, &args) else {
            panic!("expected linked-stage refusal for kind {kind}");
        };
        assert_eq!(
            err.text(),
            b"DXIL: geometry, hull and domain shaders compile only with their linked stage"
        );
    }
}

#[test]
fn tessellation_hull_and_domain_compile_end_to_end() {
    use std::collections::BTreeSet;

    use airconv_ir::{
        metallib::FunctionKind,
        shader_info::{Declarations, ShaderFlags, ShaderInfo},
        tokens::tessellator,
    };

    use super::{convert_tessellation_domain, convert_tessellation_hull};
    use crate::{
        dxil::{
            initialize::DxilShader,
            signature::Io,
            types::{EntryInfo, SignatureElement, component_type, op, semantic_kind, shader_kind},
        },
        entry::{compile_domain, compile_hull},
        llvm::Module,
        shader::{Dxil, EntryView},
    };

    let empty_view = |stage| EntryView {
        is_dxil: true,
        stage,
        inputs: Vec::new(),
        outputs: Vec::new(),
        resources: Vec::new(),
        _names: Vec::new(),
        _semantic_indices: Box::new([]),
    };

    let build_bitcode = |build: &dyn Fn(&Context, &Module<'_>)| -> Vec<u8> {
        let cx = Context::new();
        let m = Module::new(&cx, c"dxil_src");
        build(&cx, &m);
        let ml = ok(writer::write(&m, &[]));
        metallib::parse(&ml).unwrap().0
    };

    let sig_el = |id, name: &[u8], kind, rows, cols, start: (i32, i32)| SignatureElement {
        id,
        name: name.to_vec(),
        semantic_indices: (0..rows).collect(),
        ty: component_type::F32,
        kind,
        interpolation: 1,
        rows,
        cols,
        start_row: start.0,
        start_col: start.1,
    };

    // Vertex shader writing SV_Position.x at output 0.
    let vertex_bc = build_bitcode(&|cx, m| {
        let tcx = cx.tcx();
        let (i8_ty, i32_ty, f32_ty) = (tcx.int(8), tcx.int(32), tcx.float());
        let store_out = ok(m.add_function(
            b"dx.op.storeOutput.f32",
            tcx.function(tcx.void(), &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty], false),
        ));
        let vs_fn = ok(m.add_function(b"vsmain", tcx.function(tcx.void(), &[], false)));
        let bb = vs_fn.append_block(cx, c"entry").unwrap();
        let b = Builder::new(cx);
        b.position_at_end(bb);
        let _ = b.call(
            store_out,
            &[
                cx.const_int(i32_ty, u64::from(op::STORE_OUTPUT), false),
                cx.const_int(i32_ty, 0, false),
                cx.const_int(i32_ty, 0, false),
                cx.const_int(i8_ty, 0, false),
                cx.const_real(f32_ty, 1.0),
            ],
        );
        b.ret_void();
    });
    let vertex_entry = EntryInfo {
        kind: shader_kind::VERTEX,
        sm_major: 6,
        sm_minor: 0,
        name: b"vsmain".to_vec(),
        outputs: vec![sig_el(
            0,
            b"SV_Position",
            semantic_kind::POSITION,
            1,
            4,
            (0, 0),
        )],
        ..EntryInfo::default()
    };
    let mut vertex_io = Io::default();
    ok(vertex_io.add_signature_handlers(
        &vertex_entry,
        &BTreeSet::new(),
        &BTreeSet::new(),
        false,
        false,
    ));
    let vertex_shader = Shader::Dxil(Dxil {
        ir: DxilShader {
            program_type: shader_kind::VERTEX,
            info: ShaderInfo::default(),
            decls: Declarations::default(),
            io: vertex_io,
            bitcode: vertex_bc,
            entry: vertex_entry,
            lane_index_arg: u32::MAX,
            lane_count_arg: u32::MAX,
            include_helper_lanes: false,
            quads_2d: false,
            gs_locks: 0,
            capture_hash: 0,
            writes_uav: false,
        },
        entry: empty_view(shader_kind::VERTEX),
    });

    // Helper for building both non-passthrough and passthrough DXIL hull shaders.
    let make_hs = |with_cp: bool| -> Shader {
        let hull_bc = build_bitcode(&|cx, m| {
            let tcx = cx.tcx();
            let (i8_ty, i32_ty, f32_ty) = (tcx.int(8), tcx.int(32), tcx.float());
            let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
            let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);
            let store_pc = ok(m.add_function(
                b"dx.op.storePatchConstant.f32",
                tcx.function(tcx.void(), &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty], false),
            ));
            let b = Builder::new(cx);
            if with_cp {
                let store_out = ok(m.add_function(
                    b"dx.op.storeOutput.f32",
                    tcx.function(tcx.void(), &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty], false),
                ));
                let cp_fn = ok(m.add_function(b"hsmain", tcx.function(tcx.void(), &[], false)));
                b.position_at_end(cp_fn.append_block(cx, c"entry").unwrap());
                let _ = b.call(
                    store_out,
                    &[
                        c32(op::STORE_OUTPUT),
                        c32(0),
                        c32(0),
                        c8(0),
                        cx.const_real(f32_ty, 2.0),
                    ],
                );
                b.ret_void();
            }
            let pcf_fn = ok(m.add_function(b"pcf", tcx.function(tcx.void(), &[], false)));
            b.position_at_end(pcf_fn.append_block(cx, c"entry").unwrap());
            for row in 0..3 {
                let _ = b.call(
                    store_pc,
                    &[
                        c32(op::STORE_PATCH_CONSTANT),
                        c32(0),
                        c32(row),
                        c8(0),
                        cx.const_real(f32_ty, 4.0),
                    ],
                );
            }
            b.ret_void();
        });
        let hull_entry = EntryInfo {
            kind: shader_kind::HULL,
            sm_major: 6,
            sm_minor: 0,
            name: if with_cp {
                b"hsmain".to_vec()
            } else {
                Vec::new()
            },
            hs_patch_constant_name: b"pcf".to_vec(),
            hs_input_control_point_count: 3,
            hs_output_control_point_count: 3,
            tess_domain: tessellator::DOMAIN_TRI,
            hs_partitioning: tessellator::PARTITIONING_INTEGER,
            hs_output_primitive: tessellator::OUTPUT_TRIANGLE_CW,
            hs_max_tess_factor: 16.0,
            inputs: vec![sig_el(
                0,
                b"SV_Position",
                semantic_kind::POSITION,
                1,
                4,
                (0, 0),
            )],
            outputs: vec![sig_el(
                0,
                b"SV_Position",
                semantic_kind::POSITION,
                1,
                4,
                (0, 0),
            )],
            patch_constants: vec![sig_el(
                0,
                b"SV_TessFactor",
                semantic_kind::TESS_FACTOR,
                3,
                1,
                (0, 0),
            )],
            ..EntryInfo::default()
        };
        let mut hull_io = Io::default();
        ok(hull_io.add_signature_handlers(
            &hull_entry,
            &BTreeSet::new(),
            &BTreeSet::new(),
            false,
            false,
        ));
        let mut hull_info = ShaderInfo::default();
        hull_info
            .flags
            .insert(ShaderFlags::NO_CONTROL_POINT_PHASE_PASSTHROUGH);
        let hull_decls = Declarations {
            tessellation_domain: tessellator::DOMAIN_TRI,
            tessellation_partition: tessellator::PARTITIONING_INTEGER,
            tessellator_output_primitive: tessellator::OUTPUT_TRIANGLE_CW,
            max_tessellation_factor: 16.0,
            input_control_point_count: 3,
            output_control_point_count: 3,
            hull_maximum_threads_per_patch: 3,
            ..Declarations::default()
        };
        Shader::Dxil(Dxil {
            ir: DxilShader {
                program_type: shader_kind::HULL,
                info: hull_info,
                decls: hull_decls,
                io: hull_io,
                bitcode: hull_bc,
                entry: hull_entry,
                lane_index_arg: u32::MAX,
                lane_count_arg: u32::MAX,
                include_helper_lanes: false,
                quads_2d: false,
                gs_locks: 0,
                capture_hash: 0,
                writes_uav: false,
            },
            entry: empty_view(shader_kind::HULL),
        })
    };

    let hs_cp = make_hs(true);
    let hs_passthrough = make_hs(false);

    // Domain shader reading domainLocation.x + patch constant, writing SV_Position + SV_ClipDistance.
    let domain_bc = build_bitcode(&|cx, m| {
        let tcx = cx.tcx();
        let (i8_ty, i32_ty, f32_ty) = (tcx.int(8), tcx.int(32), tcx.float());
        let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
        let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);
        let dom_loc = ok(m.add_function(
            b"dx.op.domainLocation.f32",
            tcx.function(f32_ty, &[i32_ty, i8_ty], false),
        ));
        let load_pc = ok(m.add_function(
            b"dx.op.loadPatchConstant.f32",
            tcx.function(f32_ty, &[i32_ty, i32_ty, i32_ty, i8_ty], false),
        ));
        let store_out = ok(m.add_function(
            b"dx.op.storeOutput.f32",
            tcx.function(tcx.void(), &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty], false),
        ));
        let ds_fn = ok(m.add_function(b"dsmain", tcx.function(tcx.void(), &[], false)));
        let b = Builder::new(cx);
        b.position_at_end(ds_fn.append_block(cx, c"entry").unwrap());
        let u = b.call(dom_loc, &[c32(op::DOMAIN_LOCATION), c8(0)]);
        let pc = b.call(
            load_pc,
            &[c32(op::LOAD_PATCH_CONSTANT), c32(0), c32(0), c8(0)],
        );
        let sum = b.fadd(u, pc);
        let _ = b.call(
            store_out,
            &[c32(op::STORE_OUTPUT), c32(0), c32(0), c8(0), sum],
        );
        let _ = b.call(
            store_out,
            &[c32(op::STORE_OUTPUT), c32(1), c32(0), c8(0), u],
        );
        b.ret_void();
    });
    let domain_entry = EntryInfo {
        kind: shader_kind::DOMAIN,
        sm_major: 6,
        sm_minor: 0,
        name: b"dsmain".to_vec(),
        tess_domain: tessellator::DOMAIN_TRI,
        ds_input_control_point_count: 3,
        outputs: vec![
            sig_el(0, b"SV_Position", semantic_kind::POSITION, 1, 4, (0, 0)),
            sig_el(
                1,
                b"SV_ClipDistance",
                semantic_kind::CLIP_DISTANCE,
                1,
                1,
                (1, 0),
            ),
        ],
        patch_constants: vec![sig_el(
            0,
            b"SV_TessFactor",
            semantic_kind::TESS_FACTOR,
            3,
            1,
            (0, 0),
        )],
        ..EntryInfo::default()
    };
    let mut domain_io = Io::default();
    ok(domain_io.add_signature_handlers(
        &domain_entry,
        &BTreeSet::new(),
        &BTreeSet::new(),
        false,
        false,
    ));
    let domain_shader = Shader::Dxil(Dxil {
        ir: DxilShader {
            program_type: shader_kind::DOMAIN,
            info: ShaderInfo::default(),
            decls: Declarations {
                tessellation_domain: tessellator::DOMAIN_TRI,
                ..Declarations::default()
            },
            io: domain_io,
            bitcode: domain_bc,
            entry: domain_entry,
            lane_index_arg: u32::MAX,
            lane_count_arg: u32::MAX,
            include_helper_lanes: false,
            quads_2d: false,
            gs_locks: 0,
            capture_hash: 0,
            writes_uav: false,
        },
        entry: empty_view(shader_kind::DOMAIN),
    });

    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };

    // Non-passthrough HS emits cp_start guard, pcf_start guard, and threadgroup barriers.
    let hull_cx = Context::new();
    let hull_m = ok(convert_tessellation_hull(
        &vertex_shader,
        &hs_cp,
        b"hull_obj",
        &hull_cx,
        &args,
    ));
    let hull_ir = String::from_utf8(hull_m.print()).unwrap();
    assert!(hull_ir.contains("hs_cp_start:"), "{hull_ir}");
    assert!(hull_ir.contains("hs_pcf_start:"), "{hull_ir}");
    assert!(hull_ir.contains("call void @air.wg.barrier("), "{hull_ir}");
    assert!(hull_ir.contains("!air.object"), "{hull_ir}");

    // Passthrough HS still emits hs_cp_start guard + semantic copy before hs_pcf_start.
    let pass_cx = Context::new();
    let pass_m = ok(convert_tessellation_hull(
        &vertex_shader,
        &hs_passthrough,
        b"hull_pass",
        &pass_cx,
        &args,
    ));
    let pass_ir = String::from_utf8(pass_m.print()).unwrap();
    assert!(pass_ir.contains("hs_cp_start:"), "{pass_ir}");
    assert!(pass_ir.contains("hs_cp_copy:"), "{pass_ir}");
    assert!(pass_ir.contains("hs_pcf_start:"), "{pass_ir}");

    // Domain converter emits air.mesh and clip-distance/vertex outputs.
    let dom_cx = Context::new();
    let dom_m = ok(convert_tessellation_domain(
        &domain_shader,
        &hs_cp,
        b"dom_mesh",
        &dom_cx,
        &args,
    ));
    let dom_ir = String::from_utf8(dom_m.print()).unwrap();
    assert!(dom_ir.contains("!air.mesh"), "{dom_ir}");
    assert!(dom_ir.contains("@air.set_clip_distance_mesh("), "{dom_ir}");
    assert!(
        dom_ir.contains("@air.set_vertex_data_mesh.v4f32("),
        "{dom_ir}"
    );

    // Full entry compile_hull / compile_domain round-trip into valid metallibs.
    let hull_ml = ok(compile_hull(&vertex_shader, &hs_cp, &args, c"hull_obj"));
    let (_, hull_entries) = metallib::parse(&hull_ml).unwrap();
    assert_eq!(hull_entries.len(), 1);
    assert!(matches!(hull_entries[0].kind, FunctionKind::Object));

    let dom_ml = ok(compile_domain(&hs_cp, &domain_shader, &args, c"dom_mesh"));
    let (_, dom_entries) = metallib::parse(&dom_ml).unwrap();
    assert_eq!(dom_entries.len(), 1);
    assert!(matches!(dom_entries[0].kind, FunctionKind::Mesh));
}

#[test]
fn tessellation_relinks_custom_patch_constant_passthrough_and_wave_inputs() {
    use std::collections::BTreeSet;

    use airconv_ir::{
        shader_info::{Declarations, ShaderFlags, ShaderInfo},
        tokens::tessellator,
    };

    use super::{convert_tessellation_domain, convert_tessellation_hull};
    use crate::{
        dxil::{
            initialize::DxilShader,
            signature::Io,
            types::{EntryInfo, SignatureElement, component_type, op, semantic_kind, shader_kind},
        },
        llvm::Module,
        shader::{Dxil, EntryView},
    };

    let empty_view = |stage| EntryView {
        is_dxil: true,
        stage,
        inputs: Vec::new(),
        outputs: Vec::new(),
        resources: Vec::new(),
        _names: Vec::new(),
        _semantic_indices: Box::new([]),
    };
    let build_bc = |build: &dyn Fn(&Context, &Module<'_>)| -> Vec<u8> {
        let cx = Context::new();
        let m = Module::new(&cx, c"dxil_src");
        build(&cx, &m);
        let ml = ok(writer::write(&m, &[]));
        metallib::parse(&ml).unwrap().0
    };
    let sig_el = |id, name: &[u8], kind, rows, cols, start: (i32, i32)| SignatureElement {
        id,
        name: name.to_vec(),
        semantic_indices: (0..rows).collect(),
        ty: component_type::F32,
        kind,
        interpolation: 1,
        rows,
        cols,
        start_row: start.0,
        start_col: start.1,
    };

    // 1) VS uses WaveGetLaneIndex (index 0 in VS) and outputs SV_Position at row 0 only
    //    (max_vertex_output = 1 < max_hull_output = 2).
    let vertex_bc = build_bc(&|cx, m| {
        let tcx = cx.tcx();
        let (i8_ty, i32_ty, f32_ty) = (tcx.int(8), tcx.int(32), tcx.float());
        let lane_fn = ok(m.add_function(
            b"dx.op.waveGetLaneIndex.i32",
            tcx.function(i32_ty, &[i32_ty], false),
        ));
        let store_out = ok(m.add_function(
            b"dx.op.storeOutput.f32",
            tcx.function(tcx.void(), &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty], false),
        ));
        let vs_fn = ok(m.add_function(b"vsmain", tcx.function(tcx.void(), &[], false)));
        let b = Builder::new(cx);
        b.position_at_end(vs_fn.append_block(cx, c"entry").unwrap());
        let lane = b.call(
            lane_fn,
            &[cx.const_int(i32_ty, u64::from(op::WAVE_GET_LANE_INDEX), false)],
        );
        let lane_f = b.uitofp(lane, f32_ty);
        let _ = b.call(
            store_out,
            &[
                cx.const_int(i32_ty, u64::from(op::STORE_OUTPUT), false),
                cx.const_int(i32_ty, 0, false),
                cx.const_int(i32_ty, 0, false),
                cx.const_int(i8_ty, 0, false),
                lane_f,
            ],
        );
        b.ret_void();
    });
    let vertex_entry = EntryInfo {
        kind: shader_kind::VERTEX,
        sm_major: 6,
        sm_minor: 0,
        name: b"vsmain".to_vec(),
        outputs: vec![sig_el(
            0,
            b"SV_Position",
            semantic_kind::POSITION,
            1,
            4,
            (0, 0),
        )],
        ..EntryInfo::default()
    };
    let mut vertex_io = Io::default();
    ok(vertex_io.add_signature_handlers(
        &vertex_entry,
        &BTreeSet::new(),
        &BTreeSet::new(),
        false,
        false,
    ));
    let vs_ops = BTreeSet::from([op::WAVE_GET_LANE_INDEX]);
    let (vs_lane_idx, vs_lane_cnt) = vertex_io.add_wave_inputs(&vs_ops, shader_kind::VERTEX);
    let vertex_shader = Shader::Dxil(Dxil {
        ir: DxilShader {
            program_type: shader_kind::VERTEX,
            info: ShaderInfo::default(),
            decls: Declarations::default(),
            io: vertex_io,
            bitcode: vertex_bc,
            entry: vertex_entry,
            lane_index_arg: vs_lane_idx,
            lane_count_arg: vs_lane_cnt,
            include_helper_lanes: false,
            quads_2d: false,
            gs_locks: 0,
            capture_hash: 0,
            writes_uav: false,
        },
        entry: empty_view(shader_kind::VERTEX),
    });

    // 2) Pass-through HS places SV_Position at output row 1 (max_hull_output = 2 > max_vertex_output),
    //    uses WaveGetLaneCount in PCF, reads output control point via LOAD_OUTPUT_CONTROL_POINT,
    //    and writes custom patch constant PATCHDATA at row 4.
    let hull_bc = build_bc(&|cx, m| {
        let tcx = cx.tcx();
        let (i8_ty, i32_ty, f32_ty) = (tcx.int(8), tcx.int(32), tcx.float());
        let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
        let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);
        let cnt_fn = ok(m.add_function(
            b"dx.op.waveGetLaneCount.i32",
            tcx.function(i32_ty, &[i32_ty], false),
        ));
        let load_ocp = ok(m.add_function(
            b"dx.op.loadOutputControlPoint.f32",
            tcx.function(f32_ty, &[i32_ty, i32_ty, i32_ty, i8_ty, i32_ty], false),
        ));
        let store_pc = ok(m.add_function(
            b"dx.op.storePatchConstant.f32",
            tcx.function(tcx.void(), &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty], false),
        ));
        let pcf_fn = ok(m.add_function(b"pcf", tcx.function(tcx.void(), &[], false)));
        let b = Builder::new(cx);
        b.position_at_end(pcf_fn.append_block(cx, c"entry").unwrap());
        let lane_cnt = b.call(cnt_fn, &[c32(op::WAVE_GET_LANE_COUNT)]);
        let ocp_val = b.call(
            load_ocp,
            &[
                c32(op::LOAD_OUTPUT_CONTROL_POINT),
                c32(0),
                c32(0),
                c8(0),
                c32(1),
            ],
        );
        let combined = b.fadd(ocp_val, b.uitofp(lane_cnt, f32_ty));
        for row in 0..3 {
            let _ = b.call(
                store_pc,
                &[
                    c32(op::STORE_PATCH_CONSTANT),
                    c32(0),
                    c32(row),
                    c8(0),
                    cx.const_real(f32_ty, 4.0),
                ],
            );
        }
        let _ = b.call(
            store_pc,
            &[
                c32(op::STORE_PATCH_CONSTANT),
                c32(1),
                c32(0),
                c8(0),
                combined,
            ],
        );
        b.ret_void();
    });
    let hull_entry = EntryInfo {
        kind: shader_kind::HULL,
        sm_major: 6,
        sm_minor: 0,
        name: Vec::new(),
        hs_patch_constant_name: b"pcf".to_vec(),
        hs_input_control_point_count: 3,
        hs_output_control_point_count: 3,
        tess_domain: tessellator::DOMAIN_TRI,
        hs_partitioning: tessellator::PARTITIONING_INTEGER,
        hs_output_primitive: tessellator::OUTPUT_TRIANGLE_CW,
        hs_max_tess_factor: 16.0,
        inputs: vec![sig_el(
            0,
            b"SV_Position",
            semantic_kind::POSITION,
            1,
            4,
            (0, 0),
        )],
        outputs: vec![sig_el(
            0,
            b"SV_Position",
            semantic_kind::POSITION,
            1,
            4,
            (1, 0),
        )],
        patch_constants: vec![
            sig_el(
                0,
                b"SV_TessFactor",
                semantic_kind::TESS_FACTOR,
                3,
                1,
                (0, 0),
            ),
            sig_el(1, b"PATCHDATA", semantic_kind::ARBITRARY, 1, 1, (4, 0)),
        ],
        ..EntryInfo::default()
    };
    let mut hull_io = Io::default();
    ok(hull_io.add_signature_handlers(
        &hull_entry,
        &BTreeSet::new(),
        &BTreeSet::new(),
        false,
        false,
    ));
    // Reverse wave input order in HS alone (ThreadsPerSimdgroup first) to verify remapping.
    let hs_lane_cnt = hull_io
        .func_signature
        .define_input(airconv_ir::binding::FunctionInput::ThreadsPerSimdgroup);
    let mut hull_info = ShaderInfo::default();
    hull_info
        .flags
        .insert(ShaderFlags::NO_CONTROL_POINT_PHASE_PASSTHROUGH);
    hull_info
        .flags
        .insert(ShaderFlags::OUTPUT_CONTROL_POINT_READ);
    let hull_shader = Shader::Dxil(Dxil {
        ir: DxilShader {
            program_type: shader_kind::HULL,
            info: hull_info,
            decls: Declarations {
                tessellation_domain: tessellator::DOMAIN_TRI,
                tessellation_partition: tessellator::PARTITIONING_INTEGER,
                tessellator_output_primitive: tessellator::OUTPUT_TRIANGLE_CW,
                max_tessellation_factor: 16.0,
                input_control_point_count: 3,
                output_control_point_count: 3,
                hull_maximum_threads_per_patch: 3,
                ..Declarations::default()
            },
            io: hull_io,
            bitcode: hull_bc,
            entry: hull_entry,
            lane_index_arg: u32::MAX,
            lane_count_arg: hs_lane_cnt,
            include_helper_lanes: false,
            quads_2d: false,
            gs_locks: 0,
            capture_hash: 0,
            writes_uav: false,
        },
        entry: empty_view(shader_kind::HULL),
    });

    // 3) DS declares ONLY PATCHDATA at row 0 (differing PSG1 from HS row 4).
    let domain_bc = build_bc(&|cx, m| {
        let tcx = cx.tcx();
        let (i8_ty, i32_ty, f32_ty) = (tcx.int(8), tcx.int(32), tcx.float());
        let c32 = |v: u32| cx.const_int(i32_ty, u64::from(v), false);
        let c8 = |v: u8| cx.const_int(i8_ty, u64::from(v), false);
        let load_pc = ok(m.add_function(
            b"dx.op.loadPatchConstant.f32",
            tcx.function(f32_ty, &[i32_ty, i32_ty, i32_ty, i8_ty], false),
        ));
        let store_out = ok(m.add_function(
            b"dx.op.storeOutput.f32",
            tcx.function(tcx.void(), &[i32_ty, i32_ty, i32_ty, i8_ty, f32_ty], false),
        ));
        let ds_fn = ok(m.add_function(b"dsmain", tcx.function(tcx.void(), &[], false)));
        let b = Builder::new(cx);
        b.position_at_end(ds_fn.append_block(cx, c"entry").unwrap());
        let pc = b.call(
            load_pc,
            &[c32(op::LOAD_PATCH_CONSTANT), c32(0), c32(0), c8(0)],
        );
        let _ = b.call(
            store_out,
            &[c32(op::STORE_OUTPUT), c32(0), c32(0), c8(0), pc],
        );
        b.ret_void();
    });
    let domain_entry = EntryInfo {
        kind: shader_kind::DOMAIN,
        sm_major: 6,
        sm_minor: 0,
        name: b"dsmain".to_vec(),
        tess_domain: tessellator::DOMAIN_TRI,
        ds_input_control_point_count: 3,
        outputs: vec![sig_el(
            0,
            b"SV_Position",
            semantic_kind::POSITION,
            1,
            4,
            (0, 0),
        )],
        patch_constants: vec![sig_el(
            0,
            b"PATCHDATA",
            semantic_kind::ARBITRARY,
            1,
            1,
            (0, 0),
        )],
        ..EntryInfo::default()
    };
    let mut domain_io = Io::default();
    ok(domain_io.add_signature_handlers(
        &domain_entry,
        &BTreeSet::new(),
        &BTreeSet::new(),
        false,
        false,
    ));
    let domain_shader = Shader::Dxil(Dxil {
        ir: DxilShader {
            program_type: shader_kind::DOMAIN,
            info: ShaderInfo::default(),
            decls: Declarations {
                tessellation_domain: tessellator::DOMAIN_TRI,
                ..Declarations::default()
            },
            io: domain_io,
            bitcode: domain_bc,
            entry: domain_entry,
            lane_index_arg: u32::MAX,
            lane_count_arg: u32::MAX,
            include_helper_lanes: false,
            quads_2d: false,
            gs_locks: 0,
            capture_hash: 0,
            writes_uav: false,
        },
        entry: empty_view(shader_kind::DOMAIN),
    });

    // SAFETY: a null chain is the empty one.
    let args = unsafe { ArgChain::walk(core::ptr::null()) };

    let hull_cx = Context::new();
    let hull_m = ok(convert_tessellation_hull(
        &vertex_shader,
        &hull_shader,
        b"hull_relink",
        &hull_cx,
        &args,
    ));
    let hull_text = String::from_utf8(hull_m.print()).unwrap();
    // Pass-through copy stores VS row 0 -> HS output row 1, and PCF loads HS output row 1.
    assert!(hull_text.contains("hs_cp_copy:"), "{hull_text}");
    assert!(
        hull_text.contains("@hull_control_point_phase_out"),
        "{hull_text}"
    );
    assert!(
        hull_text.contains("getelementptr [3 x [2 x <4 x i32>]], ptr addrspace(3)"),
        "{hull_text}"
    );
    // VS WaveGetLaneIndex and HS WaveGetLaneCount must bind to distinct merged parameters.
    let uitofp_sources: Vec<_> = hull_text
        .lines()
        .filter_map(|line| line.split_once("uitofp i32 ").map(|(_, rhs)| rhs.trim()))
        .collect();
    assert_eq!(uitofp_sources.len(), 2, "{hull_text}");
    assert_ne!(uitofp_sources[0], uitofp_sources[1], "{hull_text}");

    let dom_cx = Context::new();
    let dom_m = ok(convert_tessellation_domain(
        &domain_shader,
        &hull_shader,
        b"dom_relink",
        &dom_cx,
        &args,
    ));
    let dom_ir = String::from_utf8(dom_m.print()).unwrap();
    // DS allocates 5 patch-constant rows and relinks PATCHDATA from DS row 0 to HS row 4.
    assert!(dom_ir.contains("alloca [5 x <4 x i32>]"), "{dom_ir}");
    assert!(
        dom_ir
            .lines()
            .any(|l| l.contains("getelementptr [5 x <4 x i32>], ptr %")
                && l.contains(", i32 0, i32 4, i32 0")),
        "{dom_ir}"
    );
}
