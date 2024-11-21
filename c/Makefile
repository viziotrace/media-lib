CC = clang
CFLAGS = -Wall -Werror -g
FRAMEWORKS = -framework VideoToolbox \
            -framework CoreMedia \
            -framework CoreFoundation \
            -framework CoreGraphics \
            -framework CoreVideo \
            -framework ImageIO \
            -framework Accelerate

TARGET = decode-videotoolbox
SRCS = decode-videotoolbox.c main.c mp4.c
OBJS = $(SRCS:.c=.o)

all: $(TARGET)

$(TARGET): $(OBJS)
	$(CC) $(OBJS) $(FRAMEWORKS) -o $(TARGET)

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

clean:
	rm -f $(TARGET) $(OBJS)

.PHONY: all clean
