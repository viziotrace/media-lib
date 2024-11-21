#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <errno.h>
#include "mp4.h"
#include "decode-videotoolbox.h"

// Create directory if it doesn't exist
static int ensure_directory_exists(const char* path) {
    struct stat st;
    if (stat(path, &st) == 0) {
        return S_ISDIR(st.st_mode) ? 0 : -1;
    }
    
    // Create directory with permissions 0755
    if (mkdir(path, 0755) != 0) {
        printf("Failed to create directory %s: %s\n", path, strerror(errno));
        return -1;
    }
    return 0;
}

int main(int argc, char *argv[]) {
    if (argc != 3) {
        printf("Usage: %s <input_file> <output_directory>\n", argv[0]);
        return 1;
    }

    const char* input_file = argv[1];
    const char* output_dir = argv[2];

    // Ensure output directory exists
    if (ensure_directory_exists(output_dir) != 0) {
        printf("Failed to create or access output directory: %s\n", output_dir);
        return 1;
    }

    // Open MP4 file first
    MP4Context* mp4_ctx = mp4_open(input_file);
    if (!mp4_ctx) {
        printf("Failed to open MP4 file\n");
        return 1;
    }

    // Print MP4 file details
    printf("\nMP4 File Details:\n");
    printf("File size: %llu bytes\n", mp4_ctx->file_size);
    printf("Video timescale: %u\n", mp4_ctx->video_timescale);
    printf("Total frames: %u\n", mp4_ctx->sample_count);
    printf("SPS size: %u bytes\n", mp4_ctx->sps_size);
    printf("PPS size: %u bytes\n", mp4_ctx->pps_size);
    printf("Video width: %u\n", mp4_ctx->width);
    printf("Video height: %u\n", mp4_ctx->height);
    
    printf("\n");

    // Initialize decoder with MP4 context
    VideoDecoder decoder;
    DecoderStatus status = init_decoder(&decoder, output_dir, mp4_ctx);
    if (status != DECODER_SUCCESS) {
        printf("Failed to initialize decoder\n");
        mp4_close(mp4_ctx);
        return 1;
    }

    printf("Decoder initialized successfully\n");
    printf("Input file: %s\n", input_file);
    printf("Output directory: %s\n", output_dir);

    // Read and decode frames
    uint8_t* frame_data = NULL;
    size_t frame_size;
    CMTime pts = kCMTimeZero;
    int frame_index = 0;
    
    while (1) {
        // Read next frame from MP4
        status = read_next_frame(mp4_ctx, &frame_data, &frame_size, &pts);
        if (status == DECODER_ERROR_EOF) {
            break; // End of file
        } else if (status != DECODER_SUCCESS) {
            printf("Failed to read frame %d\n", frame_index);
            break;
        }

        // Decode the frame
        status = decode_frame(&decoder, frame_data, frame_size, pts);
        if (status != DECODER_SUCCESS) {
            printf("Failed to decode frame %d\n", frame_index);
            free(frame_data);
            break;
        }

        frame_index++;
        free(frame_data);
        frame_data = NULL;
    }

    printf("Processed %d frames\n", frame_index);
    
    // Cleanup
    if (frame_data) {
        free(frame_data);
    }
    cleanup_decoder(&decoder);
    mp4_close(mp4_ctx);
    return 0;
}
