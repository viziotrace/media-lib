#include "decode-videotoolbox.h"
#include "mp4.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <CoreGraphics/CoreGraphics.h>
#include <CoreMedia/CoreMedia.h>
#include <CoreVideo/CoreVideo.h>
#include <VideoToolbox/VideoToolbox.h>

// Callback function for handling decoded frames
static void decoder_output_callback(void* decompressionOutputRefCon,
                                  void* sourceFrameRefCon,
                                  OSStatus status,
                                  VTDecodeInfoFlags infoFlags,
                                  CVImageBufferRef imageBuffer,
                                  CMTime presentationTimeStamp,
                                  CMTime presentationDuration) {
    if (status != noErr || imageBuffer == NULL) {
        printf("Decoder callback error: %d\n", (int)status);
        return;
    }

    VideoDecoder* decoder = (VideoDecoder*)decompressionOutputRefCon;
    
    // Lock the base address of the pixel buffer
    CVPixelBufferLockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
    
    // Get the pixel buffer details
    size_t width = CVPixelBufferGetWidth(imageBuffer);
    size_t height = CVPixelBufferGetHeight(imageBuffer);
    size_t bytesPerRow = CVPixelBufferGetBytesPerRow(imageBuffer);
    void* baseAddress = CVPixelBufferGetBaseAddress(imageBuffer);
    
    // Create a CG bitmap context
    CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
    CGContextRef context = CGBitmapContextCreate(baseAddress,
                                               width,
                                               height,
                                               8,
                                               bytesPerRow,
                                               colorSpace,
                                               kCGImageAlphaNoneSkipFirst | kCGBitmapByteOrder32Little);
    
    if (!context) {
        printf("Failed to create bitmap context\n");
        CGColorSpaceRelease(colorSpace);
        CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
        return;
    }
    
    CGImageRef cgImage = CGBitmapContextCreateImage(context);
    if (!cgImage) {
        printf("Failed to create CGImage\n");
        CGContextRelease(context);
        CGColorSpaceRelease(colorSpace);
        CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
        return;
    }
    
    // Create output filename
    char filename[1024];
    snprintf(filename, sizeof(filename), "%s/frame_%06d.jpg",
            decoder->output_directory, decoder->frame_count++);
    
    // Create URL reference for the output file
    CFStringRef cfFilename = CFStringCreateWithCString(NULL, filename, kCFStringEncodingUTF8);
    CFURLRef url = CFURLCreateWithFileSystemPath(NULL, cfFilename, kCFURLPOSIXPathStyle, false);
    
    // Create the destination data consumer
    CGImageDestinationRef destination = CGImageDestinationCreateWithURL(url, CFSTR("public.jpeg"), 1, NULL);
    
    if (destination) {
        // Set JPEG compression quality
        CFMutableDictionaryRef options = CFDictionaryCreateMutable(NULL, 1, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        float compression = 0.9f;  // 90% quality
        CFNumberRef compressionNumber = CFNumberCreate(NULL, kCFNumberFloat32Type, &compression);
        CFDictionaryAddValue(options, kCGImageDestinationLossyCompressionQuality, compressionNumber);
        
        // Add the image to the destination with the specified options
        CGImageDestinationAddImage(destination, cgImage, options);
        CGImageDestinationFinalize(destination);
        
        CFRelease(compressionNumber);
        CFRelease(options);
        CFRelease(destination);
    }
    
    // Cleanup
    CFRelease(url);
    CFRelease(cfFilename);
    CGImageRelease(cgImage);
    CGContextRelease(context);
    CGColorSpaceRelease(colorSpace);
    
    // Unlock the buffer
    CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
}

DecoderStatus init_decoder(VideoDecoder* decoder, const char* output_directory, MP4Context* mp4_ctx) {
    if (!decoder || !output_directory || !mp4_ctx) {
        return DECODER_ERROR_INIT;
    }

    memset(decoder, 0, sizeof(VideoDecoder));
    decoder->output_directory = strdup(output_directory);
    decoder->frame_count = 0;

    // Create format description using SPS/PPS from MP4Context
    const uint8_t* parameterSetPointers[2] = { mp4_ctx->sps, mp4_ctx->pps };
    const size_t parameterSetSizes[2] = { mp4_ctx->sps_size, mp4_ctx->pps_size };
    
    OSStatus status = CMVideoFormatDescriptionCreateFromH264ParameterSets(
        kCFAllocatorDefault,
        2,  // parameter set count
        parameterSetPointers,
        parameterSetSizes,
        4,  // NAL length size
        &decoder->format_desc
    );

    if (status != noErr) {
        printf("Failed to create format description: %d\n", (int)status);
        return DECODER_ERROR_INIT;
    }

    // Create video decoder session
    CFMutableDictionaryRef decoder_spec = CFDictionaryCreateMutable(
        kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks);

    // Set video decoder specifications
    int32_t pixel_format = kCVPixelFormatType_32BGRA;
    CFNumberRef pixel_format_ref = CFNumberCreate(NULL, kCFNumberSInt32Type, &pixel_format);
    CFDictionarySetValue(decoder_spec, 
        kCVPixelBufferPixelFormatTypeKey, 
        pixel_format_ref);
    CFRelease(pixel_format_ref);
    
    // Create decompression session
    VTDecompressionOutputCallbackRecord callback = {
        decoder_output_callback,
        decoder
    };
    
    status = VTDecompressionSessionCreate(
        kCFAllocatorDefault,
        decoder->format_desc,
        decoder_spec,
        NULL,
        &callback,
        &decoder->session);
    
    CFRelease(decoder_spec);
    
    if (status != noErr) {
        printf("Failed to create decompression session: %d\n", (int)status);
        return DECODER_ERROR_INIT;
    }
    
    return DECODER_SUCCESS;
}

DecoderStatus decode_frame(VideoDecoder* decoder, const uint8_t* data, size_t size, CMTime pts) {
    if (!decoder || !data || size == 0) {
        return DECODER_ERROR_DECODE;
    }
    
    // Create sample buffer
    CMBlockBufferRef block_buffer;
    OSStatus status = CMBlockBufferCreateWithMemoryBlock(
        kCFAllocatorDefault,
        (void*)data,
        size,
        kCFAllocatorNull,
        NULL,
        0,
        size,
        0,
        &block_buffer);
    
    if (status != noErr) {
        return DECODER_ERROR_DECODE;
    }
    
    // Create sample timing info
    CMSampleTimingInfo timing = {
        .duration = kCMTimeInvalid,
        .presentationTimeStamp = pts,
        .decodeTimeStamp = kCMTimeInvalid
    };
    
    // Create sample buffer
    CMSampleBufferRef sample_buffer;
    status = CMSampleBufferCreate(
        kCFAllocatorDefault,
        block_buffer,
        true,
        NULL,
        NULL,
        decoder->format_desc,
        1,
        1,
        &timing,
        1,
        &size,
        &sample_buffer);
    
    CFRelease(block_buffer);
    
    if (status != noErr) {
        return DECODER_ERROR_DECODE;
    }
    
    // Decode frame
    VTDecodeFrameFlags flags = kVTDecodeFrame_EnableAsynchronousDecompression;
    VTDecodeInfoFlags info_flags;
    status = VTDecompressionSessionDecodeFrame(
        decoder->session,
        sample_buffer,
        flags,
        NULL,
        &info_flags);
    
    CFRelease(sample_buffer);
    
    return (status == noErr) ? DECODER_SUCCESS : DECODER_ERROR_DECODE;
}

void cleanup_decoder(VideoDecoder* decoder) {
    if (decoder) {
        if (decoder->session) {
            VTDecompressionSessionInvalidate(decoder->session);
            CFRelease(decoder->session);
        }
        if (decoder->format_desc) {
            CFRelease(decoder->format_desc);
        }
        free(decoder->output_directory);
        memset(decoder, 0, sizeof(VideoDecoder));
    }
}
