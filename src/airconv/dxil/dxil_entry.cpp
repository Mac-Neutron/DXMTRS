/*
 * Copyright 2026 MacNeutron contributors
 *
 * This library is free software; you can redistribute it and/or
 * modify it under the terms of the GNU Lesser General Public
 * License as published by the Free Software Foundation; either
 * version 2.1 of the License, or (at your option) any later version.
 *
 * This library is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * Lesser General Public License for more details.
 *
 * You should have received a copy of the GNU Lesser General Public
 * License along with this library; if not, write to the Free Software
 * Foundation, Inc., 51 Franklin St, Fifth Floor, Boston, MA 02110-1301, USA
 */
// DXILGetEntry: the test tools' view of a shader (dxil_public.h). SM50Initialize builds the arrays once into the handle
// (EntryView), so the getter allocates nothing and a handle shared by threads is only read.
#include "dxil.hpp"
#include "dxil_public.h"
#include "../dxbc_converter.hpp"
#include <algorithm>
#include <deque>
#include <string>

namespace dxmt::dxil {

using namespace microsoft;

// The header's values are DXC's and the DXBC program types' (the tool has no other header).
template <auto A, auto B> constexpr bool kSame = (uint32_t)A == (uint32_t)B;
static_assert(kSame<ShaderKind::Pixel, DXIL_SHADER_KIND_PIXEL>);
static_assert(kSame<ShaderKind::Vertex, DXIL_SHADER_KIND_VERTEX>);
static_assert(kSame<ShaderKind::Geometry, DXIL_SHADER_KIND_GEOMETRY>);
static_assert(kSame<ShaderKind::Hull, DXIL_SHADER_KIND_HULL>);
static_assert(kSame<ShaderKind::Domain, DXIL_SHADER_KIND_DOMAIN>);
static_assert(kSame<ShaderKind::Compute, DXIL_SHADER_KIND_COMPUTE>);
static_assert(kSame<D3D10_SB_PIXEL_SHADER, DXIL_SHADER_KIND_PIXEL>);
static_assert(kSame<D3D10_SB_VERTEX_SHADER, DXIL_SHADER_KIND_VERTEX>);
static_assert(kSame<D3D10_SB_GEOMETRY_SHADER, DXIL_SHADER_KIND_GEOMETRY>);
static_assert(kSame<D3D11_SB_HULL_SHADER, DXIL_SHADER_KIND_HULL>);
static_assert(kSame<D3D11_SB_DOMAIN_SHADER, DXIL_SHADER_KIND_DOMAIN>);
static_assert(kSame<D3D11_SB_COMPUTE_SHADER, DXIL_SHADER_KIND_COMPUTE>);
static_assert(kSame<SemanticKind::Arbitrary, DXIL_SEMANTIC_ARBITRARY>);
static_assert(kSame<SemanticKind::Position, DXIL_SEMANTIC_POSITION>);
static_assert(kSame<SemanticKind::Depth, DXIL_SEMANTIC_DEPTH>);
static_assert(kSame<SemanticKind::DepthLessEqual, DXIL_SEMANTIC_DEPTH_LESS_EQUAL>);
static_assert(kSame<SemanticKind::DepthGreaterEqual, DXIL_SEMANTIC_DEPTH_GREATER_EQUAL>);
static_assert(kSame<SemanticKind::StencilRef, DXIL_SEMANTIC_STENCIL_REF>);
static_assert(kSame<ResourceClass::SRV, DXIL_RESOURCE_CLASS_SRV>);
static_assert(kSame<ResourceClass::UAV, DXIL_RESOURCE_CLASS_UAV>);
static_assert(kSame<ResourceClass::CBuffer, DXIL_RESOURCE_CLASS_CBUFFER>);
static_assert(kSame<ResourceClass::Sampler, DXIL_RESOURCE_CLASS_SAMPLER>);

struct EntryView {
  uint32_t is_dxil = 0, stage = 0;
  std::vector<DXIL_ENTRY_ELEMENT> inputs, outputs;
  std::vector<DXIL_ENTRY_RESOURCE> resources;
  // A DXBC element's name and semantic index, which its pointers keep (the parser's memory ends with SM50Initialize); a
  // deque never moves what it holds. A DXIL element points into the shader's own EntryInfo.
  std::deque<std::pair<std::string, uint32_t>> dxbc_names;
};

namespace {

// A DXBC element's DXIL kind: its system value's, or for SV_Depth and its kin their name. Anything else, a user varying
// and SV_Target too, is Arbitrary.
SemanticKind SemanticOf(D3D10_SB_NAME system_value, std::string name) {
  switch (system_value) {
  case D3D10_SB_NAME_POSITION: return SemanticKind::Position;
  case D3D10_SB_NAME_CLIP_DISTANCE: return SemanticKind::ClipDistance;
  case D3D10_SB_NAME_CULL_DISTANCE: return SemanticKind::CullDistance;
  case D3D10_SB_NAME_RENDER_TARGET_ARRAY_INDEX: return SemanticKind::RenderTargetArrayIndex;
  case D3D10_SB_NAME_VIEWPORT_ARRAY_INDEX: return SemanticKind::ViewPortArrayIndex;
  case D3D10_SB_NAME_VERTEX_ID: return SemanticKind::VertexID;
  case D3D10_SB_NAME_PRIMITIVE_ID: return SemanticKind::PrimitiveID;
  case D3D10_SB_NAME_INSTANCE_ID: return SemanticKind::InstanceID;
  case D3D10_SB_NAME_IS_FRONT_FACE: return SemanticKind::IsFrontFace;
  case D3D10_SB_NAME_SAMPLE_INDEX: return SemanticKind::SampleIndex;
  default: break;
  }
  std::transform(name.begin(), name.end(), name.begin(), [](char c) { return (char)tolower((unsigned char)c); });
  if (name == "sv_depth")
    return SemanticKind::Depth;
  if (name == "sv_depthlessequal")
    return SemanticKind::DepthLessEqual;
  if (name == "sv_depthgreaterequal")
    return SemanticKind::DepthGreaterEqual;
  if (name == "sv_stencilref")
    return SemanticKind::StencilRef;
  return SemanticKind::Arbitrary;
}

void Elements(const CSignatureParser &from, EntryView &view, std::vector<DXIL_ENTRY_ELEMENT> &to) {
  const D3D11_SIGNATURE_PARAMETER *parameters = nullptr;
  from.GetParameters(&parameters);
  for (UINT i = 0; i < from.GetNumParameters(); i++) {
    auto &p = parameters[i];
    auto &[name, index] = view.dxbc_names.emplace_back(p.SemanticName, p.SemanticIndex);
    to.push_back({(uint32_t)SemanticOf(p.SystemValue, name), 1, (int32_t)p.Register, name.c_str(), 1, &index});
  }
}

void Elements(const std::vector<SignatureElement> &from, std::vector<DXIL_ENTRY_ELEMENT> &to) {
  for (auto &e : from)
    to.push_back({(uint32_t)e.kind, e.rows, e.start_row, e.name.c_str(), (uint32_t)e.semantic_indices.size(),
                  e.semantic_indices.data()});
}

} // namespace

void BuildEntryView(dxbc::SM50ShaderInternal *shader) {
  auto view = std::make_shared<EntryView>();
  auto &entry = shader->dxil->entry;
  view->is_dxil = 1;
  view->stage = (uint32_t)entry.kind;
  Elements(entry.inputs, view->inputs);
  Elements(entry.outputs, view->outputs);
  for (auto &r : entry.resources)
    view->resources.push_back({(uint32_t)r.cls, r.space, r.lower_bound, r.size, r.heap});
  shader->entry_view = std::move(view);
}

void BuildEntryView(dxbc::SM50ShaderInternal *shader, const CSignatureParser &input, const CSignatureParser5 &output) {
  auto view = std::make_shared<EntryView>();
  view->stage = (uint32_t)shader->shader_type;
  Elements(input, *view, view->inputs);
  Elements(*output.RastSignature(), *view, view->outputs); // as SM50ShaderInternal::output_signature
  shader->entry_view = std::move(view);
}

} // namespace dxmt::dxil

AIRCONV_API void DXILGetEntry(sm50_shader_t shader, DXIL_ENTRY_INFO *out) {
  auto &view = *((dxmt::dxbc::SM50ShaderInternal *)shader)->entry_view;
  *out = {view.is_dxil,
          view.stage,
          (uint32_t)view.inputs.size(),
          (uint32_t)view.outputs.size(),
          (uint32_t)view.resources.size(),
          view.inputs.data(),
          view.outputs.data(),
          view.resources.data()};
}
