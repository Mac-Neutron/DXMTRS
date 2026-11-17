//! Forward a notification into Wine's application delegate with its Objective-C exceptions caught.
//!
//! This library is built with `panic = "abort"`, under which every Rust frame
//! aborts the process when an unwind reaches it, a foreign Objective-C
//! exception included, and rustc emits no landing pad that could stop one.
//! `objc2::exception::catch` cannot help there, because the closure it runs
//! is a Rust frame between its `@try` and the send. So the send and its catch
//! are one function written in assembly, [`mtld3d_try_send`]: the frame that
//! calls `objc_msgSend` is the frame that holds the landing pad, and what the
//! delegate throws is handed back here as an object instead of unwinding.

use objc2::{
    rc::Retained,
    runtime::{AnyObject, NSObjectProtocol, ProtocolObject, Sel},
};
use objc2_foundation::{NSException, NSNotification};

// `mtld3d_try_send`, as Apple clang compiles this Objective-C (`-O3 -fobjc-exceptions
// -fno-objc-arc`, the text of `clang -S` with its labels renamed and nothing else changed):
//
//     id mtld3d_try_send(id receiver, SEL selector, id argument)
//     {
//         @try {
//             ((void (*)(id, SEL, id))objc_msgSend)(receiver, selector, argument);
//         } @catch (id exception) {
//             return [exception retain];
//         }
//         return nil;
//     }
//
// The parts, in order: the frame and its unwind rules (`.cfi_*`), with Objective-C's personality
// routine and the address of the function's exception table; the send, between two labels the
// table names; the return of nil; the landing pad the unwinder enters when the send throws
// (`objc_begin_catch`, a retain of the caught object, `objc_end_catch`, the return of the object);
// the cleanup that ends the catch and resumes the unwind if the retain itself throws; then the
// table (`__gcc_except_tab`), every number in it a difference of two labels that the assembler
// computes: four call ranges, of which the send's leads to the landing pad, and one caught type,
// `id` (`OBJC_EHTYPE_id`: every Objective-C object). The selector is an argument, so the block
// needs no selector reference and no Objective-C image info of its own.
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".section __TEXT,__text,regular,pure_instructions",
    ".globl _mtld3d_try_send",
    ".p2align 2",
    "_mtld3d_try_send:",
    "Lmtld3d_try_begin:",
    ".cfi_startproc",
    ".cfi_personality 155, ___objc_personality_v0",
    ".cfi_lsda 16, Lmtld3d_try_table",
    "stp x20, x19, [sp, #-32]!",
    "stp x29, x30, [sp, #16]",
    "add x29, sp, #16",
    ".cfi_def_cfa w29, 16",
    ".cfi_offset w30, -8",
    ".cfi_offset w29, -16",
    ".cfi_offset w19, -24",
    ".cfi_offset w20, -32",
    "Lmtld3d_try_send:",
    "bl _objc_msgSend",
    "Lmtld3d_try_sent:",
    "mov x0, #0",
    "ldp x29, x30, [sp, #16]",
    "ldp x20, x19, [sp], #32",
    "ret",
    "Lmtld3d_try_caught:",
    "mov x19, x0",
    "cmp w1, #1",
    "b.ne Lmtld3d_try_resume",
    "mov x0, x19",
    "bl _objc_begin_catch",
    "Lmtld3d_try_retain:",
    "bl _objc_retain",
    "Lmtld3d_try_retained:",
    "mov x19, x0",
    "bl _objc_end_catch",
    "mov x0, x19",
    "ldp x29, x30, [sp, #16]",
    "ldp x20, x19, [sp], #32",
    "ret",
    "Lmtld3d_try_cleanup:",
    "mov x19, x0",
    "bl _objc_end_catch",
    "Lmtld3d_try_resume:",
    "mov x0, x19",
    "bl __Unwind_Resume",
    "Lmtld3d_try_end:",
    ".cfi_endproc",
    ".section __TEXT,__gcc_except_tab",
    ".p2align 2, 0x0",
    "mtld3d_try_send_table:",
    "Lmtld3d_try_table:",
    ".byte 255",
    ".byte 155",
    ".uleb128 Lmtld3d_try_types_end-Lmtld3d_try_types_ref",
    "Lmtld3d_try_types_ref:",
    ".byte 1",
    ".uleb128 Lmtld3d_try_sites_end-Lmtld3d_try_sites",
    "Lmtld3d_try_sites:",
    ".uleb128 Lmtld3d_try_send-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_sent-Lmtld3d_try_send",
    ".uleb128 Lmtld3d_try_caught-Lmtld3d_try_begin",
    ".byte 1",
    ".uleb128 Lmtld3d_try_sent-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_retain-Lmtld3d_try_sent",
    ".byte 0",
    ".byte 0",
    ".uleb128 Lmtld3d_try_retain-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_retained-Lmtld3d_try_retain",
    ".uleb128 Lmtld3d_try_cleanup-Lmtld3d_try_begin",
    ".byte 0",
    ".uleb128 Lmtld3d_try_retained-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_end-Lmtld3d_try_retained",
    ".byte 0",
    ".byte 0",
    "Lmtld3d_try_sites_end:",
    ".byte 1",
    ".byte 0",
    ".p2align 2, 0x0",
    "Lmtld3d_try_type_id:",
    ".long _OBJC_EHTYPE_id@GOT-Lmtld3d_try_type_id",
    "Lmtld3d_try_types_end:",
    ".p2align 2, 0x0",
    ".section __TEXT,__text,regular,pure_instructions",
);

// The same function for x86-64, from the same source and flags.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".section __TEXT,__text,regular,pure_instructions",
    ".globl _mtld3d_try_send",
    ".p2align 4",
    "_mtld3d_try_send:",
    "Lmtld3d_try_begin:",
    ".cfi_startproc",
    ".cfi_personality 155, ___objc_personality_v0",
    ".cfi_lsda 16, Lmtld3d_try_table",
    "pushq %rbp",
    ".cfi_def_cfa_offset 16",
    ".cfi_offset %rbp, -16",
    "movq %rsp, %rbp",
    ".cfi_def_cfa_register %rbp",
    "pushq %rbx",
    "pushq %rax",
    ".cfi_offset %rbx, -24",
    "Lmtld3d_try_send:",
    "callq _objc_msgSend",
    "Lmtld3d_try_sent:",
    "xorl %eax, %eax",
    "addq $8, %rsp",
    "popq %rbx",
    "popq %rbp",
    "retq",
    "Lmtld3d_try_caught:",
    "movq %rax, %rbx",
    "cmpl $1, %edx",
    "jne Lmtld3d_try_resume",
    "movq %rbx, %rdi",
    "callq _objc_begin_catch",
    "Lmtld3d_try_retain:",
    "movq %rax, %rdi",
    "callq *_objc_retain@GOTPCREL(%rip)",
    "Lmtld3d_try_retained:",
    "movq %rax, %rbx",
    "callq _objc_end_catch",
    "movq %rbx, %rax",
    "addq $8, %rsp",
    "popq %rbx",
    "popq %rbp",
    "retq",
    "Lmtld3d_try_cleanup:",
    "movq %rax, %rbx",
    "callq _objc_end_catch",
    "Lmtld3d_try_resume:",
    "movq %rbx, %rdi",
    "callq __Unwind_Resume",
    "Lmtld3d_try_end:",
    ".cfi_endproc",
    ".section __TEXT,__gcc_except_tab",
    ".p2align 2, 0x0",
    "mtld3d_try_send_table:",
    "Lmtld3d_try_table:",
    ".byte 255",
    ".byte 155",
    ".uleb128 Lmtld3d_try_types_end-Lmtld3d_try_types_ref",
    "Lmtld3d_try_types_ref:",
    ".byte 1",
    ".uleb128 Lmtld3d_try_sites_end-Lmtld3d_try_sites",
    "Lmtld3d_try_sites:",
    ".uleb128 Lmtld3d_try_send-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_sent-Lmtld3d_try_send",
    ".uleb128 Lmtld3d_try_caught-Lmtld3d_try_begin",
    ".byte 1",
    ".uleb128 Lmtld3d_try_sent-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_retain-Lmtld3d_try_sent",
    ".byte 0",
    ".byte 0",
    ".uleb128 Lmtld3d_try_retain-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_retained-Lmtld3d_try_retain",
    ".uleb128 Lmtld3d_try_cleanup-Lmtld3d_try_begin",
    ".byte 0",
    ".uleb128 Lmtld3d_try_retained-Lmtld3d_try_begin",
    ".uleb128 Lmtld3d_try_end-Lmtld3d_try_retained",
    ".byte 0",
    ".byte 0",
    "Lmtld3d_try_sites_end:",
    ".byte 1",
    ".byte 0",
    ".p2align 2, 0x0",
    ".long _OBJC_EHTYPE_id@GOTPCREL+4",
    "Lmtld3d_try_types_end:",
    ".p2align 2, 0x0",
    ".section __TEXT,__text,regular,pure_instructions",
    options(att_syntax),
);

unsafe extern "C" {
    /// Send `selector` to `receiver` with one object argument, catching what the method throws.
    ///
    /// Returns null when the send returns, and otherwise the object thrown, retained once.
    /// Defined by the assembly above. The method `selector` names must take one object and
    /// return nothing.
    fn mtld3d_try_send(
        receiver: *const AnyObject,
        selector: Sel,
        argument: *const AnyObject,
    ) -> *mut AnyObject;
}

/// The selector of `applicationDidChangeScreenParameters:`.
fn screen_parameters_selector() -> Sel {
    Sel::register(c"applicationDidChangeScreenParameters:")
}

/// Whether `delegate` implements `applicationDidChangeScreenParameters:`.
///
/// `AppKit` subscribes a delegate to the notification only when it does, so
/// this is also whether the delegate is an observer to take the notification
/// over from.
pub fn delegate_handles_screen_parameters(delegate: &AnyObject) -> bool {
    // SAFETY: `ProtocolObject` is a transparent wrapper over `AnyObject`, and every object an
    // application delegate can be conforms to `NSObject`'s protocol.
    let object =
        unsafe { &*core::ptr::from_ref(delegate).cast::<ProtocolObject<dyn NSObjectProtocol>>() };
    object.respondsToSelector(screen_parameters_selector())
}

/// Send `applicationDidChangeScreenParameters:` to `delegate`, catching what it throws.
///
/// # Errors
///
/// Returns the caught exception's name and reason when the delegate, or
/// anything it called, threw an Objective-C exception. The send's own effects
/// up to the throw stay as they are, as they would after `AppKit` caught the
/// exception in its run loop.
///
/// # Safety
///
/// `delegate` must be messaged only on the thread it belongs to (the main
/// thread for Wine's application delegate), and `notification` must be the
/// notification being delivered.
pub unsafe fn forward_screen_parameters(
    delegate: &AnyObject,
    notification: &NSNotification,
) -> Result<(), String> {
    let notification = core::ptr::from_ref(notification).cast::<AnyObject>();
    // SAFETY: both pointers come from live references; the caller upholds the
    // thread the delegate is messaged on; the method the selector names takes
    // one object and returns nothing; the function catches every Objective-C
    // exception, so nothing unwinds out of it.
    let thrown = unsafe { mtld3d_try_send(delegate, screen_parameters_selector(), notification) };
    // SAFETY: a non-null return is an object retained once for us.
    let Some(thrown) = (unsafe { Retained::from_raw(thrown) }) else {
        return Ok(());
    };
    Err(describe_exception(thrown))
}

/// Name and reason of a caught exception, or the object's description when it is no `NSException`.
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

#[cfg(test)]
mod tests;
