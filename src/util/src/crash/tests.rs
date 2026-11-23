//! The crash lines.
//!
//! The panic-line tests pin std's wording, the stand-in for a payload that is not a string, the
//! cut of a message too long for the buffer, and that a finished line takes no more text.

use super::*;

#[test]
fn a_panic_line_words_the_report_as_the_default_hook_does() {
    let location = Location::caller();
    let line = PanicLine::new("[t] PANIC: ", "worker", 2180, Some(location), Some("boom"));
    assert_eq!(
        core::str::from_utf8(line.bytes()).expect("the line is UTF-8"),
        format!("[t] PANIC: thread 'worker' (2180) panicked at {location}:\nboom\n")
    );
}

#[test]
fn a_panic_line_names_a_payload_that_is_not_a_string_and_a_missing_location() {
    let line = PanicLine::new("", "<unnamed>", 7, None, None);
    assert_eq!(
        core::str::from_utf8(line.bytes()).expect("the line is UTF-8"),
        "thread '<unnamed>' (7) panicked at an unknown location:\nBox<dyn Any>\n"
    );
}

#[test]
fn a_panic_line_cuts_a_long_message_at_a_character_boundary_and_marks_it() {
    // Two-byte characters, so the room left over is odd for one of the
    // two prefixes and the cut has to step back off half a character.
    let message = "\u{e9}".repeat(PANIC_LINE_CAPACITY);
    for prefix in ["p", "pp"] {
        let line = PanicLine::new(prefix, "t", 1, None, Some(&message));
        let text = core::str::from_utf8(line.bytes()).expect("the cut keeps the line UTF-8");
        assert!(line.bytes().len() <= PANIC_LINE_CAPACITY);
        assert!(
            line.bytes().len() > PANIC_LINE_CAPACITY - PANIC_LINE_TRUNCATED.len() - 2,
            "the line fills the buffer up to the marker"
        );
        assert!(text.ends_with("\u{e9} [truncated]\n"), "{text}");
        assert!(text.starts_with(&format!("{prefix}thread 't' (1) panicked at ")));
    }
}

#[test]
fn a_full_cursor_refuses_more_text_and_still_ends_the_line() {
    let mut buf = [0; PANIC_LINE_CAPACITY];
    let mut cursor = LineCursor::new(&mut buf);
    let fill = "x".repeat(PANIC_LINE_CAPACITY);
    assert!(cursor.write_str(&fill).is_err(), "the fill does not fit");
    assert!(
        cursor.write_str("more").is_err(),
        "a full cursor takes nothing"
    );
    assert!(cursor.write_str("").is_ok(), "an empty write fits anywhere");
    let len = cursor.finish();
    assert_eq!(len, PANIC_LINE_CAPACITY);
    assert!(buf.ends_with(PANIC_LINE_TRUNCATED.as_bytes()));
}

/// Asks whether `T` implements [`Write`]: a method call on `&WriteProbe<T>` picks [`ViaWrite`].
///
/// [`ViaWrite`] is implemented for the probe itself and so applies to the
/// call's receiver as written when `T` is [`Write`]; [`NotWrite`], on the
/// probe's reference, is reached only by the extra borrow the compiler tries
/// next, so it answers for every other `T`.
struct WriteProbe<T>(core::marker::PhantomData<T>);

/// The answer for a probed type that implements [`Write`].
trait ViaWrite {
    fn implements_write(&self) -> bool {
        true
    }
}

impl<T: Write> ViaWrite for WriteProbe<T> {}

/// The answer for a probed type that does not.
trait NotWrite {
    fn implements_write(&self) -> bool {
        false
    }
}

impl<T> NotWrite for &WriteProbe<T> {}

#[test]
fn a_finished_panic_line_takes_no_more_text() {
    // Only the private cursor writes; the finished line has no writer of its own.
    let line = &WriteProbe::<PanicLine>(core::marker::PhantomData);
    assert!(!line.implements_write());
    let cursor = &WriteProbe::<LineCursor<'_>>(core::marker::PhantomData);
    assert!(cursor.implements_write());
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[test]
fn foreign_line_formats_owned_and_unowned_faults_without_caller() {
    let mod_path = utf16("C:\\game\\foo.dll");
    let owned = foreign_line(
        b"[dx] foreign: code=",
        0xC000_0005,
        0x1234_5678,
        Some((&mod_path, 0x1000_0000)),
        &ForeignCaller::None,
        (0x100, 0x80),
    );
    assert_eq!(
        core::str::from_utf8(owned.bytes()).expect("UTF-8"),
        "[dx] foreign: code=0x00000000c0000005 addr=0x0000000012345678 \
         module=C:\\game\\foo.dll base=0x0000000010000000 \
         free_mib=0x0000000000000100 largest_free_mib=0x0000000000000080\n"
    );

    let unowned = foreign_line(
        b"[dx] foreign: code=",
        0xC000_0005,
        0xDEAD_BEEF,
        None,
        &ForeignCaller::None,
        (0x20, 0x10),
    );
    assert_eq!(
        core::str::from_utf8(unowned.bytes()).expect("UTF-8"),
        "[dx] foreign: code=0x00000000c0000005 addr=0x00000000deadbeef \
         module=? free_mib=0x0000000000000020 largest_free_mib=0x0000000000000010\n"
    );
}

#[test]
fn foreign_line_formats_execute_fault_caller_variants() {
    let unknown = foreign_line(
        b"code=",
        0xC000_0005,
        0,
        None,
        &ForeignCaller::Unknown,
        (1, 1),
    );
    assert_eq!(
        core::str::from_utf8(unknown.bytes()).expect("UTF-8"),
        "code=0x00000000c0000005 addr=0x0000000000000000 module=? caller=? \
         free_mib=0x0000000000000001 largest_free_mib=0x0000000000000001\n"
    );

    let unowned_caller = foreign_line(
        b"code=",
        0xC000_0005,
        0,
        None,
        &ForeignCaller::Address {
            addr: 0x0040_1234,
            module: None,
        },
        (1, 1),
    );
    assert_eq!(
        core::str::from_utf8(unowned_caller.bytes()).expect("UTF-8"),
        "code=0x00000000c0000005 addr=0x0000000000000000 module=? \
         caller=0x0000000000401234 caller_module=? \
         free_mib=0x0000000000000001 largest_free_mib=0x0000000000000001\n"
    );

    let caller_path = utf16("C:\\game\\d3dx9_43.dll");
    let owned_caller = foreign_line(
        b"code=",
        0xC000_0005,
        0,
        None,
        &ForeignCaller::Address {
            addr: 0x6789_0123,
            module: Some((&caller_path, 0x6780_0000)),
        },
        (2, 1),
    );
    assert_eq!(
        core::str::from_utf8(owned_caller.bytes()).expect("UTF-8"),
        "code=0x00000000c0000005 addr=0x0000000000000000 module=? \
         caller=0x0000000067890123 caller_module=C:\\game\\d3dx9_43.dll \
         caller_base=0x0000000067800000 \
         free_mib=0x0000000000000002 largest_free_mib=0x0000000000000001\n"
    );
}

#[test]
fn only_dep_access_violation_counts_as_execute_fault() {
    assert!(is_execute_fault(2, 8));
    assert!(is_execute_fault(3, 8));
    assert!(!is_execute_fault(1, 8));
    assert!(!is_execute_fault(0, 8));
    assert!(!is_execute_fault(2, 0));
    assert!(!is_execute_fault(2, 1));
}

#[test]
fn readable_span_requires_committed_readable_non_guard_bounds() {
    const PAGE_NOACCESS: u32 = 0x01;
    const PAGE_READWRITE: u32 = 0x04;
    const PAGE_EXECUTE: u32 = 0x10;
    const PAGE_EXECUTE_READ: u32 = 0x20;
    const MEM_RESERVE: u32 = 0x2000;

    let region = |state, protect| QueriedRegion::new(0x10_0000, 0x2000, state, protect);
    let word = size_of::<usize>();

    assert!(readable_span(
        &region(MEM_COMMIT, PAGE_READWRITE),
        0x10_0ff0,
        word
    ));
    assert!(readable_span(
        &region(MEM_COMMIT, PAGE_EXECUTE_READ),
        0x10_0000,
        word
    ));
    assert!(!readable_span(
        &region(MEM_COMMIT, PAGE_NOACCESS),
        0x10_0ff0,
        word
    ));
    assert!(!readable_span(
        &region(MEM_COMMIT, PAGE_EXECUTE),
        0x10_0ff0,
        word
    ));
    assert!(!readable_span(
        &region(MEM_COMMIT, PAGE_READWRITE | PAGE_GUARD),
        0x10_0ff0,
        word
    ));
    assert!(!readable_span(
        &region(MEM_RESERVE, PAGE_READWRITE),
        0x10_0ff0,
        word
    ));

    let exact = QueriedRegion::new(0x10_0000, 0x2000, MEM_COMMIT, PAGE_READWRITE);
    assert!(readable_span(&exact, 0x10_1ffc, 4));
    assert!(!readable_span(&exact, 0x10_1ffd, 4));
    assert!(!readable_span(&exact, 0x0f_fffc, 4));
    let whole = QueriedRegion::new(0, usize::MAX, MEM_COMMIT, PAGE_READWRITE);
    assert!(!readable_span(&whole, usize::MAX - 1, 4));
}
