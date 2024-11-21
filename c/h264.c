#include "h264.h"
#include <string.h>
#include <stdio.h>

// Helper function to read a 32-bit big-endian integer
static uint32_t read_golomb(const uint8_t* data, size_t size, size_t* bit_offset) {
    uint32_t leading_zeros = 0;
    while (*bit_offset < size * 8) {
        if ((data[*bit_offset / 8] & (0x80 >> (*bit_offset % 8))) != 0)
            break;
        leading_zeros++;
        (*bit_offset)++;
    }
    
    if (*bit_offset + leading_zeros >= size * 8)
        return 0;
    
    uint32_t code = 1;
    for (uint32_t i = 0; i < leading_zeros; i++) {
        (*bit_offset)++;
        if (*bit_offset >= size * 8)
            return 0;
        code = (code << 1) | ((data[*bit_offset / 8] & (0x80 >> (*bit_offset % 8))) ? 1 : 0);
    }
    (*bit_offset)++;
    return code - 1;
}

// Parse SPS NAL unit to get width and height
static H264Status parse_sps(H264Context* ctx, const uint8_t* data, size_t size) {
    if (!ctx || !data || size < 4) {
        return H264_ERROR_INVALID_PARAM;
    }
    
    ctx->profile_idc = data[0];
    ctx->level_idc = data[2];
    
    size_t bit_offset = 24;  // Skip first 3 bytes
    
    // seq_parameter_set_id
    read_golomb(data, size, &bit_offset);
    
    if (ctx->profile_idc == 100 || ctx->profile_idc == 110 ||
        ctx->profile_idc == 122 || ctx->profile_idc == 244 ||
        ctx->profile_idc == 44  || ctx->profile_idc == 83  ||
        ctx->profile_idc == 86  || ctx->profile_idc == 118 ||
        ctx->profile_idc == 128 || ctx->profile_idc == 138) {
        
        int chroma_format_idc = read_golomb(data, size, &bit_offset);
        if (chroma_format_idc == 3) {
            bit_offset++;  // separate_colour_plane_flag
        }
        read_golomb(data, size, &bit_offset);  // bit_depth_luma_minus8
        read_golomb(data, size, &bit_offset);  // bit_depth_chroma_minus8
        bit_offset++;  // qpprime_y_zero_transform_bypass_flag
        
        // seq_scaling_matrix_present_flag
        int scaling_matrix_present = (data[bit_offset / 8] & (0x80 >> (bit_offset % 8))) != 0;
        bit_offset++;
        
        if (scaling_matrix_present) {
            // Skip scaling matrices
            for (int i = 0; i < ((chroma_format_idc != 3) ? 8 : 12); i++) {
                if ((data[bit_offset / 8] & (0x80 >> (bit_offset % 8))) != 0) {
                    bit_offset += 64;  // scaling_list_8x8
                }
                bit_offset++;
            }
        }
    }
    
    // log2_max_frame_num_minus4
    read_golomb(data, size, &bit_offset);
    
    // pic_order_cnt_type
    int poc_type = read_golomb(data, size, &bit_offset);
    if (poc_type == 0) {
        read_golomb(data, size, &bit_offset);  // log2_max_pic_order_cnt_lsb_minus4
    } else if (poc_type == 1) {
        bit_offset++;  // delta_pic_order_always_zero_flag
        read_golomb(data, size, &bit_offset);  // offset_for_non_ref_pic
        read_golomb(data, size, &bit_offset);  // offset_for_top_to_bottom_field
        int num_ref_frames_in_poc_cycle = read_golomb(data, size, &bit_offset);
        for (int i = 0; i < num_ref_frames_in_poc_cycle; i++) {
            read_golomb(data, size, &bit_offset);  // offset_for_ref_frame[i]
        }
    }
    
    read_golomb(data, size, &bit_offset);  // max_num_ref_frames
    bit_offset++;  // gaps_in_frame_num_value_allowed_flag
    
    // pic_width_in_mbs_minus1
    int width_in_mbs = read_golomb(data, size, &bit_offset) + 1;
    
    // pic_height_in_map_units_minus1
    int height_in_map_units = read_golomb(data, size, &bit_offset) + 1;
    
    // frame_mbs_only_flag
    int frame_mbs_only = (data[bit_offset / 8] & (0x80 >> (bit_offset % 8))) != 0;
    bit_offset++;
    
    if (!frame_mbs_only) {
        bit_offset++;  // mb_adaptive_frame_field_flag
    }
    
    ctx->width = width_in_mbs * 16;
    ctx->height = (2 - frame_mbs_only) * height_in_map_units * 16;
    
    printf("Parsed SPS: %dx%d (Profile: %d, Level: %d)\n",
           ctx->width, ctx->height, ctx->profile_idc, ctx->level_idc);
    
    return H264_SUCCESS;
}

H264Context* h264_context_create(void) {
    H264Context* ctx = calloc(1, sizeof(H264Context));
    return ctx;
}

void h264_context_free(H264Context* ctx) {
    if (ctx) {
        free(ctx->sps);
        free(ctx->pps);
        free(ctx);
    }
}

static H264Status find_nal_unit(const uint8_t* data, size_t size, size_t* start, size_t* end) {
    if (!data || !start || !end || size < 4) {
        return H264_ERROR_INVALID_PARAM;
    }

    // Check if this is length-prefixed (common in MP4)
    uint32_t length = (data[0] << 24) | (data[1] << 16) | (data[2] << 8) | data[3];
    if (length > 0 && length <= size - 4) {
        *start = 4;  // Skip the length prefix
        *end = 4 + length;
        return H264_SUCCESS;
    }

    // If not length-prefixed, look for start codes
    size_t i = 0;
    while (i + 3 < size) {
        if (data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1) {
            *start = i + 3;
            break;
        }
        if (i + 4 < size && data[i] == 0 && data[i + 1] == 0 && 
            data[i + 2] == 0 && data[i + 3] == 1) {
            *start = i + 4;
            break;
        }
        i++;
    }
    
    if (i + 3 >= size) return H264_ERROR_INVALID_DATA;
    
    // Find next start code or use remaining data
    i = *start;
    while (i + 3 < size) {
        if ((data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1) ||
            (i + 4 < size && data[i] == 0 && data[i + 1] == 0 && 
             data[i + 2] == 0 && data[i + 3] == 1)) {
            *end = i;
            return H264_SUCCESS;
        }
        i++;
    }
    
    *end = size;
    return H264_SUCCESS;
}

H264Status h264_find_nal_units(const uint8_t* data, size_t size, 
                              NALUnit** nal_units, size_t* nal_count) {
    if (!data || !nal_units || !nal_count || size < 4) {
        return H264_ERROR_INVALID_PARAM;
    }

    *nal_units = NULL;
    *nal_count = 0;
    size_t capacity = 16;
    
    *nal_units = malloc(capacity * sizeof(NALUnit));
    if (!*nal_units) return H264_ERROR_MEMORY;
    
    const uint8_t* current = data;
    size_t remaining = size;
    size_t start, end;
    H264Status status;
    
    printf("Starting NAL unit parsing (input size: %zu bytes)\n", size);
    printf("First few bytes: ");
    for (int i = 0; i < 8 && i < size; i++) {
        printf("%02x ", data[i]);
    }
    printf("\n");
    
    while (remaining > 4) {
        status = find_nal_unit(current, remaining, &start, &end);
        if (status != H264_SUCCESS) break;
        if (end <= start) break;

        if (*nal_count >= capacity) {
            capacity *= 2;
            NALUnit* new_units = realloc(*nal_units, capacity * sizeof(NALUnit));
            if (!new_units) {
                h264_free_nal_units(*nal_units, *nal_count);
                return H264_ERROR_MEMORY;
            }
            *nal_units = new_units;
        }
        
        NALUnit* unit = &(*nal_units)[*nal_count];
        unit->type = current[start] & 0x1F;
        unit->size = end - start;
        unit->data = malloc(unit->size);
        
        if (!unit->data) {
            h264_free_nal_units(*nal_units, *nal_count);
            return H264_ERROR_MEMORY;
        }
        
        memcpy(unit->data, current + start, unit->size);
        
        printf("Found NAL unit %zu at offset %zu: ", *nal_count, 
               (size_t)(current - data + start));
        h264_print_nal_unit(unit);
        
        (*nal_count)++;
        
        current += end;
        remaining = size - (current - data);
    }
    
    printf("Finished parsing: found %zu NAL units\n", *nal_count);
    
    return H264_SUCCESS;
}

H264Status h264_parse_sample(H264Context* ctx, const uint8_t* data, size_t size) {
    if (!ctx || !data || size < 4) {
        return H264_ERROR_INVALID_PARAM;
    }

    NALUnit* units;
    size_t count;
    H264Status status = h264_find_nal_units(data, size, &units, &count);
    
    if (status != H264_SUCCESS) {
        return status;
    }
    
    for (size_t i = 0; i < count; i++) {
        NALUnit* unit = &units[i];
        
        switch (unit->type) {
            case NAL_SPS:
                if (!ctx->sps || ctx->sps_size < unit->size) {
                    free(ctx->sps);
                    ctx->sps = malloc(unit->size);
                    if (!ctx->sps) {
                        h264_free_nal_units(units, count);
                        return H264_ERROR_MEMORY;
                    }
                }
                memcpy(ctx->sps, unit->data, unit->size);
                ctx->sps_size = unit->size;
                status = parse_sps(ctx, unit->data, unit->size);
                if (status != H264_SUCCESS) {
                    h264_free_nal_units(units, count);
                    return status;
                }
                break;
                
            case NAL_PPS:
                if (!ctx->pps || ctx->pps_size < unit->size) {
                    free(ctx->pps);
                    ctx->pps = malloc(unit->size);
                    if (!ctx->pps) {
                        h264_free_nal_units(units, count);
                        return H264_ERROR_MEMORY;
                    }
                }
                memcpy(ctx->pps, unit->data, unit->size);
                ctx->pps_size = unit->size;
                break;

            default:
                break;
        }
    }
    
    h264_free_nal_units(units, count);
    return H264_SUCCESS;
}

void h264_print_nal_unit(const NALUnit* nal) {
    static const char* nal_type_strings[] = {
        "Unspecified", "Slice", "DPA", "DPB", "DPC", "IDR", "SEI", "SPS", "PPS",
        "AUD", "End Sequence", "End Stream", "Filler", "SPS Ext", "Prefix",
        "Subset SPS", "Depth PS", "Reserved 17", "Reserved 18", "Aux Slice",
        "Reserved 20", "Reserved 21", "Reserved 22", "Reserved 23"
    };
    
    if (!nal) return;
    
    printf("NAL Unit Type: %s (%d), Size: %zu bytes\n",
           nal_type_strings[nal->type], nal->type, nal->size);
}

void h264_free_nal_units(NALUnit* units, size_t count) {
    if (units) {
        for (size_t i = 0; i < count; i++) {
            free(units[i].data);
        }
        free(units);
    }
}
