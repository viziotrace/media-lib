use clap::{Parser, Subcommand};
use media_client::load;
use media_client::media_types::{MediaFrameDecoderDynMut, VideoFrameDyn};
use std::fs;
use std::path::Path;

fn get_jpeg_buffer(slice: &[u8], width: u32, height: u32) -> Result<Vec<u8>, std::io::Error> {
    match std::panic::catch_unwind(|| {
        let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_EXT_RGBA);

        comp.set_size(width as usize, height as usize);
        let mut comp = comp.start_compress(Vec::new()).unwrap(); // any io::Write will work

        println!("Writing {} bytes to jpeg", slice.len());
        // replace with your image data
        comp.write_scanlines(&slice).unwrap();

        match comp.finish() {
            Ok(buf) => Ok(buf),
            Err(e) => panic!("Failed to finish jpeg compression: {}", e),
        }
    }) {
        Ok(result) => result,
        Err(err) => Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("JPEG compression panicked: {:?}", err),
        )),
    }
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

            let mut decoder = client
                .new_frame_decoder(
                    input.as_str(),
                    media_client::media_types::MediaFrameDecoderOptions {
                        target_size: media_client::media_types::VideoSize::P720,
                    },
                )
                .unwrap();

            let mut i = 0;
            loop {
                let frame = decoder.get_frame();
                if frame.is_none() {
                    break;
                }
                let frame_result = frame.unwrap().unwrap();

                // Only save key frames
                if frame_result.get_key_frame() == 1 {
                    let frame_data = unsafe {
                        std::slice::from_raw_parts(frame_result.data_ptr(), frame_result.data_len())
                    };
                    let output_path = Path::new(&output_dir).join(format!("{}.jpeg", i));
                    let jpeg_buffer = get_jpeg_buffer(
                        frame_data,
                        frame_result.get_width(),
                        frame_result.get_height(),
                    )
                    .unwrap();
                    fs::write(output_path, jpeg_buffer).expect("Failed to write JPEG file");
                    i += 1;
                }
            }
        }
    }
}
