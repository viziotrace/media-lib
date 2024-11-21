#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h> // For ntohl, ntohs
#include "mp4.h"

// Add these box type definitions at the top of the file
#define BOX_TYPE_MOOV 0x6D6F6F76  // 'moov'
#define BOX_TYPE_TRAK 0x7472616B  // 'trak'
#define BOX_TYPE_MDIA 0x6D646961  // 'mdia'
#define BOX_TYPE_MINF 0x6D696E66  // 'minf'
#define BOX_TYPE_STBL 0x7374626C  // 'stbl'
#define BOX_TYPE_STSD 0x73747364  // 'stsd'
#define BOX_TYPE_STSZ 0x7374737A  // 'stsz'
#define BOX_TYPE_STCO 0x7374636F  // 'stco'
#define BOX_TYPE_AVCC 0x61766343  // 'avcC'
#define BOX_TYPE_MDHD 0x6D646864  // 'mdhd'
#define BOX_TYPE_TKHD 0x746B6864  // 'tkhd'

// Helper function to find a box within a container
static long find_box(FILE* file, long start_offset, long end_offset, uint32_t box_type) {
    long current_offset = start_offset;
    while (current_offset < end_offset) {
        uint32_t size;
        uint32_t type;
        
        fseek(file, current_offset, SEEK_SET);
        if (fread(&size, sizeof(uint32_t), 1, file) != 1) break;
        if (fread(&type, sizeof(uint32_t), 1, file) != 1) break;
        
        size = ntohl(size);
        type = ntohl(type);
        
        if (type == box_type) {
            return current_offset;
        }
        
        if (size == 0) break;  // Error or end of file
        current_offset += size;
    }
    return -1;
}

// Helper function to parse 'tkhd' box for width and height
static int parse_tkhd_box(FILE* file, MP4Context* ctx, uint64_t box_size) {
    fseek(file, 76, SEEK_CUR); // Skip to width and height
    uint32_t width, height;
    if (fread(&width, sizeof(uint32_t), 1, file) != 1) return -1;
    if (fread(&height, sizeof(uint32_t), 1, file) != 1) return -1;
    ctx->width = ntohl(width) >> 16;
    ctx->height = ntohl(height) >> 16;
    return 0;
}

// Helper function to parse 'stsz' box for sample sizes
static int parse_stsz_box(FILE* file, MP4Context* ctx, uint64_t box_size) {
    fseek(file, 8, SEEK_CUR); // Skip version and flags
    if (fread(&ctx->sample_count, sizeof(uint32_t), 1, file) != 1) return -1;
    ctx->sample_count = ntohl(ctx->sample_count);
    ctx->sample_sizes = (uint32_t*)malloc(ctx->sample_count * sizeof(uint32_t));
    if (!ctx->sample_sizes) return -1;
    for (uint32_t i = 0; i < ctx->sample_count; i++) {
        if (fread(&ctx->sample_sizes[i], sizeof(uint32_t), 1, file) != 1) return -1;
        ctx->sample_sizes[i] = ntohl(ctx->sample_sizes[i]);
    }
    return 0;
}

// Helper function to parse 'stco' box for sample offsets
static int parse_stco_box(FILE* file, MP4Context* ctx, uint64_t box_size) {
    fseek(file, 8, SEEK_CUR); // Skip version and flags
    uint32_t entry_count;
    if (fread(&entry_count, sizeof(uint32_t), 1, file) != 1) return -1;
    entry_count = ntohl(entry_count);
    ctx->sample_offsets = (uint64_t*)malloc(entry_count * sizeof(uint64_t));
    if (!ctx->sample_offsets) return -1;
    for (uint32_t i = 0; i < entry_count; i++) {
        uint32_t offset;
        if (fread(&offset, sizeof(uint32_t), 1, file) != 1) return -1;
        ctx->sample_offsets[i] = ntohl(offset);
    }
    return 0;
}

// Helper function to parse 'avcC' box for SPS and PPS
static int parse_avcc_box(FILE* file, MP4Context* ctx, uint64_t box_size) {
    fseek(file, 6, SEEK_CUR); // Skip to SPS size
    uint16_t sps_size;
    if (fread(&sps_size, sizeof(uint16_t), 1, file) != 1) return -1;
    sps_size = ntohs(sps_size);
    ctx->sps = (uint8_t*)malloc(sps_size);
    if (!ctx->sps) return -1;
    if (fread(ctx->sps, sizeof(uint8_t), sps_size, file) != sps_size) return -1;
    ctx->sps_size = sps_size;

    fseek(file, 1, SEEK_CUR); // Skip to PPS size
    uint16_t pps_size;
    if (fread(&pps_size, sizeof(uint16_t), 1, file) != 1) return -1;
    pps_size = ntohs(pps_size);
    ctx->pps = (uint8_t*)malloc(pps_size);
    if (!ctx->pps) return -1;
    if (fread(ctx->pps, sizeof(uint8_t), pps_size, file) != pps_size) return -1;
    ctx->pps_size = pps_size;

    return 0;
}

// Helper function to parse 'mdhd' box for timescale
static int parse_mdhd_box(FILE* file, MP4Context* ctx, uint64_t box_size) {
    fseek(file, 12, SEEK_CUR); // Skip to timescale
    uint32_t timescale;
    if (fread(&timescale, sizeof(uint32_t), 1, file) != 1) return -1;
    ctx->video_timescale = ntohl(timescale);
    return 0;
}

// Function to open and parse MP4 file
MP4Context* mp4_open(const char* filename) {
    FILE* file = fopen(filename, "rb");
    if (!file) {
        fprintf(stderr, "Failed to open file: %s\n", filename);
        return NULL;
    }

    MP4Context* ctx = (MP4Context*)malloc(sizeof(MP4Context));
    if (!ctx) {
        fclose(file);
        return NULL;
    }
    memset(ctx, 0, sizeof(MP4Context));
    ctx->file = file;

    // Get file size
    fseek(file, 0, SEEK_END);
    ctx->file_size = ftell(file);
    fseek(file, 0, SEEK_SET);

    // Find moov box
    long moov_offset = find_box(file, 0, ctx->file_size, BOX_TYPE_MOOV);
    if (moov_offset < 0) {
        fprintf(stderr, "Failed to find moov box\n");
        goto error;
    }

    uint32_t moov_size;
    fseek(file, moov_offset, SEEK_SET);
    if (fread(&moov_size, sizeof(uint32_t), 1, file) != 1) goto error;
    moov_size = ntohl(moov_size);

    // Find trak box within moov
    long trak_offset = find_box(file, moov_offset + 8, moov_offset + moov_size, BOX_TYPE_TRAK);
    if (trak_offset < 0) {
        fprintf(stderr, "Failed to find trak box\n");
        goto error;
    }

    uint32_t trak_size;
    fseek(file, trak_offset, SEEK_SET);
    if (fread(&trak_size, sizeof(uint32_t), 1, file) != 1) goto error;
    trak_size = ntohl(trak_size);

    // Find mdia box within trak
    long mdia_offset = find_box(file, trak_offset + 8, trak_offset + trak_size, BOX_TYPE_MDIA);
    if (mdia_offset < 0) {
        fprintf(stderr, "Failed to find mdia box\n");
        goto error;
    }

    uint32_t mdia_size;
    fseek(file, mdia_offset, SEEK_SET);
    if (fread(&mdia_size, sizeof(uint32_t), 1, file) != 1) goto error;
    mdia_size = ntohl(mdia_size);

    // Find minf box within mdia
    long minf_offset = find_box(file, mdia_offset + 8, mdia_offset + mdia_size, BOX_TYPE_MINF);
    if (minf_offset < 0) {
        fprintf(stderr, "Failed to find minf box\n");
        goto error;
    }

    uint32_t minf_size;
    fseek(file, minf_offset, SEEK_SET);
    if (fread(&minf_size, sizeof(uint32_t), 1, file) != 1) goto error;
    minf_size = ntohl(minf_size);

    // Find stbl box within minf
    long stbl_offset = find_box(file, minf_offset + 8, minf_offset + minf_size, BOX_TYPE_STBL);
    if (stbl_offset < 0) {
        fprintf(stderr, "Failed to find stbl box\n");
        goto error;
    }

    uint32_t stbl_size;
    fseek(file, stbl_offset, SEEK_SET);
    if (fread(&stbl_size, sizeof(uint32_t), 1, file) != 1) goto error;
    stbl_size = ntohl(stbl_size);

    // Parse required boxes within stbl
    long stsz_offset = find_box(file, stbl_offset + 8, stbl_offset + stbl_size, BOX_TYPE_STSZ);
    if (stsz_offset >= 0) {
        if (parse_stsz_box(file, ctx, stsz_offset) != 0) goto error;
    }

    long stco_offset = find_box(file, stbl_offset + 8, stbl_offset + stbl_size, BOX_TYPE_STCO);
    if (stco_offset >= 0) {
        if (parse_stco_box(file, ctx, stco_offset) != 0) goto error;
    }

    // Find avcC box (usually within stsd box in stbl)
    long stsd_offset = find_box(file, stbl_offset + 8, stbl_offset + stbl_size, BOX_TYPE_STSD);
    if (stsd_offset >= 0) {
        // Skip stsd header and first entry header
        long avc1_offset = stsd_offset + 16;
        long avcc_offset = find_box(file, avc1_offset + 78, avc1_offset + 100, BOX_TYPE_AVCC);
        if (avcc_offset >= 0) {
            if (parse_avcc_box(file, ctx, avcc_offset) != 0) goto error;
        }
    }

    // Find mdhd box for timescale
    long mdhd_offset = find_box(file, mdia_offset + 8, mdia_offset + mdia_size, BOX_TYPE_MDHD);
    if (mdhd_offset >= 0) {
        if (parse_mdhd_box(file, ctx, mdhd_offset) != 0) goto error;
    }

    // Find tkhd box for dimensions
    long tkhd_offset = find_box(file, trak_offset + 8, trak_offset + trak_size, BOX_TYPE_TKHD);
    if (tkhd_offset >= 0) {
        if (parse_tkhd_box(file, ctx, tkhd_offset) != 0) goto error;
    }

    return ctx;

error:
    mp4_close(ctx);
    return NULL;
}

// Function to close MP4 file and free resources
void mp4_close(MP4Context* ctx) {
    if (ctx) {
        if (ctx->file) fclose(ctx->file);
        if (ctx->sample_offsets) free(ctx->sample_offsets);
        if (ctx->sample_sizes) free(ctx->sample_sizes);
        if (ctx->sps) free(ctx->sps);
        if (ctx->pps) free(ctx->pps);
        free(ctx);
    }
}

// Function to read next frame from MP4 file
DecoderStatus read_next_frame(MP4Context* ctx, uint8_t** frame_data, size_t* frame_size, CMTime* pts) {
    if (ctx->current_sample >= ctx->sample_count) return DECODER_ERROR_EOF;

    uint64_t offset = ctx->sample_offsets[ctx->current_sample];
    uint32_t size = ctx->sample_sizes[ctx->current_sample];

    *frame_data = (uint8_t*)malloc(size);
    if (!*frame_data) return DECODER_ERROR_DECODE;

    fseek(ctx->file, offset, SEEK_SET);
    if (fread(*frame_data, sizeof(uint8_t), size, ctx->file) != size) {
        free(*frame_data);
        return DECODER_ERROR_DECODE;
    }

    *frame_size = size;
    *pts = CMTimeMake(ctx->current_sample, ctx->video_timescale);
    ctx->current_sample++;

    return DECODER_SUCCESS;
}
