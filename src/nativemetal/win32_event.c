/* The native build's Win32 events (src/util/win32_native_event.h's inline functions), handle reference count and
 * thread-local not-implemented flag for winemetal's Rust half and the Rust front ends. */
#include <stdbool.h>
#include "../util/win32_native_event.h"

static _Thread_local bool not_implemented;

int wmt_native_set_event(void *handle) { return mn_win32_set_event(handle); }

void wmt_native_ref(void *handle) { mn_win32_ref(handle); }

int wmt_native_unref(void *handle) { return mn_win32_unref(handle); }

void wmt_native_not_implemented(void) { not_implemented = true; }

bool MTLD3DNotImplemented(void) {
  bool v = not_implemented;
  not_implemented = false;
  return v;
}

uint32_t wmt_native_handle_count(void *handle) {
  return mn_win32_is_handle(handle)
             ? (uint32_t)__atomic_load_n(&((struct mn_win32_handle *)handle)->refs, __ATOMIC_RELAXED)
             : 0;
}
