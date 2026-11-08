// File-scope state of this tool lives in its own sections so jtools.c can snapshot and restore it between calls
// (the tools were written as one-shot processes: skyconv's argument parser, for one, refuses a second run).
#pragma clang section bss="__DATA,jtb_cb" data="__DATA,jtd_cb"
#define fail_parse cb_fail_parse
#define readaifccodebook cb_readaifccodebook
#define main aiff_extract_codebook_main
#include "aiff_extract_codebook.c"
#pragma clang section bss="" data=""
