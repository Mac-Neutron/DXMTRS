//! Forward a notification into Wine's application delegate with Objective-C exceptions caught.
//!
//! Calls `objc_msgSend` inside an assembly frame (`_wmt_try_send`) carrying an Objective-C
//! personality landing pad so any exception thrown by Wine's delegate is caught and returned as
//! a retained object rather than unwinding through Rust frames.

use objc2::{
    rc::Retained,
    runtime::{AnyObject, NSObjectProtocol, ProtocolObject, Sel},
};
use objc2_foundation::{NSException, NSNotification};

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".section __TEXT,__text,regular,pure_instructions",
    ".private_extern _wmt_try_send",
    ".p2align 2",
    "_wmt_try_send:",
    "Lwmt_try_begin:",
    ".cfi_startproc",
    ".cfi_personality 155, ___objc_personality_v0",
    ".cfi_lsda 16, Lwmt_try_table",
    "stp x20, x19, [sp, #-32]!",
    "stp x29, x30, [sp, #16]",
    "add x29, sp, #16",
    ".cfi_def_cfa w29, 16",
    ".cfi_offset w30, -8",
    ".cfi_offset w29, -16",
    ".cfi_offset w19, -24",
    ".cfi_offset w20, -32",
    "Lwmt_try_send:",
    "bl _objc_msgSend",
    "Lwmt_try_sent:",
    "mov x0, #0",
    "ldp x29, x30, [sp, #16]",
    "ldp x20, x19, [sp], #32",
    "ret",
    "Lwmt_try_caught:",
    "mov x19, x0",
    "cmp w1, #1",
    "b.ne Lwmt_try_resume",
    "mov x0, x19",
    "bl _objc_begin_catch",
    "Lwmt_try_retain:",
    "bl _objc_retain",
    "Lwmt_try_retained:",
    "mov x19, x0",
    "bl _objc_end_catch",
    "mov x0, x19",
    "ldp x29, x30, [sp, #16]",
    "ldp x20, x19, [sp], #32",
    "ret",
    "Lwmt_try_cleanup:",
    "mov x19, x0",
    "bl _objc_end_catch",
    "Lwmt_try_resume:",
    "mov x0, x19",
    "bl __Unwind_Resume",
    "Lwmt_try_end:",
    ".cfi_endproc",
    ".section __TEXT,__gcc_except_tab",
    ".p2align 2, 0x0",
    "wmt_try_send_table:",
    "Lwmt_try_table:",
    ".byte 255",
    ".byte 155",
    ".uleb128 Lwmt_try_types_end-Lwmt_try_types_ref",
    "Lwmt_try_types_ref:",
    ".byte 1",
    ".uleb128 Lwmt_try_sites_end-Lwmt_try_sites",
    "Lwmt_try_sites:",
    ".uleb128 Lwmt_try_send-Lwmt_try_begin",
    ".uleb128 Lwmt_try_sent-Lwmt_try_send",
    ".uleb128 Lwmt_try_caught-Lwmt_try_begin",
    ".byte 1",
    ".uleb128 Lwmt_try_sent-Lwmt_try_begin",
    ".uleb128 Lwmt_try_retain-Lwmt_try_sent",
    ".byte 0",
    ".byte 0",
    ".uleb128 Lwmt_try_retain-Lwmt_try_begin",
    ".uleb128 Lwmt_try_retained-Lwmt_try_retain",
    ".uleb128 Lwmt_try_cleanup-Lwmt_try_begin",
    ".byte 0",
    ".uleb128 Lwmt_try_retained-Lwmt_try_begin",
    ".uleb128 Lwmt_try_end-Lwmt_try_retained",
    ".byte 0",
    ".byte 0",
    "Lwmt_try_sites_end:",
    ".byte 1",
    ".byte 0",
    ".p2align 2, 0x0",
    "Lwmt_try_type_id:",
    ".long _OBJC_EHTYPE_id@GOT-Lwmt_try_type_id",
    "Lwmt_try_types_end:",
    ".p2align 2, 0x0",
    ".section __TEXT,__text,regular,pure_instructions",
);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".section __TEXT,__text,regular,pure_instructions",
    ".private_extern _wmt_try_send",
    ".p2align 4",
    "_wmt_try_send:",
    "Lwmt_try_begin:",
    ".cfi_startproc",
    ".cfi_personality 155, ___objc_personality_v0",
    ".cfi_lsda 16, Lwmt_try_table",
    "pushq %rbp",
    ".cfi_def_cfa_offset 16",
    ".cfi_offset %rbp, -16",
    "movq %rsp, %rbp",
    ".cfi_def_cfa_register %rbp",
    "pushq %rbx",
    "pushq %rax",
    ".cfi_offset %rbx, -24",
    "Lwmt_try_send:",
    "callq _objc_msgSend",
    "Lwmt_try_sent:",
    "xorl %eax, %eax",
    "addq $8, %rsp",
    "popq %rbx",
    "popq %rbp",
    "retq",
    "Lwmt_try_caught:",
    "movq %rax, %rbx",
    "cmpl $1, %edx",
    "jne Lwmt_try_resume",
    "movq %rbx, %rdi",
    "callq _objc_begin_catch",
    "Lwmt_try_retain:",
    "movq %rax, %rdi",
    "callq *_objc_retain@GOTPCREL(%rip)",
    "Lwmt_try_retained:",
    "movq %rax, %rbx",
    "callq _objc_end_catch",
    "movq %rbx, %rax",
    "addq $8, %rsp",
    "popq %rbx",
    "popq %rbp",
    "retq",
    "Lwmt_try_cleanup:",
    "movq %rax, %rbx",
    "callq _objc_end_catch",
    "Lwmt_try_resume:",
    "movq %rbx, %rdi",
    "callq __Unwind_Resume",
    "Lwmt_try_end:",
    ".cfi_endproc",
    ".section __TEXT,__gcc_except_tab",
    ".p2align 2, 0x0",
    "wmt_try_send_table:",
    "Lwmt_try_table:",
    ".byte 255",
    ".byte 155",
    ".uleb128 Lwmt_try_types_end-Lwmt_try_types_ref",
    "Lwmt_try_types_ref:",
    ".byte 1",
    ".uleb128 Lwmt_try_sites_end-Lwmt_try_sites",
    "Lwmt_try_sites:",
    ".uleb128 Lwmt_try_send-Lwmt_try_begin",
    ".uleb128 Lwmt_try_sent-Lwmt_try_send",
    ".uleb128 Lwmt_try_caught-Lwmt_try_begin",
    ".byte 1",
    ".uleb128 Lwmt_try_sent-Lwmt_try_begin",
    ".uleb128 Lwmt_try_retain-Lwmt_try_sent",
    ".byte 0",
    ".byte 0",
    ".uleb128 Lwmt_try_retain-Lwmt_try_begin",
    ".uleb128 Lwmt_try_retained-Lwmt_try_retain",
    ".uleb128 Lwmt_try_cleanup-Lwmt_try_begin",
    ".byte 0",
    ".uleb128 Lwmt_try_retained-Lwmt_try_begin",
    ".uleb128 Lwmt_try_end-Lwmt_try_retained",
    ".byte 0",
    ".byte 0",
    "Lwmt_try_sites_end:",
    ".byte 1",
    ".byte 0",
    ".p2align 2, 0x0",
    ".long _OBJC_EHTYPE_id@GOTPCREL+4",
    "Lwmt_try_types_end:",
    ".p2align 2, 0x0",
    ".section __TEXT,__text,regular,pure_instructions",
    options(att_syntax),
);

unsafe extern "C" {
    fn wmt_try_send(
        receiver: *const AnyObject,
        selector: Sel,
        argument: *const AnyObject,
    ) -> *mut AnyObject;
}

fn screen_parameters_selector() -> Sel {
    Sel::register(c"applicationDidChangeScreenParameters:")
}

/// Whether `delegate` implements `applicationDidChangeScreenParameters:`.
#[must_use]
pub fn delegate_handles_screen_parameters(delegate: &AnyObject) -> bool {
    // SAFETY: `ProtocolObject` is a transparent wrapper over `AnyObject` and every NSApp delegate
    // conforms to `NSObjectProtocol`.
    let object =
        unsafe { &*core::ptr::from_ref(delegate).cast::<ProtocolObject<dyn NSObjectProtocol>>() };
    object.respondsToSelector(screen_parameters_selector())
}

/// Send `applicationDidChangeScreenParameters:` to `delegate`, catching any Objective-C exception.
///
/// # Errors
///
/// Returns a description of any caught Objective-C exception object.
///
/// # Safety
///
/// `delegate` must be messaged on the main thread, and `notification` must be the notification
/// currently being delivered.
pub unsafe fn forward_screen_parameters(
    delegate: &AnyObject,
    notification: &NSNotification,
) -> Result<(), String> {
    let notification = core::ptr::from_ref(notification).cast::<AnyObject>();
    // SAFETY: caller ensures main-thread delivery and live references; `_wmt_try_send` catches
    // any Objective-C exception thrown by the selector and returns it retained.
    let thrown = unsafe { wmt_try_send(delegate, screen_parameters_selector(), notification) };
    // SAFETY: non-null return from `_wmt_try_send` is retained +1 for the caller.
    let Some(thrown) = (unsafe { Retained::from_raw(thrown) }) else {
        return Ok(());
    };
    Err(describe_exception(thrown))
}

fn describe_exception(thrown: Retained<AnyObject>) -> String {
    match thrown.downcast::<NSException>() {
        Ok(exception) => {
            let reason = exception.reason();
            format!(
                "{}: {}",
                exception.name(),
                reason
                    .as_deref()
                    .map_or_else(|| "(no reason)".to_owned(), ToString::to_string),
            )
        }
        Err(other) => format!("a non-NSException object {other:?}"),
    }
}
