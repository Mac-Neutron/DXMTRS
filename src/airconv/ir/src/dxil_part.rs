//! The DXIL part of a container and its program header.
//!
//! Ported from `FindDXIL` of `dxil_container.cpp`: the part's bitcode, with the C++'s three
//! refusals in its words.

use crate::container::{Container, Error, fourcc, read_u32};

/// The bitcode a container's DXIL part holds.
#[derive(Debug, PartialEq, Eq)]
pub struct DxilPart<'a> {
    pub bitcode: &'a [u8],
}

const TRUNCATED: Error = Error::fail("DXIL: program header truncated");
const OUTSIDE: Error = Error::fail("DXIL: bitcode lies outside the DXIL part");

/// The program header: `DxilProgramHeader`'s two words and `DxilBitcodeHeader`'s four.
const HEADER_SIZE: usize = 24;
/// Where the bitcode header, and its offset field, start.
const BITCODE_HEADER: usize = 8;

/// The bitcode of the container's first DXIL part, or `None` for a container without one.
///
/// # Errors
///
/// The part's header is shorter than 24 bytes (`DXIL: program header truncated`), does not carry
/// the `DXIL` magic (`DXIL: bad program header`), or places fewer than four bytes of bitcode, or
/// bitcode that ends past the part (`DXIL: bitcode lies outside the DXIL part`).
pub fn find<'a>(container: &Container<'a>) -> Result<Option<DxilPart<'a>>, Error> {
    let Some(part) = container.find(fourcc::DXIL) else {
        return Ok(None);
    };
    if part.len() < HEADER_SIZE {
        return Err(TRUNCATED);
    }
    if part.get(BITCODE_HEADER..BITCODE_HEADER + 4) != Some(b"DXIL".as_slice()) {
        return Err(Error::fail("DXIL: bad program header"));
    }
    let offset = read_u32(part, 16).ok_or(TRUNCATED)?;
    let size = read_u32(part, 20).ok_or(TRUNCATED)?;
    let bitcode = usize::try_from(offset)
        .ok()
        .and_then(|offset| offset.checked_add(BITCODE_HEADER))
        .and_then(|start| Some((start, start.checked_add(usize::try_from(size).ok()?)?)))
        .filter(|_| size >= 4)
        .and_then(|(start, end)| part.get(start..end))
        .ok_or(OUTSIDE)?;
    Ok(Some(DxilPart { bitcode }))
}

#[cfg(test)]
mod tests;
