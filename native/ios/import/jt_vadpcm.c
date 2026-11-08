// File-scope state of this tool lives in its own sections so jtools.c can snapshot and restore it between calls
// (the tools were written as one-shot processes: skyconv's argument parser, for one, refuses a second run).
#pragma clang section bss="__DATA,jtb_vad" data="__DATA,jtd_vad"
#define main vadpcm_enc_main
#include "sdk-tools/adpcm/vadpcm_enc.c"
#undef main
#include "sdk-tools/adpcm/vpredictor.c"
#include "sdk-tools/adpcm/quant.c"
#include "sdk-tools/adpcm/util.c"
#include "sdk-tools/adpcm/vencode.c"
#pragma clang section bss="" data=""
