#include <stdlib.h>
#include <string.h>
#include <arpa/inet.h> // For ntohl, ntohs
#include "mp4.h"
#include <stdio.h>

// Define box type constants
#define BOX_TYPE_MOOV 0x6D6F6F76 // 'moov'
#define BOX_TYPE_TRAK 0x7472616B // 'trak'
#define BOX_TYPE_MDIA 0x6D646961 // 'mdia'
#define BOX_TYPE_MINF 0x6D696E66 // 'minf'
#define BOX_TYPE_STBL 0x7374626C // 'stbl'
#define BOX_TYPE_STSZ 0x7374737A // 'stsz'
#define BOX_TYPE_STCO 0x7374636F // 'stco'
#define BOX_TYPE_HDLR 0x68646C72 // 'hdlr'
#define BOX_TYPE_AVCC 0x61766343 // 'avcC'
#define BOX_TYPE_STSD 0x73747364 // 'stsd'

// Define handler type constants
#define HANDLER_TYPE_VIDEO 0x76696465 // 'vide'
#define HANDLER_TYPE_AUDIO 0x736F756E // 'soun'

// Add these function declarations at the top with the other static functions
static int parse_stsz_box(FILE *file, MP4Context *ctx, long offset);
static int parse_stco_box(FILE *file, MP4Context *ctx, long offset);
static int parse_avcC_box(FILE *file, MP4Context *ctx, long offset);
static int parse_video_info(FILE *file, MP4Context *ctx, long stsd_offset);

// Add these at the top after includes
#define DEBUG_LOG(fmt, ...)                                \
    do                                                     \
    {                                                      \
        if (getenv("MP4_DEBUG"))                           \
        {                                                  \
            fprintf(stderr, "[MP4Debug] %s:%d: " fmt "\n", \
                    __func__, __LINE__, ##__VA_ARGS__);    \
        }                                                  \
    } while (0)

#define DEBUG_BOX(box_type, offset)                                             \
    do                                                                          \
    {                                                                           \
        if (getenv("MP4_DEBUG"))                                                \
        {                                                                       \
            char fourcc[5];                                                     \
            fourcc_to_string(box_type, fourcc);                                 \
            fprintf(stderr, "[MP4Debug] %s:%d: Found '%s' box at offset %ld\n", \
                    __func__, __LINE__, fourcc, offset);                        \
        }                                                                       \
    } while (0)

// Helper function to read a 32-bit big-endian integer
// static uint32_t read_uint32(FILE* fp) {
//     uint8_t buf[4];
//     if (fread(buf, 1, 4, fp) != 4) return 0;
//     return (buf[0] << 24) | (buf[1] << 16) | (buf[2] << 8) | buf[3];
// }

void fourcc_to_string(uint32_t fourcc, char *str)
{
    str[0] = (fourcc >> 24) & 0xFF;
    str[1] = (fourcc >> 16) & 0xFF;
    str[2] = (fourcc >> 8) & 0xFF;
    str[3] = fourcc & 0xFF;
    str[4] = '\0';
}

// Parse an MP4 box (atom)
static int read_box_header(FILE *file, MP4BoxHeader *header)
{
    uint32_t tmp;
    // Check if we can read 8 bytes (4 for size + 4 for type)
    long current_pos = ftell(file);
    fseek(file, 0, SEEK_END);
    long file_size = ftell(file);
    fseek(file, current_pos, SEEK_SET);

    if (current_pos + 8 > file_size)
    {
        DEBUG_LOG("Not enough bytes left to read box header at offset %ld", current_pos);
        return 0;
    }

    if (fread(&tmp, 1, 4, file) != 4)
        return 0;
    header->size = ntohl(tmp); // Convert from big-endian
    if (fread(&tmp, 1, 4, file) != 4)
        return 0;
    header->type = ntohl(tmp);

    // Validate size
    if (header->size < 8 && header->size != 1)
    { // Box size must be at least 8 bytes
        DEBUG_LOG("Invalid box size %u at offset %ld", header->size, current_pos);
        return 0;
    }

    char fourcc[5];
    fourcc_to_string(header->type, fourcc);
    DEBUG_LOG("Box header: type='%s' size=%u at offset %ld", fourcc, header->size, current_pos);

    if (header->size == 1)
    { // 64-bit size
        if (current_pos + 16 > file_size)
        { // Check if we can read the large size
            DEBUG_LOG("Not enough bytes left to read large size at offset %ld", current_pos);
            return 0;
        }
        if (fread(&header->largesize, 1, 8, file) != 8)
            return 0;
        header->largesize = ((uint64_t)ntohl(header->largesize >> 32) << 32) |
                            ntohl((uint32_t)header->largesize);
        if (header->largesize < 16)
        { // Large size box must be at least 16 bytes
            DEBUG_LOG("Invalid large size %llu at offset %ld",
                      (unsigned long long)header->largesize, current_pos);
            return 0;
        }
        DEBUG_LOG("Large size box: %llu bytes", (unsigned long long)header->largesize);
    }
    else
    {
        header->largesize = header->size;
    }
    return 1;
}

// Helper function to parse track type from hdlr box
static TrackType parse_hdlr_box(FILE *file, long offset)
{
    fseek(file, offset + 16, SEEK_SET);

    uint32_t handler_type;
    if (fread(&handler_type, sizeof(uint32_t), 1, file) != 1)
    {
        return TRACK_TYPE_UNKNOWN;
    }
    handler_type = ntohl(handler_type);

    if (handler_type == HANDLER_TYPE_VIDEO)
    {
        return TRACK_TYPE_VIDEO;
    }
    else if (handler_type == HANDLER_TYPE_AUDIO)
    {
        return TRACK_TYPE_AUDIO;
    }

    return TRACK_TYPE_UNKNOWN;
}

// Add these implementations before mp4_open

// Parse sample size box
static int parse_stsz_box(FILE *file, MP4Context *ctx, long offset)
{
    fseek(file, offset + 8, SEEK_SET); // Skip box header
    fseek(file, 4, SEEK_CUR);          // Skip version and flags

    uint32_t sample_size;
    if (fread(&sample_size, sizeof(uint32_t), 1, file) != 1)
        return -1;
    sample_size = ntohl(sample_size);

    if (fread(&ctx->sample_count, sizeof(uint32_t), 1, file) != 1)
        return -1;
    ctx->sample_count = ntohl(ctx->sample_count);

    DEBUG_LOG("Sample count: %u, default sample size: %u",
              ctx->sample_count, sample_size);

    ctx->sample_sizes = malloc(ctx->sample_count * sizeof(uint32_t));
    if (!ctx->sample_sizes)
        return -1;

    if (sample_size == 0)
    {
        DEBUG_LOG("Variable sample sizes detected");
        // Variable sample sizes
        for (uint32_t i = 0; i < ctx->sample_count; i++)
        {
            if (fread(&ctx->sample_sizes[i], sizeof(uint32_t), 1, file) != 1)
                return -1;
            ctx->sample_sizes[i] = ntohl(ctx->sample_sizes[i]);
        }
    }
    else
    {
        DEBUG_LOG("Fixed sample size: %u", sample_size);
        // Fixed sample size
        for (uint32_t i = 0; i < ctx->sample_count; i++)
        {
            ctx->sample_sizes[i] = sample_size;
        }
    }

    return 0;
}

// Parse chunk offset box
static int parse_stco_box(FILE *file, MP4Context *ctx, long offset)
{
    fseek(file, offset + 8, SEEK_SET); // Skip box header
    fseek(file, 4, SEEK_CUR);          // Skip version and flags

    uint32_t entry_count;
    if (fread(&entry_count, sizeof(uint32_t), 1, file) != 1)
        return -1;
    entry_count = ntohl(entry_count);

    ctx->sample_offsets = malloc(entry_count * sizeof(uint64_t));
    if (!ctx->sample_offsets)
        return -1;

    for (uint32_t i = 0; i < entry_count; i++)
    {
        uint32_t chunk_offset;
        if (fread(&chunk_offset, sizeof(uint32_t), 1, file) != 1)
            return -1;
        ctx->sample_offsets[i] = ntohl(chunk_offset);
    }

    return 0;
}

static int parse_avcC_box(FILE *file, MP4Context *ctx, long offset)
{
    DEBUG_LOG("Parsing avcC box at offset %ld", offset);

    fseek(file, offset + 8, SEEK_SET); // Skip box header

    // Read configuration version
    uint8_t version;
    if (fread(&version, 1, 1, file) != 1)
        return -1;
    DEBUG_LOG("avcC version: %u", version);

    // Read profile, compatibility, and level
    uint8_t profile, compatibility, level;
    if (fread(&profile, 1, 1, file) != 1)
        return -1;
    if (fread(&compatibility, 1, 1, file) != 1)
        return -1;
    if (fread(&level, 1, 1, file) != 1)
        return -1;

    DEBUG_LOG("H.264 Profile: %u, Compatibility: %u, Level: %u",
              profile, compatibility, level);

    // Skip length size minus one
    uint8_t length_size;
    if (fread(&length_size, 1, 1, file) != 1)
        return -1;
    length_size = (length_size & 0x03) + 1;
    DEBUG_LOG("NAL length size: %u bytes", length_size);

    // Read number of SPS
    uint8_t num_sps;
    if (fread(&num_sps, 1, 1, file) != 1)
        return -1;
    num_sps &= 0x1F; // Lower 5 bits
    DEBUG_LOG("Number of SPS: %u", num_sps);

    if (num_sps > 0)
    {
        // Read SPS length
        uint16_t sps_size;
        if (fread(&sps_size, 2, 1, file) != 1)
            return -1;
        sps_size = ntohs(sps_size);
        DEBUG_LOG("SPS size: %u bytes", sps_size);

        // Validate SPS size
        if (sps_size == 0 || sps_size > 1024)
        { // reasonable maximum size
            DEBUG_LOG("Invalid SPS size: %u", sps_size);
            return -1;
        }

        // Allocate and read SPS
        ctx->h264_params.sps = malloc(sps_size);
        if (!ctx->h264_params.sps)
            return -1;

        if (fread(ctx->h264_params.sps, 1, sps_size, file) != sps_size)
        {
            DEBUG_LOG("Failed to read SPS data");
            free(ctx->h264_params.sps);
            ctx->h264_params.sps = NULL;
            return -1;
        }
        ctx->h264_params.sps_size = sps_size;
    }

    // Read number of PPS
    uint8_t num_pps;
    if (fread(&num_pps, 1, 1, file) != 1)
        return -1;
    DEBUG_LOG("Number of PPS: %u", num_pps);

    if (num_pps > 0)
    {
        // Read PPS length
        uint16_t pps_size;
        if (fread(&pps_size, 2, 1, file) != 1)
            return -1;
        pps_size = ntohs(pps_size);
        DEBUG_LOG("PPS size: %u bytes", pps_size);

        // Validate PPS size
        if (pps_size == 0 || pps_size > 1024)
        { // reasonable maximum size
            DEBUG_LOG("Invalid PPS size: %u", pps_size);
            return -1;
        }

        // Allocate and read PPS
        ctx->h264_params.pps = malloc(pps_size);
        if (!ctx->h264_params.pps)
            return -1;

        if (fread(ctx->h264_params.pps, 1, pps_size, file) != pps_size)
        {
            DEBUG_LOG("Failed to read PPS data");
            free(ctx->h264_params.pps);
            ctx->h264_params.pps = NULL;
            return -1;
        }
        ctx->h264_params.pps_size = pps_size;
    }

    DEBUG_LOG("Successfully parsed avcC box: SPS size=%zu, PPS size=%zu",
              ctx->h264_params.sps_size,
              ctx->h264_params.pps_size);

    return 0;
}

static int parse_video_info(FILE *file, MP4Context *ctx, long stsd_offset)
{
    fseek(file, stsd_offset + 8, SEEK_SET); // Skip stsd box header
    fseek(file, 4, SEEK_CUR);               // Skip version and flags

    uint32_t entry_count;
    if (fread(&entry_count, 4, 1, file) != 1)
        return -1;
    entry_count = ntohl(entry_count);

    DEBUG_LOG("STSD entry count: %u", entry_count);

    if (entry_count > 0)
    {
        // Read sample description box size and type
        uint32_t size, type;
        if (fread(&size, 4, 1, file) != 1)
            return -1;
        if (fread(&type, 4, 1, file) != 1)
            return -1;
        size = ntohl(size);
        type = ntohl(type);

        char fourcc[5];
        fourcc_to_string(type, fourcc);
        DEBUG_LOG("Sample description box: type='%s' size=%u", fourcc, size);

        if (size < 78)
        { // Minimum size for valid avc1 box
            DEBUG_LOG("Invalid avc1 box size: %u", size);
            return -1;
        }

        // Skip reserved bytes
        fseek(file, 6, SEEK_CUR);

        // Read data reference index
        uint16_t data_ref_idx;
        if (fread(&data_ref_idx, 2, 1, file) != 1)
            return -1;

        // Skip pre-defined and reserved
        fseek(file, 16, SEEK_CUR);

        // Read width and height
        uint16_t width, height;
        if (fread(&width, 2, 1, file) != 1)
            return -1;
        if (fread(&height, 2, 1, file) != 1)
            return -1;

        ctx->h264_params.width = ntohs(width);
        ctx->h264_params.height = ntohs(height);

        DEBUG_LOG("Video dimensions: %dx%d",
                  ctx->h264_params.width,
                  ctx->h264_params.height);

        // Skip remaining fixed fields
        fseek(file, 50, SEEK_CUR);

        // Now we should be at the avcC box
        MP4BoxHeader avcC_header;
        if (!read_box_header(file, &avcC_header))
        {
            DEBUG_LOG("Failed to read avcC box header");
            return -1;
        }

        if (avcC_header.type != BOX_TYPE_AVCC)
        {
            char type_str[5];
            fourcc_to_string(avcC_header.type, type_str);
            DEBUG_LOG("Expected avcC box, found: %s", type_str);
            return -1;
        }

        // Parse avcC box at current position
        long avcC_offset = ftell(file) - 8; // Go back to start of avcC box
        if (parse_avcC_box(file, ctx, avcC_offset) != 0)
        {
            DEBUG_LOG("Failed to parse avcC box");
            return -1;
        }
    }

    return 0;
}

// Add these new implementations
static MP4Box *create_box(uint32_t type, uint64_t size, long offset, MP4Box *parent)
{
    MP4Box *box = (MP4Box *)malloc(sizeof(MP4Box));
    if (!box)
        return NULL;

    box->type = type;
    box->size = size;
    box->offset = offset;
    box->parent = parent;
    box->first_child = NULL;
    box->next_sibling = NULL;

    return box;
}

MP4Box *create_box_tree(FILE *file, long start_offset, long end_offset, MP4Box *parent)
{
    MP4Box *first_box = NULL;
    MP4Box *current_box = NULL;
    long current_offset = start_offset;

    // Validate offsets
    fseek(file, 0, SEEK_END);
    long file_size = ftell(file);
    if (start_offset < 0 || end_offset > file_size || start_offset >= end_offset)
    {
        DEBUG_LOG("Invalid offset range: start=%ld, end=%ld, file_size=%ld",
                  start_offset, end_offset, file_size);
        return NULL;
    }

    int max_boxes = 1000; // Reasonable limit to prevent infinite loops
    int box_count = 0;

    while (current_offset < end_offset && box_count < max_boxes)
    {
        MP4BoxHeader header;

        fseek(file, current_offset, SEEK_SET);
        if (!read_box_header(file, &header))
        {
            DEBUG_LOG("Failed to read box header at offset %ld", current_offset);
            break;
        }

        // Validate box size
        if (header.largesize < 8 || current_offset + header.largesize > end_offset)
        {
            DEBUG_LOG("Invalid box size: %llu at offset %ld (end_offset=%ld)",
                      (unsigned long long)header.largesize, current_offset, end_offset);
            break;
        }

        MP4Box *new_box = create_box(header.type, header.largesize, current_offset, parent);
        if (!new_box)
            break;

        // Link the box into the tree
        if (!first_box)
        {
            first_box = new_box;
        }
        else
        {
            current_box->next_sibling = new_box;
        }
        current_box = new_box;

        // Parse children if this is a container box
        long box_end = current_offset + header.largesize;
        long children_start = current_offset + (header.size == 1 ? 16 : 8);

        // List of known container boxes
        switch (header.type)
        {
        case BOX_TYPE_MOOV:
        case BOX_TYPE_TRAK:
        case BOX_TYPE_MDIA:
        case BOX_TYPE_MINF:
        case BOX_TYPE_STBL:
        case BOX_TYPE_STSD:
            if (children_start < box_end && children_start > current_offset)
            {
                new_box->first_child = create_box_tree(file, children_start, box_end, new_box);
            }
            break;
        }

        current_offset += header.largesize;
        box_count++;
    }

    if (box_count >= max_boxes)
    {
        DEBUG_LOG("Reached maximum box count limit at offset %ld", current_offset);
    }

    return first_box;
}

void free_box_tree(MP4Box *box)
{
    if (!box)
        return;

    // Free all siblings
    free_box_tree(box->next_sibling);

    // Free all children
    free_box_tree(box->first_child);

    // Free this box
    free(box);
}

MP4Box *find_box_by_type(MP4Box *root, uint32_t type)
{
    if (!root)
        return NULL;

    // Check this box
    if (root->type == type)
        return root;

    // Check children
    MP4Box *found = find_box_by_type(root->first_child, type);
    if (found)
        return found;

    // Check siblings
    return find_box_by_type(root->next_sibling, type);
}

MP4Box *find_next_box_by_type(MP4Box *current, uint32_t type)
{
    if (!current)
        return NULL;

    // Start with siblings
    MP4Box *sibling = current->next_sibling;
    while (sibling)
    {
        if (sibling->type == type)
            return sibling;
        MP4Box *found = find_box_by_type(sibling->first_child, type);
        if (found)
            return found;
        sibling = sibling->next_sibling;
    }

    // If no siblings have it, try parent's next sibling
    if (current->parent)
    {
        return find_next_box_by_type(current->parent, type);
    }

    return NULL;
}

// Update mp4_open to use these functions
MP4Context *mp4_open(const char *filename)
{
    DEBUG_LOG("Opening file: %s", filename);

    FILE *file = fopen(filename, "rb");
    if (!file)
    {
        DEBUG_LOG("Failed to open file: %s", filename);
        return NULL;
    }

    MP4Context *ctx = (MP4Context *)malloc(sizeof(MP4Context));
    if (!ctx)
    {
        fclose(file);
        return NULL;
    }
    memset(ctx, 0, sizeof(MP4Context));
    ctx->file = file;

    // Get file size
    fseek(file, 0, SEEK_END);
    ctx->file_size = ftell(file);
    fseek(file, 0, SEEK_SET);

    // Create box tree
    MP4Box *root = create_box_tree(file, 0, ctx->file_size, NULL);
    if (!root)
    {
        DEBUG_LOG("Failed to create box tree");
        goto error;
    }

    // Find required boxes
    MP4Box *moov = find_box_by_type(root, BOX_TYPE_MOOV);
    if (!moov)
    {
        DEBUG_LOG("Failed to find moov box");
        goto error;
    }

    // Parse all tracks
    MP4Box *trak = find_box_by_type(moov, BOX_TYPE_TRAK);
    while (trak)
    {
        // Parse track info
        MP4Box *hdlr = find_box_by_type(trak, BOX_TYPE_HDLR);
        if (hdlr)
        {
            ctx->track_type = parse_hdlr_box(file, hdlr->offset);
            if (ctx->track_type == TRACK_TYPE_VIDEO)
            {
                // Parse video specific boxes
                MP4Box *stsd = find_box_by_type(trak, BOX_TYPE_STSD);
                if (stsd && parse_video_info(file, ctx, stsd->offset) != 0)
                {
                    DEBUG_LOG("Failed to parse video info");
                    goto error;
                }

                // Parse sample information
                MP4Box *stbl = find_box_by_type(trak, BOX_TYPE_STBL);
                if (stbl)
                {
                    MP4Box *stsz = find_box_by_type(stbl, BOX_TYPE_STSZ);
                    if (stsz && parse_stsz_box(file, ctx, stsz->offset) != 0)
                    {
                        DEBUG_LOG("Failed to parse stsz box");
                        goto error;
                    }

                    MP4Box *stco = find_box_by_type(stbl, BOX_TYPE_STCO);
                    if (stco && parse_stco_box(file, ctx, stco->offset) != 0)
                    {
                        DEBUG_LOG("Failed to parse stco box");
                        goto error;
                    }
                }
            }
        }

        // Get next track
        trak = find_next_box_by_type(trak, BOX_TYPE_TRAK);
    }

    // Clean up
    free_box_tree(root);
    return ctx;

error:
    if (root)
        free_box_tree(root);
    mp4_close(ctx);
    return NULL;
}

// Function to close MP4 file and free resources
void mp4_close(MP4Context *ctx)
{
    if (ctx)
    {
        if (ctx->file)
            fclose(ctx->file);
        if (ctx->sample_offsets)
            free(ctx->sample_offsets);
        if (ctx->sample_sizes)
            free(ctx->sample_sizes);
        if (ctx->h264_params.sps)
            free(ctx->h264_params.sps);
        if (ctx->h264_params.pps)
            free(ctx->h264_params.pps);
        free(ctx);
    }
}

// Function to read next sample from MP4 file
MP4Status read_next_sample(MP4Context *ctx, MP4Sample *sample)
{
    if (!ctx || !sample)
    {
        return MP4_ERROR_INVALID_PARAM;
    }

    if (ctx->current_sample >= ctx->sample_count)
    {
        return MP4_ERROR_EOF;
    }

    // Validate array bounds
    if (ctx->current_sample >= ctx->sample_count ||
        !ctx->sample_offsets || !ctx->sample_sizes)
    {
        return MP4_ERROR_INVALID_PARAM;
    }

    // Get sample data
    uint64_t offset = ctx->sample_offsets[ctx->current_sample];
    uint32_t size = ctx->sample_sizes[ctx->current_sample];

    DEBUG_LOG("Reading sample %u/%u (size: %u, offset: %llu)",
              ctx->current_sample + 1, ctx->sample_count,
              size, (unsigned long long)offset);

    sample->data = (uint8_t *)malloc(size);
    if (!sample->data)
    {
        return MP4_ERROR_MEMORY;
    }

    // Read sample data
    if (fseek(ctx->file, offset, SEEK_SET) != 0)
    {
        free(sample->data);
        return MP4_ERROR_IO;
    }

    size_t bytes_read = fread(sample->data, 1, size, ctx->file);
    if (bytes_read != size)
    {
        DEBUG_LOG("Failed to read sample: expected %u bytes, got %zu",
                  size, bytes_read);
        free(sample->data);
        return feof(ctx->file) ? MP4_ERROR_EOF : MP4_ERROR_IO;
    }

    // Fill in sample metadata
    sample->size = size;
    sample->pts = CMTimeMake(ctx->current_sample, ctx->timescale);
    sample->track_id = 1; // For now, assuming single track
    sample->track_type = ctx->track_type;
    sample->timescale = ctx->timescale;

    ctx->current_sample++;
    DEBUG_LOG("Successfully read sample %u", ctx->current_sample);
    return MP4_SUCCESS;
}

void free_sample(MP4Sample *sample)
{
    if (sample)
    {
        free(sample->data);
        sample->data = NULL;
        sample->size = 0;
    }
}
