use std::env;

#[cfg(test)]
pub(crate) fn target_dir() -> std::path::PathBuf {
    let current_dir = env::current_dir().expect("Failed to get current directory");
    let target_dir = current_dir
        .ancestors()
        .nth(2)
        .expect("Failed to go up two directories")
        .join("target")
        .join("debug");

    target_dir
}

#[cfg(test)]
pub(crate) fn get_test_data_dir() -> std::path::PathBuf {
    let current_dir = env::current_dir().expect("Failed to get current directory");
    let target_dir = current_dir
        .ancestors()
        .nth(2)
        .expect("Failed to go up two directories")
        .join("data");

    target_dir
}

#[cfg(test)]
pub(crate) fn get_media_client_lib() -> std::path::PathBuf {
    let target_dir = target_dir();
    let lib_name = {
        #[cfg(target_os = "windows")]
        {
            "media_lib.dll"
        }
        #[cfg(target_os = "macos")]
        {
            "libmedia_lib.dylib"
        }
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        {
            "libmedia_lib.so"
        }
    };
    let full_path = target_dir.join(lib_name);
    assert!(
        full_path.exists(),
        "Library file does not exist: {:?}",
        full_path
    );
    full_path
}

#[cfg(test)]
pub(crate) fn get_test_data_file(file_name: &str) -> std::path::PathBuf {
    let test_data_dir = get_test_data_dir();
    test_data_dir.join(file_name)
}
