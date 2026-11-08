// File-scope state of this tool lives in its own sections so jtools.c can snapshot and restore it between calls
// (the tools were written as one-shot processes: skyconv's argument parser, for one, refuses a second run).
#pragma clang section bss="__DATA,jtb_n64g" data="__DATA,jtd_n64g"
// n64graphics (+ its utils): standalone main renamed; skyconv links against the same object.
#define N64GRAPHICS_STANDALONE 1
#define main n64graphics_main
#include "sm64tools/n64graphics.c"
#undef main
#include "sm64tools/utils.c"
#pragma clang section bss="" data=""
