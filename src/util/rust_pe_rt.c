// DXMTRS: what Rust's *-pc-windows-msvc code needs from MSVC's runtime that llvm-mingw's lacks.
#if defined(_WIN32) && defined(__x86_64__) && !defined(__arm64ec__) // ARM64EC defines __x86_64__ too
// The stack probe: MSVC's __chkstk and mingw's ___chkstk_ms (compiler-rt) both only touch the pages below rsp, leaving
// rsp alone, so a jump keeps the caller's frame exactly as __chkstk would.
__asm__(".globl __chkstk\n__chkstk:\n\tjmp ___chkstk_ms\n");
#endif
#if defined(_WIN32) && defined(__i386__)
// The stack probe: MSVC's _chkstk and compiler-rt's i386 ___chkstk both probe and then lower esp by eax.
__asm__(".globl __chkstk\n__chkstk:\n\tjmp ___chkstk\n");
// The -msvc target reads the TEB's TLS array at the absolute __tls_array, which MSVC's CRT defines (the TEB's
// ThreadLocalStoragePointer); llvm-mingw's doesn't.
__asm__(".globl __tls_array\n.set __tls_array, 0x2c\n");
#endif
