use std::ptr::null;
use std::{path::Path, ptr::null_mut};

use ffmpeg_next::ffi::av_hwframe_transfer_data;
use ffmpeg_next::Codec;
use ffmpeg_next::{
    codec::{self, context::Context},
    ffi::{
        av_buffer_unref, av_hwdevice_ctx_create, avcodec_get_hw_config, AVBufferRef, AVHWDeviceType,
    },
};
use media_types::MediaLibError;

// The error status sent when the decoder needs more data
const NEED_MORE_DATA: i32 = 35;

// We in fact do use this function but it's passed into a c style callback.
#[allow(unused)]
extern "C" fn get_hw_format(
    ctx: *mut ffmpeg_next::ffi::AVCodecContext,
    params: *const ffmpeg_next::ffi::AVPixelFormat,
) -> ffmpeg_next::ffi::AVPixelFormat {
    // To avoid global state, we'll use the decoder context's opaque field to store the state
    let state = unsafe { (*ctx).opaque } as *const DecoderContextState;
    let pix_fmt = unsafe { (*state).pix_fmt };

    let mut i = 0;
    loop {
        let format = unsafe { *params.offset(i) };
        if format == ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            break;
        }
        if format == pix_fmt {
            return format;
        }
        i += 1;
    }
    ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE
}

#[repr(C)]
struct DecoderContextState {
    device_type: AVHWDeviceType,
    pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
}

pub struct HardwareAcceleratedVideoDecoder {
    pub hardware_accelerated: bool,
    pub eof_sent: bool,
    video_decoder: codec::decoder::Video,
    codec: Codec,
    ictx: ffmpeg_next::format::context::Input,
    hw_device_ctx: *mut AVBufferRef,
    pub pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
    pub device_type: AVHWDeviceType,
    pub video_stream_index: usize,
}

impl HardwareAcceleratedVideoDecoder {
    pub unsafe fn new(input_path: &Path) -> Result<Self, MediaLibError> {
        // Input stream for the file.
        let ictx = ffmpeg_next::format::input(input_path)
            .map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;

        // Find the video stream
        let input = ictx
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or_else(|| MediaLibError::FFmpegError("No video stream found".into()))?;

        let video_stream_index = input.index();

        let mut decoder_context = Context::from_parameters(input.parameters())
            .map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;

        let decoder_context_ptr = decoder_context.as_mut_ptr();

        // The object underlying decoder is just the decoder context
        let decoder = decoder_context.decoder();

        let video_decoder = decoder
            .video()
            .map_err(|e| MediaLibError::FFmpegError(e.to_string().into()))?;

        // Now that we have the decoder context, we can try to initialize hardware acceleration
        let codec = video_decoder
            .codec()
            .ok_or(MediaLibError::FFmpegError("Failed to find codec".into()))?;

        let mut i = 0;
        let mut hardware_accelerated = false;
        let mut hw_pixel_format = ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE;
        let mut device_type = AVHWDeviceType::AV_HWDEVICE_TYPE_NONE;
        let mut hw_device_ctx: *mut AVBufferRef = std::ptr::null_mut();

        loop {
            let config = avcodec_get_hw_config(codec.as_ptr(), i);
            if config.is_null() {
                break;
            }

            let hw_config = &*config;
            // Check if this config uses a hardware device context
            if (hw_config.methods
                & ffmpeg_next::ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32)
                != 0
            {
                // This is a valid hardware acceleration method
                hw_pixel_format = hw_config.pix_fmt;
                device_type = hw_config.device_type;

                let state = DecoderContextState {
                    device_type,
                    pix_fmt: hw_pixel_format,
                };

                // We then need to inject the state into the decoder context
                (*decoder_context_ptr).opaque =
                    Box::into_raw(Box::new(state)) as *mut std::ffi::c_void;
                (*decoder_context_ptr).get_format = Some(get_hw_format);

                // try to initialize the hardware acceleration
                if av_hwdevice_ctx_create(&mut hw_device_ctx, device_type, null(), null_mut(), 0)
                    < 0
                {
                    // This particular hardware acceleration type isn't supported
                    log::error!(
                        "Hardware acceleration type {:?} isn't supported",
                        device_type
                    );
                    // Revert our shared state to the default
                    hw_pixel_format = ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NONE;
                    continue;
                }

                // Now we need to set the hw_device_ctx in the decoder context
                (*decoder_context_ptr).hw_device_ctx = hw_device_ctx;

                log::info!("Hardware acceleration type {:?} is supported", device_type);
                println!("Hardware acceleration type {:?} is supported", device_type);
                hardware_accelerated = true;
                break;
            }

            i += 1;
        }

        Ok(HardwareAcceleratedVideoDecoder {
            codec,
            ictx,
            video_decoder,
            hw_device_ctx,
            hardware_accelerated,
            pix_fmt: hw_pixel_format,
            device_type,
            eof_sent: false,
            video_stream_index,
        })
    }

    pub fn get_frame(&mut self) -> Option<Result<ffmpeg_next::frame::Video, MediaLibError>> {
        while !self.eof_sent {
            match self.ictx.packets().next() {
                Some((stream, packet)) => {
                    if stream.index() == self.video_stream_index {
                        if let Err(e) = self.video_decoder.send_packet(&packet) {
                            return Some(Err(MediaLibError::FFmpegError(e.to_string().into())));
                        }
                        break; // Exit the loop after sending a video packet
                    }
                    // Continue looping if it's not a video packet
                }
                None => {
                    if let Err(e) = self.video_decoder.send_eof() {
                        return Some(Err(MediaLibError::FFmpegError(e.to_string().into())));
                    }
                    self.eof_sent = true;
                    break; // Exit the loop after sending EOF
                }
            }
        }

        let mut decoded = ffmpeg_next::frame::Video::empty();
        match self.video_decoder.receive_frame(&mut decoded) {
            Ok(_) => {
                let frame_format = unsafe { *decoded.as_ptr() }.format;
                let is_key = decoded.is_key();

                if !is_key {
                    return self.get_frame();
                }

                if self.hardware_accelerated && frame_format == self.pix_fmt as i32 {
                    // okay now we need to transfer the frame to a software frame
                    let mut sw_frame = ffmpeg_next::frame::Video::empty();
                    unsafe {
                        let res =
                            av_hwframe_transfer_data(sw_frame.as_mut_ptr(), decoded.as_ptr(), 0);
                        if res < 0 {
                            return Some(Err(MediaLibError::FFmpegError(
                                format!("Failed to transfer frame: {}", res).into(),
                            )));
                        }

                        return Some(Ok(sw_frame));
                    };
                }

                // if we're hardware accelerated, we need to convert the frame to a software frame
                // if is_key {
                Some(Ok(decoded))
                // } else {
                //     println!("nutbar get frame");
                //     self.get_frame()
                // }
            }
            Err(ffmpeg_next::Error::Other { errno }) => {
                if errno == NEED_MORE_DATA {
                    println!("nutbar need more data");
                    self.get_frame()
                } else {
                    Some(Err(MediaLibError::FFmpegError(errno.to_string().into())))
                }
            }
            Err(ffmpeg_next::Error::Eof) => None,
            Err(e) => Some(Err(MediaLibError::FFmpegError(e.to_string().into()))),
        }
    }

    pub fn width(&self) -> u32 {
        self.video_decoder.width()
    }

    pub fn height(&self) -> u32 {
        self.video_decoder.height()
    }

    pub fn format(&self) -> ffmpeg_next::format::Pixel {
        self.video_decoder.format()
    }
}
