#include "decode-videotoolbox.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <CoreGraphics/CoreGraphics.h>
#include <CoreMedia/CoreMedia.h>
#include <CoreVideo/CoreVideo.h>
#include <VideoToolbox/VideoToolbox.h>
#include <Accelerate/Accelerate.h>

// Add these H.264 NAL unit type definitions
#define H264_NAL_SLICE 1
#define H264_NAL_IDR_SLICE 5
#define H264_NAL_SEI 6
#define H264_NAL_SPS 7
#define H264_NAL_PPS 8
#define H264_NAL_AUD 9

// Add this validation function
static int validate_h264_sample(const uint8_t *data, size_t size, uint8_t nal_length_size)
{
    size_t offset = 0;
    int valid_nals_found = 0;

    printf("Validating H.264 sample (size: %zu, NAL length size: %u)\n", size, nal_length_size);

    while (offset + nal_length_size <= size)
    {
        // Read NAL unit length (assuming big-endian storage)
        uint32_t nal_size = 0;
        if (nal_length_size == 4)
        {
            // Read as a single 32-bit value
            nal_size = (data[offset] << 24) | (data[offset + 1] << 16) |
                       (data[offset + 2] << 8) | data[offset + 3];
        }
        else
        {
            // Fallback for other sizes
            for (int i = 0; i < nal_length_size; i++)
            {
                nal_size = (nal_size << 8) | data[offset + i];
            }
        }

        // Print the raw bytes for debugging
        printf("NAL length bytes at offset %zu: ", offset);
        for (int i = 0; i < nal_length_size; i++)
        {
            printf("%02x ", data[offset + i]);
        }
        printf("-> size=%u\n", nal_size);

        // Validate NAL unit size
        if (nal_size == 0 || offset + nal_length_size + nal_size > size)
        {
            printf("Invalid NAL unit size: %u at offset %zu (total size: %zu)\n",
                   nal_size, offset, size);
            return 0;
        }

        // Get NAL unit type
        uint8_t nal_type = data[offset + nal_length_size] & 0x1F;
        printf("NAL unit at offset %zu: size=%u, type=%u\n", offset, nal_size, nal_type);

        // Validate NAL unit type
        switch (nal_type)
        {
        case H264_NAL_SLICE:
        case H264_NAL_IDR_SLICE:
        case H264_NAL_SEI:
        case H264_NAL_SPS:
        case H264_NAL_PPS:
        case H264_NAL_AUD:
            valid_nals_found++;
            break;
        default:
            printf("Warning: Unknown NAL unit type: %u\n", nal_type);
            break;
        }

        // Move to next NAL unit
        offset += nal_length_size + nal_size;
    }

    // Check if we found any valid NAL units
    if (valid_nals_found == 0)
    {
        printf("No valid NAL units found in sample\n");
        return 0;
    }

    // Check if we consumed exactly all bytes
    if (offset != size)
    {
        printf("Warning: Sample size mismatch. Consumed %zu of %zu bytes\n", offset, size);
        return 0;
    }

    printf("Sample validation successful: found %d valid NAL units\n", valid_nals_found);
    return 1;
}

// Callback function for handling decoded frames
static void decoder_output_callback(void *decompressionOutputRefCon,
                                    void *sourceFrameRefCon,
                                    OSStatus status,
                                    VTDecodeInfoFlags infoFlags,
                                    CVImageBufferRef imageBuffer,
                                    CMTime presentationTimeStamp,
                                    CMTime presentationDuration)
{
    if (status != noErr || imageBuffer == NULL)
    {
        printf("Decoder callback error: %d (0x%x)\n", (int)status, (int)status);
        return;
    }

    VideoDecoder *decoder = (VideoDecoder *)decompressionOutputRefCon;

    // Lock the base address of the pixel buffer
    CVPixelBufferLockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);

    // Get the pixel buffer details
    size_t width = CVPixelBufferGetWidth(imageBuffer);
    size_t height = CVPixelBufferGetHeight(imageBuffer);
    // OSType pixelFormat = CVPixelBufferGetPixelFormatType(imageBuffer);

    // Get plane data for YUV
    uint8_t *yPlane = CVPixelBufferGetBaseAddressOfPlane(imageBuffer, 0);
    uint8_t *uvPlane = CVPixelBufferGetBaseAddressOfPlane(imageBuffer, 1);
    size_t yStride = CVPixelBufferGetBytesPerRowOfPlane(imageBuffer, 0);
    size_t uvStride = CVPixelBufferGetBytesPerRowOfPlane(imageBuffer, 1);

    // Create vImage buffers for conversion
    vImage_Buffer srcY = {
        .data = yPlane,
        .width = width,
        .height = height,
        .rowBytes = yStride};

    vImage_Buffer srcUV = {
        .data = uvPlane,
        .width = width / 2,
        .height = height / 2,
        .rowBytes = uvStride};

    // Allocate memory for RGB output
    uint8_t *rgbData = malloc(width * height * 4); // 4 bytes per pixel for RGBA
    vImage_Buffer destRGB = {
        .data = rgbData,
        .width = width,
        .height = height,
        .rowBytes = width * 4};

    // Setup conversion info
    vImage_YpCbCrToARGB info;
    vImage_YpCbCrPixelRange pixelRange = {
        .Yp_bias = 16,
        .CbCr_bias = 128,
        .YpRangeMax = 235,
        .CbCrRangeMax = 240,
        .YpMax = 255,
        .YpMin = 0,
        .CbCrMax = 255,
        .CbCrMin = 0};

    vImageConvert_YpCbCrToARGB_GenerateConversion(
        kvImage_YpCbCrToARGBMatrix_ITU_R_601_4,
        &pixelRange,
        &info,
        kvImage420Yp8_CbCr8,
        kvImageARGB8888,
        0);

    // Perform the conversion
    vImage_Error error = vImageConvert_420Yp8_CbCr8ToARGB8888(
        &srcY,
        &srcUV,
        &destRGB,
        &info,
        NULL,
        255,
        kvImageNoFlags);

    if (error != kvImageNoError)
    {
        printf("vImage conversion error: %ld\n", error);
        free(rgbData);
        CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
        return;
    }

    // Create CGImage from the RGB data
    CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
    CGContextRef context = CGBitmapContextCreate(
        rgbData,
        width,
        height,
        8,
        width * 4,
        colorSpace,
        kCGImageAlphaNoneSkipFirst | kCGBitmapByteOrder32Little);

    if (!context)
    {
        printf("Failed to create bitmap context\n");
        CGColorSpaceRelease(colorSpace);
        free(rgbData);
        CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
        return;
    }

    CGImageRef cgImage = CGBitmapContextCreateImage(context);
    if (!cgImage)
    {
        printf("Failed to create CGImage\n");
        CGContextRelease(context);
        CGColorSpaceRelease(colorSpace);
        free(rgbData);
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

    if (destination)
    {
        // Set JPEG compression quality
        CFMutableDictionaryRef options = CFDictionaryCreateMutable(NULL, 1, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
        float compression = 0.9f; // 90% quality
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
    free(rgbData);

    // Unlock the buffer
    CVPixelBufferUnlockBaseAddress(imageBuffer, kCVPixelBufferLock_ReadOnly);
}

DecoderStatus init_decoder(VideoDecoder *decoder,
                           const char *output_directory,
                           MP4Context *mp4_ctx)
{
    if (!decoder || !output_directory || !mp4_ctx)
    {
        return DECODER_ERROR_INIT;
    }

    memset(decoder, 0, sizeof(VideoDecoder));
    decoder->output_directory = strdup(output_directory);
    decoder->frame_count = 0;

    // Create format description using SPS/PPS from MP4Context
    const uint8_t *parameterSetPointers[2] = {
        mp4_ctx->h264_params.sps,
        mp4_ctx->h264_params.pps};
    const size_t parameterSetSizes[2] = {
        mp4_ctx->h264_params.sps_size,
        mp4_ctx->h264_params.pps_size};

    OSStatus status = CMVideoFormatDescriptionCreateFromH264ParameterSets(
        kCFAllocatorDefault,
        2, // parameter set count
        parameterSetPointers,
        parameterSetSizes,
        mp4_ctx->h264_params.nal_length_size,
        &decoder->format_desc);

    if (status != noErr)
    {
        printf("Failed to create format description: %d\n", (int)status);
        return DECODER_ERROR_INIT;
    }

    // Create video decoder session
    CFMutableDictionaryRef decoder_spec = CFDictionaryCreateMutable(
        kCFAllocatorDefault, 0, &kCFTypeDictionaryKeyCallBacks,
        &kCFTypeDictionaryValueCallBacks);

    // Set video decoder specifications
    int32_t pixel_format = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
    CFNumberRef pixel_format_ref = CFNumberCreate(NULL, kCFNumberSInt32Type, &pixel_format);
    CFDictionarySetValue(decoder_spec,
                         kCVPixelBufferPixelFormatTypeKey,
                         pixel_format_ref);
    CFRelease(pixel_format_ref);

    // Create decompression session
    VTDecompressionOutputCallbackRecord callback = {
        decoder_output_callback,
        decoder};

    status = VTDecompressionSessionCreate(
        kCFAllocatorDefault,
        decoder->format_desc,
        decoder_spec,
        NULL,
        &callback,
        &decoder->session);

    CFRelease(decoder_spec);

    if (status != noErr)
    {
        printf("Failed to create decompression session: %d\n", (int)status);
        return DECODER_ERROR_INIT;
    }

    // Print video information
    CMVideoDimensions dimensions = CMVideoFormatDescriptionGetDimensions(decoder->format_desc);
    printf("Created decoder for video: %dx%d\n", dimensions.width, dimensions.height);

    return DECODER_SUCCESS;
}

DecoderStatus decode_frame(VideoDecoder *decoder, const uint8_t *data, size_t size, CMTime pts)
{
    if (!decoder || !data || size == 0)
    {
        return DECODER_ERROR_DECODE;
    }

    // Get NAL length size from format description
    int nal_length_size = 4; // Default to 4
    CFDictionaryRef extensions = CMFormatDescriptionGetExtensions(decoder->format_desc);
    if (extensions)
    {
        CFNumberRef length_ref = CFDictionaryGetValue(extensions, CFSTR("NALUnitLength"));
        if (length_ref)
        {
            CFNumberGetValue(length_ref, kCFNumberIntType, &nal_length_size);
        }
    }

    // Validate H.264 sample
    if (!validate_h264_sample(data, size, nal_length_size))
    {
        printf("H.264 sample validation failed\n");
        return DECODER_ERROR_DECODE;
    }

    // Create sample buffer
    CMBlockBufferRef block_buffer;
    OSStatus status = CMBlockBufferCreateWithMemoryBlock(
        kCFAllocatorDefault,
        (void *)data,
        size,
        kCFAllocatorNull,
        NULL,
        0,
        size,
        0,
        &block_buffer);

    if (status != noErr)
    {
        return DECODER_ERROR_DECODE;
    }

    // Create sample timing info
    CMSampleTimingInfo timing = {
        .duration = kCMTimeInvalid,
        .presentationTimeStamp = pts,
        .decodeTimeStamp = kCMTimeInvalid};

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

    if (status != noErr)
    {
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

void cleanup_decoder(VideoDecoder *decoder)
{
    if (decoder)
    {
        if (decoder->session)
        {
            VTDecompressionSessionInvalidate(decoder->session);
            CFRelease(decoder->session);
        }
        if (decoder->format_desc)
        {
            CFRelease(decoder->format_desc);
        }
        free(decoder->output_directory);
        memset(decoder, 0, sizeof(VideoDecoder));
    }
}

DecoderStatus flush_decoder(VideoDecoder *decoder)
{
    if (!decoder || !decoder->session)
    {
        return DECODER_ERROR_DECODE;
    }

    // Flush any remaining frames
    VTDecompressionSessionFinishDelayedFrames(decoder->session);
    VTDecompressionSessionWaitForAsynchronousFrames(decoder->session);

    return DECODER_SUCCESS;
}
