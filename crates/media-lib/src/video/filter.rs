use super::hardware::HardwareContext;
use super::types::VideoSize;
use ffmpeg_next::ffi::{
    av_buffer_ref, av_buffersink_get_frame, av_buffersrc_add_frame_flags,
    av_buffersrc_parameters_alloc, av_buffersrc_parameters_set, av_free, avfilter_graph_alloc,
    avfilter_graph_alloc_filter, avfilter_graph_config, avfilter_graph_create_filter,
    avfilter_graph_free, avfilter_graph_parse2, avfilter_init_str, avfilter_link, AVFilterContext,
    AVFilterGraph,
};
use media_types::MediaLibError;
use std::ffi::CString;
use std::ptr::{null, null_mut};
use std::sync::Arc;

/// FFmpeg filter graph implementation for hardware-accelerated video processing.
/// Handles frame format conversion and scaling using hardware-specific filters.
///
/// The filter graph consists of:
/// 1. A buffer source filter that receives input frames
/// 2. Hardware-specific scaling and format conversion filters
/// 3. A buffer sink filter that outputs the processed frames
pub struct FilterGraph {
    /// The FFmpeg filter graph containing all filters and their connections
    graph: *mut AVFilterGraph,
    /// The buffer source filter that receives input frames
    buffersrc: *mut AVFilterContext,
    /// The buffer sink filter that outputs processed frames
    buffersink: *mut AVFilterContext,
    /// Hardware acceleration context used for processing
    hw_context: Arc<HardwareContext>,
}

impl FilterGraph {
    /// Creates a new filter graph for hardware-accelerated video processing.
    ///
    /// # Arguments
    /// * `hw_context` - Hardware acceleration context
    /// * `original_width` - Width of input frames
    /// * `original_height` - Height of input frames  
    /// * `target_size` - Desired output frame size
    /// * `time_base` - Time base for frame timestamps
    ///
    /// # Returns
    /// A configured filter graph ready for processing frames
    pub fn new(
        hw_context: Arc<HardwareContext>,
        original_width: u32,
        original_height: u32,
        target_size: VideoSize,
        time_base: ffmpeg_next::Rational,
        pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
    ) -> Result<Self, MediaLibError> {
        unsafe {
            // Create the filter graph
            let mut graph = Self::create_filter_graph()?;

            // Create and configure the source/sink filters
            let buffersrc = match Self::create_buffer_source(
                &mut graph,
                original_width,
                original_height,
                time_base,
                pix_fmt,
                &hw_context,
            ) {
                Ok(src) => src,
                Err(e) => {
                    avfilter_graph_free(&mut graph);
                    return Err(e);
                }
            };

            let buffersink = match Self::create_buffer_sink(
                &mut graph,
                target_size.dimensions(),
                pix_fmt,
                &hw_context,
            ) {
                Ok(sink) => sink,
                Err(e) => {
                    avfilter_graph_free(&mut graph);
                    return Err(e);
                }
            };

            // Create the filter chain for hardware processing
            let filter_str = match Self::create_filter_str(
                hw_context.device_type(),
                original_width,
                original_height,
                target_size.dimensions().0,
                target_size.dimensions().1,
                time_base,
                hw_context.pixel_format(),
            ) {
                Ok(str) => str,
                Err(e) => {
                    avfilter_graph_free(&mut graph);
                    return Err(e);
                }
            };

            // Parse and link the filter chain
            if let Err(e) =
                Self::configure_filter_chain(&mut graph, &filter_str, buffersrc, buffersink)
            {
                avfilter_graph_free(&mut graph);
                return Err(e);
            }

            Ok(Self {
                graph,
                buffersrc,
                buffersink,
                hw_context,
            })
        }
    }

    /// Creates an empty FFmpeg filter graph
    unsafe fn create_filter_graph() -> Result<*mut AVFilterGraph, MediaLibError> {
        let graph = avfilter_graph_alloc();
        if graph.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate filter graph".into(),
            ));
        }
        Ok(graph)
    }

    /// Creates and configures the buffer source filter
    unsafe fn create_buffer_source(
        graph: &mut *mut AVFilterGraph,
        width: u32,
        height: u32,
        time_base: ffmpeg_next::Rational,
        pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
        hw_context: &HardwareContext,
    ) -> Result<*mut AVFilterContext, MediaLibError> {
        // Get the buffer source filter
        let buffersrc_name = CString::new("buffer").map_err(|_| {
            MediaLibError::FFmpegError("Failed to create buffer source name".into())
        })?;
        let filter = ffmpeg_next::ffi::avfilter_get_by_name(buffersrc_name.as_ptr());
        if filter.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Could not find FFmpeg buffer source filter 'buffer'".into(),
            ));
        }

        // Allocate the filter context
        let buffersrc = avfilter_graph_alloc_filter(*graph, filter, buffersrc_name.as_ptr());
        if buffersrc.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate memory for buffer source filter context".into(),
            ));
        }

        // Allocate and initialize buffer source parameters
        let params = ffmpeg_next::ffi::av_buffersrc_parameters_alloc();
        if params.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate buffer source parameters".into(),
            ));
        }

        // Set the parameters
        (*params).format = pix_fmt as i32;
        (*params).time_base = ffmpeg_next::ffi::AVRational {
            num: time_base.numerator(),
            den: time_base.denominator(),
        };
        (*params).width = width as i32;
        (*params).height = height as i32;
        (*params).hw_frames_ctx = av_buffer_ref(hw_context.as_ptr());

        let ret = ffmpeg_next::ffi::av_buffersrc_parameters_set(buffersrc, params);
        ffmpeg_next::ffi::av_free(params as *mut _);

        if ret < 0 {
            return Err(MediaLibError::FFmpegError(
                "Failed to set buffer source parameters".into(),
            ));
        }

        // Initialize the filter
        let ret = avfilter_init_str(buffersrc, std::ptr::null());
        if ret < 0 {
            return Err(MediaLibError::FFmpegError(
                "Failed to initialize buffer source filter".into(),
            ));
        }

        Ok(buffersrc)
    }

    /// Creates and configures the buffer sink filter
    unsafe fn create_buffer_sink(
        graph: &mut *mut AVFilterGraph,
        dimensions: (u32, u32),
        pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
        hw_context: &HardwareContext,
    ) -> Result<*mut AVFilterContext, MediaLibError> {
        let buffersink_name = CString::new("buffersink")
            .map_err(|_| MediaLibError::FFmpegError("Failed to create buffersink name".into()))?;
        let mut buffersink = avfilter_graph_alloc_filter(
            *graph,
            ffmpeg_next::ffi::avfilter_get_by_name(buffersink_name.as_ptr()),
            buffersink_name.as_ptr(),
        );
        if buffersink.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate buffer sink filter".into(),
            ));
        }
        (*buffersink).hw_device_ctx = av_buffer_ref(hw_context.as_ptr());

        // Create the buffer sink filter
        let ret = avfilter_graph_create_filter(
            &mut buffersink,
            ffmpeg_next::ffi::avfilter_get_by_name(buffersink_name.as_ptr()),
            buffersink_name.as_ptr(),
            null(),
            null_mut(),
            *graph,
        );

        if ret < 0 {
            return Err(MediaLibError::FFmpegError(
                "Failed to create buffer sink filter".into(),
            ));
        }

        Ok(buffersink)
    }

    /// Configures and links the filter chain between source and sink
    unsafe fn configure_filter_chain(
        graph: &mut *mut AVFilterGraph,
        filter_str: &str,
        buffersrc: *mut AVFilterContext,
        buffersink: *mut AVFilterContext,
    ) -> Result<(), MediaLibError> {
        let filter_str = CString::new(filter_str)
            .map_err(|_| MediaLibError::FFmpegError("Failed to create filter string".into()))?;

        // Initialize inputs
        let mut inputs = ffmpeg_next::ffi::avfilter_inout_alloc();
        if inputs.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate filter inputs".into(),
            ));
        }

        // Initialize outputs
        let mut outputs = ffmpeg_next::ffi::avfilter_inout_alloc();
        if outputs.is_null() {
            ffmpeg_next::ffi::avfilter_inout_free(&mut inputs);
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate filter outputs".into(),
            ));
        }

        // Parse the filter string
        let ret = avfilter_graph_parse2(*graph, filter_str.as_ptr(), &mut inputs, &mut outputs);
        if ret < 0 {
            if !inputs.is_null() {
                ffmpeg_next::ffi::avfilter_inout_free(&mut inputs);
            }
            if !outputs.is_null() {
                ffmpeg_next::ffi::avfilter_inout_free(&mut outputs);
            }
            return Err(MediaLibError::FFmpegError(
                "Failed to parse filter graph".into(),
            ));
        }

        // Link the source filter to the first filter in the chain
        if !inputs.is_null() {
            let ret = avfilter_link(buffersrc, 0, (*inputs).filter_ctx, 0);
            if ret < 0 {
                ffmpeg_next::ffi::avfilter_inout_free(&mut inputs);
                ffmpeg_next::ffi::avfilter_inout_free(&mut outputs);
                return Err(MediaLibError::FFmpegError(
                    "Failed to link buffer source".into(),
                ));
            }
        }

        // Link the last filter in the chain to the sink
        if !outputs.is_null() {
            let ret = avfilter_link((*outputs).filter_ctx, 0, buffersink, 0);
            if ret < 0 {
                ffmpeg_next::ffi::avfilter_inout_free(&mut inputs);
                ffmpeg_next::ffi::avfilter_inout_free(&mut outputs);
                return Err(MediaLibError::FFmpegError(
                    "Failed to link buffer sink".into(),
                ));
            }
        }

        // Dump the filter graph for debugging
        let ret = ffmpeg_next::ffi::avfilter_graph_dump(*graph, null());
        if !ret.is_null() {
            let graph_str = unsafe { std::ffi::CStr::from_ptr(ret) }.to_string_lossy();
            println!("Filter graph configuration:\n{}", graph_str);
            unsafe { av_free(ret as *mut ::std::os::raw::c_void) };
        }

        // Print pointer addresses for debugging
        println!(
            "Filter graph pointers:\n\
             graph: {:p}\n\
             inputs: {:p}\n\
             outputs: {:p}\n\
             buffersrc: {:p}\n\
             buffersink: {:p}",
            *graph, inputs, outputs, buffersrc, buffersink
        );

        // Configure the complete graph
        let ret = avfilter_graph_config(*graph, null_mut());
        println!("ret: {}", ret);
        if ret < 0 {
            return Err(MediaLibError::FFmpegError(
                format!("Failed to configure filter graph: {}", ret).into(),
            ));
        }

        Ok(())
    }

    /// Creates the filter chain string for hardware-specific processing
    fn create_filter_str(
        device_type: ffmpeg_next::ffi::AVHWDeviceType,
        original_width: u32,
        original_height: u32,
        target_width: u32,
        target_height: u32,
        time_base: ffmpeg_next::Rational,
        pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
    ) -> Result<String, MediaLibError> {
        let hw_filter = match device_type {
            ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA => {
                format!(
                    "hwupload_cuda,scale_cuda=w={}:h={}",
                    target_width, target_height
                )
            }
            ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX => {
                format!(
                    "format=pix_fmts={},scale_vt=w={}:h={}",
                    pix_fmt as i32, target_width, target_height
                )
            }
            _ => {
                return Err(MediaLibError::FFmpegError(
                    "Unsupported hardware device type".into(),
                ))
            }
        };

        Ok(format!("{},hwdownload,format=rgba", hw_filter))
    }

    /// Processes a single video frame through the filter graph
    ///
    /// # Arguments
    /// * `input` - Input video frame to process
    ///
    /// # Returns
    /// The processed output frame
    pub fn process_frame(
        &mut self,
        input: &mut ffmpeg_next::frame::Video,
    ) -> Result<ffmpeg_next::frame::Video, MediaLibError> {
        if self.buffersrc.is_null() || self.buffersink.is_null() {
            return Err(MediaLibError::FFmpegError(
                "Filter contexts are null".into(),
            ));
        }

        unsafe {
            // Add the input frame to the source
            let ret = av_buffersrc_add_frame_flags(
                self.buffersrc,
                input.as_mut_ptr(),
                ffmpeg_next::ffi::AV_BUFFERSRC_FLAG_KEEP_REF as i32,
            );

            if ret < 0 {
                return Err(MediaLibError::FFmpegError(
                    "Failed to add frame to filter".into(),
                ));
            }

            // Get the processed frame from the sink
            let mut output = ffmpeg_next::frame::Video::empty();
            let ret = av_buffersink_get_frame(self.buffersink, output.as_mut_ptr());

            if ret < 0 {
                return Err(MediaLibError::FFmpegError(
                    "Failed to get filtered frame".into(),
                ));
            }

            Ok(output)
        }
    }
}

impl Drop for FilterGraph {
    fn drop(&mut self) {
        unsafe {
            if !self.graph.is_null() {
                avfilter_graph_free(&mut self.graph);
                self.graph = null_mut();
                self.buffersrc = null_mut();
                self.buffersink = null_mut();
            }
        }
    }
}
