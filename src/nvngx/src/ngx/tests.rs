use std::io::{self, Write};

use super::line;

/// A sink every write fails on, as a game's stderr pipe whose reader has gone.
struct Broken;

impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::BrokenPipe))
    }
}

#[test]
fn a_line_is_prefixed() {
    let mut out = Vec::new();
    line(
        &mut out,
        format_args!("DLSS {}x{} answered by MetalFX", 1280, 720),
    );
    assert_eq!(out, b"macneutron-dlss: DLSS 1280x720 answered by MetalFX\n");
}

#[test]
fn a_failed_write_is_ignored() {
    line(&mut Broken, format_args!("no reader"));
}
