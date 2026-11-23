//! Pure logic backing `IDirect3DQuery9::OCCLUSION` queries.

/// Convert a sample count produced on a `render_area` grid into `logical_area` pixels.
///
/// A target rasterized below the resolution D3D9 reports holds fewer pixels
/// than the game was told, so the counter is multiplied by the ratio of the
/// reported area to the render area, rounded to nearest. Equal areas (the
/// identity, and every target the scale does not reach) and a zero area (a
/// query begun before any target was bound) pass the count through; the
/// result saturates at `u64::MAX`.
#[must_use]
pub fn logical_samples(render_samples: u64, render_area: u64, logical_area: u64) -> u64 {
    if render_area == 0 || logical_area == 0 || render_area == logical_area {
        return render_samples;
    }
    let scaled = (u128::from(render_samples) * u128::from(logical_area)
        + u128::from(render_area) / 2)
        / u128::from(render_area);
    u64::try_from(scaled).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
