use std::ptr::null;
use std::{path::Path, ptr::null_mut};

use ffmpeg_next::ffi::{
    av_buffer_ref, av_buffer_unref, av_hwframe_transfer_data, avfilter_graph_alloc,
    avfilter_graph_free,
};
use ffmpeg_next::Packet;
use ffmpeg_next::{
    codec::{self, context::Context},
    ffi::{av_hwdevice_ctx_create, avcodec_get_hw_config, AVBufferRef, AVHWDeviceType},
};
use image::buffer;
use media_types::MediaLibError;

// The error status sent when the decoder needs more data
const NEED_MORE_DATA: i32 = 35;

#[derive(Debug, Clone, Copy)]
pub enum VideoSize {
    P240,  // 426x240
    P360,  // 640x360
    P480,  // 854x480
    P720,  // 1280x720
    P1080, // 1920x1080
}

impl VideoSize {
    fn dimensions(&self) -> (u32, u32) {
        match self {
            VideoSize::P240 => (426, 240),
            VideoSize::P360 => (640, 360),
            VideoSize::P480 => (854, 480),
            VideoSize::P720 => (1280, 720),
            VideoSize::P1080 => (1920, 1080),
        }
    }
}

pub struct DecodedVideoFrame {
    pub frame: ffmpeg_next::frame::Video,
}

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

struct FilterGraphConfig {
    original_width: u32,
    original_height: u32,
    target_width: u32,
    target_height: u32,
    pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
    time_base: ffmpeg_next::Rational,
}

pub struct HardwareAcceleratedGraphTransform {
    graph: *mut ffmpeg_next::ffi::AVFilterGraph,
    buffersrc: *mut ffmpeg_next::ffi::AVFilterContext,
    buffersink: *mut ffmpeg_next::ffi::AVFilterContext,
    device_type: AVHWDeviceType,
}

impl HardwareAcceleratedGraphTransform {
    pub unsafe fn new(
        device_type: AVHWDeviceType,
        original_width: u32,
        original_height: u32,
        target_size: VideoSize,
        pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
        time_base: ffmpeg_next::Rational,
        hw_device_ctx: *mut AVBufferRef,
    ) -> Result<Self, MediaLibError> {
        let mut graph = avfilter_graph_alloc();
        if graph.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate filter graph".into(),
            ));
        }

        let mut buffersrc = null_mut();
        let mut buffersink = null_mut();

        let (target_width, target_height) = target_size.dimensions();

        let config = FilterGraphConfig {
            original_width,
            original_height,
            target_width,
            target_height,
            pix_fmt,
            time_base,
        };

        let filter_str = Self::create_filter_str(device_type, &config)?;

        println!("filter_str: {}", filter_str);

        if let Err(e) = Self::configure_filter_graph(
            graph,
            &filter_str,
            &mut buffersrc,
            &mut buffersink,
            hw_device_ctx,
            device_type,
        ) {
            avfilter_graph_free(&mut graph);
            return Err(e);
        }

        Ok(Self {
            graph,
            buffersrc,
            buffersink,
            device_type,
        })
    }

    fn create_filter_str(
        device_type: AVHWDeviceType,
        config: &FilterGraphConfig,
    ) -> Result<String, MediaLibError> {
        let FilterGraphConfig {
            original_width,
            original_height,
            target_width,
            target_height,
            pix_fmt,
            time_base,
        } = config;

        // Convert AVPixelFormat to string format name
        let input_pix_fmt = match *pix_fmt {
            ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_NV12 => "nv12",
            ffmpeg_next::ffi::AVPixelFormat::AV_PIX_FMT_YUV420P => "yuv420p",
            _ => "nv12", // default to nv12 for hardware acceleration
        };

        let base_input = format!(
            "buffer=width={}:height={}:pix_fmt={}:time_base={}/{}[in];",
            original_width,
            original_height,
            input_pix_fmt,
            time_base.numerator(),
            time_base.denominator()
        );

        let processing = match device_type {
            AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA => format!(
                "[in]hwupload_cuda,scale_cuda={}:{}[scaled];\
                 [scaled]hwdownload,format=rgba[out];",
                target_width, target_height
            ),
            AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX => format!(
                "[in]hwupload,scale_vt={}:{}[scaled];\
                 [scaled]format=rgba[out];",
                target_width, target_height
            ),
            _ => {
                return Err(MediaLibError::FFmpegError(
                    "Unsupported hardware device type".into(),
                ))
            }
        };

        Ok(format!(
            "{}{}\
                   [out]buffersink",
            base_input, processing
        ))
    }

    unsafe fn configure_filter_graph(
        graph: *mut ffmpeg_next::ffi::AVFilterGraph,
        filter_str: &str,
        buffersrc: &mut *mut ffmpeg_next::ffi::AVFilterContext,
        buffersink: &mut *mut ffmpeg_next::ffi::AVFilterContext,
        hw_device_ctx: *mut AVBufferRef,
        device_type: AVHWDeviceType,
    ) -> Result<(), MediaLibError> {
        (**buffersrc).hw_device_ctx = av_buffer_ref(hw_device_ctx);
        (**buffersink).hw_device_ctx = av_buffer_ref(hw_device_ctx);

        let mut inputs = null_mut();
        let mut outputs = null_mut();

        let ret = ffmpeg_next::ffi::avfilter_graph_parse2(
            graph,
            filter_str.as_ptr() as *const i8,
            &mut inputs,
            &mut outputs,
        );

        if ret < 0 {
            println!("Failed to parse filter graph: {}", ret);
            return Err(MediaLibError::FFmpegError(
                "Failed to parse filter graph".into(),
            ));
        }

        // Get the first input and output filters
        if !inputs.is_null() {
            *buffersrc = (*inputs).filter_ctx;
            ffmpeg_next::ffi::avfilter_inout_free(&mut inputs);
        }

        if !outputs.is_null() {
            *buffersink = (*outputs).filter_ctx;
            ffmpeg_next::ffi::avfilter_inout_free(&mut outputs);
        }

        if !(*buffersrc).is_null() {
            let mut hw_frames_ctx = ffmpeg_next::ffi::av_hwframe_ctx_alloc(hw_device_ctx);
            if hw_frames_ctx.is_null() {
                return Err(MediaLibError::FFmpegError(
                    "Failed to create hardware frames context".into(),
                ));
            }
        }

        let ret = ffmpeg_next::ffi::avfilter_graph_config(graph, null_mut());
        if ret < 0 {
            return Err(MediaLibError::FFmpegError(
                "Failed to configure filter graph".into(),
            ));
        }

        Ok(())
    }

    pub unsafe fn process_frame(
        &mut self,
        input: &mut ffmpeg_next::frame::Video,
    ) -> Result<ffmpeg_next::frame::Video, MediaLibError> {
        // Add frame to source buffer
        let ret = ffmpeg_next::ffi::av_buffersrc_add_frame_flags(
            self.buffersrc,
            input.as_mut_ptr(),
            ffmpeg_next::ffi::AV_BUFFERSRC_FLAG_PUSH as i32,
        );

        if ret < 0 {
            return Err(MediaLibError::FFmpegError(
                "Failed to add frame to filter graph".into(),
            ));
        }

        // Get processed frame from sink buffer
        let mut output = ffmpeg_next::frame::Video::empty();
        let ret = ffmpeg_next::ffi::av_buffersink_get_frame(self.buffersink, output.as_mut_ptr());

        if ret < 0 {
            return Err(MediaLibError::FFmpegError(
                "Failed to get frame from filter graph".into(),
            ));
        }

        Ok(output)
    }
}

impl Drop for HardwareAcceleratedGraphTransform {
    fn drop(&mut self) {
        unsafe { avfilter_graph_free(&mut self.graph) };
    }
}

pub struct HardwareAcceleratedVideoDecoder {
    pub hardware_accelerated: bool,
    pub eof_sent: bool,
    pub video_decoder: codec::decoder::Video,
    pub video_stream_index: usize,

    pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
    ictx: ffmpeg_next::format::context::Input,
    hw_transform: Option<HardwareAcceleratedGraphTransform>,
    device_type: AVHWDeviceType,
    target_size: VideoSize,
    hw_device_ctx: *mut AVBufferRef,
}

impl Drop for HardwareAcceleratedVideoDecoder {
    fn drop(&mut self) {
        unsafe { av_buffer_unref(&mut self.hw_device_ctx) };
    }
}

impl HardwareAcceleratedVideoDecoder {
    pub unsafe fn new(input_path: &Path, target_size: VideoSize) -> Result<Self, MediaLibError> {
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
            println!("config: {:?}", config);
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
                hardware_accelerated = true;
                break;
            }

            i += 1;
        }

        Ok(HardwareAcceleratedVideoDecoder {
            ictx,
            video_decoder,
            hardware_accelerated,
            pix_fmt: hw_pixel_format,
            eof_sent: false,
            video_stream_index,
            hw_transform: None,
            device_type,
            target_size,
            hw_device_ctx,
        })
    }

    fn get_next_video_packet(&mut self) -> Option<Result<Packet, MediaLibError>> {
        loop {
            match self.ictx.packets().next() {
                Some((stream, packet)) => {
                    if stream.index() == self.video_stream_index {
                        return Some(Ok(packet));
                    }
                    // Continue looping if it's not a video packet
                }
                None => {
                    if let Err(e) = self.video_decoder.send_eof() {
                        return Some(Err(MediaLibError::FFmpegError(e.to_string().into())));
                    }
                    self.eof_sent = true;
                    return None;
                }
            }
        }
    }

    pub fn get_frame(&mut self) -> Option<Result<DecodedVideoFrame, MediaLibError>> {
        // If EoF is sent, we need to return None
        if self.eof_sent {
            return None;
        }

        // Try to receive the frame and if we NEED_MORE_DATA then send a packet do this in a loop until we get a frame or EOF.
        let mut decoded = ffmpeg_next::frame::Video::empty();
        loop {
            match self.video_decoder.receive_frame(&mut decoded) {
                Ok(_) => {
                    // We need to check if the frame format is the same as the hardware accelerated format but the safe method
                    // returns an enum we can't check against directly.
                    let frame_format = unsafe { *decoded.as_ptr() }.format;
                    if self.hardware_accelerated && frame_format == self.pix_fmt as i32 {
                        // Initialize hardware transform if needed
                        if self.hw_transform.is_none() {
                            let width = decoded.width();
                            let height = decoded.height();
                            // let time_base = decoded.aspect_ratio();
                            let rational = ffmpeg_next::Rational::new(1, 1);
                            println!("time_base: {:?}", rational);
                            match unsafe {
                                HardwareAcceleratedGraphTransform::new(
                                    self.device_type,
                                    width,
                                    height,
                                    self.target_size,
                                    self.pix_fmt,
                                    rational,
                                    self.hw_device_ctx,
                                )
                            } {
                                Ok(transform) => self.hw_transform = Some(transform),
                                Err(e) => return Some(Err(e)),
                            }
                        }

                        // Process frame through hardware transform
                        if let Some(ref mut transform) = self.hw_transform {
                            unsafe {
                                let processed_frame = transform.process_frame(&mut decoded);
                                return Some(match processed_frame {
                                    Ok(frame) => Ok(DecodedVideoFrame { frame }),
                                    Err(e) => Err(e),
                                });
                            }
                        }
                    }

                    return Some(Ok(DecodedVideoFrame { frame: decoded }));
                }
                Err(ffmpeg_next::Error::Other { errno }) => {
                    if errno == NEED_MORE_DATA {
                        match self.get_next_video_packet() {
                            Some(result) => match result {
                                Ok(packet) => {
                                    let send_packet_result =
                                        self.video_decoder.send_packet(&packet);
                                    if let Err(e) = send_packet_result {
                                        return Some(Err(MediaLibError::FFmpegError(
                                            e.to_string().into(),
                                        )));
                                    }
                                }
                                Err(e) => return Some(Err(e)),
                            },
                            None => return None,
                        }
                    } else {
                        return Some(Err(MediaLibError::FFmpegError(
                            format!("Failed to receive frame: {}", errno).into(),
                        )));
                    }
                }
                Err(ffmpeg_next::Error::Eof) => return None,
                Err(e) => return Some(Err(MediaLibError::FFmpegError(e.to_string().into()))),
            }
        }
    }
}
