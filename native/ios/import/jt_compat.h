// Force-included (-include) into every decomp tool translation unit so the unmodified C sources can run
// *in-process*: iOS has no fork/exec, so `mio0`, `n64graphics`, `skyconv`, `aifc_decode`, `aiff_extract_codebook` and
// `vadpcm_enc` are linked into the app and called through the `_jtools` Python module.  exit() becomes a longjmp back
// to the caller, stdout/stderr become per-call memory streams.
#ifndef JT_COMPAT_H
#define JT_COMPAT_H
#include <stdio.h>
#include <stdlib.h>
extern FILE *jt_out, *jt_err;
extern void jt_exit(int) __attribute__((noreturn));
#undef stdout
#undef stderr
#define stdout jt_out
#define stderr jt_err
#define exit(rc) jt_exit(rc)
// printf & friends write to the real stdout, not the macro'd `stdout`
#define printf(...) fprintf(jt_out, __VA_ARGS__)
#define puts(s) (fputs((s), jt_out), fputc('\n', jt_out))
#define putchar(c) fputc((c), jt_out)
#endif
