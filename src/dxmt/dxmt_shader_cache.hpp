#pragma once
#include "Metal.hpp"
#include "thread.hpp"
#include "airconv_public.h"

extern "C" {
/* dxmt-abi begin */
void *dxmt_shader_cache_create(uint32_t metal_version, uint64_t version); /* writer opened, then reader */
void dxmt_shader_cache_destroy(void *store); /* releases both: SQLite closes, as at today's exit */
bool dxmt_shader_cache_has_reader(void *store);
bool dxmt_shader_cache_has_writer(void *store);
obj_handle_t dxmt_shader_cache_read(void *store, const void *key, uint64_t len); /* +1 DispatchData or 0 */
void dxmt_shader_cache_write(void *store, const void *key, uint64_t len, obj_handle_t data); /* data +0 */
/* dxmt-abi end */
}

namespace dxmt {

// The cache table's version: airconv's and the fork build's (`git describe`), so a DXMT update never reads another
// build's translations. (MacNeutron)
uint64_t ShaderCacheVersion();

// The store itself is Rust's (dxmt_shader_cache_*, src/dxmt/src/shader_cache.rs): it opens a writer, then a reader, on
// the cache file, and takes a lock of its own around each call.
class ShaderCache {
public:
  class Reader {
  public:
    operator bool() const {
      return open_;
    }

    Reader *
    operator->() {
      return this;
    }

    WMT::Reference<WMT::DispatchData>
    get(const void *key, uint64_t length) {
      return WMT::Reference<WMT::DispatchData>(dxmt_shader_cache_read(store_, key, length));
    }

    template <typename K>
    WMT::Reference<WMT::DispatchData>
    get(const K &key) {
      return get(reinterpret_cast<const void *>(&key), sizeof(key));
    }

  private:
    friend class ShaderCache;
    Reader(void *store, bool open) : store_(store), open_(open) {}
    void *store_;
    bool open_;
  };

  class Writer {
  public:
    operator bool() const {
      return open_;
    }

    Writer *
    operator->() {
      return this;
    }

    void
    set(const void *key, uint64_t key_length, WMT::DispatchData value) {
      dxmt_shader_cache_write(store_, key, key_length, value);
    }

    template <typename K>
    void
    set(const K &key, WMT::DispatchData value) {
      set(reinterpret_cast<const void *>(&key), sizeof(key), value);
    }

  private:
    friend class ShaderCache;
    Writer(void *store, bool open) : store_(store), open_(open) {}
    void *store_;
    bool open_;
  };

  static ShaderCache &getInstance(WMTMetalVersion version);

  Writer
  getWriter() {
    return {store_, dxmt_shader_cache_has_writer(store_)};
  }

  Reader
  getReader() {
    return {store_, dxmt_shader_cache_has_reader(store_)};
  }

  ShaderCache(WMTMetalVersion metal_version);
  ShaderCache(const ShaderCache &copy) = delete;
  ~ShaderCache();

private:
  void *store_;
};

} // namespace dxmt