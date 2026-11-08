// File-scope state of this tool lives in its own sections so jtools.c can snapshot and restore it between calls
// (the tools were written as one-shot processes: skyconv's argument parser, for one, refuses a second run).
#pragma clang section bss="__DATA,jtb_aifc" data="__DATA,jtd_aifc"
// Helper names clash with vadpcm_enc and the codebook extractor (separate programs upstream): prefix them.
#define readlooppoints ad_readlooppoints
#define fail_parse ad_fail_parse
#define qsample ad_qsample
#define readaifccodebook ad_readaifccodebook
#define inner_product ad_inner_product
#define main aifc_decode_main
#include "aifc_decode.c"
#pragma clang section bss="" data=""
