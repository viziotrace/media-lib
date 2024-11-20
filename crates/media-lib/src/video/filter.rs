use super::hardware::HardwareContext;
use super::types::VideoSize;
use ffmpeg_next::ffi::{
    av_buffer_ref, av_buffer_unref, av_buffersink_get_frame, av_buffersrc_add_frame_flags,
    av_hwframe_ctx_alloc, av_hwframe_ctx_init, avfilter_graph_alloc, avfilter_graph_alloc_filter,
    avfilter_graph_config, avfilter_graph_create_filter, avfilter_graph_free,
    avfilter_graph_parse2, avfilter_init_str, avfilter_link, AVFilterContext, AVFilterGraph,
    AVHWFramesContext, AVPixelFormat,
};
use log::{debug, error, info, trace, warn};
use media_types::MediaLibError;
use std::ffi::{CStr, CString};
use std::ptr::{null, null_mut};
use std::sync::Arc;

struct HwFramesCtxGuard(*mut ffmpeg_next::ffi::AVBufferRef);
impl Drop for HwFramesCtxGuard {
    fn drop(&mut self) {
        unsafe {
            ffmpeg_next::ffi::av_buffer_unref(&mut self.0);
        }
    }
}

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
    /// * `pix_fmt` - Pixel format for input frames
    /// # Returns
    /// A configured filter graph ready for processing frames
    pub fn new(
        hw_context: Option<Arc<HardwareContext>>,
        original_width: u32,
        original_height: u32,
        target_width: u32,
        target_height: u32,
        time_base: ffmpeg_next::Rational,
        pix_fmt: ffmpeg_next::ffi::AVPixelFormat,
    ) -> Result<Self, MediaLibError> {
        debug!(
            "Creating new filter graph with dimensions {}x{} -> {}x{}",
            original_width, original_height, target_width, target_height
        );

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
                hw_context.as_ref(),
            ) {
                Ok(src) => src,
                Err(e) => {
                    error!("Failed to create buffer source: {}", e);
                    avfilter_graph_free(&mut graph);
                    return Err(e);
                }
            };

            let buffersink = match Self::create_buffer_sink(&mut graph, hw_context.as_ref()) {
                Ok(sink) => sink,
                Err(e) => {
                    error!("Failed to create buffer sink: {}", e);
                    avfilter_graph_free(&mut graph);
                    return Err(e);
                }
            };

            let device_type = match hw_context {
                Some(ref hw_context) => Some(hw_context.device_type()),
                None => None,
            };

            let pix_fmt = match hw_context {
                Some(ref hw_context) => Some(hw_context.pixel_format()),
                None => None,
            };

            // Create the filter chain for hardware processing
            let filter_str =
                match Self::create_filter_str(device_type, target_width, target_height, pix_fmt) {
                    Ok(str) => str,
                    Err(e) => {
                        error!("Failed to create filter string: {}", e);
                        avfilter_graph_free(&mut graph);
                        return Err(e);
                    }
                };
            info!("Using filter string: {}", filter_str);

            // Parse and link the filter chain
            if let Err(e) = Self::configure_filter_chain(
                &mut graph,
                &filter_str,
                buffersrc,
                buffersink,
                hw_context.as_ref(),
            ) {
                error!("Failed to configure filter chain: {}", e);
                avfilter_graph_free(&mut graph);
                return Err(e);
            }

            // Dump the filter graph for debugging
            let graph_dump = ffmpeg_next::ffi::avfilter_graph_dump(graph, std::ptr::null_mut());
            if !graph_dump.is_null() {
                let graph_str = CStr::from_ptr(graph_dump).to_string_lossy();
                debug!("Filter Graph Configuration:\n{}", graph_str);
                ffmpeg_next::ffi::av_free(graph_dump as *mut _);
            }

            // Verify filter graph configuration
            let ret = avfilter_graph_config(graph, null_mut());
            if ret < 0 {
                error!("Failed to configure filter graph");
                avfilter_graph_free(&mut graph);
                return Err(MediaLibError::FFmpegError(
                    "Failed to configure filter graph".into(),
                ));
            }

            info!("Successfully created filter graph");
            Ok(Self {
                graph,
                buffersrc,
                buffersink,
            })
        }
    }

    /// Creates an empty FFmpeg filter graph
    unsafe fn create_filter_graph() -> Result<*mut AVFilterGraph, MediaLibError> {
        trace!("Creating empty filter graph");
        let graph = avfilter_graph_alloc();
        if graph.is_null() {
            error!("Failed to allocate filter graph");
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
        hw_context: Option<&Arc<HardwareContext>>,
    ) -> Result<*mut AVFilterContext, MediaLibError> {
        debug!("Creating buffer source filter {}x{}", width, height);

        // Get the buffer source filter
        let buffersrc_name = CString::new("buffer").map_err(|_| {
            error!("Failed to create buffer source name");
            MediaLibError::FFmpegError("Failed to create buffer source name".into())
        })?;
        let filter = ffmpeg_next::ffi::avfilter_get_by_name(buffersrc_name.as_ptr());
        if filter.is_null() {
            error!("Could not find FFmpeg buffer source filter 'buffer'");
            return Err(MediaLibError::FFmpegError(
                "Could not find FFmpeg buffer source filter 'buffer'".into(),
            ));
        }

        // Allocate the filter context
        let buffersrc = avfilter_graph_alloc_filter(*graph, filter, buffersrc_name.as_ptr());
        if buffersrc.is_null() {
            error!("Failed to allocate memory for buffer source filter context");
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate memory for buffer source filter context".into(),
            ));
        }

        if let Some(hw_context) = hw_context {
            (*buffersrc).hw_device_ctx = av_buffer_ref(hw_context.as_ptr());
        }
        // Allocate and initialize buffer source parameters
        let params = ffmpeg_next::ffi::av_buffersrc_parameters_alloc();
        if params.is_null() {
            error!("Failed to allocate buffer source parameters");
            return Err(MediaLibError::FFmpegError(
                "Failed to allocate buffer source parameters".into(),
            ));
        }

        match hw_context {
            Some(ctx) => {
                // Allocate hardware frames context
                let hw_frames_ctx = av_hwframe_ctx_alloc(ctx.as_ptr());
                if hw_frames_ctx.is_null() {
                    return Err(MediaLibError::FFmpegError(
                        "Failed to allocate hardware frames context".into(),
                    ));
                }

                // Use defer pattern to ensure cleanup on error
                let _guard = HwFramesCtxGuard(hw_frames_ctx);

                // Configure frames context
                let frames_ctx = (*hw_frames_ctx).data as *mut AVHWFramesContext;
                (*frames_ctx).format = pix_fmt;
                (*frames_ctx).sw_format = AVPixelFormat::AV_PIX_FMT_NV12;
                (*frames_ctx).width = width as i32;
                (*frames_ctx).height = height as i32;

                // Initialize
                let ret = av_hwframe_ctx_init(hw_frames_ctx);
                if ret < 0 {
                    return Err(MediaLibError::FFmpegError(
                        "Failed to initialize hardware frames context".into(),
                    ));
                }

                // Set the parameters for hardware context
                (*params).hw_frames_ctx = av_buffer_ref(hw_frames_ctx);
            }
            _ => {
                // We don't need to do anything if we're not in a hardware context
            }
        }

        // Set the common parameters
        (*params).format = pix_fmt as i32;
        (*params).time_base = time_base.into();
        (*params).width = width as i32;
        (*params).height = height as i32;

        let ret = ffmpeg_next::ffi::av_buffersrc_parameters_set(buffersrc, params);
        ffmpeg_next::ffi::av_free(params as *mut _);

        if ret < 0 {
            error!("Failed to set buffer source parameters");
            return Err(MediaLibError::FFmpegError(
                "Failed to set buffer source parameters".into(),
            ));
        }

        // Initialize the filter
        let ret = avfilter_init_str(buffersrc, std::ptr::null());
        if ret < 0 {
            error!("Failed to initialize buffer source filter");
            return Err(MediaLibError::FFmpegError(
                "Failed to initialize buffer source filter".into(),
            ));
        }

        debug!("Successfully created buffer source filter");
        Ok(buffersrc)
    }

    /// Creates and configures the buffer sink filter
    unsafe fn create_buffer_sink(
        graph: &mut *mut AVFilterGraph,
        hw_context: Option<&Arc<HardwareContext>>,
    ) -> Result<*mut AVFilterContext, MediaLibError> {
        debug!("Creating buffer sink filter");
        let buffersink_name = CString::new("buffersink")
            .map_err(|_| MediaLibError::FFmpegError("Failed to create buffersink name".into()))?;
        let filter = ffmpeg_next::ffi::avfilter_get_by_name(buffersink_name.as_ptr());
        if filter.is_null() {
            error!("Could not find buffersink filter");
            return Err(MediaLibError::FFmpegError(
                "Could not find buffersink filter".into(),
            ));
        }

        let mut buffersink = null_mut();
        let ret = avfilter_graph_create_filter(
            &mut buffersink,
            filter,
            buffersink_name.as_ptr(),
            null(),
            null_mut(),
            *graph,
        );

        if ret < 0 {
            error!("Failed to create buffer sink filter");
            return Err(MediaLibError::FFmpegError(
                "Failed to create buffer sink filter".into(),
            ));
        }

        if let Some(hw_context) = hw_context {
            (*buffersink).hw_device_ctx = av_buffer_ref(hw_context.as_ptr());
        }

        debug!("Successfully created buffer sink filter");
        Ok(buffersink)
    }

    /// Configures and links the filter chain between source and sink
    unsafe fn configure_filter_chain(
        graph: &mut *mut AVFilterGraph,
        filter_str: &str,
        buffersrc: *mut AVFilterContext,
        buffersink: *mut AVFilterContext,
        hw_context: Option<&Arc<HardwareContext>>,
    ) -> Result<(), MediaLibError> {
        debug!("Configuring filter chain with string: {}", filter_str);
        let filter_str = CString::new(filter_str)
            .map_err(|_| MediaLibError::FFmpegError("Failed to create filter string".into()))?;

        let result = (|| {
            // Initialize inputs and outputs
            let mut inputs = ffmpeg_next::ffi::avfilter_inout_alloc();
            let mut outputs = ffmpeg_next::ffi::avfilter_inout_alloc();

            // Use defer pattern for cleanup
            struct InOutGuard {
                inputs: *mut ffmpeg_next::ffi::AVFilterInOut,
                outputs: *mut ffmpeg_next::ffi::AVFilterInOut,
            }
            impl Drop for InOutGuard {
                fn drop(&mut self) {
                    unsafe {
                        ffmpeg_next::ffi::avfilter_inout_free(&mut self.inputs);
                        ffmpeg_next::ffi::avfilter_inout_free(&mut self.outputs);
                    }
                }
            }
            let _guard = InOutGuard { inputs, outputs };

            // Set up the inputs
            (*inputs).name = CString::new("in").unwrap().into_raw();
            (*inputs).filter_ctx = buffersrc;
            (*inputs).pad_idx = 0;
            (*inputs).next = null_mut();

            // Set up the outputs
            (*outputs).name = CString::new("out").unwrap().into_raw();
            (*outputs).filter_ctx = buffersink;
            (*outputs).pad_idx = 0;
            (*outputs).next = null_mut();

            // Parse the filter string
            let ret = avfilter_graph_parse2(*graph, filter_str.as_ptr(), &mut inputs, &mut outputs);
            if ret < 0 {
                error!("Failed to parse filter graph");
                return Err(MediaLibError::FFmpegError(
                    "Failed to parse filter graph".into(),
                ));
            }

            // XXX: Super ugly AI helped me figure this out but its pretty bad.
            // Set hardware device context for all filters in the graph. Because we've initialized the filters with a string we need to iterate through and set the hw_device_ctx manually.
            if let Some(hw_context) = hw_context {
                for i in 0..(**graph).nb_filters as isize {
                    let filter = *(**graph).filters.offset(i);
                    if !filter.is_null() {
                        (*filter).hw_device_ctx = av_buffer_ref(hw_context.as_ptr());
                    }
                }
            }

            // ers Directly link buffer source to first filter and last filter to buffer sink
            let ret = avfilter_link(buffersrc, 0, (*inputs).filter_ctx, 0);
            if ret < 0 {
                error!("Failed to link buffer source to filter chain");
                return Err(MediaLibError::FFmpegError(
                    "Failed to link buffer source to filter chain".into(),
                ));
            }

            let ret = avfilter_link((*outputs).filter_ctx, 0, buffersink, 0);
            if ret < 0 {
                error!("Failed to link filter chain to buffer sink");
                return Err(MediaLibError::FFmpegError(
                    "Failed to link filter chain to buffer sink".into(),
                ));
            }

            // Free the filter inputs/outputs
            ffmpeg_next::ffi::avfilter_inout_free(&mut inputs);
            ffmpeg_next::ffi::avfilter_inout_free(&mut outputs);

            debug!("Successfully configured filter chain");
            Ok(())
        })();

        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                error!("Filter chain configuration failed: {}", e);
                Err(e)
            }
        }
    }

    /// Creates the filter chain string for hardware-specific processing
    fn create_filter_str(
        device_type: Option<ffmpeg_next::ffi::AVHWDeviceType>,
        target_width: u32,
        target_height: u32,
        pix_fmt: Option<ffmpeg_next::ffi::AVPixelFormat>,
    ) -> Result<String, MediaLibError> {
        debug!("Creating filter string for device type: {:?}", device_type);
        match device_type {
            Some(ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA) => {
                let filter_str = format!(
                    "hwupload_cuda,scale_cuda={}:{},hwdownload_cuda,format=yuv420p",
                    target_width, target_height
                );
                debug!("Created CUDA filter string: {}", filter_str);
                Ok(filter_str)
            }
            Some(ffmpeg_next::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX) => {
                if let Some(pix_fmt) = pix_fmt {
                    let filter_str = format!(
                        "format={},hwupload,scale_vt={}:{},hwdownload,format=nv12,format=yuv420p",
                        pix_fmt as i32, target_width, target_height
                    );
                    debug!("Created VideoToolbox filter string: {}", filter_str);
                    Ok(filter_str)
                } else {
                    error!("No pixel format provided for VideoToolbox");
                    Err(MediaLibError::FFmpegError(
                        "No pixel format provided for VideoToolbox".into(),
                    ))
                }
            }
            _ => {
                debug!("Using software scaling fallback");
                let filter_str = format!("scale={}:{},format=yuv420p", target_width, target_height);
                debug!("Created software filter string: {}", filter_str);
                Ok(filter_str)
            }
        }
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
        trace!("Processing frame through filter graph");
        if self.buffersrc.is_null() || self.buffersink.is_null() {
            error!("Filter contexts are null");
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
                error!("Failed to add frame to filter");
                return Err(MediaLibError::FFmpegError(
                    "Failed to add frame to filter".into(),
                ));
            }

            // Get the processed frame from the sink
            let mut output = ffmpeg_next::frame::Video::empty();
            let ret = av_buffersink_get_frame(self.buffersink, output.as_mut_ptr());

            if ret < 0 {
                error!("Failed to get filtered frame");
                return Err(MediaLibError::FFmpegError(
                    "Failed to get filtered frame".into(),
                ));
            }

            trace!("Successfully processed frame");
            Ok(output)
        }
    }
}

impl Drop for FilterGraph {
    fn drop(&mut self) {
        debug!("Dropping FilterGraph");
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
