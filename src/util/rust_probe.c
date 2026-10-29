// DXMTRS Phase 0, E1: each binary calls into its Rust static library once, when it is loaded.
#include <stdint.h>
#include <stdlib.h>

uint32_t dxmtrs_probe(void);

__attribute__((constructor)) static void dxmtrs_probe_at_load(void) {
  if (dxmtrs_probe() != 0x44584d54)
    abort();
}
