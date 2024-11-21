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

// Structure for MP4 context
typedef struct {
    FILE* file;
    uint64_t file_size;
    // Track specific info
    uint32_t video_timescale;
    uint32_t sample_count;
    uint32_t current_sample;
    TrackType track_type;    // Add this field
    uint32_t timescale;      // Add this field
    // Offsets and sizes
    uint64_t* sample_offsets;  // Array of offsets to each frame
    uint32_t* sample_sizes;    // Array of sizes for each frame
} MP4Context;

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

#endif // MP4_H 
