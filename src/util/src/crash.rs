//! The crash reporter's lines, each built in a fixed buffer, and the PE reporter (`veh`).
//!
//! From mtld3d (zlib, README.md's list). A crash line may come from a thread whose allocator
//! state is not to be trusted, or from the exception handler on a thread with no thread-local
//! storage, so every line here is formatted into a buffer of its own without allocating: the
//! panic line std's default hook prints to stderr ([`PanicLine`]), and the fatal, foreign-fault
//! and image-base lines ([`fatal_line`], [`foreign_line`], [`base_line`]), each opened by its
//! API's literal prefix.

use core::{fmt::Write, panic::Location};

#[cfg(windows)]
pub mod veh;

/// Bytes a [`PanicLine`] holds, its final line break included.
const PANIC_LINE_CAPACITY: usize = 2048;

/// What ends a [`PanicLine`] whose message did not fit, in place of the line break.
const PANIC_LINE_TRUNCATED: &str = " [truncated]\n";

/// A panic's thread, location and message, worded as std's default hook prints them.
///
/// `<prefix>thread '<name>' (<id>) panicked at <file>:<line>:<column>:`,
/// then the message on the next line: the layout the default hook writes to
/// stderr, so the log carries what stderr does and a reader that knows one
/// finds the other. A payload that is not a string reads `Box<dyn Any>`, as
/// it does there. The text lives in a fixed buffer, since a panic can come
/// from a thread whose allocator state is not to be trusted; a message too
/// long for it is cut at a character boundary and ends in `[truncated]`.
pub struct PanicLine {
    buf: [u8; PANIC_LINE_CAPACITY],
    len: usize,
}

impl PanicLine {
    /// The line for a panic on thread `thread` (`thread_id`) at `location` with `message`.
    ///
    /// `prefix` opens the first line, the log target the way the other
    /// crash lines name it.
    #[must_use]
    pub fn new(
        prefix: &str,
        thread: &str,
        thread_id: u32,
        location: Option<&Location<'_>>,
        message: Option<&str>,
    ) -> Self {
        let mut buf = [0; PANIC_LINE_CAPACITY];
        let mut cursor = LineCursor::new(&mut buf);
        let message = message.unwrap_or("Box<dyn Any>");
        // An `Err` only says the text ran out of room, which the cursor
        // records, so the rest of the line is skipped and the end marked.
        let _ = match location {
            Some(location) => write!(
                cursor,
                "{prefix}thread '{thread}' ({thread_id}) panicked at {location}:\n{message}"
            ),
            None => write!(
                cursor,
                "{prefix}thread '{thread}' ({thread_id}) panicked at an unknown location:\n{message}"
            ),
        };
        let len = cursor.finish();
        Self { buf, len }
    }

    /// The line's bytes, UTF-8 and ending in a line break.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

/// The write position in a [`PanicLine`]'s buffer while [`PanicLine::new`] formats it.
///
/// It keeps room for the end marker until [`Self::finish`], which writes the
/// marker and consumes the cursor, so nothing is written after the end.
struct LineCursor<'a> {
    buf: &'a mut [u8; PANIC_LINE_CAPACITY],
    len: usize,
    truncated: bool,
}

impl<'a> LineCursor<'a> {
    /// A cursor at the start of `buf`.
    const fn new(buf: &'a mut [u8; PANIC_LINE_CAPACITY]) -> Self {
        Self {
            buf,
            len: 0,
            truncated: false,
        }
    }

    /// End the line, with the marker when text was cut, and return its length.
    fn finish(self) -> usize {
        let end = if self.truncated {
            PANIC_LINE_TRUNCATED
        } else {
            "\n"
        };
        self.buf[self.len..self.len + end.len()].copy_from_slice(end.as_bytes());
        self.len + end.len()
    }
}

impl Write for LineCursor<'_> {
    /// Append `text`, or as much of it as fits ahead of the room the end marker keeps.
    fn write_str(&mut self, text: &str) -> core::fmt::Result {
        let room = (PANIC_LINE_CAPACITY - PANIC_LINE_TRUNCATED.len()).saturating_sub(self.len);
        let mut take = text.len().min(room);
        while !text.is_char_boundary(take) {
            take -= 1;
        }
        self.buf[self.len..self.len + take].copy_from_slice(&text.as_bytes()[..take]);
        self.len += take;
        if take < text.len() {
            self.truncated = true;
            return Err(core::fmt::Error);
        }
        Ok(())
    }
}

/// A crash line of at most `N` bytes, in a fixed buffer.
pub struct Line<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Line<N> {
    /// The line's bytes, ending in a line break unless the buffer was cut.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.buf[..self.len]
    }
}

/// The `FATAL` line for an exception in the image or with a fatal code.
///
/// `<prefix><code> addr=<addr> free_mib=<total> largest_free_mib=<largest>`, every number in
/// 16-digit hex; `free_mib` is the free address space's total and largest region in MiB.
#[must_use]
pub fn fatal_line(prefix: &[u8], code: u32, addr: usize, free_mib: (u64, u64)) -> Line<160> {
    let mut buf = [0u8; 160];
    let mut pos = 0;
    push(&mut buf, &mut pos, prefix);
    push_hex(&mut buf, &mut pos, u64::from(code));
    push(&mut buf, &mut pos, b" addr=");
    push_hex(&mut buf, &mut pos, addr as u64);
    push_free_space(&mut buf, &mut pos, free_mib);
    push(&mut buf, &mut pos, b"\n");
    Line { buf, len: pos }
}

/// The line for a fault in someone else's code, with the module it landed in.
///
/// `module` is the owning module's path and load base, or `None` (`module=?`) when the lookup
/// found none.
#[must_use]
pub fn foreign_line(
    prefix: &[u8],
    code: u32,
    addr: usize,
    module: Option<(&[u16], usize)>,
    free_mib: (u64, u64),
) -> Line<1024> {
    let mut buf = [0u8; 1024];
    let mut pos = 0;
    push(&mut buf, &mut pos, prefix);
    push_hex(&mut buf, &mut pos, u64::from(code));
    push(&mut buf, &mut pos, b" addr=");
    push_hex(&mut buf, &mut pos, addr as u64);
    push(&mut buf, &mut pos, b" module=");
    match module {
        None => push(&mut buf, &mut pos, b"?"),
        Some((path, base)) => {
            push_utf16(&mut buf, &mut pos, path);
            push(&mut buf, &mut pos, b" base=");
            push_hex(&mut buf, &mut pos, base as u64);
        }
    }
    push_free_space(&mut buf, &mut pos, free_mib);
    push(&mut buf, &mut pos, b"\n");
    Line { buf, len: pos }
}

/// The load base of the image, so a frame's RVA can be computed (`pc - base`).
#[must_use]
pub fn base_line(prefix: &[u8], base: usize) -> Line<64> {
    let mut buf = [0u8; 64];
    let mut pos = 0;
    push(&mut buf, &mut pos, prefix);
    push_hex(&mut buf, &mut pos, base as u64);
    push(&mut buf, &mut pos, b"\n");
    Line { buf, len: pos }
}

/// Append the free address space so a crash line carries it.
fn push_free_space(buf: &mut [u8], pos: &mut usize, (total, largest): (u64, u64)) {
    push(buf, pos, b" free_mib=");
    push_hex(buf, pos, total);
    push(buf, pos, b" largest_free_mib=");
    push_hex(buf, pos, largest);
}

fn push(buf: &mut [u8], pos: &mut usize, bytes: &[u8]) {
    let avail = buf.len() - *pos;
    let take = bytes.len().min(avail);
    buf[*pos..*pos + take].copy_from_slice(&bytes[..take]);
    *pos += take;
}

/// Append UTF-16 text as UTF-8, an unpaired surrogate as U+FFFD.
fn push_utf16(buf: &mut [u8], pos: &mut usize, units: &[u16]) {
    for c in char::decode_utf16(units.iter().copied()) {
        let mut utf8 = [0u8; 4];
        let c = c.unwrap_or(char::REPLACEMENT_CHARACTER);
        push(buf, pos, c.encode_utf8(&mut utf8).as_bytes());
    }
}

fn push_hex(buf: &mut [u8], pos: &mut usize, v: u64) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    if *pos + 18 > buf.len() {
        return;
    }
    buf[*pos] = b'0';
    buf[*pos + 1] = b'x';
    *pos += 2;
    for i in (0..16).rev() {
        let nib = usize::try_from((v >> (i * 4)) & 0xf).expect("4-bit nibble fits usize");
        buf[*pos] = HEX[nib];
        *pos += 1;
    }
}

#[cfg(test)]
mod tests;
