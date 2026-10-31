// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The DXBC container: its header, its part index and each part's bytes.
//!
//! Ported from `CDXBCParser` of `libs/DXBCParser` (Microsoft, MIT licence). Every refusal of the
//! C++'s `ReadDXBC` is a refusal here, with the C++'s text; what the C++ trusted from the blob (a
//! part that runs one byte past the container, a 32-bit sum that wraps) is refused as well, and
//! no read goes past the slice.

use core::fmt;

/// The four-character codes of the parts the translator and the PE users read.
pub mod fourcc {
    pub const DXBC: u32 = tag(*b"DXBC");
    pub const SHDR: u32 = tag(*b"SHDR");
    pub const SHEX: u32 = tag(*b"SHEX");
    pub const ISGN: u32 = tag(*b"ISGN");
    pub const ISG1: u32 = tag(*b"ISG1");
    pub const PCSG: u32 = tag(*b"PCSG");
    pub const PSG1: u32 = tag(*b"PSG1");
    pub const OSGN: u32 = tag(*b"OSGN");
    pub const OSG5: u32 = tag(*b"OSG5");
    pub const OSG1: u32 = tag(*b"OSG1");
    pub const RTS0: u32 = tag(*b"RTS0");
    pub const DXIL: u32 = tag(*b"DXIL");

    const fn tag(name: [u8; 4]) -> u32 {
        u32::from_le_bytes(name)
    }
}

/// The container's two on-disk records, as `#[repr(C)]` types with the C++'s sizes.
pub mod raw {
    use super::{read_u16, read_u32};

    /// `DXBCHeader`: the container's first 32 bytes, followed by one `u32` offset per part.
    #[repr(C)]
    #[derive(Debug)]
    pub struct Header {
        pub fourcc: u32,
        pub hash: [u8; 16],
        pub version_major: u16,
        pub version_minor: u16,
        /// The size of the whole container, this header included.
        pub container_size: u32,
        pub blob_count: u32,
    }

    /// `DXBCBlobHeader`: a part's four-character code and size, followed by its bytes.
    #[repr(C)]
    #[derive(Debug)]
    pub struct BlobHeader {
        pub fourcc: u32,
        pub size: u32,
    }

    const _: () = assert!(size_of::<Header>() == Header::SIZE);
    const _: () = assert!(size_of::<BlobHeader>() == BlobHeader::SIZE);

    impl Header {
        pub const SIZE: usize = 32;
        /// Where `container_size` sits (`DXBCSizeOffset`).
        pub const SIZE_OFFSET: usize = 24;

        /// Reads the header at `at`, if the bytes hold it.
        #[must_use]
        pub fn read(bytes: &[u8], at: usize) -> Option<Self> {
            let hash_at = at.checked_add(4)?;
            let hash = bytes.get(hash_at..hash_at.checked_add(16)?)?;
            Some(Self {
                fourcc: read_u32(bytes, at)?,
                hash: hash.try_into().ok()?,
                version_major: read_u16(bytes, at.checked_add(20)?)?,
                version_minor: read_u16(bytes, at.checked_add(22)?)?,
                container_size: read_u32(bytes, at.checked_add(Self::SIZE_OFFSET)?)?,
                blob_count: read_u32(bytes, at.checked_add(28)?)?,
            })
        }
    }

    impl BlobHeader {
        pub const SIZE: usize = 8;

        /// Reads the part header at `at`, if the bytes hold it.
        #[must_use]
        pub fn read(bytes: &[u8], at: usize) -> Option<Self> {
            Some(Self {
                fourcc: read_u32(bytes, at)?,
                size: read_u32(bytes, at.checked_add(4)?)?,
            })
        }
    }
}

/// How a read of guest bytes failed, in the terms a Direct3D caller answers with.
#[derive(Debug, PartialEq, Eq)]
pub enum Code {
    /// `E_FAIL`.
    Fail,
    /// `E_INVALIDARG`.
    InvalidArg,
    /// `E_OUTOFMEMORY`, where the C++ answered it for an input that allocates nothing.
    OutOfMemory,
}

/// A read of guest bytes that failed.
///
/// `what` is the text the translator reports: the C++'s own where it checked, and for a read the
/// C++ trusted (past its blob) `Invalid DXBC bytecode: ` and the thing that ran past.
#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    pub code: Code,
    pub what: &'static str,
}

/// A container whose header and part index `ReadDXBC` accepts.
#[derive(Debug)]
pub struct Container<'a> {
    bytes: &'a [u8],
    count: usize,
}

/// One part of a container.
#[derive(Debug, PartialEq, Eq)]
pub struct Part<'a> {
    pub fourcc: u32,
    pub data: &'a [u8],
}

impl Error {
    /// `ReadDXBC`'s refusal, as `SM50Initialize` words it.
    pub const INVALID: Self = Self::fail("Invalid DXBC bytecode");

    /// An `E_FAIL` with `what` as its text.
    #[must_use]
    pub const fn fail(what: &'static str) -> Self {
        Self {
            code: Code::Fail,
            what,
        }
    }

    /// An error with the given code and text.
    #[must_use]
    pub const fn new(code: Code, what: &'static str) -> Self {
        Self { code, what }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.what)
    }
}

impl<'a> Container<'a> {
    /// Checks the container as `ReadDXBC` does and keeps it.
    ///
    /// The size field must equal the slice's length, the version be 1.0, the index fit, and the
    /// parts follow the index and each other without a gap and end inside the container. The C++
    /// let a part overrun the container by one byte and let a 32-bit sum wrap; neither passes
    /// here.
    ///
    /// # Errors
    ///
    /// [`Error::INVALID`] for every refusal.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, Error> {
        let header = raw::Header::read(bytes, 0).ok_or(Error::INVALID)?;
        if usize::try_from(header.container_size) != Ok(bytes.len())
            || header.fourcc != fourcc::DXBC
            || header.version_major != 1
            || header.version_minor != 0
        {
            return Err(Error::INVALID);
        }
        let count = usize::try_from(header.blob_count).map_err(|_| Error::INVALID)?;
        let index_end = count
            .checked_mul(4)
            .and_then(|index| index.checked_add(raw::Header::SIZE))
            .filter(|&end| end <= bytes.len())
            .ok_or(Error::INVALID)?;
        // The first part starts right after the index, each next one where the last ended.
        let mut next = index_end;
        for part in 0..count {
            let offset = part
                .checked_mul(4)
                .and_then(|at| at.checked_add(raw::Header::SIZE))
                .and_then(|at| read_u32(bytes, at))
                .ok_or(Error::INVALID)?;
            if usize::try_from(offset) != Ok(next) {
                return Err(Error::INVALID);
            }
            let blob = raw::BlobHeader::read(bytes, next).ok_or(Error::INVALID)?;
            next = usize::try_from(blob.size)
                .ok()
                .and_then(|size| size.checked_add(next))
                .and_then(|end| end.checked_add(raw::BlobHeader::SIZE))
                .filter(|&end| end <= bytes.len())
                .ok_or(Error::INVALID)?;
        }
        Ok(Self { bytes, count })
    }

    /// The size a container's own header declares (`DXBCGetSizeAssumingValidPointer`).
    ///
    /// For a caller that holds a pointer and no length: it reads the header, makes the slice and
    /// calls [`Container::parse`].
    #[must_use]
    pub fn declared_size(header: &[u8]) -> Option<usize> {
        usize::try_from(read_u32(header, raw::Header::SIZE_OFFSET)?).ok()
    }

    /// The number of parts.
    #[must_use]
    pub const fn part_count(&self) -> usize {
        self.count
    }

    /// The part at `index`, in the container's order.
    #[must_use]
    pub fn part(&self, index: usize) -> Option<Part<'a>> {
        if index >= self.count {
            return None;
        }
        let offset = read_u32(
            self.bytes,
            index.checked_mul(4)?.checked_add(raw::Header::SIZE)?,
        )?;
        let at = usize::try_from(offset).ok()?;
        let blob = raw::BlobHeader::read(self.bytes, at)?;
        let start = at.checked_add(raw::BlobHeader::SIZE)?;
        let end = start.checked_add(usize::try_from(blob.size).ok()?)?;
        Some(Part {
            fourcc: blob.fourcc,
            data: self.bytes.get(start..end)?,
        })
    }

    /// The parts in order.
    pub fn parts(&self) -> impl Iterator<Item = Part<'a>> + '_ {
        (0..self.count).filter_map(|index| self.part(index))
    }

    /// The bytes of the first part with this code (`FindNextMatchingBlob` from 0).
    #[must_use]
    pub fn find(&self, fourcc: u32) -> Option<&'a [u8]> {
        self.parts()
            .find(|part| part.fourcc == fourcc)
            .map(|part| part.data)
    }
}

/// Lays out a container of the given parts, version 1.0 and a zero hash.
///
/// The counterpart of [`Container::parse`], for a caller that serializes (it sets the hash
/// itself) and for tests. `None` when a size does not fit the format's 32 bits.
#[must_use]
pub fn build(parts: &[(u32, &[u8])]) -> Option<Vec<u8>> {
    let index_end = raw::Header::SIZE.checked_add(parts.len().checked_mul(4)?)?;
    let mut index = Vec::new();
    let mut body = Vec::new();
    for &(fourcc, data) in parts {
        let offset = u32::try_from(index_end.checked_add(body.len())?).ok()?;
        index.extend_from_slice(&offset.to_le_bytes());
        body.extend_from_slice(&fourcc.to_le_bytes());
        body.extend_from_slice(&u32::try_from(data.len()).ok()?.to_le_bytes());
        body.extend_from_slice(data);
    }
    let total = u32::try_from(index_end.checked_add(body.len())?).ok()?;
    let mut out = Vec::with_capacity(index_end + body.len());
    out.extend_from_slice(&fourcc::DXBC.to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&u32::try_from(parts.len()).ok()?.to_le_bytes());
    out.extend_from_slice(&index);
    out.extend_from_slice(&body);
    Some(out)
}

/// The little-endian `u16` at `at`, if the bytes hold it.
#[must_use]
pub fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let end = at.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(at..end)?.try_into().ok()?))
}

/// The little-endian `u32` at `at`, if the bytes hold it.
#[must_use]
pub fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let end = at.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(at..end)?.try_into().ok()?))
}

#[cfg(test)]
mod tests;
