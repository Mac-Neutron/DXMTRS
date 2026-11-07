use core::{
    ffi::{c_char, c_void},
    ptr,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::sync::Mutex;

use dxmt_util::sync::lock;
use winemetal_sys::{
    Obj,
    sm50::{
        MtlShaderReflection, MtlShaderReflectionUnion, SM50_INDEX_BUFFER_FORMAT_UINT16,
        SM50_SHADER_COMMON, SM50_SHADER_FLAG_DEFUSE_FMA, SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO,
        SM50_SHADER_IA_INPUT_LAYOUT, SM50_SHADER_METAL_310, SM50_SHADER_PSO_GEOMETRY_SHADER,
        SM50_SHADER_PSO_PIXEL_SHADER, SM50_SHADER_PSO_TESSELLATOR, SM50_SHADER_ROOT_SIGNATURE,
        Sm50CompiledBitcode, Sm50IaInputElement, Sm50ShaderCommonData,
        Sm50ShaderCompilationArgumentData, Sm50ShaderIaInputLayoutData,
        Sm50ShaderPsoGeometryShaderData, Sm50ShaderPsoPixelShaderData,
        Sm50ShaderPsoTessellatorData, Sm50ShaderRootSignatureData,
    },
};

use super::{
    CachedShader, CompileEnv, FunctionKind, Sha1Digest, ShaderFunctionCache,
    compile_function_with_env, decode_reflection, encode_reflection, function_key_from_digests,
    hash_compile_args, reflection_key, sha1,
};

static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());
static CACHE_HIT: AtomicBool = AtomicBool::new(false);
static READER_GET_CALLS: AtomicUsize = AtomicUsize::new(0);
static WRITER_SET_CALLS: AtomicUsize = AtomicUsize::new(0);
static SM50_COMPILE_CALLS: AtomicUsize = AtomicUsize::new(0);
static NEW_LIBRARY_CALLS: AtomicUsize = AtomicUsize::new(0);

#[unsafe(export_name = "CacheWriter_alloc_init")]
extern "C-unwind" fn fake_cache_writer_alloc_init(_path: *const c_char, _version: u64) -> u64 {
    0x7001
}

#[unsafe(export_name = "CacheReader_alloc_init")]
extern "C-unwind" fn fake_cache_reader_alloc_init(_path: *const c_char, _version: u64) -> u64 {
    0x7002
}

#[unsafe(export_name = "CacheReader_get")]
extern "C-unwind" fn fake_cache_reader_get(_reader: u64, _key: *const c_void, _length: u64) -> u64 {
    READER_GET_CALLS.fetch_add(1, Ordering::Relaxed);
    if CACHE_HIT.load(Ordering::Relaxed) {
        0x7003
    } else {
        0
    }
}

#[unsafe(export_name = "CacheWriter_set")]
extern "C-unwind" fn fake_cache_writer_set(
    _writer: u64,
    _key: *const c_void,
    _length: u64,
    _value: u64,
) {
    WRITER_SET_CALLS.fetch_add(1, Ordering::Relaxed);
}

#[unsafe(export_name = "DispatchData_alloc_init")]
extern "C-unwind" fn fake_dispatch_data_alloc_init(_native_ptr: u64, _length: u64) -> u64 {
    0x7004
}

#[unsafe(export_name = "DispatchData_copyBytes")]
extern "C-unwind" fn fake_dispatch_data_copy_bytes(
    _data: u64,
    buffer: *mut c_void,
    capacity: u64,
) -> u64 {
    let Ok(cap) = usize::try_from(capacity) else {
        return 0;
    };
    if cap == GOLDEN_REFLECTION_BYTES.len() && !buffer.is_null() {
        // SAFETY: `buffer` is writable for `cap` bytes.
        unsafe {
            ptr::copy_nonoverlapping(GOLDEN_REFLECTION_BYTES.as_ptr(), buffer.cast::<u8>(), cap);
        }
        return capacity;
    }
    0
}

#[unsafe(export_name = "MTLDevice_newLibrary")]
extern "C-unwind" fn fake_new_library(_device: u64, _data: u64, err_out: *mut u64) -> u64 {
    NEW_LIBRARY_CALLS.fetch_add(1, Ordering::Relaxed);
    if !err_out.is_null() {
        // SAFETY: `err_out` is a valid writable `u64` pointer when non-null.
        unsafe { *err_out = 0 };
    }
    0x7005
}

#[unsafe(export_name = "MTLLibrary_newFunction")]
extern "C-unwind" fn fake_new_function(_library: u64, _name: *const c_char) -> u64 {
    0x7006
}

#[unsafe(export_name = "SM50Initialize")]
extern "C-unwind" fn fake_sm50_initialize(
    _bytecode: *const c_void,
    _bytecode_size: usize,
    shader: *mut u64,
    refl: *mut MtlShaderReflection,
    error: *mut u64,
) -> i32 {
    if !shader.is_null() {
        // SAFETY: `shader` is a valid out-pointer.
        unsafe { *shader = 0x8001 };
    }
    if !refl.is_null() {
        // SAFETY: `refl` is a valid out-pointer.
        unsafe { *refl = golden_reflection() };
    }
    if !error.is_null() {
        // SAFETY: `error` is a valid out-pointer.
        unsafe { *error = 0 };
    }
    0
}

#[unsafe(export_name = "SM50Destroy")]
extern "C-unwind" fn fake_sm50_destroy(_shader: u64) {}

#[unsafe(export_name = "SM50Compile")]
extern "C-unwind" fn fake_sm50_compile(
    _shader: u64,
    _args: *mut Sm50ShaderCompilationArgumentData,
    _function_name: *const c_char,
    bitcode: *mut u64,
    error: *mut u64,
) -> i32 {
    SM50_COMPILE_CALLS.fetch_add(1, Ordering::Relaxed);
    if !bitcode.is_null() {
        // SAFETY: `bitcode` is a valid out-pointer.
        unsafe { *bitcode = 0x8002 };
    }
    if !error.is_null() {
        // SAFETY: `error` is a valid out-pointer.
        unsafe { *error = 0 };
    }
    0
}

#[unsafe(export_name = "SM50CompileGeometryPipelineVertex")]
extern "C-unwind" fn fake_sm50_compile_gs_vs(
    vs: u64,
    _gs: u64,
    args: *mut Sm50ShaderCompilationArgumentData,
    function_name: *const c_char,
    bitcode: *mut u64,
    error: *mut u64,
) -> i32 {
    fake_sm50_compile(vs, args, function_name, bitcode, error)
}

#[unsafe(export_name = "SM50CompileGeometryPipelineGeometry")]
extern "C-unwind" fn fake_sm50_compile_gs_gs(
    _vs: u64,
    gs: u64,
    args: *mut Sm50ShaderCompilationArgumentData,
    function_name: *const c_char,
    bitcode: *mut u64,
    error: *mut u64,
) -> i32 {
    fake_sm50_compile(gs, args, function_name, bitcode, error)
}

#[unsafe(export_name = "SM50GetCompiledBitcode")]
extern "C-unwind" fn fake_sm50_get_compiled_bitcode(_bitcode: u64, data: *mut Sm50CompiledBitcode) {
    if !data.is_null() {
        // SAFETY: `data` is a valid out-pointer.
        unsafe {
            *data = Sm50CompiledBitcode {
                data: 0x1000,
                size: 16,
            };
        }
    }
}

#[unsafe(export_name = "SM50DestroyBitcode")]
extern "C-unwind" fn fake_sm50_destroy_bitcode(_bitcode: u64) {}

#[unsafe(export_name = "SM50GetErrorMessage")]
extern "C-unwind" fn fake_sm50_get_error_message(
    error: u64,
    buffer: *mut c_char,
    buffer_size: usize,
) -> usize {
    let msg: &[u8] = if error == 2 {
        b"DXIL: unsupported instruction"
    } else {
        b"compile error"
    };
    let n = msg.len().min(buffer_size);
    if !buffer.is_null() && n > 0 {
        // SAFETY: `buffer` has at least `n` writable bytes.
        unsafe { ptr::copy_nonoverlapping(msg.as_ptr(), buffer.cast::<u8>(), n) };
    }
    n
}

#[unsafe(export_name = "SM50FreeError")]
extern "C-unwind" fn fake_sm50_free_error(_error: u64) {}

/// Recorded from `d3d12_shader_cache.cpp::HashCompileArgs` over the 5-node chain below.
const GOLDEN_HASH_COMPILE_ARGS: Sha1Digest = [
    0xc3, 0xd0, 0x73, 0xae, 0xea, 0x25, 0xc4, 0xb6, 0x10, 0x79, 0x21, 0x22, 0x74, 0x61, 0x9f, 0xb4,
    0xa3, 0xd9, 0x0f, 0x8f,
];

/// Recorded from `Sha1HashState::compute("first-shader-dxbc", 17)`.
const GOLDEN_SHADER_1_DIGEST: Sha1Digest = [
    0xd2, 0xb8, 0x15, 0xbe, 0xa3, 0x39, 0x04, 0x99, 0x70, 0x20, 0x1e, 0x34, 0x2b, 0x42, 0x4c, 0x17,
    0x03, 0xf1, 0x05, 0x00,
];

/// Recorded from `Sha1HashState::compute("second-shader-dxbc", 18)`.
const GOLDEN_SHADER_2_DIGEST: Sha1Digest = [
    0x23, 0xcd, 0x1b, 0xfa, 0x8f, 0xeb, 0x24, 0x23, 0xa9, 0xd6, 0x82, 0x6f, 0xd8, 0xa9, 0x86, 0x99,
    0xa1, 0xb1, 0xe7, 0x11,
];

/// Recorded from `d3d12_shader_cache.cpp::CompileFunction`'s key calculation (`second = null`,
///
/// `name = "vertex_main"`, `DXMT_DXIL_BOUNDS` empty).
const GOLDEN_FUNCTION_KEY_SINGLE: [u8; 40] = [
    0x10, 0xab, 0x06, 0x4f, 0xcc, 0x4b, 0xac, 0x2c, 0x71, 0x03, 0xac, 0x0a, 0x9a, 0xa4, 0x94, 0x5e,
    0xa5, 0x61, 0xf4, 0x02, 0xe6, 0xe6, 0x0e, 0x42, 0x71, 0x2a, 0x0b, 0x11, 0x5c, 0xab, 0xda, 0x32,
    0xec, 0xcb, 0x27, 0x0f, 0x00, 0xb4, 0x81, 0xc2,
];

/// Recorded from `d3d12_shader_cache.cpp::CompileFunction`'s key calculation (`second = &d2`,
///
/// `name = "geometry_main"`, `DXMT_DXIL_BOUNDS = "check"`).
const GOLDEN_FUNCTION_KEY_PAIR_BOUNDS: [u8; 40] = [
    0x05, 0xef, 0x44, 0x01, 0x52, 0x03, 0xe5, 0xc7, 0xa5, 0xb3, 0x87, 0xe5, 0xa0, 0xf2, 0xb8, 0x64,
    0xf4, 0x48, 0xc3, 0x8c, 0xb3, 0x88, 0xee, 0x83, 0xde, 0x4b, 0xed, 0x3c, 0x34, 0xd5, 0x38, 0xea,
    0xb4, 0xbb, 0xef, 0xb2, 0xc1, 0x87, 0x7f, 0x7d,
];

/// Recorded from `d3d12_shader_cache.cpp::CachedShader::Reflection` (`d3d12-reflection 2`).
const GOLDEN_REFLECTION_KEY: [u8; 40] = [
    0xd2, 0xb8, 0x15, 0xbe, 0xa3, 0x39, 0x04, 0x99, 0x70, 0x20, 0x1e, 0x34, 0x2b, 0x42, 0x4c, 0x17,
    0x03, 0xf1, 0x05, 0x00, 0x4b, 0xda, 0xec, 0x80, 0x89, 0xdb, 0xa5, 0x51, 0x3a, 0xfa, 0x7a, 0x46,
    0x8b, 0x81, 0x46, 0xca, 0xcb, 0x32, 0xb0, 0x11,
];

/// Recorded 72-byte `MTL_SHADER_REFLECTION` record from C++.
const GOLDEN_REFLECTION_BYTES: [u8; 72] = [
    0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00,
    0x08, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x07, 0x00, 0x03, 0x00,
    0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11,
    0x28, 0x27, 0x26, 0x25, 0x24, 0x23, 0x22, 0x21, 0x05, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00,
    0x0e, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
];

const fn golden_reflection() -> MtlShaderReflection {
    MtlShaderReflection {
        constantt_buffer_table_bind_index: 1,
        argument_buffer_bind_index: 2,
        num_constant_buffers: 3,
        num_arguments: 4,
        u: MtlShaderReflectionUnion {
            threadgroup_size: [8, 8, 1],
        },
        constant_buffer_slot_mask: 7,
        sampler_slot_mask: 3,
        uav_slot_mask: 0x0102_0304_0506_0708,
        srv_slot_mask_hi: 0x1112_1314_1516_1718,
        srv_slot_mask_lo: 0x2122_2324_2526_2728,
        num_output_element: 5,
        threads_per_patch: 32,
        argument_table_qwords: 14,
        writes_uav: 1,
    }
}

#[test]
fn hash_compile_args_is_the_golden() {
    let mut gs = Sm50ShaderPsoGeometryShaderData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_PSO_GEOMETRY_SHADER,
        strip_topology: 1,
    };
    let mut ps = Sm50ShaderPsoPixelShaderData {
        next: (&raw mut gs).cast(),
        r#type: SM50_SHADER_PSO_PIXEL_SHADER,
        sample_mask: 0xffff_ffff,
        dual_source_blending: 1,
        disable_depth_output: 0,
        unorm_output_reg_mask: 0x03,
        pixel_formats: [70, 80, 115, 0, 0, 0, 0, 0],
    };
    let mut elems = [
        Sm50IaInputElement::new(0, 0, 0, 30, 0, 0),
        Sm50IaInputElement::new(3, 2, 12, 29, 1, 4),
    ];
    let mut ia = Sm50ShaderIaInputLayoutData {
        next: (&raw mut ps).cast(),
        r#type: SM50_SHADER_IA_INPUT_LAYOUT,
        index_buffer_format: SM50_INDEX_BUFFER_FORMAT_UINT16,
        slot_mask: 0x5,
        num_elements: 2,
        elements: elems.as_mut_ptr(),
    };
    let rs_bytes: [u8; 8] = [0x44, 0x58, 0x42, 0x43, 0x11, 0x22, 0x33, 0x44];
    let mut rs = Sm50ShaderRootSignatureData {
        next: (&raw mut ia).cast(),
        r#type: SM50_SHADER_ROOT_SIGNATURE,
        bytecode: rs_bytes.as_ptr().cast(),
        bytecode_length: rs_bytes.len(),
    };
    let common = Sm50ShaderCommonData {
        next: (&raw mut rs).cast(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: SM50_SHADER_FLAG_SAMPLE_NAN_TO_ZERO | SM50_SHADER_FLAG_DEFUSE_FMA,
    };

    // SAFETY: `common` heads a valid 5-node chain whose pointers remain live for the call.
    let got = unsafe { hash_compile_args((&raw const common).cast()) };
    assert_eq!(got, Some(GOLDEN_HASH_COMPILE_ARGS));
}

#[test]
fn unknown_kind_gives_up() {
    let known = Sm50ShaderCommonData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };
    // SAFETY: `known` is a valid 1-node chain.
    assert!(unsafe { hash_compile_args((&raw const known).cast()) }.is_some());

    let mut tess = Sm50ShaderPsoTessellatorData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_PSO_TESSELLATOR,
        max_potential_tess_factor: 16,
    };
    let common = Sm50ShaderCommonData {
        next: (&raw mut tess).cast(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };

    // SAFETY: `common` heads a valid 2-node chain ending in an unknown argument type (`PsoTessellator`).
    let got = unsafe { hash_compile_args((&raw const common).cast()) };
    assert_eq!(got, None);
}

#[test]
fn function_key_is_the_golden() {
    let d1 = sha1(b"first-shader-dxbc");
    let d2 = sha1(b"second-shader-dxbc");
    assert_eq!(d1, GOLDEN_SHADER_1_DIGEST);
    assert_eq!(d2, GOLDEN_SHADER_2_DIGEST);

    let single = function_key_from_digests(&d1, None, "vertex_main", &GOLDEN_HASH_COMPILE_ARGS, "");
    assert_eq!(single, GOLDEN_FUNCTION_KEY_SINGLE);

    let pair = function_key_from_digests(
        &d1,
        Some(&d2),
        "geometry_main",
        &GOLDEN_HASH_COMPILE_ARGS,
        "check",
    );
    assert_eq!(pair, GOLDEN_FUNCTION_KEY_PAIR_BOUNDS);
}

#[test]
fn reflection_record_is_the_golden() {
    let key = reflection_key(&GOLDEN_SHADER_1_DIGEST);
    assert_eq!(key, GOLDEN_REFLECTION_KEY);

    let refl = golden_reflection();
    let encoded = encode_reflection(&refl);
    assert_eq!(encoded, GOLDEN_REFLECTION_BYTES);

    let decoded = decode_reflection(&GOLDEN_REFLECTION_BYTES).expect("72-byte reflection decodes");
    assert_eq!(encode_reflection(&decoded), GOLDEN_REFLECTION_BYTES);
}

#[test]
fn probe_bypasses_the_cache() {
    let _guard = lock(&CACHE_TEST_LOCK);
    dxmt_core::shader_cache::close_all();

    CACHE_HIT.store(true, Ordering::Relaxed);
    READER_GET_CALLS.store(0, Ordering::Relaxed);
    WRITER_SET_CALLS.store(0, Ordering::Relaxed);
    SM50_COMPILE_CALLS.store(0, Ordering::Relaxed);
    NEW_LIBRARY_CALLS.store(0, Ordering::Relaxed);

    let mut common = Sm50ShaderCommonData {
        next: ptr::null_mut(),
        r#type: SM50_SHADER_COMMON,
        metal_version: SM50_SHADER_METAL_310,
        flags: 0,
    };
    // SAFETY: non-zero fake Metal device handle.
    let device = unsafe { Obj::adopt(0x6001) }.expect("fake device");
    let mut shader = CachedShader {
        // Opens with `DXBC`, as `dxmt_core::shader::initialize` requires; the digest is separate.
        bytecode: b"DXBCfirst-shader-dxbc".to_vec(),
        digest: GOLDEN_SHADER_1_DIGEST,
        shader: None,
        reflected: false,
        reflection: golden_reflection(),
    };

    // With `probe = true` (`DXMT_PROBE` active), the cache is neither read nor written: airconv
    // translates the shader directly and returns the compiled Metal function.
    // SAFETY: `common` is a valid 1-node argument chain.
    let probed_fn = unsafe {
        compile_function_with_env(
            &device,
            None,
            FunctionKind::Shader,
            &mut shader,
            None,
            (&raw mut common).cast(),
            &CompileEnv {
                name: "vertex_main",
                stage: "vs",
                probe: true,
                bounds: "",
            },
        )
    }
    .expect("probe compilation succeeds");
    assert_eq!(probed_fn.handle(), 0x7006);
    assert_eq!(READER_GET_CALLS.load(Ordering::Relaxed), 0);
    assert_eq!(WRITER_SET_CALLS.load(Ordering::Relaxed), 0);
    assert_eq!(SM50_COMPILE_CALLS.load(Ordering::Relaxed), 1);
    assert_eq!(NEW_LIBRARY_CALLS.load(Ordering::Relaxed), 1);

    // With `probe = false` and `CACHE_HIT = true`, the cached library is used without calling
    // `SM50Compile` again.
    // SAFETY: `common` is a valid 1-node argument chain.
    let cached_fn = unsafe {
        compile_function_with_env(
            &device,
            None,
            FunctionKind::Shader,
            &mut shader,
            None,
            (&raw mut common).cast(),
            &CompileEnv {
                name: "vertex_main",
                stage: "vs",
                probe: false,
                bounds: "",
            },
        )
    }
    .expect("cached compilation succeeds");
    assert_eq!(cached_fn.handle(), 0x7006);
    assert_eq!(READER_GET_CALLS.load(Ordering::Relaxed), 1);
    assert_eq!(WRITER_SET_CALLS.load(Ordering::Relaxed), 0);
    assert_eq!(SM50_COMPILE_CALLS.load(Ordering::Relaxed), 1);
    assert_eq!(NEW_LIBRARY_CALLS.load(Ordering::Relaxed), 2);
}

#[test]
fn function_cache_retains_a_live_function() {
    let cache = ShaderFunctionCache::new();
    let key = [0x5a; 40];
    assert!(cache.get(&key).is_none());
    // SAFETY: fake retained object handle; the test fakes `NSObject_retain`/`_release`.
    let function = unsafe { Obj::adopt(0x9100) }.expect("non-null");
    cache.insert(key, &function);
    let cached = cache.get(&key).expect("cached");
    assert_eq!(cached.handle(), 0x9100);
}
