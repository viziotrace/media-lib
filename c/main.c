#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <errno.h>
#include "mp4.h"
#include "h264.h"
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

// Helper function to print track type
static const char* track_type_to_string(TrackType type) {
    switch (type) {
        case TRACK_TYPE_VIDEO:
            return "Video";
        case TRACK_TYPE_AUDIO:
            return "Audio";
        default:
            return "Unknown";
    }
}

// Helper function to print sample information
static void print_sample_info(const MP4Sample* sample, int index) {
    printf("\nSample %d:\n", index);
    printf("├─ Track Type: %s\n", track_type_to_string(sample->track_type));
    printf("├─ Track ID: %u\n", sample->track_id);
    printf("├─ Size: %zu bytes\n", sample->size);
    printf("├─ Timescale: %u\n", sample->timescale);
    printf("├─ PTS: %lld/%d (%.3f seconds)\n", 
           sample->pts.value, sample->pts.timescale,
           (float)sample->pts.value / sample->pts.timescale);
    
    // Add hex dump of first 16 bytes (or less if sample is smaller)
    printf("└─ First bytes: ");
    const size_t bytes_to_show = sample->size < 16 ? sample->size : 16;
    for (size_t i = 0; i < bytes_to_show; i++) {
        printf("%02x ", (unsigned char)sample->data[i]);
    }
    printf("\n");
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

    // First pass: Parse H.264 parameters
    MP4Context* mp4_ctx = mp4_open(input_file);
    if (!mp4_ctx) {
        printf("Failed to open MP4 file\n");
        return 1;
    }

    int sample_index = 0;
    H264Context* h264_ctx = h264_context_create();
    if (!h264_ctx) {
        printf("Failed to create H.264 context\n");
        mp4_close(mp4_ctx);
        return 1;
    }

    // Read samples until we find SPS and PPS
    MP4Sample sample;
    const int MAX_SAMPLES = 1000000;  // Safety limit
    while (sample_index < MAX_SAMPLES) {
        MP4Status status = read_next_sample(mp4_ctx, &sample);

        printf("\nReading sample %d...\n", sample_index);
        printf("├─ Status: %s\n", status == MP4_SUCCESS ? "Success" : 
                                 status == MP4_ERROR_EOF ? "End of File" :
                                 status == MP4_ERROR_IO ? "Read Error" :
                                 status == MP4_ERROR_MEMORY ? "Memory Error" : "Unknown Error");
        if (status == MP4_SUCCESS) {
            print_sample_info(&sample, sample_index);
        }
        sample_index++;

        if (status == MP4_ERROR_EOF) {
            printf("Reached end of file without finding SPS and PPS\n");
            h264_context_free(h264_ctx);
            mp4_close(mp4_ctx);
            return 1;
        }

        if (status != MP4_SUCCESS) {
            printf("Error reading sample: %s\n", 
                   status == MP4_ERROR_IO ? "I/O Error" :
                   status == MP4_ERROR_MEMORY ? "Memory Error" :
                   status == MP4_ERROR_FORMAT ? "Format Error" :
                   status == MP4_ERROR_INVALID_PARAM ? "Invalid Parameter" :
                   "Unknown Error");
            h264_context_free(h264_ctx);
            mp4_close(mp4_ctx);
            return 1;
        }

        if (!sample.data) {
            printf("Error: Sample data allocation failed\n");
            free_sample(&sample);
            h264_context_free(h264_ctx);
            mp4_close(mp4_ctx);
            return 1;
        }

        if (sample.track_type == TRACK_TYPE_VIDEO) {
            H264Status status = h264_parse_sample(h264_ctx, sample.data, sample.size);
            if (status != H264_SUCCESS) {
                printf("Error parsing H.264 sample\n");
                free_sample(&sample);
                h264_context_free(h264_ctx);
                mp4_close(mp4_ctx);
                return 1;
            }
            if (h264_ctx->sps && h264_ctx->pps) {
                // Found both SPS and PPS
                free_sample(&sample);
                break;
            }
        }
        free_sample(&sample);
    }

    if (sample_index >= MAX_SAMPLES) {
        printf("Error: Exceeded maximum sample limit without finding SPS and PPS\n");
        h264_context_free(h264_ctx);
        mp4_close(mp4_ctx);
        return 1;
    }

    // Reset MP4 context for second pass
    mp4_close(mp4_ctx);
    mp4_ctx = mp4_open(input_file);

    // Initialize decoder with H.264 parameters
    MP4Status mp4_status;
    VideoDecoder decoder;
    DecoderStatus status = init_decoder(&decoder, output_dir, 
                                      h264_ctx->sps, h264_ctx->sps_size,
                                      h264_ctx->pps, h264_ctx->pps_size);
    if (status != DECODER_SUCCESS) {
        printf("Failed to initialize decoder\n");
        h264_context_free(h264_ctx);
        mp4_close(mp4_ctx);
        return 1;
    }

    // Second pass: Process all samples
    sample_index = 0;
    int video_samples = 0;
    int audio_samples = 0;
    size_t total_bytes = 0;
    
    while (sample_index < MAX_SAMPLES) {
        mp4_status = read_next_sample(mp4_ctx, &sample);
        if (mp4_status == MP4_ERROR_EOF) {
            break;
        } else if (mp4_status != MP4_SUCCESS) {
            printf("Failed to read sample %d\n", sample_index);
            break;
        }

        if (!sample.data) {
            printf("Error: Sample data allocation failed\n");
            cleanup_decoder(&decoder);
            h264_context_free(h264_ctx);
            mp4_close(mp4_ctx);
            return 1;
        }

        // Print sample information
        print_sample_info(&sample, sample_index);

        // Update statistics
        total_bytes += sample.size;
        if (sample.track_type == TRACK_TYPE_VIDEO) {
            video_samples++;
            status = decode_frame(&decoder, sample.data, sample.size, sample.pts);
            if (status != DECODER_SUCCESS) {
                printf("Failed to decode video sample %d\n", sample_index);
                free_sample(&sample);
                break;
            }
        } else if (sample.track_type == TRACK_TYPE_AUDIO) {
            audio_samples++;
        }

        free_sample(&sample);
        sample_index++;
    }

    if (sample_index >= MAX_SAMPLES) {
        printf("Error: Exceeded maximum sample limit\n");
        cleanup_decoder(&decoder);
        h264_context_free(h264_ctx);
        mp4_close(mp4_ctx);
        return 1;
    }

    // Print final statistics
    printf("\nProcessing complete!\n");
    printf("Total samples processed: %d\n", sample_index);
    printf("├─ Video samples: %d\n", video_samples);
    printf("├─ Audio samples: %d\n", audio_samples);
    printf("└─ Total data processed: %.2f MB\n", (float)total_bytes / (1024*1024));

    // Cleanup
    cleanup_decoder(&decoder);
    h264_context_free(h264_ctx);
    mp4_close(mp4_ctx);
    return 0;
}
