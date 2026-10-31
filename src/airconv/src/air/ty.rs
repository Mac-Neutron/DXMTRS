//! AIR's types: Metal's texture, sampler and buffer types as LLVM sees them.
//!
//! Ported from `air_type.*`. [`AirTypes::new`] makes them in `AirType`'s order: the identified
//! structs are named in the context as they are made (LLVM suffixes a name already taken), so the
//! order is the C++'s, every one of them included.

use core::ffi::CStr;

use crate::llvm::{Ty, TyCx};

/// Metal's device address space (`AddressSpace::device`).
const DEVICE: u32 = 1;

/// The types the translator names, made once for a context (`AirType`).
///
/// Each field is the C++ member of the same name without its `_`; a table of handles, read
/// directly as the C++ reads `types._int`.
pub struct AirTypes {
    /// `i1`.
    pub bool: Ty,
    /// `i8`.
    pub byte: Ty,
    /// `<2 x i8>`.
    pub char2: Ty,
    /// `<4 x i8>`.
    pub char4: Ty,
    /// `i16`.
    pub short: Ty,
    /// `<2 x i16>`.
    pub short2: Ty,
    /// `<4 x i16>`.
    pub short4: Ty,
    /// `float`.
    pub float: Ty,
    /// `<2 x float>`.
    pub float2: Ty,
    /// `<3 x float>`.
    pub float3: Ty,
    /// `<4 x float>`.
    pub float4: Ty,
    /// `i32`.
    pub int: Ty,
    /// `<2 x i32>`.
    pub int2: Ty,
    /// `<3 x i32>`.
    pub int3: Ty,
    /// `<4 x i32>`.
    pub int4: Ty,
    /// `half`.
    pub half: Ty,
    /// `<2 x half>`.
    pub half2: Ty,
    /// `<4 x half>`.
    pub half4: Ty,
    /// `dxmt_vertex_buffer_entry`: a device byte pointer, the stride and the length.
    pub dxmt_vertex_buffer_entry: Ty,
    /// `dxmt_draw_arguments`: `D3D11_DRAW_INSTANCED_INDIRECT_ARGS`' four words.
    pub dxmt_draw_arguments: Ty,
    /// `dxmt_draw_indexed_arguments`: the indexed form's five words.
    pub dxmt_draw_indexed_arguments: Ty,
    /// `dxmt_stream_output_buffer_entry`: two device `i32` pointers.
    pub dxmt_stream_output_buffer_entry: Ty,
}

impl AirTypes {
    /// The types of `tcx`'s context, made in `AirType::AirType`'s order.
    ///
    /// The four `dxmt_*` structs are new each time (`StructType::create`); the others are found
    /// by name first (`get_or_create_struct`).
    #[must_use]
    pub fn new(tcx: &TyCx) -> Self {
        let int = tcx.int(32);
        let float = tcx.float();
        let bool = tcx.int(1);
        let byte = tcx.int(8);
        let long = tcx.int(64);
        let _ = tcx.vector(long, 4);
        let half = tcx.half();
        let half2 = tcx.vector(half, 2);
        let _ = tcx.vector(half, 3);
        let half4 = tcx.vector(half, 4);
        let short = tcx.int(16);
        let int4 = tcx.vector(int, 4);
        let _ = tcx.vector(int, 8);
        let float4 = tcx.vector(float, 4);
        let short4 = tcx.vector(short, 4);
        let char4 = tcx.vector(byte, 4);
        let int3 = tcx.vector(int, 3);
        let _ = tcx.vector(short, 3);
        let float3 = tcx.vector(float, 3);
        let _ = tcx.vector(byte, 3);
        let int2 = tcx.vector(int, 2);
        let short2 = tcx.vector(short, 2);
        let float2 = tcx.vector(float, 2);
        let char2 = tcx.vector(byte, 2);
        let byte_ptr = tcx.ptr(byte, DEVICE);
        let int_ptr = tcx.ptr(int, DEVICE);
        let created = |name: &CStr, fields: &[Ty]| tcx.create_struct(name, Some((fields, false)));
        let dxmt_vertex_buffer_entry = created(c"dxmt_vertex_buffer_entry", &[byte_ptr, int, int]);
        let dxmt_stream_output_buffer_entry =
            created(c"dxmt_stream_output_buffer_entry", &[int_ptr, int_ptr]);
        let dxmt_draw_arguments = created(c"dxmt_draw_arguments", &[int; 4]);
        let dxmt_draw_indexed_arguments = created(c"dxmt_draw_indexed_arguments", &[int; 5]);
        let opaque = tcx.named_struct(c"opaque");
        let _ = tcx.ptr(opaque, 1);
        let _ = tcx.ptr(opaque, 2);
        let _ = tcx.ptr(opaque, 3);
        let _ = tcx.named_struct(c"struct._texture_1d_t");
        let _ = tcx.named_struct(c"struct._texture_1d_array_t");
        let _ = tcx.named_struct(c"struct._texture_2d_t");
        let _ = tcx.named_struct(c"struct._texture_2d_array_t");
        let _ = tcx.named_struct(c"struct._texture_2d_ms_t");
        let _ = tcx.named_struct(c"struct._texture_2d_ms_array_t");
        let _ = tcx.named_struct(c"struct._texture_cube_t");
        let _ = tcx.named_struct(c"struct._texture_cube_array_t");
        let _ = tcx.named_struct(c"struct._texture_3d_t");
        let _ = tcx.named_struct(c"struct._texture_buffer_1d_t");
        let _ = tcx.named_struct(c"struct._depth_2d_t");
        let _ = tcx.named_struct(c"struct._depth_2d_array_t");
        let _ = tcx.named_struct(c"struct._depth_2d_ms_t");
        let _ = tcx.named_struct(c"struct._depth_2d_ms_array_t");
        let _ = tcx.named_struct(c"struct._depth_cube_t");
        let _ = tcx.named_struct(c"struct._depth_cube_array_t");
        let _ = tcx.named_struct(c"struct._sampler_t");
        let _ = tcx.named_struct(c"struct._mesh_grid_properties_t");
        let _ = tcx.named_struct(c"struct._interpolant_t");
        let _ = tcx.named_struct(c"struct._mesh_t");
        Self {
            bool,
            byte,
            char2,
            char4,
            short,
            short2,
            short4,
            float,
            float2,
            float3,
            float4,
            int,
            int2,
            int3,
            int4,
            half,
            half2,
            half4,
            dxmt_vertex_buffer_entry,
            dxmt_draw_arguments,
            dxmt_draw_indexed_arguments,
            dxmt_stream_output_buffer_entry,
        }
    }
}
