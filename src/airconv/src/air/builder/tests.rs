//! Tests of the AIR builder: its declarations against the C++ baseline's, refusals and flags.
//!
//! The baseline test reads `AIRCONV_STEP4_BASE`, step 4's frozen C++ outputs: unset, it prints why
//! and passes; set, a missing folder fails it.

use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::CString,
    fs,
    path::{Path, PathBuf},
};

use super::{
    ARRAY, AirBuilder, Attrs, CUBE, Call, DEPTH, FN, FpBinOp, FpUnOp, Gradient, IntBinOp, IntUnOp,
    Lod, MIPMAPPED, MS, MemFlags, MemoryAccess, Query, SampleAt, SampleType, Signedness, TexelAt,
    Texture, TextureKind, ThreadScope, offset_width,
};
use crate::{
    abi::{SM50_SHADER_FLAG_DEFUSE_FMA, SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO},
    air::ty::AirTypes,
    api::Error,
    llvm::{Builder, Context, Module, Ty, TyCx, Val, sys},
    module,
};

/// The atomic fence.
const ATOMIC_FENCE: &Attrs = &[
    (FN, b"mustprogress"),
    (FN, b"nounwind"),
    (FN, b"willreturn"),
];

impl<'cx> AirBuilder<'_, 'cx> {
    /// `air.atomic.fence`, sequentially consistent or relaxed (`CreateAtomicFence`).
    ///
    /// # Errors
    ///
    /// None in practice: only P15's name check, which the name passes.
    pub fn create_atomic_fence(
        &self,
        flags: MemFlags,
        scope: ThreadScope,
        relaxed: bool,
    ) -> Result<Val<'cx>, Error> {
        let mut call = Call::new(self);
        call.push(self.int_ty(1), self.get_int(flags.bits()));
        call.push(self.int_ty(1), self.get_int(if relaxed { 0 } else { 5 }));
        call.push(self.int_ty(1), self.get_int(scope as u32));
        call.finish(b"air.atomic.fence", self.void_ty(), ATOMIC_FENCE)
    }

    /// `air.set_primitive_data_mesh` of a varying (`CreateSetMeshPrimitiveData`).
    ///
    /// # Errors
    ///
    /// A function without a mesh argument (where the C++ called with null).
    pub fn create_set_mesh_primitive_data(
        &self,
        primitive: Val<'cx>,
        data_index: Val<'cx>,
        value: Val<'cx>,
    ) -> Result<Val<'cx>, Error> {
        let mut name = b"air.set_primitive_data_mesh".to_vec();
        name.extend(self.type_overload_suffix(value.ty(), Signedness::DontCare));
        let int = self.int_ty(1);
        self.mesh_call(
            &name,
            &[(int, data_index), (int, primitive), (value.ty(), value)],
        )
    }
}

/// The Metal helpers `build.rs` compiles, as `module.rs` links them.
const HELPERS: [&[u8]; 3] = [
    include_bytes!(concat!(env!("OUT_DIR"), "/air_msad.air")),
    include_bytes!(concat!(env!("OUT_DIR"), "/air_samplepos.air")),
    include_bytes!(concat!(env!("OUT_DIR"), "/air_tessellation.air")),
];

/// The declarations of other emitters: `air_operations.cpp`'s unpacks, `dxil_lower.cpp`'s `Simd`.
const OTHER_EMITTERS: [&str; 3] = ["air.unpack.", "air.simd_", "air.quad_"];

/// Every texture kind, longest air suffix first where one is a prefix of another.
const KINDS: [TextureKind; 16] = [
    TextureKind::TextureBuffer,
    TextureKind::Texture1dArray,
    TextureKind::Texture1d,
    TextureKind::Texture2dMsArray,
    TextureKind::Texture2dMs,
    TextureKind::Texture2dArray,
    TextureKind::Texture2d,
    TextureKind::Texture3d,
    TextureKind::TextureCubeArray,
    TextureKind::TextureCube,
    TextureKind::Depth2dMsArray,
    TextureKind::Depth2dMs,
    TextureKind::Depth2dArray,
    TextureKind::Depth2d,
    TextureKind::DepthCubeArray,
    TextureKind::DepthCube,
];

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

/// The names the three Metal helpers declare (linked in, they bring their declarations).
fn helper_names() -> BTreeSet<String> {
    let cx = Context::new();
    HELPERS
        .iter()
        .flat_map(|air| {
            let m = ok(module::parse(&cx, air, b"", b""));
            declarations(&String::from_utf8_lossy(&m.print())).into_keys()
        })
        .collect()
}

/// The baseline folder, or `None` (and why) when `AIRCONV_STEP4_BASE` is unset.
fn base(skipped: &str) -> Option<PathBuf> {
    let Some(base) = env::var_os("AIRCONV_STEP4_BASE") else {
        eprintln!("AIRCONV_STEP4_BASE is not set: {skipped}");
        return None;
    };
    Some(PathBuf::from(base))
}

fn sorted(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .collect();
    paths.sort();
    paths
}

/// Every distinct `declare … @air.*` line of the baseline's `*.dis/*.ll`, with a file holding it.
fn baseline_declarations(base: &Path) -> BTreeMap<String, PathBuf> {
    let mut lines = BTreeMap::new();
    for dis in sorted(base)
        .into_iter()
        .filter(|d| d.is_dir() && d.extension().is_some_and(|e| e == "dis"))
    {
        for ll in sorted(&dis) {
            let text = String::from_utf8_lossy(&fs::read(&ll).unwrap()).into_owned();
            for line in declarations(&text).into_values() {
                lines.entry(line).or_insert_with(|| ll.clone());
            }
        }
    }
    assert!(!lines.is_empty(), "no declaration under {}", base.display());
    lines
}

/// The type at the start of `text`, and the rest: LLVM's syntax for the types AIR declares.
fn parse_ty<'t>(tcx: &TyCx, text: &'t str) -> (Ty, &'t str) {
    let (mut ty, mut rest) = parse_unpointed(tcx, text.trim_start());
    loop {
        if let Some(after) = rest.strip_prefix(" addrspace(") {
            let (space, after) = after.split_once(")*").unwrap();
            ty = tcx.ptr(ty, space.parse().unwrap());
            rest = after;
        } else if let Some(after) = rest.strip_prefix('*') {
            ty = tcx.ptr(ty, 0);
            rest = after;
        } else {
            return (ty, rest);
        }
    }
}

/// A type before its pointer suffixes, and the rest.
fn parse_unpointed<'t>(tcx: &TyCx, text: &'t str) -> (Ty, &'t str) {
    if let Some(rest) = text.strip_prefix('<') {
        let (n, rest) = rest.split_once(" x ").unwrap();
        let (element, rest) = parse_ty(tcx, rest);
        return (
            tcx.vector(element, n.parse().unwrap()),
            rest.strip_prefix('>').unwrap(),
        );
    }
    if let Some(mut rest) = text.strip_prefix("{ ") {
        let mut fields = Vec::new();
        loop {
            let (field, after) = parse_ty(tcx, rest);
            fields.push(field);
            match after.strip_prefix(", ") {
                Some(after) => rest = after,
                None => {
                    return (
                        tcx.literal_struct(&fields, false),
                        after.strip_prefix(" }").unwrap(),
                    );
                }
            }
        }
    }
    if let Some(rest) = text.strip_prefix('%') {
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_'))
            .unwrap_or(rest.len());
        let name = CString::new(&rest[..end]).unwrap();
        return (tcx.named_struct(&name), &rest[end..]);
    }
    let end = text
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(text.len());
    let ty = match &text[..end] {
        "void" => tcx.void(),
        "half" => tcx.half(),
        "float" => tcx.float(),
        "double" => tcx.double(),
        int => tcx.int(int.strip_prefix('i').unwrap().parse().unwrap()),
    };
    (ty, &text[end..])
}

/// A baseline declaration: its name, return type and parameter types, as text.
struct Decl<'l> {
    name: &'l str,
    ret: &'l str,
    params: Vec<&'l str>,
}

impl<'l> Decl<'l> {
    fn parse(line: &'l str) -> Self {
        let (ret, tail) = line
            .strip_prefix("declare ")
            .unwrap()
            .split_once(" @")
            .unwrap();
        let (name, tail) = tail.split_once('(').unwrap();
        // The parameters end at the parenthesis closing the list (`addrspace(1)` holds one).
        let mut depth = 0;
        let end = tail
            .find(|c| {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                depth < 0
            })
            .unwrap();
        let params = &tail[..end];
        let params = if params.is_empty() {
            Vec::new()
        } else {
            params.split(", ").collect()
        };
        Self { name, ret, params }
    }
}

/// The parameters of a test function, taken by type in the order the call passes them.
struct Pick<'p, 'cx> {
    params: &'p [Val<'cx>],
    next: usize,
}

impl<'cx> Pick<'_, 'cx> {
    /// The next parameter of type `ty` (the last one again when none is left).
    fn take(&mut self, ty: Ty) -> Val<'cx> {
        let found = (self.next..self.params.len())
            .chain(0..self.params.len())
            .find(|&i| self.params[i].ty() == ty)
            .expect("a parameter of the operand's type");
        self.next = found + 1;
        self.params[found]
    }
}

/// The texture whose air suffix starts `text` (its sample type from the overload suffix), the rest.
fn texture_of(text: &str) -> Option<(Texture, &str)> {
    KINDS.iter().find_map(|&kind| {
        let rest = text.strip_prefix(core::str::from_utf8(kind.info().suffix).unwrap())?;
        if !(rest.is_empty() || rest.starts_with('.') || rest.starts_with("_grad")) {
            return None;
        }
        let sample_type = if rest.contains(".u.") {
            SampleType::Uint
        } else if rest.contains(".s.") {
            SampleType::Int
        } else {
            SampleType::Float
        };
        let texture = Texture {
            kind,
            sample_type,
            memory_access: MemoryAccess::Read,
        };
        Some((texture, rest))
    })
}

/// The C API's atomic operation of a name part, signed or not.
fn atomic_op_of(op: &str, signed: bool) -> sys::AtomicRMWBinOp {
    match (op, signed) {
        ("add", _) => sys::ATOMIC_RMW_BIN_OP_ADD,
        ("sub", _) => sys::ATOMIC_RMW_BIN_OP_SUB,
        ("and", _) => sys::ATOMIC_RMW_BIN_OP_AND,
        ("or", _) => sys::ATOMIC_RMW_BIN_OP_OR,
        ("xor", _) => sys::ATOMIC_RMW_BIN_OP_XOR,
        ("max", true) => sys::ATOMIC_RMW_BIN_OP_MAX,
        ("min", true) => sys::ATOMIC_RMW_BIN_OP_MIN,
        ("max", false) => sys::ATOMIC_RMW_BIN_OP_U_MAX,
        ("min", false) => sys::ATOMIC_RMW_BIN_OP_U_MIN,
        ("xchg", _) => sys::ATOMIC_RMW_BIN_OP_XCHG,
        _ => panic!("atomic operation {op}"),
    }
}

/// The floating-point unary operation of a name part.
fn fp_un_op(name: &str) -> Option<FpUnOp> {
    Some(match name {
        "saturate" => FpUnOp::Saturate,
        "log2" => FpUnOp::Log2,
        "exp2" => FpUnOp::Exp2,
        "sqrt" => FpUnOp::Sqrt,
        "rsqrt" => FpUnOp::Rsqrt,
        "fract" => FpUnOp::Fract,
        "rint" => FpUnOp::Rint,
        "floor" => FpUnOp::Floor,
        "ceil" => FpUnOp::Ceil,
        "trunc" => FpUnOp::Trunc,
        "cos" => FpUnOp::Cos,
        "sin" => FpUnOp::Sin,
        "fabs" => FpUnOp::Fabs,
        "tan" => FpUnOp::Tan,
        "acos" => FpUnOp::Acos,
        "asin" => FpUnOp::Asin,
        "atan" => FpUnOp::Atan,
        "cosh" => FpUnOp::Cosh,
        "sinh" => FpUnOp::Sinh,
        "tanh" => FpUnOp::Tanh,
        _ => return None,
    })
}

/// The sign an `air.convert` source part asks for: `f.`, `s.` and `u.` are not `DontCare`.
fn sign_of(part: &str) -> Signedness {
    if part.starts_with("u.") {
        Signedness::Unsigned
    } else if part.starts_with("s.") || part.starts_with("f.") {
        Signedness::Signed
    } else {
        Signedness::DontCare
    }
}

/// The `AIRBuilder` call declaring `name` from `params`; `None` for a name no method makes.
///
/// The call's own name may still differ from `name`: the caller compares.
fn emit<'cx>(
    air: &AirBuilder<'_, 'cx>,
    name: &str,
    params: &[Val<'cx>],
    result_ty: Ty,
) -> Option<Result<Vec<Val<'cx>>, Error>> {
    let one = |r: Result<Val<'cx>, Error>| Some(r.map(|v| vec![v]));
    let two = |r: Result<(Val<'cx>, Val<'cx>), Error>| Some(r.map(|(a, b)| vec![a, b]));
    let mut p = Pick { params, next: 0 };
    let int = air.int_ty(1);
    let float = air.float_ty(1);
    let op = name.strip_prefix("air.")?;
    if let Some(rest) = op.strip_prefix("sample_compare_") {
        let (tex, rest) = texture_of(rest)?;
        let info = tex.kind.info();
        let coord_ty = air.texture_sample_coord_type(&tex);
        let handle = p.take(air.texture_handle_type(tex.kind));
        let sampler = p.take(air.sampler_handle_type());
        let coord = p.take(coord_ty);
        let array_index = info.has(ARRAY).then(|| p.take(int));
        let reference = p.take(float);
        if rest.starts_with("_grad") {
            let (dx, dy, min_lod) = (p.take(coord_ty), p.take(coord_ty), p.take(float));
            let offset = (!info.has(CUBE)).then(|| p.take(air.int_ty(2)));
            let at = SampleAt {
                coord,
                array_index,
                offset,
            };
            let grad = Gradient { dx, dy, min_lod };
            return two(air.create_sample_cmp_grad(&tex, handle, sampler, &at, reference, &grad));
        }
        let depth_2d = matches!(tex.kind, TextureKind::Depth2d | TextureKind::Depth2dArray);
        let offset = depth_2d.then(|| p.take(air.int_ty(2)));
        let lod = &Lod::BiasMin(p.take(float), p.take(float));
        let at = SampleAt {
            coord,
            array_index,
            offset,
        };
        return two(air.create_sample_cmp(&tex, handle, sampler, &at, reference, lod));
    }
    if let Some(rest) = op.strip_prefix("sample_") {
        let (tex, rest) = texture_of(rest)?;
        let info = tex.kind.info();
        let coord_ty = air.texture_sample_coord_type(&tex);
        let handle = p.take(air.texture_handle_type(tex.kind));
        let sampler = p.take(air.sampler_handle_type());
        let coord = p.take(coord_ty);
        let array_index = info.has(ARRAY).then(|| p.take(int));
        if rest.starts_with("_grad") {
            let (dx, dy, min_lod) = (p.take(coord_ty), p.take(coord_ty), p.take(float));
            let offset = offset_width(tex.kind, false).map(|n| p.take(air.int_ty(n)));
            let at = SampleAt {
                coord,
                array_index,
                offset,
            };
            let grad = Gradient { dx, dy, min_lod };
            return two(air.create_sample_grad(&tex, handle, sampler, &at, &grad));
        }
        let offset = offset_width(tex.kind, false).map(|n| p.take(air.int_ty(n)));
        let lod = &Lod::BiasMin(p.take(float), p.take(float));
        let at = SampleAt {
            coord,
            array_index,
            offset,
        };
        return two(air.create_sample(&tex, handle, sampler, &at, lod));
    }
    if let Some(rest) = op.strip_prefix("gather_compare_") {
        let (tex, _) = texture_of(rest)?;
        let handle = p.take(air.texture_handle_type(tex.kind));
        let sampler = p.take(air.sampler_handle_type());
        let coord = p.take(air.texture_sample_coord_type(&tex));
        let array_index = tex.kind.info().has(ARRAY).then(|| p.take(int));
        let reference = p.take(float);
        let offset = offset_width(tex.kind, true).map(|n| p.take(air.int_ty(n)));
        let at = SampleAt {
            coord,
            array_index,
            offset,
        };
        return two(air.create_gather_compare(&tex, handle, sampler, &at, reference));
    }
    if let Some(rest) = op.strip_prefix("gather_") {
        let (tex, _) = texture_of(rest)?;
        let info = tex.kind.info();
        let handle = p.take(air.texture_handle_type(tex.kind));
        let sampler = p.take(air.sampler_handle_type());
        let coord = p.take(air.texture_sample_coord_type(&tex));
        let array_index = info.has(ARRAY).then(|| p.take(int));
        let offset = offset_width(tex.kind, true).map(|n| p.take(air.int_ty(n)));
        let component = (!info.has(DEPTH)).then(|| p.take(int));
        let at = SampleAt {
            coord,
            array_index,
            offset,
        };
        return two(air.create_gather(&tex, handle, sampler, &at, component));
    }
    if let Some(rest) = op
        .strip_prefix("read_")
        .or_else(|| op.strip_prefix("write_"))
    {
        let (tex, rest) = texture_of(rest)?;
        let info = tex.kind.info();
        let handle = p.take(air.texture_handle_type(tex.kind));
        let pos = p.take(air.texture_rw_position_type(&tex));
        let face = info.has(CUBE).then(|| p.take(int));
        let array_index = info.has(ARRAY).then(|| p.take(int));
        let coherent = rest.starts_with(".device_coherent");
        if op.starts_with("write_") {
            let value = p.take(air.texel_type(&tex));
            let level = info.has(MIPMAPPED).then(|| p.take(int));
            let at = TexelAt {
                pos,
                array_index,
                sample_or_face: face,
                level,
            };
            return one(air.create_write(&tex, handle, &at, value, coherent));
        }
        let sample = info.has(MS).then(|| p.take(int));
        let level = info.has(MIPMAPPED).then(|| p.take(int));
        let at = TexelAt {
            pos,
            array_index,
            sample_or_face: face.or(sample),
            level,
        };
        return two(air.create_read(&tex, handle, &at, coherent));
    }
    let texture_atomic = op
        .strip_prefix("atomic_exchange_explicit_")
        .map(|rest| ("xchg", rest))
        .or_else(|| op.strip_prefix("atomic_fetch_")?.split_once("_explicit_"));
    if let Some((op_name, rest)) = texture_atomic {
        let (tex, rest) = texture_of(rest)?;
        let handle = p.take(air.texture_handle_type(tex.kind));
        let pos = p.take(air.texture_rw_position_type(&tex));
        let array_index = tex.kind.info().has(ARRAY).then(|| p.take(int));
        let value = p.take(air.texel_type(&tex));
        let atomic = atomic_op_of(op_name, rest.starts_with(".s."));
        return one(air.create_texture_atomic_rmw(&tex, handle, atomic, pos, value, array_index));
    }
    if let Some(rest) = op.strip_prefix("atomic_compare_exchange_weak_explicit_") {
        let (tex, _) = texture_of(rest)?;
        let handle = p.take(air.texture_handle_type(tex.kind));
        let pos = p.take(air.texture_rw_position_type(&tex));
        let array_index = tex.kind.info().has(ARRAY).then(|| p.take(int));
        let texel = air.texel_type(&tex);
        let (cmp, new) = (p.take(texel), p.take(texel));
        return two(air.create_atomic_cmpxchg(&tex, handle, pos, cmp, new, array_index));
    }
    if op == "get_num_samples.i32" {
        return one(air.create_get_num_samples());
    }
    if let Some(rest) = op.strip_prefix("get_") {
        let queries = [
            ("width_", Query::Width),
            ("height_", Query::Height),
            ("depth_", Query::Depth),
            ("array_size_", Query::ArrayLength),
            ("num_mip_levels_", Query::NumMipLevels),
            ("num_samples_", Query::NumSamples),
        ];
        let (query, rest) = queries
            .into_iter()
            .find_map(|(part, query)| Some((query, rest.strip_prefix(part)?)))?;
        let (tex, _) = texture_of(rest)?;
        let sized = matches!(query, Query::Width | Query::Height | Query::Depth);
        let handle = p.take(air.texture_handle_type(tex.kind));
        let level = (sized && tex.kind.info().has(MIPMAPPED)).then(|| p.take(int));
        return one(air.create_texture_query(&tex, handle, query, level));
    }
    if let Some(rest) = op
        .strip_prefix("calculate_clamped_lod_")
        .or_else(|| op.strip_prefix("calculate_unclamped_lod_"))
    {
        let (tex, _) = texture_of(rest)?;
        let handle = p.take(air.texture_handle_type(tex.kind));
        let sampler = p.take(air.sampler_handle_type());
        let coord = p.take(air.texture_sample_coord_type(&tex));
        return two(air.create_calculate_lod(&tex, handle, sampler, coord));
    }
    if let Some(rest) = op.strip_prefix("fence_") {
        let (tex, _) = texture_of(rest)?;
        let handle = p.take(air.texture_handle_type(tex.kind));
        return one(air.create_texture_fence(&tex, handle));
    }
    let first = params.first().copied();
    if op.starts_with("dfdx.") || op.starts_with("dfdy.") {
        return one(air.create_derivative(first?, op.starts_with("dfdy")));
    }
    match op {
        "discard_fragment" => return one(air.create_discard()),
        "wg.barrier" | "simdgroup.barrier" => {
            return one(air.create_barrier(MemFlags::THREADGROUP, op.starts_with("simdgroup")));
        }
        "atomic.fence" => {
            return one(air.create_atomic_fence(MemFlags::DEVICE, ThreadScope::Device, false));
        }
        "set_threadgroups_per_grid_mesh_properties" => {
            return one(air.create_set_mesh_properties(p.take(air.int_ty(3))));
        }
        "set_render_target_array_index_mesh.i32" => {
            let (vertex, index) = (p.take(int), p.take(int));
            return one(air.create_set_mesh_render_target_array_index(vertex, index));
        }
        "set_viewport_array_index_mesh.i32" => {
            let (vertex, index) = (p.take(int), p.take(int));
            return one(air.create_set_mesh_viewport_array_index(vertex, index));
        }
        "set_position_mesh" => {
            let (vertex, position) = (p.take(int), p.take(air.float_ty(4)));
            return one(air.create_set_mesh_position(vertex, position));
        }
        "set_clip_distance_mesh" => {
            let (index, vertex, value) = (p.take(int), p.take(int), p.take(float));
            return one(air.create_set_mesh_clip_distance(vertex, index, value));
        }
        "set_index_mesh" => {
            let (index, vertex) = (p.take(int), p.take(air.byte_ty()));
            return one(air.create_set_mesh_index(index, vertex));
        }
        "set_primitive_count_mesh" => return one(air.create_set_mesh_primitive_count(p.take(int))),
        "set_point_size_mesh" => {
            let (vertex, size) = (p.take(int), p.take(float));
            return one(air.create_set_mesh_point_size(vertex, size));
        }
        _ => {}
    }
    if op.starts_with("set_vertex_data_mesh.") || op.starts_with("set_primitive_data_mesh.") {
        let value_ty = params.last()?.ty();
        let (index, at, value) = (p.take(int), p.take(int), p.take(value_ty));
        return one(if op.starts_with("set_vertex") {
            air.create_set_mesh_vertex_data(at, index, value)
        } else {
            air.create_set_mesh_primitive_data(at, index, value)
        });
    }
    if let Some(rest) = op
        .strip_prefix("atomic.global.")
        .or_else(|| op.strip_prefix("atomic.local."))
    {
        let (op_name, suffix) = rest.split_once('.')?;
        let atomic = atomic_op_of(op_name, suffix.starts_with("s."));
        return one(air.create_atomic_rmw(atomic, first?, *params.get(1)?));
    }
    if let Some(rest) = op.strip_prefix("convert.") {
        let (to_sign, to) = rest
            .get(..2)
            .filter(|s| matches!(*s, "f." | "s." | "u."))
            .map_or(("", rest), |s| (s, &rest[2..]));
        let (to, from) = to.split_once('.')?;
        let v = first?;
        return one(match (to_sign, to.ends_with("f16")) {
            ("s.", _) => air.create_convert_to_signed(v),
            ("u.", _) => air.create_convert_to_unsigned(v),
            (_, true) => air.create_convert_to_half(v, sign_of(from)),
            (_, false) => air.create_convert_to_float(v, sign_of(from)),
        });
    }
    if op.starts_with("load.device_coherent.") {
        return one(air.create_device_coherent_load(result_ty, first?));
    }
    if op.starts_with("store.device_coherent.") {
        return one(air.create_device_coherent_store(first?, *params.get(1)?));
    }
    if let Some(rest) = op.strip_prefix("interpolate_") {
        let perspective = !rest.contains("no_perspective");
        let interpolant = first?;
        return one(if rest.starts_with("center_") {
            air.create_interpolate_at_center(interpolant, perspective)
        } else if rest.starts_with("centroid_") {
            air.create_interpolate_at_centroid(interpolant, perspective)
        } else if rest.starts_with("sample_") {
            air.create_interpolate_at_sample(interpolant, *params.get(1)?, perspective)
        } else {
            air.create_interpolate_at_offset(interpolant, *params.get(1)?, perspective)
        });
    }
    let (base, suffix) = op.split_once('.')?;
    match base {
        "fma" => return one(air.create_fma(first?, *params.get(1)?, *params.get(2)?)),
        "dot" => return one(air.create_dot_product(first?, *params.get(1)?)),
        "ctz" | "clz" => return one(air.create_count_zero(first?, base == "ctz")),
        "reverse_bits" => return one(air.create_int_un_op(IntUnOp::ReverseBits, first?)),
        "popcount" => return one(air.create_int_un_op(IntUnOp::Popcount, first?)),
        "max" | "min" | "mul_hi" => {
            let op = match base {
                "max" => IntBinOp::Max,
                "min" => IntBinOp::Min,
                _ => IntBinOp::MulHi,
            };
            let signed = suffix.starts_with("s.");
            return one(air.create_int_bin_op(op, first?, *params.get(1)?, signed));
        }
        _ => {}
    }
    let (fast, base) = base
        .strip_prefix("fast_")
        .map_or((false, base), |base| (true, base));
    match base {
        "fmax" | "fmin" => {
            let op = if base == "fmax" {
                FpBinOp::Fmax
            } else {
                FpBinOp::Fmin
            };
            one(air.create_fp_bin_op(op, first?, *params.get(1)?, fast))
        }
        _ => one(air.create_fp_un_op(fp_un_op(base)?, first?, fast)),
    }
}

/// Stores `v` to a new external global, so the pipeline keeps the call that made it.
fn sink<'cx>(m: &Module<'cx>, b: &Builder<'cx>, i: usize, v: Val<'cx>) {
    let cx = m.cx();
    let name = CString::new(format!("sink{i}")).unwrap();
    // SAFETY: a live module; `v`'s type lowers to a type of its context; a NUL-terminated name.
    let raw = unsafe { sys::add_global(m.raw(), cx.tcx().lower(v.ty()), name.as_ptr()) };
    // SAFETY: a global of this context, a pointer to `v`'s type in address space 0.
    let global = unsafe { cx.val(raw, cx.tcx().ptr(v.ty(), 0)) };
    b.store(v, global);
}

/// A declaration's text before and after the pass pipeline, `None` where the module has none.
type Made = (Option<String>, Option<String>);

/// The Rust declaration of `line`'s name, before and after the pass pipeline.
///
/// A function whose parameters are the declaration's makes the call `emit` names and keeps its
/// values; `None` when no method makes the name, an error when the method refuses.
fn produce(line: &str) -> Option<Result<Made, Error>> {
    let decl = Decl::parse(line);
    let cx = Context::new();
    let m = module::initialize(&cx, c"declares");
    let tcx = cx.tcx();
    let types = AirTypes::new(tcx);
    let ret = parse_ty(tcx, decl.ret).0;
    let params: Vec<Ty> = decl.params.iter().map(|p| parse_ty(tcx, p).0).collect();
    let f = ok(m.add_function(b"t", tcx.function(tcx.void(), &params, false)));
    let b = Builder::new(&cx);
    b.position_at_end(f.append_block(&cx, c"entry").unwrap());
    let args: Vec<Val<'_>> = (0..params.len())
        .map(|i| f.param(tcx, u32::try_from(i).unwrap()).unwrap())
        .collect();
    let air = AirBuilder::new(&b, &m, &types, 0);
    let values = match emit(&air, decl.name, &args, ret)? {
        Ok(values) => values,
        Err(e) => return Some(Err(e)),
    };
    for (i, &v) in values.iter().enumerate() {
        sink(&m, &b, i, v);
    }
    b.ret_void();
    let text =
        |m: &Module<'_>| declarations(&String::from_utf8_lossy(&m.print())).remove(decl.name);
    let before = text(&m);
    module::optimize(&m);
    Some(Ok((before, text(&m))))
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

/// Every `declare … @air.*` of the C++ baseline that `AIRBuilder` made is the Rust declarer's.
///
/// Each line is rebuilt from its own operand types: before the pass pipeline when the baseline's
/// lacks `local_unnamed_addr` (declared after the pipeline's `GlobalOpt`, as the barrier pass
/// does), after it otherwise (the pipeline adds `local_unnamed_addr`, `mustprogress`, `nofree`
/// and `nosync`). A line whose name no method makes (with these operands) must come from another
/// emitter: a Metal helper, `air_operations.cpp`, `dxil_lower.cpp`; a line the declarer makes with
/// other text fails, whoever else declares the name.
#[test]
fn declares_match_baseline() {
    let Some(base) = base("the baseline declarations are not compared") else {
        return;
    };
    let helpers = helper_names();
    let mut matched = 0;
    let mut elsewhere = BTreeSet::new();
    let mut failures = Vec::new();
    for (line, file) in baseline_declarations(&base) {
        let name = Decl::parse(&line).name.to_owned();
        let from_elsewhere =
            helpers.contains(&name) || OTHER_EMITTERS.iter().any(|prefix| name.starts_with(prefix));
        let made = match produce(&line) {
            None => None,
            Some(Err(e)) => {
                let text = String::from_utf8_lossy(e.text()).into_owned();
                failures.push(format!("{line}\n  refused: {text} ({})", file.display()));
                continue;
            }
            Some(Ok((before, after))) => {
                if line.contains(" local_unnamed_addr ") {
                    after
                } else {
                    before
                }
            }
        };
        if made.as_deref() == Some(line.as_str()) {
            matched += 1;
        } else if made.is_none() && from_elsewhere {
            elsewhere.insert(name);
        } else {
            failures.push(format!(
                "{line}\n  made: {}\n  ({})",
                made.as_deref().unwrap_or("nothing"),
                file.display()
            ));
        }
    }
    eprintln!(
        "declarations: {matched} made by the Rust declarer, {} from other emitters: {elsewhere:?}",
        elsewhere.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(matched > 0);
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
