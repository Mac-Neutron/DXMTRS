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
