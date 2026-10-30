//! The shader cache and Metal's shader cache path.

use core::ffi::{c_char, c_void};

use winemetal_wire::{
    UnixcallCacheAllocInit, UnixcallCacheGet, UnixcallCacheSet, UnixcallSetmetalcachepath, slot,
};

use crate::transport::{call, mem_ptr};

exports! {
    fn CacheReader_alloc_init(path: *const c_char, version: u64) -> u64 {
        call(slot::CACHE_READER_ALLOC_INIT, |p: &mut UnixcallCacheAllocInit| {
            p.path = mem_ptr(path);
            p.version = version;
        })
        .ret_cache
    }

    fn CacheReader_get(reader: u64, key: *const c_void, length: u64) -> u64 {
        call(slot::CACHE_READER_GET, |p: &mut UnixcallCacheGet| {
            p.cache = reader;
            p.key = mem_ptr(key);
            p.key_length = length;
        })
        .ret_data
    }

    fn CacheWriter_alloc_init(path: *const c_char, version: u64) -> u64 {
        call(slot::CACHE_WRITER_ALLOC_INIT, |p: &mut UnixcallCacheAllocInit| {
            p.path = mem_ptr(path);
            p.version = version;
        })
        .ret_cache
    }

    fn CacheWriter_set(writer: u64, key: *const c_void, key_length: u64, value: u64) {
        call(slot::CACHE_WRITER_SET, |p: &mut UnixcallCacheSet| {
            p.cache = writer;
            p.key = mem_ptr(key);
            p.key_length = key_length;
            p.value_data = value;
        });
    }

    fn WMTSetMetalShaderCachePath(path: *const c_char) -> bool {
        call(slot::WMT_SET_METAL_SHADER_CACHE_PATH, |p: &mut UnixcallSetmetalcachepath| p.path = mem_ptr(path))
            .ret_success
            != 0
    }
}
