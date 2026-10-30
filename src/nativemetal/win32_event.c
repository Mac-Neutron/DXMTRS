/* The native build's Win32 events (src/util/win32_native_event.h's inline functions) for winemetal's Rust half, which
 * sets the events a fence's SetEventOnCompletion names and holds them from the registration to the set. */
#include "../util/win32_native_event.h"

int wmt_native_set_event(void *handle) { return mn_win32_set_event(handle); }

void wmt_native_ref(void *handle) { mn_win32_ref(handle); }

int wmt_native_unref(void *handle) { return mn_win32_unref(handle); }
