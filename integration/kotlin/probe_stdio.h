/*
 * probe_stdio.h — minimal CRT surface for the probe's host FILE* song_io.
 *
 * Declared locally (not via <stdio.h>) because cinterop drops declarations
 * that arrive from system headers through an include chain. These are
 * binding declarations only — resolved at link time against the platform
 * CRT (msvcrt / glibc).
 *
 * The frozen song_io contract carries 64-bit offsets on every platform,
 * so the seek/tell binding is split: Windows takes _fseeki64/_ftelli64
 * (mingw `long` is 32-bit and would clip offsets to +/-2GB); Linux keeps
 * fseek/ftell (glibc `long` is 64-bit).
 *
 * Consumer-side only — never included by production code.
 */
#ifndef QIANQIAN_PROBE_STDIO_H
#define QIANQIAN_PROBE_STDIO_H

#include <stddef.h>

/* Opaque marker type for pointer passing; never dereferenced from Kotlin. */
typedef struct qn_probe_FILE FILE;

extern FILE *fopen(const char *filename, const char *mode);
extern int   fclose(FILE *stream);
extern size_t fread(void *dst, size_t size, size_t count, FILE *stream);

#if defined(_WIN32)
extern int        _fseeki64(FILE *stream, long long offset, int whence);
extern long long  _ftelli64(FILE *stream);
#else
extern int   fseek(FILE *stream, long offset, int whence);
extern long  ftell(FILE *stream);
#endif

#define PROBE_SEEK_SET 0
#define PROBE_SEEK_CUR 1
#define PROBE_SEEK_END 2

#endif /* QIANQIAN_PROBE_STDIO_H */
