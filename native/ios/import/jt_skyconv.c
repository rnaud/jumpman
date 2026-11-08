// File-scope state of this tool lives in its own sections so jtools.c can snapshot and restore it between calls
// (the tools were written as one-shot processes: skyconv's argument parser, for one, refuses a second run).
#pragma clang section bss="__DATA,jtb_sky" data="__DATA,jtd_sky"
#define main skyconv_main
#include "skyconv.c"
#pragma clang section bss="" data=""
