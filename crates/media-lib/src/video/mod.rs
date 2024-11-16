//! Hardware accelerated video decoding implementation using FFmpeg.
//!
//! Supports CUDA and VideoToolbox acceleration with filter graph-based frame processing.
//! Hardware contexts are shared via Arc to allow multiple decoders to use the same device.
//!
//! # Thread Safety
//! The decoder is not thread-safe and should only be used from a single thread.
//! However, multiple decoders can safely share the same hardware context across threads.

mod decoder;
mod filter;
mod hardware;
#[cfg(test)]
mod tests;
mod types;

pub use decoder::HardwareAcceleratedVideoDecoder;
pub use hardware::HardwareContext;
pub use types::{DecodedVideoFrame, VideoSize};
