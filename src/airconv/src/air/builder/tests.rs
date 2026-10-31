//! Tests of the AIR builder: barrier declarations, refusals, shader flags and overload spellings.

use std::collections::BTreeMap;

use super::{
    AirBuilder, Lod, MemFlags, MemoryAccess, SampleAt, SampleType, Signedness, Texture, TextureKind,
};
use crate::{
    abi::{SM50_SHADER_FLAG_DEFUSE_FMA, SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO},
    air::ty::AirTypes,
    api::Error,
    llvm::{Builder, Context, Ty, TyCx, Val, sys},
    module,
};

/// The value of `result`, or a panic showing the error's text (`Error` has no `Debug`).
fn ok<T>(result: Result<T, Error>) -> T {
    result.unwrap_or_else(|e| panic!("{}", String::from_utf8_lossy(e.text())))
}

/// The error text of `result`, which must be an error.
fn error_text<T>(result: Result<T, Error>) -> String {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(e) => String::from_utf8_lossy(e.text()).into_owned(),
    }
}

/// Each `declare … @air.*` of a module's text by name, its attribute group written out.
///
/// `declare void @f(i32) #0` with `attributes #0 = { nounwind }` reads
/// `declare void @f(i32) ;; nounwind`.
fn declarations(text: &str) -> BTreeMap<String, String> {
    let groups: BTreeMap<&str, &str> = text
        .lines()
        .filter_map(|line| {
            let (group, attrs) = line.strip_prefix("attributes ")?.split_once(" = { ")?;
            Some((group, attrs.strip_suffix(" }")?))
        })
        .collect();
    text.lines()
        .filter(|line| line.starts_with("declare ") && line.contains(" @air."))
        .map(|line| {
            let name = line.split_once(" @").unwrap().1.split_once('(').unwrap().0;
            let normal = match line.rsplit_once(" #") {
                Some((head, group)) if group.bytes().all(|b| b.is_ascii_digit()) => {
                    format!("{head} ;; {}", groups[format!("#{group}").as_str()])
                }
                _ => format!("{line} ;; "),
            };
            (name.to_owned(), normal)
        })
        .collect()
}

/// A module with one function `f(params…)` positioned at its entry, for `test`.
fn with_function(
    params: fn(&TyCx) -> Vec<Ty>,
    shader_flags: u32,
    test: impl FnOnce(&AirBuilder<'_, '_>, &[Val<'_>]),
) -> String {
    let cx = Context::new();
    let m = module::initialize(&cx, c"t");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let params = params(tcx);
    let f = ok(m.add_function(b"f", tcx.function(tcx.void(), &params, false)));
    let b = Builder::new(&cx);
    b.position_at_end(f.append_block(&cx, c"entry").unwrap());
    let args: Vec<Val<'_>> = (0..params.len())
        .map(|i| f.param(tcx, u32::try_from(i).unwrap()).unwrap())
        .collect();
    test(&AirBuilder::new(&b, &m, &types, shader_flags), &args);
    b.ret_void();
    String::from_utf8_lossy(&m.print()).into_owned()
}

/// The shim's barrier declaration (the C++ pass's, G1b) and `create_barrier`'s print the same.
#[test]
fn barrier_declaration_matches_the_shim() {
    let cx = Context::new();
    let shim = module::initialize(&cx, c"shim");
    // SAFETY: a live module.
    let declared = unsafe { sys::airconv_shim_declare_simdgroup_barrier(shim.raw()) };
    assert!(!declared.is_null());
    let shim_text = String::from_utf8_lossy(&shim.print()).into_owned();
    let ours = with_function(
        |_| Vec::new(),
        0,
        |air, _| {
            ok(air.create_barrier(MemFlags::THREADGROUP, true));
        },
    );
    let name = "air.simdgroup.barrier";
    let theirs = declarations(&shim_text).remove(name);
    assert!(theirs.is_some(), "{shim_text}");
    assert_eq!(theirs, declarations(&ours).remove(name));
}

/// Where the C++ answered null or called with a null operand, the method answers an error.
#[test]
fn null_answers_are_errors() {
    let texture = |kind, sample_type| Texture {
        kind,
        sample_type,
        memory_access: MemoryAccess::ReadWrite,
    };
    let mut texts = Vec::new();
    with_function(
        |_| Vec::new(),
        0,
        |air, _| {
            let tex2d = texture(TextureKind::Texture2d, SampleType::Float);
            let handle = air
                .builder()
                .cx()
                .null(air.texture_handle_type(TextureKind::Texture2d));
            let sampler = air.builder().cx().null(air.sampler_handle_type());
            let pos = air.get_int2(0, 0);
            let float = air.get_float(0.0);
            let value = air.builder().cx().null(air.float_ty(4));
            texts.push(error_text(air.create_texture_atomic_rmw(
                &tex2d,
                handle,
                sys::ATOMIC_RMW_BIN_OP_ADD,
                pos,
                value,
                None,
            )));
            let depth = texture(TextureKind::Depth2d, SampleType::Uint);
            texts.push(error_text(
                air.create_atomic_cmpxchg(&depth, handle, pos, value, value, None),
            ));
            let at = SampleAt {
                coord: air.get_float2(0.0, 0.0),
                array_index: None,
                offset: Some(pos),
            };
            texts.push(error_text(air.create_sample_cmp(
                &tex2d,
                handle,
                sampler,
                &at,
                float,
                &Lod::Implicit,
            )));
            let array = texture(TextureKind::Texture2dArray, SampleType::Float);
            texts.push(error_text(air.create_sample(
                &array,
                handle,
                sampler,
                &at,
                &Lod::Implicit,
            )));
            texts.push(error_text(air.create_derivative(pos, false)));
            texts.push(error_text(air.create_fma(float, float, pos)));
            texts.push(error_text(air.create_atomic_rmw(
                sys::ATOMIC_RMW_BIN_OP_ADD,
                float,
                float,
            )));
            texts.push(error_text(
                air.create_set_mesh_position(air.get_int(0), value),
            ));
        },
    );
    assert_eq!(
        texts,
        [
            "invalid operation: atomic operation on non integer texture.",
            "invalid operation: atomic operation on depth texture.",
            "invalid operation: compare on non depth texture.",
            "invalid operation: air.sample_texture_2d_array.v4f32 lacks an operand.",
            "invalid operation: derivative on non-fp type.",
            "invalid operation: fma has unmatched operand types.",
            "invalid operation: atomicrmw: not a pointer.",
            "invalid operation: air.set_position_mesh lacks an operand.",
        ]
    );
}

/// The two shader flags: NaN sample coordinates zeroed by a select, FMA as a multiply and an add.
#[test]
fn shader_flags_zero_nan_coordinates_and_defuse_fma() {
    let tex2d = Texture {
        kind: TextureKind::Texture2d,
        sample_type: SampleType::Float,
        memory_access: MemoryAccess::Sample,
    };
    let params = |tcx: &TyCx| {
        let float = tcx.float();
        vec![tcx.vector(float, 2), float, float, float]
    };
    let run = |flags| {
        with_function(params, flags, |air, args| {
            let handle = air.builder().cx().null(air.texture_handle_type(tex2d.kind));
            let sampler = air.builder().cx().null(air.sampler_handle_type());
            let at = SampleAt {
                coord: args[0],
                array_index: None,
                offset: air.get_offset(&tex2d, [1, -1, 0]),
            };
            ok(air.create_sample(&tex2d, handle, sampler, &at, &Lod::Level(args[1])));
            ok(air.create_fma(args[1], args[2], args[3]));
        })
    };
    let plain = run(0);
    let flagged = run(SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO | SM50_SHADER_FLAG_DEFUSE_FMA);
    assert!(plain.contains("call float @air.fma.f32(") && !plain.contains("select"));
    assert!(plain.contains("<2 x i32> <i32 1, i32 -1>, i1 true, float %1, float 0.000000e+00"));
    assert!(!flagged.contains("@air.fma") && flagged.contains("fmul") && flagged.contains("fadd"));
    assert!(flagged.contains("icmp ugt <2 x i32>") && flagged.contains("select <2 x i1>"));
}

/// `getTypeOverloadSuffix`'s spellings.
#[test]
fn type_overload_suffix_matches_the_cxx() {
    let mut spelled = Vec::new();
    with_function(
        |_| Vec::new(),
        0,
        |air, _| {
            let tcx = air.builder().cx().tcx();
            let (int, float, half) = (tcx.int(32), tcx.float(), tcx.half());
            let cases = [
                (tcx.vector(float, 4), Signedness::DontCare),
                (float, Signedness::Unsigned),
                (tcx.vector(half, 2), Signedness::Signed),
                (int, Signedness::Signed),
                (tcx.int(16), Signedness::Unsigned),
                (tcx.ptr(int, 1), Signedness::Unsigned),
                (tcx.ptr(tcx.vector(int, 4), 3), Signedness::DontCare),
                (tcx.double(), Signedness::DontCare),
            ];
            for (ty, sign) in cases {
                spelled.push(String::from_utf8(air.type_overload_suffix(ty, sign)).unwrap());
            }
        },
    );
    let expected = [
        ".v4f32",
        ".f.f32",
        ".f.v2f16",
        ".s.i32",
        ".u.i16",
        ".u.p1i32",
        ".p3v4i32",
        ".unknown_type_overload",
    ];
    assert_eq!(spelled, expected);
}
