#ifndef H264_H
#define H264_H

#include <stdint.h>
#include <stdlib.h>

// H.264 specific error codes
typedef enum {
    H264_SUCCESS = 0,
    H264_ERROR_MEMORY = -1,
    H264_ERROR_INVALID_DATA = -2,
    H264_ERROR_PARSE = -3,
    H264_ERROR_INVALID_PARAM = -4
} H264Status;

// H.264 NAL unit types
typedef enum {
    NAL_UNSPECIFIED = 0,
    NAL_SLICE = 1,
    NAL_SLICE_DPA = 2,
    NAL_SLICE_DPB = 3,
    NAL_SLICE_DPC = 4,
    NAL_SLICE_IDR = 5,
    NAL_SEI = 6,
    NAL_SPS = 7,
    NAL_PPS = 8,
    NAL_AUD = 9,
    NAL_END_SEQUENCE = 10,
    NAL_END_STREAM = 11,
    NAL_FILLER_DATA = 12,
    NAL_SPS_EXT = 13,
    NAL_PREFIX = 14,
    NAL_SUBSET_SPS = 15,
    NAL_DEPTH_PS = 16,
    NAL_RESERVED_17 = 17,
    NAL_RESERVED_18 = 18,
    NAL_AUX_SLICE = 19,
    NAL_RESERVED_20 = 20,
    NAL_RESERVED_21 = 21,
    NAL_RESERVED_22 = 22,
    NAL_RESERVED_23 = 23
} NALUnitType;

// Structure to hold H.264 stream parameters
typedef struct {
    uint8_t* sps;
    size_t sps_size;
    uint8_t* pps;
    size_t pps_size;
    int width;
    int height;
    int profile_idc;
    int level_idc;
} H264Context;

// Structure to represent a NAL unit
typedef struct {
    uint8_t* data;
    size_t size;
    NALUnitType type;
} NALUnit;

// Initialize H264 context
H264Context* h264_context_create(void);

// Free H264 context
void h264_context_free(H264Context* ctx);

// Parse H.264 sample data
H264Status h264_parse_sample(H264Context* ctx, const uint8_t* data, size_t size);

// Find and parse NAL units in a buffer
H264Status h264_find_nal_units(const uint8_t* data, size_t size, 
                              NALUnit** nal_units, size_t* nal_count);

// Free NAL units array
void h264_free_nal_units(NALUnit* units, size_t count);

// Helper function to print NAL unit information
void h264_print_nal_unit(const NALUnit* nal);

#endif // H264_H 
