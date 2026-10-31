//! Root signatures: the RTS0 part and its 1.0 and 1.1 descriptions.
//!
//! Ported from `RootSignatureDeserializer` of `dxbc_root_signature.hpp`. The blob is read in the
//! version it holds and the other version's description is made from it, with the flags
//! `SetFlag` gives a 1.0 description's ranges and descriptors; every read is checked against the
//! blob, where the C++ trusted the offsets in it.

use crate::container::{Code, Container, Error, fourcc, read_u32};

pub const VERSION_1_0: u32 = 1;
pub const VERSION_1_1: u32 = 2;
pub const PARAMETER_TYPE_DESCRIPTOR_TABLE: u32 = 0;
pub const PARAMETER_TYPE_32BIT_CONSTANTS: u32 = 1;
pub const PARAMETER_TYPE_CBV: u32 = 2;
pub const PARAMETER_TYPE_SRV: u32 = 3;
pub const PARAMETER_TYPE_UAV: u32 = 4;
pub const RANGE_TYPE_SAMPLER: u32 = 3;
pub const RANGE_FLAG_DESCRIPTORS_VOLATILE: u32 = 0x1;
pub const RANGE_FLAG_DATA_VOLATILE: u32 = 0x2;
pub const DESCRIPTOR_FLAG_DATA_VOLATILE: u32 = 0x2;

/// The blob's records, as `#[repr(C)]` types with the C++'s sizes.
pub mod raw {
    use super::words;

    /// `RawRootSignatureDesc`: the first 24 bytes of the RTS0 part.
    ///
    /// Every offset in the blob counts from the start of the part.
    #[repr(C)]
    #[derive(Debug)]
    pub struct RootSignatureDesc {
        pub version: u32,
        pub num_parameters: u32,
        pub root_parameters_offset: u32,
        pub num_static_samplers: u32,
        pub static_samplers_offset: u32,
        pub flags: u32,
    }

    /// `RawRootParameter`: one entry of the parameter array.
    #[repr(C)]
    #[derive(Debug)]
    pub struct RootParameter {
        pub parameter_type: u32,
        pub shader_visibility: u32,
        /// Where the parameter's table, constants or descriptor sit.
        pub payload_offset: u32,
    }

    /// `RawRootDescriptorTable`: a descriptor table's payload.
    #[repr(C)]
    #[derive(Debug)]
    pub struct RootDescriptorTable {
        pub num_descriptor_ranges: u32,
        pub descriptor_ranges_offset: u32,
    }

    const _: () = assert!(size_of::<RootSignatureDesc>() == RootSignatureDesc::SIZE);
    const _: () = assert!(size_of::<RootParameter>() == RootParameter::SIZE);
    const _: () = assert!(size_of::<RootDescriptorTable>() == RootDescriptorTable::SIZE);

    impl RootSignatureDesc {
        pub const SIZE: usize = 24;

        /// Reads the header at `at`, if the bytes hold it.
        #[must_use]
        pub fn read(blob: &[u8], at: usize) -> Option<Self> {
            let [
                version,
                num_parameters,
                root_parameters_offset,
                num_static_samplers,
                static_samplers_offset,
                flags,
            ] = words(blob, at)?;
            Some(Self {
                version,
                num_parameters,
                root_parameters_offset,
                num_static_samplers,
                static_samplers_offset,
                flags,
            })
        }
    }

    impl RootParameter {
        pub const SIZE: usize = 12;

        /// Reads the entry at `at`, if the bytes hold it.
        #[must_use]
        pub fn read(blob: &[u8], at: usize) -> Option<Self> {
            let [parameter_type, shader_visibility, payload_offset] = words(blob, at)?;
            Some(Self {
                parameter_type,
                shader_visibility,
                payload_offset,
            })
        }
    }

    impl RootDescriptorTable {
        pub const SIZE: usize = 8;

        /// Reads the table at `at`, if the bytes hold it.
        #[must_use]
        pub fn read(blob: &[u8], at: usize) -> Option<Self> {
            let [num_descriptor_ranges, descriptor_ranges_offset] = words(blob, at)?;
            Some(Self {
                num_descriptor_ranges,
                descriptor_ranges_offset,
            })
        }
    }
}

/// A root signature in both versions' descriptions.
#[derive(Debug)]
pub struct RootSignature {
    /// The version the blob holds, [`VERSION_1_0`] or [`VERSION_1_1`].
    pub version: u32,
    /// The 1.0 description; its ranges and descriptors have no flags (0).
    pub v1_0: Description,
    pub v1_1: Description,
    /// The static samplers, which both versions' descriptions share.
    pub static_samplers: Vec<StaticSampler>,
}

/// A root signature's flags and parameters in one version.
#[derive(Debug)]
pub struct Description {
    pub flags: u32,
    pub parameters: Vec<Parameter>,
}

/// One root parameter.
#[derive(Debug)]
pub struct Parameter {
    pub visibility: u32,
    pub payload: Payload,
}

/// What a root parameter holds.
#[derive(Debug)]
pub enum Payload {
    Table(Vec<Range>),
    Constants(Constants),
    Cbv(Descriptor),
    Srv(Descriptor),
    Uav(Descriptor),
    /// A parameter type the format does not have; the C++ kept it with nothing in it.
    Unknown(u32),
}

/// A descriptor table's range.
#[derive(Debug)]
pub struct Range {
    pub range_type: u32,
    pub num_descriptors: u32,
    pub base_register: u32,
    pub space: u32,
    /// `RANGE_FLAG_*`; 0 in a 1.0 description.
    pub flags: u32,
    pub offset: u32,
}

/// A parameter of 32-bit root constants.
#[derive(Debug)]
pub struct Constants {
    pub register: u32,
    pub space: u32,
    pub values: u32,
}

/// A root CBV, SRV or UAV.
#[derive(Debug)]
pub struct Descriptor {
    pub register: u32,
    pub space: u32,
    /// `DESCRIPTOR_FLAG_*`; 0 in a 1.0 description.
    pub flags: u32,
}

/// `D3D12_STATIC_SAMPLER_DESC`: the enumerations stay the integers the blob holds.
#[derive(Debug)]
pub struct StaticSampler {
    pub filter: u32,
    pub address_u: u32,
    pub address_v: u32,
    pub address_w: u32,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
    pub comparison: u32,
    pub border_color: u32,
    pub min_lod: f32,
    pub max_lod: f32,
    pub register: u32,
    pub space: u32,
    pub visibility: u32,
}

const RAN_PAST: Error = Error::fail("Invalid DXBC bytecode: a root signature ran past its part");
const RANGE_STRIDE_1_0: usize = 20;
const RANGE_STRIDE_1_1: usize = 24;
const SAMPLER_STRIDE: usize = 52;

/// The root signature part of a container (`DXBCGetRootSignature`).
///
/// # Errors
///
/// The container has no RTS0 part, or an empty one.
pub fn from_container<'a>(container: &Container<'a>) -> Result<&'a [u8], Error> {
    container
        .find(fourcc::RTS0)
        .filter(|part| !part.is_empty())
        .ok_or(Error::fail(
            "Invalid DXBC bytecode: root signature not found",
        ))
}

impl RootSignature {
    /// Reads the RTS0 part's bytes.
    ///
    /// # Errors
    ///
    /// [`Code::InvalidArg`] for a version but 1.0 and 1.1; [`Code::OutOfMemory`] for a
    /// descriptor table without ranges, which the C++ answered so, as it allocated nothing for
    /// them; [`Code::Fail`] for any read past the part.
    pub fn deserialize(blob: &[u8]) -> Result<Self, Error> {
        let version = read_u32(blob, 0).ok_or(RAN_PAST)?;
        let from_1_1 = match version {
            VERSION_1_0 => false,
            VERSION_1_1 => true,
            _ => {
                return Err(Error::new(
                    Code::InvalidArg,
                    "Invalid DXBC bytecode: the root signature has an unsupported version",
                ));
            }
        };
        let desc = raw::RootSignatureDesc::read(blob, 0).ok_or(RAN_PAST)?;
        let read = Description {
            flags: desc.flags,
            parameters: read_parameters(blob, &desc, from_1_1)?,
        };
        let converted = Description {
            flags: desc.flags,
            parameters: convert(&read.parameters, !from_1_1)?,
        };
        let (v1_0, v1_1) = if from_1_1 {
            (converted, read)
        } else {
            (read, converted)
        };
        Ok(Self {
            version,
            v1_0,
            v1_1,
            static_samplers: read_samplers(blob, &desc)?,
        })
    }
}

impl Parameter {
    /// The parameter type as the blob numbers it (`PARAMETER_TYPE_*`).
    #[must_use]
    pub const fn parameter_type(&self) -> u32 {
        match self.payload {
            Payload::Table(_) => PARAMETER_TYPE_DESCRIPTOR_TABLE,
            Payload::Constants(_) => PARAMETER_TYPE_32BIT_CONSTANTS,
            Payload::Cbv(_) => PARAMETER_TYPE_CBV,
            Payload::Srv(_) => PARAMETER_TYPE_SRV,
            Payload::Uav(_) => PARAMETER_TYPE_UAV,
            Payload::Unknown(kind) => kind,
        }
    }
}

/// `N` consecutive little-endian words at `at`.
fn words<const N: usize>(blob: &[u8], at: usize) -> Option<[u32; N]> {
    let mut out = [0; N];
    for (index, word) in out.iter_mut().enumerate() {
        *word = read_u32(blob, at.checked_add(index * 4)?)?;
    }
    Some(out)
}

/// Where an array of `count` records of `stride` bytes starts, if the blob holds it.
///
/// An empty array is never read, wherever its offset points.
fn array(blob: &[u8], offset: u32, count: u32, stride: usize) -> Result<(usize, usize), Error> {
    let count = usize::try_from(count).map_err(|_| RAN_PAST)?;
    let start = usize::try_from(offset).map_err(|_| RAN_PAST)?;
    if count > 0
        && count
            .checked_mul(stride)
            .and_then(|size| size.checked_add(start))
            .is_none_or(|end| end > blob.len())
    {
        return Err(RAN_PAST);
    }
    Ok((start, count))
}

fn read_parameters(
    blob: &[u8],
    desc: &raw::RootSignatureDesc,
    v1_1: bool,
) -> Result<Vec<Parameter>, Error> {
    let (start, count) = array(
        blob,
        desc.root_parameters_offset,
        desc.num_parameters,
        raw::RootParameter::SIZE,
    )?;
    (0..count)
        .map(|index| {
            let at = start + index * raw::RootParameter::SIZE;
            let entry = raw::RootParameter::read(blob, at).ok_or(RAN_PAST)?;
            let payload_at = usize::try_from(entry.payload_offset).map_err(|_| RAN_PAST)?;
            let payload = match entry.parameter_type {
                PARAMETER_TYPE_DESCRIPTOR_TABLE => {
                    Payload::Table(read_ranges(blob, payload_at, v1_1)?)
                }
                PARAMETER_TYPE_32BIT_CONSTANTS => {
                    let [register, space, values] = words(blob, payload_at).ok_or(RAN_PAST)?;
                    Payload::Constants(Constants {
                        register,
                        space,
                        values,
                    })
                }
                PARAMETER_TYPE_CBV => Payload::Cbv(read_descriptor(blob, payload_at, v1_1)?),
                PARAMETER_TYPE_SRV => Payload::Srv(read_descriptor(blob, payload_at, v1_1)?),
                PARAMETER_TYPE_UAV => Payload::Uav(read_descriptor(blob, payload_at, v1_1)?),
                other => Payload::Unknown(other),
            };
            Ok(Parameter {
                visibility: entry.shader_visibility,
                payload,
            })
        })
        .collect()
}

fn read_descriptor(blob: &[u8], at: usize, v1_1: bool) -> Result<Descriptor, Error> {
    if v1_1 {
        let [register, space, flags] = words(blob, at).ok_or(RAN_PAST)?;
        Ok(Descriptor {
            register,
            space,
            flags,
        })
    } else {
        let [register, space] = words(blob, at).ok_or(RAN_PAST)?;
        Ok(Descriptor {
            register,
            space,
            flags: 0,
        })
    }
}

fn read_ranges(blob: &[u8], table_at: usize, v1_1: bool) -> Result<Vec<Range>, Error> {
    let table = raw::RootDescriptorTable::read(blob, table_at).ok_or(RAN_PAST)?;
    let stride = if v1_1 {
        RANGE_STRIDE_1_1
    } else {
        RANGE_STRIDE_1_0
    };
    let (start, count) = array(
        blob,
        table.descriptor_ranges_offset,
        table.num_descriptor_ranges,
        stride,
    )?;
    (0..count)
        .map(|index| {
            let at = start + index * stride;
            if v1_1 {
                let [
                    range_type,
                    num_descriptors,
                    base_register,
                    space,
                    flags,
                    offset,
                ] = words(blob, at).ok_or(RAN_PAST)?;
                Ok(Range {
                    range_type,
                    num_descriptors,
                    base_register,
                    space,
                    flags,
                    offset,
                })
            } else {
                let [range_type, num_descriptors, base_register, space, offset] =
                    words(blob, at).ok_or(RAN_PAST)?;
                Ok(Range {
                    range_type,
                    num_descriptors,
                    base_register,
                    space,
                    flags: 0,
                    offset,
                })
            }
        })
        .collect()
}

fn read_samplers(blob: &[u8], desc: &raw::RootSignatureDesc) -> Result<Vec<StaticSampler>, Error> {
    let (start, count) = array(
        blob,
        desc.static_samplers_offset,
        desc.num_static_samplers,
        SAMPLER_STRIDE,
    )?;
    (0..count)
        .map(|index| {
            let at = start + index * SAMPLER_STRIDE;
            let [
                filter,
                address_u,
                address_v,
                address_w,
                mip_lod_bias,
                max_anisotropy,
                comparison,
                border_color,
                min_lod,
                max_lod,
                register,
                space,
                visibility,
            ] = words(blob, at).ok_or(RAN_PAST)?;
            Ok(StaticSampler {
                filter,
                address_u,
                address_v,
                address_w,
                mip_lod_bias: f32::from_bits(mip_lod_bias),
                max_anisotropy,
                comparison,
                border_color,
                min_lod: f32::from_bits(min_lod),
                max_lod: f32::from_bits(max_lod),
                register,
                space,
                visibility,
            })
        })
        .collect()
}

/// `ConvertVersionedRootSignature`: the other version's parameters.
///
/// Going to 1.1, a range gets the flags `SetFlag` gives (data and descriptors volatile, the
/// descriptors alone for a sampler range) and a root descriptor gets data volatile; going to 1.0
/// the flags go. A descriptor table without ranges is refused, as the C++'s allocation of none
/// was.
fn convert(from: &[Parameter], to_1_1: bool) -> Result<Vec<Parameter>, Error> {
    from.iter()
        .map(|parameter| {
            let descriptor = |from: &Descriptor| Descriptor {
                register: from.register,
                space: from.space,
                flags: if to_1_1 {
                    DESCRIPTOR_FLAG_DATA_VOLATILE
                } else {
                    0
                },
            };
            let payload = match &parameter.payload {
                Payload::Table(ranges) if ranges.is_empty() => {
                    return Err(Error::new(
                        Code::OutOfMemory,
                        "Invalid DXBC bytecode: a descriptor table has no ranges",
                    ));
                }
                Payload::Table(ranges) => Payload::Table(
                    ranges
                        .iter()
                        .map(|range| Range {
                            range_type: range.range_type,
                            num_descriptors: range.num_descriptors,
                            base_register: range.base_register,
                            space: range.space,
                            flags: match (to_1_1, range.range_type) {
                                (false, _) => 0,
                                (true, RANGE_TYPE_SAMPLER) => RANGE_FLAG_DESCRIPTORS_VOLATILE,
                                (true, _) => {
                                    RANGE_FLAG_DESCRIPTORS_VOLATILE | RANGE_FLAG_DATA_VOLATILE
                                }
                            },
                            offset: range.offset,
                        })
                        .collect(),
                ),
                Payload::Constants(constants) => Payload::Constants(Constants {
                    register: constants.register,
                    space: constants.space,
                    values: constants.values,
                }),
                Payload::Cbv(from) => Payload::Cbv(descriptor(from)),
                Payload::Srv(from) => Payload::Srv(descriptor(from)),
                Payload::Uav(from) => Payload::Uav(descriptor(from)),
                Payload::Unknown(kind) => Payload::Unknown(*kind),
            };
            Ok(Parameter {
                visibility: parameter.visibility,
                payload,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
