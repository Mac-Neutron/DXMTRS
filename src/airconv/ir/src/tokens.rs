// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License (MIT).

//! The tokenized program format's constants and fields that the decoder reads.
//!
//! Ported from `libs/DXBCParser/d3d12tokenizedprogramformat.hpp` (Microsoft, MIT licence), the
//! subset the Rust readers use, and grown by the lane as the decoder needs more.

/// `D3D10_NAME`: the system value a signature blob stores for a parameter.
pub mod name {
    pub const UNDEFINED: u32 = 0;
    pub const FINAL_QUAD_EDGE_TESSFACTOR: u32 = 11;
    pub const FINAL_QUAD_INSIDE_TESSFACTOR: u32 = 12;
    pub const FINAL_TRI_EDGE_TESSFACTOR: u32 = 13;
    pub const FINAL_TRI_INSIDE_TESSFACTOR: u32 = 14;
    pub const FINAL_LINE_DETAIL_TESSFACTOR: u32 = 15;
    pub const FINAL_LINE_DENSITY_TESSFACTOR: u32 = 16;
    pub const TARGET: u32 = 64;
    pub const DEPTH: u32 = 65;
    pub const COVERAGE: u32 = 66;
}

/// `D3D10_SB_NAME`: the system value the shader's code names.
///
/// Values 0 to 10 are the same as [`name`]'s; the tessellation factors, which the blob stores
/// once per kind, are told apart by edge here.
pub mod sb_name {
    pub const UNDEFINED: u32 = 0;
    pub const FINAL_QUAD_U_EQ_0_EDGE_TESSFACTOR: u32 = 11;
    pub const FINAL_QUAD_V_EQ_0_EDGE_TESSFACTOR: u32 = 12;
    pub const FINAL_QUAD_U_EQ_1_EDGE_TESSFACTOR: u32 = 13;
    pub const FINAL_QUAD_V_EQ_1_EDGE_TESSFACTOR: u32 = 14;
    pub const FINAL_QUAD_U_INSIDE_TESSFACTOR: u32 = 15;
    pub const FINAL_QUAD_V_INSIDE_TESSFACTOR: u32 = 16;
    pub const FINAL_TRI_U_EQ_0_EDGE_TESSFACTOR: u32 = 17;
    pub const FINAL_TRI_V_EQ_0_EDGE_TESSFACTOR: u32 = 18;
    pub const FINAL_TRI_W_EQ_0_EDGE_TESSFACTOR: u32 = 19;
    pub const FINAL_TRI_INSIDE_TESSFACTOR: u32 = 20;
    pub const FINAL_LINE_DETAIL_TESSFACTOR: u32 = 21;
    pub const FINAL_LINE_DENSITY_TESSFACTOR: u32 = 22;
}
