use ffmpeg_next::ffi::{
    av_buffer_ref, av_buffer_unref, av_hwdevice_ctx_create, AVBufferRef, AVHWDeviceType,
    AVPixelFormat,
};
use media_types::MediaLibError;
use std::ptr::null_mut;
use std::sync::Arc;

/// RAII wrapper for FFmpeg hardware device contexts.
/// Handles creation and cleanup of hardware acceleration contexts.
pub struct HardwareContext {
    ctx: *mut AVBufferRef,
    device_type: AVHWDeviceType,
    pix_fmt: AVPixelFormat,
}

impl HardwareContext {
    /// Attempts to create a hardware context for the specified device type.
    /// Currently supports CUDA and VideoToolbox.
    pub fn new(
        device_type: AVHWDeviceType,
        pix_fmt: AVPixelFormat,
    ) -> Result<Arc<Self>, MediaLibError> {
        let mut hw_device_ctx = null_mut();

        unsafe {
            if av_hwdevice_ctx_create(
                &mut hw_device_ctx,
                device_type,
                std::ptr::null(),
                null_mut(),
                0,
            ) < 0
            {
                return Err(MediaLibError::FFmpegError(
                    format!(
                        "Failed to create hardware device context for {:?}",
                        device_type
                    )
                    .into(),
                ));
            }
        }

        Ok(Arc::new(Self {
            ctx: hw_device_ctx,
            device_type,
            pix_fmt,
        }))
    }

    pub fn as_ptr(&self) -> *mut AVBufferRef {
        self.ctx
    }

    pub fn device_type(&self) -> AVHWDeviceType {
        self.device_type
    }

    pub fn pixel_format(&self) -> AVPixelFormat {
        self.pix_fmt
    }
}

impl Drop for HardwareContext {
    fn drop(&mut self) {
        unsafe {
            if !self.ctx.is_null() {
                av_buffer_unref(&mut self.ctx);
            }
        }
    }
}

unsafe impl Send for HardwareContext {}
unsafe impl Sync for HardwareContext {}
