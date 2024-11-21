#ifndef MP4_H
#define MP4_H

#include <stdio.h>
#include <stdint.h>
#include <CoreMedia/CoreMedia.h>

// Track types
typedef enum {
    TRACK_TYPE_UNKNOWN = 0,
    TRACK_TYPE_VIDEO = 1,
    TRACK_TYPE_AUDIO = 2,
} TrackType;

// MP4 specific error codes
typedef enum {
    MP4_SUCCESS = 0,
    MP4_ERROR_IO = -1,
    MP4_ERROR_MEMORY = -2,
    MP4_ERROR_FORMAT = -3,
    MP4_ERROR_EOF = -4,
    MP4_ERROR_INVALID_PARAM = -5
    
} MP4Status;

// Structure for MP4 box header
typedef struct {
    uint32_t size;
    uint32_t type;
    uint64_t largesize;  // for 64-bit sizes
} MP4BoxHeader;

// Structure to represent a sample from the MP4
typedef struct {
    uint8_t* data;           // Sample data
    size_t size;            // Size of the sample data
    CMTime pts;             // Presentation timestamp
    uint32_t track_id;      // Track this sample belongs to
    TrackType track_type;   // Type of track (video/audio)
    uint32_t timescale;     // Timescale for this track
} MP4Sample;

// Add new structure for H264 specific parameters
typedef struct {
    uint8_t* sps;
    size_t sps_size;
    uint8_t* pps;
    size_t pps_size;
    uint32_t width;
    uint32_t height;
} H264Parameters;

// Structure for MP4 context
typedef struct {
    FILE* file;
    uint64_t file_size;
    // Track specific info
    uint32_t video_timescale;
    uint32_t sample_count;
    uint32_t current_sample;
    TrackType track_type;
    uint32_t timescale;
    // Video specific parameters
    H264Parameters h264_params;
    // Offsets and sizes
    uint64_t* sample_offsets;
    uint32_t* sample_sizes;
} MP4Context;

// Add these new structures and types
typedef struct MP4Box {
    uint32_t type;
    uint64_t size;
    long offset;
    struct MP4Box* parent;
    struct MP4Box* first_child;
    struct MP4Box* next_sibling;
} MP4Box;

// Function declarations
MP4Context* mp4_open(const char* filename);
void mp4_close(MP4Context* ctx);

// New sample reading interface
MP4Status read_next_sample(MP4Context* ctx, MP4Sample* sample);
void free_sample(MP4Sample* sample);  // Helper to clean up sample data

// Helper function to convert FourCC to string
void fourcc_to_string(uint32_t fourcc, char* str);

// Add these FourCC constants
#define TRACK_TYPE_VIDEO_FOURCC 0x76696465  // 'vide'
#define TRACK_TYPE_AUDIO_FOURCC 0x736F756E  // 'soun'

// Add these box type constants
#define BOX_TYPE_AVCC 0x61766343  // 'avcC'
#define BOX_TYPE_STSD 0x73747364  // 'stsd'

// Add these function declarations
MP4Box* create_box_tree(FILE* file, long start_offset, long end_offset, MP4Box* parent);
void free_box_tree(MP4Box* box);
MP4Box* find_box_by_type(MP4Box* root, uint32_t type);
MP4Box* find_next_box_by_type(MP4Box* current, uint32_t type);

#endif // MP4_H 
