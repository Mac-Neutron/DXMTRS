//! The unscaled depth plane a pass draws into when only its depth surface is scaled.
//!
//! Under `render.scale` or `render.panelFit` a render target 0 that is not
//! back-buffer sized rasterizes unscaled, while a depth surface or sampleable
//! depth texture created at the back-buffer size scales with the back buffer.
//! Metal rasterizes a pass on one grid, so a pass pairing the two would test
//! and write depth texel (x, y) of the scaled surface for colour pixel (x, y),
//! a texel that stands for another reported pixel. Such a pass binds a depth
//! texture of the surface's reported size and format instead, the plane, which
//! shares the colour target's grid. A nearest-texel transfer carries the
//! surface into the plane at the first draw of the pairing that reads or writes
//! depth or stencil, or at a depth or stencil clear, and back when render
//! target 0 or the depth binding changes, before anything else reads or writes
//! the surface, and before the frame is submitted; the passes in between share
//! the one span.
//!
//! A draw that can neither fail a test nor change a value
//! (`depth_stencil_state::draw_reads_or_writes`), such as a depth test with an
//! always-pass compare and depth writes off, needs no plane and opens none.
//! Nearest up then nearest down is the identity at any scale up to one, so
//! depth the passes do not touch survives the round trip unchanged.
//!
//! When the surface is a level of a sampleable depth texture (INTZ, DF16,
//! DF24), a draw that samples that texture while the plane is bound reads the
//! plane's depth: the runtime carries the plane back into the level ahead of
//! such a draw whenever depth was written since it last did.

use mtld3d_shared::mtl::PixelFormat;

use crate::{ids::TextureId, render_scale::RenderScale};

/// How render target 0 and the depth surface share a raster grid.
#[derive(Debug, PartialEq, Eq)]
pub enum Pairing {
    /// Both are rasterized at one scale, or the pass binds no depth surface beside render target 0.
    Shared,
    /// Render target 0 unscaled over a scaled, single-sampled depth surface: the plane serves it.
    Mixed,
    /// The mixed shape over a multisampled depth surface, which the transfer cannot write.
    MixedMultisampled,
    /// Render target 0 is scaled over a depth surface at another scale: keeps the target's grid.
    Unserved,
}

/// The attachments a pass would open with, as far as their grids go.
pub struct PassTargets {
    /// What render target 0 is rasterized at.
    pub rt0_scale: RenderScale,
    /// Whether render target 0 is a `NULL` surface, shaped after the depth surface.
    pub rt0_is_null: bool,
    /// Render target 0's sample count.
    pub rt0_samples: u8,
    /// What the depth surface is rasterized at.
    pub depth_scale: RenderScale,
    /// The depth surface's sample count.
    pub depth_samples: u8,
}

/// Classify the pass a draw or clear opens from what its attachments are rasterized at.
///
/// A `NULL` render target 0 is shaped after the depth surface, scale included,
/// so it never pairs with one of another grid. A pass whose render target 0
/// and depth surface disagree on samples leaves the depth surface out, so it
/// pairs with nothing either. Two targets scaled alike share one grid; two
/// scaled differently, or a scaled render target 0 over an unscaled depth
/// surface, are left unserved on the colour target's grid.
#[must_use]
pub const fn pairing(targets: &PassTargets) -> Pairing {
    if targets.rt0_is_null
        || targets.rt0_samples != targets.depth_samples
        || targets.rt0_scale.equals(targets.depth_scale)
    {
        Pairing::Shared
    } else if !targets.rt0_scale.is_identity() {
        Pairing::Unserved
    } else if targets.depth_samples > 1 {
        Pairing::MixedMultisampled
    } else {
        Pairing::Mixed
    }
}

/// One end of a [`DepthTransfer`]: a stored depth texture level or the unscaled plane `P`.
#[derive(Debug, PartialEq, Eq)]
pub enum DepthEnd<P> {
    /// A stored depth surface or depth texture level at `(id, metal_level)`.
    Surface { id: TextureId, level: u32 },
    /// The unscaled depth plane (level 0).
    Plane(P),
}

/// Nearest-texel copy between a scaled depth surface/level and its unscaled plane.
#[derive(Debug, PartialEq, Eq)]
pub struct DepthTransfer<P> {
    pub source: DepthEnd<P>,
    pub source_size: (u32, u32),
    pub destination: DepthEnd<P>,
    pub destination_size: (u32, u32),
    pub format: PixelFormat,
}

/// The scaled depth surface a span carries into the plane and back.
pub struct DepthSurface {
    /// The stored texture id in `CoreDevice`.
    pub id: TextureId,
    /// The Metal mip level addressed (0 for a standalone/auto surface).
    pub level: u32,
    /// The texture level's extent, the reported one under `scale`.
    pub texture_size: (u32, u32),
    /// The extent D3D9 reports for the surface, which the plane takes.
    pub logical_size: (u32, u32),
    /// The texture's format, which the plane takes.
    pub format: PixelFormat,
    /// What the surface is rasterized at.
    pub scale: RenderScale,
    /// Whether shaders can sample this depth texture while the plane stands in for it.
    pub is_sampleable: bool,
}

impl DepthSurface {
    /// Whether the surface has a stencil plane.
    #[must_use]
    pub fn has_stencil(&self) -> bool {
        self.format == PixelFormat::Depth32FloatStencil8
    }
}

/// The plane a device owns and the span that has it standing in for a depth surface.
///
/// The caller allocates the plane `P`, binds it while the span is open and
/// retires it; this keeps which surface the plane holds and builds the
/// transfers in, back, and out. It also keeps what the device classified the
/// bound targets as, so a draw whose render states changed asks no surface
/// again: the device forgets the answer wherever render target 0, the depth
/// binding or an implicit surface changes.
pub struct DepthPlane<P> {
    plane: Option<Plane<P>>,
    /// The surface whose depth the plane holds while a span is open.
    span: Option<DepthSurface>,
    /// The reported size and format of a plane the device was refused, not asked for again.
    refused: Option<PlaneKey>,
    targets: Targets,
}

impl<P> DepthPlane<P> {
    /// No plane allocated, no span open, and the targets not classified.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            plane: None,
            span: None,
            refused: None,
            targets: Targets::Unknown,
        }
    }

    /// Whether the bound targets are classified since they last changed.
    #[must_use]
    pub const fn targets_known(&self) -> bool {
        !matches!(self.targets, Targets::Unknown)
    }

    /// Keep the classification of the bound targets: the mixed depth surface, or none.
    ///
    /// Only whether the surface has a stencil plane is kept of it; the device
    /// reads the surface again when it opens a span.
    pub fn note_targets(&mut self, mixed: Option<&DepthSurface>) {
        self.targets = mixed.map_or(Targets::Unserved, |surface| Targets::Mixed {
            has_stencil: surface.has_stencil(),
        });
    }

    /// Forget the classification: a target, the depth binding or an implicit surface changed.
    pub const fn forget_targets(&mut self) {
        self.targets = Targets::Unknown;
    }

    /// For targets classified as mixed, whether the depth surface has a stencil plane.
    #[must_use]
    pub const fn mixed_has_stencil(&self) -> Option<bool> {
        match self.targets {
            Targets::Mixed { has_stencil } => Some(has_stencil),
            Targets::Unknown | Targets::Unserved => None,
        }
    }

    /// Whether no draw or clear can open a span now, from what is kept alone.
    ///
    /// True while a span is open and while the targets are classified as
    /// pairing on one grid: the one test a draw over such targets pays.
    #[must_use]
    pub const fn rules_out_span(&self) -> bool {
        self.span.is_some() || matches!(self.targets, Targets::Unserved)
    }

    /// Whether a plane for `surface` was refused, so allocating one again is not worth a try.
    #[must_use]
    pub fn refused_for(&self, surface: &DepthSurface) -> bool {
        self.refused == Some(PlaneKey::of(surface))
    }

    /// Remember that a plane for `surface` was refused, until a plane is installed or taken.
    pub const fn note_refused(&mut self, surface: &DepthSurface) {
        self.refused = Some(PlaneKey::of(surface));
    }

    /// Whether a span is open, so the plane stands in for a depth surface.
    #[must_use]
    pub const fn span_open(&self) -> bool {
        self.span.is_some()
    }

    /// The bound surface of the open span, if one is open.
    #[must_use]
    pub const fn span_surface(&self) -> Option<&DepthSurface> {
        self.span.as_ref()
    }

    /// Forget an open span without carrying the plane back, for a frame that is dropped whole.
    ///
    /// The frame's bindings are reasserted after it, so the targets'
    /// classification goes with the span.
    pub const fn abandon_span(&mut self) {
        self.span = None;
        self.targets = Targets::Unknown;
    }

    /// Take the plane for retirement, and forget a remembered refusal and the targets.
    ///
    /// `Reset` and the device's release call this; both replace what the
    /// targets were classified from.
    ///
    /// # Panics
    ///
    /// Panics while a span is open: close it first.
    pub fn take_plane(&mut self) -> Option<P> {
        assert!(self.span.is_none(), "the plane of an open span is taken");
        self.refused = None;
        self.targets = Targets::Unknown;
        self.plane.take().map(|plane| plane.handle)
    }
}

impl<P: Clone> DepthPlane<P> {
    /// The existing plane if its reported size and format serve `surface`.
    #[must_use]
    pub fn plane_for(&self, surface: &DepthSurface) -> Option<P> {
        let plane = self.plane.as_ref()?;
        (plane.key == PlaneKey::of(surface)).then(|| plane.handle.clone())
    }

    /// Install a freshly allocated plane for `surface`, returning any previous plane to retire.
    ///
    /// Clears any remembered refusal, even for another size or format.
    ///
    /// # Panics
    ///
    /// Panics while a span is open: its plane cannot change underneath it.
    pub fn replace_plane(&mut self, handle: P, surface: &DepthSurface) -> Option<P> {
        assert!(self.span.is_none(), "an open span's plane is replaced");
        self.refused = None;
        self.plane
            .replace(Plane {
                handle,
                key: PlaneKey::of(surface),
            })
            .map(|old| old.handle)
    }

    /// Open a span over `surface`: the transfer that copies the surface into the plane.
    ///
    /// `None` when no plane of the surface's size and format is allocated or a
    /// span is already open; nothing changes then.
    pub fn open(&mut self, surface: DepthSurface) -> Option<DepthTransfer<P>> {
        if self.span.is_some() {
            return None;
        }
        let plane = self.plane_for(&surface)?;
        let transfer = DepthTransfer {
            source: DepthEnd::Surface {
                id: surface.id,
                level: surface.level,
            },
            source_size: surface.texture_size,
            destination: DepthEnd::Plane(plane),
            destination_size: surface.logical_size,
            format: surface.format,
        };
        self.span = Some(surface);
        Some(transfer)
    }

    /// Copies the open span's plane back into its depth texture level without closing.
    ///
    /// `None` when no span is open or the open span does not serve a sampleable depth texture.
    ///
    /// # Panics
    ///
    /// Panics if the open span has no plane, which [`Self::open`] and
    /// [`Self::replace_plane`] rule out.
    #[must_use]
    pub fn write_back_stand_in(&self) -> Option<DepthTransfer<P>> {
        let surface = self.span.as_ref().filter(|s| s.is_sampleable)?;
        let plane = self
            .plane
            .as_ref()
            .expect("an open span has its plane")
            .handle
            .clone();
        Some(DepthTransfer {
            source: DepthEnd::Plane(plane),
            source_size: surface.logical_size,
            destination: DepthEnd::Surface {
                id: surface.id,
                level: surface.level,
            },
            destination_size: surface.texture_size,
            format: surface.format,
        })
    }

    /// Close the open span: the transfer that carries the plane back, and the surface it served.
    ///
    /// # Panics
    ///
    /// Panics if the open span has no plane, which [`Self::open`] and
    /// [`Self::replace_plane`] rule out.
    pub fn close(&mut self) -> Option<(DepthTransfer<P>, DepthSurface)> {
        let surface = self.span.take()?;
        let plane = self
            .plane
            .as_ref()
            .expect("an open span has its plane")
            .handle
            .clone();
        let transfer = DepthTransfer {
            source: DepthEnd::Plane(plane),
            source_size: surface.logical_size,
            destination: DepthEnd::Surface {
                id: surface.id,
                level: surface.level,
            },
            destination_size: surface.texture_size,
            format: surface.format,
        };
        Some((transfer, surface))
    }
}

impl<P> Default for DepthPlane<P> {
    fn default() -> Self {
        Self::new()
    }
}

/// What the device classified the bound render target 0 and depth surface as.
enum Targets {
    /// Not classified since a target, the depth binding or an implicit surface changed.
    Unknown,
    /// No plane serves the pair: both on one grid, or a pairing left on the surface's grid.
    Unserved,
    /// An unscaled render target 0 over a scaled depth surface the plane can serve.
    Mixed {
        /// Whether that surface has a stencil plane.
        has_stencil: bool,
    },
}

/// The device's plane: a Private depth texture of one key.
struct Plane<P> {
    handle: P,
    key: PlaneKey,
}

/// What a plane serves: a depth surface's reported size and format.
#[derive(PartialEq, Eq)]
struct PlaneKey {
    logical_size: (u32, u32),
    format: PixelFormat,
}

impl PlaneKey {
    const fn of(surface: &DepthSurface) -> Self {
        Self {
            logical_size: surface.logical_size,
            format: surface.format,
        }
    }
}

#[cfg(test)]
mod tests;
