#ifndef DECODE_VIDEOTOOLBOX_H
#define DECODE_VIDEOTOOLBOX_H

#include <stdint.h>
#include <VideoToolbox/VideoToolbox.h>
#include <CoreGraphics/CoreGraphics.h>
#include <ImageIO/ImageIO.h>
#include <CoreVideo/CVBuffer.h>
#include "common.h"

// Structure to hold decoder context
typedef struct {
    VTDecompressionSessionRef session;
    CMFormatDescriptionRef format_desc;
    char* output_directory;
    int frame_count;
} VideoDecoder;

// Initialize the decoder with H.264 parameters
DecoderStatus init_decoder(VideoDecoder* decoder, 
                         const char* output_directory,
                         const uint8_t* sps, size_t sps_size,
                         const uint8_t* pps, size_t pps_size);

// Decode a video frame
DecoderStatus decode_frame(VideoDecoder* decoder, const uint8_t* data, size_t size, CMTime pts);

// Clean up decoder resources
void cleanup_decoder(VideoDecoder* decoder);

#endif // DECODE_VIDEOTOOLBOX_H
