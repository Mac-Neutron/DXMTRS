// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The input, output and patch-constant signatures of a DXBC container.
//!
//! Ported from `CSignatureParser` and `CSignatureParser5` of `libs/DXBCParser` (Microsoft, MIT
//! licence), with the lookups no caller uses (`FindParameter`, `CanOutputTo`, the character sums)
//! left out. Names keep the case the blob has; system values are the code's
//! ([`sb_name`]), as `ConvertToSB` made them.

use crate::{
    container::{Container, Error, fourcc, read_u32},
    tokens::{name, sb_name},
};

/// One element of a signature.
#[derive(Debug, PartialEq, Eq)]
pub struct Parameter {
    /// The output stream; 0 for every signature that has no streams.
    pub stream: u32,
    /// The semantic name as the blob spells it, without its NUL. Not necessarily UTF-8.
    pub semantic_name: Vec<u8>,
    pub semantic_index: u32,
    /// The system value, as [`sb_name`] numbers it.
    pub system_value: u32,
    pub component_type: u32,
    pub register: u32,
    pub mask: u8,
    /// `NeverWrites_Mask` of an output and `AlwaysReads_Mask` of an input: one byte, two names.
    pub rw_mask: u8,
    pub min_precision: u32,
}

/// The elements of one signature, in the blob's order.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Signature {
    pub parameters: Vec<Parameter>,
}

/// An output signature that may be split in streams (`CSignatureParser5`).
#[derive(Debug, PartialEq, Eq)]
pub struct OutputSignature {
    streams: [Signature; STREAM_COUNT],
    num_streams: u32,
}

const STREAM_COUNT: usize = 4;

/// Where a parameter's fields sit in a blob's records.
struct Layout {
    stride: usize,
    /// Whether the record has a stream and the signature reads it.
    stream: Option<usize>,
    name: usize,
    index: usize,
    system_value: usize,
    component_type: usize,
    register: usize,
    mask: usize,
    rw_mask: usize,
    min_precision: Option<usize>,
}

/// `D3D10_INTERNALSHADER_PARAMETER`: ISGN, OSGN and PCSG.
const V4: Layout = Layout {
    stride: 24,
    stream: None,
    name: 0,
    index: 4,
    system_value: 8,
    component_type: 12,
    register: 16,
    mask: 20,
    rw_mask: 21,
    min_precision: None,
};

/// `D3D11_INTERNALSHADER_PARAMETER_11_1`: ISG1, OSG1 and PSG1.
const V11_1: Layout = Layout {
    stride: 32,
    stream: Some(0),
    name: 4,
    index: 8,
    system_value: 12,
    component_type: 16,
    register: 20,
    mask: 24,
    rw_mask: 25,
    min_precision: Some(28),
};

/// `D3D11_INTERNALSHADER_PARAMETER_FOR_GS`: OSG5.
const V5: Layout = Layout {
    stride: 28,
    stream: Some(0),
    name: 4,
    index: 8,
    system_value: 12,
    component_type: 16,
    register: 20,
    mask: 24,
    rw_mask: 25,
    min_precision: None,
};

/// The blob's header: the parameter count and where the records start.
const HEADER_SIZE: usize = 8;

/// The input signature (`DXBCGetInputSignature`): ISG1, else ISGN.
///
/// # Errors
///
/// A signature part that is missing, empty, short, or malformed in a way the C++ refused, or
/// that runs past its part.
pub fn input(container: &Container<'_>) -> Result<Signature, Error> {
    plain(
        container,
        &[(fourcc::ISG1, &V11_1), (fourcc::ISGN, &V4)],
        Error::fail("Invalid DXBC bytecode: input signature not found"),
    )
}

/// The output signature as D3D11's stream output reads it: OSG1, else OSGN.
///
/// This is `DXBCGetOutputSignature` into a `CSignatureParser`: every element is on stream 0,
/// whatever stream the blob gives it.
///
/// # Errors
///
/// As [`input`].
pub fn output_v4(container: &Container<'_>) -> Result<Signature, Error> {
    plain(
        container,
        &[(fourcc::OSG1, &V11_1), (fourcc::OSGN, &V4)],
        Error::fail("Invalid DXBC bytecode: output signature not found"),
    )
}

/// The output signature of a translated shader: OSG1, else OSGN, else OSG5, split in streams.
///
/// This is `DXBCGetOutputSignature` into a `CSignatureParser5`.
///
/// # Errors
///
/// As [`input`].
pub fn output_v5(container: &Container<'_>) -> Result<OutputSignature, Error> {
    let (blob, layout) = [
        (fourcc::OSG1, &V11_1),
        (fourcc::OSGN, &V4),
        (fourcc::OSG5, &V5),
    ]
    .into_iter()
    .find_map(|(part, layout)| container.find(part).map(|blob| (blob, layout)))
    .ok_or(Error::fail(
        "Invalid DXBC bytecode: output signature not found",
    ))?;
    let parameters = read(blob, layout)?;
    if layout.stream.is_some() {
        return split(parameters);
    }
    // OSGN has no streams: its one stream holds it all.
    let mut streams: [Signature; STREAM_COUNT] = core::array::from_fn(|_| Signature::default());
    if let Some(first) = streams.first_mut() {
        first.parameters = ordered(parameters)?;
    }
    Ok(OutputSignature {
        streams,
        num_streams: 1,
    })
}

/// The patch-constant signature (`DXBCGetPatchConstantSignature`): PSG1, else PCSG.
///
/// # Errors
///
/// As [`input`].
pub fn patch_constant(container: &Container<'_>) -> Result<Signature, Error> {
    plain(
        container,
        &[(fourcc::PSG1, &V11_1), (fourcc::PCSG, &V4)],
        Error::fail("Invalid DXBC bytecode: patch constant signature not found"),
    )
}

impl OutputSignature {
    /// How many streams the blob names (`NumStreams`).
    ///
    /// That is the last stream that holds an element, plus one.
    #[must_use]
    pub const fn num_streams(&self) -> u32 {
        self.num_streams
    }

    /// The stream the rasterizer reads (`RasterizedStream`): a blob never names one but 0.
    #[must_use]
    pub const fn rasterized_stream(&self) -> u32 {
        0
    }

    /// One of the four streams, empty past `num_streams` (`Signature`).
    #[must_use]
    pub fn stream(&self, stream: usize) -> Option<&Signature> {
        self.streams.get(stream)
    }

    /// The rasterized stream's signature (`RastSignature`).
    #[must_use]
    pub const fn rasterized(&self) -> &Signature {
        let [first, ..] = &self.streams;
        first
    }
}

/// Reads the first of `parts` the container has, every element on stream 0.
fn plain(
    container: &Container<'_>,
    parts: &[(u32, &Layout)],
    missing: Error,
) -> Result<Signature, Error> {
    let (blob, layout) = parts
        .iter()
        .find_map(|&(part, layout)| container.find(part).map(|blob| (blob, layout)))
        .ok_or(missing)?;
    Ok(Signature {
        parameters: ordered(read(blob, layout)?)?,
    })
}

/// `ReadSignature`'s rules for a signature without streams.
///
/// The stream is 0, and the registers do not decrease.
fn ordered(mut parameters: Vec<Parameter>) -> Result<Vec<Parameter>, Error> {
    for parameter in &mut parameters {
        parameter.stream = 0;
    }
    if parameters.is_sorted_by_key(|parameter| parameter.register) {
        Ok(parameters)
    } else {
        Err(Error::fail(
            "Invalid DXBC bytecode: signature registers decrease",
        ))
    }
}

/// `CSignatureParser5`'s split of the elements in streams.
///
/// Stream 0 takes the elements that name it, from the first on, stream 1 the next ones, and so
/// on; an element out of that order or past stream 3 belongs to no stream. Each stream's
/// registers do not decrease.
fn split(parameters: Vec<Parameter>) -> Result<OutputSignature, Error> {
    let mut streams: [Signature; STREAM_COUNT] = core::array::from_fn(|_| Signature::default());
    let mut num_streams = 0;
    let mut rest = parameters.into_iter().peekable();
    for (signature, stream) in streams.iter_mut().zip(0u32..) {
        let mut last = None;
        while let Some(parameter) = rest.next_if(|parameter| parameter.stream == stream) {
            if last.is_some_and(|last| last > parameter.register) {
                return Err(Error::fail(
                    "Invalid DXBC bytecode: signature registers decrease",
                ));
            }
            last = Some(parameter.register);
            num_streams = stream + 1;
            signature.parameters.push(parameter);
        }
    }
    Ok(OutputSignature {
        streams,
        num_streams,
    })
}

/// Reads a signature blob.
///
/// The header, then every record, each name checked to end inside the blob.
fn read(blob: &[u8], layout: &Layout) -> Result<Vec<Parameter>, Error> {
    const RAN_PAST: Error = Error::fail("Invalid DXBC bytecode: a signature ran past its blob");
    if blob.is_empty() {
        return Err(Error::fail(
            "Invalid DXBC bytecode: a signature part is empty",
        ));
    }
    let count = read_u32(blob, 0).ok_or(RAN_PAST)?;
    let records = read_u32(blob, 4).ok_or(RAN_PAST)?;
    if count == 0 {
        return Ok(Vec::new());
    }
    // The C++ checked that the records fit by their count alone, assuming they start at the
    // header's end; they are read from where the header says, so that must fit as well.
    let count = usize::try_from(count).map_err(|_| RAN_PAST)?;
    let size = count.checked_mul(layout.stride).ok_or(RAN_PAST)?;
    if size
        .checked_add(HEADER_SIZE)
        .is_none_or(|end| end > blob.len())
    {
        return Err(RAN_PAST);
    }
    let start = usize::try_from(records).map_err(|_| RAN_PAST)?;
    let records = start
        .checked_add(size)
        .and_then(|end| blob.get(start..end))
        .ok_or(RAN_PAST)?;
    records
        .chunks_exact(layout.stride)
        .map(|record| parameter(blob, record, layout))
        .collect()
}

fn parameter(blob: &[u8], record: &[u8], layout: &Layout) -> Result<Parameter, Error> {
    const RAN_PAST: Error = Error::fail("Invalid DXBC bytecode: a signature ran past its blob");
    let field = |at: usize| read_u32(record, at).ok_or(RAN_PAST);
    let byte = |at: usize| record.get(at).copied().ok_or(RAN_PAST);
    let name_at = usize::try_from(field(layout.name)?).map_err(|_| RAN_PAST)?;
    // `BoundedStringLength`: the name ends at a NUL inside the blob.
    let name_bytes = blob.get(name_at..).ok_or(RAN_PAST)?;
    let length = name_bytes
        .iter()
        .position(|&byte| byte == 0)
        .ok_or(Error::fail(
            "Invalid DXBC bytecode: a signature name is not terminated",
        ))?;
    let semantic_index = field(layout.index)?;
    Ok(Parameter {
        stream: layout.stream.map_or(Ok(0), field)?,
        semantic_name: name_bytes.get(..length).ok_or(RAN_PAST)?.to_vec(),
        semantic_index,
        system_value: system_value(field(layout.system_value)?, semantic_index),
        component_type: field(layout.component_type)?,
        register: field(layout.register)?,
        mask: byte(layout.mask)?,
        rw_mask: byte(layout.rw_mask)?,
        min_precision: layout.min_precision.map_or(Ok(0), field)?,
    })
}

/// `ConvertToSB`: the blob's system value as the code names it.
///
/// Render target, depth and coverage have no system value in the code; the tessellation factors
/// are told apart by the element's index; a value the C++ had no case for, or an index it had
/// none for, passes through.
const fn system_value(value: u32, index: u32) -> u32 {
    match (value, index) {
        (name::TARGET | name::DEPTH | name::COVERAGE | name::UNDEFINED, _) => sb_name::UNDEFINED,
        (name::FINAL_QUAD_EDGE_TESSFACTOR, 0) => sb_name::FINAL_QUAD_U_EQ_0_EDGE_TESSFACTOR,
        (name::FINAL_QUAD_EDGE_TESSFACTOR, 1) => sb_name::FINAL_QUAD_V_EQ_0_EDGE_TESSFACTOR,
        (name::FINAL_QUAD_EDGE_TESSFACTOR, 2) => sb_name::FINAL_QUAD_U_EQ_1_EDGE_TESSFACTOR,
        (name::FINAL_QUAD_EDGE_TESSFACTOR, 3) => sb_name::FINAL_QUAD_V_EQ_1_EDGE_TESSFACTOR,
        (name::FINAL_QUAD_INSIDE_TESSFACTOR, 0) => sb_name::FINAL_QUAD_U_INSIDE_TESSFACTOR,
        (name::FINAL_QUAD_INSIDE_TESSFACTOR, 1) => sb_name::FINAL_QUAD_V_INSIDE_TESSFACTOR,
        (name::FINAL_TRI_EDGE_TESSFACTOR, 0) => sb_name::FINAL_TRI_U_EQ_0_EDGE_TESSFACTOR,
        (name::FINAL_TRI_EDGE_TESSFACTOR, 1) => sb_name::FINAL_TRI_V_EQ_0_EDGE_TESSFACTOR,
        (name::FINAL_TRI_EDGE_TESSFACTOR, 2) => sb_name::FINAL_TRI_W_EQ_0_EDGE_TESSFACTOR,
        (name::FINAL_TRI_INSIDE_TESSFACTOR, 0) => sb_name::FINAL_TRI_INSIDE_TESSFACTOR,
        (name::FINAL_LINE_DETAIL_TESSFACTOR, 1) => sb_name::FINAL_LINE_DETAIL_TESSFACTOR,
        (name::FINAL_LINE_DENSITY_TESSFACTOR, 0) => sb_name::FINAL_LINE_DENSITY_TESSFACTOR,
        _ => value,
    }
}

#[cfg(test)]
mod tests;
