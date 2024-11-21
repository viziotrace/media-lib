#ifndef COMMON_H
#define COMMON_H

// Status codes for our decoder
typedef enum {
    DECODER_SUCCESS = 0,
    DECODER_ERROR_INIT = -1,
    DECODER_ERROR_DECODE = -2,
    DECODER_ERROR_OUTPUT = -3,
    DECODER_ERROR_EOF = -4,
    DECODER_ERROR_READ = -5,
    DECODER_ERROR_MEMORY = -6
} DecoderStatus;

#endif // COMMON_H 
