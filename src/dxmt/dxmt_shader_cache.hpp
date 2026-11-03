#pragma once
#include "winemetal.h"
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
