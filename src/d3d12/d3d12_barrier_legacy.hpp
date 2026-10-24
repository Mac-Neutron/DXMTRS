#pragma once

// MacNeutron: enhanced barriers (ID3D12GraphicsCommandList7::Barrier) as the legacy barriers that mean the same to
// DXMT, which orders whole encoders (GPU overlap spec §3): ResourceBarrier then classifies them as it does its own
// (command list 7 spec §5.1). Sync stages, layouts, subresource ranges and the DISCARD flag change nothing here.
// Header-only on d3d12.h, as the VM's guest forwarder translates a forwarded Barrier with it too (eac-vm/d3d).

#include "d3d12.h"
#include <vector>

namespace dxmt {

// The legacy state of an access; *known false for one with none (acceleration structures, video, sampler feedback).
inline D3D12_RESOURCE_STATES
LegacyState(D3D12_BARRIER_ACCESS access, bool *known) {
  static const struct {
    unsigned access;
    unsigned state;
  } kStates[] = {
      {D3D12_BARRIER_ACCESS_VERTEX_BUFFER | D3D12_BARRIER_ACCESS_CONSTANT_BUFFER,
       D3D12_RESOURCE_STATE_VERTEX_AND_CONSTANT_BUFFER},
      {D3D12_BARRIER_ACCESS_INDEX_BUFFER, D3D12_RESOURCE_STATE_INDEX_BUFFER},
      {D3D12_BARRIER_ACCESS_RENDER_TARGET, D3D12_RESOURCE_STATE_RENDER_TARGET},
      {D3D12_BARRIER_ACCESS_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_UNORDERED_ACCESS},
      {D3D12_BARRIER_ACCESS_DEPTH_STENCIL_WRITE, D3D12_RESOURCE_STATE_DEPTH_WRITE},
      {D3D12_BARRIER_ACCESS_DEPTH_STENCIL_READ, D3D12_RESOURCE_STATE_DEPTH_READ},
      {D3D12_BARRIER_ACCESS_SHADER_RESOURCE,
       D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE},
      {D3D12_BARRIER_ACCESS_STREAM_OUTPUT, D3D12_RESOURCE_STATE_STREAM_OUT},
      {D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT, D3D12_RESOURCE_STATE_INDIRECT_ARGUMENT}, // = PREDICATION
      {D3D12_BARRIER_ACCESS_COPY_DEST, D3D12_RESOURCE_STATE_COPY_DEST},
      {D3D12_BARRIER_ACCESS_COPY_SOURCE, D3D12_RESOURCE_STATE_COPY_SOURCE},
      {D3D12_BARRIER_ACCESS_RESOLVE_DEST, D3D12_RESOURCE_STATE_RESOLVE_DEST},
      {D3D12_BARRIER_ACCESS_RESOLVE_SOURCE, D3D12_RESOURCE_STATE_RESOLVE_SOURCE},
      {D3D12_BARRIER_ACCESS_SHADING_RATE_SOURCE, D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE},
  };
  unsigned left = access, state = 0;
  for (auto &s : kStates)
    if (left & s.access) {
      state |= s.state;
      left &= ~s.access;
    }
  *known = !left; // COMMON (0) is COMMON
  return D3D12_RESOURCE_STATES(state);
}

// Accesses that only read: a barrier between two of them orders nothing.
inline bool
ReadOnlyAccess(D3D12_BARRIER_ACCESS access) {
  const unsigned reads = D3D12_BARRIER_ACCESS_VERTEX_BUFFER | D3D12_BARRIER_ACCESS_CONSTANT_BUFFER |
                         D3D12_BARRIER_ACCESS_INDEX_BUFFER | D3D12_BARRIER_ACCESS_DEPTH_STENCIL_READ |
                         D3D12_BARRIER_ACCESS_SHADER_RESOURCE | D3D12_BARRIER_ACCESS_INDIRECT_ARGUMENT |
                         D3D12_BARRIER_ACCESS_COPY_SOURCE | D3D12_BARRIER_ACCESS_RESOLVE_SOURCE |
                         D3D12_BARRIER_ACCESS_SHADING_RATE_SOURCE;
  return access && !(access & ~reads);
}

// A barrier naming any resource, which ResourceBarrier joins (opacity "unknown").
inline D3D12_RESOURCE_BARRIER
AnyResourceBarrier() {
  D3D12_RESOURCE_BARRIER b = {D3D12_RESOURCE_BARRIER_TYPE_ALIASING};
  b.Aliasing = {nullptr, nullptr};
  return b;
}

inline D3D12_RESOURCE_BARRIER
UavBarrier(ID3D12Resource *resource) {
  D3D12_RESOURCE_BARRIER b = {D3D12_RESOURCE_BARRIER_TYPE_UAV};
  b.UAV.pResource = resource;
  return b;
}

// One texture or buffer barrier's: nothing, a UAV barrier, any resource, or a transition of the resource.
inline void
LegacyResourceBarrier(
    D3D12_BARRIER_SYNC sync_before, D3D12_BARRIER_ACCESS before, D3D12_BARRIER_ACCESS after,
    ID3D12Resource *resource, std::vector<D3D12_RESOURCE_BARRIER> &out
) {
  if (sync_before == D3D12_BARRIER_SYNC_NONE || before == D3D12_BARRIER_ACCESS_NO_ACCESS)
    return; // nothing earlier to wait on
  bool known_before, known_after = true;
  auto state_before = LegacyState(before, &known_before);
  // No access after: whatever wrote it still has to end, as when it leaves for COMMON.
  auto state_after =
      after == D3D12_BARRIER_ACCESS_NO_ACCESS ? D3D12_RESOURCE_STATE_COMMON : LegacyState(after, &known_after);
  if (!known_before || !known_after) {
    out.push_back(AnyResourceBarrier());
  } else if (before == D3D12_BARRIER_ACCESS_UNORDERED_ACCESS && after == D3D12_BARRIER_ACCESS_UNORDERED_ACCESS) {
    out.push_back(UavBarrier(resource));
  } else {
    D3D12_RESOURCE_BARRIER b = {D3D12_RESOURCE_BARRIER_TYPE_TRANSITION};
    b.Transition = {resource, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, state_before, state_after};
    out.push_back(b);
  }
}

// A Barrier call's groups as the barriers of one ResourceBarrier call, in `out` (emptied first).
inline void
LegacyBarriers(UINT32 group_count, const D3D12_BARRIER_GROUP *groups, std::vector<D3D12_RESOURCE_BARRIER> &out) {
  out.clear();
  for (UINT32 g = 0; groups && g < group_count; g++) {
    auto &group = groups[g];
    for (UINT32 i = 0; i < group.NumBarriers; i++) {
      switch (group.Type) {
      case D3D12_BARRIER_TYPE_GLOBAL: {
        if (!group.pGlobalBarriers)
          break;
        auto &b = group.pGlobalBarriers[i];
        if (b.SyncBefore == D3D12_BARRIER_SYNC_NONE || b.AccessBefore == D3D12_BARRIER_ACCESS_NO_ACCESS ||
            ReadOnlyAccess(b.AccessBefore))
          break;
        out.push_back(
            b.AccessBefore == D3D12_BARRIER_ACCESS_UNORDERED_ACCESS ? UavBarrier(nullptr) : AnyResourceBarrier()
        );
        break;
      }
      case D3D12_BARRIER_TYPE_TEXTURE:
        if (group.pTextureBarriers) {
          auto &b = group.pTextureBarriers[i];
          LegacyResourceBarrier(b.SyncBefore, b.AccessBefore, b.AccessAfter, b.pResource, out);
        }
        break;
      case D3D12_BARRIER_TYPE_BUFFER:
        if (group.pBufferBarriers) {
          auto &b = group.pBufferBarriers[i];
          LegacyResourceBarrier(b.SyncBefore, b.AccessBefore, b.AccessAfter, b.pResource, out);
        }
        break;
      default:
        out.push_back(AnyResourceBarrier());
        break;
      }
    }
  }
}

// Device10's initial layout as the state a resource's creation takes (ValidateResourceStates': an upload heap's
// GENERIC_READ, a readback heap's COPY_DEST).
inline D3D12_RESOURCE_STATES
InitialState(D3D12_BARRIER_LAYOUT layout, D3D12_HEAP_TYPE heap) {
  if (heap == D3D12_HEAP_TYPE_UPLOAD)
    return D3D12_RESOURCE_STATE_GENERIC_READ;
  if (heap == D3D12_HEAP_TYPE_READBACK)
    return D3D12_RESOURCE_STATE_COPY_DEST;
  switch (layout) {
  case D3D12_BARRIER_LAYOUT_GENERIC_READ:
  case D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_GENERIC_READ:
  case D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_GENERIC_READ:
    return D3D12_RESOURCE_STATE_GENERIC_READ;
  case D3D12_BARRIER_LAYOUT_RENDER_TARGET:
    return D3D12_RESOURCE_STATE_RENDER_TARGET;
  case D3D12_BARRIER_LAYOUT_UNORDERED_ACCESS:
  case D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_UNORDERED_ACCESS:
  case D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_UNORDERED_ACCESS:
    return D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
  case D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_WRITE:
    return D3D12_RESOURCE_STATE_DEPTH_WRITE;
  case D3D12_BARRIER_LAYOUT_DEPTH_STENCIL_READ:
    return D3D12_RESOURCE_STATE_DEPTH_READ;
  case D3D12_BARRIER_LAYOUT_SHADER_RESOURCE:
  case D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_SHADER_RESOURCE:
  case D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_SHADER_RESOURCE:
    return D3D12_RESOURCE_STATES(
        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
    );
  case D3D12_BARRIER_LAYOUT_COPY_SOURCE:
  case D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_SOURCE:
  case D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_SOURCE:
    return D3D12_RESOURCE_STATE_COPY_SOURCE;
  case D3D12_BARRIER_LAYOUT_COPY_DEST:
  case D3D12_BARRIER_LAYOUT_DIRECT_QUEUE_COPY_DEST:
  case D3D12_BARRIER_LAYOUT_COMPUTE_QUEUE_COPY_DEST:
    return D3D12_RESOURCE_STATE_COPY_DEST;
  case D3D12_BARRIER_LAYOUT_RESOLVE_SOURCE:
    return D3D12_RESOURCE_STATE_RESOLVE_SOURCE;
  case D3D12_BARRIER_LAYOUT_RESOLVE_DEST:
    return D3D12_RESOURCE_STATE_RESOLVE_DEST;
  case D3D12_BARRIER_LAYOUT_SHADING_RATE_SOURCE:
    return D3D12_RESOURCE_STATE_SHADING_RATE_SOURCE;
  default: // UNDEFINED, COMMON (PRESENT), the queues' COMMON, video
    return D3D12_RESOURCE_STATE_COMMON;
  }
}

} // namespace dxmt
