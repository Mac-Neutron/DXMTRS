//! Root signatures: serialization and each parameter's qword offset.
//!
//! Ported from `d3d12_root_signature.cpp`: the blob of `D3D12SerializeVersionedRootSignature` and
//! `D3D12SerializeRootSignature`, a DXBC container with one `RTS0` part whose records are those
//! of `dxbc_root_signature.hpp` (which step 4's `airconv-ir` reads back), and
//! `CalculateRootSignatureQwordOffsets`. Written ahead of D3D12's switch, which adds the exports,
//! the deserializer objects and the root signature object around them.
//!
//! The descriptions are the application's: every array is read with no alignment assumed, and a
//! null array whose count is not 0 is refused where the C++ would dereference it.

use dxmt_util::com::Hresult;

use crate::abi::{
    E_INVALIDARG, E_OUTOFMEMORY,
    root_signature::{
        D3D_ROOT_SIGNATURE_VERSION_1_0, D3D_ROOT_SIGNATURE_VERSION_1_1,
        D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS, D3D12_ROOT_PARAMETER_TYPE_CBV,
        D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, D3D12_ROOT_PARAMETER_TYPE_SRV,
        D3D12_ROOT_PARAMETER_TYPE_UAV, D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED,
        D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED, RootParameter, RootParameter1,
        RootSignatureDesc, RootSignatureDesc1, VersionedRootSignatureDesc,
    },
};

mod md5;

/// `RawRootSignatureDesc`'s size: the part's header.
const HEADER_SIZE: usize = 24;
/// `RawRootParameter`'s size: one parameter's record.
const PARAMETER_SIZE: usize = 12;
/// `RawRootDescriptorTable`'s size: a table's count and its ranges' offset.
const TABLE_SIZE: usize = 8;
/// The container's header, the part's offset and the part's own header before its bytes.
const CONTAINER_HEADER_SIZE: u32 = 44;
/// Where the container's header says the part (its fourcc) starts.
const PART_AT: u32 = 36;

/// Where each root parameter's arguments sit among the qwords a command list uploads.
pub struct QwordOffsets {
    /// Each parameter's first qword, in order; a parameter of an unknown type has none.
    pub parameters: Vec<u32>,
    /// The qwords in all.
    pub total: u32,
    /// The first of the two descriptor heaps' bases.
    ///
    /// `u32::MAX` when the flags index neither heap directly.
    pub heaps: u32,
    /// The lock table's address (64-bit atomics), the last qword.
    pub lock: u32,
}

/// `D3D12SerializeVersionedRootSignature`'s blob of `desc`.
///
/// # Errors
///
/// `E_INVALIDARG` for a version other than 1.0 and 1.1, as the C++, or for a null array whose
/// count is not 0; `E_OUTOFMEMORY` for a blob of 4 GiB or more.
///
/// # Safety
///
/// Each array of the description `desc.version` names, its parameters, their ranges and its
/// static samplers, is null or holds as many readable elements as its count says.
pub unsafe fn serialize_versioned(desc: &VersionedRootSignatureDesc) -> Result<Vec<u8>, Hresult> {
    match desc.version {
        D3D_ROOT_SIGNATURE_VERSION_1_0 => {
            // SAFETY: the version names the 1.0 arm, and either arm is integers and pointers.
            let desc_1_0 = unsafe { &desc.desc.desc_1_0 };
            // SAFETY: the caller's, for the arm the version names.
            unsafe { serialize_1_0(desc_1_0) }
        }
        D3D_ROOT_SIGNATURE_VERSION_1_1 => {
            // SAFETY: the version names the 1.1 arm, and either arm is integers and pointers.
            let desc_1_1 = unsafe { &desc.desc.desc_1_1 };
            // SAFETY: the caller's, for the arm the version names.
            unsafe { serialize_1_1(desc_1_1) }
        }
        _ => Err(E_INVALIDARG),
    }
}

/// `D3D12SerializeRootSignature`'s blob of `desc`, which takes version 1.0 only.
///
/// # Errors
///
/// `E_INVALIDARG` for a version other than 1.0, as the C++, or for a null array whose count is
/// not 0; `E_OUTOFMEMORY` for a blob of 4 GiB or more.
///
/// # Safety
///
/// Each array of `desc`, its parameters, their ranges and its static samplers, is null or holds
/// as many readable elements as its count says.
pub unsafe fn serialize(desc: &RootSignatureDesc, version: u32) -> Result<Vec<u8>, Hresult> {
    if version != D3D_ROOT_SIGNATURE_VERSION_1_0 {
        return Err(E_INVALIDARG);
    }
    // SAFETY: the caller's.
    unsafe { serialize_1_0(desc) }
}

/// `CalculateRootSignatureQwordOffsets`: where `desc`'s arguments sit among the uploaded qwords.
///
/// One qword per table and root descriptor, a constant parameter's values two to a qword, then,
/// when the flags index a descriptor heap directly, the two heaps' bases, and always the lock
/// table's address last.
///
/// # Errors
///
/// `E_INVALIDARG` for null parameters whose count is not 0, as `Initialize` answers before it.
///
/// # Safety
///
/// `desc.p_parameters` is null or holds `desc.num_parameters` readable parameters.
pub unsafe fn qword_offsets(desc: &RootSignatureDesc1) -> Result<QwordOffsets, Hresult> {
    // SAFETY: the caller's.
    let bytes = unsafe { array(desc.p_parameters, desc.num_parameters) }?;
    let mut parameters = Vec::with_capacity(bytes.len() / size_of::<RootParameter1>());
    let mut current = 0_u32;
    // SAFETY: a parameter is integers, pointers and a union of them.
    for parameter in unsafe { elements::<RootParameter1>(bytes) } {
        match parameter.parameter_type {
            D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE
            | D3D12_ROOT_PARAMETER_TYPE_CBV
            | D3D12_ROOT_PARAMETER_TYPE_SRV
            | D3D12_ROOT_PARAMETER_TYPE_UAV => {
                parameters.push(current);
                current = current.wrapping_add(1);
            }
            D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS => {
                // SAFETY: the type names the constants arm, and every arm is integers and pointers.
                let values = unsafe { parameter.payload.constants.num_32bit_values };
                parameters.push(current);
                current = current.wrapping_add(values / 2 + (values & 1));
            }
            _ => {}
        }
    }
    let mut heaps = u32::MAX;
    if desc.flags
        & (D3D12_ROOT_SIGNATURE_FLAG_CBV_SRV_UAV_HEAP_DIRECTLY_INDEXED
            | D3D12_ROOT_SIGNATURE_FLAG_SAMPLER_HEAP_DIRECTLY_INDEXED)
        != 0
    {
        heaps = current;
        current = current.wrapping_add(2);
    }
    let lock = current;
    Ok(QwordOffsets {
        parameters,
        total: current.wrapping_add(1),
        heaps,
        lock,
    })
}

/// `SerializeVersionedRootSignatureImpl` for a 1.0 description.
///
/// Its ranges are 20 bytes, its root descriptors 8: the records are the description's own.
///
/// # Safety
///
/// As [`serialize`].
unsafe fn serialize_1_0(desc: &RootSignatureDesc) -> Result<Vec<u8>, Hresult> {
    // SAFETY: the caller's.
    let bytes = unsafe { array(desc.p_parameters, desc.num_parameters) }?;
    let mut part = Part::new(
        D3D_ROOT_SIGNATURE_VERSION_1_0,
        desc.flags,
        desc.num_parameters,
        desc.num_static_samplers,
    )?;
    // SAFETY: a parameter is integers, pointers and a union of them.
    for (i, parameter) in unsafe { elements::<RootParameter>(bytes) }.enumerate() {
        let (kind, visibility) = (parameter.parameter_type, parameter.shader_visibility);
        match kind {
            D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE => {
                // SAFETY: the type names the table arm, and every arm is integers and pointers.
                let table = unsafe { &parameter.payload.descriptor_table };
                let count = table.num_descriptor_ranges;
                // SAFETY: the caller's.
                let ranges = unsafe { array(table.p_descriptor_ranges, count) }?;
                part.table(i, kind, visibility, count, ranges)?;
            }
            D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS => {
                // SAFETY: the type names the constants arm, and every arm is integers and pointers.
                let c = unsafe { &parameter.payload.constants };
                let words = [c.shader_register, c.register_space, c.num_32bit_values];
                part.payload(i, kind, visibility, &words)?;
            }
            D3D12_ROOT_PARAMETER_TYPE_CBV
            | D3D12_ROOT_PARAMETER_TYPE_SRV
            | D3D12_ROOT_PARAMETER_TYPE_UAV => {
                // SAFETY: the type names the descriptor arm, and every arm is integers and pointers.
                let d = unsafe { &parameter.payload.descriptor };
                part.payload(i, kind, visibility, &[d.shader_register, d.register_space])?;
            }
            _ => part.record(i, kind, visibility, 0),
        }
    }
    // SAFETY: the caller's.
    let samplers = unsafe { array(desc.p_static_samplers, desc.num_static_samplers) }?;
    part.finish(samplers)
}

/// `SerializeVersionedRootSignatureImpl` for a 1.1 description.
///
/// Its ranges are 24 bytes, its root descriptors 12, each with its flags.
///
/// # Safety
///
/// As [`serialize`].
unsafe fn serialize_1_1(desc: &RootSignatureDesc1) -> Result<Vec<u8>, Hresult> {
    // SAFETY: the caller's.
    let bytes = unsafe { array(desc.p_parameters, desc.num_parameters) }?;
    let mut part = Part::new(
        D3D_ROOT_SIGNATURE_VERSION_1_1,
        desc.flags,
        desc.num_parameters,
        desc.num_static_samplers,
    )?;
    // SAFETY: a parameter is integers, pointers and a union of them.
    for (i, parameter) in unsafe { elements::<RootParameter1>(bytes) }.enumerate() {
        let (kind, visibility) = (parameter.parameter_type, parameter.shader_visibility);
        match kind {
            D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE => {
                // SAFETY: the type names the table arm, and every arm is integers and pointers.
                let table = unsafe { &parameter.payload.descriptor_table };
                let count = table.num_descriptor_ranges;
                // SAFETY: the caller's.
                let ranges = unsafe { array(table.p_descriptor_ranges, count) }?;
                part.table(i, kind, visibility, count, ranges)?;
            }
            D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS => {
                // SAFETY: the type names the constants arm, and every arm is integers and pointers.
                let c = unsafe { &parameter.payload.constants };
                let words = [c.shader_register, c.register_space, c.num_32bit_values];
                part.payload(i, kind, visibility, &words)?;
            }
            D3D12_ROOT_PARAMETER_TYPE_CBV
            | D3D12_ROOT_PARAMETER_TYPE_SRV
            | D3D12_ROOT_PARAMETER_TYPE_UAV => {
                // SAFETY: the type names the descriptor arm, and every arm is integers and pointers.
                let d = unsafe { &parameter.payload.descriptor };
                let words = [d.shader_register, d.register_space, d.flags];
                part.payload(i, kind, visibility, &words)?;
            }
            _ => part.record(i, kind, visibility, 0),
        }
    }
    // SAFETY: the caller's.
    let samplers = unsafe { array(desc.p_static_samplers, desc.num_static_samplers) }?;
    part.finish(samplers)
}

/// The `RTS0` part as `SerializeVersionedRootSignatureImpl` lays it out.
///
/// The header, every parameter's record, each parameter's payload in order, then the static
/// samplers. Offsets count from the part's start; words are little-endian, as every target
/// stores them.
struct Part(Vec<u8>);

impl Part {
    /// The header and room for `num_parameters` records.
    ///
    /// The header's static samplers' offset is [`Part::finish`]'s.
    fn new(
        version: u32,
        flags: u32,
        num_parameters: u32,
        num_static_samplers: u32,
    ) -> Result<Self, Hresult> {
        let records = usize::try_from(num_parameters)
            .ok()
            .and_then(|n| n.checked_mul(PARAMETER_SIZE))
            .ok_or(E_OUTOFMEMORY)?;
        let mut part = Self(Vec::with_capacity(HEADER_SIZE + records));
        // RawRootSignatureDesc: Version, NumParameters, RootParametersOffset, NumStaticSamplers,
        // StaticSamplersOffset, Flags.
        let parameters_at = offset(HEADER_SIZE)?;
        part.push(&[
            version,
            num_parameters,
            parameters_at,
            num_static_samplers,
            0,
            flags,
        ]);
        part.0.resize(HEADER_SIZE + records, 0);
        Ok(part)
    }

    /// Parameter `i`'s record.
    ///
    /// `RawRootParameter`: `ParameterType`, `ShaderVisibility`, `PayloadOffset`, which is 0 for a
    /// type without a payload, as the C++ leaves it.
    fn record(&mut self, i: usize, kind: u32, visibility: u32, payload_at: u32) {
        let at = HEADER_SIZE + i * PARAMETER_SIZE;
        for (j, word) in [kind, visibility, payload_at].into_iter().enumerate() {
            self.0[at + 4 * j..at + 4 * j + 4].copy_from_slice(&word.to_le_bytes());
        }
    }

    /// Parameter `i`'s record and its constants or root descriptor, `words`.
    fn payload(
        &mut self,
        i: usize,
        kind: u32,
        visibility: u32,
        words: &[u32],
    ) -> Result<(), Hresult> {
        let at = offset(self.0.len())?;
        self.record(i, kind, visibility, at);
        self.push(words);
        Ok(())
    }

    /// Parameter `i`'s record, its table and the table's ranges.
    ///
    /// `RawRootDescriptorTable`: `NumDescriptorRanges`, `DescriptorRangesOffset`; the ranges'
    /// bytes follow it.
    fn table(
        &mut self,
        i: usize,
        kind: u32,
        visibility: u32,
        count: u32,
        ranges: &[u8],
    ) -> Result<(), Hresult> {
        let at = offset(self.0.len())?;
        let ranges_at = offset(self.0.len() + TABLE_SIZE)?;
        self.record(i, kind, visibility, at);
        self.push(&[count, ranges_at]);
        self.0.extend_from_slice(ranges);
        Ok(())
    }

    /// The static samplers, then the container around the part.
    ///
    /// The header gets the samplers' offset, the container its checksum.
    fn finish(mut self, samplers: &[u8]) -> Result<Vec<u8>, Hresult> {
        let samplers_at = offset(self.0.len())?;
        self.0[16..20].copy_from_slice(&samplers_at.to_le_bytes());
        self.0.extend_from_slice(samplers);
        let size = offset(self.0.len())?;
        let total = size
            .checked_add(CONTAINER_HEADER_SIZE)
            .ok_or(E_OUTOFMEMORY)?;
        let mut blob = Vec::new();
        // DXBCHeader: the magic, the checksum (below), version 1.0, the size, one part.
        blob.extend_from_slice(b"DXBC");
        blob.extend_from_slice(&[0; 16]);
        blob.extend_from_slice(&1_u16.to_le_bytes());
        blob.extend_from_slice(&0_u16.to_le_bytes());
        blob.extend_from_slice(&total.to_le_bytes());
        blob.extend_from_slice(&1_u32.to_le_bytes());
        // The part's offset, then DXBCBlobHeader: its fourcc and size.
        blob.extend_from_slice(&PART_AT.to_le_bytes());
        blob.extend_from_slice(b"RTS0");
        blob.extend_from_slice(&size.to_le_bytes());
        blob.extend_from_slice(&self.0);
        let hash = md5::dxbc_hash(&blob);
        blob[4..20].copy_from_slice(&hash);
        Ok(blob)
    }

    /// Appends `words`.
    fn push(&mut self, words: &[u32]) {
        for word in words {
            self.0.extend_from_slice(&word.to_le_bytes());
        }
    }
}

/// `len` as a part offset or size, which the format keeps in 32 bits.
fn offset(len: usize) -> Result<u32, Hresult> {
    u32::try_from(len).map_err(|_| E_OUTOFMEMORY)
}

/// The bytes of an application's array of `count` `T`s at `ptr`, which promises no alignment.
///
/// # Errors
///
/// `E_INVALIDARG` for a null `ptr` with a `count` other than 0.
///
/// # Safety
///
/// `ptr` is null or holds `count` readable `T`s, which stay unchanged while the bytes are used.
unsafe fn array<'a, T>(ptr: *const T, count: u32) -> Result<&'a [u8], Hresult> {
    if count == 0 {
        return Ok(&[]);
    }
    if ptr.is_null() {
        return Err(E_INVALIDARG);
    }
    let len = usize::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(size_of::<T>()))
        .ok_or(E_INVALIDARG)?;
    // SAFETY: the caller's: `ptr` holds `count` `T`s, `len` bytes, and a byte needs no alignment.
    Ok(unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), len) })
}

/// The `T`s in `bytes`, each read with no alignment assumed.
///
/// # Safety
///
/// Any bytes make a valid `T` (integers, floats, pointers and unions of them).
unsafe fn elements<T>(bytes: &[u8]) -> impl Iterator<Item = T> + '_ {
    bytes.chunks_exact(size_of::<T>()).map(|chunk| {
        // SAFETY: the chunk holds `size_of::<T>()` bytes, which make a `T` (the caller's).
        unsafe { chunk.as_ptr().cast::<T>().read_unaligned() }
    })
}

#[cfg(test)]
mod tests;
