//! The hull shader's estimates: tessellation factors, workload counts and payload sizes.
//!
//! Ported from `A/dxbc_converter_ts.cpp:36-175` and `next_pow2` (`A/dxbc_converter.cpp:90-92`):
//! Initialize sizes a tessellation pipeline's threadgroups and meshes with them, and the hull
//! and domain compiles size their buffers. The C++'s unsigned arithmetic wraps here too, and its
//! float conversions are `float_cast`'s.

use crate::{
    float_cast::{f32_to_u32, f64_to_u32, u32_to_f32},
    tokens::tessellator,
};

/// The hull shader's values a payload estimate reads (`SM50ShaderInternal`'s).
#[derive(Debug)]
pub struct HullCounts {
    /// `D3D11_SB_TESSELLATOR_PARTITIONING`.
    pub partition: u32,
    /// `D3D11_SB_TESSELLATOR_DOMAIN`.
    pub domain: u32,
    pub max_output_register: u32,
    pub output_control_point_count: u32,
    /// How many patch constant scalars the hull shader writes.
    pub patch_constant_scalars: usize,
}

/// The tessellator's partitioning (`TessellatorPartitioning`).
#[derive(Debug, PartialEq, Eq)]
pub enum Partitioning {
    Integer,
    Pow2,
    FractionalOdd,
    FractionalEven,
}

/// The tessellator's output primitive (`TessellatorOutputPrimitive`).
#[derive(Debug, PartialEq, Eq)]
pub enum OutputPrimitive {
    Point,
    Line,
    Triangle,
    TriangleCcw,
}

/// `sizeof(TessMeshWorkload)`: one tessellation workload's record in the payload.
const WORKLOAD_SIZE: u32 = 56;

/// `next_pow2`: the least power of two not below `x`.
///
/// For 0 and above 2^31 the C++ shifted by 32, which ARM64 takes modulo 32: 1.
#[must_use]
pub const fn next_pow2(x: u32) -> u32 {
    if x == 1 {
        1
    } else {
        1u32.wrapping_shl(32 - x.wrapping_sub(1).leading_zeros())
    }
}

/// `get_partitioning`: an unknown partitioning is integer.
#[must_use]
pub const fn partitioning(partition: u32) -> Partitioning {
    match partition {
        tessellator::PARTITIONING_POW2 => Partitioning::Pow2,
        tessellator::PARTITIONING_FRACTIONAL_ODD => Partitioning::FractionalOdd,
        tessellator::PARTITIONING_FRACTIONAL_EVEN => Partitioning::FractionalEven,
        _ => Partitioning::Integer,
    }
}

/// `get_output_primitive`: an unknown primitive is a point.
#[must_use]
pub const fn output_primitive(primitive: u32) -> OutputPrimitive {
    match primitive {
        tessellator::OUTPUT_LINE => OutputPrimitive::Line,
        tessellator::OUTPUT_TRIANGLE_CW => OutputPrimitive::Triangle,
        tessellator::OUTPUT_TRIANGLE_CCW => OutputPrimitive::TriangleCcw,
        _ => OutputPrimitive::Point,
    }
}

/// `get_integer_factor`: the integer factor a `factor` tessellates at under `partition`.
///
/// An undefined or unknown partitioning answers 0. A NaN factor passes `std::clamp` unchanged
/// and converts to 0, as the C++'s ran.
#[must_use]
pub fn integer_factor(factor: f32, partition: u32) -> u32 {
    match partition {
        tessellator::PARTITIONING_INTEGER => f32_to_u32(clamp(factor, 1.0, 64.0).ceil()),
        tessellator::PARTITIONING_POW2 => next_pow2(f32_to_u32(clamp(factor, 1.0, 64.0))),
        tessellator::PARTITIONING_FRACTIONAL_ODD => {
            let f = f32_to_u32(clamp(factor, 1.0, 63.0).ceil());
            if f & 1 == 0 { f.wrapping_add(1) } else { f }
        }
        tessellator::PARTITIONING_FRACTIONAL_EVEN => {
            let f = f32_to_u32(clamp(factor, 2.0, 64.0).ceil());
            if f & 1 == 0 { f } else { f.wrapping_add(1) }
        }
        _ => 0,
    }
}

/// `get_max_potential_workload_count`: the workloads of a patch at `max_tess_factor`.
///
/// An isoline or undefined domain makes none; so does an unknown one, where the C++ fell off
/// its end.
#[must_use]
pub fn max_potential_workload_count(max_tess_factor: u32, domain: u32) -> u32 {
    let per_side = match domain {
        tessellator::DOMAIN_TRI => 3.0,
        tessellator::DOMAIN_QUAD => 4.0,
        _ => return 0,
    };
    let quarters = (f64::from(max_tess_factor.wrapping_sub(1)) / 4.0).ceil();
    // The values are integers below 2^53, so fused or not the sum is exact.
    f64_to_u32(quarters.mul_add(per_side, f64::from(max_tess_factor & 1)))
}

/// `estimate_payload_size`: the bytes a threadgroup of `patch_per_group` patches needs.
///
/// At the maximum factor `factor`.
#[must_use]
pub fn payload_size(hull: &HullCounts, factor: f32, patch_per_group: u32) -> u64 {
    let per_point = 16u64.wrapping_mul(u64::from(hull.max_output_register));
    let per_patch = per_point.wrapping_mul(u64::from(hull.output_control_point_count));
    let control_points = per_patch.wrapping_mul(u64::from(patch_per_group));
    let scalars = u64::try_from(hull.patch_constant_scalars).unwrap_or(u64::MAX);
    let constants = 4u64
        .wrapping_mul(scalars)
        .wrapping_mul(u64::from(patch_per_group));
    let workloads =
        max_potential_workload_count(integer_factor(factor, hull.partition), hull.domain);
    // The workloads' bytes are a `uint32_t` product in the C++.
    let workload_bytes = workloads
        .wrapping_mul(patch_per_group)
        .wrapping_mul(WORKLOAD_SIZE);
    control_points
        .wrapping_add(constants)
        .wrapping_add(4)
        .wrapping_add(u64::from(workload_bytes))
}

/// `estimate_mesh_size`: the bytes a domain shader's mesh needs at integer factor `factor`.
#[must_use]
pub fn mesh_size(max_output_register: u32, factor: u32) -> u64 {
    // Both are `uint32_t` sums and products in the C++, widened after.
    let per_vertex = u64::from(max_output_register.wrapping_mul(16));
    let edge_points = u64::from(factor.wrapping_add(1));
    edge_points
        .wrapping_add(2)
        .wrapping_sub(edge_points & 1)
        .wrapping_mul(2)
        .wrapping_add(1)
        .wrapping_mul(per_vertex)
}

/// `get_final_maxtessfactor`: the hull's maximum factor and its integer factor, fitted.
///
/// They are lowered until the integer factor fits the pipeline's `pso_max` (its tessellator
/// data's maximum potential factor).
/// The domain's mesh is sized for the answer, which no factor the domain shader receives exceeds.
#[must_use]
pub fn final_max_tess_factor(max_factor: f32, partition: u32, pso_max: Option<u32>) -> (f32, u32) {
    let integer = integer_factor(max_factor, partition);
    let Some(pso_max) = pso_max.filter(|&pso_max| integer > pso_max) else {
        return (max_factor, integer);
    };
    let mut factor = u32_to_f32(pso_max);
    loop {
        let integer = integer_factor(factor, partition);
        if integer <= pso_max {
            return (factor, integer);
        }
        factor -= 1.0;
        if factor <= 1.0 {
            break;
        }
    }
    let minimal = integer_factor(1.0, partition);
    (u32_to_f32(minimal), minimal)
}

/// `std::clamp<float>`: `lo` below, `hi` above, otherwise (NaN included) `v`.
fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    if v < lo {
        lo
    } else if hi < v {
        hi
    } else {
        v
    }
}

#[cfg(test)]
mod tests;
