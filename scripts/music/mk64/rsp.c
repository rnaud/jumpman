// Interpreter for the Mario Kart 64 audio microcode command list.
//
// The decomp's synthesis.c writes `Acmd` words (w0, w1) exactly like the console's RSP task does;
// this decodes them and calls the portable mixer (sm64-port `src/pc/mixer.c`, built for the
// "new" ucode with a DMEM base of 0) that implements each operation.  Bit layouts follow
// `include/PR/abi.h` of n64decomp/mk64.  Pointers in the list are "physical" (virtual - 0x80000000).
#include <stdint.h>

typedef struct {
    uint32_t w0, w1;
} RspCmd;

void aClearBufferImpl(uint16_t addr, int nbytes);
void aLoadBufferImpl(const void* source_addr, uint16_t dest_addr, uint16_t nbytes);
void aSaveBufferImpl(uint16_t source_addr, int16_t* dest_addr, uint16_t nbytes);
void aLoadADPCMImpl(int num_entries_times_16, const int16_t* book_source_addr);
void aSetBufferImpl(uint8_t flags, uint16_t in, uint16_t out, uint16_t nbytes);
void aDMEMMoveImpl(uint16_t in_addr, uint16_t out_addr, int nbytes);
void aSetLoopImpl(int16_t* adpcm_loop_state);
void aADPCMdecImpl(uint8_t flags, int16_t* state);
void aResampleImpl(uint8_t flags, uint16_t pitch, int16_t* state);
void aInterleaveImpl(uint16_t dest, uint16_t left, uint16_t right, uint16_t c);
void aMixImpl(int16_t gain, uint16_t in_addr, uint16_t out_addr, uint16_t count);
void aEnvSetup1Impl(uint8_t initial_vol_wet, uint16_t rate_wet, uint16_t rate_left, uint16_t rate_right);
void aEnvSetup2Impl(uint16_t initial_vol_left, uint16_t initial_vol_right);
void aEnvMixerImpl(uint16_t in_addr, uint16_t n_samples, _Bool swap_reverb, _Bool neg_left, _Bool neg_right,
                   uint16_t dry_left_addr, uint16_t dry_right_addr, uint16_t wet_left_addr, uint16_t wet_right_addr);
void aDMEMMove2Impl(uint8_t t, uint16_t in_addr, uint16_t out_addr, uint16_t count);
void aDownsampleHalfImpl(uint16_t n_samples, uint16_t in_addr, uint16_t out_addr);

// opcodes (abi.h, VERSION_US/EU set)
enum {
    OP_NOOP = 0, OP_ADPCM = 1, OP_CLEARBUFF = 2, OP_RESAMPLE = 5, OP_SEGMENT = 7, OP_SETBUFF = 8,
    OP_DMEMMOVE = 10, OP_LOADADPCM = 11, OP_MIXER = 12, OP_INTERLEAVE = 13, OP_SETLOOP = 15,
    OP_DMEMMOVE2 = 16, OP_DOWNSAMPLE_HALF = 17, OP_ENVSETUP1 = 18, OP_ENVMIXER = 19, OP_LOADBUFF = 20,
    OP_SAVEBUFF = 21, OP_ENVSETUP2 = 22
};

static void* phys(uint32_t w) { return (void*) (uintptr_t) (w + 0x80000000u); }

unsigned rsp_unknown_ops;

void rsp_run(const RspCmd* c, int n) {
    uint16_t buf_in = 0, buf_out = 0, buf_len = 0;
    for (int i = 0; i < n; i++) {
        uint32_t w0 = c[i].w0, w1 = c[i].w1;
        switch (w0 >> 24) {
            case OP_NOOP:
            case OP_SEGMENT:
                break;
            case OP_ADPCM: aADPCMdecImpl((w0 >> 16) & 0xFF, phys(w1)); break;
            case OP_CLEARBUFF: aClearBufferImpl(w0 & 0xFFFF, (int) w1); break;
            case OP_RESAMPLE: aResampleImpl((w0 >> 16) & 0xFF, w0 & 0xFFFF, phys(w1)); break;
            case OP_SETBUFF:
                buf_in = w0 & 0xFFFF;
                buf_out = w1 >> 16;
                buf_len = w1 & 0xFFFF;
                aSetBufferImpl((w0 >> 16) & 0xFF, buf_in, buf_out, buf_len);
                break;
            case OP_DMEMMOVE: aDMEMMoveImpl(w0 & 0xFFFF, w1 >> 16, w1 & 0xFFFF); break;
            case OP_LOADADPCM: aLoadADPCMImpl(w0 & 0xFFFFFF, phys(w1)); break;
            case OP_MIXER:
                aMixImpl((int16_t) (w0 & 0xFFFF), w1 >> 16, w1 & 0xFFFF, ((w0 >> 16) & 0xFF) << 4);
                break;
            case OP_INTERLEAVE:
                // two-operand form: destination and length come from the preceding aSetBuffer
                aInterleaveImpl(buf_out, w1 >> 16, w1 & 0xFFFF, (buf_len + 15) & ~15);
                break;
            case OP_SETLOOP: aSetLoopImpl(phys(w1)); break;
            case OP_DMEMMOVE2: aDMEMMove2Impl((w0 >> 16) & 0xFF, w0 & 0xFFFF, w1 >> 16, w1 & 0xFFFF); break;
            case OP_DOWNSAMPLE_HALF: aDownsampleHalfImpl(w0 & 0xFFFF, w1 >> 16, w1 & 0xFFFF); break;
            case OP_ENVSETUP1: aEnvSetup1Impl((w0 >> 16) & 0xFF, w0 & 0xFFFF, w1 >> 16, w1 & 0xFFFF); break;
            case OP_ENVSETUP2: aEnvSetup2Impl(w1 >> 16, w1 & 0xFFFF); break;
            case OP_ENVMIXER:
                aEnvMixerImpl(((w0 >> 16) & 0xFF) << 4, (w0 >> 8) & 0xFF, (w0 >> 2) & 1, (w0 >> 1) & 1, w0 & 1,
                              ((w1 >> 24) & 0xFF) << 4, ((w1 >> 16) & 0xFF) << 4, ((w1 >> 8) & 0xFF) << 4,
                              (w1 & 0xFF) << 4);
                break;
            case OP_LOADBUFF: aLoadBufferImpl(phys(w1), w0 & 0xFFFF, ((w0 >> 16) & 0xFF) << 4); break;
            case OP_SAVEBUFF: aSaveBufferImpl(w0 & 0xFFFF, phys(w1), ((w0 >> 16) & 0xFF) << 4); break;
            default: rsp_unknown_ops++; break;
        }
    }
}
