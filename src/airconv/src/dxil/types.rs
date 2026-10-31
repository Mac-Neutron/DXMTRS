//! DXIL's types and constants.
//!
//! Ported from `A/dxil/dxil_types.hpp`. DXC's enumerations (`DxilConstants.h`) stay raw `u32`s:
//! the metadata casts any integer into them and every reader has a default for a value it does
//! not know, so a closed Rust enum would refuse what the C++ passed on.

/// `ShaderKind`: the stage `dx.shaderModel` names (the DXBC program types' numbers).
pub mod shader_kind {
    pub const PIXEL: u32 = 0;
    pub const VERTEX: u32 = 1;
    pub const GEOMETRY: u32 = 2;
    pub const HULL: u32 = 3;
    pub const DOMAIN: u32 = 4;
    pub const COMPUTE: u32 = 5;
    pub const LIBRARY: u32 = 6;
    pub const INVALID: u32 = u32::MAX;
}

/// `ResourceClass`.
pub mod resource_class {
    pub const SRV: u32 = 0;
    pub const UAV: u32 = 1;
    pub const CBUFFER: u32 = 2;
    pub const SAMPLER: u32 = 3;
}

/// `ResourceKind`.
pub mod resource_kind {
    pub const TEXTURE_1D: u32 = 1;
    pub const TEXTURE_2D: u32 = 2;
    pub const TEXTURE_2D_MS: u32 = 3;
    pub const TEXTURE_3D: u32 = 4;
    pub const TEXTURE_CUBE: u32 = 5;
    pub const TEXTURE_1D_ARRAY: u32 = 6;
    pub const TEXTURE_2D_ARRAY: u32 = 7;
    pub const TEXTURE_2D_MS_ARRAY: u32 = 8;
    pub const TEXTURE_CUBE_ARRAY: u32 = 9;
    pub const TYPED_BUFFER: u32 = 10;
    pub const RAW_BUFFER: u32 = 11;
    pub const STRUCTURED_BUFFER: u32 = 12;
    pub const CBUFFER: u32 = 13;
    pub const SAMPLER: u32 = 14;
}

/// `ComponentType`.
pub mod component_type {
    pub const I1: u32 = 1;
    pub const I16: u32 = 2;
    pub const U16: u32 = 3;
    pub const I32: u32 = 4;
    pub const U32: u32 = 5;
    pub const I64: u32 = 6;
    pub const U64: u32 = 7;
    pub const F32: u32 = 9;
}

/// `SemanticKind`.
pub mod semantic_kind {
    pub const ARBITRARY: u32 = 0;
    pub const VERTEX_ID: u32 = 1;
    pub const INSTANCE_ID: u32 = 2;
    pub const POSITION: u32 = 3;
    pub const RENDER_TARGET_ARRAY_INDEX: u32 = 4;
    pub const VIEWPORT_ARRAY_INDEX: u32 = 5;
    pub const PRIMITIVE_ID: u32 = 10;
    pub const GS_INSTANCE_ID: u32 = 11;
    pub const SAMPLE_INDEX: u32 = 12;
    pub const IS_FRONT_FACE: u32 = 13;
    pub const COVERAGE: u32 = 14;
    pub const TARGET: u32 = 16;
    pub const DEPTH: u32 = 17;
    pub const DEPTH_LESS_EQUAL: u32 = 18;
    pub const DEPTH_GREATER_EQUAL: u32 = 19;
    pub const STENCIL_REF: u32 = 20;
}

/// `Interpolation`.
pub mod interpolation {
    pub const CONSTANT: u32 = 1;
    pub const LINEAR_CENTROID: u32 = 3;
    pub const LINEAR_NOPERSPECTIVE: u32 = 4;
    pub const LINEAR_NOPERSPECTIVE_CENTROID: u32 = 5;
    pub const LINEAR_SAMPLE: u32 = 6;
    pub const LINEAR_NOPERSPECTIVE_SAMPLE: u32 = 7;
}

/// `dx.op` opcodes: the first argument of every `dx.op` call.
pub mod op {
    pub const LOAD_INPUT: u32 = 4;
    pub const STORE_OUTPUT: u32 = 5;
    pub const F_ABS: u32 = 6;
    pub const SATURATE: u32 = 7;
    pub const IS_NAN: u32 = 8;
    pub const IS_INF: u32 = 9;
    pub const IS_FINITE: u32 = 10;
    pub const IS_NORMAL: u32 = 11;
    pub const COS: u32 = 12;
    pub const SIN: u32 = 13;
    pub const TAN: u32 = 14;
    pub const ACOS: u32 = 15;
    pub const ASIN: u32 = 16;
    pub const ATAN: u32 = 17;
    pub const HCOS: u32 = 18;
    pub const HSIN: u32 = 19;
    pub const HTAN: u32 = 20;
    pub const EXP: u32 = 21;
    pub const FRC: u32 = 22;
    pub const LOG: u32 = 23;
    pub const SQRT: u32 = 24;
    pub const RSQRT: u32 = 25;
    pub const ROUND_NE: u32 = 26;
    pub const ROUND_NI: u32 = 27;
    pub const ROUND_PI: u32 = 28;
    pub const ROUND_Z: u32 = 29;
    pub const BFREV: u32 = 30;
    pub const COUNTBITS: u32 = 31;
    pub const FIRSTBIT_LO: u32 = 32;
    pub const FIRSTBIT_HI: u32 = 33;
    pub const FIRSTBIT_SHI: u32 = 34;
    pub const F_MAX: u32 = 35;
    pub const F_MIN: u32 = 36;
    pub const I_MAX: u32 = 37;
    pub const I_MIN: u32 = 38;
    pub const U_MAX: u32 = 39;
    pub const U_MIN: u32 = 40;
    pub const I_MUL: u32 = 41;
    pub const U_MUL: u32 = 42;
    pub const U_DIV: u32 = 43;
    pub const U_ADDC: u32 = 44;
    pub const U_SUBB: u32 = 45;
    pub const F_MAD: u32 = 46;
    pub const FMA: u32 = 47;
    pub const I_MAD: u32 = 48;
    pub const U_MAD: u32 = 49;
    pub const MSAD: u32 = 50;
    pub const IBFE: u32 = 51;
    pub const UBFE: u32 = 52;
    pub const BFI: u32 = 53;
    pub const DOT2: u32 = 54;
    pub const DOT3: u32 = 55;
    pub const DOT4: u32 = 56;
    pub const CREATE_HANDLE: u32 = 57;
    pub const C_BUFFER_LOAD: u32 = 58;
    pub const C_BUFFER_LOAD_LEGACY: u32 = 59;
    pub const SAMPLE: u32 = 60;
    pub const SAMPLE_BIAS: u32 = 61;
    pub const SAMPLE_LEVEL: u32 = 62;
    pub const SAMPLE_GRAD: u32 = 63;
    pub const SAMPLE_CMP: u32 = 64;
    pub const SAMPLE_CMP_LEVEL_ZERO: u32 = 65;
    pub const TEXTURE_LOAD: u32 = 66;
    pub const TEXTURE_STORE: u32 = 67;
    pub const BUFFER_LOAD: u32 = 68;
    pub const BUFFER_STORE: u32 = 69;
    pub const BUFFER_UPDATE_COUNTER: u32 = 70;
    pub const CHECK_ACCESS_FULLY_MAPPED: u32 = 71;
    pub const GET_DIMENSIONS: u32 = 72;
    pub const TEXTURE_GATHER: u32 = 73;
    pub const TEXTURE_GATHER_CMP: u32 = 74;
    pub const TEXTURE_2D_MS_GET_SAMPLE_POSITION: u32 = 75;
    pub const RENDER_TARGET_GET_SAMPLE_POSITION: u32 = 76;
    pub const RENDER_TARGET_GET_SAMPLE_COUNT: u32 = 77;
    pub const ATOMIC_BIN_OP: u32 = 78;
    pub const ATOMIC_COMPARE_EXCHANGE: u32 = 79;
    pub const BARRIER: u32 = 80;
    pub const CALCULATE_LOD: u32 = 81;
    pub const DISCARD: u32 = 82;
    pub const DERIV_COARSE_X: u32 = 83;
    pub const DERIV_COARSE_Y: u32 = 84;
    pub const DERIV_FINE_X: u32 = 85;
    pub const DERIV_FINE_Y: u32 = 86;
    pub const SAMPLE_INDEX: u32 = 90;
    pub const COVERAGE: u32 = 91;
    pub const THREAD_ID: u32 = 93;
    pub const GROUP_ID: u32 = 94;
    pub const THREAD_ID_IN_GROUP: u32 = 95;
    pub const FLATTENED_THREAD_ID_IN_GROUP: u32 = 96;
    pub const EMIT_STREAM: u32 = 97;
    pub const CUT_STREAM: u32 = 98;
    pub const EMIT_THEN_CUT_STREAM: u32 = 99;
    pub const GS_INSTANCE_ID: u32 = 100;
    pub const MAKE_DOUBLE: u32 = 101;
    pub const SPLIT_DOUBLE: u32 = 102;
    pub const PRIMITIVE_ID: u32 = 108;
    pub const WAVE_IS_FIRST_LANE: u32 = 110;
    pub const WAVE_GET_LANE_INDEX: u32 = 111;
    pub const WAVE_GET_LANE_COUNT: u32 = 112;
    pub const WAVE_ANY_TRUE: u32 = 113;
    pub const WAVE_ALL_TRUE: u32 = 114;
    pub const WAVE_ACTIVE_ALL_EQUAL: u32 = 115;
    pub const WAVE_ACTIVE_BALLOT: u32 = 116;
    pub const WAVE_READ_LANE_AT: u32 = 117;
    pub const WAVE_READ_LANE_FIRST: u32 = 118;
    pub const WAVE_ACTIVE_OP: u32 = 119;
    pub const WAVE_ACTIVE_BIT: u32 = 120;
    pub const WAVE_PREFIX_OP: u32 = 121;
    pub const QUAD_READ_LANE_AT: u32 = 122;
    pub const QUAD_OP: u32 = 123;
    pub const BITCAST_I16_TO_F16: u32 = 124;
    pub const BITCAST_F16_TO_I16: u32 = 125;
    pub const BITCAST_I32_TO_F32: u32 = 126;
    pub const BITCAST_F32_TO_I32: u32 = 127;
    pub const LEGACY_F32_TO_F16: u32 = 130;
    pub const LEGACY_F16_TO_F32: u32 = 131;
    pub const WAVE_ALL_BIT_COUNT: u32 = 135;
    pub const WAVE_PREFIX_BIT_COUNT: u32 = 136;
    pub const VIEW_ID: u32 = 138;
    pub const RAW_BUFFER_LOAD: u32 = 139;
    pub const RAW_BUFFER_STORE: u32 = 140;
    pub const DOT2_ADD_HALF: u32 = 162;
    pub const DOT4_ADD_I8_PACKED: u32 = 163;
    pub const DOT4_ADD_U8_PACKED: u32 = 164;
    pub const WAVE_MATCH: u32 = 165;
    pub const WAVE_MULTI_PREFIX_OP: u32 = 166;
    pub const WAVE_MULTI_PREFIX_BIT_COUNT: u32 = 167;
    pub const ANNOTATE_HANDLE: u32 = 216;
    pub const CREATE_HANDLE_FROM_BINDING: u32 = 217;
    pub const CREATE_HANDLE_FROM_HEAP: u32 = 218;
    pub const UNPACK4X8: u32 = 219;
    pub const PACK4X8: u32 = 220;
    pub const IS_HELPER_LANE: u32 = 221;
    pub const QUAD_VOTE: u32 = 222;
    pub const BARRIER_BY_MEMORY_TYPE: u32 = 244;
    pub const BARRIER_BY_MEMORY_HANDLE: u32 = 245;
    pub const SAMPLE_CMP_GRAD: u32 = 254;
    pub const SAMPLE_CMP_BIAS: u32 = 255;
    pub const START_VERTEX_LOCATION: u32 = 256;
    pub const START_INSTANCE_LOCATION: u32 = 257;
}

/// A heap range's register space (no root parameter's).
pub const HEAP_SPACE: u32 = u32::MAX;
/// The first heap range's range ID.
pub const HEAP_RANGE_BASE: u32 = 1 << 24;
/// `wave_size_min` of a `WaveSize` or `WaveSizeRange` node too short to read.
///
/// Initialize refuses it.
pub const MALFORMED_WAVE_SIZE: u32 = u32::MAX;
/// A vertex shader's unpacked `SV_VertexID` and `SV_InstanceID` (`unpacked_inputs` values).
///
/// They are read from the IO map instead of an argument.
pub const UNPACKED_VERTEX_ID: u32 = u32::MAX;
pub const UNPACKED_INSTANCE_ID: u32 = u32::MAX - 1;

/// An entry point input or output (`SignatureElement`).
///
/// Elements live in airconv's register file at `(start_row + row, start_col + col)`;
/// `start_row` -1 is a system value that is not packed. `semantic_indices` holds at least `rows`
/// indices (the reader pads with 0).
pub struct SignatureElement {
    pub id: u32,
    /// The semantic name, any bytes (`TEXCOORD`).
    pub name: Vec<u8>,
    pub semantic_indices: Vec<u32>,
    /// A [`component_type`].
    pub ty: u32,
    /// A [`semantic_kind`].
    pub kind: u32,
    /// An [`interpolation`].
    pub interpolation: u32,
    pub rows: u32,
    pub cols: u32,
    pub start_row: i32,
    pub start_col: i32,
}

/// A resource range of the entry point, or a descriptor heap range (`Resource`).
///
/// `size` `u32::MAX` is unbounded. A heap range (`ResourceDescriptorHeap`, op 218) is space
/// [`HEAP_SPACE`], lower bound 0, unbounded, one per distinct `annotateHandle` properties
/// (`heap_props`).
pub struct Resource {
    /// The range ID: `createHandle`'s `rangeId`, airconv's `RangeId`.
    pub id: u32,
    /// A [`resource_class`].
    pub cls: u32,
    /// A [`resource_kind`].
    pub kind: u32,
    pub space: u32,
    pub lower_bound: u32,
    pub size: u32,
    /// A typed texture's or buffer's [`component_type`].
    pub element_type: u32,
    /// A structured buffer's stride.
    pub stride: u32,
    /// A constant buffer's size in bytes.
    pub cbuffer_size: u32,
    pub flags: ResourceFlags,
    pub heap_props: [u32; 2],
}

/// A resource's flags, one bit each.
#[derive(Clone, Copy)]
pub struct ResourceFlags(u8);

impl ResourceFlags {
    pub const GLOBALLY_COHERENT: Self = Self(1);
    pub const HAS_COUNTER: Self = Self(1 << 1);
    pub const RASTERIZER_ORDERED: Self = Self(1 << 2);
    pub const COMPARISON_SAMPLER: Self = Self(1 << 3);
    /// A descriptor heap range.
    pub const HEAP: Self = Self(1 << 4);
    pub const NONE: Self = Self(0);

    /// Whether every bit of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Sets `other`'s bits when `on`.
    pub const fn set(&mut self, other: Self, on: bool) {
        if on {
            self.0 |= other.0;
        }
    }
}

/// The entry point: its stage, function, signatures, resources and properties (`EntryInfo`).
pub struct EntryInfo {
    /// A [`shader_kind`].
    pub kind: u32,
    pub sm_major: u32,
    pub sm_minor: u32,
    /// The entry function's name.
    pub name: Vec<u8>,
    pub numthreads: [u32; 3],
    /// `[WaveSize]`'s widths, 0 without it; [`MALFORMED_WAVE_SIZE`] for a node too short.
    pub wave_size_min: u32,
    pub wave_size_max: u32,
    /// `GSState`: the primitive and topology are `D3D10_SB_PRIMITIVE(_TOPOLOGY)`'s values.
    pub gs_input_primitive: u32,
    pub gs_max_vertex_count: u32,
    pub gs_stream_mask: u32,
    pub gs_output_topology: u32,
    pub gs_instances: u32,
    pub inputs: Vec<SignatureElement>,
    pub outputs: Vec<SignatureElement>,
    /// All four classes, then the heap ranges.
    pub resources: Vec<Resource>,
}

impl Default for EntryInfo {
    /// `EntryInfo`'s initial values: an invalid stage, one geometry shader instance.
    fn default() -> Self {
        Self {
            kind: shader_kind::INVALID,
            sm_major: 0,
            sm_minor: 0,
            name: Vec::new(),
            numthreads: [0; 3],
            wave_size_min: 0,
            wave_size_max: 0,
            gs_input_primitive: 0,
            gs_max_vertex_count: 0,
            gs_stream_mask: 0,
            gs_output_topology: 0,
            gs_instances: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            resources: Vec::new(),
        }
    }
}

/// `UserName`: a varying's name, its semantic upper-cased and its row's index (`TEXCOORD0`).
///
/// Varyings link by semantic, not register. A row past the indices names itself.
#[must_use]
pub fn user_name(e: &SignatureElement, row: u32) -> Vec<u8> {
    let index = usize::try_from(row)
        .ok()
        .and_then(|r| e.semantic_indices.get(r))
        .copied()
        .unwrap_or(row);
    [e.name.to_ascii_uppercase(), index.to_string().into_bytes()].concat()
}

/// The low 32 bits of a metadata integer, as the C++'s assignment to `uint32_t` keeps them.
#[must_use]
pub const fn low32(v: i64) -> u32 {
    let [b0, b1, b2, b3, ..] = v.to_le_bytes();
    u32::from_le_bytes([b0, b1, b2, b3])
}

#[cfg(test)]
mod tests;
