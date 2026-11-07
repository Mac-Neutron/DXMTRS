//! The MSL half of semantic linkage.
//!
//! The `Varyings` members, the vertex epilogue and the pixel prologue.
//!
//! The data half (`Semantic`, `SemanticSet`, `LinkInputs`, the passthrough codes) lives in
//! `airconv_ir::dxso::link`, which the AIR translator reads too; see its module docs for how
//! the two stages link by semantic.

use std::{collections::BTreeMap, fmt::Write};

pub use airconv_ir::dxso::link::{
    LinkInputs, MAX_LINKED_INPUTS, Semantic, SemanticSet, decl_passthrough_code, is_output_kind,
    passthrough_extras,
};
use airconv_ir::dxso::{
    ir::{DeclUsage, DstOperand, DxsoProgram, RegKind, ShaderType, WriteMask},
    link::{Element, elements, passthrough_entries},
};

/// The value a pixel shader reads for an input no vertex output supplies.
const ZERO: &str = "float4(0.0)";

/// The component names of lanes 0 to 3.
const LANE_NAMES: [char; 4] = ['x', 'y', 'z', 'w'];

/// The `.xyzw` suffix selecting `mask`'s lanes, empty for all four.
fn lanes(mask: WriteMask) -> String {
    if mask == WriteMask::ALL {
        return String::new();
    }
    let mut s = String::from(".");
    for (c, name) in (0..4).zip(LANE_NAMES) {
        if mask.covers(c) {
            s.push(name);
        }
    }
    s
}

/// The first lane `mask` covers, 0 for x through 3 for w.
fn first_lane_index(mask: WriteMask) -> u8 {
    (0..4).find(|c| mask.covers(*c)).unwrap_or(0)
}

/// The first lane `mask` covers, as `x`..`w`.
fn first_lane(mask: WriteMask) -> char {
    LANE_NAMES[usize::from(first_lane_index(mask))]
}

/// How an SM3 vertex shader's output registers reach the `Varyings` members.
///
/// A register carrying one semantic is written straight into its member. A
/// register several semantics share is written into a staging local, and the
/// epilogue copies each semantic's lanes into its member, so packed
/// semantics neither collide nor lose their lanes.
pub struct VsOutputs {
    /// Write target per output register: a member, the point-size store, or a staging local.
    targets: BTreeMap<(RegKind, u16), String>,
    /// Registers several semantics share, each with its staging local and elements.
    staged: Vec<(String, Vec<Element>)>,
    /// Members beyond the base set, in code order.
    extras: Vec<Semantic>,
    /// The register holding `FOG0` and its lane.
    fog: Option<((RegKind, u16), WriteMask)>,
}

impl VsOutputs {
    /// Plan the outputs of `vs`; `None` for anything but an SM3 vertex shader.
    #[must_use]
    pub fn build(vs: &DxsoProgram) -> Option<Self> {
        if vs.shader_type != ShaderType::Vertex || vs.major != 3 {
            return None;
        }
        let mut by_reg: BTreeMap<(RegKind, u16), Vec<Element>> = BTreeMap::new();
        let mut fog = None;
        for element in elements(vs).filter(|e| is_output_kind(e.reg.0)) {
            if (element.semantic.usage, element.semantic.index) == (DeclUsage::Fog, 0) {
                fog = Some((element.reg, WriteMask(1 << first_lane_index(element.mask))));
            }
            by_reg.entry(element.reg).or_default().push(element);
        }
        let mut targets = BTreeMap::new();
        let mut staged = Vec::new();
        for (reg, elements) in by_reg {
            let direct = match elements.as_slice() {
                [only] => direct_target(*only),
                _ => None,
            };
            let target = direct.unwrap_or_else(|| {
                let local = format!("_o{}", staged.len());
                staged.push((local.clone(), elements));
                local
            });
            targets.insert(reg, target);
        }
        Some(Self {
            targets,
            staged,
            extras: SemanticSet::vs_outputs(vs).iter().collect(),
            fog,
        })
    }

    /// Write target per output register, for the destination path.
    #[must_use]
    pub const fn targets(&self) -> &BTreeMap<(RegKind, u16), String> {
        &self.targets
    }

    /// The members this vertex shader declares beyond the base set.
    #[must_use]
    pub fn extras(&self) -> &[Semantic] {
        &self.extras
    }

    /// Whether `dst` writes the lane that holds `FOG0`.
    #[must_use]
    pub fn writes_fog(&self, dst: DstOperand) -> bool {
        self.fog.is_some_and(|(reg, lane)| {
            reg == (dst.reg.kind, dst.reg.index) && dst.write_mask.0 & lane.0 != 0
        })
    }

    /// Zero the extra members and declare each staging local with its lanes' defaults.
    pub fn write_prologue(&self, out: &mut String) {
        for semantic in &self.extras {
            let _ = writeln!(out, "    out.{} = float4(0.0);", semantic.member());
        }
        for (local, elements) in &self.staged {
            let lane = |c: u8| {
                elements
                    .iter()
                    .find(|e| e.mask.covers(c))
                    .map_or("0.0", |e| e.semantic.vertex_default())
            };
            let _ = writeln!(
                out,
                "    float4 {local} = float4({}, {}, {}, {});",
                lane(0),
                lane(1),
                lane(2),
                lane(3)
            );
        }
    }

    /// Copy each staged semantic's lanes into its member at their own positions.
    pub fn write_epilogue(&self, out: &mut String) {
        for (local, elements) in &self.staged {
            for element in elements {
                let Element { semantic, mask, .. } = *element;
                let sel = lanes(mask);
                match (semantic.usage, semantic.index) {
                    (DeclUsage::PSize, 0) => {
                        let lane = first_lane(mask);
                        let _ = writeln!(out, "    _psize_storage.x = {local}.{lane};");
                    }
                    (DeclUsage::Fog, 0) => {
                        let lane = first_lane(mask);
                        let _ = writeln!(out, "    out.fog = float4({local}.{lane});");
                    }
                    _ => {
                        let member = semantic.member();
                        let _ = writeln!(out, "    out.{member}{sel} = {local}{sel};");
                    }
                }
            }
        }
    }
}

/// The member a register carrying only `element` is written into, or `None` to stage it.
///
/// `FOG0` and `PSIZE0` are scalars read from lane x of their store, so one
/// declared on another lane goes through a staging local, whose epilogue
/// moves the lane.
fn direct_target(element: Element) -> Option<String> {
    let scalar_off_x = first_lane(element.mask) != 'x';
    Some(match (element.semantic.usage, element.semantic.index) {
        (DeclUsage::PSize | DeclUsage::Fog, 0) if scalar_off_x => return None,
        (DeclUsage::PSize, 0) => "_psize_storage".to_owned(),
        _ => format!("out.{}", element.semantic.member()),
    })
}

/// How an SM3 pixel shader's input registers read the `Varyings` members.
///
/// A register carrying one semantic reads its member whole, lanes outside
/// the dcl mask included, as a title may read a lane its declaration leaves
/// out. A register several semantics share reads a prologue local assembled
/// lane by lane from the semantic that covers each lane.
pub struct PsInputs {
    /// Read expression per input register index.
    reads: BTreeMap<u16, String>,
    /// Prologue lines declaring the assembled locals of shared registers.
    locals: Vec<String>,
    /// Extra members the stage-in struct declares: the inputs the vertex shader outputs.
    extras: Vec<Semantic>,
}

impl PsInputs {
    /// Plan the inputs of `ps` given which of its extra semantics the bound vertex shader outputs.
    #[must_use]
    pub fn build(ps: &DxsoProgram, linked_input_mask: u8) -> Self {
        let mut plan = Self {
            reads: BTreeMap::new(),
            locals: Vec::new(),
            extras: Vec::new(),
        };
        let inputs = elements(ps).filter(|e| e.reg.0 == RegKind::Input);
        if ps.major != 3 {
            // SM1/SM2 `dcl vN` is structural: input N is diffuse or specular colour N.
            for element in inputs {
                let index = element.reg.1;
                plan.reads.insert(index, format!("in.color{index}"));
            }
            return plan;
        }
        let link = super::link_inputs(ps);
        plan.extras = link
            .iter()
            .enumerate()
            .filter(|(i, _)| linked_input_mask & (1 << i) != 0)
            .map(|(_, semantic)| semantic)
            .collect();
        let read = |semantic: Semantic| -> Option<String> {
            match (semantic.usage, semantic.index) {
                // The clip position and the point size are no varyings a
                // pixel shader can read; `dcl_position0 vN` is rejected at
                // creation and reads the rasterizer position here.
                (DeclUsage::Position, 0) => Some("in.position".to_owned()),
                (DeclUsage::PSize, 0) => None,
                _ if !semantic.is_extra() => Some(format!("in.{}", semantic.member())),
                _ => {
                    let position = link.position(semantic);
                    let linked = position.is_some_and(|i| linked_input_mask & (1 << i) != 0);
                    if position.is_none() {
                        mtld3d_shared::log_once_info_by!(target: super::LOG_TARGET,
                            key: u64::from(semantic.code()),
                            "dxso: PS input {} is past the first {MAX_LINKED_INPUTS} extra \
                             input semantics → reads zero",
                            semantic.member()
                        );
                    } else if !linked {
                        mtld3d_shared::log_once_info_by!(target: super::LOG_TARGET,
                            key: u64::from(semantic.code()),
                            "dxso: PS input {} has no vertex output → reads zero",
                            semantic.member()
                        );
                    }
                    linked.then(|| format!("in.{}", semantic.member()))
                }
            }
        };
        let mut by_reg: BTreeMap<u16, Vec<Element>> = BTreeMap::new();
        for element in inputs {
            by_reg.entry(element.reg.1).or_default().push(element);
        }
        for (index, elements) in by_reg {
            if let [only] = elements.as_slice() {
                let expr = read(only.semantic).unwrap_or_else(|| ZERO.to_owned());
                plan.reads.insert(index, expr);
                continue;
            }
            let lane = |c: u8| {
                let element = elements
                    .iter()
                    .find(|e| e.mask.covers(c))
                    .unwrap_or(&elements[0]);
                let name = LANE_NAMES[usize::from(c)];
                read(element.semantic).map_or_else(|| "0.0".to_owned(), |e| format!("{e}.{name}"))
            };
            let local = format!("_v{index}");
            plan.locals.push(format!(
                "    float4 {local} = float4({}, {}, {}, {});",
                lane(0),
                lane(1),
                lane(2),
                lane(3)
            ));
            plan.reads.insert(index, local);
        }
        plan
    }

    /// Read expression per input register index.
    #[must_use]
    pub const fn reads(&self) -> &BTreeMap<u16, String> {
        &self.reads
    }

    /// The members the stage-in struct declares beyond the base set.
    #[must_use]
    pub fn extras(&self) -> &[Semantic] {
        &self.extras
    }

    /// Declare the locals of registers several semantics share.
    pub fn write_prologue(&self, out: &mut String) {
        for line in &self.locals {
            out.push_str(line);
            out.push('\n');
        }
    }
}

/// Declare one `float4` member per extra semantic, appended to a `Varyings` struct.
pub fn write_extra_members(out: &mut String, extras: &[Semantic]) {
    for semantic in extras {
        let _ = writeln!(out, "    float4 {};", semantic.member());
    }
}

/// Declare the vertex input of each passthrough entry, `p<k>` at attribute `attr_base + k`.
pub fn write_passthrough_inputs(
    out: &mut String,
    passthrough: [u8; MAX_LINKED_INPUTS],
    attr_base: u16,
) {
    for (k, _) in passthrough_entries(&passthrough) {
        let _ = writeln!(
            out,
            "    float4 p{k} [[attribute({})]];",
            usize::from(attr_base) + k
        );
    }
}

/// Write each passthrough entry's input into the member named after its semantic.
///
/// `FOG0` lands in `fog`, which the fixed-function pixel stage also reads as
/// the vertex fog factor, so it is written only when `write_fog` says the
/// vertex stage supplies no factor of its own.
pub fn write_passthrough_outputs(
    out: &mut String,
    passthrough: [u8; MAX_LINKED_INPUTS],
    write_fog: bool,
) {
    for (k, semantic) in passthrough_entries(&passthrough) {
        if (semantic.usage, semantic.index) == (DeclUsage::Fog, 0) && !write_fog {
            continue;
        }
        let _ = writeln!(out, "    out.{} = in.p{k};", semantic.member());
    }
}

#[cfg(test)]
mod tests;
