use clap::{Parser, Subcommand};
use media_client::load;
use media_client::media_types::MediaKeyFrameIteratorDynMut;
use std::fs;
use std::path::Path;

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

            let mut key_frame_getter = client.get_key_frames(input.as_str()).unwrap();

            let mut i = 0;
            loop {
                let frame = key_frame_getter.get_keyframe();
                if frame.is_none() {
                    break;
                }
                let frame = frame.unwrap().unwrap();
                let output_path = Path::new(&output_dir).join(format!("{}.jpeg", i));
                fs::write(output_path, frame).expect("Failed to write frame to output file");
                i += 1;
            }
        }
    }
}
