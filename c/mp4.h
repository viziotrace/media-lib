#ifndef MP4_H
#define MP4_H

#include <stdio.h>
#include <stdint.h>
#include <CoreMedia/CoreMedia.h>
#include "common.h"

// Structure for MP4 box header
typedef struct {
    uint32_t size;
    uint32_t type;
    uint64_t largesize;  // for 64-bit sizes
} MP4BoxHeader;

// Structure for MP4 context
typedef struct {
    FILE* file;
    uint64_t file_size;
    // Track specific info
    uint32_t video_timescale;
    uint32_t sample_count;
    uint32_t current_sample;
    // Offsets and sizes
    uint64_t* sample_offsets;  // Array of offsets to each frame
    uint32_t* sample_sizes;    // Array of sizes for each frame
    // Parameter sets
    uint8_t* sps;
    uint32_t sps_size;
    uint8_t* pps;
    uint32_t pps_size;
    uint32_t width;      // Video width
    uint32_t height;     // Video height
} MP4Context;

// Function declarations
MP4Context* mp4_open(const char* filename);
void mp4_close(MP4Context* ctx);
DecoderStatus read_next_frame(MP4Context* ctx, uint8_t** frame_data, 
                            size_t* frame_size, CMTime* pts);

// Helper function to convert FourCC to string
void fourcc_to_string(uint32_t fourcc, char* str);

#endif // MP4_H 
