use super::{
    filter::FilterGraph,
    hardware::HardwareContext,
    types::{DecodedVideoFrame, VideoSize},
};
use ffmpeg_next::codec::Context;
use ffmpeg_next::ffi::{
    av_buffer_ref, avcodec_get_hw_config, AVCodecContext, AVCodecHWConfig, AVPixelFormat,
};
use ffmpeg_next::{codec, Packet};
use media_types::MediaLibError;
use stabby::string::String as StabbyString;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;

const NEED_MORE_DATA: i32 = 35;

#[repr(C)]
struct DecoderContextState {
    device_type: ffmpeg_next::ffi::AVHWDeviceType,
    pix_fmt: AVPixelFormat,
}

extern "C" fn get_hw_format(
    ctx: *mut AVCodecContext,
    pix_fmts: *const AVPixelFormat,
) -> AVPixelFormat {
    let state = unsafe { (*ctx).opaque } as *const DecoderContextState;
    let pix_fmt = unsafe { (*state).pix_fmt };

    let mut i = 0;
    unsafe {
        loop {
            let format = *pix_fmts.offset(i);
            if format == AVPixelFormat::AV_PIX_FMT_NONE {
                break;
            }
            if format == pix_fmt {
                return format;
            }
            i += 1;
        }
    }
    AVPixelFormat::AV_PIX_FMT_NONE
}

/// Decodes video frames using hardware acceleration when available.
/// Uses FFmpeg's hw_device_ctx for hardware frame management and filter graphs for format conversion.
///
/// Hardware context initialization happens during decoder creation, with format negotiation
/// handled via the get_hw_format callback. Frame processing is deferred until the first
/// frame is received to allow for proper stream info detection.
pub struct HardwareAcceleratedVideoDecoder {
    hardware_context: Option<Arc<HardwareContext>>,
    filter_graph: Option<FilterGraph>,
    decoder: codec::decoder::Video,
    input_context: ffmpeg_next::format::context::Input,
    video_stream_index: usize,
    target_size: VideoSize,
    eof_sent: bool,
}

impl HardwareAcceleratedVideoDecoder {
    /// Attempts hardware decoder initialization, falling back to software if unavailable.
    /// Hardware support is detected by checking codec hw_configs and attempting device creation.
    pub fn new(input_path: &Path, target_size: VideoSize) -> Result<Self, MediaLibError> {
        // Open input context
        let input_context = ffmpeg_next::format::input(input_path)
            .map_err(|e| MediaLibError::FFmpegError(StabbyString::from(e.to_string())))?;

        // Find best video stream
        let input = input_context
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or_else(|| {
                MediaLibError::FFmpegError(StabbyString::from("No video stream found"))
            })?;

        let video_stream_index = input.index();

        // Create decoder context
        let mut decoder_context = Context::from_parameters(input.parameters())
            .map_err(|e| MediaLibError::FFmpegError(StabbyString::from(e.to_string())))?;

        let decoder_context_ptr = unsafe { decoder_context.as_mut_ptr() };

        // Get decoder first
        let decoder = decoder_context
            .decoder()
            .video()
            .map_err(|e| MediaLibError::FFmpegError(StabbyString::from(e.to_string())))?;

        let codec = decoder
            .codec()
            .ok_or_else(|| MediaLibError::FFmpegError(StabbyString::from("Failed to get codec")))?;

        // Try to find hardware acceleration
        let mut i = 0;
        let mut hw_pixel_format = AVPixelFormat::AV_PIX_FMT_NONE;
        let mut device_type = ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE;
        let mut hardware_context = None;

        unsafe {
            loop {
                let config = avcodec_get_hw_config(codec.as_ptr(), i);
                if config.is_null() {
                    break;
                }

                let hw_config = &*config;
                if (hw_config.methods
                    & ffmpeg_next::ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32)
                    != 0
                {
                    hw_pixel_format = hw_config.pix_fmt;
                    device_type = hw_config.device_type;

                    // Try to create hardware context
                    match HardwareContext::new(device_type, hw_pixel_format) {
                        Ok(ctx) => {
                            let state = DecoderContextState {
                                device_type,
                                pix_fmt: hw_pixel_format,
                            };

                            (*decoder_context_ptr).opaque =
                                Box::into_raw(Box::new(state)) as *mut std::ffi::c_void;
                            (*decoder_context_ptr).get_format = Some(get_hw_format);

                            let hw_ref = av_buffer_ref(ctx.as_ptr());
                            if hw_ref.is_null() {
                                return Err(MediaLibError::FFmpegError(StabbyString::from(
                                    "Failed to reference hardware context",
                                )));
                            }
                            (*decoder_context_ptr).hw_device_ctx = hw_ref;

                            hardware_context = Some(ctx);
                            break;
                        }
                        Err(e) => {
                            log::warn!("Failed to initialize hardware context: {}", e);
                        }
                    }
                }
                i += 1;
            }
        }

        Ok(Self {
            hardware_context,
            filter_graph: None,
            decoder,
            input_context,
            video_stream_index,
            target_size,
            eof_sent: false,
        })
    }

    fn get_next_video_packet(&mut self) -> Option<Result<Packet, MediaLibError>> {
        loop {
            match self.input_context.packets().next() {
                Some((stream, packet)) => {
                    if stream.index() == self.video_stream_index {
                        return Some(Ok(packet));
                    }
                }
                None => {
                    if let Err(e) = self.decoder.send_eof() {
                        return Some(Err(MediaLibError::FFmpegError(StabbyString::from(
                            e.to_string(),
                        ))));
                    }
                    self.eof_sent = true;
                    return None;
                }
            }
        }
    }

    /// Processes frames through hardware filters if acceleration is active.
    /// Filter graphs are created lazily on first frame to ensure proper stream info.
    pub fn get_frame(&mut self) -> Option<Result<DecodedVideoFrame, MediaLibError>> {
        if self.eof_sent {
            return None;
        }

        let mut decoded = ffmpeg_next::frame::Video::empty();
        loop {
            match self.decoder.receive_frame(&mut decoded) {
                Ok(_) => {
                    if let Some(ref hw_context) = self.hardware_context {
                        // Initialize filter graph if needed
                        if self.filter_graph.is_none() {
                            let width = decoded.width();
                            let height = decoded.height();
                            let pix_fmt = decoded.format();

                            // Get time base from stream we don't have this until we've started decoding.
                            let time_base = match self.input_context.stream(self.video_stream_index)
                            {
                                Some(stream) => stream.time_base(),
                                None => {
                                    return Some(Err(MediaLibError::FFmpegError(
                                        StabbyString::from(
                                            "Failed to get stream time base".to_string(),
                                        ),
                                    )))
                                }
                            };

                            match FilterGraph::new(
                                hw_context.clone(),
                                width,
                                height,
                                self.target_size,
                                time_base,
                                pix_fmt.into(),
                            ) {
                                Ok(graph) => self.filter_graph = Some(graph),
                                Err(e) => return Some(Err(e)),
                            }
                        }

                        // Process frame through hardware filter
                        if let Some(ref mut graph) = self.filter_graph {
                            return Some(
                                graph
                                    .process_frame(&mut decoded)
                                    .map(|frame| DecodedVideoFrame { frame }),
                            );
                        }
                    }

                    return Some(Ok(DecodedVideoFrame { frame: decoded }));
                }
                Err(ffmpeg_next::Error::Other { errno }) if errno == NEED_MORE_DATA => {
                    match self.get_next_video_packet() {
                        Some(Ok(packet)) => {
                            if let Err(e) = self.decoder.send_packet(&packet) {
                                return Some(Err(MediaLibError::FFmpegError(StabbyString::from(
                                    e.to_string(),
                                ))));
                            }
                        }
                        Some(Err(e)) => return Some(Err(e)),
                        None => return None,
                    }
                }
                Err(ffmpeg_next::Error::Eof) => return None,
                Err(e) => {
                    return Some(Err(MediaLibError::FFmpegError(StabbyString::from(
                        e.to_string(),
                    ))))
                }
            }
        }
    }
}

impl Drop for HardwareAcceleratedVideoDecoder {
    fn drop(&mut self) {
        unsafe {
            // Clean up the decoder context state that we leaked
            if let Some(decoder_ctx) = self.decoder.as_mut_ptr().as_mut() {
                if !decoder_ctx.opaque.is_null() {
                    let _ = Box::from_raw(decoder_ctx.opaque as *mut DecoderContextState);
                    decoder_ctx.opaque = std::ptr::null_mut();
                }
            }
        }
    }
}
