// File-scope state of this tool lives in its own sections so jtools.c can snapshot and restore it between calls
// (the tools were written as one-shot processes: skyconv's argument parser, for one, refuses a second run).
#pragma clang section bss="__DATA,jtb_mio0" data="__DATA,jtd_mio0"
// mio0 decoder: libmio0.c built with its standalone main, renamed.
#define MIO0_STANDALONE 1
#define main mio0_main
#include "sm64tools/libmio0.c"
#pragma clang section bss="" data=""
