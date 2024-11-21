#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h> // For ntohl, ntohs
#include "mp4.h"

// Define box type constants
#define BOX_TYPE_MOOV 0x6D6F6F76  // 'moov'
#define BOX_TYPE_TRAK 0x7472616B  // 'trak'
#define BOX_TYPE_MDIA 0x6D646961  // 'mdia'
#define BOX_TYPE_MINF 0x6D696E66  // 'minf'
#define BOX_TYPE_STBL 0x7374626C  // 'stbl'
#define BOX_TYPE_STSZ 0x7374737A  // 'stsz'
#define BOX_TYPE_STCO 0x7374636F  // 'stco'
#define BOX_TYPE_HDLR 0x68646C72  // 'hdlr'

// Define handler type constants
#define HANDLER_TYPE_VIDEO 0x76696465  // 'vide'
#define HANDLER_TYPE_AUDIO 0x736F756E  // 'soun'

// Add these function declarations at the top with the other static functions
static long find_box(FILE* file, long start_offset, long end_offset, uint32_t box_type);
static int parse_stsz_box(FILE* file, MP4Context* ctx, long offset);
static int parse_stco_box(FILE* file, MP4Context* ctx, long offset);

// Helper function to read a 32-bit big-endian integer
// static uint32_t read_uint32(FILE* fp) {
//     uint8_t buf[4];
//     if (fread(buf, 1, 4, fp) != 4) return 0;
//     return (buf[0] << 24) | (buf[1] << 16) | (buf[2] << 8) | buf[3];
// }

void fourcc_to_string(uint32_t fourcc, char* str) {
    str[0] = (fourcc >> 24) & 0xFF;
    str[1] = (fourcc >> 16) & 0xFF;
    str[2] = (fourcc >> 8) & 0xFF;
    str[3] = fourcc & 0xFF;
    str[4] = '\0';
}

// Parse an MP4 box (atom)
static int read_box_header(FILE* file, MP4BoxHeader* header) {
    uint32_t tmp;
    if (fread(&tmp, 1, 4, file) != 4) return 0;
    header->size = ntohl(tmp);  // Convert from big-endian
    if (fread(&tmp, 1, 4, file) != 4) return 0;  // Changed to read into tmp
    header->type = ntohl(tmp);  // Added ntohl conversion for type
    
    if (header->size == 1) {  // 64-bit size
        if (fread(&header->largesize, 1, 8, file) != 8) return 0;
        // Remove OSSwapBigToHostInt64 as it's not standard C
        // Use platform-independent conversion if needed
        header->largesize = ((uint64_t)ntohl(header->largesize >> 32) << 32) | 
                            ntohl((uint32_t)header->largesize);
    } else {
        header->largesize = header->size;
    }
    return 1;
}

// Helper function to parse track type from hdlr box
static TrackType parse_hdlr_box(FILE* file, long offset) {
    fseek(file, offset + 16, SEEK_SET);
    
    uint32_t handler_type;
    if (fread(&handler_type, sizeof(uint32_t), 1, file) != 1) {
        return TRACK_TYPE_UNKNOWN;
    }
    handler_type = ntohl(handler_type);
    
    if (handler_type == HANDLER_TYPE_VIDEO) {
        return TRACK_TYPE_VIDEO;
    } else if (handler_type == HANDLER_TYPE_AUDIO) {
        return TRACK_TYPE_AUDIO;
    }
    
    return TRACK_TYPE_UNKNOWN;
}

// Add these implementations before mp4_open

// Helper function to find a box within a container
static long find_box(FILE* file, long start_offset, long end_offset, uint32_t box_type) {
    long current_offset = start_offset;
    while (current_offset < end_offset) {
        MP4BoxHeader header;
        
        fseek(file, current_offset, SEEK_SET);
        if (!read_box_header(file, &header)) {
            break;
        }
        
        if (header.type == box_type) {
            return current_offset;
        }
        
        if (header.size == 0) break;
        current_offset += header.largesize;
    }
    return -1;
}

// Parse sample size box
static int parse_stsz_box(FILE* file, MP4Context* ctx, long offset) {
    fseek(file, offset + 8, SEEK_SET);  // Skip box header
    fseek(file, 4, SEEK_CUR);  // Skip version and flags
    
    uint32_t sample_size;
    if (fread(&sample_size, sizeof(uint32_t), 1, file) != 1) return -1;
    sample_size = ntohl(sample_size);
    
    if (fread(&ctx->sample_count, sizeof(uint32_t), 1, file) != 1) return -1;
    ctx->sample_count = ntohl(ctx->sample_count);
    
    ctx->sample_sizes = malloc(ctx->sample_count * sizeof(uint32_t));
    if (!ctx->sample_sizes) return -1;
    
    if (sample_size == 0) {
        // Variable sample sizes
        for (uint32_t i = 0; i < ctx->sample_count; i++) {
            if (fread(&ctx->sample_sizes[i], sizeof(uint32_t), 1, file) != 1) return -1;
            ctx->sample_sizes[i] = ntohl(ctx->sample_sizes[i]);
        }
    } else {
        // Fixed sample size
        for (uint32_t i = 0; i < ctx->sample_count; i++) {
            ctx->sample_sizes[i] = sample_size;
        }
    }
    
    return 0;
}

// Parse chunk offset box
static int parse_stco_box(FILE* file, MP4Context* ctx, long offset) {
    fseek(file, offset + 8, SEEK_SET);  // Skip box header
    fseek(file, 4, SEEK_CUR);  // Skip version and flags
    
    uint32_t entry_count;
    if (fread(&entry_count, sizeof(uint32_t), 1, file) != 1) return -1;
    entry_count = ntohl(entry_count);
    
    ctx->sample_offsets = malloc(entry_count * sizeof(uint64_t));
    if (!ctx->sample_offsets) return -1;
    
    for (uint32_t i = 0; i < entry_count; i++) {
        uint32_t chunk_offset;
        if (fread(&chunk_offset, sizeof(uint32_t), 1, file) != 1) return -1;
        ctx->sample_offsets[i] = ntohl(chunk_offset);
    }
    
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

    // Find hdlr box within mdia
    long hdlr_offset = find_box(file, mdia_offset + 8, mdia_offset + mdia_size, BOX_TYPE_HDLR);
    if (hdlr_offset >= 0) {
        ctx->track_type = parse_hdlr_box(file, hdlr_offset);
    } else {
        goto error;
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
        free(ctx);
    }
}

// Function to read next sample from MP4 file
MP4Status read_next_sample(MP4Context* ctx, MP4Sample* sample) {
    if (!ctx || !sample) {
        return MP4_ERROR_INVALID_PARAM;
    }
    
    if (ctx->current_sample >= ctx->sample_count) {
        return MP4_ERROR_EOF;
    }

    // Validate array bounds
    if (ctx->current_sample >= ctx->sample_count || 
        !ctx->sample_offsets || !ctx->sample_sizes) {
        return MP4_ERROR_INVALID_PARAM;
    }

    // Get sample data
    uint64_t offset = ctx->sample_offsets[ctx->current_sample];
    uint32_t size = ctx->sample_sizes[ctx->current_sample];

    sample->data = (uint8_t*)malloc(size);
    if (!sample->data) {
        return MP4_ERROR_MEMORY;
    }

    // Read sample data
    if (fseek(ctx->file, offset, SEEK_SET) != 0) {
        free(sample->data);
        return MP4_ERROR_IO;
    }

    size_t bytes_read = fread(sample->data, 1, size, ctx->file);
    if (bytes_read != size) {
        free(sample->data);
        return feof(ctx->file) ? MP4_ERROR_EOF : MP4_ERROR_IO;
    }

    // Fill in sample metadata
    sample->size = size;
    sample->pts = CMTimeMake(ctx->current_sample, ctx->timescale);
    sample->track_id = 1;  // For now, assuming single track
    sample->track_type = ctx->track_type;
    sample->timescale = ctx->timescale;

    ctx->current_sample++;
    return MP4_SUCCESS;
}

void free_sample(MP4Sample* sample) {
    if (sample) {
        free(sample->data);
        sample->data = NULL;
        sample->size = 0;
    }
}
