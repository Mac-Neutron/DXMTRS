//! Shader identities shared by the live draw path and the on-disk shader cache.
//!
//! The record kind and the content keys a compiled shader is known by. The draw
//! path and the perf records use them in every build; the cache that stores
//! records under them sits behind `mtld3d-core`'s `disk-cache` feature.

use std::hash::{Hash, Hasher};

use xxhash_rust::xxh3::Xxh3;

use crate::{
    dxso::{VariantKey, VsSamplerKinds},
    ids::ProgramId,
    shader_compile_stats::CompileBucket,
};

/// On-disk record kind.
///
/// Discriminants are wire bytes: never reorder without bumping
/// `SHADER_CACHE_SCHEMA_VERSION`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CachedKind {
    FfVs = 0,
    FfPs = 1,
    Sm1Vs = 2,
    Sm1Ps = 3,
    Sm2Vs = 4,
    Sm2Ps = 5,
    Sm3Vs = 6,
    Sm3Ps = 7,
    AirFfVs = 8,
    AirFfPs = 9,
    AirSm1Vs = 10,
    AirSm1Ps = 11,
    AirSm2Vs = 12,
    AirSm2Ps = 13,
    AirSm3Vs = 14,
    AirSm3Ps = 15,
}

impl CachedKind {
    /// Round-trip helper for the parser.
    ///
    /// `None` if the byte is outside the discriminant range, and the parser
    /// drops the record.
    #[must_use]
    pub const fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(Self::FfVs),
            1 => Some(Self::FfPs),
            2 => Some(Self::Sm1Vs),
            3 => Some(Self::Sm1Ps),
            4 => Some(Self::Sm2Vs),
            5 => Some(Self::Sm2Ps),
            6 => Some(Self::Sm3Vs),
            7 => Some(Self::Sm3Ps),
            8 => Some(Self::AirFfVs),
            9 => Some(Self::AirFfPs),
            10 => Some(Self::AirSm1Vs),
            11 => Some(Self::AirSm1Ps),
            12 => Some(Self::AirSm2Vs),
            13 => Some(Self::AirSm2Ps),
            14 => Some(Self::AirSm3Vs),
            15 => Some(Self::AirSm3Ps),
            _ => None,
        }
    }

    /// Map to the live-compile bucket.
    ///
    /// Pre-warm uses the same `(FF, SM1, SM2, SM3)` breakdown as the
    /// existing burst log.
    #[must_use]
    pub const fn compile_bucket(self) -> CompileBucket {
        match self {
            Self::FfVs | Self::FfPs | Self::AirFfVs | Self::AirFfPs => CompileBucket::Ff,
            Self::Sm1Vs | Self::Sm1Ps | Self::AirSm1Vs | Self::AirSm1Ps => CompileBucket::Sm1,
            Self::Sm2Vs | Self::Sm2Ps | Self::AirSm2Vs | Self::AirSm2Ps => CompileBucket::Sm2,
            Self::Sm3Vs | Self::Sm3Ps | Self::AirSm3Vs | Self::AirSm3Ps => CompileBucket::Sm3,
        }
    }

    /// The AIR twin of an MSL kind; `None` for AIR kinds.
    #[must_use]
    pub const fn air(self) -> Option<Self> {
        match self {
            Self::FfVs => Some(Self::AirFfVs),
            Self::FfPs => Some(Self::AirFfPs),
            Self::Sm1Vs => Some(Self::AirSm1Vs),
            Self::Sm1Ps => Some(Self::AirSm1Ps),
            Self::Sm2Vs => Some(Self::AirSm2Vs),
            Self::Sm2Ps => Some(Self::AirSm2Ps),
            Self::Sm3Vs => Some(Self::AirSm3Vs),
            Self::Sm3Ps => Some(Self::AirSm3Ps),
            Self::AirFfVs
            | Self::AirFfPs
            | Self::AirSm1Vs
            | Self::AirSm1Ps
            | Self::AirSm2Vs
            | Self::AirSm2Ps
            | Self::AirSm3Vs
            | Self::AirSm3Ps => None,
        }
    }

    #[must_use]
    pub const fn is_air(self) -> bool {
        match self {
            Self::AirFfVs
            | Self::AirFfPs
            | Self::AirSm1Vs
            | Self::AirSm1Ps
            | Self::AirSm2Vs
            | Self::AirSm2Ps
            | Self::AirSm3Vs
            | Self::AirSm3Ps => true,
            Self::FfVs
            | Self::FfPs
            | Self::Sm1Vs
            | Self::Sm1Ps
            | Self::Sm2Vs
            | Self::Sm2Ps
            | Self::Sm3Vs
            | Self::Sm3Ps => false,
        }
    }

    /// The MSL kind of the same program and specialization (identity for non-AIR kinds).
    #[must_use]
    pub const fn msl_twin(self) -> Self {
        match self {
            Self::AirFfVs => Self::FfVs,
            Self::AirFfPs => Self::FfPs,
            Self::AirSm1Vs => Self::Sm1Vs,
            Self::AirSm1Ps => Self::Sm1Ps,
            Self::AirSm2Vs => Self::Sm2Vs,
            Self::AirSm2Ps => Self::Sm2Ps,
            Self::AirSm3Vs => Self::Sm3Vs,
            Self::AirSm3Ps => Self::Sm3Ps,
            Self::FfVs
            | Self::FfPs
            | Self::Sm1Vs
            | Self::Sm1Ps
            | Self::Sm2Vs
            | Self::Sm2Ps
            | Self::Sm3Vs
            | Self::Sm3Ps => self,
        }
    }

    #[must_use]
    pub const fn is_vertex(self) -> bool {
        match self {
            Self::FfVs
            | Self::Sm1Vs
            | Self::Sm2Vs
            | Self::Sm3Vs
            | Self::AirFfVs
            | Self::AirSm1Vs
            | Self::AirSm2Vs
            | Self::AirSm3Vs => true,
            Self::FfPs
            | Self::Sm1Ps
            | Self::Sm2Ps
            | Self::Sm3Ps
            | Self::AirFfPs
            | Self::AirSm1Ps
            | Self::AirSm2Ps
            | Self::AirSm3Ps => false,
        }
    }

    #[must_use]
    pub const fn is_pixel(self) -> bool {
        match self {
            Self::FfPs
            | Self::Sm1Ps
            | Self::Sm2Ps
            | Self::Sm3Ps
            | Self::AirFfPs
            | Self::AirSm1Ps
            | Self::AirSm2Ps
            | Self::AirSm3Ps => true,
            Self::FfVs
            | Self::Sm1Vs
            | Self::Sm2Vs
            | Self::Sm3Vs
            | Self::AirFfVs
            | Self::AirSm1Vs
            | Self::AirSm2Vs
            | Self::AirSm3Vs => false,
        }
    }

    #[must_use]
    pub const fn is_programmable(self) -> bool {
        match self {
            Self::FfVs | Self::FfPs | Self::AirFfVs | Self::AirFfPs => false,
            Self::Sm1Vs
            | Self::Sm1Ps
            | Self::Sm2Vs
            | Self::Sm2Ps
            | Self::Sm3Vs
            | Self::Sm3Ps
            | Self::AirSm1Vs
            | Self::AirSm1Ps
            | Self::AirSm2Vs
            | Self::AirSm2Ps
            | Self::AirSm3Vs
            | Self::AirSm3Ps => true,
        }
    }

    /// Programmable: derive the kind from `(sm_major, is_pixel_shader)`.
    ///
    /// `None` for SM majors d3d9 should never see (DX10+).
    #[must_use]
    pub const fn from_programmable(sm_major: u8, is_pixel: bool) -> Option<Self> {
        match (sm_major, is_pixel) {
            (1, false) => Some(Self::Sm1Vs),
            (1, true) => Some(Self::Sm1Ps),
            (2, false) => Some(Self::Sm2Vs),
            (2, true) => Some(Self::Sm2Ps),
            (3, false) => Some(Self::Sm3Vs),
            (3, true) => Some(Self::Sm3Ps),
            _ => None,
        }
    }

    /// Per-shader Metal entry-point name, e.g. `mtld3d_vs_ff_5f3a0001`, `mtld3d_ps_sm3_a2b1c4d8`.
    ///
    /// The same string is written into the MSL function definition by the
    /// emitter and looked up via `newFunctionWithName:` on the unix side,
    /// so each compiled `MTLFunction` reports a distinct name in Xcode's
    /// pipeline-state inspector. Live-path (`encoder.rs`) and cache-load
    /// (`shader_prewarm.rs`) must share this helper to stay consistent.
    #[must_use]
    pub fn entry_name(self, disk_key: u64) -> String {
        let stage = if self.is_vertex() { "vs" } else { "ps" };
        let kind_label = match self {
            Self::FfVs | Self::FfPs => "ff",
            Self::AirFfVs | Self::AirFfPs => "air_ff",
            Self::Sm1Vs | Self::Sm1Ps => "sm1",
            Self::Sm2Vs | Self::Sm2Ps => "sm2",
            Self::Sm3Vs | Self::Sm3Ps => "sm3",
            Self::AirSm1Vs | Self::AirSm1Ps => "air_sm1",
            Self::AirSm2Vs | Self::AirSm2Ps => "air_sm2",
            Self::AirSm3Vs | Self::AirSm3Ps => "air_sm3",
        };
        format!("mtld3d_{stage}_{kind_label}_{disk_key:08x}")
    }
}

/// Hash any `Hash`-implementing FF state key to a u64 disk identifier.
///
/// `FfVsKey` / `FfPsKey` already implement `Hash` via `derive`, so this
/// is a one-liner at every call site.
pub fn ff_key_hash<T: Hash>(key: &T) -> u64 {
    let mut h = Xxh3::new();
    key.hash(&mut h);
    h.finish()
}

/// Content identity of a programmable vertex shader and its emission inputs.
#[must_use]
pub fn vs_source_disk_key_programmable(
    vs_id: ProgramId,
    provided_input_mask: u16,
    clip_plane_count: u8,
    sampler_kinds: VsSamplerKinds,
) -> u64 {
    ff_key_hash(&(
        vs_id.raw(),
        provided_input_mask,
        clip_plane_count,
        sampler_kinds,
    ))
}

/// Content identity of a programmable pixel shader and its emission inputs.
#[must_use]
pub fn ps_source_disk_key_programmable(ps_id: ProgramId, variant: VariantKey) -> u64 {
    ff_key_hash(&(ps_id.raw(), variant))
}

#[cfg(test)]
mod tests;
