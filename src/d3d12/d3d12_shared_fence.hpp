#pragma once

// MacNeutron: a fence's shared handles (ID3D12Device::CreateSharedHandle and OpenSharedHandle: 3DMark's Steel Nomad),
// one process's. The program's handle is an event used for its identity alone; a duplicate of it and the fence stay
// until every handle the program had on it is closed (D3D12's handle keeps its fence), which each share and open sweep
// for. Any device opens it as the fence itself (one GPU). Header-only on d3d12.h, as the VM's guest forwarder keeps a
// guest program's handles with it too (eac-vm/d3d): the host never sees them.
// ponytail: not another process's (dxmt::Fence starts new Metal events by generation: a mach port of one would go
// stale), no names, and a handle the program duplicated opens nothing (looked up by value).

#include "d3d12.h"
#ifndef _WIN32
#include "win32_native.h" // the native build's handles
#endif
#include <mutex>
#include <unordered_map>

namespace dxmt {

struct SharedFences {
  struct Entry {
    HANDLE ours;
    ID3D12Fence1 *fence; // a reference
  };
  std::mutex mutex;
  std::unordered_map<HANDLE, Entry> entries;

  static SharedFences &
  Get() {
    static SharedFences s; // its fences are plain pointers: none is let go at exit, when DXMT may be gone
    return s;
  }

  // The handles open on h's object, h among them.
  static ULONG
  HandleCount(HANDLE h) {
#ifdef _WIN32
    using PFN_NtQueryObject = LONG(WINAPI *)(HANDLE, int, void *, ULONG, ULONG *);
    static auto query =
        reinterpret_cast<PFN_NtQueryObject>(::GetProcAddress(::GetModuleHandleW(L"ntdll.dll"), "NtQueryObject"));
    ULONG info[14] = {}; // OBJECT_BASIC_INFORMATION (ObjectBasicInformation, 0): HandleCount third
    return query(h, 0, info, sizeof info, nullptr) ? 0 : info[2];
#else
    return __atomic_load_n(&static_cast<mn_win32_handle *>(h)->refs, __ATOMIC_RELAXED);
#endif
  }

  // Under mutex: the fences whose handles the program closed, let go.
  void
  Sweep() {
    for (auto it = entries.begin(); it != entries.end();) {
      if (HandleCount(it->second.ours) > 1) {
        ++it;
        continue;
      }
      CloseHandle(it->second.ours);
      it->second.fence->Release();
      it = entries.erase(it);
    }
  }
};

// CreateSharedHandle's: a handle for a fence made SHARED (or SHARED_CROSS_ADAPTER); E_NOTIMPL for anything else, or a
// name. The attributes and access don't matter to a handle of one process.
inline HRESULT
ShareFence(ID3D12DeviceChild *object, const WCHAR *name, HANDLE *handle) {
  if (!handle)
    return E_INVALIDARG;
  *handle = nullptr;
  ID3D12Fence1 *fence = nullptr;
  if (!object)
    return E_INVALIDARG;
  if (name || FAILED(object->QueryInterface(__uuidof(ID3D12Fence1), (void **)&fence)))
    return E_NOTIMPL;
  if (!(fence->GetCreationFlags() & (D3D12_FENCE_FLAG_SHARED | D3D12_FENCE_FLAG_SHARED_CROSS_ADAPTER))) {
    fence->Release();
    return E_INVALIDARG;
  }
  HANDLE h = CreateEventW(nullptr, TRUE, FALSE, nullptr), ours = nullptr;
  if (!h || !DuplicateHandle(GetCurrentProcess(), h, GetCurrentProcess(), &ours, 0, FALSE, DUPLICATE_SAME_ACCESS)) {
    if (h)
      CloseHandle(h);
    fence->Release();
    return E_OUTOFMEMORY;
  }
  auto &s = SharedFences::Get();
  std::lock_guard<std::mutex> lock(s.mutex);
  s.Sweep();
  s.entries[h] = {ours, fence};
  *handle = h;
  return S_OK;
}

// OpenSharedHandle's: the fence a handle of ShareFence's names, as riid; E_INVALIDARG for any other handle.
inline HRESULT
OpenSharedFence(HANDLE handle, REFIID riid, void **object) {
  if (object)
    *object = nullptr;
  auto &s = SharedFences::Get();
  std::lock_guard<std::mutex> lock(s.mutex);
  s.Sweep();
  auto it = s.entries.find(handle);
  return it == s.entries.end() ? E_INVALIDARG : it->second.fence->QueryInterface(riid, object);
}

} // namespace dxmt
