//! Always-on PE-side crash diagnostics, for a DLL that installs them (D3D9 does).
//!
//! Installed once from the DLL's `DllMain` `PROCESS_ATTACH`, and
//! the vectored handler is removed again on a `PROCESS_DETACH` the process
//! survives: a VEH registration is a process-wide pointer into this image, and
//! launchers and benchmarks routinely `LoadLibrary` d3d9, probe the caps and
//! `FreeLibrary` it before carrying on. Left behind, the registration turns
//! the process's next exception (C++ throws included) into an instruction
//! fetch from unmapped memory, which raises the next exception, which reaches
//! the same stale entry, until the main thread's stack is gone. The panic hook
//! needs no such care: it lives in this cdylib's own `std` and nothing else in
//! the process can reach it once the image is gone.
//!
//! Two pieces, both diagnostic-only — they print the crumb trail and
//! delegate termination to the normal SEH / `panic_abort` paths:
//!
//! 1. A Vectored Exception Handler that filters to truly-fatal `NTSTATUS`
//!    codes AND faults that originate inside the installing DLL's image. On a
//!    match it writes a one-line FATAL banner + dumps the shared crumb
//!    trail, then returns `EXCEPTION_CONTINUE_SEARCH` — `WoW`'s
//!    unhandled-exception filter still gets a chance to write
//!    `Crash.txt` before the process dies.
//!
//! 2. A panic hook. `panic = "abort"` ends the process inside the hook's
//!    caller, so the hook writes the report into the log synchronously:
//!    the panic's thread name, thread id, location and message, worded as
//!    the default hook words them, then the crumb trail. It then chains
//!    to the previous (default) hook, which prints the same
//!    `thread '…' panicked at …` report and (with `RUST_BACKTRACE`) a
//!    backtrace to stderr, where a game a launcher spawned shows nothing.
//!    Empty backtraces under Wine are accepted as-is.
//!
//! Faults in other modules (game code, `ClientExtensions.dll` `VMProtect`
//! probes, system DLLs) pass through to SEH normally, so `VMProtect`'s
//! first-chance recovery still works and `WoW` continues. Their report is a
//! first-chance one: it waits in the backlog and opens no log, since the
//! fault may be handled. When nothing handles it, Wine ends the process with
//! `TerminateProcess` on itself, which runs nothing of ours, so before
//! `Direct3DCreate9` that report dies with the process and only the unix
//! side's first-chance line on stderr remains. A top-level filter for
//! unhandled exceptions does not cover that case: the startup code of a game
//! built with Visual Studio 2015 or later replaces such a filter without
//! chaining, and one left behind by a `FreeLibrary` before any device would
//! point into unmapped code.
//!
//! The vectored handler runs on whatever thread raised the exception, and
//! nothing it reaches touches a Rust thread-local or the allocator: fixed
//! buffers, atomics, `kernel32` lookups that allocate nothing, and the
//! synchronous log thunk. Wine leaves `TEB.ThreadLocalStoragePointer` null
//! on a thread created without thread-attach notifications and after a
//! thread's detach, and copy protectors raise exceptions on such threads, so
//! a thread-local read in the handler faults inside this image; that fault
//! reaches the handler again, and a protector's frame that resumes the first
//! exception at its own PC loops the thread. A faulting thread may also hold
//! an allocator's lock.
//!
//! `RUST_BACKTRACE=1` is set here (if unset) so the default panic hook
//! attempts a backtrace.
//!
//! What a report says and where it goes is the installer's [`Reporter`]: its literals open each
//! line, so the hook and the handler build nothing, and its functions write the lines, dump the
//! crumb trail and measure the free address space.

use core::{
    ffi::c_void,
    sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering},
};

use super::{PanicLine, base_line, fatal_line, foreign_line};

/// What a DLL's crash reports say and where they go.
///
/// Literals, so the hook and the handler build nothing, and functions that run on the faulting
/// thread: none may allocate or touch a Rust thread-local.
pub struct Reporter {
    /// What opens the panic line, ahead of std's own wording.
    pub panic_prefix: &'static str,
    /// The line ahead of the crumb trail a panic dumps.
    pub panic_heading: &'static [u8],
    /// What opens the line of a fault in the image or with a fatal code, up to `code=`.
    pub fatal_prefix: &'static [u8],
    /// What opens the line of a fault in another module, up to `code=`.
    pub foreign_prefix: &'static [u8],
    /// What opens the image's load-base line, up to `base=`.
    pub base_prefix: &'static [u8],
    /// Writes a report of a fault that ends the process.
    pub write_crash: fn(&[u8]),
    /// Writes a first-chance report: the fault may be handled.
    pub write_fault: fn(&[u8]),
    /// Runs first in the panic hook (D3D9 forwards the lines it parked).
    pub before_panic: fn(),
    /// Dumps the last `n` crumbs.
    pub trail: fn(usize),
    /// The free address space's total and largest region, in MiB.
    pub free_mib: fn() -> (u64, u64),
    /// The size of the image mapped at a base, from its own headers.
    ///
    /// Called with the installing DLL's base only.
    pub image_size: unsafe fn(usize) -> Option<usize>,
}

// NTSTATUS codes the handler filters on.
const STATUS_ACCESS_VIOLATION: u32 = 0xC000_0005;
const STATUS_DATATYPE_MISALIGNMENT: u32 = 0x8000_0002;
const STATUS_HEAP_CORRUPTION: u32 = 0xC000_0374;
const STATUS_PRIVILEGED_INSTRUCTION: u32 = 0xC000_0096;
const STATUS_ILLEGAL_INSTRUCTION: u32 = 0xC000_001D;
const STATUS_STACK_BUFFER_OVERRUN: u32 = 0xC000_0409;
const STATUS_ASSERTION_FAILURE: u32 = 0xC000_0420;

const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
pub const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 4;
/// `GetModuleHandleEx` flag that leaves the module's reference count as it is.
const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: u32 = 2;
/// `GetModuleHandleEx` flag that keeps the module loaded until the process ends.
pub const GET_MODULE_HANDLE_EX_FLAG_PIN: u32 = 1;
/// The name a panic line gives a thread that has none, as std's default hook does.
const UNNAMED_THREAD: &str = "<unnamed>";
/// Bytes of a thread's name a panic line carries.
const THREAD_NAME_CAPACITY: usize = 128;
/// UTF-16 units of a thread's description read at most, in search of its terminator.
const THREAD_DESCRIPTION_SCAN: usize = 512;

// The statics below are process-wide on purpose: the VEH and the panic hook are called by the
// system and by std with no object of ours to reach.

/// Whether [`install`] ran and [`uninstall`] has not since.
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// The installing DLL's `HMODULE`, for [`fault_in_our_dll`] and the base line.
static MODULE: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());
/// The installing DLL's [`Reporter`]; null until [`install`] stores it.
///
/// A static, as the others here: the VEH and the panic hook have no object to reach.
static REPORTER: AtomicPtr<Reporter> = AtomicPtr::new(core::ptr::null_mut());
/// The registration `RtlAddVectoredExceptionHandler` handed back, for [`uninstall`].
static VEH_HANDLE: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());
/// Faults outside our image reported so far; the report stops after a few.
static FOREIGN_REPORTS: AtomicU32 = AtomicU32::new(0);

/// How many faults outside our image get a line in the log.
///
/// Some programs probe with deliberate access violations, and every one
/// of those would otherwise cost a module lookup and a crumb dump.
const FOREIGN_REPORT_LIMIT: u32 = 4;

#[repr(C)]
struct ExceptionRecord {
    code: u32,
    flags: u32,
    nested: *mut Self,
    address: *mut c_void,
    n_params: u32,
    information: [usize; 15],
}

#[repr(C)]
struct ExceptionPointers {
    record: *mut ExceptionRecord,
    context: *mut c_void,
}

type VectoredHandler = extern "system" fn(*mut ExceptionPointers) -> i32;

unsafe extern "system" {
    fn RtlAddVectoredExceptionHandler(first: u32, handler: VectoredHandler) -> *mut c_void;
    fn RtlRemoveVectoredExceptionHandler(handle: *mut c_void) -> u32;
    pub fn GetModuleHandleExA(flags: u32, module_name: *const u8, out: *mut *mut c_void) -> i32;
    fn GetModuleFileNameW(module: *mut c_void, filename: *mut u16, size: u32) -> u32;
    fn GetCurrentThread() -> *mut c_void;
    fn GetCurrentThreadId() -> u32;
    fn GetThreadDescription(thread: *mut c_void, description: *mut *mut u16) -> i32;
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
}

/// Install the VEH and the panic hook. Idempotent.
///
/// `module` is the `HMODULE` passed to `DllMain` — saved so we can
/// later check whether a faulting PC lives in our DLL image. `reporter`
/// and the module are stored before either is registered. Called from
/// `DllMain`'s process attach.
pub fn install(module: *mut c_void, reporter: &'static Reporter) {
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }

    if std::env::var_os("RUST_BACKTRACE").is_none() {
        // `full` over `1` because Wine's unwinder returns very few
        // frames; the `1`-mode elision of std-internal frames usually
        // strips the result to empty. `full` keeps everything captured.
        // SAFETY: DllMain runs single-threaded on the main thread
        // before any of our spawned threads exist.
        unsafe { std::env::set_var("RUST_BACKTRACE", "full") };
    }

    REPORTER.store(core::ptr::from_ref(reporter).cast_mut(), Ordering::Release);
    MODULE.store(module, Ordering::Release);
    // SAFETY: ntdll export; safe to call from DllMain.
    let veh = unsafe { RtlAddVectoredExceptionHandler(1, handler) };
    VEH_HANDLE.store(veh, Ordering::Release);
    install_panic_hook(reporter);
}

/// Remove the VEH before the image goes away. Idempotent.
///
/// Called from `DllMain` `PROCESS_DETACH` when no device was created: a
/// `FreeLibrary` the process survives, or the exit of a process that never
/// created one. Once a device exists the image is pinned, so the only
/// detach left is process exit, which terminates the process instead and
/// never gets here.
pub fn uninstall() {
    if !INSTALLED.swap(false, Ordering::AcqRel) {
        return;
    }
    let veh = VEH_HANDLE.swap(core::ptr::null_mut(), Ordering::AcqRel);
    if veh.is_null() {
        return;
    }
    // SAFETY: `veh` is the live registration `install` stored; ntdll export.
    unsafe { RtlRemoveVectoredExceptionHandler(veh) };
}

fn install_panic_hook(reporter: &'static Reporter) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A panic ends the process: its report may open the early log, with
        // the lines queued since load ahead of it.
        (reporter.before_panic)();
        write_panic_line(reporter, info);
        (reporter.write_crash)(reporter.panic_heading);
        (reporter.trail)(32);
        emit_image_bases(reporter);
        // Chain to the default hook, which prints the same report to stderr
        // and a backtrace (possibly empty on Wine). The log has the report
        // already; backtrace quality is best-effort.
        prev(info);
    }));
}

/// Write the panic's thread, location and message into the log, as std's default hook words them.
///
/// The default hook prints them to stderr only, which a game a launcher
/// spawned does not have, and under `panic = "abort"` nothing runs after the
/// hooks. So the line goes out here, synchronously, before the crumb trail.
/// It reads no Rust thread-local and allocates nothing from the Rust
/// allocator: `std::thread::current` may allocate on a thread std has not
/// seen and panics on one past its thread-local teardown, and a panic inside
/// the hook would abort before the line is written. The thread's name comes
/// from its Win32 description instead, which std sets for every thread it
/// names, and the id is the one the default hook prints.
fn write_panic_line(reporter: &Reporter, info: &std::panic::PanicHookInfo<'_>) {
    let mut name = [0u8; THREAD_NAME_CAPACITY];
    let name = thread_name(&mut name);
    // SAFETY: kernel32 export with no arguments.
    let thread_id = unsafe { GetCurrentThreadId() };
    let line = PanicLine::new(
        reporter.panic_prefix,
        name,
        thread_id,
        info.location(),
        info.payload_as_str(),
    );
    (reporter.write_crash)(line.bytes());
}

/// The calling thread's Win32 description as UTF-8 in `buf`, or `<unnamed>` when it has none.
///
/// `GetThreadDescription` hands back a buffer from the process heap, not
/// from the Rust allocator, freed here with `LocalFree`. A name longer than
/// `buf` is cut, at a character boundary.
fn thread_name(buf: &mut [u8; THREAD_NAME_CAPACITY]) -> &str {
    // SAFETY: kernel32 export; the pseudo-handle names the calling thread.
    let thread = unsafe { GetCurrentThread() };
    let mut description: *mut u16 = core::ptr::null_mut();
    // SAFETY: kernel32 export writing one pointer to the out slot given.
    let hr = unsafe { GetThreadDescription(thread, &raw mut description) };
    if hr < 0 || description.is_null() {
        return UNNAMED_THREAD;
    }
    let mut units = 0;
    while units < THREAD_DESCRIPTION_SCAN {
        let unit = description.wrapping_add(units);
        // SAFETY: the description is NUL-terminated and the scan stops at
        // the terminator, so every unit read lies inside it.
        if unsafe { unit.read() } == 0 {
            break;
        }
        units += 1;
    }
    // SAFETY: the `units` units ahead of the terminator, read above.
    let units = unsafe { core::slice::from_raw_parts(description, units) };
    let mut pos = 0;
    super::push_utf16(buf, &mut pos, units);
    // SAFETY: the buffer `GetThreadDescription` allocated, freed once.
    unsafe { LocalFree(description.cast()) };
    let text = match core::str::from_utf8(&buf[..pos]) {
        Ok(text) => text,
        Err(cut) => core::str::from_utf8(&buf[..cut.valid_up_to()]).unwrap_or_default(),
    };
    if text.is_empty() {
        UNNAMED_THREAD
    } else {
        text
    }
}

/// Print the runtime load bases for our own DLL(s).
///
/// Wine's `dbghelp` rarely loads PDBs end-to-end, so symbolicated frames
/// inside d3d9.dll come out as `<unknown>` in std's backtrace. Knowing the
/// load base lets you compute RVAs (`pc - base`) and resolve them
/// externally:
///
/// ```sh
/// llvm-symbolizer --obj=windows/target/i686-pc-windows-msvc/release/d3d9.dll \
///                 --pdb=windows/target/i686-pc-windows-msvc/release/d3d9.pdb \
///                 <RVA>
/// ```
fn emit_image_bases(reporter: &Reporter) {
    let our = MODULE.load(Ordering::Acquire);
    if our.is_null() {
        return;
    }
    (reporter.write_crash)(base_line(reporter.base_prefix, our as usize).bytes());
}

extern "system" fn handler(ep: *mut ExceptionPointers) -> i32 {
    // SAFETY: `REPORTER` holds null or the `&'static Reporter` `install` stored.
    let Some(reporter) = (unsafe { REPORTER.load(Ordering::Acquire).as_ref() }) else {
        return EXCEPTION_CONTINUE_SEARCH;
    };
    if ep.is_null() {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    // SAFETY: ep non-null per check; kernel-supplied for handler lifetime.
    let rec = unsafe { (*ep).record };
    if rec.is_null() {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    // SAFETY: rec non-null per check.
    let code = unsafe { (*rec).code };
    // SAFETY: rec non-null per check.
    let addr = unsafe { (*rec).address };

    let always_fatal = matches!(
        code,
        STATUS_HEAP_CORRUPTION | STATUS_STACK_BUFFER_OVERRUN | STATUS_ASSERTION_FAILURE
    );
    let possibly_fatal = matches!(
        code,
        STATUS_ACCESS_VIOLATION
            | STATUS_DATATYPE_MISALIGNMENT
            | STATUS_PRIVILEGED_INSTRUCTION
            | STATUS_ILLEGAL_INSTRUCTION
    );

    // Diagnostic-only. Do NOT terminate — let SEH unwind so the game's own
    // unhandled-exception filter still gets to write its crash report.
    // Only an always-fatal code is known to end the process here; a fault in
    // our image is still a first chance a frame up the stack may handle.
    if always_fatal || (possibly_fatal && fault_in_our_dll(reporter, addr)) {
        let line = fatal_line(
            reporter.fatal_prefix,
            code,
            addr as usize,
            (reporter.free_mib)(),
        );
        if always_fatal {
            (reporter.write_crash)(line.bytes());
        } else {
            (reporter.write_fault)(line.bytes());
        }
        (reporter.trail)(32);
    } else if possibly_fatal {
        report_foreign_fault(reporter, code, addr);
    }
    EXCEPTION_CONTINUE_SEARCH
}

/// Note a fault in someone else's code, with the module it landed in.
///
/// Wine names the address of an unhandled fault but not the module, and a
/// launcher-spawned game leaves no way to run the debugger afterwards. The
/// first few faults get the owning module's path and our most recent API
/// crumbs, which is usually enough to tell "the game dereferenced what we
/// returned" from "unrelated".
fn report_foreign_fault(reporter: &Reporter, code: u32, addr: *mut c_void) {
    if FOREIGN_REPORTS.fetch_add(1, Ordering::AcqRel) >= FOREIGN_REPORT_LIMIT {
        return;
    }
    // The lookup and the file name each take Wine's loader lock for the
    // walk of the module list (`RtlPcToFileHeader` and `LdrGetDllFullName`
    // enter `loader_section` in `dlls/ntdll/loader.c`). The lock is a critical
    // section, so a fault on the thread that holds it re-enters it, and the
    // report costs this only for the first few foreign faults. The handle is
    // borrowed: without UNCHANGED_REFCOUNT every report would add a reference
    // the faulting module never loses.
    let mut module: *mut c_void = core::ptr::null_mut();
    // SAFETY: kernel32 export; `addr` is only used as a lookup key.
    let found = unsafe {
        GetModuleHandleExA(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            addr.cast::<u8>(),
            &raw mut module,
        )
    };
    // The wide form: the ANSI one converts through a buffer it allocates on
    // the process heap.
    let mut path = [0u16; 260];
    let path_len = if found == 0 || module.is_null() {
        0
    } else {
        // SAFETY: kernel32 export writing at most `path.len()` UTF-16 units.
        unsafe { GetModuleFileNameW(module, path.as_mut_ptr(), 260) }
    };
    let owner = (path_len != 0).then(|| {
        let n = (path_len as usize).min(path.len());
        (&path[..n], module as usize)
    });
    let line = foreign_line(
        reporter.foreign_prefix,
        code,
        addr as usize,
        owner,
        (reporter.free_mib)(),
    );
    // A first chance: the fault may be handled, so the line opens no log.
    (reporter.write_fault)(line.bytes());
    (reporter.trail)(16);
}

/// Whether `addr` lies in this image, from its own headers.
///
/// Every access violation and illegal instruction in the process asks this,
/// so it makes no call: no loader lock, and no reference taken on the module
/// that faulted.
fn fault_in_our_dll(reporter: &Reporter, addr: *mut c_void) -> bool {
    let base = MODULE.load(Ordering::Acquire) as usize;
    if base == 0 {
        return false;
    }
    // SAFETY: `base` is this image's `HMODULE` from `DllMain`, and the
    // handler is registered only while the image is mapped.
    let Some(size) = (unsafe { (reporter.image_size)(base) }) else {
        return false;
    };
    (addr as usize).wrapping_sub(base) < size
}
