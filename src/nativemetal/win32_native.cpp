/* The native build's Win32 calls (src/util/win32_native.h's inline functions) for the Rust front ends: the one
 * implementation DXMT's C++ uses, on the same handles, exported by winemetal.dylib, which every native dylib links. One
 * process: a duplicate's source and target process are this one, and the security attributes and names Windows takes
 * are never given. Declared in winemetal-sys (src/winemetal/sys/src/lib.rs), whose prototype checks
 * tools/winemetal-layouts.sh --decls prints from this block. */
#include "../util/win32_native.h"

extern "C" {
/* dxmt-abi begin */
void *wmt_native_create_event(int32_t manual, int32_t initial);
void *wmt_native_create_semaphore(int32_t initial, int32_t max);
int32_t wmt_native_release_semaphore(void *handle, int32_t count, int32_t *previous);
uint32_t wmt_native_wait_for_single_object(void *handle, uint32_t milliseconds);
int32_t wmt_native_duplicate_handle(void *source, void **target, uint32_t options);
void *wmt_native_open_thread(uint32_t thread_id);
int32_t wmt_native_close_handle(void *handle);
/* dxmt-abi end */
}

void *wmt_native_create_event(int32_t manual, int32_t initial) {
  return CreateEventW(nullptr, manual, initial, nullptr);
}

void *wmt_native_create_semaphore(int32_t initial, int32_t max) {
  return CreateSemaphoreW(nullptr, initial, max, nullptr);
}

int32_t wmt_native_release_semaphore(void *handle, int32_t count, int32_t *previous) {
  return ReleaseSemaphore(handle, count, previous);
}

uint32_t wmt_native_wait_for_single_object(void *handle, uint32_t milliseconds) {
  return WaitForSingleObject(handle, milliseconds);
}

int32_t wmt_native_duplicate_handle(void *source, void **target, uint32_t options) {
  return DuplicateHandle(GetCurrentProcess(), source, GetCurrentProcess(), target, 0, FALSE, options);
}

void *wmt_native_open_thread(uint32_t thread_id) {
  return OpenThread(THREAD_QUERY_LIMITED_INFORMATION, FALSE, thread_id);
}

int32_t wmt_native_close_handle(void *handle) {
  return CloseHandle(handle);
}
