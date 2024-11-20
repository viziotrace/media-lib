use clap::{Parser, Subcommand};
use media_client::load;
use media_client::media_types::{
    MediaFrameDecoderDynMut, VideoFrameDyn, VideoFrameTrait, VideoSize,
};
use std::fs;
use std::path::Path;
use turbojpeg::{Compressor, Subsamp};

fn get_jpeg_buffer(
    frame: VideoFrameTrait,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, std::io::Error> {
    // Log frame information
    println!("Frame dimensions: {}x{}", width, height);

    let mut compressor = Compressor::new().map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::Other, format!("TurboJPEG error: {}", e))
    })?;

    // For YUV420p, the U and V planes are quarter size of Y plane
    let y_size = frame.stride(0) * height as usize;
    let uv_size = frame.stride(1) * ((height as usize + 1) / 2);

    let mut pixels = Vec::with_capacity(y_size + 2 * uv_size);
    pixels.extend_from_slice(frame.data(0).as_slice());
    pixels.extend_from_slice(frame.data(1).as_slice());
    pixels.extend_from_slice(frame.data(2).as_slice());

    // Create YUV image with proper alignment for each plane
    let yuv_image = turbojpeg::YuvImage {
        pixels,
        width: width as usize,
        height: height as usize,
        subsamp: Subsamp::Sub2x2, // YUV420p uses 2x2 subsampling
        align: 1,                 // Use 1-byte alignment since we're concatenating the planes
    };

    compressor
        .compress_yuv_to_vec(yuv_image.as_deref())
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("JPEG compression failed: {}", e),
            )
        })
}

#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    GetKeyFrames { input: String, output_dir: String },
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn main() {
    use std::env;
    use std::path::PathBuf;

    let lib_name = if cfg!(target_os = "windows") {
        "media_lib.dll"
    } else if cfg!(target_os = "macos") {
        "libmedia_lib.dylib"
    } else {
        "libmedia_lib.so"
    };

    let default_lib_path = PathBuf::from("./target/debug").join(lib_name);
    let lib_path = env::var("MEDIA_LIB_PATH")
        .map(PathBuf::from)
        .unwrap_or(default_lib_path);

    let cli = Cli::parse();
    let client = load(&lib_path).unwrap();

    match cli.command {
        Command::GetKeyFrames { input, output_dir } => {
            println!("Getting key frames from {} to {}", input, output_dir);

            // Create the output directory if it doesn't exist
            if !Path::new(&output_dir).exists() {
                fs::create_dir_all(&output_dir).expect("Failed to create output directory");
            }

            let (target_width, target_height) = VideoSize::P240.dimensions();

            let mut decoder = client
                .new_frame_decoder(
                    input.as_str(),
                    media_client::media_types::MediaFrameDecoderOptions {
                        target_width,
                        target_height,
                    },
                )
                .unwrap();

            let mut i = 0;
            loop {
                let frame = decoder.get_frame();
                if frame.is_none() {
                    println!("No frame");
                    break;
                }
                println!("Got frame {}", i);
                let frame_result = frame.unwrap().unwrap();
                println!("Frame length: {}", frame_result.data(0).len());

                // Only save key frames
                if frame_result.get_key_frame() == 1 {
                    let width = frame_result.get_width();
                    let height = frame_result.get_height();
                    let output_path = Path::new(&output_dir).join(format!("{}.jpeg", i));
                    let jpeg_buffer = get_jpeg_buffer(frame_result, width, height).unwrap();
                    fs::write(output_path, jpeg_buffer).expect("Failed to write JPEG file");
                    i += 1;
                }
            }
        }
    }
}
