use core::{cell::RefCell, ffi::c_void, ptr};

use winemetal_sys::sm50::MtlShaderReflection;

use super::{SM50_SHADER_D3D9, initialize, initialize_dxso};

/// The handle the fake `SM50Initialize` gives.
const SHADER: u64 = 0x5e50;

thread_local! {
    /// What this thread's `SM50Initialize` calls read.
    ///
    /// The bytes, and whether a reflection was asked for.
    static INITIALIZED: RefCell<Vec<(Vec<u8>, bool)>> = const { RefCell::new(Vec::new()) };
}

#[unsafe(export_name = "SM50Initialize")]
extern "C-unwind" fn fake_initialize(
    bytecode: *const c_void,
    size: usize,
    shader: *mut u64,
    reflection: *mut MtlShaderReflection,
    _error: *mut u64,
) -> i32 {
    // SAFETY: the caller's `size` bytes.
    let bytes = unsafe { core::slice::from_raw_parts(bytecode.cast::<u8>(), size) }.to_vec();
    INITIALIZED.with_borrow_mut(|calls| calls.push((bytes, !reflection.is_null())));
    // SAFETY: the caller's out pointer, writable.
    unsafe { shader.write(SHADER) };
    0
}

#[unsafe(export_name = "SM50Destroy")]
const extern "C-unwind" fn fake_destroy(_shader: u64) {}

/// `vs_2_0` then the end token.
fn vs_2_0() -> Vec<u8> {
    [0xfffe_0200_u32, 0x0000_ffff]
        .iter()
        .flat_map(|t| t.to_le_bytes())
        .collect()
}

#[test]
fn refuses_a_dxso_token_stream() {
    // SAFETY: the reflection is plain integers, valid all zero.
    let mut reflection: MtlShaderReflection = unsafe { core::mem::zeroed() };
    // The refusal comes before `SM50Initialize`, which would read the token stream.
    assert!(matches!(
        initialize(&vs_2_0(), &mut reflection),
        Err(super::Sm50Error(0, Some(_)))
    ));
    assert!(matches!(
        initialize(b"DXB", &mut reflection),
        Err(super::Sm50Error(0, Some(_)))
    ));
    assert_eq!(INITIALIZED.take(), []);
    let Err(error) = initialize(b"DXB", &mut reflection) else {
        panic!("initialized");
    };
    assert_eq!(error.message(), "not a DXBC container");
}

#[test]
fn another_handle_zero_refusal_is_not_called_a_dxbc_problem() {
    assert_eq!(
        super::Sm50Error(0, None).message(),
        "no airconv error object"
    );
}

/// A DXBC container never reaches airconv through D3D9's entry.
#[test]
fn initialize_dxso_refuses_dxbc() {
    let Err(error) = initialize_dxso(b"DXBC\0\0\0\0", (0, [0; 6], [0; 22])) else {
        panic!("initialized");
    };
    assert_eq!(error.message(), "a DXBC container, not a D3D9 token stream");
    assert!(
        INITIALIZED.take().is_empty(),
        "SM50Initialize was not called"
    );
}

/// The tokens reach `SM50Initialize`, and the D3D9 node carries the stage's specialization.
///
/// No reflection is asked for: a DXSO shader writes none.
#[test]
fn initialize_dxso_adds_the_d3d9_argument() {
    let (vs, ps) = ([1, 2, 3, 4, 5, 6], [7; 22]);
    let Ok((shader, d3d9)) = initialize_dxso(&vs_2_0(), (1, vs, ps)) else {
        panic!("refused");
    };
    assert_eq!(shader.0, SHADER);
    assert_eq!(INITIALIZED.take(), [(vs_2_0(), false)]);
    assert_eq!(d3d9.r#type, SM50_SHADER_D3D9);
    assert_eq!((d3d9.stage, d3d9.vs, d3d9.ps), (1, vs, ps));
    assert_eq!(d3d9.next, ptr::null_mut());
}
