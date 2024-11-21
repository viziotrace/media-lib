#ifndef LOG_H
#define LOG_H

#include <stdio.h>
#include <stdlib.h>

// Log levels
typedef enum
{
    LOG_LEVEL_DEBUG,
    LOG_LEVEL_INFO,
    LOG_LEVEL_WARNING,
    LOG_LEVEL_ERROR
} LogLevel;

// Main logging macro
#define LOG(level, fmt, ...)                            \
    do                                                  \
    {                                                   \
        if (getenv("MP4_DEBUG"))                        \
        {                                               \
            fprintf(stderr, "[%s] %s:%d: " fmt "\n",    \
                    log_level_to_string(level),         \
                    __func__, __LINE__, ##__VA_ARGS__); \
        }                                               \
    } while (0)

// Convenience macros for different log levels
#define DEBUG_LOG(fmt, ...) LOG(LOG_LEVEL_DEBUG, fmt, ##__VA_ARGS__)
#define INFO_LOG(fmt, ...) LOG(LOG_LEVEL_INFO, fmt, ##__VA_ARGS__)
#define WARN_LOG(fmt, ...) LOG(LOG_LEVEL_WARNING, fmt, ##__VA_ARGS__)
#define ERROR_LOG(fmt, ...) LOG(LOG_LEVEL_ERROR, fmt, ##__VA_ARGS__)

// Helper function to convert log level to string
const char *log_level_to_string(LogLevel level);

// Helper function for logging box types (FourCC codes)
#define LOG_BOX(level, box_type, offset)                                  \
    do                                                                    \
    {                                                                     \
        if (getenv("MP4_DEBUG"))                                          \
        {                                                                 \
            char fourcc[5];                                               \
            fourcc_to_string(box_type, fourcc);                           \
            fprintf(stderr, "[%s] %s:%d: Found '%s' box at offset %ld\n", \
                    log_level_to_string(level),                           \
                    __func__, __LINE__, fourcc, offset);                  \
        }                                                                 \
    } while (0)

#endif // LOG_H
