//! Shader model 3 vertex-to-pixel linkage by declaration semantic.
//!
//! An SM3 vertex shader names each output with a `dcl_<usage><index> oN.mask`
//! and an SM3 pixel shader each input with a `dcl_<usage><index> vN.mask`.
//! D3D9 links the two stages by semantic, not by register: the same semantic
//! may sit in different registers on the two sides, and several semantics may
//! share one register in disjoint lanes. A semantic keeps its lanes at their
//! component positions, so `TEXCOORD1` declared on `o1.zw` is lanes `zw` of
//! `TEXCOORD1` on both sides.
//!
//! Each semantic travels in a `Varyings` member named after it, so the two
//! stages, compiled separately, agree on the member without knowing each
//! other. The members every struct declares (the fixed-function set: the
//! position, sixteen texture coordinates, two colours, the secondary position
//! and fog) need nothing more. Every other semantic is an extra member, which
//! the vertex shader declares when it outputs it and the pixel shader only
//! when the bound vertex shader outputs it, since Metal rejects a pipeline
//! whose fragment function reads a member the vertex function does not write.
//! [`LinkInputs`] and [`SemanticSet`] are the two halves of that decision,
//! and `VariantKey::linked_input_mask` carries it to the pixel emitter.

use super::ir::{DeclUsage, Declaration, DxsoProgram, DxsoWarning, RegKind, ShaderType, WriteMask};

/// Most extra input semantics a pixel shader links to the vertex shader.
///
/// Bit `i` of `VariantKey::linked_input_mask` stands for the `i`-th entry of
/// [`LinkInputs`]. A pixel shader declaring more reads zero for the rest.
pub const MAX_LINKED_INPUTS: usize = 8;

/// One declaration semantic: a usage and its usage index.
// Copy: two bytes, stored in and read out of the fixed linkage arrays by value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Semantic {
    pub usage: DeclUsage,
    pub index: u8,
}

impl Semantic {
    /// The semantic of a `dcl` declaration; the usage index is a four-bit field.
    #[must_use]
    pub const fn new(usage: DeclUsage, index: u32) -> Self {
        Self {
            usage,
            index: (index & 0xF) as u8,
        }
    }

    /// A dense code below 224: the usage in the high nibble, the index in the low one.
    #[must_use]
    pub const fn code(self) -> u8 {
        ((self.usage as u8) << 4) | self.index
    }

    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        let usage = match code >> 4 {
            0 => DeclUsage::Position,
            1 => DeclUsage::BlendWeight,
            2 => DeclUsage::BlendIndices,
            3 => DeclUsage::Normal,
            4 => DeclUsage::PSize,
            5 => DeclUsage::Texcoord,
            6 => DeclUsage::Tangent,
            7 => DeclUsage::Binormal,
            8 => DeclUsage::TessFactor,
            9 => DeclUsage::PositionT,
            10 => DeclUsage::Color,
            11 => DeclUsage::Fog,
            12 => DeclUsage::Depth,
            _ => DeclUsage::Sample,
        };
        Self {
            usage,
            index: code & 0xF,
        }
    }

    /// The semantic of a vertex declaration element; `None` for a usage outside `D3DDECLUSAGE_*`.
    #[must_use]
    pub const fn from_decl(usage: u8, usage_index: u8) -> Option<Self> {
        if usage > DeclUsage::Sample as u8 {
            return None;
        }
        Some(Self::from_code((usage << 4) | (usage_index & 0xF)))
    }

    /// Whether a pre-transformed draw's vertex stage passes an element of this semantic through.
    ///
    /// With no vertex shader in the way, D3D9 feeds each `ps_3_0` input from
    /// the declaration element of the same semantic. The fixed-function
    /// vertex stage already writes the position, both colours, the texture
    /// coordinates and the point size from the declaration, so the elements
    /// left to pass through are the secondary position, fog and every extra
    /// semantic but the pre-transformed position itself.
    #[must_use]
    pub const fn passes_through(self) -> bool {
        match (self.usage, self.index) {
            (DeclUsage::PositionT, 0) => false,
            (DeclUsage::Position, 1) | (DeclUsage::Fog, 0) => true,
            _ => self.is_extra(),
        }
    }

    /// Whether the semantic travels in a member outside the set every `Varyings` declares.
    ///
    /// The clip position (`POSITION0`) and the point size (`PSIZE0`) are not
    /// members a pixel shader can read, and the rest of the fixed set is
    /// declared by every vertex and pixel struct.
    #[must_use]
    pub const fn is_extra(self) -> bool {
        !matches!(
            (self.usage, self.index),
            (DeclUsage::Texcoord, _)
                | (DeclUsage::Color | DeclUsage::Position, 0 | 1)
                | (DeclUsage::Fog | DeclUsage::PSize, 0)
        )
    }

    /// The `Varyings` member carrying the semantic, `<usage><index>` but for `position` and `fog`.
    #[must_use]
    pub fn member(self) -> String {
        let name = match self.usage {
            DeclUsage::Position if self.index == 0 => return "position".to_owned(),
            DeclUsage::Fog if self.index == 0 => return "fog".to_owned(),
            DeclUsage::Position => "position",
            DeclUsage::BlendWeight => "blendweight",
            DeclUsage::BlendIndices => "blendindices",
            DeclUsage::Normal => "normal",
            DeclUsage::PSize => "psize",
            DeclUsage::Texcoord => "texcoord",
            DeclUsage::Tangent => "tangent",
            DeclUsage::Binormal => "binormal",
            DeclUsage::TessFactor => "tessfactor",
            DeclUsage::PositionT => "positiont",
            DeclUsage::Color => "color",
            DeclUsage::Fog => "fog",
            DeclUsage::Depth => "depth",
            DeclUsage::Sample => "sample",
        };
        format!("{name}{}", self.index)
    }

    /// The value a vertex output lane of this semantic holds until the shader writes it.
    ///
    /// D3D9's defaults for an unwritten output: opaque white diffuse, no fog,
    /// the render-state point size, zero for everything else.
    #[must_use]
    pub const fn vertex_default(self) -> &'static str {
        match (self.usage, self.index) {
            (DeclUsage::Color | DeclUsage::Fog, 0) => "1.0",
            (DeclUsage::PSize, 0) => "vs_draw.point.x",
            _ => "0.0",
        }
    }
}

/// A set of semantics, one bit per `Semantic` code.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SemanticSet([u64; 4]);

impl SemanticSet {
    /// The extra semantics an SM3 vertex shader outputs, each of which it emits as a member.
    ///
    /// Empty for every other vertex shader: SM1 and SM2 outputs are the fixed
    /// registers of the base set.
    #[must_use]
    pub fn vs_outputs(vs: &DxsoProgram) -> Self {
        let mut set = Self::default();
        if vs.shader_type != ShaderType::Vertex || vs.major != 3 {
            return set;
        }
        for element in elements(vs).filter(|e| is_output_kind(e.reg.0)) {
            if element.semantic.is_extra() {
                set.insert(element.semantic);
            }
        }
        set
    }

    /// The extra semantics a pre-transformed draw's vertex stage outputs from its declaration.
    ///
    /// `passthrough` is the list [`decl_passthrough_code`] builds; the rest
    /// of it lands in members every `Varyings` declares.
    #[must_use]
    pub fn passthrough_outputs(passthrough: &[u8; MAX_LINKED_INPUTS]) -> Self {
        let mut set = Self::default();
        for (_, semantic) in passthrough_entries(passthrough) {
            if semantic.is_extra() {
                set.insert(semantic);
            }
        }
        set
    }

    /// Whether the set holds no semantic.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|w| *w == 0)
    }

    pub fn insert(&mut self, semantic: Semantic) {
        let code = semantic.code();
        if let Some(word) = self.0.get_mut(usize::from(code >> 6)) {
            *word |= 1 << (code & 63);
        }
    }

    #[must_use]
    pub fn contains(&self, semantic: Semantic) -> bool {
        self.has(semantic.code())
    }

    fn has(&self, code: u8) -> bool {
        self.0
            .get(usize::from(code >> 6))
            .is_some_and(|word| word & (1 << (code & 63)) != 0)
    }

    /// The semantics in code order.
    pub fn iter(&self) -> impl Iterator<Item = Semantic> + '_ {
        (0..=u8::MAX)
            .filter(|code| self.has(*code))
            .map(Semantic::from_code)
    }
}

/// The extra input semantics of an SM3 pixel shader, in first-declaration order.
///
/// The order fixes which bit of `VariantKey::linked_input_mask` stands for
/// which semantic, for the encoder that sets the bits and the emitter that
/// reads them.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct LinkInputs {
    codes: [u8; MAX_LINKED_INPUTS],
    len: u8,
}

impl LinkInputs {
    /// Collect the extra input semantics of `ps`; empty for anything but an SM3 pixel shader.
    ///
    /// A pixel shader declaring more than [`MAX_LINKED_INPUTS`] of them reads zero for the rest,
    /// and the second element says so with [`DxsoWarning::LinkedInputsOverflow`].
    #[must_use]
    pub fn ps_inputs(ps: &DxsoProgram) -> (Self, Option<DxsoWarning>) {
        let mut inputs = Self::default();
        if ps.shader_type != ShaderType::Pixel || ps.major != 3 {
            return (inputs, None);
        }
        for element in elements(ps).filter(|e| e.reg.0 == RegKind::Input) {
            let semantic = element.semantic;
            if !semantic.is_extra() || inputs.position(semantic).is_some() {
                continue;
            }
            let Some(slot) = inputs.codes.get_mut(usize::from(inputs.len)) else {
                return (inputs, Some(DxsoWarning::LinkedInputsOverflow));
            };
            *slot = semantic.code();
            inputs.len += 1;
        }
        (inputs, None)
    }

    /// Whether the pixel shader reads no extra semantic.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The `linked_input_mask` for this pixel shader against a vertex shader's outputs.
    ///
    /// Bit `i` is set when the vertex shader outputs the `i`-th input.
    #[must_use]
    pub fn mask_against(&self, outputs: &SemanticSet) -> u8 {
        self.iter()
            .enumerate()
            .filter(|(_, semantic)| outputs.contains(*semantic))
            .fold(0, |mask, (i, _)| mask | (1 << i))
    }

    /// The index of `semantic` among the inputs.
    #[must_use]
    pub fn position(&self, semantic: Semantic) -> Option<usize> {
        self.codes
            .iter()
            .take(usize::from(self.len))
            .position(|code| *code == semantic.code())
    }

    /// The semantics in first-declaration order.
    pub fn iter(&self) -> impl Iterator<Item = Semantic> + '_ {
        self.codes
            .iter()
            .take(usize::from(self.len))
            .map(|code| Semantic::from_code(*code))
    }
}

/// One `dcl` of an input or output register with its semantic and lanes.
// Copy: a few bytes, iterated out of the declaration list by value.
#[derive(Clone, Copy)]
pub struct Element {
    pub semantic: Semantic,
    pub reg: (RegKind, u16),
    pub mask: WriteMask,
}

/// The `dcl`s of input and output registers, in declaration order.
pub fn elements(program: &DxsoProgram) -> impl Iterator<Item = Element> + '_ {
    program.declarations.iter().filter_map(|decl| match decl {
        Declaration::Semantic {
            usage,
            usage_index,
            reg,
            mask,
        } => Some(Element {
            semantic: Semantic::new(*usage, *usage_index),
            reg: (reg.kind, reg.index),
            // A dcl without a mask covers the whole register.
            mask: if mask.0 == 0 { WriteMask::ALL } else { *mask },
        }),
        Declaration::Sampler { .. } => None,
    })
}

/// The register kinds an SM3 vertex shader's outputs arrive under.
///
/// The dcl carries the semantic whichever output kind the compiler picked:
/// some ship `TexcoordOut` (`D3DSPR_OUTPUT` shares its type 6), others
/// `Output`, `RastOut` or `AttrOut`, sometimes with overlapping indices, so
/// registers are keyed on the kind as well as the index.
#[must_use]
pub const fn is_output_kind(kind: RegKind) -> bool {
    matches!(
        kind,
        RegKind::RastOut | RegKind::AttrOut | RegKind::TexcoordOut | RegKind::Output
    )
}

/// The passthrough code of a declaration element; `None` for one a pre-transformed draw drops.
///
/// A pre-transformed draw's passthrough list holds these codes in the
/// order the declaration names them, without repeats, at most
/// [`MAX_LINKED_INPUTS`] of them, and zero in the slots past its end: the
/// code of `POSITION0`, which never passes through. Entry `k` is read from
/// the vertex attribute the fixed-function convention reserves for it.
#[must_use]
pub const fn decl_passthrough_code(usage: u8, usage_index: u8) -> Option<u8> {
    match Semantic::from_decl(usage, usage_index) {
        Some(semantic) if semantic.passes_through() => Some(semantic.code()),
        _ => None,
    }
}

/// The extra semantics of a passthrough list, the `Varyings` members its vertex stage declares.
#[must_use]
pub fn passthrough_extras(passthrough: [u8; MAX_LINKED_INPUTS]) -> Vec<Semantic> {
    passthrough_entries(&passthrough)
        .map(|(_, semantic)| semantic)
        .filter(|semantic| semantic.is_extra())
        .collect()
}

/// The occupied slots of a passthrough list with their semantics.
pub fn passthrough_entries(
    passthrough: &[u8; MAX_LINKED_INPUTS],
) -> impl Iterator<Item = (usize, Semantic)> + '_ {
    passthrough
        .iter()
        .take_while(|code| **code != 0)
        .map(|code| Semantic::from_code(*code))
        .enumerate()
}

#[cfg(test)]
mod tests;
