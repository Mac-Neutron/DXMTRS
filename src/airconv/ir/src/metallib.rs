//! The metallib container: a module's bitcode and its function list, packed and parsed.
//!
//! Ported from `metallib_writer.cpp` of DXMT's airconv, which wrote the container Metal reads a
//! library from: an 88-byte header, the function list (one run of tags per function, each a
//! four-character code and a 16-bit length), the public and private metadata of every function,
//! and the bitcode. Like the C++, it writes one version of everything (metallib 2.7 for macOS
//! 14.4, AIR 2.6, Metal 3.1) and gives every function the SHA-256 of the whole bitcode.

use core::fmt;

use crate::sha256;

/// The first 16 header bytes: the magic, macOS, metallib 2.7, an executable, macOS 14.4.
///
/// Nine 64-bit sizes and offsets follow, up to [`HEADER_LEN`].
const HEADER_PREFIX: [u8; 16] = [
    b'M', b'T', b'L', b'B', 0x01, 0x80, 2, 0, 7, 0, 0x00, 0x81, 14, 0, 4, 0,
];
const HEADER_LEN: usize = 88;
/// The end of a function's tags, of the function list and of a function's metadata.
const END: [u8; 4] = *b"ENDT";
/// AIR 2.6 and Metal language 3.1, as two 16-bit pairs.
const VERSIONS: [u8; 8] = [2, 0, 6, 0, 3, 0, 1, 0];
/// A vertex input's last byte: the usage and padding bits clear, `active` set.
const ATTRIBUTE_ACTIVE: u8 = 0x80;

/// A metallib [`parse`] could not read.
#[derive(Debug, PartialEq, Eq)]
pub struct Error {
    what: &'static str,
}

/// What a function of the list is.
#[derive(Debug, PartialEq, Eq)]
pub enum FunctionKind {
    Vertex,
    Fragment,
    Kernel,
    Object,
    Mesh,
}

impl FunctionKind {
    const fn tag(&self) -> u8 {
        match self {
            Self::Vertex => 0x00,
            Self::Fragment => 0x01,
            Self::Kernel => 0x02,
            Self::Mesh => 0x07,
            Self::Object => 0x08,
        }
    }

    const fn from_tag(tag: u8) -> Result<Self, Error> {
        match tag {
            0x00 => Ok(Self::Vertex),
            0x01 => Ok(Self::Fragment),
            0x02 => Ok(Self::Kernel),
            0x07 => Ok(Self::Mesh),
            0x08 => Ok(Self::Object),
            _ => Err(Error::new("unknown function type")),
        }
    }
}

/// One vertex input of a vertex function.
#[derive(Debug, PartialEq, Eq)]
pub struct VertexAttribute {
    /// The attribute index, an `air.vertex_input`'s location.
    pub attribute: u8,
    pub name: Vec<u8>,
    /// The type code [`VertexAttribute::data_type_for`] gives a type name.
    pub data_type: u8,
}

/// The patch type of a vertex function that feeds a tessellator.
#[derive(Debug, PartialEq, Eq)]
pub enum Patch {
    Triangle,
    Quad,
}

/// A function of the list, with what its metadata carries.
#[derive(Debug, PartialEq, Eq)]
pub struct FunctionEntry {
    /// The name as bytes: LLVM names are not UTF-8 by promise, and a guest names the entry.
    pub name: Vec<u8>,
    pub kind: FunctionKind,
    /// The inputs of a vertex function; with none, no vertex attribute metadata is written.
    pub vertex_attributes: Vec<VertexAttribute>,
    /// The patch type of a vertex function that feeds a tessellator.
    pub patch: Option<Patch>,
}

impl VertexAttribute {
    /// The type code of an `air.vertex_input`'s type name, as the C++ writer chose it.
    ///
    /// Only `float4` and `uint4` have their own code; every other name gets the one the C++
    /// hoped would do in general.
    #[must_use]
    pub const fn data_type_for(type_name: &[u8]) -> u8 {
        match type_name {
            b"float4" => 0x06,
            b"uint4" => 0x24,
            _ => 0x20,
        }
    }
}

impl Patch {
    /// The patch type an `air.patch` tuple names: a triangle if it says so, otherwise a quad.
    #[must_use]
    pub const fn from_air_name(name: &[u8]) -> Self {
        match name {
            b"triangle" => Self::Triangle,
            _ => Self::Quad,
        }
    }

    /// The `TESS` byte: the patch type in two bits and the control point count (always 0) above.
    const fn tag(&self) -> u8 {
        match self {
            Self::Triangle => 1,
            Self::Quad => 2,
        }
    }

    const fn from_tag(tag: u8) -> Result<Self, Error> {
        match tag {
            1 => Ok(Self::Triangle),
            2 => Ok(Self::Quad),
            _ => Err(Error::new("unknown tessellation tag")),
        }
    }
}

impl Error {
    const TRUNCATED: Self = Self::new("truncated");

    const fn new(what: &'static str) -> Self {
        Self { what }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.what)
    }
}

/// Packs `bitcode` and its function list into a metallib.
///
/// The functions are written in slice order, and every one gets the SHA-256 of `bitcode`. The C++
/// walked the module's named metadata as `air.vertex`, `air.fragment`, `air.kernel`, `air.object`
/// and `air.mesh`; a caller that must match its bytes lists the functions in that order. Sizes
/// that do not fit their field wrap as the C++'s casts did (a name of 65535 bytes has length 0).
#[must_use]
pub fn pack(bitcode: &[u8], functions: &[FunctionEntry]) -> Vec<u8> {
    let hash = sha256::digest(bitcode);
    let mut list = Vec::new();
    let mut public = Vec::new();
    let mut private = Vec::new();
    for function in functions {
        function.write_list_entry(&mut list, &hash, bitcode.len(), public.len(), private.len());
        function.write_public(&mut public);
        private.extend_from_slice(&le32(END.len()));
        private.extend_from_slice(&END);
    }
    // The size of the list counts its end tag; the count and the size field come before the
    // list, and the end tag after it.
    let list_size = list.len() + END.len();
    let public_offset = HEADER_LEN + 4 + 4 + list_size;
    let private_offset = public_offset + public.len();
    let bitcode_offset = private_offset + private.len();
    let file_size = bitcode_offset + bitcode.len();

    let mut out = Vec::with_capacity(file_size);
    out.extend_from_slice(&HEADER_PREFIX);
    for field in [
        file_size,
        HEADER_LEN,
        list_size,
        public_offset,
        public.len(),
        private_offset,
        private.len(),
        bitcode_offset,
        bitcode.len(),
    ] {
        out.extend_from_slice(&le64(field));
    }
    out.extend_from_slice(&le32(functions.len()));
    out.extend_from_slice(&le32(list_size));
    out.extend_from_slice(&list);
    out.extend_from_slice(&END);
    out.extend_from_slice(&public);
    out.extend_from_slice(&private);
    out.extend_from_slice(bitcode);
    out
}

/// Reads a metallib back into its bitcode and function list.
///
/// This reads what [`pack`] writes and nothing more general: for a file it accepts,
/// `pack(&bitcode, &functions)` gives the file again, apart from the hashes it recomputes.
///
/// # Errors
///
/// A file that is not a metallib, one cut short or with offsets past its end, a tag [`pack`]
/// does not write, or a function without a name, a kind or a metadata block.
pub fn parse(file: &[u8]) -> Result<(Vec<u8>, Vec<FunctionEntry>), Error> {
    let mut header = Reader::new(file);
    if header.array::<4>()? != *b"MTLB" {
        return Err(Error::new("not a metallib"));
    }
    header.take(HEADER_PREFIX.len() - 4)?;
    let _file_size = header.u64()?;
    let list_offset = header.u64()?;
    let _list_size = header.u64()?;
    let public_offset = header.u64()?;
    let public_size = header.u64()?;
    let _private_offset = header.u64()?;
    let _private_size = header.u64()?;
    let bitcode_offset = header.u64()?;
    let bitcode_size = header.u64()?;

    let bitcode = region(file, bitcode_offset, bitcode_size)?.to_vec();
    let public = region(file, public_offset, public_size)?;
    let mut list = Reader::new(file.get(to_usize(list_offset)?..).ok_or(Error::TRUNCATED)?);
    let count = list.u32()?;
    let _list_size = list.u32()?;
    let mut functions = Vec::new();
    for _ in 0..count {
        functions.push(read_function(&mut list, public)?);
    }
    Ok((bitcode, functions))
}

impl FunctionEntry {
    /// Appends this function's run of tags to the function list.
    fn write_list_entry(
        &self,
        list: &mut Vec<u8>,
        hash: &[u8; 32],
        bitcode_len: usize,
        public_offset: usize,
        private_offset: usize,
    ) {
        put_tag(list, *b"NAME", &[&self.name, &[0]]);
        put_tag(list, *b"TYPE", &[&[self.kind.tag()]]);
        put_tag(list, *b"HASH", &[hash]);
        put_tag(list, *b"MDSZ", &[&le64(bitcode_len)]);
        // The bitcode offset stays 0, as in the C++.
        put_tag(
            list,
            *b"OFFT",
            &[&le64(public_offset), &le64(private_offset), &le64(0)],
        );
        put_tag(list, *b"VERS", &[&VERSIONS]);
        if let Some(patch) = &self.patch {
            put_tag(list, *b"TESS", &[&[patch.tag()]]);
        }
        list.extend_from_slice(&END);
    }

    /// Appends this function's public metadata: its length, the vertex inputs if it has any.
    fn write_public(&self, public: &mut Vec<u8>) {
        let mut metadata = Vec::new();
        if !self.vertex_attributes.is_empty() {
            let mut names = le16(self.vertex_attributes.len()).to_vec();
            let mut types = names.clone();
            for attribute in &self.vertex_attributes {
                names.extend_from_slice(&attribute.name);
                names.extend_from_slice(&[0, attribute.attribute, ATTRIBUTE_ACTIVE]);
                types.push(attribute.data_type);
            }
            put_tag(&mut metadata, *b"VATT", &[&names]);
            put_tag(&mut metadata, *b"VATY", &[&types]);
        }
        metadata.extend_from_slice(&END);
        public.extend_from_slice(&le32(metadata.len()));
        public.extend_from_slice(&metadata);
    }
}

/// Appends a tag: its code, the length of `body` as 16 bits, and `body`'s parts.
fn put_tag(out: &mut Vec<u8>, tag: [u8; 4], body: &[&[u8]]) {
    out.extend_from_slice(&tag);
    out.extend_from_slice(&le16(body.iter().map(|part| part.len()).sum()));
    for part in body {
        out.extend_from_slice(part);
    }
}

/// The low 16 bits of `n`, little endian: the C++'s `(uint16_t)` cast.
const fn le16(n: usize) -> [u8; 2] {
    let [b0, b1, ..] = n.to_le_bytes();
    [b0, b1]
}

/// The low 32 bits of `n`, little endian: the C++'s `(uint32_t)` cast.
const fn le32(n: usize) -> [u8; 4] {
    let [b0, b1, b2, b3, ..] = n.to_le_bytes();
    [b0, b1, b2, b3]
}

const fn le64(n: usize) -> [u8; 8] {
    (n as u64).to_le_bytes()
}

fn to_usize(n: u64) -> Result<usize, Error> {
    usize::try_from(n).map_err(|_| Error::TRUNCATED)
}

/// `len` bytes of `file` from `offset`.
fn region(file: &[u8], offset: u64, len: u64) -> Result<&[u8], Error> {
    let start = to_usize(offset)?;
    let end = start.checked_add(to_usize(len)?).ok_or(Error::TRUNCATED)?;
    file.get(start..end).ok_or(Error::TRUNCATED)
}

/// Reads the next function of the list.
fn read_function(list: &mut Reader<'_>, public: &[u8]) -> Result<FunctionEntry, Error> {
    let (mut name, mut kind, mut public_offset, mut patch) = (None, None, None, None);
    loop {
        let tag = list.array::<4>()?;
        if tag == END {
            break;
        }
        let mut body = list.sized()?;
        match &tag {
            b"NAME" => {
                let terminated = body.rest.strip_suffix(&[0]);
                name = Some(
                    terminated
                        .ok_or(Error::new("a name lacks its end"))?
                        .to_vec(),
                );
            }
            b"TYPE" => kind = Some(FunctionKind::from_tag(body.u8()?)?),
            b"OFFT" => public_offset = Some(body.u64()?),
            b"TESS" => patch = Some(Patch::from_tag(body.u8()?)?),
            // `pack` writes these from the bitcode, so there is nothing to keep.
            b"HASH" | b"MDSZ" | b"VERS" => {}
            _ => return Err(Error::new("unknown function tag")),
        }
    }
    let name = name.ok_or(Error::new("a function has no name"))?;
    let kind = kind.ok_or(Error::new("a function has no type"))?;
    let public_offset = public_offset.ok_or(Error::new("a function has no offsets"))?;
    let vertex_attributes = read_vertex_attributes(public, public_offset)?;
    Ok(FunctionEntry {
        name,
        kind,
        vertex_attributes,
        patch,
    })
}

/// Reads the vertex inputs of the public metadata block at `offset`.
fn read_vertex_attributes(public: &[u8], offset: u64) -> Result<Vec<VertexAttribute>, Error> {
    let mut block = Reader::new(public.get(to_usize(offset)?..).ok_or(Error::TRUNCATED)?);
    let len = to_usize(block.u32()?.into())?;
    let mut metadata = Reader::new(block.take(len)?);
    let mut inputs = Vec::new();
    let mut types: &[u8] = &[];
    loop {
        match &metadata.array::<4>()? {
            b"ENDT" => break,
            b"VATT" => {
                let mut body = metadata.sized()?;
                for _ in 0..body.u16()? {
                    let name = body.cstr()?.to_vec();
                    let attribute = body.u8()?;
                    if body.u8()? != ATTRIBUTE_ACTIVE {
                        return Err(Error::new("a vertex input is not plainly active"));
                    }
                    inputs.push((attribute, name));
                }
            }
            b"VATY" => {
                let mut body = metadata.sized()?;
                let count = usize::from(body.u16()?);
                types = body.take(count)?;
            }
            _ => return Err(Error::new("unknown metadata tag")),
        }
    }
    if inputs.len() != types.len() {
        return Err(Error::new("vertex inputs and their types differ in number"));
    }
    Ok(inputs
        .into_iter()
        .zip(types)
        .map(|((attribute, name), &data_type)| VertexAttribute {
            attribute,
            name,
            data_type,
        })
        .collect())
}

/// A cursor over bytes that never reads past their end.
struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    const fn new(rest: &'a [u8]) -> Self {
        Self { rest }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let (head, tail) = self.rest.split_at_checked(len).ok_or(Error::TRUNCATED)?;
        self.rest = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let (head, tail) = self.rest.split_first_chunk().ok_or(Error::TRUNCATED)?;
        self.rest = tail;
        Ok(*head)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        self.array().map(|[byte]| byte)
    }

    fn u16(&mut self) -> Result<u16, Error> {
        self.array().map(u16::from_le_bytes)
    }

    fn u32(&mut self) -> Result<u32, Error> {
        self.array().map(u32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, Error> {
        self.array().map(u64::from_le_bytes)
    }

    /// The body of a tag: a 16-bit length, then that many bytes.
    fn sized(&mut self) -> Result<Self, Error> {
        let len = usize::from(self.u16()?);
        self.take(len).map(Self::new)
    }

    /// The bytes up to a NUL, which is consumed.
    fn cstr(&mut self) -> Result<&'a [u8], Error> {
        let len = self.rest.iter().position(|&byte| byte == 0);
        let name = self.take(len.ok_or(Error::TRUNCATED)?)?;
        self.take(1)?;
        Ok(name)
    }
}

#[cfg(test)]
mod tests;
